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
        native: None,
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
        ..
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
        native: None,
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
    let output = session.perform_minimal().expect("perform");

    // (100 base + 40 node) * (1 + 20/100) = 168.
    assert_eq!(output.life, 168.0);
    // Unparseable modifier text is still retained.
    assert_eq!(session.unsupported_modifier_texts(), ["mirrored"]);
}

fn native_node() -> AllocatedNode {
    let mut defs: Vec<pobr_data::catalog::PassiveNodeDef> = serde_json::from_str(include_str!(
        "../../../pobr-data/tests/fixtures/native_passive/base/passive_tree.json"
    ))
    .unwrap();
    let def = defs.remove(0);
    AllocatedNode {
        node_id: NodeId(def.skill),
        modifier_texts: def.stats,
        native: Some(pobr_core::passive::NativePassiveInput {
            packet: def.native_stats.unwrap(),
            ineligible: None,
        }),
        ..Default::default()
    }
}

#[test]
fn native_passive_without_parser_reaches_resistance_and_preserves_provenance() {
    let node = native_node();
    let mut session = pobr_core::calc::CalculationSession::new(MinimalInput::default());
    session
        .add_passive_nodes(std::slice::from_ref(&node))
        .unwrap();
    assert_eq!(session.perform_minimal().unwrap().fire_resistance, 5.0);
    assert!(session.unsupported_modifier_texts().is_empty());
    assert!(session.native_passive_diagnostics().is_empty());
    let mods: Vec<_> = session.mod_db().named("FireResistance").collect();
    assert_eq!(mods.len(), 1);
    let origin = mods[0].origin.as_ref().unwrap();
    assert_eq!(
        origin.source_id,
        pobr_core::passive::node_source(62518, false)
    );
    assert_eq!(origin.raw_text.as_ref(), Some(&node.modifier_texts[0]));
    assert_eq!(origin.stat_id, Some("FireResistance".into()));
    assert_eq!(origin.mod_type, Some(ModType::Base));
    assert_eq!(
        origin.native_stat.as_ref(),
        Some(&node.native.unwrap().packet.stats[0])
    );
}

#[test]
fn native_passive_and_text_are_exclusive_and_legacy_stays_identical() {
    let node = native_node();
    let native = ingest_passive_nodes(std::slice::from_ref(&node)).unwrap();
    let mut legacy_node = node.clone();
    legacy_node.native = None;
    let legacy = ingest_passive_nodes(&[legacy_node]).unwrap();
    assert_eq!(native.modifiers.len(), 1);
    assert_eq!(legacy.modifiers.len(), 1);
    let mut native_mod = native.modifiers[0].clone();
    native_mod.origin.as_mut().unwrap().native_stat = None;
    assert_eq!(native_mod.name, legacy.modifiers[0].name);
    assert_eq!(native_mod.value, legacy.modifiers[0].value);
    assert_eq!(native_mod.mod_type, legacy.modifiers[0].mod_type);
    assert_eq!(native_mod.flags, legacy.modifiers[0].flags);
    assert_eq!(native_mod.keyword_flags, legacy.modifiers[0].keyword_flags);
    assert_eq!(native_mod.tags, legacy.modifiers[0].tags);
    assert_eq!(native_mod.origin, legacy.modifiers[0].origin);
    let old_origin = serde_json::to_value(legacy.modifiers[0].origin.as_ref().unwrap()).unwrap();
    assert!(old_origin.get("native_stat").is_none());
    let old_origin: ModifierSource = serde_json::from_value(old_origin).unwrap();
    assert!(old_origin.native_stat.is_none());
}

