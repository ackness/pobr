use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb};

use super::ailment::{
    AilmentSource, StackConfig, ailment_crit_chance, ailment_duration, ailment_effect_mod,
    ailment_rate_mod, apply_dot_dps_cap, bleed_traced, chill_traced, cross_type_source_hit,
    cross_type_source_hit_at_roll, electrocute_poise_buildup_traced, estimate_active_stacks,
    freeze_poise_buildup_traced, ignite_traced, poison_traced, roll_average, shock_traced,
    stack_potential, stacking_ailment_dps_traced,
};
use super::skill_mechanics::{
    calc_aoe, calc_cooldown, calc_life_cost, calc_mana_cost, calc_projectile_count,
    calc_spirit_reservation,
};
use super::trigger::{
    RotationSkill, TriggerSourceStats, calc_cwc_trigger_rate_traced, calc_multi_spell_rotation,
    resolve_trigger_rate_traced,
};
use super::{
    BreakdownTable, CalcError, EhpOptions, Env, LeechResource, MinimalInput, MinionOutput,
    MitigationInputs, OutputTable, RecoupResource, ResistanceSuite, build_mitigation_ctx,
    calc_avoidance, calc_crit_extra_reduction, calc_defence, calc_ehp_with_opts, calc_es_recharge,
    calc_leech_from_db, calc_recoup_from_db, calc_regen, calc_skill_use_time,
    calc_taken_multi_suite, calculate_minimal_vs_enemy, enemy_crit_effect, es_recharge_per_second,
    reservation, resolve_all_charges, round, taken_mult_for_type_default,
};
use crate::{TraceGraph, TraceOperation};

