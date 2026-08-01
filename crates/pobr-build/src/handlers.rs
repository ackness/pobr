//! Aggregation point for handler registration.
//!
//! Entries in the data tables (`overlay/config_options.json` /
//! `overlay/buff_definitions.json` ...) that can't be expressed in the restricted
//! template DSL carry only a stable `handler_id` string; at runtime, dispatch is decided
//! by the [`HandlerRegistry`] built here. The registered set is fixed at startup, zero I/O.
//!
//! Aggregation convention: T1/T2 each expose
//! `pub fn register_xxx_handlers(&mut HandlerRegistry)` in **their own module**, and
//! this file's `build_registry` appends a call to each, one per line (append-only, to
//! minimize conflicts in this shared file).
//!
//! handler_id naming convention: `config:<name>` (config domain, budget ≤54; `<name>`
//! follows the vendor var's original spelling as it appears in the overlay data),
//! `buff:<name>` (buff domain, budget ≤8); total <100 (a hard-boundary monitor for the
//! DSL — approaching the cap signals the data-driven split has failed).

use pobr_core::CampaignProgress;
use pobr_core::modifier::{ModTag, ModValue, Modifier};
use pobr_core::rules::config_interpreter::{ConfigInputValue, ConfigOutcome};
use pobr_core::rules::{Handler, HandlerOutcome, HandlerRegistry};
use pobr_data::modifier::ModType;
use pobr_data::monster::EnemyTier;

/// Upper budget for config-domain handlers (542 entries × 10% ≈ 54).
pub const CONFIG_HANDLER_BUDGET: usize = 54;
/// Upper budget for buff-domain handlers.
pub const BUFF_HANDLER_BUDGET: usize = 8;
/// Hard cap on the total handler count.
pub const TOTAL_HANDLER_CAP: usize = 100;

/// Handlers that are registered but currently only placeholder stubs (consumers should
/// report any hit entry as a warning, not silently treat it as covered):
/// - `config:presetBossSkills`: belongs to the boss skill preset table `boss_skills.json`.
/// - `buff:onslaught_flask`: the Silver Flask source effect needs a flask base data
///   column `effectInc` (gap F8) plus a rarity channel; and the PoE2 base item table has
///   no Silver Flask (vendor CalcPerform.lua:541-573 is a leftover PoE1 branch).
pub const STUB_HANDLER_IDS: &[&str] = &["buff:onslaught_flask", "config:presetBossSkills"];

/// Handlers that are registered and fully implemented but **context-gated** — they
/// depend on the `main_skill` field of [`HandlerCtx`] ([`pobr_core::rules::HandlerCtx`]),
/// which the config consumption point (`config_resolve` → `interpret`) hasn't wired up
/// yet, so they conservatively produce zero output until it is (unlike a stub: unit
/// tests already pin down the output once the gate holds, so wiring the context through
/// takes effect immediately with no handler changes needed).
pub const CTX_GATED_HANDLER_IDS: &[&str] = &[
    "config:ConcPathBypassCD",
    "config:FlickerStrikeBypassCD",
    "config:VigilantStrikeBypassCD",
];

/// Builds the full handler registry (append-only; each comment marker below is an
/// insertion point, one register call per line):
/// - config domain: first batch in [`register_config_handlers`], second batch (commit B)
///   in [`register_config_handlers_batch2`].
/// - buff domain: `pobr_core::rules::buff_expander::register_handlers` (commit C:
///   fortify/elusive/fanaticism implementations + the onslaught_flask stub, B2).
pub fn build_registry() -> HandlerRegistry {
    let mut registry = HandlerRegistry::new();
    // T1 append point: config handlers
    register_config_handlers(&mut registry);
    register_config_handlers_batch2(&mut registry);
    // T2 append point: buff handlers
    pobr_core::rules::buff_expander::register_handlers(&mut registry)
        .expect("启动期 buff handler 注册不冲突");
    registry
}

/// Builds the special-mod handler registry — the runtime dispatch point for
/// `handler_id: "special:<name>"` entries in `overlay/special_mods.json`. `BuildData`
/// uses it at load time to compile [`pobr_core::rules::SpecialModRules`], kept separate
/// from the buff/config domain's [`build_registry`] (special is an independent parsing
/// surface).
///
/// The gate test `special_mods_gate.rs` verifies: this registry covers every
/// `handler_id`, the total is <100, and it's <10% of all special entries (architecture
/// §5's hard-boundary monitor for the DSL).
pub fn build_special_registry() -> HandlerRegistry {
    let mut registry = HandlerRegistry::new();
    pobr_core::rules::register_special_handlers(&mut registry)
        .expect("启动期 special handler 注册不冲突");
    registry
}

