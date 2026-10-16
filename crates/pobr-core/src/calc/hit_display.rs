//! Selected-hit Calcs views, using inputs retained by the offence pass.
use pobr_data::prelude::DamageType;

use super::{DamageComponent, Env, OutputTable, output::HitCalcs, session::detail::DerivedDetail};

const TYPES: [(DamageType, &str); 5] = [
    (DamageType::Physical, "Physical"),
    (DamageType::Fire, "Fire"),
    (DamageType::Cold, "Cold"),
    (DamageType::Lightning, "Lightning"),
    (DamageType::Chaos, "Chaos"),
];

fn split(stat: &str) -> (&str, &str) {
    for hand in ["MainHand", "OffHand"] {
        if let Some(suffix) = stat.strip_prefix(hand) {
            return (hand, suffix);
        }
    }
    ("", stat)
}

fn typed(stat: &str) -> Option<(DamageType, &str)> {
    TYPES.iter().find_map(|(kind, name)| {
        let suffix = stat.strip_prefix(name)?;
        matches!(suffix, "Min" | "Max" | "EffMult").then_some((*kind, suffix))
    })
}

pub(crate) fn is_display_stat(stat: &str) -> bool {
    let (hand, suffix) = split(stat);
    (hand.is_empty() && matches!(suffix, "Repeats" | "TotalNumberOfHits"))
        || matches!(
            suffix,
            "TotalMin" | "TotalMax" | "CritEffect" | "DpsMultiplier"
        )
        || typed(suffix).is_some()
}

struct Pass<'a> {
    components: &'a [DamageComponent],
    terms: &'a HitCalcs,
    crit_chance: f64,
    crit_multiplier: f64,
}

fn pass<'a>(out: &'a OutputTable, hand: &str) -> Option<Pass<'a>> {
    if hand.is_empty() {
        // PoB keeps attack ranges, mitigation and crit effect in hand tables;
        // they are not combined into the skill's top-level output.
        return (out.main_hand.is_none() && out.off_hand.is_none()).then_some(Pass {
            components: &out.damage_components,
            terms: &out.hit_calcs,
            crit_chance: out.crit_chance,
            crit_multiplier: out.crit_multiplier,
        });
    }
    let h = match hand {
        "MainHand" => out.main_hand.as_ref(),
        "OffHand" => out.off_hand.as_ref(),
        _ => None,
    }?;
    Some(Pass {
        components: &h.damage_components,
        terms: &h.hit_calcs,
        crit_chance: h.crit_chance,
        crit_multiplier: h.crit_multiplier,
    })
}

impl Pass<'_> {
    fn bound(&self, kind: DamageType, maximum: bool) -> f64 {
        self.components
            .iter()
            .filter(|c| c.damage_type == kind)
            .map(|c| if maximum { c.max } else { c.min })
            .sum()
    }

    fn effective(&self, kind: DamageType) -> f64 {
        self.terms
            .effective
            .iter()
            .find(|t| t.damage_type == kind)
            .map_or(1.0, |t| t.multiplier)
    }

    fn present(&self, kind: DamageType) -> bool {
        self.components
            .iter()
            .any(|c| c.damage_type == kind && (c.min != 0.0 || c.max != 0.0))
    }
}

pub(crate) fn display_value(out: &OutputTable, stat: &str) -> Option<f64> {
    if !is_display_stat(stat) {
        return None;
    }
    match stat {
        "Repeats" => return Some(out.repeats),
        "TotalNumberOfHits" => return Some(out.total_number_of_hits),
        "DpsMultiplier" => return Some(out.hit_calcs.dps.dps_multiplier),
        _ => (),
    }
    let (hand, suffix) = split(stat);
    let Some(p) = pass(out, hand) else {
        return Some(0.0);
    };
    Some(match suffix {
        "CritEffect" => p.terms.crit_effect,
        "DpsMultiplier" => p.terms.dps.dps_multiplier,
        "TotalMin" | "TotalMax" => TYPES
            .iter()
            .map(|(kind, _)| p.bound(*kind, suffix == "TotalMax") * p.effective(*kind))
            .sum(),
        _ => {
            let (kind, bound) = typed(suffix)?;
            if bound == "EffMult" {
                if p.present(kind) {
                    p.effective(kind)
                } else {
                    0.0
                }
            } else {
                p.bound(kind, bound == "Max") * p.effective(kind)
            }
        }
    })
}

