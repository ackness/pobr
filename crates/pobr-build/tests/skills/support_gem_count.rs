//! 已装辅助宝石颜色计数 → `<Color>SupportGems` multiplier 端到端验证
//! （PoB2 CalcSetup.lua:2015-2044；消费方 = a2-real-gaps 钉值条目的
//! `MultiplierThreshold{<Color>SupportGems, 10}` 下界阈值）。
//!
//! 断言：`5% increased Max Life if you have at least 10 Red Support Gems
//! Socketed` 词条在 **10 颗红辅助已装**时生效（Life ×~1.05）、**0 颗**时不生效
//! （下界缺键 = 不生效，欠算安全语义）。

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

/// 最小 build：Fireball 主组 + 可选一组 N 颗红辅助（AncestralCall，gem_colour=1）。
/// 辅助单独成组——计数遍历全部 enabled 组（vendor 同口径），且不干扰主技能。
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
    // 0 颗：阈值下界缺 10 → 词条不生效（Life 不含 +5% INC）。
    // 10 颗：RedSupportGems=10 ≥ 10 → +5% Life INC 生效。
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
