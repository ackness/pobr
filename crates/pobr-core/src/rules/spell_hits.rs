//! Skill-local timing stats. Some alternate stat sets inherit these stats
//! but describe a separate explosion, not the primary orb's repeated hits.

use crate::calc::{flameblast, periodic, rain};
use crate::{ModTag, Modifier};
use pobr_data::prelude::*;

pub fn modifiers(effect: &str, set: Option<&str>, stat: &str, value: f64) -> Option<Vec<Modifier>> {
    let primary = set.unwrap_or("1") == "1";
    let periodic = matches!(
        effect,
        "FirestormPlayer" | "IcestormPlayer" | "ThunderstormPlayer"
    ) || primary && matches!(effect, "BallLightningPlayer" | "SolarOrbPlayer");
    let base = |name| vec![Modifier::number(name, ModType::Base, value)];
    if periodic {
        match stat {
            "ball_lightning_base_hit_frequency_ms"
            | "solar_orb_base_pulse_frequency_ms"
            | "lightning_storm_hit_frequency_ms" => return Some(base(periodic::INTERVAL)),
            "fire_storm_fireball_delay_ms" => {
                return Some(vec![
                    Modifier::number(periodic::INTERVAL, ModType::Base, value),
                    Modifier::flag(periodic::EVEN_LANDINGS),
                ]);
            }
            "lightning_storm_hit_prevention_duration_ms" => return Some(base(rain::HIT_INTERVAL)),
            "base_skill_effect_duration" => {
                return Some(vec![
                    Modifier::number(periodic::DURATION, ModType::Base, value),
                    Modifier::number("SkillDurationBase", ModType::Base, value / 1000.0),
                ]);
            }
            "firestorm_max_number_of_storms"
            | "icestorm_max_number_of_storms"
            | "lightning_storm_max_number_of_storms"
            | "solar_orb_base_maximum_number_of_orbs" => {
                return Some(base(periodic::LIMIT));
            }
            _ => {}
        }
    }
    if effect == "FlameblastPlayer" {
        let result = match stat {
            // "{0}% more damage per Stage"; stages are bounded by
            // `flameblast_maximum_stages` ("{0} maximum Stages").
            "charged_blast_spell_damage_+%_final_per_stack" => vec![
                Modifier::number("Damage", ModType::More, value)
                    .with_flags(ModFlags::SPELL)
                    .with_tag(ModTag::multiplier(flameblast::STAGE, 1.0, None)),
            ],
            "flameblast_maximum_stages" => base(flameblast::MAX_STAGES),
            "base_minimum_channel_time_ms" => base(flameblast::MIN_CHANNEL),
            "channel_skill_end_animation_duration_override_ms" => base(flameblast::RELEASE),
            _ => return None,
        };
        return Some(result);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CalcConfig, ModDb};

    #[test]
    fn periodic_duration_also_reaches_the_skill_duration_view() {
        let mut db = ModDb::new();
        db.add_list(
            modifiers(
                "FirestormPlayer",
                None,
                "base_skill_effect_duration",
                3000.0,
            )
            .unwrap(),
        );
        let cfg = CalcConfig::spell();
        assert_eq!(
            db.sum(ModType::Base, &cfg, &[ModName::from(periodic::DURATION)]),
            3000.0
        );
        assert_eq!(
            crate::calc::skill_mechanics::skill_durations(&db, &ModDb::new(), &cfg).primary,
            3.003
        );
    }

    #[test]
    fn orb_explosions_do_not_inherit_the_primary_effects_hit_interval() {
        for (effect, stat) in [
            (
                "BallLightningPlayer",
                "ball_lightning_base_hit_frequency_ms",
            ),
            ("SolarOrbPlayer", "solar_orb_base_pulse_frequency_ms"),
        ] {
            assert!(modifiers(effect, Some("1"), stat, 200.0).is_some());
            assert!(modifiers(effect, Some("2"), stat, 200.0).is_none());
        }
        assert!(
            modifiers(
                "FirestormPlayer",
                Some("2"),
                "fire_storm_fireball_delay_ms",
                200.0
            )
            .is_some()
        );
        assert!(
            modifiers(
                "BonestormPlayer",
                None,
                "base_minimum_channel_time_ms",
                300.0
            )
            .is_none()
        );
    }

    #[test]
    fn snap_bonus_requires_a_unique_enemy_and_only_applies_to_hits() {
        let mut db = ModDb::new();
        db.add_list(
            super::super::skill_stats::skill_modifiers(
                "snap_damage_+%_final_if_created_from_unique",
                50.0,
            )
            .unwrap(),
        );
        let damage = [ModName::from("Damage")];
        let hit = CalcConfig::spell().with_flags(ModFlags::SPELL | ModFlags::HIT);
        assert_eq!(db.more(&hit, &damage), 1.0);
        assert_eq!(db.more(&hit.with_condition("Unique", true), &damage), 1.5);
        let ailment = CalcConfig::default()
            .with_flags(ModFlags::DOT)
            .with_condition("Unique", true);
        assert_eq!(db.more(&ailment, &damage), 1.0);
    }
}
