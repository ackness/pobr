//! `extract-lua --what gem-quality`: extracts the `qualityStats` field from
//! vendor PoB2's `Data/Skills/*.lua` into `data/<version>/overlay/gem_quality_stats.json`
//!
//! **Channel note**: originally planned to come from the `.dat` table
//! `GrantedEffectQualityStats` via the adapter into `base/`, but the bundle
//! containing that table is no longer downloadable at the pinned patch
//! 4.5.0.3.4 (verified — see `_tablesUnavailableForPinnedPatch` in
//! `pipeline/config.json`). Per the owner's call to "let the producing tool
//! define the layer," it's extracted via extract-lua into **overlay/**
//! instead. The vendor data file is itself an export artifact (rate already
//! `/1000`, support gems already skipped per export conditions,
//! `Export/Scripts/skills.lua:304-313`), so the extraction is a faithful
//! transcription. If the `.dat` table channel comes back, this should
//! migrate back to `base/` (a byte-equivalent migration commit).
//!
//! Responsibility split matches [`crate::extract_lua`]: the Lua bootstrap
//! script (`extract_gem_quality.lua`, embedded at compile time) only does
//! faithful extraction and emits JSONL; the Rust side handles sorting
//! (ascending effect_id, preserving vendor order within each effect),
//! shortest-round-trip number representation, and whole-document
//! serialization, guaranteeing **byte-stable** output on repeated runs with the same input.
//!
//! NOTE(T2): the luajit invocation / `_meta` construction here overlaps a
//! bit with `extract_lua.rs` — this file belongs to the T2 (stat-map
//! extraction) owner, who can factor out the shared layer once T2 lands.

use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use pobr_data::catalog::{GemQualityStatDef, QualityStat};
use serde::{Deserialize, Serialize};

use crate::extract_lua::{ExtractLuaArgs, OverlayMeta};

/// Bootstrap script content (piped into luajit via stdin; the binary is
/// self-contained and doesn't depend on the working directory)
const BOOTSTRAP_LUA: &str = include_str!("extract_gem_quality.lua");

/// Current overlay document schema identifier (bumped when fields evolve)
pub const GEM_QUALITY_SCHEMA: &str = "gem_quality_stats/v1";

/// One JSONL line emitted by the bootstrap script: one (effect, stat, per-quality slope) triple.
#[derive(Debug, Clone, Deserialize)]
pub struct QualityRow {
    /// The granted effect id (e.g. `CometPlayer`).
    pub effect: String,
    /// The stable stat id.
    pub stat: String,
    /// The slope per 1 point of quality (vendor data already `/1000`, transcribed as-is).
    pub rate: f64,
    /// Whether this is a vendor `altQualityStats` entry (only active on GemlingQuality flag builds).
    #[serde(default)]
    pub alt: bool,
}

/// The full overlay document (generation side; see
/// [`pobr_data::catalog::GemQualityStatsDef`] for the consumption-side
/// schema — matching serde shapes guard against field drift).
#[derive(Debug, Serialize, Deserialize)]
pub struct GemQualityDoc {
    /// Header metadata (serialized as `_meta`, placed at the top of the file)
    #[serde(rename = "_meta")]
    pub meta: OverlayMeta,
    /// The quality stat table, ascending by effect_id.
    pub effects: Vec<GemQualityStatDef>,
}

/// Run the extraction, returning the final (byte-stable) JSON text.
pub fn run_extract_gem_quality(args: &ExtractLuaArgs) -> io::Result<String> {
    let rows = invoke_luajit(args)?;
    let meta = build_meta(args)?;
    Ok(assemble_quality_document(meta, rows))
}

/// Assemble the final document: group and sort by effect_id (preserving
/// arrival order = vendor order within each effect) + serde_json
/// serialization (identical input always yields identical output).
pub fn assemble_quality_document(meta: OverlayMeta, rows: Vec<QualityRow>) -> String {
    // Grouping: effect_id -> slope list (arrival order is vendor's ipairs order; not reordered within an effect).
    let mut by_effect: std::collections::BTreeMap<String, Vec<QualityStat>> =
        std::collections::BTreeMap::new();
    for row in rows {
        by_effect.entry(row.effect).or_default().push(QualityStat {
            stat: row.stat,
            per_quality_rate: row.rate,
            alt: row.alt,
        });
    }
    let effects = by_effect
        .into_iter()
        .map(|(effect_id, stats)| GemQualityStatDef { effect_id, stats })
        .collect();
    let doc = GemQualityDoc { meta, effects };
    let mut json = serde_json::to_string_pretty(&doc)
        .expect("gem quality document serialization should not fail");
    json.push('\n');
    json
}