/// First batch of config handlers.
///
/// Convention: when a handler's real consumption goes through the **scalar channel**
/// (list/numeric options read from [`ConfigOutcome::scalars`] by the build layer and fed
/// into existing logic), it's registered as a zero-Modifier-output handler — the only
/// purpose is to remove the entry from the `unhandled` report and pin down ownership of
/// its coverage:
/// - `config:enemyIsBoss`: wraps the existing EnemyTier wiring (scalar consumption goes
///   through [`enemy_tier_from_config`]; the actual enemy-tier bonuses live in the
///   `enemy_presets` domain + orchestrator, the handler itself produces no Modifier).
/// - `config:presetBossSkills`: a stub warning (belongs to the boss skill preset table
///   `boss_skills.json`), see [`STUB_HANDLER_IDS`].
///
/// `resistancePenalty` is a plain list entry in the overlay data (carries no
/// handler_id); its "wraps existing logic" lives in
/// [`campaign_progress_from_config`] (the existing CampaignProgress seven-tier table),
/// and doesn't count against the handler budget.
fn register_config_handlers(registry: &mut HandlerRegistry) {
    registry
        .register(
            "config:enemyIsBoss",
            Box::new(|_| HandlerOutcome::default()),
        )
        .expect("启动期注册不重复");
    registry
        .register(
            "config:presetBossSkills",
            Box::new(|_| HandlerOutcome::default()),
        )
        .expect("启动期注册不重复");
}

