//! Bounded, source-independent damage of one successful hit, before target effects.
//!
//! Callers resolve source facts and actor applicability, retain mapping diagnostics,
//! and submit the existing attributed modifiers. This module neither maps game stat
//! IDs nor admits a dataset or build. It deliberately has no pool, enemy, timing or
//! runtime-constants input. Projectile counts and lifetimes are not hit multipliers.

use std::fmt;

use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb, ModTag, ModValue, Modifier, TraceGraph};

use super::crit::{ResolvedCriticalBase, resolve_conditional_crit_traced};
use super::crit_pass::calculate_hit_damage;

/// The source category used by the existing damage and modifier queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConditionalHitSource {
    Attack,
    Spell,
    Secondary,
}

/// Static skill scope. No condition, actor stat, multiplier or dynamic name channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConditionalHitContext {
    pub source: ConditionalHitSource,
    /// Only `MELEE`, `AREA` and `PROJECTILE` are admitted; no weapon/actor state.
    pub delivery_flags: ModFlags,
    /// Only damage-type keywords are admitted here. Hit and source are derived.
    pub damage_keywords: KeywordFlags,
}

impl ConditionalHitContext {
    /// An explicitly selected source with no additional delivery or type keywords.
    pub fn new(source: ConditionalHitSource) -> Self {
        Self {
            source,
            delivery_flags: ModFlags::NONE,
            damage_keywords: KeywordFlags::NONE,
        }
    }

    fn config(self) -> CalcConfig {
        let (flags, keywords, skill_types) = match self.source {
            ConditionalHitSource::Attack => {
                (ModFlags::ATTACK, KeywordFlags::ATTACK, SkillTypes::ATTACK)
            }
            ConditionalHitSource::Spell => {
                (ModFlags::SPELL, KeywordFlags::SPELL, SkillTypes::SPELL)
            }
            ConditionalHitSource::Secondary => {
                (ModFlags::NONE, KeywordFlags::NONE, SkillTypes::NONE)
            }
        };
        // CalcConfig remains the common query representation. Its default constants
        // object is never factual input to this operation: the critical base is
        // explicit, no target exists, and the validated channels cannot read it.
        CalcConfig::new()
            .with_flags(flags | self.delivery_flags | ModFlags::HIT)
            .with_keyword_flags(keywords | self.damage_keywords | KeywordFlags::HIT)
            .with_skill_types(skill_types)
    }
}

/// Inputs for version 1 of conditional hit evaluation.
///
/// `critical_base` is required at evaluation; `None` is an error, never a request
/// for a default. Intrinsic damage is supplied through the caller's owned mapping
/// as `<Type>DamageMin/Max BASE`. It must not also be supplied as an implicit base.
///
/// The closed modifier vocabulary is those five typed BASE ranges and INC for
/// generic/type/elemental/attack/spell/projectile/area/melee damage and critical
/// chance/bonus. Static flags, keywords and `SkillTypes` tags are allowed.
/// `DamageType` tags require typed BASE ranges or typed/elemental INC names;
/// generic/source damage and critical queries have no damage-type context.
/// MORE (including precision-table rules), flags, overrides, conditions,
/// dynamic tags, costs, counts and durations require another admitted contract.
#[derive(Debug, Clone, Copy)]
pub struct ConditionalSkillHitV1<'a> {
    pub context: ConditionalHitContext,
    pub critical_base: Option<ResolvedCriticalBase>,
    pub modifiers: &'a [Modifier],
}

/// One leg's range and arithmetic mean; no final aggregate rounding is applied.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConditionalHitRange {
    pub min: f64,
    pub max: f64,
    pub mean: f64,
}

/// Noncritical and critical damage of one type, before mitigation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConditionalHitDamage {
    pub damage_type: DamageType,
    pub non_critical: ConditionalHitRange,
    pub critical: ConditionalHitRange,
}

/// An original modifier and the actual shared calculator queries that saw it.
#[derive(Debug, Clone, PartialEq)]
pub struct ConditionalHitModifierEvidence {
    /// Includes the original source, stat ID, raw text and mapping rule provenance.
    pub modifier: Modifier,
    pub queries: Vec<String>,
    /// False for zero or statically inapplicable contributions; never discarded.
    pub counted: bool,
}

