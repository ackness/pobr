//! `calculate-build --breakdown`: how the calculation arrives at the stats
//! asked for. The report carries them as [`StatBreakdownEntry`] values
//! ([`pobr_build::breakdown`], the same shape as the web contract's
//! breakdowns); [`render_breakdowns`] prints them as text.

use std::cmp::Ordering;
use std::fmt::Write as _;

use pobr_build::ReferenceRow;
use pobr_build::breakdown::{
    AilmentBreakdown, BREAKDOWN_STATS, BreakdownFormula, BreakdownMod, HitDamageTable,
    HitMitigationRow, SkillDotRow, StatBreakdown, minion_stat_breakdown, stat_breakdown,
};
use pobr_core::calc::{CalculationSession, MinionOutput};
use pobr_core::display_catalog::{display_catalog, display_value};
use pobr_data::display_stat::ParityStatus;
use serde::Serialize;

use crate::CalculateBuildReport;

/// What `calculate-build` reports beyond its default output.
#[derive(Debug, Clone, Default)]
pub struct BuildDetail {
    /// Evaluate all included effects, exposing FullDPS and its contributions.
    pub full_dps: bool,
    /// The stats to break down, in the order wanted: display stat ids such
    /// as `TotalDPS` or `Life`, `all` for the panel's set
    /// ([`BREAKDOWN_STATS`]), or modifier names, which are aggregated under
    /// their own name. Empty: no breakdowns.
    pub breakdown: Vec<String>,
    /// Calculate on the basis of the reference values the code carries
    /// ([`pobr_build::DataOrchestratorOptions::on_reference_basis`]): the
    /// models their exporter lacks are off for the whole report, so the
    /// output and the breakdowns answer the question the reference, and
    /// PoB2, answers.
    pub reference_basis: bool,
    /// Report what the calculation did with each of the build's lines
    /// ([`pobr_build::usage::line_usage`]).
    pub usage: bool,
}

/// One stat asked for with `--breakdown`.
#[derive(Debug, Clone, Serialize)]
pub struct StatBreakdownEntry {
    /// The name as asked for.
    pub stat: String,
    /// The displayed value, when `stat` is a computed display stat.
    pub value: Option<f64>,
    /// The exporting tool's (PoB2's) output name for the stat.
    pub pob_key: Option<String>,
    /// The value the build code carries under `pob_key`. Present only when
    /// the report compares with the code's values
    /// ([`CalculateBuildReport::reference`]) and the two are comparable.
    pub reference: Option<f64>,
    /// `None` when the calculation has neither modifier lines nor steps for
    /// the stat (or it is a damaging ailment the skill does not apply).
    pub breakdown: Option<StatBreakdown>,
}

/// The names `--breakdown` asked for, `all` expanded, each once.
pub(crate) fn requested_stats(asked: &[String]) -> Vec<String> {
    let mut stats: Vec<String> = Vec::new();
    for name in asked.iter().map(|name| name.trim()) {
        let names: Vec<&str> = if name == "all" {
            BREAKDOWN_STATS.to_vec()
        } else {
            vec![name]
        };
        for name in names {
            if !name.is_empty() && !stats.iter().any(|stat| stat == name) {
                stats.push(name.to_string());
            }
        }
    }
    stats
}

/// The prefix of a stat of the main skill's minion (`Minion:Life`).
const MINION_PREFIX: &str = "Minion:";

/// The value a minion reports for `stat`.
fn minion_value(minion: &MinionOutput, stat: &str) -> Option<f64> {
    match stat {
        "TotalDPS" => Some(minion.dps),
        "Life" => Some(minion.life),
        "Armour" => Some(minion.armour),
        "Evasion" => Some(minion.evasion),
        "EnergyShield" => Some(minion.energy_shield),
        _ => minion.stats.get(stat).copied(),
    }
}

