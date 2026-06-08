//! 节点坐标推导：从 PoB2 vendor `tree.lua` 抽取 `groups` 中心坐标 + orbit 常量 +
//! 节点自身的 group/orbit/orbitIndex，按 PoB `PassiveTree.lua` 公式计算每个节点的平面
//! (x, y)，按 `skill` id 回填入既有 `passive_tree.json`（保留全部现有字段，仅新增 x/y）。
//!
//! 关键：**节点→group/orbit/orbitIndex 一律取自 `tree.lua` 的 `nodes` 块**，不能用
//! `passive_tree.json` 现有的 `group`（源自 GGG `data.json`，其 group 编号与 PoB
//! `tree.lua` 的 `groups` 表键**不一致**——同一 skill 在两边 group 号不同）。坐标的三个
//! 输入（groups 中心、orbit 常量、节点 orbit 槽位）必须全部来自同一份 `tree.lua`。
//!
//! PoB2 `Classes/PassiveTree.lua:ProcessNode` 坐标公式（`scaleImage = 1`）：
//! ```text
//! angle       = orbitAnglesByOrbit[orbit + 1][orbitIndex + 1]   -- 预算弧度表
//! orbitRadius = orbitRadii[orbit + 1]
//! node.x = group.x + sin(angle) * orbitRadius
//! node.y = group.y - cos(angle) * orbitRadius
//! ```
//! 源 `tree.lua` = `vendor/PathOfBuilding-PoE2/src/TreeData/0_5/tree.lua`，
//! 即 GGG 官方树导出经 PoB 处理后的完整布局数据（含 `groups`/`constants`/`nodes`）。

use std::collections::BTreeMap;
use std::path::PathBuf;

use pobr_data::catalog::PassiveNodeDef;

use crate::write_pretty;

pub struct TreeCoordsArgs {
    /// vendor `tree.lua`（PoB2 完整树数据）。
    pub tree_lua: PathBuf,
    /// `data/<patch>/` 所在根目录（与 `--out` 一致）。
    pub out: PathBuf,
    pub patch: String,
}

/// 节点在 orbit 布局中的位置（取自 `tree.lua` nodes 块）。
struct NodeOrbit {
    group: u32,
    orbit: u32,
    orbit_index: u32,
}

/// 从 vendor `tree.lua` 解析出的布局常量、group 坐标与节点 orbit 槽位。
struct TreeLayout {
    /// group id → (x, y) 中心坐标。
    groups: BTreeMap<u32, (f64, f64)>,
    /// orbit 半径表，索引 = orbit（已 0-based，对齐 `node.orbit`）。
    orbit_radii: Vec<f64>,
    /// 每个 orbit 的预算角度表（弧度），索引 = orbit，内层索引 = orbit_index。
    orbit_angles: Vec<Vec<f64>>,
    /// skill id → orbit 槽位（PoB 权威 group/orbit/orbitIndex）。
    node_orbits: BTreeMap<u32, NodeOrbit>,
}

impl TreeLayout {
    /// 按 PoB 公式计算 skill id 对应节点的 (x, y)；缺数据返回 None。
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
        .map_err(|e| format!("读取 {} 失败：{e}", args.tree_lua.display()))?;
    let layout = parse_layout(&lua)?;

    let version_dir = args.out.join(&args.patch);
    let tree_path = version_dir.join("passive_tree.json");
    let bytes =
        std::fs::read(&tree_path).map_err(|e| format!("读取 {} 失败：{e}", tree_path.display()))?;
    let mut nodes: Vec<PassiveNodeDef> = serde_json::from_slice(&bytes)
        .map_err(|e| format!("解析 {} 失败：{e}", tree_path.display()))?;

    let mut filled = 0usize;
    let mut missing = 0usize;
    for node in &mut nodes {
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
        "节点坐标回填完成：{filled}/{} 节点写入 x/y（{missing} 个在 tree.lua 无 orbit 数据）；\
         groups {} 组，orbit 档 {} 个，tree.lua nodes {} 条 → {}",
        nodes.len(),
        layout.groups.len(),
        layout.orbit_radii.len(),
        layout.node_orbits.len(),
        tree_path.display(),
    ))
}

/// 保留 6 位小数，避免浮点尾差污染 diff（tree units 精度足够）。
fn round6(v: f64) -> f64 {
    (v * 1e6).round() / 1e6
}

