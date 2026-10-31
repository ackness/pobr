use super::*;

fn input() -> trust::VerifiedInput {
    let fixture: Value = serde_json::from_slice(include_bytes!(
        "../../../tests/fixtures/native-runtime/spark-physical.json"
    ))
    .unwrap();
    let mut documents = BTreeMap::new();
    for (table, selected) in fixture["tables"].as_object().unwrap() {
        let mut rows = Vec::new();
        for (index, values) in selected.as_object().unwrap() {
            let index = index.parse::<usize>().unwrap();
            rows.resize(rows.len().max(index + 1), json!({"values":{}}));
            rows[index] = json!({"values":values});
        }
        documents.insert(format!("tables/{table}.json"), json!({"rows":rows}));
    }
    trust::VerifiedInput {
        profile: Profile::ItemsActorsBc,
        manifest: fixture["manifest"].clone(),
        manifest_bytes: vec![],
        lock_bytes: vec![],
        bytes: BTreeMap::new(),
        documents,
    }
}

fn mutate(input: &mut trust::VerifiedInput, table: &str, index: usize, key: &str, value: Value) {
    input
        .documents
        .get_mut(&format!("tables/{table}.json"))
        .unwrap()["rows"][index]["values"][key] = value;
}

fn document(files: &Files, name: &str) -> Value {
    serde_json::from_slice(&files[name]).unwrap()
}

#[test]
fn native_runtime_actual_cells_use_stored_damage_and_keep_native_branch() {
    let files = skills::project(&input()).unwrap();
    let effects = document(&files, "granted_effects.json");
    assert_eq!(effects[0]["id"], "SparkPlayer");
    assert_eq!(effects[0]["cast_time"], 700);
    let levels = document(&files, "granted_effect_levels.json");
    assert_eq!(levels["SparkPlayer"][0]["crit_chance"], 9.0);
    assert_eq!(levels["SparkPlayer"][0]["cost_amounts"], json!([5]));
    assert!(levels["SparkPlayer"][0].get("attack_time_ms").is_none());
    let sets = document(&files, "granted_effect_stat_sets.json");
    assert_eq!(sets[0]["sets"].as_array().unwrap().len(), 1);
    let set = &sets[0]["sets"][0];
    assert_eq!(set["set_id"], "SparkPlayer");
    assert!(set.get("vendor_set_index").is_none());
    assert!(set.get("user_level_scaling").is_none());
    assert_eq!(
        set["levels"][0]["stats"],
        json!([
        {"stat":"spell_minimum_base_lightning_damage","value":1.0},
        {"stat":"spell_maximum_base_lightning_damage","value":10.0},
        {"stat":"base_number_of_projectiles","value":4.0}])
    );
    assert!(
        set["constant_stats"]
            .as_array()
            .unwrap()
            .contains(&json!({"stat":"base_skill_effect_duration","value":2000.0}))
    );
    assert!(
        set["implicit_stats"]
            .as_array()
            .unwrap()
            .contains(&json!("base_is_projectile"))
    );
}

#[test]
fn native_runtime_values_come_from_selected_source_not_spark_constants() {
    let mut input = input();
    mutate(
        &mut input,
        "GrantedEffectStatSetsPerLevel",
        1467,
        "BaseResolvedValues",
        json!([7, 21]),
    );
    mutate(
        &mut input,
        "GrantedEffectStatSetsPerLevel",
        1467,
        "FloatStatsValues",
        json!([1000, -999]),
    );
    mutate(
        &mut input,
        "GrantedEffectStatSetsPerLevel",
        1467,
        "AdditionalStatsValues",
        json!([6]),
    );
    mutate(
        &mut input,
        "GrantedEffectStatSets",
        38,
        "ConstantStatsValues",
        json!([1500, 20, 1009, -70, 160, 50, 660]),
    );
    let files = skills::project(&input).unwrap();
    let sets = document(&files, "granted_effect_stat_sets.json");
    let stats = &sets[0]["sets"][0]["levels"][0]["stats"];
    assert_eq!(stats[0]["value"], 7.0);
    assert_eq!(stats[1]["value"], 21.0);
    assert_eq!(stats[2]["value"], 6.0);
    assert_eq!(sets[0]["sets"][0]["constant_stats"][0]["value"], 1500.0);
}

