//! Uhtred's Omen: "+2 to Level of Supported Skills if exactly one other
//! Support is modifying them" (`supported_active_skill_gem_level_+_if_one_other_support`).
//! Supports are support gems: in a Cast on Elemental Ailment group, the
//! meta gem's trigger is not one, nor is a support that only supports the
//! meta gem (Boundless Energy II needs a skill that generates Energy).

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};

const UHTRED: &str = r#"<Gem gemId="Metadata/Items/Gem/SupportGemUhtredsOmen" skillId="SupportUhtredOmenPlayer" nameSpec="Uhtred's Omen" level="1" quality="0" enabled="true"/>"#;

/// Comet's average hit at `level`, in a Cast on Elemental Ailment group with
/// Execute III, Boundless Energy II and `extra`.
fn comet_hit(data: &BuildData, level: u32, extra: &str) -> f64 {
    let xml = format!(
        r#"<PathOfBuilding2>
<Build level="90" className="Witch" ascendClassName="None" mainSocketGroup="1"/>
<Skills defaultGemLevel="20" activeSkillSet="1"><SkillSet id="1">
  <Skill enabled="true" mainActiveSkill="2">
    <Gem gemId="Metadata/Items/Gems/SkillGemCastOnElementalAilment" skillId="MetaCastOnElementalAilmentPlayer" nameSpec="Cast on Elemental Ailment" level="20" quality="0" enabled="true"/>
    <Gem gemId="Metadata/Items/Gems/SkillGemComet" skillId="CometPlayer" nameSpec="Comet" level="{level}" quality="0" enabled="true"/>
    <Gem gemId="Metadata/Items/Gems/SupportGemExecuteThree" skillId="SupportExecutePlayerThree" nameSpec="Execute III" level="1" quality="0" enabled="true"/>
    <Gem gemId="Metadata/Items/Gem/SupportGemImpetusTwo" skillId="SupportBoundlessEnergyPlayerTwo" nameSpec="Boundless Energy II" level="1" quality="0" enabled="true"/>
    {extra}
  </Skill>
</SkillSet></Skills>
</PathOfBuilding2>"#
    );
    let build = parse_build(&xml).expect("parse synthetic build");
    let output =
        calculate_with_data(&build, data, &DataOrchestratorOptions::default()).expect("calculate");
    assert!(output.total_hit_avg > 0.0);
    output.total_hit_avg
}

#[test]
fn uhtreds_omen_counts_the_support_gems_modifying_the_skill() {
    let data = crate::test_data();
    assert_eq!(comet_hit(&data, 20, UHTRED), comet_hit(&data, 22, ""));
    assert!(comet_hit(&data, 22, "") > comet_hit(&data, 20, ""));
}
