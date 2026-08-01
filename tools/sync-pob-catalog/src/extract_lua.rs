//! `extract-lua` subcommand: runs vendor PoB2's Lua data files under luajit
//! in a minimal stub environment, capturing the hand-curated layer (Export
//! template #baseMod / per-skill overrides, etc.) as **deterministic JSON**
//! written to `data/<version>/overlay/`, replacing the one-off patch of
//! "bypass the adapter and hand-edit the output JSON".
//!
//! Responsibility split:
//! - The Lua bootstrap script (`extract_skill_overrides.lua`, embedded at
//!   compile time) only does faithful extraction and emits JSONL;
//! - The Rust side handles sorting, number formatting (serde_json's
//!   shortest round-trip representation), and whole-document serialization,
//!   guaranteeing **byte-stable** output on repeated runs with the same input.
//!
//! This module also carries the **shared layer** for every `--what`
//! extraction target (the luajit JSONL invocation
//! [`invoke_luajit_jsonl`] / vendor version parsing [`read_vendor_version`]);
//! see [`crate::extract_stat_map`] and [`crate::extract_quality`] for the other targets.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

/// Bootstrap script content (piped into luajit via stdin; the binary is
/// self-contained and doesn't depend on the working directory)
const BOOTSTRAP_LUA: &str = include_str!("extract_skill_overrides.lua");

/// Default luajit path (macOS Homebrew); overridable via `--luajit` or `POBR_LUAJIT`
const DEFAULT_LUAJIT_HOMEBREW: &str = "/opt/homebrew/bin/luajit";

/// Default vendor skill data files to extract: the three active-skill
/// classes (dex/int/str) plus minion / spectre / other plus the three
/// support-gem classes — covering every skill source that carries per-skill
/// values the `.dat` channel can't reach (per-level baseMultiplier values /
/// statSet baseMods Speed MORE); the consumption-side merge needs the full
/// set, since missing one class drops values. As of the critChance /
/// attackSpeedMultiplier switch to reading directly from `.dat` table
/// columns, those no longer go through this channel (see the header comment
/// in `extract_skill_overrides.lua`).
pub const DEFAULT_SKILL_FILES: &[&str] = &[
    "act_dex", "act_int", "act_str", "minion", "other", "spectre", "sup_dex", "sup_int", "sup_str",
];

/// `--what stat-map`'s ([`crate::extract_stat_map`]) default `Data/Skills/`
/// files to extract: the three active-skill classes plus the three
/// support-gem classes plus other. **Excludes** minion / spectre — summon statMap is left for later.
pub const DEFAULT_STAT_MAP_SKILL_FILES: &[&str] = &[
    "act_dex", "act_int", "act_str", "other", "sup_dex", "sup_int", "sup_str",
];

/// Current overlay document schema identifier (bumped when fields evolve)
pub const SKILL_OVERRIDES_SCHEMA: &str = "skill_overrides/v1";

/// Resolve the luajit executable path: explicit argument > `POBR_LUAJIT`
/// environment variable > the Homebrew default path (if it exists) > `luajit` on PATH.
pub fn resolve_luajit(explicit: Option<&Path>) -> PathBuf {
    if let Some(path) = explicit {
        return path.to_path_buf();
    }
    if let Ok(env_path) = std::env::var("POBR_LUAJIT")
        && !env_path.is_empty()
    {
        return PathBuf::from(env_path);
    }
    let homebrew = Path::new(DEFAULT_LUAJIT_HOMEBREW);
    if homebrew.exists() {
        return homebrew.to_path_buf();
    }
    PathBuf::from("luajit")
}

