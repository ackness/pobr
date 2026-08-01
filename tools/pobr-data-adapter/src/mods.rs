//! Mods + Stats 数据域适配：原始 `Mods.json` / `Stats.json` → PoBR 最小 JSON。
//!
//! - `Stats.json` → `stats.json`（stat 注册表：id / is_local / semantic / category）。
//! - `Mods.json`  → `mods.json`（词缀池：合并 Stat1..4 外键 + Stat1Value..4Value 掷值，
//!   解析 Tags 外键，保留 domain / generation_type / mod_type / level）。
//! - zh-TW 词缀名（若与英文不同）→ `i18n/zh-TW/mods.json` 边车。
//!
//! 过滤规则：
//! - Stats：保留全部（注册表需完整，供 Mods 外键解析）。
//! - Mods：跳过既无显示名（`Name` 空）**又**无任何 stat 槽位的纯占位/内部空壳条目；
//!   只要有 stat 或有名称即入库（内部无名 mod 携带 stat 仍是计算所需，保留）。

use std::collections::BTreeMap;
use std::path::Path;

use pobr_data::catalog::{ModDef, ModStat, SpawnWeight, StatDef};
use serde::Deserialize;

use crate::{RawNamed, read_json, resolve, write_pretty};

// 原始 .dat JSON 行结构（只取需要的列）

#[derive(Deserialize)]
struct RawStat {
    #[serde(rename = "Id")]
    id: String,
    #[serde(rename = "IsLocal")]
    is_local: Option<bool>,
    #[serde(rename = "Semantic")]
    semantic: Option<u32>,
    #[serde(rename = "Category")]
    category: Option<u32>,
}

#[derive(Deserialize)]
struct RawMod {
    #[serde(rename = "Id")]
    id: String,
    #[serde(rename = "Name")]
    name: Option<String>,
    #[serde(rename = "ModType")]
    mod_type: Option<u32>,
    #[serde(rename = "Domain")]
    domain: Option<u32>,
    #[serde(rename = "GenerationType")]
    generation_type: Option<u32>,
    #[serde(rename = "Level")]
    level: Option<i64>,
    #[serde(rename = "Stat1")]
    stat1: Option<usize>,
    #[serde(rename = "Stat2")]
    stat2: Option<usize>,
    #[serde(rename = "Stat3")]
    stat3: Option<usize>,
    #[serde(rename = "Stat4")]
    stat4: Option<usize>,
    #[serde(rename = "Stat1Value", default)]
    stat1_value: [i64; 2],
    #[serde(rename = "Stat2Value", default)]
    stat2_value: [i64; 2],
    #[serde(rename = "Stat3Value", default)]
    stat3_value: [i64; 2],
    #[serde(rename = "Stat4Value", default)]
    stat4_value: [i64; 2],
    #[serde(rename = "Tags", default)]
    tags: Vec<usize>,
    #[serde(rename = "SpawnWeight_Tags", default)]
    spawn_weight_tags: Vec<usize>,
    #[serde(rename = "SpawnWeight_Values", default)]
    spawn_weight_values: Vec<i64>,
}

/// 适配 Stats + Mods 域，写出 `base/stats.json` / `base/mods.json` /
/// `i18n/zh-TW/mods.json`（i18n 边车留版本根 `version_dir`）。
/// 返回 `(stats 条数, mods 入库条数, mods 过滤条数, zh-TW 名称条数)`。
pub fn adapt(
    en: &Path,
    tw: &Path,
    stat_lookup: &[String],
    tags_lookup: &[String],
    base_dir: &Path,
    version_dir: &Path,
) -> Result<(usize, usize, usize, usize), String> {
    let stats = adapt_stats(en, base_dir)?;
    let (kept, filtered, zh) = adapt_mods(en, tw, stat_lookup, tags_lookup, base_dir, version_dir)?;
    Ok((stats, kept, filtered, zh))
}

/// `ModType.json`（`_index` + `Name`）→ 按位置索引的组名查表。
/// 旧 tables 快照无此表时降级为空表（group 字段整列缺省，非致命）。
fn mod_type_lookup(en: &Path) -> Vec<String> {
    #[derive(Deserialize)]
    struct RawModType {
        #[serde(rename = "_index")]
        index: usize,
        #[serde(rename = "Name")]
        name: Option<String>,
    }
    let Ok(rows) = read_json::<Vec<RawModType>>(&en.join("ModType.json")) else {
        eprintln!("⚠ pobr-data-adapter：ModType.json 缺失/不可读——mods.json 的 group 字段整列缺省");
        return Vec::new();
    };
    let max = rows.iter().map(|r| r.index).max().map_or(0, |m| m + 1);
    let mut table = vec![String::new(); max];
    for r in rows {
        if let Some(n) = r.name {
            table[r.index] = n;
        }
    }
    table
}

