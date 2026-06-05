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

// ---------------------------------------------------------------------------
// Wave 1 / Wave 2 新增字段快照（空 Build + 空 MinimalInput → 全为默认中性值）
//
// 目的：锁定 Wave1/2 新增 OutputTable 字段在默认（无词条）状态下的基准值，
// 防止这些字段在重构过程中被静默改写。
// 基线值均来自 OutputTable::default() 中性规则：
//   - 大多数字段 0.0（无词条时无效果）
//   - taken_multi_* = 1.0（乘数中性，不减伤也不增伤）
//   - enemy_crit_effect = 1.0（中性）
// ---------------------------------------------------------------------------

/// Wave 1 暴击字段快照（crit_chance / crit_multiplier / pre_effective_crit_chance）。
///
/// 说明：crit_multiplier 基值 = 1 + PLAYER_BASE_CRIT_DAMAGE_BONUS/100 = 2.0（PoE2 口径）；
/// 无额外增伤词条时维持 2.0。crit_chance 无词条 → 0.0（无暴击 base）。
const DEADEYE_GOLDEN_CRIT_MULTIPLIER: f64 = 2.0;

#[test]
fn deadeye_golden_wave1_crit_fields() {
    let build = build_from_code(DEADEYE_CODE);
    let out = calculate(&build, &default_opts()).expect("calc");

    // 暴击率：无暴击 base 词条 → 0.0。
    assert_near_float("crit_chance", 0.0, out.crit_chance);
    // 暴击加成：PoE2 基础值 2.0（PLAYER_BASE_CRIT_DAMAGE_BONUS = 100%）。
    assert_near_float(
        "crit_multiplier",
        DEADEYE_GOLDEN_CRIT_MULTIPLIER,
        out.crit_multiplier,
    );
    // 未应用 mode_effective 前暴击率：无 base → 0.0。
    assert_near_float(
        "pre_effective_crit_chance",
        0.0,
        out.pre_effective_crit_chance,
    );
}

/// Wave 2 防御扩展字段快照：ES 充能 / 规避 / 承受乘数 / 暴击减免。
#[test]
fn deadeye_golden_wave2_defence_extension_fields() {
    let build = build_from_code(DEADEYE_CODE);
    let out = calculate(&build, &default_opts()).expect("calc");

    // ES 充能：无词条 → 充能速率 0，延迟 0，每秒 0。
    assert_near_float("es_recharge_rate", 0.0, out.es_recharge_rate);
    assert_near_float("es_recharge_delay", 0.0, out.es_recharge_delay);
    assert_near_float("es_recharge_per_second", 0.0, out.es_recharge_per_second);

    // 规避：无词条 → 全 0。
    assert_near_float(
        "avoid_all_damage_from_hits",
        0.0,
        out.avoid_all_damage_from_hits,
    );
    assert_near_float("avoid_ignite", 0.0, out.avoid_ignite);
    assert_near_float("avoid_shock", 0.0, out.avoid_shock);
    assert_near_float("avoid_chill", 0.0, out.avoid_chill);
    assert_near_float("avoid_freeze", 0.0, out.avoid_freeze);
    assert_near_float("avoid_poison", 0.0, out.avoid_poison);
    assert_near_float("avoid_bleeding", 0.0, out.avoid_bleeding);
    assert_near_float("avoid_stun", 0.0, out.avoid_stun);

    // 承受乘数中性：默认 1.0。
    assert_near_float("taken_multi_physical", 1.0, out.taken_multi_physical);
    assert_near_float("taken_multi_fire", 1.0, out.taken_multi_fire);
    assert_near_float("taken_multi_cold", 1.0, out.taken_multi_cold);
    assert_near_float("taken_multi_lightning", 1.0, out.taken_multi_lightning);
    assert_near_float("taken_multi_chaos", 1.0, out.taken_multi_chaos);

    // 暴击额外减免 / 敌方暴击效果（中性）。
    assert_near_float(
        "crit_extra_damage_reduction",
        0.0,
        out.crit_extra_damage_reduction,
    );
    assert_near_float("enemy_crit_effect", 1.0, out.enemy_crit_effect);
}

