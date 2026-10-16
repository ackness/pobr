//! A shield's block chance.
//!
//! The game's modifiers, by stat id:
//!
//! - `local_additional_block_chance_%`, "+N% to Block chance", and
//!   `local_block_chance_+%`, "N% increased Block chance": local to the
//!   shield they are on, like its base type's block chance. The item shows
//!   `floor((base + added) × (1 + increased))`.
//! - "N% increased Block chance" anywhere else, Magma Barrier's "Grants {0}%
//!   increased Block chance" (`skill_igneous_shield_grants_block_chance_+%`,
//!   25 plus 0.25 per quality) among them: multiplies the shield's block
//!   chance.

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};
use pobr_gamedata::{GameData, data_version, repo_data_root};

const SHIELD: &str = "Crucible Tower Shield";

const MAGMA_BARRIER: &str = r#"<Skill enabled="true" mainActiveSkill="1"><Gem gemId="Metadata/Items/Gem/SkillGemMagmaBarrier" skillId="MagmaBarrierPlayer" nameSpec="Magma Barrier" level="18" quality="20" enabled="true"/></Skill>"#;

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

#[test]
fn magma_barrier_increases_block_chance() {
    let data = data();
    let base = base_block(&data);
    // 25 + 0.25 x 20 quality = 30% increased.
    assert_eq!(
        block_chance(&data, &[], MAGMA_BARRIER, &[]),
        (base * 1.3).round()
    );
}

/// Shield bashes use their own attack time and each complete five points
/// of the shield's defence, never the main-hand weapon or unarmed damage.
/// Raise Shield assumes a blockable hit arrives whenever the bash can be used;
/// its 250 ms minimum raise and 35 ms release do not determine bash frequency.
#[test]
fn shield_bashes_use_the_shield_source_and_skill_attack_time() {
    let data = data();
    for (skill, base, defence, multiplier, time) in [
        (
            "ShieldBlockPlayer",
            "Splintered Tower Shield",
            "Armour",
            0.8,
            0.55,
        ),
        (
            "ParryPlayer",
            "Leather Buckler",
            "Evasion Rating",
            0.65,
            0.75,
        ),
    ] {
        let calculate = |rating, main_hand| {
            let xml = format!(
                r#"<PathOfBuilding2>
<Build level="90" className="Witch" mainSocketGroup="1"/>
<Skills activeSkillSet="1"><SkillSet id="1"><Skill enabled="true" mainActiveSkill="1"><Gem skillId="{skill}" level="1" quality="0" enabled="true"/></Skill></SkillSet></Skills>
<Items activeItemSet="1"><Item id="1">Rarity: NORMAL
{base}
{defence}: {rating}
Implicits: 0
</Item><Item id="2">Rarity: RARE
Test Mace
Wooden Club
Physical Damage: 100-200
Attacks per Second: 3.00
Implicits: 0
</Item><ItemSet id="1"><Slot name="Weapon 2" itemId="1"/>{main_hand}</ItemSet></Items>
</PathOfBuilding2>"#
            );
            let build = parse_build(&xml).expect("shield bash build");
            calculate_with_data(&build, &data, &DataOrchestratorOptions::default())
                .expect("shield bash")
        };
        let low = calculate(10, "");
        let fractional = calculate(14, "");
        let next = calculate(15, "");
        let armed = calculate(10, r#"<Slot name="Weapon 1" itemId="2"/>"#);
        // 3–4 Physical per 5 defence, twice, then the skill multiplier.
        let damage: f64 = low.damage_components.iter().map(|part| part.avg()).sum();
        assert!(
            (damage - 7.0 * multiplier).abs() < 1e-6,
            "{skill}: {damage}"
        );
        assert!((low.action_rate - 1.0 / time).abs() < 1e-6, "{skill}");
        assert!(low.off_hand.is_some() && low.main_hand.is_none(), "{skill}");
        assert_eq!(
            low.damage_components, fractional.damage_components,
            "{skill}: complete groups only"
        );
        let next_damage: f64 = next.damage_components.iter().map(|part| part.avg()).sum();
        assert!((next_damage - damage * 1.5).abs() < 1e-6, "{skill}");
        assert_eq!(
            low.damage_components, armed.damage_components,
            "{skill}: main hand must not supply damage"
        );
        assert_eq!(
            low.action_rate, armed.action_rate,
            "{skill}: main hand must not supply speed"
        );
    }
}

/// Shield Charge is an `ActiveBlock` skill like Raise Shield, but its release
/// does not wait for a blocked hit: one use is its 1300 ms minimum channel
/// (`base_minimum_channel_time_ms`), however fast its attack time is.
#[test]
fn shield_charge_uses_its_minimum_channel_time() {
    let data = data();
    let xml = r#"<PathOfBuilding2>
<Build level="90" className="Witch" mainSocketGroup="1"/>
<Skills activeSkillSet="1"><SkillSet id="1"><Skill enabled="true" mainActiveSkill="1"><Gem skillId="ShieldChargePlayer" level="20" quality="0" enabled="true"/></Skill></SkillSet></Skills>
<Items activeItemSet="1"><Item id="1">Rarity: NORMAL
Splintered Tower Shield
Armour: 100
Implicits: 0
</Item><ItemSet id="1"><Slot name="Weapon 2" itemId="1"/></ItemSet></Items>
</PathOfBuilding2>"#;
    let build = parse_build(xml).expect("shield charge build");
    let output = calculate_with_data(&build, &data, &DataOrchestratorOptions::default())
        .expect("shield charge");
    assert!(
        (output.action_rate - 1.0 / 1.3).abs() < 1e-6,
        "{}",
        output.action_rate
    );
}
