//! 四层语料收集器（蓝图 §5.1）。
//!
//! | 层 | 来源 | 提取 |
//! |----|------|------|
//! | C1 | `examples/demo-bd-test/builds/*/decoded.xml` 的 `<Item>` 文本块 | 行级（strip 注解） |
//! | C2 | `<data>/base/passive_tree.json` 每节点 `stats[]` | 行级 |
//! | SD | `<data>/generated/special_derived.json` 的 `pattern` | 整条 |
//! | CX | `--corpus-extra <file>` 每行 | 整行 |
//!
//! 去重后按字典序排，保证 `parsed_mods.json` byte-stable。每行记录其来源集合
//! （多来源命中取并集）供覆盖率报表分组。
//!
//! C3（vendor ModCache.lua）/C4（QueryMods.lua）需要 luajit，归 C-track（T6）的
//! 抽取，不在本 T7 骨架——`--corpus-extra` 提供注入点（落盘的 ModCache key dump
//! 可直接喂入）。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use pobr_core::item_text::strip_pob_annotations;
use pobr_gamedata::GameData;

/// 语料来源标记（位集，便于一行归多来源）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SourceSet {
    pub c1_build: bool,
    pub c2_tree: bool,
    pub sd_derived: bool,
    pub cx_extra: bool,
}

impl SourceSet {
    fn merge(&mut self, other: SourceSet) {
        self.c1_build |= other.c1_build;
        self.c2_tree |= other.c2_tree;
        self.sd_derived |= other.sd_derived;
        self.cx_extra |= other.cx_extra;
    }

    /// 主来源标签（用于报表分组；优先级 C2>C1>SD>CX，仅取一个代表）。
    pub fn primary_label(&self) -> &'static str {
        if self.c2_tree {
            "C2_tree"
        } else if self.c1_build {
            "C1_build"
        } else if self.sd_derived {
            "SD_derived"
        } else {
            "CX_extra"
        }
    }
}

/// 收集后的语料：字典序行 + 每行来源。
pub struct Corpus {
    /// 字典序去重后的 (text, sources)。
    pub lines: Vec<(String, SourceSet)>,
}

impl Corpus {
    pub fn source_summary(&self) -> String {
        let (mut c1, mut c2, mut sd, mut cx) = (0usize, 0usize, 0usize, 0usize);
        for (_, s) in &self.lines {
            if s.c1_build {
                c1 += 1;
            }
            if s.c2_tree {
                c2 += 1;
            }
            if s.sd_derived {
                sd += 1;
            }
            if s.cx_extra {
                cx += 1;
            }
        }
        format!("C1={c1} C2={c2} SD={sd} CX={cx}")
    }
}

/// 收集四层语料。`data_dir` 是版本目录（如 `data/4.5.0.3.4`）。
pub fn collect(data_dir: &Path, corpus_extra: Option<&Path>) -> Result<Corpus, String> {
    // text → 来源集合（BTreeMap 同时去重 + 字典序）。
    let mut map: BTreeMap<String, SourceSet> = BTreeMap::new();

    // C2：passive_tree 节点 stats（最可靠、最大批；从 data 内自举）。
    collect_passive_tree(data_dir, &mut map)?;

    // C1：build XML item 文本块（门禁主语料）。
    let examples = examples_dir(data_dir);
    if let Some(examples) = examples {
        collect_build_xml(&examples, &mut map)?;
    } else {
        eprintln!(
            "precompile-mods: 警告 —— 未定位到 examples/demo-bd-test/builds（跳过 C1 build XML 语料）"
        );
    }

    // SD：special_derived 展开（pattern 即整行锚定语料）。
    collect_special_derived(data_dir, &mut map)?;

    // CX：外挂语料。
    if let Some(extra) = corpus_extra {
        collect_extra(extra, &mut map)?;
    }

    let lines = map.into_iter().collect();
    Ok(Corpus { lines })
}