#[test]
fn native_runtime_projection_rejects_ambiguous_or_unaligned_selected_channels() {
    for (table, index, key, value) in [
        (
            "GrantedEffectStatSetsPerLevel",
            1467,
            "BaseResolvedValues",
            json!([1]),
        ),
        (
            "GrantedEffectStatSetsPerLevel",
            1467,
            "BaseResolvedValues",
            json!([9007199254740993i64, 10]),
        ),
        (
            "GrantedEffectStatSetsPerLevel",
            1467,
            "FloatStatsValues",
            json!([0.1]),
        ),
        (
            "GrantedEffectStatSetsPerLevel",
            1467,
            "AdditionalStatsValues",
            json!([]),
        ),
        (
            "GrantedEffectStatSetsPerLevel",
            1467,
            "AttackCritChance",
            json!(10),
        ),
        (
            "GrantedEffectStatSetsPerLevel",
            1467,
            "AdditionalFlags",
            json!([1]),
        ),
        (
            "GrantedEffectStatSetsPerLevel",
            1467,
            "ActorLevel",
            json!(2),
        ),
        ("GrantedEffectStatSets", 38, "Id", json!("unrelated")),
        ("GrantedEffects", 36, "CostTypes", json!([0, 0])),
    ] {
        let mut input = input();
        mutate(&mut input, table, index, key, value);
        assert!(skills::project(&input).is_err(), "accepted {table}.{key}");
    }
    let mut input = input();
    let rows = input
        .documents
        .get_mut("tables/GrantedEffectsPerLevel.json")
        .unwrap()["rows"]
        .as_array_mut()
        .unwrap();
    rows.push(rows[1301].clone());
    assert!(skills::project(&input).unwrap_err().contains("exactly one"));
}

fn tree_case() -> (trust::VerifiedInput, super::super::model::Candidate, Value) {
    let input = input();
    let raw = json!({"payload":"tables/PassiveSkills.json","pointer":"/rows/1390/values"});
    let candidate=serde_json::from_value(json!({"format":"test","input_manifest_sha256":"0".repeat(64),
        "summary":{"passive_definitions":1,"graph_instances":1,"graph_stat_entries":1,"class_overrides":0,"effects":0,"qualities":0},
        "passive_order":["maximum_resistances1"],"passives":{"maximum_resistances1":{"raw":raw,"graph_id_raw":62518,"in_default_graph":true,
            "stats":[{"stat_id":"base_fire_damage_resistance_%","raw_stat":{"payload":"tables/Stats.json","pointer":"/rows/139/values"},"slot":1,"value_field":"Stat1Value","value":5}]}},
        "graph":{"tree_id":"Default","raw_tree":raw,"raw":raw,"header":[],"skills_per_orbit_raw":[],"roots":[],
            "groups":[{"x_raw":0.0,"y_raw":0.0,"flag_raw":0,"unknown1":0,"unknown2":0,"node_order":[62518]}]},
        "instances":[{"graph_id":62518,"definition_id":"maximum_resistances1","group":0,"radius_raw":3,"position_raw":7,"connections":[],"splines":[]}],
        "class_relations":[],"effect_order":[],"effects":{},"stat_set_order":[],"stat_sets":{},"qualities":[],"source_diagnostics":"unused"
    })).unwrap();
    let official = json!({"tree":"Default","nodes":{"62518":{"skill":62518,"id":"maximum_resistances1","group":1,"orbit":3,"orbitIndex":7,"out":[],"x":12.25,"y":-5.75}},
        "groups":{"1":{"x":0.0,"y":0.0,"nodes":["62518"]}},"classes":[]});
    let mut official = official;
    official["nodes"]["root"] = json!({"group":0,"orbit":0,"orbitIndex":0,"out":[]});
    (input, candidate, official)
}

