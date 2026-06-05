//! 召唤物域集成测试（greenfield，验证 build_minion_context 三通道 + 怪物式 scaling）。
//!
//! 出处：agent-docs/minions.md；PoB2 CalcPerform.lua / CalcActiveSkill.lua / Misc.lua。

use pobr_core::calc::minion::{
    AttributeInfusion, MinionData, MinionInput, MinionModifierEntry, build_minion_context,
    derive_minion_base_stats, minion_level_from_gem_level, minion_modifier_applies,
};
use pobr_core::{CalcConfig, Modifier};
use pobr_data::prelude::*;

fn empty_input(gem_level: u32, data: MinionData) -> MinionInput {
    MinionInput {
        gem_level,
        data,
        minion_modifiers: vec![],
        ally_buff_mods: vec![],
        attribute_infusion: AttributeInfusion::default(),
        minion_type: None,
    }
}

#[test]
fn gem_level_table_maps_per_pob2() {
    // minionLevelTable = {2,4,…,80}（PoB2 Misc.lua）。
    assert_eq!(minion_level_from_gem_level(1), 2);
    assert_eq!(minion_level_from_gem_level(10), 20);
    assert_eq!(minion_level_from_gem_level(40), 80);
}

#[test]
fn base_stats_use_monster_table_times_normalizer() {
    // Zombie-like：life=0.7 归一化乘数。基础生命 = 怪物等级表[level] × 0.7。
    let data = MinionData {
        life: 0.7,
        ..MinionData::default()
    };
    let base = derive_minion_base_stats(20, &data); // 怪物等级 40
    let row = MonsterScalingRow::at_level(40);
    let expected = (row.life as f64 * 0.7 * 1_000_000_000.0).round() / 1_000_000_000.0;
    assert!((base.life - expected).abs() < 1e-6);
    assert!(base.life > 0.0);
}

#[test]
fn crit_multiplier_base_is_100_not_player_default() {
    // 召唤物爆伤基础 = 怪物 30 + 内禀 70 = 100；走 ModDb 也能查到。
    let ctx = build_minion_context(&empty_input(20, MinionData::default()));
    let cfg = CalcConfig::attack();
    let crit_mult = ctx
        .mod_db
        .sum(ModType::Base, &cfg, &[ModName::from("CritMultiplier")]);
    assert_eq!(crit_mult, 100.0);
    assert_eq!(base_crit(&ctx), 100.0);
}

fn base_crit(ctx: &pobr_core::calc::minion::MinionContext) -> f64 {
    ctx.base.crit_multiplier_base
}

#[test]
fn minion_always_hit_flag_present() {
    let ctx = build_minion_context(&empty_input(20, MinionData::default()));
    assert!(
        ctx.mod_db
            .flag(&CalcConfig::attack(), ModName::from("CannotBeEvaded"))
    );
    assert!(ctx.base.always_hit);
}

#[test]
fn channel_1_minion_modifier_injected_and_queryable() {
    // 通道 1：「Minions deal 50% increased Damage」展开成 Damage INC 50，注入召唤物 ModDb。
    let inner = Modifier::number("Damage", ModType::Inc, 50.0);
    let mut input = empty_input(20, MinionData::default());
    input.minion_modifiers = vec![MinionModifierEntry {
        inner,
        minion_type: None,
    }];
    let ctx = build_minion_context(&input);
    let inc = ctx.mod_db.sum(
        ModType::Inc,
        &CalcConfig::attack(),
        &[ModName::from("Damage")],
    );
    assert_eq!(inc, 50.0);
}

#[test]
fn channel_1_type_limited_modifier_filtered_out() {
    // type 限定为 "Zombie"，但召唤物类型为 None → 不注入。
    let inner = Modifier::number("Damage", ModType::Inc, 50.0);
    let mut input = empty_input(20, MinionData::default());
    input.minion_modifiers = vec![MinionModifierEntry {
        inner,
        minion_type: Some("Zombie".into()),
    }];
    let ctx = build_minion_context(&input);
    let inc = ctx.mod_db.sum(
        ModType::Inc,
        &CalcConfig::attack(),
        &[ModName::from("Damage")],
    );
    assert_eq!(inc, 0.0);

    // 类型相符则注入。
    let inner2 = Modifier::number("Damage", ModType::Inc, 50.0);
    input.minion_modifiers = vec![MinionModifierEntry {
        inner: inner2,
        minion_type: Some("Zombie".into()),
    }];
    input.minion_type = Some("Zombie".into());
    let ctx2 = build_minion_context(&input);
    let inc2 = ctx2.mod_db.sum(
        ModType::Inc,
        &CalcConfig::attack(),
        &[ModName::from("Damage")],
    );
    assert_eq!(inc2, 50.0);
}

