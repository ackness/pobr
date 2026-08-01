//! isSwitchable variant backfill: extracts `isSwitchable` nodes'
//! `options.<Class>` per-class/ascendancy variants from PoB2 vendor
//! `tree.lua`, backfilling them by `skill` id into the existing
//! `passive_tree.json`'s `variants` field (keeping every existing field, only overwriting `variants`).
//!
//! Data channel: GGG's official tree export `data.json` **doesn't carry**
//! `options` variants (and the local pipeline has no such export snapshot
//! either), so vendor `tree.lua` is the only regenerable channel — same
//! source as `--tree-coords` (`vendor/PathOfBuilding-PoE2/src/TreeData/0_5/tree.lua`).
//!
//! Semantics matching PoB2:
//! - `PassiveSpec.lua:1251-1256`: when `options[curClassName]` exists (or
//!   `options[curAscendClassName]` as a fallback), the whole node is replaced (`ReplaceNode`).
//! - `PassiveTree.lua:546-556`: an option inherits the base node's fields
//!   via a Lua `__index` metatable, with `switchNode.sd = switchNode.stats`
//!   — **an option with no `stats` of its own behaves identically to the
//!   base node** (purely cosmetic), so it isn't stored.
//! - Small attribute nodes (`isAttribute`, whose options keys are numeric
//!   indices) don't belong to this channel — they're handled by the
//!   `attribute_overrides` three-way-choice logic instead; numeric keys are always skipped.

use std::collections::BTreeMap;
use std::path::PathBuf;

use pobr_data::catalog::{PassiveNodeDef, PassiveNodeVariant};

use crate::tree_coords::{balanced_block, block_offset, scalar_u32, strip_nested_blocks};
use crate::write_pretty;

pub struct TreeVariantsArgs {
    /// vendor `tree.lua` (PoB2's full tree data).
    pub tree_lua: PathBuf,
    /// The root directory that contains `data/<patch>/` (matches `--out`).
    pub out: PathBuf,
    pub patch: String,
}

pub fn run(args: TreeVariantsArgs) -> Result<String, String> {
    let lua = std::fs::read_to_string(&args.tree_lua)
        .map_err(|e| format!("读取 {} 失败：{e}", args.tree_lua.display()))?;
    let variants = parse_switchable_variants(&lua)?;
    let switchable_total = variants.len();
    let variant_total: usize = variants.values().map(Vec::len).sum();

    // The three-layer layout: prefer reading the existing passive_tree.json
    // from base/, falling back to the old layout (version root); write the
    // backfill back to wherever it was read from (same convention as tree_coords).
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
    // Nodes present in tree.lua but not in passive_tree.json (e.g. cluster placeholders): reported but not an error.
    let unmatched: Vec<u32> = remaining.keys().copied().collect();

    write_pretty(&tree_path, &nodes)?;

    Ok(format!(
        "isSwitchable 变体回填完成：tree.lua 含变体节点 {switchable_total} 个 / 变体 {variant_total} 条，\
         回填 {filled} 个节点（未匹配 skill：{unmatched:?}）→ {}",
        tree_path.display(),
    ))
}

/// Extracts the string-keyed options of every `isSwitchable` node from
/// `tree.lua`'s top-level `nodes` block (only including options with their
/// own `stats`). Returns `skill id -> variant list (sorted by class)`.
fn parse_switchable_variants(lua: &str) -> Result<BTreeMap<u32, Vec<PassiveNodeVariant>>, String> {
    // The top-level nodes block comes after groups; search starting from the
    // end of the groups block to avoid a group's nested `nodes=`.
    let groups_block = balanced_block(lua, "\tgroups={").ok_or("tree.lua 未找到顶层 groups 块")?;
    let groups_end = block_offset(lua, groups_block);
    let nodes_block =
        balanced_block(&lua[groups_end..], "\tnodes={").ok_or("tree.lua 未找到顶层 nodes 块")?;

    let mut out: BTreeMap<u32, Vec<PassiveNodeVariant>> = BTreeMap::new();
    for (skill, node_block) in iter_keyed_blocks(nodes_block) {
        let BlockKey::Num(skill) = skill else {
            continue;
        };
        // isSwitchable is a top-level node flag (no field of the same name
        // inside the options subtable, but stripping nested blocks is kept as a defensive measure).
        if !strip_nested_blocks(node_block).contains("isSwitchable=true") {
            continue;
        }
        let Some(options_block) = balanced_block(node_block, "options={") else {
            continue;
        };
        let mut variants: Vec<PassiveNodeVariant> = Vec::new();
        for (key, option_block) in iter_keyed_blocks(options_block) {
            // A string key = a class/ascendancy variant; a numeric key = an
            // attribute-node three-way choice (the attribute_overrides channel).
            let BlockKey::Str(class) = key else {
                continue;
            };
            let stats = parse_string_array(option_block, "stats={");
            if stats.is_empty() {
                // An option with no stats of its own inherits the base mods via Lua's `__index` (purely cosmetic), skip.
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

/// A top-level key (a Lua table key) within a balanced block.
pub(crate) enum BlockKey {
    /// A numeric key like `[123]=`.
    Num(u32),
    /// A string key like `["Abyssal Lich"]=` or `Druid=`.
    Str(String),
}

/// Iterates the depth-1 `key={...}` subblocks within a balanced block
/// (including both `{}`), returning (key, subblock).
///
/// Supports three key shapes: `[123]={`, `["Name with space"]={`,
/// `BareWord={`; scalar fields (`id=64801,`/`name="..."`) aren't produced.
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
                // Skip string literals (including escapes), preventing a `{`/`}` inside the string from polluting depth tracking.
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
                // At the start of a line (with any tab indentation), try matching `key={`.
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

/// Parses a `key={` shaped key at `inner[at..]`, returning (key, the offset of `{`). Returns None for a non-subblock line.
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

/// Parses the string array at `marker` within a block (e.g. `stats={
/// [1]="...", [2]="..." }`), expanded in numeric-index order; returns empty when the subblock is absent.
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

/// A top-level scalar string field within a block (e.g. `name="Jagged Shards"`).
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

/// Parses a Lua double-quoted string literal starting with `"` (handling
/// `\"`, `\n`, `\\` escapes), returning (decoded value, total literal length). Returns None when it doesn't start with a string.
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
                // Advance by UTF-8 character (tree.lua is UTF-8 text).
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

    /// A minimal sample simulating tree.lua's top-level structure (a groups block plus a nodes block).
    fn sample_lua() -> &'static str {
        concat!(
            "tree={\n",
            "\tgroups={\n",
            "\t\t[1]={\n\t\t\tx=0,\n\t\t\ty=0\n\t\t}\n",
            "\t},\n",
            "\tnodes={\n",
            // isSwitchable + a Witch variant (with stats)
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
            // isSwitchable + an ascendancy-name key (no stats of its own, purely cosmetic -> not stored)
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
            // An attribute node (numeric-keyed options, not isSwitchable) -> skipped
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
