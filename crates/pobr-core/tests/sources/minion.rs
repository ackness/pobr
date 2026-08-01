//! Minion-domain integration tests (greenfield, verifying build_minion_context's
//! three channels + monster-style scaling).
//!
//! Source: agent-docs/minions.md; PoB2 CalcPerform.lua / CalcActiveSkill.lua / Misc.lua.

use pobr_core::calc::minion::{
    AttributeInfusion, MinionData, MinionInput, MinionModifierEntry, build_minion_context,
    build_minion_context_from_def, derive_minion_base_stats, minion_level_from_gem_level,
    minion_modifier_applies, write_summoned_minion_multipliers,
};
use pobr_core::{CalcConfig, ModDb, Modifier};
use pobr_data::minion::{
    minion_def_raging_spirit, minion_def_skeletal_storm_mage, minion_def_skeletal_warrior,
    minion_def_zombie,
};
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
    assert_eq!(minion_level_from_gem_level(1), 2);
    assert_eq!(minion_level_from_gem_level(10), 20);
    assert_eq!(minion_level_from_gem_level(40), 80);
}

#[test]
fn base_stats_use_ally_life_table_times_normalizer() {
    // As of #12, minion life goes through the ally table + floor (vendor
    // CalcPerform.lua:1046 `m_floor(monsterAllyLifeTable[level] x minionData.life)`),
    // no longer the enemy monsterLifeTable. armour/evasion still use the enemy
    // table (same source as vendor).
    let data = MinionData {
        life: 0.7,
        ..MinionData::default()
    };
    let base = derive_minion_base_stats(20, &data);
    let expected = (pobr_data::monster::monster_ally_life(40) as f64 * 0.7).floor();
    assert!((base.life - expected).abs() < 1e-6);
    assert!(base.life > 0.0);
}

#[test]
fn crit_multiplier_base_is_100_not_player_default() {
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

    assert!(scaling.weapon.physical_max < ignoring.weapon.physical_max);
    assert!((scaling.weapon.physical_max - ignoring.weapon.physical_max * 0.5).abs() < 1e-6);
    assert!((ignoring.weapon.attack_rate - 2.0).abs() < 1e-9);
}

#[test]
fn energy_shield_derived_from_life_fraction() {
    let data = MinionData {
        life: 1.0,
        energy_shield: 0.15,
        ..MinionData::default()
    };
    let base = derive_minion_base_stats(20, &data);
    assert!((base.energy_shield - base.life * 0.15).abs() < 1e-6);
    assert!(base.energy_shield > 0.0);
}

// Integration tests driven by the MinionDef catalog schema
// Source: PoB2 src/Data/Minions.lua; agent-docs/minions.md §1 / §4.

#[test]
fn build_context_from_zombie_def_matches_manual() {
    // build_minion_context_from_def(zombie, gem=20) matches a manually filled
    // MinionData.
    let def = minion_def_zombie();
    let ctx_def =
        build_minion_context_from_def(&def, 20, vec![], vec![], AttributeInfusion::default());
    let data = MinionData {
        life: 0.7,
        damage: 0.75,
        damage_spread: 0.3,
        attack_time: 1.25,
        crit_chance: 5.0,
        base_damage_ignores_attack_speed: true,
        ..MinionData::default()
    };
    let ctx_manual = build_minion_context(&empty_input(20, data));

    assert_eq!(ctx_def.base.life, ctx_manual.base.life);
    assert_eq!(ctx_def.base.level, ctx_manual.base.level);
    assert_eq!(
        ctx_def.base.crit_multiplier_base,
        ctx_manual.base.crit_multiplier_base
    );
    assert!((ctx_def.base.weapon.physical_min - ctx_manual.base.weapon.physical_min).abs() < 1e-9);
    assert!((ctx_def.base.weapon.physical_max - ctx_manual.base.weapon.physical_max).abs() < 1e-9);
}

#[test]
fn build_context_from_storm_mage_def_has_es_and_lightning_resist() {
    // Skeletal storm mage: energyShield=0.15, lightningResist=50.
    let def = minion_def_skeletal_storm_mage();
    let ctx = build_minion_context_from_def(&def, 20, vec![], vec![], AttributeInfusion::default());
    assert!(ctx.base.energy_shield > 0.0);
    let cfg = CalcConfig::attack();
    let lr = ctx
        .mod_db
        .sum(ModType::Base, &cfg, &[ModName::from("LightningResist")]);
    assert!(
        (lr - 50.0).abs() < 1e-6,
        "lightning resist should be 50, got {lr}"
    );
}

