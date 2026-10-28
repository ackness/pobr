//! Compares a calculation with reference values recorded under PoB2 output
//! names: the `<PlayerStat>` values a build code carries
//! ([`ImportReport::reference_stats`](crate::ImportReport)) or an oracle dump,
//! and its `<MinionStat>` values for the minion of the main skill
//! ([`compare_build_to_reference`]).
//!
//! The pairs come from [`display_catalog`], so a stat is comparable exactly
//! when the catalog names its PoB2 counterpart. A reference is another
//! tool's result, not the game's: a row that differs says the two tools
//! disagree, not which one is right.

use std::collections::BTreeMap;

use pobr_core::calc::{MinionOutput, OutputTable};
use pobr_core::display_catalog::{display_catalog, display_value};
use pobr_data::display_stat::ParityStatus;

/// One stat both sides report.
#[derive(Debug, Clone, PartialEq)]
pub struct ReferenceRow {
    /// Display stat id, as in [`display_catalog`].
    pub id: String,
    /// The reference's name for it.
    pub pob_key: String,
    pub pobr: f64,
    pub reference: f64,
}

impl ReferenceRow {
    /// `pobr / reference`. `None` when the reference is zero or either side is
    /// not finite, where a ratio says nothing.
    pub fn ratio(&self) -> Option<f64> {
        (self.reference != 0.0 && self.reference.is_finite() && self.pobr.is_finite())
            .then(|| self.pobr / self.reference)
    }

    /// Whether the two values agree within `tolerance` of the reference.
    /// Below 1 the tolerance is absolute, so rounding near zero is not a miss.
    pub fn agrees(&self, tolerance: f64) -> bool {
        if self.pobr == self.reference {
            return true;
        }
        self.pobr.is_finite()
            && self.reference.is_finite()
            && (self.pobr - self.reference).abs() <= tolerance * self.reference.abs().max(1.0)
    }
}

/// Catalog pairs whose two sides are defined differently, so comparing them
/// measures the definitions rather than the calculation.
const NOT_COMPARABLE: &[&str] = &[
    // PoBR's value is the hit before the enemy's mitigation; PoB2's
    // `AverageHit` includes mitigation. Neither includes chance to hit.
    "TotalHitAvg",
    // Same PoB2 key as `ActionRate`; listed once.
    "EffectiveActionRate",
];

/// PoB2's name for the rate of a skill's hits when it is not its rate of use.
const HIT_RATE_KEY: &str = "HitSpeed";

/// Every computed catalog stat the reference also reports, in catalog order.
pub fn compare_to_reference(
    output: &OutputTable,
    reference: &BTreeMap<String, f64>,
) -> Vec<ReferenceRow> {
    display_catalog()
        .into_iter()
        .filter(|def| def.parity_status == ParityStatus::Computed)
        .filter(|def| !NOT_COMPARABLE.contains(&def.id.as_str()))
        .filter(|def| {
            output.full_dps.is_some() || !matches!(def.id.as_str(), "FullDPS" | "FullDotDPS")
        })
        .filter_map(|def| {
            let mut pob_key = def.pob_key?;
            // PoB2 reports how often a skill is used as `Speed` and, where
            // its hits come at another rate, that rate as `HitSpeed`. Its DPS
            // is by the latter, like the one rate PoBR reports.
            if def.id.as_str() == "ActionRate" && reference.contains_key(HIT_RATE_KEY) {
                pob_key = HIT_RATE_KEY.to_string();
            }
            let reference = *reference.get(&pob_key)?;
            Some(ReferenceRow {
                pobr: display_value(output, def.id.as_str()),
                id: def.id.as_str().to_string(),
                pob_key,
                reference,
            })
        })
        .collect()
}

/// The minion rows: (row id, the reference's `<MinionStat>` name).
const MINION_ROWS: [(&str, &str); 17] = [
    ("MinionTotalDPS", "TotalDPS"),
    ("MinionLife", "Life"),
    ("MinionEnergyShield", "EnergyShield"),
    ("MinionArmour", "Armour"),
    ("MinionEvasion", "Evasion"),
    ("MinionFireResist", "FireResist"),
    ("MinionColdResist", "ColdResist"),
    ("MinionLightningResist", "LightningResist"),
    ("MinionChaosResist", "ChaosResist"),
    ("MinionFireResistMax", "FireResistMax"),
    ("MinionColdResistMax", "ColdResistMax"),
    ("MinionLightningResistMax", "LightningResistMax"),
    ("MinionChaosResistMax", "ChaosResistMax"),
    ("MinionFireResistOverCap", "FireResistOverCap"),
    ("MinionColdResistOverCap", "ColdResistOverCap"),
    ("MinionLightningResistOverCap", "LightningResistOverCap"),
    ("MinionChaosResistOverCap", "ChaosResistOverCap"),
];

