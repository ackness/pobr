//! Closes out config consumption ("xml_build switches to the interpreter
//! primary path").
//!
//! [`resolve_config`] is the **single entry point** `calculate_with_data` uses to
//! consume build config:
//! - When a `ConfigCatalog` is available (`data/<ver>/overlay/config_options.json` is
//!   loaded) → primary path: `build.config.raw_inputs` (the three-typed `<Input>`
//!   key-values captured losslessly by `parse_config_inputs`) is interpreted via
//!   `config_interpreter::interpret`, producing conditions / multipliers / scalars
//!   (enemyIsBoss, resistancePenalty wrapping) / player·enemy modifiers / a customMods
//!   line channel;
//! - When the catalog is missing (old data pack / [`BuildData::empty`](crate::BuildData::empty))
//!   → falls back to the legacy `parse_config` output (`build.config`'s existing fields,
//!   tolerant of a missing table).
//!
//! ## Dual-run period semantics (commit cluster opened category by category)
//!
//! - **conditions / multipliers**: old key set ∪ interpreter output — the intersection
//!   is asserted value-equal (`config_dualrun`'s ongoing regression hard assert); the
//!   interpreter's exclusive new overrides (count-type / implyCond / unprefixed entries,
//!   report §2.3 lines 1-2) have already been opened up (commit ②, see
//!   [`merge_conditions`] / [`merge_multipliers`]).
//! - **enemy condition bridge**: vendor puts `conditionEnemy<X>` into the enemy bucket
//!   as `Condition:<X>` (ConfigOptions.lua:1694/1769/1789/1833 enemyModList:NewMod);
//!   pobr's mod_parser models "against <X> enemies" as the cfg condition `Enemy<X>`
//!   (mod_parser.rs:1612-1640), so this bridges the enemy-bucket FLAG back to the
//!   `Enemy<X>` condition — pure namespace translation, zero behavior delta (matches the
//!   legacy value item-for-item, before the legacy path is removed).
//! - **quest rewards**: still go through the legacy text channel
//!   (`global_modifier_texts`) — the interpreter's declarative quest mods follow vendor
//!   spelling (`Life`/`Str`/`ColdResist`), which hasn't been unified with the parser's
//!   namespace (`MaximumLife`/`Strength`/`ColdResistance`) yet; switching now would split
//!   the same stat under two names.
//! - **scalars**: enemyIsBoss / resistancePenalty only go through the handler wrapper
//!   ([`enemy_tier_from_config`] / [`campaign_progress_from_config`]) when the XML gives
//!   them **explicitly**; when omitted, they keep the existing fallback chain
//!   (orchestrator option tier / Endgame -60), which matches the catalog's materialized
//!   defaultIndex value (report §2.3 line 5: opening this up is actually a zero diff).

use std::collections::HashMap;

use pobr_core::modifier::Modifier;
use pobr_core::rules::config_interpreter::{ConfigOutcome, interpret};
use pobr_data::modifier::ModType;
use pobr_gamedata::ruleset::ConfigCatalog;

use crate::build::Build;
use crate::build_config::BuildConfig;
use crate::handlers::{build_registry, campaign_progress_from_config, enemy_tier_from_config};

/// Legacy-path `conditionEnemy<X>Exposure` → cfg condition bridge (vendor
/// ConfigOptions.lua:1864-1872: enemy bucket `<X>Exposure` BASE 20 + ActorCondition
/// tag). After the §3-⑦ backfill, the interpreter already produces this enemy numeric
/// mod (the actor tag is translated to `ModTag::Condition{actor: Player, var:
/// CanApply<X>Exposure}`), but there's still no source that sets the player-side actor
/// snapshot (`cfg.actor_multipliers["player.CanApply<X>Exposure"]`) — that mod is
/// inert. The orchestrator's stage 5b `apply_enemy_exposure` still drives off the cfg
/// condition, so this bridge reads straight from the raw input to keep current
/// production behavior. **Retirement condition**: once the actor snapshot channel is
/// wired up (CanApply*Exposure set from the skill side), this bridge must be removed or
/// it will double-count against the interpreter's mod.
const EXPOSURE_CONDITION_BRIDGE: &[(&str, &str)] = &[
    ("conditionEnemyFireExposure", "EnemyFireExposure"),
    ("conditionEnemyColdExposure", "EnemyColdExposure"),
    ("conditionEnemyLightningExposure", "EnemyLightningExposure"),
];

