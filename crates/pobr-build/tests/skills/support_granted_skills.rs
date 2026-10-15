//! Skills that a support gem grants beside its support effect.
//!
//! Doedre's Undoing supports a curse: "Each Cursed Ground area from Supported
//! Skills spawns a Witchtoad every 0.25 seconds" (`doedre_cursed_ground_toad_
//! spawn_interval_ms = 250`), "Limit 2 areas of Cursed Ground". The gem also
//! grants Dark Consequences, which "is Triggered when Witchtoads leap at
//! enemies". The character never casts it: it is used once per Witchtoad.

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};
use pobr_core::calc::OutputTable;
use pobr_gamedata::{GameData, data_version, repo_data_root};

/// A curse supported by Doedre's Undoing, calculating the group's
/// `main_active_skill`-th skill: 1 is the curse, 2 is Dark Consequences.
fn calculate(data: &BuildData, main_active_skill: u32, modifiers: &[&str]) -> OutputTable {
    let xml = format!(
        r#"<PathOfBuilding2>
<Build level="80" className="Witch" ascendClassName="None" mainSocketGroup="1"/>
<Skills defaultGemLevel="20" activeSkillSet="1"><SkillSet id="1">
  <Skill enabled="true" mainActiveSkill="{main_active_skill}">
    <Gem gemId="Metadata/Items/Gem/SkillGemElementalWeakness" skillId="ElementalWeaknessPlayer" nameSpec="Elemental Weakness" level="20" quality="0" enabled="true"/>
    <Gem gemId="Metadata/Items/Gem/SupportGemDoedresUndoing" skillId="SupportDoedresUndoingPlayer" nameSpec="Doedre's Undoing" level="1" quality="0" enabled="true"/>
  </Skill>
</SkillSet></Skills>
</PathOfBuilding2>"#
    );
    let build = parse_build(&xml).expect("parse synthetic build");
    let options = DataOrchestratorOptions {
        extra_modifier_texts: modifiers.iter().map(|text| text.to_string()).collect(),
        ..Default::default()
    };
    calculate_with_data(&build, data, &options).expect("calculate")
}

fn data() -> BuildData {
    BuildData::load(&GameData::new(repo_data_root().join(data_version())))
        .expect("load repository data")
}

const FASTER_CASTING: &[&str] = &["50% increased Cast Speed"];

/// Two areas, a Witchtoad every quarter second in each: eight explosions a
/// second, however fast the character casts.
#[test]
fn dark_consequences_is_used_at_the_rate_witchtoads_spawn() {
    let data = data();
    let output = calculate(&data, 2, &[]);
    assert_eq!(output.action_rate, 8.0);
    assert!(output.dps > 0.0);

    let faster = calculate(&data, 2, FASTER_CASTING);
    assert_eq!(faster.action_rate, 8.0);
    assert_eq!(faster.dps, output.dps);
}

/// The curse the support applies to is still cast by the character.
#[test]
fn the_supported_curse_keeps_the_characters_cast_rate() {
    let data = data();
    let curse = calculate(&data, 1, &[]);
    assert!(curse.action_rate > 0.0 && curse.action_rate < 8.0);
    let faster = calculate(&data, 1, FASTER_CASTING);
    assert!((faster.action_rate / curse.action_rate - 1.5).abs() < 1e-6);
}
