//! Calculation orchestration: feeds a [`Build`] into a REAL [`CalculationSession`],
//! producing an [`OutputTable`].
//!
//! Provides two paths:
//!
//! 1. [`calculate`] (**text-only, backward compatible**): only feeds item mod text into
//!    [`CalculationSession::add_modifier_texts`], losing source-level attribution.
//!    Passive nodes / skill gems / character base / enemy interaction are **all
//!    unparsed**. This entry point is kept so it doesn't break existing callers and tests.
//!
//! 2. [`calculate_with_data`] (**end-to-end attribution**): given the caller has already
//!    loaded [`BuildData`] (from [`pobr_gamedata::GameData`]), resolves every source of
//!    the Build into attributed modifiers:
//!    - equipment → [`CalculationSession::add_item`] (preserves slot + source-category attribution);
//!    - passive tree → [`pobr_tree::collect_allocated_mods`] →
//!      [`CalculationSession::add_passive_nodes`] (node-level attribution);
//!    - skill gems → classified active/support via [`BuildData`] →
//!      [`CalculationSession::add_skill_gem`] / [`CalculationSession::add_support_gem`]
//!      (gem-level attribution);
//!    - character base (level + class-derived attributes) → [`pobr_core::CharacterBase`]
//!      → [`CalculationSession::add_modifiers`] (CharacterBase attribution);
//!    - enemy + effective DPS → [`CalculationSession::setup_enemy`] + `mode_effective`.
//!
//! Gem stat injection (already wired through):
//! - **Main skill**: the per-level stat set (base damage + its own `damage_+%`) is
//!   injected via [`skill_base_modifiers`] → [`map_skill_stat`]; cost/cooldown →
//!   `SkillManaCostBase`/`SkillCooldownBase`; use_time → `base_action_rate`.
//! - **Support gems**: the same group's supports' per-level stats (added damage,
//!   `damage_+%[_final]` multipliers) are injected via [`support_modifiers`] →
//!   [`map_skill_stat`] (SupportGem attribution). Currently global scope (correct
//!   semantics with a single main skill); per-skill tag isolation for multiple skills is
//!   pending the flag system.
//! - **Passive node mods**: fully parsed (node `stats` already land alongside the
//!   official tree export), including Mastery selection and JewelSocket gating.
//!
//! Known gaps: weapon damage (attack skills depend on the not-yet-wired weapon base),
//! DoT per-minute, and SkillStatMap mapping for non-damage families like area/speed/crit
//! ([`map_skill_stat`] is filled in incrementally).

use std::borrow::Cow;

use pobr_core::calc::minion::AttributeInfusion;
use pobr_core::calc::{BuffKind, BuffSpec, CalculationSession, MinimalInput, OutputTable};
use pobr_core::mod_parser::ParseCtx;
use pobr_core::passive::AllocatedNode;
use pobr_core::rules::stat_map_engine::{self, StatMapCatalog};
use pobr_core::skill_source::GemModSource;
use pobr_core::{CalcConfig, CampaignProgress, CharacterBase, ModTag, Modifier};
use pobr_data::catalog::GrantedEffectDef;
use pobr_data::catalog::local_mods::WeaponLocalModsDef;
use pobr_data::item::{EquipmentSlot, Item};
use pobr_data::modifier::{ModFlags, ModType};
use pobr_data::monster::EnemyTier;
use pobr_data::skill::SkillTypes;
use pobr_data::source::{ModifierSource, SourceId, SourceKind};
use pobr_tree::{
    ClassContext, JewelRadius, collect_allocated_mods_for_class,
    compute_radius_jewel_effect_with_radii,
};

use crate::buff_stat_map::{map_aura_buff_stat, map_self_buff_offensive_stat};
use crate::build::{Build, RadiusJewel, SocketGroup};
use crate::build_data::{BuildData, ResolvedSkillLevel};
use crate::error::BuildError;

mod defence;
mod granted_skills;
mod skill_resolve;
use defence::*;
use granted_skills::*;
pub use skill_resolve::resolve_main_skill_selection;
use skill_resolve::*;
mod conditions;
use conditions::*;
mod weapon;
use weapon::*;
mod skill_mods;
use skill_mods::*;
mod triggers;
use triggers::*;
mod buffs;
use buffs::*;
mod collect;
use collect::*;
mod stat_map;
use stat_map::*;
pub use stat_map::{StatMapCompareRecord, take_stat_map_compare_records};
mod inject;
use inject::*;

/// The default exposure magnitude (matching PoB2 ConfigOptions.lua: each
/// `conditionEnemy*Exposure` = -20% resistance).
const EXPOSURE_MAGNITUDE: f64 = 20.0;

/// Orchestrator options: allows injecting the base [`MinimalInput`] (character base
/// life/resistances etc., assembled by the caller).
#[derive(Debug, Clone, Default)]
pub struct OrchestratorOptions {
    pub base_input: MinimalInput,
    /// Extra global modifier text (e.g. campaign rewards, debug overrides).
    pub extra_modifier_texts: Vec<String>,
}

/// End-to-end orchestrator options (dedicated to [`calculate_with_data`]).
///
/// Adds enemy configuration and the effective-DPS-semantics toggle on top of [`OrchestratorOptions`].
#[derive(Debug, Clone)]
pub struct DataOrchestratorOptions {
    /// The base [`MinimalInput`] (preconditions like resistance floor / hit range / action rate).
    pub base_input: MinimalInput,
    /// Extra global modifier text (campaign rewards / debug overrides).
    pub extra_modifier_texts: Vec<String>,
    /// Whether to inject character base (level + class-derived attributes → life/mana/accuracy BASE). Defaults to `true`.
    pub inject_character_base: bool,
    /// Enemy level (`0` = follows character level, see [`CalculationSession::setup_enemy`]).
    pub enemy_level: u32,
    /// Enemy tier (normal / Boss / Pinnacle / Uber).
    pub enemy_tier: EnemyTier,
    /// Effective-DPS-semantics toggle (`true` → accounts for hit / enemy damage
    /// reduction; `false` → panel semantics).
    pub mode_effective: bool,
    /// The statmap mapping channel. Defaults to [`StatMapMode::Data`]; `Compare` is a
    /// pure observation mode (output identical to Data; the outcome record is retrieved
    /// via [`take_stat_map_compare_records`]).
    pub stat_map_mode: StatMapMode,
    /// The statmap data catalog (`overlay/skill_stat_map.json` loaded and injected via
    /// gamedata). `None` (default) = falls back to [`BuildData::stat_map_catalog`]
    /// (already loaded alongside the data pack by `BuildData::load`); when neither is
    /// present, the data channel treats everything as a miss.
    pub stat_map_catalog: Option<std::sync::Arc<StatMapCatalog>>,
}

impl Default for DataOrchestratorOptions {
    fn default() -> Self {
        Self {
            base_input: MinimalInput::default(),
            extra_modifier_texts: Vec::new(),
            inject_character_base: true,
            enemy_level: 0,
            enemy_tier: EnemyTier::default(),
            mode_effective: false,
            stat_map_mode: StatMapMode::default(),
            stat_map_catalog: None,
        }
    }
}

/// The statmap mapping channel selection (a dual-run framework, contract C3; a
/// deliberate decision: Compare is kept as a long-term comparison tool — config / parser
/// dual-runs reuse the same pattern).
///
/// A runtime enum rather than a cargo feature: the 18-build dual-run completes within a
/// single process, making reporting easy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatMapMode {
    /// The data engine (`overlay/skill_stat_map.json` + `rules/stat_map_engine`).
    /// **Default** (the switch commit; the four-precondition checklist is at
    /// `audits/rearchitecture-2026-06-10/blueprints/m1-statmap-switch-log.md`).
    #[default]
    Data,
    /// Observation comparison: the Data computation + recording a mapping outcome per
    /// stat (**output identical to Data**; pure observation that changes no computed
    /// result; records are retrieved via [`take_stat_map_compare_records`]). Kept as a
    /// long-term comparison framework after the Legacy heuristic was removed (T2.4) —
    /// config / parser dual-runs reuse the same pattern. Reverting after old code
    /// removal = reverting the removal commit.
    Compare,
}

/// The default parse rules for the text-only path ([`calculate`]): loaded and compiled
/// once from the repo data directory (`pobr_gamedata::current_data_dir()`), cached
/// process-wide.
///
/// After the legacy parser was removed, there's no built-in fallback parser — a missing
/// data directory / compile failure returns an error (fail-fast, doesn't silently treat
/// everything as Unsupported). The data-carrying primary path
/// ([`calculate_with_data`]) doesn't go through this function (its rules are compiled alongside [`BuildData::load`]).
fn default_parser_rules()
-> Result<std::sync::Arc<pobr_core::mod_parser::CompiledParserRules>, BuildError> {
    use std::sync::{Arc, OnceLock};
    static RULES: OnceLock<Result<Arc<pobr_core::mod_parser::CompiledParserRules>, String>> =
        OnceLock::new();
    RULES
        .get_or_init(|| {
            let data = pobr_gamedata::GameData::new(pobr_gamedata::current_data_dir());
            let doc = data
                .mod_parser_rules()
                .map_err(|e| format!("加载 mod_parser_rules.json 失败：{e}"))?
                .ok_or_else(|| "数据目录缺 overlay/mod_parser_rules.json".to_string())?;
            let special = data
                .load_ruleset()
                .map_err(|e| format!("加载 ruleset 失败：{e}"))?
                .special_mods
                .unwrap_or_default();
            pobr_core::mod_parser::CompiledParserRules::compile_with_special(&doc, &special)
                .map(Arc::new)
                .map_err(|e| format!("parser 规则编译失败：{e:?}"))
        })
        .clone()
        .map_err(BuildError::Parse)
}

/// Runs a minimal calculation on a [`Build`], returning a scalar [`OutputTable`].
///
/// **The text-only path** (backward compatible): item mods are fed in as text, losing
/// attribution; passives / gems / character base / enemy are all unparsed. Mod parsing
/// goes through [`default_parser_rules`] (the default data directory; missing = error).
/// For end-to-end attribution, use [`calculate_with_data`].
pub fn calculate(build: &Build, options: &OrchestratorOptions) -> Result<OutputTable, BuildError> {
    let cfg = build.config.to_calc_config();
    let mut session = CalculationSession::new(options.base_input).with_config(cfg);
    session.set_parser_rules(default_parser_rules()?);

    // Item mods: injected in enchant → implicit → explicit order (matching PoB's source layering).
    let item_texts = collect_item_texts(build);
    session
        .add_modifier_texts(item_texts)
        .map_err(|e| BuildError::Parse(e.to_string()))?;

    if !options.extra_modifier_texts.is_empty() {
        session
            .add_modifier_texts(options.extra_modifier_texts.iter())
            .map_err(|e| BuildError::Parse(e.to_string()))?;
    }

    let minimal = session.perform_minimal();
    Ok(OutputTable::from(&minimal))
}

/// Passive tree version reconciliation diagnostic (gap B): the build's recorded
/// `treeVersion` + node ids that are **allocated but not in the loaded tree**. The
/// latter is the actual symptom of a tree version mismatch — once a node has been
/// moved/removed across versions, calc silently skips that id (`pobr_tree`'s node.rs
/// drops unknown ids via `filter_map`), and this diagnostic makes it explicit.
///
/// **Non-fatal / doesn't change calc behavior**: only surfaces a warning for the caller
/// (CLI / tests / upper layers); "load the matching tree per the build's `treeVersion` +
/// migrate" is future work (needs a tree-version↔data-version mapping + a multi-tree-
/// version dataset, see `devs/docs/architecture/16-data-versioning-and-iteration.md` §6 gap B).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TreeVersionReport {
    /// The build's `<Spec treeVersion>` annotation (`None` = an old save with no annotation).
    pub build_tree_version: Option<String>,
    /// Allocated node skill ids that are **not in the loaded tree** (in `allocated_nodes`'s original order, deterministic).
    pub unknown_nodes: Vec<u32>,
}

impl TreeVersionReport {
    /// Whether every allocated node is in the loaded tree (no mismatch symptoms).
    pub fn is_clean(&self) -> bool {
        self.unknown_nodes.is_empty()
    }
}

/// Reconciles the build's allocated passive nodes against the loaded tree
/// ([`BuildData::passive_nodes`]) — see [`TreeVersionReport`]. Purely read-only, zero calc behavior change.
pub fn diagnose_tree_version(build: &Build, data: &BuildData) -> TreeVersionReport {
    let unknown_nodes = build
        .tree
        .allocated_nodes
        .iter()
        .map(|n| n.0)
        .filter(|id| !data.passive_nodes.contains_key(id))
        .collect();
    TreeVersionReport {
        build_tree_version: build.tree_version.clone(),
        unknown_nodes,
    }
}

/// A single socket group's DPS contribution (a FullDPS line item).
#[derive(Debug, Clone)]
pub struct SkillDps {
    /// The 0-based index into `build.socket_groups`.
    pub group_index: usize,
    /// This group's main skill's granted effect id (selected by `pick_group_main_skill`).
    pub skill_id: String,
    /// This skill's CombinedDPS, calculated independently from the whole build's perspective.
    pub combined_dps: f64,
}

/// A FullDPS report (matching PoB2's `FullDPS`, a multi-skill scaffold).
#[derive(Debug, Clone)]
pub struct FullDpsReport {
    /// The sum of every enabled damaging skill's CombinedDPS (= the sum of `per_skill`'s entries).
    pub full_dps: f64,
    /// Per-skill breakdown (only enabled groups with CombinedDPS>0).
    pub per_skill: Vec<SkillDps>,
    /// The full output table of the main skill (selected by `resolve_main_skill`);
    /// unchanged from the single-skill/panel semantics.
    pub primary: OutputTable,
}

/// Calculates FullDPS (a multi-skill scaffold) — PoB2's "sum of all skills' DPS".
///
/// Walks every socket group that's **enabled and has a resolvable damaging main
/// skill**, calculating each independently via [`calculate_with_data`] (temporarily
/// setting that group as `mainSocketGroup`, while **every other group stays enabled**
/// to preserve aura/buff contributions, matching PoB2's "whole-build perspective per
/// skill"), summing each one's CombinedDPS. `primary` remains the full output of the
/// main skill selected by [`resolve_main_skill`].
///
/// **Scaffold boundaries** (later refinements to PoB2's FullDPS, not handled in this
/// version):
/// - doesn't deduplicate DoT/ailments shared across multiple skills (may double-count
///   ongoing damage);
/// - doesn't special-case trigger shells / Mirage clone's inner skills;
/// - recomputes sequentially, not in parallel (parallel multi-skill execution is a
///   target for later performance work).
///
/// Only iterates groups where [`pick_group_main_skill`] is `Some`, avoiding double
/// counting from `resolve_main_skill` falling back to a different group when
/// `mainSocketGroup` points to a group with no damaging skill.
pub fn calculate_full_dps(
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
) -> Result<FullDpsReport, BuildError> {
    // FullDPS must synthesize granted skills from the same equipment view as a normal
    // calculation, otherwise an unlocked Ring 3 item would produce a skill group first,
    // bypassing the item gate inside calculate_with_data.
    let ring3_gated;
    let build = match gate_locked_ring3(build, data) {
        Some(gated) => {
            ring3_gated = gated;
            &ring3_gated
        }
        None => build,
    };

    // Item-granted skill's synthesized group also enters the per-skill list (the
    // scoped recompute internally synthesizes it again, but the dedup key is the same →
    // idempotent; synthesizing it here first is so the per_skill iteration can see this group).
    let granted_augmented;
    let build = match augment_item_granted_skills(build, data) {
        Some(augmented) => {
            granted_augmented = augmented;
            &granted_augmented
        }
        None => build,
    };
    let primary = calculate_with_data(build, data, options)?;

    let mut per_skill = Vec::new();
    let mut full_dps = 0.0;
    for (i, group) in build.socket_groups.iter().enumerate() {
        if !group.enabled {
            continue;
        }
        let Some((skill_id, _level, _set)) = pick_group_main_skill(data, group) else {
            continue;
        };
        let skill_id = skill_id.to_string();

        let mut scoped = build.clone();
        scoped.main_socket_group = Some(i + 1);
        let out = calculate_with_data(&scoped, data, options)?;
        if out.combined_dps > 0.0 {
            full_dps += out.combined_dps;
            per_skill.push(SkillDps {
                group_index: i,
                skill_id,
                combined_dps: out.combined_dps,
            });
        }
    }

    Ok(FullDpsReport {
        full_dps,
        per_skill,
        primary,
    })
}

/// Runs an **end-to-end attribution** calculation on a [`Build`], returning a scalar
/// [`OutputTable`].
///
/// The caller loads [`BuildData`] (node table / gem table / class attributes) via
/// [`pobr_gamedata::GameData`] first, then passes it to this function; this function
/// does zero additional I/O. Each source is injected into [`CalculationSession`]
/// through its own attribution entry point, letting [`pobr_core::trace::TraceGraph`]
/// trace outputs back to an equipment slot / passive node / gem / character base /
/// enemy configuration.
///
/// Assembly order (deterministic): character base → equipment → passive tree → skill
/// gems → enemy → extra text.
///
/// # Loading [`BuildData`] (for caller reference)
///
/// ```ignore
/// use pobr_gamedata::GameData;
/// use pobr_build::{BuildData, calculate_with_data, DataOrchestratorOptions};
///
/// let data = GameData::new("data/4.5.0.3.4");
/// let build_data = BuildData::load(&data)?;            // Load once, reuse many times
/// let opts = DataOrchestratorOptions { mode_effective: true, ..Default::default() };
/// let out = calculate_with_data(&build, &build_data, &opts)?;
/// ```
pub fn calculate_with_data(
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
) -> Result<OutputTable, BuildError> {
    calculate_with_data_session(build, data, options).map(|session| session.output().clone())
}

/// Runs the same pipeline as [`calculate_with_data`], but returns the completed
/// [`CalculationSession`] itself (after perform) — for callers that need to read
/// ModDb's per-source contributions (breakdown / attribution panels, e.g. `pobr-wasm`'s
/// JSON contract layer) to keep querying beyond the output, without recomputing.
pub fn calculate_with_data_session(
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
) -> Result<CalculationSession, BuildError> {
    // The statmap channel context: the guard's scope = this calculation; defaults to
    // Data (the T2.4 switch). The catalog prefers what the orchestrator options
    // explicitly inject, falling back to BuildData's catalog loaded alongside the data
    // pack when absent; Compare is pure observation (the diff record is taken by the caller).
    let _stat_map_guard = install_stat_map_context(
        options.stat_map_mode,
        options
            .stat_map_catalog
            .clone()
            .or_else(|| data.stat_map_catalog.clone()),
    );

    // Stage 0: build view transformation (Ring3 gate → item-granted skill synthesis → quality conversion)
    let build = stage_build_view(build, data);
    let build: &Build = &build;

    // Stages 1-4: pre-session resolution (order = dependency: main skill → config → cfg → weapon base)
    let mut ctx = StageCtx::new(build, data, options);
    stage_resolve_main_skill(&mut ctx);
    stage_resolve_config(&mut ctx);
    stage_build_cfg(&mut ctx);
    stage_weapon_bases(&mut ctx);

    // Stage 5+: session assembly + source injection (numbering follows the existing assembly-order doc)
    let mut session = stage_create_session(&mut ctx);
    stage_hand_sources(&mut session, &ctx);
    stage_cooldown_bypass(&mut session, &ctx);

    // 1. Character base (level + class-derived attributes) + elemental resistance penalty (campaign progress tier).
    inject_character_base(&mut session, build, data, options, &ctx.resolved_config);

    // 1b/1b-ii/1c. Main skill base/quality/unselected-set/DoT/corpse-explosion/crossbow/support/trigger + damage multiplier + weapon crit.
    inject_main_skill_mods(
        &mut session,
        build,
        data,
        options,
        &ctx.main_skill,
        ctx.weapon.as_ref(),
        ctx.dmg_mult,
    );

    // 1d. Item base defence / shield base block / per-item Spirit / Ward → BASE mods.
    inject_defence_base(&mut session, build, data);

    // 2. Equipment: attribution-path injection (per-item filter / Kalandra mirroring / local mod stripping / slot bonus numeric copies).
    let main_weapon_active = ctx
        .main_effect
        .is_some_and(|e| e.is_attack() && !e.is_non_weapon_attack());
    inject_items(
        &mut session,
        build,
        data,
        ctx.off_weapon.is_some(),
        main_weapon_active,
    )?;

    // 2b. Jewels (passive tree/abyss sockets): mods injected globally.
    stage_inject_jewels(&mut session, &ctx)?;

    // 2b''. Active flask/charm payload injection (consumed by env_finalize stage 3's merge).
    inject_flasks_charms(&mut session, build, data);

    // 2b'. Radius jewels' grant mods expanded and injected as global modifier text.
    stage_inject_radius_jewels(&mut session, &ctx)?;

    // 2c/2d/2e. Quest reward global text + config interpreter player mods + the customMods line channel.
    stage_inject_config_mods(&mut session, &ctx)?;

    // 3/3a'/3b/3b'/3c. Passive tree nodes + anointed notables + small/Notable effect scaling + keystone mapping.
    stage_inject_passives(&mut session, &ctx)?;

    // 4. Skill gems: classified active/support, each injected via its own attribution entry point.
    inject_skill_gems(&mut session, build, data)?;

    // 4b/4b'/4b''. Aura·curse BuffSpec + support-granted buffs + herald presence count/conditions.
    inject_buffs_and_heralds(&mut session, build, data);

    // 4c/4c'/4d. Mark's self offensive buff + non-main-group exposure supports + Spirit reservation aggregation.
    inject_self_buff_exposure_spirit(
        &mut session,
        build,
        data,
        ctx.main_skill.as_ref().map(|(_, g, _)| *g),
    );

    // 5/5a/5b. Enemy configuration (setup_enemy) + the config interpreter's enemy bucket + player-applied elemental exposure.
    inject_enemy(
        &mut session,
        build,
        options,
        ctx.enemy_tier,
        &ctx.resolved_config,
    );

    // 6. Extra global text (campaign rewards / debug overrides).
    stage_inject_extra_texts(&mut session, &ctx)?;

    // 6b. PoE2 attribute derivation (final Str/Dex/Int → Life/Mana/Accuracy delta).
    inject_attribute_derivation(&mut session, build, data, options);

    // 6c. Backfills per-X resource/attribute scaling amounts (PoB2's PerStat denominator variables).
    inject_per_x_multipliers(&mut session, build, data);

    // 6c2. Equipped support gems counted by color (matching PoB2
    //      CalcSetup.lua:2015-2044) → Red/Green/BlueSupportGems multipliers (the
    //      denominator for pinned MultiplierThreshold entries like "if you have at
    //      least 10 <color> Support Gems Socketed").
    inject_support_gem_counts(&mut session, build, data);

    // 6d. Source-granted condition flags → cfg condition bridging (Bonded modifiers / Arcane Surge).
    inject_condition_bridges(&mut session);

    // 6e. The low-life automatic condition (matching vendor CalcDefence.lua:335-350:
    //     unreserved ratio ≤ 0.35 → Condition:LowLife). Must run after reservation
    //     mods are injected (4d) and pool values are computable (6c).
    session.bridge_low_pool_conditions();

    // Diagnostic dumps (POBR_DBG_UNSUPPORTED / ALLMODS / STAT, for parity investigation).
    stage_debug_dumps(&session);

    // Minion wiring: after every player source is injected, before perform, recognizes
    // summoning gems (a nonempty `effect_minion_list`) and wires them into
    // `Env.minions`. At the end of perform, `perform_minions` runs the same
    // offence/defence pass for every minion, landing results in
    // `OutputTable.minions`. Gate: only wired in when some active skill resolves a
    // nonempty minion_list — a non-summoning build never triggers this, zero behavior
    // impact on the existing 18 builds.
    spawn_minions(&mut session, build, data, &options.extra_modifier_texts);

    // perform fills env.player.output entirely (including every fill-stage field of
    // calc_defence — armour/evasion/ES, ailments, EHP, etc.); the full OutputTable is
    // taken, not the MinimalOutput subset (which loses defence etc.).
    session.perform_minimal();
    Ok(session)
}

