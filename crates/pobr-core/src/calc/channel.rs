//! Approximation of a channel-and-release cycle, without in-game timing measurements.
//!
//! The channel lasts at least its minimum, start lock and selected stages' time.
//! One stage is assumed to take one attack time. Release uses the stated fraction
//! of attack time, or the fixed-ms override when present. Fixed durations do not
//! scale with speed. Perfect-window widths do not identify their opening: without
//! an opening-time stat, the selected perfect set uses the same minimum/start lock.
//! Animation multipliers without a stated phase duration are not another duration.
//! Shield Charge assumes an immediate collision after its minimum channel.

use super::formula::{FormulaBuilder, FormulaOp};
use crate::{CalcConfig, ModDb, ModTag, Modifier};
use pobr_data::prelude::*;

pub const STAGES: &str = "ChannelStages";

/// The durations and stage assumption behind a use of a channelled attack.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChannelCycle {
    pub attack_time: f64,
    pub minimum: f64,
    pub start_lock: f64,
    pub maximum_stages: f64,
    pub requested_stages: f64,
    pub stages: f64,
    pub default_stages: bool,
    pub channel_time: f64,
    pub release_time: f64,
    pub fixed_release: bool,
    pub cycle_time: f64,
}

/// Skills with a defined cycle in this approximation. Parry has no channel-phase
/// duration; its bash retains its own level-row attack time. Raise Shield has
/// no cycle either: "Release immediately after Blocking ... to perform a Shield
/// Bash" waits for a blocked hit, so its 250 ms minimum and 35 ms release are
/// the raise, not a bash rate. It keeps its own attack time, assuming a
/// blockable hit arrives whenever the bash can be used. Shield Charge is also
/// an `ActiveBlock` skill, but its release does not wait for a block.
pub fn skill_modifiers(
    skill: &str,
    attack_time: Option<f64>,
    aftershocks_only: bool,
) -> Vec<Modifier> {
    if !matches!(
        skill,
        "DetonatingArrowPlayer"
            | "SuperchargedSlamPlayer"
            | "PerfectStrikePlayer"
            | "GatheringStormPlayer"
            | "SnipePlayer"
            | "ShieldChargePlayer"
    ) {
        return Vec::new();
    }
    let mut mods = vec![Modifier::flag("ChannelCycle")];
    if skill == "SuperchargedSlamPlayer" {
        // "This Skill has its own Attack time ... cannot be modified."
        if let Some(time) = attack_time {
            mods.push(Modifier::number(
                "ChannelFixedAttackTime",
                ModType::Base,
                time,
            ));
        }
        // channelled_slam_max_stages: "Releases 1 Aftershock per Stage".
        // The impact set counts impact + aftershocks; the aftershock set only
        // counts the aftershocks. Each hits the target once, after its delay.
        mods.push(
            Modifier::number("DPS", ModType::More, 100.0).with_tag(ModTag::multiplier(
                "ChannelExtraHits",
                1.0,
                None,
            )),
        );
        if aftershocks_only {
            mods.push(Modifier::flag("ChannelAftershocksOnly"));
        }
    }
    mods.into_iter()
        .map(|m| m.with_source(format!("skill.{skill}.channel")))
        .collect()
}

fn base(db: &ModDb, cfg: &CalcConfig, name: &str) -> f64 {
    db.sum(ModType::Base, cfg, &[ModName::from(name)]).max(0.0)
}

fn stages(db: &ModDb, cfg: &CalcConfig) -> (f64, f64, f64, bool) {
    let max = base(db, cfg, "ChannelMaxStages").floor();
    let requested = cfg
        .multiplier_if_set("ChannelStagesRequested")
        .or_else(|| cfg.multiplier_if_set(STAGES));
    let defaulted = requested.is_none() || cfg.condition("ChannelStagesDefaulted");
    let request = requested.unwrap_or(max);
    (
        max,
        request,
        request.floor().clamp(max.min(1.0), max),
        defaulted,
    )
}

/// Resolve the count once before both damage and timing. The API's ChannelStages
/// override is optional; absent one, use the maximum supplied by the skill/quality.
pub(crate) fn resolve_stages(db: &ModDb, cfg: &mut CalcConfig) {
    if !db.flag(cfg, "ChannelCycle") {
        return;
    }
    let (maximum, request, count, defaulted) = stages(db, cfg);
    if maximum <= 0.0 {
        return;
    }
    cfg.multipliers
        .insert("ChannelStagesRequested".into(), request);
    cfg.multipliers.insert(STAGES.into(), count);
    cfg.conditions
        .insert("ChannelStagesDefaulted".into(), defaulted);
    let extra_hits = if db.flag(cfg, "ChannelAftershocksOnly") {
        (count - 1.0).max(0.0)
    } else {
        count
    };
    cfg.multipliers
        .insert("ChannelExtraHits".into(), extra_hits);
}

