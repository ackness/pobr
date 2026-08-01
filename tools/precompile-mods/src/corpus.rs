//! Four-layer corpus collector.
//!
//! | Layer | Source | Extraction |
//! |----|------|------|
//! | C1 | `<Item>` text blocks in `examples/demo-bd-test/builds/*/decoded.xml` | per-line (annotations stripped) |
//! | C2 | `stats[]` of every node in `<data>/base/passive_tree.json` | per-line |
//! | SD | `pattern` in `<data>/generated/special_derived.json` | whole entry |
//! | CX | each line of `--corpus-extra <file>` | whole line |
//!
//! Sorted lexicographically after deduplication, so `parsed_mods.json` stays
//! byte-stable. Each line records its set of contributing sources (union
//! across multiple hits) for coverage-report grouping.
//!
//! C3 (vendor `ModCache.lua`) / C4 (`QueryMods.lua`) need luajit and belong
//! to C-track's (T6) extraction, not this T7 skeleton — `--corpus-extra`
//! gives an injection point for them (a dumped ModCache key list can be fed
//! straight in).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use pobr_core::item_text::strip_pob_annotations;
use pobr_gamedata::GameData;

/// Corpus source markers (a bit set, so one line can belong to multiple sources).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SourceSet {
    pub c1_build: bool,
    pub c2_tree: bool,
    pub sd_derived: bool,
    pub cx_extra: bool,
}

impl SourceSet {
    fn merge(&mut self, other: SourceSet) {
        self.c1_build |= other.c1_build;
        self.c2_tree |= other.c2_tree;
        self.sd_derived |= other.sd_derived;
        self.cx_extra |= other.cx_extra;
    }

    /// Primary source label for report grouping (priority C2 > C1 > SD > CX, one representative only).
    pub fn primary_label(&self) -> &'static str {
        if self.c2_tree {
            "C2_tree"
        } else if self.c1_build {
            "C1_build"
        } else if self.sd_derived {
            "SD_derived"
        } else {
            "CX_extra"
        }
    }
}

/// The collected corpus: lines in lexicographic order, plus each line's sources.
pub struct Corpus {
    /// Deduplicated (text, sources) pairs in lexicographic order.
    pub lines: Vec<(String, SourceSet)>,
}

impl Corpus {
    pub fn source_summary(&self) -> String {
        let (mut c1, mut c2, mut sd, mut cx) = (0usize, 0usize, 0usize, 0usize);
        for (_, s) in &self.lines {
            if s.c1_build {
                c1 += 1;
            }
            if s.c2_tree {
                c2 += 1;
            }
            if s.sd_derived {
                sd += 1;
            }
            if s.cx_extra {
                cx += 1;
            }
        }
        format!("C1={c1} C2={c2} SD={sd} CX={cx}")
    }
}

/// Collect all four corpus layers. `data_dir` is the version directory (e.g. `data/4.5.0.3.4`).
pub fn collect(data_dir: &Path, corpus_extra: Option<&Path>) -> Result<Corpus, String> {
    // text → source set (BTreeMap gives us dedup and lexicographic order for free).
    let mut map: BTreeMap<String, SourceSet> = BTreeMap::new();

    // C2: passive_tree node stats (most reliable, largest batch; self-contained within data).
    collect_passive_tree(data_dir, &mut map)?;

    // C1: build XML item text blocks (the main gate corpus).
    let examples = examples_dir(data_dir);
    if let Some(examples) = examples {
        collect_build_xml(&examples, &mut map)?;
    } else {
        eprintln!(
            "precompile-mods: warning -- examples/demo-bd-test/builds not found (skipping C1 build XML corpus)"
        );
    }

    // SD: special_derived expansion (pattern is a whole-line anchored corpus entry).
    collect_special_derived(data_dir, &mut map)?;

    // CX: add-on corpus.
    if let Some(extra) = corpus_extra {
        collect_extra(extra, &mut map)?;
    }

    let lines = map.into_iter().collect();
    Ok(Corpus { lines })
}