/// The inter-stage context for [`calculate_with_data_session`]: each pre-session
/// resolution stage ([`stage_resolve_main_skill`] → [`stage_resolve_config`] →
/// [`stage_build_cfg`] → [`stage_weapon_bases`]) fills in fields in order, and the
/// session stage only reads them. Each field's comment notes which stage produces it;
/// see each stage fn's doc comment for the ordering constraints between stages.
struct StageCtx<'a> {
    build: &'a Build,
    data: &'a BuildData,
    options: &'a DataOrchestratorOptions,
    /// The main skill's per-level parameters + owning group + real skill id (from stage_resolve_main_skill).
    main_skill: Option<(ResolvedSkillLevel, &'a SocketGroup, &'a str)>,
    /// The main skill's granted effect definition (from stage_resolve_main_skill; meta/trigger shells already skipped).
    main_effect: Option<&'a GrantedEffectDef>,
    /// The main skill's **final** type set (from stage_resolve_main_skill; the addSkillTypes fixed point).
    main_skill_types: Vec<String>,
    /// Main skill type → cfg damage flags (from stage_resolve_main_skill).
    skill_flags: ModFlags,
    /// Main skill type → `cfg.skill_types` classification bits (from stage_resolve_main_skill).
    skill_type_bits: SkillTypes,
    /// Main skill keyword + main weapon category → extra damage-scaling ModName (from
    /// stage_resolve_main_skill; taken by stage_build_cfg and folded into cfg).
    dmg_keywords: Vec<String>,
    /// The config consumption view (from stage_resolve_config).
    resolved_config: crate::config_resolve::ResolvedConfig,
    /// The calc context (stage_resolve_config produces the base, stage_build_cfg layers
    /// on skill-derived pieces; reset to default once stage_create_session takes it).
    cfg: CalcConfig,
    /// Enemy tier (from stage_build_cfg: the build XML's explicit value takes priority,
    /// falling back to the orchestrator option when absent).
    enemy_tier: EnemyTier,
    /// The base calculation input (new() takes it from the orchestrator options,
    /// stage_weapon_bases backfills the action rate).
    base_input: MinimalInput,
    /// The skill damage multiplier (from stage_weapon_bases).
    dmg_mult: f64,
    /// The main-hand weapon base contribution (from stage_weapon_bases; attack skills only).
    weapon: Option<WeaponContribution>,
    /// The dual-wielding off-hand weapon base contribution (from stage_weapon_bases).
    off_weapon: Option<WeaponContribution>,
    /// The main hand's converted HandSource value (from stage_weapon_bases).
    hand_weapon: Option<pobr_core::calc::WeaponBase>,
    /// The off hand's converted HandSource value (from stage_weapon_bases).
    off_hand_weapon: Option<pobr_core::calc::WeaponBase>,
    /// Whether the main skill bypasses cooldown (from stage_weapon_bases; used the
    /// instant a charge is consumed, e.g. Flicker).
    bypasses_cooldown: bool,
}

impl<'a> StageCtx<'a> {
    fn new(build: &'a Build, data: &'a BuildData, options: &'a DataOrchestratorOptions) -> Self {
        Self {
            build,
            data,
            options,
            main_skill: None,
            main_effect: None,
            main_skill_types: Vec::new(),
            skill_flags: ModFlags::NONE,
            skill_type_bits: SkillTypes::NONE,
            dmg_keywords: Vec::new(),
            resolved_config: crate::config_resolve::ResolvedConfig::default(),
            cfg: CalcConfig::default(),
            enemy_tier: options.enemy_tier,
            base_input: options.base_input,
            dmg_mult: 1.0,
            weapon: None,
            off_weapon: None,
            hand_weapon: None,
            off_hand_weapon: None,
            bypasses_cooldown: false,
        }
    }
}

/// Stage 0: build view transformation (the collapsed form of what was originally a
/// chain of shadow variables) — each step clones only when it actually takes effect
/// (Cow), value-equal to mutating the build in place. The three steps run in the order
/// of vendor CalcSetup's item pre-processing: first strips inactive items, then
/// synthesizes granted skill groups from the remaining items, and finally converts gem quality.
fn stage_build_view<'a>(build: &'a Build, data: &BuildData) -> Cow<'a, Build> {
    let mut build = Cow::Borrowed(build);

    // Ring 3 gate (PoB2 CalcSetup.lua:821): when "+1 Ring Slot" isn't allocated on the
    // tree (vendor's `AdditionalRingSlot` flag, ModParser.lua:3128; the Ritualist
    // ascendancy's "Unfurled Finger"), the Ring 3 item is ignored entirely — stripped
    // from the build view once here, so it applies consistently at every downstream
    // consumption point (injection/gem-level scanning/text collection).
    if let Some(gated) = gate_locked_ring3(&build, data) {
        build = Cow::Owned(gated);
    }

    // Item-granted skills (`Grants Skill: [Level N] X`) → synthesized skill groups
    // (matching vendor CalcSetup.lua:1414-1453, which builds an independent socket
    // group; deduplicated by source, slot, skill and level — zero behavior change when
    // a PoB2-XML-pre-expanded group already exists).
    if let Some(augmented) = augment_item_granted_skills(&build, data) {
        build = Cow::Owned(augmented);
    }

    // Gem quality bonuses: "+N% to Quality of all <X> Skills" (tree small passives/items)
    // is pre-folded into each gem's quality (matching vendor's applyGemMods, which
    // stacks effect.quality onto every gem effect, CalcSetup.lua:410-435), so it applies
    // consistently at every downstream quality consumption point.
    if let Some(adjusted) = apply_gem_quality_bonuses(&build, data) {
        build = Cow::Owned(adjusted);
    }

    // nameSpec-only gem references → skill_id backfilled (matching PoB2 SkillsTab
    // looking up a gem's equivalent by nameSpec): a lineage support (e.g. Atziri's
    // Communion) lacks skillId/gemId in the XML, only a display name. Matched against
    // granted_effects ids by normalized name; a miss keeps an empty id (every consumer
    // silently skips it).
    if let Some(resolved) = resolve_name_spec_gems(&build, data) {
        build = Cow::Owned(resolved);
    }

    build
}

/// Resolves a `GemSkillRef { skill_id: "", name_spec: Some(name) }`'s display name into
/// a granted effect id. Normalization = lowercase + keep only alphanumerics; candidate
/// ids have the `Player` suffix stripped (lineage variants like `PlayerTwo/Three` not
/// matching is fine — their XML carries a skillId), and a support id additionally has
/// the `Support` prefix stripped (`SupportAtzirisCommunionPlayer` → `atziriscommunion` =
/// the normalized form of nameSpec "Atziri's Communion"). Returns None when nothing changed.
fn resolve_name_spec_gems(build: &Build, data: &BuildData) -> Option<Build> {
    fn norm(s: &str) -> String {
        s.chars()
            .filter(char::is_ascii_alphanumeric)
            .map(|c| c.to_ascii_lowercase())
            .collect()
    }
    let pending: Vec<String> = build
        .socket_groups
        .iter()
        .flat_map(|g| &g.gem_skills)
        .filter(|gem| gem.skill_id.is_empty())
        .filter_map(|gem| gem.name_spec.clone())
        .collect();
    if pending.is_empty() {
        return None;
    }
    let mut lookup: std::collections::HashMap<String, &str> = std::collections::HashMap::new();
    for id in data.granted_effects.keys() {
        let stem = id.strip_suffix("Player").unwrap_or(id);
        let stem = stem.strip_prefix("Support").unwrap_or(stem);
        lookup.insert(norm(stem), id.as_str());
    }
    let mut out = build.clone();
    let mut changed = false;
    for group in &mut out.socket_groups {
        for gem in &mut group.gem_skills {
            if gem.skill_id.is_empty()
                && let Some(name) = &gem.name_spec
                && let Some(id) = lookup.get(&norm(name))
            {
                gem.skill_id = (*id).to_string();
                changed = true;
            }
        }
    }
    changed.then_some(out)
}

/// Stage 1: main skill resolution — per-level parameters, the final skillTypes fixed
/// point (matching vendor CalcActiveSkill.lua:179-214), damage flags / classification
/// bits / damage keywords. Must run first: the action rate needs to go into base_input
/// (stage_weapon_bases), and the type flags / combat conditions need to go into cfg
/// (stage_build_cfg) — both consume this stage's output.
fn stage_resolve_main_skill(ctx: &mut StageCtx<'_>) {
    let (build, data) = (ctx.build, ctx.data);
    // The main skill's per-level parameters (cast/attack time → action rate; cost /
    // cooldown injected via BASE mods). Resolved before building the session, so the
    // action rate can be written into base_input + the cfg damage flags can be set based on its type.
    ctx.main_skill = resolve_main_skill(build, data);

    // Main skill type → cfg damage flags (Attack/Spell/Projectile/Area/Melee), making
    // `increased <Projectile|Area|Spell|Melee> Damage` apply to this skill (damage
    // aggregation picks these up by flag name). Main skill's effect definition: uses the
    // **real main skill id** resolved by resolve_main_skill (meta/trigger shells already
    // skipped), not the first gem's active_skill_id in the group (which is a meta shell
    // in a multi-active-skill group, causing flag/damage-type mismatches).
    ctx.main_effect = ctx
        .main_skill
        .as_ref()
        .and_then(|(_, _, skill_id)| data.granted_effects.get(*skill_id));
    // The main skill's **final** type set = its own skill_types + the addSkillTypes
    // fixed point over compatible supports (matching vendor CalcActiveSkill.lua:179-214,
    // which merges addSkillTypes into activeSkill.skillTypes, with every downstream
    // flag/condition derivation using the final set — e.g. Cast on Critical adds
    // `Triggered` to the triggered spell, making the "Triggered Spells deal …" mod
    // family hit + the combat-condition trigger exemption apply per vendor :248).
    // Sorted for determinism.
    ctx.main_skill_types = ctx
        .main_skill
        .as_ref()
        .map(|(_, group, skill_id)| {
            let mut types: Vec<String> = judge_group_supports(group, data, skill_id)
                .final_skill_types
                .into_iter()
                .collect();
            // A meta trigger shell's `Triggered`: vendor injects this from the gem's
            // **support half** (e.g. Cast on Critical → SupportMetaCastOnCritPlayer's
            // addSkillTypes=[Triggered]); PoBR's cataloged data doesn't model a gem's
            // second granted-effect half (skill_gems only has the primary
            // grantedEffect half), so this backfills equivalently using the existing
            // trigger recognition (trigger_configs's four-level key, the same
            // determination as trigger_modifiers).
            if !types.iter().any(|t| t == "Triggered")
                && recognize_trigger_config(data, group, skill_id).is_some()
            {
                types.push("Triggered".to_string());
            }
            types.sort();
            types
        })
        .unwrap_or_default();
    ctx.skill_flags = ctx
        .main_effect
        .map(|_| skill_type_flags(&ctx.main_skill_types))
        .unwrap_or(ModFlags::NONE);
    // Main skill type → `cfg.skill_types` classification bits: `is_attack()` drives the
    // hit-chance check (only attacks do an accuracy/evasion check, vendor
    // CalcOffence.lua:2611); see skill_type_bits's doc.
    ctx.skill_type_bits = ctx
        .main_effect
        .map(|_| skill_type_bits(&ctx.main_skill_types))
        .unwrap_or(SkillTypes::NONE);
    ctx.dmg_keywords = damage_keywords(
        build,
        data,
        ctx.main_effect
            .map(|_| ctx.main_skill_types.as_slice())
            .unwrap_or(&[]),
    );
}

/// Stage 2: closes out config consumption (the primary-path switch) — goes through
/// `config_interpreter::interpret` when a ConfigCatalog is available (raw_inputs →
/// conditions/multipliers/scalar wrapping/Config-attributed modifiers); falls back to
/// the legacy parse_config output when the catalog is missing (tolerant of a missing
/// table). Produces the base cfg (including backfilling the config multiplier bridge
/// for the Effective gate); stage_build_cfg layers skill-derived pieces on top of it.
fn stage_resolve_config(ctx: &mut StageCtx<'_>) {
    ctx.resolved_config =
        crate::config_resolve::resolve_config(ctx.build, ctx.data.config_catalog.as_deref());
    let mut base_cfg = ctx.resolved_config.config.to_calc_config();
    // The config multiplier bridge for the Effective gate: the interpreter's bare-effect
    // Condition bridge only accepts "tagless" entries, so a count-type placeholder for
    // `Multiplier:<X>` carrying a `Condition:Effective` tag (e.g. vendor
    // ConfigOptions.lua:1642's `multiplierDifferentGrenadeFired`'s
    // defaultPlaceholderState=1) doesn't land in cfg.multipliers. Vendor's semantics =
    // `GetMultiplier` queries modDB directly (the tag is evaluated against cfg; under
    // EFFECTIVE mode, Effective is always true, CalcSetup.lua:583-588); PoBR's
    // multiplier goes through a cfg snapshot → backfilled here after evaluating against
    // mode_effective (only for the single-tag Effective shape; other tag shapes stay on
    // the mod channel).
    if ctx.options.mode_effective {
        for m in &ctx.resolved_config.player_mods {
            // Only accepts the shape "has tags and they're all Effective" — **an empty-tag
            // entry must be excluded**: a bare `Multiplier:` effect is already backfilled
            // into cfg.multipliers by the interpreter's bare-effect path
            // (config_interpreter.rs:362-377); adding it again here would double-count
            // (confirmed: sigilOfPowerStages's placeholder 1 got boosted to 2 under
            // effective semantics, making Sigil of Power's per-stage MORE falsely go
            // from 17→34). `Combat` and `Effective` share the same gate (vendor's main
            // output env has both always true, CalcSetup.lua:583-588 + mode_combat;
            // e.g. `multiplierNearbyAlly`'s `Multiplier:NearbyAlly BASE +
            // Condition{Combat}` — the denominator for the NearbyAlly≥1 threshold row,
            // ConfigOptions.lua:1018).
            if m.mod_type == ModType::Base
                && let Some(var) = m.name.as_str().strip_prefix("Multiplier:")
                && let pobr_core::ModValue::Number(n) = m.value
                && !m.tags.is_empty()
                && m.tags.iter().all(|t| {
                    matches!(t, pobr_core::ModTag::Condition { var, negated: false, actor: None } if var == "Effective" || var == "Combat")
                })
            {
                *base_cfg.multipliers.entry(var.to_string()).or_insert(0.0) += n;
            }
        }
    }
    ctx.cfg = base_cfg;
}

