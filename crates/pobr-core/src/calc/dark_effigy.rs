//! Vile Effusion fires once per distinct Chaos damage-over-time effect on
//! its target, not once plus that count, and not the raw 100 additional
//! projectiles in its targeting data.
//!
//! Game text: https://poe2db.tw/us/Dark_Effigy. The interaction description
//! at https://www.poe2wiki.net/wiki/Dark_Effegy explicitly counts Poison
//! once regardless of stacks, and includes Profane Ritual.
//!
//! This is the existing enabled-debuff static snapshot, not a timeline:
//! a supported DoT is present after buff/curse application. An explicitly
//! declared Poison state takes precedence; otherwise an Effigy that already
//! has a target may contribute one Poison if its hit can inflict it. It
//! cannot bootstrap a target from its own chance to Poison. No uptime or
//! extra totem count is invented.
//! Presence here is a sustained-state assumption, not a measurement or a
//! guarantee of full Poison uptime. Poison from another source can be
//! declared with the existing enemy-Poisoned condition.

use std::collections::BTreeSet;

use pobr_data::prelude::{AilmentType, ModType, ModifierSource, SourceId, SourceKind};

use crate::{Modifier, TraceGraph};

use super::ailment::{AilmentSource, poison_traced, stored_source_at_roll};
use super::{Env, MinimalOutput};

pub(crate) const SKILL: &str = "DarkEffigyProjectile";
pub(crate) const DOT: &str = "DarkEffigyChaosDot";
pub(crate) const PROJECTILES: &str = "DarkEffigyProjectileCount";

/// Resolve the target count after buffs/curses and the unmultiplied hit.
/// Returns whether the hit pass must read the newly added DPS factor.
pub(super) fn apply(env: &mut Env, hit: &MinimalOutput) -> bool {
    // Read the debuff payload even for other selected skills: it describes
    // the target, but only Vile Effusion changes its hit count from it.
    let mut dots: BTreeSet<_> = env.enemy.mod_db.list(&env.cfg, DOT).into_iter().collect();
    if !env.player.mod_db.flag(&env.cfg, SKILL) {
        return false;
    }
    let count = if env.cfg.mode_effective {
        let poisoned = match env.cfg.conditions.get("EnemyPoisoned") {
            Some(_) => env.cfg.condition("EnemyPoisoned"),
            None => {
                env.enemy.mod_db.flag(&env.cfg, "Condition:Poisoned")
                    || (!dots.is_empty() && can_poison(env, hit))
            }
        };
        if poisoned {
            dots.insert("Poison".into());
        }
        dots.len() as f64
    } else {
        // Panel mode displays one hypothetical projectile, not an enemy's
        // debuffs. Its raw targeting limit is never a displayed shot count.
        1.0
    };
    let origin = ModifierSource::new(SourceId::new(
        SourceKind::SkillGem,
        "skill.DarkEffigyProjectilePlayer.chaos_dot_targets",
    ))
    .with_raw_text(format!(
        "Vile Effusion: {count} projectiles per target; distinct active Chaos DoTs: {}",
        dots.into_iter().collect::<Vec<_>>().join(", ")
    ));
    env.player.mod_db.add_list([
        Modifier::number(PROJECTILES, ModType::Override, count).with_origin(origin.clone()),
        Modifier::number("DPS", ModType::More, (count - 1.0) * 100.0).with_origin(origin),
    ]);
    count != 1.0
}

/// Use the same stored source, chance, damage prohibitions, magnitude and
/// duration as the ailment calculation, without estimating an event rate.
fn can_poison(env: &Env, hit: &MinimalOutput) -> bool {
    if hit.hit_chance <= 0.0
        || hit.action_rate <= 0.0
        || env.player.mod_db.flag(&env.cfg, "CannotPoison")
    {
        return false;
    }
    let (hit_avg, crit_avg) = stored_source_at_roll(
        AilmentType::Poison,
        &hit.stored_ranges,
        &env.player.mod_db,
        &env.cfg,
        50.0,
    );
    let never_from_crit = env.player.mod_db.flag(&env.cfg, "AilmentsAreNeverFromCrit");
    let source = AilmentSource {
        hit_avg,
        crit_avg: if never_from_crit { hit_avg } else { crit_avg },
        crit_chance: if never_from_crit {
            0.0
        } else {
            hit.crit_chance
        },
    };
    let (poison, _) = poison_traced(
        &source,
        &env.player.mod_db,
        &env.enemy.mod_db,
        &env.cfg,
        &mut TraceGraph::default(),
    );
    poison.chance > 0.0 && poison.magnitude_dps > 0.0 && poison.duration_secs > 0.0
}

pub(super) fn has_no_target(env: &Env) -> bool {
    env.player.mod_db.override_(&env.cfg, PROJECTILES) == Some(0.0)
}
