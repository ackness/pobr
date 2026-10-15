//! Rage: a resource whose every point makes attacks hit harder.
//!
//! The rules, as the game's own stats and constants define them:
//!
//! - A character has Rage only if something lets it gain Rage. How much it
//!   holds is the player's statement about combat (the `multiplierRage` config
//!   input), clamped between Minimum and Maximum Rage.
//! - Maximum Rage is the game constant `BaseMaximumRage` plus the
//!   `maximum_rage` stat, with its increases.
//! - Each point grants 1% more Attack damage (the character's inherent
//!   `attack_damage_final_permyriad_per_rage = 100`). Modifiers to the
//!   "inherent effects from having Rage" (`rage_effect_+%`) scale how many
//!   points count; the count is a whole number
//!   (`effective_rage_for_per_rage_effects`). "Rage grants Spell damage
//!   instead of Attack damage" moves the effect to spells.
//! - "Each Rage also grants …" and "per Rage" modifiers read the same counts.
//!
//! The stage writes what the rest of the calculation reads: the more-damage
//! modifier, the `Rage` and `RageEffect` multipliers, the `Rage` stat and the
//! `HaveMaximumRage` condition.

use pobr_data::constants::{ATTACK_DAMAGE_MORE_PER_RAGE, BASE_MAXIMUM_RAGE};
use pobr_data::prelude::*;

use crate::Modifier;

use super::Env;

/// Source of the injected modifier; also the guard against applying it twice.
const RAGE_SOURCE: &str = "Rage";

