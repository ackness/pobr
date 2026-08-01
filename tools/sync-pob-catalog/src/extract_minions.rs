//! `extract-lua --what minions|spectres|minion-list`（数据生产）：
//!
//! - `minions` / `spectres`：执行 vendor `Data/Minions.lua` / `Data/Spectres.lua`
//!   （引导脚本 `extract_minions.lua`，stub 完整序列化 `mod(...)`/`flag(...)`
//!   构造参数——丢参警告的丢参问题在 stub 层规避），产
//!   `overlay/minions.json` / `overlay/spectres.json`；
//! - `minion-list`：执行 vendor `Data/Skills/*.lua`（引导脚本
//!   `extract_minion_list.lua`），抽 `minionList`/`minionUses`/`minionHasItemSet`
//!   外键边车，产 `overlay/granted_effect_minions.json`。
//!
//! 公共层（luajit JSONL 调用 / vendor 版本解析 / byte-stable 序列化约定）
//! 复用 [`crate::extract_lua`]。

use std::io;

use serde::Serialize;

use pobr_data::catalog::actors::{GrantedEffectMinionDef, MinionEntryDef};

use crate::extract_lua::{
    ExtractLuaArgs, OverlayMeta, invoke_luajit_jsonl, read_vendor_version, resolve_version_file,
};

/// 引导脚本（编译期内嵌）。
const MINIONS_BOOTSTRAP_LUA: &str = include_str!("extract_minions.lua");
const MINION_LIST_BOOTSTRAP_LUA: &str = include_str!("extract_minion_list.lua");

/// `minions` / `spectres` 两个抽取目标的差异参数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MinionsKind {
    /// `Data/Minions.lua` → `overlay/minions.json`（schema `minions/v1`）。
    Minions,
    /// `Data/Spectres.lua` → `overlay/spectres.json`（schema `spectres/v1`）。
    Spectres,
}

impl MinionsKind {
    fn schema(self) -> &'static str {
        match self {
            Self::Minions => "minions/v1",
            Self::Spectres => "spectres/v1",
        }
    }

    fn vendor_file(self) -> &'static str {
        match self {
            Self::Minions => "Minions",
            Self::Spectres => "Spectres",
        }
    }

    fn what(self) -> &'static str {
        match self {
            Self::Minions => "minions",
            Self::Spectres => "spectres",
        }
    }
}

/// `overlay/minions.json` / `overlay/spectres.json` 完整文档（生成侧）。
#[derive(Debug, Serialize)]
struct MinionsDoc {
    #[serde(rename = "_meta")]
    meta: OverlayMeta,
    minions: Vec<MinionEntryDef>,
}

/// 执行 minions / spectres 抽取，返回 byte-stable JSON 文本。
pub fn run_extract_minions(args: &ExtractLuaArgs, kind: MinionsKind) -> io::Result<String> {
    // 数据文件固定单文件；显式 --files 与目标不符时报错（防误用）。
    let expected = [kind.vendor_file().to_string()];
    if args.files != expected {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "--what {} 固定抽取 Data/{}.lua，不接受 --files 自定义（收到 {:?}）",
                kind.what(),
                kind.vendor_file(),
                args.files
            ),
        ));
    }
    let mut entries: Vec<MinionEntryDef> = invoke_luajit_jsonl(args, MINIONS_BOOTSTRAP_LUA)?;
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    let meta = build_meta(
        args,
        kind.schema(),
        kind.what(),
        vec![format!("Data/{}.lua", kind.vendor_file())],
    )?;
    let doc = MinionsDoc {
        meta,
        minions: entries,
    };
    Ok(to_pretty_json(&doc))
}

/// `overlay/granted_effect_minions.json` 完整文档（生成侧）。
#[derive(Debug, Serialize)]
struct GrantedEffectMinionsDoc {
    #[serde(rename = "_meta")]
    meta: OverlayMeta,
    entries: Vec<GrantedEffectMinionDef>,
}

/// 当前 overlay 文档 schema 标识。
pub const GRANTED_EFFECT_MINIONS_SCHEMA: &str = "granted_effect_minions/v1";

/// 执行 minion-list 外键边车抽取，返回 byte-stable JSON 文本。
pub fn run_extract_minion_list(args: &ExtractLuaArgs) -> io::Result<String> {
    let mut entries: Vec<GrantedEffectMinionDef> =
        invoke_luajit_jsonl(args, MINION_LIST_BOOTSTRAP_LUA)?;
    entries.sort_by(|a, b| a.effect_id.cmp(&b.effect_id));
    let extracted = args
        .files
        .iter()
        .map(|name| format!("Data/Skills/{name}.lua"))
        .collect();
    let meta = build_meta(
        args,
        GRANTED_EFFECT_MINIONS_SCHEMA,
        "minion-list",
        extracted,
    )?;
    let doc = GrantedEffectMinionsDoc { meta, entries };
    Ok(to_pretty_json(&doc))
}

/// 组装 `_meta`（vendor commit / regen 命令；canonical 相对路径约定与
/// `extract_lua::build_meta` 一致）。
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

/// 统一 serde_json pretty 序列化（同输入必然同输出，尾随换行）。
pub(crate) fn to_pretty_json<T: Serialize>(doc: &T) -> String {
    let mut json = serde_json::to_string_pretty(doc).expect("overlay 文档序列化不应失败");
    json.push('\n');
    json
}
