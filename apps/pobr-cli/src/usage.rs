//! `calculate-build --usage` as text: the lines of a build the calculation
//! did not fully use, by what became of them. The JSON report
//! (`--usage --json`) also lists every applied line and the stats it reaches.

use std::fmt::Write as _;

use pobr_build::usage::{
    LineStatus, LineUsage, ModifierStatus, UsageLine, UsageModifier, UsageRef, UsageValue,
};

/// Appends the text of `usage` to `s`.
pub(crate) fn write_usage(s: &mut String, usage: &LineUsage) {
    let n = &usage.summary;
    let _ = writeln!(
        s,
        "== line usage{}",
        usage
            .main_skill
            .as_deref()
            .map(|skill| format!(" (reasons in the context of {skill})"))
            .unwrap_or_default()
    );
    let _ = writeln!(
        s,
        "  {} lines: {} applied, {} partly, {} left out, {} never read, {} read before any modifier is stored, \
         {} with no effect on any output, {} recognised but not modelled, {} not parsed, \
         {} parsed yet storing nothing",
        n.lines,
        n.lines_applied,
        n.lines_partial,
        n.lines_inactive,
        n.lines_unread,
        n.lines_outside,
        n.lines_no_effect,
        n.lines_unmodelled,
        n.lines_unparsed,
        n.lines_no_modifier
    );
    let _ = writeln!(
        s,
        "  {} gem stats rejected, {} without a mapping · {} conditions and {} multipliers nothing provides",
        n.unmapped_gem_stats, n.unknown_gem_stats, n.unset_conditions, n.unprovided_multipliers
    );

    if !usage.unparsed.is_empty() {
        let _ = writeln!(s, "\nnot parsed (no rule reads the line):");
        for line in &usage.unparsed {
            let holders: Vec<String> = line
                .sources
                .iter()
                .map(|source| {
                    let mut holder = source.kind.to_string();
                    if let Some(id) = &source.id {
                        let _ = write!(holder, " {id}");
                    }
                    if let Some(name) = &source.holder {
                        let _ = write!(holder, " ({name})");
                    }
                    holder
                })
                .collect();
            let count = if line.count > 1 {
                format!("{}× ", line.count)
            } else {
                String::new()
            };
            let held = if holders.is_empty() {
                "source not found".to_string()
            } else {
                holders.join(", ")
            };
            let _ = writeln!(s, "  {count}[{held}] {}", line.text);
        }
    }

    for (status, title) in [
        (
            LineStatus::Unmodelled,
            "recognised, not modelled (a rule stores nothing for it)",
        ),
        (
            LineStatus::NoModifier,
            "parsed, yet storing nothing (the ledger does not know why)",
        ),
        (LineStatus::Partial, "partly applied"),
        (LineStatus::Inactive, "left out"),
        (LineStatus::Unread, "never read"),
    ] {
        let lines: Vec<&UsageLine> = usage
            .lines
            .iter()
            .filter(|line| line.status == status)
            .collect();
        if lines.is_empty() {
            continue;
        }
        let _ = writeln!(s, "\n{title}:");
        for line in lines {
            let _ = writeln!(
                s,
                "  [{}] {}",
                line_holder(line),
                line.text.as_deref().unwrap_or("(no line)")
            );
            if let Some(reason) = &line.reason {
                let _ = writeln!(s, "      {reason}");
            }
            for modifier in line.modifiers.iter().filter(|modifier| {
                !matches!(
                    modifier.status,
                    ModifierStatus::Applied | ModifierStatus::Outside
                )
            }) {
                let _ = writeln!(s, "      {}", modifier_text(modifier));
            }
        }
    }

    if !usage.gem_stats.is_empty() {
        let _ = writeln!(s, "\ngem stats:");
        for stat in &usage.gem_stats {
            let _ = writeln!(
                s,
                "  {} · {} · {}{}",
                stat.owner,
                stat.stat,
                stat.status,
                stat.reason
                    .as_deref()
                    .map(|reason| format!(": {reason}"))
                    .unwrap_or_default()
            );
        }
    }
    if !usage.conditions.is_empty() {
        let _ = writeln!(s, "\nconditions modifiers wait for that are not set:");
        for condition in &usage.conditions {
            let set_by = match &condition.config_option {
                Some(option) => format!("config option {option} is off"),
                None => "nothing sets it".to_string(),
            };
            let _ = writeln!(s, "  {} ({set_by})", condition.condition);
            for modifier in &condition.modifiers {
                let _ = writeln!(s, "      {}", reference_text(modifier));
            }
        }
    }
    if !usage.multipliers.is_empty() {
        let _ = writeln!(s, "\nmultipliers nothing provides:");
        for multiplier in &usage.multipliers {
            let _ = writeln!(s, "  {}", multiplier.multiplier);
            for modifier in &multiplier.modifiers {
                let _ = writeln!(s, "      {}", reference_text(modifier));
            }
        }
    }
    let _ = writeln!(
        s,
        "\n(--json lists every line: applied ones with the stats they reach, and \
         those read before any modifier is stored or with no effect, with why)"
    );
}

/// What holds a line: its source, slot and holder's name.
fn line_holder(line: &UsageLine) -> String {
    let mut holder = line.source_kind.clone().unwrap_or_else(|| "?".to_string());
    if let Some(id) = &line.source_id {
        let _ = write!(holder, " {id}");
    }
    if let Some(name) = &line.holder {
        let _ = write!(holder, " ({name})");
    }
    holder
}

fn modifier_text(modifier: &UsageModifier) -> String {
    let value = match &modifier.value {
        UsageValue::Number(number) => crate::breakdown::num(*number),
        UsageValue::Bool(flag) => flag.to_string(),
        UsageValue::Text(text) => text.clone(),
    };
    let status = match modifier.status {
        ModifierStatus::Applied => "applied",
        ModifierStatus::Inactive => "left out",
        ModifierStatus::Unread => "never read",
        ModifierStatus::Outside => "used outside the database",
    };
    let mut text = format!("{} {} {value}: {status}", modifier.name, modifier.mod_type);
    if let Some(reason) = &modifier.reason {
        let _ = write!(text, ", {reason}");
    }
    text
}

fn reference_text(modifier: &UsageRef) -> String {
    format!(
        "{} {} {} · {}",
        modifier.name, modifier.mod_type, modifier.value, modifier.source
    )
}