/// Stage 3: cfg assembly — layers the main skill's damage flags / classification bits /
/// display name / keywords / mode toggles onto the base cfg (matching vendor
/// CalcSetup.lua:583-597's buffMode "EFFECTIVE" semantics), then adds combat
/// conditions, enemy tier conditions, PoB2's condition implication chain, and
/// build-state equipment/weapon conditions. Depends on stage 1/2's output
/// (skill_flags / base cfg), must run before session creation (with_config replaces cfg wholesale).
fn stage_build_cfg(ctx: &mut StageCtx<'_>) {
    let (build, data, options) = (ctx.build, ctx.data, ctx.options);
    let base_cfg = std::mem::take(&mut ctx.cfg);
    let base_flags = base_cfg.flags;
    let mut cfg = base_cfg
        .with_flags(base_flags | ctx.skill_flags)
        .with_skill_types(ctx.skill_type_bits)
        // Main skill's display name (matching vendor's `skillCfg.skillName`): the
        // matching semantics for the special channel's `SkillName` tag. Same source as
        // gem_level_category_matches (skill_name_from_id, lowercase).
        .with_skill_name(
            ctx.main_skill
                .as_ref()
                .map(|(_, _, skill_id)| skill_resolve::skill_name_from_id(skill_id)),
        )
        .with_damage_keywords(std::mem::take(&mut ctx.dmg_keywords))
        .with_mode_effective(options.mode_effective)
        // Vendor's buffMode is always "EFFECTIVE" outside CALCS mode
        // (CalcSetup.lua:583-597 → env.mode_buffs = true), so mode_buffs is always set
        // here — enabling buff_pass (the aura multiplier zone / curse priority+limit).
        // mode_effective still follows the caller's option.
        .with_mode_buffs(true)
        // Same as above (CalcSetup.lua:583-597's buffMode "EFFECTIVE" →
        // env.mode_combat = true). Activation surface: automatic combat condition
        // setting (combat_conditions below) + env_finalize stage 3's flask/charm merge +
        // stage 6's buff_expander.
        .with_mode_combat(true);
    // DistanceRamp's skillDist (matching vendor CalcActiveSkill.lua:671+684, 0.22.0):
    // `effectiveRange = env.configInput.enemyDistance or env.configPlaceholder.enemyDistance`,
    // `skillDist = env.mode_effective and effectiveRange`. From 0.22.0 on, **a
    // placeholder feeds skillDist as a fallback** (old vendor only read the explicit
    // `<Input>` — back then, the demo suite was all placeholders → None → the Close
    // Combat distance MORE was skipped entirely). The fallback chain matches vendor
    // ConfigTab: explicit `<Input>` → XML `<Placeholder>` → the catalog's
    // `defaultPlaceholderState` (ConfigTab.lua:559 pre-fills a placeholder default for
    // an entry with no value, enemyDistance = 20).
    let skill_distance = options
        .mode_effective
        .then(|| {
            let raw = &build.config.raw_inputs;
            raw.values
                .get("enemyDistance")
                .or_else(|| raw.placeholders.get("enemyDistance"))
                .and_then(|v| v.as_number())
                .or_else(|| {
                    data.config_catalog
                        .as_deref()
                        .and_then(|c| c.get("enemyDistance"))
                        .and_then(|def| def.default.as_ref())
                        .and_then(|d| d.placeholder_number)
                })
        })
        .flatten();
    cfg = cfg.with_skill_distance(skill_distance);
    // Main skill-derived combat conditions (read directly from vendor
    // CalcPerform.lua:242-266's `if env.mode_combat` section): attack/spell/Movement/
    // Minion/Vaal/Channel → "...Recently"/Channelling conditions;
    // triggered/trap/mine/totem exempted (using the **final** type set — a meta
    // support's addSkillTypes `Triggered` makes the exemption apply, matching vendor :248).
    if ctx.main_effect.is_some() {
        for cond in combat_conditions(&ctx.main_skill_types, ctx.skill_flags) {
            cfg = cfg.with_condition(cond, true);
        }
    }
    // Enemy tier (19-G3 wiring): the build XML Config's explicitly saved `enemyIsBoss`
    // takes priority; falls back to the caller's orchestrator option when omitted
    // (PoB2's defaultIndex=3 = Pinnacle, matching existing callers).
    ctx.enemy_tier = ctx
        .resolved_config
        .config
        .enemy_tier
        .unwrap_or(options.enemy_tier);
    // Enemy rarity condition: the default DPS view vs. Boss/Pinnacle/Uber (= Unique) →
    // set true, making condition-type damage boosts like "... against Rare or Unique
    // Enemies" apply (PoB's boss-DPS semantics).
    if matches!(
        ctx.enemy_tier,
        EnemyTier::Boss | EnemyTier::Pinnacle | EnemyTier::Uber
    ) {
        cfg = cfg
            .with_condition("Unique", true)
            .with_condition("RareOrUnique", true);
    }

    // PoB2's condition implication chain (ConfigOptions.lua's `implyCond`/
    // `implyCondList`): a parent condition checked in build config automatically sets
    // several child conditions true. PoBR only reads build config's parent condition
    // names, so implications must be filled in here, or child-condition-type mods
    // (already parsed by PoBR as condition tags) wouldn't apply. Generic, independent of build/skill.
    cfg = apply_condition_implications(cfg);

    // PoB2's `Condition:UsingShield` (CalcSetup: set true when the off-hand is a
    // shield). Determined from whether the current active equipment group's off-hand
    // slot has a shield-category base — a build-state default, consistent across the
    // whole build, not specialized.
    if main_hand_offhand_is_shield(build, data) {
        cfg = cfg.with_condition("UsingShield", true);
    }
    // Enemy within Presence (matching vendor CalcPerform.lua:524's
    // `condList["EnemyInPresence"] = PresenceRadius >= enemyDistance`): the default
    // Presence radius (a few meters) is always greater than the default enemy distance
    // → true by default, making the "Enemies in your Presence ..." enemy-side mod
    // family apply.
    // ponytail: pobr doesn't model a numeric PresenceRadius/enemyDistance comparison,
    // always sets it true; if a user pulls enemyDistance out far, the semantics gap is
    // left for the parity gate to flag before being wired up.
    if !cfg.conditions.contains_key("EnemyInPresence") {
        cfg = cfg.with_condition("EnemyInPresence", true);
    }
    // Companion-in-presence condition (matching vendor ConfigOptions.lua:1012-1014's
    // `companionInPresence`, defaultState=true, gated by ifSkillType=CreatesCompanion):
    // set true by default when an enabled skill includes `CreatesCompanion`, making the
    // "while your Companion is in your Presence" mod family apply (twister's tree node
    // Tree:37769's +10 INC). An explicit config input (the XML's `companionInPresence`)
    // takes priority; falls back to the default only when absent.
    if !cfg.conditions.contains_key("CompanionInPresence") && build_has_companion_skill(build, data)
    {
        cfg = cfg.with_condition("CompanionInPresence", true);
    }
    // The equipment condition for the "Body Armour grants <mod>" prefix family
    // (matching PoB2 ModParser.lua:1418 / :3255-3268's
    // `ItemCondition{itemSlot="Body Armour", rarityCond="NORMAL"}`): set true when the
    // body armour slot has an item equipped with Normal rarity. A build-state default,
    // consistent across the whole build, not specialized.
    if build
        .items
        .get(&EquipmentSlot::BodyArmour)
        .is_some_and(|item| item.rarity == pobr_data::item::ItemRarity::Normal)
    {
        cfg = cfg.with_condition("NormalBodyArmourEquipped", true);
    }
    // Main-hand weapon category → grip conditions (makes tree/mods like "... with
    // Quarterstaves" or "while Dual Wielding" apply). Cooldown-limited main skills
    // (grenades) are no longer a special case — the old "attack-speed compensates
    // throughput" approximation has been removed; the end of the speed chain uniformly
    // uses `min(rate, repeats/effective_cooldown)` (matching vendor's ordering), so
    // weapon-category attack-speed mods no longer incorrectly amplify grenade rate;
    // weapon-category conditions / weapon bit flags are enabled fully, matching vendor.
    for var in weapon_type_conditions(build, data) {
        cfg = cfg.with_condition(var, true);
    }
    // Main-hand weapon bits → cfg.flags: derived from the **same source**
    // (weapon_type_info table) with the **same gating** as the Using* conditions above
    // — the mod-side dual-written weapon-bit channel doesn't get a separate activation
    // path outside the condition channel.
    let weapon_bits = weapon_cfg_flags(build, data);
    if !weapon_bits.is_empty() {
        cfg.flags |= weapon_bits;
    }
    ctx.cfg = cfg;
}

/// Stage 4: weapon base assembly — main skill's use_time → action rate, skill damage
/// multiplier, main-/off-hand weapon base contribution converted into
/// [`pobr_core::calc::WeaponBase`] for HandSource, cooldown-bypass determination.
/// Depends on stage 1's main skill output; must run before session creation (base_input goes into `CalculationSession::new`).
fn stage_weapon_bases(ctx: &mut StageCtx<'_>) {
    let (build, data) = (ctx.build, ctx.data);
    if let Some((skill, _, skill_id)) = &ctx.main_skill
        && let Some(use_time) = skill.use_time_s
        && use_time > 0.0
    {
        if pobr_core::dbg_env!("POBR_DBG_SPEED").is_some() {
            eprintln!("[POBR_DBG_SPEED] main skill_id={skill_id} use_time={use_time}");
        }
        ctx.base_input.base_action_rate = 1.0 / use_time;
    }

    // Skill damage multiplier (PoB's baseMultiplier, e.g. a grenade's 7.57): scales weapon hit + added damage.
    ctx.dmg_mult = ctx
        .main_skill
        .as_ref()
        .map(|(s, _, _)| s.damage_multiplier)
        .filter(|m| *m > 0.0)
        .unwrap_or(1.0);

    // Weapon base contribution (attack skills only): hit physical damage (× skill
    // multiplier) + attack rate override. Uses the resolved real main skill id (meta
    // shells skipped), ensuring correct attack/spell determination and weighting.
    //
    // Weapon base no longer folds directly into `base_input`; it's now assembled into a
    // `HandSource` and injected via `set_hand_sources`, and `perform`'s internal
    // `run_hand_passes` injects the same set of values into a per-hand `MinimalInput`
    // copy — a single HandSource is value-for-value equal to the old conversion (a
    // direct pass-through, pinned by an equivalence test). The conversion semantics are
    // unchanged: phys × dmg_mult, attack_rate × attackSpeedMultiplier (matching
    // CalcOffence L2721-2723).
    ctx.weapon = ctx
        .main_skill
        .as_ref()
        .and_then(|(skill, _, skill_id)| weapon_contribution(build, data, skill_id, skill));
    // Dual-wielding off-hand: when the main hand is a real one-handed weapon and
    // Weapon2 is also a weapon base, assembles a second off-hand weapon source
    // (matching vendor's weapon2Attack pass, CalcOffence.lua:2369-2449).
    ctx.off_weapon = ctx
        .weapon
        .as_ref()
        .and_then(|_| dual_wield_off_hand_contribution(build, data, ctx.main_effect));
    let asm = ctx
        .main_skill
        .as_ref()
        .and_then(|(s, _, _)| s.attack_speed_multiplier)
        .map_or(1.0, |m| 1.0 + m / 100.0);
    let dmg_mult = ctx.dmg_mult;
    let to_hand_base = |w: &WeaponContribution| pobr_core::calc::WeaponBase {
        hit_min: w.phys_min * dmg_mult,
        hit_max: w.phys_max * dmg_mult,
        attack_rate: (w.attack_rate > 0.0).then_some(w.attack_rate * asm),
        crit_chance: w.crit_chance,
        flags: w.flags,
    };
    ctx.hand_weapon = ctx.weapon.as_ref().map(to_hand_base);
    ctx.off_hand_weapon = ctx.off_weapon.as_ref().map(to_hand_base);

    // Cooldown-limited rate: PoB's order — first fully compute every speed inc/more,
    // then apply `min(rate, 1/effective_cooldown)` (effective_cooldown shortened via
    // `CooldownRecovery`). This min is pushed down into offence.rs's
    // `apply_cooldown_cap`, which reads `SkillCooldownBase` BASE (injected by
    // `skill_base_modifiers`) + `CooldownRecovery` (aggregated across the whole
    // statmap/quality/tree/quest chain) + `SkillStoredUsesBase` (no rounding to a
    // frame when stored uses >1). Spells and cooldown-limited attacks (grenades)
    // uniformly go through this semantics (the old "attack speed compensates
    // throughput" pre-truncation approximation has been removed — the throughput
    // multiplier is now handled by GrenadeActivateTwice → dps_end_factors, matching
    // vendor CalcOffence.lua:2852-2856's ordering).
    //
    // Exception (bypasses cooldown): a skill whose cooldown resets by consuming charges
    // (e.g. Flicker Strike's `SkillConsumesPowerChargesOnUse`) → PoB2's Cooldown=nil,
    // fires at attack speed unrestricted → `CooldownBypass`.
    //
    // Whether the main skill bypasses cooldown (used the instant a charge is consumed,
    // e.g. Flicker) → injects `CooldownBypass` (single source).
    ctx.bypasses_cooldown = ctx
        .main_effect
        .map(|e| {
            e.skill_types
                .iter()
                .any(|t| t == "SkillConsumesPowerChargesOnUse")
        })
        .unwrap_or(false);
}

/// Stage 5: session creation + runtime rule-pack injection (constants / special /
/// parser rules, buff definitions / handlers, curse priority, rounding precision).
/// Rule injection must precede any subsequent `add_item` / `add_passive_nodes` /
/// `add_gem` (each injection point's comment notes the basis).
fn stage_create_session(ctx: &mut StageCtx<'_>) -> CalculationSession {
    let data = ctx.data;
    let mut session =
        CalculationSession::new(ctx.base_input).with_config(std::mem::take(&mut ctx.cfg));
    // The injection pipeline: injects the runtime constants bundle loaded by GameData
    // into calc (must come after with_config — with_config replaces cfg wholesale). The
    // data is value-for-value equal to the Default fallback, zero behavior change.
    session.set_constants(data.constants.clone());
    // Injects the data-driven ModParser engine rules (the sole parser, with the special
    // channel already compiled in). Must precede add_item/add_passive_nodes/add_gem
    // below. Missing parser_rules (an old data pack) = not injected — in that case
    // every mod is collected wholesale as Unsupported (has no effect, visible in the
    // unsupported report), with no more legacy fallback.
    if let Some(parser_rules) = &data.parser_rules {
        session.set_parser_rules(parser_rules.clone());
    }
    // Injects the built-in buff definitions + handler registry (the data/decision
    // source for env_finalize stage 6's doActorMisc-equivalent expansion). The whole
    // stage is gated by `cfg.mode_combat` — default false (B4's automatic activation is
    // its own behavior commit), so this injection is a zero behavior change.
    session.set_buff_definitions(data.buff_definitions.clone());
    session.set_buff_handler_registry(std::sync::Arc::new(crate::handlers::build_registry()));
    // Injects the curse priority data (the data source for env_finalize stage 4
    // buff_pass's curse priority/limit, following the buff_definitions channel's
    // precedent). The whole stage is gated by `cfg.mode_buffs` — default false, so this
    // injection is a zero behavior change; missing overlay file (old data pack) = None,
    // not injected (the consumer falls back to all weights being 0).
    if let Some(curse_priority) = &data.curse_priority {
        session.set_curse_priority(curse_priority.clone());
    }
    // Deduplicated wiring: injects the rounding-precision exception table (consumed by
    // buff_pass / merge_flasks_charms's ScaleAddMod scaling; T1's write primitive uses
    // the same rule set; the overlay data mirrors the earlier hardcoded name family,
    // value-for-value equal across every cataloged entry, verified by ninja_parity).
    session.set_high_precision_rules(data.high_precision.clone());
    session
}

/// Stage 6: weapon base injected via HandSource — depends on stage 4's converted WeaponBase.
///
/// Weapon base is injected via HandSource (a single-pass direct pass-through — OR mode
/// is value-for-value equal to the old base_input conversion). Dual wielding (Weapon2
/// is a weapon base) assembles a second off-hand HandSource, with per-hand weapon bits
/// following WeaponBase::flags into the hand pass; data channels like
/// doubleHitsWhenDualWielding are always false. A non-weapon attack's (Shield Wall type)
/// source is off-hand (matching PoB2 CalcOffence L2418-2431).
fn stage_hand_sources(session: &mut CalculationSession, ctx: &StageCtx<'_>) {
    if let Some(wb) = ctx.hand_weapon {
        let is_off_hand_source = ctx
            .main_effect
            .map(|e| e.is_attack() && e.is_non_weapon_attack())
            .unwrap_or(false);
        let sources = if is_off_hand_source {
            vec![pobr_core::calc::HandSource::off_hand(wb)]
        } else if let Some(ohb) = ctx.off_hand_weapon {
            vec![
                pobr_core::calc::HandSource::main_hand(wb),
                pobr_core::calc::HandSource::off_hand(ohb),
            ]
        } else {
            vec![pobr_core::calc::HandSource::main_hand(wb)]
        };
        session.set_hand_sources(sources, false);
    }
}

/// Stage 7: cooldown-bypass flag injection (determined in stage 4; `CooldownBypass`'s single source).
fn stage_cooldown_bypass(session: &mut CalculationSession, ctx: &StageCtx<'_>) {
    if ctx.bypasses_cooldown {
        let origin =
            ModifierSource::new(SourceId::new(SourceKind::SkillGem, "skill.cooldownBypass"))
                .with_raw_text("skill bypasses cooldown (consumes charges on use)");
        session.add_modifiers(vec![Modifier::flag("CooldownBypass").with_origin(origin)]);
    }
}

/// 2b. Jewels (passive tree/abyss sockets): mods injected **globally** (most jewels
///     are global mods; radius jewels are currently approximated as global too). Follows
///     add_item's skip-and-collect error tolerance.
fn stage_inject_jewels(
    session: &mut CalculationSession,
    ctx: &StageCtx<'_>,
) -> Result<(), BuildError> {
    let adorned_inc = adorned_corrupted_magic_jewel_inc(&ctx.build.jewels);
    for jewel in &ctx.build.jewels {
        let filtered = filter_item_parseable(jewel, engine_ctx(ctx.data));
        let texts: Vec<&str> = filtered
            .implicit_texts
            .iter()
            .chain(&filtered.modifier_texts)
            .chain(&filtered.enchant_texts)
            .map(String::as_str)
            .collect();
        // The Adorned (matching vendor CalcSetup.lua:944-948 + :1342-1347): every mod
        // on a **corrupted magic** jewel in a tree socket is scaled by `1 + N/100` on
        // injection (ScaleAddList semantics, value = trunc(round(v×scale, 2)),
        // ModStore.lua:70-79).
        // ponytail: doesn't model vendor's sinister/containJewelSocket slot exemptions
        // or the unscalable marker (no corpus source for them), wire up when the parity gate flags it.
        if let Some(inc) = adorned_inc
            && jewel.rarity == pobr_data::item::ItemRarity::Magic
            && jewel.corrupted
        {
            let scale = 1.0 + inc / 100.0;
            let parse_ctx = engine_ctx(ctx.data);
            let mut mods: Vec<pobr_core::Modifier> = Vec::new();
            for text in texts {
                let Ok(outcome) = parse_ctx.parse(text) else {
                    continue;
                };
                for mut m in outcome.mods {
                    if let pobr_core::ModValue::Number(v) = m.value {
                        m.value = pobr_core::ModValue::Number(scale_trunc_2dp(v, scale));
                    }
                    mods.push(m);
                }
            }
            session.add_modifiers(mods);
        } else {
            session
                .add_modifier_texts(texts)
                .map_err(|e| BuildError::Parse(e.to_string()))?;
        }
    }
    Ok(())
}

/// The value of The Adorned's "N% increased Effect of Jewel Socket Passive Skills
/// containing Corrupted Magic Jewels" among the jewel list (this mod wraps across two
/// physical lines in the XML; matched after joining with a space; vendor parses it as
/// `JewelData{corruptedMagicJewelIncEffect}`). Returns `None` when this jewel isn't present.
fn adorned_corrupted_magic_jewel_inc(jewels: &[Item]) -> Option<f64> {
    const SUFFIX: &str =
        "% increased Effect of Jewel Socket Passive Skills containing Corrupted Magic Jewels";
    for jewel in jewels {
        if jewel.rarity != pobr_data::item::ItemRarity::Unique {
            continue;
        }
        let joined = jewel.modifier_texts.join(" ");
        if let Some(pos) = joined.find(SUFFIX) {
            let head = &joined[..pos];
            let num_start = head
                .rfind(|c: char| !c.is_ascii_digit())
                .map_or(0, |i| i + 1);
            if let Ok(v) = head[num_start..].parse::<f64>() {
                return Some(v);
            }
        }
    }
    None
}

/// Vendor's `ModStore:ScaleAddMod` numeric scaling semantics (ModStore.lua:70-79):
/// `m_modf(round(v × scale, 2))` — rounds to 2 decimal places first, then truncates.
fn scale_trunc_2dp(value: f64, scale: f64) -> f64 {
    ((value * scale * 100.0).round() / 100.0).trunc()
}

/// 2b'. Radius jewels' `... Passive Skills in Radius also grant <mod>`: expanded by
///      the jewel socket's **allocated node count of the matching kind within radius** ×
///      the grant, injected as global modifier text (matching PoB2's geometric semantics).
///      Consistent with the equipment/passive path: hard-failing mods are filtered via
///      skip-and-collect first, so a single bad line doesn't abort the whole batch.
fn stage_inject_radius_jewels(
    session: &mut CalculationSession,
    ctx: &StageCtx<'_>,
) -> Result<(), BuildError> {
    let (build, data) = (ctx.build, ctx.data);
    let radius_texts = filter_parseable(radius_jewel_grant_texts(build, data), engine_ctx(data));
    if !radius_texts.is_empty() {
        let refs: Vec<&str> = radius_texts.iter().map(String::as_str).collect();
        session
            .add_modifier_texts(&refs)
            .map_err(|e| BuildError::Parse(e.to_string()))?;
    }
    Ok(())
}

/// 2c/2d/2e. config-derived mod injection: quest reward global text, the config
/// interpreter's player mods, the customMods line channel — all three share the
/// [`ResolvedConfig`](crate::config_resolve::ResolvedConfig) output, injected adjacent
/// to each other per the existing assembly order.
fn stage_inject_config_mods(
    session: &mut CalculationSession,
    ctx: &StageCtx<'_>,
) -> Result<(), BuildError> {
    let (data, resolved_config) = (ctx.data, &ctx.resolved_config);
    // 2c. Quest rewards / global config mods (PoB2's `questRewards`): injected as
    //     **global** modifier text (permanent global boosts to attributes / resistances
    //     / defence inc etc.). Follows add_modifier_texts's error tolerance. Quest
    //     still goes through the legacy text channel (dualrun report §3-⑤: not
    //     switched to declarative mods until vendor/parser naming is unified;
    //     `config_resolve` already excludes quest-attributed entries from the injection
    //     list to avoid double-counting).
    if !resolved_config.config.global_modifier_texts.is_empty() {
        // Consistent with the equipment/jewel path: hard-failing mods are filtered
        // first (skip-and-collect), so a single unparseable text doesn't abort the whole batch.
        let texts = filter_parseable(
            resolved_config.config.global_modifier_texts.clone(),
            engine_ctx(data),
        );
        if !texts.is_empty() {
            session
                .add_modifier_texts(&texts)
                .map_err(|e| BuildError::Parse(e.to_string()))?;
        }
    }

    // 2d. Output of the config interpreter: player modifiers attributed to
    //     `SourceKind::Config`. Combat-gated entries (`Condition:Combat` tag) are
    //     naturally inert under mode_combat=false (D5); the list is empty when the
    //     catalog is missing (tolerant fallback — conditions still go through
    //     `resolved_config.config` via the legacy channel).
    if !resolved_config.player_mods.is_empty() {
        session.add_modifiers(resolved_config.player_mods.clone());
    }

    // 2e. customMods line channel (commit ④, vendor ConfigOptions.lua:2278-2296:
    //     line-by-line StripEscapes + parseMod, source=Custom): the interpreter
    //     strips color codes then feeds lines to add_modifier_texts one at a time —
    //     unparseable lines naturally fall into the `ParseStatus::Unsupported`
    //     visibility channel (session.unsupported_modifier_texts); structurally
    //     hard-failing lines are skipped via filter_parseable (same treatment as
    //     the 2c quest / equipment text channels).
    if !resolved_config.custom_mod_lines.is_empty() {
        let texts = filter_parseable(resolved_config.custom_mod_lines.clone(), engine_ctx(data));
        if !texts.is_empty() {
            session
                .add_modifier_texts(&texts)
                .map_err(|e| BuildError::Parse(e.to_string()))?;
        }
    }
    Ok(())
}