#[test]
fn native_passive_rejections_fall_back_whole_and_keep_diagnostics_without_parser() {
    use pobr_core::rules::passive_stats::NativePassiveRejection as Reason;
    use pobr_data::native_stat::NativePassiveContext;
    for reason in [
        Reason::UnknownStat,
        Reason::UnsupportedContext,
        Reason::InexactValue,
        Reason::BindingMismatch,
        Reason::IncompletePacket,
        Reason::TransformedNode,
    ] {
        let mut node = native_node();
        let input = node.native.as_mut().unwrap();
        match reason {
            Reason::UnknownStat => {
                input.packet.stats[0].id = "chaos_resistance_+_while_using_flask".into()
            }
            Reason::UnsupportedContext => input.packet.context = NativePassiveContext::Unsupported,
            Reason::InexactValue => input.packet.stats[0].value = i64::MAX,
            Reason::BindingMismatch => input.packet.node_id += 1,
            Reason::IncompletePacket => input.packet.stats.push(input.packet.stats[0].clone()),
            Reason::TransformedNode => input.ineligible = Some(reason),
            _ => unreachable!(),
        }
        for ctx in [
            crate::support::ctx(),
            pobr_core::mod_parser::ParseCtx::none(),
        ] {
            let ingest = ingest_passive_nodes_with_ctx(std::slice::from_ref(&node), ctx).unwrap();
            assert_eq!(ingest.native_diagnostics.len(), 1);
            let issue = &ingest.native_diagnostics[0];
            assert_eq!(issue.reason, reason);
            assert_eq!(issue.fallback_texts, node.modifier_texts);
            assert_eq!(issue.packet, node.native.as_ref().unwrap().packet);
            assert_eq!(ingest.modifiers.len() + ingest.unsupported.len(), 1);
            if let Some(m) = ingest.modifiers.first() {
                assert_eq!(m.value.as_number(), Some(5.0));
                assert!(m.origin.as_ref().unwrap().native_stat.is_none());
            } else {
                assert_eq!(ingest.unsupported, node.modifier_texts);
            }
        }
    }
}

#[test]
fn native_passive_signed_zero_and_exact_conversion_edges() {
    use pobr_core::rules::passive_stats::NativePassiveRejection;
    for value in [0, -5, 7, 1_i64 << 53, (1_i64 << 53) + 2, i64::MIN] {
        let mut node = native_node();
        // Synthetic mapping boundaries, not new claims about the source passive.
        node.native.as_mut().unwrap().packet.stats[0].value = value;
        let ingest =
            ingest_passive_nodes_with_ctx(&[node], pobr_core::mod_parser::ParseCtx::none())
                .unwrap();
        assert_eq!(ingest.modifiers.len(), 1, "{value}");
        assert_eq!(ingest.modifiers[0].value.as_number(), Some(value as f64));
        assert!(ingest.native_diagnostics.is_empty());
        assert_eq!(
            ingest.modifiers[0]
                .origin
                .as_ref()
                .unwrap()
                .native_stat
                .as_ref()
                .unwrap()
                .value,
            value
        );
    }
    for value in [(1_i64 << 53) + 1, -((1_i64 << 53) + 1), i64::MAX] {
        let mut node = native_node();
        node.native.as_mut().unwrap().packet.stats[0].value = value;
        let ingest =
            ingest_passive_nodes_with_ctx(&[node], pobr_core::mod_parser::ParseCtx::none())
                .unwrap();
        assert!(ingest.modifiers.is_empty());
        assert_eq!(
            ingest.native_diagnostics[0].reason,
            NativePassiveRejection::InexactValue
        );
    }
}

#[test]
fn native_passive_does_not_dispatch_by_node_name_number_or_value() {
    let mut first = native_node();
    first.node_id = NodeId(99);
    let input = first.native.as_mut().unwrap();
    input.packet.node_id = 99;
    input.packet.node_slug = "synthetic_equal_effect".into();
    input.packet.stats[0].provenance.passive_id = input.packet.node_slug.clone();
    let second = native_node();
    let ingest =
        ingest_passive_nodes_with_ctx(&[first, second], pobr_core::mod_parser::ParseCtx::none())
            .unwrap();
    let mut db = ModDb::new();
    db.add_list(ingest.modifiers);
    let contributions = db.contributions(
        ModType::Base,
        &CalcConfig::new(),
        &["FireResistance".into()],
    );
    assert_eq!(contributions.len(), 2);
    assert_ne!(
        contributions[0].origin.as_ref().unwrap().source_id,
        contributions[1].origin.as_ref().unwrap().source_id
    );
    assert_eq!(
        db.sum(
            ModType::Base,
            &CalcConfig::new(),
            &["FireResistance".into()]
        ),
        10.0
    );
}