#[test]
fn native_runtime_tree_uses_client_facts_and_only_official_coordinates() {
    let (input, candidate, mut official) = tree_case();
    official["nodes"]["62518"]["stats"] = json!(["untrusted numeric text"]);
    official["nodes"]["62518"]["name"] = json!("not the client name");
    let tree = tree::project(&input, &candidate, &official, &"a".repeat(64), "0.5.5.5").unwrap();
    assert_eq!(tree.nodes[0].name.as_deref(), Some("Fire Resistance"));
    assert_eq!(tree.nodes[0].x, Some(12.25));
    assert_eq!(tree.nodes[0].y, Some(-5.75));
    assert!(tree.nodes[0].stats.is_empty());
    let packet = tree.nodes[0].native_stats.as_ref().unwrap();
    assert_eq!(packet.tree_version.as_deref(), Some("0_5"));
    assert_eq!(packet.stats[0].value, 5);
    assert_eq!(packet.stats[0].provenance.passive_row, 1390);
    assert_eq!(tree.meta.classes[0].base_int, 15);
}

#[test]
fn native_runtime_tree_requires_graph_agreement_and_ordinary_packet_context() {
    for (key, value) in [
        ("out", json!(["123"])),
        ("orbitIndex", json!(8)),
        ("id", json!("different")),
        ("x", json!(null)),
    ] {
        let (input, candidate, mut official) = tree_case();
        official["nodes"]["62518"][key] = value;
        assert!(tree::project(&input, &candidate, &official, &"a".repeat(64), "0.5.5.5").is_err());
    }
    let (mut input, candidate, official) = tree_case();
    mutate(
        &mut input,
        "PassiveSkills",
        1390,
        "IsMultipleChoice",
        json!(true),
    );
    assert!(tree::project(&input, &candidate, &official, &"a".repeat(64), "0.5.5.5").is_err());
}

fn ordinary_damage_tree_case() -> (trust::VerifiedInput, super::super::model::Candidate, Value) {
    let (mut input, mut candidate, mut official) = tree_case();
    let evidence: Value = serde_json::from_slice(include_bytes!(
        "../../../../../crates/pobr-data/tests/fixtures/native_passive_damage/evidence.json"
    ))
    .unwrap();
    for sample in evidence["samples"].as_array().unwrap() {
        let source = &sample["passive"];
        let id = source["definition"]["id"].as_str().unwrap();
        let row = source["definition"]["row"].as_u64().unwrap() as usize;
        let node = source["graph_id"].as_u64().unwrap();
        let stat = &source["stats"][0];
        let stat_row = stat["stat"]["row"].as_u64().unwrap();
        let rows = input
            .documents
            .get_mut("tables/PassiveSkills.json")
            .unwrap()["rows"]
            .as_array_mut()
            .unwrap();
        rows.resize(rows.len().max(row + 1), json!({"values":{}}));
        rows[row] = json!({"values":sample["physical_context"]});
        candidate.passives.insert(
            id.into(),
            serde_json::from_value(json!({
            "raw":{"payload":"tables/PassiveSkills.json","pointer":format!("/rows/{row}/values")},
            "graph_id_raw":node,"in_default_graph":true,"stats":[{
                "stat_id":stat["stat"]["id"],"value":stat["value"],"slot":stat["slot"],
                "value_field":stat["value_field"],"raw_stat":{"payload":"tables/Stats.json",
                    "pointer":format!("/rows/{stat_row}/values")}}]}))
            .unwrap(),
        );
        candidate.instances.push(
            serde_json::from_value(json!({"graph_id":node,"definition_id":id,
            "group":0,"radius_raw":3,"position_raw":7,"connections":[],"splines":[]}))
            .unwrap(),
        );
        candidate.graph.groups[0].node_order.push(node);
        official["groups"]["1"]["nodes"]
            .as_array_mut()
            .unwrap()
            .push(json!(node.to_string()));
        official["nodes"][node.to_string()] = json!({"skill":node,"id":id,"group":1,"orbit":3,
            "orbitIndex":7,"out":[],"x":12.25,"y":-5.75});
    }
    (input, candidate, official)
}

