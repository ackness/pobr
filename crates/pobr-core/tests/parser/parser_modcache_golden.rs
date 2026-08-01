//! ModCache golden differential.
//!
//! Cross-checks the data-driven engine's output (`parse_mod_engine` + the real rule
//! set) mod-by-mod against a golden dump offline-captured from vendor PoB2's
//! `Data/ModCache.lua` (`tests/fixtures/modcache_golden.json`, produced by
//! `tools/pob2-oracle/oracle.lua --mode modcache-dump`), producing a **five-state
//! report**. This is the correctness baseline for the Track B engine rewrite: once B
//! switches over it reuses the same golden fixture and the same report schema, and the
//! diff-fixing loop treats this report as the shared input (contract 5).
//!
//! # The five-state verdict (§5.2 terms, relative to where the vendor golden lands)
//!
//! | state | definition | §5.2 equivalent |
//! |----|------|-----------|
//! | `eq`     | golden parsed, PoBR parsed, and the canonical forms are equal | EQ |
//! | `diff`   | both sides parsed but the canonical forms differ | DIFF (blocks the switch — B has final say on the fix) |
//! | `miss`   | golden parsed but PoBR is unsupported (a PoBR gap) | the mirror of NEW_ONLY (vendor can, PoBR can't) |
//! | `extra`  | golden unsupported but PoBR parsed (PoBR over-parses) | the mirror of OLD_ONLY (PoBR can, vendor's cache is empty) |
//! | `unsup`  | both sides unsupported | UNSUP (counted as a coverage gap, doesn't block) |
//!
//! Hit rate = `eq / (total golden-parsed entries)` — the Track B rewrite's goal is to
//! drive `miss`/`diff` to zero, pushing the hit rate toward 100%.
//!
//! # canonical comparison rules (**the key tradeoff in this batch**)
//!
//! Modifier canonical = `(name, type, value, flags[], keywordFlags[])`. **The tag
//! structure does not participate in the EQ/DIFF verdict** — the vendor tag table and
//! pobr's `ModTag` have different field shapes (vendor uses
//! `{type="Multiplier",var=...,div=...}`, pobr uses a shaped enum), and mapping them
//! field-by-field is Track B rewrite work, not this cross-check baseline's job. Tag
//! count differences are recorded in the report detail (`tag_delta`) as a reference
//! for B, but they don't trigger DIFF. Once the B engine lands, this can be tightened
//! to full tag equality on the same fixture.
//!
//! This tradeoff keeps the baseline **stable** (no flood of false DIFFs from tag
//! representation differences) while still precisely capturing real deviations at the
//! name/type/value/flag level — which is the core correctness signal for the engine.
//!
//! # Artifacts
//!
//! The report is written to `target/parser-modcache-diff-report.json` (schema
//! documented on `DiffReport` in this module, contract 5), consumed by both CI and B's
//! diff-fixing loop. This test does **not** enforce a hard hit-rate threshold (the
//! engine's coverage of the full ModCache corpus is inherently partial — the hard
//! gate is the parity suite); it only asserts that the fixture loads, the report
//! generates, and the five-state counts are self-consistent (keeping the baseline
//! runnable).
//!
//! Implementation note: pobr-core's dev-deps only include `serde_json` (no `serde`
//! derive), so golden loading and report construction go entirely through
//! `serde_json::Value` field access by hand — consistent with the repo convention of
//! "zero serde_json in the lib itself" (this is test-only usage).

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use crate::support::parse_mod;
use pobr_core::mod_parser::ParseStatus;
use pobr_core::{ModValue, Modifier};
use pobr_data::modifier::ModType;
use serde_json::{Map, Value, json};

const GOLDEN_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/modcache_golden.json"
);

// golden fixture (produced by oracle.lua --mode modcache-dump) — a serde_json::Value view

struct GoldenDoc {
    meta_total: usize,
    meta_parsed: usize,
    meta_unsupported: usize,
    entries: Vec<GoldenEntry>,
}

struct GoldenEntry {
    text: String,
    /// The vendor-side canonical form (already sorted into normal form). Empty = unsupported.
    canon: Vec<CanonMod>,
    parsed: bool,
}

fn as_usize(v: &Value, key: &str) -> usize {
    v.get(key)
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("golden _meta.{key} missing/not-uint")) as usize
}

