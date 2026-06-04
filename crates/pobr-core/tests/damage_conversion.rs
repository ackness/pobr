//! 伤害转换链端到端集成测试（接入 `calculate_components` 管线）。
//!
//! 覆盖 gap：conversion-chain-not-wired、double-dip-accumulated-type-flags、
//! hit-dot-split-missing-in-damagecomponent、gain-as-extra。
//!
//! 核对基准：PoB2 `CalcOffence.lua`（`processDamageConversion` / `calcConvertedDamage`
//! / `buildGainTable` / `calcGainedDamage`）、agent-docs/damage-scaling.md §伤害转换。

use pobr_core::calc::damage::DAMAGE_TYPES;
use pobr_core::calc::{DamageComponent, MinimalInput, calculate_minimal};
use pobr_core::{CalcConfig, ModDb, ModTag, Modifier};
use pobr_data::prelude::*;

fn base_input() -> MinimalInput {
    MinimalInput {
        base_life: 1.0,
        base_mana: 1.0,
        base_fire_resistance: 0.0,
        base_cold_resistance: 0.0,
        base_lightning_resistance: 0.0,
        base_accuracy: 0.0,
        enemy_evasion: 0.0,
        base_hit_min: 100.0,
        base_hit_max: 200.0,
        base_action_rate: 1.0,
    }
}

fn component(output: &pobr_core::calc::MinimalOutput, ty: DamageType) -> &DamageComponent {
    output
        .damage_components
        .iter()
        .find(|c| c.damage_type == ty)
        .unwrap_or_else(|| panic!("missing {ty:?} damage component"))
}

fn find(output: &pobr_core::calc::MinimalOutput, ty: DamageType) -> Option<&DamageComponent> {
    output
        .damage_components
        .iter()
        .find(|c| c.damage_type == ty)
}

/// 50% Phys→Fire 转换：物理留 50%、火焰得 50%。
/// 验证基础搬运正确，且火焰分量的 `type_path` 含 [Physical, Fire]。
#[test]
fn convert_50_percent_phys_to_fire_splits_base() {
    let mut db = ModDb::new();
    db.add_mod(
        Modifier::number("PhysicalDamageConvertToFire", ModType::Base, 50.0)
            .with_flags(ModFlags::ATTACK),
    );

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());

    let phys = component(&output, DamageType::Physical);
    let fire = component(&output, DamageType::Fire);
    // base 100-200 → 各 50%
    assert_eq!(phys.min, 50.0);
    assert_eq!(phys.max, 100.0);
    assert_eq!(fire.min, 50.0);
    assert_eq!(fire.max, 100.0);
    // 火焰分量携带 phys→fire 沿途类型集合
    assert!(fire.type_path.contains(&DamageType::Physical));
    assert!(fire.type_path.contains(&DamageType::Fire));
}

/// double-dip：50% Phys→Fire 后，火焰分量同时吃 PhysicalDamage inc、FireDamage inc、
/// ElementalDamage inc 三条 increased（来源 + 目标 + 元素共享组）。
#[test]
fn converted_fire_double_dips_phys_and_fire_and_elemental_increased() {
    let mut db = ModDb::new();
    db.add_mod(
        Modifier::number("PhysicalDamageConvertToFire", ModType::Base, 50.0)
            .with_flags(ModFlags::ATTACK),
    );
    // PhysicalDamage inc 100：物理分量与转换出的火焰分量都吃
    db.add_mod(
        Modifier::number("PhysicalDamage", ModType::Inc, 100.0)
            .with_flags(ModFlags::ATTACK)
            .with_tag(ModTag::DamageType(DamageType::Physical)),
    );
    // FireDamage inc 100：仅火焰分量吃
    db.add_mod(
        Modifier::number("FireDamage", ModType::Inc, 100.0)
            .with_flags(ModFlags::ATTACK)
            .with_tag(ModTag::DamageType(DamageType::Fire)),
    );
    // ElementalDamage inc 50：火焰分量吃
    db.add_mod(
        Modifier::number("ElementalDamage", ModType::Inc, 50.0)
            .with_flags(ModFlags::ATTACK)
            .with_tag(ModTag::DamageType(DamageType::Fire)),
    );

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());

    // 物理分量：base 50-100，吃 PhysicalDamage inc 100 → ×2.0
    let phys = component(&output, DamageType::Physical);
    assert_eq!(phys.min, 100.0);
    assert_eq!(phys.max, 200.0);

    // 火焰分量：base 50-100，inc = Phys100 + Fire100 + Elem50 = 250 → ×3.5
    // min 50*3.5 = 175, max 100*3.5 = 350
    let fire = component(&output, DamageType::Fire);
    assert_eq!(fire.min, 175.0);
    assert_eq!(fire.max, 350.0);
}