#[test]
fn native_runtime_ordinary_packets_use_core_rules_source_values_and_complete_context() {
    let (mut input, mut candidate, official) = ordinary_damage_tree_case();
    for value in [10, 0, -11, 37] {
        candidate.passives.get_mut("chaos6").unwrap().stats[0].value = value;
        mutate(
            &mut input,
            "PassiveSkills",
            3241,
            "Stat1Value",
            json!(value),
        );
        let projected =
            tree::project(&input, &candidate, &official, &"a".repeat(64), "0.5.5.5").unwrap();
        assert_eq!(projected.nodes.len(), 9);
        let nodes = projected
            .nodes
            .iter()
            .map(|node| {
                let packet = node
                    .native_stats
                    .clone()
                    .expect("all nine reviewed stat IDs have packets");
                assert!(node.stats.is_empty());
                assert!(packet.modifier_texts.is_empty());
                assert_eq!(packet.node_slug, node.id);
                assert_eq!(packet.node_id, node.skill);
                assert_eq!(packet.stats[0].provenance.passive_id, node.id);
                if node.skill == 52695 {
                    assert_eq!(packet.stats[0].id, "physical_damage_+%");
                    assert_eq!(packet.stats[0].value, value);
                    assert_eq!(packet.stats[0].provenance.passive_row, 3241);
                    assert_eq!(packet.stats[0].provenance.stat_row, 26);
                    assert_eq!(
                        packet.stats[0].provenance.references[0].pointer,
                        "/rows/3241/values/Stat1Value"
                    );
                }
                pobr_core::passive::AllocatedNode {
                    node_id: pobr_data::passive_tree::NodeId(node.skill),
                    modifier_texts: node.stats.clone(),
                    native: Some(pobr_core::passive::NativePassiveInput {
                        packet,
                        ineligible: None,
                    }),
                    ..Default::default()
                }
            })
            .collect::<Vec<_>>();
        let ingest = pobr_core::passive::ingest_passive_nodes_with_ctx(
            &nodes,
            pobr_core::mod_parser::ParseCtx::none(),
        )
        .unwrap();
        assert_eq!(ingest.modifiers.len(), 9);
        assert!(ingest.native_diagnostics.is_empty());
        assert!(ingest.unsupported.is_empty());
        assert!(
            ingest
                .modifiers
                .iter()
                .all(|m| m.origin.as_ref().unwrap().raw_text.is_none())
        );
    }
}

#[test]
fn native_runtime_ordinary_packet_expansion_rejects_unknown_and_changed_context_as_whole() {
    for (field, value) in [
        ("Stat2Value", json!(10)),
        ("Flag0", json!(true)),
        ("GrantedSkill", json!(1)),
        ("Unknown7", json!(999)),
        ("extra_context", json!(0)),
    ] {
        let (mut input, candidate, official) = ordinary_damage_tree_case();
        mutate(&mut input, "PassiveSkills", 3241, field, value);
        let projected =
            tree::project(&input, &candidate, &official, &"a".repeat(64), "0.5.5.5").unwrap();
        assert!(
            projected
                .nodes
                .iter()
                .find(|n| n.skill == 52695)
                .unwrap()
                .native_stats
                .is_none(),
            "{field}"
        );
        assert_eq!(
            projected
                .nodes
                .iter()
                .filter(|n| n.native_stats.is_some())
                .count(),
            8
        );
    }
    for changed in ["unknown", "multiple"] {
        let (input, mut candidate, official) = ordinary_damage_tree_case();
        let passive = candidate.passives.get_mut("chaos6").unwrap();
        if changed == "unknown" {
            passive.stats[0].stat_id = "unknown_stat".into();
        } else {
            let additional =
                serde_json::from_value(serde_json::to_value(&passive.stats[0]).unwrap()).unwrap();
            passive.stats.push(additional);
        }
        let projected =
            tree::project(&input, &candidate, &official, &"a".repeat(64), "0.5.5.5").unwrap();
        assert!(
            projected
                .nodes
                .iter()
                .find(|n| n.skill == 52695)
                .unwrap()
                .native_stats
                .is_none(),
            "{changed}"
        );
    }
}

