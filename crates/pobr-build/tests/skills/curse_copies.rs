//! One curse used by two skill groups. "Debuffs of the same type do not
//! stack, and only the copy with the strongest effect applies": the enemy
//! has the stronger Despair, whichever group comes first.

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};

const DESPAIR: &str = r#"<Gem gemId="Metadata/Items/Gems/SkillGemDespair" skillId="DespairPlayer" nameSpec="Despair" level="20" quality="0" enabled="true"/>"#;
const HEIGHTENED_CURSE: &str = r#"<Gem gemId="Metadata/Items/Gems/SupportGemHeightenedCurse" skillId="SupportCurseEffectPlayer" nameSpec="Heightened Curse" level="1" quality="0" enabled="true"/>"#;

/// Essence Drain's damage with the curse groups `groups` after it.
fn chaos_damage(data: &BuildData, groups: &[&str]) -> f64 {
    let groups: String = groups
        .iter()
        .map(|gems| format!(r#"<Skill enabled="true" mainActiveSkill="1">{gems}</Skill>"#))
        .collect();
    let xml = format!(
        r#"<PathOfBuilding2>
<Build level="80" className="Witch" ascendClassName="None" mainSocketGroup="1"/>
<Skills defaultGemLevel="20" activeSkillSet="1"><SkillSet id="1">
  <Skill enabled="true" mainActiveSkill="1">
    <Gem gemId="Metadata/Items/Gems/SkillGemEssenceDrain" skillId="EssenceDrainPlayer" nameSpec="Essence Drain" level="20" quality="0" enabled="true"/>
  </Skill>
  {groups}
</SkillSet></Skills>
</PathOfBuilding2>"#
    );
    let build = parse_build(&xml).expect("parse synthetic build");
    let options = DataOrchestratorOptions::against_reference_enemy();
    calculate_with_data(&build, data, &options)
        .expect("calculate")
        .dps
}

#[test]
fn the_enemy_has_the_strongest_copy_of_a_curse() {
    let data = crate::test_data();
    let stronger = format!("{DESPAIR}{HEIGHTENED_CURSE}");
    let plain = chaos_damage(&data, &[DESPAIR]);
    let heightened = chaos_damage(&data, &[&stronger]);
    assert!(heightened > plain, "{heightened} vs {plain}");
    assert_eq!(chaos_damage(&data, &[DESPAIR, &stronger]), heightened);
    assert_eq!(chaos_damage(&data, &[&stronger, DESPAIR]), heightened);
}
