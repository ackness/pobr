//! StatDescriptions overlay domain schema
//! (`overlay/stat_descriptions.json`, schema `stat_descriptions/v1`).
//!
//! The legacy catalog comes from vendor PoB2 `Data/StatDescriptions/*.lua`,
//! deterministically extracted by
//! `sync-pob-catalog extract-lua --what stat-descriptions`: luajit loads
//! the description tables in a minimal environment, feeds each stat_id a
//! representative value (V=1) and renders a line of text; the Rust side
//! groups the result by scope and serializes with a BTreeMap for
//! byte-stable dictionary order.
//! The ordinary RePoE producer retains this representative shape and adds
//! optional complete descriptors with raw-unit conditions and display formats.
//! The tier consumer gives those members' branch metadata precedence over the
//! representative channel. Old JSON without descriptors retains its behavior.
//! Numeric 1 remains the legacy item-tier capture marker. Source ownership,
//! unsupported records and applicability are recorded in the producer receipt.
//!
//! Purpose (the gap this fills — "a second stat_id → Modifier channel"): a
//! lot of the game's tree nodes / item implicits / gem mods are given in
//! **stat_id** form (rather than English text) in the game data. This
//! table turns a stat_id back into canonical English text, which then goes
//! through `parse_mod_engine` to become a Modifier — a second injection
//! path running in parallel with the existing English-text channel; once
//! the two paths' output converges, consumers can switch over domain by domain.
//!
//! **Extraction-fidelity principle** (matches [`crate::catalog::stat_map`]):
//! - Rendered text is stored in JSON **as-is** — extraction does no
//!   semantic filtering; deciding "which stat_id is supported" is the
//!   engine's (`parse_mod_engine`'s) job.
//! - Multiple scopes (the root `stat_descriptions` plus
//!   `passive_skill_*`, etc.) are **each kept in their own section**;
//!   precedence isn't decided at extraction time — "child overrides
//!   parent" is decided by the consumer's §B generator, so the overlay's
//!   drift diff stays readable scope by scope when vendor updates.
//! - A compound descriptor spanning multiple stats **isn't force-parsed**
//!   (one sentence tied to several stat_ids) — the template and its member
//!   list are kept as-is for §B to handle.
//! - A stat_id with no renderable variant (e.g. only a negative limit) is
//!   recorded in the `unrendered` diagnostic set, not silently dropped.
//!
//! The numeric display helper is shared by CSD rendering and tier interval matching.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// The body of the StatDescriptions overlay document (the flat part
/// outside the `_meta` header).
///
/// Grouped by scope name (the root `stat_descriptions` plus each dedicated
/// domain like `passive_skill_stat_descriptions`); each section
/// independently holds the single / compound / unrendered three
/// categories, with precedence decided by the consumer.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StatDescriptionsDef {
    /// scope name → that scope's description set (BTreeMap for deterministic dictionary order).
    #[serde(default)]
    pub scopes: BTreeMap<String, ScopeDescriptions>,
}

/// The description set for a single scope (the "own keys" of one
/// StatDescriptions file).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ScopeDescriptions {
    /// Optional source-owned branches in raw stat units. Consumers use these
    /// instead of the representative single/compound entry for their members.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub descriptors: BTreeMap<String, StatDescriptor>,
    /// Shared raw integer keys to source-owned literal names, by CSD handler.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub lookup_values: BTreeMap<String, BTreeMap<i64, String>>,
    /// Single-stat representative templates. A complete multiline description
    /// remains one template; numeric 1 marks a capture, not an actual scaled value.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub single: BTreeMap<String, Vec<String>>,
    /// A multi-stat (compound) descriptor → the template as-is plus its
    /// member list (not force-parsed). Key = the descriptor's first
    /// stat_id (vendor's `stats[1]`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub compound: BTreeMap<String, CompoundDescription>,
    /// stat_ids with no renderable variant (for diagnostics; §B skips
    /// these, left for a manual overlay addition).
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub unrendered: BTreeSet<String>,
}

/// A multi-stat descriptor kept as-is (one template sentence bound to
/// several stat_ids).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CompoundDescription {
    /// All the stat_ids this descriptor is bound to (vendor's
    /// `descriptor.stats`, original order).
    pub member_stats: Vec<String>,
    /// The template text as-is (includes `{0}`/`{1}` placeholders and
    /// literal `\n` newlines; §B handles it on its own).
    pub template: String,
}

