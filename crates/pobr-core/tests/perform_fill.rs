use pobr_core::calc::actor::{Actor, ActorBaseStats};
use pobr_core::calc::env::Env;
use pobr_core::calc::perform::perform;
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
