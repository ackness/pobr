//! isSwitchable 变体回填：从 PoB2 vendor `tree.lua` 抽取 `isSwitchable` 节点的
//! `options.<Class>` 按职业/飞升变体，按 `skill` id 回填入既有 `passive_tree.json`
//! 的 `variants` 字段（保留全部现有字段，仅覆写 `variants`）。
//!
//! 数据通道：GGG 官方树导出 `data.json` **不携带** `options` 变体（本地 pipeline
//! 亦无该导出快照），vendor `tree.lua` 是唯一可再生通道——与 `--tree-coords`
//! 同源（`vendor/PathOfBuilding-PoE2/src/TreeData/0_5/tree.lua`）。
//!
//! 语义对齐 PoB2：
//! - `PassiveSpec.lua:1251-1256`：`options[curClassName]`（其次
//!   `options[curAscendClassName]`）存在时整体替换节点（`ReplaceNode`）。
//! - `PassiveTree.lua:546-556`：option 经 `__index` 元表继承基础节点字段，
//!   `switchNode.sd = switchNode.stats`——**无自有 `stats` 的 option 行为等同
//!   基础版**（纯外观），不入库。
//! - 属性小点（`isAttribute`，options 键为数值索引）不属于本通道，由
//!   `attribute_overrides` 三选一逻辑处理；数值键一律跳过。

use std::collections::BTreeMap;
use std::path::PathBuf;

use pobr_data::catalog::{PassiveNodeDef, PassiveNodeVariant};

use crate::tree_coords::{balanced_block, block_offset, scalar_u32, strip_nested_blocks};
use crate::write_pretty;

pub struct TreeVariantsArgs {
    /// vendor `tree.lua`（PoB2 完整树数据）。
    pub tree_lua: PathBuf,
    /// `data/<patch>/` 所在根目录（与 `--out` 一致）。
    pub out: PathBuf,
    pub patch: String,
}

pub fn run(args: TreeVariantsArgs) -> Result<String, String> {
    let lua = std::fs::read_to_string(&args.tree_lua)
        .map_err(|e| format!("读取 {} 失败：{e}", args.tree_lua.display()))?;
    let variants = parse_switchable_variants(&lua)?;
    let switchable_total = variants.len();
    let variant_total: usize = variants.values().map(Vec::len).sum();

    // 三层布局：base/ 优先读取既有 passive_tree.json，旧布局（版本根）回退；
    // 回填后原地写回所读到的位置（与 tree_coords 同约定）。
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

    let mut filled = 0usize;
    let mut remaining = variants;
    for node in &mut nodes {
        node.variants = remaining.remove(&node.skill).unwrap_or_default();
        if !node.variants.is_empty() {
            filled += 1;
        }
    }
    // tree.lua 有而 passive_tree.json 没有的节点（如 cluster 占位）：报告但不报错。
    let unmatched: Vec<u32> = remaining.keys().copied().collect();

    write_pretty(&tree_path, &nodes)?;

    Ok(format!(
        "isSwitchable 变体回填完成：tree.lua 含变体节点 {switchable_total} 个 / 变体 {variant_total} 条，\
         回填 {filled} 个节点（未匹配 skill：{unmatched:?}）→ {}",
        tree_path.display(),
    ))
}