/// 超 100% 转换归一：100% Phys→Fire + 50% Phys→Cold → 归一为 ~67% Fire / ~33% Cold。
/// 物理被完全转出（留存 0）。
#[test]
fn over_100_percent_conversion_normalizes_to_one() {
    let mut db = ModDb::new();
    db.add_mod(
        Modifier::number("PhysicalDamageConvertToFire", ModType::Base, 100.0)
            .with_flags(ModFlags::ATTACK),
    );
    db.add_mod(
        Modifier::number("PhysicalDamageConvertToCold", ModType::Base, 50.0)
            .with_flags(ModFlags::ATTACK),
    );

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());

    // 物理完全转出（留存 0）→ 无物理分量或为 0
    let phys = find(&output, DamageType::Physical);
    if let Some(p) = phys {
        assert!(p.min.abs() < 1e-6 && p.max.abs() < 1e-6, "phys should be 0");
    }
    // fire = 100/150 = 2/3，cold = 50/150 = 1/3（无 inc）
    let fire = component(&output, DamageType::Fire);
    let cold = component(&output, DamageType::Cold);
    // base avg 150 → fire avg ≈ 100, cold avg ≈ 50
    assert!(
        (fire.avg() - 100.0).abs() < 0.5,
        "fire avg ~100, got {}",
        fire.avg()
    );
    assert!(
        (cold.avg() - 50.0).abs() < 0.5,
        "cold avg ~50, got {}",
        cold.avg()
    );
    // 转换后总伤害守恒（≈ base avg 150）
    let total: f64 = output.damage_components.iter().map(|c| c.avg()).sum();
    assert!(
        (total - 150.0).abs() < 0.5,
        "total conserved ~150, got {total}"
    );
}

/// gain-as-extra：25% Phys gain as Lightning，物理来源**不扣减**，额外追加闪电包。
#[test]
fn gain_as_extra_does_not_reduce_source() {
    let mut db = ModDb::new();
    db.add_mod(
        Modifier::number("PhysicalDamageGainAsLightning", ModType::Base, 25.0)
            .with_flags(ModFlags::ATTACK),
    );

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());

    // 物理来源保持原样（gain 不扣减）
    let phys = component(&output, DamageType::Physical);
    assert_eq!(phys.min, 100.0);
    assert_eq!(phys.max, 200.0);
    // 额外闪电 = 25% 物理
    let lightning = component(&output, DamageType::Lightning);
    assert_eq!(lightning.min, 25.0);
    assert_eq!(lightning.max, 50.0);
    // 闪电分量 type_path 含来源 Physical（double-dip）
    assert!(lightning.type_path.contains(&DamageType::Physical));
    assert!(lightning.type_path.contains(&DamageType::Lightning));
    // 总伤害 = 物理 150 + 闪电 37.5 = 187.5（gain 是净增）
    let total: f64 = output.damage_components.iter().map(|c| c.avg()).sum();
    assert_eq!(total, 187.5);
}

/// gain-as-extra double-dip：物理 gain as 火焰后，额外火焰吃 PhysicalDamage inc + FireDamage inc。
#[test]
fn gain_as_extra_fire_double_dips_phys_and_fire() {
    let mut db = ModDb::new();
    db.add_mod(
        Modifier::number("PhysicalDamageGainAsFire", ModType::Base, 50.0)
            .with_flags(ModFlags::ATTACK),
    );
    db.add_mod(
        Modifier::number("PhysicalDamage", ModType::Inc, 100.0)
            .with_flags(ModFlags::ATTACK)
            .with_tag(ModTag::DamageType(DamageType::Physical)),
    );
    db.add_mod(
        Modifier::number("FireDamage", ModType::Inc, 100.0)
            .with_flags(ModFlags::ATTACK)
            .with_tag(ModTag::DamageType(DamageType::Fire)),
    );

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());

    // 物理：base 100-200 不扣减，吃 PhysicalDamage inc 100 → ×2 = 200-400
    let phys = component(&output, DamageType::Physical);
    assert_eq!(phys.min, 200.0);
    assert_eq!(phys.max, 400.0);
    // 额外火焰：50% 物理 base = 50-100，inc = Phys100 + Fire100 = 200 → ×3 = 150-300
    let fire = component(&output, DamageType::Fire);
    assert_eq!(fire.min, 150.0);
    assert_eq!(fire.max, 300.0);
}

