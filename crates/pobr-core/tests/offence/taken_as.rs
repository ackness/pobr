//! M2 Track B（13-G1 / 13-G7）：taken-as 管线 + effectiveAppliedArmour 集成测试。
//!
//! 期望值均按 PoB2 公式手算，注释标注 `Modules/CalcDefence.lua` 行号
//! （蓝图 m2-defence §4.1 第 6 条惯例）。词条文本走 mod_parser（W0.1 覆盖表契约），
//! 无解析来源的中间量（如 ArmourDefense）直接注入 Modifier。

use crate::support::parse_mod;
use pobr_core::calc::{
    MitigationCtx, MitigationInputs, armour_applies_pct, build_mitigation_ctx, damage_shift_table,
    effective_applied_armour, taken_hit_from_damage,
};
use pobr_core::mod_parser::ParseStatus;
use pobr_core::{CalcConfig, ModDb, Modifier};
use pobr_data::prelude::*;

const PHYS: usize = DamageType::Physical as usize;
const FIRE: usize = DamageType::Fire as usize;
const COLD: usize = DamageType::Cold as usize;
const LIGHT: usize = DamageType::Lightning as usize;
const CHAOS: usize = DamageType::Chaos as usize;

/// 在中性 ctx 上做局部覆写（绕开 clippy field_reassign_with_default 的写法约束）。
fn ctx_with(adjust: impl FnOnce(&mut MitigationCtx)) -> MitigationCtx {
    let mut ctx = MitigationCtx::default();
    adjust(&mut ctx);
    ctx
}

/// 解析词条文本并注入 db（W0.1 解析表即契约）。
fn add_text(db: &mut ModDb, text: &str) {
    let outcome = parse_mod(text).unwrap_or_else(|e| panic!("parse {text:?}: {e}"));
    assert_eq!(
        outcome.status,
        ParseStatus::Parsed,
        "unsupported modifier in fixture: {text:?}"
    );
    db.add_list(outcome.mods);
}

// ─────────────────────────────────────────────────────────────────
// damage_shift_table（CalcDefence.lua:2171-2190）
// ─────────────────────────────────────────────────────────────────

/// 无词条 → 恒等矩阵（每类型 100% 保留自身）。
#[test]
fn shift_table_defaults_to_identity() {
    let db = ModDb::new();
    let shift = damage_shift_table(&db, &CalcConfig::attack());
    for (s, row) in shift.iter().enumerate() {
        for (d, value) in row.iter().enumerate() {
            let expected = if s == d { 1.0 } else { 0.0 };
            assert_eq!(*value, expected, "shift[{s}][{d}]");
        }
    }
}

/// 「30% of Cold Damage taken as Lightning」→ 冰系 30% 转电、70% 保留
/// （:2184 BASE 求和、:2189 源保留 max(100−total,0)）。
#[test]
fn shift_table_single_conversion() {
    // 裸目标 taken-as 经 special_mods `cold_damage_taken_as_lightning`
    // （vendor ModParser.lua:5655）整行解析——端到端走文本通道。
    let mut db = ModDb::new();
    add_text(&mut db, "30% of Cold Damage taken as Lightning");
    let shift = damage_shift_table(&db, &CalcConfig::attack());
    assert_eq!(shift[COLD][LIGHT], 0.3);
    assert_eq!(shift[COLD][COLD], 0.7);
    assert_eq!(shift[COLD][FIRE], 0.0);
    // 其它源类型不受影响。
    assert_eq!(shift[PHYS][PHYS], 1.0);
}