fn minion_value(minion: &MinionOutput, key: &str) -> f64 {
    match key {
        "TotalDPS" => minion.dps,
        "Life" => minion.life,
        "EnergyShield" => minion.energy_shield,
        "Armour" => minion.armour,
        "Evasion" => minion.evasion,
        _ => minion.stats.get(key).copied().unwrap_or(0.0),
    }
}

/// The `<MinionStat>` values PoBR reports for `minion`, the minion of the
/// main skill. Without one every value is compared with 0: the reference
/// has a minion the calculation did not create.
pub fn compare_minion_to_reference(
    minion: Option<&MinionOutput>,
    reference: &BTreeMap<String, f64>,
) -> Vec<ReferenceRow> {
    MINION_ROWS
        .iter()
        .filter_map(|(id, key)| {
            Some(ReferenceRow {
                id: (*id).to_string(),
                pob_key: format!("Minion:{key}"),
                pobr: minion.map_or(0.0, |minion| minion_value(minion, key)),
                reference: *reference.get(*key)?,
            })
        })
        .collect()
}

/// The minion of the main skill, whose outputs a build code's
/// `<MinionStat>` values describe: the monster its gem chose, if it chose
/// one, otherwise the first minion the skill summoned.
pub fn main_skill_minion<'a>(
    build: &crate::Build,
    data: &crate::BuildData,
    output: &'a OutputTable,
) -> Option<&'a MinionOutput> {
    if output.minions.is_empty() {
        return None;
    }
    let prepared = crate::calc_orchestrator::prepared_build_view(build, data, &Default::default());
    main_skill_minion_in_view(&prepared, data, output)
}

/// [`main_skill_minion`] for the view
/// [`crate::calc_orchestrator::calculate_with_data_session_prepared`] returned: the
/// minion is found in the view the calculation used, so the answer follows the
/// calculation's own options, and the build is not prepared again.
pub fn main_skill_minion_in_view<'a>(
    prepared: &crate::Build,
    data: &crate::BuildData,
    output: &'a OutputTable,
) -> Option<&'a MinionOutput> {
    if output.minions.is_empty() {
        return None;
    }
    let (group_index, ordinal, skill_id) =
        crate::calc_orchestrator::main_skill_selection_in_view(prepared, data)?;
    let group = prepared.socket_groups.get(group_index)?;
    if !group.enabled {
        return None;
    }
    let effect = pobr_core::skill_env::group_active_effects(
        group.gem_skills.iter().map(|gem| {
            (
                gem.skill_id.as_str(),
                gem.gem_level,
                gem.quality,
                gem.stat_set_index,
            )
        }),
        data,
    )
    .nth(ordinal.checked_sub(1)?)?;
    let chosen = group
        .gem_skills
        .get(effect.gem_index)
        .and_then(|gem| gem.minion.as_deref());
    let origin = (
        prepared.socket_groups[..group_index]
            .iter()
            .filter(|group| group.enabled)
            .count(),
        effect.gem_index,
    );
    let summoned = |minion: &&MinionOutput| {
        minion.summoned_by == skill_id && minion.summoner.is_none_or(|summoner| summoner == origin)
    };
    chosen
        .and_then(|id| {
            output
                .minions
                .iter()
                .filter(summoned)
                .find(|minion| minion.minion_id == id)
        })
        .or_else(|| output.minions.iter().find(summoned))
}

/// Every reference value of `report` the calculation reports: the
/// character's, then those of the main skill's minion.
pub fn compare_build_to_reference(
    build: &crate::Build,
    data: &crate::BuildData,
    output: &OutputTable,
    report: &crate::ImportReport,
) -> Vec<ReferenceRow> {
    let mut rows = compare_to_reference(output, &report.reference_stats);
    rows.extend(compare_minion_to_reference(
        main_skill_minion(build, data, output),
        &report.minion_reference_stats,
    ));
    rows
}