/// Wave 2 充能 / 偷取 / Recoup 快照（无词条 → 全 0）。
#[test]
fn deadeye_golden_wave2_charges_leech_recoup_fields() {
    let build = build_from_code(DEADEYE_CODE);
    let out = calculate(&build, &default_opts()).expect("calc");

    // 充能：无词条 → 当前/最大均 0。
    assert_eq!(out.charge_power_current, 0, "charge_power_current");
    assert_eq!(out.charge_power_maximum, 0, "charge_power_maximum");
    assert_eq!(out.charge_frenzy_current, 0, "charge_frenzy_current");
    assert_eq!(out.charge_frenzy_maximum, 0, "charge_frenzy_maximum");
    assert_eq!(out.charge_endurance_current, 0, "charge_endurance_current");
    assert_eq!(out.charge_endurance_maximum, 0, "charge_endurance_maximum");

    // 偷取速率：无词条 → 0。
    assert_near_float("life_leech_rate", 0.0, out.life_leech_rate);
    assert_near_float("mana_leech_rate", 0.0, out.mana_leech_rate);
    assert_near_float("es_leech_rate", 0.0, out.es_leech_rate);

    // Recoup：无词条 → 0。
    assert_near_float("life_recoup_rate", 0.0, out.life_recoup_rate);
    assert_near_float("es_recoup_rate", 0.0, out.es_recoup_rate);
}

/// Wave 2 异常扩展快照（无词条 → 全 0）。
#[test]
fn deadeye_golden_wave2_ailment_extension_fields() {
    let build = build_from_code(DEADEYE_CODE);
    let out = calculate(&build, &default_opts()).expect("calc");

    assert_near_float("chill_effect", 0.0, out.chill_effect);
    assert_near_float("freeze_buildup_pct", 0.0, out.freeze_buildup_pct);
    assert_near_float("electrocute_buildup_pct", 0.0, out.electrocute_buildup_pct);

    // 叠层 DPS / 活跃层数：无命中词条 → 0。
    assert_near_float("bleed_stacked_dps", 0.0, out.bleed_stacked_dps);
    assert_near_float("bleed_active_stacks", 0.0, out.bleed_active_stacks);
    assert_near_float("poison_stacked_dps", 0.0, out.poison_stacked_dps);
    assert_near_float("poison_active_stacks", 0.0, out.poison_active_stacks);
}

/// Wave 2 技能功能 / 触发快照（无 base 配置 → 全 0）。
#[test]
fn deadeye_golden_wave2_skill_mechanics_and_trigger_fields() {
    let build = build_from_code(DEADEYE_CODE);
    let out = calculate(&build, &default_opts()).expect("calc");

    // 技能功能：无 base 词条 → 0。
    assert_near_float("aoe_radius", 0.0, out.aoe_radius);
    assert_near_float("aoe_area_mod", 0.0, out.aoe_area_mod);
    assert_near_float("projectile_count", 0.0, out.projectile_count);
    assert_near_float("cooldown", 0.0, out.cooldown);
    assert_eq!(out.cooldown_stored_uses, 0, "cooldown_stored_uses");
    assert_near_float("mana_cost", 0.0, out.mana_cost);
    assert_near_float("life_cost", 0.0, out.life_cost);
    assert_near_float("spirit_reserved", 0.0, out.spirit_reserved);

    // 触发：无触发词条 → 0。
    assert_near_float("trigger_rate_cap", 0.0, out.trigger_rate_cap);
    assert_near_float("skill_trigger_rate", 0.0, out.skill_trigger_rate);
}

/// Wave 1 异常 DPS 字段快照（空 Build → 全 0 期望）。
#[test]
fn deadeye_golden_wave1_ailment_dps_fields() {
    let build = build_from_code(DEADEYE_CODE);
    let out = calculate(&build, &default_opts()).expect("calc");

    assert_near_float("bleed_dps", 0.0, out.bleed_dps);
    assert_near_float("ignite_dps", 0.0, out.ignite_dps);
    assert_near_float("poison_dps", 0.0, out.poison_dps);
    assert_near_float("shock_effect", 0.0, out.shock_effect);
}

