//! Modifiers that apply while a Quarterstaff is wielded.
//!
//! The game words the test two ways: "with Quarterstaves" (attack speed,
//! critical hits, buildup) and "while wielding a Quarterstaff", as on the
//! passive "30% increased Armour, Evasion and Energy Shield while wielding a
//! Quarterstaff".

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};
use pobr_core::calc::OutputTable;
use pobr_gamedata::{GameData, data_version, repo_data_root};

const WHILE_WIELDING: &str =
    "30% increased Armour, Evasion and Energy Shield while wielding a Quarterstaff";

fn data() -> BuildData {
    BuildData::load(&GameData::new(repo_data_root().join(data_version())))
        .expect("load repository data")
}

/// A Monk with flat defences from elsewhere, wielding `weapon` (nothing when
/// empty), with `extra` modifiers.
fn calculate(data: &BuildData, weapon: &str, extra: &[&str]) -> OutputTable {
    let items = if weapon.is_empty() {
        String::new()
    } else {
        format!(
            r#"<Items activeItemSet="1"><Item id="1">Rarity: RARE
Test Weapon
{weapon}
Implicits: 0
</Item><ItemSet id="1"><Slot name="Weapon 1" itemId="1"/></ItemSet></Items>"#
        )
    };
    let xml = format!(
        r#"<PathOfBuilding2>
<Build level="80" className="Monk" ascendClassName="None"/>
{items}
</PathOfBuilding2>"#
    );
    let build = parse_build(&xml).expect("parse synthetic build");
    let mut extra_modifier_texts: Vec<String> = [
        "+1000 to Armour",
        "+1000 to Evasion Rating",
        "+1000 to maximum Energy Shield",
    ]
    .map(String::from)
    .to_vec();
    extra_modifier_texts.extend(extra.iter().map(|text| (*text).to_string()));
    let options = DataOrchestratorOptions {
        extra_modifier_texts,
        ..Default::default()
    };
    calculate_with_data(&build, data, &options).expect("calculate")
}

/// A rating is a whole number: the one nearest to `expected`.
fn close(actual: f64, expected: f64) {
    assert_eq!(actual, expected.round(), "{actual} != {expected}");
}

#[test]
fn defences_increase_while_wielding_a_quarterstaff() {
    let data = data();
    let plain = calculate(&data, "Sinister Quarterstaff", &[]);
    let with = calculate(&data, "Sinister Quarterstaff", &[WHILE_WIELDING]);
    assert!(plain.armour > 0.0 && plain.evasion > 0.0 && plain.energy_shield > 0.0);
    close(with.armour, plain.armour * 1.3);
    close(with.evasion, plain.evasion * 1.3);
    close(with.energy_shield, plain.energy_shield * 1.3);
}

#[test]
fn another_weapon_or_none_does_not_count() {
    let data = data();
    for weapon in ["Heavy Bow", ""] {
        let plain = calculate(&data, weapon, &[]);
        let with = calculate(&data, weapon, &[WHILE_WIELDING]);
        assert_eq!(with.armour, plain.armour, "{weapon:?}");
        assert_eq!(with.evasion, plain.evasion, "{weapon:?}");
        assert_eq!(with.energy_shield, plain.energy_shield, "{weapon:?}");
    }
}
