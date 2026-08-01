//! End-to-end verification of socketed support gem color counting ->
//! `<Color>SupportGems` multiplier (PoB2 CalcSetup.lua:2015-2044; consumed by a
//! pinned-value entry from a2-real-gaps, the
//! `MultiplierThreshold{<Color>SupportGems, 10}` lower-bound threshold).
//!
//! Asserts: the `5% increased Max Life if you have at least 10 Red Support
//! Gems Socketed` mod activates (Life ×~1.05) with **10 red supports
//! socketed**, and stays inactive with **0** (a missing lower-bound key means
//! "not active" — a fail-safe under-count semantics).

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};
use pobr_core::calc::MinimalInput;
use pobr_data::monster::EnemyTier;
use pobr_gamedata::{GameData, repo_data_root};

fn load_build_data() -> BuildData {
    let data = GameData::new(repo_data_root().join(pobr_gamedata::data_version()));
    BuildData::load(&data).expect("load BuildData from repo data")
}

fn opts_with_threshold_line() -> DataOrchestratorOptions {
    DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        enemy_level: 0,
        enemy_tier: EnemyTier::None,
        mode_effective: false,
        extra_modifier_texts: vec![
            "5% increased Max Life if you have at least 10 Red Support Gems Socketed".into(),
        ],
        ..Default::default()
    }
}

/// Minimal build: a Fireball main group plus an optional group of N red
/// supports (AncestralCall, gem_colour=1). The supports form their own group —
/// the count walks every enabled group (matching vendor behaviour) without
/// interfering with the main skill.
fn xml_with_red_supports(count: usize) -> String {
    let supports: String = (0..count)
        .map(|_| {
            r#"<Gem enabled="true" gemId="Metadata/Items/Gem/SupportGemAncestralCall" skillId="SupportAncestralCallPlayer" level="1" quality="0"/>"#
        })
        .collect();
    let support_group = if count > 0 {
        format!(
            r#"<Skill enabled="true" slot="weapon2">
        {supports}
      </Skill>"#
        )
    } else {
        String::new()
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<PathOfBuilding2>
  <Build level="90" className="Sorceress" ascendClassName="" mainSocketGroup="1"/>
  <Skills activeSkillSet="1">
    <SkillSet id="1">
      <Skill enabled="true" slot="weapon1">
        <Gem enabled="true" gemId="Metadata/Items/Gems/Fireball" skillId="FireballPlayer" level="20" quality="0"/>
      </Skill>
      {support_group}
    </SkillSet>
  </Skills>
</PathOfBuilding2>"#
    )
}

#[test]
fn red_support_gem_count_activates_threshold_line() {
    let build_data = load_build_data();
    let opts = opts_with_threshold_line();

    let bare = parse_build(&xml_with_red_supports(0)).expect("parse bare");
    let ten = parse_build(&xml_with_red_supports(10)).expect("parse with 10 red supports");

    let base = calculate_with_data(&bare, &build_data, &opts).expect("bare calc");
    let boosted = calculate_with_data(&ten, &build_data, &opts).expect("boosted calc");

    assert!(base.life > 0.0, "基线 Life 应非零");
    // At 0: the count falls short of the lower bound (10), so the mod stays inactive (Life has no +5% INC).
    // At 10: RedSupportGems=10 >= 10, so the +5% Life INC activates.
    assert!(
        boosted.life > base.life,
        "10 颗红辅助应激活 +5% Life 阈值词条：base {} vs boosted {}",
        base.life,
        boosted.life
    );
    let ratio = boosted.life / base.life;
    assert!(
        (1.0..1.06).contains(&ratio),
        "Life 提升应来自单条 5% INC（池内稀释后 ≤5%）：ratio {ratio}"
    );
}
