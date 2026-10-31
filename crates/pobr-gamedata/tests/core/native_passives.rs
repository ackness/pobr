//! Synthetic packet/index regressions; these do not attest real raw-source acquisition.

use super::*;
use pobr_data::catalog::native_player::{
    NATIVE_ALLOCATION_ASSUMPTION, NATIVE_OTHER_ALLOCATIONS_EXCLUSION,
    NATIVE_PASSIVES_ACTIVE_CONDITION,
};
use pobr_data::catalog::native_runtime::NATIVE_REFERENCE_INDEX_PATH;
use pobr_gamedata::NativeData;

const INPUT: &str = "sorceress-spark-level1-q0-ordinary-damage-passives-v1";
const PROFILE: &str = "spark-level1-q0-ordinary-damage-passives";
const POLICY: &str = "selected-player-generic-ot-ordinary-damage-passives-v1";
const TREE: &str = "base/passive_tree.json";
const BINDING: &str = "base/native_actor_binding.json";

fn passive_pack() -> BTreeMap<String, Vec<u8>> {
    let mut files = player_pack();
    // Extend only this synthetic parent inventory, then reseal its dependent archives.
    let mut context: Value =
        serde_json::from_slice(&files["base/native_player_context.json"]).unwrap();
    let mut parent: Value = serde_json::from_str(
        context["source_archive"]["parent-manifest.json"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    let resources = json!({
        "tables/PassiveSkills.json":{"bytes":2,"sha256":hash(b"[]")},
        "tables/Stats.json":{"bytes":3,"sha256":hash(b"[]\n")}
    });
    parent["files"] = resources.clone();
    let parent_text = serde_json::to_string(&parent).unwrap();
    let parent_sha = hash(parent_text.as_bytes());
    context["parent_manifest_sha256"] = parent_sha.clone().into();
    context["source_archive"]["parent-manifest.json"] = parent_text.clone().into();
    let mut producer: Value =
        serde_json::from_str(context["source_archive"]["manifest.json"].as_str().unwrap()).unwrap();
    producer["parent"]["manifest_sha256"] = parent_sha.clone().into();
    producer["files"]["parent-manifest.json"] =
        json!({"sha256":parent_sha,"bytes":parent_text.len()});
    let producer_text = serde_json::to_string(&producer).unwrap();
    let producer_sha = hash(producer_text.as_bytes());
    context["source_manifest_sha256"] = producer_sha.clone().into();
    context["source_archive"]["manifest.json"] = producer_text.into();
    files.insert(
        "base/native_player_context.json".into(),
        serde_json::to_vec(&context).unwrap(),
    );
    edit(&mut files, BINDING, |v| {
        v["policy"] = POLICY.into();
        v["excluded_sources"][2] = NATIVE_OTHER_ALLOCATIONS_EXCLUSION.into();
        v["required_conditions"]
            .as_array_mut()
            .unwrap()
            .push(NATIVE_PASSIVES_ACTIVE_CONDITION.into());
        v["assumptions"]
            .as_array_mut()
            .unwrap()
            .push(NATIVE_ALLOCATION_ASSUMPTION.into());
    });
    edit(&mut files, NATIVE_RUNTIME_PATH, |v| {
        v["input_profile"] = INPUT.into();
        v["normalization"]["profile"] = PROFILE.into();
        v["rules"]["profile"] = PROFILE.into();
        v["tree"]["source_manifest_sha256"] = parent_sha.clone().into();
        v["player"]["parent_manifest_sha256"] = parent_sha.clone().into();
        v["player"]["source_manifest_sha256"] = producer_sha.clone().into();
        v["allowed_source_manifests"] = json!([parent_sha, producer_sha]);
    });
    let mut nodes = Vec::new();
    let mut definitions = serde_json::Map::new();
    let mut instances = Vec::new();
    for (id, slug, row, stat_row, stat_id, value) in [
        (11, "synthetic_damage", 3, 5, "spell_damage_+%", -17),
        (
            12,
            "synthetic_resistance",
            4,
            6,
            "base_fire_damage_resistance_%",
            7,
        ),
    ] {
        nodes.push(json!({"skill":id,"id":slug,"kind":"normal","native_stats":{
            "data_version":"0.5.5.5","tree_version":"0_5","node_id":id,"node_slug":slug,
            "modifier_texts":[],"context":"unconditional_player","stats":[{"id":stat_id,"value":value,
            "provenance":{"game_patch":"0.5.5.5","source_manifest_sha256":parent_sha,
                "passive_id":slug,"passive_row":row,"stat_row":stat_row,"references":[
                    {"resource":"tables/PassiveSkills.json","sha256":resources["tables/PassiveSkills.json"]["sha256"],
                        "pointer":format!("/rows/{row}/values/Stat1Value")},
                    {"resource":"tables/Stats.json","sha256":resources["tables/Stats.json"]["sha256"],
                        "pointer":format!("/rows/{stat_row}/values/Id")}]} }]}}));
        definitions.insert(slug.into(), json!({"graph_id_raw":id,"in_default_graph":true,
            "raw":{"payload":"tables/PassiveSkills.json","pointer":format!("/rows/{row}/values")},
            "stats":[{"stat_id":stat_id,"value":value,"slot":1,"value_field":"Stat1Value",
                "raw_stat":{"payload":"tables/Stats.json","pointer":format!("/rows/{stat_row}/values")}}]}));
        instances.push(
            json!({"graph_id":id,"definition_id":slug,"group":0,"radius_raw":1,
            "position_raw":0,"connections":[],"splines":[]}),
        );
    }
    files.insert(TREE.into(), serde_json::to_vec(&nodes).unwrap());
    files.insert(
        NATIVE_REFERENCE_INDEX_PATH.into(),
        serde_json::to_vec(&json!({
            "format":"pobr-native-runtime-reference-index/v2",
            "archive":{"manifest_sha256":parent_sha,"files":resources},
            "passive_definitions":definitions,"graph_instances":instances,
            "class_relations":[],"descriptions":{},"player":{},"skill_selection":{},
            "retained_opaque_evidence":{"unknown_column":42}
        }))
        .unwrap(),
    );
    reseal_passives(&mut files);
    files
}

fn reseal_passives(files: &mut BTreeMap<String, Vec<u8>>) {
    let index_sha = hash(&files[NATIVE_REFERENCE_INDEX_PATH]);
    edit(files, NATIVE_RUNTIME_PATH, |v| {
        v["reference_index_sha256"] = index_sha.into()
    });
    reseal_player(files);
}

#[test]
fn native_passive_profile_loads_disk_and_memory_with_resistance_evidence() {
    let files = passive_pack();
    let disk = DiskPack::new(&files);
    let expected = NativeData::load(&GameData::from_memory(files.clone())).unwrap();
    for data in [
        GameData::new(disk.version()),
        GameData::from_memory_prechecked(files.clone()),
        GameData::from_memory_releasing(files, true),
    ] {
        let native = NativeData::load(&data).unwrap();
        assert_eq!(native.identity(), expected.identity());
        assert_eq!(native.passive_nodes(), expected.passive_nodes());
        assert_eq!(native.passive_nodes().len(), 2);
        assert_eq!(
            native.passive_nodes()[0]
                .native_stats
                .as_ref()
                .unwrap()
                .stats[0]
                .value,
            -17
        );
        assert!(data.validate_calculation_admission().is_err());
    }
}

#[test]
fn native_passive_profiles_accept_only_exact_old_and_new_tuples() {
    for mask in 0..16 {
        let mut files = passive_pack();
        edit(&mut files, NATIVE_RUNTIME_PATH, |v| {
            v["input_profile"] = if mask & 1 != 0 {
                INPUT
            } else {
                "sorceress-spark-level1-q0-v1"
            }
            .into();
            v["normalization"]["profile"] = if mask & 2 != 0 {
                PROFILE
            } else {
                "spark-level1-q0"
            }
            .into();
            v["rules"]["profile"] = if mask & 4 != 0 {
                PROFILE
            } else {
                "spark-level1-q0"
            }
            .into();
        });
        if mask & 8 == 0 {
            edit(&mut files, BINDING, |v| {
                v["policy"] = "selected-player-generic-ot-v1".into();
                v["excluded_sources"][2] = "allocations".into();
                v["required_conditions"].as_array_mut().unwrap().pop();
                v["assumptions"].as_array_mut().unwrap().pop();
            });
        }
        reseal_passives(&mut files);
        assert_eq!(
            NativeData::load(&GameData::from_memory(files)).is_ok(),
            mask == 0 || mask == 15,
            "tuple mask {mask}"
        );
    }
    let mut v1 = pack();
    edit(&mut v1, NATIVE_RUNTIME_PATH, |v| {
        v["input_profile"] = INPUT.into();
        v["normalization"]["profile"] = PROFILE.into();
        v["rules"]["profile"] = PROFILE.into();
    });
    reseal(&mut v1);
    rejected(v1, "unsupported native input/profile tuple");
}

#[test]
fn native_passive_policy_requires_explicit_activation_and_legality_limit() {
    for key in ["required_conditions", "assumptions", "excluded_sources"] {
        let mut files = passive_pack();
        edit(&mut files, BINDING, |v| {
            if key == "excluded_sources" {
                v[key][2] = "allocations".into();
            } else {
                v[key].as_array_mut().unwrap().pop();
            }
        });
        reseal_passives(&mut files);
        rejected(files, "predicates/exclusions incomplete");
    }
}

#[test]
fn native_passive_packets_reject_resealed_identity_source_context_and_channel_drift() {
    for (pointer, value) in [
        ("/0/id", json!("synthetic_resistance")),
        ("/0/skill", json!(99)),
        ("/0/kind", json!("notable")),
        ("/0/native_stats/node_id", json!(99)),
        ("/0/native_stats/node_slug", json!("other")),
        ("/0/native_stats/data_version", json!("0.5.5.4")),
        ("/0/native_stats/tree_version", json!("unknown")),
        ("/0/native_stats/context", json!("unmodelled_context")),
        (
            "/0/native_stats/modifier_texts",
            json!(["10% increased Spell Damage"]),
        ),
        ("/0/native_stats/stats/0/id", json!("lightning_damage_+%")),
        ("/0/native_stats/stats/0/value", json!(42)),
        (
            "/0/native_stats/stats/0/provenance/game_patch",
            json!("0.5.5.4"),
        ),
        (
            "/0/native_stats/stats/0/provenance/passive_id",
            json!("other"),
        ),
        ("/0/native_stats/stats/0/provenance/passive_row", json!(4)),
        ("/0/native_stats/stats/0/provenance/stat_row", json!(6)),
        (
            "/0/native_stats/stats/0/provenance/source_manifest_sha256",
            json!("a".repeat(64)),
        ),
        (
            "/0/native_stats/stats/0/provenance/references/0/resource",
            json!("wrong.json"),
        ),
        (
            "/0/native_stats/stats/0/provenance/references/0/sha256",
            json!("a".repeat(64)),
        ),
        (
            "/0/native_stats/stats/0/provenance/references/0/pointer",
            json!("/rows/3/values/Stat2Value"),
        ),
        (
            "/0/native_stats/stats/0/provenance/references/1/pointer",
            json!("/rows/5/values/Name"),
        ),
        // Resistance is not admitted to hit calculation, but its emitted evidence must be consistent.
        ("/1/native_stats/stats/0/value", json!(99)),
    ] {
        let mut files = passive_pack();
        edit(&mut files, TREE, |v| {
            *v.pointer_mut(pointer).unwrap() = value
        });
        reseal_passives(&mut files);
        let error = NativeData::load(&GameData::from_memory(files)).unwrap_err();
        assert!(
            error.to_string().contains("native passive reference index"),
            "{pointer}: {error}"
        );
    }
    for mutation in 0..6 {
        let mut files = passive_pack();
        edit(&mut files, TREE, |v| match mutation {
            0 => {
                let duplicate = v[0].clone();
                v.as_array_mut().unwrap().push(duplicate);
            }
            1 => {
                let second = v[0]["native_stats"]["stats"][0].clone();
                v[0]["native_stats"]["stats"]
                    .as_array_mut()
                    .unwrap()
                    .push(second);
            }
            2 => {
                v[0]["native_stats"]["new_numerical_channel"] = json!(0);
            }
            3 => {
                v[0]["native_stats"]["stats"][0]["provenance"]["references"]
                    .as_array_mut()
                    .unwrap()
                    .pop();
            }
            4 => {
                let duplicate =
                    v[0]["native_stats"]["stats"][0]["provenance"]["references"][0].clone();
                v[0]["native_stats"]["stats"][0]["provenance"]["references"][1] = duplicate;
            }
            _ => {
                v[0]["apply_to_armour"] = true.into();
            }
        });
        reseal_passives(&mut files);
        assert!(
            NativeData::load(&GameData::from_memory(files)).is_err(),
            "mutation {mutation}"
        );
    }
}

#[test]
fn native_passive_index_rejects_resealed_wrong_parent_slots_rows_and_unknown_channels() {
    for (pointer, value) in [
        ("/archive/manifest_sha256", json!("a".repeat(64))),
        (
            "/archive/files/tables~1Stats.json/sha256",
            json!("b".repeat(64)),
        ),
        (
            "/graph_instances/0/definition_id",
            json!("synthetic_resistance"),
        ),
        (
            "/passive_definitions/synthetic_damage/graph_id_raw",
            json!(12),
        ),
        (
            "/passive_definitions/synthetic_damage/in_default_graph",
            json!(false),
        ),
        (
            "/passive_definitions/synthetic_damage/raw/payload",
            json!("tables/Stats.json"),
        ),
        (
            "/passive_definitions/synthetic_damage/raw/pointer",
            json!("/rows/4/values"),
        ),
        (
            "/passive_definitions/synthetic_damage/stats/0/stat_id",
            json!("unknown_stat"),
        ),
        (
            "/passive_definitions/synthetic_damage/stats/0/value",
            json!(-18),
        ),
        (
            "/passive_definitions/synthetic_damage/stats/0/slot",
            json!(2),
        ),
        (
            "/passive_definitions/synthetic_damage/stats/0/value_field",
            json!("Stat2Value"),
        ),
        (
            "/passive_definitions/synthetic_damage/stats/0/raw_stat/payload",
            json!("other.json"),
        ),
        (
            "/passive_definitions/synthetic_damage/stats/0/raw_stat/pointer",
            json!("/rows/6/values"),
        ),
    ] {
        let mut files = passive_pack();
        edit(&mut files, NATIVE_REFERENCE_INDEX_PATH, |v| {
            *v.pointer_mut(pointer).unwrap() = value
        });
        reseal_passives(&mut files);
        let error = NativeData::load(&GameData::from_memory(files)).unwrap_err();
        assert!(
            error.to_string().contains("native passive reference index"),
            "{pointer}: {error}"
        );
    }
    for mutation in 0..4 {
        let mut files = passive_pack();
        edit(
            &mut files,
            NATIVE_REFERENCE_INDEX_PATH,
            |v| match mutation {
                0 => {
                    let duplicate = v["graph_instances"][0].clone();
                    v["graph_instances"].as_array_mut().unwrap().push(duplicate);
                }
                1 => {
                    v["passive_definitions"]["synthetic_damage"]["unknown_context"] = json!(0);
                }
                2 => {
                    v["passive_definitions"]["synthetic_damage"]["stats"][0]["extra_channel"] =
                        json!(0);
                }
                _ => {
                    v["passive_definitions"]["synthetic_damage"]["stats"] = json!([]);
                }
            },
        );
        reseal_passives(&mut files);
        assert!(
            NativeData::load(&GameData::from_memory(files)).is_err(),
            "index mutation {mutation}"
        );
    }
}

#[test]
fn native_passive_index_uses_integrity_and_duplicate_key_path_without_widening_old_profiles() {
    let original = passive_pack();
    let mut corrupt = original.clone();
    corrupt
        .get_mut(NATIVE_REFERENCE_INDEX_PATH)
        .unwrap()
        .push(b' ');
    assert!(NativeData::load(&GameData::from_memory_prechecked(corrupt)).is_err());
    let mut files = original;
    let valid = String::from_utf8(files[NATIVE_REFERENCE_INDEX_PATH].clone()).unwrap();
    let duplicate = valid.replacen("{", "{\"graph_instances\":[],", 1);
    files.insert(NATIVE_REFERENCE_INDEX_PATH.into(), duplicate.into_bytes());
    reseal_passives(&mut files);
    let error = NativeData::load(&GameData::from_memory(files)).unwrap_err();
    assert!(error.to_string().contains("duplicate"), "{error}");
    // The old query-only and actor profiles keep their original index-loading behavior.
    for mut old in [pack(), player_pack()] {
        old.insert(NATIVE_REFERENCE_INDEX_PATH.into(), b"{}".to_vec());
        edit(&mut old, NATIVE_RUNTIME_PATH, |v| {
            v["reference_index_sha256"] = hash(b"{}").into()
        });
        reseal(&mut old);
        NativeData::load(&GameData::from_memory(old)).unwrap();
    }
}

#[test]
fn native_passive_packet_cannot_use_another_allowed_manifest_as_its_tree_source() {
    let mut files = passive_pack();
    let contract: Value = serde_json::from_slice(&files[NATIVE_RUNTIME_PATH]).unwrap();
    let other = contract["player"]["source_manifest_sha256"].clone();
    assert!(
        contract["allowed_source_manifests"]
            .as_array()
            .unwrap()
            .contains(&other)
    );
    edit(&mut files, TREE, |v| {
        v[0]["native_stats"]["stats"][0]["provenance"]["source_manifest_sha256"] = other;
    });
    reseal_passives(&mut files);
    let error = NativeData::load(&GameData::from_memory(files)).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("definition/stat identity, value, slot or source mismatch"),
        "{error}"
    );
}

fn shared_stat_row_pack() -> BTreeMap<String, Vec<u8>> {
    let mut files = passive_pack();
    edit(&mut files, TREE, |v| {
        let stat = &mut v[1]["native_stats"]["stats"][0];
        stat["id"] = "spell_damage_+%".into();
        stat["provenance"]["stat_row"] = 5.into();
        stat["provenance"]["references"][1]["pointer"] = "/rows/5/values/Id".into();
    });
    edit(&mut files, NATIVE_REFERENCE_INDEX_PATH, |v| {
        let stat = &mut v["passive_definitions"]["synthetic_resistance"]["stats"][0];
        stat["stat_id"] = "spell_damage_+%".into();
        stat["raw_stat"]["pointer"] = "/rows/5/values".into();
    });
    reseal_passives(&mut files);
    files
}

#[test]
fn native_passive_reference_rows_reject_conflicting_cross_record_identities() {
    // Both sides of each local packet/index relation agree. Only the cross-record
    // physical identity reveals the contradiction; resealing cannot legitimize it.
    let mut passive_alias = shared_stat_row_pack();
    edit(&mut passive_alias, TREE, |v| {
        let stat = &mut v[1]["native_stats"]["stats"][0];
        stat["value"] = (-17).into();
        stat["provenance"]["passive_row"] = 3.into();
        stat["provenance"]["references"][0]["pointer"] = "/rows/3/values/Stat1Value".into();
    });
    edit(&mut passive_alias, NATIVE_REFERENCE_INDEX_PATH, |v| {
        let definition = &mut v["passive_definitions"]["synthetic_resistance"];
        definition["raw"]["pointer"] = "/rows/3/values".into();
        definition["stats"][0]["value"] = (-17).into();
    });
    let mut stat_alias = passive_pack();
    edit(&mut stat_alias, TREE, |v| {
        let provenance = &mut v[1]["native_stats"]["stats"][0]["provenance"];
        provenance["stat_row"] = 5.into();
        provenance["references"][1]["pointer"] = "/rows/5/values/Id".into();
    });
    edit(&mut stat_alias, NATIVE_REFERENCE_INDEX_PATH, |v| {
        v["passive_definitions"]["synthetic_resistance"]["stats"][0]["raw_stat"]["pointer"] =
            "/rows/5/values".into();
    });
    for (mut files, reason) in [
        (passive_alias, "conflicting PassiveSkills row identity"),
        (stat_alias, "conflicting Stats row identity"),
    ] {
        reseal_passives(&mut files);
        let error = NativeData::load(&GameData::from_memory(files))
            .err()
            .expect("conflicting physical row identities must be rejected");
        assert!(error.to_string().contains(reason), "{error}");
    }
}

#[test]
fn native_passive_reference_rows_allow_shared_stats_and_reordered_references() {
    let mut files = shared_stat_row_pack();
    edit(&mut files, TREE, |v| {
        v[1]["native_stats"]["stats"][0]["provenance"]["references"]
            .as_array_mut()
            .unwrap()
            .reverse();
    });
    reseal_passives(&mut files);
    let native = NativeData::load(&GameData::from_memory(files)).unwrap();
    assert_eq!(native.passive_nodes().len(), 2);
    let stats: Vec<_> = native
        .passive_nodes()
        .iter()
        .map(|node| &node.native_stats.as_ref().unwrap().stats[0])
        .collect();
    assert_eq!(stats[0].provenance.stat_row, stats[1].provenance.stat_row);
    assert_eq!(stats[0].id, stats[1].id);
    assert_ne!(
        stats[0].provenance.passive_row,
        stats[1].provenance.passive_row
    );
    assert_ne!(stats[0].value, stats[1].value);
}

#[test]
fn historical_projectile_minimum_reasons_remain_unchanged_in_both_policies() {
    for files in [player_pack(), passive_pack()] {
        let native = NativeData::load(&GameData::from_memory(files)).unwrap();
        let stats = &native.player_context().unwrap().facts.metadata_stats;
        let index = stats
            .iter()
            .position(|s| s.key_raw == "object_innate_minimum_number_of_projectiles_to_fire_is_1")
            .unwrap();
        let selector = format!("/facts/metadata_stats/{index}");
        let fact = native
            .actor_binding()
            .unwrap()
            .dispositions
            .iter()
            .find(|d| d.selector == selector)
            .unwrap();
        assert_eq!(
            fact.reason,
            "The selected intrinsic total projectile count already satisfies this minimum; no generic-OT projectile adjustment is applied."
        );
        assert_eq!(
            fact.disposition,
            pobr_data::catalog::native_player::NativeFactDispositionKind::Inactive
        );
        assert!(fact.relevant_to_conditional_hit);
    }
}
