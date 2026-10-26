use pobr_core::calc::{Actor, ActorBaseStats, CalculationSession, Env, MinimalInput, perform};
use pobr_core::{CalcConfig, ModTag, Modifier};
use pobr_data::prelude::*;

const XOPH: &str = "Enemies in your Presence have -25% to Fire Resistance";

fn add_line(env: &mut Env, text: &str) {
    let parsed = crate::support::parse_mod(text).unwrap();
    assert!(parsed.is_complete(), "{text}: {parsed:?}");
    env.player.mod_db.add_list(parsed.mods.into_iter().map(|m| {
        m.with_origin(
            ModifierSource::new(SourceId::new(SourceKind::Item, "xoph")).with_raw_text(text),
        )
    }));
}

fn environment(present: bool) -> Env {
    let mut env = Env::new(Actor::new(
        90,
        ActorBaseStats {
            life: 1000.0,
            action_rate: 1.0,
            ..Default::default()
        },
    ))
    .with_config(
        CalcConfig::spell()
            .with_mode_effective(true)
            .with_condition("EnemyInPresence", present),
    );
    add_line(&mut env, XOPH);
    add_line(&mut env, "Adds 100 to 100 Fire Damage");
    env.player
        .mod_db
        .add_mod(Modifier::number("FireDot", ModType::Base, 100.0));
    env.enemy.mod_db.add_list([
        Modifier::number("ElementalResist", ModType::Base, 50.0),
        Modifier::number("FireExposure", ModType::Base, 12.0),
    ]);
    let mut minion = Actor::new(
        36,
        ActorBaseStats {
            hit_min: 100.0,
            hit_max: 100.0,
            action_rate: 1.0,
            ..Default::default()
        },
    );
    minion.mod_db.add_mod(Modifier::number(
        "PhysicalDamageConvertToFire",
        ModType::Base,
        100.0,
    ));
    env.minions.push(minion);
    env
}

fn snapshot(env: &Env) -> CalculationSession {
    let mut session =
        CalculationSession::new(MinimalInput::from(env.player.base)).with_config(env.cfg.clone());
    session.add_enemy_modifiers(env.enemy.mod_db.iter_mods().cloned());
    session
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
}

#[test]
fn xoph_presence_resistance_combines_with_exposure_for_hits_dot_and_minions() {
    // Game stat enemies_in_presence_have_fire_resistance_% is a resistance
    // change, independent of Exposure: 50 - 12 - 25 = 13, or 38 out of Presence.
    for present in [false, true] {
        let mut env = environment(present);
        perform(&mut env).unwrap();
        let expected = if present { 87.0 } else { 62.0 };
        close(env.player.output.dps, expected);
        close(env.player.output.skill_total_dot, expected);
        close(env.minions[0].output.dps, expected);
        let session = snapshot(&env);
        let rows = session.hit_mitigation_mods(0, &[DamageType::Fire]);
        let lines = &rows[0].1.defence;
        close(
            lines
                .iter()
                .filter(|l| l.active)
                .filter_map(|l| l.value)
                .sum(),
            100.0 - expected,
        );
        assert_eq!(
            lines.iter().any(|l| l.active
                && l.modifier
                    .origin
                    .as_ref()
                    .is_some_and(|o| o.raw_text.as_deref() == Some(XOPH))),
            present
        );
    }
}

#[test]
fn resistance_alias_overrides_and_source_rows_agree() {
    for name in ["FireResistance", "FireResist"] {
        let mut env = environment(true);
        env.enemy.mod_db.add_list([
            Modifier::number(name, ModType::Override, 25.0),
            Modifier::number(name, ModType::Override, 0.0),
            Modifier::number(name, ModType::Override, 99.0)
                .with_tag(ModTag::condition("InactiveOverride", false)),
        ]);
        perform(&mut env).unwrap();
        close(env.player.output.dps, 100.0);
        close(env.player.output.skill_total_dot, 100.0);
        close(env.minions[0].output.dps, 100.0);
        let session = snapshot(&env);
        let rows = session.hit_mitigation_mods(0, &[DamageType::Fire]);
        let active: Vec<_> = rows[0].1.defence.iter().filter(|l| l.active).collect();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].modifier.name.as_str(), name);
        assert_eq!(active[0].value, Some(0.0));
    }
}

#[test]
fn sacred_flame_uses_the_alias_when_selecting_lowest_resistance() {
    let mut env = environment(true);
    add_line(
        &mut env,
        "Enemies in your Presence Resist Elemental Damage based on their Lowest Resistance",
    );
    add_line(&mut env, "Adds 100 to 100 Cold Damage");
    env.player
        .mod_db
        .add_mod(Modifier::number("ColdDot", ModType::Base, 100.0));
    perform(&mut env).unwrap();
    close(env.player.output.dps, 174.0);
    close(env.player.output.skill_total_dot, 174.0);
    let session = snapshot(&env);
    let rows = session.hit_mitigation_mods(0, &[DamageType::Cold]);
    assert!(rows[0].1.defence.iter().any(|l| l.active
        && l.modifier.name.as_str() == "FireResistance"
        && l.value == Some(-25.0)));
}
