//! M1-T4.4：辅助宝石 cost 倍率（SupportManaMultiplier）端到端验证。
//!
//! PoB2 一手依据：注入 `CalcActiveSkill.lua:689-691`（兼容 support 的
//! `level.manaMultiplier` → `SupportManaMultiplier` MORE）；消费
//! `CalcOffence.lua:2052` `mult = floor(More("SupportManaMultiplier"), 4)` +
//! `:2076-2077` `finalBaseCost = floor(baseCost × mult)`（先于 inc/more 链）。
//! 只对 T3.6 兼容名单注入——被拒 support 的倍率不吃（对齐 PoB2 拒收）。

use pobr_build::{
    Build, BuildData, CharacterIdentity, DataOrchestratorOptions, SocketGroup, calculate_with_data,
    parse_build_from_code,
};
use pobr_data::catalog::{GrantedEffectDef, SkillLevelDef};
use pobr_gamedata::{GameData, repo_data_root};
use serde_json::json;
use std::path::Path;

fn repo_data() -> BuildData {
    let data = GameData::new(repo_data_root().join("4.5.0.3.4"));
    BuildData::load(&data).expect("加载仓库数据")
}

/// oracle 对拍（蓝图 T4.4 验收）：druid-oracle-comet——Comet + 兼容 cost 倍率
/// support（当前兼容集 = Magnified Area II(+30%)）。
///
/// **PoB2 golden**：headless oracle（`tools/pob2-oracle/run.sh`，与 meta.json
/// `player_stats.ManaCost = 577` 一致）给出 `ManaCostRaw = 577.72 = 404 × 1.43`，
/// 404 = Comet **L29** 的 base cost，1.43 = `floor4(More("SupportManaMultiplier"))`。
///
/// **PoBR 现行口径与已知残差**（2026-06 补刀波核对）：
/// - 等级解析：L28 = XML L19 + 物品 +9（`+3/+1 spell` + `+5 cold spell`——多词
///   类别按 PoB2 `CalcSetup.lua:414-419` keywordList 全 token 匹配）。与 PoB2 的
///   L29 余 +1 差（来源在物品文本通道之外，oracle `skillInfo.activeGemLevel = 29`
///   备查）。
/// - 倍率链：PoBR 兼容集 = MagArea II → `floor4(1.3) = 1.3`。PoB2 的 1.43 还含
///   Energy Retention(+10%)——但 ER 与 Boundless Energy II **都** require
///   `GeneratesEnergy`（vendor `sup_int.lua:4200/1041`，Comet 类型表无此 token），
///   PoB2 却只计 ER 不计 Boundless（1.43 ≠ 1.1×1.15×1.3）；该差异属 meta gem
///   （Spellslinger）能量链路（M1 验收报告已登记的 meta gem 缺口），非
///   doesTypeExpressionMatch 语义差。PoBR 按 require 表达式拒收两者，宁可跳过
///   不可错算。
/// - 历史注记：本测试旧锚 301 曾被解读为 `floor(211 × 1.43)`（L22 + ER 计入），
///   实为 `floor(232 × 1.3)`（L23 + ER 不计入）的数值巧合——两式同值 301。
///   等级修复后两种解读分道（527 vs 479），按实际链路锚定 479。
#[test]
fn oracle_druid_comet_mana_cost_with_support_multipliers() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/demo-bd-test/builds/druid-oracle-comet");
    let code = std::fs::read_to_string(dir.join("code.txt")).expect("read code.txt");
    let build = parse_build_from_code(code.trim()).expect("decode build");
    let data = repo_data();
    let opts = DataOrchestratorOptions {
        inject_character_base: true,
        ..Default::default()
    };
    let out = calculate_with_data(&build, &data, &opts).expect("calc");

    // 现行兼容集倍率：floor4(1.3) = 1.3（MagArea II；ER/Boundless 被 require
    // `GeneratesEnergy` 拒收，见函数 doc）。
    let support_mult = 1.3;
    // 断言锚定**倍率链**而非等级解析：输出必须 = floor(base × 1.3)，且 base 是
    // Comet 某等级行的 cost（等级解析改动时本断言仍约束链路形状）。
    let comet_rows = &data.granted_effect_levels["CometPlayer"];
    let matched = comet_rows.iter().find(|r| {
        r.cost_amounts
            .first()
            .is_some_and(|&c| (f64::from(c) * support_mult).floor() == out.mana_cost)
    });
    assert!(
        matched.is_some(),
        "mana cost {} 必须 = floor(base × 1.3)（现行兼容集倍率链），且 base 是 \
         Comet 某等级行的 cost——倍率链或兼容裁决漂移",
        out.mana_cost
    );
    // 当前等级解析口径下的具体值锚定（等级/链路改动时显式更新此行并复核 oracle）：
    // M4-H 起宝石等级加成扫描计入**树节点**词条（vendor GemProperty 全局
    // modDB 同源），druid 树 +1 spell skill level → Comet L29 base 404 ×
    // 1.3 → floor = 525（与 PoB2 golden 577 = 404 × 1.43 同一 base 行，
    // 余差 = ER 倍率，登记在 doc）。
    assert_eq!(
        out.mana_cost, 525.0,
        "Comet L29 base 404 × 1.3 → floor = 525"
    );
}

