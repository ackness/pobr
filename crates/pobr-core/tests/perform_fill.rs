use pobr_core::calc::actor::{Actor, ActorBaseStats};
use pobr_core::calc::env::Env;
use pobr_core::calc::perform::perform;
use pobr_core::calc::{
    AttributeInfusion, MinionData, MinionInput, MinionModifierEntry, build_minion_context,
};
use pobr_core::{CalcConfig, Modifier};
use pobr_data::prelude::*;

fn player_with(base: ActorBaseStats, mods: Vec<Modifier>) -> Env {
    let mut actor = Actor::new(1, base);
    actor.mod_db.add_list(mods);
    Env::new(actor)
}

#[test]
fn perform_fills_effective_action_rate_and_skill_use_time() {
    let base = ActorBaseStats {
        action_rate: 1.0,
        hit_min: 100.0,
        hit_max: 100.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(
        base,
        vec![Modifier::number("AttackSpeed", ModType::Inc, 50.0)],
    );
    perform(&mut env).unwrap();

    assert!(env.player.output.skill_use_time.is_some());
    assert!(env.player.output.effective_action_rate > 0.0);
}

#[test]
fn perform_fills_ehp_from_pools_and_resistances() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(base, vec![]);
    perform(&mut env).unwrap();

    assert_eq!(env.player.output.life, 1000.0);
    assert!(env.player.output.total_ehp > 0.0);
    // With 0% resist, an element max hit equals the life pool.
    assert_eq!(env.player.output.fire_max_hit, 1000.0);
}

/// PoE2 口径修正（gap: no-ailment-chance-pipeline）：流血需显式 `BleedChance` 才施加。
/// 无 `BleedChance` 时 `bleed_dps == 0`（即便打出巨额物理击中）；有几率时按 几率×DoT 期望值输出。
#[test]
fn perform_fills_bleed_dps_only_with_bleed_chance() {
    let base = ActorBaseStats {
        life: 1000.0,
        hit_min: 1000.0,
        hit_max: 1000.0,
        ..ActorBaseStats::default()
    };

    // 无 BleedChance → 不施加流血。
    let mut no_chance = player_with(base, vec![]);
    no_chance.cfg = CalcConfig::attack().with_damage_type(DamageType::Physical);
    perform(&mut no_chance).unwrap();
    assert_eq!(no_chance.player.output.bleed_dps, 0.0);

    // 100% BleedChance → 施加流血，DPS > 0。
    let mut with_chance = player_with(
        base,
        vec![Modifier::number("BleedChance", ModType::Base, 100.0)],
    );
    with_chance.cfg = CalcConfig::attack().with_damage_type(DamageType::Physical);
    perform(&mut with_chance).unwrap();
    assert!(with_chance.player.output.bleed_dps > 0.0);
}

/// PoE2 格挡上限测试（Bug#11：上限为 90%，非 PoE1 的 75%）。
///
/// 出处：agent-docs/block.md §被动格挡、PoB2 `BlockChanceCap = 90`。
#[test]
fn perform_fills_block_chance() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(
        base,
        vec![
            // 95% block → capped at PoE2 limit 90%
            Modifier::number("BlockChance", ModType::Base, 95.0),
        ],
    );
    perform(&mut env).unwrap();

    // PoE2: block capped at 90 (not PoE1's 75).
    assert_eq!(env.player.output.block_chance, 90.0);
}

/// 端到端：点燃几率派生 + effMult（敌方火抗降低点燃 DPS）经 `setup_enemy` 全管线。
#[test]
fn perform_ignite_dps_drops_with_enemy_fire_resist() {
    use pobr_core::calc::setup_env::setup_enemy;

    let base = ActorBaseStats {
        life: 1000.0,
        hit_min: 2000.0,
        hit_max: 2000.0,
        ..ActorBaseStats::default()
    };

    // 火焰附加伤害技能，有效口径，对 lv1 敌人。
    let make_env = |fire_resist: f64| {
        let mut actor = Actor::new(1, base);
        actor
            .mod_db
            .add_mod(Modifier::number("FireDamageMin", ModType::Base, 2000.0));
        actor
            .mod_db
            .add_mod(Modifier::number("FireDamageMax", ModType::Base, 2000.0));
        let mut env = Env::new(actor);
        env.cfg = CalcConfig::attack()
            .with_damage_type(DamageType::Fire)
            .with_mode_effective(true);
        setup_enemy(&mut env, 1, EnemyTier::None);
        // 注入敌方火抗（覆盖默认）。
        if fire_resist != 0.0 {
            env.enemy
                .mod_db
                .add_mod(Modifier::number("FireResist", ModType::Base, fire_resist));
        }
        env
    };

    let mut no_resist = make_env(0.0);
    perform(&mut no_resist).unwrap();
    let mut with_resist = make_env(50.0);
    perform(&mut with_resist).unwrap();

    assert!(
        no_resist.player.output.ignite_dps > 0.0,
        "ignite should apply"
    );
    // 50% 火抗 → effMult 0.5 → 点燃 DPS 约减半（几率派生不变，仅 effMult 变）。
    assert!(
        with_resist.player.output.ignite_dps < no_resist.player.output.ignite_dps,
        "fire resist should reduce ignite DPS via effMult"
    );
}

#[test]
fn perform_does_not_disturb_base_outputs() {
    let base = ActorBaseStats {
        life: 500.0,
        mana: 200.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(
        base,
        vec![Modifier::number("MaximumLife", ModType::Inc, 20.0)],
    );
    perform(&mut env).unwrap();

    // base offence/defence pipeline unaffected by the fill phase.
    assert_eq!(env.player.output.life, 600.0);
    assert_eq!(env.player.output.mana, 200.0);
}

// ─────────────────────────────────────────────────────────────────
// Lane2 集成：防御扩展字段（ES 充能 / 规避 / 承受乘数 / 暴击减免）
// ─────────────────────────────────────────────────────────────────

/// 防御新字段默认中性：无 ES → 充能 0、延迟 4；无规避词条 → 0；承受乘数默认 1.0；
/// 敌人暴击效果默认 1.0（无敌人暴击）。
#[test]
fn perform_defence_ext_defaults_are_neutral() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(base, vec![]);
    perform(&mut env).unwrap();

    let o = &env.player.output;
    assert_eq!(o.es_recharge_rate, 0.0);
    assert_eq!(o.es_recharge_delay, 4.0);
    assert_eq!(o.es_recharge_per_second, 0.0);
    assert_eq!(o.avoid_all_damage_from_hits, 0.0);
    assert_eq!(o.avoid_freeze, 0.0);
    // 承受乘数默认中性。
    assert_eq!(o.taken_multi_physical, 1.0);
    assert_eq!(o.taken_multi_fire, 1.0);
    assert_eq!(o.crit_extra_damage_reduction, 0.0);
    assert_eq!(o.enemy_crit_effect, 1.0);
}