pub fn perform(env: &mut Env) -> Result<(), CalcError> {
    if env.player.level == 0 {
        return Err(CalcError::InvalidActorState(
            "player level must be greater than 0",
        ));
    }

    super::env_finalize::env_finalize(env); // M3 环境终结阶段（T0 全 no-op；蓝图 §1 D1）

    // 充能 multiplier：按 PoB2 口径，仅 `Condition:UseXCharges` 为真（或常驻满层）时把
    // PowerCharge/FrenzyCharge/EnduranceCharge 置为最大层，供 `per X charge` 词条展开。
    // 未启用该充能时保持 0（PoB2 面板 current=0），避免错误施加 per-charge 增益/罚减。
    env.cfg = super::survivability::charge_multipliers_panel_default(&env.player.mod_db, &env.cfg);

    // 五元防御资源转换矩阵（M2 C-3，PoB2 CalcDefence.lua:1301-1390）：defence 源
    // （Armour/Evasion/ES）→ 非 defence 目标（Life/Mana）的转入量在 minimal 计算前
    // 注入为 MaximumLife/MaximumMana BASE（对应 PoB2 `NewMod("Extra"..name, "BASE", …)`
    // :1383，享 Life/Mana 全局乘区）。既有 ES→Mana 专用通道（es_to_mana_rate）已并入
    // 本矩阵（ES 侧缩残在 calc_defence 内部完成）；矩阵无词条时转入恒 0，本段空转。
    {
        let keystones = crate::rules::DefenceKeystones::from_db(&env.player.mod_db, &env.cfg);
        let resources = super::defence::calc_defence_resources(
            &env.player.mod_db,
            &env.cfg,
            &env.player.base,
            &keystones,
        );
        let extras = [
            ("MaximumLife", resources.extra_life),
            ("MaximumMana", resources.extra_mana),
        ];
        for (name, value) in extras {
            if value > 0.0 {
                env.player.mod_db.add_list([crate::Modifier::number(
                    ModName::from(name),
                    ModType::Base,
                    value,
                )
                .with_source("defence resource conversion")]);
            }
        }
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
    // M4-T2 W-B2：hand pass 入口。`hand_sources` 空（非攻击技能 / 旧入口）时
    // `run_hand_passes` 内部直通 `calculate_minimal_vs_enemy`，行为逐值不变；
    // 编排层装配 HandSource 后走 per-hand 管线 + combineStat 合并。
    let hand_pass = super::hand_pass::run_hand_passes(
        &env.player.mod_db,
        &env.enemy.mod_db,
        &env.cfg,
        &env.hand_sources,
        &input,
        env.double_hits_when_dual_wielding,
    );
    let output = hand_pass.combined;
    env.player.output = OutputTable::from(&output);
    env.player.output.main_hand = hand_pass.main_hand;
    env.player.output.off_hand = hand_pass.off_hand;
    // M3 T3：curse 面板回填（buff_pass 在 env_finalize 阶段 4 产出，先于上行整表
    // 覆盖，经 Env::curse_pass_output 中转；None = buff_pass 未运行，维持 Default 0）。
    if let Some(curse) = &env.curse_pass_output {
        env.player.output.enemy_curse_limit = curse.enemy_curse_limit;
        env.player.output.curse_slots = curse.curse_slots.clone();
    }
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
    let minion_limit = env.player.mod_db.get_multiplier("SummonedMinion", &env.cfg);
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

    // --- keystone 开关快照（M2 Track C：集中构造一次，下游各机制段只读本结构）---
    let keystones = crate::rules::DefenceKeystones::from_db(db, cfg);

    // --- 技能使用时间 / 有效行动速率 ---
    // M4-T2 W-B2：武器速率迁到 HandSource 后，面板基础速率从首个 hand source 读
    // （单 MainHand source = 旧 base_input 折算值，逐值不变；双持的合并口径
    // 等 W-A1 后随 combineStat Speed 对齐）。无 hand source 沿用 base.action_rate。
    let panel_base_rate = env
        .hand_sources
        .first()
        .and_then(|hand| hand.weapon.attack_rate)
        .filter(|rate| *rate > 0.0)
        .unwrap_or(env.player.base.action_rate);
    let base_use_time = if panel_base_rate > 0.0 {
        1.0 / panel_base_rate
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
        chaos: {
            // 混沌抗同样按最大抗性上限截断（PoB2 默认 75%，+max chaos res 提升、硬上限 90%）。
            let total = db.sum(ModType::Base, cfg, &[ModName::from("ChaosResistance")]);
            let max_bonus = db.sum(
                ModType::Base,
                cfg,
                &[ModName::from("MaximumChaosResistance")],
            );
            // M0-W3：抗性边界改读注入常量包（fallback == 旧 const，值不变）。
            let max = (cfg.constants.character().base_maximum_all_resistances_pct + max_bonus)
                .min(cfg.constants.game().resist_hard_cap);
            total.min(max)
        },
    };
    let reference_hit = (env.player.output.life + env.player.output.energy_shield).max(1.0);
    // M2 Track B-2（13-G7）：减伤侧统一整备 MitigationCtx——ArmourAppliesTo 改百分比模型
    // （词条单一来源：ModParser.lua:2519-2544 三变体 → ArmourAppliesTo<X>DamageTaken BASE
    // + ArmourDoesNotApplyToPhysicalDamageTaken flag；合成 CalcDefence.lua:2336-2362、
    // 物理隐式 BASE 100 见 :1862-1863），DamageReductionMax / overwhelm 一并入 ctx
    // （per-type 上限 :2333；全局缺省走 cfg.constants）。
    let mit_ctx = build_mitigation_ctx(
        db,
        cfg,
        &MitigationInputs {
            armour: env.player.output.armour,
            evasion: env.player.output.evasion,
            energy_shield: env.player.output.energy_shield,
            resist_pct: [
                0.0, // 物理无抗性（减伤走护甲/flat DR）
                resistances.fire,
                resistances.cold,
                resistances.lightning,
                resistances.chaos,
            ],
            // 偏斜乘数：Track D 接 DeflectChance/DeflectEffect 输出后由 F 折入；当前 0 → 1。
            deflect_chance_pct: 0.0,
            deflect_effect_pct: 0.0,
        },
    );
    let phys = DamageType::Physical as usize;
    // 「instead of physical」全量重定向（物理护甲清零**仅此变体**，flag 经
    // armour_applies_pct 把物理份额归 0）。旧 EhpOptions 的 [bool;3] 只能表达该形态：
    // 元素吃全额护甲 + 物理清零；百分比/also 变体在旧 max-hit 口径下维持原行为
    // （物理保留护甲、元素暂不吃护甲），完整百分比口径由 Track F 消费
    // taken_hit_from_damage 后生效（蓝图 m2-defence §2 Track B 过渡语义）。
    let instead_redirect = mit_ctx.armour_applies_pct[phys] <= 0.0;
    // CI 接线（13-G16）：keystone 快照驱动，不再写死 false。CI build 的 ES 作生命池、
    // 混沌免疫（EhpOptions 语义）。vendor：CalcDefence.lua:85（flag 读出）/:120-123
    // （Life=1 + FullLife）/:2537-2539（CI 用「CI 前 Life」作眩晕阈值基底）。
    let ehp_opts = EhpOptions {
        chaos_inoculation: keystones.chaos_inoculation,
        physical_overwhelm: mit_ctx.overwhelm_pct[phys] / 100.0,
        armour_applies_to_element: [
            instead_redirect && mit_ctx.armour_applies_pct[DamageType::Fire as usize] > 0.0,
            instead_redirect && mit_ctx.armour_applies_pct[DamageType::Cold as usize] > 0.0,
            instead_redirect && mit_ctx.armour_applies_pct[DamageType::Lightning as usize] > 0.0,
        ],
        damage_reduction_caps: crate::calc::ehp::DamageReductionCaps {
            global: mit_ctx.dr_max_pct[phys] / 100.0,
        },
    };
    let ehp = calc_ehp_with_opts(
        env.player.output.life,
        env.player.output.energy_shield,
        env.player.output.mana,
        &resistances,
        env.player.output.armour,
        reference_hit,
        ehp_opts,
    );
    // 承受伤害乘区（玩家侧受击口径）：用 `taken_mult_for_type_default`，对齐 PoB2 默认
    // `damageCategoryConfig = "Average"`（CalcDefence.lua L2013/L2429）——base
    // (`DamageTaken`/`<Type>DamageTaken`[/`ElementalDamageTaken`]) 叠加 WhenHit，再取
    // Attack/Spell 两层（`AttackDamageTaken`/`SpellDamageTaken`）均值。无 Attack/Spell 词条时
    // 两层相等，退化为基础 hit 口径，对现有回归输出保持一致。
    // PoE2 已移除法术抑制、deflect 罕用，二者按 1.0 略去（与 PoB2 default 单一 hit 口径一致）。
    // 最大承受击中 = ehp / dt（dt<1 → 承受更少 → 可承受更大击中）。dt≤0（完全免疫）→ ∞。
    // 出处：PoB2 CalcDefence.lua:2250-2269（TakenHitMult 聚合）、2422-2430（damageCategory 选取）。
    let apply_dt = |max_hit: f64, dtype: DamageType| -> f64 {
        let dt = taken_mult_for_type_default(db, cfg, dtype);
        if dt <= 0.0 {
            f64::INFINITY
        } else {
            round(max_hit / dt)
        }
    };
    env.player.output.physical_max_hit = apply_dt(ehp.physical_max_hit, DamageType::Physical);
    env.player.output.fire_max_hit = apply_dt(ehp.fire_max_hit, DamageType::Fire);
    env.player.output.cold_max_hit = apply_dt(ehp.cold_max_hit, DamageType::Cold);
    env.player.output.lightning_max_hit = apply_dt(ehp.lightning_max_hit, DamageType::Lightning);
    env.player.output.chaos_max_hit = apply_dt(ehp.chaos_max_hit, DamageType::Chaos);
    env.player.output.total_ehp = ehp.total_ehp;
    // 旧 lowest-max-hit 口径附加指标（F-3 后唯一权威出口：上面写入的 canonical
    // `total_ehp`/`*_max_hit` 会在 `fill_ehp_pob2` 末尾被新口径覆盖；本段旧管线
    // 不删码，revert fill_ehp_pob2 的切换段即回旧口径——蓝图 §5 R2 行）。
    env.player.output.total_ehp_lowest_max_hit = ehp.total_ehp;

    // --- 预留 / 剩余 ---
    // M2 Track D（13-G11）：补 ReservationMultiplier more 与 Reservation
    // Efficiency 除法语义（CalcDefence.lua:197/:240-241/:249-258）——
    // mult = floor(More(ReservationMultiplier), 4)；efficiency inc/more 为**除数**；
    // 除数下界钳极小正数：efficiency −100% 时 raw 发散，由 reservation 的
    // [0, pool] clamp 收口为「池满预留」，与 vendor 语义一致。
    let reservation_mult =
        (db.more(cfg, &[ModName::from("ReservationMultiplier")]) * 10_000.0).floor() / 10_000.0;
    let res_eff_divisor = |kind: &str| -> f64 {
        let names = [
            ModName::from(format!("{kind}ReservationEfficiency").as_str()),
            ModName::from("ReservationEfficiency"),
        ];
        let inc = db.sum(ModType::Inc, cfg, &names).max(-100.0);
        ((1.0 + inc / 100.0) * db.more(cfg, &names)).max(1e-12)
    };
    let life_factor = reservation_mult / res_eff_divisor("Life");
    let mana_factor = reservation_mult / res_eff_divisor("Mana");
    let life_res = reservation(
        env.player.output.life,
        db.sum(ModType::Base, cfg, &[ModName::from("LifeReserved")]) * life_factor,
        db.sum(ModType::Inc, cfg, &[ModName::from("LifeReservedPercent")]) * life_factor,
    );
    let mana_res = reservation(
        env.player.output.mana,
        db.sum(ModType::Base, cfg, &[ModName::from("ManaReserved")]) * mana_factor,
        db.sum(ModType::Inc, cfg, &[ModName::from("ManaReservedPercent")]) * mana_factor,
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
    env.player.output.block_chance = super::block_chance(
        db.sum(ModType::Base, cfg, &[ModName::from("BlockChance")]),
        cfg.constants.game().block_chance_cap,
    );
    env.player.output.spell_block_chance = super::block_chance(
        db.sum(ModType::Base, cfg, &[ModName::from("SpellBlockChance")]),
        cfg.constants.game().block_chance_cap,
    );

    // --- ES 充能（Lane2：充能与再生独立；energy_shield_regen 字段保持现有逻辑）---
    let zealots_oath = db.flag(cfg, ModName::from("ZealotsOath"));
    let es_recharge = calc_es_recharge(db, cfg, env.player.output.energy_shield, zealots_oath);
    env.player.output.es_recharge_rate = es_recharge.rate_fraction;
    env.player.output.es_recharge_delay = es_recharge.delay_seconds;
    env.player.output.es_recharge_per_second =
        es_recharge_per_second(&es_recharge, env.player.output.energy_shield);

    // --- 规避几率（Lane2：击中/投射物/各异常）---
    // M2-E2（CalcDefence.lua:2554-2557）：眩晕规避的 ES 减半条件改为
    // 「ES > totalTakenHit 且非 EB」；totalTakenHit 在 Track F 接线前用
    // reference_hit（= life + ES，与 EhpOptions 同源）近似。
    // EB flag 走 C-1 keystone 快照（蓝图 §3.3 契约 2，不散读 flag）。
    let avoidance = calc_avoidance(
        db,
        cfg,
        env.player.output.energy_shield,
        reference_hit,
        keystones.energy_shield_protects_mana,
    );
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

    // --- 技能功能（Lane C：AoE / 投射物 / 冷却 / 消耗）---
    fill_skill_mechanics(env);

    // --- 触发速率（Lane B：冷却驱动 / CWC；无触发词条时保持 0）---
    fill_trigger(env);

    // --- Evade 四分型 + Stun（M2 Track E，蓝图 §3.2 预登记的一行调用）---
    super::defence::fill_evade_stun(env, &keystones);

    // --- Block/Spirit/Ward/Deflection 面板族（M2 Track D，蓝图 §3.2 预登记的一行调用）---
    super::defence_panels::fill_defence_panels(env, &keystones);

    // --- EHP PoB2 口径管线（M2 Track F；须在 D/E 两个 fill 之后——not-hit/block/
    //     deflect 层读其 OutputTable 输出，缺省 0 → 中性 1.0）。F-3 起在末尾把
    //     canonical `total_ehp`/`*_max_hit` 切换为新口径值，并以真值 totalTakenHit
    //     覆盖 avoid_stun / Stun 体系。---
    let recoupable_total = super::ehp::fill_ehp_pob2(env, &keystones, &resistances);

    // --- Recoup（M2 F-4，13-G15 部分：基数替换）---
    // 旧口径 = life × 10% 估算（与受击管线脱钩）；新口径 = reduce_pools 在
    // mitigated EHP 循环上累计的 recoupable 伤害（vendor reducePoolsByDamage
    // :489/:537 记录 damageTakenThatCanBeRecouped → :3119-3123 累计 →
    // :3347-3361 `TotalRecoupRecovery = Recoup%/100 × totalDamage`、
    // :3382 `RecoupRecoveryMax = Total / recoupTime`——calc_recoup_from_db 的
    // total/duration×rateMod 公式骨架不变，仅 damage_taken 入参换真值）。
    // 无敌人进伤（裸 Env）→ 基数 0 → 速率 0（与 vendor 无进伤语义一致）。
    let (life_recoup_rate, es_recoup_rate) = {
        let db = &env.player.mod_db;
        let cfg = &env.cfg;
        (
            calc_recoup_from_db(db, cfg, recoupable_total, RecoupResource::Life).rate_per_second,
            calc_recoup_from_db(db, cfg, recoupable_total, RecoupResource::EnergyShield)
                .rate_per_second,
        )
    };
    env.player.output.life_recoup_rate = life_recoup_rate;
    env.player.output.es_recoup_rate = es_recoup_rate;
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
/// 作为触发冷却除数。源速率门控优先取 build 层注入的 `TriggerSourceRate` BASE（触发源技能
/// 有效 cast/attack rate，对应 PoB2 EffectiveSourceRate）；未注入时回退到主技能
/// `effective_action_rate`（占位语义——主技能并非触发源）。触发速率末端再乘 triggerChance
/// （命中/暴击/显式触发几率折算，PoB2 CalcTriggers.lua L715-777）。
///
/// 无 `TriggerCooldownBase` / `CWCTriggerTime` 词条时两字段保持 0（无触发的普通 build 不进入
/// 任一分支，面板保持 0）。**build 层 `calc_orchestrator` 现已对内建触发（`Triggered` /
/// `InbuiltTrigger` 主技能）注入 `TriggeredSkillCooldown` + `TriggerCooldownBase`，并在组内有
/// 触发源技能时注入 `TriggerSourceRate`；CWC 主技能注入 `CWCTriggerTime` + `CWCAddsCastTime`。**
/// 出处：agent-docs/triggers.md §三 / §4.2；Lane B integration_spec；PoB2 CalcTriggers.lua
/// L74-86 findTriggerSkill / L702-707 EffectiveSourceRate / L715-777 triggerChance；
/// CWCHandler L262-263（finding 03-06：CWC 经 calcMultiSpellRotationImpact）。
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
    // 触发源速率：PoB2 EffectiveSourceRate 取自触发源技能（findTriggerSkill 命中的源技能
    // HitSpeed/Speed，CalcTriggers.lua L74-86/L702-707），而非被触发的主技能自身速率。build 层
    // 对源技能跑完整子计算（W-E2，GlobalCache 等价物最小版），把**计算后**有效 cast/attack
    // rate 写入 `TriggerSourceRate` BASE 注入（修 14-G2：源速率随攻速乘区增长）；未注入(=0)
    // 时回退到主技能 `effective_action_rate`（向后兼容；回退值仅为占位语义）。
    let injected_source_rate = db.sum(ModType::Base, cfg, &[ModName::from("TriggerSourceRate")]);
    let source_rate = if injected_source_rate > 0.0 {
        injected_source_rate
    } else {
        env.player.output.effective_action_rate
    };
    // 触发源命中/暴击（W-E2 契约 4 `TriggerSourceStats`）：build 层子计算结果经百分数
    // BASE 词条注入（0 = 未注入，折算跳过；triggerOnUse 链路注入侧不注入命中）。
    let source_stats = TriggerSourceStats {
        action_rate: source_rate,
        hit_chance: db.sum(
            ModType::Base,
            cfg,
            &[ModName::from("TriggerSourceHitChance")],
        ) / 100.0,
        crit_chance: db.sum(
            ModType::Base,
            cfg,
            &[ModName::from("TriggerSourceCritChance")],
        ) / 100.0,
    };
    let has_source_stats = source_stats.hit_chance > 0.0 || source_stats.crit_chance > 0.0;
    // triggerOnCrit（CoC 链路）：cfg 条件（历史通道）或 build 层数据驱动识别注入的
    // `TriggerOnCrit` FLAG（W-E1 trigger_configs 接线）。
    let trigger_on_crit =
        cfg.condition("TriggerOnCrit") || db.flag(cfg, ModName::from("TriggerOnCrit"));
    // 触发几率折算（PoB2 CalcTriggers.lua defaultTriggerHandler L715-777）：默认 1.0（=100%），
    // 仅攻击源未必中 / triggerOnCrit / 显式触发几率<100% 时降速。源统计注入时优先用**源**的
    // 命中/暴击（vendor :721/:748 取 GlobalCache 源数据）；未注入退回主技能 output 口径。
    let trigger_chance = trigger_chance_multiplier(
        cfg,
        &env.player.output,
        has_source_stats.then_some(&source_stats),
        trigger_on_crit,
    );
    // 数据驱动识别注入的触发上下文（W-E1）：
    // - `TriggerSourceGlobal` FLAG：vendor skillFlags.globalTrigger——不依赖源速率，
    //   EffectiveSourceRate = TriggerRateCap（CalcTriggers.lua:705-707）。
    // - `TriggerRateCapOverride` BASE：vendor skillData.triggerRateCapOverride
    //   （如 The Hidden Blade = 2/s）。
    // - `SkillIsTriggered` FLAG：无冷却数据的触发关系门控（vendor triggerCD=nil →
    //   模拟退化为纯源速率）。
    let is_global = db.flag(cfg, ModName::from("TriggerSourceGlobal"));
    let rate_cap_override = db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("TriggerRateCapOverride")],
    );
    let is_triggered_flagged = db.flag(cfg, ModName::from("SkillIsTriggered"));

    let mut trace = TraceGraph::new();

    if rate_cap_override > 0.0 {
        // 速率上限覆盖：cap = override（vendor 直接替换帧取整 cap）；global 时源速率
        // 即 cap，否则 min(cap, sourceRate)；末端乘 triggerChance。
        let gated = if is_global {
            rate_cap_override
        } else {
            source_rate.min(rate_cap_override)
        };
        env.player.output.trigger_rate_cap = round(rate_cap_override);
        env.player.output.skill_trigger_rate = round(gated * trigger_chance);
    } else if trigger_cd > 0.0 {
        // 冷却驱动：双门控 min(cap, sourceRate) 后乘 triggerChance（对齐 PoB2
        // calcMultiSpellRotationImpact 单技能稳态：rate ≈ min(cap, sourceRate) × geometric(chance)）。
        // global 触发不受源速率门控（源速率传 0 → resolve 取 cap）。
        let (tr, _) = resolve_trigger_rate_traced(
            trigger_cd,
            triggered_cd,
            icdr,
            if is_global { 0.0 } else { source_rate },
            cfg.constants.game().server_tick_seconds,
            &mut trace,
        );
        env.player.output.trigger_rate_cap = tr.trigger_rate_cap;
        env.player.output.skill_trigger_rate = round(tr.skill_trigger_rate * trigger_chance);
    } else if is_triggered_flagged && cwc_trigger_time <= 0.0 {
        // 识别出触发关系但无冷却数据：vendor triggerCD/triggeredCD 均空 → 触发速率
        // 纯源速率驱动（global 时无稳态速率语义，保持 0）。cap 面板保持 0（无冷却）。
        if !is_global && source_rate > 0.0 {
            env.player.output.skill_trigger_rate = round(source_rate * trigger_chance);
        }
    } else if cwc_trigger_time > 0.0 {
        // CWC：引导触发，被触发技能冷却 clamp。adds_cast_time 由 build 层经
        // `CWCAddsCastTime` BASE 注入（被触发法术 base_cast_time/cast_speed；无则 0）。
        let adds_cast_time = db.sum(ModType::Base, cfg, &[ModName::from("CWCAddsCastTime")]);
        let (cwc, _) = calc_cwc_trigger_rate_traced(
            cwc_trigger_time,
            triggered_cd,
            adds_cast_time,
            icdr,
            cfg.constants.game().server_tick_seconds,
            &mut trace,
        );
        env.player.output.trigger_rate_cap = cwc.trigger_rate_cap;
        // PoB2 CWCHandler（CalcTriggers.lua L262-263）：cap = min(1/effCD, channellingRate)，
        // 随后 SkillTriggerRate = calcMultiSpellRotationImpact(triggeredSkills, channellingRate, 0)。
        // 单被触发技能路径（finding 03-06）：用 channelling_rate 作源速率喂单技能轮转，
        // 取该技能稳态速率再被 cap clamp。多被触发技能轮转分摊待 gem-link 数据接入后扩展。
        let rotation = calc_multi_spell_rotation(
            &[RotationSkill::new(cwc.effective_triggered_cd)],
            cwc.channelling_trigger_rate,
            cfg.constants.game().server_tick_seconds,
        );
        let rotated = rotation.rates.first().copied().unwrap_or(0.0);
        env.player.output.skill_trigger_rate = round(rotated.min(cwc.trigger_rate_cap));
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

/// 触发几率乘子（fraction，0.0–1.0）：移植 PoB2 `CalcTriggers.lua` `defaultTriggerHandler`
/// 的 triggerChance（L715-777）。
///
/// - **源命中率**（W-E2 契约 4）：build 层注入触发源子计算统计时，乘**源**命中率
///   （vendor :721 `GlobalCache.cachedData[uuid].HitChance`——折的是源技能的命中，
///   非被触发主技能）；未注入时退回历史口径——触发源是攻击技能（`cfg.is_attack()`，
///   对齐 L720 `source.skillTypes[Melee] or [Attack]`）且命中率≠100% 时乘
///   `output.hit_chance`（主技能近似）。
/// - **triggerOnCrit 暴击率**：`cfg` 条件 `TriggerOnCrit`（历史通道）或 build 层
///   数据驱动识别注入 FLAG 时，乘源暴击率（注入源统计时取**源**暴击，vendor :748；
///   未注入退回 `output.crit_chance`，对齐 L743-767）。
/// - **显式触发几率**：`cfg.multipliers["TriggerChance"]`（百分数，build 层注入）<100% 时乘其
///   `/100`（对齐 L772-776）。**用 `.get()` 区分「未注入」与「注入 0」——`cfg.multiplier()` 缺省
///   返回 0.0 会把未注入误判为 0% 几率。**
///
/// 无任何触发上下文注入时返回 1.0（与历史输出一致）。`hit_chance`/`crit_chance` 在 `output` /
/// `TriggerSourceStats` 中已是 fraction，可直接相乘。
fn trigger_chance_multiplier(
    cfg: &CalcConfig,
    output: &OutputTable,
    source_stats: Option<&TriggerSourceStats>,
    trigger_on_crit: bool,
) -> f64 {
    let mut chance = 1.0_f64;
    match source_stats {
        // W-E2：源子计算统计在场 → 命中/暴击折算全取源口径（contract 4）。
        Some(stats) => {
            chance *= stats.chance_multiplier(trigger_on_crit);
        }
        // 历史回退：主技能 output 近似（仅攻击 cfg 折命中）。
        None => {
            if cfg.is_attack() && output.hit_chance < 1.0 {
                chance *= output.hit_chance.clamp(0.0, 1.0);
            }
            if trigger_on_crit && output.crit_chance < 1.0 {
                chance *= output.crit_chance.clamp(0.0, 1.0);
            }
        }
    }
    if let Some(&pct) = cfg.multipliers.get("TriggerChance")
        && pct < 100.0
    {
        chance *= (pct / 100.0).clamp(0.0, 1.0);
    }
    chance.clamp(0.0, 1.0)
}

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

    // 活跃叠层估算所需的面板信号（PoB2 `ailmentStacks = hitChance × applyChance × duration × speed`）。
    // `output.hit_chance` 已是 fraction（命中公式直接产出 0..1，见 defence::hit_chance）；
    // 命中速率取 fill_mechanics 已写入的有效行动速率（无速率 → 0）。
    let hit_chance_frac = env.player.output.hit_chance.clamp(0.0, 1.0);
    let hit_speed = env.player.output.effective_action_rate.max(0.0);

    // 敌方异常阈值（怪物等级查表 × EnemyAilmentThreshold mod）；无敌人配置时回退裸表。
    let threshold = enemy_ailment_threshold_effective(enemy, cfg, env.enemy.level);
    // 敌方姿态阈值（冰冻/电击姿态积累用；与异常阈值平行，含 floor）。
    let poise_thr = enemy_poise_threshold_effective(enemy, cfg, env.enemy.level);

    // trace 与本步聚合的归因绑在 player.breakdown 之外的临时图：写入 output 字段即可，
    // 完整 trace 由 traced offence/归因路径统一收口（本函数构建并保留贡献节点拓扑）。
    let mut trace = TraceGraph::new();

    if phys_hit > 0.0 {
        // Pass 1（50% roll, 裸暴击）：取施加几率 + 持续时间，用于估算活跃叠层（PoB2 L5043-5046）。
        let probe = AilmentSource::new(phys_hit, crit_mult, crit_chance, never_from_crit);
        let (probe_out, _) = bleed_traced(&probe, player, enemy, cfg, &mut trace);
        // 活跃叠层估算 → StackConfig（active_stacks 真正生效，05-01）。SP = active/max，可 > 1。
        let bleed_stack = resolve_stack_config(
            player,
            cfg,
            "Bleed",
            hit_chance_frac,
            probe_out.chance,
            ailment_duration(AilmentType::Bleed, player, cfg),
            hit_speed,
        );
        let sp = stack_potential(&bleed_stack);
        // over-stacking 暴击放大（PoB2 L5144 `ailmentCritChance`）：SP>1 时"至少一层暴击"概率升高。
        let bleed_crit = ailment_crit_chance(crit_chance, sp);
        // RollAverage 高位偏移（05-04，PoB2 L5098-5125）：SP>1 时来源命中向高端内插。
        let roll = roll_average(&bleed_stack);
        let phys_hit_rolled =
            cross_type_source_hit_at_roll(AilmentType::Bleed, components, player, cfg, roll);
        // Pass 2：高 roll 来源 + over-stacking 暴击 → 最终伤害。
        let source = AilmentSource::new(phys_hit_rolled, crit_mult, bleed_crit, never_from_crit);
        let (bleed, _) = bleed_traced(&source, player, enemy, cfg, &mut trace);
        // Lane C：AilmentEffect（MORE）× rateMod（Faster/Slower）应用到期望 DPS，再 clamp DotDpsCap。
        let bleed_dps = finalize_ailment_dps(bleed.expected_dps, "Bleed", player, enemy, cfg);
        env.player.output.bleed_dps = bleed_dps;

        // Lane B：流血叠层（BleedStacks BASE × 活跃叠层）。复用上方已解析的 bleed_stack。
        let (bleed_stacked, _) =
            stacking_ailment_dps_traced(bleed_dps, &bleed_stack, AilmentType::Bleed, &mut trace);
        // 叠层 DPS 也吃全局 DotDpsCap（PoB2：DotDpsCap 是叠层后的绝对上限）。
        env.player.output.bleed_stacked_dps =
            apply_dot_dps_cap(bleed_stacked, cfg.constants.game().dot_dps_cap);
        env.player.output.bleed_active_stacks = active_stacks_of(&bleed_stack);
    }
    if fire_hit > 0.0 {
        // Pass 1（50% roll, 裸暴击）：施加几率 + 持续 → 活跃叠层估算。
        let probe = AilmentSource::new(fire_hit, crit_mult, crit_chance, never_from_crit);
        let (probe_out, _) = ignite_traced(&probe, player, enemy, cfg, threshold, &mut trace);
        let ignite_stack = resolve_stack_config(
            player,
            cfg,
            "Ignite",
            hit_chance_frac,
            probe_out.chance,
            ailment_duration(AilmentType::Ignite, player, cfg),
            hit_speed,
        );
        let sp = stack_potential(&ignite_stack);
        let ignite_crit = ailment_crit_chance(crit_chance, sp);
        let roll = roll_average(&ignite_stack);
        let fire_hit_rolled =
            cross_type_source_hit_at_roll(AilmentType::Ignite, components, player, cfg, roll);
        let source = AilmentSource::new(fire_hit_rolled, crit_mult, ignite_crit, never_from_crit);
        let (ignite, _) = ignite_traced(&source, player, enemy, cfg, threshold, &mut trace);
        let ignite_dps = finalize_ailment_dps(ignite.expected_dps, "Ignite", player, enemy, cfg);
        env.player.output.ignite_dps = ignite_dps;

        // Lane B：点燃叠层（IgniteStacks BASE）。PoE2 点燃默认不叠层（只取最强一层），
        // 仅在携带 `IgniteStacks` 词条时 max_stacks>1；复用上方已解析的 ignite_stack。
        let (ignite_stacked, _) =
            stacking_ailment_dps_traced(ignite_dps, &ignite_stack, AilmentType::Ignite, &mut trace);
        env.player.output.ignite_stacked_dps =
            apply_dot_dps_cap(ignite_stacked, cfg.constants.game().dot_dps_cap);
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
        // Pass 1（50% roll, 裸暴击）：施加几率 + 持续 → 活跃叠层估算。
        let probe = AilmentSource::new(chaos_phys_hit, crit_mult, crit_chance, never_from_crit);
        let (probe_out, _) = poison_traced(&probe, player, enemy, cfg, &mut trace);
        let poison_stack = resolve_stack_config(
            player,
            cfg,
            "Poison",
            hit_chance_frac,
            probe_out.chance,
            ailment_duration(AilmentType::Poison, player, cfg),
            hit_speed,
        );
        let sp = stack_potential(&poison_stack);
        let poison_crit = ailment_crit_chance(crit_chance, sp);
        let roll = roll_average(&poison_stack);
        let chaos_phys_rolled =
            cross_type_source_hit_at_roll(AilmentType::Poison, components, player, cfg, roll);
        let source = AilmentSource::new(chaos_phys_rolled, crit_mult, poison_crit, never_from_crit);
        let (poison, _) = poison_traced(&source, player, enemy, cfg, &mut trace);
        let poison_dps = finalize_ailment_dps(poison.expected_dps, "Poison", player, enemy, cfg);
        env.player.output.poison_dps = poison_dps;

        // Lane B：中毒叠层（PoisonStacks BASE × 活跃叠层）。复用上方已解析的 poison_stack。
        let (poison_stacked, _) =
            stacking_ailment_dps_traced(poison_dps, &poison_stack, AilmentType::Poison, &mut trace);
        env.player.output.poison_stacked_dps =
            apply_dot_dps_cap(poison_stacked, cfg.constants.game().dot_dps_cap);
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
    apply_dot_dps_cap(scaled, cfg.constants.game().dot_dps_cap)
}

/// 从 ModDb 解析某 damaging ailment 的叠层配置（`<Ailment>Stacks` BASE → max_stacks）+
/// 估算平均活跃叠层数（05-01：`ailmentStacks = hitChance × applyChance × duration × speed`）。
///
/// - `max_stacks`：`1 + Σ<Ailment>Stacks BASE`（PoB2 仅在 `<Ailment>CanStack` 时 >1，此处用
///   `<Ailment>Stacks` 词条存在与否近似；无词条时 max_stacks=1，不叠层）。
/// - `active_stacks`：[`estimate_active_stacks`] 估算值。**面板信号缺失（无攻击/施法速率，
///   如纯单元 build）时为 0**，由 [`active_stacks_of`]/`stacking_ailment_dps` 回退到 max_stacks
///   作上界（= 旧的"恒满层"占位口径，向后兼容）。有速率时估算值真正生效，使
///   StackPotential 可 > 1，触发 over-stacking 暴击放大与 RollAverage 高位偏移。
///
/// 出处：PoB2 `CalcOffence.lua` L5021-5069。
#[allow(clippy::too_many_arguments)]
fn resolve_stack_config(
    db: &ModDb,
    cfg: &CalcConfig,
    ailment: &str,
    hit_chance_frac: f64,
    apply_chance_frac: f64,
    duration_secs: f64,
    hit_speed: f64,
) -> StackConfig {
    let base_stacks = db.sum(
        ModType::Base,
        cfg,
        &[ModName::from(format!("{ailment}Stacks"))],
    );
    let max_stacks = (1.0 + base_stacks).max(1.0) as u32;
    let active_stacks =
        estimate_active_stacks(hit_chance_frac, apply_chance_frac, duration_secs, hit_speed);
    StackConfig::new(max_stacks, active_stacks)
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
    // 不在此处用 0.9 提前截断：PoB2 仅在 armour+flat 求和后按 DamageReductionMax clamp 一次
    // （CalcDefence.lua:396），上限截断统一交给 ehp 层（可变 dr_max）。这里只保证非负。
    (pct / 100.0).max(0.0)
}
