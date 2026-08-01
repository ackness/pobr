//! 被动天赋树域适配：GGG 官方树导出 `data.json` → PoBR 最小树 JSON。
//!
//! 源 = `github.com/grindinggear/poe2-skilltree-export` 的 `data.json`（PoB 导出脚本
//! 确认）。**只取结构化数据，不取任何 assets/图集**：忽略 `icon`/`x`/`y`/`edges` 等
//! 显示冗余字段，输出按 `skill` id 排序、diff 友好的 `passive_tree.json` +
//! `passive_tree_meta.json` 到 `data/<patch>/`。
//!
//! 原始 `data.json` 体积大（数 MB），落在 gitignore 路径，不入库。

use std::collections::BTreeMap;
use std::path::PathBuf;

use pobr_data::catalog::{
    PassiveAscendancy, PassiveClass, PassiveNodeDef, PassiveNodeKind, PassiveTreeMeta,
};
use serde::Deserialize;

use crate::write_pretty;

pub struct TreeArgs {
    pub data_json: PathBuf,
    pub out: PathBuf,
    pub patch: String,
}

// 原始 data.json 结构（只取我们需要的字段）

#[derive(Deserialize)]
struct RawTree {
    #[serde(default)]
    tree: Option<String>,
    classes: Vec<RawClass>,
    nodes: BTreeMap<String, RawNode>,
}

#[derive(Deserialize)]
struct RawClass {
    name: String,
    #[serde(default)]
    base_str: i32,
    #[serde(default)]
    base_dex: i32,
    #[serde(default)]
    base_int: i32,
    #[serde(default)]
    ascendancies: Vec<RawAscendancy>,
}

#[derive(Deserialize)]
struct RawAscendancy {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Deserialize)]
struct RawNode {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    skill: Option<u32>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    stats: Vec<String>,
    #[serde(default)]
    group: Option<u32>,
    #[serde(default)]
    orbit: Option<u32>,
    #[serde(rename = "orbitIndex", default)]
    orbit_index: Option<u32>,
    #[serde(default)]
    out: Vec<String>,
    #[serde(rename = "ascendancyId", default)]
    ascendancy_id: Option<String>,
    #[serde(rename = "isNotable", default)]
    is_notable: bool,
    #[serde(rename = "isKeystone", default)]
    is_keystone: bool,
    #[serde(rename = "isMastery", default)]
    is_mastery: bool,
    #[serde(rename = "isJewelSocket", default)]
    is_jewel_socket: bool,
    #[serde(rename = "isAscendancyStart", default)]
    is_ascendancy_start: bool,
}

fn classify(n: &RawNode) -> PassiveNodeKind {
    // 优先级：keystone > notable > mastery > jewel > ascendancy_start > normal。
    if n.is_keystone {
        PassiveNodeKind::Keystone
    } else if n.is_notable {
        PassiveNodeKind::Notable
    } else if n.is_mastery {
        PassiveNodeKind::Mastery
    } else if n.is_jewel_socket {
        PassiveNodeKind::JewelSocket
    } else if n.is_ascendancy_start {
        PassiveNodeKind::AscendancyStart
    } else {
        PassiveNodeKind::Normal
    }
}

/// 把出边的字符串节点 key 解析为目标 `skill` id（借助原始 nodes 表的 key→skill 映射）。
fn resolve_connections(out: &[String], skill_by_key: &BTreeMap<&str, u32>) -> Vec<u32> {
    let mut conns: Vec<u32> = out
        .iter()
        .filter_map(|k| skill_by_key.get(k.as_str()).copied())
        .collect();
    conns.sort_unstable();
    conns.dedup();
    conns
}

pub fn run(args: TreeArgs) -> Result<String, String> {
    let bytes = std::fs::read(&args.data_json)
        .map_err(|e| format!("读取 {} 失败：{e}", args.data_json.display()))?;
    let raw_mb = bytes.len() as f64 / (1024.0 * 1024.0);
    let raw: RawTree = serde_json::from_slice(&bytes)
        .map_err(|e| format!("解析 {} 失败：{e}", args.data_json.display()))?;

    // key（map 键，如 "18684" 或 "root"）→ skill id，用于解析连线。
    let skill_by_key: BTreeMap<&str, u32> = raw
        .nodes
        .iter()
        .filter_map(|(k, n)| n.skill.map(|s| (k.as_str(), s)))
        .collect();

    let total = raw.nodes.len();
    let mut nodes: Vec<PassiveNodeDef> = Vec::new();
    for n in raw.nodes.values() {
        // 跳过 `root` 等无 skill/id 的布局占位节点。
        let (Some(skill), Some(id)) = (n.skill, n.id.clone()) else {
            continue;
        };
        nodes.push(PassiveNodeDef {
            skill,
            id,
            name: n.name.clone(),
            kind: classify(n),
            stats: n.stats.clone(),
            group: n.group,
            orbit: n.orbit,
            orbit_index: n.orbit_index,
            // 坐标由独立的 `--tree-coords <tree.lua>` 步骤回填（GGG data.json 不含 group 坐标）。
            x: None,
            y: None,
            connections: resolve_connections(&n.out, &skill_by_key),
            ascendancy_id: n.ascendancy_id.clone(),
            // isSwitchable 变体由独立的 `--tree-variants <tree.lua>` 步骤回填
            // （GGG data.json 不携带 options 变体）。
            variants: Vec::new(),
            apply_to_armour: false,
        });
    }
    nodes.sort_by_key(|n| n.skill);

    let mut classes: Vec<PassiveClass> = raw
        .classes
        .into_iter()
        .map(|c| PassiveClass {
            name: c.name,
            base_str: c.base_str,
            base_dex: c.base_dex,
            base_int: c.base_int,
            ascendancies: c
                .ascendancies
                .into_iter()
                // 过滤无名 / 无 id 的占位飞升槽（GGG 导出含 null 占位）。
                .filter_map(|a| match (a.id, a.name) {
                    (Some(id), Some(name)) if !id.is_empty() && !name.is_empty() => {
                        Some(PassiveAscendancy { id, name })
                    }
                    _ => None,
                })
                .collect(),
        })
        .collect();
    classes.sort_by(|a, b| a.name.cmp(&b.name));

    let meta = PassiveTreeMeta {
        tree: raw.tree.unwrap_or_else(|| "Default".into()),
        classes,
    };

    // 三层布局：天赋树域 JSON 落 base/ 层。
    let base_dir = args.out.join(&args.patch).join("base");
    std::fs::create_dir_all(&base_dir).map_err(|e| format!("创建输出目录失败：{e}"))?;
    write_pretty(&base_dir.join("passive_tree.json"), &nodes)?;
    write_pretty(&base_dir.join("passive_tree_meta.json"), &meta)?;

    Ok(format!(
        "天赋树适配完成：节点 {}/{} 条（跳过 {} 个布局占位），职业 {} → {}（源 {:.1}MB）",
        nodes.len(),
        total,
        total - nodes.len(),
        meta.classes.len(),
        base_dir.display(),
        raw_mb,
    ))
}