/// 从 `tree.lua` 顶层 `nodes` 块抽取所有 `isSwitchable` 节点的字符串键 options
/// （仅收录带自有 `stats` 的 option）。返回 `skill id -> 变体列表（按 class 排序）`。
fn parse_switchable_variants(lua: &str) -> Result<BTreeMap<u32, Vec<PassiveNodeVariant>>, String> {
    // 顶层 nodes 块在 groups 之后；从 groups 块末尾起查找以避开组内 `nodes=`。
    let groups_block = balanced_block(lua, "\tgroups={").ok_or("tree.lua 未找到顶层 groups 块")?;
    let groups_end = block_offset(lua, groups_block);
    let nodes_block =
        balanced_block(&lua[groups_end..], "\tnodes={").ok_or("tree.lua 未找到顶层 nodes 块")?;

    let mut out: BTreeMap<u32, Vec<PassiveNodeVariant>> = BTreeMap::new();
    for (skill, node_block) in iter_keyed_blocks(nodes_block) {
        let BlockKey::Num(skill) = skill else {
            continue;
        };
        // isSwitchable 是节点顶层标志（options 子块内无同名字段，剥嵌套防御性保留）。
        if !strip_nested_blocks(node_block).contains("isSwitchable=true") {
            continue;
        }
        let Some(options_block) = balanced_block(node_block, "options={") else {
            continue;
        };
        let mut variants: Vec<PassiveNodeVariant> = Vec::new();
        for (key, option_block) in iter_keyed_blocks(options_block) {
            // 字符串键 = 职业/飞升变体；数值键 = 属性小点三选一（attribute_overrides 通道）。
            let BlockKey::Str(class) = key else {
                continue;
            };
            let stats = parse_string_array(option_block, "stats={");
            if stats.is_empty() {
                // 无自有 stats 的 option 经 Lua `__index` 继承基础词条（纯外观），跳过。
                continue;
            }
            variants.push(PassiveNodeVariant {
                class,
                variant_skill: scalar_u32(option_block, "id="),
                name: string_field(option_block, "name="),
                stats,
            });
        }
        if !variants.is_empty() {
            variants.sort_by(|a, b| a.class.cmp(&b.class));
            out.insert(skill, variants);
        }
    }
    if out.is_empty() {
        return Err("tree.lua 未解析出任何 isSwitchable 变体".into());
    }
    Ok(out)
}

/// 配平块内顶层键（Lua table key）。
pub(crate) enum BlockKey {
    /// `[123]=` 数值键。
    Num(u32),
    /// `["Abyssal Lich"]=` 或 `Druid=` 字符串键。
    Str(String),
}

/// 迭代配平块（含两端 `{}`）内深度 1 的 `key={...}` 子块，返回（键，子块）。
///
/// 支持三种键形态：`[123]={`、`["Name with space"]={`、`BareWord={`；
/// 标量字段（`id=64801,`/`name="..."`）不产出。
pub(crate) fn iter_keyed_blocks(block: &str) -> Vec<(BlockKey, &str)> {
    let inner = &block[1..block.len() - 1];
    let mut out = Vec::new();
    let bytes = inner.as_bytes();
    let mut i = 0usize;
    let mut depth = 0i32;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            b'"' => {
                // 跳过字符串字面量（含转义），防止串内 `{`/`}` 污染深度计数。
                i += 1;
                while i < bytes.len() {
                    match bytes[i] {
                        b'\\' => i += 1,
                        b'"' => break,
                        _ => {}
                    }
                    i += 1;
                }
            }
            b'\n' if depth == 0 => {
                // 行首（任意制表符缩进）尝试匹配 `key={`。
                let line_start = i + 1;
                let mut j = line_start;
                while j < bytes.len() && bytes[j] == b'\t' {
                    j += 1;
                }
                if let Some((key, brace_at)) = parse_block_key(inner, j)
                    && let Some(sub) = balanced_block(&inner[brace_at..], "{")
                {
                    out.push((key, sub));
                    i = brace_at + sub.len();
                    continue;
                }
            }
            _ => {}
        }
        i += 1;
    }
    out
}

/// 在 `inner[at..]` 处解析 `key={` 形态的键，返回（键，`{` 的偏移）。非子块行返回 None。
fn parse_block_key(inner: &str, at: usize) -> Option<(BlockKey, usize)> {
    let rest = &inner[at..];
    if let Some(r) = rest.strip_prefix("[\"") {
        // ["Name"]={
        let close = r.find("\"]")?;
        let after = &r[close + 2..];
        if !after.starts_with("={") {
            return None;
        }
        let brace_at = at + 2 + close + 2 + 1;
        return Some((BlockKey::Str(r[..close].to_string()), brace_at));
    }
    if let Some(r) = rest.strip_prefix('[') {
        // [123]={
        let close = r.find(']')?;
        let num: u32 = r[..close].trim().parse().ok()?;
        let after = &r[close + 1..];
        if !after.starts_with("={") {
            return None;
        }
        let brace_at = at + 1 + close + 1 + 1;
        return Some((BlockKey::Num(num), brace_at));
    }
    // BareWord={
    let word_len = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    if word_len == 0 || !rest[word_len..].starts_with("={") {
        return None;
    }
    Some((
        BlockKey::Str(rest[..word_len].to_string()),
        at + word_len + 1,
    ))
}

