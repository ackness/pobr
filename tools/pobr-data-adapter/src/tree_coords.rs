//! Node coordinate derivation: extracts `groups` center coordinates + orbit
//! constants + each node's own group/orbit/orbitIndex from PoB2 vendor
//! `tree.lua`, computes each node's planar (x, y) using PoB's
//! `PassiveTree.lua` formula, and backfills them into the existing
//! `passive_tree.json` by `skill` id (keeping every existing field, only adding x/y).
//!
//! Key point: **node -> group/orbit/orbitIndex must always come from
//! `tree.lua`'s `nodes` block**, never from `passive_tree.json`'s existing
//! `group` (which comes from GGG's `data.json`, whose group numbering
//! **doesn't match** PoB `tree.lua`'s `groups` table keys — the same skill
//! has a different group number on each side). All three coordinate inputs
//! (groups' centers, orbit constants, a node's orbit slot) must come from the same `tree.lua`.
//!
//! PoB2's `Classes/PassiveTree.lua:ProcessNode` coordinate formula (`scaleImage = 1`):
//! ```text
//! angle       = orbitAnglesByOrbit[orbit + 1][orbitIndex + 1]   -- a precomputed radian table
//! orbitRadius = orbitRadii[orbit + 1]
//! node.x = group.x + sin(angle) * orbitRadius
//! node.y = group.y - cos(angle) * orbitRadius
//! ```
//! Source `tree.lua` = `vendor/PathOfBuilding-PoE2/src/TreeData/0_5/tree.lua`,
//! i.e. GGG's official tree export after PoB's processing into full layout
//! data (including `groups`/`constants`/`nodes`).

use std::collections::BTreeMap;
use std::path::PathBuf;

use pobr_data::catalog::PassiveNodeDef;

use crate::write_pretty;

pub struct TreeCoordsArgs {
    /// vendor `tree.lua` (PoB2's full tree data).
    pub tree_lua: PathBuf,
    /// The root directory that contains `data/<patch>/` (matches `--out`).
    pub out: PathBuf,
    pub patch: String,
}

/// A node's position within the orbit layout (taken from `tree.lua`'s nodes block).
struct NodeOrbit {
    group: u32,
    orbit: u32,
    orbit_index: u32,
    /// Vendor's `applyToArmour=true` (the Smith of Kitava body-armour
    /// connection notable marker; extracted from the same nodes-block
    /// top-level field, backfilled into `PassiveNodeDef::apply_to_armour` along the way).
    apply_to_armour: bool,
}

/// Layout constants, group coordinates, and node orbit slots parsed from vendor `tree.lua`.
struct TreeLayout {
    /// group id -> (x, y) center coordinates.
    groups: BTreeMap<u32, (f64, f64)>,
    /// The orbit radius table, indexed by orbit (already 0-based, matching `node.orbit`).
    orbit_radii: Vec<f64>,
    /// Each orbit's precomputed angle table (radians), indexed by orbit, inner index = orbit_index.
    orbit_angles: Vec<Vec<f64>>,
    /// skill id -> orbit slot (the authoritative group/orbit/orbitIndex from PoB).
    node_orbits: BTreeMap<u32, NodeOrbit>,
}

impl TreeLayout {
    /// Computes (x, y) for the node with the given skill id, using PoB's formula; returns None when data is missing.
    fn position(&self, skill: u32) -> Option<(f64, f64)> {
        let no = self.node_orbits.get(&skill)?;
        let &(gx, gy) = self.groups.get(&no.group)?;
        let &radius = self.orbit_radii.get(no.orbit as usize)?;
        let angle = *self
            .orbit_angles
            .get(no.orbit as usize)?
            .get(no.orbit_index as usize)?;
        Some((gx + angle.sin() * radius, gy - angle.cos() * radius))
    }
}