/// Second batch of config handlers (commit B, covering the 8 gaps that dualrun report
/// §2.4 hit across the 18-build set; all vendor line numbers were read directly from
/// `vendor/PathOfBuilding-PoE2/src/Modules/ConfigOptions.lua`). Grouped by treatment:
///
/// **Implemented (takes effect immediately)**:
/// - `config:multiplierNearbyEnemies` (:1102-1105): `Multiplier:NearbyEnemies` BASE val,
///   plus `Condition:OnlyOneNearbyEnemy` FLAG `val==1` (both Combat-tagged); scalar
///   addition backfills cfg.multipliers (keeps consuming the legacy path's
///   `multiplier*`-prefixed channel, and becomes the sole source once that path is removed).
/// - `config:multiplierNearbyRareOrUniqueEnemies` (:1106-1111): this var +
///   **folds into `Multiplier:NearbyEnemies`** (vendor's dual-write at :1108) +
///   `Condition:AtMostOneNearbyRareOrUniqueEnemy` FLAG `val<=1` + enemy bucket
///   `Condition:NearbyRareOrUniqueEnemy` FLAG `val>=1` (all Combat-tagged). The
///   NearbyEnemies fold-in is a behavior improvement the legacy path didn't have
///   (vendor's two same-named BASE NewMods add up ≡ additive merge in the scalar channel).
/// - `config:inDemonForm` (:345-347): `Condition:DemonForm`. Vendor carries a
///   `StatThreshold{stat=Life, threshold=2}` tag (gating whether the demon form can be
///   entered while at critically low life) — pobr's `ModTag` has no StatThreshold
///   dimension, so it's set with no threshold, matching the legacy
///   DEFAULT_TRUE_CONDITIONS semantics (the only difference is the low-life + DemonForm
///   combination, noted in the handler doc).
///
/// **Implemented (context-gated, see [`CTX_GATED_HANDLER_IDS`])**:
/// - `config:{ConcPath,FlickerStrike,VigilantStrike}BypassCD` (:309-311 /
///   :387-389 / :700-702): `CooldownRecovery` OVERRIDE 0; vendor scopes this to a named
///   skill via a `SkillName` tag — pobr matches equivalently via
///   `ctx.main_skill.skill_name` (OVERRIDE only fires when the main skill is that skill,
///   same scope; conservatively produces zero output while `main_skill` isn't wired up).
///
/// **Wraps existing logic (zero output, following the `config:enemyIsBoss` precedent)**:
/// - `config:questAct 4Eye of HinekoraTribal Medicine` /
///   `config:questInterlude 2QimahSeven Pillars` (dynamic quest entries, :56-108
///   `addQuestModsRewardsConfigOptions`: parseMod line by line over the option text) —
///   real consumption goes through the existing quest text channel (xml_build's
///   `push_quest_lines` feeds the `<Input string>` option text into
///   `global_modifier_texts` → mod_parser, matching vendor's `applyModsFromString`
///   semantics); registering the handler only removes the entry from the unhandled
///   report and pins down coverage ownership (the injection channel isn't switched
///   until quest naming is unified per §3-⑤, to avoid double-counting).
fn register_config_handlers_batch2(registry: &mut HandlerRegistry) {
    registry
        .register(
            "config:ConcPathBypassCD",
            bypass_cd_handler("Consecrated Path of Endurance"),
        )
        .expect("启动期注册不重复");
    registry
        .register(
            "config:FlickerStrikeBypassCD",
            bypass_cd_handler("Flicker Strike"),
        )
        .expect("启动期注册不重复");
    registry
        .register(
            "config:VigilantStrikeBypassCD",
            bypass_cd_handler("Vigilant Strike"),
        )
        .expect("启动期注册不重复");
    registry
        .register(
            "config:inDemonForm",
            Box::new(|_| HandlerOutcome {
                player_mods: vec![Modifier::flag("Condition:DemonForm")],
                conditions: vec![("DemonForm".to_string(), true)],
                ..HandlerOutcome::default()
            }),
        )
        .expect("启动期注册不重复");
    registry
        .register(
            "config:multiplierNearbyEnemies",
            Box::new(|ctx| {
                let val = ctx.input();
                HandlerOutcome {
                    player_mods: vec![
                        combat_gated(Modifier::number(
                            "Multiplier:NearbyEnemies",
                            ModType::Base,
                            val,
                        )),
                        combat_gated(Modifier::new(
                            "Condition:OnlyOneNearbyEnemy",
                            ModType::Flag,
                            ModValue::Bool(val == 1.0),
                        )),
                    ],
                    scalars: vec![("NearbyEnemies".to_string(), val)],
                    ..HandlerOutcome::default()
                }
            }),
        )
        .expect("启动期注册不重复");
    registry
        .register(
            "config:multiplierNearbyRareOrUniqueEnemies",
            Box::new(|ctx| {
                let val = ctx.input();
                HandlerOutcome {
                    player_mods: vec![
                        combat_gated(Modifier::number(
                            "Multiplier:NearbyRareOrUniqueEnemies",
                            ModType::Base,
                            val,
                        )),
                        combat_gated(Modifier::number(
                            "Multiplier:NearbyEnemies",
                            ModType::Base,
                            val,
                        )),
                        combat_gated(Modifier::new(
                            "Condition:AtMostOneNearbyRareOrUniqueEnemy",
                            ModType::Flag,
                            ModValue::Bool(val <= 1.0),
                        )),
                    ],
                    enemy_mods: vec![combat_gated(Modifier::new(
                        "Condition:NearbyRareOrUniqueEnemy",
                        ModType::Flag,
                        ModValue::Bool(val >= 1.0),
                    ))],
                    scalars: vec![
                        ("NearbyRareOrUniqueEnemies".to_string(), val),
                        ("NearbyEnemies".to_string(), val),
                    ],
                    ..HandlerOutcome::default()
                }
            }),
        )
        .expect("启动期注册不重复");
    registry
        .register(
            "config:elementalConfluxElement",
            Box::new(|ctx| {
                // vendor ConfigOptions.lua:390-409: list option 1=Average → all
                // three elemental multipliers are 3 (consumer inverts to take a 1/3
                // split); 2/3/4 = lock a single element → that element is 1, the
                // others 0 (after inverting, ×1/×0). The consumer is the Elemental
                // Conflux buff payload's `Multiplier:ElementalConflux<El>Effect`
                // (SkillStatMap's
                // `skill_elemental_conflux_active_element_damage_+%_final`, an invert
                // Multiplier tag). The scalar is folded into cfg.multipliers by the
                // interpreter (matches vendor's NewMod GlobalEffect tag paired with a
                // global GetMultiplier lookup).
                let val = ctx.input();
                let (lightning, cold, fire) = match val {
                    2.0 => (1.0, 0.0, 0.0),
                    3.0 => (0.0, 1.0, 0.0),
                    4.0 => (0.0, 0.0, 1.0),
                    _ => (3.0, 3.0, 3.0), // 1 = Average (defaultIndex).
                };
                HandlerOutcome {
                    scalars: vec![
                        ("ElementalConfluxLightningEffect".to_string(), lightning),
                        ("ElementalConfluxColdEffect".to_string(), cold),
                        ("ElementalConfluxFireEffect".to_string(), fire),
                    ],
                    ..HandlerOutcome::default()
                }
            }),
        )
        .expect("启动期注册不重复");
    registry
        .register(
            "config:questAct 4Eye of HinekoraTribal Medicine",
            Box::new(|_| HandlerOutcome::default()),
        )
        .expect("启动期注册不重复");
    registry
        .register(
            "config:questInterlude 2QimahSeven Pillars",
            Box::new(|_| HandlerOutcome::default()),
        )
        .expect("启动期注册不重复");
}