/// The row that says whether a build's damage agrees: its minion's
/// `TotalDPS` when the reference gives the minion damage, otherwise the
/// character's.
pub fn damage_row(rows: &[ReferenceRow]) -> Option<&ReferenceRow> {
    rows.iter()
        .find(|row| row.id == "MinionTotalDPS" && row.reference != 0.0)
        .or_else(|| rows.iter().find(|row| row.id == "TotalDPS"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(pobr: f64, reference: f64) -> ReferenceRow {
        ReferenceRow {
            id: "Life".into(),
            pob_key: "Life".into(),
            pobr,
            reference,
        }
    }

    #[test]
    fn agreement_is_relative_above_one_and_absolute_below() {
        assert!(row(1823.85, 1824.0).agrees(0.05));
        assert!(!row(6418.5, 12818.0).agrees(0.05));
        assert!(row(0.0, 0.04).agrees(0.05));
        assert!(!row(0.0, 0.5).agrees(0.05));
        assert!(row(f64::INFINITY, f64::INFINITY).agrees(0.05));
        assert!(!row(100.0, f64::INFINITY).agrees(0.05));
    }

    /// A build whose main skill is a minion's is judged by the minion's
    /// damage; without a minion its values are compared with 0.
    #[test]
    fn the_minion_of_the_main_skill_is_compared_with_the_minion_stats() {
        let reference = BTreeMap::from([
            ("TotalDPS".to_string(), 1000.0),
            ("Life".to_string(), 4000.0),
            ("Speed".to_string(), 1.2),
        ]);
        let minion = MinionOutput {
            dps: 990.0,
            life: 2000.0,
            ..Default::default()
        };
        let rows = compare_minion_to_reference(Some(&minion), &reference);
        assert_eq!(
            rows.iter()
                .map(|row| (row.id.as_str(), row.pob_key.as_str(), row.pobr))
                .collect::<Vec<_>>(),
            [
                ("MinionTotalDPS", "Minion:TotalDPS", 990.0),
                ("MinionLife", "Minion:Life", 2000.0)
            ]
        );
        assert!(
            compare_minion_to_reference(None, &reference)
                .iter()
                .all(|row| row.pobr == 0.0)
        );

        let mut all = vec![ReferenceRow {
            id: "TotalDPS".into(),
            pob_key: "TotalDPS".into(),
            pobr: 0.0,
            reference: 0.0,
        }];
        assert_eq!(
            damage_row(&all).map(|row| row.id.as_str()),
            Some("TotalDPS")
        );
        all.extend(rows);
        assert_eq!(
            damage_row(&all).map(|row| row.id.as_str()),
            Some("MinionTotalDPS")
        );
    }

    #[test]
    fn ratio_is_absent_where_it_says_nothing() {
        assert_eq!(row(50.0, 100.0).ratio(), Some(0.5));
        assert_eq!(row(5.0, 0.0).ratio(), None);
        assert_eq!(row(5.0, f64::INFINITY).ratio(), None);
    }

    #[test]
    fn only_stats_the_reference_reports_are_compared() {
        let output = OutputTable {
            life: 1000.0,
            mana: 500.0,
            ..Default::default()
        };
        let reference = BTreeMap::from([("Life".to_string(), 1010.0)]);
        let rows = compare_to_reference(&output, &reference);
        assert_eq!(rows, [row(1000.0, 1010.0)]);
    }

    /// A skill used once a second whose hits come four times a second: the
    /// reference's DPS is by its `HitSpeed`, so that is what PoBR's rate is
    /// compared with.
    #[test]
    fn the_action_rate_is_compared_with_the_rate_of_hits_when_the_reference_has_one() {
        let output = OutputTable {
            action_rate: 4.0,
            ..Default::default()
        };
        let rate = |reference: &[(&str, f64)]| {
            let reference = reference
                .iter()
                .map(|(key, value)| (key.to_string(), *value))
                .collect();
            let rows = compare_to_reference(&output, &reference);
            let row = rows
                .iter()
                .find(|row| row.id == "ActionRate")
                .expect("the rate");
            (row.pob_key.clone(), row.reference)
        };
        assert_eq!(rate(&[("Speed", 1.0)]), ("Speed".to_string(), 1.0));
        assert_eq!(
            rate(&[("Speed", 1.0), ("HitSpeed", 3.96)]),
            ("HitSpeed".to_string(), 3.96)
        );
    }
}
