//! Blind: a debuff on the Accuracy Rating and Evasion Rating of its target.
//!
//! The rules, as the game defines the debuff:
//!
//! - "Being Blinded causes 20% less Accuracy Rating and Evasion Rating"
//!   ([`BLIND_LESS_ACCURACY_AND_EVASION`]).
//! - "Blind Effect" scales that number for the one it is on. The character's
//!   "increased Blind Effect" is a modifier to the enemy, so each side's
//!   database holds the effect of the Blind on itself. The scaled number is
//!   whole, like every stat value.
//! - "The Effect of Blind on you is reversed" turns the penalty into a bonus
//!   of the same size.
//!
//! Either side can be Blinded, and the build states which: the enemy for the
//! effective view (`conditionEnemyBlinded`), the character in combat
//! (`conditionBlinded`). A Blinded enemy is hit more often, and its own hits
//! are evaded and deflected more often.

use pobr_data::constants::BLIND_LESS_ACCURACY_AND_EVASION;
use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb, Modifier};

use super::Env;

/// The flag of an actor that is Blinded.
pub const BLINDED: &str = "Condition:Blinded";

/// Source of the injected modifiers; also the guard against applying them twice.
const BLIND_SOURCE: &str = "Blind";

/// The two ratings Blind lowers.
const BLINDED_RATINGS: [&str; 2] = ["Accuracy", "Evasion"];

/// Applies Blind to the character and to the enemy, whichever is Blinded.
pub fn apply_blind(env: &mut Env) {
    for (actor, id) in [
        (&mut env.player, "debuff.Blind.player"),
        (&mut env.enemy, "debuff.Blind.enemy"),
    ] {
        let Some(percent) = blind_percent(&actor.mod_db, &env.cfg) else {
            continue;
        };
        let origin = ModifierSource::new(SourceId::new(SourceKind::Buff, id)).with_raw_text(
            format!("Blinded: {percent}% Accuracy Rating and Evasion Rating"),
        );
        for rating in BLINDED_RATINGS {
            actor.mod_db.add_mod(
                Modifier::number(rating, ModType::More, percent)
                    .with_source(BLIND_SOURCE)
                    .with_origin(origin.clone()),
            );
        }
    }
}

