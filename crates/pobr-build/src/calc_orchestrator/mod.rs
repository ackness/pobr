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

use pobr_core::calc::{CalculationSession, MinimalInput, OutputTable};
use pobr_core::rules::stat_map_engine::StatMapCatalog;
use pobr_core::{CalcConfig, CharacterBase, Modifier};
use pobr_data::item::{EquipmentSlot, Item};
use pobr_data::modifier::{ModFlags, ModType};
use pobr_data::monster::EnemyTier;
use pobr_data::source::{ModifierSource, SourceId, SourceKind};

use crate::build::Build;
use crate::build_data::BuildData;
use crate::error::BuildError;

mod granted_skills;
pub(crate) mod item;
mod skill;
use granted_skills::*;
pub use skill::resolve::resolve_main_skill_selection;
use skill::{
    minions::spawn_minions,
    resolve::{additional_ring_slot_allocated, apply_gem_quality_bonuses, pick_group_main_skill},
};
mod collect;
mod conditions;
use collect::*;
mod stat_map;
pub use stat_map::StatMapCompareRecord;
mod prepare;
use prepare::*;
mod context;
use context::CalculationContext;
mod sources;
use sources::SourceWriter;
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
    /// via [`calculate_with_data_report`]).
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
    /// **Default** (the switch commit, gated on a four-precondition checklist).
    #[default]
    Data,
    /// Observation comparison: the Data computation + recording a mapping outcome per
    /// stat (**output identical to Data**; pure observation that changes no computed
    /// result; records are retrieved via [`calculate_with_data_report`]). Kept as a
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
                .map_err(|e| format!("failed to load mod_parser_rules.json: {e}"))?
                .ok_or_else(|| {
                    "data directory is missing overlay/mod_parser_rules.json".to_string()
                })?;
            let special = data
                .load_ruleset()
                .map_err(|e| format!("failed to load ruleset: {e}"))?
                .special_mods
                .unwrap_or_default();
            pobr_core::mod_parser::CompiledParserRules::compile_with_special(&doc, &special)
                .map(Arc::new)
                .map_err(|e| format!("parser rule compilation failed: {e:?}"))
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

    let minimal = session
        .perform_minimal()
        .map_err(|e| BuildError::Calc(e.to_string()))?;
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
    let primary_group = resolve_main_skill_selection(build, data).map(|(index, _)| index);

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
        // Reuse the primary result when this group is the one actually selected.
        // This also covers an absent or out-of-range main socket group.
        let out = if primary_group == Some(i) {
            primary.clone()
        } else {
            calculate_with_data(&scoped, data, options)?
        };
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
    calculate_with_data_report(build, data, options).map(|report| report.session)
}

/// A completed calculation and its mapping diagnostics, including trigger sources.
/// Each report owns its records; subsequent calculations cannot overwrite them.
pub struct CalculationReport {
    pub session: CalculationSession,
    pub stat_map_records: Vec<StatMapCompareRecord>,
}

/// Calculates a build and returns the observations requested by `stat_map_mode`.
pub fn calculate_with_data_report(
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
) -> Result<CalculationReport, BuildError> {
    let mut context = CalculationContext::new(data, options);
    let session = calculate_with_context(build, data, options, &mut context)?;
    Ok(CalculationReport {
        session,
        stat_map_records: context.compare_records,
    })
}

