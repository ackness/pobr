use pobr_core::calc::MinimalInput;
use pobr_core::passive::{AllocatedNode, PassiveIngest, ingest_passive_nodes_with_ctx};

/// Engine-facing passive ingest (signature matches the historical `ingest_passive_nodes`).
fn ingest_passive_nodes(
    nodes: &[AllocatedNode],
) -> Result<PassiveIngest, pobr_core::mod_parser::ParseError> {
    ingest_passive_nodes_with_ctx(nodes, crate::support::ctx())
}
use pobr_core::{CalcConfig, ModDb};
use pobr_data::prelude::*;

fn node(id: u32, texts: &[&str]) -> AllocatedNode {
    AllocatedNode {
        node_id: NodeId(id),
        ascendancy: false,
        modifier_texts: texts.iter().map(|t| (*t).to_string()).collect(),
    }
}

#[test]
fn ingest_passive_nodes_parses_texts_into_modifiers_with_passive_source() {
    let nodes = vec![
        node(1001, &["+10 to maximum Life"]),
        node(1002, &["+12% to Fire Resistance"]),
    ];

    let PassiveIngest {
        modifiers,
        unsupported,
    } = ingest_passive_nodes(&nodes).unwrap();

    assert!(unsupported.is_empty());
    assert_eq!(modifiers.len(), 2);

    for modifier in &modifiers {
        let origin = modifier
            .origin
            .as_ref()
            .expect("passive node modifier carries an origin");
        assert_eq!(origin.source_id.kind, SourceKind::PassiveNode);
        // The raw modifier text must be retained so the breakdown display can be compared against PoB.
        assert!(origin.raw_text.is_some());
    }

    // stat_id / mod_type are back-filled from the modifier by with_origin.
    let life = modifiers
        .iter()
        .find(|m| m.name == ModName::from("MaximumLife"))
        .unwrap();
    assert_eq!(
        life.origin.as_ref().unwrap().stat_id,
        Some(ModName::from("MaximumLife"))
    );
    assert_eq!(life.origin.as_ref().unwrap().mod_type, Some(ModType::Base));
}

#[test]
fn ingested_passive_modifiers_attribute_back_to_node_id() {
    let nodes = vec![node(55555, &["+25 to maximum Life"])];

    let ingest = ingest_passive_nodes(&nodes).unwrap();

    let mut db = ModDb::new();
    db.add_list(ingest.modifiers);
    let cfg = CalcConfig::new();

    let contributions = db.contributions(ModType::Base, &cfg, &[ModName::from("MaximumLife")]);
    assert_eq!(contributions.len(), 1);
    let origin = contributions[0]
        .origin
        .as_ref()
        .expect("contribution carries the passive node origin");
    assert_eq!(origin.source_id.kind, SourceKind::PassiveNode);
    // Stable ID convention: node.<NodeId>, so it can be traced back to the specific node.
    assert_eq!(origin.source_id.id, "node.55555");
}

#[test]
fn ascendancy_nodes_attribute_to_ascendancy_source_kind() {
    let nodes = vec![AllocatedNode {
        node_id: NodeId(70000),
        ascendancy: true,
        modifier_texts: vec!["+15 to maximum Life".to_string()],
    }];

    let ingest = ingest_passive_nodes(&nodes).unwrap();
    assert_eq!(ingest.modifiers.len(), 1);
    let origin = ingest.modifiers[0].origin.as_ref().unwrap();
    assert_eq!(origin.source_id.kind, SourceKind::AscendancyNode);
    assert_eq!(origin.source_id.id, "node.70000");
}

#[test]
fn ingest_passive_nodes_collects_unsupported_texts() {
    let nodes = vec![node(2001, &["+40 to maximum Life", "mirrored"])];

    let ingest = ingest_passive_nodes(&nodes).unwrap();

    assert_eq!(ingest.modifiers.len(), 1);
    assert_eq!(ingest.unsupported, vec!["mirrored".to_string()]);
}

#[test]
fn session_add_passive_nodes_feeds_minimal_calc() {
    let input = MinimalInput {
        base_life: 100.0,
        ..MinimalInput::default()
    };
    let mut session = crate::support::session(input);

    let nodes = vec![
        node(1, &["+40 to maximum Life", "mirrored"]),
        node(2, &["20% increased maximum Life"]),
    ];

    session.add_passive_nodes(&nodes).unwrap();
    let output = session.perform_minimal();

    // (100 base + 40 node) * (1 + 20/100) = 168.
    assert_eq!(output.life, 168.0);
    // Unparseable modifier text is still retained.
    assert_eq!(session.unsupported_modifier_texts(), ["mirrored"]);
}