/// ES 充能：有 ES 时充能速率 12.5%/s，每秒绝对量 = rate × ES。
#[test]
fn perform_fills_es_recharge_from_energy_shield() {
    let base = ActorBaseStats {
        life: 1000.0,
        energy_shield: 800.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(base, vec![]);
    perform(&mut env).unwrap();

    let o = &env.player.output;
    assert_eq!(o.energy_shield, 800.0);
    // 默认 750%/min → 12.5%/s。
    assert!((o.es_recharge_rate - 0.125).abs() < 1e-9);
    assert!((o.es_recharge_per_second - 0.125 * 800.0).abs() < 1e-6);
    // ZealotsOath 禁用充能。
    let mut zealots = player_with(base, vec![Modifier::flag("ZealotsOath")]);
    perform(&mut zealots).unwrap();
    assert_eq!(zealots.player.output.es_recharge_rate, 0.0);
}

/// 规避词条接入：AvoidAllDamageFromHitsChance 写入面板，超 75 被 cap。
#[test]
fn perform_fills_avoidance_chances() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(
        base,
        vec![
            Modifier::number("AvoidAllDamageFromHitsChance", ModType::Base, 90.0),
            Modifier::number("AvoidShock", ModType::Base, 40.0),
        ],
    );
    perform(&mut env).unwrap();

    // 击中规避 cap 到 75。
    assert_eq!(env.player.output.avoid_all_damage_from_hits, 75.0);
    assert_eq!(env.player.output.avoid_shock, 40.0);
}

/// 承受乘数 + 暴击额外减免接入：增加承受 → 乘数 > 1；ReduceCritExtraDamage 写入。
#[test]
fn perform_fills_taken_multi_and_crit_reduction() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(
        base,
        vec![
            Modifier::number("PhysicalDamageTaken", ModType::Inc, 20.0),
            Modifier::number("ReduceCritExtraDamage", ModType::Base, 30.0),
        ],
    );
    perform(&mut env).unwrap();

    // +20% 物理承受 → 乘数 1.2。
    assert!((env.player.output.taken_multi_physical - 1.2).abs() < 1e-9);
    assert_eq!(env.player.output.crit_extra_damage_reduction, 30.0);
}

/// 敌人暴击效果：敌人有暴击几率/爆伤 → enemy_crit_effect > 1；减免缩放它。
#[test]
fn perform_enemy_crit_effect_scales_with_reduction() {
    use pobr_core::calc::setup_env::setup_enemy;

    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };

    let make = |reduce: f64| {
        let mut actor = Actor::new(1, base);
        if reduce != 0.0 {
            actor.mod_db.add_mod(Modifier::number(
                "ReduceCritExtraDamage",
                ModType::Base,
                reduce,
            ));
        }
        let mut env = Env::new(actor);
        setup_enemy(&mut env, 1, EnemyTier::None);
        // 注入敌人暴击。
        env.enemy
            .mod_db
            .add_mod(Modifier::number("CritChance", ModType::Base, 50.0));
        env.enemy
            .mod_db
            .add_mod(Modifier::number("CritMultiplier", ModType::Base, 100.0));
        env
    };

    let mut no_reduce = make(0.0);
    perform(&mut no_reduce).unwrap();
    let mut with_reduce = make(50.0);
    perform(&mut with_reduce).unwrap();

    // 1 + 0.5 * 1.0 = 1.5（无减免）。
    assert!((no_reduce.player.output.enemy_crit_effect - 1.5).abs() < 1e-9);
    // 50% 减免 → 1 + 0.5 * 1.0 * 0.5 = 1.25。
    assert!((with_reduce.player.output.enemy_crit_effect - 1.25).abs() < 1e-9);
}

// ─────────────────────────────────────────────────────────────────
// Lane4 集成：召唤物多 Actor（offence/defence 复用玩家管线）
// ─────────────────────────────────────────────────────────────────

/// 无召唤物时 minions 输出为空，玩家行为不变（向后兼容）。
#[test]
fn perform_without_minions_leaves_minion_output_empty() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(base, vec![]);
    perform(&mut env).unwrap();
    assert!(env.player.output.minions.is_empty());
}

/// 召唤物接入：单召唤物经 build_minion_context → Env::add_minion，perform 后产出
/// 独立的 offence/defence 快照（life/dps 来自召唤物管线）。
#[test]
fn perform_runs_minion_offence_and_defence() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(base, vec![]);
    env.cfg = CalcConfig::attack().with_damage_type(DamageType::Physical);

    // 攻击型召唤物：有虚拟武器伤害（damage 归一化 > 0）。
    let data = MinionData {
        damage: 1.0,
        attack_time: 1.0,
        ..MinionData::default()
    };
    let ctx = build_minion_context(&MinionInput {
        gem_level: 20,
        data,
        minion_modifiers: vec![],
        ally_buff_mods: vec![],
        attribute_infusion: AttributeInfusion::default(),
        minion_type: None,
    });
    env.add_minion(ctx);

    perform(&mut env).unwrap();

    assert_eq!(env.player.output.minions.len(), 1);
    let m = &env.player.output.minions[0];
    // 召唤物等级 = 宝石 20 → 怪物等级 40。
    assert_eq!(m.level, 40);
    // 召唤物生命来自怪物表（> 0）。
    assert!(m.life > 0.0, "minion life should derive from monster table");
    // 召唤物虚拟武器 → DPS > 0。
    assert!(m.dps > 0.0, "attacking minion should deal damage");
    // 玩家自身输出不受召唤物影响。
    assert_eq!(env.player.output.life, 1000.0);
}

