//! M1-T4.5：持续保留型效果的 Spirit 预留聚合（`OutputTable::spirit_reserved`）。
//!
//! 口径对照 PoB2 `CalcDefence.lua:192-249` Reservation 段（Spirit 子集）：
//! `reserved = max(round(flat_total × floor4(Π(1 + reservation_multiplier/100))), 0)`，
//! flat/倍率含同组 support 贡献（`CalcActiveSkill.lua:692-700` / `:754-756`）。
//! M1 只做技能侧聚合——`Reserved`/`ReservationEfficiency` 词条族与 Spirit 池本值
//! /unreserved 归 M2 Track D（00-index 裁决 §4-12）；超载只报告不拦截。

use pobr_build::{
    Build, BuildData, CharacterIdentity, DataOrchestratorOptions, SocketGroup, calculate_with_data,
};
use pobr_data::catalog::{GrantedEffectDef, SkillLevelDef};
use pobr_gamedata::GameData;
use serde_json::json;

fn repo_data() -> BuildData {
    let data = GameData::new(pobr_gamedata::repo_data_root().join(pobr_gamedata::data_version()));
    BuildData::load(&data).expect("加载仓库数据")
}

/// 构造合成 [`BuildData`]：一个持续保留型光环（Spirit 60，可带自身保留倍率）+
/// 一个 support（可带 spirit flat / 保留倍率）。经 serde 构造，避免与并行 track
/// 的 schema 字段追加耦合（serde default 吸收新增字段）。
fn synthetic_data(
    aura_reservation_multiplier: Option<f64>,
    support_spirit_flat: Option<f64>,
    support_reservation_multiplier: Option<f64>,
) -> BuildData {
    let mut data = BuildData::empty();
    let aura: GrantedEffectDef = serde_json::from_value(json!({
        "id": "TestAura",
        "is_support": false,
        "active_skill": "test_aura",
        "cast_time": null,
        "skill_types": ["HasReservation", "Buff", "Persistent", "Aura"],
    }))
    .unwrap();
    let support: GrantedEffectDef = serde_json::from_value(json!({
        "id": "TestSupport",
        "is_support": true,
        "active_skill": null,
        "cast_time": null,
    }))
    .unwrap();
    let aura_row: SkillLevelDef = serde_json::from_value(json!({
        "level": 1,
        "spirit_reservation_flat": 60.0,
        "reservation_multiplier": aura_reservation_multiplier,
    }))
    .unwrap();
    let support_row: SkillLevelDef = serde_json::from_value(json!({
        "level": 1,
        "spirit_reservation_flat": support_spirit_flat,
        "reservation_multiplier": support_reservation_multiplier,
    }))
    .unwrap();
    data.granted_effects.insert("TestAura".into(), aura);
    data.granted_effects.insert("TestSupport".into(), support);
    data.granted_effect_levels
        .insert("TestAura".into(), vec![aura_row]);
    data.granted_effect_levels
        .insert("TestSupport".into(), vec![support_row]);
    data
}

