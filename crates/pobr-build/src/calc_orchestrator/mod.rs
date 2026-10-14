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
use pobr_data::item::EquipmentSlot;
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
    resolve::{additional_ring_slot_allocated, apply_gem_quality_bonuses, group_active_effects},
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
mod diagnosis;
pub use diagnosis::{Diagnosis, MainSkillSummary, ModifierRef, UnmappedStat, diagnose};
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
    /// Whether a spell in a meta gem that fills with Energy (Cast on Critical)
    /// is used as often as the gem triggers it. Defaults to `true`, which is
    /// what happens in the game. `false` gives the spell its own rate of use:
    /// the basis of a build code's reference values, whose exporter has no
    /// Energy model, so a comparison with them asks the same question.
    pub energy_triggers: bool,
    /// Whether a rain (Rain of Arrows) hits an enemy with the first arrow to
    /// land once the enemy can be hit again, so that the hits come in steps
    /// of the landing interval. Defaults to `true`, which is what the game's
    /// numbers give. `false` counts a hit whenever the enemy can be hit
    /// again, as if an arrow landed then: the basis of a build code's
    /// reference values.
    pub rain_landings: bool,
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
            energy_triggers: true,
            rain_landings: true,
        }
    }
}

impl DataOrchestratorOptions {
    /// The enemy and mode a build code's reference values (its `<PlayerStat>`
    /// rows) were exported under: the exporter's main panel is its EFFECTIVE
    /// mode against its default enemy, a Pinnacle boss. Everything the engine
    /// models stays on; this is the build as the game plays it against that
    /// enemy.
    pub fn against_reference_enemy() -> Self {
        Self {
            enemy_tier: EnemyTier::Pinnacle,
            mode_effective: true,
            ..Self::default()
        }
    }

    /// These options with the models the exporter of a build code's reference
    /// values does not have turned off, so that a comparison with those values
    /// asks the question they answer. The exporter has no Energy model: a
    /// spell in a meta gem is counted at the spell's own rate of use. It
    /// counts a rain's hits as often as an enemy can be hit, whenever the
    /// arrows land.
    ///
    /// A model added here is verified by its own tests, not by the reference.
    pub fn on_reference_basis(self) -> Self {
        Self {
            energy_triggers: false,
            rain_landings: false,
            ..self
        }
    }

    /// What a build code's reference values are compared under: their enemy
    /// and mode, on their basis.
    pub fn reference_conditions() -> Self {
        Self::against_reference_enemy().on_reference_basis()
    }
}

/// The statmap mapping channel selection (a dual-run framework, contract C3; a
/// deliberate decision: Compare is kept as a long-term comparison tool — config /
/// parser dual-runs reuse the same pattern). Re-exported from `pobr-core::skill_env`.
pub use pobr_core::skill_env::StatMapMode;

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
/// version dataset).
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

/// Reconciles allocated nodes against the build's selected tree, using the same
/// fallback as calculation when the requested historical tree is unavailable.
pub fn diagnose_tree_version(build: &Build, data: &BuildData) -> TreeVersionReport {
    let nodes = data.passive_nodes_for(build.tree_version.as_deref());
    let unknown_nodes = build
        .tree
        .allocated_nodes
        .iter()
        .map(|n| n.0)
        .filter(|id| !nodes.contains_key(id))
        .collect();
    TreeVersionReport {
        build_tree_version: build.tree_version.clone(),
        unknown_nodes,
    }
}

/// An independently calculated active effect, identified by its original group
/// and complete PoB active-effect ordinal. Unavailable is distinct from zero DPS.
#[derive(Debug, Clone)]
pub struct SkillMetrics {
    pub group_index: usize,
    pub active_skill_index: usize,
    pub skill_id: String,
    pub output: Option<OutputTable>,
    pub unsupported_modifiers: Vec<String>,
    pub error: Option<String>,
}

/// Independent skill metrics, never a sum of mutually exclusive skills.
#[derive(Debug, Clone)]
pub struct SkillMetricsReport {
    pub skills: Vec<SkillMetrics>,
}

