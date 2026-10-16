//! Calcs views of the inputs already resolved by the ailment stage.
use pobr_data::prelude::{AilmentType, DamageType};

use super::{AilmentStageTerms, Env, OutputTable, ailment, session::detail::DerivedDetail};

fn split_stat(stat: &str) -> Option<(AilmentType, &str)> {
    for (prefix, kind) in [
        ("Ignite", AilmentType::Ignite),
        ("Bleed", AilmentType::Bleed),
        ("Poison", AilmentType::Poison),
    ] {
        if let Some(suffix) = stat.strip_prefix(prefix) {
            let scalar = matches!(
                suffix,
                "Duration"
                    | "EffMult"
                    | "MagnitudeEffect"
                    | "RollAverage"
                    | "StacksMax"
                    | "StackPotentialPercent"
            ) || (kind == AilmentType::Ignite && suffix == "ChancePerHit")
                || (kind != AilmentType::Ignite && suffix == "Chance");
            let range = match kind {
                AilmentType::Ignite => matches!(
                    suffix,
                    "FireMin" | "FireMax" | "ColdMin" | "ColdMax" | "LightningMin" | "LightningMax"
                ),
                AilmentType::Bleed => matches!(suffix, "PhysicalMin" | "PhysicalMax"),
                AilmentType::Poison => matches!(
                    suffix,
                    "PhysicalMin" | "PhysicalMax" | "ChaosMin" | "ChaosMax"
                ),
                _ => false,
            };
            return (scalar || range).then_some((kind, suffix));
        }
    }
    None
}

pub(crate) fn is_display_stat(stat: &str) -> bool {
    stat == "ShockChance" || split_stat(stat).is_some()
}

fn stage(out: &OutputTable, kind: AilmentType) -> Option<&AilmentStageTerms> {
    out.ailment_terms
        .iter()
        .find(|(k, _)| *k == kind)
        .map(|(_, t)| t)
}

fn source_range(stage: &AilmentStageTerms, suffix: &str) -> f64 {
    let (prefix, maximum) = if let Some(prefix) = suffix.strip_suffix("Max") {
        (prefix, true)
    } else {
        (suffix.trim_end_matches("Min"), false)
    };
    let damage_type = match prefix {
        "Physical" => DamageType::Physical,
        "Fire" => DamageType::Fire,
        "Cold" => DamageType::Cold,
        "Lightning" => DamageType::Lightning,
        "Chaos" => DamageType::Chaos,
        _ => return 0.0,
    };
    stage
        .sources
        .last()
        .into_iter()
        .flatten()
        .filter(|part| part.damage_type == damage_type)
        .map(|part| if maximum { part.hit_max } else { part.hit_min })
        .sum()
}

pub(crate) fn display_value(out: &OutputTable, stat: &str) -> Option<f64> {
    if stat == "ShockChance" {
        return Some(out.shock_chance * 100.0);
    }
    let (kind, suffix) = split_stat(stat)?;
    let Some(stage) = stage(out, kind) else {
        return Some(0.0);
    };
    let Some((_, t)) = stage.passes.last() else {
        return Some(0.0);
    };
    Some(match suffix {
        // The application chance is measured before over-stacking changes the roll.
        "ChancePerHit" => t.apply_chance * 100.0,
        "Chance" => {
            stage
                .passes
                .iter()
                .map(|(_, p)| p.apply_chance_rolled)
                .sum::<f64>()
                / stage.pass_count.max(1) as f64
                * 100.0
        }
        "Duration" => t.duration,
        "EffMult" => t.enemy,
        "MagnitudeEffect" => (1.0 + t.magnitude_inc / 100.0) * t.magnitude_more,
        "RollAverage" => t.damage_roll,
        "StacksMax" => t.max_stacks,
        "StackPotentialPercent" => t.stack_potential * 100.0,
        _ => source_range(stage, suffix),
    })
}