/// 总转出 >100%：源截断到 0，目标份额**不归一化**（vendor 同语义——
/// :2189 仅对源保留取 max(100−total,0)，目标各自保持 BASE 求和值）。
#[test]
fn shift_table_over_100_truncates_source_only() {
    let mut db = ModDb::new();
    add_text(&mut db, "60% of Physical Damage taken as Fire Damage");
    add_text(&mut db, "60% of Physical Damage taken as Lightning Damage");
    let shift = damage_shift_table(&db, &CalcConfig::attack());
    assert_eq!(shift[PHYS][PHYS], 0.0, "源保留 max(1-1.2, 0) = 0");
    assert_eq!(shift[PHYS][FIRE], 0.6);
    assert_eq!(shift[PHYS][LIGHT], 0.6);
}

/// from-hits 变体与普通变体同入 hit 口径 shiftTable（:2184 两族求和）。
#[test]
fn shift_table_sums_hits_variant() {
    let mut db = ModDb::new();
    add_text(&mut db, "20% of Physical Damage taken as Fire Damage");
    add_text(
        &mut db,
        "30% of Physical Damage from Hits taken as Fire Damage",
    );
    let shift = damage_shift_table(&db, &CalcConfig::attack());
    assert_eq!(shift[PHYS][FIRE], 0.5);
    assert_eq!(shift[PHYS][PHYS], 0.5);
}

// ─────────────────────────────────────────────────────────────────
// effectiveAppliedArmour（CalcDefence.lua:2336-2362、:1862-1863）
// ─────────────────────────────────────────────────────────────────

/// 物理隐式 BASE 100（:1862-1863）：空 db 时物理吃全额护甲、元素不吃。
#[test]
fn effective_armour_physical_implicit_100() {
    let db = ModDb::new();
    let cfg = CalcConfig::attack();
    assert_eq!(armour_applies_pct(&db, &cfg, DamageType::Physical), 100.0);
    assert_eq!(armour_applies_pct(&db, &cfg, DamageType::Fire), 0.0);
    assert_eq!(
        effective_applied_armour(&db, &cfg, 1000.0, 0.0, 0.0, DamageType::Physical),
        1000.0
    );
    assert_eq!(
        effective_applied_armour(&db, &cfg, 1000.0, 0.0, 0.0, DamageType::Fire),
        0.0
    );
}

/// 「50% of armour applies to fire, cold and lightning」部分适用（ModParser.lua:2525-2529）：
/// 元素吃 50% 护甲，物理**保留全额**（区别于 instead 变体）。
#[test]
fn effective_armour_partial_pct_keeps_physical() {
    // 词条文本不在当前引擎规则内——按其数据展开（ModParser.lua:2525-2529）直接注入。
    let mut db = ModDb::new();
    for name in [
        "ArmourAppliesToFireDamageTaken",
        "ArmourAppliesToColdDamageTaken",
        "ArmourAppliesToLightningDamageTaken",
    ] {
        db.add_mod(Modifier::number(name, ModType::Base, 50.0));
    }
    let cfg = CalcConfig::attack();
    for dt in [DamageType::Fire, DamageType::Cold, DamageType::Lightning] {
        assert_eq!(
            effective_applied_armour(&db, &cfg, 1000.0, 0.0, 0.0, dt),
            500.0,
            "{dt:?}"
        );
    }
    assert_eq!(
        effective_applied_armour(&db, &cfg, 1000.0, 0.0, 0.0, DamageType::Physical),
        1000.0,
        "无 instead flag → 物理隐式 100 保留"
    );
}

/// 「instead of physical」变体（ModParser.lua:2519-2524）：元素吃全额护甲、
/// `ArmourDoesNotApplyToPhysicalDamageTaken` flag 把物理清零——物理清零**仅此变体**。
#[test]
fn effective_armour_instead_zeroes_physical() {
    // 词条文本不在当前引擎规则内——按其数据展开（ModParser.lua:2519-2524）直接注入。
    let mut db = ModDb::new();
    for name in [
        "ArmourAppliesToFireDamageTaken",
        "ArmourAppliesToColdDamageTaken",
        "ArmourAppliesToLightningDamageTaken",
    ] {
        db.add_mod(Modifier::number(name, ModType::Base, 100.0));
    }
    db.add_mod(Modifier::flag("ArmourDoesNotApplyToPhysicalDamageTaken"));
    let cfg = CalcConfig::attack();
    assert_eq!(
        effective_applied_armour(&db, &cfg, 1000.0, 0.0, 0.0, DamageType::Physical),
        0.0
    );
    assert_eq!(
        effective_applied_armour(&db, &cfg, 1000.0, 0.0, 0.0, DamageType::Fire),
        1000.0
    );
}