/// 召唤物三通道之一：MinionModifier 注入「增加召唤物生命」→ 召唤物生命提升。
#[test]
fn perform_minion_modifier_channel_scales_minion_life() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };

    let make = |mods: Vec<MinionModifierEntry>| {
        let mut env = player_with(base, vec![]);
        let ctx = build_minion_context(&MinionInput {
            gem_level: 20,
            data: MinionData::default(),
            minion_modifiers: mods,
            ally_buff_mods: vec![],
            attribute_infusion: AttributeInfusion::default(),
            minion_type: None,
        });
        env.add_minion(ctx);
        perform(&mut env).unwrap();
        env.player.output.minions[0].life
    };

    let base_life = make(vec![]);
    // 通道 1：MinionModifier 包裹「召唤物 +50% 最大生命」。
    let buffed_life = make(vec![MinionModifierEntry {
        inner: Modifier::number("MaximumLife", ModType::Inc, 50.0),
        minion_type: None,
    }]);
    assert!(
        buffed_life > base_life,
        "MinionModifier(+50% life) should raise minion life: {buffed_life} vs {base_life}"
    );
}

// ─────────────────────────────────────────────────────────────────
// Lane A 集成：防御恢复（充能 / 偷取 / Recoup / regen 超集）
// ─────────────────────────────────────────────────────────────────

/// 防御恢复新字段默认中性：无来源 → 充能 current=0/maximum=3、偷取 0、Recoup 0。
#[test]
fn perform_recovery_ext_defaults_are_neutral() {
    let base = ActorBaseStats {
        life: 1000.0,
        mana: 200.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(base, vec![]);
    perform(&mut env).unwrap();

    let o = &env.player.output;
    // 充能默认上限 3、当前 0（无 multiplier config）。
    assert_eq!(o.charge_power_current, 0);
    assert_eq!(o.charge_power_maximum, 3);
    assert_eq!(o.charge_frenzy_maximum, 3);
    assert_eq!(o.charge_endurance_maximum, 3);
    // 无偷取/Recoup 词条 → 速率 0。
    assert_eq!(o.life_leech_rate, 0.0);
    assert_eq!(o.mana_leech_rate, 0.0);
    assert_eq!(o.es_leech_rate, 0.0);
    assert_eq!(o.life_recoup_rate, 0.0);
    assert_eq!(o.es_recoup_rate, 0.0);
}

/// 充能上限词条接入：+2 to Maximum Power Charges → maximum=5；当前层数由 multiplier config。
#[test]
fn perform_fills_charge_maximum_and_current() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(
        base,
        vec![Modifier::number("PowerChargesMax", ModType::Base, 2.0)],
    );
    // 当前 4 层 power charge（multiplier config），会被 cap 到 maximum=5。
    env.cfg = env.cfg.with_multiplier("PowerCharge", 4.0);
    perform(&mut env).unwrap();

    assert_eq!(env.player.output.charge_power_maximum, 5);
    assert_eq!(env.player.output.charge_power_current, 4);
}

/// 偷取接入：物理命中 + LifeLeech BASE → life_leech_rate > 0。
#[test]
fn perform_fills_life_leech_from_physical_hit() {
    let base = ActorBaseStats {
        life: 1000.0,
        hit_min: 1000.0,
        hit_max: 1000.0,
        ..ActorBaseStats::default()
    };
    // 无偷取 → 0。
    let mut no_leech = player_with(base, vec![]);
    no_leech.cfg = CalcConfig::attack().with_damage_type(DamageType::Physical);
    perform(&mut no_leech).unwrap();
    assert_eq!(no_leech.player.output.life_leech_rate, 0.0);

    // 5% LifeLeech → 速率 > 0（取最高速率实例口径）。
    let mut with_leech = player_with(
        base,
        vec![Modifier::number("LifeLeech", ModType::Base, 5.0)],
    );
    with_leech.cfg = CalcConfig::attack().with_damage_type(DamageType::Physical);
    perform(&mut with_leech).unwrap();
    assert!(with_leech.player.output.life_leech_rate > 0.0);
}

/// Recoup 接入：LifeRecoup BASE → life_recoup_rate > 0（以 10% 生命估算受击）。
#[test]
fn perform_fills_life_recoup_rate() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(
        base,
        vec![Modifier::number("LifeRecoup", ModType::Base, 20.0)],
    );
    perform(&mut env).unwrap();
    // damage_taken_estimate = 1000 * 0.1 = 100；recoup 20% = 20 在 8s 内 → 2.5/s。
    assert!((env.player.output.life_recoup_rate - 2.5).abs() < 1e-9);
}

/// regen 超集：XRecoveryRate 全局恢复速率乘进 regen（calc_regen 行为超集）。
#[test]
fn perform_regen_picks_up_global_recovery_rate() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    // 1%/s 生命再生 + 100% 增加生命恢复速率（LifeRecoveryRate INC）。
    let mut env = player_with(
        base,
        vec![
            Modifier::number("LifeRegenPercent", ModType::Base, 1.0),
            Modifier::number("LifeRecoveryRate", ModType::Inc, 100.0),
        ],
    );
    perform(&mut env).unwrap();
    // base regen = 1000 * 1% = 10；×(1 + 100/100) = 20。
    assert!((env.player.output.life_regen - 20.0).abs() < 1e-6);
}

// ─────────────────────────────────────────────────────────────────
// Lane B 集成：异常扩展（冰缓 / 冰冻·电击姿态积累 / 流血·中毒叠层）
// ─────────────────────────────────────────────────────────────────

fn cold_hit_env(extra: Vec<Modifier>) -> Env {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut actor = Actor::new(1, base);
    // 巨额冷伤命中以越过冰缓最低阈值。
    actor
        .mod_db
        .add_mod(Modifier::number("ColdDamageMin", ModType::Base, 200000.0));
    actor
        .mod_db
        .add_mod(Modifier::number("ColdDamageMax", ModType::Base, 200000.0));
    actor.mod_db.add_list(extra);
    let mut env = Env::new(actor);
    env.cfg = CalcConfig::attack().with_damage_type(DamageType::Cold);
    env
}

