use pobr_core::calc::{Actor, ActorBaseStats, Env, perform};
use pobr_core::{CalcConfig, ModTag, Modifier};
use pobr_data::prelude::*;

fn environment(extra: &[&str]) -> Env {
    let mut env = Env::new(Actor::new(
        80,
        ActorBaseStats {
            life: 1000.0,
            hit_min: 100.0,
            hit_max: 100.0,
            action_rate: 2.0,
            ..Default::default()
        },
    ));
    env.cfg = CalcConfig::spell().with_mode_combat(true);
    for text in extra {
        env.player
            .mod_db
            .add_list(crate::support::parse_mod(text).unwrap().mods);
    }
    env
}

#[test]
fn shock_effect_scales_only_shock_damage_taken_not_other_damage_or_duration() {
    // shocked_effect_on_self_+%: Stormwalker gives 40% reduced effect.
    // A supplied 20% Shock becomes 12%; a separate 10% damage taken stays 10%.
    for (reduction, taken) in [("40%", 1.22), ("150%", 1.1)] {
        let mut env = environment(&[&format!("{reduction} reduced effect of Shock on you")]);
        env.cfg = env.cfg.with_condition("Shocked", true);
        env.player.mod_db.add_list([
            Modifier::number("DamageTaken", ModType::Inc, 20.0)
                .with_source("Shock")
                .with_tag(ModTag::condition("Shocked", false)),
            Modifier::number("DamageTaken", ModType::Inc, 10.0).with_source("Other"),
            Modifier::number("PhysicalDegen", ModType::Base, 100.0),
        ]);
        perform(&mut env).unwrap();
        assert!((env.player.output.taken_multi_physical - taken).abs() < 1e-9);
        assert!((env.player.output.total_build_degen - 100.0 * taken).abs() < 1e-9);
    }
}

#[test]
fn explicit_chill_magnitude_scales_action_and_movement_with_slow_potency() {
    // chill_effectiveness_on_self_+%: 40% reduced Chill effect.
    // Configured 50% Chill -> 30% slow, then Mail Coat's 30% reduced
    // slowing potency -> 21% slow: 2 * .79 = 1.58 actions/s.
    let mut env = environment(&[
        "40% reduced Effect of Chill on you",
        "30% reduced Slowing Potency of Debuffs on You",
    ]);
    env.cfg = env.cfg.with_condition("Chilled", true);
    env.player.mod_db.add_mod(
        Modifier::number("ChillVal", ModType::Override, 50.0)
            .with_tag(ModTag::condition("Chilled", false)),
    );
    perform(&mut env).unwrap();
    assert!((env.player.output.action_rate - 1.58).abs() < 1e-9);
    assert_eq!(env.player.output.effective_movement_speed_mod, 0.79);
}
