use super::super::tests::Fixture;
use super::super::{Args, BranchCodecArgs, Domain, run};
use super::*;
use crate::native_facts::reader;
use std::fs;

fn write(path: &std::path::Path, doc: &Value) -> String {
    let bytes = reader::json_bytes(doc).unwrap();
    fs::write(path, &bytes).unwrap();
    sha256_hex(&bytes)
}
fn fixture() -> (Fixture, Args, Value, Value) {
    let mut f = Fixture::new();
    let all: BTreeMap<String, Value> = serde_json::from_slice(include_bytes!(
        "../../../tests/fixtures/ordinary-facts/catalyst-tables.json"
    ))
    .unwrap();
    f.documents.retain(|k, _| k == "source-lock.json");
    let mut extra = json!({});
    for (name, doc) in all {
        if TABLES.contains(&name.as_str()) {
            f.documents.insert(format!("tables/{name}.json"), doc);
        } else {
            extra[&name] = doc;
        }
    }
    f.seal();
    let dir = f.root.join("extra");
    fs::create_dir(&dir).unwrap();
    let policy_dir = f.root.join("policy");
    fs::create_dir(&policy_dir).unwrap();
    let source = f.documents["source-lock.json"]["sources"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap()
        .clone();
    let mut source = source;
    source.as_object_mut().unwrap().remove("profile");
    source["resources"] = json!({});
    for name in extra.as_object().unwrap().keys() {
        source["resources"][format!("Data/Balance/{name}.datc64")] =
            json!({"bytes":10,"sha256":"a".repeat(64)});
    }
    let lock = json!({"schema_version":1,"sources":{"selected":source}});
    let mut m = json!({"format":"repoe-native-tables/v1","status":"partial-candidate","production_ready":false,
        "schema":f.manifest["schema"],"decoder":f.manifest["decoder"],
        "producer":serde_json::from_slice::<Value>(include_bytes!("../../../tests/fixtures/ordinary-facts/selected-tables-producer.json")).unwrap(),
        "source_identity":f.manifest["source_identity"],"summary":{"tables":{}},"files":{}});
    m["source_identity"]["source"] = json!("selected");
    m["source_identity"]["selected_source_sha256"] =
        json!(sha256_hex(&reader::json_bytes(&source).unwrap()));
    m["source_identity"]["raw_resources"] = source["resources"].clone();
    let lock_hash = write(&dir.join("source-lock.json"), &lock);
    m["source_identity"]["source_lock_sha256"] = json!(lock_hash);
    m["files"]["source-lock.json"] = json!({"bytes":fs::metadata(dir.join("source-lock.json")).unwrap().len(),"sha256":lock_hash});
    let mut args = Args {
        domain: Domain::Catalysts,
        supplement: Some(BranchCodecArgs {
            path: dir.clone(),
            sha256: String::new(),
        }),
        catalog_policy: Some(BranchCodecArgs {
            path: policy_dir.join("policy.json"),
            sha256: String::new(),
        }),
        ..f.args()
    };
    let policy = json!({"schema":"pobr-ordinary-catalyst-policy/v1","version":"0.5.5.5","import_ids":{"TestQuality":7},"statement":"Synthetic codec identity"});
    args.catalog_policy.as_mut().unwrap().sha256 = write(&policy_dir.join("policy.json"), &policy);
    seal(&mut args, &mut m, &extra);
    (f, args, m, extra)
}
fn seal(args: &mut Args, m: &mut Value, tables: &Value) {
    let extra = args.supplement.as_mut().unwrap();
    let h = write(&extra.path.join("tables.json"), tables);
    m["files"]["tables.json"] =
        json!({"bytes":fs::metadata(extra.path.join("tables.json")).unwrap().len(),"sha256":h});
    for (name, t) in tables.as_object().unwrap() {
        m["summary"]["tables"][name] = json!(t["rows"].as_array().unwrap().len());
    }
    extra.sha256 = write(&extra.path.join("manifest.json"), m);
}

#[test]
fn target_catalog_and_all_refined_currency_records_are_retained() {
    let (f, args, _, _) = fixture();
    run(args).unwrap();
    let out: Value = crate::read_json(&f.root.join("out/overlay/catalysts.json")).unwrap();
    assert_eq!(
        out["catalysts"],
        json!([{"id":7,"name":"Test","descriptor":"Test","tags":["test"]}])
    );
    let data = pobr_gamedata::GameData::from_memory(BTreeMap::from([(
        "overlay/catalysts.json".into(),
        fs::read(f.root.join("out/overlay/catalysts.json")).unwrap(),
    )]));
    let loaded = data.catalysts().unwrap().unwrap();
    assert_eq!(loaded.catalysts[0].id, 7);
    assert_eq!(loaded.catalysts[0].tags, ["test"]);
    let receipt: Value = crate::read_json(&f.root.join("out/producer-receipt.json")).unwrap();
    assert_eq!(
        receipt["conversion"]["records"].as_array().unwrap().len(),
        2
    );
    assert_eq!(
        receipt["conversion"]["records"][1]["name"],
        "Refined Test Catalyst"
    );
}
#[test]
fn source_name_descriptor_tags_and_new_currency_variant_propagate() {
    let (mut f, mut args, mut m, mut tables) = fixture();
    f.values("BaseItemTypes", 0)["Name"] = json!("Changed Catalyst");
    f.seal();
    args.expected_manifest_sha256 = f.args().expected_manifest_sha256;
    tables["ModEffectStats"]["rows"][0]["values"]["Tags"] = json!([1, 0]);
    for row in tables["AlternateQualityTypes"]["rows"]
        .as_array_mut()
        .unwrap()
    {
        row["values"]["Description"] = json!("Quality (Fresh Modifiers)");
    }
    let mut variant = tables["AlternateQualityTypes"]["rows"][1].clone();
    variant["_row"] = json!(2);
    variant["values"]["Id"] = json!("NewVariant");
    tables["AlternateQualityTypes"]["rows"]
        .as_array_mut()
        .unwrap()
        .push(variant);
    tables["AlternateQualityTypes"]["validation"]["rows"] = json!(3);
    seal(&mut args, &mut m, &tables);
    run(args).unwrap();
    let out: Value = crate::read_json(&f.root.join("out/overlay/catalysts.json")).unwrap();
    assert_eq!(
        out["catalysts"][0],
        json!({"id":7,"name":"Changed","descriptor":"Fresh","tags":["changed","test"]})
    );
}
#[test]
fn incompatible_patch_producer_payload_and_schema_reject_before_output() {
    for mode in 0..4 {
        let (f, mut args, mut m, tables) = fixture();
        match mode {
            0 => m["source_identity"]["game_patch"] = json!("0.5.5.6"),
            1 => m["producer"]["RePoE/native_tables.py"] = json!("f".repeat(64)),
            2 => m["schema"]["compatibility_views"] = json!(1),
            _ => {}
        }
        seal(&mut args, &mut m, &tables);
        if mode == 3 {
            fs::write(
                args.supplement.as_ref().unwrap().path.join("tables.json"),
                b"{}",
            )
            .unwrap();
        }
        assert!(run(args).is_err());
        assert!(!f.root.join("out").exists());
    }
}
#[test]
fn dangling_effect_or_tag_scope_and_new_unknown_quality_are_not_dropped() {
    for mode in 0..5 {
        let (f, mut args, mut m, mut tables) = fixture();
        match mode {
            0 => tables["AlternateQualityTypes"]["rows"][0]["values"]["ModEffectStat"] = json!(9),
            1 => tables["ModEffectStats"]["rows"][0]["values"]["Tags"] = json!([999]),
            2 => tables["ModEffectStats"]["rows"][0]["values"]["ApplyToImplicit"] = json!(false),
            3 => tables["AlternateQualityTypes"]["rows"][0]["values"]["Id"] = json!("NewQuality"),
            _ => {
                tables["AlternateQualityTypes"]["columns"][3]["schema"]["references"]["table"] =
                    json!("Stats")
            }
        }
        seal(&mut args, &mut m, &tables);
        assert!(run(args).is_err());
        assert!(!f.root.join("out").exists());
    }
}
