//! pobr-data-adapter：把 pathofexile-dat 的原始 `.dat` JSON 适配为 PoBR 最小 JSON。
//!
//! 解析整型外键（ItemClass / Tags / Implicit_Mods → 稳定字符串 ID）、反范式化、
//! 过滤开发用占位条目，输出按 id 排序的 diff 友好 JSON 到 `data/<patch>/`。
//!
//! 用法：
//! ```text
//! # 物品基底域（pathofexile-dat 表）
//! cargo run -p pobr-data-adapter -- --raw pipeline/tables --out data --patch 4.5.0.3.4
//! # 被动天赋树域（GGG 官方树导出 data.json）
//! cargo run -p pobr-data-adapter -- --tree pipeline/tree/data.json --out data --patch 4.5.0.3.4
//! ```

mod tree;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use pobr_data::catalog::{BaseItemDef, CATALOG_SCHEMA_VERSION, DataManifest};
use serde::Deserialize;

mod mods;
mod skills;

const ZH_TW: &str = "Traditional Chinese";

fn main() -> ExitCode {
    let result = match parse_args() {
        Ok(Mode::BaseItems(args)) => run(args),
        Ok(Mode::Tree(args)) => tree::run(args),
        Err(err) => Err(err),
    };
    match result {
        Ok(summary) => {
            println!("{summary}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("pobr-data-adapter 失败：{err}");
            ExitCode::FAILURE
        }
    }
}

struct Args {
    raw: PathBuf,
    out: PathBuf,
    patch: String,
}

/// 适配器子命令：物品基底域（`--raw`）或被动树域（`--tree`），二者互斥。
enum Mode {
    BaseItems(Args),
    Tree(tree::TreeArgs),
}

fn parse_args() -> Result<Mode, String> {
    let mut raw = None;
    let mut tree = None;
    let mut out = None;
    let mut patch = None;
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let mut take = |name: &str| it.next().ok_or_else(|| format!("{name} 缺少参数值"));
        match flag.as_str() {
            "--raw" => raw = Some(PathBuf::from(take("--raw")?)),
            "--tree" => tree = Some(PathBuf::from(take("--tree")?)),
            "--out" => out = Some(PathBuf::from(take("--out")?)),
            "--patch" => patch = Some(take("--patch")?),
            other => return Err(format!("未知参数：{other}")),
        }
    }
    let out = out.ok_or("缺少 --out <data>")?;
    let patch = patch.ok_or("缺少 --patch <version>")?;
    match (raw, tree) {
        (Some(_), Some(_)) => Err("--raw 与 --tree 互斥，请分别运行".into()),
        (Some(raw), None) => Ok(Mode::BaseItems(Args { raw, out, patch })),
        (None, Some(data_json)) => Ok(Mode::Tree(tree::TreeArgs {
            data_json,
            out,
            patch,
        })),
        (None, None) => Err("缺少 --raw <pipeline/tables> 或 --tree <data.json>".into()),
    }
}

// ---- 原始 .dat JSON 行结构（只取我们需要的列）----

#[derive(Deserialize)]
pub(crate) struct RawIndexed {
    #[serde(rename = "_index")]
    index: usize,
    #[serde(rename = "Id")]
    id: String,
}

#[derive(Deserialize)]
struct RawBaseItem {
    #[serde(rename = "Id")]
    id: String,
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "ItemClass")]
    item_class: Option<usize>,
    #[serde(rename = "DropLevel")]
    drop_level: Option<i64>,
    #[serde(rename = "Width")]
    width: Option<u8>,
    #[serde(rename = "Height")]
    height: Option<u8>,
    #[serde(rename = "Tags", default)]
    tags: Vec<usize>,
    #[serde(rename = "Implicit_Mods", default)]
    implicit_mods: Vec<usize>,
    #[serde(rename = "ModDomain")]
    mod_domain: Option<u32>,
}

#[derive(Deserialize)]
pub(crate) struct RawNamed {
    #[serde(rename = "_index")]
    pub(crate) index: usize,
    #[serde(rename = "Name")]
    pub(crate) name: Option<String>,
}

