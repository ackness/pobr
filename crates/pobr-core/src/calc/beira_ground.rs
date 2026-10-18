//! Beira's Anguish on-use ground, independent of the selected skill.
//! Game Keywords: Ignited Ground builds Flammability and ignites at 50%;
//! default radius 2 m. The item supplies duration and equivalent Fire damage.
//! https://poe2db.tw/us/Keywords and https://poe2db.tw/us/Beiras_Anguish
use pobr_data::prelude::*;

use super::{
    Env, ailment, round,
    utility::{self, UtilityTerms},
};

pub const STATS: &[&str] = &["BeiraIgniteDPS", "CombinedIgniteDPS"];

fn write(env: &mut Env, id: String, value: f64, formula: &str, inputs: Vec<(&str, f64)>) {
    env.player.output.utility_stats.insert(id.clone(), value);
    env.utility_details.insert(
        id,
        UtilityTerms {
            formula: formula.into(),
            inputs: inputs
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect(),
        },
    );
}

pub(super) fn calculate(env: &mut Env) {
    // A charm's non-hit source cannot inherit the main skill's local supports,
    // gem quality, damage ranges, critical state, or attack/spell/weapon scope.
    // filtered() shares the original usage log, retaining source attribution.
    let player = env.player.mod_db.filtered(|modifier| {
        modifier.origin.as_ref().is_none_or(|origin| {
            !matches!(
                origin.source_id.kind,
                SourceKind::SkillGem
                    | SourceKind::SupportGem
                    | SourceKind::GemQuality
                    | SourceKind::SkillLevel
            )
        })
    });
    let cfg = utility::query_config(&env.cfg)
        .with_flags(ModFlags::DOT)
        .with_keyword_flags(KeywordFlags::AILMENT | KeywordFlags::IGNITE | KeywordFlags::FIRE_DOT)
        .with_damage_type(DamageType::Fire)
        .with_damage_keywords(Vec::new())
        .with_skill_distance(None)
        .with_condition("CriticalStrike", false)
        .with_condition("EnemyIgnited", true);
    let requested = env.cfg.condition("BeiraIgniteActive");
    let active = env.cfg.mode_combat
        && if env.cfg.conditions.contains_key("BeiraIgniteActive") {
            requested
        } else {
            player.flag(&cfg, "Condition:BeiraIgniteActive")
        };
    let slots: Vec<_> = env
        .utility_items
        .iter()
        .filter(|item| item.is_charm)
        .map(|item| {
            let slot = item.slot.replace(' ', "");
            let local = |name| {
                player
                    .sum(
                        ModType::Base,
                        &cfg,
                        &[ModName::from(utility::local_name(&slot, name))],
                    )
                    .max(0.0)
            };
            let duration = local("UtilityIgnitedGroundDuration");
            let percent = local("UtilityIgnitedGroundLifePercent");
            (slot, duration, percent)
        })
        .collect();
    if !slots
        .iter()
        .any(|(_, duration, percent)| *duration > 0.0 && *percent > 0.0)
    {
        for (slot, duration, _) in slots {
            write(
                env,
                format!("{slot}IgnitedGroundDuration"),
                duration,
                "UtilityIgnitedGroundDuration",
                vec![("UtilityIgnitedGroundDuration", duration)],
            );
            for field in [
                "IgnitedGroundRadiusMetres",
                "IgnitedGroundSourceDamage",
                "IgnitedGroundIgniteDPS",
                "IgnitedGroundIgniteDuration",
            ] {
                write(
                    env,
                    format!("{slot}{field}"),
                    0.0,
                    "0 * BeiraGroundPresent",
                    vec![("BeiraGroundPresent", 0.0)],
                );
            }
        }
        fill_totals(env, active, 0.0);
        return;
    }
    let enemy = &env.enemy.mod_db;
    let enabled = !["CannotIgnite", "DealNoDamage", "DealNoFire"]
        .into_iter()
        .any(|name| player.flag(&cfg, name));
    let effect = ailment::ailment_effect_mod(&player, &cfg).max(0.0);
    let rate = ailment::ailment_rate_mod(&player, enemy, &cfg, "Ignite").max(0.0);
    let enemy_mult =
        ailment::effmult_for_ailment(enemy, &cfg, DamageType::Fire, cfg.mode_effective).max(0.0);
    let expiration = ailment::debuff_duration_mult(enemy, &cfg);
    let names = ailment::duration_mod_names(AilmentType::Ignite);
    let target_names = ailment::self_duration_mod_names(AilmentType::Ignite);
    let duration_inc =
        player.sum(ModType::Inc, &cfg, &names) + enemy.sum(ModType::Inc, &cfg, &target_names);
    let duration_more = player.more(&cfg, &names) * enemy.more(&cfg, &target_names);
    let magnitude_inc = player.sum(ModType::Inc, &cfg, &[ModName::from("AilmentMagnitude")]);
    let magnitude_more = player.more(&cfg, &[ModName::from("AilmentMagnitude")]);
    let life = env.player.output.life.max(0.0);
    let fraction = cfg.constants.game().ignite_base_fraction;
    let base_duration = cfg.constants.game().ignite_base_duration;
    let cap = cfg.constants.game().dot_dps_cap;
    let mut strongest: f64 = 0.0;
    for (slot, ground_duration, percent) in slots {
        let present = ground_duration > 0.0 && percent > 0.0;
        let source = if present { life * percent / 100.0 } else { 0.0 };
        let instance = ailment::ground_ignite_instance(source, &player, &cfg);
        let can_ignite = present && enabled && rate > 0.0;
        let dps = if can_ignite {
            ailment::apply_dot_dps_cap(
                instance.magnitude_dps.max(0.0) * enemy_mult * effect * rate,
                cap,
            )
        } else {
            0.0
        };
        let duration = if can_ignite {
            round(
                (base_duration * (1.0 + duration_inc / 100.0) * duration_more * expiration / rate)
                    .max(0.0),
            )
        } else {
            0.0
        };
        strongest = strongest.max(dps);
        write(
            env,
            format!("{slot}IgnitedGroundDuration"),
            ground_duration,
            "UtilityIgnitedGroundDuration",
            vec![("UtilityIgnitedGroundDuration", ground_duration)],
        );
        // Keyword default, not the charm buff's duration, quality or effect.
        write(
            env,
            format!("{slot}IgnitedGroundRadiusMetres"),
            if present { 2.0 } else { 0.0 },
            "BeiraGroundPresent * IgnitedGroundBaseRadius",
            vec![
                ("BeiraGroundPresent", f64::from(present)),
                ("IgnitedGroundBaseRadius", 2.0),
            ],
        );
        write(
            env,
            format!("{slot}IgnitedGroundSourceDamage"),
            source,
            "MaximumLife * IgnitedGroundLifePercent / 100",
            vec![
                ("MaximumLife", life),
                (
                    "IgnitedGroundLifePercent",
                    if present { percent } else { 0.0 },
                ),
            ],
        );
        write(
            env,
            format!("{slot}IgnitedGroundIgniteDPS"),
            dps,
            "BeiraCanIgnite * round(min(max(round(IgnitedGroundSourceDamage * IgniteBaseFraction * (1 + IgniteMagnitudeIncreased / 100) * IgniteMagnitudeMore, 9), 0) * EnemyIgniteMultiplier * IgniteEffect * IgniteRate, DotDpsCap), 9)",
            vec![
                ("BeiraCanIgnite", f64::from(can_ignite)),
                ("IgnitedGroundSourceDamage", source),
                ("IgniteBaseFraction", fraction),
                ("IgniteMagnitudeIncreased", magnitude_inc),
                ("IgniteMagnitudeMore", magnitude_more),
                ("EnemyIgniteMultiplier", enemy_mult),
                ("IgniteEffect", effect),
                ("IgniteRate", rate),
                ("DotDpsCap", cap),
            ],
        );
        write(
            env,
            format!("{slot}IgnitedGroundIgniteDuration"),
            duration,
            if can_ignite {
                "round(max(IgniteBaseDuration * (1 + IgniteDurationIncreased / 100) * IgniteDurationMore * EnemyDebuffDuration / IgniteRate, 0), 9)"
            } else {
                "0 * IgniteBaseDuration"
            },
            vec![
                ("BeiraCanIgnite", f64::from(can_ignite)),
                ("IgniteBaseDuration", base_duration),
                ("IgniteDurationIncreased", duration_inc),
                ("IgniteDurationMore", duration_more),
                ("EnemyDebuffDuration", expiration),
                ("IgniteRate", rate),
            ],
        );
    }
    fill_totals(env, active, strongest);
}

fn fill_totals(env: &mut Env, active: bool, strongest: f64) {
    let current = if active { strongest } else { 0.0 };
    write(
        env,
        "BeiraIgniteDPS".into(),
        current,
        "BeiraIgniteActive * BeiraStrongestIgniteDPS",
        vec![
            ("BeiraIgniteActive", f64::from(active)),
            ("BeiraStrongestIgniteDPS", strongest),
        ],
    );
    let out = &env.player.output;
    let main = if out.ignite_stacked_dps > 0.0 {
        out.ignite_stacked_dps
    } else {
        out.ignite_dps
    };
    // Different supported-skill multi-Ignite rules do not establish a shared
    // cross-source stack budget. Keep that unresolved case independent.
    let can_merge = out.ignite_max_stacks <= 1.0;
    let combined = if can_merge { main.max(current) } else { main };
    write(
        env,
        "CombinedIgniteDPS".into(),
        combined,
        "MainIgniteDPS + BeiraCanMerge * max(BeiraIgniteDPS - MainIgniteDPS, 0)",
        vec![
            ("MainIgniteDPS", main),
            ("BeiraCanMerge", f64::from(can_merge)),
            ("BeiraIgniteDPS", current),
        ],
    );
}
