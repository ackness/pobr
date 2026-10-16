//! Secondary hits retain damage independently of an unmodelled event rate.

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};
use pobr_core::calc::OutputTable;
use pobr_gamedata::{GameData, current_data_dir};

fn data() -> BuildData {
    BuildData::load(&GameData::new(current_data_dir())).unwrap()
}

fn calculate(data: &BuildData, primary: &str, extra: &[&str]) -> OutputTable {
    let xml = format!(
        r#"<PathOfBuilding2>
<Build level="90" className="Witch" mainSocketGroup="1"/>
<Skills><Skill enabled="true" mainActiveSkill="2"><Gem skillId="{primary}" level="20" quality="0" enabled="true"/></Skill></Skills>
<Items activeItemSet="1"><Item id="1">Rarity: NORMAL
Wooden Club
Implicits: 0
</Item><Item id="2">Rarity: NORMAL
Splintered Tower Shield
Armour: 30
Implicits: 0
</Item><ItemSet id="1"><Slot name="Weapon 1" itemId="1"/><Slot name="Weapon 2" itemId="2"/></ItemSet></Items>
</PathOfBuilding2>"#
    );
    calculate_with_data(
        &parse_build(&xml).unwrap(),
        data,
        &DataOrchestratorOptions {
            extra_modifier_texts: extra.iter().map(|line| (*line).into()).collect(),
            ..Default::default()
        },
    )
    .unwrap()
}

fn noncrit_hit(output: &OutputTable) -> f64 {
    output.damage_components.iter().map(|part| part.avg()).sum()
}

#[test]
fn blood_explosion_uses_its_own_base_damage_and_checks_accuracy() {
    let data = data();
    let output = calculate(&data, "BloodhoundsMarkPlayer", &[]);
    // GrantedEffectStatSets level 20: 196–294 physical, 408% damage.
    // ActiveSkills calls this Physical Attack damage, independent of weapons;
    // its implicit stats do not grant global_always_hit. Crit base is 5%.
    assert!((noncrit_hit(&output) - (196.0 + 294.0) / 2.0 * 4.08).abs() < 1e-6);
    // Exercise the sweep's boss view so the accuracy distinction cannot
    // silently disappear.
    let mut build = pobr_build::Build::new()
        .with_character(pobr_build::CharacterIdentity {
            level: 90,
            class_name: "Witch".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(
            pobr_build::SocketGroup::new().with_gem_skill("BloodhoundsMarkPlayer", 20),
        );
    build.socket_groups[0].main_active_skill = Some(2);
    let effective = calculate_with_data(
        &build,
        &data,
        &DataOrchestratorOptions::against_reference_enemy(),
    )
    .unwrap();
    assert!(effective.hit_chance > 0.0 && effective.hit_chance < 1.0);
    assert!((effective.crit_chance - 0.05 * effective.hit_chance).abs() < 1e-6);
}

#[test]
fn curse_and_shield_shockwaves_have_hits_without_an_event_rate() {
    let data = data();
    for (skill, expected) in [
        // Level 20: 128–192 physical, 204% damage.
        ("CurseOfRepulsionPlayer", (128.0 + 192.0) / 2.0 * 2.04),
        // Level 20: 85–128 plus 5–7 per complete 15 shield Armour.
        (
            "FortifyingCryPlayer",
            (85.0 + 128.0 + 2.0 * (5.0 + 7.0)) / 2.0 * 2.04,
        ),
    ] {
        let output = calculate(&data, skill, &["100% increased Attack Speed"]);
        assert!(
            (noncrit_hit(&output) - expected).abs() < 1e-6,
            "{skill}: {output:?}"
        );
        assert!(output.average_damage > 0.0, "{skill}");
        assert_eq!(output.action_rate, 0.0, "{skill}");
        assert_eq!(output.effective_action_rate, 0.0, "{skill}");
        assert_eq!(output.dps, 0.0, "{skill}");
    }
}

#[test]
fn combust_and_caltrops_do_not_use_weapon_speed_as_event_frequency() {
    let data = data();
    for (skill, expected) in [
        // Level 20 Combust: 362% weapon damage, all converted to fire.
        ("TemperWeaponPlayer", (6.0 + 10.0) / 2.0 * 3.62),
        // Level 20 Caltrops: 159–238 physical, 143% damage, plus the club.
        (
            "TrailOfCaltropsPlayer",
            (159.0 + 238.0 + 6.0 + 10.0) / 2.0 * 1.43,
        ),
    ] {
        let output = calculate(&data, skill, &["100% increased Attack Speed"]);
        assert!(
            (noncrit_hit(&output) - expected).abs() < 1e-6,
            "{skill}: {output:?}"
        );
        assert!(output.average_damage > 0.0, "{skill}");
        assert_eq!(output.action_rate, 0.0, "{skill}");
        assert_eq!(output.effective_action_rate, 0.0, "{skill}");
        assert_eq!(output.dps, 0.0, "{skill}");
        assert_eq!(output.combined_dps, 0.0, "{skill}");
    }
}
