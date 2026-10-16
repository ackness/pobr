//! Stacking player buffs must retain their data-driven stack limit: the cap is
//! a `Multiplier:<X>Limit` BASE in the buff payload, read back as the per-stack
//! MORE tag's `limitVar`.
//!
//! - Wind Dancer (PoB2 act_dex.lua:11042-11053): 10% more Evasion per stage,
//!   capped by `wind_dancer_maximum_number_of_stages` (3).
//! - Scavenged Plating (PoB2 act_str.lua:16685-16703): 4% more Armour per
//!   stack, capped by `scavenged_plating_maximum_stacks_display` (10).

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};
use pobr_core::calc::OutputTable;
use pobr_gamedata::{GameData, data_version, repo_data_root};

struct StackingBuff {
    gem_id: &'static str,
    skill_id: &'static str,
    name: &'static str,
    config_input: &'static str,
    base_modifier: &'static str,
    stat: fn(&OutputTable) -> f64,
}

/// Each case is `(stacks, group enabled, gem enabled, expected stat)`.
fn assert_stacking_buff(buff: &StackingBuff, cases: &[(u32, bool, bool, f64)]) {
    let data = BuildData::load(&GameData::new(repo_data_root().join(data_version())))
        .expect("load repository data");
    let options = DataOrchestratorOptions {
        inject_character_base: false,
        mode_effective: true,
        extra_modifier_texts: vec![buff.base_modifier.into()],
        ..Default::default()
    };
    let StackingBuff {
        gem_id,
        skill_id,
        name,
        config_input,
        ..
    } = buff;
    for &(stacks, group_enabled, gem_enabled, expected) in cases {
        let xml = format!(
            r#"<PathOfBuilding2>
<Build level="80" className="Monk" ascendClassName="None" mainSocketGroup="1"/>
<Skills defaultGemLevel="20" activeSkillSet="1"><SkillSet id="1">
  <Skill enabled="true" mainActiveSkill="1">
    <Gem gemId="Metadata/Items/Gem/SkillGemFireball" skillId="FireballPlayer" nameSpec="Fireball" level="20" quality="0" enabled="true"/>
  </Skill>
  <Skill enabled="{group_enabled}" mainActiveSkill="1">
    <Gem gemId="{gem_id}" skillId="{skill_id}" nameSpec="{name}" level="20" quality="0" enabled="{gem_enabled}"/>
  </Skill>
</SkillSet></Skills>
<Config><Input name="{config_input}" number="{stacks}"/></Config>
</PathOfBuilding2>"#
        );
        let build = parse_build(&xml).expect("parse synthetic build");
        assert_eq!(build.socket_groups.len(), 2);
        assert_eq!(build.socket_groups[1].gem_skills[0].enabled, gem_enabled);
        let output = calculate_with_data(&build, &data, &options).expect("calculate");
        let actual = (buff.stat)(&output);
        assert!(
            (actual - expected).abs() < 1e-8,
            "{name}: stacks={stacks}, group={group_enabled}, gem={gem_enabled}: expected {expected}, got {actual}"
        );
    }
}

#[test]
fn wind_dancer_evasion_uses_stack_limit_and_enabled_state() {
    assert_stacking_buff(
        &StackingBuff {
            gem_id: "Metadata/Items/Gem/SkillGemWindDancer",
            skill_id: "WindDancerPlayer",
            name: "Wind Dancer",
            config_input: "windDancerStacks",
            base_modifier: "+1000 to Evasion Rating",
            stat: |output| output.evasion,
        },
        &[
            (0, true, true, 1000.0),
            (1, true, true, 1100.0),
            (3, true, true, 1300.0),
            (10, true, true, 1300.0),
            (3, false, true, 1000.0),
            (3, true, false, 1000.0),
        ],
    );
}

#[test]
fn scavenged_plating_armour_uses_stack_limit_and_enabled_state() {
    assert_stacking_buff(
        &StackingBuff {
            gem_id: "Metadata/Items/Gem/SkillGemScavengedPlating",
            skill_id: "ScavengedPlatingPlayer",
            name: "Scavenged Plating",
            config_input: "scavengedPlatingStacks",
            base_modifier: "+1000 to Armour",
            stat: |output| output.armour,
        },
        &[
            (0, true, true, 1000.0),
            (1, true, true, 1040.0),
            (10, true, true, 1400.0),
            (25, true, true, 1400.0),
            (10, false, true, 1000.0),
            (10, true, false, 1000.0),
        ],
    );
}
