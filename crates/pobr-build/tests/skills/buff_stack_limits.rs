//! Wind Dancer's player buff must retain its data-driven stack limit.
//! PoB2 act_dex.lua: WindDancerPlayer stat set 1 grants 10% more Evasion
//! per stage, capped by wind_dancer_maximum_number_of_stages (3).

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};
use pobr_gamedata::{GameData, data_version, repo_data_root};

#[test]
fn wind_dancer_evasion_uses_stack_limit_and_enabled_state() {
    let data = BuildData::load(&GameData::new(repo_data_root().join(data_version())))
        .expect("load repository data");
    let options = DataOrchestratorOptions {
        inject_character_base: false,
        mode_effective: true,
        extra_modifier_texts: vec!["+1000 to Evasion Rating".into()],
        ..Default::default()
    };
    for (stacks, group_enabled, gem_enabled, expected) in [
        (0, true, true, 1000.0),
        (1, true, true, 1100.0),
        (3, true, true, 1300.0),
        (10, true, true, 1300.0),
        (3, false, true, 1000.0),
        (3, true, false, 1000.0),
    ] {
        let xml = format!(
            r#"<PathOfBuilding2>
<Build level="80" className="Monk" ascendClassName="None" mainSocketGroup="1"/>
<Skills defaultGemLevel="20" activeSkillSet="1"><SkillSet id="1">
  <Skill enabled="true" mainActiveSkill="1">
    <Gem gemId="Metadata/Items/Gem/SkillGemFireball" skillId="FireballPlayer" nameSpec="Fireball" level="20" quality="0" enabled="true"/>
  </Skill>
  <Skill enabled="{group_enabled}" mainActiveSkill="1">
    <Gem gemId="Metadata/Items/Gem/SkillGemWindDancer" skillId="WindDancerPlayer" nameSpec="Wind Dancer" level="20" quality="0" enabled="{gem_enabled}"/>
  </Skill>
</SkillSet></Skills>
<Config><Input name="windDancerStacks" number="{stacks}"/></Config>
</PathOfBuilding2>"#
        );
        let build = parse_build(&xml).expect("parse synthetic build");
        assert_eq!(build.socket_groups.len(), if gem_enabled { 2 } else { 1 });
        let output = calculate_with_data(&build, &data, &options).expect("calculate");
        assert!(
            (output.evasion - expected).abs() < 1e-8,
            "stacks={stacks}, group={group_enabled}, gem={gem_enabled}: expected {expected}, got {}",
            output.evasion
        );
    }
}
