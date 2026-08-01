//!  暴击/非暴击双 pass 集成测试。
//!
//! I5 等价性（无 CriticalStrike 条件词条 → 短路走旧单因子，数学恒等
//! `blend(c, x×m, x) == x×crit.effect`）+ 暴击腿专属词条只放大 crit 腿 +
//! Stored 族落值。vendor 参照 CalcOffence.lua:3978-4057 / :4395。

use pobr_core::calc::{MinimalInput, calculate_minimal};
use pobr_core::{CalcConfig, ModDb, ModTag, Modifier};
use pobr_data::prelude::*;

fn input() -> MinimalInput {
    MinimalInput {
        base_hit_min: 100.0,
        base_hit_max: 200.0,
        base_action_rate: 1.0,
        ..MinimalInput::default()
    }
}

fn crit_db() -> ModDb {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "CriticalStrikeChance",
        ModType::Base,
        40.0,
    ));
    db.add_mod(Modifier::number(
        "CriticalStrikeMultiplier",
        ModType::Base,
        100.0, // 总爆伤 = 1 + (100+100)/100 = 3.0
    ));
    db
}

/// I5：无 CriticalStrike 条件词条时，total_hit_avg == 非暴击均值 × crit.effect
/// （旧单因子公式；短路路径取整顺序复刻，逐字节等价由 workspace 全量回归证明，
/// 此处显式断言数学恒等）。
#[test]
fn i5_no_crit_conditional_mods_equals_single_factor_formula() {
    let db = crit_db();
    let cfg = CalcConfig::attack();
    let out = calculate_minimal(&db, &cfg, &input());

    // 面板口径无命中降级：c = 0.40，m = 3.0，effect = 1 - c + c×m = 1.8。
    let non_crit_avg = 150.0;
    let effect = 1.0 - out.crit_chance + out.crit_chance * out.crit_multiplier;
    assert!((out.crit_chance - 0.40).abs() < 1e-9);
    assert!((out.crit_multiplier - 3.0).abs() < 1e-9);
    assert!(
        (out.total_hit_avg - non_crit_avg * effect).abs() < 1e-9,
        "I5 恒等：got {}",
        out.total_hit_avg
    );

    // Stored 族（玩家侧 pre-resist）：crit 腿 = hit 腿 × m；combined = 加权。
    let (_, hit_avg) = out.stored_hit_avg[0];
    let (_, crit_avg) = out.stored_crit_avg[0];
    let (_, combined) = out.stored_combined_avg[0];
    assert!((hit_avg - 150.0).abs() < 1e-9);
    assert!((crit_avg - 450.0).abs() < 1e-9);
    assert!(
        (combined - (450.0 * 0.40 + 150.0 * 0.60)).abs() < 1e-9,
        "StoredCombinedAvg = crit×c + hit×(1−c)，got {combined}"
    );
}

/// 暴击腿专属词条（`increased Damage on Critical Hit` 形）只放大 crit 腿
/// （vendor :3979 `skillCond["CriticalStrike"] = (pass == 1)`）。
#[test]
fn crit_conditional_mod_only_amplifies_crit_leg() {
    let mut db = crit_db();
    db.add_mod(
        Modifier::number("PhysicalDamage", ModType::Inc, 100.0)
            .with_tag(ModTag::condition("CriticalStrike", false)),
    );
    let cfg = CalcConfig::attack();
    let out = calculate_minimal(&db, &cfg, &input());

    let (_, hit_avg) = out.stored_hit_avg[0];
    let (_, crit_avg) = out.stored_crit_avg[0];
    // 非暴击腿不吃该词条：150；暴击腿：150×2（inc）×3（爆伤）= 900。
    assert!(
        (hit_avg - 150.0).abs() < 1e-9,
        "非暴击腿不得吃 on-crit 词条"
    );
    assert!(
        (crit_avg - 900.0).abs() < 1e-9,
        "暴击腿吃 on-crit 词条 + 爆伤"
    );

    // 真双腿 blend（:4395）：150×0.6 + 900×0.4 = 450。
    assert!(
        (out.total_hit_avg - 450.0).abs() < 1e-9,
        "blend got {}",
        out.total_hit_avg
    );
    // 对照：单因子口径会给 150×2.5(=300+150 错误折算)…该场景下两口径必然不同，
    // 旧公式 = 150 × 1.8 = 270 ≠ 450。
    assert!((out.total_hit_avg - 270.0).abs() > 1.0);
}

/// 负向词条（仅在**非**暴击时生效，negated 条件）同样按腿路由。
#[test]
fn negated_crit_condition_routes_to_non_crit_leg() {
    let mut db = crit_db();
    db.add_mod(
        Modifier::number("PhysicalDamage", ModType::Inc, 100.0)
            .with_tag(ModTag::condition("CriticalStrike", true)), // negated：非暴击时生效
    );
    let cfg = CalcConfig::attack();
    let out = calculate_minimal(&db, &cfg, &input());

    let (_, hit_avg) = out.stored_hit_avg[0];
    let (_, crit_avg) = out.stored_crit_avg[0];
    assert!((hit_avg - 300.0).abs() < 1e-9, "非暴击腿吃 negated 词条");
    assert!((crit_avg - 450.0).abs() < 1e-9, "暴击腿不吃（仅爆伤 ×3）");
    // blend：300×0.6 + 450×0.4 = 360。
    assert!((out.total_hit_avg - 360.0).abs() < 1e-9);
}

/// 无暴击词条的 build（c=0）：双 pass 不改变任何输出（effect=1）。
#[test]
fn zero_crit_chance_is_neutral() {
    let db = ModDb::new();
    let cfg = CalcConfig::attack();
    let out = calculate_minimal(&db, &cfg, &input());
    assert!((out.total_hit_avg - 150.0).abs() < 1e-9);
    let (_, combined) = out.stored_combined_avg[0];
    assert!((combined - 150.0).abs() < 1e-9);
}
