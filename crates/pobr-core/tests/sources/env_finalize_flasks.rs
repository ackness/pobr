//! env_finalize stage 3: end-to-end flask/charm mod merging.
//!
//! Payload construction is already pinned by `item::ingest_flask_charm` unit tests
//! (tests/item_source.rs). This file verifies the **merge semantics**
//! (vendor CalcPerform.lua:1429-1663): the whole section gated by `cfg.mode_combat`, effect
//! multiplier scaling (FlaskEffect/CharmEffect + the payload-local LocalUtilityEffect),
//! charm limit (Override/Sum + a cap constant), mergeBuff taking the max for matching bases,
//! Using* condition writeback, and idempotency guards.
//!
//! Key invariant (a ninja_parity anchor): with `mode_combat` defaulting to false, the output
//! is value-for-value unchanged whether or not the payload is in the player db.

use pobr_core::calc::env_finalize::merge_flasks_charms;
use pobr_core::calc::{Actor, ActorBaseStats, Env, MinimalInput};
use pobr_core::item::{ItemIngest, ingest_flask_charm_with_ctx};

/// Engine-side flask/charm ingest (signature matches the legacy `ingest_flask_charm`).
fn ingest_flask_charm(slot_name: &str, item: &Item) -> ItemIngest {
    ingest_flask_charm_with_ctx(slot_name, item, crate::support::ctx())
}
use pobr_core::{CalcConfig, ModDb, Modifier};
use pobr_data::prelude::*;

fn utility_item(base: &str, explicits: &[&str]) -> Item {
    Item {
        base: ItemBaseId::from(base),
        rarity: ItemRarity::Magic,
        quality: 0,
        corrupted: false,
        implicit_texts: Vec::new(),
        modifier_texts: explicits.iter().map(|t| (*t).to_string()).collect(),
        enchant_texts: Vec::new(),
        rolled_defence: RolledDefence::default(),
        socketed: Vec::new(),
        parsed_stats: Vec::new(),
    }
}

/// An Env pre-loaded with carriers (mode_combat is set by the caller).
fn env_with_carriers(mode_combat: bool, carriers: Vec<Modifier>) -> Env {
    let mut env = Env::new(Actor::new(1, ActorBaseStats::default()));
    env.cfg = CalcConfig::attack()
        .with_damage_type(DamageType::Physical)
        .with_mode_combat(mode_combat);
    for c in carriers {
        env.player.mod_db.add_mod(c);
    }
    env
}

fn carrier_for(slot: &str, base: &str, explicits: &[&str]) -> Vec<Modifier> {
    ingest_flask_charm(slot, &utility_item(base, explicits)).modifiers
}

fn lightning_res(db: &ModDb, cfg: &CalcConfig) -> f64 {
    db.sum(ModType::Base, cfg, &[ModName::from("LightningResistance")])
}

/// Baseline: a charm mod + CharmLimit 1 -> merges into the player db and gets scaled by the
/// CharmEffect multiplier (15 x 1.2 = 18, integer-truncated per vendor ScaleAddMod);
/// Using* conditions get set.
#[test]
fn charm_mods_merge_and_scale_with_charm_effect() {
    let mut carriers = carrier_for(
        "Charm 1",
        "Sapphire Charm of Lightning",
        &["+15% to Lightning Resistance"],
    );
    carriers.push(Modifier::number("CharmEffect", ModType::Inc, 20.0));
    carriers.push(Modifier::number("CharmLimit", ModType::Base, 1.0));
    let mut env = env_with_carriers(true, carriers);

    merge_flasks_charms(&mut env);

    assert_eq!(
        lightning_res(&env.player.mod_db, &env.cfg),
        18.0,
        "+15% lightning resist x (1 + 20/100) = 18 (integer-truncated)"
    );
    assert_eq!(env.cfg.conditions.get("UsingCharm"), Some(&true));
    assert_eq!(
        env.cfg.conditions.get("UsingSapphireCharmofLightning"),
        Some(&true)
    );
    // Attribution passthrough: the merged output traces back to the Flask source.
    let contributions = env.player.mod_db.contributions(
        ModType::Base,
        &env.cfg,
        &[ModName::from("LightningResistance")],
    );
    assert_eq!(
        contributions[0].origin.as_ref().unwrap().source_id,
        SourceId::new(SourceKind::Flask, "flask.charm1")
    );
}