/// 3/3a'/3b/3b'/3c. Passive tree injection: node mods (node-level attribution) →
/// anointed notables → small-passive effect scaling delta → radius-jewel Notable
/// effect scaling delta → mod-granted keystone mapping. Positioned per the
/// existing assembly order: after equipment and config injection, before skill gems.
fn stage_inject_passives(
    session: &mut CalculationSession,
    ctx: &StageCtx<'_>,
) -> Result<(), BuildError> {
    let (build, data) = (ctx.build, ctx.data);
    // 3. Passive tree: NodeId → node mod text (node-level attribution).
    let mut passive_nodes = resolve_passive_nodes(build, data);
    // 3a'. Anointed notables (vendor `Allocates <name>` enchant → `GrantedPassive`
    //      LIST, ModParser.lua:5809 → CalcSetup.lua:1322-1331 merges notableMap
    //      into allocNodes): matched by name against Notable nodes and appended as
    //      an AllocatedNode (same node-level attribution).
    append_granted_passives(build, data, &mut passive_nodes);
    let passive_nodes = passive_nodes;
    if !passive_nodes.is_empty() {
        session
            .add_passive_nodes(&passive_nodes)
            .map_err(|e| BuildError::Parse(e.to_string()))?;

        // 3b. Small-passive effect scaling (Titan's "Hulking Form" and similar
        //     "N% increased effect of Small Passive Skills"): vendor
        //     CalcSetup.lua:286-292 first sums the SmallPassiveSkillEffect INC
        //     across all allocated nodes, then :271-277 scales each "Normal,
        //     non-attribute, non-ascendancy" node's modList as a whole via
        //     ScaleAddList ×(1+inc/100) — the value scaling truncates
        //     ([`vendor_scale_mod_value`], e.g. 3×1.5=4.5→4). PoBR's equivalent:
        //     the base share is already injected at 1.0 (add_passive_nodes above),
        //     so here we append a **numeric delta copy** for affected small passives:
        //     `trunc(round(v×scale,2)) − v` (BASE/INC only; small passives have no
        //     MORE-type numeric mods, and flag copies would be a no-op, so both are skipped).
        let small_inc = small_passive_effect_inc(build, data);
        if small_inc > 0.0 {
            let small_scale = 1.0 + small_inc / 100.0;
            let small_nodes: Vec<AllocatedNode> = passive_nodes
                .iter()
                .filter(|n| {
                    data.passive_nodes.get(&n.node_id.0).is_some_and(|def| {
                        def.kind == pobr_data::catalog::PassiveNodeKind::Normal
                            && def.ascendancy_id.is_none()
                            && !is_attribute_node(def)
                    })
                })
                .cloned()
                .collect();
            if !small_nodes.is_empty() {
                let ingest = pobr_core::passive::ingest_passive_nodes_with_ctx(
                    &small_nodes,
                    engine_ctx(data),
                )
                .map_err(|e| BuildError::Parse(e.to_string()))?;
                let scaled: Vec<Modifier> = ingest
                    .modifiers
                    .into_iter()
                    .filter(|m| matches!(m.mod_type, ModType::Base | ModType::Inc))
                    .filter_map(|m| match m.value {
                        pobr_core::ModValue::Number(v) => {
                            let delta = vendor_scale_mod_value(v, small_scale) - v;
                            (delta != 0.0).then_some(Modifier {
                                value: pobr_core::ModValue::Number(delta),
                                ..m
                            })
                        }
                        _ => None,
                    })
                    .collect();
                session.add_modifiers(scaled);
            }
        }

        // 3b'. Radius-jewel Notable effect scaling (Time-Lost's "N% increased
        //      Effect of Notable Passive Skills in Radius"): appends a scaling
        //      delta copy for the own mods of allocated notables within radius
        //      (vendor CalcSetup.lua:246-275 ScaleAddList; the equivalent scaling
        //      on the granted-mod side is handled inline in radius_jewel_grant_texts).
        let notable_copies = radius_jewel_notable_effect_copies(build, data, &passive_nodes)?;
        if !notable_copies.is_empty() {
            session.add_modifiers(notable_copies);
        }
    }

    // 3c. Mod-granted keystone mapping: stats on tree keystone nodes (**excluding
    //     already-allocated ones**) are parsed into a keystone-name → mods map and
    //     injected via `session.set_keystone_mods`; consumed by merge_keystones in
    //     env_finalize stage 1/5 based on the player db's `Keystone` LIST entries
    //     ("You have <X>" / bare-name lines). Mods for already-allocated keystones
    //     are already injected by add_passive_nodes above, so excluding them from
    //     the map is PoBR's equivalent of PoB2's `env.keystonesAdded` dedup
    //     (CalcPerform.lua:66-76; see the keystone_merge.rs module doc for the
    //     tree-path modelling difference).
    session.set_keystone_mods(keystone_mod_map(data, &passive_nodes));
    Ok(())
}

/// 6. Inject extra global texts (campaign rewards / debug overrides).
fn stage_inject_extra_texts(
    session: &mut CalculationSession,
    ctx: &StageCtx<'_>,
) -> Result<(), BuildError> {
    if !ctx.options.extra_modifier_texts.is_empty() {
        session
            .add_modifier_texts(ctx.options.extra_modifier_texts.iter())
            .map_err(|e| BuildError::Parse(e.to_string()))?;
    }
    Ok(())
}

/// Diagnostic dumps (gated by env vars, for parity investigation; read-only, doesn't mutate session).
fn stage_debug_dumps(session: &CalculationSession) {
    // Diagnostic: POBR_DBG_UNSUPPORTED=1 dumps every unparsed modifier text (for parity investigation).
    if pobr_core::dbg_env!("POBR_DBG_UNSUPPORTED").is_some() {
        for t in session.unsupported_modifier_texts() {
            eprintln!("[POBR_UNSUP] {t}");
        }
    }
    // Diagnostic: POBR_DBG_ALLMODS=1 dumps the entire player ModDb (for diffing the
    // full mod set between engine and legacy ingest; used to locate fork(a) ingest
    // divergences). Sorted by name prefix to make sort+diff easy.
    if pobr_core::dbg_env!("POBR_DBG_ALLMODS").is_some() {
        for m in session.all_mods() {
            eprintln!(
                "[POBR_ALLMOD] {:?} {:?} {:?} flags={:?} kw={:?} tags={:?}",
                m.name, m.mod_type, m.value, m.flags, m.keyword_flags, m.tags
            );
        }
    }
    // Diagnostic: POBR_DBG_STAT=<ModName> dumps every modifier for that stat, per source (for parity investigation).
    if let Some(stat) = pobr_core::dbg_env!("POBR_DBG_STAT") {
        for m in session.mods_named(stat) {
            eprintln!(
                "[POBR_DBG] {stat} {:?} {:?} tags={:?} src={:?} origin={:?}",
                m.mod_type,
                m.value,
                m.tags,
                m.source,
                m.origin.as_ref().map(|o| &o.source_id)
            );
        }
    }
}

/// Returns a calc view with the locked Ring 3 item removed; avoids cloning the Build when no gating is needed.
fn gate_locked_ring3(build: &Build, data: &BuildData) -> Option<Build> {
    if !build.items.contains_key(&EquipmentSlot::Ring3)
        || additional_ring_slot_allocated(build, data)
    {
        return None;
    }

    let mut gated = build.clone();
    gated.items.remove(&EquipmentSlot::Ring3);
    Some(gated)
}

#[cfg(test)]
/// Shared engine rules for tests (real data directory, reused across the whole test binary).
pub(crate) fn test_parser_rules() -> std::sync::Arc<pobr_core::mod_parser::CompiledParserRules> {
    static RULES: std::sync::LazyLock<std::sync::Arc<pobr_core::mod_parser::CompiledParserRules>> =
        std::sync::LazyLock::new(|| {
            std::sync::Arc::new(pobr_core::mod_parser::test_compiled_rules())
        });
    RULES.clone()
}

