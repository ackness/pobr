use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb};

use super::{
    BreakdownTable, CalcError, Env, MinimalInput, OutputTable, ResistanceSuite, bleed_instance,
    calc_defence, calc_ehp, calc_skill_use_time, calculate_minimal_vs_enemy, ignite_instance,
    poison_instance, regen, reservation, shock_effect,
};

pub fn perform(env: &mut Env) -> Result<(), CalcError> {
    if env.player.level == 0 {
        return Err(CalcError::InvalidActorState(
            "player level must be greater than 0",
        ));
    }

    let mut input = MinimalInput::from(env.player.base);
    // 命中率的敌人闪避来源：优先用 enemy.mod_db 的 Evasion BASE（setup_env 注入，含档位倍率），
    // 回退到 enemy.base.evasion 标量（兼容直接构造 Env 的旧入口）。
    let enemy_evasion_from_db =
        env.enemy
            .mod_db
            .sum(ModType::Base, &env.cfg, &[ModName::from("Evasion")]);
    input.enemy_evasion = if enemy_evasion_from_db > 0.0 {
        enemy_evasion_from_db
    } else {
        env.enemy.base.evasion
    };
    let output =
        calculate_minimal_vs_enemy(&env.player.mod_db, &env.enemy.mod_db, &env.cfg, &input);
    env.player.output = OutputTable::from(&output);
    env.player.breakdown = BreakdownTable::from_steps(output.breakdown);
    calc_defence(&mut env.player, &env.cfg, env.enemy.base.accuracy);

    fill_mechanics(env);

    Ok(())
}

/// Fill 阶段：在基础 offence + defence 之上，把 skill-use-time / ailment / EHP /
/// reservation / regen / 防御几率写入 [`OutputTable`]。纯增量，不改既有字段。
fn fill_mechanics(env: &mut Env) {
    let db = &env.player.mod_db;
    let cfg = &env.cfg;

    // --- 技能使用时间 / 有效行动速率 ---
    let base_use_time = if env.player.base.action_rate > 0.0 {
        1.0 / env.player.base.action_rate
    } else {
        0.0
    };
    let is_channelling = cfg.condition("Channelling");
    let skill_use_time = calc_skill_use_time(db, cfg, base_use_time, 0.0, is_channelling);
    env.player.output.effective_action_rate = skill_use_time.effective_rate;
    env.player.output.skill_use_time = Some(skill_use_time);

    // --- 异常状态 DPS（基于非暴击分类型命中作为 pre-mitigation magnitude 源） ---
    let phys_hit = component_avg(&env.player.output.damage_components, DamageType::Physical);
    let fire_hit = component_avg(&env.player.output.damage_components, DamageType::Fire);
    let lightning_hit = component_avg(&env.player.output.damage_components, DamageType::Lightning);
    let chaos_phys_hit =
        phys_hit + component_avg(&env.player.output.damage_components, DamageType::Chaos);

    if phys_hit > 0.0 {
        env.player.output.bleed_dps = bleed_instance(phys_hit, db, cfg).magnitude_dps;
    }
    if fire_hit > 0.0 {
        env.player.output.ignite_dps = ignite_instance(fire_hit, db, cfg).magnitude_dps;
    }
    if chaos_phys_hit > 0.0 {
        env.player.output.poison_dps = poison_instance(chaos_phys_hit, db, cfg).magnitude_dps;
    }
    let ailment_threshold = pool_or(env.player.output.life, 1.0);
    env.player.output.shock_effect = shock_effect(lightning_hit, ailment_threshold);

    // --- EHP / max hit ---
    let resistances = ResistanceSuite {
        physical_pdr: physical_pdr_fraction(db, cfg),
        fire: env.player.output.fire_resistance,
        cold: env.player.output.cold_resistance,
        lightning: env.player.output.lightning_resistance,
        chaos: db.sum(ModType::Base, cfg, &[ModName::from("ChaosResistance")]),
    };
    let reference_hit = (env.player.output.life + env.player.output.energy_shield).max(1.0);
    let ehp = calc_ehp(
        env.player.output.life,
        env.player.output.energy_shield,
        env.player.output.mana,
        &resistances,
        env.player.output.armour,
        reference_hit,
    );
    env.player.output.physical_max_hit = ehp.physical_max_hit;
    env.player.output.fire_max_hit = ehp.fire_max_hit;
    env.player.output.cold_max_hit = ehp.cold_max_hit;
    env.player.output.lightning_max_hit = ehp.lightning_max_hit;
    env.player.output.chaos_max_hit = ehp.chaos_max_hit;
    env.player.output.total_ehp = ehp.total_ehp;

    // --- 预留 / 剩余 ---
    let life_res = reservation(
        env.player.output.life,
        db.sum(ModType::Base, cfg, &[ModName::from("LifeReserved")]),
        db.sum(ModType::Inc, cfg, &[ModName::from("LifeReservedPercent")]),
    );
    let mana_res = reservation(
        env.player.output.mana,
        db.sum(ModType::Base, cfg, &[ModName::from("ManaReserved")]),
        db.sum(ModType::Inc, cfg, &[ModName::from("ManaReservedPercent")]),
    );
    env.player.output.life_reserved = life_res.reserved;
    env.player.output.life_unreserved = life_res.unreserved;
    env.player.output.mana_reserved = mana_res.reserved;
    env.player.output.mana_unreserved = mana_res.unreserved;

    // --- 每秒恢复 ---
    env.player.output.life_regen = stat_regen(db, cfg, env.player.output.life, "LifeRegen");
    env.player.output.mana_regen = stat_regen(db, cfg, env.player.output.mana, "ManaRegen");
    env.player.output.energy_shield_regen = stat_regen(
        db,
        cfg,
        env.player.output.energy_shield,
        "EnergyShieldRegen",
    );

    // --- 防御几率类 ---
    env.player.output.block_chance =
        super::block_chance(db.sum(ModType::Base, cfg, &[ModName::from("BlockChance")]));
    env.player.output.spell_block_chance =
        super::block_chance(db.sum(ModType::Base, cfg, &[ModName::from("SpellBlockChance")]));
    env.player.output.spell_suppression_chance = super::suppression_chance(db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("SpellSuppressionChance")],
    ));
}

/// 取某伤害类型分量的平均击中值（无该分量返回 0）。
fn component_avg(components: &[super::DamageComponent], damage_type: DamageType) -> f64 {
    components
        .iter()
        .find(|component| component.damage_type == damage_type)
        .map_or(0.0, super::DamageComponent::avg)
}

/// 物理减伤固定加成（fraction），来自 `PhysicalDamageReduction` Base（百分点 → fraction）。
fn physical_pdr_fraction(db: &ModDb, cfg: &CalcConfig) -> f64 {
    let pct = db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("PhysicalDamageReduction")],
    );
    (pct / 100.0).clamp(0.0, 0.9)
}

/// 某池子的每秒恢复：`base_flat + pool * %regen/100`，再吃 `<stat>Rate` inc/more。
fn stat_regen(db: &ModDb, cfg: &CalcConfig, pool: f64, stat: &str) -> f64 {
    let flat = db.sum(ModType::Base, cfg, &[ModName::from(stat)]);
    let percent = db.sum(
        ModType::Base,
        cfg,
        &[ModName::from(format!("{stat}Percent"))],
    );
    let rate = [ModName::from(format!("{stat}Rate"))];
    let inc = db.sum(ModType::Inc, cfg, &rate);
    let more = db.more(cfg, &rate);
    regen(pool, flat, percent, inc, more)
}

fn pool_or(value: f64, fallback: f64) -> f64 {
    if value > 0.0 { value } else { fallback }
}
