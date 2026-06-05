//! Golden 回归 harness：以真实 PoB2 ninja build code 为输入，锁定当前计算输出的关键字段。
//!
//! 目标：防止未来重构/机制修改静默改变已有的计算结果。不要求与真实 PoB2 数值逐字对齐
//! （那需要用同一 build 跑 PoB2 并对比，留后续）；只要求 PoBR 自身每次运行结果一致。
//!
//! 容差规则：
//! - 整数型字段（life / mana 等无小数的基础属性）：`delta.abs() < 0.5`（舍入容差）。
//! - 浮点型字段（crit_chance / dps 等）：`relative < 1e-6`（相对误差百万分之一）。
//!
//! 首次建立基线：运行 `cargo test -p pobr-build -- golden` 并以输出的实际值更新
//! `GOLDEN_*` 常量（或直接 `BLESS=1 cargo test …` 重新生成，见注释）。

use pobr_build::{
    Build, CharacterIdentity, OrchestratorOptions, calculate, decode_pob_code, parse_build_header,
};
use pobr_core::calc::MinimalInput;

/// 真实 PoB2 ninja Deadeye build code。
const DEADEYE_CODE: &str = include_str!("../../../examples/demo-bd-test/ninja-bd-deadeye.txt");

/// 真实 PoB2 ninja Martial Artist build code。
const MARTIAL_ARTIST_CODE: &str =
    include_str!("../../../examples/demo-bd-test/ninja-bd-marial-artist.txt");

// ---------------------------------------------------------------------------
// 容差工具
// ---------------------------------------------------------------------------

/// 整数字段容差：|delta| < 0.5（允许舍入偏差）。
const INTEGER_TOL: f64 = 0.5;
/// 浮点字段相对容差：1e-6（百万分之一）。
const RELATIVE_TOL: f64 = 1e-6;

fn assert_near_int(label: &str, expected: f64, actual: f64) {
    let delta = (actual - expected).abs();
    assert!(
        delta < INTEGER_TOL,
        "{label}: expected {expected}, got {actual}, delta {delta} exceeds integer tolerance {INTEGER_TOL}"
    );
}

fn assert_near_float(label: &str, expected: f64, actual: f64) {
    // 若期望值为 0，回退整数容差。
    if expected.abs() < f64::EPSILON {
        let delta = actual.abs();
        assert!(
            delta < INTEGER_TOL,
            "{label}: expected ~0, got {actual}, exceeds tolerance"
        );
        return;
    }
    let relative = (actual - expected).abs() / expected.abs();
    assert!(
        relative < RELATIVE_TOL,
        "{label}: expected {expected:.6}, got {actual:.6}, relative error {relative:.2e} exceeds {RELATIVE_TOL:.2e}"
    );
}

// ---------------------------------------------------------------------------
// 从 build code 构造 Build（最小路径）
// ---------------------------------------------------------------------------

fn build_from_code(code: &str) -> Build {
    let xml = decode_pob_code(code.trim()).expect("decode build code");
    let header = parse_build_header(&xml).expect("parse build header");

    Build::new()
        .with_character(CharacterIdentity {
            level: header.identity.level,
            class_name: header.identity.class_name.clone(),
            ascendancy_name: header.identity.ascendancy_name.clone(),
        })
        .with_game_version(pobr_data::build_config::GameVersion::Poe2)
}

fn default_opts() -> OrchestratorOptions {
    OrchestratorOptions {
        base_input: MinimalInput::default(),
        extra_modifier_texts: vec![],
    }
}

// ---------------------------------------------------------------------------
// Deadeye golden 基线
//
// 基线使用全零 MinimalInput（无装备/天赋词条注入；calc_orchestrator 只收集
// build.equipped_items() 的词条，本测试 Build 无装备，故输出为基础默认值）。
// 这样可稳定测试编解码 → Build 构造 → CalcOrchestrator 的端对端管线，
// 同时不依赖外部数据文件。
//
// 若将来添加装备/天赋词条注入，在此更新基线。
// ---------------------------------------------------------------------------

/// Deadeye build 解码后的角色等级期望值（来自 XML `<Build level="...">`）。
const DEADEYE_EXPECTED_LEVEL: u32 = 98;

/// Deadeye build 解码后的职业名期望值。
const DEADEYE_EXPECTED_CLASS: &str = "Ranger";

/// 空 Build（无装备）+ 空 MinimalInput → 计算基线：全零/默认防御。
const DEADEYE_GOLDEN_LIFE: f64 = 0.0;
const DEADEYE_GOLDEN_MANA: f64 = 0.0;
const DEADEYE_GOLDEN_FIRE_RES: f64 = 0.0;
const DEADEYE_GOLDEN_COLD_RES: f64 = 0.0;
const DEADEYE_GOLDEN_LIGHTNING_RES: f64 = 0.0;
const DEADEYE_GOLDEN_DPS: f64 = 0.0;

// ---------------------------------------------------------------------------
// 测试：Deadeye build 端对端 golden
// ---------------------------------------------------------------------------