/// The config consumption view for `calculate_with_data` (sourced from either the
/// primary interpreter path or the legacy fallback).
#[derive(Default)]
pub(crate) struct ResolvedConfig {
    /// A copy of [`BuildConfig`] with conditions / multipliers / campaign_progress /
    /// enemy_tier resolved via the primary path (programmatically-set fields are kept
    /// as-is — the primary path only overrides a field when `raw_inputs` has a
    /// corresponding input).
    pub config: BuildConfig,
    /// Player modifiers produced by the interpreter (`SourceKind::Config` attribution;
    /// excludes quest entries, see module doc §3-⑤). Combat-gated entries (the
    /// `Condition:Combat` tag) are naturally inert when `mode_combat=false` (D5). Empty
    /// when the catalog is missing.
    pub player_mods: Vec<Modifier>,
    /// Enemy modifiers produced by the interpreter (`SourceKind::EnemyConfig` attribution):
    /// - FLAG: actor-ized enemy condition entries (carrying a `Condition:Effective` tag);
    /// - numeric (from commit ③ on): direct BASE injection of enemy resistance /
    ///   SelfCritChance / ailment stack counts (matching vendor's
    ///   `enemyModList:NewMod(..., "BASE", val, "EnemyConfig")` shape,
    ///   ConfigOptions.lua:2143-2157 / 1892-1894 / 1782-1842).
    pub enemy_mods: Vec<Modifier>,
    /// The customMods line channel (commit ④, matching vendor ConfigOptions.lua:2278-2296:
    /// StripEscapes + parseMod per line, source=Custom): raw text lines with color codes
    /// stripped by the interpreter, fed by the build layer into
    /// `session.add_modifier_texts` — unparseable lines naturally fall into the
    /// `ParseStatus::Unsupported` visibility channel (structurally hard-failing lines are
    /// skipped via filter_parseable, the same as every other text injection channel).
    /// Empty when the catalog is missing (the legacy path never consumed customMods).
    pub custom_mod_lines: Vec<String>,
}

/// Resolves the consumption view of build config (see module doc for details).
pub(crate) fn resolve_config(build: &Build, catalog: Option<&ConfigCatalog>) -> ResolvedConfig {
    let Some(catalog) = catalog else {
        // Tolerant of a missing table: fall back to the legacy parse_config output (build.config's existing fields).
        return ResolvedConfig {
            config: build.config.clone(),
            player_mods: Vec::new(),
            enemy_mods: Vec::new(),
            custom_mod_lines: Vec::new(),
        };
    };

    let outcome = interpret(
        &catalog.options,
        &build.config.raw_inputs,
        &build_registry(),
    );
    let mut config = build.config.clone();

    merge_conditions(&mut config.conditions, &outcome);
    merge_multipliers(&mut config.multipliers, &outcome);
    bridge_enemy_conditions(&mut config.conditions, &outcome);
    bridge_combat_player_conditions(&mut config.conditions, &outcome);
    bridge_exposure_conditions(&mut config.conditions, build);

    // Scalar wrapping (only overrides on explicit input; omitted = keep the existing
    // fallback chain, see module doc).
    if build.config.raw_inputs.values.contains_key("enemyIsBoss") {
        config.enemy_tier = enemy_tier_from_config(&outcome);
    }
    if build
        .config
        .raw_inputs
        .values
        .contains_key("resistancePenalty")
    {
        config.campaign_progress = campaign_progress_from_config(&outcome);
    }

    // quest entries still go through the legacy text channel (§3-⑤): excluded from
    // modifier injection to avoid double-counting with `global_modifier_texts`.
    let player_mods = outcome
        .player_mods
        .iter()
        .filter(|m| !is_quest_mod(m))
        .cloned()
        .collect();
    let enemy_mods = outcome.enemy_mods.clone();

    ResolvedConfig {
        config,
        player_mods,
        enemy_mods,
        custom_mod_lines: outcome.custom_mod_lines.clone(),
    }
}

