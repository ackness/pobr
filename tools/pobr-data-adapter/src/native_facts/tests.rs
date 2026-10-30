use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};

use super::{Args, convert, json_bytes, model::Candidate, run, trust};
use crate::raw_source::sha256_hex;

const ACTUAL_MANIFEST: &[u8] =
    include_bytes!("../../tests/fixtures/native-facts/actual-a-manifest.json");
const SOURCE_LOCK: &[u8] = include_bytes!("../../../../pipeline/sources.lock.json");
static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
    manifest: Value,
    documents: BTreeMap<String, Value>,
}

impl Fixture {
    fn new() -> Self {
        // The worktree cache avoids platform /tmp symlinks, which the input verifier rejects.
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join(".cache/native-facts-tests")
            .join(format!(
                "{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(root.join("input/tables")).unwrap();
        fs::write(root.join("source-lock.json"), SOURCE_LOCK).unwrap();
        let mut manifest: Value = serde_json::from_slice(ACTUAL_MANIFEST).unwrap();
        let lock: Value = serde_json::from_slice(SOURCE_LOCK).unwrap();
        let selected = &lock["sources"]["official-client-0.5.5.5"];
        manifest["source_identity"]["source_lock_sha256"] = sha256_hex(SOURCE_LOCK).into();
        manifest["source_identity"]["selected_source_sha256"] =
            sha256_hex(&json_bytes(selected).unwrap()).into();
        let stats = [
            "zero",
            "negative",
            "third",
            "fourth",
            "recover_X_life_on_block",
        ];
        let passive_values = [0, -12, 2, 3, 10];
        let mut documents = BTreeMap::new();
        for (table, rows) in [
            ("Characters", vec![json!({"Id":"Druid", "Unknown":null})]),
            ("ClassPassiveSkillOverrides", vec![json!({"CharacterToOverrideFor":0, "SkillToOverride":1, "Override":2})]),
            ("Stats", stats.iter().map(|id| json!({"Id":id,"Unknown":null})).collect()),
            ("PassiveSkills", (0..3).map(|row| json!({
                "Id":(["stable-node", "absent-base", "replacement"][row]),
                "PassiveSkillGraphId":([31955,55194,57601][row]),
                "Stats":if row==0 {vec![0,1,2,3,4]} else {vec![]},
                "Stat1Value":0,"Stat2Value":-12,"Stat3Value":2,"Stat4Value":3,"Stat5Value":10,
                "Stat6Value":-7,"Stat7Value":0,
                "Unknown":{"null":null,"empty":[],"text":"","zero":0,"negative":-4,"ordered":[2,0,1]}
            })).collect()),
            ("PassiveSkillTrees", vec![json!({"Id":"Default", "PassiveSkillGraph":"Metadata/PassiveSkillGraph"})]),
            ("GrantedEffectStatSets", ["SnipePlayer", "SnipeExplosionPlayer", "SnipeFrozenExplosionPlayer"].iter()
                .map(|id| json!({"Id":id,"Label":0,"ConstantStats":[],"Unknown":null})).collect()),
            ("GrantedEffects", vec![json!({"Id":"SnipePlayer","StatSet":0,"AdditionalStatSets":[1,2], "Unknown":null})]),
            ("GrantedEffectQualityStats", vec![json!({"GrantedEffect":0,"ApplyToStatSets":[1,2,3],"Stats":[0],
                "StatsValuesPermille":[200],"AltApplyToStatSets":[0],"AltStats":[1,2],
                "AltStatValuesPermille":[0,-1000],"AddTypes":[],"AddMinionTypes":[],"Unknown":null})]),
        ] {
            documents.insert(format!("tables/{table}.json"), table_document(table, rows));
        }
        let links = json!([
            {"source_field":"StatSet","source_slot":null,"local_index_hypothesis":0,"stat_set":{"row":0,"id":"SnipePlayer"}},
            {"source_field":"AdditionalStatSets","source_slot":0,"local_index_hypothesis":1,"stat_set":{"row":1,"id":"SnipeExplosionPlayer"}},
            {"source_field":"AdditionalStatSets","source_slot":1,"local_index_hypothesis":2,"stat_set":{"row":2,"id":"SnipeFrozenExplosionPlayer"}}
        ]);
        let node =
            json!({"graph_id":31955,"radius":0,"position":0,"connections":[31955],"splines":[-9]});
        documents.insert("graph.json".into(), json!({"tree":{"row":0,"id":"Default"},
            "raw_resource":"Metadata/PassiveSkillGraph.psg", "header":[3,0],"skills_per_orbit":[1,12],
            "roots":[{"graph_id":31955,"unknown":0}],
            "groups":[{"group":0,"x":-1.25,"y":0.0,"flag":0,"unknown1":29,"unknown2":-5,"nodes":[node]}],
            "parsed_bytes":manifest["source_identity"]["raw_resources"]["Metadata/PassiveSkillGraph.psg"]["bytes"],
            "radius_semantics":"Raw orbit only", "warnings":[]}));
        let mut instance = node;
        instance["group"] = 0.into();
        instance["definition"] = json!({"row":0,"id":"stable-node"});
        documents.insert("passives.json".into(), json!({
            "definitions":(0..3).map(|row| json!({"definition":{"row":row,"id":(["stable-node","absent-base","replacement"][row])},
                "graph_id":([31955,55194,57601][row]),"in_default_graph":row==0,
                "stats":if row==0 {stats.iter().enumerate().map(|(i,id)|json!({"slot":i+1,"value_field":format!("Stat{}Value",i+1),
                    "stat":{"row":i,"id":id},"value":passive_values[i]})).collect::<Vec<_>>()} else {vec![]}})).collect::<Vec<_>>(),
            "default_graph_instances":[instance],
            "class_overrides":[{"row":0,"character":{"row":0,"id":"Druid"},"base":{"row":1,"id":"absent-base"},
                "replacement":{"row":2,"id":"replacement"},"base_graph_id":55194,"replacement_graph_id":57601,
                "base_in_default_graph":false,"replacement_in_default_graph":false}]
        }));
        documents.insert("skills.json".into(), json!({"effects":[{"effect":{"row":0,"id":"SnipePlayer"},"ordered_stat_sets":links}],
            "qualities":[{"row":0,"effect":{"row":0,"id":"SnipePlayer"},"ordered_stat_sets":links,
                "main":{"scope_raw":[1,2,3],"stats":[{"slot":0,"stat":{"row":0,"id":"zero"},"value_permille":200}]},
                "alternate":{"scope_raw":[0],"stats":[{"slot":0,"stat":{"row":1,"id":"negative"},"value_permille":0},
                    {"slot":1,"stat":{"row":2,"id":"third"},"value_permille":-1000}]}}]}));
        documents.insert("diagnostics.json".into(), json!({"scope":manifest["scope"],"diagnostics":[
            {"code":"quality_scope_out_of_range_under_local_index_hypothesis","row":0,"value":3,"scope_raw":[1,2,3],"ordered_stat_sets":links},
            {"code":"override_base_absent_from_default_graph","row":0,"base_graph_id":55194},
            {"code":"unavailable_relationship_target","field":"Label","occurrences":[{"row":0,"slot":null,"value":0}],"unknown":[]}
        ]}));
        manifest["summary"] = json!({"passive_definitions":3,"graph_instances":1,"graph_stat_entries":5,
            "class_overrides":1,"effects":1,"qualities":1});
        let mut fixture = Self {
            root,
            manifest,
            documents,
        };
        fixture.seal();
        fixture
    }

    fn seal(&mut self) {
        for (path, doc) in &self.documents {
            let bytes = json_bytes(doc).unwrap();
            fs::write(self.root.join("input").join(path), &bytes).unwrap();
            self.manifest["files"][path] = json!({"sha256":sha256_hex(&bytes),"bytes":bytes.len()});
        }
        fs::write(
            self.root.join("input/manifest.json"),
            json_bytes(&self.manifest).unwrap(),
        )
        .unwrap();
    }

    fn args(&self, out: &str) -> Args {
        Args {
            input: self.root.join("input"),
            source_lock: self.root.join("source-lock.json"),
            expected_manifest_sha256: sha256_hex(
                &fs::read(self.root.join("input/manifest.json")).unwrap(),
            ),
            out: self.root.join(out),
        }
    }

    fn rejects(&self, expected: &str) {
        let args = self.args("bad-output");
        let out = args.out.clone();
        let error = run(args).unwrap_err();
        assert!(
            error.contains(expected),
            "expected {expected:?}, got {error:?}"
        );
        assert!(!out.exists(), "invalid input created output");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn table_document(table: &str, rows: Vec<Value>) -> Value {
    let fields: Vec<_> = rows[0].as_object().unwrap().keys().cloned().collect();
    json!({"table":table,"raw_resource":format!("Data/Balance/{table}.datc64"),"row_key":"Ephemeral source row",
        "physical_fields":fields,"columns":fields.iter().enumerate().map(|(i,f)|json!({"source_column":i,
            "schema":{"name":f,"description":null},"physical_fields":[f]})).collect::<Vec<_>>(),
        "validation":{"rows":rows.len(),"schema_row_bytes":1,"actual_row_bytes":1,"warnings":[]},
        "rows":rows.into_iter().enumerate().map(|(i,v)|json!({"_row":i,"values":v})).collect::<Vec<_>>()})
}

#[test]
fn normalizes_stable_ids_without_losing_values_metadata_or_scope_discrepancy() {
    let f = Fixture::new();
    let verified = trust::verify(&f.args("unused")).unwrap();
    // Alter the filesystem after verification. Normalization must use retained verified bytes.
    fs::write(f.root.join("input/passives.json"), b"corrupt mutable input").unwrap();
    let c = convert::normalize(&verified).unwrap();
    assert_eq!(
        c.passive_order,
        ["stable-node", "absent-base", "replacement"]
    );
    let stats = &c.passives["stable-node"].stats;
    assert_eq!(
        stats.iter().map(|s| s.value).collect::<Vec<_>>(),
        [0, -12, 2, 3, 10]
    );
    assert_eq!(stats[4].stat_id, "recover_X_life_on_block");
    assert_eq!(c.instances[0].definition_id, "stable-node");
    assert_eq!(c.instances[0].splines, [-9]);
    assert_eq!(c.graph.groups[0].unknown2, -5);
    assert!(!c.class_relations[0].base_in_default_graph);
    assert!(!c.class_relations[0].replacement_in_default_graph);
    assert_eq!(c.qualities[0].main.scope_raw, [1, 2, 3]);
    assert_eq!(c.qualities[0].alternate.scope_raw, [0]);
    assert_eq!(
        c.qualities[0]
            .alternate
            .stats
            .iter()
            .map(|s| s.value_permille)
            .collect::<Vec<_>>(),
        [0, -1000]
    );
    assert_eq!(
        c.effects["SnipePlayer"]
            .ordered_stat_sets
            .iter()
            .map(|s| s.stat_set_id.as_str())
            .collect::<Vec<_>>(),
        [
            "SnipePlayer",
            "SnipeExplosionPlayer",
            "SnipeFrozenExplosionPlayer"
        ]
    );
    let raw = &c.passives["stable-node"].raw;
    let metadata = verified.documents[&raw.payload]
        .pointer(&raw.pointer)
        .unwrap();
    assert_eq!(
        metadata["Unknown"],
        json!({"null":null,"empty":[],"text":"","zero":0,"negative":-4,"ordered":[2,0,1]})
    );
    assert_eq!(metadata["Stat6Value"], -7);
    assert_eq!(c.stat_sets["SnipePlayer"].raw.pointer, "/rows/0/values");
}

#[test]
fn preserves_binary_exact_raw_coordinates() {
    // Actual-a PSG group 101: the default serde_json float parser changes this by one ULP.
    let mut f = Fixture::new();
    f.documents.get_mut("graph.json").unwrap()["groups"][0]["y"] = json!(445.30499267578125);
    f.seal();
    let input = trust::verify(&f.args("unused")).unwrap();
    let candidate = convert::normalize(&input).unwrap();
    assert_eq!(candidate.graph.groups[0].y_raw, 445.30499267578125);
    let output = json_bytes(&candidate).unwrap();
    let reread: Candidate = serde_json::from_slice(&output).unwrap();
    assert_eq!(reread.graph.groups[0].y_raw, 445.30499267578125);
}

#[test]
fn preserves_empty_channels_null_links_and_hypothesis_without_applying_them() {
    let mut f = Fixture::new();
    let effects =
        &mut f.documents.get_mut("tables/GrantedEffects.json").unwrap()["rows"][0]["values"];
    effects["StatSet"] = Value::Null;
    effects["AdditionalStatSets"] = json!([null, 2]);
    let link = json!([{"source_field":"AdditionalStatSets","source_slot":1,"local_index_hypothesis":7,
        "stat_set":{"row":2,"id":"SnipeFrozenExplosionPlayer"}}]);
    let skills = f.documents.get_mut("skills.json").unwrap();
    skills["effects"][0]["ordered_stat_sets"] = link.clone();
    skills["qualities"][0]["ordered_stat_sets"] = link;
    skills["qualities"][0]["main"] = json!({"scope_raw":[],"stats":[]});
    let quality = &mut f
        .documents
        .get_mut("tables/GrantedEffectQualityStats.json")
        .unwrap()["rows"][0]["values"];
    for field in ["ApplyToStatSets", "Stats", "StatsValuesPermille"] {
        quality[field] = json!([]);
    }
    f.seal();
    run(f.args("candidate")).unwrap();
    let c: Candidate =
        serde_json::from_slice(&fs::read(f.root.join("candidate/candidate.json")).unwrap())
            .unwrap();
    assert!(c.qualities[0].main.scope_raw.is_empty());
    assert!(c.qualities[0].main.stats.is_empty());
    assert_eq!(
        c.effects["SnipePlayer"].ordered_stat_sets[0].local_index_hypothesis,
        7
    );
    let raw = &c.effects["SnipePlayer"].raw;
    let source = &f.documents[&raw.payload].pointer(&raw.pointer).unwrap();
    assert!(source["StatSet"].is_null());
    assert_eq!(source["AdditionalStatSets"], json!([null, 2]));
}

#[test]
fn deterministic_output_has_complete_receipt_and_verbatim_diagnostics() {
    let f = Fixture::new();
    run(f.args("a")).unwrap();
    run(f.args("b")).unwrap();
    let receipt_bytes = fs::read(f.root.join("a/candidate-receipt.json")).unwrap();
    let receipt: Value = serde_json::from_slice(&receipt_bytes).unwrap();
    assert_eq!(receipt["status"], "partial-candidate");
    assert_eq!(receipt["production_ready"], false);
    for entry in fs::read_dir(f.root.join("a")).unwrap() {
        let name = entry.unwrap().file_name();
        let a = fs::read(f.root.join("a").join(&name)).unwrap();
        assert_eq!(a, fs::read(f.root.join("b").join(&name)).unwrap());
        if name != "candidate-receipt.json" {
            let expected = &receipt["files"][name.to_str().unwrap()];
            assert_eq!(expected["sha256"], sha256_hex(&a));
            assert_eq!(expected["bytes"], a.len());
        }
    }
    assert_eq!(
        fs::read(f.root.join("a/source-diagnostics.json")).unwrap(),
        fs::read(f.root.join("input/diagnostics.json")).unwrap()
    );
}

#[test]
fn verifies_external_pin_before_json_and_lock_exact_bytes() {
    assert_eq!(
        sha256_hex(ACTUAL_MANIFEST),
        "519364561e03e351b36c3aca0d9a986cf240e420183baa25d3c22ff1513f8257"
    );
    let f = Fixture::new();
    fs::write(f.root.join("input/manifest.json"), b"not json").unwrap();
    let mut args = f.args("bad-output");
    args.expected_manifest_sha256 = "0".repeat(64);
    assert!(run(args).unwrap_err().contains("external manifest"));
    assert!(!f.root.join("bad-output").exists());
    let f = Fixture::new();
    let mut bytes = SOURCE_LOCK.to_vec();
    bytes.push(b' ');
    fs::write(f.root.join("source-lock.json"), bytes).unwrap();
    f.rejects("source-lock bytes");
}

#[test]
fn binds_selected_lock_entry_decoder_and_exact_raw_pin_inventory() {
    for change in [
        "selected-hash",
        "decoder",
        "schema",
        "raw-pin",
        "extra-resource",
        "missing-resource",
    ] {
        let mut f = Fixture::new();
        let mut lock: Value = serde_json::from_slice(SOURCE_LOCK).unwrap();
        let source = &mut lock["sources"]["official-client-0.5.5.5"];
        match change {
            "selected-hash" => source["scope"] = "changed acquisition metadata".into(),
            "decoder" => source["decoder"]["revision"] = "0".repeat(40).into(),
            "schema" => source["decoder"]["schema_sha256"] = "0".repeat(64).into(),
            "raw-pin" => source["resources"]["Data/Balance/Stats.datc64"]["bytes"] = 1.into(),
            "extra-resource" => {
                source["resources"]["unexpected"] = json!({"bytes":1,"sha256":"0".repeat(64)})
            }
            "missing-resource" => {
                source["resources"]
                    .as_object_mut()
                    .unwrap()
                    .remove("Data/Balance/Stats.datc64");
            }
            _ => unreachable!(),
        }
        if change != "selected-hash" {
            f.manifest["source_identity"]["selected_source_sha256"] =
                sha256_hex(&json_bytes(source).unwrap()).into();
        }
        let bytes = json_bytes(&lock).unwrap();
        f.manifest["source_identity"]["source_lock_sha256"] = sha256_hex(&bytes).into();
        fs::write(f.root.join("source-lock.json"), bytes).unwrap();
        f.seal();
        assert!(
            run(f.args("bad-output")).is_err(),
            "accepted modified source lock {change}"
        );
        assert!(!f.root.join("bad-output").exists());
    }
}

#[test]
fn rejects_untrusted_identities_and_inventories_before_output() {
    for change in [
        "format",
        "status",
        "production_ready",
        "realm",
        "patch",
        "producer",
        "decoder",
        "schema",
        "raw-pin",
        "extra",
        "missing",
        "traversal",
    ] {
        let mut f = Fixture::new();
        match change {
            "format" => f.manifest["format"] = "other/v1".into(),
            "status" => f.manifest["status"] = "complete".into(),
            "production_ready" => f.manifest["production_ready"] = true.into(),
            "realm" => f.manifest["source_identity"]["realm"] = "china".into(),
            "patch" => f.manifest["source_identity"]["game_patch"] = "0.5.6".into(),
            "producer" => f.manifest["producer"]["RePoE/native_facts.py"] = "0".repeat(64).into(),
            "decoder" => f.manifest["decoder"]["pypoe_revision"] = "0".repeat(40).into(),
            "schema" => f.manifest["schema"]["compatibility_views"] = 1.into(),
            "raw-pin" => {
                f.manifest["source_identity"]["raw_resources"]["Data/Balance/Stats.datc64"]["bytes"] =
                    1.into()
            }
            "extra" => {
                f.manifest["files"]["extra.json"] = json!({"bytes":1,"sha256":"0".repeat(64)});
            }
            "missing" => {
                f.documents.remove("graph.json");
                f.manifest["files"]
                    .as_object_mut()
                    .unwrap()
                    .remove("graph.json");
            }
            "traversal" => {
                f.manifest["files"]["../outside.json"] = json!({"bytes":1,"sha256":"0".repeat(64)});
            }
            _ => unreachable!(),
        }
        f.seal();
        let args = f.args("bad-output");
        assert!(run(args).is_err(), "accepted forged {change}");
        assert!(!f.root.join("bad-output").exists(), "output on {change}");
    }
}

#[test]
fn rejects_payload_size_hash_missing_files_and_extra_paths() {
    for change in ["size", "hash", "missing", "extra-file", "extra-directory"] {
        let mut f = Fixture::new();
        match change {
            "size" => f.manifest["files"]["skills.json"]["bytes"] = 1.into(),
            "hash" => f.manifest["files"]["skills.json"]["sha256"] = "0".repeat(64).into(),
            _ => {}
        }
        fs::write(
            f.root.join("input/manifest.json"),
            json_bytes(&f.manifest).unwrap(),
        )
        .unwrap();
        match change {
            "missing" => fs::remove_file(f.root.join("input/skills.json")).unwrap(),
            "extra-file" => fs::write(f.root.join("input/tables/extra"), b"x").unwrap(),
            "extra-directory" => fs::create_dir(f.root.join("input/empty")).unwrap(),
            _ => {}
        }
        assert!(run(f.args("bad-output")).is_err(), "accepted {change}");
        assert!(!f.root.join("bad-output").exists());
    }
}

#[test]
fn rejects_duplicate_keys_at_every_json_boundary_including_nested_metadata() {
    for path in [
        "input/manifest.json",
        "source-lock.json",
        "input/tables/PassiveSkills.json",
        "input/diagnostics.json",
    ] {
        let mut f = Fixture::new();
        let bytes = fs::read(f.root.join(path)).unwrap();
        let mut bad = b"{\"duplicated\":1,\"duplicated\":2,".to_vec();
        bad.extend_from_slice(&bytes[1..]);
        fs::write(f.root.join(path), &bad).unwrap();
        if path == "source-lock.json" {
            f.manifest["source_identity"]["source_lock_sha256"] = sha256_hex(&bad).into();
        } else if let Some(payload) = path
            .strip_prefix("input/")
            .filter(|p| *p != "manifest.json")
        {
            f.manifest["files"][payload] = json!({"sha256":sha256_hex(&bad),"bytes":bad.len()});
        }
        if path != "input/manifest.json" {
            fs::write(
                f.root.join("input/manifest.json"),
                json_bytes(&f.manifest).unwrap(),
            )
            .unwrap();
        }
        f.rejects("duplicate JSON key");
    }
    assert!(
        trust::strict_json(br#"{"raw":{"a":null,"a":[]}}"#)
            .unwrap_err()
            .contains("duplicate JSON key")
    );
    assert!(trust::strict_json(br#"{"v":NaN}"#).is_err());
}

#[test]
fn rejects_truncation_bad_joins_and_counts_without_partial_output() {
    for change in [
        "passive", "value", "quality", "class", "edge", "count", "physical",
    ] {
        let mut f = Fixture::new();
        match change {
            "passive" => {
                f.documents.get_mut("passives.json").unwrap()["definitions"][0]["stats"]
                    .as_array_mut()
                    .unwrap()
                    .pop();
            }
            "value" => {
                f.documents.get_mut("passives.json").unwrap()["definitions"][0]["stats"][0]["value"] =
                    1.into()
            }
            "quality" => {
                f.documents.get_mut("skills.json").unwrap()["qualities"][0]["alternate"]["stats"]
                    .as_array_mut()
                    .unwrap()
                    .pop();
            }
            "class" => {
                f.documents.get_mut("passives.json").unwrap()["class_overrides"][0]["base_in_default_graph"] =
                    true.into()
            }
            "edge" => {
                f.documents.get_mut("passives.json").unwrap()["default_graph_instances"][0]["connections"]
                    [0] = 42.into()
            }
            "count" => f.manifest["summary"]["graph_stat_entries"] = 4.into(),
            "physical" => {
                f.documents.get_mut("tables/PassiveSkills.json").unwrap()["rows"][0]["values"]
                    .as_object_mut()
                    .unwrap()
                    .remove("Unknown");
            }
            _ => unreachable!(),
        }
        f.seal();
        assert!(
            run(f.args("bad-output")).is_err(),
            "accepted lossy {change}"
        );
        assert!(!f.root.join("bad-output").exists());
    }
}

#[test]
fn refuses_every_existing_output_and_preserves_previous_completion() {
    let f = Fixture::new();
    fs::create_dir(f.root.join("empty")).unwrap();
    assert!(
        run(f.args("empty"))
            .unwrap_err()
            .contains("existing output")
    );
    fs::write(f.root.join("file"), b"unchanged").unwrap();
    assert!(run(f.args("file")).unwrap_err().contains("existing output"));
    assert_eq!(fs::read(f.root.join("file")).unwrap(), b"unchanged");
    run(f.args("complete")).unwrap();
    let before = fs::read(f.root.join("complete/candidate-receipt.json")).unwrap();
    assert!(
        run(f.args("complete"))
            .unwrap_err()
            .contains("existing output")
    );
    assert_eq!(
        before,
        fs::read(f.root.join("complete/candidate-receipt.json")).unwrap()
    );
}

#[cfg(unix)]
#[test]
fn rejects_symlink_files_components_root_and_dangling_output() {
    use std::os::unix::fs::symlink;
    for kind in ["payload", "tables", "root", "lock", "out"] {
        let f = Fixture::new();
        let mut args = f.args("bad-output");
        match kind {
            "payload" => {
                fs::rename(f.root.join("input/skills.json"), f.root.join("skills.json")).unwrap();
                symlink(f.root.join("skills.json"), f.root.join("input/skills.json")).unwrap();
            }
            "tables" => {
                fs::rename(f.root.join("input/tables"), f.root.join("tables")).unwrap();
                symlink(f.root.join("tables"), f.root.join("input/tables")).unwrap();
            }
            "root" => {
                symlink(f.root.join("input"), f.root.join("alias")).unwrap();
                args.input = f.root.join("alias");
            }
            "lock" => {
                symlink(f.root.join("source-lock.json"), f.root.join("lock-alias")).unwrap();
                args.source_lock = f.root.join("lock-alias");
            }
            "out" => {
                symlink(f.root.join("nonexistent"), &args.out).unwrap();
            }
            _ => unreachable!(),
        }
        assert!(run(args).is_err(), "accepted {kind} symlink");
        assert!(!f.root.join("bad-output/candidate-receipt.json").exists());
    }
}

#[test]
fn independent_arguments_are_explicit_and_reject_legacy_modes_and_duplicates() {
    let parse = |s: &str| super::parse_args(s.split_whitespace().map(String::from));
    assert!(
        parse("--native-facts input --expected-manifest-sha256 hash --source-lock lock --out out")
            .is_ok()
    );
    for s in [
        "--native-facts input --out out",
        "--native-facts input --patch x",
        "--out a --out b",
        "--native-facts --source-lock lock",
        "--raw x",
    ] {
        assert!(parse(s).is_err(), "accepted {s}");
    }
}