pub(crate) fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let bytes = fs::read(path).map_err(|e| format!("读取 {} 失败：{e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("解析 {} 失败：{e}", path.display()))
}

/// 把 `_index -> Id` 建成按位置索引的查表（`_index` 连续，越界返回 None）。
fn id_lookup(rows: &[RawIndexed]) -> Vec<String> {
    let max = rows.iter().map(|r| r.index).max().map_or(0, |m| m + 1);
    let mut table = vec![String::new(); max];
    for r in rows {
        table[r.index] = r.id.clone();
    }
    table
}

pub(crate) fn resolve(lookup: &[String], idx: usize) -> Option<String> {
    lookup.get(idx).filter(|s| !s.is_empty()).cloned()
}

/// 开发用占位 / 未启用条目（不入库）。
pub(crate) fn is_placeholder(name: &str) -> bool {
    name.is_empty() || name.contains("[DNT") || name.contains("[UNUSED") || name.contains("[OLD")
}

fn run(args: Args) -> Result<String, String> {
    let en = args.raw.join("English");
    let tw = args.raw.join(ZH_TW);

    // 外键解析表
    let classes = id_lookup(&read_json::<Vec<RawIndexed>>(&en.join("ItemClasses.json"))?);
    let tags = id_lookup(&read_json::<Vec<RawIndexed>>(&en.join("Tags.json"))?);
    let mods = id_lookup(&read_json::<Vec<RawIndexed>>(&en.join("Mods.json"))?);
    let stats = id_lookup(&read_json::<Vec<RawIndexed>>(&en.join("Stats.json"))?);

    // 基底（英文 canonical + 繁中名称边车）
    let raw_bases = read_json::<Vec<RawBaseItem>>(&en.join("BaseItemTypes.json"))?;
    let tw_names = read_json::<Vec<RawNamed>>(&tw.join("BaseItemTypes.json"))?;
    let tw_by_index: BTreeMap<usize, String> = tw_names
        .into_iter()
        .filter_map(|r| r.name.map(|n| (r.index, n)))
        .collect();

    let mut bases = Vec::new();
    let mut i18n_zh: BTreeMap<String, String> = BTreeMap::new();
    let total = raw_bases.len();
    for (index, raw) in raw_bases.into_iter().enumerate() {
        if is_placeholder(&raw.name) {
            continue;
        }
        let item_class = raw
            .item_class
            .and_then(|i| resolve(&classes, i))
            .unwrap_or_default();
        let tag_ids: Vec<String> = raw.tags.iter().filter_map(|&i| resolve(&tags, i)).collect();
        let implicits: Vec<String> = raw
            .implicit_mods
            .iter()
            .filter_map(|&i| resolve(&mods, i))
            .collect();

        if let Some(zh) = tw_by_index.get(&index)
            && !zh.is_empty()
            && *zh != raw.name
        {
            i18n_zh.insert(raw.id.clone(), zh.clone());
        }

        bases.push(BaseItemDef {
            id: raw.id,
            name: raw.name,
            item_class,
            drop_level: raw.drop_level.unwrap_or(0).max(0) as u32,
            width: raw.width.unwrap_or(1),
            height: raw.height.unwrap_or(1),
            tags: tag_ids,
            implicits,
            mod_domain: raw.mod_domain.unwrap_or(0),
        });
    }

    bases.sort_by(|a, b| a.id.cmp(&b.id));

    // 写出
    let version_dir = args.out.join(&args.patch);
    fs::create_dir_all(version_dir.join("i18n").join("zh-TW"))
        .map_err(|e| format!("创建输出目录失败：{e}"))?;

    write_pretty(&version_dir.join("base_items.json"), &bases)?;
    write_pretty(&version_dir.join("i18n/zh-TW/base_items.json"), &i18n_zh)?;

    // Mods + Stats 域（stat 注册表 + 词缀池）。
    let (stat_count, mod_count, mod_filtered, mod_zh) =
        mods::adapt(&en, &tw, &stats, &tags, &version_dir)?;

    // 技能宝石域（SkillGems / GrantedEffects / ActiveSkills）
    let skills = skills::adapt_skills(&en, &tw)?;
    write_pretty(&version_dir.join("skill_gems.json"), &skills.gems)?;
    write_pretty(&version_dir.join("granted_effects.json"), &skills.effects)?;
    write_pretty(
        &version_dir.join("i18n/zh-TW/skills.json"),
        &skills.zh_skill_names,
    )?;

    let manifest = DataManifest {
        schema_version: CATALOG_SCHEMA_VERSION,
        poe_version: args.patch.clone(),
        languages: vec!["zh-TW".into()],
        domains: vec![
            "base_items".into(),
            "mods".into(),
            "stats".into(),
            "skill_gems".into(),
            "granted_effects".into(),
        ],
    };
    write_pretty(&version_dir.join("manifest.json"), &manifest)?;

    Ok(format!(
        "适配完成：base_items {}/{} 条（过滤 {} 个占位），zh-TW 名称 {} 条；\
         stats {} 条；mods {} 条（过滤 {} 个空壳），mods zh-TW 名称 {} 条；\
         skill_gems {}/{} 条，granted_effects {}/{} 条，zh-TW 技能名 {} 条 → {}",
        bases.len(),
        total,
        total - bases.len(),
        i18n_zh.len(),
        stat_count,
        mod_count,
        mod_filtered,
        mod_zh,
        skills.gems.len(),
        skills.gems_total,
        skills.effects.len(),
        skills.effects_total,
        skills.zh_skill_names.len(),
        version_dir.display()
    ))
}

pub(crate) fn write_pretty<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|e| format!("序列化 {} 失败：{e}", path.display()))?;
    fs::write(path, format!("{json}\n")).map_err(|e| format!("写入 {} 失败：{e}", path.display()))
}
