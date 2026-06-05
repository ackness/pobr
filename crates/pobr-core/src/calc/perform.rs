use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb};

use super::ailment::{
    AilmentSource, StackConfig, ailment_effect_mod, ailment_rate_mod, apply_dot_dps_cap,
    bleed_traced, chill_traced, cross_type_source_hit, electrocute_poise_buildup_traced,
    freeze_poise_buildup_traced, ignite_traced, poison_traced, shock_traced,
    stacking_ailment_dps_traced,
};
use super::skill_mechanics::{
    calc_aoe, calc_cooldown, calc_life_cost, calc_mana_cost, calc_projectile_count,
    calc_spirit_reservation,
};
use super::trigger::{calc_cwc_trigger_rate_traced, resolve_trigger_rate_traced};
use super::{
    BreakdownTable, CalcError, Env, LeechResource, MinimalInput, MinionOutput, OutputTable,
    RecoupResource, ResistanceSuite, calc_avoidance, calc_crit_extra_reduction, calc_defence,
    calc_ehp, calc_es_recharge, calc_leech_from_db, calc_recoup_from_db, calc_regen,
    calc_skill_use_time, calc_taken_multi_suite, calculate_minimal_vs_enemy, enemy_crit_effect,
    es_recharge_per_second, reservation, resolve_all_charges,
};
use crate::{TraceGraph, TraceOperation};

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
    // 异常状态：几率 + 暴击加权 + magnitude + effMult（几率 × DoT 期望值口径）。
    // 单独成段，避免与 fill_mechanics 内 player.mod_db 的不可变借用冲突。
    fill_ailments(env);

    // 召唤物（Lane4）：每个召唤物是独立 Actor，复用玩家同款 offence/defence 管线。
    // 无召唤物时该段空转，行为与无此字段时完全一致（向后兼容）。
    perform_minions(env);

    Ok(())
}

/// 对每个召唤物跑同一套 offence/defence 管线，并把关键输出快照收集到玩家
/// `OutputTable.minions`。召唤物复用 `calculate_minimal_vs_enemy` + `calc_defence`，
/// 不另写公式。召唤物对敌人的命中沿用玩家敌人配置（同一 `env.enemy`）。
fn perform_minions(env: &mut Env) {
    if env.minions.is_empty() {
        return;
    }

    // 召唤物数量上限（玩家 `Multiplier:SummonedMinion`，由 add_minion_from_def 写入）。
    // 把它注入 cfg 的 multiplier，使召唤物 `Damage per Summoned Minion` 等词条可引用（PoB2）。
    // 无该 multiplier 时为 0（不影响任何输出，向后兼容）。
    let minion_limit = env.player.mod_db.sum(
        ModType::Base,
        &env.cfg,
        &[ModName::from("Multiplier:SummonedMinion")],
    );
    let minion_cfg = if minion_limit > 0.0 {
        env.cfg
            .clone()
            .with_multiplier("SummonedMinion", minion_limit)
            .with_multiplier("MinionPresenceCount", minion_limit)
    } else {
        env.cfg.clone()
    };

    // 跨 Actor 归因：玩家来源（数量上限）→ 召唤物输出，建一个 source 节点供 trace DAG 连接。
    let mut trace = TraceGraph::new();
    let player_limit_node = trace.add_source_node(
        "summoned minion limit (player)",
        minion_limit,
        SourceId::new(SourceKind::GameConstant, "minion.limit"),
    );

    let mut snapshots = Vec::with_capacity(env.minions.len());
    for minion in &mut env.minions {
        let mut input = MinimalInput::from(minion.base);
        // 召唤物命中敌人：与玩家一致，敌方闪避优先取 enemy.mod_db 的 Evasion BASE。
        let enemy_evasion_from_db =
            env.enemy
                .mod_db
                .sum(ModType::Base, &minion_cfg, &[ModName::from("Evasion")]);
        input.enemy_evasion = if enemy_evasion_from_db > 0.0 {
            enemy_evasion_from_db
        } else {
            env.enemy.base.evasion
        };

        let output =
            calculate_minimal_vs_enemy(&minion.mod_db, &env.enemy.mod_db, &minion_cfg, &input);
        minion.output = OutputTable::from(&output);
        minion.breakdown = BreakdownTable::from_steps(output.breakdown);
        calc_defence(minion, &minion_cfg, env.enemy.base.accuracy);

        // 跨 Actor trace 边：玩家数量上限 → 本召唤物 DPS 输出（player-source → minion-output）。
        let minion_dps_node =
            trace.add_node("minion dps", minion.output.dps, TraceOperation::Aggregate);
        trace.add_edge(player_limit_node, minion_dps_node);

        snapshots.push(MinionOutput {
            level: minion.level as u32,
            dps: minion.output.dps,
            life: minion.output.life,
            armour: minion.output.armour,
            evasion: minion.output.evasion,
            energy_shield: minion.output.energy_shield,
        });
    }
    env.player.output.minions = snapshots;
}