/// The breakdown of each stat in `stats` from the finished `session`.
/// `rows` is the comparison with the build code's values, when the report
/// makes it. A stat named `Minion:<stat>` is of `minion`, the main skill's
/// minion and its position among the minions the calculation created.
pub(crate) fn breakdown_entries(
    session: &CalculationSession,
    stats: &[String],
    rows: &[ReferenceRow],
    minion: Option<(usize, &MinionOutput)>,
) -> Vec<StatBreakdownEntry> {
    let output = session.output();
    let catalog = display_catalog();
    stats
        .iter()
        .map(|stat| {
            if let Some(name) = stat.strip_prefix(MINION_PREFIX) {
                let row = rows.iter().find(|row| row.pob_key == *stat);
                return StatBreakdownEntry {
                    stat: stat.clone(),
                    value: minion.and_then(|(_, minion)| minion_value(minion, name)),
                    pob_key: Some(stat.clone()),
                    reference: row.map(|row| row.reference),
                    breakdown: minion
                        .and_then(|(index, _)| minion_stat_breakdown(session, index, name)),
                };
            }
            let def = catalog.iter().find(|def| {
                def.id.as_str() == stat.as_str() && def.parity_status == ParityStatus::Computed
            });
            // The comparison picks the reference's own name where it has two
            // (a rate of hits beside the rate of use).
            let row = rows.iter().find(|row| row.id == *stat);
            StatBreakdownEntry {
                stat: stat.clone(),
                value: def.map(|def| display_value(output, def.id.as_str())),
                pob_key: row
                    .map(|row| row.pob_key.clone())
                    .or_else(|| def.and_then(|def| def.pob_key.clone())),
                reference: row.map(|row| row.reference),
                breakdown: stat_breakdown(session, stat),
            }
        })
        .collect()
}

/// Compact number: integers as such, otherwise up to four decimals (one
/// from a thousand up), trailing zeros dropped.
pub(crate) fn num(n: f64) -> String {
    if !n.is_finite() {
        return format!("{n}");
    }
    let text = if n.abs() >= 1000.0 {
        format!("{n:.1}")
    } else {
        format!("{n:.4}")
    };
    let text = if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        text
    };
    if text == "-0" { "0".to_string() } else { text }
}

/// A number with its sign: `+12`, `-3`.
fn signed(n: f64) -> String {
    if n >= 0.0 {
        format!("+{}", num(n))
    } else {
        num(n)
    }
}

fn type_rank(mod_type: &str) -> u8 {
    match mod_type {
        "BASE" => 0,
        "INC" => 1,
        "MORE" => 2,
        "OVERRIDE" => 3,
        "FLAG" => 4,
        _ => 5,
    }
}

/// `kind:id [slot]`, or `-` without an origin.
fn origin(line: &BreakdownMod) -> String {
    let mut text = match (&line.origin_kind, &line.origin_id) {
        (Some(kind), Some(id)) => format!("{kind}:{id}"),
        (Some(kind), None) => kind.clone(),
        (None, Some(id)) => id.clone(),
        (None, None) => "-".to_string(),
    };
    if let Some(slot) = &line.slot {
        let _ = write!(text, " [{slot}]");
    }
    text
}

/// Modifier lines sorted by type, then value (largest first), the ones
/// that apply before the ones that do not; those are marked `inactive`.
fn write_mods(s: &mut String, pad: &str, title: &str, lines: &[BreakdownMod]) {
    if lines.is_empty() {
        return;
    }
    let inactive = lines.iter().filter(|line| !line.active).count();
    let _ = if inactive > 0 {
        writeln!(s, "{pad}{title} ({}, {inactive} inactive):", lines.len())
    } else {
        writeln!(s, "{pad}{title} ({}):", lines.len())
    };
    let mut sorted: Vec<&BreakdownMod> = lines.iter().collect();
    sorted.sort_by(|a, b| {
        type_rank(a.mod_type)
            .cmp(&type_rank(b.mod_type))
            .then_with(|| match (a.value, b.value) {
                (Some(a), Some(b)) => b.partial_cmp(&a).unwrap_or(Ordering::Equal),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            })
            .then_with(|| b.active.cmp(&a.active))
            .then_with(|| origin(a).cmp(&origin(b)))
    });
    for line in sorted {
        let value = line.value.map_or_else(|| "-".to_string(), signed);
        let state = if line.active { "" } else { "inactive" };
        let text = line.source_text.as_deref().unwrap_or("");
        let _ = writeln!(
            s,
            "{pad}  {:<8} {value:>12} {state:<8} {}  {text}",
            line.mod_type,
            origin(line),
        );
    }
}

