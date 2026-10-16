use pobr_core::calc::MinimalInput;
use pobr_data::{catalog::UtilityBaseStats, prelude::*};

#[test]
fn ultimate_flask_local_properties_and_global_bonuses_are_separate() {
    let mut session = super::support::session(MinimalInput::default());
    // Ultimate Life Flask: 920 life over 3 seconds, 75 charges, 10 per use.
    let base = UtilityBaseStats {
        life: 920.0,
        mana: 0.0,
        duration: 3.0,
        charges_max: 75.0,
        charges_used: 10.0,
    };
    let item = Item {
        base: pobr_data::item::ItemBaseId::from("Ultimate Life Flask"),
        quality: 20,
        modifier_texts: vec![
            "50% increased Amount Recovered".into(),
            "50% increased Recovery Rate".into(),
            "50% of Recovery applied Instantly".into(),
            "40% increased Charges".into(),
            "20% reduced Charges per use".into(),
        ],
        rarity: ItemRarity::Magic,
        corrupted: false,
        implicit_texts: vec![],
        enchant_texts: vec![],
        rolled_defence: RolledDefence::default(),
        socketed: vec![],
        parsed_stats: vec![],
    };
    session.add_utility_base("Flask 1", &item, &base);
    session.add_flask_charm("Flask 1", &item);
    let other = Item {
        modifier_texts: vec![],
        quality: 0,
        ..item.clone()
    };
    session.add_utility_base("Flask 2", &other, &base);
    session.add_flask_charm("Flask 2", &other);
    session
        .add_modifier_texts(&[
            "20% increased Life Recovery from Flasks",
            "20% of Flask Recovery applied Instantly",
            "25% increased Flask Charges gained",
            "Flasks gain 0.2 charges per Second",
            "10% reduced Flask Charges used",
        ])
        .unwrap();
    session.perform_minimal().unwrap();
    let out = &session.output().utility_stats;
    let near = |id: &str, expected: f64| {
        assert!(
            (out[id] - expected).abs() < 1e-8,
            "{id}: {} != {expected}",
            out[id]
        )
    };
    // 920 * 1.2 quality * 1.5 local recovery * 1.2 global recovery = 1987.2.
    near("Flask1Recovery", 1987.2);
    near("Flask1InstantRecovery", 1391.04); // 50% local + 20% global.
    near("Flask1Duration", 2.0); // 3 / 1.5 local recovery rate.
    near("Flask1RecoveryPerSecond", 298.08);
    near("Flask1ChargesMax", 105.0);
    near("Flask1ChargesUsed", 7.0); // floor(floor(10 * .8) * .9).
    near("Flask1Uses", 15.0);
    near("Flask1ChargeGen", 0.25); // .2 * 1.25 global charge gain.
    near("Flask2Recovery", 1104.0);
    near("Flask2Duration", 3.0);
    near("Flask2ChargesMax", 75.0);
    near("Flask2ChargesUsed", 9.0);
    assert!(
        session.unsupported_modifier_texts().is_empty(),
        "{:?}",
        session.unsupported_modifier_texts()
    );
    let details = session.derived_detail("Flask1Recovery").unwrap();
    let input = |id: &str| details.inputs.iter().find(|(key, _)| key == id).unwrap().1;
    assert!(
        (input("UtilityBaseRecovery")
            * input("UtilityQualityMod")
            * input("UtilityLocalRecoveryMod")
            * input("UtilityRecoveryMod")
            * input("UtilityEffectMod")
            - out["Flask1Recovery"])
            .abs()
            < 1e-8
    );
    assert_eq!(session.detail_sources("LifeFlaskChargeGen").len(), 2);
}

fn charm(lines: &[&str]) -> Item {
    Item {
        base: pobr_data::item::ItemBaseId::from("Golden Charm"),
        quality: 20,
        modifier_texts: lines.iter().map(|line| (*line).into()).collect(),
        rarity: ItemRarity::Unique,
        corrupted: false,
        implicit_texts: vec!["Used when you kill a Rare or Unique enemy".into()],
        enchant_texts: vec![],
        rolled_defence: RolledDefence::default(),
        socketed: vec![],
        parsed_stats: vec![],
    }
}

