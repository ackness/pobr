//! `extract-lua --what mod-scalability|runes|uniques|catalysts`：物品编辑态
//! 四张 overlay 表。
//!
//! - `mod-scalability`：`Data/ModScalability.lua`（纯 return 表）→
//!   `overlay/mod_scalability.json`；
//! - `runes`：`Data/ModRunes.lua` → `overlay/runes.json`；
//! - `uniques`：`Data/Uniques/*.lua`（raw 文本块）→ `overlay/uniques.json`
//!   （双层：raw 逐字节保留 + name/base/variants 最小预解析索引）；
//! - `catalysts`：`Classes/Item.lua:14-29` 三个 local 表字面量（`%b{}` 截取 +
//!   load 执行）→ `overlay/catalysts.json`。
//!
//! schema 见 [`pobr_data::catalog::item_overlay`]；公共层复用
//! [`crate::extract_lua`]。

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

/// uniques 抽取的缺省文件集 = vendor `Modules/Data.lua:26` itemTypes 列表
/// （27 项，按 vendor 数组序）+ `Special/race`（Data.lua:1058）。
/// `Special/Generated`（程序化生成）与 `Special/New`（未定稿池）显式排除。
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

/// 执行 ModScalability 抽取，返回 byte-stable JSON 文本。
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

/// 执行 ModRunes 抽取，返回 byte-stable JSON 文本。
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

/// 引导脚本输出行（raw 层；索引列由本模块预解析）。
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

/// 执行 Uniques 抽取，返回 byte-stable JSON 文本。
pub fn run_extract_uniques(args: &ExtractLuaArgs) -> io::Result<String> {
    let blocks: Vec<RawUniqueBlock> = invoke_luajit_jsonl(args, UNIQUES_BOOTSTRAP_LUA)?;
    let mut uniques: Vec<UniqueDef> = blocks
        .into_iter()
        .map(|block| parse_unique_block(block.item_type, block.raw))
        .collect::<io::Result<_>>()?;
    // 同 (item_type, name) 可能多块（变体拆条目）——稳定排序保留 vendor 出现序。
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

/// raw 文本块 → 双层 UniqueDef：前两行 = name/base，`Variant:`/`League:`/
/// `Source:`/`Upgrade:` 行进最小索引（词条模板行解析留 pobr-item 运行时，
/// ——「预解析索引最小化」）。
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

/// 执行 catalysts 抽取（`Classes/Item.lua` 表字面量截取），返回 byte-stable
/// JSON 文本。
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

// 公共

/// 校验固定文件集目标的 --files（防误用）。
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
