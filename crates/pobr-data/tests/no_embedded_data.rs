//! Lint against game data embedded in engine source (CI data guardrail #2).
//!
//! Scans every `.rs` file under the `src/` of the crates in
//! [`SCANNED_CRATES`]. Game data should flow through the owner packages into
//! `data/<ver>/` and be loaded at runtime by pobr-gamedata, not hardcoded
//! into Rust source. Three checks:
//!
//! 1. An array/const table with a run of consecutive literal-element lines
//!    longer than [`MAX_CONSECUTIVE_LITERAL_LINES`] is judged to be an
//!    embedded data table ([`LONG_TABLES`] lists the allowed ones).
//! 2. `include_str!` / `include_bytes!` of a sidecar file embeds a table the
//!    first check cannot see ([`SIDECARS`] lists the allowed ones).
//! 3. Known small game tables the heuristics cannot recognise are listed in
//!    [`SMALL_TABLES`]. The list does not detect new tables; it records the
//!    remaining ones so that each removal also removes its entry.
//!
//! Every allowlist entry names its reason and the roadmap item that removes
//! it, and an entry whose table is gone fails the test, so the lists only
//! shrink. New entries need a reason the data cannot come from an owner
//! package (for example a build-code compatibility default).
//!
//! The scan reads sibling crates through the workspace layout, so it runs
//! from a workspace checkout (`cargo test -p pobr-data --test no_embedded_data`).

use std::fs;
use std::path::{Path, PathBuf};

/// Crates whose `src/` is scanned, relative to the workspace root.
const SCANNED_CRATES: &[&str] = &[
    "crates/pobr-data",
    "crates/pobr-core",
    "crates/pobr-build",
    "crates/pobr-tree",
];

/// Threshold for a run of consecutive literal-element lines: exceeding it
/// counts as an embedded data table.
const MAX_CONSECUTIVE_LITERAL_LINES: usize = 200;

/// One allowed embedded table.
struct Allowed {
    /// Source file, relative to the workspace root.
    file: &'static str,
    /// The table: a `const` / `static` name, or the included file for a sidecar.
    item: &'static str,
    /// Why it is still in the engine.
    reason: &'static str,
    /// The roadmap item that removes it, or why it stays.
    removed_by: &'static str,
}

/// Files allowed past the literal-run threshold.
const LONG_TABLES: &[Allowed] = &[Allowed {
    file: "crates/pobr-core/src/calc/session/detail_views.rs",
    item: "DERIVED_STATS",
    reason: "engine output ids of derived detail views, not game data",
    removed_by: "stays: engine output contract",
}];

/// Allowed `include_str!` / `include_bytes!` sidecar tables.
const SIDECARS: &[Allowed] = &[Allowed {
    file: "crates/pobr-data/src/skill_type_names.rs",
    item: "skill_type_names.txt",
    reason: "frozen PoB2 SkillType names, needed for build-code compatibility",
    removed_by: "roadmap 4.3: ship with the RePoE compatibility package",
}];

