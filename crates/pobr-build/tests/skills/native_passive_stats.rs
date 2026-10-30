//! One real native passive fact in a controlled, independently sealed test pack.

use std::collections::BTreeMap;
use std::path::PathBuf;

use pobr_build::usage::{LineStatus, line_usage};
use pobr_build::{Build, BuildData, DataOrchestratorOptions, Diagnosis, diagnose, parse_build};
use pobr_core::rules::passive_stats::NativePassiveRejection as Reason;
use pobr_data::catalog::{PassiveNodeDef, PassiveNodeKind, PassiveNodeVariant};
use pobr_data::native_stat::NativePassiveContext;
use pobr_data::passive_tree::{AttributeChoice, NodeId};
use pobr_gamedata::GameData;

const NODE: u32 = 62518;
const TEXT: &str = "+5% to [Resistances|Fire Resistance]";

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../pobr-data/tests/fixtures/native_passive")
}

fn pack() -> BTreeMap<String, Vec<u8>> {
    let root = fixture();
    let manifest = std::fs::read(root.join("manifest.json")).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&manifest).unwrap();
    let mut files: BTreeMap<_, _> = value["files"]
        .as_object()
        .unwrap()
        .keys()
        .map(|path| (path.clone(), std::fs::read(root.join(path)).unwrap()))
        .collect();
    files.insert("manifest.json".into(), manifest);
    files
}

fn data(parser: bool) -> BuildData {
    let mut data = BuildData::load(&GameData::from_memory(pack())).unwrap();
    assert!(
        data.parser_rules.is_none(),
        "the sealed pack has no parser catalog"
    );
    if parser {
        data.parser_rules = Some(std::sync::Arc::new(
            pobr_core::mod_parser::test_compiled_rules(),
        ));
    }
    data
}

fn build(nodes: &str) -> Build {
    let mut build = parse_build(&format!(
        r#"<PathOfBuilding2>
<Build level="1" className="Fixture"/>
<Tree activeSpec="1"><Spec nodes="{nodes}"/></Tree>
</PathOfBuilding2>"#
    ))
    .unwrap();
    // The imported default quest rewards are deliberately absent in this baseline.
    build.config.global_modifier_texts.clear();
    build
}

fn run(build: &Build, data: &BuildData) -> Diagnosis {
    diagnose(
        build,
        data,
        &DataOrchestratorOptions {
            inject_character_base: false,
            ..Default::default()
        },
    )
    .unwrap()
}

fn synthetic_node(skill: u32, stats: &[&str]) -> PassiveNodeDef {
    serde_json::from_value(serde_json::json!({
        "skill":skill,"id":format!("synthetic_{skill}"),"kind":"normal","stats":stats
    }))
    .unwrap()
}

#[test]
fn native_passive_pack_allocation_full_calculation_and_ledger_without_parser() {
    let data = data(false);
    let allocated = build("62518");
    let baseline = run(&build(""), &data);
    assert_eq!(baseline.session.output().fire_resistance, 0.0);
    let diagnosis = run(&allocated, &data);
    assert_eq!(diagnosis.session.output().fire_resistance, 5.0);
    assert!(diagnosis.unparsed_lines.is_empty());
    assert!(diagnosis.session.native_passive_diagnostics().is_empty());
    let modifiers = diagnosis.session.mods_named("FireResistance");
    assert_eq!(modifiers.len(), 1);
    let origin = modifiers[0].origin.as_ref().unwrap();
    assert_eq!(origin.source_id.id, "node.62518");
    assert_eq!(origin.raw_text.as_deref(), Some(TEXT));
    assert_eq!(origin.stat_id, Some("FireResistance".into()));
    let raw = origin.native_stat.as_ref().unwrap();
    assert_eq!(raw.id, "base_fire_damage_resistance_%");
    assert_eq!(raw.value, 5);
    assert_eq!(raw.provenance.stat_row, 139);
    assert_eq!(raw.provenance.passive_row, 1390);
    assert_eq!(raw.provenance.game_patch, "0.5.5.5");
    assert_eq!(data.passive_nodes[&NODE].stats, [TEXT]);
    let ledger = line_usage(&diagnosis, &allocated, &data);
    let lines: Vec<_> = ledger
        .lines
        .iter()
        .filter(|l| l.source_id.as_deref() == Some("node.62518"))
        .collect();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].status, LineStatus::Applied);
    assert_eq!(lines[0].modifiers.len(), 1);
}

