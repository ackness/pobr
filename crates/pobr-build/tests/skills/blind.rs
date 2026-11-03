//! Blind: "Being Blinded causes 20% less Accuracy Rating and Evasion Rating".
//!
//! A build states that the enemy is Blinded with the `conditionEnemyBlinded`
//! config input. The enemy's lower Evasion Rating raises the character's
//! chance to hit; its lower Accuracy Rating raises the character's chances to
//! evade and to deflect.

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};
use pobr_core::calc::OutputTable;

const ICE_SHOT: &str = r#"<Gem gemId="Metadata/Items/Gem/SkillGemIceShot" skillId="IceShotPlayer" nameSpec="Ice Shot" level="20" quality="0" enabled="true"/>"#;

const BLIND_II: &str = r#"<Gem gemId="Metadata/Items/Gems/SupportGemBlindTwo" skillId="SupportBlindPlayerTwo" nameSpec="Blind II" level="1" quality="0" enabled="true"/>"#;

use crate::test_data as data;

/// A bow attacker with some Evasion and Deflection Rating, against an enemy
/// that is or is not stated to be Blinded.
fn calculate(data: &BuildData, blinded: bool, effective: bool, extra: &[&str]) -> OutputTable {
    calculate_with(data, &[ICE_SHOT], blinded, effective, extra)
}

