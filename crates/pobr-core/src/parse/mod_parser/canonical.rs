//! Canonical serialization for [`ParseOutcome`] — the **shared comparison
//! unit** for dual-run diffing (Track C) and precompile (Track D); we don't
//! maintain two separate serializations.
//!
//! Comparison unit: a canonical string built from a sorted `Vec<Modifier>`.
//! Modifiers sort by `(name, mod_type, tags, flags, kw, value)`; f64 uses
//! shortest round-trip representation. Neither `source` (the original text) nor
//! `origin` (SourceId) takes part — the two sides of a dual run construct both
//! differently. `status`/`unparsed` are compared too.

use super::outcome::{ParseOutcome, ParseStatus};
use crate::modifier::ModValue;
use crate::{ModTag, Modifier};

/// Canonicalizes one line's parse result into a comparable string.
pub fn canonical_outcome(outcome: &ParseOutcome) -> String {
    let mut lines: Vec<String> = outcome.mods.iter().map(canonical_mod).collect();
    lines.sort();
    let status = match outcome.status {
        ParseStatus::Parsed => "parsed",
        ParseStatus::Unsupported => "unsupported",
    };
    let unparsed = outcome.unparsed.as_deref().unwrap_or("");
    format!(
        "status={status}|unparsed={unparsed}|mods=[{}]",
        lines.join(";")
    )
}

/// Canonical form of a single Modifier (excludes origin).
fn canonical_mod(m: &Modifier) -> String {
    let value = canonical_value(&m.value);
    let tags = canonical_tags(&m.tags);
    format!(
        "name={}|type={:?}|flags={:#x}|kw={:#x}|tags={}|value={}",
        m.name.as_str(),
        m.mod_type,
        m.flags.bits(),
        m.keyword_flags.bits(),
        tags,
        value,
    )
}

fn canonical_value(v: &ModValue) -> String {
    match v {
        ModValue::Number(n) => fmt_f64(*n),
        ModValue::Bool(b) => format!("bool:{b}"),
        ModValue::Text(s) => format!("text:{s}"),
        ModValue::NestedMods(mods) => {
            let mut inner: Vec<String> = mods.iter().map(canonical_mod).collect();
            inner.sort();
            format!("nested:[{}]", inner.join(","))
        }
    }
}

/// Shortest round-trip representation of an f64 (integers drop the decimal
/// point; NaN/inf are marked explicitly).
fn fmt_f64(n: f64) -> String {
    if n.is_nan() {
        return "nan".to_string();
    }
    if n.is_infinite() {
        return if n > 0.0 { "inf" } else { "-inf" }.to_string();
    }
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        // serde_json gets shortest round-trip via ryu; plain `{}` here is
        // already deterministic since Rust's f64 Display is itself
        // shortest-round-trip.
        format!("{n}")
    }
}

/// Canonical string for a set of tags (used for structural comparison and
/// stat_id_map template's single-source tag serialization).
pub fn canonical_tags(tags: &[ModTag]) -> String {
    let mut out: Vec<String> = tags.iter().map(canonical_tag).collect();
    out.sort();
    format!("[{}]", out.join(","))
}

