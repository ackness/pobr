//! M4-T3 W-C2（LuckyHits 掷骰平均）+ W-C3（canDeal / DealNo<Type> 门控）手算单测。
//! vendor 对照：CalcOffence.lua:4036-4046（lucky）、:2226-2230（canDeal）。

use pobr_core::calc::{DamageComponent, apply_can_deal, convert_damage, lucky_hit_chance};
use pobr_core::{CalcConfig, ModDb, Modifier};
use pobr_data::prelude::*;

const EPS: f64 = 1e-9;

// ---------------------------------------------------------------- W-C2 avg 函数族

/// 手算（蓝图 §2 W-C2 指定用例）：(min,max)=(10,100)——lucky avg = 70 vs 普通 55。
#[test]
fn lucky_avg_hand_calc() {
    let comp = DamageComponent::new(DamageType::Physical, 10.0, 100.0);
    // p=0 与旧 avg() 逐位一致（等价性锚点）。
    assert!((comp.avg_with_lucky(0.0) - comp.avg()).abs() < EPS);
    assert!((comp.avg() - 55.0).abs() < EPS);
    // p=1：min/3 + 2max/3 = 10/3 + 200/3 = 70。
    assert!((comp.avg_with_lucky(1.0) - 70.0).abs() < EPS);
    // p=0.5：55×0.5 + 70×0.5 = 62.5。
    assert!((comp.avg_with_lucky(0.5) - 62.5).abs() < EPS);
    // 越界 clamp。
    assert!((comp.avg_with_lucky(1.5) - 70.0).abs() < EPS);
    assert!((comp.avg_with_lucky(-0.5) - 55.0).abs() < EPS);
}

/// `LuckyHits` flag：所有 pass × 所有类型恒 lucky（p=1）。
#[test]
fn lucky_hits_flag_applies_everywhere() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::flag("LuckyHits"));
    let cfg = CalcConfig::attack();
    for crit in [true, false] {
        for ty in [
            DamageType::Physical,
            DamageType::Lightning,
            DamageType::Chaos,
        ] {
            assert!((lucky_hit_chance(&db, &cfg, ty, crit) - 1.0).abs() < EPS);
        }
    }
}

/// `CritLucky` 只影响暴击 pass（蓝图指定用例；与 crit.rs 的 CritChanceLucky 是两个机制）。
#[test]
fn crit_lucky_only_affects_crit_pass() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::flag("CritLucky"));
    let cfg = CalcConfig::attack();
    assert!((lucky_hit_chance(&db, &cfg, DamageType::Physical, true) - 1.0).abs() < EPS);
    assert!((lucky_hit_chance(&db, &cfg, DamageType::Physical, false) - 0.0).abs() < EPS);
}

/// `LightningNoCritLucky` 仅非暴击 pass 且 Lightning 分量。
#[test]
fn lightning_no_crit_lucky_scope() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::flag("LightningNoCritLucky"));
    let cfg = CalcConfig::attack();
    assert!((lucky_hit_chance(&db, &cfg, DamageType::Lightning, false) - 1.0).abs() < EPS);
    assert!((lucky_hit_chance(&db, &cfg, DamageType::Lightning, true) - 0.0).abs() < EPS);
    assert!((lucky_hit_chance(&db, &cfg, DamageType::Cold, false) - 0.0).abs() < EPS);
}

/// `ElementalLuckHits` 仅三元素（Lightning/Cold/Fire），物理/混沌不享。
#[test]
fn elemental_luck_hits_only_elements() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::flag("ElementalLuckHits"));
    let cfg = CalcConfig::attack();
    for ty in [DamageType::Lightning, DamageType::Cold, DamageType::Fire] {
        assert!((lucky_hit_chance(&db, &cfg, ty, false) - 1.0).abs() < EPS);
    }
    assert!((lucky_hit_chance(&db, &cfg, DamageType::Physical, false) - 0.0).abs() < EPS);
    assert!((lucky_hit_chance(&db, &cfg, DamageType::Chaos, false) - 0.0).abs() < EPS);
}