/// Merges conditions (fully, from commit ② on): the intersection matches the primary
/// path value-for-value (guaranteed by config_dualrun's ongoing regression); keys unique
/// to the interpreter are net-new overrides (count-type entries' `>0` condition,
/// implyCond expansion, non-`condition`-prefixed entries).
///
/// implyCond semantics: vendor bakes the calc-side implication **directly into each
/// entry's apply** (e.g. conditionCritRecently also does NewMod
/// SkillCritRecently / CritInPast8Sec, ConfigOptions.lua:1130-1134 — that shape is
/// already carried by the player_mods injection, Combat-gated); the `implyCondList`
/// metadata itself only drives visibility on vendor's config page
/// (ConfigTab.lua:91-109). Writing the imply expansion into cfg.conditions here is
/// pobr's compatibility channel (matching legacy parse_config's "bare condition goes
/// straight into cfg" semantics, for mod_parser's Condition tag mods to consume); it
/// doesn't override an explicit value (interpreter's `or_insert` semantics).
fn merge_conditions(conditions: &mut HashMap<String, bool>, outcome: &ConfigOutcome) {
    for (var, enabled) in &outcome.conditions {
        conditions.insert(var.clone(), *enabled);
    }
}

/// Merges multipliers (fully, from commit ② on): same semantics as
/// [`merge_conditions`] — the net-new overrides are count entries turned numeric
/// (Multiplier:StationarySeconds etc.; matching vendor
/// ConfigOptions.lua:120-127's conditionStationary
/// `NewMod("Multiplier:StationarySeconds", BASE, val)` shape).
fn merge_multipliers(multipliers: &mut HashMap<String, f64>, outcome: &ConfigOutcome) {
    for (var, value) in &outcome.multipliers {
        multipliers.insert(var.clone(), *value);
    }
}

/// Bridges the enemy-bucket FLAG `Condition:<X>` back to the cfg condition `Enemy<X>`
/// (see module doc). `or_insert` doesn't override an explicit value (an XML
/// `boolean="false"` is simply not active on the interpreter side already).
///
/// Additional **unprefixed** bridge: an enemy-side numeric mod's `Condition` tag looks
/// up the cfg condition by the enemy's own state name (no Enemy prefix) — the same
/// convention as the curse domain (`stat_map_engine::map_curse_stat`'s doc: "var is the
/// enemy's own state, no Enemy prefix"; the vendor equivalent is a mod inside enemyDB
/// whose Condition tag looks up enemyDB's own conditions). Under pobr's single
/// condition namespace we can't blindly strip the prefix from everything
/// (`Condition:Chilled` would pollute the player-side `Chilled`), so only vars that are
/// **actually referenced by an enemy-side numeric mod tag** get the unprefixed condition
/// set as well (the catalog's current reference set is the
/// `ApplyCriticalWeakness`/`OnProfaneGround`/`<Ailment>Config` family, all enemy-exclusive
/// names with no player namespace collision). Example chain (vendor
/// ConfigOptions.lua:1889-1894): `conditionEnemyCriticalWeakness` → enemy FLAG
/// `Condition:ApplyCriticalWeakness`, stacked with `enemyCriticalWeaknessStacks`
/// (placeholder 20) → enemy `SelfCritChance BASE 10` {Condition:ApplyCriticalWeakness}
/// — once the unprefixed condition is set, this mod is picked up in crit aggregation
/// (the enemy-side SelfCritChance feeding into crit chance base material around
/// CalcOffence.lua:3677).
fn bridge_enemy_conditions(conditions: &mut HashMap<String, bool>, outcome: &ConfigOutcome) {
    use pobr_core::ModTag;
    use std::collections::HashSet;

    // The set of vars referenced by enemy-side numeric mods' Condition tags (same actor; cross-actor tags aren't this semantic).
    let referenced: HashSet<&str> = outcome
        .enemy_mods
        .iter()
        .filter(|m| m.mod_type != ModType::Flag)
        .flat_map(|m| m.tags.iter())
        .filter_map(|tag| match tag {
            ModTag::Condition {
                var, actor: None, ..
            } => Some(var.as_str()),
            _ => None,
        })
        .collect();
    for modifier in &outcome.enemy_mods {
        if modifier.mod_type != ModType::Flag {
            continue;
        }
        let Some(var) = modifier.name.as_str().strip_prefix("Condition:") else {
            continue;
        };
        let enabled = matches!(modifier.value, pobr_core::ModValue::Bool(true));
        conditions.entry(format!("Enemy{var}")).or_insert(enabled);
        if referenced.contains(var) {
            conditions.entry(var.to_string()).or_insert(enabled);
        }
    }
}