pub fn run(args: TreeCoordsArgs) -> Result<String, String> {
    let lua = std::fs::read_to_string(&args.tree_lua)
        .map_err(|e| format!("failed to read {}: {e}", args.tree_lua.display()))?;
    let layout = parse_layout(&lua)?;

    // The three-layer layout: prefer reading the existing passive_tree.json
    // from base/, falling back to the old layout (version root); write the backfill back to wherever it was read from.
    let version_dir = args.out.join(&args.patch);
    let layered = version_dir.join("base/passive_tree.json");
    let tree_path = if layered.exists() {
        layered
    } else {
        version_dir.join("passive_tree.json")
    };
    let bytes = std::fs::read(&tree_path)
        .map_err(|e| format!("failed to read {}: {e}", tree_path.display()))?;
    let mut nodes: Vec<PassiveNodeDef> = serde_json::from_slice(&bytes)
        .map_err(|e| format!("failed to parse {}: {e}", tree_path.display()))?;

    // Off-graph nodes (the anointable pool: DeliriumAnoint / voices etc. —
    // no connections and referenced by nothing) don't participate in the
    // tree topology, so no coordinates are written even if tree.lua carries
    // a nominal orbit slot for them — otherwise the frontend would render
    // them as floating orphan points, and it would break the data invariant that off-graph nodes have no coordinates.
    let mut referenced: std::collections::HashSet<u32> = std::collections::HashSet::new();
    for node in &nodes {
        referenced.extend(node.connections.iter().copied());
    }
    let on_graph =
        |node: &PassiveNodeDef| !node.connections.is_empty() || referenced.contains(&node.skill);

    let mut filled = 0usize;
    let mut missing = 0usize;
    let mut off_graph = 0usize;
    for node in &mut nodes {
        // applyToArmour (the Smith body-armour connection notable) is backfilled from the same source alongside coordinates (regardless of on/off-graph status).
        node.apply_to_armour = layout
            .node_orbits
            .get(&node.skill)
            .is_some_and(|no| no.apply_to_armour);
        if !on_graph(node) {
            node.x = None;
            node.y = None;
            off_graph += 1;
            continue;
        }
        match layout.position(node.skill) {
            Some((x, y)) => {
                node.x = Some(round6(x));
                node.y = Some(round6(y));
                filled += 1;
            }
            None => {
                node.x = None;
                node.y = None;
                missing += 1;
            }
        }
    }

    write_pretty(&tree_path, &nodes)?;

    Ok(format!(
        "node coordinate backfill complete: {filled}/{} node(s) got x/y written ({missing} have no orbit data \
         in tree.lua, {off_graph} off-graph node(s) skipped); {} group(s), {} orbit tier(s), {} tree.lua node(s) -> {}",
        nodes.len(),
        layout.groups.len(),
        layout.orbit_radii.len(),
        layout.node_orbits.len(),
        tree_path.display(),
    ))
}

/// Rounds to 6 decimal places, avoiding floating-point noise polluting diffs (plenty of precision for tree units).
fn round6(v: f64) -> f64 {
    (v * 1e6).round() / 1e6
}

/// Extracts `constants.orbitRadii` / `constants.orbitAnglesByOrbit` / the
/// top-level `groups` / the top-level `nodes` (each node's group/orbit/orbitIndex) from `tree.lua`'s text.
fn parse_layout(lua: &str) -> Result<TreeLayout, String> {
    let const_block =
        balanced_block(lua, "constants={").ok_or("tree.lua: constants block not found")?;
    let orbit_radii = parse_number_array(const_block, "orbitRadii={")
        .ok_or("tree.lua: orbitRadii block not found")?;
    let orbit_angles = parse_nested_number_arrays(const_block, "orbitAnglesByOrbit={")
        .ok_or("tree.lua: orbitAnglesByOrbit block not found")?;

    // The top-level groups / nodes blocks (indented with `\t`), distinguished from a group's nested `nodes=`.
    let groups_block =
        balanced_block(lua, "\tgroups={").ok_or("tree.lua: top-level groups block not found")?;
    let groups = parse_groups(groups_block).ok_or("groups block parsed to empty")?;

    // The top-level nodes block comes after groups; search starting from the
    // end of the groups block to avoid a group's nested `nodes=`.
    let groups_end = block_offset(lua, groups_block);
    let nodes_block = balanced_block(&lua[groups_end..], "\tnodes={")
        .ok_or("tree.lua: top-level nodes block not found")?;
    let node_orbits = parse_node_orbits(nodes_block);
    if node_orbits.is_empty() {
        return Err("nodes block: no orbit slots were parsed".into());
    }

    Ok(TreeLayout {
        groups,
        orbit_radii,
        orbit_angles,
        node_orbits,
    })
}