/// Fill 阶段：在基础 offence + defence 之上，把 skill-use-time / ailment / EHP /
/// reservation / regen / 防御几率写入 [`OutputTable`]。纯增量，不改既有字段。
fn fill_mechanics(env: &mut Env) {
    // 敌人暴击几率/爆伤先行读出（避免后续 player.mod_db 可变借用与 enemy 不可变借用冲突）。
    let enemy_crit_chance =
        env.enemy
            .mod_db
            .sum(ModType::Base, &env.cfg, &[ModName::from("CritChance")]);
    let enemy_crit_damage =
        env.enemy
            .mod_db
            .sum(ModType::Base, &env.cfg, &[ModName::from("CritMultiplier")]);

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

    // --- 每秒恢复（Lane A：calc_regen 行为超集，含 XRecoveryRate 全局恢复速率）---
    env.player.output.life_regen = calc_regen(db, cfg, env.player.output.life, "LifeRegen");
    env.player.output.mana_regen = calc_regen(db, cfg, env.player.output.mana, "ManaRegen");
    env.player.output.energy_shield_regen = calc_regen(
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

    // --- ES 充能（Lane2：充能与再生独立；energy_shield_regen 字段保持现有逻辑）---
    let zealots_oath = db.flag(cfg, ModName::from("ZealotsOath"));
    let es_recharge = calc_es_recharge(db, cfg, env.player.output.energy_shield, zealots_oath);
    env.player.output.es_recharge_rate = es_recharge.rate_fraction;
    env.player.output.es_recharge_delay = es_recharge.delay_seconds;
    env.player.output.es_recharge_per_second =
        es_recharge_per_second(&es_recharge, env.player.output.energy_shield);

    // --- 规避几率（Lane2：击中/投射物/各异常）---
    let avoidance = calc_avoidance(db, cfg, env.player.output.energy_shield);
    env.player.output.avoid_all_damage_from_hits = avoidance.avoid_all_damage_from_hits;
    env.player.output.avoid_projectile_damage = avoidance.avoid_projectile_damage;
    env.player.output.avoid_stun = avoidance.avoid_stun;
    env.player.output.avoid_ignite = avoidance.avoid_ignite;
    env.player.output.avoid_shock = avoidance.avoid_shock;
    env.player.output.avoid_chill = avoidance.avoid_chill;
    env.player.output.avoid_freeze = avoidance.avoid_freeze;
    env.player.output.avoid_poison = avoidance.avoid_poison;
    env.player.output.avoid_bleeding = avoidance.avoid_bleeding;

    // --- 承受伤害乘数（Lane2：受击口径，按类型）---
    let taken = calc_taken_multi_suite(db, cfg);
    env.player.output.taken_multi_physical = taken.physical_when_hit;
    env.player.output.taken_multi_fire = taken.fire_when_hit;
    env.player.output.taken_multi_cold = taken.cold_when_hit;
    env.player.output.taken_multi_lightning = taken.lightning_when_hit;
    env.player.output.taken_multi_chaos = taken.chaos_when_hit;

    // --- 暴击额外伤害减免 + 敌人暴击效果（Lane2）---
    let crit_red = calc_crit_extra_reduction(db, cfg);
    env.player.output.crit_extra_damage_reduction = crit_red.reduction_pct;
    env.player.output.enemy_crit_effect =
        enemy_crit_effect(enemy_crit_chance, enemy_crit_damage, &crit_red);

    // --- 充能状态（Lane A：供 per-charge 词条引用与面板显示；无来源时 current=0, maximum=3）---
    let charges = resolve_all_charges(db, cfg);
    env.player.output.charge_power_current = charges.power.current;
    env.player.output.charge_power_maximum = charges.power.maximum;
    env.player.output.charge_frenzy_current = charges.frenzy.current;
    env.player.output.charge_frenzy_maximum = charges.frenzy.maximum;
    env.player.output.charge_endurance_current = charges.endurance.current;
    env.player.output.charge_endurance_maximum = charges.endurance.maximum;

    // --- 偷取（Lane A：传入物理平均命中作为 hit_damage；PoE2 默认仅物理偷取）---
    // 无偷取词条时各 display_rate 为 0（calc_leech_from_db 短路），不影响面板。
    let phys_hit = component_avg(&env.player.output.damage_components, DamageType::Physical);
    env.player.output.life_leech_rate = calc_leech_from_db(
        db,
        cfg,
        env.player.output.life,
        phys_hit,
        LeechResource::Life,
    )
    .display_rate_per_second;
    env.player.output.mana_leech_rate = calc_leech_from_db(
        db,
        cfg,
        env.player.output.mana,
        phys_hit,
        LeechResource::Mana,
    )
    .display_rate_per_second;
    env.player.output.es_leech_rate = calc_leech_from_db(
        db,
        cfg,
        env.player.output.energy_shield,
        phys_hit,
        LeechResource::EnergyShield,
    )
    .display_rate_per_second;

    // --- Recoup（Lane A：事件触发，面板口径以「假设受到 10% 生命的伤害」估算返还速率）---
    // 无 Recoup 词条时 calc_recoup_from_db 返回 rate=0（短路），不影响面板。
    let damage_taken_estimate = env.player.output.life * RECOUP_DAMAGE_BASIS_FRACTION;
    env.player.output.life_recoup_rate =
        calc_recoup_from_db(db, cfg, damage_taken_estimate, RecoupResource::Life).rate_per_second;
    env.player.output.es_recoup_rate =
        calc_recoup_from_db(db, cfg, damage_taken_estimate, RecoupResource::EnergyShield)
            .rate_per_second;

    // --- 技能功能（Lane C：AoE / 投射物 / 冷却 / 消耗）---
    fill_skill_mechanics(env);

    // --- 触发速率（Lane B：冷却驱动 / CWC；无触发词条时保持 0）---
    fill_trigger(env);
}

/// 触发速率 fill（Lane B）：读冷却驱动 / CWC 触发词条，写 `trigger_rate_cap` /
/// `skill_trigger_rate`。
///
/// 两种可由词条立即驱动的触发模型（能量驱动需 build 层注入插槽法术数据，defer）：
/// - **冷却驱动**（`TriggerCooldownBase` BASE，秒）：源技能本身有触发冷却。
///   `action_cd = max(TriggeredSkillCooldown, TriggerCooldownBase / icdr)`，
///   `cap = 1/ceil_tick(action_cd)`，`rate = min(cap, effective_action_rate)`。
/// - **CWC**（`CWCTriggerTime` BASE，秒）：引导触发，由引导间隔取整到帧决定节奏，被触发冷却 clamp。
///
/// `icdr` = `(1 + Σinc_CooldownRecovery/100) × Πmore_CooldownRecovery`（PoB2 `calcLib.mod`），
/// 作为触发冷却除数。`effective_action_rate` 取自 `fill_mechanics` 已写入的有效行动速率，
/// 作为冷却驱动触发的源速率门控。
///
/// 无 `TriggerCooldownBase` / `CWCTriggerTime` 词条时两字段保持 0（向后兼容）。
/// 出处：agent-docs/triggers.md §三 / §4.2；Lane B integration_spec；PoB2 CalcTriggers.lua。
fn fill_trigger(env: &mut Env) {
    let db = &env.player.mod_db;
    let cfg = &env.cfg;

    let trigger_cd = db.sum(ModType::Base, cfg, &[ModName::from("TriggerCooldownBase")]);
    let triggered_cd = db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("TriggeredSkillCooldown")],
    );
    let cwc_trigger_time = db.sum(ModType::Base, cfg, &[ModName::from("CWCTriggerTime")]);

    // ICDR 乘子（CooldownRecovery INC/MORE 折算；默认 1.0，作触发冷却除数）。
    let icdr = cooldown_recovery_multiplier(db, cfg);
    let source_rate = env.player.output.effective_action_rate;

    let mut trace = TraceGraph::new();

    if trigger_cd > 0.0 {
        // 冷却驱动：双门控 SkillTriggerRate = min(cap, sourceRate)。
        let (tr, _) =
            resolve_trigger_rate_traced(trigger_cd, triggered_cd, icdr, source_rate, &mut trace);
        env.player.output.trigger_rate_cap = tr.trigger_rate_cap;
        env.player.output.skill_trigger_rate = tr.skill_trigger_rate;
    } else if cwc_trigger_time > 0.0 {
        // CWC：引导触发，被触发技能冷却 clamp。adds_cast_time 由 build 层注入（当前传 0）。
        let (cwc, _) =
            calc_cwc_trigger_rate_traced(cwc_trigger_time, triggered_cd, 0.0, icdr, &mut trace);
        env.player.output.trigger_rate_cap = cwc.trigger_rate_cap;
        env.player.output.skill_trigger_rate = cwc.trigger_rate_cap;
    }
}