#[test]
fn native_passive_is_exclusive_legacy_unchanged_and_effect_copies_share_channel() {
    for parser in [false, true] {
        let mut data = data(parser);
        let allocated = build("62518");
        assert_eq!(run(&allocated, &data).session.output().fire_resistance, 5.0);
        data.passive_nodes.insert(
            99,
            synthetic_node(99, &["100% increased effect of Small Passive Skills"]),
        );
        let diagnosis = run(&build("62518,99"), &data);
        assert_eq!(diagnosis.session.output().fire_resistance, 10.0);
        let mods = diagnosis.session.mods_named("FireResistance");
        assert_eq!(mods.len(), 2, "base plus existing scaled delta");
        assert!(
            mods.iter()
                .all(|m| m.origin.as_ref().unwrap().native_stat.is_some())
        );
        data.passive_nodes.get_mut(&NODE).unwrap().native_stats = None;
        let legacy = run(&allocated, &data);
        assert_eq!(
            legacy.session.output().fire_resistance,
            if parser { 5.0 } else { 0.0 }
        );
        assert!(legacy.session.native_passive_diagnostics().is_empty());
        assert_eq!(
            run(&build("62518,99"), &data)
                .session
                .output()
                .fire_resistance,
            if parser { 10.0 } else { 0.0 }
        );
    }
}

#[test]
fn native_passive_rejected_packets_keep_whole_fallback_and_reason() {
    for parser in [false, true] {
        for reason in [
            Reason::UnknownStat,
            Reason::UnsupportedContext,
            Reason::BindingMismatch,
            Reason::VersionMismatch,
            Reason::InexactValue,
            Reason::IncompletePacket,
        ] {
            let mut data = data(parser);
            let packet = data
                .passive_nodes
                .get_mut(&NODE)
                .unwrap()
                .native_stats
                .as_mut()
                .unwrap();
            match reason {
                Reason::UnknownStat => {
                    packet.stats[0].id = "chaos_resistance_+_while_using_flask".into()
                }
                Reason::UnsupportedContext => packet.context = NativePassiveContext::Unsupported,
                Reason::BindingMismatch => packet.node_slug = "stale_definition".into(),
                Reason::VersionMismatch => packet.data_version = "old-pack".into(),
                Reason::InexactValue => packet.stats[0].value = i64::MAX,
                Reason::IncompletePacket => packet.stats.push(packet.stats[0].clone()),
                _ => unreachable!(),
            }
            let allocated = build("62518");
            let diagnosis = run(&allocated, &data);
            assert_eq!(
                diagnosis.session.output().fire_resistance,
                if parser { 5.0 } else { 0.0 },
                "{reason:?}"
            );
            let issues = diagnosis.session.native_passive_diagnostics();
            assert_eq!(issues.len(), 1);
            assert_eq!(issues[0].reason, reason);
            assert_eq!(issues[0].fallback_texts, [TEXT]);
            assert_eq!(
                issues[0].packet,
                data.passive_nodes[&NODE].native_stats.clone().unwrap()
            );
            assert!(
                diagnosis
                    .session
                    .mods_named("FireResistance")
                    .iter()
                    .all(|m| m.origin.as_ref().unwrap().native_stat.is_none())
            );
            let ledger = line_usage(&diagnosis, &allocated, &data);
            let line = ledger
                .lines
                .iter()
                .find(|l| l.source_id.as_deref() == Some("node.62518"))
                .unwrap();
            assert_eq!(
                line.status,
                if parser {
                    LineStatus::Applied
                } else {
                    LineStatus::Unparsed
                }
            );
        }
    }
}

