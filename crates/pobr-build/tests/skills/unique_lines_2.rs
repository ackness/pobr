//! Named gems, Parry, and increases shared by unique item text.
use pobr_build::breakdown::stat_breakdown;
use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data_session, parse_build};
use pobr_core::calc::CalculationSession;
use pobr_data::prelude::DamageType;

use crate::test_data as data;

fn calc(data: &BuildData, skill: &str, level: u32, extra: &[&str]) -> CalculationSession {
    let xml = format!(
        r#"<PathOfBuilding2><Build level="80" className="Sorceress" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="{skill}" level="{level}" enabled="true"/></Skill></Skills>
<Items activeItemSet="1"><Item id="1">Rarity: RARE
Test Mace
Wooden Club
Physical Damage: 100-100
Attacks per Second: 1.00
Implicits: 0
</Item><Item id="2">Rarity: NORMAL
Leather Buckler
Evasion Rating: 100
Implicits: 0
</Item><ItemSet id="1"><Slot name="Weapon 1" itemId="1"/><Slot name="Weapon 2" itemId="2"/></ItemSet></Items>
</PathOfBuilding2>"#
    );
    calculate_with_data_session(
        &parse_build(&xml).unwrap(),
        data,
        &DataOrchestratorOptions {
            mode_effective: false,
            extra_modifier_texts: extra
                .iter()
                .copied()
                .chain(["Never deal Critical Hits", "Hits can't be Evaded"])
                .map(String::from)
                .collect(),
            ..Default::default()
        },
    )
    .unwrap()
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
}

#[test]
fn cursecarver_named_levels_reach_each_curses_cost() {
    let data = data();
    for (skill, name) in [
        ("DespairPlayer", "Despair"),
        ("ElementalWeaknessPlayer", "Elemental Weakness"),
        ("EnfeeblePlayer", "Enfeeble"),
        ("TemporalChainsPlayer", "Temporal Chains"),
        ("VulnerabilityPlayer", "Vulnerability"),
    ] {
        let line = format!("+4 to Level of {name} Skills");
        let plain = calc(&data, skill, 10, &[]);
        let boosted = calc(&data, skill, 10, &[&line]);
        let level14 = calc(&data, skill, 14, &[]);
        assert!(
            boosted.output().mana_cost > plain.output().mana_cost,
            "{name}"
        );
        close(boosted.output().mana_cost, level14.output().mana_cost);
        // A named curse does not raise the Fireball gem.
        close(
            calc(&data, "FireballPlayer", 10, &[&line]).output().dps,
            calc(&data, "FireballPlayer", 10, &[]).output().dps,
        );
        assert!(
            !boosted
                .unsupported_modifier_texts()
                .iter()
                .any(|s| s == &line)
        );
    }
}

#[test]
fn named_levels_match_the_catalog_display_name_including_punctuation() {
    let data = data();
    let line = "+4 to Level of Sniper's Mark Skills";
    let boosted = calc(&data, "SnipersMarkPlayer", 10, &[line]);
    let plain = calc(&data, "SnipersMarkPlayer", 10, &[]);
    let level14 = calc(&data, "SnipersMarkPlayer", 14, &[]);
    assert!(boosted.output().mana_cost > plain.output().mana_cost);
    close(boosted.output().mana_cost, level14.output().mana_cost);
    close(
        calc(
            &data,
            "SnipersMarkPlayer",
            10,
            &["+4 to Level of all Sniper's Mark Skills"],
        )
        .output()
        .mana_cost,
        level14.output().mana_cost,
    );
}

#[test]
fn parry_damage_and_conversion_are_scoped_to_parry() {
    let data = data();
    let damage = "100% increased Parry Damage";
    let conversion = "100% of Parry Physical Damage Converted to Cold Damage";
    let plain = calc(&data, "ParryPlayer", 10, &[]);
    let boosted = calc(&data, "ParryPlayer", 10, &[damage]);
    assert!(plain.output().dps > 0.0);
    close(
        boosted.output().total_hit_avg,
        2.0 * plain.output().total_hit_avg,
    );
    let converted = calc(&data, "ParryPlayer", 10, &[conversion]);
    let by_type = |s: &CalculationSession, kind| {
        s.output()
            .damage_components
            .iter()
            .filter(|c| c.damage_type == kind)
            .map(|c| c.avg())
            .sum::<f64>()
    };
    close(by_type(&converted, DamageType::Physical), 0.0);
    close(
        by_type(&converted, DamageType::Cold),
        by_type(&plain, DamageType::Physical),
    );
    for skill in ["ArmourBreakerPlayer", "FireballPlayer"] {
        close(
            calc(&data, skill, 10, &[damage, conversion])
                .output()
                .total_hit_avg,
            calc(&data, skill, 10, &[]).output().total_hit_avg,
        );
    }
}

