//! 乘区与 DPS 末端（scaled_damage.rs）：Double/Triple Damage 乘区 +
//! DPS 末端两因子。每乘区手算单测 + oracle 中间值对拍。

use pobr_core::calc::{AllMultExtras, all_mult, dps_end_factors, scaled_damage_effect};
use pobr_core::{CalcConfig, ModDb, Modifier};
use pobr_data::prelude::*;

const EPS: f64 = 1e-9;

fn base(name: &str, value: f64) -> Modifier {
    Modifier::number(name, ModType::Base, value)
}

// ----------------------------------------------------------------

/// 手算：仅 DoubleDamageChance 30 → DD=30、TD=0、effect = 1 + 30/100 = 1.3。
#[test]
fn double_damage_only_hand_calc() {
    let mut db = ModDb::new();
    db.add_mod(base("DoubleDamageChance", 30.0));
    let out = scaled_damage_effect(&db, &ModDb::new(), &CalcConfig::attack(), 0.0);
    assert!((out.double_chance - 30.0).abs() < EPS);
    assert!((out.triple_chance - 0.0).abs() < EPS);
    assert!((out.effect - 1.3).abs() < EPS);
}

/// 手算：OnCrit 折算与 CritChance 联动——DoubleDamageChanceOnCrit 50 × crit 0.5 → DD=25。
#[test]
fn double_damage_on_crit_scales_with_crit_chance() {
    let mut db = ModDb::new();
    db.add_mod(base("DoubleDamageChanceOnCrit", 50.0));
    let out = scaled_damage_effect(&db, &ModDb::new(), &CalcConfig::attack(), 0.5);
    assert!((out.double_chance - 25.0).abs() < EPS);
    assert!((out.effect - 1.25).abs() < EPS);
}

/// 手算：Triple 抵扣 Double（去重）——DD 50、TD 40 → DD' = 50 − 40×50/100 = 30，
/// effect = 1 + 30/100 + 2×40/100 = 2.1。
#[test]
fn triple_offsets_double_hand_calc() {
    let mut db = ModDb::new();
    db.add_mod(base("DoubleDamageChance", 50.0));
    db.add_mod(base("TripleDamageChance", 40.0));
    let out = scaled_damage_effect(&db, &ModDb::new(), &CalcConfig::attack(), 0.0);
    assert!((out.double_chance - 30.0).abs() < EPS);
    assert!((out.triple_chance - 40.0).abs() < EPS);
    assert!((out.effect - 2.1).abs() < EPS);
}

/// 手算：cap 100——DD 150 → 100（effect 2.0）；TD 100 时 Double 全额抵扣为 0、effect 3.0。
#[test]
fn chances_cap_at_100_and_full_triple_eats_double() {
    let mut db = ModDb::new();
    db.add_mod(base("DoubleDamageChance", 150.0));
    let out = scaled_damage_effect(&db, &ModDb::new(), &CalcConfig::attack(), 0.0);
    assert!((out.double_chance - 100.0).abs() < EPS);
    assert!((out.effect - 2.0).abs() < EPS);

    db.add_mod(base("TripleDamageChance", 100.0));
    let out = scaled_damage_effect(&db, &ModDb::new(), &CalcConfig::attack(), 0.0);
    assert!((out.double_chance - 0.0).abs() < EPS);
    assert!((out.triple_chance - 100.0).abs() < EPS);
    assert!((out.effect - 3.0).abs() < EPS);
}

/// 手算：TripleDamageChanceOnCrit 先 cap 100 再乘 crit（vendor :3843 先 min 后折算）。
#[test]
fn triple_on_crit_caps_before_crit_scaling() {
    let mut db = ModDb::new();
    db.add_mod(base("TripleDamageChanceOnCrit", 150.0));
    let out = scaled_damage_effect(&db, &ModDb::new(), &CalcConfig::attack(), 0.5);
    // min(150,100)=100 → ×0.5 = 50（若不先 cap 会得 75）。
    assert!((out.triple_chance - 50.0).abs() < EPS);
    assert!((out.effect - 2.0).abs() < EPS);
}