fn load_golden() -> GoldenDoc {
    let raw = fs::read_to_string(GOLDEN_PATH)
        .unwrap_or_else(|e| panic!("read golden fixture {GOLDEN_PATH}: {e}"));
    let doc: Value = serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse golden: {e}"));

    let meta = doc.get("_meta").expect("golden _meta");
    let meta_total = as_usize(meta, "total");
    let meta_parsed = as_usize(meta, "parsed");
    let meta_unsupported = as_usize(meta, "unsupported");

    let arr = doc
        .get("entries")
        .and_then(Value::as_array)
        .expect("golden entries[]");
    let mut entries = Vec::with_capacity(arr.len());
    for e in arr {
        let text = e
            .get("text")
            .and_then(Value::as_str)
            .expect("entry.text")
            .to_string();
        let status = e.get("status").and_then(Value::as_str).unwrap_or("");
        let mods = e.get("mods").and_then(Value::as_array);
        let canon: Vec<CanonMod> = mods
            .map(|ms| ms.iter().map(golden_mod_to_canon).collect())
            .unwrap_or_default();
        let parsed = status == "parsed" && !canon.is_empty();
        entries.push(GoldenEntry {
            text,
            canon: sorted_canon(canon),
            parsed,
        });
    }
    GoldenDoc {
        meta_total,
        meta_parsed,
        meta_unsupported,
        entries,
    }
}

// canonical mod

/// The canonical form `(name, type, value, flags[], keywordFlags[])`. `tag_count`
/// rides along but doesn't participate in equality (see the module doc's "canonical
/// comparison rules").
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CanonMod {
    name: String,
    mod_type: String,
    /// f64 stored as its shortest round-trip string, sidestepping NaN incomparability
    /// and floating-point jitter.
    value: String,
    flags: Vec<String>,
    keyword_flags: Vec<String>,
    /// A side signal that doesn't feed into core_eq.
    tag_count: usize,
}

impl CanonMod {
    /// The "core key" used for the EQ verdict (excludes tag_count).
    fn core_eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.mod_type == other.mod_type
            && self.value == other.value
            && self.flags == other.flags
            && self.keyword_flags == other.keyword_flags
    }

    fn to_json(&self) -> Value {
        json!({
            "name": self.name,
            "type": self.mod_type,
            "value": self.value,
            "flags": self.flags,
            "keywordFlags": self.keyword_flags,
            "tag_count": self.tag_count,
        })
    }
}

fn mod_type_label(t: ModType) -> &'static str {
    t.as_trace_label()
}

/// f64 -> shortest round-trip string (integers render as integers to match vendor's encoding).
fn num_to_canon(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        serde_json::Number::from_f64(v)
            .map(|n| n.to_string())
            .unwrap_or_else(|| format!("{v}"))
    }
}

fn value_to_canon(v: &ModValue) -> String {
    match v {
        ModValue::Number(n) => num_to_canon(*n),
        ModValue::Bool(b) => b.to_string(),
        ModValue::Text(s) => format!("{s:?}"),
        ModValue::NestedMods(mods) => format!("nested:{}", mods.len()),
    }
}

/// PoBR ModFlags -> vendor Global.lua ModFlag names (bit values match one-for-one,
/// see pobr-data/src/modifier.rs docs). Only single bits are listed; compound masks
/// aren't named individually (vendor's flagNames table is likewise expanded per
/// single bit).
const MOD_FLAG_NAMES: &[(u64, &str)] = &[
    (0x1, "Attack"),
    (0x2, "Spell"),
    (0x4, "Hit"),
    (0x8, "Dot"),
    (0x10, "Cast"),
    (0x20, "Thorns"),
    (0x100, "Melee"),
    (0x200, "Area"),
    (0x400, "Projectile"),
    (0x800, "Ailment"),
    (0x1000, "MeleeHit"),
    (0x2000, "Weapon"),
    (0x10000, "Axe"),
    (0x20000, "Bow"),
    (0x40000, "Claw"),
    (0x80000, "Dagger"),
    (0x100000, "Mace"),
    (0x200000, "Staff"),
    (0x400000, "Sword"),
    (0x800000, "Wand"),
    (0x1000000, "Unarmed"),
    (0x2000000, "Fishing"),
    (0x4000000, "Crossbow"),
    (0x8000000, "Flail"),
    (0x10000000, "Spear"),
    (0x20000000, "Warstaff"),
    (0x40000000, "Talisman"),
    (0x1_0000_0000, "WeaponMelee"),
    (0x2_0000_0000, "WeaponRanged"),
    (0x4_0000_0000, "Weapon1H"),
    (0x8_0000_0000, "Weapon2H"),
];