/// Calculates every active effect of every enabled group without requiring a
/// primary skill. Each calculation keeps the whole build's aura/support context.
/// Missing effect data and individual calculation failures remain visible rows.
pub fn calculate_skill_metrics(
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
) -> Result<SkillMetricsReport, BuildError> {
    // Share exactly the panel's equipment gates, granted skills, name resolution
    // and quality preparation. Reusing this view avoids applying quality twice.
    let prepared = prepared_build_view(build, data, options);
    let mut scoped = prepared.as_ref().clone();
    let mut skills = Vec::new();
    for (group_index, group) in prepared.socket_groups.iter().enumerate() {
        if !group.enabled {
            continue;
        }
        for effect in group_active_effects(data, group) {
            scoped.main_socket_group = Some(group_index + 1);
            scoped.socket_groups[group_index].main_active_skill = Some(effect.active_skill_index);
            let mut row = SkillMetrics {
                group_index,
                active_skill_index: effect.active_skill_index,
                skill_id: effect.skill_id.to_string(),
                output: None,
                unsupported_modifiers: Vec::new(),
                error: None,
            };
            if !data.granted_effects.contains_key(effect.skill_id)
                || data
                    .granted_effect_levels
                    .get(effect.skill_id)
                    .is_none_or(Vec::is_empty)
            {
                row.error = Some(format!(
                    "Cannot resolve effect {} at group {} active skill {} (gem level {}, stat set {:?})",
                    effect.skill_id,
                    group_index + 1,
                    effect.active_skill_index,
                    effect.gem_level,
                    effect.stat_set_index,
                ));
            } else {
                let mut context = CalculationContext::new(data, options);
                match calculate_prepared_with_context(&scoped, data, options, &mut context) {
                    Ok(session) => {
                        row.unsupported_modifiers = session.unsupported_modifier_texts().to_vec();
                        row.output = Some(session.output().clone());
                    }
                    Err(error) => row.error = Some(error.to_string()),
                }
            }
            skills.push(row);
        }
        scoped.socket_groups[group_index].main_active_skill = group.main_active_skill;
    }
    Ok(SkillMetricsReport { skills })
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
/// let data = GameData::new("data/<version>");
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

pub(crate) fn calculate_with_data_memo(
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
    memo: std::rc::Rc<std::cell::RefCell<crate::calc_cache::TriggerMemo>>,
) -> Result<OutputTable, BuildError> {
    let mut context = CalculationContext::new(data, options);
    context.trigger_memo = Some(memo);
    calculate_with_context(build, data, options, &mut context)
        .map(|session| session.output().clone())
}

fn calculate_with_context(
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
    context: &mut CalculationContext,
) -> Result<CalculationSession, BuildError> {
    let build = prepared_build_view(build, data, options);
    calculate_prepared_with_context(&build, data, options, context)
}

/// Stage 0: the build as a calculation under `options` reads it. Its lines
/// first ([`with_extra_gem_bonuses`]), then the view of them
/// ([`stage_build_view`]: Ring 3 gate → item-granted skills → quality).
fn prepared_build_view<'a>(
    build: &'a Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
) -> Cow<'a, Build> {
    match with_extra_gem_bonuses(build, options) {
        Cow::Borrowed(build) => stage_build_view(build, data),
        Cow::Owned(build) => Cow::Owned(stage_build_view(&build, data).into_owned()),
    }
}

/// `build` with the gem level and quality bonuses among the extra modifier
/// texts added to its own lines. A gem's level is settled from the build's
/// lines before any modifier is stored, so a bonus given only as an extra text
/// would arrive too late; as a line of the build it counts like the same line
/// on an item. The stages that store lines leave gem bonuses out, so each is
/// read once, where gem levels are settled.
pub(crate) fn with_extra_gem_bonuses<'a>(
    build: &'a Build,
    options: &DataOrchestratorOptions,
) -> Cow<'a, Build> {
    let bonuses: Vec<&String> = options
        .extra_modifier_texts
        .iter()
        .filter(|line| skill::resolve::parse_gem_property_bonus(line).is_some())
        .collect();
    if bonuses.is_empty() {
        return Cow::Borrowed(build);
    }
    let mut build = build.clone();
    build
        .config
        .global_modifier_texts
        .extend(bonuses.into_iter().cloned());
    Cow::Owned(build)
}