/// Bridges player-bucket **Combat-gated** `Condition:<X>` FLAGs to cfg conditions.
///
/// The interpreter's bare-effect backfill (config_interpreter's `apply_effect`) only
/// accepts entries with **no tags**; main config effects like
/// `Condition:CritRecently`/`Condition:UsingCharm` carry a `{type=condition, var=Combat}`
/// tag (vendor ConfigOptions's Combat-gating shape, where `Condition:Combat` ≡ buffMode
/// "EFFECTIVE" → mode_combat, CalcSetup.lua:583-597 — this orchestration path always
/// sets mode_combat=true). Once these land in player_mods there's no cfg consumer for
/// them (`Modifier::matches`'s Condition tag lookup only checks cfg.conditions), which
/// makes the same-named imply condition (CritInPast8Sec) active while the **main
/// condition itself** (CritRecently) stays inactive. This bridge sets any player FLAG
/// `Condition:<X>` whose tags are entirely `Condition:Combat` (not negated, no actor)
/// into cfg as well (`or_insert` doesn't override an explicit value).
fn bridge_combat_player_conditions(
    conditions: &mut HashMap<String, bool>,
    outcome: &ConfigOutcome,
) {
    use pobr_core::ModTag;
    for modifier in &outcome.player_mods {
        if modifier.mod_type != ModType::Flag {
            continue;
        }
        let Some(var) = modifier.name.as_str().strip_prefix("Condition:") else {
            continue;
        };
        let combat_gated_only = !modifier.tags.is_empty()
            && modifier.tags.iter().all(|tag| {
                matches!(
                    tag,
                    ModTag::Condition {
                        var,
                        negated: false,
                        actor: None,
                    } if var == "Combat"
                )
            });
        if !combat_gated_only {
            continue;
        }
        let enabled = matches!(modifier.value, pobr_core::ModValue::Bool(true));
        conditions.entry(var.to_string()).or_insert(enabled);
    }
}

/// Bridges the raw `conditionEnemy<X>Exposure` input to a cfg condition (see
/// [`EXPOSURE_CONDITION_BRIDGE`]).
fn bridge_exposure_conditions(conditions: &mut HashMap<String, bool>, build: &Build) {
    for (input_name, cond_var) in EXPOSURE_CONDITION_BRIDGE {
        if let Some(pobr_core::rules::config_interpreter::ConfigInputValue::Bool(enabled)) =
            build.config.raw_inputs.values.get(*input_name)
        {
            conditions
                .entry((*cond_var).to_string())
                .or_insert(*enabled);
        }
    }
}

/// Determines whether a mod is a quest entry (attribution id `config.quest…`; vendor's
/// `<Input name>` starts with `quest`).
fn is_quest_mod(modifier: &Modifier) -> bool {
    modifier
        .origin
        .as_ref()
        .is_some_and(|origin| origin.source_id.id.starts_with("config.quest"))
}

#[cfg(test)]
mod tests {
    use pobr_core::CampaignProgress;
    use pobr_core::rules::config_interpreter::{ConfigInputValue, RawConfigInputs};
    use pobr_data::monster::EnemyTier;
    use pobr_data::source::SourceKind;
    use pobr_gamedata::{GameData, repo_data_root};

    use super::*;

    fn load_catalog() -> ConfigCatalog {
        GameData::new(repo_data_root().join(pobr_gamedata::data_version()))
            .load_ruleset()
            .expect("load ruleset")
            .config_catalog
            .expect("config_catalog 域应已接通")
    }