fn write_formula(s: &mut String, pad: &str, formula: Option<&BreakdownFormula>) {
    let Some(formula) = formula else {
        let _ = writeln!(s, "{pad}steps: not available for this stat");
        return;
    };
    let _ = writeln!(s, "{pad}steps (ending at {}):", num(formula.value));
    for step in &formula.steps {
        let _ = writeln!(
            s,
            "{pad}  {:<22} {:>14}  -> {}",
            step.op,
            num(step.operand),
            num(step.result)
        );
    }
}

fn write_damage(s: &mut String, pad: &str, table: &HitDamageTable) {
    let _ = writeln!(
        s,
        "{pad}hit damage by type (non-critical, before the enemy; damage effect x{}):",
        num(table.damage_effect)
    );
    let deeper = format!("{pad}    ");
    for row in &table.rows {
        let _ = writeln!(
            s,
            "{pad}  {}: base {}-{} · increased {} · more x{} (min x{}, max x{}) -> {}-{} (avg {}, crit avg {})",
            row.damage_type,
            num(row.base_min),
            num(row.base_max),
            signed(row.increased),
            num(row.more),
            num(row.more_min),
            num(row.more_max),
            num(row.min),
            num(row.max),
            num(row.avg),
            num(row.crit_avg)
        );
        write_mods(s, &deeper, "lines of this type", &row.mods);
        write_mods(s, &deeper, "lines of the minimum", &row.min_mods);
        write_mods(s, &deeper, "lines of the maximum", &row.max_mods);
    }
    write_mods(
        s,
        &format!("{pad}  "),
        "lines every type reads",
        &table.shared_mods,
    );
    if table.sources.is_empty() {
        return;
    }
    let _ = writeln!(s, "{pad}where the hit starts (before conversion and gain):");
    for source in &table.sources {
        let _ = writeln!(
            s,
            "{pad}  {}: {}-{} (added {}-{} x effect {})",
            source.damage_type,
            num(source.min),
            num(source.max),
            num(source.added_min),
            num(source.added_max),
            num(source.added_effect)
        );
        write_mods(s, &deeper, "added minimum", &source.added_min_mods);
        write_mods(s, &deeper, "added maximum", &source.added_max_mods);
        write_mods(s, &deeper, "added damage effect", &source.added_effect_mods);
        write_mods(s, &deeper, "conversion", &source.convert_mods);
        write_mods(s, &deeper, "gained as another type", &source.gain_mods);
    }
}

fn write_mitigation(s: &mut String, pad: &str, rows: &[HitMitigationRow]) {
    let _ = writeln!(s, "{pad}against the enemy, by type:");
    let deeper = format!("{pad}    ");
    for row in rows {
        let resist = row
            .resist
            .map_or_else(String::new, |resist| format!("resist {} · ", num(resist)));
        let _ = writeln!(
            s,
            "{pad}  {}: average {} · {resist}reduction {}% · taken x{} -> x{} = {}{}",
            row.damage_type,
            num(row.average),
            num(row.reduction),
            num(row.taken),
            num(row.multiplier),
            num(row.mitigated),
            if row.resist_inverted != 0.0 {
                format!(
                    " ({}% of hits invert the resistance)",
                    num(row.resist_inverted)
                )
            } else {
                String::new()
            }
        );
        write_mods(s, &deeper, "enemy defence lines", &row.defence_mods);
        write_mods(s, &deeper, "enemy damage taken lines", &row.taken_mods);
    }
}

fn write_skill_dot(s: &mut String, pad: &str, rows: &[SkillDotRow]) {
    let _ = writeln!(s, "{pad}the skill's damage over time by type:");
    let deeper = format!("{pad}    ");
    for row in rows {
        let _ = writeln!(
            s,
            "{pad}  {}: base {} · increased {} · more x{} · dot multiplier {} · aura x{} · enemy x{} = {}",
            row.damage_type,
            num(row.base),
            signed(row.increased),
            num(row.more),
            signed(row.dot_multiplier),
            num(row.aura),
            num(row.enemy),
            num(row.total)
        );
        write_mods(s, &deeper, "lines", &row.mods);
    }
}

