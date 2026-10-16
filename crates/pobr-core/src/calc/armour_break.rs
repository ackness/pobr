//! Armour Break removes a stated amount of Armour, or all of it when no
//! amount is supplied. "You can Break enemy Armour to below 0" extends the
//! lower bound to the inverse of the target's original Armour.

use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb, Modifier};

use super::Env;

pub(crate) fn apply_armour_break(env: &mut Env) {
    if !env.cfg.mode_effective
        || !env.enemy.mod_db.flag(&env.cfg, "ArmourBreakConfigured")
        || env.enemy.mod_db.flag(&env.cfg, "ArmourBreakImmune")
        || env
            .enemy
            .mod_db
            .named("Armour")
            .any(|modifier| modifier.source.as_deref() == Some("ArmourBreak"))
    {
        return;
    }
    let db = &env.enemy.mod_db;
    let names = [ModName::from("Armour")];
    let original = db.override_(&env.cfg, "Armour").unwrap_or_else(|| {
        db.sum(ModType::Base, &env.cfg, &names)
            * (1.0 + db.sum(ModType::Inc, &env.cfg, &names) / 100.0)
            * db.more(&env.cfg, &names)
    });
    let original = original.max(0.0);
    let below_zero = env
        .player
        .mod_db
        .flag(&env.cfg, "Condition:CanArmourBreakBelowZero")
        || env.cfg.condition("CanArmourBreakBelowZero");
    let minimum = if below_zero { -original } else { 0.0 };
    let amount = db.sum(
        ModType::Base,
        &env.cfg,
        &[ModName::from("ArmourBreakAmount")],
    );
    let remaining = if amount > 0.0 {
        (original - amount).max(minimum)
    } else {
        minimum
    };
    let origin =
        ModifierSource::new(SourceId::new(SourceKind::Buff, "debuff.ArmourBreak")).with_raw_text(
            format!("Armour Break: {} Armour removed", original - remaining),
        );
    env.enemy.mod_db.add_mod(
        Modifier::number("Armour", ModType::Override, remaining)
            .with_source("ArmourBreak")
            .with_origin(origin.clone()),
    );
    for (condition, enabled) in [
        ("ArmourBroken", remaining < original),
        ("ArmourFullyBroken", remaining <= 0.0),
    ] {
        env.cfg
            .conditions
            .insert(format!("Enemy{condition}"), enabled);
        env.enemy.mod_db.add_mod(
            Modifier::new(
                format!("Condition:{condition}"),
                ModType::Flag,
                crate::ModValue::Bool(enabled),
            )
            .with_origin(origin.clone()),
        );
    }
}

/// Game Armour Break keyword: non-player targets take 20% increased
/// Physical damage from Hits while Fully Broken (poe2db.tw/us/Keywords).
/// `broken_armour_and_sundered_armour_debuff_effect_+%` scales that effect.
pub(super) fn fully_broken_effect(db: &ModDb, cfg: &CalcConfig) -> f64 {
    let names = [ModName::from("FullyBrokenArmourEffect")];
    20.0 * (1.0 + db.sum(ModType::Inc, cfg, &names) / 100.0).max(0.0) * db.more(cfg, &names)
}

pub(crate) fn apply_fully_broken_effect(env: &mut Env) {
    if !env.cfg.mode_effective
        || env.enemy.mod_db.flag(&env.cfg, "ArmourBreakImmune")
        || !(env.cfg.condition("EnemyArmourFullyBroken")
            || env
                .enemy
                .mod_db
                .flag(&env.cfg, "Condition:ArmourFullyBroken"))
        || env
            .enemy
            .mod_db
            .named("PhysicalDamageTaken")
            .any(|m| m.source.as_deref() == Some("FullyBrokenArmour"))
    {
        return;
    }
    let effect = fully_broken_effect(&env.player.mod_db, &env.cfg);
    env.enemy.mod_db.add_mod(
        Modifier::number("PhysicalDamageTaken", ModType::Inc, effect)
            .with_keyword_flags(KeywordFlags::HIT)
            .with_source("FullyBrokenArmour")
            .with_origin(
                ModifierSource::new(SourceId::new(SourceKind::Buff, "debuff.FullyBrokenArmour"))
                    .with_raw_text(format!(
                        "Fully Broken Armour: {effect}% increased Physical damage taken from Hits"
                    )),
            ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Actor, ActorBaseStats};

    #[test]
    fn configured_amount_respects_full_break_and_the_lower_bound() {
        for (amount, below_zero, expected) in [
            (0.0, false, 0.0),
            (250.0, false, 750.0),
            (1500.0, false, 0.0),
            (1500.0, true, -500.0),
            (2500.0, true, -1000.0),
            (0.0, true, -1000.0),
        ] {
            let mut env = Env::new(Actor::new(80, ActorBaseStats::default()));
            env.cfg = env
                .cfg
                .with_mode_effective(true)
                .with_condition("CanArmourBreakBelowZero", below_zero);
            env.enemy.mod_db.add_list([
                Modifier::number("Armour", ModType::Base, 1000.0),
                Modifier::flag("ArmourBreakConfigured"),
                Modifier::number("ArmourBreakAmount", ModType::Base, amount),
            ]);
            apply_armour_break(&mut env);
            assert_eq!(
                env.enemy.mod_db.override_(&env.cfg, "Armour"),
                Some(expected)
            );
            assert_eq!(env.cfg.condition("EnemyArmourFullyBroken"), expected <= 0.0);
            apply_armour_break(&mut env);
            assert_eq!(
                env.enemy.mod_db.override_(&env.cfg, "Armour"),
                Some(expected)
            );
        }
    }
}