#[cfg(test)]
mod ring3_tests {
    use super::{DataOrchestratorOptions, calculate_with_data};
    use crate::build::Build;
    use crate::build_data::BuildData;
    use pobr_core::calc::MinimalInput;
    use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};
    use pobr_data::passive_tree::{NodeId, PassiveTreeSpec};
    use std::collections::HashMap;

    fn life_ring() -> Item {
        Item {
            base: ItemBaseId::from("Ring"),
            rarity: ItemRarity::Rare,
            quality: 0,
            corrupted: false,
            implicit_texts: vec![],
            modifier_texts: vec!["+30 to maximum Life".into()],
            enchant_texts: vec![],
            rolled_defence: RolledDefence::default(),
            parsed_stats: vec![],
        }
    }

    fn ring_slot_data() -> BuildData {
        // A "+1 Ring Slot" mod node (modelled after Ritualist's "Unfurled Finger").
        let node = pobr_data::catalog::PassiveNodeDef {
            apply_to_armour: false,
            skill: 34785,
            id: "ascendancy_ritualist_unfurled_finger".into(),
            name: Some("Unfurled Finger".into()),
            kind: pobr_data::catalog::PassiveNodeKind::Notable,
            stats: vec!["+1 Ring Slot".into()],
            group: None,
            orbit: None,
            orbit_index: None,
            x: None,
            y: None,
            connections: vec![],
            ascendancy_id: Some("Huntress3".into()),
            variants: vec![],
        };
        let mut passive_nodes = HashMap::new();
        passive_nodes.insert(34785u32, node);
        BuildData {
            passive_nodes,
            parser_rules: Some(super::test_parser_rules()),
            ..BuildData::empty()
        }
    }

    fn base_opts() -> DataOrchestratorOptions {
        DataOrchestratorOptions {
            base_input: MinimalInput {
                base_life: 100.0,
                ..MinimalInput::default()
            },
            inject_character_base: false,
            ..Default::default()
        }
    }

    /// Without an allocated "+1 Ring Slot", the Ring 3 item is ignored entirely
    /// (same semantics as PoB2 CalcSetup.lua:821 "ignore item in Ring 3 if The
    /// Unseen Hand is not allocated").
    #[test]
    fn ring3_ignored_without_additional_ring_slot() {
        let build = Build::new().set_item(EquipmentSlot::Ring3, life_ring());
        let out = calculate_with_data(&build, &ring_slot_data(), &base_opts()).expect("calc");
        assert_eq!(out.life, 100.0, "未分配 +1 Ring Slot 时 Ring 3 不参与计算");
    }

    /// Ring 3 mods take effect once the "+1 Ring Slot" node is allocated.
    #[test]
    fn ring3_counts_with_additional_ring_slot() {
        let build = Build::new()
            .set_item(EquipmentSlot::Ring3, life_ring())
            .with_tree(PassiveTreeSpec {
                allocated_nodes: vec![NodeId(34785)],
                ..Default::default()
            });
        let out = calculate_with_data(&build, &ring_slot_data(), &base_opts()).expect("calc");
        assert_eq!(out.life, 130.0, "分配 +1 Ring Slot 后 Ring 3 词条生效");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::{CharacterIdentity, SocketGroup};
    use crate::build_data::ClassBaseAttributes;
    use pobr_core::CalcConfig;
    use pobr_core::calc::CalculationSession;
    use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};
    use pobr_data::passive_tree::{NodeId, PassiveTreeSpec};
    use pobr_gamedata::{GameData, repo_data_root};
    use std::collections::HashMap;

    /// Engine parse context for tests (real rules, compiled once and shared across the process).
    fn test_ctx() -> ParseCtx<'static> {
        use std::sync::LazyLock;
        static RULES: LazyLock<std::sync::Arc<pobr_core::mod_parser::CompiledParserRules>> =
            LazyLock::new(super::test_parser_rules);
        ParseCtx::with_engine(&RULES)
    }

    /// Wrapped tree-line merging (vendor PassiveTree.lua:445-462): when a single
    /// line fails to parse, it's joined with the next line and retried; if that
    /// also fails, the line is dropped and subsequent lines continue independently.
    #[test]
    fn combine_wrapped_then_filter_joins_wrapped_tree_lines() {
        // Demolitionist example: two lines = one mod (a `\n`-wrapped stat in the catalog).
        let joined = combine_wrapped_then_filter(
            vec![
                "Gain 4% of Damage as Extra Fire Damage for".into(),
                "every different Grenade fired in the past 8 seconds".into(),
            ],
            test_ctx(),
        );
        assert_eq!(
            joined,
            vec![
                "Gain 4% of Damage as Extra Fire Damage for every different Grenade fired in the past 8 seconds"
                    .to_string()
            ]
        );

        // Independently parseable lines are unaffected; lines that still fail after merging are dropped, as before.
        let mixed = combine_wrapped_then_filter(
            vec![
                "10% increased Damage".into(),
                "this line is not a known modifier".into(),
                "+50 to maximum Life".into(),
            ],
            test_ctx(),
        );
        assert_eq!(
            mixed,
            vec![
                "10% increased Damage".to_string(),
                "+50 to maximum Life".to_string()
            ]
        );
    }

    fn life_item(amount: &str) -> Item {
        Item {
            base: ItemBaseId::from("Iron Ring"),
            rarity: ItemRarity::Rare,
            quality: 0,
            corrupted: false,
            implicit_texts: vec![],
            modifier_texts: vec![format!("+{amount} to maximum Life")],
            enchant_texts: vec![],
            rolled_defence: RolledDefence::default(),
            parsed_stats: vec![],
        }
    }

    /// Anointed notables feed into the GemProperty scan (vendor: a granted node's
    /// modList joins the global modDB just like an allocated node,
    /// CalcSetup.lua:1322-1331 + applyGemMods): an amulet enchant "Allocates
    /// Paragon" → the anoint-pool node 20686 (backfilled via --tree-anoints) whose
    /// `+5% to Quality of all Skills` should produce a Quality +5 catch-all mod.
    #[test]
    fn granted_anoint_notable_feeds_gem_property_scan() {
        let data = repo_data();
        let amulet = Item {
            base: ItemBaseId::from("Solar Amulet"),
            rarity: ItemRarity::Rare,
            quality: 0,
            corrupted: false,
            implicit_texts: vec![],
            modifier_texts: vec![],
            enchant_texts: vec!["Allocates Paragon".into()],
            rolled_defence: RolledDefence::default(),
            parsed_stats: vec![],
        };
        let build = Build::new().set_item(EquipmentSlot::Amulet, amulet);

        // Name resolution: Paragon = anoint-pool node 20686 (not on the main tree, reachable only via a grant).
        let defs = granted_passive_defs(&build, &data);
        assert_eq!(
            defs.iter().map(|d| d.skill).collect::<Vec<_>>(),
            vec![20686],
            "Allocates Paragon 应解析到油涂池节点 20686"
        );

        // GemProperty scan: +5% Quality (bare "all Skills", no attribute requirement).
        let bonuses = gem_property_bonuses(&build, &data);
        assert!(
            bonuses.contains(&GemPropertyBonus {
                value: 5,
                kind: GemPropertyKind::Quality,
                category: String::new(),
                attr_req: None,
            }),
            "授予 Paragon 应产出 Quality +5 全匹配词条，实得 {bonuses:?}"
        );

        // Idempotency: allocating the same node on the tree must not double-count it.
        let allocated = Build::new()
            .set_item(EquipmentSlot::Amulet, {
                let mut a = build.items[&EquipmentSlot::Amulet].clone();
                a.enchant_texts = vec!["Allocates Paragon".into()];
                a
            })
            .with_tree(PassiveTreeSpec {
                allocated_nodes: vec![NodeId(20686)],
                ..Default::default()
            });
        let quality_count = gem_property_bonuses(&allocated, &data)
            .iter()
            .filter(|b| b.kind == GemPropertyKind::Quality && b.value == 5)
            .count();
        assert_eq!(quality_count, 1, "已分配 + 授予应只计一次");
    }

    fn repo_data() -> BuildData {
        let data = GameData::new(repo_data_root().join(pobr_gamedata::data_version()));
        BuildData::load(&data).expect("load repo build data")
    }

    // Text-only path (backward compatible, preserves the existing assertions)

    #[test]
    fn calculates_with_life_modifier() {
        let build = Build::new()
            .with_character(CharacterIdentity {
                level: 1,
                class_name: "Ranger".into(),
                ascendancy_name: String::new(),
            })
            .set_item(EquipmentSlot::Ring1, life_item("50"));

        let opts = OrchestratorOptions {
            base_input: MinimalInput {
                base_life: 100.0,
                ..MinimalInput::default()
            },
            extra_modifier_texts: vec![],
        };

        let out = calculate(&build, &opts).expect("calc");
        assert_eq!(out.life, 150.0);
    }

    #[test]
    fn empty_build_calculates_base() {
        let build = Build::new();
        let opts = OrchestratorOptions {
            base_input: MinimalInput {
                base_life: 80.0,
                ..MinimalInput::default()
            },
            extra_modifier_texts: vec![],
        };
        let out = calculate(&build, &opts).expect("calc");
        assert_eq!(out.life, 80.0);
    }

    // End-to-end attribution path (calculate_with_data)

    #[test]
    fn data_path_item_life_matches_text_path() {
        // Equipment goes through the add_item attribution path; values should match the text-only path.
        let build = Build::new().set_item(EquipmentSlot::Ring1, life_item("50"));
        let data = BuildData {
            parser_rules: Some(super::test_parser_rules()),
            ..BuildData::empty()
        };
        let opts = DataOrchestratorOptions {
            base_input: MinimalInput {
                base_life: 100.0,
                ..MinimalInput::default()
            },
            inject_character_base: false,
            ..Default::default()
        };
        let out = calculate_with_data(&build, &data, &opts).expect("calc");
        assert_eq!(out.life, 150.0);
    }

    #[test]
    fn character_base_injects_life_from_class_and_level() {
        // Derive CharacterBase from the injected class-attributes table; life = 28 + 12*level + 2*str.
        let mut class_attributes = HashMap::new();
        class_attributes.insert(
            "Warrior".to_string(),
            ClassBaseAttributes {
                strength: 15,
                dexterity: 7,
                intelligence: 7,
            },
        );
        let data = BuildData {
            class_attributes,
            ..BuildData::empty()
        };
        let build = Build::new().with_character(CharacterIdentity {
            level: 10,
            class_name: "Warrior".into(),
            ascendancy_name: String::new(),
        });

        let opts = DataOrchestratorOptions {
            inject_character_base: true,
            ..Default::default()
        };
        let out = calculate_with_data(&build, &data, &opts).expect("calc");
        // 12*10 + 16 + 2*15 = 166 (PoB2 `Life BASE 12 × Level + 16`).
        assert_eq!(out.life, 166.0);

        // Injection off → no CharacterBase life.
        let opts_off = DataOrchestratorOptions {
            inject_character_base: false,
            ..Default::default()
        };
        let out_off = calculate_with_data(&build, &data, &opts_off).expect("calc");
        assert_eq!(out_off.life, 0.0);
        assert!(out.life > out_off.life, "CharacterBase 生效抬升生命");
    }

    #[test]
    fn passive_node_contributes_attributed_life() {
        // Build a Normal node carrying +30 maximum Life; allocating it should raise life.
        let node = pobr_data::catalog::PassiveNodeDef {
            apply_to_armour: false,
            skill: 12345,
            id: "test_life_node".into(),
            name: Some("Life Node".into()),
            kind: pobr_data::catalog::PassiveNodeKind::Normal,
            stats: vec!["+30 to maximum Life".into()],
            group: None,
            orbit: None,
            orbit_index: None,
            x: None,
            y: None,
            connections: vec![],
            ascendancy_id: None,
            variants: vec![],
        };
        let mut passive_nodes = HashMap::new();
        passive_nodes.insert(12345u32, node);
        let data = BuildData {
            passive_nodes,
            parser_rules: Some(super::test_parser_rules()),
            ..BuildData::empty()
        };

        let build = Build::new().with_tree(PassiveTreeSpec {
            allocated_nodes: vec![NodeId(12345)],
            ..Default::default()
        });

        let opts = DataOrchestratorOptions {
            base_input: MinimalInput {
                base_life: 100.0,
                ..MinimalInput::default()
            },
            inject_character_base: false,
            ..Default::default()
        };
        let out = calculate_with_data(&build, &data, &opts).expect("calc");
        assert_eq!(out.life, 130.0, "节点 +30 生命经节点归因路径生效");
    }

    /// Build a Normal node with coordinates (defaults to a +5 to maximum Life mod; overridable).
    fn normal_node_at(
        skill: u32,
        x: f64,
        y: f64,
        stats: Vec<String>,
    ) -> pobr_data::catalog::PassiveNodeDef {
        pobr_data::catalog::PassiveNodeDef {
            apply_to_armour: false,
            skill,
            id: format!("n{skill}"),
            name: None,
            kind: pobr_data::catalog::PassiveNodeKind::Normal,
            stats,
            group: None,
            orbit: None,
            orbit_index: None,
            x: Some(x),
            y: Some(y),
            connections: vec![],
            ascendancy_id: None,
            variants: vec![],
        }
    }

    /// Dedicated unit test for the attribute-small-passive predicate.
    #[test]
    fn is_attribute_node_matches_any_attribute_choice() {
        let attr = normal_node_at(1, 0.0, 0.0, vec!["+5 to any Attribute".into()]);
        let attrs = normal_node_at(2, 0.0, 0.0, vec!["+5 to any Attributes".into()]);
        let life = normal_node_at(3, 0.0, 0.0, vec!["+5 to maximum Life".into()]);
        assert!(super::is_attribute_node(&attr));
        assert!(super::is_attribute_node(&attrs));
        assert!(!super::is_attribute_node(&life));
    }

    /// **Dedicated regression: radius-jewel attribute miscounting** (a named
    /// roadmap acceptance item).
    ///
    /// The Small count for `Small Passive Skills in Radius also grant <mod>` must
    /// exclude attribute small passives (vendor ModParser.lua:6855-6857
    /// `node.type=="Normal" and not node.isAttribute`). With 1 normal life small
    /// passive + 1 attribute-choice small passive in radius, the grant count
    /// should be 1 (not 2).
    #[test]
    fn radius_small_grant_excludes_attribute_nodes() {
        let socket = 100u32;
        // All three nodes sit near the socket (distance << any radius tier).
        let mut passive_nodes = HashMap::new();
        // The socket node itself (Normal; excluded from the geometry calc by definition).
        passive_nodes.insert(socket, normal_node_at(socket, 0.0, 0.0, vec![]));
        // A normal life small passive (should count toward Small).
        passive_nodes.insert(
            101,
            normal_node_at(101, 50.0, 0.0, vec!["+5 to maximum Life".into()]),
        );
        // An attribute-choice small passive (must be excluded).
        passive_nodes.insert(
            102,
            normal_node_at(102, 0.0, 50.0, vec!["+5 to any Attribute".into()]),
        );

        let data = BuildData {
            passive_nodes,
            ..BuildData::empty()
        };

        let jewel = RadiusJewel {
            socket_node: socket,
            radius_label: Some("Large".into()),
            grant_lines: vec![
                "Small Passive Skills in Radius also grant +10 to maximum Mana".into(),
            ],
            notable_effect_inc: 0,
        };
        let build = Build::new()
            .with_tree(PassiveTreeSpec {
                allocated_nodes: vec![NodeId(socket), NodeId(101), NodeId(102)],
                ..Default::default()
            })
            .with_radius_jewels(vec![jewel]);

        let texts = radius_jewel_grant_texts(&build, &data);
        // Only 1 non-attribute Small node → the grant text appears once (the attribute small passive is excluded, otherwise it would be 2).
        let count = texts
            .iter()
            .filter(|t| t.contains("+10 to maximum Mana"))
            .count();
        assert_eq!(
            count, 1,
            "属性三选一小点不应计入 Small grant 份数；得到 {texts:?}"
        );
    }

    #[test]
    fn unknown_passive_node_is_skipped() {
        // Allocating a node absent from the node table → skipped, no error, life stays at base.
        let data = BuildData::empty();
        let build = Build::new().with_tree(PassiveTreeSpec {
            allocated_nodes: vec![NodeId(99999)],
            ..Default::default()
        });
        let opts = DataOrchestratorOptions {
            base_input: MinimalInput {
                base_life: 100.0,
                ..MinimalInput::default()
            },
            inject_character_base: false,
            ..Default::default()
        };
        let out = calculate_with_data(&build, &data, &opts).expect("calc");
        assert_eq!(out.life, 100.0);
    }

    #[test]
    fn gems_classified_and_do_not_error() {
        // An enabled socket group (one active + one support); classification doesn't error, and these gems carry no mods → life is unchanged.
        let mut skill_gems = HashMap::new();
        skill_gems.insert(
            "ActiveGem".to_string(),
            pobr_data::catalog::SkillGemDef {
                id: "ActiveGem".into(),
                gem_type: Some(0),
                gem_colour: Some(1),
                min_level_req: 1,
                str_pct: 0,
                dex_pct: 0,
                int_pct: 0,
                is_support: false,
                granted_effect_id: None,
                additional_granted_effect_ids: Vec::new(),
            },
        );
        skill_gems.insert(
            "SupportGem".to_string(),
            pobr_data::catalog::SkillGemDef {
                id: "SupportGem".into(),
                gem_type: Some(1),
                gem_colour: Some(1),
                min_level_req: 1,
                str_pct: 0,
                dex_pct: 0,
                int_pct: 0,
                is_support: true,
                granted_effect_id: None,
                additional_granted_effect_ids: Vec::new(),
            },
        );
        let data = BuildData {
            skill_gems,
            ..BuildData::empty()
        };
        let build = Build::new().add_socket_group(
            SocketGroup::new()
                .with_gem("ActiveGem")
                .with_gem("SupportGem"),
        );
        let opts = DataOrchestratorOptions {
            base_input: MinimalInput {
                base_life: 100.0,
                ..MinimalInput::default()
            },
            inject_character_base: false,
            ..Default::default()
        };
        let out = calculate_with_data(&build, &data, &opts).expect("calc");
        assert_eq!(out.life, 100.0);
    }

    #[test]
    fn mode_effective_changes_hit_chance_vs_panel() {
        // (Semantics update) Non-attacks always hit: a build with no main skill (so
        // non-attack) skips the accuracy/evasion check, and both hit_chance readings
        // are 1 (vendor CalcOffence.lua:2611-2612
        // `if not isAttack then output.AccuracyHitChance = 100`).
        let data = BuildData::empty();
        let build = Build::new();
        let base = MinimalInput {
            base_accuracy: 1000.0,
            base_hit_min: 100.0,
            base_hit_max: 100.0,
            base_action_rate: 1.0,
            ..MinimalInput::default()
        };

        for mode_effective in [false, true] {
            let out = calculate_with_data(
                &build,
                &data,
                &DataOrchestratorOptions {
                    base_input: base,
                    inject_character_base: false,
                    mode_effective,
                    enemy_level: 80,
                    enemy_tier: EnemyTier::Pinnacle,
                    ..Default::default()
                },
            )
            .expect("calc");
            assert_eq!(
                out.hit_chance, 1.0,
                "非攻击必中（vendor :2611）：mode_effective={mode_effective}"
            );
        }

        // Attack context (CalcConfig::attack() sets SkillTypes::ATTACK): enemy
        // evasion enters the accuracy formula → hit_chance < 1 (PoE2 formula
        // acc*1.25/(acc+eva*0.3), CalcDefence.lua:32-38).
        let mut session = CalculationSession::new(base)
            .with_config(CalcConfig::attack().with_mode_effective(true));
        session.setup_enemy(80, EnemyTier::Pinnacle);
        let out = session.perform_minimal();
        assert!(
            out.hit_chance < 1.0,
            "攻击应做精准/闪避检定：hit_chance={}",
            out.hit_chance
        );
    }

    /// (Regression pin) main-skill type drives the accuracy check: a Spell main
    /// skill always hits (hit_chance=1, vendor CalcOffence.lua:2611-2612), an
    /// Attack main skill does the accuracy/evasion check (<1). Pins the fix for
    /// "orchestration failed to fill cfg.skill_types → spells got pulled into the accuracy formula".
    #[test]
    fn spell_main_skill_skips_accuracy_check_attack_does_not() {
        let data = repo_data();
        let base = MinimalInput {
            base_accuracy: 1000.0,
            base_hit_min: 100.0,
            base_hit_max: 100.0,
            base_action_rate: 1.0,
            ..MinimalInput::default()
        };
        let run = |skill: &str| {
            let build = Build::new()
                .add_socket_group(SocketGroup::new().with_gem_skill(skill, 10))
                .with_main_socket_group(1);
            calculate_with_data(
                &build,
                &data,
                &DataOrchestratorOptions {
                    base_input: base,
                    inject_character_base: false,
                    mode_effective: true,
                    enemy_level: 80,
                    enemy_tier: EnemyTier::Pinnacle,
                    ..Default::default()
                },
            )
            .expect("calc")
        };
        // FireballPlayer: a projectile spell; ArmourBreakerPlayer: a melee attack (both real data).
        assert_eq!(
            run("FireballPlayer").hit_chance,
            1.0,
            "法术必中（vendor :2611）"
        );
        assert!(
            run("ArmourBreakerPlayer").hit_chance < 1.0,
            "攻击应做精准/闪避检定"
        );
    }

    /// Attribute-derived stats consume the **final** attribute value (PoB2
    /// CalcPerform.lua:381-388 `round(calcLib.val(modDB, stat))` + :424-431 Life
    /// from Str×2): `N% increased Strength` must scale the full BASE — including
    /// the class starting value — before it feeds into derivation.
    #[test]
    fn attribute_increased_modifiers_scale_derived_life() {
        let data = repo_data();
        let character = CharacterIdentity {
            level: 1,
            class_name: "Warrior".into(),
            ascendancy_name: String::new(),
        };
        let run = |texts: Vec<String>| {
            let build = Build::new().with_character(character.clone());
            calculate_with_data(
                &build,
                &data,
                &DataOrchestratorOptions {
                    extra_modifier_texts: texts,
                    ..Default::default()
                },
            )
            .expect("calc")
        };

        let base = run(vec!["+100 to Strength".into()]);
        let inc = run(vec![
            "+100 to Strength".into(),
            "50% increased Strength".into(),
        ]);

        let cls_str = f64::from(
            data.class_attributes("Warrior")
                .expect("warrior attrs")
                .strength,
        );
        // Δlife = life_per_strength × (round((cls+100)×1.5) − (cls+100)).
        let expected = 2.0 * (((cls_str + 100.0) * 1.5).round() - (cls_str + 100.0));
        assert_eq!(inc.life - base.life, expected);
    }

    #[test]
    fn setup_enemy_session_method_is_exposed() {
        // setup_enemy is exposed via the session and usable standalone (minimal smoke test for the attribution path).
        let mut session = CalculationSession::new(MinimalInput {
            base_accuracy: 1000.0,
            base_hit_min: 50.0,
            base_hit_max: 50.0,
            base_action_rate: 1.0,
            ..MinimalInput::default()
        })
        .with_config(CalcConfig::attack().with_mode_effective(true));
        session.setup_enemy(80, EnemyTier::Pinnacle);
        let out = session.perform_minimal();
        assert!(out.hit_chance <= 1.0);
    }

    #[test]
    fn resistance_penalty_follows_campaign_progress() {
        // resistancePenalty wiring (19-G5): unconfigured → PoB2 defaults to Endgame
        // (-60); explicit Act1 → 0 penalty, all three elemental resistances 60 points
        // higher (chaos has no penalty, so it's unaffected either way).
        let data = repo_data();
        let character = CharacterIdentity {
            level: 90,
            class_name: "Ranger".into(),
            ascendancy_name: String::new(),
        };
        let opts = DataOrchestratorOptions::default();

        let build = Build::new().with_character(character.clone());
        let endgame = calculate_with_data(&build, &data, &opts).expect("endgame calc");

        let mut act1_build = Build::new().with_character(character);
        act1_build.config.campaign_progress = Some(CampaignProgress::Act1);
        let act1 = calculate_with_data(&act1_build, &data, &opts).expect("act1 calc");

        assert_eq!(act1.fire_resistance - endgame.fire_resistance, 60.0);
        assert_eq!(act1.cold_resistance - endgame.cold_resistance, 60.0);
        assert_eq!(
            act1.lightning_resistance - endgame.lightning_resistance,
            60.0
        );
    }

    #[test]
    fn xml_enemy_tier_overrides_orchestrator_option() {
        // enemyIsBoss wiring (19-G3): an explicit None tier in the build XML config
        // should override the caller-supplied Pinnacle — a normal monster's
        // dps_mult (1/4.4) is far below Pinnacle's (8/4.4), so the EHP pipeline's
        // total incoming hit damage should be lower. (A build with no main skill is
        // non-attack and always hits, so hit_chance no longer distinguishes
        // tiers; physical damage reduction hits the same DR cap at both tiers, so
        // incoming enemy damage is used as the observation point instead.)
        let data = BuildData::empty();
        let base = MinimalInput {
            base_accuracy: 1000.0,
            base_hit_min: 100.0,
            base_hit_max: 100.0,
            base_action_rate: 1.0,
            ..MinimalInput::default()
        };
        let opts = DataOrchestratorOptions {
            base_input: base,
            inject_character_base: false,
            mode_effective: true,
            enemy_level: 80,
            enemy_tier: EnemyTier::Pinnacle,
            ..Default::default()
        };

        // XML omits enemyIsBoss → falls back to the option's Pinnacle.
        let pinnacle_build = Build::new();
        let pinnacle = calculate_with_data(&pinnacle_build, &data, &opts).expect("pinnacle calc");

        // XML explicitly sets enemyIsBoss=None → overrides the option's tier.
        let mut none_build = Build::new();
        none_build.config.enemy_tier = Some(EnemyTier::None);
        let none = calculate_with_data(&none_build, &data, &opts).expect("none-tier calc");

        assert!(
            pinnacle.total_enemy_damage_in > none.total_enemy_damage_in,
            "Pinnacle 档敌伤（dps_mult 8/4.4）应高于普通怪档（1/4.4）：none={} pinnacle={}",
            none.total_enemy_damage_in,
            pinnacle.total_enemy_damage_in,
        );
    }

    #[test]
    fn full_repo_data_end_to_end_smoke() {
        // End-to-end run with real repo data: a class + one item + a real node, must not panic and must produce a finite value.
        let data = repo_data();
        // Pick a real Normal node that has stats.
        let (skill, _) = data
            .passive_nodes
            .iter()
            .find(|(_, n)| {
                n.kind == pobr_data::catalog::PassiveNodeKind::Normal && !n.stats.is_empty()
            })
            .expect("a normal node with stats exists");
        let build = Build::new()
            .with_character(CharacterIdentity {
                level: 90,
                class_name: "Ranger".into(),
                ascendancy_name: String::new(),
            })
            .set_item(EquipmentSlot::Ring1, life_item("80"))
            .with_tree(PassiveTreeSpec {
                allocated_nodes: vec![NodeId(*skill)],
                ..Default::default()
            });
        let opts = DataOrchestratorOptions {
            base_input: MinimalInput {
                base_life: 50.0,
                ..MinimalInput::default()
            },
            inject_character_base: true,
            mode_effective: true,
            enemy_level: 80,
            enemy_tier: EnemyTier::Pinnacle,
            ..Default::default()
        };
        let out = calculate_with_data(&build, &data, &opts).expect("end-to-end calc");
        // CharacterBase (level 90 Ranger: 28 + 1080 + 2*7=14 = 1122) + ring 80 ≥ contribution from the equipment.
        assert!(out.life >= 1122.0 + 80.0, "life={}", out.life);
        assert!(out.life.is_finite());
    }

    #[test]
    fn mark_gem_injects_offensive_gain_as_buff() {
        // Data-driven: an enabled Freezing Mark (grants the player a 30%
        // gain-as-cold buff on freeze hits) should produce one DamageGainAsCold
        // BASE=30 modifier; a build without the Mark produces none. Never
        // hardcoded against the gem's name.
        let data = repo_data();
        // Precondition: Freezing Mark is not an aura (it's a Mark/Buff, no Aura tag), and its stat-set contains the target buff stat.
        assert!(
            !data.is_aura("FreezingMarkPlayer"),
            "Freezing Mark 非光环（Mark/Buff）"
        );

        let build = Build::new()
            .with_character(CharacterIdentity {
                level: 90,
                class_name: "Ranger".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(SocketGroup::new().with_gem_skill("FreezingMarkPlayer", 20));

        let mods = self_buff_offensive_modifiers(&build, &data);
        let cold: f64 = mods
            .iter()
            .filter(|m| m.name.as_str() == "DamageGainAsCold")
            .filter_map(|m| m.value.as_number())
            .sum();
        assert_eq!(
            cold, 30.0,
            "Freezing Mark 应给 30% gain-as-cold，实得 {cold}"
        );

        // A build without a Mark produces no offensive self-buff.
        let bare = Build::new().with_character(CharacterIdentity {
            level: 90,
            class_name: "Ranger".into(),
            ascendancy_name: String::new(),
        });
        assert!(
            self_buff_offensive_modifiers(&bare, &data).is_empty(),
            "无 Mark build 不应产出 gain-as buff"
        );
    }

    /// T1.7: the main skill's quality tier is injected via the stat-map, with
    /// trunc truncation + `SourceKind::GemQuality` attribution (id prefix
    /// `gem.<effect id>.q<Q>`). Uses a synthetic quality entry (damage_+% is
    /// mappable), so it doesn't depend on whether any real gem's quality stat is already mapped.
    #[test]
    fn main_skill_quality_modifiers_truncate_and_attribute_gem_quality() {
        use pobr_data::catalog::QualityStat;
        let mut data = repo_data();
        data.gem_quality_stats.insert(
            "FireballPlayer".into(),
            vec![QualityStat {
                stat: "damage_+%".into(),
                per_quality_rate: 0.55,
                alt: false,
            }],
        );
        // Calling the number-crunching directly (not via calculate_with_data):
        // manually install the Data-channel context (after the T2.4 switchover the
        // data engine is the default; the catalog comes from the directory BuildData loads alongside the data pack).
        let _guard =
            install_stat_map_context(StatMapMode::default(), data.stat_map_catalog.clone());
        // q19: trunc(0.55 × 19) = trunc(10.45) = 10 (math.modf semantics, not round).
        let group = SocketGroup::new().with_gem_skill_quality("FireballPlayer", 20, 19);
        let mods = main_skill_quality_modifiers(&group, &data, "FireballPlayer");
        assert_eq!(mods.len(), 1, "damage_+% 应映射为一条 Damage INC");
        let m = &mods[0];
        assert_eq!(m.name.as_str(), "Damage");
        assert_eq!(m.mod_type, ModType::Inc);
        assert_eq!(m.value.as_number(), Some(10.0), "trunc(0.55×19)=10");
        let origin = m.origin.as_ref().expect("带归因");
        assert_eq!(origin.source_id.kind, SourceKind::GemQuality);
        assert!(
            origin.source_id.id.starts_with("gem.FireballPlayer.q19"),
            "归因 id 前缀 gem.<id>.q<Q>，实得 {}",
            origin.source_id.id
        );

        // Quality 0: no quality modifier produced.
        let group0 = SocketGroup::new().with_gem_skill("FireballPlayer", 20);
        assert!(main_skill_quality_modifiers(&group0, &data, "FireballPlayer").is_empty());
    }

    /// The per-set override key of the selected statSet threads through to the
    /// engine's set_key — with statSetIndex=2, the same stat routes to set "2"'s
    /// override entry (a synthetic catalog); by default it routes to set "1"/global.
    #[test]
    fn selected_set_key_threads_per_set_override() {
        use pobr_core::rules::stat_map_engine::StatMapCatalog;
        let mut data = repo_data();
        // A synthetic multi-set effect: primary set at vendor index 1, an additional set at index 2 (same stat).
        data.skill_stat_sets.insert(
            "SynthEff".to_string(),
            pobr_data::catalog::SkillStatSetDef {
                effect_id: "SynthEff".into(),
                sets: vec![
                    synth_stat_set("SynthMain", Some(1)),
                    synth_stat_set("SynthAlt", Some(2)),
                ],
            },
        );
        // Synthetic catalog: global → Damage INC; per-set "2" override → ColdDamage INC.
        let catalog: StatMapCatalog = StatMapCatalog::new(
            serde_json::from_str(
                r#"{
                  "global": { "synth_stat_+%": { "mods": [
                      { "kind": "mod", "name": "Damage", "mod_type": "INC" } ] } },
                  "per_stat_set": { "SynthEff": { "2": { "synth_stat_+%": { "mods": [
                      { "kind": "mod", "name": "ColdDamage", "mod_type": "INC" } ] } } } }
                }"#,
            )
            .expect("合成 statmap 合法"),
        );
        let _guard =
            install_stat_map_context(StatMapMode::default(), Some(std::sync::Arc::new(catalog)));
        let skill = ResolvedSkillLevel {
            base_damage: vec![pobr_data::catalog::SkillDamageStat {
                stat: "synth_stat_+%".into(),
                value: 25.0,
            }],
            damage_multiplier: 1.0,
            ..Default::default()
        };
        // statSetIndex=2 → the per-set override hits (ColdDamage).
        let set_key = data.selected_set_key("SynthEff", Some(2));
        assert_eq!(set_key.as_deref(), Some("2"));
        let mods = skill_base_modifiers(&skill, "SynthEff", set_key.as_deref());
        let mapped: Vec<&str> = mods
            .iter()
            .filter(|m| m.mod_type == ModType::Inc)
            .map(|m| m.name.as_str())
            .collect();
        assert_eq!(mapped, vec!["ColdDamage"], "set 2 覆盖应命中");
        // Default (primary set, key "1", no override) → falls back to global (Damage).
        let set_key = data.selected_set_key("SynthEff", None);
        assert_eq!(set_key.as_deref(), Some("1"));
        let mods = skill_base_modifiers(&skill, "SynthEff", set_key.as_deref());
        let mapped: Vec<&str> = mods
            .iter()
            .filter(|m| m.mod_type == ModType::Inc)
            .map(|m| m.name.as_str())
            .collect();
        assert_eq!(mapped, vec!["Damage"], "缺省应落回 global");
    }

    /// A synthetic stat set (shared across tests): a single level row of `synth_stat_+%`.
    fn synth_stat_set(set_id: &str, vendor_idx: Option<u32>) -> pobr_data::catalog::StatSetDef {
        pobr_data::catalog::StatSetDef {
            set_id: set_id.into(),
            label: None,
            vendor_set_index: vendor_idx,
            base_effectiveness: 0.0,
            constant_stats: Vec::new(),
            skill_attack_speed_more: None,
            dot_flags: Default::default(),
            explode_corpse: false,
            implicit_stats: Vec::new(),
            levels: vec![pobr_data::catalog::SkillStatSetLevel {
                gem_level: 1,
                damage_multiplier: 1.0,
                stats: vec![pobr_data::catalog::SkillDamageStat {
                    stat: "synth_stat_+%".into(),
                    value: 25.0,
                }],
            }],
        }
    }

    /// Global-only merge for the main skill's unselected statSet — the full path
    /// is reachable with real data (FlameWall as the multi-set carrier); gating on
    /// the GlobalEffect tag is a translation boundary (see the switchover log
    /// §5), so current injection is always zero (the structure is in place but
    /// doesn't compute wrong values). Non-global stats are never injected from an unselected set.
    #[test]
    fn unselected_set_global_only_zero_injection_before_m3() {
        let data = repo_data();
        // Precondition: FlameWall really is multi-set (vendor export has ≥2), and its unselected-set snapshot is non-empty.
        let unsel = data.unselected_set_stats("FlameWallPlayer", 20, 0, None);
        assert!(
            !unsel.is_empty(),
            "FlameWallPlayer 应有未选 set（set 2 = 投射物 buff 形态）"
        );
        let _guard =
            install_stat_map_context(StatMapMode::default(), data.stat_map_catalog.clone());
        let group = SocketGroup::new().with_gem_skill("FlameWallPlayer", 20);
        let mods = unselected_set_global_modifiers(&group, &data, "FlameWallPlayer");
        assert!(
            mods.is_empty(),
            "M3 接通 GlobalEffect tag 前未选 set 注入应为零，实得 {mods:?}"
        );
        // Builder path (no gem_skills): no statSet context → empty.
        let empty_group = SocketGroup::new();
        assert!(unselected_set_global_modifiers(&empty_group, &data, "FlameWallPlayer").is_empty());
    }

    #[test]
    fn aura_gem_injects_defensive_buff() {
        // Data-driven: an enabled Discipline (ES aura) + Purity of Fire (fire
        // resist aura) should each raise EnergyShield / FireResist respectively; a
        // build without auras (no stats) is unaffected. Never hardcoded against gem names.
        let data = repo_data();
        // Precondition check: both are actually auras (skill_types contains Aura), and their per-level stats are non-empty (data is present).
        assert!(data.is_aura("DisciplinePlayer"), "Discipline 应判定为光环");
        assert!(
            data.is_aura("PurityOfFirePlayer"),
            "Purity of Fire 应判定为光环"
        );

        let base_build = Build::new().with_character(CharacterIdentity {
            level: 90,
            class_name: "Witch".into(),
            ascendancy_name: String::new(),
        });
        let aura_build = base_build.clone().add_socket_group(
            SocketGroup::new()
                .with_gem_skill("DisciplinePlayer", 20)
                .with_gem_skill("PurityOfFirePlayer", 20),
        );

        let opts = DataOrchestratorOptions {
            inject_character_base: true,
            ..Default::default()
        };
        let base = calculate_with_data(&base_build, &data, &opts).expect("base calc");
        let aura = calculate_with_data(&aura_build, &data, &opts).expect("aura calc");

        assert!(
            aura.energy_shield > base.energy_shield,
            "Discipline 应抬升 ES: base={} aura={}",
            base.energy_shield,
            aura.energy_shield,
        );
        assert!(
            aura.fire_resistance > base.fire_resistance,
            "Purity of Fire 应抬升火抗: base={} aura={}",
            base.fire_resistance,
            aura.fire_resistance,
        );
        // A non-fire-resist aura shouldn't leak into cold/lightning resist (Purity of Fire only grants fire resist).
        assert_eq!(aura.cold_resistance, base.cold_resistance);
        assert_eq!(aura.lightning_resistance, base.lightning_resistance);
    }

    // BuffSpec extraction (aura/curse classification + double-count guard)

    /// Aura/curse skill → BuffSpec classification: an `Aura` token → Aura kind
    /// (mods = defensive buffs at the same level as aura_buff_modifiers);
    /// `Mark`/`AppliesCurse` token → Curse kind (is_mark follows the Mark token);
    /// slot/socket_index pass through as-is. Precision II support in a Persistent
    /// Buff host group → BuffSpec(kind=Buff, Accuracy INC 50,
    /// sup_dex.lua:4216-4250 constantStats); an incompatible host
    /// (require_skill_types=Persistent+Buff+AND four-way check rejects it, e.g.
    /// Fireball) → not injected.
    #[test]
    fn support_buff_specs_maps_precision_accuracy_inc() {
        let data = repo_data();
        let host = |skill: &str| {
            Build::new().add_socket_group(
                SocketGroup::new()
                    .with_gem_skill(skill, 20)
                    .with_gem_skill("SupportPrecisionPlayerTwo", 1),
            )
        };

        let specs = support_buff_specs(&host("HeraldOfAshPlayer"), &data);
        assert_eq!(
            specs.len(),
            1,
            "Persistent Buff 宿主：注入一条 support buff"
        );
        let spec = &specs[0];
        assert_eq!(spec.kind, BuffKind::Buff);
        assert_eq!(spec.skill_id, "SupportPrecisionPlayerTwo");
        assert_eq!(spec.mods.len(), 1);
        let m = &spec.mods[0];
        assert_eq!(m.name.as_str(), "Accuracy");
        assert_eq!(m.mod_type, ModType::Inc);
        assert_eq!(m.value.as_number(), Some(50.0));

        assert!(
            support_buff_specs(&host("FireballPlayer"), &data).is_empty(),
            "非 Persistent Buff 宿主：require 裁决拒收，不注入"
        );
    }

    /// Extended detection for exposure hosts outside the main group: when the
    /// host itself carries no exposure debuff payload but the exposure ability
    /// comes from a support (Fire Exposure's
    /// `inflict_exposure_for_x_ms_on_ignite` → `flag("InflictExposure",
    /// on-Ignited)`, vendor SkillStatMap.lua:1701-1703), Potent Exposure's
    /// `<El>ExposureEffect` in the same group is still injected globally (vendor
    /// CalcPerform.lua:3196-3200 gates the exposure-source config on
    /// `HasMod(FLAG, "InflictExposure")`; oracle sorceress-stormweaver-comet: skillInc=20).
    #[test]
    fn exposure_support_modifiers_detects_support_granted_inflict() {
        let data = repo_data();
        // mapped_stat_modifiers reads from a thread-local ctx catalog (the
        // production path installs it via calculate_with_data) — the test installs it too.
        let _guard =
            install_stat_map_context(StatMapMode::default(), data.stat_map_catalog.clone());
        let aux = SocketGroup::new()
            .with_gem_skill("ElementalStormPlayer", 20)
            .with_gem_skill("SupportFireExposurePlayer", 1)
            .with_gem_skill("SupportPotentExposurePlayer", 1);
        let build = Build::new().add_socket_group(aux);
        let mods = exposure_support_modifiers(&build, &data, None);
        let names: Vec<&str> = mods.iter().map(|m| m.name.as_str()).collect();
        for el in ["Fire", "Cold", "Lightning"] {
            let name = format!("{el}ExposureEffect");
            let m = mods
                .iter()
                .find(|m| m.name.as_str() == name)
                .unwrap_or_else(|| panic!("{name} 应全局注入（实得 {names:?}）"));
            assert_eq!(m.mod_type, ModType::Inc);
            assert_eq!(m.value.as_number(), Some(20.0), "Potent Exposure lv1 = 20");
        }
        // A group with no exposure ability (pure casting) gets no injection.
        let plain = Build::new().add_socket_group(
            SocketGroup::new()
                .with_gem_skill("SparkPlayer", 20)
                .with_gem_skill("SupportPotentExposurePlayer", 1),
        );
        assert!(
            exposure_support_modifiers(&plain, &data, None).is_empty(),
            "无曝光源宿主：Potent 效果词条不全局泄漏"
        );
    }

    /// The statmap buff-domain supplementary channel for Aura-kind buff skills:
    /// War Banner's `base_skill_buff_banner_accuracy_+%_to_apply` (GlobalEffect
    /// Aura + Condition BannerPlanted) → spec.mods carries Accuracy INC (the
    /// condition tag is preserved as a literal translation), value = the raw
    /// statset value at that gem level (verified independently of the mapping data).
    #[test]
    fn buff_skill_specs_maps_banner_accuracy_from_statmap() {
        let data = repo_data();
        let build =
            Build::new().add_socket_group(SocketGroup::new().with_gem_skill("WarBannerPlayer", 10));

        let specs = buff_skill_specs(&build, &data);
        let banner = specs
            .iter()
            .find(|s| s.skill_id == "WarBannerPlayer")
            .expect("War Banner spec（Aura 类）");
        assert_eq!(banner.kind, BuffKind::Aura);

        let expected: f64 = data
            .effect_stats("WarBannerPlayer", 10, 0, None)
            .all()
            .into_iter()
            .find(|ds| ds.stat == "base_skill_buff_banner_accuracy_+%_to_apply")
            .map(|ds| ds.value)
            .expect("banner accuracy stat 应在 statset 数据中");
        let acc = banner
            .mods
            .iter()
            .find(|m| m.name.as_str() == "Accuracy")
            .expect("Accuracy INC 应经 statmap buff 域入 spec.mods");
        assert_eq!(acc.mod_type, ModType::Inc);
        assert_eq!(acc.value.as_number(), Some(expected));
        assert!(
            acc.tags
                .contains(&pobr_core::ModTag::condition("BannerPlanted", false)),
            "Condition:BannerPlanted 直译保留，实得 {:?}",
            acc.tags
        );
    }

    /// Pinnacle of Power (granted by the Adonia's Ego weapon, other.lua:12503, a
    /// fromItem buff skill) → BuffSpec(kind=Buff): the statmap buff-domain flag
    /// channel produces six `<El>Can<Ailment>` FLAGs (GlobalEffect/Buff payload);
    /// the entry's leading scalar `Damage MORE` is unrelated and not swept in
    /// (each element handled independently, zero numeric injection).
    /// This is the stormweaver-comet IgniteDPS cross-type gateway (m4-skill-gaps.md §7.4).
    #[test]
    fn buff_skill_specs_emits_buff_kind_for_pinnacle_of_power_flags() {
        let data = repo_data();
        let build = Build::new()
            .add_socket_group(SocketGroup::new().with_gem_skill("PinnacleOfPowerPlayer", 20));

        let specs = buff_skill_specs(&build, &data);
        let pinnacle = specs
            .iter()
            .find(|s| s.skill_id == "PinnacleOfPowerPlayer")
            .expect("Pinnacle of Power spec（Buff 类）");
        assert_eq!(pinnacle.kind, BuffKind::Buff);

        let flags: Vec<&str> = pinnacle
            .mods
            .iter()
            .filter(|m| m.mod_type == ModType::Flag)
            .map(|m| m.name.as_str())
            .collect();
        for expected in [
            "ColdCanIgnite",
            "ColdCanShock",
            "FireCanFreeze",
            "FireCanShock",
            "LightningCanFreeze",
            "LightningCanIgnite",
        ] {
            assert!(
                flags.contains(&expected),
                "缺 {expected} flag，实得 {flags:?}"
            );
        }
    }

    /// Quiver-bonus effect (vendor `EffectOfBonusesFromQuiver`, ModParser.lua:4866;
    /// consumed per CalcSetup.lua:1366-1373's Weapon 2 quiver special case): a
    /// tree node's "N% increased bonuses gained from Equipped Quiver" → Weapon2
    /// slot scale; not collected when the off-hand isn't a quiver.
    #[test]
    fn slot_bonus_effect_scales_covers_equipped_quiver() {
        use pobr_data::passive_tree::{NodeId, PassiveTreeSpec};
        let quiver_node = pobr_data::catalog::PassiveNodeDef {
            apply_to_armour: false,
            skill: 30341,
            id: "bow_quiver_effect".into(),
            name: Some("Master Fletching".into()),
            kind: pobr_data::catalog::PassiveNodeKind::Notable,
            stats: vec!["20% increased bonuses gained from Equipped [Quiver]".into()],
            group: None,
            orbit: None,
            orbit_index: None,
            x: None,
            y: None,
            connections: vec![],
            ascendancy_id: None,
            variants: vec![],
        };
        let mut passive_nodes = HashMap::new();
        passive_nodes.insert(30341u32, quiver_node);
        let mut base_items = HashMap::new();
        base_items.insert(
            "Visceral Quiver".to_string(),
            weapon_base_item("Visceral Quiver", "Quiver"),
        );
        let data = BuildData {
            passive_nodes,
            base_items,
            ..BuildData::empty()
        };
        let quiver = Item {
            base: ItemBaseId::from("Visceral Quiver"),
            rarity: ItemRarity::Rare,
            quality: 0,
            corrupted: false,
            implicit_texts: vec![],
            modifier_texts: vec!["53% increased Damage with Bow Skills".into()],
            enchant_texts: vec![],
            rolled_defence: RolledDefence::default(),
            parsed_stats: vec![],
        };
        let tree = PassiveTreeSpec {
            allocated_nodes: vec![NodeId(30341)],
            ..Default::default()
        };
        let with_quiver = Build::new()
            .with_tree(tree.clone())
            .set_item(EquipmentSlot::Weapon2, quiver);
        let scales = slot_bonus_effect_scales(&with_quiver, &data);
        assert_eq!(
            scales,
            vec![(EquipmentSlot::Weapon2, 0.2)],
            "箭袋在副手 → Weapon2 槽 0.20 缩放"
        );

        let without_quiver = Build::new().with_tree(tree);
        assert!(
            slot_bonus_effect_scales(&without_quiver, &data).is_empty(),
            "副手非箭袋时不收集（vendor type == \"Quiver\" 门控）"
        );
    }

    /// Collecting the names of active heralds (vendor CalcPerform.lua:1792-1805
    /// heraldList + buff-branch naming `gsub(" ","")` — the connector "of" stays
    /// lowercase, matching the oracle's condVars form `AffectedByHeraldofPlague`).
    /// Deduplicated by name; supports/non-heralds don't count.
    #[test]
    fn herald_skill_names_collects_and_normalizes_of() {
        let data = repo_data();
        let build = Build::new().add_socket_group(
            SocketGroup::new()
                .with_gem_skill("HeraldOfPlaguePlayer", 10)
                .with_gem_skill("HeraldOfIcePlayer", 10)
                .with_gem_skill("FireballPlayer", 10),
        );
        let names = herald_skill_names(&build, &data);
        assert_eq!(
            names,
            vec!["Herald of Ice".to_string(), "Herald of Plague".to_string()],
            "去重 + of 小写（AffectedBy 拼接后 = AffectedByHeraldofIce/Plague）"
        );
        assert!(herald_skill_names(&Build::new(), &data).is_empty());
    }

    #[test]
    fn buff_skill_specs_classifies_aura_and_curse() {
        let data = repo_data();
        let build = Build::new()
            .with_character(CharacterIdentity {
                level: 90,
                class_name: "Witch".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(
                SocketGroup::new()
                    .with_slot("Body Armour")
                    .with_gem_skill("DisciplinePlayer", 20)
                    .with_gem_skill("TemporalChainsPlayer", 20)
                    .with_gem_skill("FreezingMarkPlayer", 20),
            );

        let specs = buff_skill_specs(&build, &data);
        assert_eq!(specs.len(), 3, "aura + hex + mark 各一条 spec");

        let aura = specs
            .iter()
            .find(|s| s.skill_id == "DisciplinePlayer")
            .expect("Discipline spec");
        assert_eq!(aura.kind, BuffKind::Aura);
        assert_eq!(aura.name, "Discipline");
        assert_eq!(aura.slot.as_deref(), Some("Body Armour"));
        assert_eq!(aura.socket_index, 1, "组内宝石序 1-based");
        assert!(!aura.is_mark);
        // mods value convention: the per-level buff stat (verified independently —
        // the raw ES apply-stat value from effect_stats, not routed back through
        // the mapping function to self-validate).
        let expected_es: f64 = data
            .effect_stats("DisciplinePlayer", 20, 0, None)
            .all()
            .filter(|ds| ds.stat == "base_skill_buff_total_maximum_energy_shield_+_to_apply")
            .map(|ds| ds.value)
            .sum();
        let spec_es: f64 = aura
            .mods
            .iter()
            .filter(|m| m.name.as_str() == "EnergyShieldTotal")
            .filter_map(|m| m.value.as_number())
            .sum();
        assert!(spec_es > 0.0, "Discipline 应携带 ES buff 词条");
        assert_eq!(
            spec_es, expected_es,
            "BuffSpec mods = 分等级 buff stat 原值"
        );

        let hex = specs
            .iter()
            .find(|s| s.skill_id == "TemporalChainsPlayer")
            .expect("Temporal Chains spec");
        assert_eq!(hex.kind, BuffKind::Curse);
        assert!(!hex.is_mark, "AppliesCurse（非 Mark）→ hex");
        assert_eq!(
            hex.name, "Temporal Chains",
            "active_skill 蛇形名派生（curse_base 查表键）"
        );
        assert_eq!(hex.socket_index, 2);

        let mark = specs
            .iter()
            .find(|s| s.skill_id == "FreezingMarkPlayer")
            .expect("Freezing Mark spec");
        assert_eq!(mark.kind, BuffKind::Curse);
        assert!(mark.is_mark, "Mark token → is_mark");
        assert_eq!(mark.socket_index, 3);

        // Active skills that aren't aura/curse, and supports, produce no spec.
        let bare = Build::new()
            .with_character(CharacterIdentity {
                level: 90,
                class_name: "Witch".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(SocketGroup::new().with_gem_skill("FireballPlayer", 20));
        assert!(buff_skill_specs(&bare, &data).is_empty());
    }

    /// Precondition for vendor curse registration: a curse skill with no
    /// GlobalEffect Curse payload in the statMap at all (Repulsion — its per-set
    /// statMap is entirely empty, so buffList is always empty,
    /// CalcActiveSkill.lua:976-1041) produces no BuffSpec — it doesn't occupy a
    /// curse slot and doesn't count toward `Multiplier:CurseOnEnemy`
    /// (CalcPerform.lua:2969 `#curseSlots`); a curse with a payload but outside
    /// the allow-list (Temporal Chains) still registers (vendor also slots it in).
    #[test]
    fn buff_skill_specs_skips_curse_without_payload() {
        let data = repo_data();
        let build = Build::new()
            .with_character(CharacterIdentity {
                level: 90,
                class_name: "Witch".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(
                SocketGroup::new()
                    .with_gem_skill("CurseOfRepulsionPlayer", 20)
                    .with_gem_skill("TemporalChainsPlayer", 20),
            );

        let specs = buff_skill_specs(&build, &data);
        assert!(
            data.granted_effects.contains_key("CurseOfRepulsionPlayer"),
            "前置：Repulsion 效果应在数据包中（否则本测试退化）"
        );
        assert!(
            !specs.iter().any(|s| s.skill_id == "CurseOfRepulsionPlayer"),
            "Repulsion 无 curse 载荷 → 不注册（vendor buffList 空）"
        );
        let hex = specs
            .iter()
            .find(|s| s.skill_id == "TemporalChainsPlayer")
            .expect("Temporal Chains 载荷存在（允收名单外亦计）→ 注册");
        assert_eq!(hex.kind, BuffKind::Curse);
    }

    /// Debuff classification: Frost Bomb (an active skill that's neither
    /// aura nor curse) has `active_skill_all_elemental_exposure_magnitude`
    /// (GlobalEffect Debuff, SkillStatMap.lua:1721-1725) → BuffSpec(kind=Debuff),
    /// mods = the three elemental `<El>Exposure BASE 20` (raw statset constants).
    /// vendor applies this to the entire activeSkillList (CalcPerform.lua:2219-2285)
    /// — a non-main skill group still produces it.
    #[test]
    fn buff_skill_specs_classifies_frost_bomb_debuff() {
        let data = repo_data();
        let build = Build::new()
            .with_character(CharacterIdentity {
                level: 90,
                class_name: "Druid".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(SocketGroup::new().with_gem_skill("FrostBombPlayer", 18));

        let specs = buff_skill_specs(&build, &data);
        let bomb = specs
            .iter()
            .find(|s| s.skill_id == "FrostBombPlayer")
            .expect("Frost Bomb debuff spec");
        assert_eq!(bomb.kind, BuffKind::Debuff);
        assert!(!bomb.is_mark);
    }

    /// Single-channel invariant (after the C5-3 legacy-code removal): the
    /// orchestrator pipeline's (BuffSpec → buff_pass multiplier zone) aura ES
    /// contribution == a manual session using only the buff_pass channel —
    /// proving the orchestrator has no leftover second aura-injection path (the
    /// old static direct-inject was removed; at mult = 1.0, ScaleAddMod returns
    /// the raw value, i.e. the value equals the raw buff stat).
    #[test]
    fn buff_spec_injection_does_not_double_count_auras() {
        let data = repo_data();
        let build = Build::new()
            .with_character(CharacterIdentity {
                level: 90,
                class_name: "Witch".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(
                SocketGroup::new()
                    .with_gem_skill("DisciplinePlayer", 20)
                    .with_gem_skill("TemporalChainsPlayer", 20),
            );
        let opts = DataOrchestratorOptions {
            inject_character_base: true,
            ..Default::default()
        };
        let through_orchestrator =
            calculate_with_data(&build, &data, &opts).expect("orchestrator calc");
        // Manual session: only the BuffSpec → buff_pass channel (same mode_buffs convention as the orchestrator).
        let mut manual = CalculationSession::new(MinimalInput::default())
            .with_config(CalcConfig::attack().with_mode_buffs(true));
        for spec in buff_skill_specs(&build, &data) {
            manual.add_buff_skill(spec);
        }
        let manual_es = {
            manual.perform_minimal();
            manual.output().energy_shield
        };
        assert!(manual_es > 0.0, "Discipline 经 buff_pass 有非零 ES 贡献");
        assert_eq!(
            through_orchestrator.energy_shield, manual_es,
            "aura 词条只经 buff_pass 单通道计入一次（无静态直注残留）"
        );
    }

    /// New-path end to end: buff_skill_specs → add_buff_skill → buff_pass aura
    /// multiplier zone (mode_buffs set — the orchestrator entry point has set it
    /// unconditionally since C5-2; here the manual session sets it explicitly).
    #[test]
    fn buff_spec_aura_path_end_to_end_with_mode_buffs() {
        let data = repo_data();
        let build = Build::new()
            .with_character(CharacterIdentity {
                level: 90,
                class_name: "Witch".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(SocketGroup::new().with_gem_skill("DisciplinePlayer", 20));

        let es_with_aura_effect = |aura_effect_inc: f64| {
            let mut session = CalculationSession::new(MinimalInput::default())
                .with_config(CalcConfig::attack().with_mode_buffs(true));
            if aura_effect_inc != 0.0 {
                session.add_modifiers([Modifier::number(
                    "AuraEffect",
                    ModType::Inc,
                    aura_effect_inc,
                )]);
            }
            for spec in buff_skill_specs(&build, &data) {
                session.add_buff_skill(spec);
            }
            session.perform_minimal();
            session.output().energy_shield
        };

        let base = es_with_aura_effect(0.0);
        assert!(base > 0.0, "新路径下 Discipline 经 buff_pass 抬升 ES");
        let boosted = es_with_aura_effect(20.0);
        assert!(
            boosted > base,
            "20% inc AuraEffect 放大 aura buff：base={base} boosted={boosted}"
        );
    }

    // Curse effect stat→mod mapping (the statmap curse domain)

    /// A curse spec's mods are filled from the statmap curse domain: Despair →
    /// enemy-side `ChaosResist` BASE (a negative resist-reducer, SkillGem
    /// attribution); Sniper's Mark → `SelfCritMultiplier` BASE; Temporal Chains
    /// (its payload name has no pobr consumer) → empty mods (falls into the
    /// Unsupported report, not silently injected).
    #[test]
    fn buff_skill_specs_fill_curse_mods_from_statmap() {
        let data = repo_data();
        let build = Build::new()
            .with_character(CharacterIdentity {
                level: 90,
                class_name: "Witch".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(
                SocketGroup::new()
                    .with_slot("Body Armour")
                    .with_gem_skill("DespairPlayer", 20)
                    .with_gem_skill("SnipersMarkPlayer", 20)
                    .with_gem_skill("TemporalChainsPlayer", 20),
            );
        let specs = buff_skill_specs(&build, &data);

        let despair = specs
            .iter()
            .find(|s| s.skill_id == "DespairPlayer")
            .expect("Despair spec");
        // Verified independently: the raw per-level buff stat (not routed back through the mapping function to self-validate).
        let expected_res: f64 = data
            .effect_stats("DespairPlayer", 20, 0, None)
            .all()
            .filter(|ds| ds.stat == "base_skill_buff_chaos_damage_resistance_%_to_apply")
            .map(|ds| ds.value)
            .sum();
        assert!(expected_res < 0.0, "Despair 减抗 stat 应为负值");
        let chaos_res: Vec<&Modifier> = despair
            .mods
            .iter()
            .filter(|m| m.name.as_str() == "ChaosResist")
            .collect();
        assert_eq!(chaos_res.len(), 1, "Despair → 敌侧 ChaosResist 单条");
        assert_eq!(chaos_res[0].mod_type, ModType::Base);
        assert_eq!(chaos_res[0].value.as_number(), Some(expected_res));
        let origin = chaos_res[0].origin.as_ref().expect("SkillGem 归因");
        assert_eq!(origin.source_id.kind, SourceKind::SkillGem);
        assert!(origin.source_id.id.starts_with("curse.DespairPlayer."));

        let mark = specs
            .iter()
            .find(|s| s.skill_id == "SnipersMarkPlayer")
            .expect("Sniper's Mark spec");
        assert!(
            mark.mods
                .iter()
                .any(|m| m.name.as_str() == "SelfCritMultiplier" && m.mod_type == ModType::Base),
            "Sniper's Mark → 敌侧 SelfCritMultiplier BASE"
        );

        let chains = specs
            .iter()
            .find(|s| s.skill_id == "TemporalChainsPlayer")
            .expect("Temporal Chains spec");
        assert!(
            !chains
                .mods
                .iter()
                .any(|m| m.name.as_str() == "TemporalChainsActionSpeed"),
            "载荷名无 pobr 消费方（TemporalChainsActionSpeed）→ 不注入（落 Compare 报表）"
        );
        // BuffExpireFaster is allow-listed (consumer = ailment::debuff_duration_mult,
        // CalcOffence.lua:1833-1835 / :5040) → a negative enemy-side MORE goes into spec.mods.
        let expire = chains
            .mods
            .iter()
            .find(|m| m.name.as_str() == "BuffExpireFaster")
            .expect("Temporal Chains → 敌侧 BuffExpireFaster MORE");
        assert_eq!(expire.mod_type, ModType::More);
        assert!(
            expire.value.as_number().is_some_and(|v| v < 0.0),
            "expire slower = MORE 负值，实得 {:?}",
            expire.value
        );
    }

    /// Visibility, not silence: in Compare mode, every mapped / unsupported stat
    /// in a curse payload lands in a [`StatMapCompareRecord`] (label = `curse.<skill_id>`).
    #[test]
    fn curse_unmapped_stats_land_in_compare_report() {
        let data = repo_data();
        let build = Build::new()
            .with_character(CharacterIdentity {
                level: 90,
                class_name: "Witch".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(
                SocketGroup::new()
                    .with_gem_skill("DespairPlayer", 20)
                    .with_gem_skill("TemporalChainsPlayer", 20),
            );
        let _ = take_stat_map_compare_records(); // Clear leftovers
        {
            let _guard =
                install_stat_map_context(StatMapMode::Compare, data.stat_map_catalog.clone());
            let _ = buff_skill_specs(&build, &data);
        }
        let records = take_stat_map_compare_records();
        assert!(
            records.iter().any(|r| r.label == "curse.DespairPlayer"
                && r.classification == "mapped"
                && r.detail.contains("ChaosResist")),
            "Despair 映射成功行入报表：{records:?}"
        );
        assert!(
            records
                .iter()
                .any(|r| r.label == "curse.TemporalChainsPlayer"
                    && r.classification == "unsupported"
                    && r.detail.contains("unknown_mod_name")),
            "Temporal Chains 未映射载荷上报 unknown_mod_name：{records:?}"
        );
    }

    /// End to end (effective mode): a build with Elemental Weakness lowers the
    /// enemy's elemental resistance → fire main-skill DPS rises; a panel-mode
    /// anchor (mode_effective=false, vendor :2289's hex gate doesn't pass)
    /// verifies every value stays unchanged.
    #[test]
    fn curse_mods_raise_effective_dps_panel_unchanged() {
        let data = repo_data();
        let base_build = Build::new()
            .with_character(CharacterIdentity {
                level: 90,
                class_name: "Witch".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(SocketGroup::new().with_gem_skill("FireballPlayer", 20));
        let cursed_build = base_build
            .clone()
            .add_socket_group(SocketGroup::new().with_gem_skill("ElementalWeaknessPlayer", 20));
        let calc = |build: &Build, effective: bool| {
            calculate_with_data(
                build,
                &data,
                &DataOrchestratorOptions {
                    inject_character_base: true,
                    mode_effective: effective,
                    enemy_tier: EnemyTier::Pinnacle,
                    ..Default::default()
                },
            )
            .expect("calc")
        };

        // Effective mode: enemy fire resist -59 (EW lv20) enters the enemy db through the CurseEffect multiplier zone → DPS rises.
        let eff_base = calc(&base_build, true);
        let eff_cursed = calc(&cursed_build, true);
        assert!(eff_base.dps > 0.0, "火系主技能基线 DPS 非零");
        assert!(
            eff_cursed.dps > eff_base.dps,
            "Elemental Weakness 减敌火抗应抬升有效 DPS：base={} cursed={}",
            eff_base.dps,
            eff_cursed.dps,
        );
        assert_eq!(
            eff_cursed.curse_slots,
            vec!["Elemental Weakness".to_string()]
        );

        // Panel-mode anchor: the hex is skipped at :2289's gate
        // (mode_effective=false) → attaching a curse gem leaves every output value unchanged.
        let panel_base = calc(&base_build, false);
        let panel_cursed = calc(&cursed_build, false);
        assert_eq!(panel_cursed.dps, panel_base.dps, "面板 DPS 逐值不变");
        assert_eq!(panel_cursed.life, panel_base.life);
        assert_eq!(panel_cursed.fire_resistance, panel_base.fire_resistance);
        assert!(panel_cursed.curse_slots.is_empty(), "面板口径 hex 不入槽");
    }

    /// End to end (effective mode): CurseEffect inc amplifies the mapped result;
    /// when limit=1 truncates, the loser (Despair, lower priority by socket order)'s mods have no DPS effect.
    #[test]
    fn curse_effect_amplifies_and_limit_truncates_end_to_end() {
        let data = repo_data();
        // Main damage: a manually injected chaos hit (affected by enemy
        // ChaosResist); the Despair spec is obtained through buff_skill_specs's real mapping.
        let despair_only = Build::new()
            .with_character(CharacterIdentity {
                level: 90,
                class_name: "Witch".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(SocketGroup::new().with_gem_skill("DespairPlayer", 20));
        // Despair(socket 1, priority 8+100) vs Enfeeble(socket 2, priority 2+200)
        // → Enfeeble takes the slot, Despair is truncated.
        let both_hexes = Build::new()
            .with_character(CharacterIdentity {
                level: 90,
                class_name: "Witch".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(
                SocketGroup::new()
                    .with_gem_skill("DespairPlayer", 20)
                    .with_gem_skill("EnfeeblePlayer", 20),
            );
        let dps = |build: Option<&Build>, curse_effect_inc: f64| {
            let mut session = CalculationSession::new(MinimalInput {
                base_accuracy: 1_000_000.0,
                base_action_rate: 1.0,
                ..Default::default()
            })
            .with_config(
                CalcConfig::attack()
                    .with_mode_buffs(true)
                    .with_mode_effective(true),
            );
            if let Some(priority) = data.curse_priority.clone() {
                session.set_curse_priority(priority);
            }
            session.add_modifiers([
                Modifier::number("ChaosDamageMin", ModType::Base, 100.0),
                Modifier::number("ChaosDamageMax", ModType::Base, 100.0),
            ]);
            if curse_effect_inc != 0.0 {
                session.add_modifiers([Modifier::number(
                    "CurseEffect",
                    ModType::Inc,
                    curse_effect_inc,
                )]);
            }
            if let Some(build) = build {
                for spec in buff_skill_specs(build, &data) {
                    session.add_buff_skill(spec);
                }
            }
            session.setup_enemy(80, EnemyTier::Pinnacle);
            session.perform_minimal();
            (session.output().dps, session.output().curse_slots.clone())
        };

        let (dps_bare, slots_bare) = dps(None, 0.0);
        let (dps_despair, slots_despair) = dps(Some(&despair_only), 0.0);
        let (dps_amplified, _) = dps(Some(&despair_only), 20.0);
        let (dps_truncated, slots_truncated) = dps(Some(&both_hexes), 0.0);

        assert!(slots_bare.is_empty());
        assert_eq!(slots_despair, vec!["Despair".to_string()]);
        assert!(
            dps_despair > dps_bare,
            "Despair 减敌混沌抗 → DPS 上升：bare={dps_bare} despair={dps_despair}"
        );
        assert!(
            dps_amplified > dps_despair,
            "20% inc CurseEffect 放大减抗：despair={dps_despair} amplified={dps_amplified}"
        );
        // limit=1 truncation: Enfeeble (higher priority) takes the slot alone,
        // Despair's mods never enter the enemy db — Enfeeble's payload (enemy
        // Damage MORE) doesn't affect player DPS → equals the bare baseline value for value.
        assert_eq!(slots_truncated, vec!["Enfeeble".to_string()]);
        assert_eq!(
            dps_truncated, dps_bare,
            "败者 Despair 词条不产生 DPS 影响（Enfeeble 载荷 DPS 中性）"
        );
    }

    // mode_combat automatic combat-condition setting

    /// combat_conditions checked branch-by-branch against vendor
    /// CalcPerform.lua:242-266: attack/spell are mutually exclusive,
    /// Movement/Minion/Channel stack, Duration suppresses minion, and exemptions clear everything.
    #[test]
    fn combat_conditions_follow_vendor_branches() {
        let types = |ts: &[&str]| ts.iter().map(|t| t.to_string()).collect::<Vec<_>>();
        // attack takes priority over spell (vendor elseif).
        assert_eq!(
            combat_conditions(&types(&["Attack"]), ModFlags::ATTACK),
            vec!["AttackedRecently"]
        );
        assert_eq!(
            combat_conditions(&types(&["Spell"]), ModFlags::SPELL),
            vec!["CastSpellRecently"]
        );
        assert_eq!(
            combat_conditions(
                &types(&["Attack", "Spell"]),
                ModFlags::ATTACK | ModFlags::SPELL
            ),
            vec!["AttackedRecently"],
            "attack elseif spell（:249-253 互斥）"
        );
        // Movement / Channel stack on top of attack/spell.
        assert_eq!(
            combat_conditions(&types(&["Attack", "Movement"]), ModFlags::ATTACK),
            vec!["AttackedRecently", "UsedMovementSkillRecently"]
        );
        assert_eq!(
            combat_conditions(&types(&["Spell", "Channel"]), ModFlags::SPELL),
            vec!["CastSpellRecently", "Channelling"]
        );
        // minion and not duration (:257-259).
        assert_eq!(
            combat_conditions(&types(&["Spell", "Minion"]), ModFlags::SPELL),
            vec!["CastSpellRecently", "UsedMinionSkillRecently"]
        );
        assert_eq!(
            combat_conditions(&types(&["Spell", "Minion", "Duration"]), ModFlags::SPELL),
            vec!["CastSpellRecently"],
            "Duration 抑制 UsedMinionSkillRecently"
        );
        // Exemptions (:248): triggered / mine / totem clear the whole set.
        for exempt in ["Triggered", "InbuiltTrigger", "RemoteMined", "SummonsTotem"] {
            assert!(
                combat_conditions(&types(&["Attack", exempt]), ModFlags::ATTACK).is_empty(),
                "{exempt} 应豁免战斗条件"
            );
        }
    }

    /// B4 end to end (existing consumer = Channelling): a Channel main skill
    /// (Bonestorm, cast 0.125s) + 5000% cast speed → the rate far exceeds the
    /// server tick cap (~30.3/s), but B4 auto-sets Channelling based on
    /// SkillType.Channel (vendor :264-266) → channelled skills are exempt from
    /// the tick cap (same convention as offence::apply_server_tick_cap / skill_use_time).
    /// Contrast against a non-Channel spell (Fireball, cast 1.2s) with the same cast speed, which does get capped.
    #[test]
    fn channel_main_skill_sets_channelling_condition() {
        let data = repo_data();
        let mk = |skill: &str| {
            Build::new()
                .with_character(CharacterIdentity {
                    level: 90,
                    class_name: "Witch".into(),
                    ascendancy_name: String::new(),
                })
                .add_socket_group(SocketGroup::new().with_gem_skill(skill, 20))
                .with_main_socket_group(1)
        };
        let opts = DataOrchestratorOptions {
            inject_character_base: true,
            extra_modifier_texts: vec!["5000% increased Cast Speed".into()],
            ..Default::default()
        };
        let server_cap = 1.0 / 0.033; // ≈ 30.3/s (game_constants server_tick_seconds)

        let channel = calculate_with_data(&mk("BonestormPlayer"), &data, &opts).expect("calc");
        let channel_sut = channel.skill_use_time.expect("skill_use_time filled");
        assert!(
            !channel_sut.capped_by_server_tick && channel.effective_action_rate > server_cap,
            "Channel 主技能应自动置 Channelling（不受帧 cap）：rate={} capped={}",
            channel.effective_action_rate,
            channel_sut.capped_by_server_tick
        );

        let spell = calculate_with_data(&mk("FireballPlayer"), &data, &opts).expect("calc");
        let spell_sut = spell.skill_use_time.expect("skill_use_time filled");
        assert!(
            spell_sut.capped_by_server_tick && spell.effective_action_rate <= server_cap + 1e-9,
            "非 Channel 法术不置 Channelling（帧 cap 生效）：rate={} capped={}",
            spell.effective_action_rate,
            spell_sut.capped_by_server_tick
        );
    }

    // Build-layer wiring for the trigger chain (findings 03-01/03-02/03-06)

    /// A built-in triggered main skill (`ElementalStormPlayer`: Spell/Damage, cd
    /// 3s, Triggered/InbuiltTrigger) → the orchestrator injects the trigger
    /// cooldown → perform's fill_trigger writes a non-placeholder
    /// trigger_rate_cap / skill_trigger_rate (cd 3s → cap ≈ 1/3.003 ≈ 0.333/s).
    #[test]
    fn inbuilt_trigger_skill_fills_trigger_rate_cap() {
        let data = repo_data();
        let build = Build::new()
            .with_character(CharacterIdentity {
                level: 80,
                class_name: "Sorceress".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(SocketGroup::new().with_gem_skill("ElementalStormPlayer", 20))
            .with_main_socket_group(1);

        let opts = DataOrchestratorOptions {
            inject_character_base: true,
            ..Default::default()
        };
        let out = calculate_with_data(&build, &data, &opts).expect("trigger calc");

        // cd 3s → cap = 1/ceil_tick(3.0) ≈ 0.333/s.
        assert!(
            out.trigger_rate_cap > 0.0,
            "内建触发应写出非零 trigger_rate_cap，实得 {}",
            out.trigger_rate_cap
        );
        assert!(
            (out.trigger_rate_cap - 0.333).abs() < 0.05,
            "cd 3s 触发上限应 ≈0.333/s，实得 {}",
            out.trigger_rate_cap
        );
        assert!(
            out.skill_trigger_rate > 0.0,
            "skill_trigger_rate 应非占位 0，实得 {}",
            out.skill_trigger_rate
        );
    }

    /// T5.6 meta/composite gem expansion: when none of the group's own gems are
    /// damage skills, the gem_effects foreign key is used to pick a damage skill
    /// from the additional granted effects as the main skill (PoB2
    /// CalcSetup.lua:1714-1718 adds additionalGrantedEffects into socketGroupSkillList too).
    #[test]
    fn meta_gem_expands_additional_granted_effect_as_main_skill() {
        // This test module has no shared effect constructor, so build one inline.
        let mk_effect = |id: &str, skill_types: &[&str]| pobr_data::catalog::GrantedEffectDef {
            id: id.into(),
            is_support: false,
            active_skill: Some(id.to_string()),
            cast_time: Some(1000),
            require_skill_types: vec![],
            add_skill_types: vec![],
            exclude_skill_types: vec![],
            cannot_be_supported: false,
            support_gems_only: false,
            stat_set: None,
            additional_stat_set_ids: vec![],
            cost_types: vec![],
            minion_list: vec![],
            add_minion_list: vec![],
            minion_uses: vec![],
            minion_has_item_set: false,
            skill_types: skill_types.iter().map(|s| s.to_string()).collect(),
        };
        let mut granted_effects = HashMap::new();
        // Host effect: a summon skill (neither attack nor spell), not a damage-skill candidate on its own.
        granted_effects.insert(
            "SummonShellPlayer".to_string(),
            mk_effect("SummonShellPlayer", &["Totem"]),
        );
        // Additional effect: the actual damage spell.
        granted_effects.insert(
            "ShellQuakePlayer".to_string(),
            mk_effect("ShellQuakePlayer", &["Spell", "Damage"]),
        );
        let mut gem_effects = HashMap::new();
        gem_effects.insert(
            "SummonShellPlayer".to_string(),
            pobr_data::catalog::GemEffectDef {
                gem_id: "Metadata/Items/Gems/SkillGemShell".into(),
                variant_id: "Shell".into(),
                granted_effect_id: "SummonShellPlayer".into(),
                additional_granted_effect_ids: vec!["ShellQuakePlayer".into()],
                additional_stat_set_ids: vec![],
            },
        );
        let data = BuildData {
            granted_effects,
            gem_effects,
            ..BuildData::empty()
        };
        let group = SocketGroup::new().with_gem_skill("SummonShellPlayer", 12);
        let picked = pick_group_main_skill(&data, &group);
        assert_eq!(
            picked,
            Some(("ShellQuakePlayer", 12, None)),
            "附加授予效果应被正向展开为主技能（等级沿用宿主宝石）"
        );

        // Missing foreign key (an old data pack without the overlay) → stays None (a pure summon group has no main skill, backward compatible).
        let data_no_link = BuildData {
            granted_effects: data.granted_effects.clone(),
            ..BuildData::empty()
        };
        assert_eq!(pick_group_main_skill(&data_no_link, &group), None);
    }

    /// A non-triggered main skill (an ordinary spell) → the orchestrator injects no
    /// trigger mods → the trigger panel stays at its placeholder 0 (backward compatible).
    #[test]
    fn non_trigger_skill_leaves_trigger_panel_zero() {
        let data = repo_data();
        // FireballPlayer: an ordinary projectile spell, not Triggered/InbuiltTrigger.
        let build = Build::new()
            .with_character(CharacterIdentity {
                level: 80,
                class_name: "Sorceress".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(SocketGroup::new().with_gem_skill("FireballPlayer", 20))
            .with_main_socket_group(1);

        let opts = DataOrchestratorOptions::default();
        let out = calculate_with_data(&build, &data, &opts).expect("non-trigger calc");

        assert_eq!(
            out.trigger_rate_cap, 0.0,
            "非触发技能 trigger_rate_cap 应保持 0"
        );
        assert_eq!(
            out.skill_trigger_rate, 0.0,
            "非触发技能 skill_trigger_rate 应保持 0"
        );
    }

    /// `trigger_modifiers` unit test: a built-in trigger + a cooldown → injects
    /// TriggeredSkillCooldown + TriggerCooldownBase; a non-triggered skill → empty (backward-compat gating).
    #[test]
    fn trigger_modifiers_gates_on_triggered_skill_type() {
        let mut granted_effects = HashMap::new();
        // A built-in triggered skill (has a cooldown).
        granted_effects.insert(
            "TrigSkill".to_string(),
            pobr_data::catalog::GrantedEffectDef {
                id: "TrigSkill".into(),
                is_support: false,
                active_skill: Some("TrigSkill".into()),
                cast_time: Some(1000),
                require_skill_types: vec![],
                add_skill_types: vec![],
                exclude_skill_types: vec![],
                cannot_be_supported: false,
                support_gems_only: false,
                stat_set: None,
                additional_stat_set_ids: vec![],
                cost_types: vec![],
                minion_list: vec![],
                add_minion_list: vec![],
                minion_uses: vec![],
                minion_has_item_set: false,
                skill_types: vec!["Spell".into(), "Triggered".into(), "InbuiltTrigger".into()],
            },
        );
        // An ordinary (non-triggered) skill.
        granted_effects.insert(
            "NormalSkill".to_string(),
            pobr_data::catalog::GrantedEffectDef {
                id: "NormalSkill".into(),
                is_support: false,
                active_skill: Some("NormalSkill".into()),
                cast_time: Some(1000),
                require_skill_types: vec![],
                add_skill_types: vec![],
                exclude_skill_types: vec![],
                cannot_be_supported: false,
                support_gems_only: false,
                stat_set: None,
                additional_stat_set_ids: vec![],
                cost_types: vec![],
                minion_list: vec![],
                add_minion_list: vec![],
                minion_uses: vec![],
                minion_has_item_set: false,
                skill_types: vec!["Spell".into()],
            },
        );
        let data = BuildData {
            granted_effects,
            ..BuildData::empty()
        };
        let build = Build::new();
        let group = SocketGroup::new();

        // A triggered skill + a cooldown → injects both cooldown BASEs.
        let triggered = ResolvedSkillLevel {
            cooldown_s: Some(0.5),
            ..ResolvedSkillLevel::default()
        };
        let opts = DataOrchestratorOptions::default();
        let mods = trigger_modifiers(&build, &data, &opts, &triggered, &group, "TrigSkill");
        let names: Vec<&str> = mods.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"TriggeredSkillCooldown"));
        assert!(names.contains(&"TriggerCooldownBase"));

        // A non-triggered skill → empty (no trigger mods injected).
        let normal = ResolvedSkillLevel {
            cooldown_s: Some(0.5),
            ..ResolvedSkillLevel::default()
        };
        let mods_none = trigger_modifiers(&build, &data, &opts, &normal, &group, "NormalSkill");
        assert!(mods_none.is_empty(), "非触发技能不应注入触发词条");
    }

    // trigger_configs recognition + source-rate sub-calc

    /// CoC fixture (a named gate item): group = [attack, MetaCastOnCritPlayer,
    /// spell], main skill = spell. `trigger_configs`'s `match_effect_ids`
    /// recognizes the CoC trigger relationship (the trigger panel no longer
    /// degrades to self-cast 0), folding in the source hit/crit.
    #[test]
    fn coc_group_recognized_and_trigger_rate_filled() {
        let data = repo_data();
        assert!(
            data.trigger_configs.contains_key("MetaCastOnCritPlayer"),
            "trigger_configs overlay 应含 CoC join 键"
        );
        let build = Build::new()
            .with_character(CharacterIdentity {
                level: 80,
                class_name: "Sorceress".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(
                SocketGroup::new()
                    .with_gem_skill("ArmourBreakerPlayer", 10)
                    .with_gem_skill("MetaCastOnCritPlayer", 10)
                    .with_gem_skill("FireballPlayer", 10)
                    .with_main_active_skill(3),
            )
            .with_main_socket_group(1);
        let out = calculate_with_data(&build, &data, &DataOrchestratorOptions::default())
            .expect("coc calc");

        assert!(
            out.skill_trigger_rate > 0.0,
            "CoC 识别后触发速率应非占位 0，实得 {}",
            out.skill_trigger_rate
        );
        // Crit folded in (trigger_on_crit): the trigger rate should be noticeably lower than the source's attack rate (source crit chance ≪ 100%).
        let source_stats = trigger_source_stats(
            &build,
            &data,
            &DataOrchestratorOptions::default(),
            &build.socket_groups[0],
            &build.socket_groups[0].gem_skills[0],
            "FireballPlayer",
        )
        .expect("source sub-calc");
        assert!(
            out.skill_trigger_rate < source_stats.action_rate,
            "CoC 触发速率 {} 应被源暴击率折减到低于源速率 {}",
            out.skill_trigger_rate,
            source_stats.action_rate
        );
    }

    /// CoC directional assertion: source skill +100% attack speed → trigger rate
    /// (the rate factor feeding DPS) rises in step — a regression guard for
    /// 14-G2's "source rate didn't scale with attack speed" bug.
    #[test]
    fn coc_directional_attack_speed_raises_trigger_rate() {
        let data = repo_data();
        let mk_build = || {
            Build::new()
                .with_character(CharacterIdentity {
                    level: 80,
                    class_name: "Sorceress".into(),
                    ascendancy_name: String::new(),
                })
                .add_socket_group(
                    SocketGroup::new()
                        .with_gem_skill("ArmourBreakerPlayer", 10)
                        .with_gem_skill("MetaCastOnCritPlayer", 10)
                        .with_gem_skill("FireballPlayer", 10)
                        .with_main_active_skill(3),
                )
                .with_main_socket_group(1)
        };
        let base_out = calculate_with_data(&mk_build(), &data, &DataOrchestratorOptions::default())
            .expect("coc base");
        let fast_opts = DataOrchestratorOptions {
            extra_modifier_texts: vec!["100% increased Attack Speed".to_string()],
            ..Default::default()
        };
        let fast_out = calculate_with_data(&mk_build(), &data, &fast_opts).expect("coc fast");
        assert!(
            fast_out.skill_trigger_rate > base_out.skill_trigger_rate * 1.5,
            "+100% 攻速应近乎翻倍触发速率（14-G2 修复）：{} → {}",
            base_out.skill_trigger_rate,
            fast_out.skill_trigger_rate
        );
    }

    /// Recursion guards: ① a cycle (source == the triggered skill itself) → None
    /// (falls back to the base convention); ② inside the depth guard (a sub-calc
    /// already in progress) → None; ③ inside the guard, trigger_modifiers strips the whole relationship.
    #[test]
    fn trigger_subcalc_recursion_guards() {
        let data = repo_data();
        let build = Build::new()
            .with_character(CharacterIdentity {
                level: 80,
                class_name: "Sorceress".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(
                SocketGroup::new()
                    .with_gem_skill("ArmourBreakerPlayer", 10)
                    .with_gem_skill("FireballPlayer", 10),
            )
            .with_main_socket_group(1);
        let opts = DataOrchestratorOptions::default();
        let group = &build.socket_groups[0];

        // ① Cycle detection: source gem id == the triggered main skill's id.
        assert!(
            trigger_source_stats(
                &build,
                &data,
                &opts,
                group,
                &group.gem_skills[0],
                "ArmourBreakerPlayer"
            )
            .is_none(),
            "源 = 被触发自身应退回 None（基础 use_time 口径）"
        );

        // ② Depth guard: a sub-calc already in progress won't expand another one.
        {
            let _guard = TriggerDepthGuard::enter();
            assert!(
                trigger_source_stats(
                    &build,
                    &data,
                    &opts,
                    group,
                    &group.gem_skills[0],
                    "FireballPlayer"
                )
                .is_none(),
                "深度 ≥1 应拒绝再展开子计算"
            );
            // ③ Inside the guard, the trigger relationship is stripped entirely.
            let resolved = ResolvedSkillLevel {
                cooldown_s: Some(0.5),
                ..ResolvedSkillLevel::default()
            };
            assert!(
                trigger_modifiers(
                    &build,
                    &data,
                    &opts,
                    &resolved,
                    group,
                    "ElementalStormPlayer"
                )
                .is_empty(),
                "子计算 env 中 trigger 关系应被剥离"
            );
        }
        // After the guard exits, normal expansion resumes.
        assert!(
            trigger_source_stats(
                &build,
                &data,
                &opts,
                group,
                &group.gem_skills[0],
                "FireballPlayer"
            )
            .is_some(),
            "护栏退出后子计算应恢复可用"
        );
    }

    /// requires_condition gating: a Hidden Blade-style entry requires Phasing —
    /// recognition hits but injection is skipped when the condition isn't met
    /// (vendor's disable semantics, panel stays at 0), and it doesn't fall into the built-in path.
    #[test]
    fn trigger_config_requires_condition_gates_injection() {
        let mut data = BuildData::empty();
        data.granted_effects.insert(
            "UnseenStrikePlayer".to_string(),
            pobr_data::catalog::GrantedEffectDef {
                id: "UnseenStrikePlayer".into(),
                is_support: false,
                active_skill: Some("UnseenStrikePlayer".into()),
                cast_time: Some(1000),
                require_skill_types: vec![],
                add_skill_types: vec![],
                exclude_skill_types: vec![],
                cannot_be_supported: false,
                support_gems_only: false,
                stat_set: None,
                additional_stat_set_ids: vec![],
                cost_types: vec![],
                minion_list: vec![],
                add_minion_list: vec![],
                minion_uses: vec![],
                minion_has_item_set: false,
                skill_types: vec!["Attack".into()],
            },
        );
        data.trigger_configs.insert(
            "UnseenStrikePlayer".to_string(),
            pobr_data::catalog::TriggerConfigDef {
                key: pobr_data::catalog::TriggerKeyDef {
                    kind: "unique_item".into(),
                    name: "the hidden blade".into(),
                },
                trigger_name: None,
                trigger_on_use: false,
                use_cast_rate: false,
                source_skill_cond: None,
                triggered_skill_cond: None,
                source_skill_name: None,
                requires_main_skill_name: None,
                trigger_chance_stat: None,
                source_rate_stat: None,
                cooldown_override_s: None,
                trigger_rate_cap_override: Some(2.0),
                global_trigger: true,
                source_is_self: true,
                source_rate_is_final: false,
                ignores_tick_rate: false,
                assuming_every_hit_kills: false,
                ignore_source_rate: false,
                trigger_on_crit: false,
                requires_condition: Some("Phasing".into()),
                match_effect_ids: vec!["UnseenStrikePlayer".into()],
                handler_id: None,
                note: None,
                vendor_ref: "Modules/CalcTriggers.lua:907-921".into(),
                verified: false,
            },
        );
        let group = SocketGroup::new().with_gem_skill("UnseenStrikePlayer", 10);
        let resolved = ResolvedSkillLevel::default();
        let opts = DataOrchestratorOptions::default();

        // Condition unmet (build config has no Phasing) → recognition hits but injection is empty.
        let build = Build::new();
        let mods = trigger_modifiers(
            &build,
            &data,
            &opts,
            &resolved,
            &group,
            "UnseenStrikePlayer",
        );
        assert!(
            mods.is_empty(),
            "Phasing 未置真时应不注入（vendor disable）"
        );

        // Condition met → injects the cap override + the global marker.
        let mut build_phasing = Build::new();
        build_phasing
            .config
            .conditions
            .insert("Phasing".to_string(), true);
        let mods = trigger_modifiers(
            &build_phasing,
            &data,
            &opts,
            &resolved,
            &group,
            "UnseenStrikePlayer",
        );
        let names: Vec<&str> = mods.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"TriggerRateCapOverride"));
        assert!(names.contains(&"TriggerSourceGlobal"));
    }

    // Unarmed base / weapon-type table-lookup switchover (migration-invariant regression)

    /// After switching the unarmed base to the injected table, values still
    /// match the old hardcoded match arm-for-arm (`BuildData::empty()` takes the
    /// Default fallback, = the JSON values value-for-value; covers all 9 classes + the unknown-class fallback).
    #[test]
    fn unarmed_contribution_matches_legacy_hardcoded_values() {
        let data = BuildData::empty();
        let legacy: &[(&str, f64)] = &[
            ("Warrior", 8.0),
            ("Scion", 6.0),
            ("Mercenary", 6.0),
            ("Druid", 6.0),
            ("Witch", 5.0),
            ("Ranger", 5.0),
            ("Sorceress", 5.0),
            ("Huntress", 5.0),
            ("Monk", 5.0),
            // Unknown class: the old match's else branch (generic fallback).
            ("NoSuchClass", 5.0),
        ];
        for &(class, phys_max) in legacy {
            let build = Build::new().with_character(CharacterIdentity {
                level: 1,
                class_name: class.into(),
                ascendancy_name: String::new(),
            });
            let c = unarmed_contribution(&build, &data);
            assert_eq!(c.phys_min, 2.0, "{class} phys_min");
            assert_eq!(c.phys_max, phys_max, "{class} phys_max");
            assert_eq!(c.attack_rate, 1.65, "{class} attack_rate");
            // Old hardcoded value 0.05 (unit-convention TODO(parity), see the unarmed_contribution doc).
            assert_eq!(c.crit_chance, 0.05, "{class} crit_chance");
        }
    }

    /// A weapon base item for tests (only item_class matters for hold/melee classification).
    fn weapon_base_item(name: &str, item_class: &str) -> pobr_data::catalog::BaseItemDef {
        pobr_data::catalog::BaseItemDef {
            req_str: 0,
            req_dex: 0,
            req_int: 0,
            id: format!("Test/{name}"),
            name: name.to_string(),
            item_class: item_class.to_string(),
            drop_level: 1,
            width: 1,
            height: 1,
            tags: vec![],
            implicits: vec![],
            mod_domain: 1,
            weapon: None,
            armour: None,
            spirit: None,
            charm_buff: Vec::new(),
        }
    }

    /// After switching weapon-type conditions to the injected table, they're
    /// equivalent class-by-class to the old scattered predicates (including a
    /// parity guard: Talisman / FishingRod aren't melee, and GGG's `Staff`
    /// (quarterstaff) gets no conditions — vendor discrepancies are pinned to the old behavior).
    #[test]
    fn weapon_type_conditions_match_legacy_predicates() {
        let mut data = BuildData::empty();
        let cases: &[(&str, &[&str])] = &[
            // GGG `Warstaff` (quarterstaff) → table key `Staff` (label=Quarterstaff).
            ("Warstaff", &["UsingQuarterstaff", "UsingTwoHandedMelee"]),
            ("One Hand Mace", &["UsingMace", "UsingOneHandedMelee"]),
            ("Two Hand Mace", &["UsingMace", "UsingTwoHandedMelee"]),
            ("Bow", &["UsingBow"]),
            ("Crossbow", &["UsingCrossbow"]),
            ("Spear", &["UsingSpear", "UsingOneHandedMelee"]),
            ("Dagger", &["UsingDagger", "UsingOneHandedMelee"]),
            ("Claw", &["UsingOneHandedMelee"]),
            ("Flail", &["UsingOneHandedMelee"]),
            ("One Hand Sword", &["UsingOneHandedMelee"]),
            ("Two Hand Sword", &["UsingTwoHandedMelee"]),
            ("Two Hand Axe", &["UsingTwoHandedMelee"]),
            // parity guard: the old predicates didn't treat Talisman / FishingRod
            // as melee (vendor has melee=true; the discrepancy is recorded as a
            // schema TODO(parity), behavior alignment left for a separate commit).
            ("Talisman", &[]),
            ("FishingRod", &[]),
            // GGG `Staff` (quarterstaff class): the vendor table has no matching entry, so no weapon-type condition at all.
            ("Staff", &[]),
            ("Wand", &[]),
            ("Sceptre", &[]),
        ];
        for &(cls, expected) in cases {
            let base_name = format!("Test {cls}");
            data.base_items
                .insert(base_name.clone(), weapon_base_item(&base_name, cls));
            let build = Build::new().set_item(
                EquipmentSlot::Weapon1,
                Item {
                    base: ItemBaseId::from(base_name.as_str()),
                    rarity: ItemRarity::Normal,
                    quality: 0,
                    corrupted: false,
                    implicit_texts: vec![],
                    modifier_texts: vec![],
                    enchant_texts: vec![],
                    rolled_defence: RolledDefence::default(),
                    parsed_stats: vec![],
                },
            );
            let vars = weapon_type_conditions(&build, &data);
            assert_eq!(&vars[..], expected, "item_class = {cls}");
        }
    }

    /// cfg weapon-slot flags: derived per vendor getWeaponFlags (same source and
    /// gating as the Using* conditions).
    #[test]
    fn weapon_cfg_flags_dual_write_channel() {
        let mut data = BuildData::empty();
        let base_name = "Test One Hand Mace".to_string();
        data.base_items.insert(
            base_name.clone(),
            weapon_base_item(&base_name, "One Hand Mace"),
        );
        let build = Build::new().set_item(
            EquipmentSlot::Weapon1,
            Item {
                base: ItemBaseId::from(base_name.as_str()),
                rarity: ItemRarity::Normal,
                quality: 0,
                corrupted: false,
                implicit_texts: vec![],
                modifier_texts: vec![],
                enchant_texts: vec![],
                rolled_defence: RolledDefence::default(),
                parsed_stats: vec![],
            },
        );
        let bits = weapon_cfg_flags(&build, &data);
        let unarmed = weapon_cfg_flags(&Build::new(), &data);
        assert_eq!(
            bits,
            ModFlags::MACE | ModFlags::WEAPON | ModFlags::WEAPON_1H | ModFlags::WEAPON_MELEE,
            "单手锤 → vendor getWeaponFlags 位集"
        );
        assert_eq!(unarmed, ModFlags::UNARMED, "空主手 → 仅 Unarmed 位");
    }
}