#[test]
fn charm_mana_recovery_is_per_use_local_and_does_not_add_regeneration() {
    let mut session = super::support::session(MinimalInput::default());
    let base = UtilityBaseStats {
        life: 0.0,
        mana: 0.0,
        duration: 1.0,
        charges_max: 80.0,
        charges_used: 80.0,
    };
    // Game Mods CharmGainManaOnUse8: maximum roll 300 mana, once on use.
    let item = charm(&["Recover 300 Mana when Used"]);
    session.add_utility_base("Charm 1", &item, &base);
    session.add_flask_charm("Charm 1", &item);
    let other = charm(&[]);
    session.add_utility_base("Charm 2", &other, &base);
    session.add_inactive_utility_properties("Charm 2", &other);
    session.perform_minimal().unwrap();
    let out = session.output();
    assert_eq!(out.utility_stats["Charm1ManaRecoveryOnUse"], 300.0);
    assert_eq!(
        pobr_core::display_value(out, "Charm1ManaRecoveryOnUse"),
        300.0
    );
    assert_eq!(out.utility_stats["Charm2ManaRecoveryOnUse"], 0.0);
    assert_eq!(out.mana_regen, 0.0);
    assert_eq!(
        session
            .derived_detail("Charm1ManaRecoveryOnUse")
            .unwrap()
            .inputs,
        vec![("UtilityManaOnUse".into(), 300.0)]
    );
    assert!(session.unsupported_modifier_texts().is_empty());
}

#[test]
fn laviangas_constant_flask_has_zero_uses_and_retains_recovery_per_second() {
    let mut session = super::support::session(MinimalInput::default());
    let item = Item {
        base: pobr_data::item::ItemBaseId::from("Gargantuan Mana Flask"),
        quality: 0,
        implicit_texts: vec![],
        modifier_texts: vec![
            "This Flask cannot be Used but applies its Effect constantly".into(),
            "70% reduced Amount Recovered".into(),
        ],
        ..charm(&[])
    };
    // Flasks/ComponentCharges game tables: 185 mana over 2 s, 75/10 charges.
    let base = UtilityBaseStats {
        life: 0.0,
        mana: 185.0,
        duration: 2.0,
        charges_max: 75.0,
        charges_used: 10.0,
    };
    session.add_utility_base("Flask 2", &item, &base);
    session.add_inactive_utility_properties("Flask 2", &item);
    session.perform_minimal().unwrap();
    let out = &session.output().utility_stats;
    assert_eq!(out["Flask2Uses"], 0.0);
    assert!((out["Flask2RecoveryPerSecond"] - 27.75).abs() < 1e-9);
}

fn possession(
    spirit: &str,
    enabled: bool,
    include_absent: bool,
) -> pobr_core::calc::CalculationSession {
    use pobr_core::{CalcConfig, Modifier};
    let mut session = super::support::session(MinimalInput {
        base_life: 1000.0,
        base_hit_min: 100.0,
        base_hit_max: 100.0,
        base_action_rate: 2.0,
        ..Default::default()
    })
    .with_config(CalcConfig::attack().with_mode_combat(true));
    session.set_exporter_rules(include_absent, Default::default());
    session.add_modifiers([
        Modifier::number("CharmLimit", ModType::Base, 3.0),
        Modifier::number("Armour", ModType::Base, 100.0),
        Modifier::number("Evasion", ModType::Base, 100.0),
        Modifier::number("EnergyShield", ModType::Base, 100.0),
        // These affect the charm, not the separately granted possession buff.
        Modifier::number("CharmEffect", ModType::Inc, 100.0),
    ]);
    let text = format!("Possessed by Spirit Of The {spirit} for 20 seconds on use");
    let item = charm(&[&text]);
    if enabled {
        session.add_flask_charm("Charm 1", &item);
    }
    session.perform_minimal().unwrap();
    session
}

