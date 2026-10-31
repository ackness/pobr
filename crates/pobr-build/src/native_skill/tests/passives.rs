//! Synthetic packet admission/composition tests; actual new-profile loading is separate.

use pobr_data::catalog::PassiveNodeDef;
use pobr_data::catalog::native_player::NativeActorPolicy;
use pobr_data::catalog::native_runtime::{NativeInputProfile, NativeProfile};
use pobr_data::native_stat::NativePassiveStats;
use pobr_data::passive_tree::NodeId;

use super::*;

const TREE: &str = "base/passive_tree.json";
const HASHES: [&str; 2] = [
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
];

fn contract() -> NativeRuntimeContract {
    let mut contract = load(pack()).contract().clone();
    contract.input_profile = NativeInputProfile::SorceressSparkLevel1Q0OrdinaryDamagePassivesV1;
    contract.normalization.profile = NativeProfile::SparkLevel1Q0OrdinaryDamagePassives;
    contract.rules.profile = NativeProfile::SparkLevel1Q0OrdinaryDamagePassives;
    contract
}

fn node(contract: &NativeRuntimeContract, graph: u32, stat: &str, value: i64) -> PassiveNodeDef {
    // Synthetic source identities and values, with the same complete packet shape.
    // The loader's sealed reference-index proof is Unit A's separate boundary.
    let packet: NativePassiveStats = serde_json::from_value(json!({
        "data_version":contract.game_patch,
        "tree_version":contract.tree.compatibility_tag,
        "node_id":graph, "node_slug":format!("synthetic-{graph}"),
        "modifier_texts":[], "context":"unconditional_player",
        "stats":[{"id":stat,"value":value,"provenance":{
            "game_patch":contract.game_patch,
            "source_manifest_sha256":contract.tree.source_manifest_sha256,
            "passive_id":format!("synthetic-{graph}"), "passive_row":graph,"stat_row":25,
            "references":[
                {"resource":"tables/PassiveSkills.json","sha256":HASHES[0],"pointer":format!("/rows/{graph}/values/Stat1Value")},
                {"resource":"tables/Stats.json","sha256":HASHES[1],"pointer":"/rows/25/values/Id"}
            ]
        }}]
    })).unwrap();
    serde_json::from_value(json!({
        "skill":graph,"id":packet.node_slug,"kind":"normal","native_stats":packet
    }))
    .unwrap()
}

fn selected(
    nodes: &[PassiveNodeDef],
    ids: &[u32],
) -> Result<Vec<passive::Selected>, NativeAdmissionError> {
    passive::resolve_nodes(
        nodes,
        &contract(),
        &ids.iter().copied().map(NodeId).collect::<Vec<_>>(),
        HASHES,
    )
}

fn composed(nodes: &[PassiveNodeDef], ids: &[u32]) -> NativeSkillResult {
    // Exercise the real selector -> binding -> shared reader -> public evidence path
    // independently of the new policy/index loader. No new-profile fixture is forged.
    let mut files = pack();
    // Keep the four-byte lexical span of the synthetic source token intact.
    bonus(&mut files, "100.");
    let data = load(files);
    let mut request = request(&data);
    let mut admitted = admit_native_skill(&data, &request).unwrap();
    let baseline_facts = serde_json::to_value(&admitted.skill_facts).unwrap();
    request.build.tree.allocated_nodes = ids.iter().copied().map(NodeId).collect();
    admitted.request_sha256 = request.input_sha256().unwrap();
    admitted.request = request;
    admitted.profile = scope::Profile::OrdinaryDamagePassives;
    for node in selected(nodes, ids).unwrap() {
        admitted.hit_modifiers.push(node.modifier);
        admitted.modifier_facts.push(node.binding);
        admitted
            .provenance
            .selection_sources
            .push(node.selection_source);
    }
    let result = calculate_native_skill(&admitted).unwrap();
    assert_eq!(
        serde_json::to_value(&result.skill_facts).unwrap(),
        baseline_facts
    );
    result
}

fn passive_evidence(result: &NativeSkillResult) -> Vec<&NativeModifierEvidence> {
    result
        .modifiers
        .iter()
        .filter(|m| m.runtime_source.artifact == TREE)
        .collect()
}

