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

/// PoE2 口径（无转换源 double-dip）：50% Phys→Fire 后，火焰分量只吃**最终类型** FireDamage inc
/// 与 ElementalDamage inc，**不**吃转换源 PhysicalDamage inc。一手依据：PoB2 `CalcOffence.lua`
/// `calcDamage(..., damageType, 0)`（:3990，typeFlags 传 0，仅含最终类型）+ headless oracle 逐分量验证。
#[test]
fn converted_fire_uses_final_type_inc_only_no_conversion_source_double_dip() {
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

    // 火焰分量：base 50-100，inc = Fire100 + Elem50 = 150（**不含**转换源 Phys100）→ ×2.5
    // min 50*2.5 = 125, max 100*2.5 = 250
    let fire = component(&output, DamageType::Fire);
    assert_eq!(fire.min, 125.0);
    assert_eq!(fire.max, 250.0);
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
    // 闪电分量 type_path 含来源 Physical（仅用于归因/展示；inc 聚合只按最终类型，不 double-dip）
    assert!(lightning.type_path.contains(&DamageType::Physical));
    assert!(lightning.type_path.contains(&DamageType::Lightning));
    // 总伤害 = 物理 150 + 闪电 37.5 = 187.5（gain 是净增）
    let total: f64 = output.damage_components.iter().map(|c| c.avg()).sum();
    assert_eq!(total, 187.5);
}

/// PoE2 口径：物理 gain as 火焰后，额外火焰分量只吃**最终类型** FireDamage inc，**不**吃来源
/// PhysicalDamage inc（与转换同口径，PoB2 calcDamage typeFlags 仅含最终类型；oracle 验证）。
#[test]
fn gain_as_extra_fire_uses_final_type_inc_only() {
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
    // 额外火焰：50% 物理 base = 50-100，inc = Fire100（**不含**来源 Phys100）→ ×2 = 100-200
    let fire = component(&output, DamageType::Fire);
    assert_eq!(fire.min, 100.0);
    assert_eq!(fire.max, 200.0);
}

/// 回归（type_path-last-not-final-type）：当某分量的 `type_path` 含**链序晚于自身**的类型时，
/// inc/more 聚合必须按分量自身 `damage_type` 而非 `type_path` 末位。
///
/// 场景：物理 100% gain as Cold（Cold 分量 path 起含 Physical），再叠加 Fire gain as Cold
/// （把 Fire 推入 Cold 的 path）。链序 [Phys,Lightning,Cold,Fire] 下 Cold 的 path 排序后末位
/// = Fire。旧实现误用 `FireDamage` inc 聚合 Cold 分量；修正后须用 `ColdDamage`。
/// PoB2 `calcDamage` typeFlags 仅含最终 damageType——oracle（ice-shot/deadeye）逐分量验证。
#[test]
fn component_uses_own_type_inc_not_path_last() {
    let mut db = ModDb::new();
    // 物理 100% gain as Cold（Cold 得物理 base，path 含 Physical）。
    db.add_mod(
        Modifier::number("PhysicalDamageGainAsCold", ModType::Base, 100.0)
            .with_flags(ModFlags::ATTACK),
    );
    // Fire flat base（使 Fire→Cold gain 有源，从而把 Fire 推入 Cold 的 type_path）。
    db.add_mod(Modifier::number("FireDamageMin", ModType::Base, 10.0).with_flags(ModFlags::ATTACK));
    db.add_mod(Modifier::number("FireDamageMax", ModType::Base, 20.0).with_flags(ModFlags::ATTACK));
    // Fire 100% gain as Cold——把 Fire 推入 Cold 的 type_path。
    db.add_mod(
        Modifier::number("FireDamageGainAsCold", ModType::Base, 100.0).with_flags(ModFlags::ATTACK),
    );
    // ColdDamage inc 100：仅当按分量自身类型聚合时 Cold 才吃到。
    db.add_mod(
        Modifier::number("ColdDamage", ModType::Inc, 100.0)
            .with_flags(ModFlags::ATTACK)
            .with_tag(ModTag::DamageType(DamageType::Cold)),
    );
    // FireDamage inc 999：若误用 path 末位（Fire），Cold 会被这个巨大值污染。
    db.add_mod(
        Modifier::number("FireDamage", ModType::Inc, 999.0)
            .with_flags(ModFlags::ATTACK)
            .with_tag(ModTag::DamageType(DamageType::Fire)),
    );

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());

    let cold = component(&output, DamageType::Cold);
    // Cold base = 物理 gain(100-200) + 火焰 gain(10-20) = 110-220，吃 ColdDamage inc 100 →
    // ×2 = 220-440。若误用 path 末位 FireDamage inc 999 → ×10.99（断言会失败）。
    assert!(cold.type_path.contains(&DamageType::Fire), "path 应含 Fire");
    assert_eq!(
        cold.min, 220.0,
        "Cold 须按 ColdDamage inc 聚合，非 path 末位 Fire"
    );
    assert_eq!(cold.max, 440.0);
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

/// random element 档折叠（vendor CalcOffence.lua:1175-1200：
/// `DamageGainAsRandom BASE n` 在 physMode=AVERAGE（configInput 缺省）下展开为
/// `DamageGainAs{Fire,Cold,Lightning} BASE n/3`——PoBR 在 build_gain_matrix 折叠
/// 同口径；druid-oracle ember-fusillade 的 Relentless Vindicator 树点实例）。
#[test]
fn random_element_gain_as_splits_average_across_elements() {
    let mut db = ModDb::new();
    db.add_mod(
        Modifier::number("DamageGainAsRandom", ModType::Base, 30.0).with_flags(ModFlags::ATTACK),
    );

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());

    // 物理源 100-200 不扣减；三元素各 gain 10%（30/3）。
    let phys = component(&output, DamageType::Physical);
    assert_eq!(phys.min, 100.0);
    assert_eq!(phys.max, 200.0);
    for ty in [DamageType::Fire, DamageType::Cold, DamageType::Lightning] {
        let c = component(&output, ty);
        assert_eq!(c.min, 10.0, "{ty:?} min = 100 × 10%");
        assert_eq!(c.max, 20.0, "{ty:?} max = 200 × 10%");
    }
    assert!(find(&output, DamageType::Chaos).is_none_or(|c| c.avg() == 0.0));
}

/// PhysicalDamageGainAsRandom 仅作用于物理源行（vendor 展开为
/// `PhysicalDamageGainAs<Elem>`；CalcOffence.lua:1193-1200）。
#[test]
fn physical_random_gain_as_only_from_physical_source() {
    let mut db = ModDb::new();
    db.add_mod(
        Modifier::number("PhysicalDamageGainAsRandom", ModType::Base, 30.0)
            .with_flags(ModFlags::ATTACK),
    );
    // 非物理源：flat 火焰附加 60-60（不应再被 phys-random 二次放大）。
    db.add_mod(Modifier::number("FireDamageMin", ModType::Base, 60.0).with_flags(ModFlags::ATTACK));
    db.add_mod(Modifier::number("FireDamageMax", ModType::Base, 60.0).with_flags(ModFlags::ATTACK));

    let output = calculate_minimal(&db, &CalcConfig::attack(), &base_input());

    // fire = flat 60 + phys 100×10% = 70 / 60 + 200×10% = 80。
    let fire = component(&output, DamageType::Fire);
    assert_eq!(fire.min, 70.0);
    assert_eq!(fire.max, 80.0);
    // cold/lightning 仅来自 phys 源 10%。
    let cold = component(&output, DamageType::Cold);
    assert_eq!(cold.min, 10.0);
    assert_eq!(cold.max, 20.0);
}