/// `attack_rate` already includes speed and added attack time. A skill whose text
/// fixes its attack time instead reads its level-row duration. Returns no cycle
/// if the skill has no supported channel phases or cannot perform an attack.
pub(crate) fn cycle(db: &ModDb, cfg: &CalcConfig, attack_rate: f64) -> Option<ChannelCycle> {
    if !db.flag(cfg, "ChannelCycle") {
        return None;
    }
    let fixed_time = base(db, cfg, "ChannelFixedAttackTime");
    let attack_time = if fixed_time > 0.0 {
        fixed_time
    } else if attack_rate > 0.0 {
        1.0 / attack_rate
    } else {
        return None;
    };
    let minimum = base(db, cfg, "ChannelMinimumTime");
    let start_lock = attack_time * base(db, cfg, "ChannelStartLock") / 100.0;
    let (maximum_stages, requested_stages, stages, default_stages) = stages(db, cfg);
    let channel_time = minimum.max(start_lock).max(stages * attack_time);
    let release_override = base(db, cfg, "ChannelReleaseTime");
    let fixed_release = release_override > 0.0;
    let release_time = if fixed_release {
        release_override
    } else {
        attack_time * base(db, cfg, "ChannelReleaseFraction") / 100.0
    };
    let cycle_time = channel_time + release_time;
    (cycle_time > 0.0).then_some(ChannelCycle {
        attack_time,
        minimum,
        start_lock,
        maximum_stages,
        requested_stages,
        stages,
        default_stages,
        channel_time,
        release_time,
        fixed_release,
        cycle_time,
    })
}

