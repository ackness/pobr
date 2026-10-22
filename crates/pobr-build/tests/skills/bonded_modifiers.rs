//! Bonded modifiers of socketed Runes and Idols, through a whole build.
//!
//! "Idols socketed in this item gain the benefits of their Bonded modifiers"
//! is a modifier the Fox Idol gives a Body Armour; its own Bonded modifier is
//! "+5% to Quality of all Skills". Medved's Tending is a Rune whose Bonded
//! modifiers are "3% increased maximum Life" and "3% increased maximum Mana".
//!
//! Quality shows in Archmage's reservation: its quality gives 0.5% Reservation
//! Efficiency per 1% (`base_reservation_efficiency_+%`), a whole number, on a
//! reservation of 100 Spirit. 20% quality reserves 100 / 1.10 = 91, 25%
//! reserves 100 / 1.12 = 89.

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};
use pobr_core::calc::OutputTable;

const ARCHMAGE: &str = r#"<Gem gemId="Metadata/Items/Gem/SkillGemArchmage" skillId="ArchmagePlayer" nameSpec="Archmage" level="20" quality="20" enabled="true"/>"#;

const UNLOCK: &str =
    "{enchant}{rune}Idols socketed in this item gain the benefits of their Bonded modifiers";
const QUALITY: &str = "{enchant}{rune}Bonded: +5% to Quality of all Skills";
const LIFE: &str = "{enchant}{rune}Bonded: 3% increased maximum Life";
const MANA: &str = "{enchant}{rune}Bonded: 3% increased maximum Mana";
const SOUL: &str = "{enchant}{rune}Can roll Soul modifiers";

/// "Wisdom of the Maji" on the 0_4 passive tree: "Gain the benefits of Bonded
/// modifiers on Runes and Idols".
const WISDOM_OF_THE_MAJI: &str = "42253";

use crate::test_data as data;

/// A Sorceress with Archmage and a Body Armour holding `socketed`, whose
/// socketed things give it `lines`.
fn calculate(data: &BuildData, socketed: &[&str], lines: &[&str], tree: &str) -> OutputTable {
    let mut armour = String::from("Rarity: RARE\nTest Piece\nSacrificial Regalia\n");
    for name in socketed {
        armour.push_str(&format!("Rune: {name}\n"));
    }
    armour.push_str("Implicits: 0\n");
    for line in lines {
        armour.push_str(line);
        armour.push('\n');
    }
    let xml = format!(
        r#"<PathOfBuilding2>
<Build level="80" className="Sorceress" ascendClassName="None" mainSocketGroup="1"/>
{tree}
<Skills defaultGemLevel="20" activeSkillSet="1"><SkillSet id="1">
  <Skill enabled="true" mainActiveSkill="1">{ARCHMAGE}</Skill>
</SkillSet></Skills>
<Items activeItemSet="1"><Item id="1">{armour}</Item><ItemSet id="1"><Slot name="Body Armour" itemId="1"/></ItemSet></Items>
</PathOfBuilding2>"#
    );
    let build = parse_build(&xml).expect("parse synthetic build");
    calculate_with_data(&build, data, &DataOrchestratorOptions::default()).expect("calculate")
}

#[test]
fn the_fox_idol_gives_its_own_quality_and_not_the_runes_life() {
    let data = data();
    let plain = calculate(&data, &[], &[], "");
    assert_eq!(plain.spirit_reserved, 91.0);

    let socketed = ["Fox Idol", "Medved's Tending"];
    let with_idol = calculate(&data, &socketed, &[LIFE, MANA, QUALITY, UNLOCK, SOUL], "");
    assert_eq!(with_idol.spirit_reserved, 89.0);
    assert_eq!(with_idol.life, plain.life);
    assert_eq!(with_idol.mana, plain.mana);
}

#[test]
fn bonded_modifiers_do_nothing_without_their_benefit() {
    let data = data();
    let plain = calculate(&data, &[], &[], "");
    let bonded = calculate(
        &data,
        &["Medved's Tending"],
        &[LIFE, MANA, QUALITY, SOUL],
        "",
    );
    assert_eq!(bonded.spirit_reserved, plain.spirit_reserved);
    assert_eq!(bonded.life, plain.life);
    assert_eq!(bonded.mana, plain.mana);
}

/// With the passive, a Rune's Bonded modifiers are worth what the same lines
/// are worth as ordinary modifiers of the item.
#[test]
fn the_passive_gives_the_bonded_modifiers_of_runes_too() {
    let data = data();
    let tree = |nodes: &str| {
        format!(r#"<Tree activeSpec="1"><Spec nodes="{nodes}" treeVersion="0_4"/></Tree>"#)
    };
    let socketed = ["Medved's Tending"];
    let ordinary = [
        "{enchant}{rune}3% increased maximum Life",
        "{enchant}{rune}3% increased maximum Mana",
        SOUL,
    ];
    let expected = calculate(&data, &socketed, &ordinary, &tree(""));
    let without = calculate(&data, &socketed, &[LIFE, MANA, SOUL], &tree(""));
    let with = calculate(
        &data,
        &socketed,
        &[LIFE, MANA, SOUL],
        &tree(WISDOM_OF_THE_MAJI),
    );
    assert!(without.life < expected.life && without.mana < expected.mana);
    assert_eq!(with.life, expected.life);
    assert_eq!(with.mana, expected.mana);
}