/// Known small game tables (`const` / `static` items) still in engine source.
const SMALL_TABLES: &[Allowed] = &[
    Allowed {
        file: "crates/pobr-data/src/constants.rs",
        item: "BASE_MAXIMUM_RAGE",
        reason: "game constant BaseMaximumRage, absent from the data package",
        removed_by: "roadmap 4.2: read the imported game constant",
    },
    Allowed {
        file: "crates/pobr-data/src/constants.rs",
        item: "ATTACK_DAMAGE_MORE_PER_RAGE",
        reason: "inherent stat attack_damage_final_permyriad_per_rage, absent from the data package",
        removed_by: "roadmap 4.2: read the imported game constant",
    },
    Allowed {
        file: "crates/pobr-core/src/calc/charm_spirits.rs",
        item: "SPIRIT_DEFINITIONS",
        reason: "Rite of Passage possession buffs from the Mods table",
        removed_by: "roadmap 4.4",
    },
    Allowed {
        file: "crates/pobr-core/src/calc/charm_spirits.rs",
        item: "SPIRIT_FAMILY_STATS",
        reason: "Rite of Passage attribute-family bonuses from the Mods table",
        removed_by: "roadmap 4.4",
    },
    Allowed {
        file: "crates/pobr-core/src/calc/mageblood.rs",
        item: "LEGACIES",
        reason: "Mageblood legacy effects",
        removed_by: "roadmap 4.4",
    },
    Allowed {
        file: "crates/pobr-core/src/rules/umbral_souls.rs",
        item: "SOULS",
        reason: "Soul buff definitions by summoning skill",
        removed_by: "roadmap 4.4",
    },
    Allowed {
        file: "crates/pobr-core/src/calc/effect_views.rs",
        item: "DAZE_BASE_DURATION",
        reason: "KeywordPopups base duration",
        removed_by: "roadmap 4.4",
    },
    Allowed {
        file: "crates/pobr-core/src/calc/effect_views.rs",
        item: "MAIM_BASE_DURATION",
        reason: "KeywordPopups base duration",
        removed_by: "roadmap 4.4",
    },
    Allowed {
        file: "crates/pobr-core/src/calc/effect_views.rs",
        item: "BLIND_BASE_DURATION",
        reason: "KeywordPopups base duration",
        removed_by: "roadmap 4.4",
    },
    Allowed {
        file: "crates/pobr-tree/src/radius_jewel.rs",
        item: "JEWEL_RADIUS_SMALL",
        reason: "fallback jewel radius",
        removed_by: "roadmap 4.4",
    },
    Allowed {
        file: "crates/pobr-tree/src/radius_jewel.rs",
        item: "JEWEL_RADIUS_MEDIUM",
        reason: "fallback jewel radius",
        removed_by: "roadmap 4.4",
    },
    Allowed {
        file: "crates/pobr-tree/src/radius_jewel.rs",
        item: "JEWEL_RADIUS_LARGE",
        reason: "fallback jewel radius",
        removed_by: "roadmap 4.4",
    },
    Allowed {
        file: "crates/pobr-tree/src/radius_jewel.rs",
        item: "JEWEL_RADIUS_VERY_LARGE",
        reason: "fallback jewel radius",
        removed_by: "roadmap 4.4",
    },
    Allowed {
        file: "crates/pobr-build/src/xml_build.rs",
        item: "DEFAULT_QUEST_STAT_REWARDS",
        reason: "quest Stat rewards that default to taken in build codes",
        removed_by: "roadmap 4.4",
    },
    Allowed {
        file: "crates/pobr-build/src/xml_build.rs",
        item: "DEFAULT_TRUE_CONDITIONS",
        reason: "config inputs a PoB build code treats as true when omitted",
        removed_by: "stays: build-code compatibility default, not game data",
    },
];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

/// Judges whether a line looks like "a data table's literal-element line":
/// after trimming, it starts with a literal (a digit / minus sign / string
/// / tuple / nested array) and ends with a comma. A normal code line
/// (starting with an identifier, keyword, or comment) never matches.
fn is_literal_element_line(line: &str) -> bool {
    let trimmed = line.trim();
    if !trimmed.ends_with(',') {
        return false;
    }
    matches!(
        trimmed.as_bytes().first(),
        Some(b'0'..=b'9' | b'-' | b'"' | b'(' | b'[')
    )
}

/// Computes the longest run of consecutive literal-element lines in the source.
fn max_literal_run(source: &str) -> usize {
    let mut max_run = 0usize;
    let mut current = 0usize;
    for line in source.lines() {
        if is_literal_element_line(line) {
            current += 1;
            max_run = max_run.max(current);
        } else {
            current = 0;
        }
    }
    max_run
}

/// The string literal paths passed to `include_str!` / `include_bytes!`.
fn included_files(source: &str) -> Vec<&str> {
    let mut found = Vec::new();
    for macro_name in ["include_str!(", "include_bytes!("] {
        for (start, _) in source.match_indices(macro_name) {
            let rest = source[start + macro_name.len()..].trim_start();
            if let Some(literal) = rest.strip_prefix('"')
                && let Some(end) = literal.find('"')
            {
                found.push(&literal[..end]);
            }
        }
    }
    found
}

/// Whether the source declares a `const` / `static` item with this name.
fn declares_item(source: &str, item: &str) -> bool {
    ["const ", "static "]
        .iter()
        .any(|keyword| source.contains(&format!("{keyword}{item}:")))
}