/// ArmourDefense 乘区（:1392 `Max("ArmourDefense")/100`、:2343 `×(1+ArmourDefense)`）
/// 与 Evasion 借入项（:2354-2356，仅 `max(…,0)`、不吃 ArmourDefense）。
#[test]
fn effective_armour_armour_defense_and_evasion_share() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("ArmourDefense", ModType::Base, 20.0));
    db.add_mod(Modifier::number(
        "EvasionAppliesToFireDamageTaken",
        ModType::Base,
        50.0,
    ));
    let cfg = CalcConfig::attack();
    // 物理：1000 × 100% × 1.2 = 1200。
    assert_eq!(
        effective_applied_armour(&db, &cfg, 1000.0, 800.0, 0.0, DamageType::Physical),
        1200.0
    );
    // 火：护甲份额 0 + Evasion 800 × 50% = 400（Evasion 项不乘 ArmourDefense）。
    assert_eq!(
        effective_applied_armour(&db, &cfg, 1000.0, 800.0, 0.0, DamageType::Fire),
        400.0
    );
}

// ─────────────────────────────────────────────────────────────────
// taken_hit_from_damage（CalcDefence.lua:422-455）
// ─────────────────────────────────────────────────────────────────

/// 纯抗性：火 raw 1000、火抗 75% → 承受 250（resMult = 1−75/100，:2363/:432）。
#[test]
fn taken_hit_pure_resist() {
    let ctx = ctx_with(|c| c.resist_taken_multi[FIRE] = 0.25);
    let (sum, parts) = taken_hit_from_damage(1000.0, DamageType::Fire, &ctx);
    assert_eq!(sum, 250.0);
    assert_eq!(parts[FIRE], 250.0);
}

/// flat DR + overwhelm（:429-431）：物理 raw 1000、固定减伤 30%、压制 10% →
/// drMulti = 1 − (30−10)/100 = 0.8 → 800。
#[test]
fn taken_hit_flat_dr_minus_overwhelm() {
    let ctx = ctx_with(|c| {
        c.flat_dr_pct[PHYS] = 30.0;
        c.overwhelm_pct[PHYS] = 10.0;
    });
    let (sum, _) = taken_hit_from_damage(1000.0, DamageType::Physical, &ctx);
    assert_eq!(sum, 800.0);
}

/// 减伤上限（:429/:431 双重 min `<src>DamageReductionMax`）：护甲 1e9 → armourDR
/// 封顶 90%；再被 overwhelm 15% 削到 75% → 承受 0.25（对照 pob2_golden
/// `defence_physical_overwhelm` 同源数值）。
#[test]
fn taken_hit_dr_capped_then_overwhelmed() {
    let ctx = ctx_with(|c| {
        c.effective_applied_armour[PHYS] = 1.0e9;
        c.overwhelm_pct[PHYS] = 15.0;
    });
    let (sum, _) = taken_hit_from_damage(1000.0, DamageType::Physical, &ctx);
    assert_eq!(sum, 250.0);
}

/// takenFlat 加量 + AfterReduction 乘数（:438/:442）：火 raw 100、takenFlat +20、
/// afterReduction 1.2 → round((100+20) × 1.2) = 144。
#[test]
fn taken_hit_taken_flat_and_after_multi() {
    let ctx = ctx_with(|c| {
        c.taken_flat[FIRE] = 20.0;
        c.after_reduction_multi[FIRE] = 1.2;
    });
    let (sum, _) = taken_hit_from_damage(100.0, DamageType::Fire, &ctx);
    assert_eq!(sum, 144.0);
}