/// Factory for the `*BypassCD` handler family (vendor shape:
/// `modList:NewMod("CooldownRecovery", "OVERRIDE", 0, "Config",
/// { type = "SkillName", skillName = … })`): emits `CooldownRecovery` OVERRIDE 0 when
/// the main skill's name matches (pobr's cooldown chain
/// `skill_mechanics::calc_cooldown` reads `db.override_("CooldownRecovery")`, same
/// semantics as PoB2 CalcOffence L326); conservatively produces zero output if
/// `main_skill` is absent / doesn't match. `includeTransfigured` (the Flicker/ColdSnap
/// shape) has no variant-gem semantics in PoE2, so this matches by name equality.
fn bypass_cd_handler(skill_name: &'static str) -> Handler {
    Box::new(move |ctx| {
        let matches_main = ctx
            .main_skill
            .is_some_and(|main| main.skill_name == skill_name);
        if !matches_main {
            return HandlerOutcome::default();
        }
        HandlerOutcome::player_mods(vec![Modifier::new(
            "CooldownRecovery",
            ModType::Override,
            ModValue::Number(0.0),
        )])
    })
}

/// Convenience wrapper for vendor's `{ type = "Condition", var = "Combat" }` tag.
fn combat_gated(modifier: Modifier) -> Modifier {
    modifier.with_tag(ModTag::condition("Combat", false))
}

/// Wraps the existing EnemyTier wiring: reads `enemyIsBoss` from the interpreter's
/// scalar output (a list, option values `None/Boss/Pinnacle/Uber`).
///
/// `None` = the entry isn't active or holds a string outside the table; the consumer
/// falls back to the orchestrator option's tier (matches the legacy parse_config path's
/// semantics; the catalog's default defaultIndex=3 resolves to Pinnacle, which is
/// exactly the PoB2/orchestrator default tier).
pub fn enemy_tier_from_config(outcome: &ConfigOutcome) -> Option<EnemyTier> {
    match outcome.scalars.get("enemyIsBoss")? {
        ConfigInputValue::Text(text) => EnemyTier::from_pob_str(text),
        _ => None,
    }
}