#[test]
fn native_passive_transformed_conditional_and_appended_text_never_revives_subset() {
    for parser in [false, true] {
        let mut data = data(parser);
        let def = data.passive_nodes.get_mut(&NODE).unwrap();
        def.stats = vec!["+5% to Fire Resistance while on Full Life".into()];
        let mut allocated = build("62518");
        let diagnosis = run(&allocated, &data);
        assert_eq!(diagnosis.session.output().fire_resistance, 0.0);
        assert_eq!(
            diagnosis.session.native_passive_diagnostics()[0].reason,
            Reason::BindingMismatch
        );
        if parser {
            assert!(
                !diagnosis.session.mods_named("FireResistance")[0]
                    .tags
                    .is_empty()
            );
        }
        allocated.config.conditions.insert("FullLife".into(), true);
        assert_eq!(
            run(&allocated, &data).session.output().fire_resistance,
            if parser { 5.0 } else { 0.0 }
        );
        allocated.config.conditions.clear();
        let def = data.passive_nodes.get_mut(&NODE).unwrap();
        def.stats = vec![TEXT.into(), "Unmodelled extra effect".into()];
        let diagnosis = run(&allocated, &data);
        assert_eq!(
            diagnosis.session.output().fire_resistance,
            if parser { 5.0 } else { 0.0 }
        );
        assert_eq!(
            diagnosis.session.native_passive_diagnostics()[0].fallback_texts,
            data.passive_nodes[&NODE].stats
        );
        assert!(
            diagnosis
                .unparsed_lines
                .iter()
                .any(|t| t.contains("Unmodelled extra effect"))
        );
        let def = data.passive_nodes.get_mut(&NODE).unwrap();
        def.stats = vec![TEXT.into()];
        def.variants.push(PassiveNodeVariant {
            class: "Fixture".into(),
            variant_skill: Some(123),
            name: None,
            stats: vec!["+9% to Fire Resistance".into()],
        });
        let diagnosis = run(&allocated, &data);
        assert_eq!(
            diagnosis.session.output().fire_resistance,
            if parser { 9.0 } else { 0.0 }
        );
        assert_eq!(
            diagnosis.session.native_passive_diagnostics()[0].reason,
            Reason::TransformedNode
        );
        data.passive_nodes.get_mut(&NODE).unwrap().variants.clear();
        data.passive_nodes.get_mut(&NODE).unwrap().stats.clear();
        let empty = run(&allocated, &data);
        assert_eq!(empty.session.output().fire_resistance, 0.0);
        assert_eq!(
            empty.session.native_passive_diagnostics()[0].reason,
            Reason::BindingMismatch
        );
        data.passive_nodes.get_mut(&NODE).unwrap().stats = vec![TEXT.into()];
        allocated
            .tree
            .attribute_overrides
            .insert(NodeId(NODE), AttributeChoice::Strength);
        assert_eq!(
            run(&allocated, &data).session.native_passive_diagnostics()[0].reason,
            Reason::TransformedNode
        );
    }
}

#[test]
fn native_passive_historical_requirement_and_gated_nodes_do_not_leak() {
    let mut data = data(false);
    data.versioned_passive_nodes
        .insert("old".into(), data.passive_nodes.clone());
    let allocated = build("62518").with_tree_version(Some("old".into()));
    let diagnosis = run(&allocated, &data);
    assert_eq!(diagnosis.session.output().fire_resistance, 0.0);
    assert_eq!(
        diagnosis.session.native_passive_diagnostics()[0].reason,
        Reason::VersionMismatch
    );
    for kind in [PassiveNodeKind::JewelSocket, PassiveNodeKind::Mastery] {
        data.passive_nodes.get_mut(&NODE).unwrap().kind = kind;
        let diagnosis = run(&build("62518"), &data);
        assert_eq!(diagnosis.session.output().fire_resistance, 0.0);
        assert!(diagnosis.session.mods_named("FireResistance").is_empty());
    }
    let def = data.passive_nodes.get_mut(&NODE).unwrap();
    def.kind = PassiveNodeKind::Normal;
    def.unlock_constraint = Some(pobr_data::catalog::PassiveUnlockConstraint {
        nodes: vec![],
        ascendancy: None,
    });
    let diagnosis = run(&build("62518"), &data);
    assert_eq!(diagnosis.session.output().fire_resistance, 0.0);
    assert_eq!(
        diagnosis.session.native_passive_diagnostics()[0].reason,
        Reason::UnsupportedContext
    );
}

