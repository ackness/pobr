//! Passive tree modifier source ingest.
//!
//! Parses the English modifier text of a set of "allocated passive nodes"
//! into `Modifier`s attributed with [`SourceKind::PassiveNode`] (or
//! [`SourceKind::AscendancyNode`] for ascendancy nodes), keeping each node's
//! stable `NodeId` and raw modifier text (`raw_text`) so the final output can
//! be traced source-level back to a specific passive node (PoBR's core
//! value-add over PoB).
//!
//! Mirrors the pattern in [`crate::item`]: source (node) → parse modifiers →
//! attributed modifiers + unsupported collection. A passive node is just a
//! modifier container with low mechanical risk — the numbers live in the
//! modifier text, so no extra formula is needed here.
//!
//! `PassiveTreeSpec` currently only holds `allocated_nodes: Vec<NodeId>` and
//! doesn't carry each node's modifier text yet, so this entry point takes the
//! standalone [`AllocatedNode`] as input (stable node ID + modifier text +
//! ascendancy flag), mirroring `Item` as the input to `ingest_item`. Once
//! `PassiveTreeSpec` can carry node modifiers and metadata, callers can
//! assemble from that instead.

use pobr_data::prelude::*;

use crate::Modifier;
use crate::mod_parser::{ParseError, ParseStatus};

/// An allocated passive node: stable `NodeId` + modifier text + ascendancy flag.
#[derive(Debug, Clone, Default)]
pub struct AllocatedNode {
    pub node_id: NodeId,
    /// Whether this is an Ascendancy node, which determines the attribution [`SourceKind`].
    pub ascendancy: bool,
    /// The node's English PoB-compatible modifier text (one modifier per line).
    pub modifier_texts: Vec<String>,
}

/// Result of ingesting a set of passive nodes: parsed modifiers + raw text that couldn't be parsed.
///
/// Mirrors [`crate::item::ItemIngest`].
#[derive(Debug, Clone, Default)]
pub struct PassiveIngest {
    pub modifiers: Vec<Modifier>,
    pub unsupported: Vec<String>,
}

/// Parses the modifier text of a set of allocated passive nodes into node-attributed modifiers.
///
/// Parse failures (structural errors) propagate as [`ParseError`]; unrecognized
/// modifiers (e.g. `mirrored`) don't error, they're collected into
/// [`PassiveIngest::unsupported`] instead, matching `CalculationSession`'s
/// semantics.
///
/// Attribution convention: `SourceId.kind` = [`SourceKind::PassiveNode`] /
/// [`SourceKind::AscendancyNode`], `SourceId.id` = `node.<NodeId>`, `raw_text`
/// keeps the original modifier line; `stat_id` / `mod_type` are filled back
/// from the modifier by [`Modifier::with_origin`]. Modifier parsing goes
/// through `ctx`.
pub fn ingest_passive_nodes_with_ctx(
    nodes: &[AllocatedNode],
    ctx: crate::mod_parser::ParseCtx<'_>,
) -> Result<PassiveIngest, ParseError> {
    let mut ingest = PassiveIngest::default();
    for node in nodes {
        let kind = if node.ascendancy {
            SourceKind::AscendancyNode
        } else {
            SourceKind::PassiveNode
        };
        let source_id = SourceId::new(kind, format!("node.{}", node.node_id.0));

        for text in &node.modifier_texts {
            let outcome = ctx.parse(text)?;
            match outcome.status {
                ParseStatus::Parsed => {
                    for modifier in outcome.mods {
                        let origin =
                            ModifierSource::new(source_id.clone()).with_raw_text(text.clone());
                        ingest.modifiers.push(modifier.with_origin(origin));
                    }
                }
                ParseStatus::Unsupported => {
                    if let Some(unparsed) = outcome.unparsed {
                        ingest.unsupported.push(unparsed);
                    }
                }
            }
        }
    }

    Ok(ingest)
}
