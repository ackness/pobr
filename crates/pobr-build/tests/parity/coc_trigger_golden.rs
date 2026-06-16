//! CoC（Cast on Critical Strike）触发链路 golden fixture（M4-I，蓝图 §4.2 验收条目 2）。
//!
//! T5 已交付方向性断言（calc_orchestrator 单测：识别命中 / 攻速→触发速率单调）；
//! 本套件补**固定数值 golden**：PoB2 XML fixture（`fixtures/coc_cast_on_crit.xml`：
//! Wooden Club + 攻击源 Armour Breaker + MetaCastOnCritPlayer 触发器 + 被触发
//! Fireball 同组，主技能 = Fireball）经生产解析器 `parse_build` →
//! `calculate_with_data` 端到端，锁定触发面板与源统计链路的当前值，防止未来
//! 重构静默改变触发输出。
//!
//! 链路对照（vendor `Modules/CalcTriggers.lua`）：
//! - 识别：`overlay/trigger_configs.json` 的 `MetaCastOnCritPlayer` 条目
//!   （`triggered_by`，`trigger_on_crit = true`，源谓词 = Attack；:1089-1092）；
//! - 源速率：W-E2 子计算取源技能计算后有效速率（GlobalCache 等价，:74-86）；
//! - 触发几率：源命中 × 源暴击折入（trigger_on_crit，:716-770）；CoC 条目无
//!   冷却覆盖、触发宝石无冷却数据 → `trigger_rate_cap` 面板保持 0（无冷却 cap），
//!   触发速率 = 源速率 × 源命中 × 源暴击（纯源速率驱动分支）。
//!
//! **vendor oracle 对拍结论（2026-06，vendor 0.18 检出）**：`tools/pob2-oracle`
//! 跑同 fixture **无法产出 TriggerRate**——vendor 自身 CoC 链路断裂：
//! `mainSkill.triggeredBy.grantedEffect.name = "SupportMetaCastOnCritPlayer"`，
//! 在 configTable 按名查键（`CalcTriggers.lua:1452-1455`）匹配不到
//! `"cast on critical strike"`（PoE1 旧名键），trigger handler 不运行、
//! `skillData.triggered` 被清空（= DeepWiki「needs an entire overhaul」已知态）。
//! 可对拍的**中间值**（源技能口径，oracle mainActiveSkill=1）已核对：
//! - 源速率 vendor `Speed = 1.16`（2 位显示舍入）vs PoBR `1.15942029` ✓；
//! - 源基础暴击 vendor `PreEffectiveCritChance = 5` vs PoBR `0.05` ✓；
//! - 源命中 vendor `77%` vs PoBR `74.2558559%`——既有 accuracy 公式 parity 缺口
//!   （ninja_parity 已跟踪），非触发链路偏差。
//!
//! 基线更新：行为变更 commit 改变下列 `GOLDEN_*` 时，按失败信息中的实际值更新
//! 常量（与 `golden_regression.rs` 同约定）。

use pobr_build::{Build, BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};
use pobr_gamedata::{GameData, repo_data_root};

/// CoC build fixture：Weapon 1 = Wooden Club（源攻击的武器基底），无天赋；组 =
/// [Armour Breaker(攻击源), Cast on Critical(触发器), Fireball(被触发)]，
/// 主技能 = Fireball（mainActiveSkill=3，组内非 support 序）。
const COC_XML: &str = include_str!("../fixtures/coc_cast_on_crit.xml");

// ---------------------------------------------------------------------------
// Golden 基线（首次建立：本套件落地 commit，worktree 基线 761ebb5）
// ---------------------------------------------------------------------------

/// 实际触发速率（次/秒）= 源速率 × 源命中 × 源暴击（trigger_on_crit 折入）：
/// `1.15942029 × 0.742558559 × 0.05`。
const GOLDEN_SKILL_TRIGGER_RATE: f64 = 0.043046873;
/// 触发速率上限：CoC 无冷却数据 → 面板保持 0。
const GOLDEN_TRIGGER_RATE_CAP: f64 = 0.0;
/// 被触发 Fireball DPS（当前 = hit_avg × 自身 cast rate；触发速率→DPS 接线
/// 落地时此基线随行为 commit 更新）。
const GOLDEN_DPS: f64 = 65.537499974;
/// 被触发 Fireball 主技能口径。
const GOLDEN_MAIN_CRIT_CHANCE: f64 = 0.07;
const GOLDEN_MAIN_HIT_CHANCE: f64 = 1.0;
const GOLDEN_MAIN_ACTION_RATE: f64 = 0.833333333;
const GOLDEN_MAIN_TOTAL_HIT_AVG: f64 = 78.645;

/// 源技能（Armour Breaker @ Wooden Club）口径：W-E2 子计算注入
/// `TriggerSourceRate`/`TriggerSourceHitChance`/`TriggerSourceCritChance` 的来源。
const GOLDEN_SRC_EFFECTIVE_ACTION_RATE: f64 = 1.15942029;
const GOLDEN_SRC_HIT_CHANCE: f64 = 0.742558559;
const GOLDEN_SRC_CRIT_CHANCE: f64 = 0.05;

