use super::tests::fixture;
use crate::ordinary_facts::{Args, CodecArgs, Domain, parse_args, run, tests::Fixture};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};

fn write(path: &Path, value: &Value) -> String {
    let raw = crate::native_facts::reader::json_bytes(value).unwrap();
    fs::write(path, &raw).unwrap();
    crate::raw_source::sha256_hex(&raw)
}
fn legacy() -> Value {
    json!({"_meta":{"origin":"synthetic-historical"},"unknown_root":{"keep":null},"overrides":[
    {"name":"Same Display Name","req_str":999,"req_dex":888,"req_int":777,"block_chance":90.0,"spirit":123,"reload_time_ms":456,
        "tags":["compat-tag"],"charm_buff":["Retained complete charm effect"],"socket_limit":3,"sub_type":"fixture","unknown_entry":{"array":[null,2]}},
    {"name":"Historical","req_str":17,"future_field":true}]})
}
fn args(f: &Fixture, old: Value) -> Args {
    let aux = f.root.join("aux");
    fs::create_dir_all(&aux).unwrap();
    let source = aux.join("legacy.json");
    let source_sha256 = write(&source, &old);
    let policy = aux.join("policy.json");
    let policy_sha256 = write(
        &policy,
        &json!({
        "schema":"pobr-base-item-field-ownership/v1","version":"0.5.5.5","source_sha256":source_sha256,"source_metadata":old["_meta"],
        "delegated_fields":["req_str","req_dex","req_int","block_chance","spirit","reload_time_ms"],"statement":"Synthetic field ownership"}),
    );
    Args {
        domain: Domain::BaseItems,
        codec: Some(CodecArgs {
            source,
            source_sha256,
            policy,
            policy_sha256,
        }),
        ..f.args()
    }
}
fn load(f: &Fixture) -> Vec<pobr_data::catalog::BaseItemDef> {
    let data = pobr_gamedata::GameData::from_memory(BTreeMap::from([
        (
            "base/base_items.json".into(),
            fs::read(f.root.join("out/base/base_items.json")).unwrap(),
        ),
        (
            "overlay/base_item_overrides.json".into(),
            fs::read(f.root.join("out/overlay/base_item_overrides.json")).unwrap(),
        ),
    ]));
    data.base_items().unwrap()
}
#[test]
fn ordinary_loader_keeps_each_same_name_base_numeric_facts_and_prepared_fields() {
    let f = fixture();
    let old = legacy();
    let a = args(&f, old.clone());
    run(a).unwrap();
    let bases = load(&f);
    let find = |id: &str| bases.iter().find(|b| b.id == id).unwrap();
    let shield = find("Metadata/a-shield");
    let sword = find("Metadata/z-sword");
    let sceptre = find("Metadata/sceptre");
    assert_eq!(
        (shield.req_str, shield.req_dex, shield.req_int),
        (10, 20, 30)
    );
    assert_eq!(shield.armour.as_ref().unwrap().block_chance, Some(0.0));
    assert!(shield.weapon.is_none());
    assert_eq!(shield.spirit, None);
    assert_eq!(sword.spirit, Some(100));
    assert!(sword.armour.is_none());
    assert_eq!(sword.weapon.as_ref().unwrap().reload_time_ms, Some(800));
    assert_eq!(sceptre.spirit, Some(0));
    assert_eq!(sceptre.req_str, 0);
    assert!(sword.tags.contains(&"compat-tag".into()));
    assert_eq!(shield.charm_buff, ["Retained complete charm effect"]);
    let output: Value =
        crate::read_json(&f.root.join("out/overlay/base_item_overrides.json")).unwrap();
    assert_eq!(output["_meta"], old["_meta"]);
    assert_eq!(output["unknown_root"], old["unknown_root"]);
    assert_eq!(output["overrides"][1], old["overrides"][1]);
    let mut expected = old["overrides"][0].clone();
    for key in [
        "req_str",
        "req_dex",
        "req_int",
        "spirit",
        "reload_time_ms",
        "block_chance",
    ] {
        expected.as_object_mut().unwrap().remove(key);
    }
    assert_eq!(output["overrides"][0], expected);
    let receipt: Value = crate::read_json(&f.root.join("out/producer-receipt.json")).unwrap();
    let records = &receipt["conversion"]["compatibility_overlay"]["records"];
    assert_eq!(records[0]["target_bases"].as_array().unwrap().len(), 5);
    assert_eq!(records[1]["mode"], "unmatched-complete-compatibility");
}
#[test]
fn source_value_presence_and_name_changes_propagate_through_ordinary_loader() {
    let mut f = fixture();
    f.values("AttributeRequirements", 1)["ReqStr"] = json!(47);
    f.values("ShieldTypes", 0)["Block"] = json!(35);
    let spirits = f.documents.get_mut("tables/ItemSpirit.json").unwrap();
    spirits["rows"] = json!([]);
    spirits["validation"]["rows"] = json!(0);
    spirits["validation"]["actual_row_bytes"] = json!(0);
    f.values("BaseItemTypes", 6)["Name"] = json!("Historical");
    f.seal();
    run(args(&f, legacy())).unwrap();
    let bases = load(&f);
    let shield = bases.iter().find(|b| b.id == "Metadata/a-shield").unwrap();
    assert_eq!(shield.req_str, 47);
    assert_eq!(shield.armour.as_ref().unwrap().block_chance, Some(35.0));
    assert!(bases.iter().all(|b| b.spirit.is_none()));
    assert_eq!(
        bases
            .iter()
            .find(|b| b.id == "Metadata/plain")
            .unwrap()
            .req_str,
        0
    );
}
#[test]
fn optional_compatibility_leaves_original_base_output_unchanged() {
    let f = fixture();
    let old_args = Args {
        domain: Domain::BaseItems,
        out: f.root.join("without-overlay"),
        ..f.args()
    };
    run(old_args).unwrap();
    run(args(&f, legacy())).unwrap();
    assert_eq!(
        fs::read(f.root.join("without-overlay/base/base_items.json")).unwrap(),
        fs::read(f.root.join("out/base/base_items.json")).unwrap()
    );
    assert!(!f.root.join("without-overlay/overlay").exists());
}
#[test]
fn duplicate_names_bad_dto_values_and_policy_drift_fail_before_output() {
    for change in [0, 1, 2, 3, 4] {
        let f = fixture();
        let mut old = legacy();
        if change == 0 {
            let duplicate = old["overrides"][0].clone();
            old["overrides"].as_array_mut().unwrap().push(duplicate);
        }
        if change == 1 {
            old["overrides"][0]["req_str"] = json!(-1);
        }
        let mut a = args(&f, old);
        if change >= 2 {
            let c = a.codec.as_mut().unwrap();
            let mut policy: Value = crate::read_json(&c.policy).unwrap();
            match change {
                2 => policy["version"] = json!("other"),
                3 => policy["delegated_fields"] = json!(["tags"]),
                _ => policy["source_metadata"] = json!({}),
            };
            c.policy_sha256 = write(&c.policy, &policy);
        }
        assert!(run(a).is_err());
        assert!(!f.root.join("out").exists());
    }
}
#[test]
fn input_pins_and_output_protection_apply_to_optional_source() {
    for bad_pin in [true, false] {
        let f = fixture();
        let mut a = args(&f, legacy());
        if bad_pin {
            a.codec.as_mut().unwrap().policy_sha256 = "0".repeat(64);
        } else {
            a.out = f.root.join("aux/out");
        }
        let out = a.out.clone();
        assert!(run(a).is_err());
        assert!(!out.exists());
    }
}
#[test]
fn codec_flags_are_optional_for_base_items_but_partial_groups_reject() {
    let basic = [
        "--ordinary-facts",
        "input",
        "--facts-manifest-sha256",
        "pin",
        "--version",
        "0.5.5.5",
        "--out",
        "out",
        "--ordinary-domain",
        "base-items",
    ];
    assert!(
        parse_args(basic.into_iter().map(str::to_owned))
            .unwrap()
            .codec
            .is_none()
    );
    let full = [
        "--codec-source",
        "old",
        "--codec-source-sha256",
        "pin",
        "--codec-policy",
        "policy",
        "--codec-policy-sha256",
        "pin",
    ];
    assert!(
        parse_args(basic.into_iter().chain(full).map(str::to_owned))
            .unwrap()
            .codec
            .is_some()
    );
    for flags in [&full[..2], &full[2..4], &full[4..]] {
        assert!(
            parse_args(
                basic
                    .into_iter()
                    .chain(flags.iter().copied())
                    .map(str::to_owned)
            )
            .is_err()
        );
    }
}