/// 合成端到端：兼容 support 倍率进 cost；不兼容 support（require 类型不匹配，
/// T3.6 裁决拒收）的倍率**不**进 cost。
#[test]
fn incompatible_support_multiplier_is_not_applied() {
    let mut data = BuildData::empty();
    let active: GrantedEffectDef = serde_json::from_value(json!({
        "id": "TestSpell",
        "is_support": false,
        "active_skill": "test_spell",
        "cast_time": 1000,
        "cost_types": [0],
        "skill_types": ["Spell", "Damage"],
    }))
    .unwrap();
    // 兼容：require Spell；cost 倍率 +30%。
    let sup_ok: GrantedEffectDef = serde_json::from_value(json!({
        "id": "TestSupportOk",
        "is_support": true,
        "active_skill": null,
        "cast_time": null,
        "require_skill_types": ["Spell"],
    }))
    .unwrap();
    // 不兼容：require Attack（TestSpell 无该类型，四段裁决第④段拒收）；倍率 +100%。
    let sup_rejected: GrantedEffectDef = serde_json::from_value(json!({
        "id": "TestSupportRejected",
        "is_support": true,
        "active_skill": null,
        "cast_time": null,
        "require_skill_types": ["Attack"],
    }))
    .unwrap();
    let active_row: SkillLevelDef = serde_json::from_value(json!({
        "level": 1,
        "cost_amounts": [100],
    }))
    .unwrap();
    let ok_row: SkillLevelDef = serde_json::from_value(json!({
        "level": 1,
        "mana_multiplier": 30.0,
    }))
    .unwrap();
    let rejected_row: SkillLevelDef = serde_json::from_value(json!({
        "level": 1,
        "mana_multiplier": 100.0,
    }))
    .unwrap();
    data.granted_effects.insert("TestSpell".into(), active);
    data.granted_effects.insert("TestSupportOk".into(), sup_ok);
    data.granted_effects
        .insert("TestSupportRejected".into(), sup_rejected);
    data.granted_effect_levels
        .insert("TestSpell".into(), vec![active_row]);
    data.granted_effect_levels
        .insert("TestSupportOk".into(), vec![ok_row]);
    data.granted_effect_levels
        .insert("TestSupportRejected".into(), vec![rejected_row]);

    let build = Build::new()
        .with_character(CharacterIdentity {
            level: 80,
            class_name: "Witch".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(
            SocketGroup::new()
                .with_gem_skill("TestSpell", 1)
                .with_gem_skill("TestSupportOk", 1)
                .with_gem_skill("TestSupportRejected", 1),
        )
        .with_main_socket_group(1);

    let opts = DataOrchestratorOptions::default();
    let out = calculate_with_data(&build, &data, &opts).expect("calc");
    // 只吃兼容的 +30%：floor(100 × 1.3) = 130。若误吃被拒的 +100% 会得 260。
    assert_eq!(out.mana_cost, 130.0, "只有兼容 support 的 cost 倍率生效");
}

/// 合成端到端：负倍率（如 Impurity 类 -100% 不在 support 路径——这里用 -50%）
/// 正确减费：floor(100 × 0.5) = 50。
#[test]
fn negative_support_multiplier_reduces_cost() {
    let mut data = BuildData::empty();
    let active: GrantedEffectDef = serde_json::from_value(json!({
        "id": "TestSpell",
        "is_support": false,
        "active_skill": "test_spell",
        "cast_time": 1000,
        "cost_types": [0],
        "skill_types": ["Spell", "Damage"],
    }))
    .unwrap();
    let sup: GrantedEffectDef = serde_json::from_value(json!({
        "id": "TestSupportCheap",
        "is_support": true,
        "active_skill": null,
        "cast_time": null,
        "require_skill_types": ["Spell"],
    }))
    .unwrap();
    let active_row: SkillLevelDef = serde_json::from_value(json!({
        "level": 1,
        "cost_amounts": [100],
    }))
    .unwrap();
    let sup_row: SkillLevelDef = serde_json::from_value(json!({
        "level": 1,
        "mana_multiplier": -50.0,
    }))
    .unwrap();
    data.granted_effects.insert("TestSpell".into(), active);
    data.granted_effects.insert("TestSupportCheap".into(), sup);
    data.granted_effect_levels
        .insert("TestSpell".into(), vec![active_row]);
    data.granted_effect_levels
        .insert("TestSupportCheap".into(), vec![sup_row]);

    let build = Build::new()
        .with_character(CharacterIdentity {
            level: 80,
            class_name: "Witch".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(
            SocketGroup::new()
                .with_gem_skill("TestSpell", 1)
                .with_gem_skill("TestSupportCheap", 1),
        )
        .with_main_socket_group(1);

    let opts = DataOrchestratorOptions::default();
    let out = calculate_with_data(&build, &data, &opts).expect("calc");
    assert_eq!(out.mana_cost, 50.0, "负倍率减费 floor(100 × 0.5) = 50");
}