/// 浮点字段相对容差（golden_regression.rs 同值）。
const RELATIVE_TOL: f64 = 1e-6;

fn assert_near_float(label: &str, expected: f64, actual: f64) {
    if expected == 0.0 {
        assert!(
            actual.abs() < RELATIVE_TOL,
            "{label}: expected 0, got {actual}"
        );
        return;
    }
    let relative = ((actual - expected) / expected).abs();
    assert!(
        relative < RELATIVE_TOL,
        "{label}: expected {expected}, got {actual}, relative error {relative} exceeds {RELATIVE_TOL}"
    );
}

fn load_build_data() -> BuildData {
    let data = GameData::new(repo_data_root().join(pobr_gamedata::data_version()));
    BuildData::load(&data).expect("load BuildData from repo data")
}

fn parse_coc_build() -> Build {
    parse_build(COC_XML).expect("parse coc fixture")
}

/// 端到端 golden：fixture XML → 生产解析器 → 数据编排计算，锁定触发面板 +
/// 被触发主技能输出。
#[test]
fn coc_golden_triggered_skill_output() {
    let data = load_build_data();
    let build = parse_coc_build();
    let out = calculate_with_data(&build, &data, &DataOrchestratorOptions::default())
        .expect("coc calculate");

    assert_near_float(
        "skill_trigger_rate",
        GOLDEN_SKILL_TRIGGER_RATE,
        out.skill_trigger_rate,
    );
    assert_near_float(
        "trigger_rate_cap",
        GOLDEN_TRIGGER_RATE_CAP,
        out.trigger_rate_cap,
    );
    assert_near_float("dps", GOLDEN_DPS, out.dps);
    assert_near_float("crit_chance", GOLDEN_MAIN_CRIT_CHANCE, out.crit_chance);
    assert_near_float("hit_chance", GOLDEN_MAIN_HIT_CHANCE, out.hit_chance);
    assert_near_float("action_rate", GOLDEN_MAIN_ACTION_RATE, out.action_rate);
    assert_near_float(
        "total_hit_avg",
        GOLDEN_MAIN_TOTAL_HIT_AVG,
        out.total_hit_avg,
    );
}

/// 源技能 golden（TriggerRate 的中间值）：同组主技能切到 Armour Breaker，即
/// W-E2 源子计算的统计口径（vendor oracle 对拍点：Speed 1.16 / PreEffectiveCrit 5%）。
#[test]
fn coc_golden_trigger_source_stats() {
    let data = load_build_data();
    let mut build = parse_coc_build();
    build.socket_groups[0].main_active_skill = Some(1);
    let out = calculate_with_data(&build, &data, &DataOrchestratorOptions::default())
        .expect("source calculate");

    assert_near_float(
        "src effective_action_rate",
        GOLDEN_SRC_EFFECTIVE_ACTION_RATE,
        out.effective_action_rate,
    );
    assert_near_float("src hit_chance", GOLDEN_SRC_HIT_CHANCE, out.hit_chance);
    assert_near_float("src crit_chance", GOLDEN_SRC_CRIT_CHANCE, out.crit_chance);
}

/// 触发链路恒等式：`skill_trigger_rate = 源速率 × 源命中 × 源暴击`
/// （CoC 纯源速率驱动分支 + trigger_on_crit 折入，perform `fill_trigger`）。
/// golden 常量间的结构关系——任一环节静默改动都会同时打破本断言与上面两套 golden。
#[test]
fn coc_trigger_rate_is_source_chain_product() {
    let expected =
        GOLDEN_SRC_EFFECTIVE_ACTION_RATE * GOLDEN_SRC_HIT_CHANCE * GOLDEN_SRC_CRIT_CHANCE;
    // fill_trigger 末端 round(·, 9 位内部精度)：golden 常量本身即 round 后值。
    assert!(
        (expected - GOLDEN_SKILL_TRIGGER_RATE).abs() < 1e-8,
        "trigger chain product {expected} != golden {GOLDEN_SKILL_TRIGGER_RATE}"
    );
}

/// 确定性：两次计算输出完全一致（触发子计算不引入非确定性）。
#[test]
fn coc_golden_is_deterministic() {
    let data = load_build_data();
    let build = parse_coc_build();
    let opts = DataOrchestratorOptions::default();
    let out1 = calculate_with_data(&build, &data, &opts).expect("first calc");
    let out2 = calculate_with_data(&build, &data, &opts).expect("second calc");
    assert_eq!(
        out1.skill_trigger_rate, out2.skill_trigger_rate,
        "skill_trigger_rate non-deterministic"
    );
    assert_eq!(out1.dps, out2.dps, "dps non-deterministic");
    assert_eq!(
        out1.trigger_rate_cap, out2.trigger_rate_cap,
        "trigger_rate_cap non-deterministic"
    );
}