/// 冷却恢复速率乘子（`CooldownRecovery` INC/MORE 折算）：`(1 + Σinc/100) × Πmore`。
///
/// 与 `skill_mechanics::calc_cooldown` 的 recovery_rate 一致语义，但只取 INC/MORE 乘子
/// 作为触发冷却除数（不处理 Base/Override，触发宝石冷却由宝石数据给出）。默认 1.0（无加成）。
fn cooldown_recovery_multiplier(db: &ModDb, cfg: &CalcConfig) -> f64 {
    let names = [ModName::from("CooldownRecovery")];
    let inc = db.sum(ModType::Inc, cfg, &names);
    let more = db.more(cfg, &names);
    ((1.0 + inc / 100.0) * more).max(f64::EPSILON)
}

/// Recoup 面板估算基准：以「假设受到玩家最大生命 10%」的伤害估算每秒返还速率。
///
/// Recoup 本质是受击事件触发；面板口径需要一个固定的受击伤害基准。10% 生命是常见
/// 估算约定（PoB2 面板亦用假设受击量）。真实受击伤害来源待 Build 层事件接入后替换。
const RECOUP_DAMAGE_BASIS_FRACTION: f64 = 0.1;

/// 技能功能 fill（Lane C）：AoE 半径 / 投射物数量 / 冷却 / 资源消耗。
///
/// 这些机制依赖技能基础参数（基础半径 / 基础冷却 / 基础消耗），当前 `Actor` 尚无对应
/// 字段（Build 层注入待接入），故从玩家 `mod_db` 的 BASE 词条读取基础值：
/// - `SkillAreaRadiusBase` / `SkillCooldownBase` / `SkillManaCostBase` /
///   `SkillLifeCostBase` / `SkillSpiritReservationBase`（均无词条时该项跳过，输出保持 0）。
///
/// 这样既不改 `Actor`/`Env`（避免跨 lane 共享文件的字段 ripple），又能让有这些基础
/// 参数的 build（经 item/gem 注入对应 BASE 词条）走完整聚合管线。基础参数随技能宝石
/// 数据接入的字段化改造 defer 到 Build 层。
fn fill_skill_mechanics(env: &mut Env) {
    let db = &env.player.mod_db;
    let cfg = &env.cfg;

    // 投射物数量：始终计算（无投射物词条时 calc_projectile_count 走 base=0 → count=0）。
    // 仅当存在投射物来源（base_count > 0）时写入面板，避免给非投射物技能误标 0 以外的值。
    let proj = calc_projectile_count(db, cfg);
    if proj.base_count > 0.0 {
        env.player.output.projectile_count = proj.projectile_count;
    }

    // AoE：需技能基础半径（SkillAreaRadiusBase BASE）。无则跳过（保持 0）。
    let base_radius = db.sum(ModType::Base, cfg, &[ModName::from("SkillAreaRadiusBase")]);
    if base_radius > 0.0 {
        let aoe = calc_aoe(db, cfg, base_radius, 0.0);
        env.player.output.aoe_radius = aoe.radius;
        env.player.output.aoe_area_mod = aoe.area_mod;
    }

    // 冷却：需技能基础冷却（SkillCooldownBase BASE，秒）。无则跳过。
    let base_cd = db.sum(ModType::Base, cfg, &[ModName::from("SkillCooldownBase")]);
    if base_cd > 0.0 {
        let stored = db
            .sum(ModType::Base, cfg, &[ModName::from("SkillStoredUsesBase")])
            .max(1.0) as u32;
        let cd = calc_cooldown(db, cfg, base_cd, stored);
        env.player.output.cooldown = cd.cooldown;
        env.player.output.cooldown_stored_uses = cd.stored_uses;
    }

    // 消耗：各资源需对应基础值 BASE 词条。无则跳过（保持 0）。
    let base_mc = db.sum(ModType::Base, cfg, &[ModName::from("SkillManaCostBase")]);
    if base_mc > 0.0 {
        env.player.output.mana_cost = calc_mana_cost(db, cfg, base_mc).final_cost;
    }
    let base_lc = db.sum(ModType::Base, cfg, &[ModName::from("SkillLifeCostBase")]);
    if base_lc > 0.0 {
        env.player.output.life_cost = calc_life_cost(db, cfg, base_lc).final_cost;
    }
    let base_sr = db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("SkillSpiritReservationBase")],
    );
    if base_sr > 0.0 {
        env.player.output.spirit_reserved = calc_spirit_reservation(db, cfg, base_sr).final_cost;
    }
}