pub(crate) fn detail(env: &Env, stat: &str) -> Option<DerivedDetail> {
    let out = &env.player.output;
    let mut context = Vec::new();
    let one = |id: &str, value| (id.to_string(), vec![(id.to_string(), value)]);
    let (formula, inputs) = if stat == "ShockChance" {
        (
            "ShockApplicationChance * 100".into(),
            vec![("ShockApplicationChance".into(), out.shock_chance)],
        )
    } else {
        let (kind, suffix) = split_stat(stat)?;
        let Some(stage) = stage(out, kind) else {
            return Some(DerivedDetail {
                formula: "0 * AilmentInstances".into(),
                inputs: vec![("AilmentInstances".into(), 0.0)],
                context,
            });
        };
        let (_, t) = stage.passes.last()?;
        match suffix {
            "ChancePerHit" => (
                "AilmentApplicationChance * 100".into(),
                vec![("AilmentApplicationChance".into(), t.apply_chance)],
            ),
            "Chance" => {
                let mut rows: Vec<_> = stage
                    .passes
                    .iter()
                    .enumerate()
                    .map(|(index, (_, t))| {
                        (
                            format!("AilmentPass{}Chance", index + 1),
                            t.apply_chance_rolled,
                        )
                    })
                    .collect();
                let formula = format!(
                    "({}) / AilmentPassCount * 100",
                    rows.iter()
                        .map(|(id, _)| id.as_str())
                        .collect::<Vec<_>>()
                        .join(" + ")
                );
                rows.push(("AilmentPassCount".into(), stage.pass_count.max(1) as f64));
                (formula, rows)
            }
            "Duration" => {
                context.push(("BaseAilmentDuration".into(), t.base_duration));
                (
                    "AilmentDurationBeforeRate * EnemyDebuffDuration / AilmentDamageRate".into(),
                    vec![
                        ("AilmentDurationBeforeRate".into(), t.duration_before_rate),
                        ("EnemyDebuffDuration".into(), t.enemy_duration),
                        (
                            "AilmentDamageRate".into(),
                            if t.rate > 0.0 { t.rate } else { 1.0 },
                        ),
                    ],
                )
            }
            "EffMult" => {
                let damage_type = match kind {
                    AilmentType::Ignite => DamageType::Fire,
                    AilmentType::Poison => DamageType::Chaos,
                    _ => DamageType::Physical,
                };
                let (resist, inc, more) = ailment::effmult_terms(
                    &env.enemy.mod_db,
                    &env.cfg,
                    damage_type,
                    env.cfg.mode_effective,
                );
                ("round((1 - AilmentEnemyResistance / 100) * (1 + AilmentDamageTakenIncreased / 100) * AilmentDamageTakenMore, 9)".into(), vec![
                    ("AilmentEnemyResistance".into(), resist), ("AilmentDamageTakenIncreased".into(), inc), ("AilmentDamageTakenMore".into(), more),
                ])
            }
            "MagnitudeEffect" => (
                "(1 + AilmentMagnitudeIncreased / 100) * AilmentMagnitudeMore".into(),
                vec![
                    ("AilmentMagnitudeIncreased".into(), t.magnitude_inc),
                    ("AilmentMagnitudeMore".into(), t.magnitude_more),
                ],
            ),
            "StacksMax" => one("MaximumAilmentStacks", t.max_stacks),
            "StackPotentialPercent" | "RollAverage" => {
                let active = if t.stacks > 0.0 {
                    t.stacks
                } else {
                    t.max_stacks
                };
                context.extend([
                    ("AilmentHitChance".into(), t.hit_chance),
                    ("AilmentApplicationChance".into(), t.apply_chance),
                    ("AilmentDuration".into(), t.duration),
                    ("AilmentHitsPerSecond".into(), t.hits_per_second),
                ]);
                let formula = if suffix == "StackPotentialPercent" {
                    "EstimatedAilmentStacks / MaximumAilmentStacks * 100"
                } else if active > t.max_stacks {
                    "min(100, max(0, (EstimatedAilmentStacks - (MaximumAilmentStacks - 1) / 2) / (EstimatedAilmentStacks + 1) * 100))"
                } else {
                    "50 + 0 * EstimatedAilmentStacks"
                };
                (
                    formula.into(),
                    vec![
                        ("EstimatedAilmentStacks".into(), active),
                        ("MaximumAilmentStacks".into(), t.max_stacks),
                    ],
                )
            }
            _ => one(
                if suffix.ends_with("Max") {
                    "AilmentSourceMaximum"
                } else {
                    "AilmentSourceMinimum"
                },
                source_range(stage, suffix),
            ),
        }
    };
    Some(DerivedDetail {
        formula,
        inputs,
        context,
    })
}