const KEYWORD_FLAG_NAMES: &[(u64, &str)] = &[
    (0x0000_0001, "Aura"),
    (0x0000_0002, "Curse"),
    (0x0000_0004, "Warcry"),
    (0x0000_0008, "Movement"),
    (0x0000_0010, "Physical"),
    (0x0000_0020, "Fire"),
    (0x0000_0040, "Cold"),
    (0x0000_0080, "Lightning"),
    (0x0000_0100, "Chaos"),
    (0x0000_0200, "Vaal"),
    (0x0000_0400, "Bow"),
    (0x0000_0800, "Arrow"),
    (0x0000_1000, "Trap"),
    (0x0000_2000, "Mine"),
    (0x0000_4000, "Totem"),
    (0x0000_8000, "Minion"),
    (0x0001_0000, "Attack"),
    (0x0002_0000, "Spell"),
    (0x0004_0000, "Hit"),
    (0x0008_0000, "Ailment"),
    (0x0010_0000, "Brand"),
    (0x0020_0000, "Poison"),
    (0x0040_0000, "Bleed"),
    (0x0080_0000, "Ignite"),
    (0x0100_0000, "PhysicalDot"),
    (0x0200_0000, "LightningDot"),
    (0x0400_0000, "ColdDot"),
    (0x0800_0000, "FireDot"),
    (0x1000_0000, "ChaosDot"),
];

fn flag_names(bits: u64, table: &[(u64, &str)]) -> Vec<String> {
    let mut names: Vec<String> = table
        .iter()
        .filter(|(bit, _)| bits & bit == *bit && *bit != 0)
        .map(|(_, name)| (*name).to_string())
        .collect();
    names.sort();
    names
}

fn pobr_mod_to_canon(m: &Modifier) -> CanonMod {
    CanonMod {
        name: m.name.as_str().to_string(),
        mod_type: mod_type_label(m.mod_type).to_string(),
        value: value_to_canon(&m.value),
        flags: flag_names(m.flags.bits(), MOD_FLAG_NAMES),
        keyword_flags: flag_names(m.keyword_flags.bits(), KEYWORD_FLAG_NAMES),
        tag_count: m.tags.len(),
    }
}

fn golden_value_to_canon(v: Option<&Value>) -> String {
    match v {
        Some(Value::Number(n)) => {
            if let Some(i) = n.as_i64() {
                format!("{i}")
            } else if let Some(f) = n.as_f64() {
                num_to_canon(f)
            } else {
                n.to_string()
            }
        }
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::String(s)) => format!("{s:?}"),
        Some(Value::Array(a)) => format!("nested:{}", a.len()),
        Some(Value::Object(_)) => "nested:1".to_string(),
        Some(Value::Null) | None => "null".to_string(),
    }
}

fn str_array(v: Option<&Value>) -> Vec<String> {
    let mut out: Vec<String> = v
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

fn golden_mod_to_canon(m: &Value) -> CanonMod {
    let obj = m.as_object().cloned().unwrap_or_else(Map::new);
    let tag_count = obj
        .get("tags")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    CanonMod {
        name: obj
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        mod_type: obj
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        value: golden_value_to_canon(obj.get("value")),
        flags: str_array(obj.get("flags")),
        keyword_flags: str_array(obj.get("keywordFlags")),
        tag_count,
    }
}

fn sorted_canon(mut v: Vec<CanonMod>) -> Vec<CanonMod> {
    v.sort();
    v
}

fn canon_lists_eq(a: &[CanonMod], b: &[CanonMod]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.core_eq(y))
}

// The five-state report (§11.3 contract 5: the shared input for B's diff-fixing and F's acceptance)

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Eq,
    Diff,
    Miss,
    Extra,
    Unsup,
}