/// One complete ordered CSD descriptor, without inherited scope entries.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StatDescriptor {
    /// Source stat order; placeholder indices address this list.
    pub member_stats: Vec<String>,
    /// Ordinary display branches, evaluated in source order on raw integers.
    pub branches: Vec<DescriptionBranch>,
}

/// A display template and its raw-unit selection and formatting rules.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DescriptionBranch {
    /// Indexed placeholders, with presentation markup removed.
    pub template: String,
    /// One condition per member, before display transforms.
    pub conditions: Vec<StatCondition>,
    /// One composed numeric format per member.
    pub formats: Vec<StatValueFormat>,
    /// Optional relational formatter per member. Empty means all numeric.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lookups: Vec<Option<String>>,
}

/// Inclusive signed-integer limits; absent bounds are unbounded.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatCondition {
    /// Inclusive lower bound in source units.
    pub min: Option<i64>,
    /// Inclusive upper bound in source units.
    pub max: Option<i64>,
    /// A single excluded value (`!n` in CSD).
    pub not: Option<i64>,
}

/// Exact affine display conversion: `(raw * multiplier + offset) / divisor`.
/// Precision belongs to the last numeric handler. `None` uses six significant
/// digits; zero truncates toward zero, positive precision rounds to decimals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatValueFormat {
    /// Signed source-value coefficient.
    pub multiplier: i64,
    /// Positive display-unit denominator.
    pub divisor: u64,
    /// Numerator offset, in the same units as `raw * multiplier`.
    pub offset: i64,
    /// Decimal places from the last handler; no handler uses exact integers.
    pub precision: Option<u8>,
    /// Keep trailing decimal zeroes.
    pub fixed: bool,
}

impl Default for StatValueFormat {
    fn default() -> Self {
        Self {
            multiplier: 1,
            divisor: 1,
            offset: 0,
            precision: None,
            fixed: false,
        }
    }
}

impl StatValueFormat {
    /// Format a supplied raw integer using the pinned CSD numeric conventions.
    /// Invalid metadata, overflow and lossy wider integer inputs return `None`.
    pub fn display(&self, raw: i64, positive_sign: bool) -> Option<String> {
        if self.divisor == 0 || self.precision.is_some_and(|p| p > 12) {
            return None;
        }
        let numerator = raw.checked_mul(self.multiplier)?.checked_add(self.offset)?;
        let divisor = self.divisor;
        let precision = self.precision.map(usize::from);
        if divisor == 1 && precision.is_none() {
            return Some(if positive_sign {
                format!("{numerator:+}")
            } else {
                numerator.to_string()
            });
        }
        // All native stat values are signed i32. Reject lossy wider inputs rather
        // than silently discarding their low bits during unit conversion.
        if numerator.unsigned_abs() > (1u64 << 53) || divisor > (1u64 << 53) {
            return None;
        }
        let value = numerator as f64 / divisor as f64;
        let mut out = if precision == Some(0) {
            (value as i64).to_string()
        } else if let Some(precision) = precision {
            format!("{value:.precision$}")
        } else if value.fract() == 0.0 {
            (value as i64).to_string()
        } else {
            general(value)
        };
        if precision.is_some() && !self.fixed && out.contains('.') {
            out = out.trim_end_matches('0').trim_end_matches('.').to_owned();
        }
        if positive_sign && !out.starts_with('-') {
            out.insert(0, '+');
        }
        Some(out)
    }
}

// PyPoE's unqualified `n` format uses six significant digits in the C locale.
// Derive fixed/scientific notation from Rust's rounded six-digit mantissa.
fn general(value: f64) -> String {
    let scientific = format!("{value:.5e}");
    let (mantissa, exponent) = scientific
        .split_once('e')
        .expect("finite scientific format");
    let exponent: i32 = exponent.parse().expect("scientific exponent");
    let sign = if mantissa.starts_with('-') { "-" } else { "" };
    let digits = mantissa.trim_start_matches('-').replace('.', "");
    if !(-4..6).contains(&exponent) {
        let mantissa = mantissa.trim_end_matches('0').trim_end_matches('.');
        return format!("{mantissa}e{exponent:+03}");
    }
    let decimal = exponent + 1;
    let mut out = if decimal <= 0 {
        format!("{sign}0.{}{digits}", "0".repeat((-decimal) as usize))
    } else {
        let at = decimal as usize;
        format!("{sign}{}.{}", &digits[..at], &digits[at..])
    };
    if out.contains('.') {
        out = out.trim_end_matches('0').trim_end_matches('.').to_owned();
    }
    out
}