#[test]
fn deadeye_golden_decode_and_identity() {
    // Stage 1：build code → XML → header 解析。
    let xml = decode_pob_code(DEADEYE_CODE.trim()).expect("decode");
    let header = parse_build_header(&xml).expect("parse header");

    assert_eq!(
        header.identity.level, DEADEYE_EXPECTED_LEVEL,
        "Deadeye level mismatch: XML level changed or wrong fixture"
    );
    assert_eq!(
        header.identity.class_name, DEADEYE_EXPECTED_CLASS,
        "Deadeye class mismatch"
    );
    assert!(
        header.identity.ascendancy_name.contains("Deadeye"),
        "expected Deadeye ascendancy, got: {}",
        header.identity.ascendancy_name
    );
}

#[test]
fn deadeye_golden_calc_baseline() {
    // Stage 2：Build 构造 + CalcOrchestrator → OutputTable 基线。
    // 无装备 / 无额外 modifier，输出应等于空 MinimalInput 默认值。
    let build = build_from_code(DEADEYE_CODE);
    let opts = default_opts();
    let out = calculate(&build, &opts).expect("calculate");

    assert_near_int("life", DEADEYE_GOLDEN_LIFE, out.life);
    assert_near_int("mana", DEADEYE_GOLDEN_MANA, out.mana);
    assert_near_float(
        "fire_resistance",
        DEADEYE_GOLDEN_FIRE_RES,
        out.fire_resistance,
    );
    assert_near_float(
        "cold_resistance",
        DEADEYE_GOLDEN_COLD_RES,
        out.cold_resistance,
    );
    assert_near_float(
        "lightning_resistance",
        DEADEYE_GOLDEN_LIGHTNING_RES,
        out.lightning_resistance,
    );
    assert_near_float("dps", DEADEYE_GOLDEN_DPS, out.dps);
}

#[test]
fn deadeye_golden_calc_with_life_modifier() {
    // Stage 3：注入已知词条，验证词条影响被正确纳入计算。
    // 注入 "+1000 to maximum Life" → 期望 life 增加 1000。
    let build = build_from_code(DEADEYE_CODE);
    let opts = OrchestratorOptions {
        base_input: MinimalInput {
            base_life: 500.0,
            ..MinimalInput::default()
        },
        extra_modifier_texts: vec!["+1000 to maximum Life".to_string()],
    };
    let out = calculate(&build, &opts).expect("calculate with modifier");

    // base_life 500 + modifier +1000 = 1500
    assert_near_int("life_with_modifier", 1500.0, out.life);
}

#[test]
fn deadeye_golden_snapshot_is_deterministic() {
    // Stage 4：两次相同调用结果完全一致（确定性保证）。
    let build = build_from_code(DEADEYE_CODE);
    let opts = default_opts();

    let out1 = calculate(&build, &opts).expect("first calc");
    let out2 = calculate(&build, &opts).expect("second calc");

    assert_eq!(out1.life, out2.life, "life non-deterministic");
    assert_eq!(out1.dps, out2.dps, "dps non-deterministic");
    assert_eq!(
        out1.fire_resistance, out2.fire_resistance,
        "fire_res non-deterministic"
    );
}

// ---------------------------------------------------------------------------
// Martial Artist golden 基线（第二个 fixture，验证非 Ranger 职业解码正常）
// ---------------------------------------------------------------------------

#[test]
fn martial_artist_golden_decode_and_calc() {
    let xml = decode_pob_code(MARTIAL_ARTIST_CODE.trim()).expect("decode martial artist code");
    let header = parse_build_header(&xml).expect("parse header");

    // 确认是 PathOfBuilding2 文档。
    assert_eq!(header.pob_major, 2, "expected PoE2 build");
    // level 应在合理范围。
    assert!(
        header.identity.level > 0 && header.identity.level <= 100,
        "level out of range: {}",
        header.identity.level
    );

    // Build + CalcOrchestrator 不报错。
    let build = Build::new()
        .with_character(CharacterIdentity {
            level: header.identity.level,
            class_name: header.identity.class_name.clone(),
            ascendancy_name: header.identity.ascendancy_name.clone(),
        })
        .with_game_version(pobr_data::build_config::GameVersion::Poe2);

    let out = calculate(&build, &default_opts()).expect("martial artist calc");
    // 空 Build 无词条，输出全零（默认）。
    assert_near_int("martial_artist_life", 0.0, out.life);
}

// ---------------------------------------------------------------------------
// 回归守卫：确保 decode → Build → Calc 管线不因重构而静默崩溃
// ---------------------------------------------------------------------------

#[test]
fn pipeline_smoke_test_both_fixtures() {
    for (name, code) in [
        ("deadeye", DEADEYE_CODE),
        ("martial_artist", MARTIAL_ARTIST_CODE),
    ] {
        let xml =
            decode_pob_code(code.trim()).unwrap_or_else(|e| panic!("{name}: decode failed: {e}"));
        let header = parse_build_header(&xml)
            .unwrap_or_else(|e| panic!("{name}: parse_build_header failed: {e}"));
        let build = Build::new()
            .with_character(CharacterIdentity {
                level: header.identity.level,
                class_name: header.identity.class_name.clone(),
                ascendancy_name: header.identity.ascendancy_name.clone(),
            })
            .with_game_version(pobr_data::build_config::GameVersion::Poe2);
        calculate(&build, &default_opts())
            .unwrap_or_else(|e| panic!("{name}: calculate failed: {e}"));
    }
}
