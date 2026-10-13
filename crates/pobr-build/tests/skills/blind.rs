//! Blind: "Being Blinded causes 20% less Accuracy Rating and Evasion Rating".
//!
//! A build states that the enemy is Blinded with the `conditionEnemyBlinded`
//! config input. The enemy's lower Evasion Rating raises the character's
//! chance to hit; its lower Accuracy Rating raises the character's chances to
//! evade and to deflect.

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};
use pobr_core::calc::OutputTable;
use pobr_gamedata::{GameData, data_version, repo_data_root};

const ICE_SHOT: &str = r#"<Gem gemId="Metadata/Items/Gem/SkillGemIceShot" skillId="IceShotPlayer" nameSpec="Ice Shot" level="20" quality="0" enabled="true"/>"#;

fn data() -> BuildData {
    BuildData::load(&GameData::new(repo_data_root().join(data_version())))
        .expect("load repository data")
}

/// A bow attacker with some Evasion and Deflection Rating, against an enemy
/// that is or is not stated to be Blinded.
fn calculate(data: &BuildData, blinded: bool, effective: bool, extra: &[&str]) -> OutputTable {
    let input = if blinded {
        r#"<Input name="conditionEnemyBlinded" boolean="true"/>"#
    } else {
        ""
    };
    let xml = format!(
        r#"<PathOfBuilding2>
<Build level="80" className="Ranger" ascendClassName="None" mainSocketGroup="1"/>
<Skills defaultGemLevel="20" activeSkillSet="1"><SkillSet id="1">
  <Skill enabled="true" mainActiveSkill="1">{ICE_SHOT}</Skill>
</SkillSet></Skills>
<Items activeItemSet="1"><Item id="1">Rarity: RARE
Test Bow
Heavy Bow
Implicits: 0
</Item><ItemSet id="1"><Slot name="Weapon 1" itemId="1"/></ItemSet></Items>
<Config>{input}</Config>
</PathOfBuilding2>"#
    );
    let build = parse_build(&xml).expect("parse synthetic build");
    let mut extra_modifier_texts: Vec<String> = [
        "+2000 to Evasion Rating",
        "Gain Deflection Rating equal to 40% of Evasion Rating",
    ]
    .map(String::from)
    .to_vec();
    extra_modifier_texts.extend(extra.iter().map(|text| (*text).to_string()));
    let options = DataOrchestratorOptions {
        mode_effective: effective,
        extra_modifier_texts,
        ..Default::default()
    };
    calculate_with_data(&build, data, &options).expect("calculate")
}

#[test]
fn a_blinded_enemy_is_hit_more_often_and_hits_less_often() {
    let data = data();
    let seeing = calculate(&data, false, true, &[]);
    let blinded = calculate(&data, true, true, &[]);
    // The comparison needs room to move on both sides.
    assert!(seeing.hit_chance < 1.0, "hit chance {}", seeing.hit_chance);
    assert!(seeing.evade_chance > 0.0 && seeing.deflect_chance > 0.0);

    assert!(
        blinded.hit_chance > seeing.hit_chance,
        "hit chance {} -> {}",
        seeing.hit_chance,
        blinded.hit_chance
    );
    assert!(
        blinded.evade_chance > seeing.evade_chance,
        "evade chance {} -> {}",
        seeing.evade_chance,
        blinded.evade_chance
    );
    assert!(
        blinded.deflect_chance > seeing.deflect_chance,
        "deflect chance {} -> {}",
        seeing.deflect_chance,
        blinded.deflect_chance
    );
    // Blind is on the enemy: the character's own ratings are what they were.
    assert_eq!(blinded.evasion, seeing.evasion);
    assert_eq!(blinded.accuracy, seeing.accuracy);
}

#[test]
fn the_enemy_is_blinded_only_for_effective_numbers() {
    let data = data();
    let seeing = calculate(&data, false, false, &[]);
    let blinded = calculate(&data, true, false, &[]);
    assert_eq!(blinded.hit_chance, seeing.hit_chance);
    assert_eq!(blinded.evade_chance, seeing.evade_chance);
    assert_eq!(blinded.deflect_chance, seeing.deflect_chance);
}

#[test]
fn blind_effect_makes_the_blind_the_character_inflicts_stronger() {
    let data = data();
    let effect = ["50% increased Blind Effect"];
    let plain = calculate(&data, true, true, &[]);
    let stronger = calculate(&data, true, true, &effect);
    assert!(stronger.evade_chance > plain.evade_chance);
    assert!(stronger.hit_chance >= plain.hit_chance);

    // With no Blind there is no effect to scale.
    let seeing = calculate(&data, false, true, &[]);
    let unused = calculate(&data, false, true, &effect);
    assert_eq!(unused.evade_chance, seeing.evade_chance);
    assert_eq!(unused.hit_chance, seeing.hit_chance);
}