#[test]
fn build_context_from_raging_spirit_life_matches_pob2() {
    // Raging spirit: life=0.25, gem_level=20 -> monster level 40.
    let def = minion_def_raging_spirit();
    let ctx = build_minion_context_from_def(&def, 20, vec![], vec![], AttributeInfusion::default());
    // #12: life goes through the ally table + floor (vendor CalcPerform.lua:1046).
    let expected = (pobr_data::monster::monster_ally_life(40) as f64 * 0.25).floor();
    assert!(
        (ctx.base.life - expected).abs() < 1e-6,
        "RagingSpirit life: expected {expected}, got {}",
        ctx.base.life
    );
}

#[test]
fn build_context_from_skeletal_warrior_armour_normalizer() {
    // Skeletal warrior: armour=0.5.
    let def = minion_def_skeletal_warrior();
    let ctx = build_minion_context_from_def(&def, 20, vec![], vec![], AttributeInfusion::default());
    let row = MonsterScalingRow::at_level(40);
    let expected = (row.armour as f64 * 0.5 * 1e9).round() / 1e9;
    assert!(
        (ctx.base.armour - expected).abs() < 1e-6,
        "SkeletalWarrior armour: expected {expected}, got {}",
        ctx.base.armour
    );
}

#[test]
fn build_context_from_def_type_filter_works() {
    // Type-scoped to "RaisedSkeletonWarriors" -> injected for the skeletal warrior,
    // not for the zombie.
    let def_warrior = minion_def_skeletal_warrior();
    let def_zombie = minion_def_zombie();
    let entry = MinionModifierEntry {
        inner: Modifier::number("Damage", ModType::Inc, 25.0),
        minion_type: Some("RaisedSkeletonWarriors".into()),
    };

    let ctx_warrior = build_minion_context_from_def(
        &def_warrior,
        20,
        vec![entry.clone()],
        vec![],
        AttributeInfusion::default(),
    );
    let inc_warrior = ctx_warrior.mod_db.sum(
        ModType::Inc,
        &CalcConfig::attack(),
        &[ModName::from("Damage")],
    );
    assert_eq!(inc_warrior, 25.0, "warrior should receive the modifier");

    let ctx_zombie = build_minion_context_from_def(
        &def_zombie,
        20,
        vec![entry],
        vec![],
        AttributeInfusion::default(),
    );
    let inc_zombie = ctx_zombie.mod_db.sum(
        ModType::Inc,
        &CalcConfig::attack(),
        &[ModName::from("Damage")],
    );
    assert_eq!(
        inc_zombie, 0.0,
        "zombie should not receive skeleton-only modifier"
    );
}

#[test]
fn summoned_minion_multipliers_integration() {
    // limit=3 -> in the player ModDb, SummonedMinion = MinionPresenceCount = 3.
    let mut player_db = ModDb::new();
    write_summoned_minion_multipliers(&mut player_db, 3, "RaisedSkeletonWarriors");
    let cfg = CalcConfig::attack();
    let summ = player_db.sum(
        ModType::Base,
        &cfg,
        &[ModName::from("Multiplier:SummonedMinion")],
    );
    let presence = player_db.sum(
        ModType::Base,
        &cfg,
        &[ModName::from("Multiplier:MinionPresenceCount")],
    );
    assert_eq!(summ, 3.0, "SummonedMinion multiplier should be limit=3");
    assert_eq!(
        presence, 3.0,
        "MinionPresenceCount multiplier should be limit=3"
    );
}

#[test]
fn summoned_minion_multipliers_stack_multiple_types() {
    // Skeleton limit=3 + zombie limit=2 -> SummonedMinion = 5 (BASE mods add up).
    let mut player_db = ModDb::new();
    write_summoned_minion_multipliers(&mut player_db, 3, "Skeleton");
    write_summoned_minion_multipliers(&mut player_db, 2, "Zombie");
    let cfg = CalcConfig::attack();
    let summ = player_db.sum(
        ModType::Base,
        &cfg,
        &[ModName::from("Multiplier:SummonedMinion")],
    );
    assert_eq!(
        summ, 5.0,
        "multiple minion types should stack in SummonedMinion"
    );
}
