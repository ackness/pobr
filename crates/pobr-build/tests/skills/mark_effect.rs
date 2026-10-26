//! Game stat mark_effect_+% scales Mark skills, including their self buffs.

use pobr_build::{Build, DataOrchestratorOptions, SocketGroup, calculate_with_data};
use pobr_core::calc::OutputTable;
use pobr_data::prelude::DamageType;

fn calculate(mark: &str, effect: u32, duplicate: bool) -> OutputTable {
    let mut build = Build::new()
        .add_socket_group(SocketGroup::new().with_gem_skill("FireballPlayer", 20))
        .add_socket_group(SocketGroup::new().with_gem_skill(mark, 20))
        .with_main_socket_group(1);
    if duplicate {
        build = build.add_socket_group(SocketGroup::new().with_gem_skill(mark, 20));
    }
    if effect > 0 {
        build
            .config
            .global_modifier_texts
            .push(format!("{effect}% increased Effect of your Mark Skills"));
    }
    calculate_with_data(
        &build,
        crate::test_data(),
        &DataOrchestratorOptions::against_reference_enemy(),
    )
    .unwrap()
}

#[test]
fn mark_effect_scales_mark_benefits_without_scaling_other_skills() {
    for (mark, damage_type) in [
        ("FreezingMarkPlayer", DamageType::Cold),
        ("VoltaicMarkPlayer", DamageType::Lightning),
    ] {
        let plain = calculate(mark, 0, false);
        let enhanced = calculate(mark, 100, false);
        let damage = |output: &OutputTable, wanted| {
            output
                .damage_components
                .iter()
                .find(|part| part.damage_type == wanted)
                .unwrap()
                .avg()
        };
        assert!(damage(&plain, damage_type) > 0.0);
        assert_eq!(
            damage(&enhanced, damage_type),
            2.0 * damage(&plain, damage_type)
        );
        assert_eq!(
            damage(&enhanced, DamageType::Fire),
            damage(&plain, DamageType::Fire)
        );
        assert_eq!(
            calculate(mark, 100, true).damage_components,
            enhanced.damage_components
        );
        // Buff magnitudes use the existing integer truncation convention.
        for (effect, gain) in [(12, 33.0), (21, 36.0)] {
            let actual = damage(&calculate(mark, effect, false), damage_type);
            assert!((actual / damage(&plain, damage_type) - gain / 30.0).abs() < 1e-9);
        }
    }
    let plain = calculate("SnipersMarkPlayer", 0, false);
    let enhanced = calculate("SnipersMarkPlayer", 100, false);
    assert!(enhanced.total_hit_avg > plain.total_hit_avg);
    // Ordinary curses are unaffected by a bonus explicitly scoped to Marks.
    assert_eq!(
        calculate("FlammabilityPlayer", 100, false).damage_components,
        calculate("FlammabilityPlayer", 0, false).damage_components,
    );
}

#[test]
fn mark_effect_survives_party_export_and_import() {
    let xml = r#"<PathOfBuilding2><Build level="80" className="Witch"/>
<Skills><Skill enabled="true"><Gem skillId="SnipersMarkPlayer" level="20" enabled="true"/></Skill></Skills>
<Config><CustomModifierBlock title="Test" enabled="true">100% increased Effect of your Mark Skills</CustomModifierBlock></Config>
</PathOfBuilding2>"#;
    let member = pobr_build::party::import_member(
        &pobr_build::encode_pob_code(xml).unwrap(),
        crate::test_data(),
    )
    .unwrap();
    let mut receiver = Build::new()
        .add_socket_group(SocketGroup::new().with_gem_skill("FireballPlayer", 20))
        .with_main_socket_group(1);
    receiver.party.members.push(member);
    let shared = calculate_with_data(
        &receiver,
        crate::test_data(),
        &DataOrchestratorOptions::against_reference_enemy(),
    )
    .unwrap();
    assert_eq!(
        shared.total_hit_avg,
        calculate("SnipersMarkPlayer", 100, false).total_hit_avg
    );
    assert!(shared.total_hit_avg > calculate("SnipersMarkPlayer", 0, false).total_hit_avg);
}