/// Only conditional landed-hit outputs. This is not a completed build calculation.
#[derive(Debug, Clone)]
pub struct ConditionalHitResult {
    /// Existing damage-type order, including the core's zero physical baseline.
    pub damage: Vec<ConditionalHitDamage>,
    /// Probability as a fraction, after the shared resolver's rounding.
    pub crit_probability: f64,
    pub crit_multiplier: f64,
    /// Sum of the dual pass's stored per-type combined means. Unlike legacy
    /// `total_hit_avg`, this does not round the crit effect or final sum again.
    pub conditional_weighted_mean: f64,
    /// Echoed resolved scalars; their source/policy binding belongs to the caller.
    pub critical_base: ResolvedCriticalBase,
    pub modifiers: Vec<ConditionalHitModifierEvidence>,
    /// Existing crit modifier attribution. Source binding of the two explicit
    /// base scalars is carried by the caller, not inferred from actor metadata.
    pub crit_trace: TraceGraph,
}

/// An inadmissible bounded input or nonfinite numerical result.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConditionalHitError {
    MissingCriticalBase,
    InvalidCriticalChance,
    InvalidCriticalDamageBonus,
    UnsupportedContext,
    UnsupportedModifier { index: usize },
    InvalidModifierValue { index: usize },
    UnsupportedModifierScope { index: usize },
    UnsupportedModifierTag { index: usize },
    InvalidDamageRange { damage_type: DamageType },
    NonFiniteResult,
}

impl fmt::Display for ConditionalHitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingCriticalBase => write!(f, "missing resolved critical base"),
            Self::InvalidCriticalChance => write!(f, "critical chance must be finite and in 0..=1"),
            Self::InvalidCriticalDamageBonus => {
                write!(f, "critical damage bonus must be finite and nonnegative")
            }
            Self::UnsupportedContext => write!(f, "unsupported conditional hit skill context"),
            Self::UnsupportedModifier { index } => write!(f, "unsupported hit modifier at {index}"),
            Self::InvalidModifierValue { index } => {
                write!(f, "invalid hit modifier value at {index}")
            }
            Self::UnsupportedModifierScope { index } => {
                write!(f, "unsupported hit modifier scope at {index}")
            }
            Self::UnsupportedModifierTag { index } => {
                write!(f, "unsupported hit modifier tag at {index}")
            }
            Self::InvalidDamageRange { damage_type } => {
                write!(f, "invalid {damage_type:?} hit damage range")
            }
            Self::NonFiniteResult => write!(f, "nonfinite conditional hit result"),
        }
    }
}

impl std::error::Error for ConditionalHitError {}

fn delivery_flags() -> ModFlags {
    ModFlags::MELEE | ModFlags::AREA | ModFlags::PROJECTILE
}

fn damage_keywords() -> KeywordFlags {
    KeywordFlags::PHYSICAL
        | KeywordFlags::LIGHTNING
        | KeywordFlags::COLD
        | KeywordFlags::FIRE
        | KeywordFlags::CHAOS
}

fn validate(
    input: &ConditionalSkillHitV1<'_>,
) -> Result<ResolvedCriticalBase, ConditionalHitError> {
    let base = input
        .critical_base
        .ok_or(ConditionalHitError::MissingCriticalBase)?;
    if !base.chance_fraction.is_finite() || !(0.0..=1.0).contains(&base.chance_fraction) {
        return Err(ConditionalHitError::InvalidCriticalChance);
    }
    if !base.damage_bonus_percent.is_finite() || base.damage_bonus_percent < 0.0 {
        return Err(ConditionalHitError::InvalidCriticalDamageBonus);
    }
    if !input.context.delivery_flags.is_subset_of(delivery_flags())
        || input.context.damage_keywords.bits() & !damage_keywords().bits() != 0
    {
        return Err(ConditionalHitError::UnsupportedContext);
    }
    let flags = delivery_flags() | ModFlags::ATTACK | ModFlags::SPELL | ModFlags::HIT;
    let keywords = damage_keywords()
        | KeywordFlags::ATTACK
        | KeywordFlags::SPELL
        | KeywordFlags::HIT
        | KeywordFlags::MATCH_ALL;
    for (index, modifier) in input.modifiers.iter().enumerate() {
        let admitted = match modifier.mod_type {
            ModType::Base => matches!(
                modifier.name.as_str(),
                "PhysicalDamageMin"
                    | "PhysicalDamageMax"
                    | "LightningDamageMin"
                    | "LightningDamageMax"
                    | "ColdDamageMin"
                    | "ColdDamageMax"
                    | "FireDamageMin"
                    | "FireDamageMax"
                    | "ChaosDamageMin"
                    | "ChaosDamageMax"
            ),
            ModType::Inc => matches!(
                modifier.name.as_str(),
                "Damage"
                    | "PhysicalDamage"
                    | "LightningDamage"
                    | "ColdDamage"
                    | "FireDamage"
                    | "ChaosDamage"
                    | "ElementalDamage"
                    | "AttackDamage"
                    | "SpellDamage"
                    | "ProjectileDamage"
                    | "AreaDamage"
                    | "MeleeDamage"
                    | "CriticalStrikeChance"
                    | "CriticalStrikeMultiplier"
            ),
            _ => false,
        };
        if !admitted {
            return Err(ConditionalHitError::UnsupportedModifier { index });
        }
        let ModValue::Number(value) = &modifier.value else {
            return Err(ConditionalHitError::InvalidModifierValue { index });
        };
        if !value.is_finite() || (modifier.mod_type == ModType::Base && *value < 0.0) {
            return Err(ConditionalHitError::InvalidModifierValue { index });
        }
        if !modifier.flags.is_subset_of(flags)
            || modifier.keyword_flags.bits() & !keywords.bits() != 0
        {
            return Err(ConditionalHitError::UnsupportedModifierScope { index });
        }
        for tag in &modifier.tags {
            match tag {
                ModTag::DamageType(_)
                    if modifier.mod_type == ModType::Base
                        || matches!(
                            modifier.name.as_str(),
                            "PhysicalDamage"
                                | "LightningDamage"
                                | "ColdDamage"
                                | "FireDamage"
                                | "ChaosDamage"
                                | "ElementalDamage"
                        ) => {}
                ModTag::SkillTypes(types)
                    if (SkillTypes::ATTACK | SkillTypes::SPELL).contains(*types) => {}
                _ => return Err(ConditionalHitError::UnsupportedModifierTag { index }),
            }
        }
    }
    Ok(base)
}