/// 负 takenFlat 下界（:442 `m_max(…, 0)`）：减伤后不会变成负承受。
#[test]
fn taken_hit_floors_at_zero() {
    let ctx = ctx_with(|c| c.taken_flat[COLD] = -500.0);
    let (sum, parts) = taken_hit_from_damage(100.0, DamageType::Cold, &ctx);
    assert_eq!(sum, 0.0);
    assert_eq!(parts[COLD], 0.0);
}

// ─────────────────────────────────────────────────────────────────
// 端到端（builder + 词条文本）
// ─────────────────────────────────────────────────────────────────

/// Lightning Coil 型「50% of Physical Damage from Hits taken as Lightning」端到端：
/// armour 2000、电抗 75%，物理 raw 1000——
/// - 物理半：converted 500，armourDR = 2000/(2000+10×500) = 28.5714%（armour_ratio 10），
///   承受 round(500 × 0.714286) = 357；
/// - 电半：converted 500 × (1−0.75) = round(125) = 125；
///
/// 合计 482 < 未转换的 round(1000 × (1 − 2000/12000)) = 833 →
/// 物理实际承受显著降低（= 物理 max hit 提高）、电部分受抗性制约。
#[test]
fn taken_hit_lightning_coil_end_to_end() {
    let mut db = ModDb::new();
    add_text(
        &mut db,
        "50% of Physical Damage from Hits taken as Lightning Damage",
    );
    let cfg = CalcConfig::attack();
    let inputs = MitigationInputs {
        armour: 2000.0,
        resist_pct: [0.0, 0.0, 0.0, 75.0, 0.0],
        ..MitigationInputs::default()
    };
    let ctx = build_mitigation_ctx(&db, &cfg, &inputs);
    assert_eq!(ctx.shift[PHYS][LIGHT], 0.5);
    assert_eq!(ctx.shift[PHYS][PHYS], 0.5);
    assert_eq!(ctx.effective_applied_armour[PHYS], 2000.0);
    assert_eq!(ctx.effective_applied_armour[LIGHT], 0.0);

    let (sum, parts) = taken_hit_from_damage(1000.0, DamageType::Physical, &ctx);
    assert_eq!(parts[PHYS], 357.0, "物理半经护甲减伤");
    assert_eq!(parts[LIGHT], 125.0, "电半经抗性");
    assert_eq!(sum, 482.0);

    // 对照：未转换时同口径承受 833 —— taken-as 把物理承受降了 42%。
    let baseline = build_mitigation_ctx(&ModDb::new(), &cfg, &inputs);
    let (no_shift_sum, _) = taken_hit_from_damage(1000.0, DamageType::Physical, &baseline);
    assert_eq!(no_shift_sum, 833.0);
    assert!(sum < no_shift_sum);
}

/// 「50% of armour applies to fire」部分适用端到端：火 raw 1000、armour 2000、火抗 0 →
/// effArmour 1000 → armourDR = 1000/(1000+10000) = 9.0909% → round(909) = 909；
/// 物理同口径仍吃全额护甲（2000）→ round(1000 × (1−1/6)) = 833。
#[test]
fn builder_partial_armour_applies_to_fire() {
    // 词条文本不在当前引擎规则内——按其数据展开直接注入（同上）。
    let mut db = ModDb::new();
    for name in [
        "ArmourAppliesToFireDamageTaken",
        "ArmourAppliesToColdDamageTaken",
        "ArmourAppliesToLightningDamageTaken",
    ] {
        db.add_mod(Modifier::number(name, ModType::Base, 50.0));
    }
    let cfg = CalcConfig::attack();
    let inputs = MitigationInputs {
        armour: 2000.0,
        ..MitigationInputs::default()
    };
    let ctx = build_mitigation_ctx(&db, &cfg, &inputs);
    assert_eq!(ctx.effective_applied_armour[FIRE], 1000.0);
    assert_eq!(ctx.effective_applied_armour[PHYS], 2000.0);

    let (fire_taken, _) = taken_hit_from_damage(1000.0, DamageType::Fire, &ctx);
    assert_eq!(fire_taken, 909.0);
    let (phys_taken, _) = taken_hit_from_damage(1000.0, DamageType::Physical, &ctx);
    assert_eq!(phys_taken, 833.0);
}

