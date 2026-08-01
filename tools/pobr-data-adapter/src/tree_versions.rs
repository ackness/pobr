//! Historical league tree version extraction: vendor `TreeData/<v>/tree.lua`
//! -> `base/passive_trees/<v>.json`.
//!
//! Background: GGG's official tree export (the `--tree` channel) only has a
//! snapshot for the **current patch**; when an old-league build (`<Spec
//! treeVersion>` = `0_1`..`0_4`) is imported and computed against the
//! current tree's mods, node values/shapes can be entirely different (e.g.
//! 53853 "Backup Plan" is two lines of 50/50 in 0_3 but three lines of
//! 20/40/40 in 0_5). PoB2's approach is a full `TreeData/<v>/tree.lua` per
//! version, loaded by the build's treeVersion — this channel extracts
//! vendor's historical trees into PoBR-storable JSON, and the consumption
//! side (`pobr-build::resolve_passive_nodes`) picks the tree by treeVersion.
//!
//! **Scope** (minimum viable = correct node mods for old-version builds):
//! skill / name / kind / stats / ascendancy attribution. connections /
//! coordinates / isSwitchable variants aren't extracted — advanced features
//! like radius jewel geometry and connected-notable degrade to a
//! default-tree approximation for old-version trees (a best-effort approach
//! for historical versions, matching the vendor-fallback approach in the "data source policy").
//!
//! kind determination (vendor's node scalar flags; only looks at the node's
//! own fields after `strip_nested_blocks`): `isKeystone` -> Keystone;
//! `isAscendancyStart` -> AscendancyStart; `isJewelSocket` -> JewelSocket;
//! `isOnlyImage` -> Mastery (PoE2's attribute/mastery icon nodes, the
//! counterpart of GGG export's kind=mastery, verified against skill 259
//! "Attack Mastery"); `isNotable` -> Notable; no flag -> Normal.

use pobr_data::catalog::{PassiveNodeDef, PassiveNodeKind};

use crate::tree_coords::{balanced_block, block_offset, strip_nested_blocks};
use crate::tree_variants::{BlockKey, iter_keyed_blocks, parse_string_array, string_field};
use crate::write_pretty;

use std::path::PathBuf;

pub struct TreeVersionsArgs {
    /// vendor `TreeData/<v>/tree.lua` (full tree data for the historical version).
    pub tree_lua: PathBuf,
    /// The tree version number (PoB's `<Spec treeVersion>` form, e.g. `0_3`) — used as the output file name.
    pub tree_version: String,
    /// The root directory that contains `data/<patch>/` (matches `--out`).
    pub out: PathBuf,
    pub patch: String,
}

pub fn run(args: TreeVersionsArgs) -> Result<String, String> {
    let lua = std::fs::read_to_string(&args.tree_lua)
        .map_err(|e| format!("failed to read {}: {e}", args.tree_lua.display()))?;

    let nodes = parse_all_nodes(&lua)?;
    let count = nodes.len();

    let out_dir = args.out.join(&args.patch).join("base/passive_trees");
    std::fs::create_dir_all(&out_dir)
        .map_err(|e| format!("failed to create directory {out_dir:?}: {e}"))?;
    let out_path = out_dir.join(format!("{}.json", args.tree_version));
    write_pretty(&out_path, &nodes)?;

    Ok(format!(
        "tree version {} extraction complete: {count} node(s) -> {}",
        args.tree_version,
        out_path.display(),
    ))
}

/// Extracts every numeric-id node from `tree.lua`'s top-level `nodes` block (the minimal set of mods).
fn parse_all_nodes(lua: &str) -> Result<Vec<PassiveNodeDef>, String> {
    // The top-level nodes block comes after groups; search starting from the
    // end of the groups block to avoid a group's nested `nodes=`
    // (same technique as tree_anoints / tree_variants).
    let groups_block =
        balanced_block(lua, "\tgroups={").ok_or("tree.lua: top-level groups block not found")?;
    let groups_end = block_offset(lua, groups_block);
    let nodes_block = balanced_block(&lua[groups_end..], "\tnodes={")
        .ok_or("tree.lua: top-level nodes block not found")?;

    let mut out: Vec<PassiveNodeDef> = Vec::new();
    for (key, node_block) in iter_keyed_blocks(nodes_block) {
        let BlockKey::Num(skill) = key else {
            continue;
        };
        let scalars = strip_nested_blocks(node_block);
        let kind = if scalars.contains("isKeystone=true") {
            PassiveNodeKind::Keystone
        } else if scalars.contains("isAscendancyStart=true") {
            PassiveNodeKind::AscendancyStart
        } else if scalars.contains("isJewelSocket=true") {
            PassiveNodeKind::JewelSocket
        } else if scalars.contains("isOnlyImage=true") {
            PassiveNodeKind::Mastery
        } else if scalars.contains("isNotable=true") {
            PassiveNodeKind::Notable
        } else {
            PassiveNodeKind::Normal
        };
        out.push(PassiveNodeDef {
            skill,
            // tree.lua has no GGG-export-side string key, so this uses the
            // skill id as a string (consumers index by the numeric `skill`;
            // `id` is diagnostic-only; same convention as tree_anoints).
            id: skill.to_string(),
            name: string_field(node_block, "name="),
            kind,
            stats: parse_string_array(node_block, "stats={"),
            group: None,
            orbit: None,
            orbit_index: None,
            x: None,
            y: None,
            // ponytail: historical trees only keep mods correct; topology/coordinates
            // degrade to a default-tree approximation. Extract connections+coordinates too
            // if connected-notable / radius jewel historical accuracy is ever needed.
            connections: Vec::new(),
            // Vendor gives the ascendancy display name (not GGG's
            // `<Class><N>` id) — the consumption side only checks
            // `is_some()` to determine "belongs to an ascendancy", which is semantically equivalent.
            ascendancy_id: string_field(node_block, "ascendancyName="),
            variants: Vec::new(),
            apply_to_armour: false,
        });
    }
    if out.is_empty() {
        return Err("tree.lua: no nodes were parsed (parse failure?)".into());
    }
    out.sort_by_key(|n| n.skill);
    Ok(out)
}