fn write_ailment(s: &mut String, pad: &str, ailment: &AilmentBreakdown) {
    let _ = writeln!(
        s,
        "{pad}applied by a hit of {} (crit {}) at roll {} · {}% of instances from crits · chance {}% · hit chance {}%",
        num(ailment.source_hit),
        num(ailment.source_crit),
        num(ailment.damage_roll),
        num(ailment.crit_share),
        num(ailment.apply_chance),
        num(ailment.hit_chance)
    );
    let _ = writeln!(
        s,
        "{pad}lasts {}s (base {}s, enemy x{}, faster /{}) · {} hits/s · {} instances of at most {}",
        num(ailment.duration),
        num(ailment.base_duration),
        num(ailment.enemy_duration),
        num(ailment.rate),
        num(ailment.hits_per_second),
        num(ailment.stacks),
        num(ailment.max_stacks)
    );
    if !ailment.source.is_empty() {
        let parts: Vec<String> = ailment
            .source
            .iter()
            .map(|row| {
                format!(
                    "{} {} (crit {})",
                    row.damage_type,
                    num(row.hit),
                    num(row.crit)
                )
            })
            .collect();
        let _ = writeln!(s, "{pad}source by type: {}", parts.join(" · "));
    }
    write_mods(s, pad, "chance lines", &ailment.chance_mods);
    write_mods(s, pad, "duration lines", &ailment.duration_mods);
    write_mods(s, pad, "effect lines", &ailment.effect_mods);
    write_mods(s, pad, "rate lines", &ailment.rate_mods);
}

fn write_breakdown(s: &mut String, pad: &str, breakdown: &StatBreakdown) {
    let _ = writeln!(
        s,
        "{pad}totals of the lines that apply: BASE {} · INC {} · MORE x{}",
        num(breakdown.base_total),
        signed(breakdown.inc_total),
        num(breakdown.more_total)
    );
    write_formula(s, pad, breakdown.formula.as_ref());
    if let Some(reload) = &breakdown.reload {
        let bolts = reload.bolts.map_or_else(
            || "no bolt used".to_string(),
            |bolts| format!("{} bolts", num(bolts)),
        );
        let _ = writeln!(
            s,
            "{pad}reload: {}/s while firing · {bolts} · fire {}s + reload {}s -> {}/s",
            num(reload.firing_rate),
            num(reload.firing_time),
            num(reload.reload_time),
            num(reload.rate)
        );
    }
    for slot in &breakdown.slots {
        let _ = writeln!(
            s,
            "{pad}slot {}: base {} · increased {} · more x{} -> +{}",
            slot.slot,
            num(slot.base),
            signed(slot.increased),
            num(slot.more),
            num(slot.extra)
        );
    }
    if let Some(table) = &breakdown.damage {
        write_damage(s, pad, table);
    }
    if let Some(rows) = &breakdown.mitigation {
        write_mitigation(s, pad, rows);
    }
    if let Some(rows) = &breakdown.skill_dot {
        write_skill_dot(s, pad, rows);
    }
    if let Some(ailment) = &breakdown.ailment {
        write_ailment(s, pad, ailment);
    }
    write_mods(s, pad, "modifier lines", &breakdown.mods);
    for hand in &breakdown.hands {
        let _ = writeln!(s, "{pad}-- {} --", hand.hand);
        write_breakdown(s, &format!("{pad}   "), &hand.detail);
    }
}