/// The more (negative: less) Accuracy Rating and Evasion Rating Blind gives
/// the actor holding `db`, in percent. `None` when it is not Blinded, cannot
/// be, or already carries the modifiers.
fn blind_percent(db: &ModDb, cfg: &CalcConfig) -> Option<f64> {
    if !db.flag(cfg, BLINDED)
        || db.flag(cfg, "BlindImmune")
        || db
            .named("Evasion")
            .any(|modifier| modifier.source.as_deref() == Some(BLIND_SOURCE))
    {
        return None;
    }
    let effect = [ModName::from("BlindEffect")];
    let maximum = (1.0 + db.sum(ModType::Inc, cfg, &effect) / 100.0) * db.more(cfg, &effect);
    let scale = db.override_(cfg, "BlindEffect").map_or(maximum, |value| {
        (value / 100.0).clamp(0.0, maximum.max(0.0))
    });
    let magnitude = (BLIND_LESS_ACCURACY_AND_EVASION * scale)
        .floor()
        .clamp(0.0, 100.0);
    Some(if db.flag(cfg, "BlindEffectReversed") {
        magnitude
    } else {
        -magnitude
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ModTag;
    use crate::calc::setup_env::{enemy_accuracy, enemy_evasion};
    use crate::calc::{Actor, ActorBaseStats};

    /// An environment whose enemy has 1000 Accuracy Rating and 2000 Evasion
    /// Rating, with `enemy` modifiers on the enemy and `player` on the
    /// character.
    fn run(player: Vec<Modifier>, enemy: Vec<Modifier>, effective: bool) -> Env {
        let mut env = Env::new(Actor::new(1, ActorBaseStats::default()));
        env.player.mod_db.add_list(player);
        env.enemy.base.accuracy = 1000.0;
        env.enemy.base.evasion = 2000.0;
        env.enemy.mod_db.add_list(enemy);
        env.cfg = env.cfg.with_mode_effective(effective);
        apply_blind(&mut env);
        env
    }

    /// What the `conditionEnemyBlinded` config input writes.
    fn enemy_blinded() -> Modifier {
        Modifier::flag(BLINDED).with_tag(ModTag::condition("Effective", false))
    }

    fn player_more(env: &Env, rating: &str) -> f64 {
        env.player.mod_db.more(&env.cfg, &[ModName::from(rating)])
    }

    #[test]
    fn a_blinded_enemy_has_a_fifth_less_accuracy_and_evasion() {
        let env = run(vec![], vec![enemy_blinded()], true);
        assert_eq!(enemy_accuracy(&env.enemy, &env.cfg), 800.0);
        assert_eq!(enemy_evasion(&env.enemy, &env.cfg), 1600.0);
    }

    #[test]
    fn an_enemy_that_is_not_blinded_keeps_its_ratings() {
        let env = run(vec![], vec![], true);
        assert_eq!(enemy_accuracy(&env.enemy, &env.cfg), 1000.0);
        assert_eq!(enemy_evasion(&env.enemy, &env.cfg), 2000.0);
    }

    #[test]
    fn the_enemy_is_blinded_only_in_the_effective_view() {
        let env = run(vec![], vec![enemy_blinded()], false);
        assert_eq!(enemy_accuracy(&env.enemy, &env.cfg), 1000.0);
        assert_eq!(enemy_evasion(&env.enemy, &env.cfg), 2000.0);
    }

    /// 20 x 1.3 = 26; 20 x 1.17 = 23.4 is 23, a whole number.
    #[test]
    fn blind_effect_scales_the_penalty_to_a_whole_number() {
        let effect = |inc: f64| Modifier::number("BlindEffect", ModType::Inc, inc);
        let evasion = |inc: f64| {
            let env = run(vec![], vec![enemy_blinded(), effect(inc)], true);
            enemy_evasion(&env.enemy, &env.cfg)
        };
        assert_eq!(evasion(30.0), 2000.0 * 0.74);
        assert_eq!(evasion(17.0), 2000.0 * 0.77);
    }

    #[test]
    fn a_blinded_character_has_the_same_penalty_on_its_own_ratings() {
        let env = run(vec![Modifier::flag(BLINDED)], vec![], false);
        assert_eq!(player_more(&env, "Accuracy"), 0.8);
        assert_eq!(player_more(&env, "Evasion"), 0.8);
        // The enemy is untouched.
        assert_eq!(enemy_evasion(&env.enemy, &env.cfg), 2000.0);
    }

    #[test]
    fn a_character_that_cannot_be_blinded_is_not() {
        let env = run(
            vec![Modifier::flag(BLINDED), Modifier::flag("BlindImmune")],
            vec![],
            false,
        );
        assert_eq!(player_more(&env, "Evasion"), 1.0);
    }

    #[test]
    fn reversed_blind_is_a_bonus_of_the_same_size() {
        let env = run(
            vec![
                Modifier::flag(BLINDED),
                Modifier::flag("BlindEffectReversed"),
            ],
            vec![],
            false,
        );
        assert_eq!(player_more(&env, "Accuracy"), 1.2);
        assert_eq!(player_more(&env, "Evasion"), 1.2);
    }

    #[test]
    fn overridden_effect_is_a_percentage_and_cannot_exceed_the_available_blind() {
        for (effect, accuracy, evasion) in [
            (50.0, 900.0, 1800.0),
            (100.0, 800.0, 1600.0),
            (200.0, 800.0, 1600.0),
        ] {
            let env = run(
                vec![],
                vec![
                    enemy_blinded(),
                    Modifier::number("BlindEffect", ModType::Override, effect),
                ],
                true,
            );
            assert_eq!(enemy_accuracy(&env.enemy, &env.cfg), accuracy);
            assert_eq!(enemy_evasion(&env.enemy, &env.cfg), evasion);
        }
    }

    #[test]
    fn applying_twice_changes_nothing() {
        let mut env = run(vec![], vec![enemy_blinded()], true);
        apply_blind(&mut env);
        assert_eq!(enemy_evasion(&env.enemy, &env.cfg), 1600.0);
    }
}