/// 从 `tree.lua` 文本抽取 `constants.orbitRadii` / `constants.orbitAnglesByOrbit` /
/// 顶层 `groups` / 顶层 `nodes`（节点 group/orbit/orbitIndex）。
fn parse_layout(lua: &str) -> Result<TreeLayout, String> {
    let const_block = balanced_block(lua, "constants={").ok_or("tree.lua 未找到 constants 块")?;
    let orbit_radii =
        parse_number_array(const_block, "orbitRadii={").ok_or("tree.lua 未找到 orbitRadii 块")?;
    let orbit_angles = parse_nested_number_arrays(const_block, "orbitAnglesByOrbit={")
        .ok_or("tree.lua 未找到 orbitAnglesByOrbit 块")?;

    // 顶层 groups / nodes 块（缩进 `\t`），与 group 内嵌的 `nodes=` 区分。
    let groups_block = balanced_block(lua, "\tgroups={").ok_or("tree.lua 未找到顶层 groups 块")?;
    let groups = parse_groups(groups_block).ok_or("groups 块解析为空")?;

    // 顶层 nodes 块在 groups 之后；从 groups 块末尾起查找以避开组内 `nodes=`。
    let groups_end = block_offset(lua, groups_block);
    let nodes_block =
        balanced_block(&lua[groups_end..], "\tnodes={").ok_or("tree.lua 未找到顶层 nodes 块")?;
    let node_orbits = parse_node_orbits(nodes_block);
    if node_orbits.is_empty() {
        return Err("nodes 块未解析出任何 orbit 槽位".into());
    }

    Ok(TreeLayout {
        groups,
        orbit_radii,
        orbit_angles,
        node_orbits,
    })
}

/// 返回 `sub`（来自 `whole` 的切片）末尾在 `whole` 中的字节偏移。
fn block_offset(whole: &str, sub: &str) -> usize {
    let start = sub.as_ptr() as usize - whole.as_ptr() as usize;
    start + sub.len()
}

/// 找到 `marker` 后第一个 `{` 起、配平到对应 `}` 的子串（含两端花括号）。
fn balanced_block<'a>(lua: &'a str, marker: &str) -> Option<&'a str> {
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

/// 解析形如 `{ [1]=0, [2]=82, ... }` 的 Lua 1-based 数值数组 → 0-based `Vec<f64>`。
fn parse_number_array(lua: &str, marker: &str) -> Option<Vec<f64>> {
    let block = balanced_block(lua, marker)?;
    parse_indexed_numbers(block)
}

/// 在配平块内解析所有 `[idx]=number`（跳过 `[idx]={` 嵌套项），铺成 0-based `Vec<f64>`。
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

/// Lua 1-based 索引 map → 0-based `Vec`（索引须从 1 起；缺位填默认）。
fn indexed_to_vec(pairs: BTreeMap<usize, f64>) -> Option<Vec<f64>> {
    let max = *pairs.keys().max()?;
    if pairs.contains_key(&0) {
        return None; // Lua 数组不含 [0]
    }
    let mut out = vec![0.0; max];
    for (idx, v) in pairs {
        out[idx - 1] = v;
    }
    Some(out)
}

/// 解析 `orbitAnglesByOrbit = { [1]={...}, [2]={...} }` → 外层 1-based 转 0-based。
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

/// 解析顶层 `groups = { [id]={ ... x=..., y=... }, ... }` → group id → (x, y)。
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

/// 解析顶层 `nodes = { [skill]={ ... group=..., orbit=..., orbitIndex=... }, ... }`。
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
                    // 节点块含 `connections`/`recipe` 等嵌套子表，其内部亦有 `orbit=` 字段。
                    // 先剥离所有嵌套 `{...}`，只在节点顶层读 group/orbit/orbitIndex。
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

/// 剥离一个配平块内的所有嵌套子表（depth ≥ 2 的 `{...}`），仅保留顶层文本。
///
/// 输入须为含两端 `{`/`}` 的配平块。节点块的 `group`/`orbit`/`orbitIndex` 在顶层，而
/// `connections`/`recipe`/`stats` 等子表内部也含同名 `orbit=` 字段——剥离后避免误读。
fn strip_nested_blocks(block: &str) -> String {
    let mut out = String::with_capacity(block.len());
    let mut depth = 0i32;
    for c in block.chars() {
        match c {
            '{' => {
                depth += 1;
                // 顶层（depth 1）的 `{` 保留为占位换行，深层不输出。
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

/// 在配平块内抽取顶层标量字段 `field`（如 `"x="` / `"group="`）的浮点值。
///
/// 用 `\n\t+field`（换行 + 任意制表符缩进 + 字段名）锚定，避免误命中嵌套子表同名 key。
fn scalar_field(block: &str, field: &str) -> Option<f64> {
    let mut search_from = 0;
    while let Some(rel) = block[search_from..].find(field) {
        let pos = search_from + rel;
        // 字段名前须紧邻换行 + 制表符缩进（顶层字段约定）。
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

/// 顶层标量字段的整数解析版。
fn scalar_u32(block: &str, field: &str) -> Option<u32> {
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
        // 节点块的 connections 子表含 `orbit=0`，不得污染节点自身的 `orbit=7`。
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
            },
        );
        let layout = TreeLayout {
            groups,
            orbit_radii: vec![0.0, 82.0],
            orbit_angles: vec![vec![0.0], vec![0.0, 0.523]],
            node_orbits,
        };
        // orbit 0 → radius 0 → 落在 group 中心。
        let (x, y) = layout.position(61419).unwrap();
        assert_eq!((x, y), (0.0, -7995.0));
    }
}