#[test]
fn exact_profile_tuples_do_not_form_a_cartesian_product() {
    let base = contract();
    for input_new in [false, true] {
        for normalization_new in [false, true] {
            for rules_new in [false, true] {
                for policy_new in [false, true] {
                    let mut c = base.clone();
                    c.input_profile = if input_new {
                        base.input_profile
                    } else {
                        NativeInputProfile::SorceressSparkLevel1Q0V1
                    };
                    c.normalization.profile = if normalization_new {
                        base.normalization.profile
                    } else {
                        NativeProfile::SparkLevel1Q0
                    };
                    c.rules.profile = if rules_new {
                        base.rules.profile
                    } else {
                        NativeProfile::SparkLevel1Q0
                    };
                    let policy = if policy_new {
                        NativeActorPolicy::SelectedPlayerGenericOtOrdinaryDamagePassivesV1
                    } else {
                        NativeActorPolicy::SelectedPlayerGenericOtV1
                    };
                    assert_eq!(
                        scope::profile(&c, policy).is_ok(),
                        input_new == normalization_new
                            && input_new == rules_new
                            && input_new == policy_new,
                        "{c:?}, {policy:?}"
                    );
                }
            }
        }
    }
    let data = load(pack());
    let mut request = request(&data);
    request.build.tree.allocated_nodes.push(NodeId(4739));
    assert!(
        admit_native_skill(&data, &request).is_err(),
        "old policy must stay closed"
    );
    scope::build(
        &request.build,
        &request,
        scope::Profile::OrdinaryDamagePassives,
    )
    .unwrap();
    request.build.tree.mastery_effects.insert(
        NodeId(4739),
        pobr_data::passive_tree::MasterySelection {
            effect_text: String::new(),
        },
    );
    assert!(
        scope::build(
            &request.build,
            &request,
            scope::Profile::OrdinaryDamagePassives
        )
        .is_err()
    );
}

#[test]
fn distinct_equal_sources_and_four_scopes_add_once_with_deterministic_evidence() {
    let c = contract();
    let nodes = vec![
        node(&c, 2, "spell_damage_+%", 10),
        node(&c, 1, "spell_damage_+%", 10),
        node(&c, 3, "lightning_damage_+%", 10),
        node(&c, 4, "elemental_damage_+%", 10),
        node(&c, 5, "projectile_damage_+%", 10),
    ];
    for (ids, mean) in [
        (vec![], 5.995),
        (vec![1], 6.5945),
        (vec![1, 2], 7.194),
        (vec![1, 3, 4, 5], 8.393),
    ] {
        let out = composed(&nodes, &ids);
        near(out.conditional_weighted_mean, mean);
        near(out.critical_probability, 0.09);
        near(out.critical_multiplier, 2.0);
        assert_eq!(out.analytical_base_use_ms, 700);
        assert_eq!(out.resource_costs[0].amount_per_use, 5.0);
        assert_eq!(out.total_projectiles, 4.0);
        assert_eq!(out.projectile_lifetime_ms, 2000.0);
        assert_eq!(passive_evidence(&out).len(), ids.len());
        assert!(
            out.unavailable
                .iter()
                .any(|u| u.output == "allocation-legality"
                    && u.reason.contains("reachability")
                    && u.reason.contains("progression"))
        );
        for evidence in passive_evidence(&out) {
            assert!(evidence.counted);
            assert!(!evidence.queries.is_empty());
            assert!(evidence.tags.is_empty());
            let origin = evidence.origin.as_ref().unwrap();
            let raw = origin.native_stat.as_ref().unwrap();
            assert_eq!(evidence.native_stat_id, raw.id);
            assert_eq!(
                origin.source_id.id,
                format!("node.{}", raw.provenance.passive_row)
            );
            let index = nodes
                .iter()
                .position(|n| n.skill == raw.provenance.passive_row)
                .unwrap();
            assert_eq!(
                evidence.runtime_source.pointer,
                format!("/{index}/native_stats/stats/0")
            );
            assert!(
                out.provenance
                    .selection_sources
                    .contains(&reference(TREE, format!("/{index}")))
            );
        }
    }
    let forward = composed(&nodes, &[1, 2]);
    let reverse = composed(&nodes, &[2, 1]);
    assert_ne!(forward.request_sha256, reverse.request_sha256);
    assert_eq!(forward.damage, reverse.damage);
    assert_eq!(
        serde_json::to_value(&forward.modifiers).unwrap(),
        serde_json::to_value(&reverse.modifiers).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&forward.provenance.selection_sources).unwrap(),
        serde_json::to_value(&reverse.provenance.selection_sources).unwrap()
    );
    assert_ne!(
        passive_evidence(&forward)[0].origin,
        passive_evidence(&forward)[1].origin
    );
    assert_eq!(
        serde_json::to_value(composed(&nodes, &[])).unwrap(),
        serde_json::to_value(composed(&nodes, &[])).unwrap(),
        "no starting grants and undo reproduces baseline"
    );
}

