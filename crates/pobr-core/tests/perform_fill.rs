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

#[test]
fn perform_fills_bleed_dps_from_physical_hits() {
    let base = ActorBaseStats {
        life: 1000.0,
        hit_min: 1000.0,
        hit_max: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(base, vec![]);
    env.cfg = CalcConfig::attack().with_damage_type(DamageType::Physical);
    perform(&mut env).unwrap();

    assert!(env.player.output.bleed_dps > 0.0);
}

#[test]
fn perform_fills_block_and_suppression_chances() {
    let base = ActorBaseStats {
        life: 1000.0,
        ..ActorBaseStats::default()
    };
    let mut env = player_with(
        base,
        vec![
            Modifier::number("BlockChance", ModType::Base, 90.0),
            Modifier::number("SpellSuppressionChance", ModType::Base, 50.0),
        ],
    );
    perform(&mut env).unwrap();

    // block capped at 75.
    assert_eq!(env.player.output.block_chance, 75.0);
    assert_eq!(env.player.output.spell_suppression_chance, 50.0);
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