fn adapt_stats(en: &Path, base_dir: &Path) -> Result<usize, String> {
    let raw = read_json::<Vec<RawStat>>(&en.join("Stats.json"))?;
    let mut stats: Vec<StatDef> = raw
        .into_iter()
        .filter(|s| !s.id.is_empty())
        .map(|s| StatDef {
            id: s.id,
            is_local: s.is_local.unwrap_or(false),
            semantic: s.semantic,
            category: s.category,
        })
        .collect();
    stats.sort_by(|a, b| a.id.cmp(&b.id));
    write_pretty(&base_dir.join("stats.json"), &stats)?;
    Ok(stats.len())
}

fn adapt_mods(
    en: &Path,
    tw: &Path,
    stat_lookup: &[String],
    tags_lookup: &[String],
    base_dir: &Path,
    version_dir: &Path,
) -> Result<(usize, usize, usize), String> {
    let raw_mods = read_json::<Vec<RawMod>>(&en.join("Mods.json"))?;
    let mod_types = mod_type_lookup(en);
    let tw_names = read_json::<Vec<RawNamed>>(&tw.join("Mods.json"))?;
    let tw_by_index: BTreeMap<usize, String> = tw_names
        .into_iter()
        .filter_map(|r| r.name.map(|n| (r.index, n)))
        .collect();

    let total = raw_mods.len();
    let mut mods: Vec<ModDef> = Vec::new();
    let mut i18n_zh: BTreeMap<String, String> = BTreeMap::new();

    for (index, raw) in raw_mods.into_iter().enumerate() {
        let stats = build_stats(&raw, stat_lookup);
        let name = raw.name.filter(|n| !n.is_empty());

        // 过滤：既无显示名又无任何 stat 槽位的纯占位空壳。
        if name.is_none() && stats.is_empty() {
            continue;
        }

        if let Some(en_name) = &name
            && let Some(zh) = tw_by_index.get(&index)
            && !zh.is_empty()
            && zh != en_name
        {
            i18n_zh.insert(raw.id.clone(), zh.clone());
        }

        let tags: Vec<String> = raw
            .tags
            .iter()
            .filter_map(|&i| resolve(tags_lookup, i))
            .collect();

        // SpawnWeight_Tags/Values 是平行数组；顺序敏感（判定取第一个命中 tag）。
        let spawn_weights: Vec<SpawnWeight> = raw
            .spawn_weight_tags
            .iter()
            .zip(raw.spawn_weight_values.iter())
            .filter_map(|(&tag_idx, &weight)| {
                Some(SpawnWeight {
                    tag: resolve(tags_lookup, tag_idx)?,
                    weight: weight.max(0) as u32,
                })
            })
            .collect();

        mods.push(ModDef {
            id: raw.id,
            name,
            mod_type: raw.mod_type,
            domain: raw.domain.unwrap_or(0),
            generation_type: raw.generation_type,
            level: raw.level.unwrap_or(0).max(0) as u32,
            stats,
            tags,
            group: raw.mod_type.and_then(|i| resolve(&mod_types, i as usize)),
            spawn_weights,
        });
    }

    mods.sort_by(|a, b| a.id.cmp(&b.id));
    let kept = mods.len();
    let filtered = total - kept;

    write_pretty(&base_dir.join("mods.json"), &mods)?;
    write_pretty(&version_dir.join("i18n/zh-TW/mods.json"), &i18n_zh)?;

    Ok((kept, filtered, i18n_zh.len()))
}

/// 合并 Stat1..4 外键 + StatNValue 掷值区间为 stats 数组，跳过空槽与无法解析的外键。
fn build_stats(raw: &RawMod, stat_lookup: &[String]) -> Vec<ModStat> {
    let slots = [
        (raw.stat1, raw.stat1_value),
        (raw.stat2, raw.stat2_value),
        (raw.stat3, raw.stat3_value),
        (raw.stat4, raw.stat4_value),
    ];
    slots
        .into_iter()
        .filter_map(|(stat, value)| {
            let idx = stat?;
            let stat_id = resolve(stat_lookup, idx)?;
            Some(ModStat {
                stat_id,
                min: value[0],
                max: value[1],
            })
        })
        .collect()
}
