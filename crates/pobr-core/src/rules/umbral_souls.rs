//! Umbral Well, a notable of the Abyssal Lich: "Skeletal Minions you would
//! create instead grant you Umbral Souls for each Minion you would have
//! created".
//!
//! The rules, as the game's own data defines them:
//!
//! - A skill whose Skeletal Minions can become Souls carries the stat
//!   [`SOUL_MAGNITUDE`] (`skeletal_graft_buff_effect_magnitude`), how much one
//!   of its Souls grants: 6 for Skeletal Reavers, 15 for Skeletal Snipers.
//! - Each kind of Skeletal Minion becomes its own buff, one of the game's
//!   `lich_skeletal_graft_*` buff definitions, which grants a single stat per
//!   Soul: "This Soul type grants {0}% increased Skill Speed per Soul" for the
//!   Reaver's.
//! - The character has a Soul for each minion the skill would have created,
//!   and the minions are not created.

use pobr_data::prelude::*;

use crate::Modifier;
use crate::rules::skill_stats::SKILL_SPEED;

/// The flag "Skeletal Minions you would create instead grant you Umbral
/// Souls for each Minion you would have created" sets.
pub const UMBRAL_WELL: &str = "UmbralWell";

/// The stat of a skill saying how much one Soul of its minions grants.
pub const SOUL_MAGNITUDE: &str = "skeletal_graft_buff_effect_magnitude";

/// The Soul a kind of Skeletal Minion becomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Soul {
    /// The game's buff definition, e.g. `lich_skeletal_graft_reaver`.
    pub buff: &'static str,
    /// The stat the buff grants per Soul.
    pub stat: &'static str,
}

/// The Souls of the game's buff definitions, by the skill summoning the
/// minions they replace.
const SOULS: [(&str, Soul); 8] = [
    (
        "SummonSkeletalWarriorsPlayer",
        Soul {
            buff: "lich_skeletal_graft_warrior",
            stat: "attack_damage_+%",
        },
    ),
    (
        "SummonSkeletalSnipersPlayer",
        Soul {
            buff: "lich_skeletal_graft_archer",
            stat: "base_projectile_speed_+%",
        },
    ),
    (
        "SummonSkeletalReaversPlayer",
        Soul {
            buff: "lich_skeletal_graft_reaver",
            stat: "skill_speed_+%",
        },
    ),
    (
        "SummonSkeletalBrutesPlayer",
        Soul {
            buff: "lich_skeletal_graft_brute",
            stat: "hit_damage_stun_multiplier_+%",
        },
    ),
    (
        "SummonSkeletalArsonistsPlayer",
        Soul {
            buff: "lich_skeletal_graft_arsonist",
            stat: "base_skill_area_of_effect_+%",
        },
    ),
    (
        "SummonSkeletalClericsPlayer",
        Soul {
            buff: "lich_skeletal_graft_cleric",
            stat: "energy_shield_recharge_rate_+%",
        },
    ),
    (
        "SummonSkeletalFrostMagesPlayer",
        Soul {
            buff: "lich_skeletal_graft_frost_mage",
            stat: "maximum_energy_shield_+%",
        },
    ),
    (
        "SummonSkeletalStormMagesPlayer",
        Soul {
            buff: "lich_skeletal_graft_storm_mage",
            stat: "spell_damage_+%",
        },
    ),
];

impl Soul {
    /// The Soul the minions of the skill `skill` become, when they are
    /// Skeletal Minions.
    pub fn of_skill(skill: &str) -> Option<Soul> {
        SOULS
            .iter()
            .find(|(id, _)| *id == skill)
            .map(|(_, soul)| *soul)
    }

    /// The modifiers `souls` of these Souls grant the character, each
    /// granting `magnitude`. Each stat's description is "{0}% increased …",
    /// and each modifier carries that line.
    pub fn modifiers(self, magnitude: f64, souls: u32) -> Vec<Modifier> {
        let (increased, names): (&str, &[&str]) = match self.stat {
            "attack_damage_+%" => ("Attack Damage", &["AttackDamage"]),
            "base_projectile_speed_+%" => ("Projectile Speed", &["ProjectileSpeed"]),
            "skill_speed_+%" => ("Skill Speed", &SKILL_SPEED),
            "hit_damage_stun_multiplier_+%" => ("Stun Buildup", &["EnemyHeavyStunBuildup"]),
            "base_skill_area_of_effect_+%" => ("Area of Effect", &["AreaOfEffect"]),
            "energy_shield_recharge_rate_+%" => {
                ("Energy Shield Recharge Rate", &["EnergyShieldRecharge"])
            }
            "maximum_energy_shield_+%" => ("maximum Energy Shield", &["EnergyShield"]),
            "spell_damage_+%" => ("Spell Damage", &["SpellDamage"]),
            _ => return Vec::new(),
        };
        let value = magnitude * f64::from(souls);
        let line = format!("{value}% increased {increased}");
        names
            .iter()
            .map(|name| Modifier::number(*name, ModType::Inc, value).with_source(line.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_soul_grants_its_stat() {
        for (skill, soul) in SOULS {
            assert_eq!(Soul::of_skill(skill), Some(soul));
            assert!(!soul.modifiers(1.0, 1).is_empty(), "{}", soul.stat);
        }
        assert_eq!(Soul::of_skill("SummonRagingSpiritsPlayer"), None);
    }

    /// Two Reaver Souls at 6 each: "12% increased Skill Speed".
    #[test]
    fn souls_add_up() {
        let reaver = Soul::of_skill("SummonSkeletalReaversPlayer").unwrap();
        let modifiers = reaver.modifiers(6.0, 2);
        let names: Vec<_> = modifiers.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, SKILL_SPEED);
        assert!(modifiers.iter().all(|m| m.mod_type == ModType::Inc
            && m.value.as_number() == Some(12.0)
            && m.source.as_deref() == Some("12% increased Skill Speed")));
    }
}