/// 敌方 SelfDoubleDamageChance / SelfTripleDamageChance 仅 effective 口径计入
/// （vendor :3845/:3849 `env.mode_effective and enemyDB:Sum(...)`）。
#[test]
fn enemy_self_chances_only_in_effective_mode() {
    let db = ModDb::new();
    let mut enemy = ModDb::new();
    enemy.add_mod(base("SelfDoubleDamageChance", 20.0));
    enemy.add_mod(base("SelfTripleDamageChance", 10.0));

    let panel = scaled_damage_effect(&db, &enemy, &CalcConfig::attack(), 0.0);
    assert!((panel.double_chance - 0.0).abs() < EPS);
    assert!((panel.triple_chance - 0.0).abs() < EPS);
    assert!((panel.effect - 1.0).abs() < EPS);

    let cfg = CalcConfig::attack().with_mode_effective(true);
    let eff = scaled_damage_effect(&db, &enemy, &cfg, 0.0);
    // DD = 20 − 10×20/100 = 18；effect = 1 + 0.18 + 0.20 = 1.38。
    assert!((eff.double_chance - 18.0).abs() < EPS);
    assert!((eff.triple_chance - 10.0).abs() < EPS);
    assert!((eff.effect - 1.38).abs() < EPS);
}

/// 无任何 DD/TD 词条时 effect 恒 1.0（T2 接线前后零行为差的等价性锚点）。
#[test]
fn no_mods_yields_identity_effect() {
    let out = scaled_damage_effect(&ModDb::new(), &ModDb::new(), &CalcConfig::attack(), 0.95);
    assert!((out.effect - 1.0).abs() < EPS);
    assert!((out.double_chance - 0.0).abs() < EPS);
    assert!((out.triple_chance - 0.0).abs() < EPS);
}

/// Oracle 中间值对拍（门禁：≥1 条带 DD 词条 build）。
///
/// 来源：`tools/pob2-oracle` 跑 monk-martial-artist-flicker-strike build，在 Sapphire Ring
/// 上加三行词条后 dump `mainOutput.MainHand`（2026-06-12，vendor 0.18.0）：
///
/// ```text
/// 词条：25% chance to deal Double Damage
///       10% chance to deal Triple Damage
///       Your Critical Hits have a 50% chance to deal Double Damage
/// oracle: CritChance = 83.6784（百分比）
///         DoubleDamageChanceOnCrit = 50
///         TripleDamageChance = 10
///         DoubleDamageChance = 60.15528   （= (25 + 50×0.836784) × (1 − 10/100)）
///         DoubleDamageEffect = 0.6015528
///         TripleDamageEffect = 0.2
///         ScaledDamageEffect = 1.8015528
/// ```
#[test]
fn oracle_parity_flicker_strike_with_dd_mods() {
    let mut db = ModDb::new();
    db.add_mod(base("DoubleDamageChance", 25.0));
    db.add_mod(base("TripleDamageChance", 10.0));
    db.add_mod(base("DoubleDamageChanceOnCrit", 50.0));
    let crit_chance = 83.6784 / 100.0; // oracle MainHand.CritChance（分数口径入参）

    let out = scaled_damage_effect(&db, &ModDb::new(), &CalcConfig::attack(), crit_chance);

    assert!((out.double_chance - 60.15528).abs() < 1e-6);
    assert!((out.triple_chance - 10.0).abs() < EPS);
    assert!((out.effect - 1.8015528).abs() < 1e-6);
}

/// AllMultExtras 默认全 1.0：`all_mult` 退化为 ScaledDamageEffect。
#[test]
fn all_mult_defaults_to_scaled_damage_effect() {
    let mut db = ModDb::new();
    db.add_mod(base("DoubleDamageChance", 30.0));
    let scaled = scaled_damage_effect(&db, &ModDb::new(), &CalcConfig::attack(), 0.0);
    let extras = AllMultExtras::default();
    assert!((extras.product() - 1.0).abs() < EPS);
    assert!((all_mult(&scaled, &extras) - scaled.effect).abs() < EPS);

    // 占位因子接入后按连乘参与（防返工接口验证）。
    let extras = AllMultExtras {
        fist_of_war: 1.2,
        offensive_warcry: 1.5,
        ..AllMultExtras::default()
    };
    assert!((all_mult(&scaled, &extras) - scaled.effect * 1.8).abs() < EPS);
}

// ----------------------------------------------------------------

