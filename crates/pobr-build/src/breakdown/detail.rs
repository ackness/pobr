//! Descriptor catalog and opt-in explanations, shared by API consumers.
use super::{BREAKDOWN_STATS, BreakdownMod, StatBreakdown, mod_entries, stat_breakdown};
use pobr_core::calc::CalculationSession;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DetailKind {
    Steps,
    Sources,
    Derived,
    None,
}

/// A descriptor exists for every catalog id, including explicit `none` entries.
/// The legacy list is only the default request, not the supported surface.
pub fn detail_kind(stat: &str) -> DetailKind {
    if pobr_core::calc::utility::item_stat(stat).is_some() {
        return DetailKind::Derived;
    }
    if BREAKDOWN_STATS.contains(&stat) {
        return DetailKind::Steps;
    }
    if pobr_core::calc::session::detail::DERIVED_STATS.contains(&stat) {
        return DetailKind::Derived;
    }
    match stat {
        // Enemy assembly and ES bypass retain resolved values, not their
        // contributing modifier/override trace. Do not invent additive sources.
        "PhysicalEnemyDamage"
        | "FireEnemyDamage"
        | "ColdEnemyDamage"
        | "LightningEnemyDamage"
        | "ChaosEnemyDamage"
        | "PhysicalEnergyShieldBypass"
        | "FireEnergyShieldBypass"
        | "ColdEnergyShieldBypass"
        | "LightningEnergyShieldBypass"
        | "ChaosEnergyShieldBypass" => DetailKind::None,
        "SkillTriggerRate"
        | "TriggerRateCap"
        | "LifeUnreservedPercent"
        | "ManaUnreservedPercent"
        | "SpiritUnreservedPercent"
        | "LifeLeechGainRate"
        | "ManaLeechGainRate"
        | "EnergyShieldLeechGainRate"
        | "LifeLeechGainPerHit"
        | "ManaLeechGainPerHit"
        | "EnergyShieldLeechGainPerHit"
        | "EsRechargePerSecond"
        | "Duration"
        | "DurationSecondary"
        | "AuraDuration"
        | "ReserveDuration"
        | "ReqStr"
        | "ReqDex"
        | "ReqInt"
        | "FireResistOverCap"
        | "ColdResistOverCap"
        | "LightningResistOverCap"
        | "ChaosResistOverCap"
        | "LifeUnreserved"
        | "ManaUnreserved"
        | "SpiritUnreserved"
        | "TotalNetRegen"
        | "NetLifeRegen"
        | "NetManaRegen"
        | "NetEnergyShieldRegen"
        | "NetWardRegen"
        | "TotalBuildDegen"
        | "ManaPerSecondCost"
        | "ManaPercentPerSecondCost"
        | "LifePerSecondCost"
        | "LifePercentPerSecondCost"
        | "ESPerSecondCost"
        | "ESPercentPerSecondCost"
        | "WardPerSecondCost"
        | "WardPercentPerSecondCost"
        | "RagePerSecondCost"
        | "AreaOfEffectRadiusMetres"
        | "TotalEHP" => DetailKind::Derived,
        _ if !pobr_core::calc::session::detail::source_names(stat).is_empty() => {
            DetailKind::Sources
        }
        _ => DetailKind::None,
    }
}

pub fn detail_descriptors() -> BTreeMap<String, DetailKind> {
    pobr_core::display_catalog()
        .into_iter()
        .map(|stat| {
            let kind = detail_kind(stat.id.as_str());
            (stat.id.as_str().to_string(), kind)
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceDetail {
    pub name: String,
    /// None: nearest two decimals; Some(p): floor at p decimals.
    pub more_precision: Option<u32>,
    pub base_total: f64,
    pub inc_total: f64,
    pub more_total: f64,
    pub mods: Vec<BreakdownMod>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DetailInput {
    pub id: String,
    pub value: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct DerivedDetail {
    pub formula: String,
    pub inputs: Vec<DetailInput>,
    pub context: Vec<DetailInput>,
}
#[derive(Debug, Clone, Serialize)]
pub struct StatDetail {
    pub kind: DetailKind,
    #[serde(flatten)]
    pub breakdown: StatBreakdown,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<SourceDetail>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub derived: Option<DerivedDetail>,
}

pub fn stat_detail(session: &CalculationSession, stat: &str) -> Option<StatDetail> {
    let kind = detail_kind(stat);
    if kind == DetailKind::None {
        return None;
    }
    let empty = || StatBreakdown {
        actor: super::BreakdownActor::Player,
        base_total: 0.0,
        inc_total: 0.0,
        more_total: 1.0,
        mods: vec![],
        formula: None,
        damage: None,
        mitigation: None,
        skill_dot: None,
        ailment: None,
        reload: None,
        slots: vec![],
        hands: vec![],
    };
    let breakdown = if kind == DetailKind::Steps {
        stat_breakdown(session, stat)?
    } else {
        empty()
    };
    let sources = if kind == DetailKind::Sources {
        session
            .detail_sources(stat)
            .into_iter()
            .filter(|group| !group.rows.is_empty())
            .map(|group| SourceDetail {
                name: group.name,
                more_precision: group.more_precision,
                base_total: group.totals.0 + 0.0,
                inc_total: group.totals.1 + 0.0,
                more_total: group.totals.2,
                mods: mod_entries(&group.rows),
            })
            .collect()
    } else {
        vec![]
    };
    let derived = if kind == DetailKind::Derived {
        let d = session.derived_detail(stat)?;
        let inputs = |rows: Vec<(String, f64)>| {
            rows.into_iter()
                .map(|(id, value)| DetailInput { id, value })
                .collect()
        };
        Some(DerivedDetail {
            formula: d.formula,
            inputs: inputs(d.inputs),
            context: inputs(d.context),
        })
    } else {
        None
    };
    Some(StatDetail {
        kind,
        breakdown,
        sources,
        derived,
    })
}

/// Minions retain their own smaller supported surface and actor identity.
pub fn minion_detail_descriptors() -> BTreeMap<String, DetailKind> {
    [
        "Life",
        "Armour",
        "Evasion",
        "EnergyShield",
        "TotalDPS",
        "TotalHitAvg",
        "ActionRate",
        "HitChance",
        "CritChance",
        "CritMultiplier",
        "Accuracy",
    ]
    .into_iter()
    .map(|id| {
        (
            id.into(),
            if matches!(id, "Armour" | "Evasion" | "EnergyShield" | "Accuracy") {
                DetailKind::Sources
            } else {
                DetailKind::Steps
            },
        )
    })
    .chain(
        pobr_core::calc::minion::MINION_RESISTANCE_STATS
            .into_iter()
            .map(|id| {
                (
                    id.into(),
                    if id.ends_with("Resist") {
                        DetailKind::Steps
                    } else {
                        DetailKind::None
                    },
                )
            }),
    )
    .collect()
}

pub fn minion_stat_detail(
    session: &CalculationSession,
    index: usize,
    stat: &str,
) -> Option<StatDetail> {
    let kind = *minion_detail_descriptors().get(stat)?;
    if kind == DetailKind::None {
        return None;
    }
    let breakdown = super::minion_stat_breakdown(session, index, stat)?;
    Some(StatDetail {
        kind,
        breakdown,
        sources: vec![],
        derived: None,
    })
}