/// 解析块内 `marker` 处的字符串数组（如 `stats={ [1]="...", [2]="..." }`），
/// 按数值索引排序展开；无该子块返回空。
pub(crate) fn parse_string_array(block: &str, marker: &str) -> Vec<String> {
    let Some(sub) = balanced_block(block, marker) else {
        return Vec::new();
    };
    let inner = &sub[1..sub.len() - 1];
    let mut pairs: BTreeMap<u32, String> = BTreeMap::new();
    let bytes = inner.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            let Some(close_rel) = inner[i..].find(']') else {
                break;
            };
            let close = i + close_rel;
            if let Ok(idx) = inner[i + 1..close].trim().parse::<u32>() {
                let after = inner[close + 1..].trim_start();
                if let Some(rest) = after.strip_prefix('=')
                    && let Some((s, _)) = parse_lua_string(rest.trim_start())
                {
                    pairs.insert(idx, s);
                }
            }
            i = close + 1;
        } else {
            i += 1;
        }
    }
    pairs.into_values().collect()
}

/// 块内顶层标量字符串字段（如 `name="Jagged Shards"`）。
pub(crate) fn string_field(block: &str, field: &str) -> Option<String> {
    let top = strip_nested_blocks(block);
    let mut search_from = 0;
    while let Some(rel) = top[search_from..].find(field) {
        let pos = search_from + rel;
        let prefix_ok = top[..pos]
            .rfind('\n')
            .map(|nl| top[nl + 1..pos].chars().all(|c| c == '\t'))
            .unwrap_or(false);
        if prefix_ok && let Some((s, _)) = parse_lua_string(&top[pos + field.len()..]) {
            return Some(s);
        }
        search_from = pos + field.len();
    }
    None
}