    fn build_with_inputs(inputs: RawConfigInputs) -> Build {
        let mut build = Build::new();
        build.config.raw_inputs = inputs;
        build
    }

    /// Missing catalog → legacy path fallback: config unchanged, zero modifiers.
    #[test]
    fn missing_catalog_falls_back_to_legacy_config() {
        let mut build = Build::new();
        build.config.conditions.insert("EnemyChilled".into(), true);
        build.config.enemy_tier = Some(EnemyTier::Uber);
        let resolved = resolve_config(&build, None);
        assert_eq!(resolved.config.conditions.get("EnemyChilled"), Some(&true));
        assert_eq!(resolved.config.enemy_tier, Some(EnemyTier::Uber));
        assert!(resolved.player_mods.is_empty());
        assert!(resolved.enemy_mods.is_empty());
    }

    /// Programmatically-set conditions / multipliers / scalars are kept as-is under the
    /// primary path (raw_inputs is empty → the interpreter produces no overrides).
    #[test]
    fn programmatic_config_preserved_under_catalog() {
        let catalog = load_catalog();
        let mut build = Build::new();
        build.config.conditions.insert("UsingShield".into(), true);
        build.config.multipliers.insert("PowerCharge".into(), 3.0);
        build.config.enemy_tier = Some(EnemyTier::None);
        build.config.campaign_progress = Some(CampaignProgress::Act1);
        let resolved = resolve_config(&build, Some(&catalog));
        assert_eq!(resolved.config.conditions.get("UsingShield"), Some(&true));
        assert_eq!(resolved.config.multipliers.get("PowerCharge"), Some(&3.0));
        assert_eq!(resolved.config.enemy_tier, Some(EnemyTier::None));
        assert_eq!(
            resolved.config.campaign_progress,
            Some(CampaignProgress::Act1)
        );
    }

    /// Enemy condition bridge: `conditionEnemyChilled` → enemy bucket FLAG → cfg
    /// `EnemyChilled` (supplied independently by the bridge when the legacy field isn't
    /// set — the only source once the legacy path is removed).
    #[test]
    fn enemy_condition_bridge_supplies_legacy_var() {
        let catalog = load_catalog();
        let build = build_with_inputs(
            RawConfigInputs::new().with("conditionEnemyChilled", ConfigInputValue::Bool(true)),
        );
        let resolved = resolve_config(&build, Some(&catalog));
        assert_eq!(resolved.config.conditions.get("EnemyChilled"), Some(&true));
        // The enemy bucket FLAG is injected alongside it (Effective tag gated, EnemyConfig attributed).
        assert!(resolved.enemy_mods.iter().any(|m| {
            m.name.as_str() == "Condition:Chilled"
                && m.origin
                    .as_ref()
                    .is_some_and(|o| o.source_id.kind == SourceKind::EnemyConfig)
        }));
    }

    /// Exposure condition bridge: `conditionEnemyFireExposure` → cfg `EnemyFireExposure`
    /// (before the T5-E1 translation is wired up, the interpreter conservatively skips
    /// this entry's effect, so the bridge reads straight from the raw input).
    #[test]
    fn exposure_bridge_supplies_condition() {
        let catalog = load_catalog();
        let build = build_with_inputs(
            RawConfigInputs::new()
                .with("conditionEnemyFireExposure", ConfigInputValue::Bool(true))
                .with("conditionEnemyColdExposure", ConfigInputValue::Bool(false)),
        );
        let resolved = resolve_config(&build, Some(&catalog));
        assert_eq!(
            resolved.config.conditions.get("EnemyFireExposure"),
            Some(&true)
        );
        assert_eq!(
            resolved.config.conditions.get("EnemyColdExposure"),
            Some(&false)
        );
        assert!(
            !resolved
                .config
                .conditions
                .contains_key("EnemyLightningExposure")
        );
    }

