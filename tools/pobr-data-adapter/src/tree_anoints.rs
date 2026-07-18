//! 油涂（anoint）notable 池回填：GGG 官方树导出 `data.json` **不含**不在主图上的
//! 油涂专属 notable（如 `Paragon`——「Allocates Paragon」项链 enchant 的目标节点），
//! 而 vendor `tree.lua` 的顶层 `nodes` 块携带完整定义（`isNotable=true` + 自有
//! `stats` + `recipe`（油配方）+ 空 `connections`，节点经 amulet anoint /
//! `GrantedPassive` 进入计算，vendor `spec.tree.notableMap` 按名查找，
//! CalcSetup.lua:1322-1331）。
//!
//! 本通道把 `tree.lua` 中存在、`passive_tree.json` 中缺失的 **notable** 节点追加
//! 入库（kind=Notable、`connections=[]`、无坐标——不参与主图拓扑，仅供按名授予
//! 消费）。与 `--tree-coords` / `--tree-variants` 同源同约定（原地回写）。

use pobr_data::catalog::{PassiveNodeDef, PassiveNodeKind};

use crate::tree_coords::{balanced_block, block_offset, scalar_u32, strip_nested_blocks};
use crate::tree_variants::{BlockKey, iter_keyed_blocks, parse_string_array, string_field};
use crate::write_pretty;

use std::path::PathBuf;

pub struct TreeAnointsArgs {
    /// vendor `tree.lua`（PoB2 完整树数据）。
    pub tree_lua: PathBuf,
    /// `data/<patch>/` 所在根目录（与 `--out` 一致）。
    pub out: PathBuf,
    pub patch: String,
}

pub fn run(args: TreeAnointsArgs) -> Result<String, String> {
    let lua = std::fs::read_to_string(&args.tree_lua)
        .map_err(|e| format!("读取 {} 失败：{e}", args.tree_lua.display()))?;

    // 三层布局：base/ 优先读取既有 passive_tree.json，旧布局（版本根）回退；
    // 回填后原地写回所读到的位置（与 tree_coords / tree_variants 同约定）。
    let version_dir = args.out.join(&args.patch);
    let layered = version_dir.join("base/passive_tree.json");
    let tree_path = if layered.exists() {
        layered
    } else {
        version_dir.join("passive_tree.json")
    };
    let bytes =
        std::fs::read(&tree_path).map_err(|e| format!("读取 {} 失败：{e}", tree_path.display()))?;
    let mut nodes: Vec<PassiveNodeDef> = serde_json::from_slice(&bytes)
        .map_err(|e| format!("解析 {} 失败：{e}", tree_path.display()))?;

    let existing: std::collections::BTreeSet<u32> = nodes.iter().map(|n| n.skill).collect();
    let missing = parse_missing_notables(&lua, &existing)?;
    let added = missing.len();
    let added_skills: Vec<u32> = missing.iter().map(|n| n.skill).collect();
    nodes.extend(missing);
    nodes.sort_by_key(|n| n.skill);

    write_pretty(&tree_path, &nodes)?;

    Ok(format!(
        "油涂 notable 池回填完成：追加 {added} 个缺失 notable（skill：{added_skills:?}）→ {}",
        tree_path.display(),
    ))
}

/// 从 `tree.lua` 顶层 `nodes` 块抽取 `passive_tree.json` 缺失的 notable 节点。
///
/// 入选条件（保守，与油涂池形态对齐）：`isNotable=true` 且有自有 `stats`。
/// 缺失的非 notable（cluster 占位等）不属于本通道，跳过。
fn parse_missing_notables(
    lua: &str,
    existing: &std::collections::BTreeSet<u32>,
) -> Result<Vec<PassiveNodeDef>, String> {
    // 顶层 nodes 块在 groups 之后；从 groups 块末尾起查找以避开组内 `nodes=`
    // （与 tree_variants 同手法）。
    let groups_block = balanced_block(lua, "\tgroups={").ok_or("tree.lua 未找到顶层 groups 块")?;
    let groups_end = block_offset(lua, groups_block);
    let nodes_block =
        balanced_block(&lua[groups_end..], "\tnodes={").ok_or("tree.lua 未找到顶层 nodes 块")?;

    let mut out: Vec<PassiveNodeDef> = Vec::new();
    for (key, node_block) in iter_keyed_blocks(nodes_block) {
        let BlockKey::Num(skill) = key else {
            continue;
        };
        if existing.contains(&skill) {
            continue;
        }
        if !strip_nested_blocks(node_block).contains("isNotable=true") {
            continue;
        }
        let stats = parse_string_array(node_block, "stats={");
        if stats.is_empty() {
            continue;
        }
        let name = string_field(node_block, "name=");
        out.push(PassiveNodeDef {
            skill,
            // GGG data.json 的 `id` 是导出侧字符串键；tree.lua 无对应物，用
            // skill id 字符串（库内消费方按 `skill` 数值索引，`id` 仅诊断用）。
            id: skill.to_string(),
            name,
            kind: PassiveNodeKind::Notable,
            stats,
            group: scalar_u32(node_block, "group="),
            orbit: scalar_u32(node_block, "orbit="),
            orbit_index: scalar_u32(node_block, "orbitIndex="),
            x: None,
            y: None,
            // 油涂池节点不在主图上（tree.lua `connections={}` 空），拓扑零参与。
            connections: Vec::new(),
            ascendancy_id: None,
            variants: Vec::new(),
            apply_to_armour: false,
        });
    }
    if out.is_empty() {
        return Err("tree.lua 未解析出任何缺失 notable（数据已齐或解析失败）".into());
    }
    Ok(out)
}