/// Calculate one successful hit through the same damage body as ordinary offence.
///
/// Critical inputs and the modifier vocabulary are checked before a query context
/// or database is created; resulting ranges are checked after aggregation. No
/// accuracy, target, action-rate, cost, resource, tick or full-build path is run.
/// Legacy component and crit rounding remains in those common primitives; only
/// the final conditional weighted mean uses their stored (unrounded) means.
///
/// # Errors
/// Rejects missing/invalid critical bases, unknown names or modifier kinds,
/// nonfinite numbers, unadmitted flags/tags, invalid ranges and overflow.
pub fn calculate_conditional_hit(
    input: &ConditionalSkillHitV1<'_>,
) -> Result<ConditionalHitResult, ConditionalHitError> {
    let critical_base = validate(input)?;
    let cfg = input.context.config();
    let mut db = ModDb::new();
    db.track_reads();
    for modifier in input.modifiers {
        db.add_mod(modifier.clone());
    }
    let mut crit_trace = TraceGraph::default();
    let crit = resolve_conditional_crit_traced(&db, &cfg, critical_base, &mut crit_trace);
    let (_, passes) = calculate_hit_damage(&db, &cfg, 0.0, 0.0, &crit, None, |_, _, _| {
        unreachable!("a conditional hit has no target or mitigation callback")
    });
    let mean = |values: &[(DamageType, f64)], ty| {
        values
            .iter()
            .find(|(kind, _)| *kind == ty)
            .map_or(0.0, |(_, value)| *value)
    };
    let damage: Vec<_> = passes
        .stored_ranges
        .iter()
        .map(|range| ConditionalHitDamage {
            damage_type: range.damage_type,
            non_critical: ConditionalHitRange {
                min: range.hit_min,
                max: range.hit_max,
                mean: mean(&passes.stored_hit_avg, range.damage_type),
            },
            critical: ConditionalHitRange {
                min: range.crit_min,
                max: range.crit_max,
                mean: mean(&passes.stored_crit_avg, range.damage_type),
            },
        })
        .collect();
    let conditional_weighted_mean: f64 = passes
        .stored_combined_avg
        .iter()
        .map(|(_, value)| value)
        .sum();
    if !crit.chance.is_finite()
        || !crit.multiplier.is_finite()
        || !conditional_weighted_mean.is_finite()
    {
        return Err(ConditionalHitError::NonFiniteResult);
    }
    for row in &damage {
        for range in [row.non_critical, row.critical] {
            if !range.min.is_finite() || !range.max.is_finite() || !range.mean.is_finite() {
                return Err(ConditionalHitError::NonFiniteResult);
            }
            if range.min < 0.0 || range.max < range.min {
                return Err(ConditionalHitError::InvalidDamageRange {
                    damage_type: row.damage_type,
                });
            }
        }
    }
    let reads = db.read_snapshot();
    let modifiers = input
        .modifiers
        .iter()
        .map(|modifier| ConditionalHitModifierEvidence {
            modifier: modifier.clone(),
            queries: reads.sites.get(&modifier.name).cloned().unwrap_or_default(),
            counted: reads.counted(modifier),
        })
        .collect();
    Ok(ConditionalHitResult {
        damage,
        crit_probability: crit.chance,
        crit_multiplier: crit.multiplier,
        conditional_weighted_mean,
        critical_base,
        modifiers,
        crit_trace,
    })
}

#[cfg(test)]
mod tests;