/// 把一条规范化后非空的文本行登记到 map（合并来源）。
fn add_line(map: &mut BTreeMap<String, SourceSet>, text: &str, src: SourceSet) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    map.entry(trimmed.to_string()).or_default().merge(src);
}

fn collect_passive_tree(
    data_dir: &Path,
    map: &mut BTreeMap<String, SourceSet>,
) -> Result<(), String> {
    let game = GameData::new(data_dir.to_path_buf());
    let nodes = game
        .passive_nodes()
        .map_err(|e| format!("加载 passive_tree.json 失败：{e}"))?;
    let src = SourceSet {
        c2_tree: true,
        ..Default::default()
    };
    for node in &nodes {
        for stat in &node.stats {
            add_line(map, stat, src);
        }
    }
    Ok(())
}

/// 从版本数据目录推断 `examples/demo-bd-test/builds`：data_dir 形如
/// `<repo>/data/<version>`，故上溯两级到 repo 根。
fn examples_dir(data_dir: &Path) -> Option<PathBuf> {
    let repo_root = data_dir.parent()?.parent()?;
    let builds = repo_root.join("examples/demo-bd-test/builds");
    builds.is_dir().then_some(builds)
}

fn collect_build_xml(
    builds_dir: &Path,
    map: &mut BTreeMap<String, SourceSet>,
) -> Result<(), String> {
    let src = SourceSet {
        c1_build: true,
        ..Default::default()
    };
    let mut build_dirs: Vec<PathBuf> = std::fs::read_dir(builds_dir)
        .map_err(|e| format!("读取 {} 失败：{e}", builds_dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    build_dirs.sort();

    for dir in build_dirs {
        let xml = dir.join("decoded.xml");
        if !xml.is_file() {
            continue;
        }
        let content = std::fs::read_to_string(&xml)
            .map_err(|e| format!("读取 {} 失败：{e}", xml.display()))?;
        for line in extract_item_mod_lines(&content) {
            add_line(map, &line, src);
        }
    }
    Ok(())
}

/// 从 build XML 抽取 `<Item>` 块内的 mod 文本行。
///
/// PoB2 item 文本块：`<Item id="N">` 与 `</Item>` 之间是物品原文，前若干行是
/// 元数据（Rarity/名称/Sockets/Implicits 计数/…），其后是 mod 文本行（可能带
/// `{enchant}{rune}{desecrated}` 等注解前缀）。本抽取走**保守启发式**：剥离注解
/// 前缀后，跳过明显的元数据/XML 标签行，其余作为候选 mod 文本喂解析器
/// （解析失败者计入覆盖率缺口，不影响正确性）。
///
/// 这是 T7 骨架的语料面，**不追求与 vendor item parser 逐行一致**——精确的
/// item-block 解析是 T6/T8 的范畴；本工具只需稳定、可去重的候选行集合。
fn extract_item_mod_lines(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_item = false;
    for raw in xml.lines() {
        let line = raw.trim();
        if line.starts_with("<Item ") || line == "<Item>" {
            in_item = true;
            continue;
        }
        if line.starts_with("</Item>") {
            in_item = false;
            continue;
        }
        if !in_item {
            continue;
        }
        // 物品块内的嵌套 XML 标签（<ModRange .../> 等）跳过。
        if line.starts_with('<') {
            continue;
        }
        let cleaned = strip_pob_annotations(line);
        let cleaned = cleaned.trim();
        if cleaned.is_empty() {
            continue;
        }
        if is_item_metadata(cleaned) {
            continue;
        }
        out.push(cleaned.to_string());
    }
    out
}

/// 物品块元数据行判定（保守白名单前缀 + 已知裸标记）。命中即不作为 mod 文本。
fn is_item_metadata(line: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "Rarity:",
        "Unique ID:",
        "Item Level:",
        "Quality:",
        "Sockets:",
        "Rune:",
        "Soul Core:",
        "LevelReq:",
        "Level Requirement:",
        "Requires Level",
        "Implicits:",
        "Energy Shield:",
        "Armour:",
        "Evasion Rating:",
        "Evasion:",
        "Ward:",
        "Physical Damage:",
        "Elemental Damage:",
        "Chaos Damage:",
        "Critical Strike Chance:",
        "Critical Hit Chance:",
        "Attacks per Second:",
        "Weapon Range:",
        "Radius:",
        "Limited to:",
        "Stack Size:",
        "Talisman Tier:",
        "Crafted:",
        "Prefix:",
        "Suffix:",
    ];
    const EXACT: &[&str] = &[
        "Corrupted",
        "Mirrored",
        "Split",
        "Unidentified",
        "Shaper Item",
        "Elder Item",
    ];
    if EXACT.iter().any(|m| line.eq_ignore_ascii_case(m)) {
        return true;
    }
    PREFIXES.iter().any(|p| line.starts_with(p))
}

/// special_derived.json 的 `entries[].pattern` 作为整行语料（这些是 special
/// 整行锚定命中的词条文本，如 keystone 名）。
fn collect_special_derived(
    data_dir: &Path,
    map: &mut BTreeMap<String, SourceSet>,
) -> Result<(), String> {
    let path = data_dir.join("generated/special_derived.json");
    if !path.is_file() {
        return Ok(());
    }
    let content =
        std::fs::read_to_string(&path).map_err(|e| format!("读取 {} 失败：{e}", path.display()))?;
    let doc: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("解析 special_derived.json 失败：{e}"))?;
    let src = SourceSet {
        sd_derived: true,
        ..Default::default()
    };
    if let Some(entries) = doc.get("entries").and_then(|v| v.as_array()) {
        for entry in entries {
            if let Some(pat) = entry.get("pattern").and_then(|v| v.as_str()) {
                add_line(map, pat, src);
            }
        }
    }
    Ok(())
}

