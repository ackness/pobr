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