fn canonical_tag(tag: &ModTag) -> String {
    match tag {
        ModTag::Condition {
            var,
            negated,
            actor,
        } => {
            format!("Condition(var={var},neg={negated},actor={actor:?})")
        }
        ModTag::ConditionAnyOf { vars, negated } => {
            format!("ConditionAnyOf(vars=[{}],neg={negated})", vars.join(","))
        }
        ModTag::Multiplier {
            var,
            div,
            limit,
            actor,
            limit_var,
            limit_actor,
            invert,
            limit_total,
        } => format!(
            "Multiplier(var={var},div={},limit={:?},actor={actor:?},limit_var={limit_var:?},limit_actor={limit_actor:?},invert={invert},limit_total={limit_total})",
            fmt_f64(*div),
            limit.map(fmt_f64),
        ),
        ModTag::PerStat {
            stat,
            div,
            limit,
            limit_var,
            actor,
        } => format!(
            "PerStat(stat={stat},div={},limit={:?},limit_var={limit_var:?},actor={actor:?})",
            fmt_f64(*div),
            limit.map(fmt_f64),
        ),
        ModTag::PercentStat { stat, percent } => format!(
            "PercentStat(stat={stat},percent={:?})",
            percent.map(fmt_f64),
        ),
        ModTag::GlobalLimit { value, key } => {
            format!("GlobalLimit(value={},key={key})", fmt_f64(*value))
        }
        ModTag::DamageType(dt) => format!("DamageType({dt:?})"),
        // Keep the legacy u64 hex form when the high words are all zero
        // (preserves existing caches / dual-run baselines byte-for-byte);
        // high-word types (e.g. Meta=122) append the full word array.
        ModTag::SkillTypes(st) => {
            let w = st.words();
            if w[1..].iter().all(|&x| x == 0) {
                format!("SkillTypes({:#x})", w[0])
            } else {
                let hex: Vec<String> = w.iter().map(|x| format!("{x:#x}")).collect();
                format!("SkillTypes({})", hex.join(","))
            }
        }
        ModTag::SkillTypesNeg(st) => {
            let w = st.words();
            if w[1..].iter().all(|&x| x == 0) {
                format!("SkillTypesNeg({:#x})", w[0])
            } else {
                let hex: Vec<String> = w.iter().map(|x| format!("{x:#x}")).collect();
                format!("SkillTypesNeg({})", hex.join(","))
            }
        }
        ModTag::SkillName { names } => format!("SkillName(names=[{}])", names.join(",")),
        ModTag::SlotName(s) => format!("SlotName({s})"),
        ModTag::DistanceRamp { ramp } => {
            let points: Vec<String> = ramp
                .iter()
                .map(|(d, m)| format!("({},{})", fmt_f64(*d), fmt_f64(*m)))
                .collect();
            format!("DistanceRamp([{}])", points.join(","))
        }
        ModTag::MultiplierThreshold {
            var,
            threshold,
            upper,
        } => format!(
            "MultiplierThreshold(var={var},threshold={},upper={upper})",
            fmt_f64(*threshold)
        ),
        ModTag::StatThreshold {
            stat,
            threshold,
            upper,
        } => format!(
            "StatThreshold(stat={stat},threshold={},upper={upper})",
            fmt_f64(*threshold)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Modifier;
    use pobr_data::prelude::ModType;

    #[test]
    fn canonical_ignores_origin_and_source() {
        let a = Modifier::number("Life", ModType::Base, 50.0).with_source("orig");
        let origin = pobr_data::prelude::ModifierSource::new(pobr_data::prelude::SourceId::new(
            pobr_data::prelude::SourceKind::PassiveNode,
            "node-1",
        ));
        let b = Modifier::number("Life", ModType::Base, 50.0)
            .with_source("orig")
            .with_origin(origin);
        let oa = ParseOutcome {
            mods: vec![a],
            status: ParseStatus::Parsed,
            unparsed: None,
            special_meta: None,
        };
        let ob = ParseOutcome {
            mods: vec![b],
            status: ParseStatus::Parsed,
            unparsed: None,
            special_meta: None,
        };
        // origin is excluded, and source doesn't participate in a mod's
        // canonical form either (only name/type/flags/kw/tags/value do).
        assert_eq!(canonical_outcome(&oa), canonical_outcome(&ob));
    }

    #[test]
    fn canonical_mod_order_independent() {
        let m1 = Modifier::number("Life", ModType::Base, 50.0);
        let m2 = Modifier::number("Mana", ModType::Inc, 20.0);
        let o1 = ParseOutcome {
            mods: vec![m1.clone(), m2.clone()],
            status: ParseStatus::Parsed,
            unparsed: None,
            special_meta: None,
        };
        let o2 = ParseOutcome {
            mods: vec![m2, m1],
            status: ParseStatus::Parsed,
            unparsed: None,
            special_meta: None,
        };
        assert_eq!(canonical_outcome(&o1), canonical_outcome(&o2));
    }
}