impl State {
    fn as_str(self) -> &'static str {
        match self {
            State::Eq => "eq",
            State::Diff => "diff",
            State::Miss => "miss",
            State::Extra => "extra",
            State::Unsup => "unsup",
        }
    }
}

#[derive(Debug, Default)]
struct StateCounts {
    eq: usize,
    diff: usize,
    miss: usize,
    extra: usize,
    unsup: usize,
}

impl StateCounts {
    fn total(&self) -> usize {
        self.eq + self.diff + self.miss + self.extra + self.unsup
    }
    fn bump(&mut self, s: State) {
        match s {
            State::Eq => self.eq += 1,
            State::Diff => self.diff += 1,
            State::Miss => self.miss += 1,
            State::Extra => self.extra += 1,
            State::Unsup => self.unsup += 1,
        }
    }
    fn to_json(&self) -> Value {
        json!({
            "eq": self.eq, "diff": self.diff, "miss": self.miss,
            "extra": self.extra, "unsup": self.unsup,
        })
    }
}

/// A single cross-check detail entry (DIFF/MISS/EXTRA get a detail entry; EQ/UNSUP
/// are just counted).
struct DiffDetail {
    text: String,
    state: &'static str,
    pobr: Vec<CanonMod>,
    golden: Vec<CanonMod>,
    /// pobr_tag_total - golden_tag_total (a hint about tag-shape differences, see the module doc).
    tag_delta: i64,
}

impl DiffDetail {
    fn to_json(&self) -> Value {
        json!({
            "text": self.text,
            "state": self.state,
            "pobr": self.pobr.iter().map(CanonMod::to_json).collect::<Vec<_>>(),
            "golden": self.golden.iter().map(CanonMod::to_json).collect::<Vec<_>>(),
            "tag_delta": self.tag_delta,
        })
    }
}

/// The five-state report's top-level schema (contract 5), written to
/// `target/parser-modcache-diff-report.json`:
///
/// ```jsonc
/// {
///   "schema": "parser-modcache-diff/v1",
///   "engine": "engine",
///   "golden": { "source", "total", "parsed", "unsupported" },
///   "counts": { "eq", "diff", "miss", "extra", "unsup" },
///   "hit_rate": 0.0..1.0,         // eq / golden.parsed
///   "miss_forms": { "<vendor's first mod name>": count, ... },  // grouped by MISS gap
///   "details": [ { "text", "state", "pobr":[CanonMod], "golden":[CanonMod], "tag_delta" } ]
/// }
/// ```
/// where `CanonMod = { name, type, value, flags[], keywordFlags[], tag_count }`.
struct DiffReport {
    counts: StateCounts,
    golden_total: usize,
    golden_parsed: usize,
    golden_unsupported: usize,
    hit_rate: f64,
    miss_forms: BTreeMap<String, usize>,
    details: Vec<DiffDetail>,
}

impl DiffReport {
    fn to_json(&self) -> Value {
        json!({
            "schema": "parser-modcache-diff/v1",
            "engine": "engine",
            "golden": {
                "source": "vendor Data/ModCache.lua",
                "total": self.golden_total,
                "parsed": self.golden_parsed,
                "unsupported": self.golden_unsupported,
            },
            "counts": self.counts.to_json(),
            "hit_rate": self.hit_rate,
            "miss_forms": Value::Object(
                self.miss_forms
                    .iter()
                    .map(|(k, v)| (k.clone(), json!(v)))
                    .collect(),
            ),
            "details": self.details.iter().map(DiffDetail::to_json).collect::<Vec<_>>(),
        })
    }
}

// Main cross-check flow