/// Returns the byte offset in `whole` of the end of `sub` (a slice taken from `whole`).
pub(crate) fn block_offset(whole: &str, sub: &str) -> usize {
    let start = sub.as_ptr() as usize - whole.as_ptr() as usize;
    start + sub.len()
}

/// Finds the substring starting at the first `{` after `marker`, balanced to its matching `}` (including both braces).
pub(crate) fn balanced_block<'a>(lua: &'a str, marker: &str) -> Option<&'a str> {
    let start = lua.find(marker)?;
    let brace_start = start + marker.len() - 1;
    let bytes = lua.as_bytes();
    debug_assert_eq!(bytes[brace_start], b'{');
    let mut depth = 0i32;
    let mut i = brace_start;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&lua[brace_start..=i]);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Parses a Lua 1-based numeric array shaped like `{ [1]=0, [2]=82, ... }` into a 0-based `Vec<f64>`.
fn parse_number_array(lua: &str, marker: &str) -> Option<Vec<f64>> {
    let block = balanced_block(lua, marker)?;
    parse_indexed_numbers(block)
}

/// Parses every `[idx]=number` within a balanced block (skipping nested `[idx]={` entries), laying them out into a 0-based `Vec<f64>`.
fn parse_indexed_numbers(block: &str) -> Option<Vec<f64>> {
    let mut pairs: BTreeMap<usize, f64> = BTreeMap::new();
    let bytes = block.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            let close = block[i..].find(']')? + i;
            if let Ok(idx) = block[i + 1..close].trim().parse::<usize>() {
                let after = block[close + 1..].trim_start();
                if let Some(rest) = after.strip_prefix('=') {
                    let rest = rest.trim_start();
                    if !rest.starts_with('{') {
                        let end = rest
                            .find(|c: char| !matches!(c, '0'..='9' | '.' | '-' | '+' | 'e' | 'E'))
                            .unwrap_or(rest.len());
                        if let Ok(v) = rest[..end].parse::<f64>() {
                            pairs.insert(idx, v);
                        }
                    }
                }
            }
            i = close + 1;
        } else {
            i += 1;
        }
    }
    indexed_to_vec(pairs)
}

/// Converts a Lua 1-based index map into a 0-based `Vec` (indices must start at 1; missing slots default).
fn indexed_to_vec(pairs: BTreeMap<usize, f64>) -> Option<Vec<f64>> {
    let max = *pairs.keys().max()?;
    if pairs.contains_key(&0) {
        return None; // A Lua array never contains [0]
    }
    let mut out = vec![0.0; max];
    for (idx, v) in pairs {
        out[idx - 1] = v;
    }
    Some(out)
}

/// Parses `orbitAnglesByOrbit = { [1]={...}, [2]={...} }`, converting the outer 1-based indexing to 0-based.
fn parse_nested_number_arrays(lua: &str, marker: &str) -> Option<Vec<Vec<f64>>> {
    let block = balanced_block(lua, marker)?;
    let inner = &block[1..block.len() - 1];
    let mut pairs: BTreeMap<usize, Vec<f64>> = BTreeMap::new();
    let bytes = inner.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            let close = inner[i..].find(']')? + i;
            if let Ok(idx) = inner[i + 1..close].trim().parse::<usize>() {
                let after = inner[close + 1..].trim_start();
                if after.starts_with('=') && after[1..].trim_start().starts_with('{') {
                    let sub = balanced_block(&inner[close + 1..], "{")?;
                    let arr = parse_indexed_numbers(sub)?;
                    pairs.insert(idx, arr);
                    i = block_offset(inner, sub);
                    continue;
                }
            }
            i = close + 1;
        } else {
            i += 1;
        }
    }
    if pairs.contains_key(&0) {
        return None;
    }
    let max = *pairs.keys().max()?;
    let mut out = vec![Vec::new(); max];
    for (idx, v) in pairs {
        out[idx - 1] = v;
    }
    Some(out)
}

