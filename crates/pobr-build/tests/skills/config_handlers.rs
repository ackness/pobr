//! Config inputs must reach calculations, not only appear in a registry.

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};
use pobr_core::calc::OutputTable;
use pobr_gamedata::{GameData, data_version, repo_data_root};

fn data() -> BuildData {
    BuildData::load(&GameData::new(repo_data_root().join(data_version()))).unwrap()
}

fn calculate(data: &BuildData, config: &str, effective: bool, extra: &[&str]) -> OutputTable {
    let xml = format!(
        r#"<PathOfBuilding2>
<Build level="80" className="Warrior" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="ArmourBreakerPlayer" level="20" enabled="true"/></Skill></Skills>
<Items activeItemSet="1"><Item id="1">Rarity: RARE
Test Mace
Wooden Club
Implicits: 0
</Item><ItemSet id="1"><Slot name="Weapon 1" itemId="1"/></ItemSet></Items>
<Config>{config}</Config></PathOfBuilding2>"#
    );
    let build = parse_build(&xml).unwrap();
    calculate_with_data(
        &build,
        data,
        &DataOrchestratorOptions {
            mode_effective: effective,
            extra_modifier_texts: extra.iter().map(|text| text.to_string()).collect(),
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn armour_break_config_changes_physical_damage_and_respects_the_effective_view() {
    let data = data();
    let enabled = r#"<Input name="conditionEnemyArmourBroken" boolean="true"/>"#;
    let partial = format!(r#"{enabled}<Input name="multiplierArmourBreak" number="100"/>"#);
    let modifiers = [
        "Never deal Critical Hits",
        "Hits can't be Evaded",
        "Adds 1000 to 1000 Physical Damage to Attacks",
    ];
    let intact = calculate(&data, "", true, &modifiers);
    let full = calculate(&data, enabled, true, &modifiers);
    let partly_broken = calculate(&data, &partial, true, &modifiers);
    assert!(intact.dps > 0.0);
    assert!(full.dps > partly_broken.dps && partly_broken.dps > intact.dps);
    // With no other mitigation, removing all Armour restores the raw hit.
    assert!(
        (full.total_hit_avg
            - full
                .damage_components
                .iter()
                .map(|c| (c.min + c.max) / 2.0)
                .sum::<f64>())
        .abs()
            < 1e-9
    );
    let panel = calculate(&data, "", false, &modifiers);
    assert_eq!(calculate(&data, enabled, false, &modifiers).dps, panel.dps);
    assert_eq!(calculate(&data, &partial, false, &modifiers).dps, panel.dps);
    // A stale numeric field does not enable the unchecked debuff.
    assert_eq!(
        calculate(
            &data,
            r#"<Input name="multiplierArmourBreak" number="100"/>"#,
            true,
            &modifiers
        )
        .dps,
        intact.dps
    );
}

#[test]
fn blind_effect_override_scales_the_enemy_ratings() {
    let data = data();
    let enabled = r#"<Input name="conditionEnemyBlinded" boolean="true"/>"#;
    let at_half = format!(r#"{enabled}<Input name="overrideBuffBlinded" number="50"/>"#);
    let at_max = format!(r#"{enabled}<Input name="overrideBuffBlinded" number="1000"/>"#);
    let plain = calculate(&data, "", true, &[]);
    let half = calculate(&data, &at_half, true, &[]);
    let full = calculate(&data, enabled, true, &[]);
    assert!(half.hit_chance > plain.hit_chance && half.hit_chance < full.hit_chance);
    assert_eq!(
        calculate(&data, &at_max, true, &[]).hit_chance,
        full.hit_chance
    );
    assert_eq!(
        calculate(
            &data,
            r#"<Input name="overrideBuffBlinded" number="50"/>"#,
            true,
            &[]
        )
        .hit_chance,
        plain.hit_chance
    );
}

#[test]
fn elusive_override_reaches_the_existing_buff_calculation() {
    let data = data();
    let enabled = r#"<Input name="buffElusive" boolean="true"/>"#;
    let extra = ["Elusive"];
    let average = calculate(&data, enabled, true, &extra);
    let maximum = calculate(
        &data,
        &format!(r#"{enabled}<Input name="overrideBuffElusive" number="100"/>"#),
        true,
        &extra,
    );
    assert_eq!(average.avoid_all_damage_from_hits, 7.0);
    assert_eq!(maximum.avoid_all_damage_from_hits, 15.0);
}

#[test]
fn inexorable_critical_uses_the_configured_seconds_and_the_game_cap() {
    let data = data();
    let crit = |seconds: u32| {
        let xml = format!(
            r#"<PathOfBuilding2>
<Build level="80" className="Witch" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="FireballPlayer" level="20" enabled="true"/>
<Gem skillId="SupportInevitableCriticalsPlayer" level="1" enabled="true"/></Skill></Skills>
<Config><Input name="SecondsSinceInevitableCrit" number="{seconds}"/></Config>
</PathOfBuilding2>"#
        );
        calculate_with_data(&parse_build(&xml).unwrap(), &data, &Default::default())
            .unwrap()
            .crit_chance
    };
    // support_inevitable_criticals_critical_strike_chance_+%_per_second = 10,
    // support_inevitable_criticals_critical_strike_chance_+%_cap = 100.
    let one = crit(1);
    assert!(one > 0.0);
    assert!((crit(5) / one - 1.5 / 1.1).abs() < 1e-9);
    assert!((crit(10) / one - 2.0 / 1.1).abs() < 1e-9);
    assert_eq!(crit(20), crit(10));
}

fn configured_spell(
    data: &BuildData,
    skill: &str,
    set: u32,
    input: &str,
    amount: f64,
) -> OutputTable {
    use pobr_build::{Build, CharacterIdentity, SocketGroup};
    use pobr_core::rules::config_interpreter::ConfigInputValue;
    let mut build = Build::new()
        .with_character(CharacterIdentity {
            level: 80,
            class_name: "Witch".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(SocketGroup::new().with_gem_skill_stat_set(skill, 1, set));
    build
        .config
        .raw_inputs
        .values
        .insert(input.into(), ConfigInputValue::Number(amount));
    calculate_with_data(
        &build,
        data,
        &DataOrchestratorOptions {
            extra_modifier_texts: vec!["Never deal Critical Hits".into()],
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn dark_pact_uses_thirteen_percent_of_the_configured_skeleton_life() {
    let data = data();
    let damage = |life| {
        configured_spell(&data, "DarkPactPlayer", 1, "darkPactSkeletonLife", life).total_hit_avg
    };
    let plain = damage(0.0);
    assert!((damage(1000.0) - plain - 130.0).abs() < 1e-9);
    assert!((damage(2000.0) - plain - 260.0).abs() < 1e-9);
    // The input is scoped to Dark Pact.
    assert_eq!(
        configured_spell(&data, "FireballPlayer", 1, "darkPactSkeletonLife", 1000.0).dps,
        configured_spell(&data, "FireballPlayer", 1, "darkPactSkeletonLife", 0.0).dps
    );
}

#[test]
fn herald_of_ash_uses_its_burning_stat_and_the_configured_overkill() {
    let data = data();
    let damage = |overkill| {
        configured_spell(&data, "HeraldOfAshPlayer", 2, "hoaOverkill", overkill).skill_total_dot
    };
    // Level 1: herald_of_ash_burning_%_overkill_damage_per_minute = 1000.
    assert_eq!(damage(0.0), 0.0);
    assert!((damage(6000.0) - 1000.0).abs() < 1e-9);
    assert!((damage(12000.0) - 2000.0).abs() < 1e-9);
    assert_eq!(
        configured_spell(&data, "FireballPlayer", 1, "hoaOverkill", 6000.0).skill_total_dot,
        0.0
    );
}

#[test]
fn inexorable_critical_two_caps_both_chance_and_critical_damage_penalty() {
    use pobr_build::{Build, CharacterIdentity, SocketGroup};
    use pobr_core::rules::config_interpreter::ConfigInputValue;
    let data = data();
    let run = |seconds| {
        let mut build = Build::new()
            .with_character(CharacterIdentity {
                level: 80,
                class_name: "Witch".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(
                SocketGroup::new()
                    .with_gem_skill("FireballPlayer", 20)
                    .with_gem_skill("SupportInevitableCriticalsPlayerTwo", 1),
            );
        build.config.raw_inputs.values.insert(
            "SecondsSinceInevitableCrit".into(),
            ConfigInputValue::Number(seconds),
        );
        calculate_with_data(&build, &data, &Default::default()).unwrap()
    };
    let zero = run(0.0);
    let one = run(1.0);
    let cap = run(5.0);
    assert!((one.crit_chance / zero.crit_chance - 1.4).abs() < 1e-9);
    assert!((cap.crit_chance / zero.crit_chance - 3.0).abs() < 1e-9);
    assert!(((one.crit_multiplier - 1.0) / (zero.crit_multiplier - 1.0) - 0.9).abs() < 1e-9);
    assert!(((cap.crit_multiplier - 1.0) / (zero.crit_multiplier - 1.0) - 0.5).abs() < 1e-9);
    assert_eq!(run(10.0).crit_multiplier, cap.crit_multiplier);
    assert_eq!(run(10.0).crit_chance, cap.crit_chance);
}

#[test]
fn unchecking_armour_break_clears_the_imported_condition() {
    use pobr_build::{Build, CharacterIdentity, SocketGroup, calculate_with_data_session};
    use pobr_core::rules::config_interpreter::ConfigInputValue;
    let data = data();
    let mut build = Build::new()
        .with_character(CharacterIdentity {
            level: 80,
            class_name: "Witch".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(SocketGroup::new().with_gem_skill("FireballPlayer", 20));
    build
        .config
        .conditions
        .insert("EnemyArmourBroken".into(), true);
    build.config.raw_inputs.values.insert(
        "conditionEnemyArmourBroken".into(),
        ConfigInputValue::Bool(false),
    );
    let session = calculate_with_data_session(&build, &data, &Default::default()).unwrap();
    assert!(!session.config().condition("EnemyArmourBroken"));
}

#[test]
fn configured_action_speed_slow_reads_potency_and_preserves_reference_basis() {
    let data = data();
    // Config custom modifiers are an existing input for a generic slow.
    // 50% is the game keyword's default maximum Chill magnitude; this
    // test supplies the slow directly, without assuming a Chill application.
    let config = r#"<Input name="customMods" string="50% reduced Action Speed"/>"#;
    let plain = calculate(&data, "", true, &[]);
    let slowed = calculate(&data, config, true, &[]);
    let reduced = calculate(
        &data,
        config,
        true,
        &["30% reduced Slowing Potency of Debuffs on You"],
    );
    let unaffected = calculate(&data, config, true, &["Your speed is unaffected by Slows"]);
    assert!((slowed.action_rate / plain.action_rate - 0.5).abs() < 1e-6);
    assert!((reduced.action_rate / plain.action_rate - 0.65).abs() < 1e-6);
    assert_eq!(unaffected.action_rate, plain.action_rate);
    assert_eq!(slowed.effective_movement_speed_mod, 0.5);
    assert_eq!(reduced.effective_movement_speed_mod, 0.65);
    assert_eq!(unaffected.effective_movement_speed_mod, 1.0);

    let build = parse_build(&format!(
        r#"<PathOfBuilding2><Build level="80" className="Witch" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="FireballPlayer" level="20" enabled="true"/></Skill></Skills>
<Config>{config}</Config></PathOfBuilding2>"#
    ))
    .unwrap();
    let mut options = DataOrchestratorOptions::reference_conditions();
    let reference = calculate_with_data(&build, &data, &options).unwrap();
    options.extra_modifier_texts = vec!["30% reduced Slowing Potency of Debuffs on You".into()];
    let omitted = calculate_with_data(&build, &data, &options).unwrap();
    assert_eq!(omitted.action_rate, reference.action_rate);
    assert_eq!(
        omitted.effective_movement_speed_mod,
        reference.effective_movement_speed_mod
    );
}

#[test]
fn consecrated_ground_config_and_native_state_agree_without_assuming_a_source() {
    use pobr_core::calc::{CalculationSession, MinimalInput};
    use pobr_core::{CalcConfig, Modifier};
    use pobr_data::prelude::*;
    let data = data();
    for (configured, combat) in [(false, true), (true, true), (true, false)] {
        let xml_config =
            format!(r#"<Input name="conditionOnConsecratedGround" boolean="{configured}"/>"#);
        // End-to-end config uses the normal combat view.
        if combat {
            let output = calculate(&data, &xml_config, true, &[]);
            let expected = if configured { 0.05 * output.life } else { 0.0 };
            assert!((output.life_regen - expected).abs() < 1e-8);
            assert_eq!(
                output.curse_effect_on_self,
                if configured { 50.0 } else { 100.0 }
            );
        }
        for native_flag in [false, true] {
            let mut cfg = CalcConfig::new().with_mode_combat(combat);
            cfg.track_reads();
            if !native_flag {
                cfg = cfg.with_condition("OnConsecratedGround", configured);
            }
            let mut session = CalculationSession::new(MinimalInput {
                base_life: 1000.0,
                ..Default::default()
            })
            .with_config(cfg);
            // A changed final maximum Life must be used; no flat 50-life grant.
            session.add_modifiers([
                Modifier::number("MaximumLife", ModType::Inc, 50.0),
                // The actor can be on an ally's ground. Its own ground effect
                // and ordinary buff effect cannot silently scale that ground.
                Modifier::number("ConsecratedGroundEffect", ModType::Inc, 100.0),
                Modifier::number("BuffEffect", ModType::Inc, 100.0),
            ]);
            if native_flag && configured {
                session.add_modifiers([
                    Modifier::flag("Condition:OnConsecratedGround"),
                    Modifier::flag("Condition:OnConsecratedGround"),
                ]);
            }
            session.perform_minimal().unwrap();
            assert!(
                session
                    .config()
                    .read_variables()
                    .conditions
                    .contains("OnConsecratedGround")
            );
            assert_eq!(session.output().life, 1500.0);
            assert_eq!(
                session.output().life_regen,
                if configured && combat { 75.0 } else { 0.0 }
            );
            assert_eq!(
                session.output().curse_effect_on_self,
                if configured && combat { 50.0 } else { 100.0 }
            );
            if configured && combat {
                assert!(!session.detail_sources("LifeRegen").is_empty());
                assert!(!session.detail_sources("CurseEffectOnSelf").is_empty());
            }
        }
    }
}

#[test]
fn consecrated_ground_creation_properties_are_local_even_when_the_charm_is_inactive() {
    use pobr_core::calc::{CalculationSession, MinimalInput};
    use pobr_core::{CalcConfig, Modifier, display_catalog, display_value};
    use pobr_data::{catalog::UtilityBaseStats, prelude::*};
    let data = data();
    // Check our fixed creation properties against the active game snapshot,
    // not an exporter tooltip or a pinned-version path.
    let mods: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(repo_data_root().join(data_version()).join("base/mods.json"))
            .unwrap(),
    )
    .unwrap();
    let source = mods
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == "UniqueCharmCreateConsecratedGround1")
        .unwrap();
    for (id, expected) in [
        ("charm_create_consecrated_ground_when_used", 1),
        (
            "charm_create_consecrated_ground_when_used_duration_ms",
            4000,
        ),
        ("charm_create_consecrated_ground_when_used_radius", 14),
    ] {
        let stat = source["stats"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["stat_id"] == id)
            .unwrap();
        assert_eq!(stat["min"], expected);
        assert_eq!(stat["max"], expected);
    }
    for present in [false, true] {
        for active in [false, true] {
            for on_ground in [false, true] {
                let item = Item {
                    base: "Staunching Charm".into(),
                    quality: 20,
                    rarity: ItemRarity::Unique,
                    corrupted: false,
                    modifier_texts: if present {
                        vec!["Creates Consecrated Ground on use".into()]
                    } else {
                        vec![]
                    },
                    implicit_texts: vec![],
                    enchant_texts: vec![],
                    rolled_defence: RolledDefence::default(),
                    socketed: vec![],
                    parsed_stats: vec![],
                };
                let mut session = CalculationSession::new(MinimalInput {
                    base_life: 2000.0,
                    ..Default::default()
                })
                .with_config(
                    CalcConfig::new()
                        .with_mode_combat(true)
                        .with_condition("OnConsecratedGround", on_ground),
                );
                session.set_parser_rules(data.parser_rules.as_ref().unwrap().clone());
                // Duplicate sources must not duplicate the ground's benefit.
                for slot in ["Charm 1", "Charm 2"] {
                    session.add_utility_base(
                        slot,
                        &item,
                        &UtilityBaseStats {
                            life: 0.0,
                            mana: 0.0,
                            duration: 3.0,
                            charges_max: 40.0,
                            charges_used: 30.0,
                        },
                    );
                    if active {
                        session.add_flask_charm(slot, &item);
                    } else {
                        session.add_inactive_utility_properties(slot, &item);
                    }
                }
                session.add_modifiers([
                    Modifier::number("CharmEffect", ModType::Inc, 100.0),
                    Modifier::number("CharmDuration", ModType::Inc, 100.0),
                ]);
                session.perform_minimal().unwrap();
                assert!(
                    session.unsupported_modifier_texts().is_empty(),
                    "{:?}",
                    session.unsupported_modifier_texts()
                );
                assert_eq!(
                    session.output().life_regen,
                    if on_ground { 100.0 } else { 0.0 }
                );
                for slot in [1, 2] {
                    for (field, value, input) in [
                        (
                            "ConsecratedGroundDuration",
                            4.0,
                            "UtilityConsecratedGroundDuration",
                        ),
                        (
                            "ConsecratedGroundRadiusMetres",
                            1.4,
                            "UtilityConsecratedGroundRadiusMetres",
                        ),
                    ] {
                        let id = format!("Charm{slot}{field}");
                        let expected = if present { value } else { 0.0 };
                        assert_eq!(display_value(session.output(), &id), expected);
                        let detail = session.derived_detail(&id).unwrap();
                        assert_eq!(detail.formula, input);
                        assert_eq!(detail.inputs, vec![(input.into(), expected)]);
                        assert!(
                            display_catalog()
                                .iter()
                                .find(|d| d.id.as_str() == id)
                                .unwrap()
                                .pob_key
                                .is_none()
                        );
                    }
                }
            }
        }
    }
    // The exporter does not model Sanguis's creation properties. Reference
    // basis excludes those local stats without excluding explicit ground state.
    let mut session = CalculationSession::new(MinimalInput::default());
    session.set_parser_rules(data.parser_rules.as_ref().unwrap().clone());
    session.set_exporter_rules(false, Default::default());
    session
        .add_modifier_texts(&["Creates Consecrated Ground on use"])
        .unwrap();
    assert!(
        session
            .exporter_rules()
            .any(|rule| rule.id == "sanguis_consecrated_ground_on_use")
    );
    assert!(
        session
            .mods_named("UtilityConsecratedGroundDuration")
            .is_empty()
    );
    assert!(
        session
            .mods_named("UtilityConsecratedGroundRadiusMetres")
            .is_empty()
    );
    assert!(!session.has_flag("Condition:OnConsecratedGround"));
}

#[test]
fn consecrated_ground_minions_require_their_own_explicit_state() {
    let data = data();
    for player_ground in [false, true] {
        for minion_ground in [false, true] {
            let xml = format!(
                r#"<PathOfBuilding2>
<Build level="90" className="Witch" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="SummonSkeletalArsonistsPlayer" level="20" enabled="true"/></Skill></Skills>
<Config><Input name="conditionOnConsecratedGround" boolean="{player_ground}"/>
<Input name="conditionMinionsOnConsecratedGround" boolean="{minion_ground}"/></Config>
</PathOfBuilding2>"#
            );
            let build = parse_build(&xml).unwrap();
            let session =
                pobr_build::calculate_with_data_session(&build, &data, &Default::default())
                    .unwrap();
            let output = session.output();
            assert!(
                (output.life_regen
                    - if player_ground {
                        output.life * 0.05
                    } else {
                        0.0
                    })
                .abs()
                    < 1e-8
            );
            let minion = &output.minions[0];
            assert!(
                (minion.stats["LifeRegen"]
                    - if minion_ground {
                        minion.life * 0.05
                    } else {
                        0.0
                    })
                .abs()
                    < 1e-8,
                "player={player_ground}, minion={minion_ground}: {minion:?}"
            );
            assert_eq!(
                minion.stats["CurseEffectOnSelf"],
                if minion_ground { 50.0 } else { 100.0 }
            );
            if minion_ground {
                for stat in ["LifeRegen", "CurseEffectOnSelf"] {
                    let detail =
                        pobr_build::breakdown::detail::minion_stat_detail(&session, 0, stat)
                            .unwrap();
                    assert!(!detail.breakdown.mods.is_empty());
                }
            }
        }
    }
}

#[test]
fn consecrated_ground_unchecked_config_overrides_imported_state() {
    use pobr_core::rules::config_interpreter::ConfigInputValue;
    let data = data();
    let mut build = parse_build(
        r#"<PathOfBuilding2><Build level="80" className="Warrior"/>
<Config><Input name="conditionOnConsecratedGround" boolean="true"/></Config></PathOfBuilding2>"#,
    )
    .unwrap();
    assert_eq!(
        build.config.conditions.get("OnConsecratedGround"),
        Some(&true)
    );
    build.config.raw_inputs.values.insert(
        "conditionOnConsecratedGround".into(),
        ConfigInputValue::Bool(false),
    );
    let output = calculate_with_data(&build, &data, &Default::default()).unwrap();
    assert_eq!(output.life_regen, 0.0);
    assert_eq!(output.curse_effect_on_self, 100.0);
}

mod beira_ignited_ground {
    use super::*;
    use pobr_core::calc::{CalculationSession, MinimalInput};
    use pobr_core::{CalcConfig, Modifier, display_catalog, display_value};
    use pobr_data::{catalog::UtilityBaseStats, prelude::*};

    const LINE: &str = "Creates Ignited Ground for 4 seconds when used, Igniting enemies as though dealing Fire damage equal to 500% of your maximum Life";

    fn setup(data: &BuildData, cfg: CalcConfig, present: bool, active: bool) -> CalculationSession {
        let mut cfg = cfg;
        cfg.track_reads();
        let mut session = CalculationSession::new(MinimalInput {
            base_life: 1000.0,
            base_hit_min: 10.0,
            base_hit_max: 10.0,
            base_action_rate: 1.0,
            ..Default::default()
        })
        .with_config(cfg);
        session.set_parser_rules(data.parser_rules.as_ref().unwrap().clone());
        let item = Item {
            base: "Dousing Charm".into(),
            quality: 20,
            rarity: ItemRarity::Unique,
            corrupted: false,
            modifier_texts: if present { vec![LINE.into()] } else { vec![] },
            implicit_texts: vec![],
            enchant_texts: vec![],
            rolled_defence: RolledDefence::default(),
            socketed: vec![],
            parsed_stats: vec![],
        };
        for slot in ["Charm 1", "Charm 2"] {
            session.add_utility_base(
                slot,
                &item,
                &UtilityBaseStats {
                    life: 0.0,
                    mana: 0.0,
                    duration: 3.0,
                    charges_max: 80.0,
                    charges_used: 60.0,
                },
            );
            if active {
                session.add_flask_charm(slot, &item);
            } else {
                session.add_inactive_utility_properties(slot, &item);
            }
        }
        session
    }

    #[test]
    fn snapshot_is_independent_of_charm_activation_and_requires_established_ignite() {
        let data = data();
        let mods: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(repo_data_root().join(data_version()).join("base/mods.json"))
                .unwrap(),
        )
        .unwrap();
        let source = mods
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["id"] == "UniqueCharmIgniteEnemiesInPresence1")
            .unwrap();
        let stat = source["stats"].as_array().unwrap().iter().find(|s|
            s["stat_id"] == "charm_ignite_ground_as_though_dealing_fire_damage_equal_to_x%_of_your_maximum_life_when_used"
        ).unwrap();
        assert_eq!(stat["min"], 500);
        assert_eq!(stat["max"], 500);
        for present in [false, true] {
            for active in [false, true] {
                for combat in [false, true] {
                    for established in [false, true] {
                        let mut s = setup(
                            &data,
                            CalcConfig::new()
                                .with_mode_combat(combat)
                                .with_condition("BeiraIgniteActive", established),
                            present,
                            active,
                        );
                        s.add_modifiers([
                            Modifier::number("MaximumLife", ModType::Inc, 50.0),
                            Modifier::number("LifeReserved", ModType::Base, 500.0),
                            Modifier::number("CharmEffect", ModType::Inc, 100.0),
                            Modifier::number("CharmDuration", ModType::Inc, 100.0),
                        ]);
                        s.perform_minimal().unwrap();
                        assert!(s.unsupported_modifier_texts().is_empty());
                        assert_eq!(s.output().life, 1500.0);
                        assert!(s.output().life_unreserved < s.output().life);
                        assert!(
                            s.config()
                                .read_variables()
                                .conditions
                                .contains("BeiraIgniteActive")
                        );
                        let expected = if present { 1500.0 } else { 0.0 };
                        for slot in [1, 2] {
                            for (field, value) in [
                                ("IgnitedGroundDuration", 4.0),
                                ("IgnitedGroundRadiusMetres", 2.0),
                                ("IgnitedGroundSourceDamage", 7500.0),
                                ("IgnitedGroundIgniteDPS", 1500.0),
                                ("IgnitedGroundIgniteDuration", 4.0),
                            ] {
                                let id = format!("Charm{slot}{field}");
                                assert_eq!(
                                    display_value(s.output(), &id),
                                    if present { value } else { 0.0 },
                                    "{id}"
                                );
                                assert!(s.derived_detail(&id).is_some());
                                assert!(
                                    display_catalog()
                                        .iter()
                                        .find(|d| d.id.as_str() == id)
                                        .unwrap()
                                        .pob_key
                                        .is_none()
                                );
                                assert_eq!(
                                    pobr_build::breakdown::detail::detail_kind(&id),
                                    pobr_build::breakdown::detail::DetailKind::Derived
                                );
                            }
                        }
                        let current = if established && combat { expected } else { 0.0 };
                        assert_eq!(display_value(s.output(), "BeiraIgniteDPS"), current);
                        assert_eq!(display_value(s.output(), "CombinedIgniteDPS"), current);
                        assert_eq!(s.output().combined_dps, s.output().dps + current);
                        // Local scope must not rewrite the selected skill's enemy state.
                        assert!(!s.config().condition("EnemyIgnited"));
                    }
                }
            }
        }
    }

    #[test]
    fn absent_ground_source_does_not_consume_global_ignite_inputs() {
        let data = data();
        for present in [false, true] {
            let mut s = setup(
                &data,
                CalcConfig::new().with_mode_combat(true),
                present,
                false,
            );
            s.add_modifiers([Modifier::number("IgniteFaster", ModType::Inc, 25.0)]);
            s.track_modifier_reads();
            s.perform_minimal().unwrap();
            assert_eq!(
                s.mod_db()
                    .read_names()
                    .contains(&ModName::from("IgniteFaster")),
                present
            );
            assert_eq!(
                display_value(s.output(), "Charm1IgnitedGroundIgniteDPS"),
                if present { 1250.0 } else { 0.0 }
            );
            assert_eq!(display_value(s.output(), "BeiraIgniteDPS"), 0.0);
        }
    }

    #[test]
    fn non_hit_scope_uses_global_ignite_scaling_and_enemy_mitigation_only() {
        let data = data();
        let cfg = CalcConfig::new()
            .with_mode_combat(true)
            .with_mode_effective(true)
            .with_condition("BeiraIgniteActive", true)
            .with_condition("CriticalStrike", true);
        let mut s = setup(&data, cfg.clone(), true, true);
        s.add_modifiers([
            Modifier::number("AilmentMagnitude", ModType::Inc, 50.0),
            Modifier::number("IgniteFaster", ModType::Inc, 25.0),
            Modifier::number("IgniteDuration", ModType::Inc, 50.0),
            Modifier::number("FirePenetration", ModType::Base, 100.0),
            Modifier::number("Damage", ModType::Inc, 900.0),
            Modifier::number("AilmentMagnitude", ModType::More, 900.0).with_flags(ModFlags::ATTACK),
            Modifier::number("AilmentMagnitude", ModType::More, 900.0).with_flags(ModFlags::SPELL),
            Modifier::number("AilmentMagnitude", ModType::More, 900.0)
                .with_tag(pobr_core::ModTag::condition("CriticalStrike", false)),
            Modifier::number("AilmentMagnitude", ModType::More, 900.0).with_origin(
                ModifierSource::new(SourceId::new(SourceKind::SupportGem, "test.support")),
            ),
            Modifier::flag("CannotIgnite").with_origin(ModifierSource::new(SourceId::new(
                SourceKind::SkillGem,
                "test.skill",
            ))),
        ]);
        s.add_enemy_modifiers([
            Modifier::number("FireResist", ModType::Base, 50.0),
            Modifier::number("FireDamageTaken", ModType::Inc, 20.0),
        ]);
        s.perform_minimal().unwrap();
        // 1000 Life * 500% * 20% Ignite * 1.5 magnitude * 1.25 rate * .5 resist * 1.2 taken.
        assert_eq!(display_value(s.output(), "BeiraIgniteDPS"), 1125.0);
        assert_eq!(
            display_value(s.output(), "Charm1IgnitedGroundIgniteDuration"),
            4.8
        );
        let detail = s.derived_detail("Charm1IgnitedGroundIgniteDPS").unwrap();
        let input = |name: &str| detail.inputs.iter().find(|(key, _)| key == name).unwrap().1;
        let expected = input("IgnitedGroundSourceDamage")
            * input("IgniteBaseFraction")
            * (1.0 + input("IgniteMagnitudeIncreased") / 100.0)
            * input("IgniteMagnitudeMore")
            * input("EnemyIgniteMultiplier")
            * input("IgniteEffect")
            * input("IgniteRate");
        assert!((expected - 1125.0).abs() < 1e-9);
        for block in ["CannotIgnite", "DealNoDamage", "DealNoFire"] {
            let mut blocked = setup(&data, cfg.clone(), true, true);
            blocked.add_modifiers([Modifier::flag(block)]);
            blocked.perform_minimal().unwrap();
            assert_eq!(
                display_value(blocked.output(), "BeiraIgniteDPS"),
                0.0,
                "{block}"
            );
        }
        let mut stopped = setup(&data, cfg, true, true);
        stopped.add_modifiers([Modifier::number("IgniteFaster", ModType::Inc, -100.0)]);
        stopped.perform_minimal().unwrap();
        assert_eq!(display_value(stopped.output(), "BeiraIgniteDPS"), 0.0);
        assert_eq!(
            display_value(stopped.output(), "Charm1IgnitedGroundIgniteDuration"),
            0.0
        );
        assert_eq!(
            stopped
                .derived_detail("Charm1IgnitedGroundIgniteDuration")
                .unwrap()
                .formula,
            "0 * IgniteBaseDuration"
        );
    }

    #[test]
    fn strongest_ignite_wins_and_reference_basis_excludes_ground_source() {
        let data = data();
        for fire in [100.0, 20000.0] {
            let cfg = CalcConfig::new()
                .with_mode_combat(true)
                .with_damage_type(DamageType::Fire)
                .with_condition("BeiraIgniteActive", true);
            let mut s = setup(&data, cfg, true, true);
            s.add_modifiers([
                Modifier::number("FireDamageMin", ModType::Base, fire),
                Modifier::number("FireDamageMax", ModType::Base, fire),
                Modifier::number("IgniteChance", ModType::Base, 100.0),
            ]);
            s.perform_minimal().unwrap();
            let main = s.output().ignite_stacked_dps.max(s.output().ignite_dps);
            assert!(main > 0.0);
            if fire == 100.0 {
                assert!(main < 1000.0);
            } else {
                assert!(main > 1000.0);
            }
            assert_eq!(
                display_value(s.output(), "CombinedIgniteDPS"),
                main.max(1000.0)
            );
            assert_eq!(s.output().combined_dps, s.output().dps + main.max(1000.0));
        }
        let mut stacked = setup(
            &data,
            CalcConfig::new()
                .with_mode_combat(true)
                .with_condition("BeiraIgniteActive", true),
            true,
            true,
        );
        stacked.add_modifiers([
            Modifier::number("FireDamageMin", ModType::Base, 100.0),
            Modifier::number("FireDamageMax", ModType::Base, 100.0),
            Modifier::number("IgniteChance", ModType::Base, 100.0),
            Modifier::flag("IgniteCanStack"),
            Modifier::number("IgniteStacks", ModType::Base, 1.0),
        ]);
        stacked.perform_minimal().unwrap();
        assert_eq!(stacked.output().ignite_max_stacks, 2.0);
        assert_eq!(display_value(stacked.output(), "BeiraIgniteDPS"), 1000.0);
        assert_eq!(
            display_value(stacked.output(), "CombinedIgniteDPS"),
            stacked
                .output()
                .ignite_stacked_dps
                .max(stacked.output().ignite_dps)
        );
        let cfg = CalcConfig::new()
            .with_mode_combat(true)
            .with_condition("BeiraIgniteActive", true);
        let mut s = setup(&data, cfg, false, false);
        s.set_exporter_rules(false, Default::default());
        s.add_modifier_texts(&[LINE]).unwrap();
        assert!(
            s.exporter_rules()
                .any(|r| r.id == "beira_ignited_ground_on_use")
        );
        assert!(s.mods_named("UtilityIgnitedGroundDuration").is_empty());
        assert!(s.mods_named("UtilityIgnitedGroundLifePercent").is_empty());
        s.perform_minimal().unwrap();
        assert_eq!(display_value(s.output(), "BeiraIgniteDPS"), 0.0);
    }

    #[test]
    fn xml_json_and_native_flag_agree_and_explicit_false_retires_import() {
        use pobr_core::rules::config_interpreter::ConfigInputValue;
        let data = data();
        let xml = format!(
            r#"<PathOfBuilding2><Build level="80" className="Warrior"/>
<Items activeItemSet="1"><Item id="1">Rarity: UNIQUE
Beira's Anguish
Dousing Charm
Implicits: 0
{LINE}
</Item><ItemSet id="1"><Slot name="Charm 1" itemId="1" active="false"/></ItemSet></Items>
<Config><Input name="conditionBeiraIgniteActive" boolean="true"/></Config></PathOfBuilding2>"#
        );
        let captured = parse_build(
            &xml.replace("4 seconds", "6 seconds")
                .replace("500%", "250%"),
        )
        .unwrap();
        let captured_out = calculate_with_data(&captured, &data, &Default::default()).unwrap();
        assert_eq!(
            display_value(&captured_out, "Charm1IgnitedGroundDuration"),
            6.0
        );
        assert_eq!(
            display_value(&captured_out, "BeiraIgniteDPS"),
            captured_out.life * 0.5
        );
        let mut build = parse_build(&xml).unwrap();
        assert_eq!(
            build.config.conditions.get("BeiraIgniteActive"),
            Some(&true)
        );
        let out = calculate_with_data(&build, &data, &Default::default()).unwrap();
        assert_eq!(display_value(&out, "BeiraIgniteDPS"), out.life);
        // Optional Full DPS must count the charm once, independent of the
        // number of included skills and their simultaneous-source counts.
        let mut full_build = build.clone();
        for (skill, count) in [("SparkPlayer", 3.0), ("FrostboltPlayer", 5.0)] {
            let mut group = pobr_build::SocketGroup::new().with_gem_skill(skill, 20);
            group.include_in_full_dps = true;
            group.group_count = Some(count);
            full_build.socket_groups.push(group);
        }
        let full = pobr_build::calc_orchestrator::full_dps::calculate(
            &full_build,
            &data,
            &Default::default(),
        )
        .unwrap();
        assert_eq!(full.dot_dps, out.life);
        assert_eq!(
            full.skills.iter().map(|row| row.dot_dps).sum::<f64>(),
            out.life
        );
        build.config.raw_inputs.values.insert(
            "conditionBeiraIgniteActive".into(),
            ConfigInputValue::Bool(false),
        );
        let off = calculate_with_data(&build, &data, &Default::default()).unwrap();
        assert_eq!(display_value(&off, "BeiraIgniteDPS"), 0.0);
        for condition in [None, Some(false)] {
            let mut cfg = CalcConfig::new().with_mode_combat(true);
            if let Some(value) = condition {
                cfg = cfg.with_condition("BeiraIgniteActive", value);
            }
            let mut s = setup(&data, cfg, true, false);
            s.add_modifiers([Modifier::flag("Condition:BeiraIgniteActive")]);
            s.perform_minimal().unwrap();
            assert_eq!(
                display_value(s.output(), "BeiraIgniteDPS"),
                if condition.is_none() { 1000.0 } else { 0.0 }
            );
        }
    }
}