fn collect_extra(extra: &Path, map: &mut BTreeMap<String, SourceSet>) -> Result<(), String> {
    let content = std::fs::read_to_string(extra)
        .map_err(|e| format!("读取 {} 失败：{e}", extra.display()))?;
    let src = SourceSet {
        cx_extra: true,
        ..Default::default()
    };
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        add_line(map, line, src);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_metadata_filtered() {
        assert!(is_item_metadata("Rarity: RARE"));
        assert!(is_item_metadata("Item Level: 82"));
        assert!(is_item_metadata("Corrupted"));
        assert!(is_item_metadata("Sockets: S"));
        assert!(!is_item_metadata("35% increased Movement Speed"));
        assert!(!is_item_metadata("+30 to maximum Runic Ward"));
    }

    #[test]
    fn extract_strips_annotations_and_metadata() {
        let xml = r#"
        <Item id="1">
            Rarity: RARE
            Sandsworn Sandals
            Item Level: 82
            Implicits: 2
            {enchant}{rune}+30 to maximum Runic Ward
            {desecrated}35% increased Movement Speed
            Corrupted
            <ModRange range="0.5" id="1"/>
        </Item>
        "#;
        let lines = extract_item_mod_lines(xml);
        assert!(lines.contains(&"+30 to maximum Runic Ward".to_string()));
        assert!(lines.contains(&"35% increased Movement Speed".to_string()));
        // 名称行（Sandsworn Sandals）不在白名单 → 会作为候选行（解析为 Unsupported，
        // 计入缺口，不影响正确性）；元数据被滤掉。
        assert!(!lines.iter().any(|l| l.starts_with("Rarity:")));
        assert!(!lines.iter().any(|l| l == "Corrupted"));
        assert!(!lines.iter().any(|l| l.starts_with("Item Level:")));
    }

    #[test]
    fn source_set_merge_union() {
        let mut a = SourceSet {
            c1_build: true,
            ..Default::default()
        };
        a.merge(SourceSet {
            c2_tree: true,
            ..Default::default()
        });
        assert!(a.c1_build && a.c2_tree);
        assert_eq!(a.primary_label(), "C2_tree");
    }
}