impl ChannelCycle {
    pub(crate) fn append_formula(&self, steps: &mut FormulaBuilder) {
        steps.set(
            FormulaOp::ChannelAttackTime,
            self.attack_time,
            1.0 / self.attack_time,
        );
        // Stage annotations retain the running rate, making the chosen default
        // and cap visible without pretending that counts have units of seconds.
        if self.maximum_stages > 0.0 {
            steps.set(
                FormulaOp::ChannelMaximumStages,
                self.maximum_stages,
                steps.current(),
            );
            let op = if self.default_stages {
                FormulaOp::ChannelDefaultStages
            } else {
                FormulaOp::ChannelRequestedStages
            };
            steps.set(op, self.requested_stages, steps.current());
            steps.set(FormulaOp::ChannelStages, self.stages, steps.current());
        }
        steps.set(
            FormulaOp::ChannelTime,
            self.channel_time,
            if self.channel_time > 0.0 {
                1.0 / self.channel_time
            } else {
                0.0
            },
        );
        steps.set(
            FormulaOp::ChannelRelease,
            self.release_time,
            1.0 / self.cycle_time,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel(skill: &str, stats: &[(&str, f64)]) -> ModDb {
        let mut db = ModDb::new();
        db.add_list(skill_modifiers(skill, Some(0.85), false));
        for (id, value) in stats {
            db.add_list(crate::rules::skill_stats::skill_modifiers(id, *value).expect("own rule"));
        }
        db
    }

    #[test]
    fn cycles_follow_each_skills_game_timing_stats() {
        // One second per attack makes the fractions explicit; Supercharged
        // uses its own 850-ms level row regardless of the supplied attack rate.
        for (skill, stats, channel_time, release) in [
            (
                "PerfectStrikePlayer",
                vec![
                    ("channel_start_lock_cancelling_of_attack_time_%", 50.0),
                    ("channel_end_duration_as_%_of_attack_time", 35.0),
                ],
                0.5,
                0.35,
            ),
            (
                "GatheringStormPlayer",
                vec![
                    ("channel_start_lock_cancelling_of_attack_time_%", 50.0),
                    ("channel_end_duration_as_%_of_attack_time", 85.0),
                ],
                0.5,
                0.85,
            ),
            (
                "SnipePlayer",
                vec![
                    ("base_minimum_channel_time_ms", 300.0),
                    ("channel_start_lock_cancelling_of_attack_time_%", 16.0),
                    ("channel_end_duration_as_%_of_attack_time", 133.0),
                ],
                0.3,
                1.33,
            ),
            (
                "ShieldChargePlayer",
                vec![("base_minimum_channel_time_ms", 1300.0)],
                1.3,
                0.0,
            ),
            (
                "DetonatingArrowPlayer",
                vec![
                    ("detonating_arrow_max_number_of_stages", 4.0),
                    ("channel_start_lock_cancelling_of_attack_time_%", 20.0),
                ],
                4.0,
                0.0,
            ),
            (
                "SuperchargedSlamPlayer",
                vec![
                    ("channelled_slam_max_stages", 3.0),
                    ("base_minimum_channel_time_ms", 1000.0),
                    ("channel_skill_end_animation_duration_override_ms", 1000.0),
                ],
                2.55,
                1.0,
            ),
        ] {
            let cycle = cycle(&channel(skill, &stats), &CalcConfig::attack(), 1.0).unwrap();
            assert!(
                (cycle.channel_time - channel_time).abs() < 1e-9,
                "{skill}: {cycle:?}"
            );
            assert_eq!(cycle.release_time, release, "{skill}");
            assert!((cycle.cycle_time - channel_time - release).abs() < 1e-9);
        }
        assert!(
            cycle(
                &channel("ParryPlayer", &[]),
                &CalcConfig::attack(),
                1.0 / 0.75
            )
            .is_none()
        );
    }

    #[test]
    fn raise_shield_uses_bash_attack_time_but_shield_charge_channels() {
        let db = channel(
            "ShieldBlockPlayer",
            &[
                ("base_minimum_channel_time_ms", 250.0),
                ("channel_skill_end_animation_duration_override_ms", 35.0),
            ],
        );
        let cfg = CalcConfig::attack().with_skill_types(
            SkillTypes::ATTACK | SkillTypes::from_pob2_name("ActiveBlock").unwrap(),
        );
        let input = super::super::MinimalInput {
            base_action_rate: 1.0 / 0.55,
            ..Default::default()
        };
        assert!(cycle(&db, &cfg, input.base_action_rate).is_none());
        let output = super::super::calculate_minimal(&db, &cfg, &input);
        assert!((output.action_rate - 1.0 / 0.55).abs() < 1e-6);
        // Shield Charge has the same skill type; its release does not wait
        // for a block, so its minimum channel time is the cycle.
        let charge = channel(
            "ShieldChargePlayer",
            &[("base_minimum_channel_time_ms", 1300.0)],
        );
        let charge_cycle = cycle(&charge, &cfg, 1.0 / 0.6).unwrap();
        assert!((charge_cycle.cycle_time - 1.3).abs() < 1e-9);
    }

    #[test]
    fn fixed_durations_do_not_scale_and_release_override_wins() {
        let db = channel(
            "SnipePlayer",
            &[
                ("base_minimum_channel_time_ms", 300.0),
                ("channel_start_lock_cancelling_of_attack_time_%", 16.0),
                ("channel_end_duration_as_%_of_attack_time", 133.0),
            ],
        );
        let slow = cycle(&db, &CalcConfig::attack(), 1.0).unwrap();
        let fast = cycle(&db, &CalcConfig::attack(), 2.0).unwrap();
        assert_eq!(slow.channel_time, fast.channel_time);
        assert_eq!(slow.release_time, 2.0 * fast.release_time);
        let override_db = channel(
            "SnipePlayer",
            &[
                ("base_minimum_channel_time_ms", 300.0),
                ("channel_end_duration_as_%_of_attack_time", 133.0),
                ("channel_skill_end_animation_duration_override_ms", 35.0),
            ],
        );
        let fixed = cycle(&override_db, &CalcConfig::attack(), 2.0).unwrap();
        assert_eq!(fixed.release_time, 0.035);
        assert!(fixed.fixed_release);
    }

    #[test]
    fn one_bounded_stage_count_drives_damage_and_channel_time() {
        let db = channel(
            "SuperchargedSlamPlayer",
            &[
                ("channelled_slam_max_stages", 3.0),
                ("channelled_slam_damage_+%_final_per_stage", 40.0),
                ("base_minimum_channel_time_ms", 1000.0),
                ("channel_skill_end_animation_duration_override_ms", 1000.0),
            ],
        );
        for (request, selected) in [
            (None, 3.0),
            (Some(2.8), 2.0),
            (Some(9.0), 3.0),
            (Some(-1.0), 1.0),
        ] {
            let mut cfg = CalcConfig::attack();
            cfg.keyword_flags = KeywordFlags::HIT;
            if let Some(request) = request {
                cfg.multipliers.insert(STAGES.into(), request);
            }
            resolve_stages(&db, &mut cfg);
            let result = cycle(&db, &cfg, 10.0).unwrap();
            assert_eq!(result.stages, selected);
            assert_eq!(result.default_stages, request.is_none());
            assert_eq!(result.channel_time, (selected * 0.85).max(1.0));
            assert_eq!(
                db.more(&cfg, &[ModName::from("Damage")]),
                1.0 + selected * 0.4
            );
            let ailment_cfg = cfg.clone().with_keyword_flags(KeywordFlags::AILMENT);
            assert_eq!(db.more(&ailment_cfg, &[ModName::from("Damage")]), 1.0);
            assert_eq!(db.more(&cfg, &[ModName::from("DPS")]), 1.0 + selected);
        }
    }
}