#[test]
fn native_runtime_rejects_duplicates_before_building_runtime_vectors() {
    for (table, row, key, value) in [
        (
            "GrantedEffectStatSetsPerLevel",
            1467,
            "FloatStats",
            json!([1468, 1468]),
        ),
        (
            "GrantedEffectStatSetsPerLevel",
            1467,
            "AdditionalStats",
            json!([1468]),
        ),
        (
            "GrantedEffectStatSets",
            38,
            "ConstantStats",
            json!([347, 347, 11237, 18231, 18294, 20007, 21278]),
        ),
        (
            "GrantedEffectStatSets",
            38,
            "ConstantStats",
            json!([1468, 17186, 11237, 18231, 18294, 20007, 21278]),
        ),
        (
            "GrantedEffectStatSets",
            38,
            "ImplicitStats",
            json!([489, 489]),
        ),
        ("GrantedEffectStatSets", 38, "ImplicitStats", json!([347])),
    ] {
        let mut input = input();
        mutate(&mut input, table, row, key, value);
        assert!(
            skills::project(&input)
                .unwrap_err()
                .contains("duplicate selected stat ID")
        );
    }
}

#[test]
fn native_runtime_scalar_integer_channels_reject_fractions_and_width_overflow() {
    for (table, row, fields) in [
        (
            "GrantedEffectStatSetsPerLevel",
            1467,
            &["SpellCritChance", "BaseMultiplier"][..],
        ),
        (
            "GrantedEffectsPerLevel",
            1301,
            &[
                "AttackSpeedMultiplier",
                "CostMultiplier",
                "Reservation",
                "EffectOnPlayer",
            ][..],
        ),
    ] {
        for key in fields {
            for value in [json!(900.5), json!(2147483648i64), json!(-2147483649i64)] {
                let mut input = input();
                mutate(&mut input, table, row, key, value);
                assert!(
                    skills::project(&input)
                        .unwrap_err()
                        .contains("physical signed i32"),
                    "{table}.{key}"
                );
            }
        }
    }
}

#[test]
fn native_runtime_official_connector_is_mandatory_and_never_allocatable() {
    let (input, candidate, mut official) = tree_case();
    assert_eq!(
        tree::project(&input, &candidate, &official, &"a".repeat(64), "0.5.5.5")
            .unwrap()
            .nodes
            .len(),
        1
    );
    official["nodes"].as_object_mut().unwrap().remove("root");
    assert!(
        tree::project(&input, &candidate, &official, &"a".repeat(64), "0.5.5.5")
            .err()
            .unwrap()
            .contains("root connector is required")
    );
}

#[test]
fn native_runtime_cli_requires_all_external_pins_and_rejects_duplicates() {
    for args in [
        vec!["--native-runtime", "in"],
        vec!["--native-runtime", "in", "--native-runtime", "again"],
        vec!["--native-runtime", "in", "--patch", "0.5.5.4.2"],
    ] {
        assert!(parse_args(args.into_iter().map(String::from)).is_err());
    }
}

#[test]
fn native_runtime_assembled_catalog_loads_and_keeps_calculation_guard() {
    let (mut input, candidate, official) = tree_case();
    input.manifest_bytes = json_bytes(&input.manifest).unwrap();
    input.lock_bytes = b"test source lock\n".to_vec();
    let descriptions = inputs::Descriptions {
        manifest: json!({"files":{},"source_identity":{
        "source_lock_sha256":"b".repeat(64),"acquisition":{"status":"unreviewed-acquisition-draft"}}}),
    };
    let args = Args {
        facts: super::super::Args {
            input: "unused".into(),
            out: "unused".into(),
            source_lock: "unused".into(),
            expected_manifest_sha256: "a".repeat(64),
            profile: Profile::ItemsActorsBc,
        },
        descriptions: "unused".into(),
        descriptions_sha256: "b".repeat(64),
        tree: "unused".into(),
        tree_sha256: "c".repeat(64),
        compiler_revision: "d".repeat(40),
        player: None,
    };
    let (base, receipt) =
        assemble(&input, &candidate, &official, &descriptions, None, &args).unwrap();
    let mut files = base
        .iter()
        .map(|(name, bytes)| (format!("base/{name}"), bytes.clone()))
        .collect::<Files>();
    files.insert("manifest.json".into(), manifest("0.5.5.5", &base).unwrap());
    validate_runtime(files.clone()).unwrap();
    let data = GameData::from_memory(files.clone());
    assert_eq!(data.passive_nodes().unwrap().len(), 1);
    assert_eq!(
        data.skill_gems().unwrap()[0].id,
        "Metadata/Items/Gems/SkillGemSpark"
    );
    assert_eq!(
        data.native_runtime().unwrap().unwrap().assembly_key,
        receipt["assembly_key"]
    );
    assert!(
        data.validate_calculation_admission()
            .unwrap_err()
            .to_string()
            .contains("native-profile-not-admitted")
    );
    files
        .get_mut("base/granted_effect_stat_sets.json")
        .unwrap()
        .push(b' ');
    assert!(
        GameData::from_memory_prechecked(files)
            .validate_manifest()
            .is_err()
    );
}