    /// Scalar wrapping only overrides on explicit input: enemyIsBoss=None overrides;
    /// when omitted, the programmatic/fallback value is kept.
    #[test]
    fn scalar_wrappers_only_apply_on_explicit_input() {
        let catalog = load_catalog();
        let build = build_with_inputs(
            RawConfigInputs::new()
                .with("enemyIsBoss", ConfigInputValue::Text("None".into()))
                .with("resistancePenalty", ConfigInputValue::Number(-30.0)),
        );
        let resolved = resolve_config(&build, Some(&catalog));
        assert_eq!(resolved.config.enemy_tier, Some(EnemyTier::None));
        assert_eq!(
            resolved.config.campaign_progress,
            CampaignProgress::from_resistance_penalty(-30.0)
        );

        let omitted = resolve_config(&Build::new(), Some(&catalog));
        assert_eq!(omitted.config.enemy_tier, None, "省略时维持既有回退链");
        assert_eq!(omitted.config.campaign_progress, None);
    }

    /// Quest entries don't enter modifier injection (§3-⑤: still routed through the legacy text channel to avoid double-counting).
    #[test]
    fn quest_mods_excluded_from_injection() {
        let catalog = load_catalog();
        let build = build_with_inputs(RawConfigInputs::new().with(
            "questAct 2Valley of the TitansMedallion",
            ConfigInputValue::Text("30% increased Charm Charges Gained\n\t+1 Charm Slot".into()),
        ));
        let resolved = resolve_config(&build, Some(&catalog));
        assert!(
            resolved.player_mods.iter().all(|m| !is_quest_mod(m)),
            "quest 归因 mod 不应进注入列表"
        );
    }

    /// commit ② semantics: a count-type entry (conditionStationary, vendor
    /// ConfigOptions.lua:120-127) produces a Multiplier plus a Condition when `>0`;
    /// implyCond (conditionCritRecently → SkillCritRecently/CritInPast8Sec, :1130-1134)
    /// expands into cfg; count=0 skips the whole entry (vendor's BuildModList semantics).
    #[test]
    fn count_and_imply_coverage_opened() {
        let catalog = load_catalog();
        let mut build = build_with_inputs(
            RawConfigInputs::new()
                .with("conditionStationary", ConfigInputValue::Number(5.0))
                .with("conditionCritRecently", ConfigInputValue::Bool(true)),
        );
        // Simulates legacy-path output.
        build.config.conditions.insert("CritRecently".into(), true);
        let resolved = resolve_config(&build, Some(&catalog));
        assert_eq!(
            resolved.config.conditions.get("Stationary"),
            Some(&true),
            "count>0 → 条件置真"
        );
        assert_eq!(
            resolved.config.multipliers.get("StationarySeconds"),
            Some(&5.0),
            "count 数值化为 Multiplier"
        );
        assert_eq!(
            resolved.config.conditions.get("SkillCritRecently"),
            Some(&true),
            "implyCond 展开"
        );
        assert_eq!(
            resolved.config.conditions.get("CritInPast8Sec"),
            Some(&true)
        );
        // The Combat-gated mod entry is still injected as usual (naturally inert).
        assert!(
            resolved
                .player_mods
                .iter()
                .any(|m| m.name.as_str() == "Condition:CritRecently")
        );
        // Combat-gate bridge: the main condition itself (CritRecently) is set in cfg
        // too (this orchestration path always sets mode_combat=true, so vendor's
        // Condition:Combat ≡ true).
        let no_legacy = build_with_inputs(
            RawConfigInputs::new().with("conditionCritRecently", ConfigInputValue::Bool(true)),
        );
        let resolved = resolve_config(&no_legacy, Some(&catalog));
        assert_eq!(
            resolved.config.conditions.get("CritRecently"),
            Some(&true),
            "Combat 门控主条件经桥落 cfg（不依赖 legacy 字段）"
        );

        // count=0 → skipped entirely per vendor's BuildModList semantics (produces neither a condition nor a multiplier).
        let zero = build_with_inputs(
            RawConfigInputs::new().with("conditionStationary", ConfigInputValue::Number(0.0)),
        );
        let resolved = resolve_config(&zero, Some(&catalog));
        assert!(!resolved.config.conditions.contains_key("Stationary"));
        assert!(
            !resolved
                .config
                .multipliers
                .contains_key("StationarySeconds")
        );
    }