#[test]
fn native_passive_zero_negative_and_distinct_equal_effects_remain_attributed() {
    for value in [0, -5] {
        let mut data = data(false);
        data.passive_nodes
            .get_mut(&NODE)
            .unwrap()
            .native_stats
            .as_mut()
            .unwrap()
            .stats[0]
            .value = value;
        let allocated = build("62518");
        let diagnosis = run(&allocated, &data);
        assert_eq!(diagnosis.session.output().fire_resistance, value as f64);
        assert_eq!(diagnosis.session.mods_named("FireResistance").len(), 1);
        let usage = line_usage(&diagnosis, &allocated, &data);
        let line = usage
            .lines
            .iter()
            .find(|l| l.source_id.as_deref() == Some("node.62518"))
            .unwrap();
        assert_eq!(
            line.status,
            if value == 0 {
                LineStatus::Inactive
            } else {
                LineStatus::Applied
            }
        );
        assert_eq!(line.modifiers.len(), 1);
    }
    let mut data = data(false);
    let mut other = data.passive_nodes[&NODE].clone();
    other.skill = 99;
    other.id = "synthetic_equal".into();
    let packet = other.native_stats.as_mut().unwrap();
    packet.node_id = other.skill;
    packet.node_slug = other.id.clone();
    packet.stats[0].provenance.passive_id = other.id.clone();
    data.passive_nodes.insert(99, other);
    let allocated = build("62518,99");
    let diagnosis = run(&allocated, &data);
    assert_eq!(diagnosis.session.output().fire_resistance, 10.0);
    let usage = line_usage(&diagnosis, &allocated, &data);
    assert_eq!(
        usage
            .lines
            .iter()
            .filter(|l| l.text.as_deref() == Some(TEXT))
            .count(),
        2
    );
}

#[test]
fn native_passive_loader_integrity_defaults_and_disk_memory_identity() {
    let disk = GameData::new(fixture());
    let memory = GameData::from_memory(pack());
    assert_eq!(disk.identity().unwrap(), memory.identity().unwrap());
    assert_eq!(
        BuildData::load(&disk).unwrap().passive_nodes,
        BuildData::load(&memory).unwrap().passive_nodes
    );
    let node = &data(false).passive_nodes[&NODE];
    let mut json = serde_json::to_value(node).unwrap();
    json.as_object_mut().unwrap().remove("native_stats");
    let legacy: PassiveNodeDef = serde_json::from_value(json.clone()).unwrap();
    assert!(legacy.native_stats.is_none());
    assert!(
        serde_json::to_value(legacy)
            .unwrap()
            .get("native_stats")
            .is_none()
    );
    json["native_stats"] = serde_json::json!({"stats": "malformed"});
    assert!(serde_json::from_value::<PassiveNodeDef>(json).is_err());
    let mut corrupt = pack();
    corrupt
        .get_mut("base/passive_tree.json")
        .unwrap()
        .push(b' ');
    assert!(BuildData::load(&GameData::from_memory(corrupt)).is_err());
    let mut unlisted = pack();
    unlisted.insert(
        "overlay/mod_parser_rules.json".into(),
        b"invalid unlisted file".to_vec(),
    );
    let unlisted = GameData::from_memory(unlisted);
    assert_eq!(unlisted.identity().unwrap(), memory.identity().unwrap());
    assert!(BuildData::load(&unlisted).unwrap().parser_rules.is_none());
}

