//! DamageComponent 向量集成测试：验证击中伤害按伤害类型拆分聚合，
//! 求和等于总击中伤害，且纯物理路径与旧实现完全一致（回归安全）。

use pobr_core::calc::output::OutputTable;
use pobr_core::calc::{MinimalInput, calculate_minimal};
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

/// 找到指定伤害类型的分量；不存在时 panic（让断言定位清晰）。
fn component(
    output: &pobr_core::calc::MinimalOutput,
    ty: DamageType,
) -> &pobr_core::calc::DamageComponent {
    output
        .damage_components
        .iter()
        .find(|c| c.damage_type == ty)
        .unwrap_or_else(|| panic!("missing {ty:?} damage component"))
}

#[test]
fn pure_physical_path_matches_legacy_total_hit_avg() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("PhysicalDamage", ModType::Inc, 50.0).with_flags(ModFlags::ATTACK));
    db.add_mod(
        Modifier::number("PhysicalDamage", ModType::More, 20.0).with_flags(ModFlags::ATTACK),
    );

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());

    // base avg 150 * (1 + 0.5) * 1.2 = 270
    assert_eq!(output.total_hit_avg, 270.0);
    // 唯一分量是物理，其 avg 即总击中伤害
    assert_eq!(output.damage_components.len(), 1);
    let phys = component(&output, DamageType::Physical);
    assert_eq!(phys.avg(), 270.0);
    assert_eq!(phys.min, 100.0 * 1.5 * 1.2);
    assert_eq!(phys.max, 200.0 * 1.5 * 1.2);
}

#[test]
fn physical_component_is_emitted_with_no_modifiers() {
    let db = ModDb::new();
    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());

    assert_eq!(output.total_hit_avg, 150.0);
    let phys = component(&output, DamageType::Physical);
    assert_eq!(phys.min, 100.0);
    assert_eq!(phys.max, 200.0);
    assert_eq!(phys.avg(), 150.0);
}

#[test]
fn fire_flat_added_contributes_to_total_hit() {
    let mut db = ModDb::new();
    // 附加火焰伤害（flat added），用 min/max 双词条表达
    db.add_mod(Modifier::number("FireDamageMin", ModType::Base, 30.0).with_flags(ModFlags::ATTACK));
    db.add_mod(Modifier::number("FireDamageMax", ModType::Base, 50.0).with_flags(ModFlags::ATTACK));

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());

    let phys = component(&output, DamageType::Physical);
    let fire = component(&output, DamageType::Fire);
    assert_eq!(phys.avg(), 150.0);
    assert_eq!(fire.min, 30.0);
    assert_eq!(fire.max, 50.0);
    assert_eq!(fire.avg(), 40.0);
    // 总击中伤害 = 物理 avg + 火焰 avg
    assert_eq!(output.total_hit_avg, 150.0 + 40.0);
}

#[test]
fn type_specific_inc_only_scales_its_own_component() {
    let mut db = ModDb::new();
    db.add_mod(
        Modifier::number("FireDamageMin", ModType::Base, 100.0).with_flags(ModFlags::ATTACK),
    );
    db.add_mod(
        Modifier::number("FireDamageMax", ModType::Base, 100.0).with_flags(ModFlags::ATTACK),
    );
    // 仅作用于火焰的 increased：通过 DamageType tag 限定
    db.add_mod(
        Modifier::number("FireDamage", ModType::Inc, 50.0)
            .with_flags(ModFlags::ATTACK)
            .with_tag(ModTag::DamageType(DamageType::Fire)),
    );

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());

    let phys = component(&output, DamageType::Physical);
    let fire = component(&output, DamageType::Fire);
    // 物理未被火焰 inc 影响
    assert_eq!(phys.avg(), 150.0);
    // 火焰 100 * (1 + 0.5) = 150
    assert_eq!(fire.avg(), 150.0);
    assert_eq!(output.total_hit_avg, 150.0 + 150.0);
}

#[test]
fn generic_damage_inc_scales_all_components() {
    let mut db = ModDb::new();
    db.add_mod(
        Modifier::number("FireDamageMin", ModType::Base, 100.0).with_flags(ModFlags::ATTACK),
    );
    db.add_mod(
        Modifier::number("FireDamageMax", ModType::Base, 100.0).with_flags(ModFlags::ATTACK),
    );
    // 通用 Damage inc 应同时作用于物理与火焰
    db.add_mod(Modifier::number("Damage", ModType::Inc, 100.0).with_flags(ModFlags::ATTACK));

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());

    let phys = component(&output, DamageType::Physical);
    let fire = component(&output, DamageType::Fire);
    // 物理 150 * 2 = 300；火焰 100 * 2 = 200
    assert_eq!(phys.avg(), 300.0);
    assert_eq!(fire.avg(), 200.0);
    assert_eq!(output.total_hit_avg, 500.0);
}

#[test]
fn components_sum_equals_total_hit_avg_with_crit() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("FireDamageMin", ModType::Base, 20.0).with_flags(ModFlags::ATTACK));
    db.add_mod(Modifier::number("FireDamageMax", ModType::Base, 40.0).with_flags(ModFlags::ATTACK));
    db.add_mod(Modifier::number(
        "CriticalStrikeChance",
        ModType::Base,
        10.0,
    ));
    db.add_mod(Modifier::number(
        "CriticalStrikeMultiplier",
        ModType::Base,
        50.0,
    ));

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());

    // total_hit_avg 含暴击平均因子；分量向量是「非暴击」的命中分量，
    // 其求和应等于 non-crit 总和（150 + 30 = 180）。
    let sum_avg: f64 = output.damage_components.iter().map(|c| c.avg()).sum();
    assert_eq!(sum_avg, 180.0);
    // total_hit_avg = 180 * crit_average_factor，crit_average=1+0.1*(2-1)=1.1
    assert_eq!(output.total_hit_avg, 198.0);
}

#[test]
fn output_table_exposes_damage_components() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("FireDamageMin", ModType::Base, 30.0).with_flags(ModFlags::ATTACK));
    db.add_mod(Modifier::number("FireDamageMax", ModType::Base, 50.0).with_flags(ModFlags::ATTACK));

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());
    let table = OutputTable::from(&output);

    assert_eq!(table.damage_components, output.damage_components);
    let phys = table
        .damage_components
        .iter()
        .find(|c| c.damage_type == DamageType::Physical)
        .unwrap();
    let fire = table
        .damage_components
        .iter()
        .find(|c| c.damage_type == DamageType::Fire)
        .unwrap();
    assert_eq!(phys.avg(), 150.0);
    assert_eq!(fire.avg(), 40.0);
}