/// 冰缓接入：足量冷伤命中 → chill_effect > 0；冰冻姿态积累 > 0。
#[test]
fn perform_fills_chill_and_freeze_buildup_from_cold_hit() {
    let mut env = cold_hit_env(vec![]);
    perform(&mut env).unwrap();

    assert!(
        env.player.output.chill_effect > 0.0,
        "large cold hit should apply chill: {}",
        env.player.output.chill_effect
    );
    assert!(
        env.player.output.freeze_buildup_pct > 0.0,
        "cold hit should accumulate freeze poise buildup"
    );
}

/// 无冷伤命中时冰缓/冰冻积累保持 0（向后兼容：纯物理 build 不受影响）。
#[test]
fn perform_chill_zero_without_cold_hit() {
    let base = ActorBaseStats {
        life: 1000.0,
        hit_min: 1000.0,
        hit_max: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(base, vec![]);
    env.cfg = CalcConfig::attack().with_damage_type(DamageType::Physical);
    perform(&mut env).unwrap();
    assert_eq!(env.player.output.chill_effect, 0.0);
    assert_eq!(env.player.output.freeze_buildup_pct, 0.0);
}

/// 电击姿态积累接入：闪电命中 → electrocute_buildup_pct > 0。
#[test]
fn perform_fills_electrocute_buildup_from_lightning_hit() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut actor = Actor::new(1, base);
    actor.mod_db.add_mod(Modifier::number(
        "LightningDamageMin",
        ModType::Base,
        5000.0,
    ));
    actor.mod_db.add_mod(Modifier::number(
        "LightningDamageMax",
        ModType::Base,
        5000.0,
    ));
    let mut env = Env::new(actor);
    env.cfg = CalcConfig::attack().with_damage_type(DamageType::Lightning);
    perform(&mut env).unwrap();

    assert!(
        env.player.output.electrocute_buildup_pct > 0.0,
        "lightning hit should accumulate electrocute poise buildup"
    );
}

/// 叠层接入：BleedStacks BASE → bleed_stacked_dps = 单层 × 活跃层数；默认单层时相等。
#[test]
fn perform_bleed_stacking_multiplies_dps() {
    let base = ActorBaseStats {
        life: 1000.0,
        hit_min: 1000.0,
        hit_max: 1000.0,
        ..ActorBaseStats::default()
    };

    // 单层（默认）：stacked == 单层 DPS。
    let mut single = player_with(
        base,
        vec![Modifier::number("BleedChance", ModType::Base, 100.0)],
    );
    single.cfg = CalcConfig::attack().with_damage_type(DamageType::Physical);
    perform(&mut single).unwrap();
    let one_layer = single.player.output.bleed_dps;
    assert!(one_layer > 0.0);
    assert!((single.player.output.bleed_stacked_dps - one_layer).abs() < 1e-6);
    assert_eq!(single.player.output.bleed_active_stacks, 1.0);

    // +2 BleedStacks → max_stacks=3 → stacked ≈ 单层 × 3。
    let mut stacked = player_with(
        base,
        vec![
            Modifier::number("BleedChance", ModType::Base, 100.0),
            Modifier::number("BleedStacks", ModType::Base, 2.0),
        ],
    );
    stacked.cfg = CalcConfig::attack().with_damage_type(DamageType::Physical);
    perform(&mut stacked).unwrap();
    assert_eq!(stacked.player.output.bleed_active_stacks, 3.0);
    assert!((stacked.player.output.bleed_stacked_dps - one_layer * 3.0).abs() < 1e-3);
}

/// 05-01/05-04 综合：高攻速 + 暴击的流血 build → StackPotential > 1，触发 over-stacking
/// 暴击放大 + RollAverage 高位偏移，bleed_dps 高于"无速率（SP=1）"的同配置基线。
///
/// 无速率 build：`effective_action_rate=0` → active_stacks 估算为 0 → SP=1 → 裸暴击 + 50% roll。
/// 有速率 build：active_stacks = hit×chance×duration×speed ≫ max_stacks(=1) → SP≫1 →
/// 暴击份额 `1-(1-c)^SP`（接近全暴击）+ roll 向高端偏移 → magnitude 明显更高。
#[test]
fn perform_overstacking_amplifies_bleed_dps_with_speed_and_crit() {
    // 物理击中区间 [400, 1600]（min≠max，使 RollAverage 内插可见）。
    let base = ActorBaseStats {
        life: 1000.0,
        hit_min: 400.0,
        hit_max: 1600.0,
        action_rate: 1.6, // 攻速基底（仅"有速率"分支用；无速率分支置 0）
        ..ActorBaseStats::default()
    };
    let crit_bleed_mods = || {
        vec![
            Modifier::number("BleedChance", ModType::Base, 100.0),
            Modifier::number("CriticalStrikeChance", ModType::Base, 50.0),
            Modifier::number("CriticalStrikeMultiplier", ModType::Base, 100.0),
        ]
    };

    // 基线：无攻速（action_rate=0）→ SP=1（active_stacks 估算为 0，回退 max=1）。
    let no_speed = ActorBaseStats {
        action_rate: 0.0,
        ..base
    };
    let mut baseline = {
        let mut actor = Actor::new(1, no_speed);
        actor.mod_db.add_list(crit_bleed_mods());
        let mut env = Env::new(actor);
        env.cfg = CalcConfig::attack().with_damage_type(DamageType::Physical);
        env
    };
    perform(&mut baseline).unwrap();
    let baseline_dps = baseline.player.output.bleed_dps;
    assert!(baseline_dps > 0.0, "baseline bleed should apply");
    // 无速率 → active_stacks 回退到 max_stacks=1（SP=1，无放大）。
    assert_eq!(
        baseline.player.output.bleed_active_stacks, 1.0,
        "no speed → active_stacks falls back to max=1"
    );

    // SP>1：有攻速 → active_stacks ≫ 1 → over-stacking 放大生效。
    let mut overstack = {
        let mut actor = Actor::new(1, base);
        actor.mod_db.add_list(crit_bleed_mods());
        let mut env = Env::new(actor);
        env.cfg = CalcConfig::attack().with_damage_type(DamageType::Physical);
        env
    };
    perform(&mut overstack).unwrap();
    let overstack_dps = overstack.player.output.bleed_dps;

    // active_stacks 应远大于 1（hit≈1 × chance=1 × 5s × ~1.6/s ≈ 8 层，max=1 → SP≈8）。
    assert!(
        overstack.player.output.bleed_active_stacks > 1.0,
        "speed → active_stacks > 1 (SP>1), got {}",
        overstack.player.output.bleed_active_stacks
    );
    // over-stacking 暴击放大 + RollAverage 高位偏移 → 单层 magnitude（bleed_dps）严格更高。
    assert!(
        overstack_dps > baseline_dps,
        "SP>1 should amplify bleed_dps: overstack {overstack_dps} vs baseline {baseline_dps}"
    );
}