fn build_with_group(group: SocketGroup) -> Build {
    Build::new()
        .with_character(CharacterIdentity {
            level: 80,
            class_name: "Witch".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(group)
}

fn calc(build: &Build, data: &BuildData) -> f64 {
    let opts = DataOrchestratorOptions::default();
    calculate_with_data(build, data, &opts)
        .expect("calc")
        .spirit_reserved
}

/// 真实数据：Alchemist's Boon（HasReservation，spiritReservationFlat 30，无倍率）
/// → spirit_reserved = 30。
#[test]
fn real_aura_reserves_flat_spirit() {
    let data = repo_data();
    let build = build_with_group(SocketGroup::new().with_gem_skill("AlchemistsBoonPlayer", 1));
    assert_eq!(calc(&build, &data), 30.0);
}

/// 无保留型技能（普通主动技能）不产生 Spirit 预留。
#[test]
fn non_reserving_skill_reserves_nothing() {
    let data = repo_data();
    let build = build_with_group(SocketGroup::new().with_gem_skill("FireballPlayer", 20));
    assert_eq!(calc(&build, &data), 0.0);
}

/// 裸 flat：60 × 1.0 = 60。
#[test]
fn flat_only_reservation() {
    let data = synthetic_data(None, None, None);
    let build = build_with_group(SocketGroup::new().with_gem_skill("TestAura", 1));
    assert_eq!(calc(&build, &data), 60.0);
}

/// 自身负保留倍率（如 -50）：60 × 0.5 = 30（蓝图验收：spirit 含 multiplier 单测）。
#[test]
fn own_negative_reservation_multiplier_halves() {
    let data = synthetic_data(Some(-50.0), None, None);
    let build = build_with_group(SocketGroup::new().with_gem_skill("TestAura", 1));
    assert_eq!(calc(&build, &data), 30.0);
}

/// 同组 support 正倍率 +20%（PoB2 `ReservationMultiplier` MORE）：60 × 1.2 = 72；
/// support spirit flat（ExtraSpirit）+10 并入 base：(60+10) × 1.2 = 84。
#[test]
fn support_multiplier_and_flat_apply() {
    let data = synthetic_data(None, None, Some(20.0));
    let build = build_with_group(
        SocketGroup::new()
            .with_gem_skill("TestAura", 1)
            .with_gem_skill("TestSupport", 1),
    );
    assert_eq!(calc(&build, &data), 72.0);

    let data = synthetic_data(None, Some(10.0), Some(20.0));
    let build = build_with_group(
        SocketGroup::new()
            .with_gem_skill("TestAura", 1)
            .with_gem_skill("TestSupport", 1),
    );
    assert_eq!(calc(&build, &data), 84.0);
}

/// 倍率叠乘 + 截断到 4 位小数 + round：自身 -50% × support +33% →
/// floor4(0.5 × 1.33) = 0.665 → round(60 × 0.665) = round(39.9) = 40。
#[test]
fn multiplier_product_truncates_to_four_decimals_then_rounds() {
    let data = synthetic_data(Some(-50.0), None, Some(33.0));
    let build = build_with_group(
        SocketGroup::new()
            .with_gem_skill("TestAura", 1)
            .with_gem_skill("TestSupport", 1),
    );
    assert_eq!(calc(&build, &data), 40.0);
}

/// -100% 保留倍率（如 Impurity 类免保留）→ 预留 0。
#[test]
fn full_negative_multiplier_zeroes_reservation() {
    let data = synthetic_data(Some(-100.0), None, None);
    let build = build_with_group(SocketGroup::new().with_gem_skill("TestAura", 1));
    assert_eq!(calc(&build, &data), 0.0);
}

/// 同一效果出现在多个组按 id 去重（只计一次）；禁用组不参与。
#[test]
fn dedupes_across_groups_and_skips_disabled() {
    let data = synthetic_data(None, None, None);
    let build = build_with_group(SocketGroup::new().with_gem_skill("TestAura", 1))
        .add_socket_group(SocketGroup::new().with_gem_skill("TestAura", 1));
    assert_eq!(calc(&build, &data), 60.0, "重复组去重");

    let mut disabled = SocketGroup::new().with_gem_skill("TestAura", 1);
    disabled.enabled = false;
    let build = Build::new()
        .with_character(CharacterIdentity {
            level: 80,
            class_name: "Witch".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(disabled);
    assert_eq!(calc(&build, &data), 0.0, "禁用组不预留");
}

/// Blasphemy per-curse 预留（vendor CalcDefence.lua:273-284）：`IsBlasphemy` 效果按
/// 同组 AppliesCurse 主动技能数各加 `blasphemy_base_spirit_reservation_per_socketed_curse`
/// （constant stat 60），口径 = 单份缩放 round 后 ×count。品质效率（:251，
/// q20 Blasphemy = 20×0.5 = 10%）除在每份上：round(60/1.1) = 55。
#[test]
fn blasphemy_reserves_per_socketed_curse_with_quality_efficiency() {
    let data = repo_data();
    // q20 Blasphemy + 2 条 curse → 55 × 2 = 110（curse 自身预留 0）。
    let build = build_with_group(
        SocketGroup::new()
            .with_gem_skill_quality("BlasphemyPlayer", 19, 20)
            .with_gem_skill("TemporalChainsPlayer", 19)
            .with_gem_skill("EnfeeblePlayer", 19),
    );
    assert_eq!(calc(&build, &data), 110.0);

    // q0 Blasphemy + 1 条 curse → 60（无效率缩放）。
    let build = build_with_group(
        SocketGroup::new()
            .with_gem_skill("BlasphemyPlayer", 19)
            .with_gem_skill("TemporalChainsPlayer", 19),
    );
    assert_eq!(calc(&build, &data), 60.0);

    // 无 curse 的裸 Blasphemy → 0（per-curse 无实例、自身 flat 为空）。
    let build = build_with_group(SocketGroup::new().with_gem_skill("BlasphemyPlayer", 19));
    assert_eq!(calc(&build, &data), 0.0);
}

/// 宝石品质预留效率对普通预留技能同样生效（vendor :251 对每技能 skillCfg 求
/// efficiency；此处数据源 = overlay/gem_quality_stats.json 的
/// `base_reservation_efficiency_+%` 斜率）。Herald of Thunder q20 = 20×0.5 = 10%
/// → round(30/1.1) = 27。
#[test]
fn gem_quality_reservation_efficiency_scales_flat() {
    let data = repo_data();
    // 数据前提自证：HeraldOfThunderPlayer 有 quality efficiency 斜率才断言缩放。
    let has_quality_eff = data
        .gem_quality_stats
        .get("HeraldOfThunderPlayer")
        .is_some_and(|rows| {
            rows.iter()
                .any(|q| q.stat == "base_reservation_efficiency_+%")
        });
    let build =
        build_with_group(SocketGroup::new().with_gem_skill_quality("HeraldOfThunderPlayer", 1, 20));
    if has_quality_eff {
        assert_eq!(calc(&build, &data), 27.0);
    } else {
        assert_eq!(calc(&build, &data), 30.0);
    }
}
