//! Flameblast releases once after channelling its chosen stages. Its game
//! stats give the per-stage bonus, maximum stages, minimum channel time and
//! release animation. The default is the first stage, not zero stages.

use crate::{CalcConfig, ModDb};
use pobr_data::prelude::*;

pub const STAGE: &str = "FlameblastStage";
pub const MAX_STAGES: &str = "FlameblastMaxStages";
pub const MIN_CHANNEL: &str = "FlameblastMinimumChannelTime";
pub const RELEASE: &str = "FlameblastReleaseTime";

pub(crate) fn configure(db: &ModDb, cfg: &mut CalcConfig) {
    let maximum = db.sum(ModType::Base, &*cfg, &[ModName::from(MAX_STAGES)]);
    if maximum >= 1.0 {
        let stage = cfg
            .multiplier_if_set(STAGE)
            .unwrap_or(1.0)
            .floor()
            .clamp(1.0, maximum);
        cfg.multipliers.insert(STAGE.into(), stage);
    }
}

/// Time for the chosen stages, then the release. Called before action speed
/// and cooldown limits, so cast speed only shortens the channel's stage time.
pub(crate) fn release_time(db: &ModDb, cfg: &CalcConfig, stage_time: f64) -> f64 {
    let maximum = db.sum(ModType::Base, cfg, &[ModName::from(MAX_STAGES)]);
    if maximum < 1.0 {
        return stage_time;
    }
    let stages = cfg
        .multiplier_if_set(STAGE)
        .unwrap_or(1.0)
        .floor()
        .clamp(1.0, maximum);
    let minimum = db.sum(ModType::Base, cfg, &[ModName::from(MIN_CHANNEL)]) / 1000.0;
    let release = db.sum(ModType::Base, cfg, &[ModName::from(RELEASE)]) / 1000.0;
    (stage_time * stages).max(minimum) + release
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::offence::{MinimalInput, calculate_minimal, calculate_minimal_traced};
    use crate::{Modifier, rules::spell_hits};

    fn skill() -> ModDb {
        let mut db = ModDb::new();
        for (stat, value) in [
            ("charged_blast_spell_damage_+%_final_per_stack", 75.0),
            ("flameblast_maximum_stages", 10.0),
            ("base_minimum_channel_time_ms", 490.0),
            ("channel_skill_end_animation_duration_override_ms", 600.0),
        ] {
            db.add_list(spell_hits::modifiers("FlameblastPlayer", None, stat, value).unwrap());
        }
        db
    }

    #[test]
    fn stages_scale_damage_and_channel_time_with_a_minimum_and_maximum() {
        let db = skill();
        for (chosen, stages, time) in [
            (None, 1.0, 1.09),
            (Some(4.0), 4.0, 1.6),
            (Some(100.0), 10.0, 3.1),
            (Some(0.0), 1.0, 1.09),
        ] {
            let mut cfg = CalcConfig::spell();
            if let Some(chosen) = chosen {
                cfg = cfg.with_multiplier(STAGE, chosen);
            }
            configure(&db, &mut cfg);
            assert_eq!(cfg.multiplier(STAGE), stages);
            assert!(
                (db.more(&cfg, &[ModName::from("Damage")]) - (1.0 + 0.75 * stages)).abs() < 1e-9
            );
            assert!((release_time(&db, &cfg, 0.25) - time).abs() < 1e-9);
            let input = MinimalInput {
                base_action_rate: 4.0,
                base_hit_min: 100.0,
                base_hit_max: 100.0,
                ..Default::default()
            };
            let output = calculate_minimal(&db, &cfg, &input);
            assert!((output.action_rate - 1.0 / time).abs() < 1e-6);
            let traced = calculate_minimal_traced(&db, &cfg, &input);
            assert_eq!(traced.output.dps, output.dps);
        }
    }

    #[test]
    fn cooldown_still_limits_releases_and_other_channels_are_unchanged() {
        let mut db = skill();
        db.add_list([Modifier::number("SkillCooldownBase", ModType::Base, 10.0)]);
        let mut cfg = CalcConfig::spell();
        configure(&db, &mut cfg);
        let input = MinimalInput {
            base_action_rate: 4.0,
            ..Default::default()
        };
        assert!(calculate_minimal(&db, &cfg, &input).action_rate <= 0.1);
        assert_eq!(release_time(&ModDb::new(), &cfg, 0.125), 0.125);
    }
}