/// Parses the top-level `groups = { [id]={ ... x=..., y=... }, ... }` into group id -> (x, y).
fn parse_groups(block: &str) -> Option<BTreeMap<u32, (f64, f64)>> {
    let inner = &block[1..block.len() - 1];
    let mut groups: BTreeMap<u32, (f64, f64)> = BTreeMap::new();
    let bytes = inner.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            let close = match inner[i..].find(']') {
                Some(c) => c + i,
                None => break,
            };
            if let Ok(id) = inner[i + 1..close].trim().parse::<u32>() {
                let after = inner[close + 1..].trim_start();
                if after.starts_with('=') && after[1..].trim_start().starts_with('{') {
                    let sub = balanced_block(&inner[close + 1..], "{")?;
                    if let (Some(x), Some(y)) = (scalar_field(sub, "x="), scalar_field(sub, "y=")) {
                        groups.insert(id, (x, y));
                    }
                    i = block_offset(inner, sub);
                    continue;
                }
            }
            i = close + 1;
        } else {
            i += 1;
        }
    }
    if groups.is_empty() {
        None
    } else {
        Some(groups)
    }
}

/// Parses the top-level `nodes = { [skill]={ ... group=..., orbit=..., orbitIndex=... }, ... }`.
fn parse_node_orbits(block: &str) -> BTreeMap<u32, NodeOrbit> {
    let inner = &block[1..block.len() - 1];
    let mut out: BTreeMap<u32, NodeOrbit> = BTreeMap::new();
    let bytes = inner.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            let close = match inner[i..].find(']') {
                Some(c) => c + i,
                None => break,
            };
            if let Ok(skill) = inner[i + 1..close].trim().parse::<u32>() {
                let after = inner[close + 1..].trim_start();
                if after.starts_with('=')
                    && after[1..].trim_start().starts_with('{')
                    && let Some(sub) = balanced_block(&inner[close + 1..], "{")
                {
                    // A node block contains nested subtables like
                    // `connections`/`recipe`, which also have their own
                    // `orbit=` field. Strip all nested `{...}` first, and
                    // only read group/orbit/orbitIndex at the node's top level.
                    let top = strip_nested_blocks(sub);
                    if let (Some(group), Some(orbit)) =
                        (scalar_u32(&top, "group="), scalar_u32(&top, "orbit="))
                    {
                        let orbit_index = scalar_u32(&top, "orbitIndex=").unwrap_or(0);
                        out.insert(
                            skill,
                            NodeOrbit {
                                group,
                                orbit,
                                orbit_index,
                                apply_to_armour: top.contains("applyToArmour=true"),
                            },
                        );
                    }
                    i = block_offset(inner, sub);
                    continue;
                }
            }
            i = close + 1;
        } else {
            i += 1;
        }
    }
    out
}

/// Strips every nested subtable (depth >= 2 `{...}`) within a balanced block, keeping only the top-level text.
///
/// The input must be a balanced block with both `{`/`}`. A node block's
/// `group`/`orbit`/`orbitIndex` sit at the top level, while subtables like
/// `connections`/`recipe`/`stats` also contain a field named `orbit=`
/// internally — stripping avoids misreading those.
pub(crate) fn strip_nested_blocks(block: &str) -> String {
    let mut out = String::with_capacity(block.len());
    let mut depth = 0i32;
    for c in block.chars() {
        match c {
            '{' => {
                depth += 1;
                // A top-level (depth 1) `{` is kept as a placeholder; deeper ones aren't output.
                if depth <= 1 {
                    out.push(c);
                }
            }
            '}' => {
                if depth <= 1 {
                    out.push(c);
                }
                depth -= 1;
            }
            _ => {
                if depth <= 1 {
                    out.push(c);
                }
            }
        }
    }
    out
}

/// Extracts the float value of a top-level scalar field `field` (e.g. `"x="` / `"group="`) within a balanced block.
///
/// Anchored on `\n\t+field` (a newline, then any number of tab indents, then
/// the field name), avoiding a false match on a nested subtable's field of the same name.
pub(crate) fn scalar_field(block: &str, field: &str) -> Option<f64> {
    let mut search_from = 0;
    while let Some(rel) = block[search_from..].find(field) {
        let pos = search_from + rel;
        // The field name must be immediately preceded by a newline plus tab indentation (the top-level-field convention).
        let prefix_ok = block[..pos]
            .rfind('\n')
            .map(|nl| block[nl + 1..pos].chars().all(|c| c == '\t'))
            .unwrap_or(false);
        if prefix_ok {
            let rest = block[pos + field.len()..].trim_start();
            let end = rest
                .find(|c: char| !matches!(c, '0'..='9' | '.' | '-' | '+' | 'e' | 'E'))
                .unwrap_or(rest.len());
            if let Ok(v) = rest[..end].parse::<f64>() {
                return Some(v);
            }
        }
        search_from = pos + field.len();
    }
    None
}