/// Flask payload: the local `25% increased effect` and the global FlaskEffect INC add
/// together into the multiplier (10 x (1 + (20+25)/100) = 14.5 -> integer-truncated to 14);
/// UsingFlask condition gets set.
///
/// Onslaught is granted only once this flask payload has merged.
#[test]
fn flask_local_and_global_effect_inc_stack_additively() {
    let mut carriers = carrier_for(
        "Flask 1",
        "Quicksilver Flask",
        &[
            "Grants Onslaught during effect",
            "25% increased effect",
            "10% increased Movement Speed during effect",
        ],
    );
    carriers.push(Modifier::number("FlaskEffect", ModType::Inc, 20.0));
    let mut env = env_with_carriers(true, carriers);

    merge_flasks_charms(&mut env);

    assert_eq!(
        env.player
            .mod_db
            .sum(ModType::Inc, &env.cfg, &[ModName::from("MovementSpeed")]),
        14.0,
        "10 x 1.45 = 14.5 -> m_modf(round(...,2)) truncates to 14"
    );
    assert!(
        env.player.mod_db.flag(&env.cfg, "Condition:Onslaught"),
        "the active flask grants Onslaught"
    );
    assert_eq!(env.cfg.conditions.get("UsingFlask"), Some(&true));
    assert_eq!(env.cfg.conditions.get("UsingQuicksilverFlask"), Some(&true));
}

/// mode_combat=false -> output is value-for-value unchanged regardless of whether the payload
/// is present (a micro-anchor for the ninja_parity invariant).
#[test]
fn mode_combat_false_keeps_everything_untouched() {
    let mut carriers = carrier_for(
        "Charm 1",
        "Sapphire Charm of Lightning",
        &["+15% to Lightning Resistance"],
    );
    carriers.push(Modifier::number("CharmLimit", ModType::Base, 1.0));
    let mut env = env_with_carriers(false, carriers);
    let mods_before = env.player.mod_db.iter_mods().count();
    let conditions_before = env.cfg.conditions.clone();

    merge_flasks_charms(&mut env);

    assert_eq!(env.player.mod_db.iter_mods().count(), mods_before);
    assert_eq!(env.cfg.conditions, conditions_before);
    assert_eq!(lightning_res(&env.player.mod_db, &env.cfg), 0.0);
}

/// charm limit: CharmLimit 1 -> only the first charm (insertion order) merges in;
/// CharmLimit Base 5 gets truncated by the cap constant (charm_limit_cap = 3); with no
/// CharmLimit mod at all, no charm takes effect (vendor :1589 `Override or Sum` is 0).
#[test]
fn charm_limit_caps_number_of_active_charms() {
    let two_charms = |limit: Option<f64>| {
        let mut carriers = carrier_for(
            "Charm 1",
            "Sapphire Charm of Lightning",
            &["+15% to Lightning Resistance"],
        );
        carriers.extend(carrier_for(
            "Charm 2",
            "Ruby Charm of Embers",
            &["+25% to Fire Resistance"],
        ));
        if let Some(v) = limit {
            carriers.push(Modifier::number("CharmLimit", ModType::Base, v));
        }
        let mut env = env_with_carriers(true, carriers);
        merge_flasks_charms(&mut env);
        (
            lightning_res(&env.player.mod_db, &env.cfg),
            env.player
                .mod_db
                .sum(ModType::Base, &env.cfg, &[ModName::from("FireResistance")]),
        )
    };

    assert_eq!(
        two_charms(None),
        (0.0, 0.0),
        "no CharmLimit -> the charm has no effect"
    );
    assert_eq!(
        two_charms(Some(1.0)),
        (15.0, 0.0),
        "limit 1 -> only the first charm (insertion order)"
    );
    assert_eq!(two_charms(Some(2.0)), (15.0, 25.0), "limit 2 -> both enter");
    assert_eq!(
        two_charms(Some(5.0)),
        (15.0, 25.0),
        "Base 5 truncated by cap=3 still covers both"
    );
}

