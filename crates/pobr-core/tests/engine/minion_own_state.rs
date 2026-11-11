//! A minion's buffs, charges and Rage are its own: it holds what its own
//! modifiers state, with the character's rules for each, and nothing because
//! the character holds it.

use pobr_core::calc::{Actor, ActorBaseStats, Env, perform};
use pobr_core::{CalcConfig, ModTag, Modifier};
use pobr_data::catalog::buffs::BuffDefinitionsDef;
use pobr_data::prelude::*;

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
}

/// An attacking character in combat with one attacking minion, both hitting
/// for 100 once a second.
fn environment() -> Env {
    let base = ActorBaseStats {
        life: 1000.0,
        mana: 100.0,
        hit_min: 100.0,
        hit_max: 100.0,
        action_rate: 1.0,
        ..Default::default()
    };
    let mut env = Env::new(Actor::new(90, base)).with_config(
        CalcConfig::attack()
            .with_mode_combat(true)
            .with_mode_buffs(true),
    );
    let defs: BuffDefinitionsDef = serde_json::from_str(
        &std::fs::read_to_string(
            crate::support::test_data_dir().join("../overlay-common/buff_definitions.json"),
        )
        .unwrap(),
    )
    .unwrap();
    env.buff_definitions = defs.buffs;
    env.minions.push(Actor::new(36, base));
    env
}

/// What the minion configuration gives a minion: the modifier as the option
/// states it, in combat.
fn configured(modifier: Modifier) -> Modifier {
    modifier.with_tag(ModTag::condition("Combat", false))
}

/// "Minion Rage: 20" for a minion that can gain Rage: 20% more Attack
/// damage. The character's own Rage, maximum Rage and stated Rage do not
/// count for it.
#[test]
fn a_minion_that_can_gain_rage_holds_its_own_stated_rage() {
    let mut env = environment();
    env.player
        .mod_db
        .add_mod(Modifier::flag("Condition:CanGainRage"));
    env.cfg.multipliers.insert("RageStack".into(), 30.0);
    env.minions[0].mod_db.add_list([
        Modifier::flag("Condition:CanGainRage"),
        configured(Modifier::number(
            "Multiplier:RageStack",
            ModType::Base,
            20.0,
        ))
        .with_tag(ModTag::condition("CanGainRage", false)),
        // Counts only while the minion is not at its own maximum Rage.
        Modifier::number("Damage", ModType::Inc, 50.0)
            .with_tag(ModTag::condition("HaveMaximumRage", true)),
    ]);
    perform(&mut env).unwrap();
    close(env.player.output.dps, 130.0);
    close(env.minions[0].output.dps, 100.0 * 1.2 * 1.5);
}

#[test]
fn a_minion_that_cannot_gain_rage_has_none_of_the_characters() {
    let mut env = environment();
    env.player
        .mod_db
        .add_mod(Modifier::flag("Condition:CanGainRage"));
    env.cfg.multipliers.insert("RageStack".into(), 30.0);
    env.cfg.conditions.insert("CanGainRage".into(), true);
    perform(&mut env).unwrap();
    close(env.player.output.dps, 130.0);
    close(env.minions[0].output.dps, 100.0);
}

/// A minion holds its own minimum charges; the character's charges, and its
/// use of them, are the character's.
#[test]
fn a_minion_counts_its_own_charges_not_the_characters() {
    let mut env = environment();
    env.cfg.conditions.insert("UsePowerCharges".into(), true);
    env.cfg.multipliers.insert("PowerCharge".into(), 3.0);
    env.minions[0].mod_db.add_list([
        Modifier::number("PowerChargesMin", ModType::Base, 2.0),
        Modifier::number("Damage", ModType::Inc, 10.0).with_tag(ModTag::multiplier(
            "PowerCharge",
            1.0,
            None,
        )),
    ]);
    perform(&mut env).unwrap();
    close(env.minions[0].output.dps, 120.0);
}

/// The configured "Do your minions have Unholy Might?" and an ally grant of
/// it are one buff: 30% of damage as extra Chaos, once.
#[test]
fn configured_and_granted_unholy_might_count_once_on_the_minion() {
    let mut env = environment();
    env.player.mod_db.add_mod(Modifier::new(
        "ExtraAura",
        ModType::List,
        pobr_core::ModValue::NestedMods(vec![Modifier::flag("Condition:UnholyMight")]),
    ));
    env.minions[0]
        .mod_db
        .add_mod(configured(Modifier::flag("Condition:UnholyMight")));
    perform(&mut env).unwrap();
    close(env.minions[0].output.dps, 130.0);
    assert_eq!(
        env.minions[0]
            .mod_db
            .named("DamageGainAsChaos")
            .filter(|m| m
                .origin
                .as_ref()
                .is_some_and(|o| o.source_id.id == "buff.UnholyMight"))
            .count(),
        1
    );
}

/// The character's Onslaught and Full Life are not its minion's.
#[test]
fn the_characters_buff_and_conditions_do_not_reach_the_minion() {
    let mut env = environment();
    env.player
        .mod_db
        .add_mod(configured(Modifier::flag("Condition:Onslaught")));
    env.cfg.conditions.insert("Onslaught".into(), true);
    env.cfg.conditions.insert("FullLife".into(), true);
    env.minions[0].mod_db.add_mod(
        Modifier::number("Damage", ModType::Inc, 50.0)
            .with_tag(ModTag::condition("FullLife", false)),
    );
    perform(&mut env).unwrap();
    close(env.minions[0].output.action_rate, 1.0);
    close(env.minions[0].output.dps, 100.0);

    let mut own = environment();
    own.minions[0].mod_db.add_list([
        configured(Modifier::flag("Condition:Onslaught")),
        Modifier::flag("Condition:FullLife"),
        Modifier::number("Damage", ModType::Inc, 50.0)
            .with_tag(ModTag::condition("FullLife", false)),
    ]);
    perform(&mut own).unwrap();
    close(own.minions[0].output.action_rate, 1.2);
    close(own.minions[0].output.dps, 100.0 * 1.2 * 1.5);
}

/// "Minions deal 10% increased Damage per Power Charge you have" counts the
/// character's charges, while the minion's own count stays its own.
#[test]
fn a_minion_line_per_the_characters_charge_counts_the_characters() {
    let mut env = environment();
    env.cfg.conditions.insert("UsePowerCharges".into(), true);
    env.cfg.multipliers.insert("PowerCharge".into(), 3.0);
    env.minions[0].mod_db.add_list([
        Modifier::number("Damage", ModType::Inc, 10.0).with_tag(ModTag::Multiplier {
            var: "PowerCharge".into(),
            div: 1.0,
            limit: None,
            actor: Some(pobr_core::ActorRef::Player),
            limit_var: None,
            limit_actor: None,
            invert: false,
            limit_total: false,
        }),
        Modifier::number("Damage", ModType::Inc, 100.0).with_tag(ModTag::multiplier(
            "PowerCharge",
            1.0,
            None,
        )),
    ]);
    perform(&mut env).unwrap();
    close(env.minions[0].output.dps, 130.0);
}
