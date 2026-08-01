//! `extract-lua --what mod-scalability|runes|uniques|catalysts`: the four
//! item-editing-state overlay tables.
//!
//! - `mod-scalability`: `Data/ModScalability.lua` (a plain return table) ->
//!   `overlay/mod_scalability.json`;
//! - `runes`: `Data/ModRunes.lua` -> `overlay/runes.json`;
//! - `uniques`: `Data/Uniques/*.lua` (raw text blocks) -> `overlay/uniques.json`
//!   (two layers: byte-exact raw text plus a minimal pre-parsed name/base/variants index);
//! - `catalysts`: the three local table literals at `Classes/Item.lua:14-29`
//!   (sliced with `%b{}` and evaluated with `load`) -> `overlay/catalysts.json`.
//!
//! See [`pobr_data::catalog::item_overlay`] for the schemas; the shared
//! layer reuses [`crate::extract_lua`].

use std::io;

use serde::{Deserialize, Serialize};

use pobr_data::catalog::item_overlay::{CatalystDef, ModScalabilityEntryDef, RuneDef, UniqueDef};

use crate::extract_lua::{
    ExtractLuaArgs, OverlayMeta, invoke_luajit_jsonl, read_vendor_version, resolve_version_file,
};
use crate::extract_minions::to_pretty_json;

const MOD_SCALABILITY_BOOTSTRAP_LUA: &str = include_str!("extract_mod_scalability.lua");
const RUNES_BOOTSTRAP_LUA: &str = include_str!("extract_runes.lua");
const UNIQUES_BOOTSTRAP_LUA: &str = include_str!("extract_uniques.lua");
const CATALYSTS_BOOTSTRAP_LUA: &str = include_str!("extract_catalysts.lua");

/// Default uniques extraction file set = vendor `Modules/Data.lua:26`'s
/// itemTypes list (27 entries, in vendor array order) plus `Special/race`
/// (Data.lua:1058). `Special/Generated` (procedurally generated) and
/// `Special/New` (unfinalized pool) are explicitly excluded.
pub const DEFAULT_UNIQUE_FILES: &[&str] = &[
    "axe",
    "bow",
    "claw",
    "crossbow",
    "dagger",
    "fishing",
    "flail",
    "focus",
    "mace",
    "spear",
    "staff",
    "sceptre",
    "sword",
    "talisman",
    "wand",
    "body",
    "gloves",
    "helmet",
    "boots",
    "shield",
    "quiver",
    "amulet",
    "ring",
    "belt",
    "jewel",
    "flask",
    "incursionlimb",
    "Special/race",
];

// mod-scalability

#[derive(Debug, Serialize)]
struct ModScalabilityDoc {
    #[serde(rename = "_meta")]
    meta: OverlayMeta,
    entries: Vec<ModScalabilityEntryDef>,
}

/// Run the ModScalability extraction, returning byte-stable JSON text.
pub fn run_extract_mod_scalability(args: &ExtractLuaArgs) -> io::Result<String> {
    expect_files(args, &["ModScalability"], "mod-scalability")?;
    let mut entries: Vec<ModScalabilityEntryDef> =
        invoke_luajit_jsonl(args, MOD_SCALABILITY_BOOTSTRAP_LUA)?;
    entries.sort_by(|a, b| a.template.cmp(&b.template));
    let meta = build_meta(
        args,
        "mod_scalability/v1",
        "mod-scalability",
        vec!["Data/ModScalability.lua".to_string()],
    )?;
    Ok(to_pretty_json(&ModScalabilityDoc { meta, entries }))
}

// runes

#[derive(Debug, Serialize)]
struct RunesDoc {
    #[serde(rename = "_meta")]
    meta: OverlayMeta,
    runes: Vec<RuneDef>,
}

/// Run the ModRunes extraction, returning byte-stable JSON text.
pub fn run_extract_runes(args: &ExtractLuaArgs) -> io::Result<String> {
    expect_files(args, &["ModRunes"], "runes")?;
    let mut runes: Vec<RuneDef> = invoke_luajit_jsonl(args, RUNES_BOOTSTRAP_LUA)?;
    runes.sort_by(|a, b| a.name.cmp(&b.name));
    let meta = build_meta(
        args,
        "runes/v1",
        "runes",
        vec!["Data/ModRunes.lua".to_string()],
    )?;
    Ok(to_pretty_json(&RunesDoc { meta, runes }))
}

// uniques

/// Row emitted by the bootstrap script (raw layer; the index columns are pre-parsed by this module).
#[derive(Debug, Deserialize)]
struct RawUniqueBlock {
    item_type: String,
    raw: String,
}

#[derive(Debug, Serialize)]
struct UniquesDoc {
    #[serde(rename = "_meta")]
    meta: OverlayMeta,
    uniques: Vec<UniqueDef>,
}