fn calculate_with_context(
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
    context: &mut CalculationContext,
) -> Result<CalculationSession, BuildError> {
    // Stage 0: build view transformation (Ring3 gate → item-granted skill synthesis → quality conversion)
    let build = stage_build_view(build, data);
    let build: &Build = &build;

    let main = stage_resolve_main_skill(build, data);
    let (resolved_config, base_cfg) = stage_resolve_config(build, data, options);
    let (cfg, enemy_tier) =
        stage_build_cfg(build, data, options, &main, &resolved_config, base_cfg);
    let weapons = stage_weapon_bases(build, data, &main, options.base_input);
    let ctx = StageCtx {
        build,
        data,
        options,
        main,
        weapons,
        resolved_config,
        enemy_tier,
    };

    let mut session = stage_create_session(&ctx, cfg);
    stage_hand_sources(&mut session, &ctx);
    stage_cooldown_bypass(&mut session, &ctx);

    // 1. Character base (level + class-derived attributes) + elemental resistance penalty (campaign progress tier).
    inject_character_base(&mut session, build, data, options, &ctx.resolved_config);

    // 1b/1b-ii/1c. Main skill base/quality/unselected-set/DoT/corpse-explosion/crossbow/support/trigger + damage multiplier + weapon crit.
    inject_main_skill_mods(context, &mut session, &ctx);

    // 1d. Item base defence / shield base block / per-item Spirit / Ward → BASE mods.
    inject_defence_base(&mut session, build, data);

    // 2. Equipment: attribution-path injection (per-item filter / Kalandra mirroring / local mod stripping / slot bonus numeric copies).
    let main_weapon_active = ctx
        .main
        .main_effect
        .is_some_and(|e| e.is_attack() && !e.is_non_weapon_attack());
    inject_items(
        &mut session,
        build,
        data,
        ctx.weapons.off_hand_weapon.is_some(),
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
    inject_buffs_and_heralds(context, &mut session, build, data);

    // Mark's self offensive buff + non-main-group exposure supports.
    inject_self_buff_exposure(
        context,
        &mut session,
        build,
        data,
        ctx.main.main_skill.as_ref().map(|(_, g, _)| *g),
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

    // All source writes are complete. Consuming the writer enables ModDb reads;
    // reservation must see equipment, passives, config, and extra modifier texts.
    let mut session = session.finish_sources();
    inject_spirit_reservation(&mut session, build, data);

    // Core owns actor-derived values; build supplies starting attributes and equipment facts.
    let class = options.inject_character_base.then(|| {
        character_base(build, data).unwrap_or(CharacterBase {
            level: build.character.level,
            strength: 0.0,
            dexterity: 0.0,
            intelligence: 0.0,
        })
    });
    session.prepare_player_stats(build.character.level, class);
    inject_build_multipliers(&mut session, build, data);

    // 6c2. Equipped support gems counted by color (matching PoB2
    //      CalcSetup.lua:2015-2044) → Red/Green/BlueSupportGems multipliers (the
    //      denominator for pinned MultiplierThreshold entries like "if you have at
    //      least 10 <color> Support Gems Socketed").
    inject_support_gem_counts(&mut session, build, data);

    session.bridge_player_conditions();

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
    session
        .perform_minimal()
        .map_err(|e| BuildError::Calc(e.to_string()))?;
    Ok(session)
}

/// Fully resolved inputs used by source injection. No field is filled by a later stage.
struct StageCtx<'a> {
    build: &'a Build,
    data: &'a BuildData,
    options: &'a DataOrchestratorOptions,
    main: ResolvedMainSkill<'a>,
    weapons: ResolvedWeapons,
    resolved_config: crate::config_resolve::ResolvedConfig,
    enemy_tier: EnemyTier,
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

    if let Some(resolved) = collect::resolve_granted_socket_jewels(&build, data) {
        build = Cow::Owned(resolved);
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

/// Resolve the same active item/socket view used by the calculation pipeline.
pub fn passive_jewel_state(
    build: &Build,
    data: &BuildData,
) -> crate::jewel_tree::PassiveJewelState {
    crate::jewel_tree::passive_jewel_state(&stage_build_view(build, data), data)
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

/// Stage 5: session creation + runtime rule-pack injection (constants / special /
/// parser rules, buff definitions / handlers, curse priority, rounding precision).
/// Rule injection must precede any subsequent `add_item` / `add_passive_nodes` /
/// `add_gem` (each injection point's comment notes the basis).
fn stage_create_session(ctx: &StageCtx<'_>, cfg: CalcConfig) -> SourceWriter {
    let data = ctx.data;
    let mut session = CalculationSession::new(ctx.weapons.base_input).with_config(cfg);
    // The injection pipeline: injects the runtime constants bundle loaded by GameData
    // into calc (must come after with_config — with_config replaces cfg wholesale). The
    // data is value-for-value equal to the Default fallback, zero behavior change.
    session.set_constants(data.constants.clone());
    // CalcSetup supplies one base projectile; SkillStatMap count overrides
    // subtract that one. Only projectile skills expose this output.
    if ctx.main.skill_flags.intersects(ModFlags::PROJECTILE) {
        session.add_modifiers(vec![
            Modifier::number("ProjectileCount", ModType::Base, 1.0)
                .with_source("Base projectile count"),
        ]);
    }
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
    SourceWriter::new(session)
}

/// Stage 6: weapon base injected via HandSource — depends on stage 4's converted WeaponBase.
///
/// Weapon base is injected via HandSource (a single-pass direct pass-through — OR mode
/// is value-for-value equal to the old base_input conversion). Dual wielding (Weapon2
/// is a weapon base) assembles a second off-hand HandSource, with per-hand weapon bits
/// following WeaponBase::flags into the hand pass; data channels like
/// doubleHitsWhenDualWielding are always false. A non-weapon attack's (Shield Wall type)
/// source is off-hand (matching PoB2 CalcOffence L2418-2431).
fn stage_hand_sources(session: &mut SourceWriter, ctx: &StageCtx<'_>) {
    if let Some(wb) = ctx.weapons.hand_weapon {
        let is_off_hand_source = ctx
            .main
            .main_effect
            .map(|e| e.is_attack() && e.is_non_weapon_attack())
            .unwrap_or(false);
        let sources = if is_off_hand_source {
            vec![pobr_core::calc::HandSource::off_hand(wb)]
        } else if let Some(ohb) = ctx.weapons.off_hand_weapon {
            vec![
                pobr_core::calc::HandSource::main_hand(wb),
                pobr_core::calc::HandSource::off_hand(ohb),
            ]
        } else {
            vec![pobr_core::calc::HandSource::main_hand(wb)]
        };
        // A skill with its own base critical chance keeps that source. Otherwise
        // each hand uses its own weapon's locally modified base, before global
        // CriticalStrikeChance increases. The main-hand value also serves the
        // preliminary unscoped offence pass; the off-hand pass replaces it.
        let skill_has_own_crit = ctx
            .main
            .main_skill
            .as_ref()
            .is_some_and(|(skill, _, _)| skill.crit_chance.is_some_and(|c| c > 0.0));
        if !skill_has_own_crit {
            for source in &sources {
                let off_hand = source.label == pobr_core::HandTag::OffHand;
                let slot = if off_hand { "weapon2" } else { "weapon1" };
                let origin =
                    ModifierSource::new(SourceId::new(SourceKind::Item, format!("{slot}.base")))
                        .with_slot(slot)
                        .with_raw_text(format!("weapon base crit {}%", source.weapon.crit_chance));
                session.add_modifiers(vec![
                    Modifier::number(
                        "SkillBaseCritChance",
                        ModType::Base,
                        source.weapon.crit_chance,
                    )
                    .with_tag(pobr_core::ModTag::condition("OffHandAttack", !off_hand))
                    .with_origin(origin),
                ]);
            }
        }
        session.set_hand_sources(sources, false);
    }
}

/// Stage 7: cooldown-bypass flag injection (determined in stage 4; `CooldownBypass`'s single source).
fn stage_cooldown_bypass(session: &mut SourceWriter, ctx: &StageCtx<'_>) {
    if ctx.weapons.bypasses_cooldown {
        let origin =
            ModifierSource::new(SourceId::new(SourceKind::SkillGem, "skill.cooldownBypass"))
                .with_raw_text("skill bypasses cooldown (consumes charges on use)");
        session.add_modifiers(vec![Modifier::flag("CooldownBypass").with_origin(origin)]);
    }
}

/// 2b. Jewels (passive tree/abyss sockets): mods injected **globally** (most jewels
///     are global mods; radius jewels are currently approximated as global too). Follows
///     add_item's skip-and-collect error tolerance.
fn stage_inject_jewels(session: &mut SourceWriter, ctx: &StageCtx<'_>) -> Result<(), BuildError> {
    let adorned_inc = adorned_corrupted_magic_jewel_inc(&ctx.build.jewels);
    // Radius directives are consumed by geometry, not the global mod parser.
    // Exempt only validated directives with available socket geometry; unknown
    // nested grants must remain visible to the trade optimizer's safety gate.
    let nodes = ctx
        .data
        .passive_nodes_for(ctx.build.tree_version.as_deref());
    let mut radius_texts = Vec::new();
    let tree_state = crate::jewel_tree::passive_jewel_state(ctx.build, ctx.data);
    radius_texts.extend(tree_state.handled_modifiers);
    for node in &ctx.build.tree.allocated_nodes {
        if tree_state.unresolved.contains(&node.0) {
            session.record_unsupported_modifier_text(format!(
                "Tree:{}: missing timeless jewel seed data",
                node.0
            ));
        }
        if let Some(change) = tree_state.nodes.get(&node.0) {
            for text in &change.stats {
                if !gate_parses(engine_ctx(ctx.data), text) {
                    session.record_unsupported_modifier_text(format!("Tree:{}: {}", node.0, text));
                }
            }
        }
    }
    for radius in &ctx.build.radius_jewels {
        if !nodes
            .get(&radius.socket_node)
            .is_some_and(|n| n.x.is_some() && n.y.is_some())
        {
            continue;
        }
        radius_texts.extend(
            radius
                .grant_lines
                .iter()
                .filter(|line| {
                    parse_grant_line(line)
                        .is_some_and(|(_, grant)| gate_parses(engine_ctx(ctx.data), &grant))
                })
                .cloned(),
        );
        for (kind, inc) in [
            ("Small", radius.small_effect_inc),
            ("Notable", radius.notable_effect_inc),
        ] {
            if inc > 0 {
                radius_texts.push(format!(
                    "{inc}% increased Effect of {kind} Passive Skills in Radius"
                ));
            }
        }
        if let Some(label) = &radius.radius_label {
            radius_texts.push(format!("Upgrades Radius to {label}"));
        }
    }
    for jewel in &ctx.build.jewels {
        let filtered = filter_item_parseable(jewel, engine_ctx(ctx.data), session, &radius_texts);
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
    pobr_core::skill_env::adorned_corrupted_magic_jewel_inc(jewels)
}

/// Vendor's `ModStore:ScaleAddMod` numeric scaling semantics (ModStore.lua:70-79):
/// `m_modf(round(v × scale, 2))` — rounds to 2 decimal places first, then truncates.
fn scale_trunc_2dp(value: f64, scale: f64) -> f64 {
    pobr_core::skill_env::scale_trunc_2dp(value, scale)
}

/// Radius grants are parsed and scaled per affected allocated node. Invalid
/// grant text remains diagnosed by the dedicated item gate above.
fn stage_inject_radius_jewels(
    session: &mut SourceWriter,
    ctx: &StageCtx<'_>,
) -> Result<(), BuildError> {
    session.add_modifiers(radius_jewel_grant_modifiers(ctx.build, ctx.data));
    Ok(())
}

/// 2c/2d/2e. config-derived mod injection: quest reward global text, the config
/// interpreter's player mods, the customMods line channel — all three share the
/// [`ResolvedConfig`](crate::config_resolve::ResolvedConfig) output, injected adjacent
/// to each other per the existing assembly order.
fn stage_inject_config_mods(
    session: &mut SourceWriter,
    ctx: &StageCtx<'_>,
) -> Result<(), BuildError> {
    let (data, resolved_config) = (ctx.data, &ctx.resolved_config);
    // 2c. Quest rewards / global config mods (PoB2's `questRewards`): injected as
    //     **global** modifier text (permanent global boosts to attributes / resistances
    //     / defence inc etc.). Follows add_modifier_texts's error tolerance. Quest
    //     still goes through the legacy text channel (not switched to declarative
    //     mods until vendor/parser naming is unified; `config_resolve` already
    //     excludes quest-attributed entries from the injection list to avoid
    //     double-counting).
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
fn stage_inject_passives(session: &mut SourceWriter, ctx: &StageCtx<'_>) -> Result<(), BuildError> {
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

        session.add_modifiers(passive_effect_copies(build, data, &passive_nodes)?);
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
    session: &mut SourceWriter,
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
fn test_context(data: &BuildData) -> CalculationContext {
    CalculationContext::new(data, &DataOrchestratorOptions::default())
}

#[cfg(test)]
mod tests;
