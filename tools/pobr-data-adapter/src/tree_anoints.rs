//! Anoint notable pool backfill: GGG's official tree export `data.json`
//! **doesn't include** anoint-only notables that aren't on the main graph
//! (e.g. `Paragon` — the target node of the "Allocates Paragon" amulet
//! enchant), whereas vendor `tree.lua`'s top-level `nodes` block carries the
//! full definition (`isNotable=true` + its own `stats` + `recipe` (the
//! anoint recipe) + empty `connections`; the node enters calculation via
//! amulet anoint / `GrantedPassive`, looked up by name through vendor's
//! `spec.tree.notableMap`, CalcSetup.lua:1322-1331).
//!
//! This channel appends **notable** nodes that exist in `tree.lua` but are
//! missing from `passive_tree.json` (kind=Notable, `connections=[]`, no
//! coordinates — they don't participate in the main graph's topology, only
//! consumed by name-based granting). Same source and conventions as
//! `--tree-coords` / `--tree-variants` (writes back in place).

use pobr_data::catalog::{PassiveNodeDef, PassiveNodeKind};

use crate::tree_coords::{balanced_block, block_offset, scalar_u32, strip_nested_blocks};
use crate::tree_variants::{BlockKey, iter_keyed_blocks, parse_string_array, string_field};
use crate::write_pretty;

use std::path::PathBuf;

pub struct TreeAnointsArgs {
    /// vendor `tree.lua` (PoB2's full tree data).
    pub tree_lua: PathBuf,
    /// The root directory that contains `data/<patch>/` (matches `--out`).
    pub out: PathBuf,
    pub patch: String,
}

pub fn run(args: TreeAnointsArgs) -> Result<String, String> {
    let lua = std::fs::read_to_string(&args.tree_lua)
        .map_err(|e| format!("读取 {} 失败：{e}", args.tree_lua.display()))?;

    // The three-layer layout: prefer reading the existing passive_tree.json
    // from base/, falling back to the old layout (version root); write the
    // backfill back to wherever it was read from (same convention as tree_coords / tree_variants).
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

/// Extracts notable nodes missing from `passive_tree.json` out of `tree.lua`'s top-level `nodes` block.
///
/// Inclusion criteria (conservative, matching the anoint pool's shape):
/// `isNotable=true` and having its own `stats`. Missing non-notables
/// (cluster placeholders, etc.) don't belong to this channel and are skipped.
fn parse_missing_notables(
    lua: &str,
    existing: &std::collections::BTreeSet<u32>,
) -> Result<Vec<PassiveNodeDef>, String> {
    // The top-level nodes block comes after groups; search starting from
    // the end of the groups block to avoid a group's nested `nodes=`
    // (same technique as tree_variants).
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
            // GGG's data.json's `id` is the export side's string key;
            // tree.lua has no equivalent, so this uses the skill id as a
            // string (consumers within the catalog index by the numeric
            // `skill`; `id` is diagnostic-only).
            id: skill.to_string(),
            name,
            kind: PassiveNodeKind::Notable,
            stats,
            group: scalar_u32(node_block, "group="),
            orbit: scalar_u32(node_block, "orbit="),
            orbit_index: scalar_u32(node_block, "orbitIndex="),
            x: None,
            y: None,
            // Anoint-pool nodes aren't on the main graph (tree.lua's
            // `connections={}` is empty), so zero topology participation.
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
