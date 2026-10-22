//! Conditional offence uses the selected skill and explicit enemy state.
//! Sources: game stats damage_+%_vs_frozen_enemies,
//! bleeding_magnitude_+%_against_pinned_enemies, triggered_spell_spell_damage_+%,
//! and Coming Calamity's three 40% Herald bonuses (passive 28044).

use pobr_build::{Build, BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};
use pobr_core::calc::OutputTable;

use crate::test_data as data;

fn gem(id: &str) -> String {
    format!(r#"<Gem skillId="{id}" level="20" quality="0" enabled="true"/>"#)
}

fn build(skill: &str, other_groups: &str, config: &str) -> Build {
    parse_build(&format!(
        r#"<PathOfBuilding2>
<Build level="80" className="Sorceress" ascendClassName="None" mainSocketGroup="1"/>
<Skills><Skill enabled="true" mainActiveSkill="1">{}</Skill>{other_groups}</Skills>
<Config>{config}</Config>
</PathOfBuilding2>"#,
        gem(skill)
    ))
    .unwrap()
}

fn calc(build: &Build, data: &BuildData, lines: &[&str]) -> OutputTable {
    calculate_with_data(
        build,
        data,
        &DataOrchestratorOptions {
            mode_effective: true,
            extra_modifier_texts: lines.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        },
    )
    .unwrap()
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
}

#[test]
fn frozen_hit_bonus_requires_the_configured_enemy_state() {
    let data = data();
    let line = "50% increased Damage with Hits against Frozen Enemies";
    for frozen in [false, true] {
        let build = build(
            "FireballPlayer",
            "",
            &format!(r#"<Input name="conditionEnemyFrozen" boolean="{frozen}"/>"#),
        );
        let plain = calc(&build, &data, &[]);
        let bonus = calc(&build, &data, &[line]);
        assert!(plain.total_hit_avg > 0.0);
        close(
            bonus.total_hit_avg,
            plain.total_hit_avg * if frozen { 1.5 } else { 1.0 },
        );
    }
}

#[test]
fn herald_bonuses_require_the_matching_enabled_herald() {
    let data = data();
    for (skill, herald, line) in [
        (
            "FireballPlayer",
            "HeraldOfAshPlayer",
            "40% increased Fire Damage while affected by Herald of Ash",
        ),
        (
            "ArcPlayer",
            "HeraldOfThunderPlayer",
            "40% increased Lightning Damage while affected by Herald of Thunder",
        ),
    ] {
        for enabled in [false, true] {
            let groups = format!(r#"<Skill enabled="{enabled}">{}</Skill>"#, gem(herald));
            let build = build(skill, &groups, "");
            let plain = calc(&build, &data, &[]);
            let bonus = calc(&build, &data, &[line]);
            close(
                bonus.total_hit_avg,
                plain.total_hit_avg * if enabled { 1.4 } else { 1.0 },
            );
        }
    }
}

#[test]
fn triggered_spell_bonus_uses_the_selected_payload_type() {
    let data = data();
    let plain = build("CometPlayer", "", "");
    let mut triggered = plain.clone();
    triggered.socket_groups[0] = pobr_build::SocketGroup::new()
        .with_gem_skill("MetaCastOnCritPlayer", 20)
        .with_gem_skill("CometPlayer", 20)
        .with_main_active_skill(2);
    for (build, factor) in [(&plain, 1.0), (&triggered, 1.4)] {
        let base = calc(build, &data, &[]);
        let bonus = calc(
            build,
            &data,
            &["Triggered Spells deal 40% increased Spell Damage"],
        );
        assert!(base.total_hit_avg > 0.0);
        close(bonus.total_hit_avg, base.total_hit_avg * factor);
    }
}

#[test]
fn pinned_bleed_bonus_uses_config_without_assuming_pin_from_capability() {
    let data = data();
    let line = "40% increased Magnitude of Bleeding you inflict against Pinned Enemies";
    for pinned in [false, true] {
        let build = build(
            "ReapPlayer",
            "",
            &format!(r#"<Input name="conditionEnemyPinned" boolean="{pinned}"/>"#),
        );
        let plain = calc(&build, &data, &["100% chance to cause Bleeding on Hit"]);
        let bonus = calc(
            &build,
            &data,
            &["100% chance to cause Bleeding on Hit", line],
        );
        assert!(plain.bleed_dps > 0.0);
        close(
            bonus.bleed_dps,
            plain.bleed_dps * if pinned { 1.4 } else { 1.0 },
        );
        close(bonus.total_hit_avg, plain.total_hit_avg);
    }
}

#[test]
fn added_attack_damage_is_not_spell_damage_or_a_unique_condition() {
    let data = data();
    // Asphyxia's Wrath's Broadhead Quiver implicit is 1–3 Physical;
    // its Cold roll is 3–4 to 5–8. Use the low roll, 3–5 Cold.
    let lines = [
        "Adds 1 to 3 Physical Damage to Attacks",
        "Adds 3 to 5 Cold damage to Attacks",
    ];
    let spell = build("FireballPlayer", "", "");
    close(
        calc(&spell, &data, &lines).total_hit_avg,
        calc(&spell, &data, &[]).total_hit_avg,
    );
    let attack = build("MeleePlayer", "", "");
    let plain = calc(&attack, &data, &[]);
    let added = calc(&attack, &data, &lines);
    assert!(added.total_hit_avg > plain.total_hit_avg);
    let cold = added
        .damage_components
        .iter()
        .find(|c| c.damage_type == pobr_data::prelude::DamageType::Cold)
        .unwrap();
    // MeleePlayer's game level-20 damage multiplier is 3.12.
    close(cold.min, 3.0 * 3.12);
    close(cold.max, 5.0 * 3.12);
    let average = |out: &OutputTable| {
        out.damage_components
            .iter()
            .map(|c| (c.min + c.max) / 2.0)
            .sum::<f64>()
    };
    close(average(&added) - average(&plain), (2.0 + 4.0) * 3.12);
}