/// [`calculate`] with a skill group for each of `groups`, the first the
/// main one.
fn calculate_with(
    data: &BuildData,
    groups: &[&str],
    blinded: bool,
    effective: bool,
    extra: &[&str],
) -> OutputTable {
    let skills: String = groups
        .iter()
        .enumerate()
        .map(|(index, gems)| {
            let main = if index == 0 {
                r#" mainActiveSkill="1""#
            } else {
                ""
            };
            format!(r#"<Skill enabled="true"{main}>{gems}</Skill>"#)
        })
        .collect();
    let input = if blinded {
        r#"<Input name="conditionEnemyBlinded" boolean="true"/>"#
    } else {
        ""
    };
    let xml = format!(
        r#"<PathOfBuilding2>
<Build level="80" className="Ranger" ascendClassName="None" mainSocketGroup="1"/>
<Skills defaultGemLevel="20" activeSkillSet="1"><SkillSet id="1">
  {skills}
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

/// Blind II gives the Blinds of the skill it supports "20% increased Blind
/// Effect" (`blind_effect_+%`). Blind does not stack: the enemy carries
/// one Blind, the strongest, so a second supported skill adds nothing.
#[test]
fn a_support_strengthens_the_blind_of_the_skill_it_supports() {
    let data = data();
    let supported = format!("{ICE_SHOT}{BLIND_II}");
    let plain = calculate(&data, true, true, &[]);
    let increased = calculate(&data, true, true, &["20% increased Blind Effect"]);
    assert!(increased.evade_chance > plain.evade_chance);

    let one = calculate_with(&data, &[&supported], true, true, &[]);
    assert_eq!(one.evade_chance, increased.evade_chance);
    assert_eq!(one.deflect_chance, increased.deflect_chance);
    assert_eq!(one.hit_chance, increased.hit_chance);
    // Supporting another skill's group, not the main one.
    let other = calculate_with(&data, &[ICE_SHOT, &supported], true, true, &[]);
    assert_eq!(other.evade_chance, increased.evade_chance);
    let two = calculate_with(&data, &[&supported, &supported], true, true, &[]);
    assert_eq!(two.evade_chance, increased.evade_chance);
}

#[test]
fn compatible_minion_attacks_can_supply_the_strongest_blind() {
    let data = data();
    let increased = calculate(&data, true, true, &["20% increased Blind Effect"]);
    let plain = calculate(&data, true, true, &[]);
    for skill in ["WolfPackPlayer", "WildProtectorPlayer"] {
        let summon = format!(r#"<Gem skillId="{skill}" level="20" enabled="true"/>"#);
        let supported = format!("{summon}{BLIND_II}");
        let one = calculate_with(&data, &[ICE_SHOT, &supported], true, true, &[]);
        assert_eq!(one.evade_chance, increased.evade_chance, "{skill}");
        assert_eq!(one.deflect_chance, increased.deflect_chance, "{skill}");
        assert_eq!(one.hit_chance, increased.hit_chance, "{skill}");
        let duplicate = calculate_with(&data, &[ICE_SHOT, &supported, &supported], true, true, &[]);
        assert_eq!(duplicate.deflect_chance, one.deflect_chance);
        assert_eq!(duplicate.evade_chance, one.evade_chance);

        for inactive in [
            format!(
                "{summon}{}",
                BLIND_II.replace("enabled=\"true\"", "enabled=\"false\"")
            ),
            format!(
                "{}{BLIND_II}",
                summon.replace("enabled=\"true\"", "enabled=\"false\"")
            ),
        ] {
            let unused = calculate_with(&data, &[ICE_SHOT, &inactive], true, true, &[]);
            assert_eq!(unused.deflect_chance, plain.deflect_chance, "{skill}");
        }
        for (blinded, effective) in [(false, true), (true, false)] {
            let control = calculate(&data, blinded, effective, &[]);
            let unused = calculate_with(&data, &[ICE_SHOT, &supported], blinded, effective, &[]);
            assert_eq!(unused.deflect_chance, control.deflect_chance);
            assert_eq!(unused.evade_chance, control.evade_chance);
        }
        let disabled = parse_build(&format!(
            r#"<PathOfBuilding2><Build level="80" className="Ranger"/>
<Skills><Skill enabled="false">{supported}</Skill></Skills></PathOfBuilding2>"#
        ))
        .unwrap();
        assert_eq!(
            pobr_core::skill_env::strongest_skill_blind(&disabled, data),
            None
        );
    }
    // A non-damaging buff cannot borrow an unrelated minion's attack types.
    let incompatible =
        format!(r#"<Gem skillId="WindDancerPlayer" level="20" enabled="true"/>{BLIND_II}"#);
    let unused = calculate_with(&data, &[ICE_SHOT, &incompatible], true, true, &[]);
    assert_eq!(unused.deflect_chance, plain.deflect_chance);
}

#[test]
fn minion_blind_does_not_combine_supports_from_different_attacks() {
    use pobr_core::skill_env::strongest_skill_blind;

    let mut data = data().clone();
    let minion_id = data.effect_minion_list("WolfPackPlayer")[0].clone();
    let attack_id = data.minions[&minion_id].skill_list[0].clone();
    for (id, skill_type, value) in [
        ("BlindTestMelee", "Melee", 20.0),
        ("BlindTestProjectile", "Projectile", 30.0),
    ] {
        let mut attack = data.granted_effects[&attack_id].clone();
        attack.id = id.into();
        attack.skill_types = vec!["Attack".into(), "Damage".into(), skill_type.into()];
        data.granted_effects.insert(id.into(), attack);
        let support_id = format!("{id}Support");
        let mut support = data.granted_effects["SupportBlindPlayerTwo"].clone();
        support.id = support_id.clone();
        support.require_skill_types = vec![skill_type.into()];
        support.exclude_skill_types.clear();
        data.granted_effects.insert(support_id.clone(), support);
        let mut stats = data.skill_stat_sets["SupportBlindPlayerTwo"].clone();
        for stat in &mut stats.sets[0].constant_stats {
            if stat.stat == "blind_effect_+%" {
                stat.value = value;
            }
        }
        data.skill_stat_sets.insert(support_id, stats);
    }
    data.minions.get_mut(&minion_id).unwrap().skill_list =
        vec!["BlindTestMelee".into(), "BlindTestProjectile".into()];
    let build = parse_build(
        r#"<PathOfBuilding2><Build level="80" className="Ranger"/>
<Skills><Skill><Gem skillId="WolfPackPlayer" level="20"/>
<Gem skillId="BlindTestMeleeSupport" level="1"/>
<Gem skillId="BlindTestProjectileSupport" level="1"/>
</Skill></Skills></PathOfBuilding2>"#,
    )
    .unwrap();
    assert_eq!(strongest_skill_blind(&build, &data).unwrap().1, 30.0);
    data.minions
        .get_mut(&minion_id)
        .unwrap()
        .skill_list
        .reverse();
    assert_eq!(strongest_skill_blind(&build, &data).unwrap().1, 30.0);
    data.granted_effects
        .get_mut("BlindTestProjectile")
        .unwrap()
        .cannot_be_supported = true;
    assert_eq!(strongest_skill_blind(&build, &data).unwrap().1, 20.0);
    data.granted_effects
        .get_mut("WolfPackPlayer")
        .unwrap()
        .cannot_be_supported = true;
    assert!(strongest_skill_blind(&build, &data).is_none());
}