/// Renders the breakdowns of a [`CalculateBuildReport`] as text: the main
/// skill and its hit by damage type, then one block per stat with the steps
/// from its starting value and the modifier lines its stage reads, sorted by
/// type and value. Lines whose condition, skill type or weapon is not met
/// are marked `inactive`. With the ledger of lines, the lines the
/// calculation did not fully use follow.
pub fn render_breakdowns(report: &CalculateBuildReport) -> String {
    let mut s = String::new();
    let b = &report.build;
    let _ = writeln!(
        s,
        "build: Lv{} {}/{} · data {}{}",
        b.level,
        b.class_name,
        b.ascendancy_name,
        report.data_version,
        if report.reference_basis {
            " · on the reference's basis"
        } else {
            ""
        }
    );
    if !report.missing_items.is_empty() {
        let _ = writeln!(
            s,
            "{} item(s) could not be read; the result is without them",
            report.missing_items.len()
        );
    }
    if !report.unsupported_modifiers.is_empty() && report.usage.is_none() {
        let _ = writeln!(
            s,
            "{} modifier line(s) not understood (--usage lists them with their sources)",
            report.unsupported_modifiers.len()
        );
    }
    if let Some(reference) = &report.reference
        && (reference.at_own_rate_of_use
            || reference.rain_hits_whenever_allowed
            || reference.periodic_hits_assume_sustained)
    {
        let _ = writeln!(
            s,
            "the code's reference counts the main skill on its exporter's basis; \
             --reference-basis calculates on it"
        );
    }
    if let Some(main) = &report.main_skill {
        let _ = writeln!(
            s,
            "main skill: {} (socket group index {}, active skill {})",
            main.skill_id, main.group_index, main.active_skill_index
        );
        let parts: Vec<String> = main
            .hit_damage
            .iter()
            .map(|part| {
                format!(
                    "{} {}-{} (avg {})",
                    part.damage_type,
                    num(part.min),
                    num(part.max),
                    num(part.avg)
                )
            })
            .collect();
        if !parts.is_empty() {
            let _ = writeln!(
                s,
                "  hit before the enemy, non-critical: {}",
                parts.join(" · ")
            );
        }
        let _ = writeln!(
            s,
            "  hit DPS {} · damage over time DPS {} · combined DPS {}",
            num(main.hit_dps),
            num(main.dot_dps),
            num(main.combined_dps)
        );
    }
    for entry in &report.breakdowns {
        let _ = writeln!(s);
        let mut header = format!("== {}", entry.stat);
        match entry.value {
            Some(value) => {
                let _ = write!(header, " = {}", num(value));
            }
            None => header.push_str(" (aggregated under its own name)"),
        }
        if let Some(key) = &entry.pob_key {
            let _ = write!(header, " · PoB2 {key}");
        }
        if let Some(reference) = entry.reference {
            let _ = write!(header, " · code reference {}", num(reference));
            if let Some(value) = entry.value
                && reference != 0.0
            {
                let _ = write!(header, " (ratio {})", num(value / reference));
            }
        }
        let _ = writeln!(s, "{header}");
        match &entry.breakdown {
            Some(breakdown) => write_breakdown(&mut s, "  ", breakdown),
            None => {
                let _ = writeln!(s, "  no modifier lines or steps for this stat");
            }
        }
    }
    if let Some(usage) = &report.usage {
        let _ = writeln!(s);
        crate::usage::write_usage(&mut s, usage);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_compact() {
        assert_eq!(num(12.0), "12");
        assert_eq!(num(1.45), "1.45");
        assert_eq!(num(0.012345), "0.0123");
        assert_eq!(num(123456.789), "123456.8");
        assert_eq!(num(-0.0), "0");
        assert_eq!(num(f64::INFINITY), "inf");
        assert_eq!(signed(30.0), "+30");
        assert_eq!(signed(-4.0), "-4");
    }

    #[test]
    fn all_expands_to_the_panel_stats_once() {
        let asked = vec![
            "Life".to_string(),
            "all".to_string(),
            " Damage ".to_string(),
        ];
        let stats = requested_stats(&asked);
        assert_eq!(stats[0], "Life");
        assert_eq!(stats.iter().filter(|stat| *stat == "Life").count(), 1);
        assert_eq!(stats.last().map(String::as_str), Some("Damage"));
        assert_eq!(stats.len(), BREAKDOWN_STATS.len() + 1);
    }

    #[test]
    fn lines_are_sorted_by_type_then_value_and_inactive_ones_marked() {
        let line = |mod_type: &'static str, value: f64, active: bool, text: &str| BreakdownMod {
            mod_type,
            value: Some(value),
            active,
            source_text: Some(text.to_string()),
            origin_kind: Some("PassiveNode".to_string()),
            origin_id: Some("node.1".to_string()),
            slot: None,
        };
        let mut s = String::new();
        write_mods(
            &mut s,
            "",
            "lines",
            &[
                line("MORE", 30.0, true, "30% more"),
                line("INC", 10.0, false, "10% increased while away"),
                line("INC", 25.0, true, "25% increased"),
                line("BASE", 5.0, true, "+5"),
            ],
        );
        let order: Vec<&str> = s
            .lines()
            .skip(1)
            .map(|line| line.rsplit("  ").next().unwrap())
            .collect();
        assert_eq!(
            order,
            [
                "+5",
                "25% increased",
                "10% increased while away",
                "30% more"
            ]
        );
        assert!(s.starts_with("lines (4, 1 inactive):"));
        let away = s.lines().find(|line| line.contains("while away")).unwrap();
        assert!(away.contains("inactive"), "{away}");
        assert!(away.contains("PassiveNode:node.1"), "{away}");
    }
}