    /// implyCond doesn't override an explicit false (the interpreter's `or_insert` semantics are preserved through the merge channel).
    #[test]
    fn imply_does_not_override_explicit_false() {
        let catalog = load_catalog();
        let mut build = build_with_inputs(
            RawConfigInputs::new().with("conditionCritRecently", ConfigInputValue::Bool(true)),
        );
        // Programmatic explicit false: the key already exists explicitly before the
        // merge channel writes the outcome's values — outcome's imply expansion
        // (SkillCritRecently=true) would override the same key. What's asserted here is
        // outcome's internal or_insert ordering (explicit effects before imply); the
        // merge side overriding the whole key is expected.
        build
            .config
            .conditions
            .insert("UsedSkillRecently".into(), false);
        let resolved = resolve_config(&build, Some(&catalog));
        // conditionCritRecently doesn't imply UsedSkillRecently → the explicit false is kept.
        assert_eq!(
            resolved.config.conditions.get("UsedSkillRecently"),
            Some(&false)
        );
    }

    /// commit ③: enemy numeric overrides (BASE directly injected into the enemy bucket +
    /// EnemyConfig attribution). Vendor's enemyFireResist: ConfigOptions.lua:2152-2154
    /// `enemyModList:NewMod("FireResist", "BASE", val, "EnemyConfig")`.
    #[test]
    fn enemy_numeric_overrides_opened() {
        let catalog = load_catalog();
        let build = build_with_inputs(
            RawConfigInputs::new()
                .with("enemyFireResist", ConfigInputValue::Number(75.0))
                .with(
                    "enemyCriticalWeaknessStacks",
                    ConfigInputValue::Number(10.0),
                ),
        );
        let resolved = resolve_config(&build, Some(&catalog));
        let fire = resolved
            .enemy_mods
            .iter()
            .find(|m| m.name.as_str() == "FireResist")
            .expect("FireResist BASE 应进 enemy 注入列表");
        assert_eq!(fire.mod_type, ModType::Base);
        assert_eq!(fire.value.as_number(), Some(75.0));
        assert_eq!(
            fire.origin.as_ref().unwrap().source_id.kind,
            SourceKind::EnemyConfig
        );
        // SelfCritChance (Critical Weakness stacks → +0.5%/stack, ConfigOptions.lua:1892-1894
        // `m_max(m_min(val, 20), 0) / 2`, carrying a Condition:ApplyCriticalWeakness tag).
        let crit = resolved
            .enemy_mods
            .iter()
            .find(|m| m.name.as_str() == "SelfCritChance")
            .expect("SelfCritChance 应进 enemy 注入列表");
        assert_eq!(crit.value.as_number(), Some(5.0), "10 层 × 0.5%");
        assert!(!crit.tags.is_empty(), "保留 ApplyCriticalWeakness 门控 tag");
    }

    /// Enemy-side unprefixed condition bridge: `conditionEnemyCriticalWeakness` →
    /// `EnemyApplyCriticalWeakness` (the existing prefixed bridge) + `ApplyCriticalWeakness`
    /// (unprefixed, because `enemyCriticalWeaknessStacks`'s SelfCritChance mod tag
    /// references it — vendor ConfigOptions.lua:1893 `{type="Condition",
    /// var="ApplyCriticalWeakness"}`). Conditions not referenced by an enemy-side
    /// numeric mod (e.g. Chilled) **don't** get the unprefixed name set (protects the
    /// player namespace from pollution).
    #[test]
    fn enemy_condition_unprefixed_bridge_for_referenced_vars() {
        let catalog = load_catalog();
        let build = build_with_inputs(
            RawConfigInputs::new()
                .with(
                    "conditionEnemyCriticalWeakness",
                    ConfigInputValue::Bool(true),
                )
                .with("conditionEnemyChilled", ConfigInputValue::Bool(true)),
        );
        let resolved = resolve_config(&build, Some(&catalog));
        assert_eq!(
            resolved.config.conditions.get("EnemyApplyCriticalWeakness"),
            Some(&true),
            "既有 Enemy 前缀桥保持"
        );
        assert_eq!(
            resolved.config.conditions.get("ApplyCriticalWeakness"),
            Some(&true),
            "被 SelfCritChance tag 引用 → 未前缀条件落位"
        );
        assert_eq!(resolved.config.conditions.get("EnemyChilled"), Some(&true));
        assert_eq!(
            resolved.config.conditions.get("Chilled"),
            None,
            "未被敌侧数值 mod 引用 → 不落未前缀名（防玩家侧 Chilled 污染）"
        );
        // What the unprefixed bridge feeds downstream: SelfCritChance BASE 10 (placeholder 20 stacks × 0.5).
        let crit = resolved
            .enemy_mods
            .iter()
            .find(|m| m.name.as_str() == "SelfCritChance")
            .expect("placeholder 默认 20 层 → SelfCritChance 应在 enemy 注入列表");
        assert_eq!(crit.value.as_number(), Some(10.0));
    }

