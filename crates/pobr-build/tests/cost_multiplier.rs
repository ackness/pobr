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
/// support Energy Retention(+10%) × Magnified Area II(+30%)。
///
/// **倍率链中间值与 PoB2 逐位一致**：PoB2 headless oracle（`tools/pob2-oracle/run.sh`，
/// 与 meta.json `player_stats.ManaCost = 577` 一致）给出
/// `ManaCostRaw = 577.72 = 404 × 1.43`，其中 404 = Comet **L29**（XML L19 + PoB2
/// 侧 +10 gem level）的 base cost，1.43 = `floor4(More("SupportManaMultiplier"))`
/// ——同组的 Boundless Energy II(+15%) 被 PoB2 裁决拒收、倍率不计入。
/// PoBR 端 T3.6 裁决得到**同一兼容集**、同一倍率 1.43；当前 gem level bonus 解析
/// 为 L22（XML L19 + 3，+10 缺口是 T4 之外的既有 gem-level 链路差异，oracle
/// `skillInfo.activeGemLevel = 29` 备查），base cost 211 →
/// `floor(211 × 1.43) = 301`。倍率链对拍以「同 base 下与 PoB2 公式逐位一致」为
/// 断言口径；gem level 解析对齐后本测试的预期值随 base 变化（届时应趋向 577）。
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

    // oracle 倍率中间值：floor4(1.1 × 1.3) = 1.43（兼容名单口径，PoB2 同值）。
    let oracle_mult = 1.43;
    // PoBR 解析行的 base cost（Comet L22 = 211）：用与 PoB2 同一公式推算预期。
    // 若 gem level 解析链路改变（向 PoB2 的 L29/404 收敛），在 Comet 等级表中
    // 找到与输出匹配的行，保证断言锚定的是**倍率链**而非等级解析。
    let comet_rows = &data.granted_effect_levels["CometPlayer"];
    let matched = comet_rows.iter().find(|r| {
        r.cost_amounts
            .first()
            .is_some_and(|&c| (f64::from(c) * oracle_mult).floor() == out.mana_cost)
    });
    assert!(
        matched.is_some(),
        "mana cost {} 必须 = floor(base × 1.43)（PoB2 oracle 倍率链），且 base 是 \
         Comet 某等级行的 cost——倍率链或兼容裁决漂移",
        out.mana_cost
    );
    // 当前等级解析口径下的具体值锚定（等级链路改动时显式更新此行并复核 oracle）。
    assert_eq!(
        out.mana_cost, 301.0,
        "Comet L22 base 211 × 1.43 → floor = 301"
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