/// mergeBuff (vendor :41-63): mods with the same base and same parameters take the max
/// rather than stacking; different bases each take effect independently.
#[test]
fn merge_buff_takes_max_for_same_base_same_params() {
    let mut carriers = carrier_for(
        "Charm 1",
        "Sapphire Charm",
        &["+15% to Lightning Resistance"],
    );
    carriers.extend(carrier_for(
        "Charm 2",
        "Sapphire Charm",
        &["+12% to Lightning Resistance"],
    ));
    carriers.push(Modifier::number("CharmLimit", ModType::Base, 3.0));
    let mut env = env_with_carriers(true, carriers);

    merge_flasks_charms(&mut env);

    assert_eq!(
        lightning_res(&env.player.mod_db, &env.cfg),
        15.0,
        "same base, same parameters: takes the max (15), not stacked (27)"
    );
}

/// Idempotency: repeated calls (the same Env going through perform multiple times) don't
/// merge the payload in again.
#[test]
fn merge_is_idempotent_across_repeated_calls() {
    let mut carriers = carrier_for(
        "Charm 1",
        "Sapphire Charm of Lightning",
        &["+15% to Lightning Resistance"],
    );
    carriers.push(Modifier::number("CharmLimit", ModType::Base, 1.0));
    let mut env = env_with_carriers(true, carriers);

    merge_flasks_charms(&mut env);
    let after_first = lightning_res(&env.player.mod_db, &env.cfg);
    merge_flasks_charms(&mut env);
    merge_flasks_charms(&mut env);

    assert_eq!(lightning_res(&env.player.mod_db, &env.cfg), after_first);
    assert_eq!(after_first, 15.0);
}

/// FlasksDoNotApplyToPlayer (vendor :1561): both the flask buff and its conditions are
/// entirely withheld from the player; charms are unaffected by this flag.
#[test]
fn flasks_do_not_apply_to_player_flag_gates_flask_only() {
    let mut carriers = carrier_for(
        "Flask 1",
        "Quicksilver Flask",
        &["10% increased Movement Speed during effect"],
    );
    carriers.extend(carrier_for(
        "Charm 1",
        "Sapphire Charm",
        &["+15% to Lightning Resistance"],
    ));
    carriers.push(Modifier::number("CharmLimit", ModType::Base, 1.0));
    carriers.push(Modifier::flag("FlasksDoNotApplyToPlayer"));
    let mut env = env_with_carriers(true, carriers);

    merge_flasks_charms(&mut env);

    assert_eq!(
        env.player
            .mod_db
            .sum(ModType::Inc, &env.cfg, &[ModName::from("MovementSpeed")]),
        0.0
    );
    assert_eq!(env.cfg.conditions.get("UsingFlask"), None);
    assert_eq!(lightning_res(&env.player.mod_db, &env.cfg), 15.0);
    assert_eq!(env.cfg.conditions.get("UsingCharm"), Some(&true));
}

/// End-to-end (session -> perform): a charm's resistance mod reaches the output when
/// mode_combat=true; with mode_combat=false the output matches the baseline (a session
/// without the charm) value-for-value.
#[test]
fn end_to_end_charm_resistance_reaches_output_under_mode_combat() {
    let input = MinimalInput {
        base_life: 1_000.0,
        ..Default::default()
    };
    let run = |mode_combat: bool, with_charm: bool| {
        let cfg = CalcConfig::attack()
            .with_damage_type(DamageType::Physical)
            .with_mode_combat(mode_combat);
        let mut session = crate::support::session(input).with_config(cfg);
        if with_charm {
            let mut mods = carrier_for(
                "Charm 1",
                "Sapphire Charm of Lightning",
                &["+15% to Lightning Resistance"],
            );
            mods.push(Modifier::number("CharmLimit", ModType::Base, 1.0));
            session.add_modifiers(mods);
        }
        session
            .perform_minimal()
            .expect("perform")
            .lightning_resistance
    };

    let baseline = run(false, false);
    assert_eq!(
        run(false, true),
        baseline,
        "mode_combat=false: output is value-for-value unchanged whether the payload is present"
    );
    assert_eq!(
        run(true, true),
        baseline + 15.0,
        "mode_combat=true: the charm's lightning resist reaches the output"
    );
}