#[test]
fn spirit_bear_grants_20_percent_life_and_20_percent_reduced_damage_taken() {
    let session = possession("Bear", true, true);
    assert_eq!(session.output().life, 1200.0);
    assert_eq!(session.output().taken_multi_physical, 0.8);
}
#[test]
fn spirit_boar_grants_20_percent_extra_fire_and_always_bleeds() {
    let session = possession("Boar", true, true);
    assert_eq!(session.base_sum("DamageGainAsFire"), 20.0);
    assert_eq!(session.base_sum("BleedChance"), 100.0);
    assert_eq!(session.output().total_hit_avg, 120.0);
}
#[test]
fn spirit_ox_grants_60_percent_armour_and_20_percent_reduced_damage_taken() {
    let session = possession("Ox", true, true);
    assert_eq!(session.output().armour, 160.0);
    assert_eq!(session.output().taken_multi_physical, 0.8);
}
#[test]
fn spirit_cat_grants_60_percent_evasion_and_dex_family_speed() {
    let session = possession("Cat", true, true);
    assert_eq!(session.output().evasion, 160.0);
    assert_eq!(session.output().action_rate, 2.6);
}
#[test]
fn spirit_stag_grants_20_percent_extra_lightning_and_30_elemental_resistance() {
    let session = possession("Stag", true, true);
    assert_eq!(session.output().total_hit_avg, 120.0);
    assert_eq!(session.output().fire_resistance, 30.0);
    assert_eq!(session.output().cold_resistance, 30.0);
    assert_eq!(session.output().lightning_resistance, 30.0);
    assert_eq!(session.output().action_rate, 2.6);
}
#[test]
fn spirit_wolf_grants_30_percent_skill_speed_only_when_source_is_active() {
    assert_eq!(possession("Wolf", true, true).output().action_rate, 2.6);
    assert_eq!(possession("Wolf", false, true).output().action_rate, 2.0);
    assert_eq!(possession("Wolf", true, false).output().action_rate, 2.0);
}
#[test]
fn spirit_owl_grants_60_percent_es_80_percent_damage_and_20_percent_extra_cold() {
    let session = possession("Owl", true, true);
    assert_eq!(session.output().energy_shield, 160.0);
    assert_eq!(session.output().total_hit_avg, 216.0);
}
#[test]
fn spirit_primate_grants_80_percent_damage_and_all_damage_can_chill() {
    let session = possession("Primate", true, true);
    assert_eq!(session.output().total_hit_avg, 180.0);
    for name in [
        "PhysicalCanChill",
        "FireCanChill",
        "LightningCanChill",
        "ChaosCanChill",
    ] {
        assert!(session.has_flag(name));
    }
}
#[test]
fn spirit_serpent_grants_80_percent_damage_and_always_poisons_with_all_types() {
    let session = possession("Serpent", true, true);
    assert_eq!(session.output().total_hit_avg, 180.0);
    assert_eq!(session.base_sum("PoisonChance"), 100.0);
    for name in ["FireCanPoison", "ColdCanPoison", "LightningCanPoison"] {
        assert!(session.has_flag(name));
    }
}

#[test]
fn spirit_family_buffs_do_not_stack_and_slot_limits_gate_grants() {
    use pobr_core::{CalcConfig, Modifier};
    for (limit, expected_gain) in [(1.0, 0.0), (2.0, 20.0)] {
        let mut session = super::support::session(MinimalInput {
            base_action_rate: 2.0,
            ..Default::default()
        })
        .with_config(CalcConfig::attack().with_mode_combat(true));
        session.add_modifiers([Modifier::number("CharmLimit", ModType::Base, limit)]);
        session.add_flask_charm(
            "Charm 1",
            &charm(&["Possessed by Spirit Of The Wolf for 20 seconds on use"]),
        );
        session.add_flask_charm(
            "Charm 2",
            &charm(&["Possessed by Spirit Of The Stag for 20 seconds on use"]),
        );
        session.perform_minimal().unwrap();
        assert_eq!(session.output().action_rate, 2.6);
        assert_eq!(session.base_sum("DamageGainAsLightning"), expected_gain);
        let contributions = session.mod_db().contributions(
            ModType::Inc,
            session.config(),
            &[ModName::from("SkillSpeed")],
        );
        assert_eq!(contributions.len(), 1);
        assert_eq!(
            contributions[0].origin.as_ref().unwrap().source_id.id,
            if limit == 1.0 {
                "flask.charm1"
            } else {
                "flask.charm2"
            }
        );
    }
}
