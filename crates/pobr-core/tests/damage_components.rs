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

/// PoE2 暴击期望值测试（base_crit_bonus = PLAYER_BASE_CRIT_DAMAGE_BONUS=100，非 PoE1 的 50）。
///
/// PoE2：CritMultiplier BASE 50 → bonus = 100+50=150 → mult = 1+1.5 = 2.5
///         crit_avg_factor = 1 + 0.1*(2.5-1) = 1.15
///         non-crit avg sum = 150+30 = 180 → total_hit_avg = 180*1.15 = 207
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
    // CritMultiplier BASE 50 → PoE2 bonus = (100+50)/100 = 1.5 → crit_mult = 2.5
    db.add_mod(Modifier::number(
        "CriticalStrikeMultiplier",
        ModType::Base,
        50.0,
    ));

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());

    // 分量向量是「非暴击」命中分量，求和 = 150 + 30 = 180
    let sum_avg: f64 = output.damage_components.iter().map(|c| c.avg()).sum();
    assert_eq!(sum_avg, 180.0);
    // PoE2 total_hit_avg = 180 * (1 + 0.1*(2.5-1)) = 180 * 1.15 = 207
    assert_eq!(output.total_hit_avg, 207.0);
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

/// Bug#6 测试：ElementalDamage 共享 inc 组作用于三元素分量。
///
/// 出处：damage-scaling.md §核心叠加语义；PoB2 `typeFlags + modNames` ElementalDamage 展开。
#[test]
fn elemental_damage_inc_scales_all_three_elemental_components() {
    let mut db = ModDb::new();
    // 附加火/冰/电各 100
    db.add_mod(
        Modifier::number("FireDamageMin", ModType::Base, 100.0).with_flags(ModFlags::ATTACK),
    );
    db.add_mod(
        Modifier::number("FireDamageMax", ModType::Base, 100.0).with_flags(ModFlags::ATTACK),
    );
    db.add_mod(
        Modifier::number("ColdDamageMin", ModType::Base, 100.0).with_flags(ModFlags::ATTACK),
    );
    db.add_mod(
        Modifier::number("ColdDamageMax", ModType::Base, 100.0).with_flags(ModFlags::ATTACK),
    );
    db.add_mod(
        Modifier::number("LightningDamageMin", ModType::Base, 100.0).with_flags(ModFlags::ATTACK),
    );
    db.add_mod(
        Modifier::number("LightningDamageMax", ModType::Base, 100.0).with_flags(ModFlags::ATTACK),
    );
    // ElementalDamage INC 50 应作用于所有三元素
    db.add_mod(
        Modifier::number("ElementalDamage", ModType::Inc, 50.0).with_flags(ModFlags::ATTACK),
    );

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());

    let fire = component(&output, DamageType::Fire);
    let cold = component(&output, DamageType::Cold);
    let lightning = component(&output, DamageType::Lightning);
    // 各元素 100 * (1 + 0.5) = 150
    assert_eq!(fire.avg(), 150.0);
    assert_eq!(cold.avg(), 150.0);
    assert_eq!(lightning.avg(), 150.0);
    // 物理不受 ElementalDamage 影响
    let phys = component(&output, DamageType::Physical);
    assert_eq!(phys.avg(), 150.0); // base_input: 100-200 avg 150, no scaler
}

/// Bug#7 测试：DAMAGE_TYPES 顺序为 PoE2 转换链顺序 Phys→Lightning→Cold→Fire→Chaos。
///
/// 出处：damage-scaling.md §转换顺序、PoB2 CalcOffence.lua `dmgTypeList`。
#[test]
fn damage_types_array_follows_poe2_conversion_chain_order() {
    use pobr_core::calc::damage::DAMAGE_TYPES;
    assert_eq!(DAMAGE_TYPES[0], DamageType::Physical);
    assert_eq!(DAMAGE_TYPES[1], DamageType::Lightning);
    assert_eq!(DAMAGE_TYPES[2], DamageType::Cold);
    assert_eq!(DAMAGE_TYPES[3], DamageType::Fire);
    assert_eq!(DAMAGE_TYPES[4], DamageType::Chaos);
}

/// Bug#8 测试：AddedDamage MORE 只作用于外部 flat added，不乘武器自带 base。
///
/// 出处：damage-scaling.md §Added Damage Effectiveness；
///       PoB2 `addedMin * addedMult` 不包含 `source[...]`（武器/技能自带）。
#[test]
fn added_damage_effectiveness_only_scales_flat_added_not_weapon_base() {
    let mut db = ModDb::new();
    // AddedDamage MORE 200（效率 200% = 乘以 3.0 … MORE 200 → factor = 1+200/100 = 3.0）
    db.add_mod(Modifier::number("AddedDamage", ModType::More, 200.0).with_flags(ModFlags::ATTACK));
    // 附加火焰 10 flat
    db.add_mod(Modifier::number("FireDamageMin", ModType::Base, 10.0).with_flags(ModFlags::ATTACK));
    db.add_mod(Modifier::number("FireDamageMax", ModType::Base, 10.0).with_flags(ModFlags::ATTACK));

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());

    let fire = component(&output, DamageType::Fire);
    // fire flat=10 * eff_factor=3.0 = 30 avg
    assert_eq!(fire.avg(), 30.0);

    // 物理 base（来自武器 base_hit_min/max）不受 AddedDamage MORE 影响
    // flat added 物理=0 → phys = (base_hit + 0*eff) scaled = base_hit * scale (no INC/MORE)
    let phys = component(&output, DamageType::Physical);
    // PhysicalDamage MORE = 3.0 also applies to phys base? NO: AddedDamage MORE只乘added
    // phys base = (100 + 0*3) = 100 min, (200+0*3) = 200 max → avg 150
    assert_eq!(phys.avg(), 150.0);
}

// ---------------------------------------------------------------------------
// 04-01：Min<Type>Damage / Max<Type>Damage 分 min/max 独立 MORE 乘区
// （PoB2 CalcOffence.lua:138-139,153-154）
// ---------------------------------------------------------------------------

#[test]
fn min_max_type_more_scales_only_one_end() {
    // "35% less minimum Physical Damage" → MinPhysicalDamage MORE -35
    // "35% more maximum Physical Damage" → MaxPhysicalDamage MORE +35
    //   min = round(100 * (1 - 0.35)) = 65; max = round(200 * (1 + 0.35)) = 270
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("MinPhysicalDamage", ModType::More, -35.0));
    db.add_mod(Modifier::number("MaxPhysicalDamage", ModType::More, 35.0));

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());
    let phys = component(&output, DamageType::Physical);
    assert_eq!(phys.min, 65.0, "min 只被 MinPhysicalDamage 缩放");
    assert_eq!(phys.max, 270.0, "max 只被 MaxPhysicalDamage 缩放");
    assert_eq!(phys.avg(), 167.5);
}

#[test]
fn min_max_type_more_stacks_with_generic_inc_more() {
    // 通用 PhysicalDamage INC 50% → scale=1.5；MaxPhysicalDamage MORE +10% 只乘 max。
    //   min = round(100 * 1.5) = 150; max = round(200 * 1.5 * 1.10) = 330
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("PhysicalDamage", ModType::Inc, 50.0).with_flags(ModFlags::ATTACK));
    db.add_mod(Modifier::number("MaxPhysicalDamage", ModType::More, 10.0));

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());
    let phys = component(&output, DamageType::Physical);
    assert_eq!(phys.min, 150.0);
    assert_eq!(phys.max, 330.0);
}