/// Register a normalized, non-empty text line in the map (merging sources).
fn add_line(map: &mut BTreeMap<String, SourceSet>, text: &str, src: SourceSet) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    map.entry(trimmed.to_string()).or_default().merge(src);
}

fn collect_passive_tree(
    data_dir: &Path,
    map: &mut BTreeMap<String, SourceSet>,
) -> Result<(), String> {
    let game = GameData::new(data_dir.to_path_buf());
    let nodes = game
        .passive_nodes()
        .map_err(|e| format!("failed to load passive_tree.json: {e}"))?;
    let src = SourceSet {
        c2_tree: true,
        ..Default::default()
    };
    for node in &nodes {
        for stat in &node.stats {
            add_line(map, stat, src);
        }
    }
    Ok(())
}

/// Infer `examples/demo-bd-test/builds` from the version data directory:
/// data_dir looks like `<repo>/data/<version>`, so go up two levels to the repo root.
fn examples_dir(data_dir: &Path) -> Option<PathBuf> {
    let repo_root = data_dir.parent()?.parent()?;
    let builds = repo_root.join("examples/demo-bd-test/builds");
    builds.is_dir().then_some(builds)
}

fn collect_build_xml(
    builds_dir: &Path,
    map: &mut BTreeMap<String, SourceSet>,
) -> Result<(), String> {
    let src = SourceSet {
        c1_build: true,
        ..Default::default()
    };
    let mut build_dirs: Vec<PathBuf> = std::fs::read_dir(builds_dir)
        .map_err(|e| format!("failed to read {}: {e}", builds_dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    build_dirs.sort();

    for dir in build_dirs {
        let xml = dir.join("decoded.xml");
        if !xml.is_file() {
            continue;
        }
        let content = std::fs::read_to_string(&xml)
            .map_err(|e| format!("failed to read {}: {e}", xml.display()))?;
        for line in extract_item_mod_lines(&content) {
            add_line(map, &line, src);
        }
    }
    Ok(())
}

/// Extract mod text lines from `<Item>` blocks in build XML.
///
/// A PoB2 item text block: between `<Item id="N">` and `</Item>` is the raw
/// item text — the first several lines are metadata (Rarity/name/Sockets/
/// Implicits count/…), followed by mod text lines (which may carry
/// `{enchant}{rune}{desecrated}` annotation prefixes). This extraction uses a
/// **conservative heuristic**: strip annotation prefixes, skip lines that are
/// obviously metadata/XML tags, and feed the rest to the parser as candidate
/// mod text (lines that fail to parse just count as coverage gaps and don't
/// affect correctness).
///
/// This is the corpus-collection surface for the T7 skeleton and **does not
/// aim to match the vendor item parser line-for-line** — precise item-block
/// parsing belongs to T6/T8; this tool only needs a stable, deduplicable set
/// of candidate lines.
fn extract_item_mod_lines(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_item = false;
    for raw in xml.lines() {
        let line = raw.trim();
        if line.starts_with("<Item ") || line == "<Item>" {
            in_item = true;
            continue;
        }
        if line.starts_with("</Item>") {
            in_item = false;
            continue;
        }
        if !in_item {
            continue;
        }
        // Skip nested XML tags inside the item block (<ModRange .../> etc.).
        if line.starts_with('<') {
            continue;
        }
        let cleaned = strip_pob_annotations(line);
        let cleaned = cleaned.trim();
        if cleaned.is_empty() {
            continue;
        }
        if is_item_metadata(cleaned) {
            continue;
        }
        out.push(cleaned.to_string());
    }
    out
}

/// Detects item-block metadata lines (a conservative prefix allowlist plus
/// known bare markers). A match means the line is not treated as mod text.
fn is_item_metadata(line: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "Rarity:",
        "Unique ID:",
        "Item Level:",
        "Quality:",
        "Sockets:",
        "Rune:",
        "Soul Core:",
        "LevelReq:",
        "Level Requirement:",
        "Requires Level",
        "Implicits:",
        "Energy Shield:",
        "Armour:",
        "Evasion Rating:",
        "Evasion:",
        "Ward:",
        "Physical Damage:",
        "Elemental Damage:",
        "Chaos Damage:",
        "Critical Strike Chance:",
        "Critical Hit Chance:",
        "Attacks per Second:",
        "Weapon Range:",
        "Radius:",
        "Limited to:",
        "Stack Size:",
        "Talisman Tier:",
        "Crafted:",
        "Prefix:",
        "Suffix:",
    ];
    const EXACT: &[&str] = &[
        "Corrupted",
        "Mirrored",
        "Split",
        "Unidentified",
        "Shaper Item",
        "Elder Item",
    ];
    if EXACT.iter().any(|m| line.eq_ignore_ascii_case(m)) {
        return true;
    }
    PREFIXES.iter().any(|p| line.starts_with(p))
}

/// `entries[].pattern` from special_derived.json, taken as whole-line corpus
/// entries (this is the text for special whole-line-anchored matches, e.g. keystone names).
fn collect_special_derived(
    data_dir: &Path,
    map: &mut BTreeMap<String, SourceSet>,
) -> Result<(), String> {
    let path = data_dir.join("generated/special_derived.json");
    if !path.is_file() {
        return Ok(());
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let doc: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("failed to parse special_derived.json: {e}"))?;
    let src = SourceSet {
        sd_derived: true,
        ..Default::default()
    };
    if let Some(entries) = doc.get("entries").and_then(|v| v.as_array()) {
        for entry in entries {
            if let Some(pat) = entry.get("pattern").and_then(|v| v.as_str()) {
                add_line(map, pat, src);
            }
        }
    }
    Ok(())
}

fn collect_extra(extra: &Path, map: &mut BTreeMap<String, SourceSet>) -> Result<(), String> {
    let content = std::fs::read_to_string(extra)
        .map_err(|e| format!("failed to read {}: {e}", extra.display()))?;
    let src = SourceSet {
        cx_extra: true,
        ..Default::default()
    };
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        add_line(map, line, src);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_metadata_filtered() {
        assert!(is_item_metadata("Rarity: RARE"));
        assert!(is_item_metadata("Item Level: 82"));
        assert!(is_item_metadata("Corrupted"));
        assert!(is_item_metadata("Sockets: S"));
        assert!(!is_item_metadata("35% increased Movement Speed"));
        assert!(!is_item_metadata("+30 to maximum Runic Ward"));
    }

    #[test]
    fn extract_strips_annotations_and_metadata() {
        let xml = r#"
        <Item id="1">
            Rarity: RARE
            Sandsworn Sandals
            Item Level: 82
            Implicits: 2
            {enchant}{rune}+30 to maximum Runic Ward
            {desecrated}35% increased Movement Speed
            Corrupted
            <ModRange range="0.5" id="1"/>
        </Item>
        "#;
        let lines = extract_item_mod_lines(xml);
        assert!(lines.contains(&"+30 to maximum Runic Ward".to_string()));
        assert!(lines.contains(&"35% increased Movement Speed".to_string()));
        // The name line (Sandsworn Sandals) isn't in the allowlist, so it
        // becomes a candidate line (parses as Unsupported, counted as a gap,
        // doesn't affect correctness); metadata is filtered out.
        assert!(!lines.iter().any(|l| l.starts_with("Rarity:")));
        assert!(!lines.iter().any(|l| l == "Corrupted"));
        assert!(!lines.iter().any(|l| l.starts_with("Item Level:")));
    }

    #[test]
    fn source_set_merge_union() {
        let mut a = SourceSet {
            c1_build: true,
            ..Default::default()
        };
        a.merge(SourceSet {
            c2_tree: true,
            ..Default::default()
        });
        assert!(a.c1_build && a.c2_tree);
        assert_eq!(a.primary_label(), "C2_tree");
    }
}
