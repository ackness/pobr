//! `extract-bases` subcommand: runs vendor PoB2's `Data/Bases/*.lua` under
//! luajit in a minimal stub environment, and captures base-item columns that
//! the GGG `.dat` route can't reach (`ShieldTypes.Block` -> `block_chance`,
//! `ItemSpirit.SpiritGranted` -> `spirit`) as **deterministic JSON** written
//! to `data/<version>/overlay/base_item_overrides.json` — the vendor
//! extraction fallback for open questions 1/2 (structured the same way as the `skill_overrides` channel).
//!
//! Responsibility split (same as [`crate::extract_lua`]):
//! - The Lua bootstrap script (`extract_base_overrides.lua`, embedded at
//!   compile time) only does faithful extraction and emits JSONL;
//! - The Rust side does the sorting (ascending by name) and whole-document
//!   serialization, guaranteeing **byte-stable** output on repeated runs with the same input.

use std::io::{self, Write};
use std::process::{Command, Stdio};

use serde::Serialize;

use crate::extract_lua::{ExtractLuaArgs, OverlayMeta, build_overlay_meta};

/// Bootstrap script content (piped into luajit via stdin; the binary is
/// self-contained and doesn't depend on the working directory)
const BOOTSTRAP_LUA: &str = include_str!("extract_base_overrides.lua");

/// Default vendor base data files to extract: since adding the `tags` field
/// (the full base tag set, needed for affix-tier reverse-lookup spawn-weight
/// checks), this covers **all of Data/Bases** — every equippable base needs
/// its full tag set, and block/spirit/reload/charm_buff each only appear in
/// individual files, so extracting everything covers them naturally.
pub const DEFAULT_BASE_FILES: &[&str] = &[
    "amulet",
    "axe",
    "belt",
    "body",
    "boots",
    "bow",
    "claw",
    "crossbow",
    "dagger",
    "fishing",
    "flail",
    "flask",
    "focus",
    "gloves",
    "helmet",
    "incursionlimb",
    "jewel",
    "mace",
    "quiver",
    "ring",
    "sceptre",
    "shield",
    "spear",
    "staff",
    "sword",
    "talisman",
    "traptool",
    "wand",
];

/// Current overlay document schema identifier (bumped when fields evolve)
pub const BASE_ITEM_OVERRIDES_SCHEMA: &str = "base_item_overrides/v1";

/// A single base-item override — the single source of truth for its shape is
/// [`pobr_data::catalog::base_item_overrides::BaseItemOverrideEntry`]
/// (shared serde shape between generation and consumption sides, so fields can't drift).
pub use pobr_data::catalog::base_item_overrides::BaseItemOverrideEntry;

/// The full overlay document (generation side; see
/// `pobr_data::catalog::base_item_overrides::BaseItemOverridesDef` for the consumption-side shape).
#[derive(Debug, Serialize)]
pub struct BaseItemOverridesDoc {
    /// Header metadata (serialized as `_meta`, placed at the top of the file)
    #[serde(rename = "_meta")]
    pub meta: OverlayMeta,
    /// The override list, ascending by `name`
    pub overrides: Vec<BaseItemOverrideEntry>,
}

/// Run the extraction, returning the final (byte-stable) JSON text
pub fn run_extract_bases(args: &ExtractLuaArgs) -> io::Result<String> {
    let entries = invoke_luajit(args)?;
    let meta = build_overlay_meta(
        args,
        BASE_ITEM_OVERRIDES_SCHEMA,
        "sync-pob-catalog extract-bases",
        "Data/Bases",
        "extract-bases",
    )?;
    Ok(assemble_base_overrides_document(meta, entries))
}

/// Assemble the final document: sort + serde_json serialization (identical input always yields identical output)
pub fn assemble_base_overrides_document(
    meta: OverlayMeta,
    mut entries: Vec<BaseItemOverrideEntry>,
) -> String {
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    let doc = BaseItemOverridesDoc {
        meta,
        overrides: entries,
    };
    let mut json = serde_json::to_string_pretty(&doc)
        .expect("base item overrides document serialization should not fail");
    json.push('\n');
    json
}