/// End-to-end: the charm budget no longer comes from a hand-built `CharmLimit` modifier —
/// it's injected by mod_parser parsing the belt implicit "Has 1 Charm Slot"
/// (vendor ModParser.lua:5453). Once the budget is unlocked, the charm's resistance reaches
/// the output; without that mod (budget 0, :1589), no charm takes effect at all.
#[test]
fn end_to_end_belt_charm_slots_text_unlocks_charm_budget() {
    let input = MinimalInput {
        base_life: 1_000.0,
        ..Default::default()
    };
    let run = |with_belt_line: bool| {
        let cfg = CalcConfig::attack()
            .with_damage_type(DamageType::Physical)
            .with_mode_combat(true);
        let mut session = crate::support::session(input).with_config(cfg);
        if with_belt_line {
            session
                .add_modifier_texts(["Has 1 Charm Slot"])
                .expect("parse belt implicit");
        }
        session.add_modifiers(carrier_for(
            "Charm 1",
            "Sapphire Charm of Lightning",
            &["+15% to Lightning Resistance"],
        ));
        session
            .perform_minimal()
            .expect("perform")
            .lightning_resistance
    };
    assert_eq!(
        run(true) - run(false),
        15.0,
        "'Has 1 Charm Slot' parses to CharmLimit BASE -> unlocks the charm budget"
    );
}

#[test]
fn local_flask_duration_does_not_extend_skill_effects() {
    let carriers = carrier_for(
        "Flask 1",
        "Life Flask",
        &[
            "47% increased Duration",
            "20% increased Skill Effect Duration during effect",
        ],
    );
    let mut env = env_with_carriers(true, carriers);
    merge_flasks_charms(&mut env);
    assert_eq!(
        env.player
            .mod_db
            .sum(ModType::Inc, &env.cfg, &[ModName::from("Duration")]),
        20.0
    );
    assert_eq!(
        env.player.mod_db.sum(
            ModType::Inc,
            &env.cfg,
            &[ModName::from("LocalUtility:Flask1:LocalUtilityDuration")]
        ),
        47.0
    );
}

#[test]
fn spell_mana_efficiency_only_reduces_spell_mana_cost() {
    use pobr_core::calc::skill_mechanics::{calc_life_cost, calc_mana_cost};
    let outcome =
        crate::support::parse_mod("39% increased Mana Cost Efficiency of Spells").unwrap();
    let mut db = ModDb::new();
    db.add_list(outcome.mods);
    // 139 mana / (1 + 39/100) = 100 mana; Attack and Life costs stay 139.
    assert_eq!(
        calc_mana_cost(&db, &CalcConfig::spell(), 139.0).final_cost,
        100.0
    );
    assert_eq!(
        calc_mana_cost(&db, &CalcConfig::attack(), 139.0).final_cost,
        139.0
    );
    assert_eq!(
        calc_life_cost(&db, &CalcConfig::spell(), 139.0).final_cost,
        139.0
    );
}