fn run_differential(doc: &GoldenDoc) -> DiffReport {
    let mut counts = StateCounts::default();
    let mut details = Vec::new();
    let mut miss_forms: BTreeMap<String, usize> = BTreeMap::new();

    for entry in &doc.entries {
        let outcome = parse_mod(&entry.text);
        let (pobr_parsed, pobr_canon) = match outcome {
            Ok(o) if o.status == ParseStatus::Parsed && !o.mods.is_empty() => (
                true,
                sorted_canon(o.mods.iter().map(pobr_mod_to_canon).collect()),
            ),
            _ => (false, Vec::new()),
        };

        let state = match (entry.parsed, pobr_parsed) {
            (true, true) => {
                if canon_lists_eq(&pobr_canon, &entry.canon) {
                    State::Eq
                } else {
                    State::Diff
                }
            }
            (true, false) => State::Miss,
            (false, true) => State::Extra,
            (false, false) => State::Unsup,
        };
        counts.bump(state);

        if state == State::Miss {
            let form = entry
                .canon
                .first()
                .map(|m| m.name.clone())
                .unwrap_or_else(|| "<unknown>".to_string());
            *miss_forms.entry(form).or_insert(0) += 1;
        }

        if matches!(state, State::Diff | State::Miss | State::Extra) {
            let pobr_tags: usize = pobr_canon.iter().map(|m| m.tag_count).sum();
            let golden_tags: usize = entry.canon.iter().map(|m| m.tag_count).sum();
            details.push(DiffDetail {
                text: entry.text.clone(),
                state: state.as_str(),
                pobr: pobr_canon,
                golden: entry.canon.clone(),
                tag_delta: pobr_tags as i64 - golden_tags as i64,
            });
        }
    }

    let golden_parsed_total = doc.entries.iter().filter(|e| e.parsed).count();
    let hit_rate = if golden_parsed_total == 0 {
        0.0
    } else {
        counts.eq as f64 / golden_parsed_total as f64
    };

    DiffReport {
        counts,
        golden_total: doc.meta_total,
        golden_parsed: doc.meta_parsed,
        golden_unsupported: doc.meta_unsupported,
        hit_rate,
        miss_forms,
        details,
    }
}

fn write_report(report: &DiffReport) -> PathBuf {
    let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target");
    let _ = fs::create_dir_all(&out_dir);
    let path = out_dir.join("parser-modcache-diff-report.json");
    let json = serde_json::to_string_pretty(&report.to_json()).expect("serialize report");
    fs::write(&path, json).expect("write report");
    path
}

// Tests

/// The golden fixture is self-consistent: the meta counts match the entries' actual
/// states (guards the fixture's on-disk correctness).
#[test]
fn golden_fixture_is_self_consistent() {
    let doc = load_golden();
    assert_eq!(doc.entries.len(), doc.meta_total, "entries vs meta.total");
    let parsed = doc.entries.iter().filter(|e| e.parsed).count();
    let unsup = doc.entries.len() - parsed;
    assert_eq!(parsed, doc.meta_parsed, "parsed count vs meta.parsed");
    assert_eq!(unsup, doc.meta_unsupported, "unsupported count vs meta");
    assert!(
        doc.meta_total > 1000,
        "golden should be the full ModCache dump"
    );
}

/// The engine's five-state cross-check against golden, plus report writing and
/// count self-consistency.
///
/// **No hard hit-rate threshold is enforced** — the engine's coverage of the full
/// ModCache corpus is inherently partial; the hard gate is the parity suite. This
/// test only guards the baseline's runnability and prints the hit rate to stderr
/// (visible with `--nocapture`) to track gap convergence.
#[test]
fn engine_modcache_differential() {
    let doc = load_golden();
    let report = run_differential(&doc);

    assert_eq!(
        report.counts.total(),
        doc.entries.len(),
        "five-state counts must cover every entry exactly once"
    );

    let path = write_report(&report);

    let c = &report.counts;
    eprintln!("\n=== ModCache golden differential (engine) ===");
    eprintln!(
        "golden: {} entries ({} parsed / {} unsupported)",
        report.golden_total, report.golden_parsed, report.golden_unsupported
    );
    eprintln!(
        "five-state: eq={} diff={} miss={} extra={} unsup={}",
        c.eq, c.diff, c.miss, c.extra, c.unsup
    );
    eprintln!(
        "hit_rate (eq / golden.parsed) = {:.4} ({}/{})",
        report.hit_rate, c.eq, report.golden_parsed
    );
    eprintln!("MISS top forms:");
    let mut top: Vec<(&String, &usize)> = report.miss_forms.iter().collect();
    top.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (name, n) in top.into_iter().take(15) {
        eprintln!("  {n:>4}  {name}");
    }
    eprintln!("report -> {}", path.display());

    assert!(
        c.eq + c.diff + c.miss > 0,
        "differential produced no signal"
    );
}
