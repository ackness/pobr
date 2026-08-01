//! BuildRaw round-trip golden regression (the edit view has no parity target to check against).
//!
//! Hard contract (gate): for every `<Item>` text block in real ninja builds,
//! `parse(build_raw(parse(x))) == parse(x)` (a semantic fixed point).
//!
//! Corpus = every `<Item>` block embedded in
//! `examples/demo-bd-test/builds/*/decoded.xml` (~360 items), covering
//! rare/unique/jewel, the Implicits header, `{enchant}{rune}`/`{fractured}`/`{desecrated}`
//! annotations, Sockets/Rune, defence items, catalysts and other real-world shapes.
//!
//! Byte contract (a report, not a gate): prints a byte-diff count between
//! `build_raw(parse(x))` and normalized x; trending to zero is the aspiration,
//! but PoB2's own BuildRaw doesn't guarantee byte-stability either, so this
//! isn't hard-asserted.

use pobr_item::ItemDraft;
use std::path::PathBuf;

/// Locates `examples/demo-bd-test/builds` (two levels up from the crate manifest dir to the workspace root).
fn builds_dir() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // crates/pobr-item -> workspace root
    manifest
        .parent()
        .and_then(|p| p.parent())
        .map(|root| root.join("examples/demo-bd-test/builds"))
        .expect("workspace root")
}

/// Extracts every `<Item ...>...</Item>` item text block from decoded.xml.
///
/// A PoB XML `<Item>`'s body is HTML-escaped item text (`&apos;`, etc.) plus
/// a trailing `<ModRange/>` child element. Extraction takes the plain-text
/// span from `<Item ...>` up to the first `<` (a child element or closing
/// tag) and unescapes it.
fn extract_item_blocks(xml: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut rest = xml;
    while let Some(open) = rest.find("<Item ") {
        let after_open = &rest[open..];
        // Skip past the opening tag itself to `>`.
        let Some(gt) = after_open.find('>') else {
            break;
        };
        let body_start = open + gt + 1;
        let body_region = &rest[body_start..];
        // Item text runs up to the first embedded child element (`<ModRange`/`</Item`).
        let text_end = body_region.find('<').unwrap_or(body_region.len());
        let raw_text = &body_region[..text_end];
        let unescaped = unescape_xml(raw_text);
        if unescaped.contains("Rarity:") {
            blocks.push(unescaped);
        }
        // Advance past the end of this Item.
        let Some(close) = body_region.find("</Item>") else {
            break;
        };
        rest = &body_region[close + "</Item>".len()..];
    }
    blocks
}

fn unescape_xml(s: &str) -> String {
    s.replace("&apos;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// Collects the corpus: every Item block from every build.
fn corpus() -> Vec<(String, String)> {
    let dir = builds_dir();
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let xml_path = entry.path().join("decoded.xml");
        if let Ok(xml) = std::fs::read_to_string(&xml_path) {
            let build_name = entry.file_name().to_string_lossy().to_string();
            for (i, block) in extract_item_blocks(&xml).into_iter().enumerate() {
                out.push((format!("{build_name}#{i}"), block));
            }
        }
    }
    out
}

#[test]
fn corpus_is_non_empty() {
    let c = corpus();
    assert!(
        c.len() >= 100,
        "expected ≥100 real item blocks, got {} (builds dir = {:?})",
        c.len(),
        builds_dir()
    );
}

#[test]
fn semantic_fixpoint_roundtrip_all_real_items() {
    let mut failures = Vec::new();
    let corpus = corpus();
    assert!(!corpus.is_empty(), "empty corpus");

    for (id, raw) in &corpus {
        let Ok(d1) = ItemDraft::parse(raw) else {
            failures.push(format!("{id}: initial parse failed"));
            continue;
        };
        let rebuilt = d1.build_raw();
        let Ok(d2) = ItemDraft::parse(&rebuilt) else {
            failures.push(format!("{id}: re-parse of build_raw failed"));
            continue;
        };
        if d1 != d2 {
            failures.push(format!(
                "{id}: NOT a fixpoint\n--- d1.build_raw ---\n{rebuilt}\n--- d2.build_raw ---\n{}",
                d2.build_raw()
            ));
        }
    }

    if !failures.is_empty() {
        let shown: Vec<_> = failures.iter().take(5).collect();
        panic!(
            "{} / {} item blocks failed semantic-fixpoint roundtrip; first {}:\n{}",
            failures.len(),
            corpus.len(),
            shown.len(),
            shown
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("\n\n")
        );
    }
}

/// Byte-contract report (not a gate): counts line-by-line diffs between build_raw and the normalized original.
#[test]
fn byte_diff_report() {
    let corpus = corpus();
    let mut exact = 0usize;
    let mut diff = 0usize;
    for (_, raw) in &corpus {
        let Ok(d) = ItemDraft::parse(raw) else {
            continue;
        };
        let rebuilt = d.build_raw();
        if normalize(raw) == normalize(&rebuilt) {
            exact += 1;
        } else {
            diff += 1;
        }
    }
    eprintln!(
        "[build_raw byte-diff report] exact={exact} diff={diff} total={}",
        corpus.len()
    );
}

fn normalize(s: &str) -> Vec<String> {
    s.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}