/// The integer-parsing variant of a top-level scalar field.
pub(crate) fn scalar_u32(block: &str, field: &str) -> Option<u32> {
    scalar_field(block, field).map(|v| v as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_indexed_number_array() {
        let arr = parse_number_array("orbitRadii={\n[1]=0,\n[2]=82,\n[3]=162\n}", "orbitRadii={")
            .unwrap();
        assert_eq!(arr, vec![0.0, 82.0, 162.0]);
    }

    #[test]
    fn parses_nested_angle_arrays() {
        let lua = "orbitAnglesByOrbit={\n[1]={\n[1]=0,\n[2]=3.5\n},\n[2]={\n[1]=0,\n[2]=1.5\n}\n}";
        let arrs = parse_nested_number_arrays(lua, "orbitAnglesByOrbit={").unwrap();
        assert_eq!(arrs.len(), 2);
        assert_eq!(arrs[0], vec![0.0, 3.5]);
        assert_eq!(arrs[1], vec![0.0, 1.5]);
    }

    #[test]
    fn parses_groups_xy() {
        let block = "{\n\t\t[1]={\n\t\t\tnodes={\n\t\t\t\t[1]=42761\n\t\t\t},\n\t\t\tx=-15304.9,\n\t\t\ty=-7077.3\n\t\t},\n\t\t[4]={\n\t\t\tx=-14964.1,\n\t\t\ty=-6594.2\n\t\t}\n\t}";
        let groups = parse_groups(block).unwrap();
        assert_eq!(groups.len(), 2);
        assert!((groups[&1].0 - -15304.9).abs() < 1e-6);
        assert!((groups[&4].1 - -6594.2).abs() < 1e-6);
    }

    #[test]
    fn parses_node_orbits() {
        let block = "{\n\t\t[61419]={\n\t\t\tisJewelSocket=true,\n\t\t\tgroup=813,\n\t\t\torbit=0,\n\t\t\torbitIndex=0,\n\t\t\tskill=61419\n\t\t}\n\t}";
        let nodes = parse_node_orbits(block);
        let n = nodes.get(&61419).unwrap();
        assert_eq!((n.group, n.orbit, n.orbit_index), (813, 0, 0));
    }

    #[test]
    fn ignores_nested_connection_orbit() {
        // A node block's connections subtable contains `orbit=0`, which must not pollute the node's own `orbit=7`.
        let block = "{\n\t\t[94]={\n\t\t\tconnections={\n\t\t\t\t[1]={\n\t\t\t\t\tid=27234,\n\t\t\t\t\torbit=0\n\t\t\t\t}\n\t\t\t},\n\t\t\tgroup=946,\n\t\t\torbit=7,\n\t\t\torbitIndex=4,\n\t\t\tskill=94\n\t\t}\n\t}";
        let nodes = parse_node_orbits(block);
        let n = nodes.get(&94).unwrap();
        assert_eq!((n.group, n.orbit, n.orbit_index), (946, 7, 4));
    }

    #[test]
    fn strips_nested_blocks_keeps_top_level() {
        let s = "{ group=946, connections={ [1]={ orbit=0 } }, orbit=7 }";
        let top = strip_nested_blocks(s);
        assert!(top.contains("group=946"));
        assert!(top.contains("orbit=7"));
        assert!(!top.contains("orbit=0"));
    }

    #[test]
    fn computes_position_from_layout() {
        let mut groups = BTreeMap::new();
        groups.insert(813u32, (0.0, -7995.0));
        let mut node_orbits = BTreeMap::new();
        node_orbits.insert(
            61419u32,
            NodeOrbit {
                group: 813,
                orbit: 0,
                orbit_index: 0,
                apply_to_armour: false,
            },
        );
        let layout = TreeLayout {
            groups,
            orbit_radii: vec![0.0, 82.0],
            orbit_angles: vec![vec![0.0], vec![0.0, 0.523]],
            node_orbits,
        };
        // orbit 0 -> radius 0 -> lands at the group's center.
        let (x, y) = layout.position(61419).unwrap();
        assert_eq!((x, y), (0.0, -7995.0));
    }
}