/// 异常 fill：对每类伤害异常算 几率 × 暴击加权 magnitude × effMult，写入 [`OutputTable`]。
///
/// 来源命中取**非暴击**分类型平均（`component_avg`，pre-mitigation），按暴击乘区/几率派生
/// 暴击来源（[`AilmentSource`]）。敌方异常阈值用怪物等级查表 × `EnemyAilmentThreshold` mod。
/// 几率派生型（点燃/感电）吃阈值；内禀型（流血/中毒）吃 `BleedChance`/`PoisonChance`。
///
/// 面板 DPS 口径 = `chance × magnitude_dps`（pobr 叠层延后，magnitude 仍可由 trace 回溯）。
fn fill_ailments(env: &mut Env) {
    let player = &env.player.mod_db;
    let enemy = &env.enemy.mod_db;
    let cfg = &env.cfg;

    // Lane C 跨类型施加：来源命中按 `<Type>Can<Ailment>` 旗标聚合非默认类型分量。
    // 无跨类型旗标时退化为各异常的默认伤害类型（流血/中毒=物理(+混沌)、点燃=火、感电=闪电、冰缓=冰），
    // 与旧的硬编码分量口径一致（向后兼容）。
    let components = &env.player.output.damage_components;
    let phys_hit = cross_type_source_hit(AilmentType::Bleed, components, player, cfg);
    let fire_hit = cross_type_source_hit(AilmentType::Ignite, components, player, cfg);
    let cold_hit = cross_type_source_hit(AilmentType::Chill, components, player, cfg);
    let lightning_hit = cross_type_source_hit(AilmentType::Shock, components, player, cfg);
    let chaos_phys_hit = cross_type_source_hit(AilmentType::Poison, components, player, cfg);

    let crit_mult = if env.player.output.crit_multiplier > 0.0 {
        env.player.output.crit_multiplier
    } else {
        1.0
    };
    let crit_chance = env.player.output.crit_chance;
    let never_from_crit = player.flag(cfg, ModName::from("AilmentsAreNeverFromCrit"));

    // 敌方异常阈值（怪物等级查表 × EnemyAilmentThreshold mod）；无敌人配置时回退裸表。
    let threshold = enemy_ailment_threshold_effective(enemy, cfg, env.enemy.level);
    // 敌方姿态阈值（冰冻/电击姿态积累用；与异常阈值平行，含 floor）。
    let poise_thr = enemy_poise_threshold_effective(enemy, cfg, env.enemy.level);

    // trace 与本步聚合的归因绑在 player.breakdown 之外的临时图：写入 output 字段即可，
    // 完整 trace 由 traced offence/归因路径统一收口（本函数构建并保留贡献节点拓扑）。
    let mut trace = TraceGraph::new();

    if phys_hit > 0.0 {
        let source = AilmentSource::new(phys_hit, crit_mult, crit_chance, never_from_crit);
        let (bleed, _) = bleed_traced(&source, player, enemy, cfg, &mut trace);
        // Lane C：AilmentEffect（MORE）× rateMod（Faster/Slower）应用到期望 DPS，再 clamp DotDpsCap。
        let bleed_dps = finalize_ailment_dps(bleed.expected_dps, "Bleed", player, enemy, cfg);
        env.player.output.bleed_dps = bleed_dps;

        // Lane B：流血叠层（BleedStacks BASE）。无叠层配置时 max_stacks=1，stacked == 单层。
        let bleed_stack = resolve_stack_config(player, cfg, "Bleed");
        let (bleed_stacked, _) =
            stacking_ailment_dps_traced(bleed_dps, &bleed_stack, AilmentType::Bleed, &mut trace);
        // 叠层 DPS 也吃全局 DotDpsCap（PoB2：DotDpsCap 是叠层后的绝对上限）。
        env.player.output.bleed_stacked_dps = apply_dot_dps_cap(bleed_stacked, player, cfg);
        env.player.output.bleed_active_stacks = active_stacks_of(&bleed_stack);
    }
    if fire_hit > 0.0 {
        let source = AilmentSource::new(fire_hit, crit_mult, crit_chance, never_from_crit);
        let (ignite, _) = ignite_traced(&source, player, enemy, cfg, threshold, &mut trace);
        let ignite_dps = finalize_ailment_dps(ignite.expected_dps, "Ignite", player, enemy, cfg);
        env.player.output.ignite_dps = ignite_dps;

        // Lane B：点燃叠层（IgniteStacks BASE）。PoE2 点燃默认不叠层（只取最强一层），
        // 仅在携带 `IgniteStacks` 词条时 max_stacks>1；无配置时 stacked == 单层（向后兼容）。
        let ignite_stack = resolve_stack_config(player, cfg, "Ignite");
        let (ignite_stacked, _) =
            stacking_ailment_dps_traced(ignite_dps, &ignite_stack, AilmentType::Ignite, &mut trace);
        env.player.output.ignite_stacked_dps = apply_dot_dps_cap(ignite_stacked, player, cfg);
        env.player.output.ignite_active_stacks = active_stacks_of(&ignite_stack);
    }
    if cold_hit > 0.0 {
        // Lane B：冰缓行动速度降低（%）。强度不足最低阈值时为 0（不施加）。
        let (chill, _) = chill_traced(cold_hit, threshold, player, cfg, &mut trace);
        env.player.output.chill_effect = chill;
        // Lane B：冰冻姿态积累（% per hit）。
        let (freeze_buildup, _) = freeze_poise_buildup_traced(poise_thr, player, cfg, &mut trace);
        env.player.output.freeze_buildup_pct = freeze_buildup;
    }
    if chaos_phys_hit > 0.0 {
        let source = AilmentSource::new(chaos_phys_hit, crit_mult, crit_chance, never_from_crit);
        let (poison, _) = poison_traced(&source, player, enemy, cfg, &mut trace);
        let poison_dps = finalize_ailment_dps(poison.expected_dps, "Poison", player, enemy, cfg);
        env.player.output.poison_dps = poison_dps;

        // Lane B：中毒叠层（PoisonStacks BASE）。
        let poison_stack = resolve_stack_config(player, cfg, "Poison");
        let (poison_stacked, _) =
            stacking_ailment_dps_traced(poison_dps, &poison_stack, AilmentType::Poison, &mut trace);
        env.player.output.poison_stacked_dps = apply_dot_dps_cap(poison_stacked, player, cfg);
        env.player.output.poison_active_stacks = active_stacks_of(&poison_stack);
    }
    if lightning_hit > 0.0 {
        let source = AilmentSource::new(lightning_hit, crit_mult, crit_chance, never_from_crit);
        // 感电是非伤害异常：面板 `shock_effect` 保留为**效果幅度**（fraction），
        // 不乘几率（与 DoT 的几率×期望值口径不同）。chance 已写入 trace 供归因/未来叠层。
        let (_chance, magnitude, _) =
            shock_traced(&source, player, enemy, cfg, threshold, &mut trace);
        env.player.output.shock_effect = magnitude;
        // Lane B：电击姿态积累（% per hit）。
        let (electrocute_buildup, _) =
            electrocute_poise_buildup_traced(poise_thr, player, cfg, &mut trace);
        env.player.output.electrocute_buildup_pct = electrocute_buildup;
    }
}

