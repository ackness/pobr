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

#[test]
fn self_debuff_views_report_modifiers_without_changing_other_outputs() {
    use pobr_core::{calc::MinimalInput, display_value};

    let mut session = crate::support::session(MinimalInput::default());
    session
        .add_modifier_texts(&[
            "30% reduced effect of Curses on you",
            "50% reduced Duration of Bleeding on You",
            "20% reduced Duration of Ailments on You",
            "10% reduced Elemental Ailment Duration on you",
            "20% increased Poison Duration on you",
            "Immune to Corrupted Blood",
            "Immune to Maim",
            "Immune to Hinder",
        ])
        .unwrap();
    session.add_modifiers([
        Modifier::number("SelfIgniteDuration", ModType::More, -50.0),
        Modifier::number("PhysicalDegen", ModType::Base, 100.0),
    ]);
    session.perform_minimal().unwrap();
    // Game self_bleed_duration_+% and base_all_ailment_duration_on_self_+%:
    // 100 - 50 - 20 = 30%. Elemental durations are 100 - 20 - 10 = 70%.
    // Poison adds its own 20%; Ignite additionally has a 0.5 MORE factor.
    for (id, expected) in [
        ("CurseEffectOnSelf", 70.0),
        ("SelfBleedDuration", 30.0),
        ("SelfPoisonDuration", 100.0),
        ("SelfIgniteDuration", 35.0),
        ("SelfChillDuration", 70.0),
        ("SelfFreezeDuration", 70.0),
        ("SelfShockDuration", 70.0),
        ("CorruptedBloodImmunity", 1.0),
        ("MaimImmunity", 1.0),
        ("HinderImmunity", 1.0),
    ] {
        assert_eq!(display_value(session.output(), id), expected, "{id}");
        let sources = session.detail_sources(id);
        assert_eq!(sources.len(), 1, "{id}");
        assert!(!sources[0].rows.is_empty(), "{id}");
        if !id.ends_with("Immunity") {
            let (_, inc, more) = sources[0].totals;
            assert_eq!(((100.0 + inc) * more).max(0.0), expected, "{id}");
        }
    }
    assert_eq!(session.output().total_build_degen, 100.0);

    let mut neutral = environment(&[]);
    perform(&mut neutral).unwrap();
    let mut reduced = environment(&[
        "150% reduced effect of Curses on you",
        "100% reduced Duration of Ailments on You",
        "Immune to Corrupted Blood",
        "Immune to Maim",
        "Immune to Hinder",
    ]);
    perform(&mut reduced).unwrap();
    for id in [
        "CurseEffectOnSelf",
        "SelfBleedDuration",
        "SelfPoisonDuration",
        "SelfIgniteDuration",
        "SelfChillDuration",
        "SelfFreezeDuration",
        "SelfShockDuration",
    ] {
        assert_eq!(display_value(&neutral.player.output, id), 100.0, "{id}");
        assert_eq!(display_value(&reduced.player.output, id), 0.0, "{id}");
    }
    for id in ["CorruptedBloodImmunity", "MaimImmunity", "HinderImmunity"] {
        assert_eq!(display_value(&neutral.player.output, id), 0.0);
    }
    assert_eq!(reduced.player.output.dps, neutral.player.output.dps);
    assert_eq!(
        reduced.player.output.action_rate,
        neutral.player.output.action_rate
    );
    assert_eq!(
        reduced.player.output.effective_movement_speed_mod,
        neutral.player.output.effective_movement_speed_mod
    );
}