/// Calculates a prepared build view. Trigger sources change only skill selection,
/// so they reuse this entry point without applying additive gem quality again.
fn calculate_prepared_with_context(
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
    context: &mut CalculationContext,
) -> Result<CalculationSession, BuildError> {
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

    // 2a. Transcendent limbs: their lines are global modifiers.
    inject_limbs(&mut session, build, data)?;

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

    // Trigger children retain limits in their context for the parent to consume;
    // do not clone strings into an internal session that only supplies source rates.
    if !context.is_trigger_source {
        for limit in std::mem::take(&mut context.support_limits) {
            session.record_unsupported_modifier_text(limit);
        }
    }

    // All source writes are complete. Consuming the writer enables ModDb reads;
    // reservation must see equipment, passives, config, and extra modifier texts.
    let mut session = session.finish_sources();
    if context.track_reads {
        session.track_modifier_reads();
    }
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
    inject_build_multipliers(&mut session, build, data);
    session.prepare_player_stats(build.character.level, class);

    // 6c2. Equipped support gems counted by color (matching PoB2
    //      CalcSetup.lua:2015-2044) → Red/Green/BlueSupportGems multipliers (the
    //      denominator for pinned MultiplierThreshold entries like "if you have at
    //      least 10 <color> Support Gems Socketed").
    inject_support_gem_counts(&mut session, build, data);
    inject_connected_support_counts(&mut session, ctx.main.connected_supports);

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
    // "for each Minion in your Presence" counts the minions just created, or
    // the configured number.
    session.bridge_minion_presence();

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
/// (Cow), value-equal to mutating the build in place. Following vendor CalcSetup's
/// item pre-processing, strips inactive items and synthesizes granted skill groups.
/// Gem identities are resolved before applying type-dependent quality bonuses.
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

    // Bonded modifiers of socketed Runes and Idols: the ones in effect become
    // ordinary lines of their item and the rest are removed, before anything
    // reads an item's lines.
    if let Some(resolved) = item::bonded::resolve_bonded_modifiers(&build, data) {
        build = Cow::Owned(resolved);
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

    // The skills an item grants without reservation, while gem qualities are
    // still the build's own: the next steps add quality to every gem.
    if let Some(marked) = mark_reservation_free_skills(&build, data) {
        build = Cow::Owned(marked);
    }

    // nameSpec-only gem references → skill_id backfilled (matching PoB2 SkillsTab
    // looking up a gem's equivalent by nameSpec): a lineage support (e.g. Atziri's
    // Communion) lacks skillId/gemId in the XML, only a display name. Matched against
    // granted_effects ids by normalized name; a miss keeps an empty id (every consumer
    // silently skips it).
    if let Some(resolved) = resolve_name_spec_gems(&build, data) {
        build = Cow::Owned(resolved);
    }

    // Gem quality bonuses: "+N% to Quality of all <X> Skills" (tree small passives/items)
    // is pre-folded into each gem's quality (matching vendor's applyGemMods, which
    // stacks effect.quality onto every gem effect, CalcSetup.lua:410-435), so it applies
    // consistently at every downstream quality consumption point. Resolved skill ids
    // are required to match skill types and gem attribute requirements.
    if let Some(adjusted) = apply_gem_quality_bonuses(&build, data) {
        build = Cow::Owned(adjusted);
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

/// Vendor's `ModStore:ScaleAddMod` numeric scaling semantics (ModStore.lua:70-79):
/// `m_modf(round(v × scale, 2))` — rounds to 2 decimal places first, then truncates.
fn scale_trunc_2dp(value: f64, scale: f64) -> f64 {
    pobr_core::skill_env::scale_trunc_2dp(value, scale)
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
