//! `extract-lua --what config-options`: probe-based extraction of `ConfigOptions.lua`.
//!
//! Unlike the other `--what` targets, this one needs a **full PoB2 headless
//! environment** (HeadlessWrapper: real `data` / `modLib` / `LoadModule`), so
//! luajit is launched with `cwd = <vendor_root>` and `LUA_PATH` pointing at
//! `runtime/lua` (the same bootstrap as `tools/pob2-oracle/run.sh`), rather
//! than through [`crate::extract_lua::invoke_luajit_jsonl`]'s cwd-less channel.
//!
//! Responsibility split (the deterministic-extraction convention):
//! - The Lua bootstrap script (`extract_config_options.lua`, embedded at
//!   compile time) does the probing and emits one serde-shaped entry per
//!   line (JSONL);
//! - This module handles launching / parsing / sorting by `var` / assembling
//!   `_meta` / byte-stable serialization.

use std::io::{self, Write};
use std::process::{Command, Stdio};

use pobr_data::catalog::config_def::{CONFIG_OPTIONS_SCHEMA, ConfigOptionDef};
use serde::{Deserialize, Serialize};

use crate::extract_lua::{ExtractLuaArgs, OverlayMeta, read_vendor_version, resolve_version_file};

/// Bootstrap script content (piped into luajit via stdin).
const BOOTSTRAP_LUA: &str = include_str!("extract_config_options.lua");

/// The full overlay document (production side; the consumption side uses `ConfigOptionsDef` and ignores `_meta`).
#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigOptionsDoc {
    /// Header metadata.
    #[serde(rename = "_meta")]
    pub meta: OverlayMeta,
    /// The entry list, ascending by `var`.
    pub options: Vec<ConfigOptionDef>,
}

/// Run the extraction, returning the final (byte-stable) JSON text.
pub fn run_extract_config_options(args: &ExtractLuaArgs) -> io::Result<String> {
    let entries = invoke_headless_luajit(args)?;
    let meta = build_meta(args)?;
    Ok(assemble_document(meta, entries))
}

/// Assemble the final document: sort by var + serde_json serialization (identical input always yields identical output).
pub fn assemble_document(meta: OverlayMeta, mut entries: Vec<ConfigOptionDef>) -> String {
    entries.sort_by(|a, b| a.var.cmp(&b.var).then_with(|| a.section.cmp(&b.section)));
    let doc = ConfigOptionsDoc {
        meta,
        options: entries,
    };
    let mut json = serde_json::to_string_pretty(&doc)
        .expect("config options document serialization should not fail");
    json.push('\n');
    json
}

/// Launch luajit in headless mode (cwd = vendor src, LUA_PATH points at runtime/lua).
fn invoke_headless_luajit(args: &ExtractLuaArgs) -> io::Result<Vec<ConfigOptionDef>> {
    let vendor_root = &args.vendor_root;
    if !vendor_root.join("HeadlessWrapper.lua").exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "HeadlessWrapper.lua not found under vendor root {} (config-options extraction needs a full PoB2 src)",
                vendor_root.display()
            ),
        ));
    }
    let mut child = Command::new(&args.luajit)
        .arg("-") // read the script from stdin
        .current_dir(vendor_root)
        .env(
            "LUA_PATH",
            "../runtime/lua/?.lua;../runtime/lua/?/init.lua;./?.lua;;",
        )
        .env("CI", "true")
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
    for line in stderr_text.lines() {
        eprintln!("extract-config-options(lua): {line}");
    }

    let stdout_text = String::from_utf8(output.stdout).map_err(io::Error::other)?;
    let mut entries = Vec::new();
    for line in stdout_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let entry: ConfigOptionDef = serde_json::from_str(line).map_err(|error| {
            io::Error::other(format!(
                "bootstrap script emitted an invalid entry JSON: {error}; line content: {line}"
            ))
        })?;
        entries.push(entry);
    }
    if entries.is_empty() {
        return Err(io::Error::other(
            "config-options extraction produced 0 entries (bootstrap script anomaly)",
        ));
    }
    Ok(entries)
}

/// Build `_meta` (vendor commit + canonical regen command).
fn build_meta(args: &ExtractLuaArgs) -> io::Result<OverlayMeta> {
    let (commit, subject) = read_vendor_version(&resolve_version_file(args))?;
    let mut regen = String::from(
        "cargo run -p sync-pob-catalog -- extract-lua --vendor-root vendor/PathOfBuilding-PoE2/src --what config-options",
    );
    if let Some(out) = &args.out_for_meta {
        regen.push_str(&format!(" --out {out}"));
    }
    Ok(OverlayMeta {
        schema: CONFIG_OPTIONS_SCHEMA.to_string(),
        generator: "sync-pob-catalog extract-lua --what config-options".to_string(),
        vendor: "PathOfBuilding-PoE2".to_string(),
        vendor_commit: commit,
        vendor_commit_subject: subject,
        extracted_files: vec!["Modules/ConfigOptions.lua".to_string()],
        regen_command: regen,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta() -> OverlayMeta {
        OverlayMeta {
            schema: CONFIG_OPTIONS_SCHEMA.to_string(),
            generator: "test".to_string(),
            vendor: "PathOfBuilding-PoE2".to_string(),
            vendor_commit: "0".repeat(40),
            vendor_commit_subject: "test".to_string(),
            extracted_files: vec!["Modules/ConfigOptions.lua".to_string()],
            regen_command: "test".to_string(),
        }
    }

    fn entry(var: &str) -> ConfigOptionDef {
        serde_json::from_str(&format!(
            r#"{{"var":"{var}","input_type":"check","section":"General","verified":true}}"#
        ))
        .unwrap()
    }

    /// Assembly is sorted by var and byte-stable (two assemblies of the same input are byte-identical).
    #[test]
    fn assemble_sorts_and_is_byte_stable() {
        let entries = vec![entry("b"), entry("a")];
        let one = assemble_document(meta(), entries.clone());
        let two = assemble_document(meta(), entries);
        assert_eq!(one, two);
        let doc: ConfigOptionsDoc = serde_json::from_str(&one).unwrap();
        assert_eq!(doc.options[0].var, "a");
        assert_eq!(doc.options[1].var, "b");
        assert_eq!(doc.meta.schema, CONFIG_OPTIONS_SCHEMA);
    }
}
