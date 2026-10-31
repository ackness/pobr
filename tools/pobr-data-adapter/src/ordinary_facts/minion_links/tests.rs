use std::collections::BTreeMap;
use std::fs;

use pobr_data::catalog::actors::GrantedEffectMinionsDef;
use pobr_data::catalog::skills::GrantedEffectDef;
use serde_json::{Value, json};

use super::super::stat_set_codec::tests::table;
use super::super::tests::Fixture;
use super::super::{Args, CodecArgs, Domain, parse_args, reader, run};
use crate::raw_source::sha256_hex;

const SOURCE: &str = "legacy/source.json";
const POLICY: &str = "policy/policy.json";

pub(in crate::ordinary_facts) fn fixture() -> Fixture {
    let mut f = Fixture::new();
    let old: Vec<_> = f
        .documents
        .keys()
        .filter(|p| p.starts_with("tables/"))
        .cloned()
        .collect();
    f.documents.retain(|p, _| p == "source-lock.json");
    f.documents.insert(
        "tables/GrantedEffects.json".into(),
        table(
            "GrantedEffects",
            &[
                ("Id", "string", false, None),
                ("IgnoreMinionTypes", "bool", false, None),
                ("MinionType", "i32", false, None),
            ],
            json!([
                {"Id":"OutsideTrue ","IgnoreMinionTypes":true,"MinionType":999},
                {"Id":"FalseToTrue","IgnoreMinionTypes":true,"MinionType":999},
                {"Id":"TrueToFalse","IgnoreMinionTypes":false,"MinionType":999},
                {"Id":"Empty","IgnoreMinionTypes":false,"MinionType":999},
                {"Id":"OutsideFalse","IgnoreMinionTypes":false,"MinionType":999}
            ]),
        ),
    );
    f.documents.insert(
        "tables/MonsterVarieties.json".into(),
        table(
            "MonsterVarieties",
            &[
                ("Id", "string", false, None),
                ("Name", "string", false, None),
            ],
            json!([{"Id":"Unused variety ","Name":"Not admitted"},{"Id":"VarZ ","Name":"Target"}]),
        ),
    );
    f.seal();
    for path in old {
        if !f.documents.contains_key(&path) {
            fs::remove_file(f.root.join("input").join(path)).unwrap();
        }
    }
    fs::create_dir(f.root.join("legacy")).unwrap();
    fs::create_dir(f.root.join("policy")).unwrap();
    let mut p: Value = serde_json::from_slice(include_bytes!(
        "../../../../../pipeline/actors/0.5.5.5-minion-links-policy.json"
    ))
    .unwrap();
    p["alias_catalog"]["aliases"] = json!([
        {"id":"AliasA","monster_variety_id":"VarZ ","source_catalog_row":77},
        {"id":"AliasB","monster_variety_id":"VarZ ","source_catalog_row":2},
        {"id":"UnusedAlias ","monster_variety_id":"Unused variety ","source_catalog_row":900}
    ]);
    let first = json!({"effect_id":"TrueToFalse","ignore_minion_types":true,
        "minion_list":["AliasB","AliasA","AliasB"],"add_minion_list":["AliasA"],
        "minion_uses":["Weapon 1","FutureSlot"],"minion_has_item_set":true,
        "unknown/~field":{"constructor":"Unknown","nested":[null,0,false]}});
    let second = json!({"effect_id":"FalseToTrue","minion_list":[],"minion_uses":[],"minion_has_item_set":false});
    let third = json!({"effect_id":"Empty","future":null});
    write(
        &f,
        SOURCE,
        &json!({"_meta":p["source"]["metadata"],"entries":[first,second,third],"unknown_top":{"keep":true}}),
    );
    write(&f, POLICY, &p);
    reseal(&mut f);
    f
}
fn doc(f: &Fixture, path: &str) -> Value {
    crate::read_json(&f.root.join(path)).unwrap()
}
fn write(f: &Fixture, path: &str, value: &Value) {
    let mut bytes = b" \n\t".to_vec();
    bytes.extend(serde_json::to_vec(value).unwrap());
    bytes.extend(b"\n\n");
    fs::write(f.root.join(path), bytes).unwrap();
}
fn reseal(f: &mut Fixture) {
    f.seal();
    let mut p = doc(f, POLICY);
    p["target"]["facts_manifest_sha256"] = f.args().expected_manifest_sha256.into();
    p["source"]["sha256"] = sha256_hex(&fs::read(f.root.join(SOURCE)).unwrap()).into();
    write(f, POLICY, &p);
}
pub(in crate::ordinary_facts) fn args(f: &Fixture) -> Args {
    Args {
        domain: Domain::GrantedEffectMinions,
        codec: Some(CodecArgs {
            source: f.root.join(SOURCE),
            source_sha256: sha256_hex(&fs::read(f.root.join(SOURCE)).unwrap()),
            policy: f.root.join(POLICY),
            policy_sha256: sha256_hex(&fs::read(f.root.join(POLICY)).unwrap()),
        }),
        ..f.args()
    }
}
fn output(f: &Fixture) -> Value {
    doc(f, "out/overlay/granted_effect_minions.json")
}
fn receipt(f: &Fixture) -> Value {
    doc(f, "out/producer-receipt.json")
}
fn rejects(f: &Fixture, expected: &str) {
    let e = run(args(f)).unwrap_err();
    assert!(e.contains(expected), "expected {expected:?}; got {e:?}");
    assert!(!f.root.join("out").exists());
}

