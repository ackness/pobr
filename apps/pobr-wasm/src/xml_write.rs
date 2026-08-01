//! Edit state -> PoB2 Build XML (the write-out side of `encode_build_json`).
//!
//! The element/attribute set corresponds one-to-one with the **read side**
//! of `pobr-build::xml_build` (the `Build` header / `Tree>Spec`
//! (nodes / AttributeOverride / Sockets) / `Skills>SkillSet>Skill>Gem` /
//! `Items>Item + ItemSet>Slot` / `Config>Input` / `Notes`), guaranteeing
//! `encode -> decode -> calculate` matches calculating directly (pinned by a
//! contract test). Gem writes both `gemId` (PoB2's import key) and
//! `skillId` (PoBR's key).

use std::collections::BTreeMap;
use std::fmt::Write as _;

/// A noise-reducing wrapper around `write!` / `writeln!` into a `String`.
///
/// The sink is always a `String`, and `impl fmt::Write for String`'s `Err`
/// branch is unreachable (`push_str` never fails), so the `Result` here has
/// no failure state worth handling. The macro swallows it inline, sparing
/// the body about forty `.unwrap()` calls — those unwraps would never panic,
/// but "N unwraps in the wasm crate" is a metric that would need
/// re-investigating on every audit otherwise.
macro_rules! w {
    ($w:expr, $($arg:tt)*) => { let _ = write!($w, $($arg)*); };
}
macro_rules! wln {
    ($w:expr, $($arg:tt)*) => { let _ = writeln!($w, $($arg)*); };
}

/// A socket group ready to write out (gem_id already reverse-looked-up).
pub(crate) struct XmlSkillGroup {
    pub slot: Option<String>,
    pub enabled: bool,
    /// The source marker for a group granted by equipment (written back as
    /// `<Skill source>`, so round-tripping can tell it apart).
    pub source: Option<String>,
    /// `(gem_id, skill_id, level, quality)`; `gem_id` is an empty string
    /// when the reverse lookup fails (the attribute is then omitted).
    pub gems: Vec<(String, String, u32, u32)>,
}

/// The write-out input (all sourced from the calculation request — the web
/// side always sends a full overwrite).
pub(crate) struct XmlInput<'a> {
    pub level: u32,
    pub class_name: &'a str,
    pub ascendancy_name: &'a str,
    /// PoB2's passive-tree version tag (e.g. `0_5`).
    pub tree_version: &'a str,
    pub allocated_nodes: &'a [u32],
    /// Attribute-choice small nodes (node skill id -> `"str"|"dex"|"int"`).
    pub attribute_choices: &'a BTreeMap<u32, String>,
    /// `(PoB slot name, raw text)`, e.g. `("Ring 1", "Rarity: ...")`.
    pub items: Vec<(String, String)>,
    /// Active flasks/charms as `(slot name, raw text)` (`Flask 1/2`, `Charm 1..3`).
    pub flasks: Vec<(String, String)>,
    /// Tree-socketed jewels as `(socket node id, raw text)`.
    pub jewels: Vec<(u32, String)>,
    pub socket_groups: Vec<XmlSkillGroup>,
    /// 0-based (written as 1-based in the XML).
    pub main_socket_group: Option<usize>,
    pub config_inputs: &'a BTreeMap<String, serde_json::Value>,
    pub notes: Option<&'a str>,
}

/// Escapes an XML attribute value.
fn esc_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;")
}

/// Escapes an XML text node.
fn esc_text(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;")
}

fn csv(nodes: impl Iterator<Item = u32>) -> String {
    nodes.map(|n| n.to_string()).collect::<Vec<_>>().join(",")
}

/// Generates the PoB2 Build XML.
pub(crate) fn write_build_xml(input: &XmlInput<'_>) -> String {
    let mut xml = String::new();
    let w = &mut xml;

    wln!(w, r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    wln!(w, "<PathOfBuilding2>");

    // The Build header (mainSocketGroup is 1-based).
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

    // Items: equipment, flasks/charms, and tree jewels are numbered
    // together as ids (1-based, document order).
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

    // Tree (a single Spec; AttributeOverride and Sockets are nested inside it).
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

    // Skills.
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

    // Items (text blocks plus the active ItemSet slot mapping).
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

    // Config.
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

    // Notes.
    if let Some(notes) = input.notes
        && !notes.is_empty()
    {
        wln!(w, "  <Notes>{}</Notes>", esc_text(notes));
    }

    wln!(w, "</PathOfBuilding2>");
    xml
}