/// 解析以 `"` 开头的 Lua 双引号字符串字面量（处理 `\"`、`\n`、`\\` 转义），
/// 返回（解码值，字面量总长）。非字符串开头返回 None。
fn parse_lua_string(s: &str) -> Option<(String, usize)> {
    let bytes = s.as_bytes();
    if bytes.first() != Some(&b'"') {
        return None;
    }
    let mut out = String::new();
    let mut i = 1usize;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => return Some((out, i + 1)),
            b'\\' => {
                let esc = *bytes.get(i + 1)?;
                match esc {
                    b'n' => out.push('\n'),
                    b't' => out.push('\t'),
                    other => out.push(other as char),
                }
                i += 2;
            }
            _ => {
                // 按 UTF-8 字符推进（tree.lua 为 UTF-8 文本）。
                let ch = s[i..].chars().next()?;
                out.push(ch);
                i += ch.len_utf8();
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 模拟 tree.lua 顶层结构（含 groups 块 + nodes 块）的最小样本。
    fn sample_lua() -> &'static str {
        concat!(
            "tree={\n",
            "\tgroups={\n",
            "\t\t[1]={\n\t\t\tx=0,\n\t\t\ty=0\n\t\t}\n",
            "\t},\n",
            "\tnodes={\n",
            // isSwitchable + Witch 变体（带 stats）
            "\t\t[51335]={\n",
            "\t\t\tisNotable=true,\n",
            "\t\t\tisSwitchable=true,\n",
            "\t\t\tname=\"Affliction Enforcer\",\n",
            "\t\t\toptions={\n",
            "\t\t\t\tWitch={\n",
            "\t\t\t\t\ticon=\"Art/2DArt/SkillIcons/WitchBoneStorm.dds\",\n",
            "\t\t\t\t\tid=64801,\n",
            "\t\t\t\t\tname=\"Jagged Shards\",\n",
            "\t\t\t\t\tstats={\n",
            "\t\t\t\t\t\t[1]=\"20% increased Critical Hit Chance for Spells\",\n",
            "\t\t\t\t\t\t[2]=\"20% increased Physical Damage\"\n",
            "\t\t\t\t\t}\n",
            "\t\t\t\t}\n",
            "\t\t\t},\n",
            "\t\t\tskill=51335,\n",
            "\t\t\tstats={\n\t\t\t\t[1]=\"40% increased Flammability Magnitude\"\n\t\t\t}\n",
            "\t\t},\n",
            // isSwitchable + 飞升名键（无自有 stats，纯外观 → 不入库）
            "\t\t[59]={\n",
            "\t\t\tisSwitchable=true,\n",
            "\t\t\toptions={\n",
            "\t\t\t\t[\"Abyssal Lich\"]={\n",
            "\t\t\t\t\tascendancyName=\"Abyssal Lich\"\n",
            "\t\t\t\t}\n",
            "\t\t\t},\n",
            "\t\t\tskill=59,\n",
            "\t\t\tstats={\n\t\t\t\t[1]=\"You can apply an additional Curse\"\n\t\t\t}\n",
            "\t\t},\n",
            // 属性小点（数值键 options，非 isSwitchable）→ 跳过
            "\t\t[51299]={\n",
            "\t\t\toptions={\n",
            "\t\t\t\t[1]={\n\t\t\t\t\tid=29939,\n\t\t\t\t\tstats={\n\t\t\t\t\t\t[1]=\"+5 to Strength\"\n\t\t\t\t\t}\n\t\t\t\t}\n",
            "\t\t\t},\n",
            "\t\t\tskill=51299,\n",
            "\t\t\tstats={\n\t\t\t\t[1]=\"+5 to any Attribute\"\n\t\t\t}\n",
            "\t\t}\n",
            "\t}\n",
            "}\n",
        )
    }

    #[test]
    fn extracts_class_keyed_variant_with_stats() {
        let map = parse_switchable_variants(sample_lua()).unwrap();
        assert_eq!(map.len(), 1, "仅带自有 stats 的字符串键 option 入库");
        let v = &map[&51335];
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].class, "Witch");
        assert_eq!(v[0].variant_skill, Some(64801));
        assert_eq!(v[0].name.as_deref(), Some("Jagged Shards"));
        assert_eq!(
            v[0].stats,
            vec![
                "20% increased Critical Hit Chance for Spells".to_string(),
                "20% increased Physical Damage".to_string(),
            ]
        );
    }

    #[test]
    fn skips_cosmetic_only_and_numeric_options() {
        let map = parse_switchable_variants(sample_lua()).unwrap();
        assert!(
            !map.contains_key(&59),
            "无自有 stats 的飞升外观 option 跳过"
        );
        assert!(!map.contains_key(&51299), "属性小点数值键 options 跳过");
    }

    #[test]
    fn parses_quoted_bracket_string_key() {
        let lua = sample_lua().replace(
            "\t\t\t\t\tascendancyName=\"Abyssal Lich\"\n",
            "\t\t\t\t\tid=11705,\n\t\t\t\t\tstats={\n\t\t\t\t\t\t[1]=\"+10 to Intelligence\"\n\t\t\t\t\t}\n",
        );
        let map = parse_switchable_variants(&lua).unwrap();
        let v = &map[&59];
        assert_eq!(v[0].class, "Abyssal Lich");
        assert_eq!(v[0].variant_skill, Some(11705));
        assert_eq!(v[0].stats, vec!["+10 to Intelligence".to_string()]);
    }

    #[test]
    fn lua_string_unescapes() {
        let (s, len) = parse_lua_string(r#""a\"b\nc""#).unwrap();
        assert_eq!(s, "a\"b\nc");
        assert_eq!(len, 9);
    }
}