#[test]
fn target_boolean_changes_both_ways_without_touching_curated_fields_or_absence() {
    let f = fixture();
    run(args(&f)).unwrap();
    let out = output(&f);
    let source = doc(&f, SOURCE);
    let dto: GrantedEffectMinionsDef = serde_json::from_value(out.clone()).unwrap();
    assert_eq!(dto.entries.len(), 3);
    assert!(!dto.entries[0].ignore_minion_types);
    assert!(out["entries"][0].get("ignore_minion_types").is_none());
    assert!(dto.entries[1].ignore_minion_types);
    assert_eq!(out["entries"][1]["ignore_minion_types"], true);
    assert_eq!(out["entries"][2], source["entries"][2]);
    assert_eq!(dto.entries[0].minion_list, ["AliasB", "AliasA", "AliasB"]);
    assert_eq!(dto.entries[0].add_minion_list, ["AliasA"]);
    assert_eq!(dto.entries[0].minion_uses, ["Weapon 1", "FutureSlot"]);
    assert!(dto.entries[0].minion_has_item_set);
    assert!(out["entries"][1].get("add_minion_list").is_none());
    assert_eq!(out["entries"][1]["minion_list"], json!([]));
    assert_eq!(out["entries"][1]["minion_has_item_set"], false);
    for (old, new) in source["entries"]
        .as_array()
        .unwrap()
        .iter()
        .zip(out["entries"].as_array().unwrap())
    {
        for (field, value) in old.as_object().unwrap() {
            if field != super::OWNED {
                assert_eq!(new[field], *value, "{field}");
            }
        }
        for field in new.as_object().unwrap().keys() {
            if field != super::OWNED {
                assert!(old.get(field).is_some(), "invented field {field}");
            }
        }
    }
    for (field, value) in source["_meta"].as_object().unwrap() {
        assert_eq!(out["_meta"][field], *value);
    }
    assert_eq!(out["unknown_top"], source["unknown_top"]);
    let r = receipt(&f);
    assert_eq!(
        r["conversion"]["changed_target_booleans"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn receipt_preserves_ordered_references_aliases_authoring_scope_and_hashes() {
    let f = fixture();
    run(args(&f)).unwrap();
    let r = receipt(&f);
    let c = &r["conversion"];
    let source = doc(&f, SOURCE);
    let out = output(&f);
    let p = doc(&f, POLICY);
    assert_eq!(c["reference_occurrences"], 4);
    let refs = &c["source_records"][0]["reference_occurrences"];
    assert_eq!(refs.as_array().unwrap().len(), 4);
    for (i, policy_row) in [1, 0, 1, 0].into_iter().enumerate() {
        assert_eq!(refs[i]["policy_row"], policy_row);
        assert_eq!(refs[i]["variety"]["id"], "VarZ ");
        assert_eq!(refs[i]["variety"]["source_row"], 1);
    }
    assert_eq!(refs[1]["source_catalog_row"], 77);
    assert_eq!(refs[2]["position"], 2);
    assert_eq!(refs[3]["position"], 0);
    assert_eq!(refs[3]["field"], "add_minion_list");
    assert_eq!(c["alias_inventory"][2]["unused"], true);
    assert_eq!(c["alias_inventory"][2]["alias"]["id"], "UnusedAlias ");
    assert_eq!(
        c["alias_authoring"]["declaration"],
        p["alias_catalog"]["source_declaration"]
    );
    assert_eq!(c["alias_authoring"]["file_read_by_producer"], false);
    assert_eq!(
        c["alias_authoring"]["file_hash_verified_by_producer"],
        false
    );
    assert_eq!(
        c["outside_sidecar"]["rows"],
        json!({"count":2,"ranges":[[0,1],[4,5]]})
    );
    assert_eq!(
        c["outside_sidecar"]["true_effects"],
        json!([{"id":"OutsideTrue ","raw":true,"source_row":0}])
    );
    for (i, record) in c["source_records"].as_array().unwrap().iter().enumerate() {
        assert_eq!(
            record["source_entry_sha256"],
            sha256_hex(&reader::json_bytes(&source["entries"][i]).unwrap())
        );
        assert_eq!(
            record["output_entry_sha256"],
            sha256_hex(&reader::json_bytes(&out["entries"][i]).unwrap())
        );
        for retained in record["retained_fields"].as_array().unwrap() {
            let value = source
                .pointer(retained["source_pointer"].as_str().unwrap())
                .unwrap();
            assert_eq!(
                retained["value_sha256"],
                sha256_hex(&reader::json_bytes(value).unwrap())
            );
        }
    }
    assert_eq!(c["source_records"][0]["effect_source_row"], 2);
    assert_eq!(c["source_records"][0]["owned_field"]["present"], false);
    assert_eq!(c["source_records"][0]["owned_field"]["value"], Value::Null);
    assert_eq!(
        c["source_records"][1]["owned_field"]["legacy_present"],
        false
    );
    assert_eq!(
        c["unconsumed_source"][0]["consumed_columns"],
        json!(["Id", "IgnoreMinionTypes"])
    );
    assert_eq!(
        c["unconsumed_source"][0]["columns"][0]["schema"]["name"],
        "MinionType"
    );
}

#[test]
fn unprovided_alias_authoring_catalog_is_a_declaration_not_an_input() {
    let f = fixture();
    let mut p = doc(&f, POLICY);
    let d = &mut p["alias_catalog"]["source_declaration"];
    d["snapshot_version"] = json!("not-provided");
    d["path"] = json!("data/not-provided/overlay/minions.json");
    d["sha256"] = json!("1".repeat(64));
    d["metadata"]["regen_command"] =
        json!("declared extraction --out data/not-provided/overlay/minions.json");
    let declaration = d.clone();
    write(&f, POLICY, &p);
    run(args(&f)).unwrap();
    assert_eq!(
        receipt(&f)["conversion"]["alias_authoring"]["declaration"],
        declaration
    );
    assert!(
        !f.root
            .join("data/not-provided/overlay/minions.json")
            .exists()
    );
}

#[test]
fn exact_used_and_unused_whitespace_ids_are_never_trimmed() {
    let mut f = fixture();
    let mut source = doc(&f, SOURCE);
    let mut p = doc(&f, POLICY);
    f.values("GrantedEffects", 2)["Id"] = json!("TrueToFalse ");
    source["entries"][0]["effect_id"] = json!("TrueToFalse ");
    p["alias_catalog"]["aliases"][1]["id"] = json!("AliasB ");
    source["entries"][0]["minion_list"] = json!(["AliasB ", "AliasA", "AliasB "]);
    write(&f, SOURCE, &source);
    write(&f, POLICY, &p);
    reseal(&mut f);
    run(args(&f)).unwrap();
    assert_eq!(output(&f)["entries"][0]["effect_id"], "TrueToFalse ");
    assert_eq!(
        output(&f)["entries"][0]["minion_list"],
        source["entries"][0]["minion_list"]
    );
    let f = fixture();
    let mut p = doc(&f, POLICY);
    p["alias_catalog"]["aliases"][0]["monster_variety_id"] = json!("VarZ");
    write(&f, POLICY, &p);
    rejects(&f, "missing target variety");
}

#[test]
fn malformed_source_values_metadata_and_physical_keys_fail_before_reserve() {
    for value in [Value::Null, json!(0), json!("true"), json!([]), json!({})] {
        for row in [0, 2] {
            let mut f = fixture();
            f.values("GrantedEffects", row)["IgnoreMinionTypes"] = value.clone();
            reseal(&mut f);
            rejects(&f, "IgnoreMinionTypes");
        }
    }
    for mode in [
        "missing",
        "schema-type",
        "schema-array",
        "schema-ref",
        "physical-key",
        "duplicate-id",
        "empty-id",
    ] {
        let mut f = fixture();
        match mode {
            "missing" => {
                f.values("GrantedEffects", 2)
                    .as_object_mut()
                    .unwrap()
                    .remove("IgnoreMinionTypes");
            }
            "schema-type" => {
                f.documents.get_mut("tables/GrantedEffects.json").unwrap()["columns"][1]["schema"]
                    ["type"] = json!("i32")
            }
            "schema-array" => {
                f.documents.get_mut("tables/GrantedEffects.json").unwrap()["columns"][1]["schema"]
                    ["array"] = json!(true)
            }
            "schema-ref" => {
                f.documents.get_mut("tables/GrantedEffects.json").unwrap()["columns"][1]["schema"]
                    ["references"] = json!({"table":"MonsterVarieties"})
            }
            "physical-key" => {
                f.documents.get_mut("tables/GrantedEffects.json").unwrap()["rows"][1]["_row"] =
                    json!(0)
            }
            "duplicate-id" => f.values("MonsterVarieties", 0)["Id"] = json!("VarZ "),
            _ => f.values("GrantedEffects", 0)["Id"] = json!(""),
        };
        reseal(&mut f);
        rejects(&f, "");
    }
}

#[test]
fn invalid_curated_entries_alias_keys_and_dangling_bindings_fail() {
    for mode in [
        "duplicate-entry",
        "empty-entry",
        "missing-effect",
        "missing-alias",
        "null-list",
        "bad-bool",
        "duplicate-alias",
        "empty-alias",
        "empty-variety",
        "unused-dangling-variety",
    ] {
        let mut f = fixture();
        let mut s = doc(&f, SOURCE);
        let mut p = doc(&f, POLICY);
        match mode {
            "duplicate-entry" => s["entries"][1]["effect_id"] = json!("TrueToFalse"),
            "empty-entry" => s["entries"][0]["effect_id"] = json!(""),
            "missing-effect" => s["entries"][0]["effect_id"] = json!("NoEffect"),
            "missing-alias" => s["entries"][0]["minion_list"] = json!(["NoAlias"]),
            "null-list" => s["entries"][0]["add_minion_list"] = Value::Null,
            "bad-bool" => s["entries"][0]["ignore_minion_types"] = json!(1),
            "duplicate-alias" => p["alias_catalog"]["aliases"][1]["id"] = json!("AliasA"),
            "empty-alias" => p["alias_catalog"]["aliases"][0]["id"] = json!(""),
            "empty-variety" => p["alias_catalog"]["aliases"][0]["monster_variety_id"] = json!(""),
            _ => p["alias_catalog"]["aliases"][2]["monster_variety_id"] = json!("NoVariety"),
        };
        write(&f, SOURCE, &s);
        write(&f, POLICY, &p);
        reseal(&mut f);
        rejects(&f, "");
    }
}

#[test]
fn strict_policy_schema_identity_and_pin_guards_precede_output() {
    for mode in [
        "schema",
        "projection",
        "owned",
        "domain",
        "file",
        "dto",
        "patch",
        "manifest",
        "source-sha",
        "source-meta",
        "source-path",
        "declaration-sha",
        "declaration-role",
        "unknown",
    ] {
        let f = fixture();
        let mut p = doc(&f, POLICY);
        match mode {
            "schema" => p["schema"] = json!("future"),
            "projection" => p["projection_schema"] = json!("summon-law"),
            "owned" => p["owned_fields"] = json!(["ignore_minion_types", "ignore_minion_types"]),
            "domain" => p["catalog"]["domain"] = json!("minions"),
            "file" => p["catalog"]["output_file"] = json!("minions.json"),
            "dto" => p["catalog"]["output_schema"] = json!("minions/v1"),
            "patch" => p["target"]["game_patch"] = json!("old"),
            "manifest" => p["target"]["facts_manifest_sha256"] = json!("0".repeat(64)),
            "source-sha" => p["source"]["sha256"] = json!("0".repeat(64)),
            "source-meta" => p["source"]["metadata"]["schema"] = json!("minions/v1"),
            "source-path" => p["source"]["path"] = json!("data/wrong.json"),
            "declaration-sha" => {
                p["alias_catalog"]["source_declaration"]["sha256"] = json!("not hex")
            }
            "declaration-role" => {
                p["alias_catalog"]["source_declaration"]["role"] = json!("verified runtime input")
            }
            _ => p["unknown"] = json!(true),
        };
        write(&f, POLICY, &p);
        rejects(&f, "");
    }
    for path in [SOURCE, POLICY] {
        let f = fixture();
        let a = args(&f);
        fs::write(f.root.join(path), b"not JSON").unwrap();
        assert!(run(a).unwrap_err().contains("SHA256 mismatch"));
        assert!(!f.root.join("out").exists());
        let mut f = fixture();
        fs::write(f.root.join(path), b"{\"duplicate\":1,\"duplicate\":2}").unwrap();
        if path == SOURCE {
            reseal(&mut f);
        }
        rejects(&f, "duplicate");
    }
    let mut f = fixture();
    let mut s = doc(&f, SOURCE);
    s["_meta"]["vendor_commit"] = json!("0".repeat(40));
    write(&f, SOURCE, &s);
    reseal(&mut f);
    rejects(&f, "metadata differs");

    let mut f = fixture();
    let mut s = doc(&f, SOURCE);
    let mut p = doc(&f, POLICY);
    s["_meta"]["target_fields"] = json!({"original":true});
    p["source"]["metadata"] = s["_meta"].clone();
    write(&f, SOURCE, &s);
    write(&f, POLICY, &p);
    reseal(&mut f);
    rejects(&f, "conflicts with generated provenance key");
}

#[test]
fn seven_files_three_payloads_and_original_auxiliary_bytes_replay() {
    let f = fixture();
    run(args(&f)).unwrap();
    run(Args {
        out: f.root.join("again"),
        ..args(&f)
    })
    .unwrap();
    let r = receipt(&f);
    assert_eq!(r["files"].as_object().unwrap().len(), 6);
    assert_eq!(
        r["input"]["selected_payloads"].as_object().unwrap().len(),
        3
    );
    assert_eq!(r["complete_runtime"], false);
    assert_eq!(r["adoption"], "not-adopted");
    assert_eq!(
        doc(&f, "out/_domain_candidate.json")["domain"],
        "ordinary-granted-effect-minions"
    );
    assert!(!f.root.join("out/base").exists());
    assert!(!f.root.join("out/manifest.json").exists());
    for (input, output) in [
        (SOURCE, "legacy-minion-links.json"),
        (POLICY, "codec-policy.json"),
    ] {
        assert_eq!(
            fs::read(f.root.join(input)).unwrap(),
            fs::read(f.root.join("out").join(output)).unwrap()
        );
    }
    for (path, identity) in r["files"].as_object().unwrap() {
        let b = fs::read(f.root.join("out").join(path)).unwrap();
        assert_eq!(identity["sha256"], sha256_hex(&b));
        assert_eq!(identity["bytes"], b.len());
        assert_eq!(b, fs::read(f.root.join("again").join(path)).unwrap());
    }
    assert_eq!(
        fs::read(f.root.join("out/producer-receipt.json")).unwrap(),
        fs::read(f.root.join("again/producer-receipt.json")).unwrap()
    );
}

#[test]
fn flags_paths_manifest_payload_version_and_overwrite_are_guarded() {
    let cli = [
        "--ordinary-facts",
        "F",
        "--facts-manifest-sha256",
        "sha",
        "--version",
        "0.5.5.5",
        "--out",
        "fresh",
        "--ordinary-domain",
        "granted-effect-minions",
        "--codec-source",
        "source",
        "--codec-source-sha256",
        "sha",
        "--codec-policy",
        "policy",
        "--codec-policy-sha256",
        "sha",
    ];
    assert!(matches!(
        crate::parse_args_from(cli.into_iter().map(String::from)),
        Ok(crate::Mode::OrdinaryFacts(_))
    ));
    for i in (10..cli.len()).step_by(2) {
        let mut a = cli.to_vec();
        a.drain(i..i + 2);
        assert!(parse_args(a.into_iter().map(String::from)).is_err());
        assert!(
            parse_args(
                cli.into_iter()
                    .chain([cli[i], "duplicate"])
                    .map(String::from)
            )
            .is_err()
        );
    }
    for pair in [
        ["--branch-codec", "path"],
        ["--branch-codec-sha256", "sha"],
        ["--quality-policy", "path"],
        ["--quality-policy-sha256", "sha"],
        ["--raw", "old"],
        ["--native-facts", "F"],
        ["--ordinary-domain", "minions"],
    ] {
        assert!(parse_args(cli.into_iter().chain(pair).map(String::from)).is_err());
    }
    assert!(parse_args(cli.into_iter().chain(["--codec-source"]).map(String::from)).is_err());
    let f = fixture();
    for parent in ["input", "legacy", "policy"] {
        let out = f.root.join(parent).join("forbidden");
        assert!(
            run(Args {
                out: out.clone(),
                ..args(&f)
            })
            .is_err()
        );
        assert!(!out.exists());
    }
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data")
        .canonicalize()
        .unwrap()
        .join(f.root.file_name().unwrap());
    assert!(
        run(Args {
            out: out.clone(),
            ..args(&f)
        })
        .is_err()
    );
    assert!(!out.exists());
    for mode in ["manifest", "version"] {
        let mut a = args(&f);
        if mode == "manifest" {
            a.expected_manifest_sha256 = "0".repeat(64);
        } else {
            a.version = "old".into();
        }
        assert!(run(a).is_err());
        assert!(!f.root.join("out").exists());
    }
    std::os::unix::fs::symlink(f.root.join("absent"), f.root.join("out")).unwrap();
    assert!(run(args(&f)).is_err());
    fs::remove_file(f.root.join("out")).unwrap();
    run(args(&f)).unwrap();
    let before = fs::read(f.root.join("out/producer-receipt.json")).unwrap();
    assert!(run(args(&f)).is_err());
    assert_eq!(
        before,
        fs::read(f.root.join("out/producer-receipt.json")).unwrap()
    );
    let f = fixture();
    fs::write(f.root.join("input/tables/GrantedEffects.json"), b"{}").unwrap();
    rejects(&f, "");
}

#[test]
fn ordinary_skills_outside_sidecar_flags_are_not_overlaid_or_synthesized() {
    let mut f = fixture();
    let all: BTreeMap<String, Value> = serde_json::from_slice(include_bytes!(
        "../../../tests/fixtures/ordinary-facts/skill-tables.json"
    ))
    .unwrap();
    for (name, doc) in all {
        f.documents.insert(format!("tables/{name}.json"), doc);
    }
    // Two curated flags change in opposite directions; an outside support remains true in base.
    f.values("GrantedEffects", 0)["IgnoreMinionTypes"] = json!(true);
    let mut source = doc(&f, SOURCE);
    source["entries"][0]["effect_id"] = json!("MinionPlayer");
    source["entries"][1]["effect_id"] = json!("AttackPlayer");
    source["entries"][2]["effect_id"] = json!("DevPlayer");
    write(&f, SOURCE, &source);
    reseal(&mut f);
    run(Args {
        domain: Domain::Skills,
        out: f.root.join("skills"),
        ..f.args()
    })
    .unwrap();
    run(args(&f)).unwrap();
    let base: Vec<GrantedEffectDef> =
        serde_json::from_value(doc(&f, "skills/base/granted_effects.json")).unwrap();
    let sidecar: GrantedEffectMinionsDef = serde_json::from_value(output(&f)).unwrap();
    let support = base.iter().find(|e| e.id == "SupportPlayer").unwrap();
    assert!(support.ignore_minion_types);
    assert!(
        sidecar
            .entries
            .iter()
            .all(|e| e.effect_id != "SupportPlayer")
    );
    for entry in &sidecar.entries {
        assert_eq!(
            entry.ignore_minion_types,
            base.iter()
                .find(|e| e.id == entry.effect_id)
                .unwrap()
                .ignore_minion_types
        );
    }
    assert_eq!(
        receipt(&f)["conversion"]["outside_sidecar"]["true_effects"],
        json!([{"id":"SupportPlayer","raw":true,"source_row":1}])
    );
}

#[test]
fn prior_domains_keep_files_and_receipts_without_minion_links_sidecars() {
    for (domain, bytes, count) in [
        (Domain::StatsMods, None, 5),
        (
            Domain::BaseItems,
            Some(
                include_bytes!("../../../tests/fixtures/ordinary-facts/base-item-tables.json")
                    .as_slice(),
            ),
            4,
        ),
        (
            Domain::Skills,
            Some(
                include_bytes!("../../../tests/fixtures/ordinary-facts/skill-tables.json")
                    .as_slice(),
            ),
            8,
        ),
        (Domain::StatSetCodec, None, 5),
        (Domain::GemEffects, None, 6),
        (Domain::MonsterResistances, None, 4),
        (Domain::TotemVarieties, None, 4),
        (Domain::MonsterScaling, None, 4),
        (Domain::SkillStatMap, None, 6),
        (Domain::SkillOverrides, None, 7),
        (Domain::Minions, None, 6),
        (Domain::Spectres, None, 6),
    ] {
        use super::super::actors::Catalog;
        let mut f = match domain {
            Domain::StatSetCodec => super::super::stat_set_codec::tests::fixture(),
            Domain::GemEffects => super::super::gem_effects::tests::fixture(),
            Domain::MonsterResistances => super::super::monster_resistances::tests::fixture(),
            Domain::TotemVarieties => super::super::totem_varieties::tests::fixture(),
            Domain::MonsterScaling => super::super::monster_scaling::tests::fixture(),
            Domain::SkillStatMap => super::super::skill_stat_map::tests::fixture(),
            Domain::SkillOverrides => super::super::skill_overrides::tests::fixture(),
            Domain::Minions => super::super::actors::tests::fixture(Catalog::Minions),
            Domain::Spectres => super::super::actors::tests::fixture(Catalog::Spectres),
            _ => Fixture::new(),
        };
        if let Some(bytes) = bytes {
            let tables: BTreeMap<String, Value> = serde_json::from_slice(bytes).unwrap();
            f.documents.retain(|p, _| p == "source-lock.json");
            f.documents.extend(
                tables
                    .into_iter()
                    .map(|(t, d)| (format!("tables/{t}.json"), d)),
            );
            f.seal();
        }
        let a = || match domain {
            Domain::StatSetCodec => super::super::stat_set_codec::tests::args(&f),
            Domain::GemEffects => super::super::gem_effects::tests::args(&f),
            Domain::SkillStatMap => super::super::skill_stat_map::tests::args(&f),
            Domain::SkillOverrides => super::super::skill_overrides::tests::args(&f),
            Domain::Minions => super::super::actors::tests::args(&f, Catalog::Minions),
            Domain::Spectres => super::super::actors::tests::args(&f, Catalog::Spectres),
            _ => Args { domain, ..f.args() },
        };
        run(a()).unwrap();
        let links = fixture();
        run(args(&links)).unwrap();
        run(Args {
            out: f.root.join("again"),
            ..a()
        })
        .unwrap();
        let r = receipt(&f);
        assert_eq!(r.as_object().unwrap().len(), 10);
        assert_eq!(r["files"].as_object().unwrap().len(), count);
        assert!(!f.root.join("out/legacy-minion-links.json").exists());
        for path in r["files"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .chain(["producer-receipt.json"])
        {
            assert_eq!(
                fs::read(f.root.join("out").join(path)).unwrap(),
                fs::read(f.root.join("again").join(path)).unwrap(),
                "{domain:?}/{path}"
            );
        }
    }
}