/// 点燃叠层接入（P1-2）：默认 max_stacks=1（stacked==单层）；`IgniteStacks` BASE → 叠层翻倍。
#[test]
fn perform_ignite_stacking_multiplies_dps() {
    let base = ActorBaseStats {
        life: 1000.0,
        hit_min: 2000.0,
        hit_max: 2000.0,
        ..ActorBaseStats::default()
    };
    let fire_skill = |extra: Vec<Modifier>| {
        let mut mods = vec![
            Modifier::number("FireDamageMin", ModType::Base, 2000.0),
            Modifier::number("FireDamageMax", ModType::Base, 2000.0),
            Modifier::number("IgniteChance", ModType::Base, 100.0),
        ];
        mods.extend(extra);
        let mut env = player_with(base, mods);
        env.cfg = CalcConfig::attack().with_damage_type(DamageType::Fire);
        env
    };

    // 默认（无 IgniteStacks）：stacked == 单层 ignite_dps，活跃层数 1。
    let mut single = fire_skill(vec![]);
    perform(&mut single).unwrap();
    let one_layer = single.player.output.ignite_dps;
    assert!(one_layer > 0.0, "ignite should apply with fire damage");
    assert!((single.player.output.ignite_stacked_dps - one_layer).abs() < 1e-6);
    assert_eq!(single.player.output.ignite_active_stacks, 1.0);

    // +2 IgniteStacks → max_stacks=3 → stacked ≈ 单层 × 3。
    let mut stacked = fire_skill(vec![Modifier::number("IgniteStacks", ModType::Base, 2.0)]);
    perform(&mut stacked).unwrap();
    assert_eq!(stacked.player.output.ignite_active_stacks, 3.0);
    assert!((stacked.player.output.ignite_stacked_dps - one_layer * 3.0).abs() < 1e-3);
}

// ─────────────────────────────────────────────────────────────────
// Lane C 集成：技能功能（AoE / 投射物 / 冷却 / 消耗）
// ─────────────────────────────────────────────────────────────────

/// 技能功能默认中性：无 base 词条 → AoE/cooldown/cost 全 0；投射物 0（无投射物来源）。
#[test]
fn perform_skill_mechanics_defaults_are_neutral() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(base, vec![]);
    perform(&mut env).unwrap();

    let o = &env.player.output;
    assert_eq!(o.aoe_radius, 0.0);
    assert_eq!(o.aoe_area_mod, 0.0);
    assert_eq!(o.projectile_count, 0.0);
    assert_eq!(o.cooldown, 0.0);
    assert_eq!(o.cooldown_stored_uses, 0);
    assert_eq!(o.mana_cost, 0.0);
    assert_eq!(o.life_cost, 0.0);
    assert_eq!(o.spirit_reserved, 0.0);
}

/// AoE 接入：SkillAreaRadiusBase BASE + AreaOfEffect INC → 半径与面积乘数 > 0。
#[test]
fn perform_fills_aoe_radius_from_base_and_inc() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(
        base,
        vec![
            Modifier::number("SkillAreaRadiusBase", ModType::Base, 20.0),
            Modifier::number("AreaOfEffect", ModType::Inc, 44.0),
        ],
    );
    perform(&mut env).unwrap();

    // areaMod = 1.44 → radius = floor(20 × floor(100×√1.44)/100) = floor(20 × 1.2) = 24。
    assert!((env.player.output.aoe_area_mod - 1.44).abs() < 1e-9);
    assert_eq!(env.player.output.aoe_radius, 24.0);
}

/// 投射物接入：ProjectileCount BASE → projectile_count；无来源时保持 0。
#[test]
fn perform_fills_projectile_count_when_source_present() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    // 基础 1 发 + 额外 2 发 = 3 发。
    let mut env = player_with(
        base,
        vec![Modifier::number("ProjectileCount", ModType::Base, 3.0)],
    );
    perform(&mut env).unwrap();
    assert_eq!(env.player.output.projectile_count, 3.0);
}

/// 冷却接入：SkillCooldownBase BASE + CooldownRecovery INC → cooldown 缩短。
#[test]
fn perform_fills_cooldown_from_base_and_recovery() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(
        base,
        vec![
            Modifier::number("SkillCooldownBase", ModType::Base, 4.0),
            Modifier::number("CooldownRecovery", ModType::Inc, 100.0),
        ],
    );
    perform(&mut env).unwrap();
    // 4s / (1 + 100/100) = 2s（向上取整到服务器帧，单次储存）。
    assert!(env.player.output.cooldown > 0.0);
    assert!(env.player.output.cooldown <= 2.1);
    assert!(env.player.output.cooldown >= 1.9);
}

/// 消耗接入：SkillManaCostBase BASE + ManaCost INC → mana_cost；Spirit 保留同理。
#[test]
fn perform_fills_mana_and_spirit_cost() {
    let base = ActorBaseStats {
        life: 1000.0,
        mana: 500.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(
        base,
        vec![
            Modifier::number("SkillManaCostBase", ModType::Base, 30.0),
            Modifier::number("ManaCost", ModType::Inc, 50.0),
            Modifier::number("SkillSpiritReservationBase", ModType::Base, 60.0),
        ],
    );
    perform(&mut env).unwrap();
    // 30 × (1 + 50/100) = 45。
    assert_eq!(env.player.output.mana_cost, 45.0);
    assert!(env.player.output.spirit_reserved > 0.0);
}

// ─────────────────────────────────────────────────────────────────
// 集成阶段：触发速率（冷却驱动 / CWC）
// ─────────────────────────────────────────────────────────────────

/// 触发默认中性：无触发词条 → trigger_rate_cap / skill_trigger_rate 保持 0（向后兼容）。
#[test]
fn perform_trigger_rate_zero_without_trigger_mods() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(base, vec![]);
    perform(&mut env).unwrap();
    assert_eq!(env.player.output.trigger_rate_cap, 0.0);
    assert_eq!(env.player.output.skill_trigger_rate, 0.0);
}