/// Run the Uniques extraction, returning byte-stable JSON text.
pub fn run_extract_uniques(args: &ExtractLuaArgs) -> io::Result<String> {
    let blocks: Vec<RawUniqueBlock> = invoke_luajit_jsonl(args, UNIQUES_BOOTSTRAP_LUA)?;
    let mut uniques: Vec<UniqueDef> = blocks
        .into_iter()
        .map(|block| parse_unique_block(block.item_type, block.raw))
        .collect::<io::Result<_>>()?;
    // The same (item_type, name) may have multiple blocks (variants split
    // into separate entries) — a stable sort preserves vendor's original order.
    uniques.sort_by(|a, b| {
        a.item_type
            .cmp(&b.item_type)
            .then_with(|| a.name.cmp(&b.name))
    });
    let extracted = args
        .files
        .iter()
        .map(|name| format!("Data/Uniques/{name}.lua"))
        .collect();
    let meta = build_meta(args, "uniques/v1", "uniques", extracted)?;
    Ok(to_pretty_json(&UniquesDoc { meta, uniques }))
}

/// Raw text block -> a two-layer UniqueDef: the first two lines are
/// name/base, and `Variant:`/`League:`/`Source:`/`Upgrade:` lines feed a
/// minimal index (parsing the mod-template lines themselves is left to the
/// pobr-item runtime — "keep the pre-parsed index minimal").
fn parse_unique_block(item_type: String, raw: String) -> io::Result<UniqueDef> {
    let mut lines = raw.lines().filter(|l| !l.trim().is_empty());
    let name = lines
        .next()
        .ok_or_else(|| io::Error::other(format!("空 unique 文本块（{item_type}）")))?
        .trim()
        .to_string();
    let base = lines
        .next()
        .ok_or_else(|| io::Error::other(format!("unique `{name}` 缺基底行（{item_type}）")))?
        .trim()
        .to_string();
    let mut variants = Vec::new();
    let mut league = None;
    let mut source = None;
    let mut upgrade = None;
    for line in lines {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("Variant: ") {
            variants.push(v.to_string());
        } else if let Some(v) = line.strip_prefix("League: ") {
            league.get_or_insert_with(|| v.to_string());
        } else if let Some(v) = line.strip_prefix("Source: ") {
            source.get_or_insert_with(|| v.to_string());
        } else if let Some(v) = line.strip_prefix("Upgrade: ") {
            upgrade.get_or_insert_with(|| v.to_string());
        }
    }
    Ok(UniqueDef {
        name,
        base,
        item_type,
        raw,
        variants,
        league,
        source,
        upgrade,
    })
}

// catalysts

#[derive(Debug, Serialize)]
struct CatalystsDoc {
    #[serde(rename = "_meta")]
    meta: OverlayMeta,
    catalysts: Vec<CatalystDef>,
}

/// Run the catalysts extraction (slicing table literals out of
/// `Classes/Item.lua`), returning byte-stable JSON text.
pub fn run_extract_catalysts(args: &ExtractLuaArgs) -> io::Result<String> {
    expect_files(args, &["Item"], "catalysts")?;
    let mut catalysts: Vec<CatalystDef> = invoke_luajit_jsonl(args, CATALYSTS_BOOTSTRAP_LUA)?;
    catalysts.sort_by_key(|c| c.id);
    let meta = build_meta(
        args,
        "catalysts/v1",
        "catalysts",
        vec!["Classes/Item.lua".to_string()],
    )?;
    Ok(to_pretty_json(&CatalystsDoc { meta, catalysts }))
}

// Shared helpers

/// Validate --files for a target with a fixed file set (guards against misuse).
fn expect_files(args: &ExtractLuaArgs, expected: &[&str], what: &str) -> io::Result<()> {
    if args.files != expected {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "--what {what} 的抽取文件固定为 {expected:?}，不接受 --files 自定义（收到 {:?}）",
                args.files
            ),
        ));
    }
    Ok(())
}

fn build_meta(
    args: &ExtractLuaArgs,
    schema: &str,
    what: &str,
    extracted_files: Vec<String>,
) -> io::Result<OverlayMeta> {
    let (commit, subject) = read_vendor_version(&resolve_version_file(args))?;
    let mut regen = format!(
        "cargo run -p sync-pob-catalog -- extract-lua --what {what} --vendor-root vendor/PathOfBuilding-PoE2/src"
    );
    if let Some(out) = &args.out_for_meta {
        regen.push_str(&format!(" --out {out}"));
    }
    Ok(OverlayMeta {
        schema: schema.to_string(),
        generator: "sync-pob-catalog extract-lua".to_string(),
        vendor: "PathOfBuilding-PoE2".to_string(),
        vendor_commit: commit,
        vendor_commit_subject: subject,
        extracted_files,
        regen_command: regen,
    })
}