#[test]
fn minion_damage_increases_and_reductions_also_reach_player_hits() {
    let data = data();
    let transfer = "Increases and Reductions to Minion Damage also affect you";
    let modifiers = [
        "Minions deal 40% increased Damage",
        "Minions deal 10% reduced Damage",
    ];
    let plain = calc(&data, "FireballPlayer", 10, &[]);
    let minion_only = calc(&data, "FireballPlayer", 10, &modifiers);
    close(minion_only.output().dps, plain.output().dps);
    let shared = calc(
        &data,
        "FireballPlayer",
        10,
        &[transfer, modifiers[0], modifiers[1]],
    );
    close(shared.output().dps, plain.output().dps * 1.3);
    // More damage and minion life are different modifiers.
    close(
        calc(
            &data,
            "FireballPlayer",
            10,
            &[
                transfer,
                "Minions deal 100% more Damage",
                "Minions have 100% increased maximum Life",
            ],
        )
        .output()
        .dps,
        plain.output().dps,
    );
    assert!(
        shared
            .mods_named("Damage")
            .iter()
            .any(|m| m.source.as_deref() == Some(modifiers[0]))
    );
}

#[test]
fn minion_attack_speed_transfer_keeps_cast_speed_and_more_separate() {
    let data = data();
    let transfer = "Increases and Reductions to Minion Attack Speed also affect you";
    let inc = "Minions have 40% increased Attack Speed";
    let reduced = "Minions have 10% reduced Attack Speed";
    let plain = calc(&data, "ArmourBreakerPlayer", 10, &[]);
    close(
        calc(&data, "ArmourBreakerPlayer", 10, &[inc])
            .output()
            .action_rate,
        plain.output().action_rate,
    );
    let shared = calc(&data, "ArmourBreakerPlayer", 10, &[transfer, inc, reduced]);
    close(
        shared.output().action_rate,
        plain.output().action_rate * 1.3,
    );
    close(
        calc(&data, "FireballPlayer", 10, &[transfer, inc])
            .output()
            .action_rate,
        calc(&data, "FireballPlayer", 10, &[]).output().action_rate,
    );
    close(
        calc(
            &data,
            "ArmourBreakerPlayer",
            10,
            &[
                transfer,
                "Minions have 100% more Attack Speed",
                "Minions have 100% increased Cast Speed",
            ],
        )
        .output()
        .action_rate,
        plain.output().action_rate,
    );
    assert!(
        stat_breakdown(&shared, "ActionRate")
            .unwrap()
            .mods
            .iter()
            .any(|m| m.active && m.source_text.as_deref() == Some(inc))
    );
}

#[test]
fn spell_damage_transfer_copies_only_increases_to_attacks_once() {
    let data = data();
    let transfer = "Increases and Reductions to Spell damage also apply to Attacks";
    let inc = "40% increased Spell Damage";
    let reduced = "10% reduced Spell Damage";
    let plain = calc(&data, "ArmourBreakerPlayer", 10, &[]);
    close(
        calc(&data, "ArmourBreakerPlayer", 10, &[inc]).output().dps,
        plain.output().dps,
    );
    close(
        calc(&data, "ArmourBreakerPlayer", 10, &[transfer, inc, reduced])
            .output()
            .dps,
        plain.output().dps * 1.3,
    );
    // A spell already uses SpellDamage: the transfer must not count it twice.
    close(
        calc(&data, "FireballPlayer", 10, &[transfer, inc])
            .output()
            .dps,
        calc(&data, "FireballPlayer", 10, &[inc]).output().dps,
    );
    close(
        calc(
            &data,
            "ArmourBreakerPlayer",
            10,
            &[
                transfer,
                "100% more Spell Damage",
                "Adds 100 to 100 Fire Damage to Spells",
            ],
        )
        .output()
        .dps,
        plain.output().dps,
    );
}