/// 冷却驱动触发接入：TriggerCooldownBase BASE → trigger_rate_cap > 0；
/// skill_trigger_rate = min(cap, effective_action_rate) 双门控。
#[test]
fn perform_fills_cooldown_driven_trigger_rate() {
    let base = ActorBaseStats {
        life: 1000.0,
        // 高行动速率，使触发由 cap 而非源速率门控。
        action_rate: 20.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(
        base,
        vec![Modifier::number("TriggerCooldownBase", ModType::Base, 0.3)],
    );
    perform(&mut env).unwrap();
    // 0.3s 冷却 → cap ≈ 1/ceil_tick(0.3) ≈ 3/s。
    assert!(
        env.player.output.trigger_rate_cap > 0.0,
        "trigger cap should be positive"
    );
    assert!((env.player.output.trigger_rate_cap - 3.03).abs() < 0.2);
    // 源速率 20/s 高于 cap → skill_trigger_rate == cap（不被源门控）。
    assert!(
        (env.player.output.skill_trigger_rate - env.player.output.trigger_rate_cap).abs() < 1e-9
    );
}

/// 冷却驱动触发被源速率门控：低行动速率 → skill_trigger_rate < trigger_rate_cap。
#[test]
fn perform_trigger_rate_gated_by_source_rate() {
    let base = ActorBaseStats {
        life: 1000.0,
        // 低行动速率（1/s），低于 cap（短冷却）→ 触发被源速率门控。
        action_rate: 1.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(
        base,
        vec![Modifier::number("TriggerCooldownBase", ModType::Base, 0.05)],
    );
    perform(&mut env).unwrap();
    // cap ≈ 10+/s，源速率 1/s → skill_trigger_rate 被门控到 ≈ 1/s。
    assert!(env.player.output.skill_trigger_rate < env.player.output.trigger_rate_cap);
    assert!((env.player.output.skill_trigger_rate - 1.0).abs() < 0.2);
}

/// ICDR（CooldownRecovery）缩短触发冷却 → trigger_rate_cap 提高。
#[test]
fn perform_trigger_icdr_increases_cap() {
    let base = ActorBaseStats {
        life: 1000.0,
        action_rate: 50.0,
        ..ActorBaseStats::default()
    };
    let make = |icdr: Vec<Modifier>| {
        let mut mods = vec![Modifier::number("TriggerCooldownBase", ModType::Base, 0.5)];
        mods.extend(icdr);
        let mut env = player_with(base, mods);
        perform(&mut env).unwrap();
        env.player.output.trigger_rate_cap
    };
    let no_icdr = make(vec![]);
    // +100% CooldownRecovery → 触发冷却减半 → cap 提高。
    let with_icdr = make(vec![Modifier::number(
        "CooldownRecovery",
        ModType::Inc,
        100.0,
    )]);
    assert!(
        with_icdr > no_icdr,
        "ICDR should raise trigger cap: {with_icdr} vs {no_icdr}"
    );
}

/// CWC 触发接入：CWCTriggerTime BASE（无冷却驱动词条）→ trigger_rate_cap 由引导间隔决定。
#[test]
fn perform_fills_cwc_trigger_rate() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(
        base,
        vec![Modifier::number("CWCTriggerTime", ModType::Base, 0.3)],
    );
    perform(&mut env).unwrap();
    // triggerTime 0.3s → ceil_tick ≈ 0.33s → rate ≈ 3.03/s。
    assert!((env.player.output.trigger_rate_cap - 3.03).abs() < 0.2);
    // CWC 单被触发技能（无冷却）经单技能轮转：源速率=引导频率 → 稳态速率≈引导频率，
    // 再被 cap clamp。无冷却时 ≈ cap（finding 03-06：CWC 走 calcMultiSpellRotationImpact）。
    assert!(
        (env.player.output.skill_trigger_rate - env.player.output.trigger_rate_cap).abs() < 0.2,
        "CWC 无冷却 skill_trigger_rate≈cap: rate={} cap={}",
        env.player.output.skill_trigger_rate,
        env.player.output.trigger_rate_cap
    );
}

/// CWC 被触发技能冷却 > 引导间隔时：rate 被冷却门控，且经单技能轮转后 ≤ cap
/// （finding 03-06：CWC skill_trigger_rate 走 calc_multi_spell_rotation 单技能路径）。
#[test]
fn perform_cwc_trigger_rate_limited_by_triggered_cooldown() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    // 引导间隔 0.1s（快），被触发技能冷却 0.5s（慢）→ 冷却门控。
    let mut env = player_with(
        base,
        vec![
            Modifier::number("CWCTriggerTime", ModType::Base, 0.1),
            Modifier::number("TriggeredSkillCooldown", ModType::Base, 0.5),
        ],
    );
    perform(&mut env).unwrap();
    // 冷却 0.5s 限速 → cap = 1/ceil_tick(0.5) ≈ 1.96/s，远低于引导频率 ~9.9/s。
    assert!(
        env.player.output.trigger_rate_cap < 3.0,
        "被触发冷却应压低 cap: {}",
        env.player.output.trigger_rate_cap
    );
    // 轮转稳态速率不得超过 cap，且为正（被触发技能确实在触发）。
    assert!(env.player.output.skill_trigger_rate > 0.0);
    assert!(
        env.player.output.skill_trigger_rate <= env.player.output.trigger_rate_cap + 1e-6,
        "skill_trigger_rate {} 不得超过 cap {}",
        env.player.output.skill_trigger_rate,
        env.player.output.trigger_rate_cap
    );
}

