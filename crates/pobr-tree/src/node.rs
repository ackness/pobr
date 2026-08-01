//! Modifier collection for allocated passive nodes, plus JewelSocket /
//! Mastery gating.
//!
//! Node data uses the REAL authoritative type [`PassiveNodeDef`] (provided by
//! `pobr_data::catalog`, indexed by numeric `skill` id), and this module
//! outputs modifier text plus [`SourceKind::PassiveNode`] attribution, handed
//! off to `pobr-core::passive::ingest_passive_nodes` to parse into sourced
//! `Modifier`s.

use std::collections::HashMap;

use pobr_data::prelude::*;

/// The modifier text one allocated passive node contributes, plus its source
/// attribution.
///
/// `modifier_texts` is the node's `stats` (unparsed, handed off to
/// `pobr-core::mod_parser`). `source_id` is always [`SourceKind::PassiveNode`],
/// with the id as the string form of the node's `skill` id (matching the
/// plain-numeric-id convention used elsewhere in `pobr-core::passive`, as
/// opposed to the `node.<id>` form).
#[derive(Debug, Clone, PartialEq)]
pub struct AllocatedNodeMods {
    /// The node's stable id (i.e. catalog `PassiveNodeDef::skill`).
    pub node_id: NodeId,
    /// The node's unparsed, PoB-compatible English modifier text, one line per entry.
    pub modifier_texts: Vec<String>,
    /// Source attribution (`SourceKind::PassiveNode`, id = the node's skill id as a string).
    pub source_id: SourceId,
}

/// The class context used to resolve isSwitchable variant selection (PoB's
/// `curClassName` / `curAscendClassName`).
///
/// Mirrors PoB2 `PassiveSpec.lua:1251-1256`: when a node has variants, it's
/// matched first by class name against `options[curClassName]`, then by
/// ascendancy name against `options[curAscendClassName]`; a match
/// **wholesale replaces** the base `stats` with the variant's stats
/// (`ReplaceNode` semantics, not a merge).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClassContext<'a> {
    /// Class name (canonical English, e.g. `Witch`); an empty string means none.
    pub class_name: &'a str,
    /// Ascendancy name (canonical English, e.g. `Abyssal Lich`); an empty string means none.
    pub ascendancy_name: &'a str,
}

impl ClassContext<'_> {
    /// Picks the applicable variant stats from a node's variants using PoB's
    /// priority (class name over ascendancy name); returns `None` on no match.
    fn select<'n>(&self, node: &'n PassiveNodeDef) -> Option<&'n [String]> {
        let by_key = |key: &str| {
            (!key.is_empty())
                .then(|| node.variants.iter().find(|v| v.class == key))
                .flatten()
        };
        by_key(self.class_name)
            .or_else(|| by_key(self.ascendancy_name))
            .map(|v| v.stats.as_slice())
    }
}

/// Collects modifier text from the allocated node set, with no class context
/// (isSwitchable nodes always use their base stats).
///
/// See [`collect_allocated_mods_for_class`] for the rules.
pub fn collect_allocated_mods(
    spec: &PassiveTreeSpec,
    nodes: &HashMap<u32, PassiveNodeDef>,
) -> Vec<AllocatedNodeMods> {
    collect_allocated_mods_for_class(spec, nodes, ClassContext::default())
}

