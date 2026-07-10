//! 编辑态 → PoB2 Build XML（`encode_build_json` 的写出端）。
//!
//! 元素/属性集合与 `pobr-build::xml_build` 的**读取面**一一对应（`Build` 头部 /
//! `Tree>Spec`（nodes / AttributeOverride / Sockets）/ `Skills>SkillSet>Skill>Gem` /
//! `Items>Item + ItemSet>Slot` / `Config>Input` / `Notes`），保证
//! `encode → decode → calculate` 与直接 calculate 一致（契约测试钉住）。
//! Gem 同时写 `gemId`（PoB2 导入主键）与 `skillId`（PoBR 主键）。

use std::collections::BTreeMap;
use std::fmt::Write as _;

/// 待写出的技能组（已做 gem_id 反查）。
pub(crate) struct XmlSkillGroup {
    pub slot: Option<String>,
    pub enabled: bool,
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

    writeln!(w, r#"<?xml version="1.0" encoding="UTF-8"?>"#).unwrap();
    writeln!(w, "<PathOfBuilding2>").unwrap();

    // Build 头部（mainSocketGroup 1-based）。
    write!(
        w,
        r#"  <Build level="{}" className="{}" targetVersion="0_1""#,
        input.level,
        esc_attr(input.class_name),
    )
    .unwrap();
    if !input.ascendancy_name.is_empty() {
        write!(
            w,
            r#" ascendClassName="{}""#,
            esc_attr(input.ascendancy_name)
        )
        .unwrap();
    }
    if let Some(main) = input.main_socket_group {
        write!(w, r#" mainSocketGroup="{}""#, main + 1).unwrap();
    }
    writeln!(w, "/>").unwrap();

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
    writeln!(w, r#"  <Tree activeSpec="1">"#).unwrap();
    writeln!(
        w,
        r#"    <Spec treeVersion="{}" nodes="{}">"#,
        esc_attr(input.tree_version),
        csv(input.allocated_nodes.iter().copied()),
    )
    .unwrap();
    if !input.attribute_choices.is_empty() {
        let pick = |want: &str| {
            csv(input
                .attribute_choices
                .iter()
                .filter(|(_, c)| c.as_str() == want)
                .map(|(&n, _)| n))
        };
        writeln!(
            w,
            r#"      <AttributeOverride strNodes="{}" dexNodes="{}" intNodes="{}"/>"#,
            pick("str"),
            pick("dex"),
            pick("int"),
        )
        .unwrap();
    }
    if !socket_lines.is_empty() {
        writeln!(w, "      <Sockets>").unwrap();
        for line in &socket_lines {
            writeln!(w, "{line}").unwrap();
        }
        writeln!(w, "      </Sockets>").unwrap();
    }
    writeln!(w, "    </Spec>").unwrap();
    writeln!(w, "  </Tree>").unwrap();

    // Skills。
    writeln!(w, r#"  <Skills activeSkillSet="1">"#).unwrap();
    writeln!(w, r#"    <SkillSet id="1">"#).unwrap();
    for group in &input.socket_groups {
        write!(w, r#"      <Skill enabled="{}""#, group.enabled).unwrap();
        if let Some(slot) = &group.slot {
            write!(w, r#" slot="{}""#, esc_attr(slot)).unwrap();
        }
        writeln!(w, ">").unwrap();
        for (gem_id, skill_id, level, quality) in &group.gems {
            write!(w, r#"        <Gem skillId="{}""#, esc_attr(skill_id)).unwrap();
            if !gem_id.is_empty() {
                write!(w, r#" gemId="{}""#, esc_attr(gem_id)).unwrap();
            }
            writeln!(
                w,
                r#" level="{level}" quality="{quality}" enabled="true"/>"#
            )
            .unwrap();
        }
        writeln!(w, "      </Skill>").unwrap();
    }
    writeln!(w, "    </SkillSet>").unwrap();
    writeln!(w, "  </Skills>").unwrap();

    // Items（文本块 + 激活 ItemSet 槽位映射）。
    writeln!(w, r#"  <Items activeItemSet="1">"#).unwrap();
    for (idx, text) in item_blocks.iter().enumerate() {
        writeln!(w, r#"    <Item id="{}">"#, idx + 1).unwrap();
        writeln!(w, "{}", esc_text(text.trim_end())).unwrap();
        writeln!(w, "    </Item>").unwrap();
    }
    writeln!(w, r#"    <ItemSet id="1" useSecondWeaponSet="false">"#).unwrap();
    for line in &slot_lines {
        writeln!(w, "{line}").unwrap();
    }
    writeln!(w, "    </ItemSet>").unwrap();
    writeln!(w, "  </Items>").unwrap();

    // Config。
    if !input.config_inputs.is_empty() {
        writeln!(w, "  <Config>").unwrap();
        for (name, value) in input.config_inputs {
            let attr = match value {
                serde_json::Value::Bool(b) => format!(r#"boolean="{b}""#),
                serde_json::Value::Number(n) => format!(r#"number="{n}""#),
                other => format!(
                    r#"string="{}""#,
                    esc_attr(other.as_str().unwrap_or_default())
                ),
            };
            writeln!(w, r#"    <Input name="{}" {attr}/>"#, esc_attr(name)).unwrap();
        }
        writeln!(w, "  </Config>").unwrap();
    }

    // Notes。
    if let Some(notes) = input.notes
        && !notes.is_empty()
    {
        writeln!(w, "  <Notes>{}</Notes>", esc_text(notes)).unwrap();
    }

    writeln!(w, "</PathOfBuilding2>").unwrap();
    xml
}
