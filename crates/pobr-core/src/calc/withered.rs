//! Withered: a debuff that makes its target take more Chaos damage.
//!
//! The rules, as the game defines the debuff:
//!
//! - Each stack makes the target take 5% increased Chaos Damage, and a target
//!   holds at most 10 stacks ([`WITHERED_CHAOS_DAMAGE_TAKEN_PER_STACK`],
//!   [`WITHERED_MAXIMUM_STACKS`]).
//! - "Withered Magnitude" (`withered_magnitude_+%`) scales what a stack does.
//!   A stack's value is a whole number, like every stat value.
//! - How many stacks the enemy holds is the player's statement about combat
//!   (the `multiplierWitheredStackCount` config input). It counts only for a
//!   character that can inflict Withered: with one of its own skills, with
//!   its Presence, or with a minion it has. "Minions have 10% chance to
//!   inflict Withered on Hit" does nothing for a character without minions.
//!
//! The stage writes the enemy's increased Chaos Damage taken, which hits and
//! damage over time read like any other damage-taken modifier.

use pobr_data::constants::{WITHERED_CHAOS_DAMAGE_TAKEN_PER_STACK, WITHERED_MAXIMUM_STACKS};
use pobr_data::prelude::*;

use crate::Modifier;

use super::Env;

/// The flag of a character, a skill or a minion that can inflict Withered.
pub const CAN_WITHER: &str = "Condition:CanWither";

/// Source of the injected modifier; also the guard against applying it twice.
const WITHERED_SOURCE: &str = "Withered";

/// Applies the Withered the enemy is stated to hold. Does nothing outside the
/// effective view, or when nothing of the character's can inflict Withered.
pub fn apply_withered(env: &mut Env) {
    let cfg = &env.cfg;
    if !cfg.mode_effective {
        return;
    }
    let player = &env.player.mod_db;
    let can_wither = player.flag(cfg, CAN_WITHER)
        || env
            .minions
            .iter()
            .any(|minion| minion.mod_db.flag(cfg, CAN_WITHER));
    if !can_wither
        || env
            .enemy
            .mod_db
            .named("ChaosDamageTaken")
            .any(|modifier| modifier.source.as_deref() == Some(WITHERED_SOURCE))
    {
        return;
    }

    let stated = env.enemy.mod_db.sum(
        ModType::Base,
        cfg,
        &[ModName::from("Multiplier:WitheredStack")],
    );
    let stacks = stated.clamp(0.0, WITHERED_MAXIMUM_STACKS);
    let magnitude = [ModName::from("WitherEffect")];
    let per_stack = (WITHERED_CHAOS_DAMAGE_TAKEN_PER_STACK
        * (1.0 + player.sum(ModType::Inc, cfg, &magnitude) / 100.0)
        * player.more(cfg, &magnitude))
    .floor();
    if stacks * per_stack == 0.0 {
        return;
    }
    let origin = ModifierSource::new(SourceId::new(SourceKind::Buff, "debuff.Withered"))
        .with_raw_text(format!("{stacks} Withered"));
    env.enemy.mod_db.add_mod(
        Modifier::number("ChaosDamageTaken", ModType::Inc, stacks * per_stack)
            .with_source(WITHERED_SOURCE)
            .with_origin(origin),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ModTag;
    use crate::calc::{Actor, ActorBaseStats};

    /// An enemy stated to hold `stated` Withered, facing a character with
    /// `player` modifiers, in the effective view unless `effective` is false.
    fn run(stated: f64, player: Vec<Modifier>, effective: bool) -> Env {
        let mut actor = Actor::new(1, ActorBaseStats::default());
        actor.mod_db.add_list(player);
        let mut env = Env::new(actor);
        // The config input's own shape: it counts in the effective view.
        env.enemy.mod_db.add_mod(
            Modifier::number("Multiplier:WitheredStack", ModType::Base, stated)
                .with_tag(ModTag::condition("Effective", false)),
        );
        env.cfg = env.cfg.with_mode_effective(effective);
        apply_withered(&mut env);
        env
    }

    fn can_wither() -> Modifier {
        Modifier::flag(CAN_WITHER)
    }

    /// The enemy's increased Chaos Damage taken, in percent.
    fn chaos_taken(env: &Env) -> f64 {
        let cfg = env.cfg.clone().with_damage_type(DamageType::Chaos);
        env.enemy
            .mod_db
            .sum(ModType::Inc, &cfg, &[ModName::from("ChaosDamageTaken")])
    }

    #[test]
    fn each_stack_is_five_percent_increased_chaos_damage_taken() {
        assert_eq!(chaos_taken(&run(1.0, vec![can_wither()], true)), 5.0);
        assert_eq!(chaos_taken(&run(7.0, vec![can_wither()], true)), 35.0);
    }

    #[test]
    fn a_target_holds_at_most_ten_stacks() {
        assert_eq!(chaos_taken(&run(10.0, vec![can_wither()], true)), 50.0);
        assert_eq!(chaos_taken(&run(15.0, vec![can_wither()], true)), 50.0);
    }

    #[test]
    fn a_character_that_cannot_inflict_withered_gains_nothing_from_stated_stacks() {
        assert_eq!(chaos_taken(&run(10.0, vec![], true)), 0.0);
    }

    #[test]
    fn stacks_are_a_statement_about_the_effective_view() {
        assert_eq!(chaos_taken(&run(10.0, vec![can_wither()], false)), 0.0);
    }

    /// 5 x 1.2 = 6 per stack; 5 x 1.1 = 5.5 is still 5, a whole number.
    #[test]
    fn magnitude_scales_a_stack_to_a_whole_number() {
        let magnitude = |inc: f64| Modifier::number("WitherEffect", ModType::Inc, inc);
        let with = |inc: f64| chaos_taken(&run(10.0, vec![can_wither(), magnitude(inc)], true));
        assert_eq!(with(20.0), 60.0);
        assert_eq!(with(10.0), 50.0);
        assert_eq!(with(40.0), 70.0);
    }

    #[test]
    fn a_minion_that_can_inflict_withered_is_enough() {
        let mut actor = Actor::new(1, ActorBaseStats::default());
        actor.mod_db.add_list(Vec::<Modifier>::new());
        let mut env = Env::new(actor);
        env.enemy.mod_db.add_mod(
            Modifier::number("Multiplier:WitheredStack", ModType::Base, 4.0)
                .with_tag(ModTag::condition("Effective", false)),
        );
        let mut minion = Actor::new(1, ActorBaseStats::default());
        minion.mod_db.add_mod(can_wither());
        env.minions.push(minion);
        env.cfg = env.cfg.with_mode_effective(true);
        apply_withered(&mut env);
        assert_eq!(chaos_taken(&env), 20.0);
    }

    #[test]
    fn applying_twice_changes_nothing() {
        let mut env = run(10.0, vec![can_wither()], true);
        apply_withered(&mut env);
        assert_eq!(chaos_taken(&env), 50.0);
    }
}