/// 手算：QuantityMultiplier BASE 聚合，floor 1.0（vendor :3128 `max(Sum, 1)`）。
#[test]
fn quantity_multiplier_floors_at_one() {
    let cfg = CalcConfig::attack();
    // 无词条 → 1.0。
    let out = dps_end_factors(&ModDb::new(), &cfg, None);
    assert!((out.quantity_multiplier - 1.0).abs() < EPS);

    // BASE 3 → 3.0。
    let mut db = ModDb::new();
    db.add_mod(base("QuantityMultiplier", 3.0));
    let out = dps_end_factors(&db, &cfg, None);
    assert!((out.quantity_multiplier - 3.0).abs() < EPS);

    // BASE 0.5 → floor 到 1.0。
    let mut db = ModDb::new();
    db.add_mod(base("QuantityMultiplier", 0.5));
    let out = dps_end_factors(&db, &cfg, None);
    assert!((out.quantity_multiplier - 1.0).abs() < EPS);
}

/// 手算：dps_multiplier = skillData × (1 + ΣInc("DPS")/100) × More("DPS")
/// （vendor :3863 `skillData.dpsMultiplier × calcLib.mod(..., "DPS")`）。
/// 1.4 × 1.5 × 1.2 = 2.52。
#[test]
fn dps_multiplier_folds_skill_data_and_dps_modname() {
    let cfg = CalcConfig::attack();
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("DPS", ModType::Inc, 50.0));
    db.add_mod(Modifier::number("DPS", ModType::More, 20.0));
    let out = dps_end_factors(&db, &cfg, Some(1.4));
    assert!((out.dps_multiplier - 2.52).abs() < EPS);

    // skillData 缺省 = 1.0（vendor `skillData.dpsMultiplier or 1`）。
    let out = dps_end_factors(&db, &cfg, None);
    assert!((out.dps_multiplier - 1.8).abs() < EPS);
}

/// 手算：grenade 二次起爆折算（vendor CalcOffence.lua:1124-1127
/// `DPS MORE min(Sum(BASE,"GrenadeActivateTwice"),100)`）：
/// Payload 50 → ×1.5；叠加超 100 时 cap（50+80=130 → ×2.0）。
#[test]
fn grenade_activate_twice_folds_into_dps_multiplier_with_cap() {
    let cfg = CalcConfig::attack();
    let mut db = ModDb::new();
    db.add_mod(base("GrenadeActivateTwice", 50.0));
    let out = dps_end_factors(&db, &cfg, None);
    assert!((out.dps_multiplier - 1.5).abs() < EPS);

    db.add_mod(base("GrenadeActivateTwice", 80.0));
    let out = dps_end_factors(&db, &cfg, None);
    assert!(
        (out.dps_multiplier - 2.0).abs() < EPS,
        "vendor m_min(…,100)"
    );
}

/// Oracle 末端恒等式对拍：deadeye explosive-grenade build——Payload 50% 二次
/// 起爆 → dpsMultiplier 1.5，vendor `TotalDPS = AverageDamage × Speed ×
/// dpsMultiplier`（:4407）数值闭合（oracle `AverageDamage=108426.0294`、
/// `Speed=0.164`、`TotalDPS=26672.80323`，meta.json golden 逐位一致）。
#[test]
fn oracle_identity_grenade_dps_multiplier_closes_total_dps() {
    let mut db = ModDb::new();
    db.add_mod(base("GrenadeActivateTwice", 50.0));
    let out = dps_end_factors(&db, &CalcConfig::attack(), None);
    let total = 108426.0294_f64 * 0.164 * out.dps_multiplier * out.quantity_multiplier;
    assert!((total - 26672.80323).abs() < 0.01);
}

/// Oracle 末端恒等式对拍：flicker-strike build（无 DPS/Quantity 词条、skillData
/// dpsMultiplier = 1）应得两因子恒 1，且 vendor `TotalDPS = AverageDamage × Speed ×
/// dps × quantity`（:4407）数值闭合。
///
/// oracle（同 [`oracle_parity_flicker_strike_with_dd_mods`] 那次 dump）：
/// `TotalDPS = 1203295.309`、`AverageDamage = 196176.1253`、`Speed = 6.13375`、
/// `skillInfo.dpsMultiplier = 1`、`QuantityMultiplier` 未写（=1）。
#[test]
fn oracle_identity_dps_end_factors_default_to_one() {
    let out = dps_end_factors(&ModDb::new(), &CalcConfig::attack(), Some(1.0));
    assert!((out.dps_multiplier - 1.0).abs() < EPS);
    assert!((out.quantity_multiplier - 1.0).abs() < EPS);

    // vendor :4407 末端公式按 oracle 值闭合（两因子=1 时）。
    let total = 196176.1253_f64 * 6.13375 * out.dps_multiplier * out.quantity_multiplier;
    assert!((total - 1203295.309).abs() < 0.5);
}