/// Wraps the existing CampaignProgress wiring ("config:resistance_penalty wraps
/// existing logic"): reads `resistancePenalty` from the interpreter's scalar output (a
/// list, numeric options across seven tiers `0/-10/…/-60`), and looks it up in the
/// existing CampaignProgress tier table.
///
/// `None` = the entry isn't active or its value isn't in the tier table; the consumer
/// falls back to the default Endgame (-60); the catalog's default defaultIndex=7
/// resolves to `-60`, matching the fallback value.
pub fn campaign_progress_from_config(outcome: &ConfigOutcome) -> Option<CampaignProgress> {
    match outcome.scalars.get("resistancePenalty")? {
        ConfigInputValue::Text(text) => text
            .parse::<f64>()
            .ok()
            .and_then(CampaignProgress::from_resistance_penalty),
        ConfigInputValue::Number(number) => CampaignProgress::from_resistance_penalty(*number),
        ConfigInputValue::Bool(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    /// Counts registered handlers in a domain by `handler_id` prefix (used by the A6 monitor assertion).
    fn count_with_prefix(registry: &HandlerRegistry, prefix: &str) -> usize {
        registry.ids().filter(|id| id.starts_with(prefix)).count()
    }

    /// A6 monitor assertion: config-domain handlers ≤54, buff-domain ≤8, total <100.
    /// This test automatically re-checks the budget whenever any track appends to the
    /// registry; approaching the cap is an architecture warning signal.
    #[test]
    fn handler_counts_within_budget() {
        let registry = build_registry();
        let config_count = count_with_prefix(&registry, "config:");
        let buff_count = count_with_prefix(&registry, "buff:");
        println!(
            "[A6] handler 计数：config = {config_count}/{CONFIG_HANDLER_BUDGET}，\
             buff = {buff_count}/{BUFF_HANDLER_BUDGET}，总数 = {}/{TOTAL_HANDLER_CAP}（stub = {}）",
            registry.len(),
            STUB_HANDLER_IDS.len()
        );

        assert!(
            config_count <= CONFIG_HANDLER_BUDGET,
            "config 域 handler 数 {config_count} 超预算 {CONFIG_HANDLER_BUDGET}（DSL 切分失败信号，回看裁决 P4/P6）"
        );
        assert!(
            buff_count <= BUFF_HANDLER_BUDGET,
            "buff 域 handler 数 {buff_count} 超预算 {BUFF_HANDLER_BUDGET}"
        );
        assert!(
            registry.len() < TOTAL_HANDLER_CAP,
            "handler 总数 {} 达到硬上限 {TOTAL_HANDLER_CAP}",
            registry.len()
        );
    }

    /// The first batch of config handlers are registered (including stubs); ids keep the overlay data's original spelling.
    #[test]
    fn first_batch_config_handlers_registered() {
        let registry = build_registry();
        assert!(registry.get("config:enemyIsBoss").is_some());
        assert!(registry.get("config:presetBossSkills").is_some());
        for stub in STUB_HANDLER_IDS {
            assert!(registry.get(stub).is_some(), "stub `{stub}` 应已注册");
        }
        // The first batch of handlers all produce zero output (real consumption goes through the scalar channel / a later stage).
        let handler = registry.get("config:enemyIsBoss").unwrap();
        let out = handler(&pobr_core::rules::HandlerCtx::with_inputs(&[0.0]));
        assert!(out.player_mods.is_empty());
        assert!(out.enemy_mods.is_empty());
        assert!(out.conditions.is_empty());
        assert!(out.scalars.is_empty());
    }

    /// The second batch of handlers (commit B) are all registered; the ctx-gated list is a subset of the registered set.
    #[test]
    fn second_batch_config_handlers_registered() {
        let registry = build_registry();
        for id in [
            "config:ConcPathBypassCD",
            "config:FlickerStrikeBypassCD",
            "config:VigilantStrikeBypassCD",
            "config:inDemonForm",
            "config:multiplierNearbyEnemies",
            "config:multiplierNearbyRareOrUniqueEnemies",
            "config:questAct 4Eye of HinekoraTribal Medicine",
            "config:questInterlude 2QimahSeven Pillars",
        ] {
            assert!(registry.get(id).is_some(), "`{id}` 应已注册");
        }
        for id in CTX_GATED_HANDLER_IDS {
            assert!(registry.get(id).is_some(), "ctx 门控 `{id}` 应已注册");
        }
    }

    /// The BypassCD family (vendor ConfigOptions.lua:387-389 etc.): main skill name
    /// match → CooldownRecovery OVERRIDE 0; missing main-skill context / name mismatch →
    /// conservatively zero output.
    #[test]
    fn bypass_cd_gated_on_main_skill() {
        use pobr_core::rules::{HandlerCtx, MainSkillCtx};

        let registry = build_registry();
        let handler = registry.get("config:FlickerStrikeBypassCD").unwrap();

        // Not wired up yet (main_skill = None) → zero output (the current shape of the config consumption point).
        let out = handler(&HandlerCtx::with_inputs(&[1.0]));
        assert!(out.player_mods.is_empty());

        // Main skill matches → OVERRIDE 0.
        let main = MainSkillCtx {
            skill_name: "Flicker Strike".to_string(),
            self_cast: false,
        };
        let ctx = HandlerCtx {
            inputs: &[1.0],
            main_skill: Some(&main),
            ..HandlerCtx::default()
        };
        let out = handler(&ctx);
        assert_eq!(out.player_mods.len(), 1);
        assert_eq!(out.player_mods[0].name.as_str(), "CooldownRecovery");
        assert_eq!(out.player_mods[0].mod_type, ModType::Override);
        assert_eq!(out.player_mods[0].value.as_number(), Some(0.0));

        // Name mismatch → zero output (matches vendor's SkillName tag scoping semantics).
        let other = MainSkillCtx {
            skill_name: "Vigilant Strike".to_string(),
            self_cast: false,
        };
        let ctx = HandlerCtx {
            inputs: &[1.0],
            main_skill: Some(&other),
            ..HandlerCtx::default()
        };
        assert!(handler(&ctx).player_mods.is_empty());
    }

    /// inDemonForm (vendor :345-347): DemonForm condition + FLAG mod (the
    /// StatThreshold(Life≥2) dimension is missing, so it matches legacy
    /// DEFAULT_TRUE_CONDITIONS semantics instead).
    #[test]
    fn in_demon_form_sets_condition() {
        let registry = build_registry();
        let handler = registry.get("config:inDemonForm").unwrap();
        let out = handler(&pobr_core::rules::HandlerCtx::with_inputs(&[1.0]));
        assert_eq!(out.conditions, vec![("DemonForm".to_string(), true)]);
        assert_eq!(out.player_mods.len(), 1);
        assert_eq!(out.player_mods[0].name.as_str(), "Condition:DemonForm");
    }

    /// multiplierNearbyEnemies (vendor :1102-1105): Multiplier BASE +
    /// OnlyOneNearbyEnemy FLAG val==1 (Combat-tagged) + scalar backfill.
    #[test]
    fn nearby_enemies_handler_outputs() {
        let registry = build_registry();
        let handler = registry.get("config:multiplierNearbyEnemies").unwrap();

        let out = handler(&pobr_core::rules::HandlerCtx::with_inputs(&[3.0]));
        assert_eq!(out.player_mods.len(), 2);
        let mult = &out.player_mods[0];
        assert_eq!(mult.name.as_str(), "Multiplier:NearbyEnemies");
        assert_eq!(mult.value.as_number(), Some(3.0));
        assert_eq!(mult.tags, vec![ModTag::condition("Combat", false)]);
        assert_eq!(out.player_mods[1].value, ModValue::Bool(false), "3 > 1");
        assert_eq!(out.scalars, vec![("NearbyEnemies".to_string(), 3.0)]);

        let out = handler(&pobr_core::rules::HandlerCtx::with_inputs(&[1.0]));
        assert_eq!(out.player_mods[1].value, ModValue::Bool(true), "恰一个");
    }

    /// multiplierNearbyRareOrUniqueEnemies (vendor :1106-1111): this var + a dual write
    /// folding into NearbyEnemies + AtMostOne FLAG + enemy bucket FLAG + two scalars.
    #[test]
    fn nearby_rare_or_unique_handler_outputs() {
        let registry = build_registry();
        let handler = registry
            .get("config:multiplierNearbyRareOrUniqueEnemies")
            .unwrap();

        let out = handler(&pobr_core::rules::HandlerCtx::with_inputs(&[2.0]));
        let names: Vec<_> = out
            .player_mods
            .iter()
            .map(|m| m.name.as_str().to_string())
            .collect();
        assert_eq!(
            names,
            vec![
                "Multiplier:NearbyRareOrUniqueEnemies",
                "Multiplier:NearbyEnemies",
                "Condition:AtMostOneNearbyRareOrUniqueEnemy"
            ]
        );
        assert_eq!(out.player_mods[2].value, ModValue::Bool(false), "2 > 1");
        assert_eq!(out.enemy_mods.len(), 1);
        assert_eq!(
            out.enemy_mods[0].name.as_str(),
            "Condition:NearbyRareOrUniqueEnemy"
        );
        assert_eq!(out.enemy_mods[0].value, ModValue::Bool(true), "2 ≥ 1");
        assert_eq!(
            out.scalars,
            vec![
                ("NearbyRareOrUniqueEnemies".to_string(), 2.0),
                ("NearbyEnemies".to_string(), 2.0)
            ]
        );

        // countAllowZero shape: 0 → AtMostOne is true, enemy bucket FLAG is false.
        let out = handler(&pobr_core::rules::HandlerCtx::with_inputs(&[0.0]));
        assert_eq!(out.player_mods[2].value, ModValue::Bool(true));
        assert_eq!(out.enemy_mods[0].value, ModValue::Bool(false));
    }

    /// elementalConfluxElement (vendor ConfigOptions.lua:390-409): the Average tier
    /// (default index 1) sets all three elemental multipliers to 3; locking a single
    /// element sets that element to 1 and the rest to 0. The scalar is folded into
    /// cfg.multipliers by the interpreter, consumed by the Conflux buff payload's invert
    /// Multiplier tag (73 × 1/3 = 24.33, same value as vendor's Tabulate).
    #[test]
    fn elemental_conflux_element_handler_outputs() {
        let registry = build_registry();
        let handler = registry.get("config:elementalConfluxElement").unwrap();

        let out = handler(&pobr_core::rules::HandlerCtx::with_inputs(&[1.0]));
        assert_eq!(
            out.scalars,
            vec![
                ("ElementalConfluxLightningEffect".to_string(), 3.0),
                ("ElementalConfluxColdEffect".to_string(), 3.0),
                ("ElementalConfluxFireEffect".to_string(), 3.0),
            ],
            "Average 档全 3"
        );
        assert!(out.player_mods.is_empty());

        let out = handler(&pobr_core::rules::HandlerCtx::with_inputs(&[3.0]));
        assert_eq!(
            out.scalars,
            vec![
                ("ElementalConfluxLightningEffect".to_string(), 0.0),
                ("ElementalConfluxColdEffect".to_string(), 1.0),
                ("ElementalConfluxFireEffect".to_string(), 0.0),
            ],
            "锁 Cold 档"
        );
    }

    /// The quest wrapper handlers: zero output (real consumption goes through the
    /// existing quest text channel, see [`register_config_handlers_batch2`]).
    #[test]
    fn quest_wrapper_handlers_are_zero_output() {
        let registry = build_registry();
        for id in [
            "config:questAct 4Eye of HinekoraTribal Medicine",
            "config:questInterlude 2QimahSeven Pillars",
        ] {
            let handler = registry.get(id).unwrap();
            let out = handler(&pobr_core::rules::HandlerCtx::with_inputs(&[0.0]));
            assert!(out.player_mods.is_empty());
            assert!(out.enemy_mods.is_empty());
            assert!(out.conditions.is_empty());
            assert!(out.scalars.is_empty());
        }
    }

    fn outcome_with_scalar(var: &str, value: ConfigInputValue) -> ConfigOutcome {
        let mut scalars = BTreeMap::new();
        scalars.insert(var.to_string(), value);
        ConfigOutcome {
            scalars,
            ..ConfigOutcome::default()
        }
    }

    /// enemyIsBoss scalar wrapper: maps the four tiers; a value outside the table or missing returns None (matches the legacy path's semantics).
    #[test]
    fn enemy_tier_wrapper_maps_scalar() {
        let outcome = outcome_with_scalar("enemyIsBoss", ConfigInputValue::Text("Uber".into()));
        assert_eq!(enemy_tier_from_config(&outcome), Some(EnemyTier::Uber));

        let outcome = outcome_with_scalar("enemyIsBoss", ConfigInputValue::Text("奇怪档".into()));
        assert_eq!(enemy_tier_from_config(&outcome), None);

        assert_eq!(enemy_tier_from_config(&ConfigOutcome::default()), None);
    }

    /// resistancePenalty scalar wrapper: both the list text-value and numeric-value shapes map onto the existing seven-tier table.
    #[test]
    fn campaign_progress_wrapper_maps_scalar() {
        let outcome =
            outcome_with_scalar("resistancePenalty", ConfigInputValue::Text("-30".into()));
        assert_eq!(
            campaign_progress_from_config(&outcome),
            CampaignProgress::from_resistance_penalty(-30.0)
        );
        assert!(campaign_progress_from_config(&outcome).is_some());

        let outcome = outcome_with_scalar("resistancePenalty", ConfigInputValue::Number(-15.0));
        assert_eq!(
            campaign_progress_from_config(&outcome),
            None,
            "表外值回 None"
        );

        assert_eq!(
            campaign_progress_from_config(&ConfigOutcome::default()),
            None
        );
    }
}