/// Applies the character's Rage. Does nothing out of combat or for a
/// character that cannot gain Rage.
pub fn apply_rage(env: &mut Env) {
    let db = &env.player.mod_db;
    let cfg = &env.cfg;
    if !cfg.mode_combat {
        return;
    }
    let rage_name = [ModName::from("RageRegen")];
    let can_gain = db.flag(cfg, "Condition:CanGainRage")
        || cfg.condition("CanGainRage")
        || db.sum(ModType::Base, cfg, &rage_name) > 0.0;
    if !can_gain
        || db
            .named("Damage")
            .any(|modifier| modifier.source.as_deref() == Some(RAGE_SOURCE))
    {
        return;
    }

    let scaled = |name: &str, base: f64| {
        let names = [ModName::from(name)];
        (base + db.sum(ModType::Base, cfg, &names))
            * (1.0 + db.sum(ModType::Inc, cfg, &names) / 100.0)
            * db.more(cfg, &names)
    };
    let maximum = scaled("MaximumRage", BASE_MAXIMUM_RAGE).max(0.0);
    let minimum = db
        .sum(ModType::Base, cfg, &[ModName::from("MinimumRage")])
        .clamp(0.0, maximum);
    // The stated Rage is a combat input for a character that can gain Rage:
    // read it in that context without switching either condition on for the
    // rest of the calculation.
    let mut stated_cfg = cfg.clone();
    stated_cfg.conditions.insert("Combat".into(), true);
    stated_cfg.conditions.insert("CanGainRage".into(), true);
    let stated = db.sum(
        ModType::Base,
        &stated_cfg,
        &[ModName::from("Multiplier:RageStack")],
    ) + cfg.multiplier("RageStack");
    let rage = stated.clamp(minimum, maximum);

    let effect_names = [ModName::from("RageEffect")];
    let effective = db.override_(cfg, "RageEffect").unwrap_or_else(|| {
        (rage
            * (1.0 + db.sum(ModType::Inc, cfg, &effect_names) / 100.0)
            * db.more(cfg, &effect_names))
        .floor()
    });
    let flags = if db.flag(cfg, "Condition:RageSpellDamage") {
        ModFlags::SPELL
    } else {
        ModFlags::ATTACK
    };

    if effective != 0.0 {
        let origin = ModifierSource::new(SourceId::new(SourceKind::Buff, "buff.Rage"))
            .with_raw_text(format!("{rage} Rage"));
        env.player.mod_db.add_mod(
            Modifier::number(
                "Damage",
                ModType::More,
                effective * ATTACK_DAMAGE_MORE_PER_RAGE,
            )
            .with_flags(flags)
            .with_source(RAGE_SOURCE)
            .with_origin(origin),
        );
    }
    env.cfg.multipliers.insert("Rage".into(), rage);
    env.cfg.multipliers.insert("RageEffect".into(), effective);
    env.cfg.stats.insert("Rage".into(), rage);
    if rage > 0.0 && rage >= maximum {
        env.cfg.conditions.insert("HaveMaximumRage".into(), true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ModTag;
    use crate::calc::{Actor, ActorBaseStats};

    /// A character in combat that states `stated` Rage, with `extra` modifiers.
    fn run(stated: f64, extra: Vec<Modifier>) -> Env {
        let mut player = Actor::new(1, ActorBaseStats::default());
        // The config input's own shape: it counts in combat, for a character
        // that can gain Rage.
        player.mod_db.add_mod(
            Modifier::number("Multiplier:RageStack", ModType::Base, stated)
                .with_tag(ModTag::condition("Combat", false))
                .with_tag(ModTag::condition("CanGainRage", false)),
        );
        player.mod_db.add_list(extra);
        let mut env = Env::new(player);
        env.cfg = env.cfg.with_mode_combat(true);
        apply_rage(&mut env);
        env
    }

    fn can_gain() -> Modifier {
        Modifier::flag("Condition:CanGainRage")
    }

    /// The more-damage percentage `flags` sees.
    fn more_damage(env: &Env, flags: ModFlags) -> f64 {
        let cfg = env.cfg.clone().with_flags(flags);
        (env.player.mod_db.more(&cfg, &[ModName::from("Damage")]) - 1.0) * 100.0
    }

    #[test]
    fn each_rage_is_one_percent_more_attack_damage() {
        let env = run(30.0, vec![can_gain()]);
        assert_eq!(more_damage(&env, ModFlags::ATTACK).round(), 30.0);
        assert_eq!(more_damage(&env, ModFlags::SPELL).round(), 0.0);
        assert_eq!(env.cfg.multiplier("Rage"), 30.0);
        assert_eq!(env.cfg.multiplier("RageEffect"), 30.0);
        assert_eq!(env.cfg.stat("Rage"), 30.0);
        assert!(env.cfg.condition("HaveMaximumRage"));
    }

    #[test]
    fn a_character_that_cannot_gain_rage_has_none() {
        let env = run(30.0, vec![]);
        assert_eq!(more_damage(&env, ModFlags::ATTACK).round(), 0.0);
        assert_eq!(env.cfg.multiplier("Rage"), 0.0);
        assert!(!env.cfg.condition("HaveMaximumRage"));
    }

    #[test]
    fn rage_regeneration_is_a_way_to_gain_rage() {
        let regen = Modifier::number("RageRegen", ModType::Base, 4.5);
        assert_eq!(
            more_damage(&run(20.0, vec![regen]), ModFlags::ATTACK).round(),
            20.0
        );
    }

    #[test]
    fn stated_rage_is_held_between_minimum_and_maximum() {
        let over = run(50.0, vec![can_gain()]);
        assert_eq!(over.cfg.multiplier("Rage"), BASE_MAXIMUM_RAGE);

        let raised = run(
            50.0,
            vec![
                can_gain(),
                Modifier::number("MaximumRage", ModType::Base, 4.0),
            ],
        );
        assert_eq!(raised.cfg.multiplier("Rage"), 34.0);

        let floor = run(
            0.0,
            vec![
                can_gain(),
                Modifier::number("MinimumRage", ModType::Base, 10.0),
            ],
        );
        assert_eq!(floor.cfg.multiplier("Rage"), 10.0);
        assert!(!floor.cfg.condition("HaveMaximumRage"));
    }

    /// Berserk at 63% increased Rage effect: 30 Rage counts as 48, not 48.9.
    #[test]
    fn rage_effect_scales_the_count_to_a_whole_number() {
        let env = run(
            30.0,
            vec![
                can_gain(),
                Modifier::number("RageEffect", ModType::Inc, 63.0),
            ],
        );
        assert_eq!(env.cfg.multiplier("Rage"), 30.0);
        assert_eq!(env.cfg.multiplier("RageEffect"), 48.0);
        assert_eq!(more_damage(&env, ModFlags::ATTACK).round(), 48.0);
    }

    #[test]
    fn rage_can_grant_spell_damage_instead() {
        let env = run(
            30.0,
            vec![can_gain(), Modifier::flag("Condition:RageSpellDamage")],
        );
        assert_eq!(more_damage(&env, ModFlags::SPELL).round(), 30.0);
        assert_eq!(more_damage(&env, ModFlags::ATTACK).round(), 0.0);
    }

    #[test]
    fn applying_twice_adds_nothing() {
        let mut env = run(30.0, vec![can_gain()]);
        apply_rage(&mut env);
        assert_eq!(more_damage(&env, ModFlags::ATTACK).round(), 30.0);
    }

    #[test]
    fn out_of_combat_there_is_no_rage() {
        let mut player = Actor::new(1, ActorBaseStats::default());
        player.mod_db.add_mod(can_gain());
        player.mod_db.add_mod(
            Modifier::number("Multiplier:RageStack", ModType::Base, 30.0)
                .with_tag(ModTag::condition("Combat", false))
                .with_tag(ModTag::condition("CanGainRage", false)),
        );
        let mut env = Env::new(player);
        apply_rage(&mut env);
        assert_eq!(more_damage(&env, ModFlags::ATTACK).round(), 0.0);
    }
}