/// Sum 通道：`<Type>LuckyHitsChance + LuckyHitsChance` 求和、cap 100、/100 折分数。
#[test]
fn lucky_chance_sums_typed_and_generic_with_cap() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number(
        "LightningLuckyHitsChance",
        ModType::Base,
        30.0,
    ));
    db.add_mod(Modifier::number("LuckyHitsChance", ModType::Base, 20.0));
    let cfg = CalcConfig::attack();
    // Lightning：30 + 20 = 50 → 0.5；Cold：仅通用 20 → 0.2。
    assert!((lucky_hit_chance(&db, &cfg, DamageType::Lightning, false) - 0.5).abs() < EPS);
    assert!((lucky_hit_chance(&db, &cfg, DamageType::Cold, false) - 0.2).abs() < EPS);

    // cap 100：再加 90 → 30+20+90=140 → cap 100 → 1.0。
    db.add_mod(Modifier::number("LuckyHitsChance", ModType::Base, 90.0));
    assert!((lucky_hit_chance(&db, &cfg, DamageType::Lightning, false) - 1.0).abs() < EPS);
}

// ---------------------------------------------------------------- W-C3 canDeal

/// 蓝图指定合成用例（Avatar of Fire 形态）：物理 50% 转火后 `DealNoPhysical`——
/// 残留物理清零、已转出的火焰保留（转换先发生，清零的是转换后残留）。
#[test]
fn avatar_of_fire_zeroes_residual_physical_keeps_converted_fire() {
    let components = vec![DamageComponent::new(DamageType::Physical, 100.0, 200.0)];
    let mut converted = convert_damage(&components, DamageType::Physical, DamageType::Fire, 0.5);

    let mut db = ModDb::new();
    db.add_mod(Modifier::flag("DealNoPhysical"));
    apply_can_deal(&mut converted, &db, &CalcConfig::attack());

    let phys = converted
        .iter()
        .find(|c| c.damage_type == DamageType::Physical)
        .unwrap();
    assert!((phys.min - 0.0).abs() < EPS);
    assert!((phys.max - 0.0).abs() < EPS);
    let fire = converted
        .iter()
        .find(|c| c.damage_type == DamageType::Fire)
        .unwrap();
    assert!((fire.min - 50.0).abs() < EPS);
    assert!((fire.max - 100.0).abs() < EPS);
}

/// `DealNoDamage` 清零全部类型（5+1 名单中的通用项）。
#[test]
fn deal_no_damage_zeroes_all_components() {
    let mut components = vec![
        DamageComponent::new(DamageType::Physical, 10.0, 20.0),
        DamageComponent::new(DamageType::Fire, 30.0, 40.0),
        DamageComponent::new(DamageType::Chaos, 50.0, 60.0),
    ];
    let mut db = ModDb::new();
    db.add_mod(Modifier::flag("DealNoDamage"));
    apply_can_deal(&mut components, &db, &CalcConfig::attack());
    for component in &components {
        assert!((component.min - 0.0).abs() < EPS);
        assert!((component.max - 0.0).abs() < EPS);
    }
}

/// 无 DealNo* 旗标时分量逐位不变（接线后零行为差的等价性锚点）。
#[test]
fn no_flags_leaves_components_unchanged() {
    let original = vec![
        DamageComponent::new(DamageType::Physical, 10.0, 20.0),
        DamageComponent::new(DamageType::Cold, 5.0, 15.0),
    ];
    let mut components = original.clone();
    apply_can_deal(&mut components, &ModDb::new(), &CalcConfig::attack());
    assert_eq!(components, original);
}

/// 逐类型门控互不串扰：`DealNoCold` 只清 Cold，其余类型保留。
#[test]
fn per_type_gating_is_independent() {
    let mut components = vec![
        DamageComponent::new(DamageType::Physical, 10.0, 20.0),
        DamageComponent::new(DamageType::Cold, 5.0, 15.0),
        DamageComponent::new(DamageType::Fire, 7.0, 9.0),
    ];
    let mut db = ModDb::new();
    db.add_mod(Modifier::flag("DealNoCold"));
    apply_can_deal(&mut components, &db, &CalcConfig::attack());
    assert!((components[0].min - 10.0).abs() < EPS);
    assert!((components[1].min - 0.0).abs() < EPS);
    assert!((components[1].max - 0.0).abs() < EPS);
    assert!((components[2].max - 9.0).abs() < EPS);
}