#[test]
fn signed_zero_changed_values_and_irrelevant_types_retain_real_reader_evidence() {
    let c = contract();
    for (amount, expected) in [(-10, 5.3955), (0, 5.995), (37, 8.21315)] {
        let nodes = [node(&c, 1, "spell_damage_+%", amount)];
        let out = composed(&nodes, &[1]);
        near(out.conditional_weighted_mean, expected);
        let evidence = passive_evidence(&out)[0];
        assert_eq!(evidence.value, amount as f64);
        assert_eq!(evidence.counted, amount != 0);
        assert!(
            !evidence.queries.is_empty(),
            "known zero retains its reader"
        );
    }
    for stat in [
        "physical_damage_+%",
        "fire_damage_+%",
        "cold_damage_+%",
        "chaos_damage_+%",
    ] {
        let out = composed(&[node(&c, 1, stat, 37)], &[1]);
        near(out.conditional_weighted_mean, 5.995);
        let evidence = passive_evidence(&out)[0];
        assert_eq!(
            evidence.counted,
            stat == "physical_damage_+%",
            "aggregation evidence is not marginal damage: {stat}"
        );
        assert_eq!(
            evidence
                .origin
                .as_ref()
                .unwrap()
                .native_stat
                .as_ref()
                .unwrap()
                .value,
            37
        );
        assert!(
            out.damage
                .iter()
                .filter(|d| d.damage_type != DamageType::Lightning)
                .all(|d| d.non_critical.max == 0.0),
            "INC cannot create a base component"
        );
    }
}

#[test]
fn reordered_source_references_preserve_selection_and_full_native_evidence() {
    let mut nodes = [node(&contract(), 1, "spell_damage_+%", 10)];
    let original = composed(&nodes, &[1]);
    nodes[0].native_stats.as_mut().unwrap().stats[0]
        .provenance
        .references
        .reverse();
    let reordered = composed(&nodes, &[1]);
    near(reordered.conditional_weighted_mean, 6.5945);
    assert_eq!(original.damage, reordered.damage);
    assert_eq!(original.request_sha256, reordered.request_sha256);
    let evidence = passive_evidence(&reordered)[0];
    assert!(evidence.counted);
    assert_eq!(
        evidence.origin.as_ref().unwrap().native_stat.as_ref(),
        Some(&nodes[0].native_stats.as_ref().unwrap().stats[0])
    );
    assert_eq!(
        evidence.runtime_source,
        passive_evidence(&original)[0].runtime_source
    );
    // Reference order is retained as evidence, while both exact bindings are required.
    for duplicate in [0, 1] {
        let mut bad = nodes.clone();
        let references = &mut bad[0].native_stats.as_mut().unwrap().stats[0]
            .provenance
            .references;
        references[1 - duplicate] = references[duplicate].clone();
        assert!(selected(&bad, &[1]).is_err());
    }
}