/// Whether luajit is runnable (tests skip based on this when CI has no luajit)
pub fn luajit_available(luajit: &Path) -> bool {
    Command::new(luajit)
        .arg("-v")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// extract-lua's run parameters
#[derive(Debug)]
pub struct ExtractLuaArgs {
    /// vendor PoB2 source directory (`vendor/PathOfBuilding-PoE2/src`, read-only input)
    pub vendor_root: PathBuf,
    /// The luajit executable path
    pub luajit: PathBuf,
    /// Skill data file names to extract (without the `.lua` suffix)
    pub files: Vec<String>,
    /// The vendor version record file; defaults to `<vendor_root>/../../.pob2-version.txt`
    pub version_file: Option<PathBuf>,
    /// The `--out` value written into `_meta.regen_command` (recorded only;
    /// this layer doesn't write to disk). By convention this is already a
    /// canonical repo-relative path — the caller normalizes it via
    /// [`canonical_out_for_meta`] (F1) where it's assigned, decoupled from the actual write path.
    pub out_for_meta: Option<String>,
}

/// A single per-skill override — the single source of truth for its shape is
/// [`pobr_data::catalog::skill_overrides::SkillOverrideEntry`] (shared serde
/// shape between generation and consumption, so fields can't drift); the
/// original name is kept here as a re-export for backward compatibility.
pub use pobr_data::catalog::skill_overrides::SkillOverrideEntry as SkillOverride;

/// Overlay document header metadata: records the vendor version and the
/// regen command, keeping the artifact traceable and reproducible
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayMeta {
    /// Schema identifier
    pub schema: String,
    /// Generator identifier
    pub generator: String,
    /// The vendor repo name
    pub vendor: String,
    /// The full vendor commit hash (read from `.pob2-version.txt`)
    pub vendor_commit: String,
    /// The vendor commit subject line (human-readable cross-reference)
    pub vendor_commit_subject: String,
    /// The vendor files actually extracted (relative to the vendor src root)
    pub extracted_files: Vec<String>,
    /// The regen command (run from the repo root; the vendor path is written as a canonical relative path by convention)
    pub regen_command: String,
}

/// The full overlay document
#[derive(Debug, Serialize, Deserialize)]
pub struct SkillOverridesDoc {
    /// Header metadata (serialized as `_meta`, placed at the top of the file)
    #[serde(rename = "_meta")]
    pub meta: OverlayMeta,
    /// The override list, sorted by (skill, stat, stat_set)
    pub overrides: Vec<SkillOverride>,
}

/// Run the extraction, returning the final (byte-stable) JSON text
pub fn run_extract_lua(args: &ExtractLuaArgs) -> io::Result<String> {
    let entries = invoke_luajit(args)?;
    let meta = build_meta(args)?;
    Ok(assemble_overrides_document(meta, entries))
}

/// Assemble the final document: sort + serde_json serialization (identical input always yields identical output)
pub fn assemble_overrides_document(meta: OverlayMeta, mut entries: Vec<SkillOverride>) -> String {
    entries.sort_by(|a, b| {
        a.skill
            .cmp(&b.skill)
            .then_with(|| a.stat.cmp(&b.stat))
            .then_with(|| a.stat_set.unwrap_or(0).cmp(&b.stat_set.unwrap_or(0)))
    });
    let doc = SkillOverridesDoc {
        meta,
        overrides: entries,
    };
    let mut json = serde_json::to_string_pretty(&doc)
        .expect("skill overrides document serialization should not fail");
    json.push('\n');
    json
}

/// Spawn luajit to run the bootstrap script (piped via stdin), and parse its JSONL output
fn invoke_luajit(args: &ExtractLuaArgs) -> io::Result<Vec<SkillOverride>> {
    invoke_luajit_jsonl(args, BOOTSTRAP_LUA)
}

/// Generic luajit JSONL extraction: pipes in the given bootstrap script
/// (with the conventional arguments `<vendor_src_dir> <comma-separated
/// file names>`), parses stdout line by line into `T`, and passes through
/// non-fatal stderr warnings. Every `--what` target (skill-overrides /
/// stat-map / gem-quality) shares this layer, avoiding the same luajit-invocation code drifting in three places.
pub fn invoke_luajit_jsonl<T: serde::de::DeserializeOwned>(
    args: &ExtractLuaArgs,
    bootstrap: &str,
) -> io::Result<Vec<T>> {
    if args.files.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "extract-lua: --files must not be empty",
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
        .write_all(bootstrap.as_bytes())?;

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
    let mut entries = Vec::new();
    for line in stdout_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let entry: T = serde_json::from_str(line).map_err(|error| {
            io::Error::other(format!(
                "bootstrap script emitted an invalid JSONL line: {error}; line content: {line}"
            ))
        })?;
        entries.push(entry);
    }
    Ok(entries)
}