#[test]
fn onslaught_only_applies_during_the_source_items_effect() {
    use pobr_data::catalog::buffs::BuffDefinitionsDef;
    let path = crate::support::test_data_dir().join("../overlay-common/buff_definitions.json");
    let definitions: BuffDefinitionsDef =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    for (slot, base) in [("Charm 1", "Silver Charm"), ("Flask 1", "Silver Flask")] {
        for (combat, charm_slots, applies, expected_rate) in [
            (true, 1.0, true, 2.4),
            (false, 1.0, true, 2.0),
            (true, 0.0, false, 2.0),
        ] {
            let mut session = crate::support::session(MinimalInput {
                base_action_rate: 2.0,
                ..MinimalInput::default()
            })
            .with_config(CalcConfig::attack().with_mode_combat(combat));
            session.set_buff_definitions(definitions.buffs.clone());
            session.add_modifiers(carrier_for(slot, base, &["Grants Onslaught during effect"]));
            session.add_modifiers([Modifier::number("CharmLimit", ModType::Base, charm_slots)]);
            if !applies {
                // Another active utility item must not activate the excluded source's grant.
                if slot.starts_with("Flask") {
                    session.add_modifiers([Modifier::flag("FlasksDoNotApplyToPlayer")]);
                    session.add_modifiers([Modifier::number("CharmLimit", ModType::Base, 1.0)]);
                    session.add_modifiers(carrier_for("Charm 2", "Ruby Charm", &[]));
                } else {
                    session.add_modifiers(carrier_for("Flask 2", "Life Flask", &[]));
                }
            }
            // Onslaught grants 20% attack speed: 2 attacks/s * 1.2 = 2.4.
            assert_eq!(
                session.perform_minimal().unwrap().action_rate,
                expected_rate,
                "{slot}: combat={combat}, slots={charm_slots}"
            );
        }
    }
}

#[test]
fn reference_basis_filters_nested_onslaught_and_stat_rules() {
    use pobr_core::calc::skill_mechanics::calc_mana_cost;
    use pobr_core::rules::skill_stats;
    use pobr_data::catalog::buffs::BuffDefinitionsDef;
    let path = crate::support::test_data_dir().join("../overlay-common/buff_definitions.json");
    let definitions: BuffDefinitionsDef =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    for (include_absent, expected_rate, expected_cost) in [(true, 2.4, 100.0), (false, 2.0, 139.0)]
    {
        let mut session = crate::support::session(MinimalInput {
            base_action_rate: 2.0,
            ..MinimalInput::default()
        })
        .with_config(CalcConfig::spell().with_mode_combat(true));
        session.set_exporter_rules(include_absent, Default::default());
        session.set_buff_definitions(definitions.buffs.clone());
        session.add_modifiers([Modifier::number("CharmLimit", ModType::Base, 1.0)]);
        session.add_modifiers(carrier_for(
            "Charm 1",
            "Silver Charm",
            &["Grants Onslaught during effect"],
        ));
        session.add_modifiers(
            skill_stats::skill_modifiers("base_spell_mana_cost_efficiency_+%", 39.0).unwrap(),
        );
        assert_eq!(
            session.perform_minimal().unwrap().action_rate,
            expected_rate
        );
        // Game Onslaught: 20% Skill Speed and 10% Movement Speed.
        assert_eq!(
            session.stat_totals("MovementSpeed").1,
            if include_absent { 10.0 } else { 0.0 }
        );
        assert_eq!(
            calc_mana_cost(session.mod_db(), &CalcConfig::spell(), 139.0).final_cost,
            expected_cost
        );
        assert_eq!(session.mod_db().exporter_rules().count(), 2);
    }
}

#[test]
fn exporter_differs_rule_stays_in_reference_mode() {
    use pobr_data::source::{ExporterCoverage, ExporterRule};
    let mut modifier = Modifier::number("Life", ModType::Base, 123.0);
    modifier.exporter_rule = Some(std::sync::Arc::new(ExporterRule {
        id: "test_differs".into(),
        exporter: ExporterCoverage::Differs,
        game_source: "+123 to maximum Life".into(),
    }));
    let mut db = ModDb::new();
    db.set_exporter_rules(false, Default::default());
    db.add_mod(modifier);
    assert_eq!(
        db.sum(ModType::Base, &CalcConfig::new(), &[ModName::from("Life")]),
        123.0
    );
    assert_eq!(db.exporter_rules().count(), 1);
}