/// 「instead of physical」flag 端到端：物理护甲清零仅此变体——物理承受回到无护甲
/// 1000，火承受改吃全额护甲 833。
#[test]
fn builder_instead_of_physical_flag() {
    // 词条文本不在当前引擎规则内——按其数据展开直接注入（同上）。
    let mut db = ModDb::new();
    for name in [
        "ArmourAppliesToFireDamageTaken",
        "ArmourAppliesToColdDamageTaken",
        "ArmourAppliesToLightningDamageTaken",
    ] {
        db.add_mod(Modifier::number(name, ModType::Base, 100.0));
    }
    db.add_mod(Modifier::flag("ArmourDoesNotApplyToPhysicalDamageTaken"));
    let cfg = CalcConfig::attack();
    let inputs = MitigationInputs {
        armour: 2000.0,
        ..MitigationInputs::default()
    };
    let ctx = build_mitigation_ctx(&db, &cfg, &inputs);
    assert_eq!(ctx.armour_applies_pct[PHYS], 0.0, "flag 清零物理份额");
    assert_eq!(ctx.effective_applied_armour[PHYS], 0.0);
    assert_eq!(ctx.effective_applied_armour[FIRE], 2000.0);

    let (phys_taken, _) = taken_hit_from_damage(1000.0, DamageType::Physical, &ctx);
    assert_eq!(phys_taken, 1000.0);
    let (fire_taken, _) = taken_hit_from_damage(1000.0, DamageType::Fire, &ctx);
    assert_eq!(fire_taken, 833.0);
}

/// builder 默认值：空 db → dr 上限 90（vendor Data.lua:178 DamageReductionCap，
/// 经 `cfg.constants.character().maximum_physical_damage_reduction_pct` 注入）、
/// shift 恒等、乘数 1。
#[test]
fn builder_neutral_defaults() {
    let db = ModDb::new();
    let cfg = CalcConfig::attack();
    let ctx = build_mitigation_ctx(&db, &cfg, &MitigationInputs::default());
    assert_eq!(ctx.dr_max_pct, [90.0; 5]);
    assert_eq!(ctx.flat_dr_pct, [0.0; 5]);
    assert_eq!(ctx.resist_taken_multi, [1.0; 5]);
    assert_eq!(ctx.after_reduction_multi, [1.0; 5]);
    assert_eq!(ctx.taken_flat, [0.0; 5]);
    assert_eq!(ctx.armour_applies_pct[PHYS], 100.0);
    assert_eq!(ctx.shift[CHAOS][CHAOS], 1.0);
}

/// builder 的 taken inc/more 折入 afterReduction（:2429 Average 口径 ×
/// `taken_mult_for_type_default`）：「20% increased Damage Taken」→ 乘数 1.2。
#[test]
fn builder_after_reduction_from_taken_inc() {
    let mut db = ModDb::new();
    add_text(&mut db, "20% increased Damage Taken");
    let cfg = CalcConfig::attack();
    let ctx = build_mitigation_ctx(&db, &cfg, &MitigationInputs::default());
    assert!(
        (ctx.after_reduction_multi[FIRE] - 1.2).abs() < 1e-9,
        "after = {}",
        ctx.after_reduction_multi[FIRE]
    );
    let (sum, _) = taken_hit_from_damage(1000.0, DamageType::Fire, &ctx);
    assert_eq!(sum, 1200.0);
}
