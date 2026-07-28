//! 编辑态 → PoB2 Build XML（`encode_build_json` 的写出端）。
//!
//! 元素/属性集合与 `pobr-build::xml_build` 的**读取面**一一对应（`Build` 头部 /
//! `Tree>Spec`（nodes / AttributeOverride / Sockets）/ `Skills>SkillSet>Skill>Gem` /
//! `Items>Item + ItemSet>Slot` / `Config>Input` / `Notes`），保证
//! `encode → decode → calculate` 与直接 calculate 一致（契约测试钉住）。
//! Gem 同时写 `gemId`（PoB2 导入主键）与 `skillId`（PoBR 主键）。

use std::collections::BTreeMap;
use std::fmt::Write as _;

/// `write!` / `writeln!` 到 `String` 的省噪包装。
///
/// sink 恒为 `String`，而 `impl fmt::Write for String` 的 `Err` 分支不可达
/// （`push_str` 不会失败），所以这里的 `Result` 没有可处理的失败态。用宏就地吞
/// 掉，免去正文四十来处 `.unwrap()` —— 那些 unwrap 不会 panic，但会让「wasm 里
/// 有 N 个 unwrap」这个指标每次审计都要重新排查一遍。
macro_rules! w {
    ($w:expr, $($arg:tt)*) => { let _ = write!($w, $($arg)*); };
}
macro_rules! wln {
    ($w:expr, $($arg:tt)*) => { let _ = writeln!($w, $($arg)*); };
}

/// 待写出的技能组（已做 gem_id 反查）。
pub(crate) struct XmlSkillGroup {
    pub slot: Option<String>,
    pub enabled: bool,
    /// 装备授予技能组的来源标记（写回 `<Skill source>`，保证往返判重）。
    pub source: Option<String>,
    /// `(gem_id, skill_id, level, quality)`；gem_id 反查不到时为空串（省略属性）。
    pub gems: Vec<(String, String, u32, u32)>,
}

/// 写出输入（全部来自计算请求——web 端始终发全量覆盖）。
pub(crate) struct XmlInput<'a> {
    pub level: u32,
    pub class_name: &'a str,
    pub ascendancy_name: &'a str,
    /// PoB2 树版本标注（如 `0_5`）。
    pub tree_version: &'a str,
    pub allocated_nodes: &'a [u32],
    /// 属性小点三选一（node skill id → `"str"|"dex"|"int"`）。
    pub attribute_choices: &'a BTreeMap<u32, String>,
    /// `(PoB 槽名, 原始文本)`，如 `("Ring 1", "Rarity: …")`。
    pub items: Vec<(String, String)>,
    /// 激活态药剂/护符 `(槽名, 原始文本)`（`Flask 1/2`、`Charm 1..3`）。
    pub flasks: Vec<(String, String)>,
    /// 树插槽珠宝 `(socket node id, 原始文本)`。
    pub jewels: Vec<(u32, String)>,
    pub socket_groups: Vec<XmlSkillGroup>,
    /// 0-based（XML 里写 1-based）。
    pub main_socket_group: Option<usize>,
    pub config_inputs: &'a BTreeMap<String, serde_json::Value>,
    pub notes: Option<&'a str>,
}

/// XML 属性值转义。
fn esc_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;")
}

/// XML 文本节点转义。
fn esc_text(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;")
}

fn csv(nodes: impl Iterator<Item = u32>) -> String {
    nodes.map(|n| n.to_string()).collect::<Vec<_>>().join(",")
}