/// Spawn luajit to run the bootstrap script (piped via stdin), and parse its JSONL output.
fn invoke_luajit(args: &ExtractLuaArgs) -> io::Result<Vec<QualityRow>> {
    if args.files.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "extract-lua --what gem-quality: --files must not be empty",
        ));
    }
    let mut child = Command::new(&args.luajit)
        .arg("-") // read the script from stdin
        .arg(&args.vendor_root)
        .arg(args.files.join(","))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "failed to launch luajit ({}): {error}; install luajit or specify the path via --luajit / POBR_LUAJIT",
                    args.luajit.display()
                ),
            )
        })?;

    child
        .stdin
        .take()
        .expect("stdin was configured as piped")
        .write_all(BOOTSTRAP_LUA.as_bytes())?;

    let output = child.wait_with_output()?;
    let stderr_text = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "luajit bootstrap script failed (exit: {:?}): {}",
            output.status.code(),
            stderr_text.trim()
        )));
    }
    // Pass through the bootstrap script's non-fatal warnings to the user
    for line in stderr_text.lines() {
        eprintln!("extract-lua(lua): {line}");
    }

    let stdout_text = String::from_utf8(output.stdout).map_err(io::Error::other)?;
    let mut rows = Vec::new();
    for line in stdout_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let row: QualityRow = serde_json::from_str(line).map_err(|error| {
            io::Error::other(format!(
                "bootstrap script emitted an invalid JSONL line: {error}; line content: {line}"
            ))
        })?;
        rows.push(row);
    }
    Ok(rows)
}

/// Read the vendor version file and build `_meta` (same convention as
/// `extract_lua::build_meta`: `regen_command` writes a canonical relative
/// path, so output is byte-identical when rerun on a different machine).
fn build_meta(args: &ExtractLuaArgs) -> io::Result<OverlayMeta> {
    let version_path = match &args.version_file {
        Some(path) => path.clone(),
        // The conventional layout is vendor/PathOfBuilding-PoE2/src -> the version file lives at vendor/.pob2-version.txt
        None => args.vendor_root.join("../../.pob2-version.txt"),
    };
    let (commit, subject) = read_vendor_version(&version_path)?;

    let extracted_files: Vec<String> = args
        .files
        .iter()
        .map(|name| format!("Data/Skills/{name}.lua"))
        .collect();

    let mut regen = format!(
        "cargo run -p sync-pob-catalog -- extract-lua --what gem-quality --vendor-root vendor/PathOfBuilding-PoE2/src --files {}",
        args.files.join(",")
    );
    if let Some(out) = &args.out_for_meta {
        regen.push_str(&format!(" --out {out}"));
    }

    Ok(OverlayMeta {
        schema: GEM_QUALITY_SCHEMA.to_string(),
        generator: "sync-pob-catalog extract-lua".to_string(),
        vendor: "PathOfBuilding-PoE2".to_string(),
        vendor_commit: commit,
        vendor_commit_subject: subject,
        extracted_files,
        regen_command: regen,
    })
}

/// Parse `.pob2-version.txt`: the first line is the commit subject, and the 40-hex-char line is the full hash.
fn read_vendor_version(version_path: &Path) -> io::Result<(String, String)> {
    let version_text = fs::read_to_string(version_path).map_err(|error| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "failed to read vendor version file {}: {error}; specify explicitly via --version-file",
                version_path.display()
            ),
        )
    })?;
    let subject = version_text.lines().next().unwrap_or("").trim().to_string();
    let commit = version_text
        .lines()
        .map(str::trim)
        .find(|line| line.len() == 40 && line.bytes().all(|b| b.is_ascii_hexdigit()))
        .unwrap_or("")
        .to_string();
    if commit.is_empty() {
        return Err(io::Error::other(format!(
            "no 40-character commit hash found in vendor version file {}",
            version_path.display()
        )));
    }
    Ok((commit, subject))
}