#[test]
fn duplicate_unknown_missing_and_unsupported_mixed_selections_fail_whole() {
    let c = contract();
    let good = node(&c, 1, "spell_damage_+%", 10);
    assert!(
        selected(std::slice::from_ref(&good), &[1, 1])
            .unwrap_err()
            .to_string()
            .contains("duplicate requested")
    );
    assert!(
        selected(std::slice::from_ref(&good), &[0]).is_err(),
        "no graph-zero invention"
    );
    assert!(selected(std::slice::from_ref(&good), &[1, 999]).is_err());
    assert!(selected(&[good.clone(), good.clone()], &[1]).is_err());
    assert!(
        selected(&[good.clone(), good.clone()], &[]).is_err(),
        "ambiguous catalogs never build an index"
    );
    for (pointer, value) in [
        ("/native_stats", Value::Null),
        ("/kind", json!("notable")),
        ("/ascendancy_id", json!("Oracle")),
        ("/unlock_constraint", json!({"nodes":[]})),
        ("/variants", json!([{"class":"Witch","stats":[]}])),
        ("/apply_to_armour", json!(true)),
        ("/native_stats/context", json!("unsupported")),
        ("/native_stats/data_version", json!("0.5.5.6")),
        ("/native_stats/tree_version", Value::Null),
        ("/native_stats/node_id", json!(999)),
        ("/native_stats/node_slug", json!("other")),
        ("/native_stats/stats", json!([])),
        (
            "/native_stats/stats/0/id",
            json!("base_fire_damage_resistance_%"),
        ),
        ("/native_stats/stats/0/id", json!("unknown_zero")),
        (
            "/native_stats/stats/0/value",
            json!(9_007_199_254_740_993_i64),
        ),
        (
            "/native_stats/stats/0/provenance/game_patch",
            json!("other"),
        ),
        (
            "/native_stats/stats/0/provenance/passive_id",
            json!("other"),
        ),
        (
            "/native_stats/stats/0/provenance/source_manifest_sha256",
            json!("f".repeat(64)),
        ),
        (
            "/native_stats/stats/0/provenance/references/0/resource",
            json!("other"),
        ),
        (
            "/native_stats/stats/0/provenance/references/0/sha256",
            json!(HASHES[1]),
        ),
        (
            "/native_stats/stats/0/provenance/references/0/pointer",
            json!("/rows/1/values/Stat2Value"),
        ),
        (
            "/native_stats/stats/0/provenance/references/1/pointer",
            json!("/rows/26/values/Id"),
        ),
    ] {
        let mut value_node = serde_json::to_value(node(&c, 2, "spell_damage_+%", 10)).unwrap();
        // Optional fields can be omitted by serialization; insert through the root object.
        if !pointer[1..].contains('/') {
            value_node[&pointer[1..]] = value;
        } else {
            *value_node.pointer_mut(pointer).unwrap() = value;
        }
        let bad: PassiveNodeDef = serde_json::from_value(value_node).unwrap();
        let error = selected(&[good.clone(), bad], &[1, 2]).expect_err("mixed set fails whole");
        assert!(
            error.to_string().contains("base/passive_tree.json#/1"),
            "{pointer}: {error}"
        );
    }
    let mut other_parent = node(&c, 2, "spell_damage_+%", 10);
    other_parent.native_stats.as_mut().unwrap().stats[0]
        .provenance
        .source_manifest_sha256 = c
        .allowed_source_manifests
        .iter()
        .find(|sha| sha.as_str() != c.tree.source_manifest_sha256.as_str())
        .expect("player source is independently allowed")
        .clone();
    assert!(
        selected(&[good.clone(), other_parent], &[1, 2]).is_err(),
        "another allowed manifest is not the tree parent"
    );
    let mut bad = node(&c, 2, "spell_damage_+%", 0);
    bad.native_stats.as_mut().unwrap().stats[0].id = "unknown_zero".into();
    assert!(selected(&[good.clone(), bad], &[1, 2]).is_err());
    let mut bad = node(&c, 2, "spell_damage_+%", 10);
    let packet = bad.native_stats.as_mut().unwrap();
    packet.stats.push(packet.stats[0].clone());
    // Valid-looking legacy text never rescues a rejected whole packet.
    bad.stats = vec!["10% increased Spell Damage".into()];
    packet.modifier_texts = bad.stats.clone();
    assert!(selected(&[good.clone(), bad], &[1, 2]).is_err());
    let mut bad = node(&c, 2, "spell_damage_+%", 10);
    bad.stats = vec!["10% increased Spell Damage".into()];
    assert!(
        selected(&[good, bad], &[1, 2]).is_err(),
        "text binding mismatch"
    );
}

#[test]
fn names_and_build_tree_labels_cannot_select_a_replacement_or_version() {
    let c = contract();
    let mut nodes = [node(&c, 1, "elemental_damage_+%", 10)];
    nodes[0].name = Some("Cold resistance".into());
    assert_eq!(
        selected(&nodes, &[1]).unwrap()[0].modifier.name.as_str(),
        "ElementalDamage"
    );
    let data = load(pack());
    let mut r = request(&data);
    r.build.character.class_name = "女巫".into();
    r.build.tree_version = Some("任意显示标签".into());
    r.build.tree.allocated_nodes = vec![NodeId(1)];
    scope::build(&r.build, &r, scope::Profile::OrdinaryDamagePassives).unwrap();
}

#[test]
fn missing_private_origin_binding_is_a_calculation_error() {
    let data = load(pack());
    let mut admitted = admit_native_skill(&data, &request(&data)).unwrap();
    admitted.modifier_facts.clear();
    assert!(matches!(
        calculate_native_skill(&admitted),
        Err(NativeCalculationError::MissingModifierFact)
    ));
}