/// Wave 1 行动速率字段快照。
///
/// 说明：hit_chance 在 base_accuracy=0 + enemy_evasion=0 时命中率公式退化为 1.0（100%）。
/// action_rate / effective_action_rate 无 base 词条 → 0.0。
const DEADEYE_GOLDEN_HIT_CHANCE: f64 = 1.0;

#[test]
fn deadeye_golden_wave1_action_rate_fields() {
    let build = build_from_code(DEADEYE_CODE);
    let out = calculate(&build, &default_opts()).expect("calc");

    assert_near_float("action_rate", 0.0, out.action_rate);
    assert_near_float("effective_action_rate", 0.0, out.effective_action_rate);
    // hit_chance: base_accuracy=0 + enemy_evasion=0 → formula returns 1.0（100%）。
    assert_near_float("hit_chance", DEADEYE_GOLDEN_HIT_CHANCE, out.hit_chance);
}

/// Wave 2 ES 词条注入：验证 ES 词条被正确解析并注入计算管线。
///
/// 注意：当前 CalcOrchestrator 通过 MinimalOutput → OutputTable 路径返回结果，
/// 该路径只保留 MinimalOutput 的核心字段（life/mana/resistance/crit/hit），
/// energy_shield / es_recharge 等防御扩展字段在此路径中回退到 OutputTable::default()。
/// 本测试验证：管线不崩溃 + life 注入路径正常 + 默认字段符合预期。
/// 完整 ES 计算的单元测试在 pobr-core 层（defense.rs / session.rs）覆盖。
#[test]
fn deadeye_golden_wave2_es_modifier_pipeline_ok() {
    let build = build_from_code(DEADEYE_CODE);
    let opts = OrchestratorOptions {
        base_input: MinimalInput {
            base_life: 500.0,
            ..MinimalInput::default()
        },
        extra_modifier_texts: vec![
            "+500 to maximum Energy Shield".to_string(),
            "+1000 to maximum Life".to_string(),
        ],
    };
    // 管线不崩溃。
    let out = calculate(&build, &opts).expect("calc with es modifier should not panic");

    // life = base(500) + modifier(+1000) = 1500（MinimalOutput 路径正常保留）。
    assert_near_int("life_with_modifiers", 1500.0, out.life);

    // 当前 orchestrator 路径 energy_shield 回退到 OutputTable::default() = 0.0。
    // 此断言作为回归锁，防止静默改写（若将来 orchestrator 改为全字段返回，
    // 此断言应更新为 500.0）。
    assert_near_int(
        "energy_shield_via_orchestrator_path",
        0.0,
        out.energy_shield,
    );
}

/// Wave 1/2 抗性 + 防御字段整体确定性（两次调用一致）。
///
/// 只注入已知可解析词条；避免引入 ParseError（不支持的词条形式会导致 Err 返回）。
#[test]
fn deadeye_golden_wave2_all_fields_deterministic() {
    let build = build_from_code(DEADEYE_CODE);
    let opts = OrchestratorOptions {
        base_input: MinimalInput {
            base_life: 1000.0,
            ..MinimalInput::default()
        },
        extra_modifier_texts: vec![
            "+300 to maximum Energy Shield".to_string(),
            "+50% to fire resistance".to_string(),
        ],
    };

    let out1 = calculate(&build, &opts).expect("first calc");
    let out2 = calculate(&build, &opts).expect("second calc");

    // 核心防御字段。
    assert_eq!(out1.life, out2.life, "life non-deterministic");
    assert_eq!(
        out1.fire_resistance, out2.fire_resistance,
        "fire_resistance"
    );
    assert_eq!(
        out1.es_recharge_per_second, out2.es_recharge_per_second,
        "es_recharge_per_second"
    );
    // Wave 2 充能/异常/技能字段。
    assert_eq!(
        out1.charge_power_maximum, out2.charge_power_maximum,
        "charge_power_max"
    );
    assert_eq!(out1.chill_effect, out2.chill_effect, "chill_effect");
    assert_eq!(out1.aoe_radius, out2.aoe_radius, "aoe_radius");
    assert_eq!(
        out1.trigger_rate_cap, out2.trigger_rate_cap,
        "trigger_rate_cap"
    );
    assert_eq!(
        out1.bleed_stacked_dps, out2.bleed_stacked_dps,
        "bleed_stacked_dps"
    );
}