/// Recursively collects every `.rs` file under a directory.
fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("failed to read directory {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("failed to read directory entry").path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// Every scanned source file as (workspace-relative path, contents).
fn scanned_sources(root: &Path) -> Vec<(String, String)> {
    let mut files = Vec::new();
    for krate in SCANNED_CRATES {
        let src = root.join(krate).join("src");
        assert!(src.is_dir(), "scanned crate has no src/: {}", src.display());
        collect_rs_files(&src, &mut files);
    }
    files.sort();
    files
        .into_iter()
        .map(|path| {
            let relative = path
                .strip_prefix(root)
                .expect("under the workspace root")
                .to_string_lossy()
                .replace('\\', "/");
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
            (relative, source)
        })
        .collect()
}

fn allowed(list: &[Allowed], file: &str, item: &str) -> bool {
    list.iter()
        .any(|entry| entry.file == file && (item.is_empty() || entry.item == item))
}

fn describe(entry: &Allowed) -> String {
    format!(
        "{} `{}` ({}; {})",
        entry.file, entry.item, entry.reason, entry.removed_by
    )
}

/// The guardrail's core check: engine source must not embed data tables
/// outside the allowlists, and every allowlist entry must still be needed.
#[test]
fn engine_src_has_no_embedded_data_tables() {
    let root = workspace_root();
    let sources = scanned_sources(&root);
    assert!(
        !sources.is_empty(),
        "no .rs files found, scan path is wrong"
    );

    let mut violations = Vec::new();
    let mut stale = Vec::new();
    for (file, source) in &sources {
        let run = max_literal_run(source);
        if run > MAX_CONSECUTIVE_LITERAL_LINES && !allowed(LONG_TABLES, file, "") {
            violations.push(format!(
                "{file}: {run} consecutive literal-element lines (threshold {MAX_CONSECUTIVE_LITERAL_LINES})"
            ));
        }
        for included in included_files(source) {
            if !allowed(SIDECARS, file, included) {
                violations.push(format!("{file}: embeds sidecar file {included}"));
            }
        }
    }
    let source_of = |file: &str| {
        sources
            .iter()
            .find(|(path, _)| path == file)
            .map(|(_, source)| source.as_str())
    };
    for entry in LONG_TABLES {
        if source_of(entry.file).is_none_or(|source| {
            max_literal_run(source) <= MAX_CONSECUTIVE_LITERAL_LINES
                || !declares_item(source, entry.item)
        }) {
            stale.push(describe(entry));
        }
    }
    for entry in SIDECARS {
        if source_of(entry.file).is_none_or(|source| !included_files(source).contains(&entry.item))
        {
            stale.push(describe(entry));
        }
    }
    for entry in SMALL_TABLES {
        if source_of(entry.file).is_none_or(|source| !declares_item(source, entry.item)) {
            stale.push(describe(entry));
        }
    }

    assert!(
        violations.is_empty(),
        "engine source embeds data tables (game data belongs in an owner package, \
         loaded through pobr-gamedata; see docs/data-architecture.md):\n{}",
        violations.join("\n")
    );
    assert!(
        stale.is_empty(),
        "allowlisted tables no longer embedded; remove their entries:\n{}",
        stale.join("\n")
    );
}

/// Heuristic unit test: a typical data-table line matches, an ordinary
/// code line doesn't.
#[test]
fn literal_element_line_heuristic() {
    // Matches: number / negative number / string / tuple / nested-array element
    assert!(is_literal_element_line("    1.5,"));
    assert!(is_literal_element_line("    -200.0,"));
    assert!(is_literal_element_line("    \"Metadata/Items/Foo\","));
    assert!(is_literal_element_line("    (3, \"bar\", 0.25),"));
    assert!(is_literal_element_line("    [1, 2, 3],"));

    // Doesn't match: ordinary code / comment / starts with an identifier / no trailing comma
    assert!(!is_literal_element_line(
        "pub const ARMOUR_RATIO: f64 = 10.0;"
    ));
    assert!(!is_literal_element_line("    DamageType::Fire,"));
    assert!(!is_literal_element_line("    // 1.5,"));
    assert!(!is_literal_element_line("    value,"));
    assert!(!is_literal_element_line("    1.5"));
    assert!(!is_literal_element_line(""));
}

/// Run-counting unit test: a consecutive block takes the max, and it
/// restarts after being interrupted by a code line.
#[test]
fn max_literal_run_counts_consecutive_lines() {
    assert_eq!(max_literal_run(""), 0);
    assert_eq!(max_literal_run("let x = 1;\nlet y = 2;"), 0);

    // 3 consecutive element lines
    let table = "const T: [f64; 3] = [\n    1.0,\n    2.0,\n    3.0,\n];";
    assert_eq!(max_literal_run(table), 3);

    // Interrupted by a non-element line: two runs of 2, max run is still 2
    let broken = "    1.0,\n    2.0,\nlet a = 0;\n    3.0,\n    4.0,";
    assert_eq!(max_literal_run(broken), 2);

    // A synthetic table exceeding the threshold must be detected
    let big = "    42,\n".repeat(MAX_CONSECUTIVE_LITERAL_LINES + 1);
    assert!(max_literal_run(&big) > MAX_CONSECUTIVE_LITERAL_LINES);
}

/// Sidecar and item detection used by the allowlist checks.
#[test]
fn sidecar_and_item_detection() {
    let source = "static RAW: &str = include_str!(\"table.txt\");\n\
                  const BYTES: &[u8] = include_bytes!( \"blob.bin\" );\n\
                  pub const SOULS: [(&str, u8); 1] = [(\"a\", 1)];";
    assert_eq!(included_files(source), vec!["table.txt", "blob.bin"]);
    assert!(declares_item(source, "SOULS"));
    assert!(declares_item(source, "RAW"));
    assert!(!declares_item(source, "SOUL"));
    assert!(!declares_item("fn SOULS() {}", "SOULS"));
}
