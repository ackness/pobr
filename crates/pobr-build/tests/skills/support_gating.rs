//! support 适用性裁决端到端验证（18-G2）。
//!
//! 经 **XML fixture → parse_build → calculate_with_data** 全链路，断言：
//! 1. **不兼容 support 拒收**：`SupportFerociousRoarPlayer`（require `[Warcry]`，
//!    PoB2 `Data/Skills/sup_str.lua` Ferocious Roar `requireSkillTypes`）塞进
//!    Fireball（法术，无 `Warcry` 类型）组后，其 `damage_+%` **不进** 击中/DPS——
//!    对照 PoB2 `Modules/CalcActiveSkill.lua:210-214`：只有
//!    `canGrantedEffectSupportActiveSkill`（CalcTools.lua:84-110）通过的 support
//!    才进 `effectList`。
//! 2. **兼容 support 正常注入**：`SupportMetaCastFireSpellOnHitPlayer`（require
//!    `[Spell, Triggerable, Fire, AND, AND]`，Fireball 全部具备；exclude
//!    `[InbuiltTrigger]` 不命中）的 `damage_+%` 照常抬升击中（INC 通道未误伤）。
//!
//! 数据为真实入库 `data/4.5.0.3.4/`（granted_effects.json 的 require/exclude token
//! 流由 GrantedEffects .dat 类型列解析，见）。

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};
use pobr_core::calc::MinimalInput;
use pobr_data::monster::EnemyTier;
use pobr_gamedata::{GameData, repo_data_root};

fn load_build_data() -> BuildData {
    let data = GameData::new(repo_data_root().join(pobr_gamedata::data_version()));
    BuildData::load(&data).expect("load BuildData from repo data")
}

fn panel_opts() -> DataOrchestratorOptions {
    DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        enemy_level: 0,
        enemy_tier: EnemyTier::None,
        mode_effective: false,
        extra_modifier_texts: vec![],
        ..Default::default()
    }
}

/// 最小 Fireball build XML（可选追加一个 support gem）。
fn fireball_xml(support: Option<(&str, &str)>) -> String {
    let support_gem = support
        .map(|(gem_id, skill_id)| {
            format!(
                r#"<Gem enabled="true" gemId="{gem_id}" skillId="{skill_id}" level="20" quality="0"/>"#
            )
        })
        .unwrap_or_default();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<PathOfBuilding2>
  <Build level="90" className="Sorceress" ascendClassName="" mainSocketGroup="1"/>
  <Skills activeSkillSet="1">
    <SkillSet id="1">
      <Skill enabled="true" slot="weapon1">
        <Gem enabled="true" gemId="Metadata/Items/Gems/Fireball" skillId="FireballPlayer" level="20" quality="0"/>
        {support_gem}
      </Skill>
    </SkillSet>
  </Skills>
</PathOfBuilding2>"#
    )
}

/// 不兼容 support（require `[Warcry]` vs 法术）被拒：倍率不进击中/DPS。
///
/// 修复前（按 `is_support` 全量注入）该 support 的 `damage_+%`（L20 ≈ +30）会被
/// 误注入 Fireball——PoB2 中 Ferocious Roar 只能支援 Warcry 技能。
#[test]
fn incompatible_support_is_rejected_end_to_end() {
    let build_data = load_build_data();

    // 前置：该 support 确实带可映射的 damage_+%（数据通道未断，拒收不是「没数值」的假阳性）。
    let sup = build_data.effect_stats("SupportFerociousRoarPlayer", 20, 0, None);
    let inc = sup
        .base
        .iter()
        .find(|s| s.stat == "damage_+%")
        .expect("FerociousRoar should carry damage_+%");
    assert!(inc.value > 0.0);

    let bare = parse_build(&fireball_xml(None)).expect("parse bare build");
    let with_incompatible = parse_build(&fireball_xml(Some((
        "Metadata/Items/Gem/SupportGemFerociousRoar",
        "SupportFerociousRoarPlayer",
    ))))
    .expect("parse build with incompatible support");

    let base = calculate_with_data(&bare, &build_data, &panel_opts()).expect("bare calc");
    let gated =
        calculate_with_data(&with_incompatible, &build_data, &panel_opts()).expect("gated calc");

    assert!(base.total_hit_avg > 0.0, "Fireball 基线击中应非零");
    assert!(
        (gated.total_hit_avg - base.total_hit_avg).abs() < 1e-9,
        "不兼容 support 的 damage_+% 不得进击中：base {} vs gated {}",
        base.total_hit_avg,
        gated.total_hit_avg
    );
    assert!(
        (gated.dps - base.dps).abs() < 1e-9,
        "不兼容 support 不得改变 DPS：base {} vs gated {}",
        base.dps,
        gated.dps
    );
}

/// 兼容 support（require `[Spell, Triggerable, Fire, AND, AND]` 全命中）正常注入：
/// `damage_+%`（L20 = 200）按 INC 通道抬升击中 ×3——裁决不误伤兼容名单。
#[test]
fn compatible_support_still_injects() {
    let build_data = load_build_data();

    let sup = build_data.effect_stats("SupportMetaCastFireSpellOnHitPlayer", 20, 0, None);
    let inc = sup
        .base
        .iter()
        .find(|s| s.stat == "damage_+%")
        .expect("MetaCastFireSpellOnHit should carry damage_+%");
    assert!(inc.value > 0.0);

    let bare = parse_build(&fireball_xml(None)).expect("parse bare build");
    let with_compatible = parse_build(&fireball_xml(Some((
        "Metadata/Items/Gem/SupportGemCastOnFireHit",
        "SupportMetaCastFireSpellOnHitPlayer",
    ))))
    .expect("parse build with compatible support");

    let base = calculate_with_data(&bare, &build_data, &panel_opts()).expect("bare calc");
    let boosted =
        calculate_with_data(&with_compatible, &build_data, &panel_opts()).expect("boosted calc");

    let expected = base.total_hit_avg * (1.0 + inc.value / 100.0);
    assert!(
        (boosted.total_hit_avg - expected).abs() < 1.0,
        "兼容 support damage_+% ({}) 应缩放击中：base {} → 期望 {}（实得 {}）",
        inc.value,
        base.total_hit_avg,
        expected,
        boosted.total_hit_avg
    );
}