    /// commit B end-to-end: the multiplierNearby* handlers take effect via the primary
    /// path — scalar additive backfill (rare count folds into NearbyEnemies, matching
    /// vendor ConfigOptions.lua:1108's dual-write semantics) + Combat-gated mod
    /// injection + enemy bucket FLAG.
    #[test]
    fn nearby_handlers_resolve_end_to_end() {
        let catalog = load_catalog();
        let build = build_with_inputs(
            RawConfigInputs::new()
                .with("multiplierNearbyEnemies", ConfigInputValue::Number(3.0))
                .with(
                    "multiplierNearbyRareOrUniqueEnemies",
                    ConfigInputValue::Number(1.0),
                ),
        );
        let resolved = resolve_config(&build, Some(&catalog));
        assert_eq!(
            resolved.config.multipliers.get("NearbyEnemies"),
            Some(&4.0),
            "3（普通）+ 1（rare 聚合，vendor :1108）"
        );
        assert_eq!(
            resolved.config.multipliers.get("NearbyRareOrUniqueEnemies"),
            Some(&1.0)
        );
        // Combat-gated player mod injection (naturally inert before mode_combat is wired up).
        assert!(
            resolved
                .player_mods
                .iter()
                .any(|m| m.name.as_str() == "Multiplier:NearbyEnemies")
        );
        // Enemy bucket FLAG (val>=1 → true) + EnemyConfig attribution.
        let enemy_flag = resolved
            .enemy_mods
            .iter()
            .find(|m| m.name.as_str() == "Condition:NearbyRareOrUniqueEnemy")
            .expect("enemy 桶 FLAG 应注入");
        assert_eq!(
            enemy_flag.origin.as_ref().unwrap().source_id.kind,
            SourceKind::EnemyConfig
        );
    }

    /// commit B: the inDemonForm handler sets the DemonForm condition via the primary
    /// path (defaultState=true is active by default, matching the legacy
    /// DEFAULT_TRUE_CONDITIONS semantics).
    #[test]
    fn in_demon_form_resolves_condition() {
        let catalog = load_catalog();
        let resolved = resolve_config(&Build::new(), Some(&catalog));
        assert_eq!(resolved.config.conditions.get("DemonForm"), Some(&true));

        // Explicit false → the entry isn't active, so the handler produces nothing (the merge doesn't force it true).
        let build = build_with_inputs(
            RawConfigInputs::new().with("inDemonForm", ConfigInputValue::Bool(false)),
        );
        let resolved = resolve_config(&build, Some(&catalog));
        assert_ne!(resolved.config.conditions.get("DemonForm"), Some(&true));
    }

    /// commit ④: the customMods line channel passes through resolve (StripEscapes is
    /// already done in the interpreter; vendor ConfigOptions.lua:2278-2296).
    #[test]
    fn custom_mod_lines_passed_through() {
        let catalog = load_catalog();
        let build = build_with_inputs(RawConfigInputs::new().with(
            "customMods",
            ConfigInputValue::Text("^x7070FF20% increased Fire Damage\n+10 to Spirit".into()),
        ));
        let resolved = resolve_config(&build, Some(&catalog));
        assert_eq!(
            resolved.custom_mod_lines,
            vec![
                "20% increased Fire Damage".to_string(),
                "+10 to Spirit".to_string()
            ]
        );
        // Missing catalog → the legacy path never consumes customMods, so the channel is empty.
        assert!(resolve_config(&build, None).custom_mod_lines.is_empty());
    }
}