/// 生成 PoB2 Build XML。
pub(crate) fn write_build_xml(input: &XmlInput<'_>) -> String {
    let mut xml = String::new();
    let w = &mut xml;

    wln!(w, r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    wln!(w, "<PathOfBuilding2>");

    // Build 头部（mainSocketGroup 1-based）。
    w!(
        w,
        r#"  <Build level="{}" className="{}" targetVersion="0_1""#,
        input.level,
        esc_attr(input.class_name),
    );
    if !input.ascendancy_name.is_empty() {
        w!(
            w,
            r#" ascendClassName="{}""#,
            esc_attr(input.ascendancy_name)
        );
    }
    if let Some(main) = input.main_socket_group {
        w!(w, r#" mainSocketGroup="{}""#, main + 1);
    }
    wln!(w, "/>");

    // 物品：装备 + 药剂/护符 + 树珠宝统一编 id（1-based，文档序）。
    let mut item_blocks: Vec<&str> = Vec::new();
    let mut slot_lines: Vec<String> = Vec::new();
    for (slot, text) in &input.items {
        item_blocks.push(text);
        slot_lines.push(format!(
            r#"      <Slot name="{}" itemId="{}"/>"#,
            esc_attr(slot),
            item_blocks.len()
        ));
    }
    for (slot, text) in &input.flasks {
        item_blocks.push(text);
        slot_lines.push(format!(
            r#"      <Slot name="{}" itemId="{}" active="true"/>"#,
            esc_attr(slot),
            item_blocks.len()
        ));
    }
    let mut socket_lines: Vec<String> = Vec::new();
    for (node, text) in &input.jewels {
        item_blocks.push(text);
        socket_lines.push(format!(
            r#"        <Socket nodeId="{}" itemId="{}"/>"#,
            node,
            item_blocks.len()
        ));
    }

    // Tree（单 Spec；AttributeOverride 与 Sockets 内嵌）。
    wln!(w, r#"  <Tree activeSpec="1">"#);
    wln!(
        w,
        r#"    <Spec treeVersion="{}" nodes="{}">"#,
        esc_attr(input.tree_version),
        csv(input.allocated_nodes.iter().copied()),
    );
    if !input.attribute_choices.is_empty() {
        let pick = |want: &str| {
            csv(input
                .attribute_choices
                .iter()
                .filter(|(_, c)| c.as_str() == want)
                .map(|(&n, _)| n))
        };
        wln!(
            w,
            r#"      <AttributeOverride strNodes="{}" dexNodes="{}" intNodes="{}"/>"#,
            pick("str"),
            pick("dex"),
            pick("int"),
        );
    }
    if !socket_lines.is_empty() {
        wln!(w, "      <Sockets>");
        for line in &socket_lines {
            wln!(w, "{line}");
        }
        wln!(w, "      </Sockets>");
    }
    wln!(w, "    </Spec>");
    wln!(w, "  </Tree>");

    // Skills。
    wln!(w, r#"  <Skills activeSkillSet="1">"#);
    wln!(w, r#"    <SkillSet id="1">"#);
    for group in &input.socket_groups {
        w!(w, r#"      <Skill enabled="{}""#, group.enabled);
        if let Some(slot) = &group.slot {
            w!(w, r#" slot="{}""#, esc_attr(slot));
        }
        if let Some(source) = &group.source {
            w!(w, r#" source="{}""#, esc_attr(source));
        }
        wln!(w, ">");
        for (gem_id, skill_id, level, quality) in &group.gems {
            w!(w, r#"        <Gem skillId="{}""#, esc_attr(skill_id));
            if !gem_id.is_empty() {
                w!(w, r#" gemId="{}""#, esc_attr(gem_id));
            }
            wln!(
                w,
                r#" level="{level}" quality="{quality}" enabled="true"/>"#
            );
        }
        wln!(w, "      </Skill>");
    }
    wln!(w, "    </SkillSet>");
    wln!(w, "  </Skills>");

    // Items（文本块 + 激活 ItemSet 槽位映射）。
    wln!(w, r#"  <Items activeItemSet="1">"#);
    for (idx, text) in item_blocks.iter().enumerate() {
        wln!(w, r#"    <Item id="{}">"#, idx + 1);
        wln!(w, "{}", esc_text(text.trim_end()));
        wln!(w, "    </Item>");
    }
    wln!(w, r#"    <ItemSet id="1" useSecondWeaponSet="false">"#);
    for line in &slot_lines {
        wln!(w, "{line}");
    }
    wln!(w, "    </ItemSet>");
    wln!(w, "  </Items>");

    // Config。
    if !input.config_inputs.is_empty() {
        wln!(w, "  <Config>");
        for (name, value) in input.config_inputs {
            let attr = match value {
                serde_json::Value::Bool(b) => format!(r#"boolean="{b}""#),
                serde_json::Value::Number(n) => format!(r#"number="{n}""#),
                other => format!(
                    r#"string="{}""#,
                    esc_attr(other.as_str().unwrap_or_default())
                ),
            };
            wln!(w, r#"    <Input name="{}" {attr}/>"#, esc_attr(name));
        }
        wln!(w, "  </Config>");
    }

    // Notes。
    if let Some(notes) = input.notes
        && !notes.is_empty()
    {
        wln!(w, "  <Notes>{}</Notes>", esc_text(notes));
    }

    wln!(w, "</PathOfBuilding2>");
    xml
}