// ─────────────────────────────────────────────────────────────────
// 集成阶段：异常维度（AilmentEffect / Faster / DotDpsCap / 跨类型施加）
// ─────────────────────────────────────────────────────────────────

fn bleed_env(extra: Vec<Modifier>) -> Env {
    let base = ActorBaseStats {
        life: 1000.0,
        hit_min: 1000.0,
        hit_max: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut mods = vec![Modifier::number("BleedChance", ModType::Base, 100.0)];
    mods.extend(extra);
    let mut env = player_with(base, mods);
    env.cfg = CalcConfig::attack().with_damage_type(DamageType::Physical);
    env
}

/// AilmentEffect（MORE）放大流血 DPS：+50% AilmentEffect → bleed_dps ×1.5。
#[test]
fn perform_ailment_effect_scales_bleed_dps() {
    let mut baseline = bleed_env(vec![]);
    perform(&mut baseline).unwrap();
    let base_dps = baseline.player.output.bleed_dps;
    assert!(base_dps > 0.0);

    let mut buffed = bleed_env(vec![Modifier::number("AilmentEffect", ModType::More, 50.0)]);
    perform(&mut buffed).unwrap();
    assert!(
        (buffed.player.output.bleed_dps - base_dps * 1.5).abs() < 1e-3,
        "AilmentEffect +50% should give 1.5× bleed: {} vs {}",
        buffed.player.output.bleed_dps,
        base_dps * 1.5
    );
}

/// BleedFaster（rateMod）放大流血 DPS：+100% BleedFaster → bleed_dps ×2。
#[test]
fn perform_ailment_faster_scales_bleed_dps() {
    let mut baseline = bleed_env(vec![]);
    perform(&mut baseline).unwrap();
    let base_dps = baseline.player.output.bleed_dps;

    let mut faster = bleed_env(vec![Modifier::number("BleedFaster", ModType::More, 100.0)]);
    perform(&mut faster).unwrap();
    assert!(
        (faster.player.output.bleed_dps - base_dps * 2.0).abs() < 1e-3,
        "BleedFaster +100% should double bleed DPS"
    );
}

/// DotDpsCap 截断：巨额流血 DPS 被全局上限 clamp（DOT_DPS_CAP）。
#[test]
fn perform_dot_dps_cap_clamps_huge_bleed() {
    // 巨额物理命中 + 大量 AilmentMagnitude/AilmentEffect → 未截断 DPS 远超上限。
    let mut env = bleed_env(vec![
        Modifier::number("AilmentMagnitude", ModType::Inc, 100000.0),
        Modifier::number("AilmentEffect", ModType::More, 100000.0),
    ]);
    // 把命中放大到天文数字。
    env.player
        .mod_db
        .add_mod(Modifier::number("PhysicalDamageMin", ModType::Base, 1e9));
    env.player
        .mod_db
        .add_mod(Modifier::number("PhysicalDamageMax", ModType::Base, 1e9));
    perform(&mut env).unwrap();
    // DotDpsCap = 35_791_394（pobr_data 常量）。
    assert!(
        env.player.output.bleed_dps <= 35_791_394.0 + 1.0,
        "bleed DPS should be capped at DotDpsCap: {}",
        env.player.output.bleed_dps
    );
    assert!(env.player.output.bleed_dps > 0.0);
}

/// 跨类型施加：FireCanBleed 旗标 → 火焰命中也计入流血来源 → 火焰 build 产生流血。
#[test]
fn perform_cross_type_fire_can_bleed() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    // 纯火焰命中，无物理 → 默认不流血。
    let make = |with_flag: bool| {
        let mut mods = vec![
            Modifier::number("FireDamageMin", ModType::Base, 5000.0),
            Modifier::number("FireDamageMax", ModType::Base, 5000.0),
            Modifier::number("BleedChance", ModType::Base, 100.0),
        ];
        if with_flag {
            mods.push(Modifier::flag("FireCanBleed"));
        }
        let mut env = player_with(base, mods);
        env.cfg = CalcConfig::attack().with_damage_type(DamageType::Fire);
        perform(&mut env).unwrap();
        env.player.output.bleed_dps
    };
    // 无旗标：火焰命中不计入流血来源 → bleed_dps == 0。
    assert_eq!(make(false), 0.0, "fire hit should not bleed by default");
    // 有 FireCanBleed：火焰命中计入流血来源 → bleed_dps > 0。
    assert!(
        make(true) > 0.0,
        "FireCanBleed should let fire hits cause bleed"
    );
}

// ─────────────────────────────────────────────────────────────────
// 集成阶段：召唤物从 MinionDef 真实底材 + 数量上限
// ─────────────────────────────────────────────────────────────────

/// add_minion_from_def 端到端：真实 MinionDef（僵尸）底材 + 数量上限注入玩家 multiplier。
#[test]
fn perform_minion_from_def_with_limit() {
    use pobr_data::minion::minion_def_zombie;

    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(base, vec![]);
    env.cfg = CalcConfig::attack().with_damage_type(DamageType::Physical);

    let def = minion_def_zombie();
    env.add_minion_from_def(&def, 20, 3, vec![], vec![], AttributeInfusion::default());
    perform(&mut env).unwrap();

    // 数量上限写入玩家 multiplier。
    let limit = env.player.mod_db.sum(
        ModType::Base,
        &env.cfg,
        &[ModName::from("Multiplier:SummonedMinion")],
    );
    assert_eq!(limit, 3.0);

    // 召唤物用真实僵尸底材：等级 = 宝石 20 → 怪物等级 40，生命来自怪物表 × 0.7 归一化。
    assert_eq!(env.player.output.minions.len(), 1);
    let m = &env.player.output.minions[0];
    assert_eq!(m.level, 40);
    assert!(m.life > 0.0, "zombie should have life from monster table");
    // 玩家自身不受召唤物影响。
    assert_eq!(env.player.output.life, 1000.0);
}

