//! Passive tree domain adapter: GGG's official tree export `data.json` -> PoBR's minimal tree JSON.
//!
//! Source = the `data.json` from `github.com/grindinggear/poe2-skilltree-export`
//! (confirmed by PoB's export script). **Only structured data is kept, no
//! assets/atlases**: display-redundant fields like `icon`/`x`/`y`/`edges` are
//! ignored, and the output is written as `skill`-id-sorted, diff-friendly
//! `passive_tree.json` + `passive_tree_meta.json` under `data/<patch>/`.
//!
//! The raw `data.json` is large (several MB) and lives at a gitignored path — it isn't checked in.

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

// Raw data.json structure (only the fields we need)

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
    // Priority: keystone > notable > mastery > jewel > ascendancy_start > normal.
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

/// Resolves an outgoing edge's string node key into a target `skill` id (via the raw nodes table's key -> skill mapping).
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

    // key (the map key, e.g. "18684" or "root") -> skill id, used to resolve connections.
    let skill_by_key: BTreeMap<&str, u32> = raw
        .nodes
        .iter()
        .filter_map(|(k, n)| n.skill.map(|s| (k.as_str(), s)))
        .collect();

    let total = raw.nodes.len();
    let mut nodes: Vec<PassiveNodeDef> = Vec::new();
    for n in raw.nodes.values() {
        // Skip layout placeholder nodes with no skill/id, like `root`.
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
            // Coordinates are backfilled by the separate `--tree-coords
            // <tree.lua>` step (GGG's data.json has no group coordinates).
            x: None,
            y: None,
            connections: resolve_connections(&n.out, &skill_by_key),
            ascendancy_id: n.ascendancy_id.clone(),
            // isSwitchable variants are backfilled by the separate
            // `--tree-variants <tree.lua>` step (GGG's data.json carries no options variants).
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
                // Filter out nameless / idless placeholder ascendancy slots (GGG's export includes null placeholders).
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

    // The three-layer layout: passive tree domain JSON goes in the base/ layer.
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