/// Lane C：对伤害异常的期望 DPS 应用 `AilmentEffect`（MORE）× `rateMod`（Faster/Slower），
/// 再 clamp 全局 `DotDpsCap`。
///
/// - `effectMod`：`ailment_effect_mod`（玩家 `AilmentEffect` MORE 聚合，默认 1.0）。
/// - `rateMod`：`ailment_rate_mod`（玩家 `<Ailment>Faster`/`Slower` + 敌方 `Self<Ailment>Faster`，默认 1.0）。
/// - DPS = `expected_dps × effectMod × rateMod`，clamp `min(_, DotDpsCap)`。
///
/// 三个修正在无对应词条时均为中性（1.0 / 无 cap），输出与未接入 Lane C 时一致（向后兼容）。
/// 出处：PoB2 `CalcOffence.lua` l.5190/l.5035/l.5193；Lane C integration_spec。
fn finalize_ailment_dps(
    expected_dps: f64,
    ailment_name: &str,
    player: &ModDb,
    enemy: &ModDb,
    cfg: &CalcConfig,
) -> f64 {
    let effect = ailment_effect_mod(player, cfg);
    let rate = ailment_rate_mod(player, enemy, cfg, ailment_name);
    let scaled = expected_dps * effect * rate;
    apply_dot_dps_cap(scaled, player, cfg)
}