/// The canonical overlay artifact file name for each extraction target
/// (`--what` target / subcommand alias -> the conventional file under
/// `data/<version>/overlay/`). The fallback lookup table for F1 normalization.
fn canonical_overlay_file(target: &str) -> Option<&'static str> {
    Some(match target {
        "skill-overrides" => "skill_overrides.json",
        "gem-quality" => "gem_quality_stats.json",
        "stat-map" => "skill_stat_map.json",
        "stat-descriptions" => "stat_descriptions.json",
        "stat-id-map" => "stat_id_map.json",
        "gem-effects" => "gem_effects.json",
        "stat-set-labels" => "stat_set_labels.json",
        "config-options" => "config_options.json",
        "curse-priority" => "curse_priority.json",
        "minions" => "minions.json",
        "spectres" => "spectres.json",
        "minion-list" => "granted_effect_minions.json",
        "mod-scalability" => "mod_scalability.json",
        "runes" => "runes.json",
        "uniques" => "uniques.json",
        "catalysts" => "catalysts.json",
        "parser-rules" => "mod_parser_rules.json",
        // Subcommand aliases (not `--what` values): extract-bases / gen-mirage-configs
        "bases" => "base_item_overrides.json",
        "mirage-configs" => "mirage_configs.json",
        _ => return None,
    })
}