/// 富化字段：转换 / gain 产生的分量 kind=Hit、source=Attack；type_path 正确去重有序。
#[test]
fn converted_components_carry_hit_attack_kind_and_ordered_path() {
    let mut db = ModDb::new();
    db.add_mod(
        Modifier::number("PhysicalDamageConvertToFire", ModType::Base, 100.0)
            .with_flags(ModFlags::ATTACK),
    );

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());
    let fire = component(&output, DamageType::Fire);

    assert_eq!(fire.kind, DamageKind::Hit);
    assert_eq!(fire.source, DamageSource::Attack);
    // type_path 按 DAMAGE_TYPES 链序：Physical(0) before Fire(3)
    let idx_phys = DAMAGE_TYPES
        .iter()
        .position(|t| *t == DamageType::Physical)
        .unwrap();
    let idx_fire = DAMAGE_TYPES
        .iter()
        .position(|t| *t == DamageType::Fire)
        .unwrap();
    let path_phys = fire
        .type_path
        .iter()
        .position(|t| *t == DamageType::Physical)
        .unwrap();
    let path_fire = fire
        .type_path
        .iter()
        .position(|t| *t == DamageType::Fire)
        .unwrap();
    assert!(idx_phys < idx_fire);
    assert!(path_phys < path_fire, "type_path must follow chain order");
}

/// 回归：无任何转换 / gain modifier 时，分量与历史逐字一致（纯物理 + 附加火焰）。
#[test]
fn no_conversion_path_matches_legacy_output_verbatim() {
    // 纯物理
    let db = ModDb::new();
    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());
    assert_eq!(output.damage_components.len(), 1);
    let phys = component(&output, DamageType::Physical);
    assert_eq!(phys.min, 100.0);
    assert_eq!(phys.max, 200.0);
    assert_eq!(phys.type_path, vec![DamageType::Physical]);

    // 附加火焰 flat + 元素 inc，无转换
    let mut db2 = ModDb::new();
    db2.add_mod(
        Modifier::number("FireDamageMin", ModType::Base, 100.0).with_flags(ModFlags::ATTACK),
    );
    db2.add_mod(
        Modifier::number("FireDamageMax", ModType::Base, 100.0).with_flags(ModFlags::ATTACK),
    );
    db2.add_mod(
        Modifier::number("ElementalDamage", ModType::Inc, 50.0).with_flags(ModFlags::ATTACK),
    );
    let output2 = calculate_minimal(&db2, &CalcConfig::attack(), &base_input());
    // 火焰 100 * 1.5 = 150（与 damage_components.rs 历史断言一致）
    let fire = component(&output2, DamageType::Fire);
    assert_eq!(fire.avg(), 150.0);
    assert_eq!(fire.type_path, vec![DamageType::Fire]);
    // 物理不受元素 inc 影响
    let phys2 = component(&output2, DamageType::Physical);
    assert_eq!(phys2.avg(), 150.0);
}

/// 技能转换先于全局转换：技能 50% Phys→Cold，全局 50% Cold→Fire 链式。
/// 物理留 50%，冷 50% 中 50% 再转火，即冷 25%、火 25%。
#[test]
fn skill_conversion_chains_into_global_conversion() {
    let mut db = ModDb::new();
    // 技能：50% Phys→Cold
    db.add_mod(
        Modifier::number("SkillPhysicalDamageConvertToCold", ModType::Base, 50.0)
            .with_flags(ModFlags::ATTACK),
    );
    // 全局：50% Cold→Fire（作用于技能转出的 cold）
    db.add_mod(
        Modifier::number("ColdDamageConvertToFire", ModType::Base, 50.0)
            .with_flags(ModFlags::ATTACK),
    );

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());

    // base avg 150。phys 留 50% → 75；cold 50%*留50% → 37.5；fire 50%*转50% → 37.5
    let phys = component(&output, DamageType::Physical);
    let cold = component(&output, DamageType::Cold);
    let fire = component(&output, DamageType::Fire);
    assert!(
        (phys.avg() - 75.0).abs() < 0.5,
        "phys ~75, got {}",
        phys.avg()
    );
    assert!(
        (cold.avg() - 37.5).abs() < 0.5,
        "cold ~37.5, got {}",
        cold.avg()
    );
    assert!(
        (fire.avg() - 37.5).abs() < 0.5,
        "fire ~37.5, got {}",
        fire.avg()
    );
    // 火焰 type_path 经 Physical→Cold→Fire 链，应含三者
    assert!(fire.type_path.contains(&DamageType::Physical));
    assert!(fire.type_path.contains(&DamageType::Cold));
    assert!(fire.type_path.contains(&DamageType::Fire));
}