pub(crate) fn detail(env: &Env, stat: &str) -> Option<DerivedDetail> {
    if !is_display_stat(stat) {
        return None;
    }
    let out = &env.player.output;
    let (hand, suffix) = split(stat);
    let detail = |formula: &str, inputs: Vec<(&str, f64)>| DerivedDetail {
        formula: formula.into(),
        inputs: inputs
            .into_iter()
            .map(|(id, value)| (id.into(), value))
            .collect(),
        context: Vec::new(),
    };
    match stat {
        "Repeats" => {
            return Some(detail(
                "1 + max(0, AdditionalRepeats)",
                vec![(
                    "AdditionalRepeats",
                    env.player.mod_db.sum(
                        pobr_data::prelude::ModType::Base,
                        &env.cfg,
                        &[pobr_data::prelude::ModName::from("RepeatCount")],
                    ),
                )],
            ));
        }
        "TotalNumberOfHits" => {
            let t = out.ehp_terms.as_ref()?;
            return Some(detail(
                "MitigatedSurvivalHits / (1 - AvoidedHitChance / 100)",
                vec![
                    ("MitigatedSurvivalHits", t.mitigated_hits),
                    ("AvoidedHitChance", t.not_hit_chance.clamp(0.0, 100.0)),
                ],
            ));
        }
        "DpsMultiplier" if out.main_hand.is_some() && out.off_hand.is_some() => {
            return Some(detail(
                "(MainHandDpsMultiplier + OffHandDpsMultiplier) / HandDpsDivisor",
                vec![
                    (
                        "MainHandDpsMultiplier",
                        out.main_hand.as_ref()?.hit_calcs.dps.dps_multiplier,
                    ),
                    (
                        "OffHandDpsMultiplier",
                        out.off_hand.as_ref()?.hit_calcs.dps.dps_multiplier,
                    ),
                    (
                        "HandDpsDivisor",
                        if env.double_hits_when_dual_wielding {
                            1.0
                        } else {
                            2.0
                        },
                    ),
                ],
            ));
        }
        _ => (),
    }
    // A single attack's DPS multiplier passes through unchanged.
    let p = if stat == "DpsMultiplier" {
        pass(out, "MainHand")
            .or_else(|| pass(out, "OffHand"))
            .or_else(|| pass(out, ""))
    } else {
        pass(out, hand)
    };
    let Some(p) = p else {
        return Some(detail(
            "0 * HitPassAvailable",
            vec![("HitPassAvailable", 0.0)],
        ));
    };
    Some(match suffix {
        "CritEffect" => detail(
            "round(1 - HitCritChance + HitCritChance * HitCritMultiplier, 9)",
            vec![
                ("HitCritChance", p.crit_chance),
                ("HitCritMultiplier", p.crit_multiplier),
            ],
        ),
        "DpsMultiplier" => {
            let t = &p.terms.dps;
            detail(
                "SkillDpsFactor * (1 + IncreasedDps / 100) * MoreDps * GrenadeDpsFactor * BarrageDpsFactor",
                vec![
                    ("SkillDpsFactor", t.skill),
                    ("IncreasedDps", t.increased),
                    ("MoreDps", t.more),
                    ("GrenadeDpsFactor", t.grenade),
                    ("BarrageDpsFactor", t.barrage),
                ],
            )
        }
        "TotalMin" | "TotalMax" => {
            let bound = if suffix == "TotalMax" { "Max" } else { "Min" };
            let inputs: Vec<_> = TYPES
                .iter()
                .map(|(kind, name)| {
                    (
                        format!("{hand}{name}{bound}"),
                        p.bound(*kind, bound == "Max") * p.effective(*kind),
                    )
                })
                .collect();
            DerivedDetail {
                formula: inputs
                    .iter()
                    .map(|(id, _)| id.as_str())
                    .collect::<Vec<_>>()
                    .join(" + "),
                inputs,
                context: Vec::new(),
            }
        }
        _ => {
            let (kind, bound) = typed(suffix)?;
            if bound == "EffMult" {
                let t = p.terms.effective.iter().find(|t| t.damage_type == kind);
                detail(
                    "HitTypePresent * EnemyTakenFactor * (1 - HitReduction / 100)",
                    vec![
                        ("HitTypePresent", if p.present(kind) { 1.0 } else { 0.0 }),
                        ("EnemyTakenFactor", t.map_or(1.0, |t| t.taken)),
                        ("HitReduction", t.map_or(0.0, |t| t.reduction)),
                    ],
                )
            } else {
                detail(
                    "PreMitigationBound * HitEnemyMultiplier",
                    vec![
                        ("PreMitigationBound", p.bound(kind, bound == "Max")),
                        ("HitEnemyMultiplier", p.effective(kind)),
                    ],
                )
            }
        }
    })
}