#[test]
fn native_passive_jewel_replacement_even_identical_text_invalidates_source() {
    for parser in [false, true] {
        for replacement in [Some("+9% to Fire Resistance"), Some(TEXT), None] {
            let mut data = data(parser);
            data.passive_jewels = serde_json::from_value(serde_json::json!({
                "tree_version":"controlled", "class_starts":{"fixture":"synthetic_98"},
                "ring_sizes":{},
                "conquerors":{"amanamu":{"family":"fixture","keystone":"unused"}},
                "nodes":{"replacement":{"name":"Synthetic replacement","stats":[replacement.unwrap_or(TEXT)]}},
                "families":{"fixture":{"small_replacement":replacement.map(|_| "replacement"),"attribute_additions":[]}}
            })).unwrap();
            let mut start = synthetic_node(98, &[]);
            start.x = Some(10000.0);
            start.y = Some(10000.0);
            data.passive_nodes.insert(98, start);
            let mut socket = synthetic_node(99, &[]);
            socket.kind = PassiveNodeKind::JewelSocket;
            socket.x = Some(0.0);
            socket.y = Some(0.0);
            data.passive_nodes.insert(99, socket);
            let def = data.passive_nodes.get_mut(&NODE).unwrap();
            def.x = Some(100.0);
            def.y = Some(0.0);
            let raw = "Rarity: UNIQUE\nSynthetic Jewel\nDiamond\nRadius: Very Large\nImplicits: 0\nGlorifying the defilement of 8000 souls in tribute to Amanamu\nPassives in radius are Conquered by the Abyssals\nHistoric";
            let allocated = build("62518,99")
                .with_jewels(vec![pobr_core::parse_pob_xml_item(raw).unwrap()])
                .with_radius_jewels(vec![pobr_build::radius_jewel_from_text(99, raw).unwrap()]);
            let state = pobr_build::jewel_tree::passive_jewel_state(&allocated, &data);
            assert!(state.conquered.contains(&NODE));
            assert_eq!(state.nodes.contains_key(&NODE), replacement.is_some());
            let diagnosis = run(&allocated, &data);
            let expected = if parser {
                if replacement == Some("+9% to Fire Resistance") {
                    9.0
                } else {
                    5.0
                }
            } else {
                0.0
            };
            assert_eq!(diagnosis.session.output().fire_resistance, expected);
            assert_eq!(
                diagnosis.session.native_passive_diagnostics()[0].reason,
                Reason::TransformedNode
            );
            assert!(
                diagnosis
                    .session
                    .mods_named("FireResistance")
                    .iter()
                    .all(|m| m.origin.as_ref().unwrap().native_stat.is_none())
            );
            assert_eq!(data.passive_nodes[&NODE].stats, [TEXT]);
        }
    }
}

#[test]
fn native_passive_resealed_value_changes_identity_and_malformed_packet_fails_loading() {
    let original = GameData::from_memory(pack());
    // SHA-256 of this fixture's exact tree bytes after the one replacement below.
    // Both are synthetic contract probes, not new source values for node 62518.
    for (value, digest) in [
        (
            "7",
            "1aeca49132263c0dbe1c4a81b98522298112131954d74dd40ef5b26d1e7c475d",
        ),
        (
            "\"malformed\"",
            "cc9ab9ef81c0c24a4ab00db55811f769b9b0d2ad0208b1941509f59d6b802116",
        ),
    ] {
        let mut files = pack();
        let tree = String::from_utf8(files["base/passive_tree.json"].clone()).unwrap();
        files.insert(
            "base/passive_tree.json".into(),
            tree.replacen("\"value\": 5", &format!("\"value\": {value}"), 1)
                .into_bytes(),
        );
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&files["manifest.json"]).unwrap();
        manifest["files"]["base/passive_tree.json"] = digest.into();
        files.insert(
            "manifest.json".into(),
            serde_json::to_vec(&manifest).unwrap(),
        );
        let changed = GameData::from_memory(files);
        assert_ne!(changed.identity().unwrap(), original.identity().unwrap());
        if value == "7" {
            let data = BuildData::load(&changed).unwrap();
            assert!(data.parser_rules.is_none());
            assert_eq!(
                run(&build("62518"), &data).session.output().fire_resistance,
                7.0
            );
        } else {
            assert!(BuildData::load(&changed).is_err());
        }
    }
}
