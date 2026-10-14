//! A shield's block chance.
//!
//! The game's modifiers, by stat id:
//!
//! - `local_additional_block_chance_%`, "+N% to Block chance", and
//!   `local_block_chance_+%`, "N% increased Block chance": local to the
//!   shield they are on, like its base type's block chance. The item shows
//!   `floor((base + added) × (1 + increased))`.
//! - "N% increased Block chance" anywhere else: multiplies the shield's
//!   block chance.

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};
use pobr_gamedata::{GameData, data_version, repo_data_root};

const SHIELD: &str = "Crucible Tower Shield";

fn data() -> BuildData {
    BuildData::load(&GameData::new(repo_data_root().join(data_version())))
        .expect("load repository data")
}

/// The block chance of the shield's base type.
fn base_block(data: &BuildData) -> f64 {
    data.armour_base(SHIELD)
        .and_then(|armour| armour.block_chance)
        .expect("a shield base with block chance")
}

/// The character's block chance with a shield carrying `modifiers`, the
/// given skill groups and `extra` modifiers from elsewhere.
fn block_chance(data: &BuildData, modifiers: &[&str], skills: &str, extra: &[&str]) -> f64 {
    let mut shield = format!("<Item id=\"1\">Rarity: RARE\nTest Shield\n{SHIELD}\nImplicits: 0\n");
    for modifier in modifiers {
        shield.push_str(modifier);
        shield.push('\n');
    }
    shield.push_str("</Item>");
    let xml = format!(
        r#"<PathOfBuilding2>
<Build level="80" className="Warrior" ascendClassName="None" mainSocketGroup="1"/>
<Skills defaultGemLevel="20" activeSkillSet="1"><SkillSet id="1">{skills}</SkillSet></Skills>
<Items activeItemSet="1">{shield}<ItemSet id="1"><Slot name="Weapon 2" itemId="1"/></ItemSet></Items>
</PathOfBuilding2>"#
    );
    let build = parse_build(&xml).expect("parse synthetic build");
    let options = DataOrchestratorOptions {
        extra_modifier_texts: extra.iter().map(|text| (*text).to_string()).collect(),
        ..Default::default()
    };
    calculate_with_data(&build, data, &options)
        .expect("calculate")
        .block_chance
}

#[test]
fn a_shields_own_block_chance_is_multiplied_by_the_characters() {
    let data = data();
    let base = base_block(&data);
    let increased = "30% increased Block chance";
    let shield = ((base * 1.3).floor() * 1.3).round();
    // Not added together: base x (1 + 0.3 + 0.3).
    assert_ne!(shield, (base * 1.6).round());
    assert_eq!(block_chance(&data, &[increased], "", &[increased]), shield);
    // Only the shield's own, and only the character's.
    assert_eq!(
        block_chance(&data, &[increased], "", &[]),
        (base * 1.3).floor()
    );
    assert_eq!(
        block_chance(&data, &[], "", &[increased]),
        (base * 1.3).round()
    );
    // "+N% to Block chance" on the shield adds to its base.
    assert_eq!(
        block_chance(&data, &["+5% to Block chance", increased], "", &[]),
        ((base + 5.0) * 1.3).floor()
    );
}
