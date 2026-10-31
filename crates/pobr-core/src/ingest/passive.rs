//! Passive source ingestion with an exclusive native or text channel per node.
//!
//! Complete supported native packets bypass parsing. Rejected packets retain a
//! diagnostic and fall back to the whole original text; absent packets keep legacy
//! parsing. Both channels retain node attribution and raw display text.

use pobr_data::prelude::*;

use crate::Modifier;
use crate::mod_parser::ParseError;
use crate::rules::passive_stats::{self, NativePassiveRejection};
use pobr_data::native_stat::{NativePassiveContext, NativePassiveStats};

/// The target node kind for a radius jewel's `also grant` line (the granted object is determined by its prefix).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantTargetKind {
    Notable,
    /// `Small Passive Skills` = a normal (non-notable/keystone/socket/mastery) node.
    Small,
}

/// Parses a `<Kind> Passive Skills in Radius also grant <mod>` line → (target kind, granted mod text).
///
/// Only recognizes the `Notable` / `Small` prefixes; any other prefix (e.g. keystone
/// grants, no samples seen so far) returns None.
pub fn parse_grant_line(line: &str) -> Option<(GrantTargetKind, String)> {
    const MARKER: &str = "Passive Skills in Radius also grant";
    let idx = line.find(MARKER)?;
    let prefix = line[..idx].trim();
    let kind = match prefix.to_ascii_lowercase().as_str() {
        "notable" => GrantTargetKind::Notable,
        "small" => GrantTargetKind::Small,
        _ => return None,
    };
    let granted = line[idx + MARKER.len()..].trim();
    if granted.is_empty() {
        None
    } else {
        Some((kind, granted.to_string()))
    }
}

/// An allocated passive node: stable `NodeId` + modifier text + ascendancy flag.
#[derive(Debug, Clone, Default)]
pub struct AllocatedNode {
    pub node_id: NodeId,
    /// Whether this is an Ascendancy node, which determines the attribution [`SourceKind`].
    pub ascendancy: bool,
    /// The node's English PoB-compatible modifier text (one modifier per line).
    pub modifier_texts: Vec<String>,
    /// Complete optional native channel, after allocation/context checks.
    pub native: Option<NativePassiveInput>,
}

/// A packet plus any invalidation discovered while resolving the allocated node.
#[derive(Debug, Clone)]
pub struct NativePassiveInput {
    pub packet: NativePassiveStats,
    pub ineligible: Option<NativePassiveRejection>,
}

/// A rejected packet and the entire text handed back to the legacy path.
/// Text parsing/application remains visible in the ordinary line ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativePassiveDiagnostic {
    pub source_id: SourceId,
    pub packet: NativePassiveStats,
    pub fallback_texts: Vec<String>,
    pub reason: NativePassiveRejection,
}

/// Selects the native channel before any parser filtering. `None` means legacy.
/// A rejection always falls back for the whole node, never a subset of its stats.
pub fn select_native_passive(
    node: &AllocatedNode,
) -> Option<Result<Modifier, NativePassiveDiagnostic>> {
    let input = node.native.as_ref()?;
    let packet = &input.packet;
    let map = || {
        if let Some(reason) = input.ineligible {
            return Err(reason);
        }
        if packet.node_id != node.node_id.0 || packet.modifier_texts != node.modifier_texts {
            return Err(NativePassiveRejection::BindingMismatch);
        }
        if node.ascendancy || packet.context != NativePassiveContext::UnconditionalPlayer {
            return Err(NativePassiveRejection::UnsupportedContext);
        }
        let [stat] = packet.stats.as_slice() else {
            return Err(NativePassiveRejection::IncompletePacket);
        };
        if packet.modifier_texts.len() != 1 || packet.modifier_texts[0].lines().count() != 1 {
            return Err(NativePassiveRejection::IncompletePacket);
        }
        if stat.provenance.passive_id != packet.node_slug {
            return Err(NativePassiveRejection::BindingMismatch);
        }
        let modifier = passive_stats::map_stat(stat)?;
        let mut origin = ModifierSource::new(node_source(node.node_id.0, node.ascendancy))
            .with_raw_text(node.modifier_texts[0].clone());
        origin.native_stat = Some(stat.clone());
        Ok(modifier.with_origin(origin))
    };
    Some(map().map_err(|reason| NativePassiveDiagnostic {
        source_id: node_source(node.node_id.0, node.ascendancy),
        packet: packet.clone(),
        fallback_texts: node.modifier_texts.clone(),
        reason,
    }))
}

/// Result of ingesting a set of passive nodes: parsed modifiers + raw text that couldn't be parsed.
///
/// Mirrors [`crate::item::ItemIngest`].
#[derive(Debug, Clone, Default)]
pub struct PassiveIngest {
    pub modifiers: Vec<Modifier>,
    pub unsupported: Vec<String>,
    pub native_diagnostics: Vec<NativePassiveDiagnostic>,
}

/// Parses the modifier text of a set of allocated passive nodes into node-attributed modifiers.
///
/// Parse failures (structural errors) propagate as [`ParseError`]; unrecognized
/// modifiers (e.g. `mirrored`) don't error, they're collected into
/// [`PassiveIngest::unsupported`] instead, matching `CalculationSession`'s
/// semantics.
///
/// The source the lines of node `node` are stored under: `node.<id>`, of
/// kind [`SourceKind::AscendancyNode`] for an ascendancy node.
pub fn node_source(node: u32, ascendancy: bool) -> SourceId {
    let kind = if ascendancy {
        SourceKind::AscendancyNode
    } else {
        SourceKind::PassiveNode
    };
    SourceId::new(kind, format!("node.{node}"))
}

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
        match select_native_passive(node) {
            Some(Ok(modifier)) => {
                ingest.modifiers.push(modifier);
                continue;
            }
            Some(Err(diagnostic)) => ingest.native_diagnostics.push(diagnostic),
            None => {}
        }
        let source_id = node_source(node.node_id.0, node.ascendancy);

        for text in &node.modifier_texts {
            match ctx.parse(text)?.into_applicable(text) {
                Ok(mods) => {
                    for modifier in mods {
                        let origin =
                            ModifierSource::new(source_id.clone()).with_raw_text(text.clone());
                        ingest.modifiers.push(modifier.with_origin(origin));
                    }
                }
                Err(unsupported) => ingest.unsupported.push(unsupported),
            }
        }
    }

    Ok(ingest)
}