#[test]
fn applies_predicate_matches_pob2_semantics() {
    let no_type = MinionModifierEntry {
        inner: Modifier::number("Damage", ModType::Inc, 1.0),
        minion_type: None,
    };
    assert!(minion_modifier_applies(&no_type, None));
    assert!(minion_modifier_applies(&no_type, Some("Zombie")));

    let typed = MinionModifierEntry {
        inner: Modifier::number("Damage", ModType::Inc, 1.0),
        minion_type: Some("Zombie".into()),
    };
    assert!(minion_modifier_applies(&typed, Some("Zombie")));
    assert!(!minion_modifier_applies(&typed, Some("Skeleton")));
    assert!(!minion_modifier_applies(&typed, None));
}

#[test]
fn channel_2_ally_buff_mods_injected() {
    // 通道 2：盟友 buff（已按召唤物 BuffEffectOnSelf 缩放的 mod）。
    let mut input = empty_input(20, MinionData::default());
    input.ally_buff_mods = vec![Modifier::number("AttackSpeed", ModType::Inc, 20.0)];
    let ctx = build_minion_context(&input);
    let spd = ctx.mod_db.sum(
        ModType::Inc,
        &CalcConfig::attack(),
        &[ModName::from("AttackSpeed")],
    );
    assert_eq!(spd, 20.0);
}

#[test]
fn channel_3_strength_infusion_injects_base_str() {
    // 通道 3：StrengthAddedToMinions → 召唤物 Str BASE = 玩家 Str。
    let mut input = empty_input(20, MinionData::default());
    input.attribute_infusion = AttributeInfusion {
        player_strength: 300.0,
        player_dexterity: 0.0,
        strength_added: true,
        half_strength_added: false,
        dexterity_added: false,
    };
    let ctx = build_minion_context(&input);
    let str_base = ctx.mod_db.sum(
        ModType::Base,
        &CalcConfig::attack(),
        &[ModName::from("Str")],
    );
    assert_eq!(str_base, 300.0);
}

#[test]
fn channel_3_half_strength_infusion() {
    let mut input = empty_input(20, MinionData::default());
    input.attribute_infusion = AttributeInfusion {
        player_strength: 300.0,
        player_dexterity: 0.0,
        strength_added: false,
        half_strength_added: true,
        dexterity_added: false,
    };
    let ctx = build_minion_context(&input);
    let str_base = ctx.mod_db.sum(
        ModType::Base,
        &CalcConfig::attack(),
        &[ModName::from("Str")],
    );
    assert_eq!(str_base, 150.0);
}

#[test]
fn player_ordinary_mods_do_not_leak_into_minion() {
    // 铁律：玩家普通词条默认不进召唤物库。空通道 → 召唤物只有内禀/基础属性。
    let ctx = build_minion_context(&empty_input(20, MinionData::default()));
    let inc = ctx.mod_db.sum(
        ModType::Inc,
        &CalcConfig::attack(),
        &[ModName::from("Damage")],
    );
    assert_eq!(inc, 0.0);
}

#[test]
fn weapon_data_respects_base_damage_ignores_attack_speed() {
    // base_damage_ignores_attack_speed=true → 基础伤害不乘 attack_time。
    let mut data = MinionData {
        damage: 1.0,
        attack_time: 0.5,
        damage_spread: 0.0,
        base_damage_ignores_attack_speed: true,
        ..MinionData::default()
    };
    let ignoring = derive_minion_base_stats(20, &data);

    data.base_damage_ignores_attack_speed = false;
    let scaling = derive_minion_base_stats(20, &data);

    // 不忽略攻速时伤害 = 忽略时 × attack_time(0.5) → 更低。
    assert!(scaling.weapon.physical_max < ignoring.weapon.physical_max);
    assert!((scaling.weapon.physical_max - ignoring.weapon.physical_max * 0.5).abs() < 1e-6);
    // 攻速 = 1/attack_time = 2/s。
    assert!((ignoring.weapon.attack_rate - 2.0).abs() < 1e-9);
}

#[test]
fn energy_shield_derived_from_life_fraction() {
    // energy_shield=0.15 → ES 基础 = life × 0.15。
    let data = MinionData {
        life: 1.0,
        energy_shield: 0.15,
        ..MinionData::default()
    };
    let base = derive_minion_base_stats(20, &data);
    assert!((base.energy_shield - base.life * 0.15).abs() < 1e-6);
    assert!(base.energy_shield > 0.0);
}