#[test]
fn native_runtime_coordinate_rounding_is_exact_and_null_slugs_are_not_definitions() {
    assert_eq!(tree::export_rounding(-19751.646484375).unwrap(), -19751.6);
    let (input, mut candidate, mut official) = tree_case();
    candidate.graph.groups[0].x_raw = 1.23456789;
    official["groups"]["1"]["x"] = json!(1.23457);
    official["nodes"]["62518"]["id"] = Value::Null;
    assert!(tree::project(&input, &candidate, &official, &"a".repeat(64), "0.5.5.5").is_ok());
    official["groups"]["1"]["x"] = json!(1.23458);
    assert!(tree::project(&input, &candidate, &official, &"a".repeat(64), "0.5.5.5").is_err());
}

pub(super) struct Scratch(pub(super) std::path::PathBuf);
static SCRATCH_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
impl Scratch {
    pub(super) fn new() -> Self {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = SCRATCH_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "pobr-runtime-{}-{suffix}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn native_runtime_external_json_is_pinned_bounded_and_duplicate_free() {
    let scratch = Scratch::new();
    let path = scratch.0.join("tree.json");
    let bytes = b"{\"a\":1,\"a\":2}";
    std::fs::write(&path, bytes).unwrap();
    assert!(
        inputs::pinned_json(&path, &sha256_hex(bytes), 100)
            .unwrap_err()
            .contains("duplicate")
    );
    assert!(inputs::pinned_json(&path, &sha256_hex(bytes), 3).is_err());
    assert!(
        inputs::pinned_json(&path, &"0".repeat(64), 100)
            .unwrap_err()
            .contains("sha256 mismatch")
    );
    std::fs::write(&path, b"{\"tree\":\"Default\"}").unwrap();
    let expected = sha256_hex(b"{\"tree\":\"Default\"}");
    assert_eq!(
        inputs::pinned_json(&path, &expected, 100).unwrap()["tree"],
        "Default"
    );
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn native_runtime_base_directory_is_held_until_completion_and_supplements_are_protected() {
    let scratch = Scratch::new();
    let source = scratch.0.join("facts");
    let supplement = scratch.0.join("supplement");
    std::fs::create_dir(&source).unwrap();
    std::fs::create_dir(&supplement).unwrap();
    let args = super::super::Args {
        input: source,
        out: scratch.0.join("candidate"),
        expected_manifest_sha256: "0".repeat(64),
        source_lock: "unused".into(),
        profile: Profile::ItemsActorsBc,
    };
    let mut output = output::Output::prepare(&args).unwrap();
    output.protect(&supplement).unwrap();
    output.reserve().unwrap();
    output
        .write_base(&BTreeMap::from([("example.json".into(), b"[]\n".to_vec())]))
        .unwrap();
    std::fs::rename(args.out.join("base"), args.out.join("saved-base")).unwrap();
    std::os::unix::fs::symlink(&supplement, args.out.join("base")).unwrap();
    assert!(
        output
            .write_new("manifest.json", b"{}")
            .unwrap_err()
            .contains("namespace changed")
    );
    assert!(!supplement.join("manifest.json").exists());
    assert!(!args.out.join("manifest.json").exists());
    let blocked_args = super::super::Args {
        out: supplement.join("inside"),
        ..args
    };
    let mut blocked = output::Output::prepare(&blocked_args).unwrap();
    assert!(blocked.protect(&supplement).is_err());
    assert!(!blocked_args.out.exists());
}