/// Spawn luajit to run the bootstrap script (piped via stdin), and parse its JSONL output
fn invoke_luajit(args: &ExtractLuaArgs) -> io::Result<Vec<BaseItemOverrideEntry>> {
    if args.files.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "extract-bases: --files must not be empty",
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
        eprintln!("extract-bases(lua): {line}");
    }

    let stdout_text = String::from_utf8(output.stdout).map_err(io::Error::other)?;
    let mut entries = Vec::new();
    for line in stdout_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let entry: BaseItemOverrideEntry = serde_json::from_str(line).map_err(|error| {
            io::Error::other(format!(
                "bootstrap script emitted an invalid JSONL line: {error}; line content: {line}"
            ))
        })?;
        entries.push(entry);
    }
    Ok(entries)
}

/// Used by main to assemble the default file name list.
pub fn default_base_files() -> Vec<String> {
    DEFAULT_BASE_FILES.iter().map(|s| s.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Document assembly: sorted by name + byte-stable (two assemblies of the same input are byte-identical).
    #[test]
    fn assembles_sorted_and_byte_stable() {
        let meta = OverlayMeta {
            schema: BASE_ITEM_OVERRIDES_SCHEMA.to_string(),
            generator: "test".to_string(),
            vendor: "PathOfBuilding-PoE2".to_string(),
            vendor_commit: "0".repeat(40),
            vendor_commit_subject: "subject".to_string(),
            extracted_files: vec!["Data/Bases/shield.lua".to_string()],
            regen_command: "cargo run -p sync-pob-catalog -- extract-bases".to_string(),
        };
        let entries = vec![
            BaseItemOverrideEntry {
                req_str: None,
                req_dex: None,
                req_int: None,
                name: "Omen Sceptre".to_string(),
                block_chance: None,
                spirit: Some(100),
                reload_time_ms: None,
                charm_buff: None,
                tags: None,
            },
            BaseItemOverrideEntry {
                req_str: None,
                req_dex: None,
                req_int: None,
                name: "Crude Tower Shield".to_string(),
                block_chance: Some(26.0),
                spirit: None,
                reload_time_ms: None,
                charm_buff: None,
                tags: None,
            },
            BaseItemOverrideEntry {
                req_str: None,
                req_dex: None,
                req_int: None,
                name: "Makeshift Crossbow".to_string(),
                block_chance: None,
                spirit: None,
                reload_time_ms: Some(800),
                charm_buff: None,
                tags: None,
            },
            BaseItemOverrideEntry {
                req_str: None,
                req_dex: None,
                req_int: None,
                name: "Ruby Charm".to_string(),
                block_chance: None,
                spirit: None,
                reload_time_ms: None,
                charm_buff: Some(vec!["+25% to Fire Resistance".to_string()]),
                tags: None,
            },
        ];
        let a = assemble_base_overrides_document(meta.clone(), entries.clone());
        let b = assemble_base_overrides_document(meta, entries);
        assert_eq!(a, b);
        // Sort order: Crude... < Makeshift... < Omen... < Ruby...
        let crude = a.find("Crude Tower Shield").unwrap();
        let makeshift = a.find("Makeshift Crossbow").unwrap();
        let omen = a.find("Omen Sceptre").unwrap();
        let ruby = a.find("Ruby Charm").unwrap();
        assert!(crude < makeshift && makeshift < omen && omen < ruby);
        // The consumption-side schema can parse the generation-side output (guards against field drift).
        let parsed: pobr_data::catalog::base_item_overrides::BaseItemOverridesDef =
            serde_json::from_str(&a).unwrap();
        assert_eq!(parsed.overrides.len(), 4);
        assert_eq!(parsed.overrides[0].block_chance, Some(26.0));
        assert_eq!(parsed.overrides[1].reload_time_ms, Some(800));
        assert_eq!(parsed.overrides[2].spirit, Some(100));
        assert_eq!(
            parsed.overrides[3].charm_buff.as_deref(),
            Some(["+25% to Fire Resistance".to_string()].as_slice())
        );
    }
}