/// 从 ModDb 解析某 damaging ailment 的叠层配置（`<Ailment>Stacks` BASE → max_stacks）。
///
/// 无 `<Ailment>Stacks` 词条时默认 max_stacks=1（不叠层，stacked == 单层 DPS，向后兼容）。
/// active_stacks 暂取 0（由 `stacking_ailment_dps` 回退到 max_stacks 作上界）；精细活跃
/// 层数（命中频率 × 持续时间）待 Build 层完整 stacking 实现接入。
fn resolve_stack_config(db: &ModDb, cfg: &CalcConfig, ailment: &str) -> StackConfig {
    let base_stacks = db.sum(
        ModType::Base,
        cfg,
        &[ModName::from(format!("{ailment}Stacks"))],
    );
    let max_stacks = (1.0 + base_stacks).max(1.0) as u32;
    StackConfig::new(max_stacks, 0.0)
}

/// 估算活跃层数（面板口径）：active_stacks>0 取之，否则取 max_stacks 作上界。
fn active_stacks_of(cfg: &StackConfig) -> f64 {
    if cfg.active_stacks > 0.0 {
        cfg.active_stacks
    } else {
        cfg.max_stacks as f64
    }
}

/// 有效敌方姿态阈值 = `enemy_poise_threshold(level) × mod(...)` 后 floor。
///
/// mod 集合与 Lane B 规格一致：`PoiseThreshold` / `FreezeThreshold` /
/// `EnemyAilmentThreshold`，INC/MORE 聚合为乘子。无敌人 mod_db 时退化为裸表值。
fn enemy_poise_threshold_effective(enemy: &ModDb, cfg: &CalcConfig, level: u8) -> f64 {
    let base = enemy_poise_threshold(level as u32) as f64;
    let names = [
        ModName::from("PoiseThreshold"),
        ModName::from("FreezeThreshold"),
        ModName::from("EnemyAilmentThreshold"),
    ];
    let inc = enemy.sum(ModType::Inc, cfg, &names);
    let more = enemy.more(cfg, &names);
    (base * (1.0 + inc / 100.0) * more).floor()
}

/// 有效敌方异常阈值 = `enemy_ailment_threshold(level) × mod(EnemyAilmentThreshold)`。
///
/// `EnemyAilmentThreshold` 以 INC/MORE 聚合为乘子（PoB2 `calcLib.mod`）。无敌人 mod_db
/// 时退化为裸表值（兼容直接构造 Env 的旧入口）。
fn enemy_ailment_threshold_effective(enemy: &ModDb, cfg: &CalcConfig, level: u8) -> f64 {
    let base = enemy_ailment_threshold(level as u32) as f64;
    let inc = enemy.sum(ModType::Inc, cfg, &[ModName::from("EnemyAilmentThreshold")]);
    let more = enemy.more(cfg, &[ModName::from("EnemyAilmentThreshold")]);
    base * (1.0 + inc / 100.0) * more
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