/// 数量上限 multiplier 接入 cfg：召唤物「per Summoned Minion」词条可引用数量。
#[test]
fn perform_minion_damage_per_summoned_minion_uses_limit() {
    use pobr_data::minion::minion_def_zombie;

    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };

    let make = |limit: u32| {
        let mut env = player_with(base, vec![]);
        env.cfg = CalcConfig::attack().with_damage_type(DamageType::Physical);
        let def = minion_def_zombie();
        // 通道 1：召唤物「每个召唤物 +X% 增伤」（Multiplier:SummonedMinion 引用数量）。
        let entry = MinionModifierEntry {
            inner: Modifier::number("Damage", ModType::Inc, 10.0).with_tag(
                pobr_core::ModTag::Multiplier {
                    var: "SummonedMinion".into(),
                    div: 1.0,
                    limit: None,
                },
            ),
            minion_type: None,
        };
        env.add_minion_from_def(
            &def,
            20,
            limit,
            vec![entry],
            vec![],
            AttributeInfusion::default(),
        );
        perform(&mut env).unwrap();
        env.player.output.minions[0].dps
    };

    let dps_1 = make(1);
    let dps_5 = make(5);
    // 数量越多，per-minion 增伤越高 → 召唤物 DPS 越高。
    assert!(
        dps_5 > dps_1,
        "more summoned minions should scale per-minion damage: {dps_5} vs {dps_1}"
    );
}

// ---------------------------------------------------------------------------
// 06-01：EHP/max-hit 计入 DamageTakenWhenHit 受击专属承受乘数
// （PoB2 CalcDefence.lua:2250-2263 TakenHitMult）
// ---------------------------------------------------------------------------

#[test]
fn perform_max_hit_includes_damage_taken_when_hit() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    // FireDamageTakenWhenHit INC -20 → 受击火承受乘数 = 0.8 → fire_max_hit = 1000/0.8 = 1250。
    let mut env = player_with(
        base,
        vec![Modifier::number(
            "FireDamageTakenWhenHit",
            ModType::Inc,
            -20.0,
        )],
    );
    perform(&mut env).unwrap();
    assert!(
        (env.player.output.fire_max_hit - 1250.0).abs() < 1e-6,
        "fire_max_hit = {} (期望 1250 = 1000/0.8)",
        env.player.output.fire_max_hit
    );
    // 冷不含 WhenHit → 不受影响（类型隔离）。
    assert!(
        (env.player.output.cold_max_hit - 1000.0).abs() < 1e-6,
        "cold_max_hit = {} (期望 1000)",
        env.player.output.cold_max_hit
    );

    // 叠加：DamageTaken MORE -10 + DamageTakenWhenHit INC -20 → 0.8*0.9=0.72。
    let mut env2 = player_with(
        base,
        vec![
            Modifier::number("DamageTaken", ModType::More, -10.0),
            Modifier::number("DamageTakenWhenHit", ModType::Inc, -20.0),
        ],
    );
    perform(&mut env2).unwrap();
    let expected = 1000.0 / 0.72;
    assert!(
        (env2.player.output.physical_max_hit - expected).abs() < 1e-3,
        "physical_max_hit = {} (期望 {} = 1000/0.72)",
        env2.player.output.physical_max_hit,
        expected
    );
}

// ---------------------------------------------------------------------------
// 03-02：触发源速率取注入的 TriggerSourceRate（PoB2 EffectiveSourceRate = 触发源技能速率）
// ---------------------------------------------------------------------------

#[test]
fn perform_trigger_uses_injected_source_rate_not_main_skill_rate() {
    let base = ActorBaseStats {
        life: 1000.0,
        action_rate: 50.0, // 主技能高速；若误用主技能速率当源速率则门控失效
        ..ActorBaseStats::default()
    };
    let mut env = player_with(
        base,
        vec![
            Modifier::number("TriggerCooldownBase", ModType::Base, 0.05), // 短冷却 → cap 高
            Modifier::number("TriggerSourceRate", ModType::Base, 1.0),    // 注入源速率 1/s
        ],
    );
    perform(&mut env).unwrap();
    assert!(
        env.player.output.trigger_rate_cap > 5.0,
        "0.05s 冷却 → cap 应较高，got {}",
        env.player.output.trigger_rate_cap
    );
    assert!(
        env.player.output.skill_trigger_rate < env.player.output.trigger_rate_cap,
        "低注入源速率须把触发速率门控到 cap 以下"
    );
    assert!(
        (env.player.output.skill_trigger_rate - 1.0).abs() < 0.2,
        "skill_trigger_rate 应跟随注入源速率 1/s，got {}",
        env.player.output.skill_trigger_rate
    );
}

// ---------------------------------------------------------------------------
// 03-01：触发速率末端乘 triggerChance（显式触发几率折算，PoB2 CalcTriggers L715-777）
// ---------------------------------------------------------------------------

#[test]
fn perform_trigger_rate_folds_explicit_trigger_chance() {
    let base = ActorBaseStats {
        life: 1000.0,
        action_rate: 20.0, // 源速率 > cap → 被 cap 门控
        ..ActorBaseStats::default()
    };
    // 基准：无显式触发几率 → 不折算。
    let mut env_base = player_with(
        base,
        vec![Modifier::number("TriggerCooldownBase", ModType::Base, 0.3)],
    );
    perform(&mut env_base).unwrap();
    let cap = env_base.player.output.trigger_rate_cap;
    assert!(
        (env_base.player.output.skill_trigger_rate - cap).abs() < 1e-9,
        "无触发上下文不应折算：skill_trigger_rate={} cap={}",
        env_base.player.output.skill_trigger_rate,
        cap
    );

    // 注入显式触发几率 50%（build 层经 cfg.multipliers["TriggerChance"] 注入，百分数）。
    let mut env_half = player_with(
        base,
        vec![Modifier::number("TriggerCooldownBase", ModType::Base, 0.3)],
    );
    env_half
        .cfg
        .multipliers
        .insert("TriggerChance".to_string(), 50.0);
    perform(&mut env_half).unwrap();
    assert!(
        (env_half.player.output.trigger_rate_cap - cap).abs() < 1e-9,
        "triggerChance 不改 cap"
    );
    assert!(
        (env_half.player.output.skill_trigger_rate - cap * 0.5).abs() < 1e-6,
        "50% 触发几率应使触发速率减半：got {} expect {}",
        env_half.player.output.skill_trigger_rate,
        cap * 0.5
    );
}