/// Whether a path component looks like a PoE patch version number (`4.5.0.3.4`: dot-separated digits, at least two segments).
fn looks_like_version(component: &str) -> bool {
    let parts: Vec<&str> = component.split('.').collect();
    parts.len() >= 2
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

/// F1 (a drill finding): normalizes the actual passed-in `--out` path into a
/// **canonical repo-relative path** for `_meta.regen_command` to record —
/// decoupled from the caller's argument (an absolute / temp path), so
/// replaying to any location no longer produces a self-referential `_meta` diff.
///
/// Normalization rules (in order):
/// 1. If the out path has a `data` component (with content after it) ->
///    take the last `data/...` relative segment (normalizing to `/`
///    separators; a temp replay only needs to preserve the
///    `data/<ver>/overlay/<file>` structure to recover the canonical form);
/// 2. Otherwise, use the extraction target's canonical default path table
///    `data/<version>/overlay/<file>`, deriving `<version>` from the out
///    path or the `--version-file` path component;
/// 3. If it can't be derived -> `None` (`regen_command` omits `--out`).
pub fn canonical_out_for_meta(
    out: Option<&Path>,
    target: &str,
    version_file: Option<&Path>,
) -> Option<String> {
    let out = out?;
    let comps: Vec<&str> = out.iter().filter_map(|c| c.to_str()).collect();
    if let Some(pos) = comps.iter().rposition(|c| *c == "data")
        && pos + 1 < comps.len()
    {
        return Some(comps[pos..].join("/"));
    }
    let file = canonical_overlay_file(target)?;
    let version = comps
        .iter()
        .copied()
        .find(|c| looks_like_version(c))
        .map(str::to_string)
        .or_else(|| {
            version_file?
                .iter()
                .filter_map(|c| c.to_str())
                .find(|c| looks_like_version(c))
                .map(str::to_string)
        })?;
    Some(format!("data/{version}/overlay/{file}"))
}

/// Resolve the vendor version file path: an explicit `--version-file` takes
/// priority, otherwise follows the conventional layout
/// `vendor/PathOfBuilding-PoE2/src` -> `vendor/.pob2-version.txt`.
pub fn resolve_version_file(args: &ExtractLuaArgs) -> PathBuf {
    match &args.version_file {
        Some(path) => path.clone(),
        None => args.vendor_root.join("../../.pob2-version.txt"),
    }
}

/// Parse `.pob2-version.txt`: the first line is the commit subject, and the
/// 40-hex-char line is the full hash. Returns `(commit, subject)`. Shared by every `--what` target.
pub fn read_vendor_version(version_path: &Path) -> io::Result<(String, String)> {
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

/// Read the vendor version file and build `_meta` (a thin wrapper with skill_overrides-specific arguments)
fn build_meta(args: &ExtractLuaArgs) -> io::Result<OverlayMeta> {
    build_overlay_meta(
        args,
        SKILL_OVERRIDES_SCHEMA,
        "sync-pob-catalog extract-lua",
        "Data/Skills",
        "extract-lua",
    )
}

/// Read the vendor version file and build `_meta` for any overlay document
/// (shared by extract-lua / extract-bases: schema / generator / vendor file
/// directory prefix / subcommand name are all parameterized).
pub fn build_overlay_meta(
    args: &ExtractLuaArgs,
    schema: &str,
    generator: &str,
    file_dir_prefix: &str,
    subcommand: &str,
) -> io::Result<OverlayMeta> {
    let (commit, subject) = read_vendor_version(&resolve_version_file(args))?;

    let extracted_files: Vec<String> = args
        .files
        .iter()
        .map(|name| format!("{file_dir_prefix}/{name}.lua"))
        .collect();

    // regen_command writes a canonical relative path by convention (run
    // from the repo root), decoupled from the actual absolute path passed
    // in, so output stays byte-identical across machines / any out location
    // -- vendor-root is fixed here; out has already been normalized by the caller via canonical_out_for_meta (F1).
    let mut regen = format!(
        "cargo run -p sync-pob-catalog -- {subcommand} --vendor-root vendor/PathOfBuilding-PoE2/src --files {}",
        args.files.join(",")
    );
    if let Some(out) = &args.out_for_meta {
        regen.push_str(&format!(" --out {out}"));
    }

    Ok(OverlayMeta {
        schema: schema.to_string(),
        generator: generator.to_string(),
        vendor: "PathOfBuilding-PoE2".to_string(),
        vendor_commit: commit,
        vendor_commit_subject: subject,
        extracted_files,
        regen_command: regen,
    })
}

#[cfg(test)]
mod tests {
    use super::canonical_out_for_meta;
    use std::path::Path;

    /// Rule 1: an out path with a `data/` component -> take the relative
    /// segment (both absolute paths and temp replays recover the canonical
    /// form, independent of the machine or temp directory).
    #[test]
    fn truncates_at_last_data_component() {
        for raw in [
            "data/4.5.0.3.4/overlay/curse_priority.json",
            "/Users/x/codes/pobr/data/4.5.0.3.4/overlay/curse_priority.json",
            "/tmp/pobr-drill.AbC/replay/data/4.5.0.3.4/overlay/curse_priority.json",
        ] {
            assert_eq!(
                canonical_out_for_meta(Some(Path::new(raw)), "curse-priority", None).as_deref(),
                Some("data/4.5.0.3.4/overlay/curse_priority.json"),
                "out = {raw}"
            );
        }
    }

    /// Rule 2: a temp path with no `data/` component -> falls back to the what-target default table plus a version number found in the path.
    #[test]
    fn falls_back_to_what_target_table_with_version_from_path() {
        assert_eq!(
            canonical_out_for_meta(
                Some(Path::new("/tmp/4.5.0.3.4/regen.json")),
                "minion-list",
                None,
            )
            .as_deref(),
            Some("data/4.5.0.3.4/overlay/granted_effect_minions.json"),
        );
        // The version number can also be derived from the --version-file path component
        assert_eq!(
            canonical_out_for_meta(
                Some(Path::new("/tmp/regen.json")),
                "bases",
                Some(Path::new("/snapshots/4.5.0.3.4/.pob2-version.txt")),
            )
            .as_deref(),
            Some("data/4.5.0.3.4/overlay/base_item_overrides.json"),
        );
    }

    /// Rule 3: the version can't be derived / the extraction target is unknown -> None (regen_command omits --out).
    #[test]
    fn omits_out_when_underivable() {
        assert_eq!(
            canonical_out_for_meta(Some(Path::new("/tmp/regen.json")), "curse-priority", None),
            None,
        );
        assert_eq!(
            canonical_out_for_meta(
                Some(Path::new("/tmp/4.5.0.3.4/x.json")),
                "no-such-target",
                None,
            ),
            None,
        );
        assert_eq!(canonical_out_for_meta(None, "curse-priority", None), None);
    }

    /// A trailing bare `data` component doesn't trigger rule 1 (no relative segment to take).
    #[test]
    fn trailing_bare_data_component_is_ignored() {
        assert_eq!(
            canonical_out_for_meta(Some(Path::new("/tmp/4.5.0.3.4/data")), "runes", None)
                .as_deref(),
            Some("data/4.5.0.3.4/overlay/runes.json"),
        );
    }
}