/// Collects modifier text from the allocated node set.
///
/// Rules:
/// - Only nodes in `spec.allocated_nodes` that **actually exist in `nodes`**
///   are processed (unknown ids are skipped).
/// - **A JewelSocket node's own `stats` are excluded** (gated; handled
///   separately once a jewel is socketed).
/// - **Mastery nodes**: if `spec.mastery_effects` has a selection for the
///   node, only the player's chosen effect text is injected; with no
///   selection recorded, the node is skipped entirely (matches historical
///   behaviour).
/// - **isSwitchable variants** (`node.variants`): on a match by `class`
///   (class name first, then ascendancy name), the variant's stats wholesale
///   replace the base `stats` (PoB `ReplaceNode` semantics).
/// - Nodes with empty `stats` are skipped (nothing to contribute).
///
/// Output order follows `spec.allocated_nodes` (deterministic).
///
/// `nodes` is keyed by node `skill` id (`u32`), the same value backing [`NodeId`].
pub fn collect_allocated_mods_for_class(
    spec: &PassiveTreeSpec,
    nodes: &HashMap<u32, PassiveNodeDef>,
    class: ClassContext<'_>,
) -> Vec<AllocatedNodeMods> {
    spec.allocated_nodes
        .iter()
        .filter_map(|node_id| {
            let node = nodes.get(&node_id.0)?;

            // JewelSocket nodes are gated entirely; handled once a jewel is socketed.
            if node.kind == PassiveNodeKind::JewelSocket {
                return None;
            }

            // Mastery nodes: inject only the player's chosen effect; no
            // selection means the whole node is gated out.
            if node.kind == PassiveNodeKind::Mastery {
                let selection = spec.mastery_effects.get(node_id)?;
                return Some(AllocatedNodeMods {
                    node_id: *node_id,
                    modifier_texts: split_lines(std::slice::from_ref(&selection.effect_text)),
                    source_id: SourceId::new(SourceKind::PassiveNode, node_id.0.to_string()),
                });
            }

            // isSwitchable variants: a class/ascendancy match wholesale
            // replaces the base stats.
            let stats = class.select(node).unwrap_or(&node.stats);
            if stats.is_empty() {
                return None;
            }
            let mut modifier_texts = split_lines(stats);
            // Attribute-choice notables (`+5 to any Attribute`) get rewritten
            // to the player's chosen attribute (PoB2 `SwitchAttributeNode`
            // semantics); with no choice recorded, the text is left as-is —
            // mod_parser doesn't recognize `any attribute` (PoB2's
            // ModParser maps it to nothing either), so the line naturally
            // ends up Unsupported and contributes nothing.
            if let Some(choice) = spec.attribute_overrides.get(node_id) {
                for text in &mut modifier_texts {
                    if let Some(rewritten) = rewrite_attribute_choice(text, *choice) {
                        *text = rewritten;
                    }
                }
            }
            if modifier_texts.is_empty() {
                return None;
            }
            Some(AllocatedNodeMods {
                node_id: *node_id,
                modifier_texts,
                source_id: SourceId::new(SourceKind::PassiveNode, node_id.0.to_string()),
            })
        })
        .collect()
}

/// Splits a node's/mastery's stat lines on embedded newlines into separate
/// entries (trimmed, blanks dropped).
///
/// Some PoE2 passive nodes (notably keystones) pack multiple lines into a
/// single `stats` element via `\n`, e.g.
/// `"Maximum Life is 1\nImmune to Chaos Damage and Bleeding"`. Feeding the
/// whole thing to `parse_mod` always fails and gets silently dropped;
/// splitting into individual lines lets each parse independently. This is a
/// generic fix shared by every multi-line node, not specific to any one node
/// or class.
fn split_lines(stats: &[String]) -> Vec<String> {
    stats
        .iter()
        .flat_map(|s| s.lines())
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// Rewrites an attribute-choice line `+N to any [Attributes|Attribute]` to
/// `+N to <Strength|Dexterity|Intelligence>` for the chosen attribute.
/// Returns `None` for any line that isn't an attribute-choice line.
fn rewrite_attribute_choice(text: &str, choice: AttributeChoice) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    let idx = lower.find(" to any ")?;
    if !lower[idx..].contains("attribute") {
        return None;
    }
    let attr = match choice {
        AttributeChoice::Strength => "Strength",
        AttributeChoice::Dexterity => "Dexterity",
        AttributeChoice::Intelligence => "Intelligence",
    };
    Some(format!("{} to {attr}", &text[..idx]))
}
