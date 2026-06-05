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
fn perform_fills_block_and_suppression_chances() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(
        base,
        vec![
            // 95% block → capped at PoE2 limit 90%
            Modifier::number("BlockChance", ModType::Base, 95.0),
            Modifier::number("SpellSuppressionChance", ModType::Base, 50.0),
        ],
    );
    perform(&mut env).unwrap();

    // PoE2: block capped at 90 (not PoE1's 75).
    assert_eq!(env.player.output.block_chance, 90.0);
    // 法术压制 PoE2 已移除，但函数保留兼容性（inert）
    assert_eq!(env.player.output.spell_suppression_chance, 50.0);
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
