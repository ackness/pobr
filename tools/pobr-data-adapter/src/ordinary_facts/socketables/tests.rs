use super::super::tests::Fixture;
use super::super::{Args, CodecArgs, Domain, run};
use super::*;
use std::fs;

const GLOBAL: &str = "Data/StatDescriptions/stat_descriptions.csd";

fn description(id: &str, template: &str) -> Value {
    json!({"kind":"description","ids":[id],"raw_slice_sha256":"synthetic",
        "groups":[{"language_origin":"implicit-default","declared_language":null,
        "branches":[{"condition_tokens_raw":["#"],"restrictions_raw":[],
            "quantifier_tokens_raw":[],"quantifiers_lexical":[],"template_syntax_diagnostics":[],
            "template_raw":template,"span":{}}]}]})
}

fn fixture() -> (Fixture, Args) {
    let mut f = Fixture::new();
    let tables: BTreeMap<String, Value> = serde_json::from_slice(include_bytes!(
        "../../../tests/fixtures/ordinary-facts/socketable-tables.json"
    ))
    .unwrap();
    f.documents.retain(|k, _| k == "source-lock.json");
    f.documents.extend(
        tables
            .into_iter()
            .map(|(k, v)| (format!("tables/{k}.json"), v)),
    );
    f.seal();
    let dir = f.root.join("aux");
    fs::create_dir(&dir).unwrap();
    let old = json!({"_meta":{"patch":"historical"},"runes":[{"name":"Old Name","slots":{
        "weapon":{"kind":"Rune","lines":["Old full effect"]},
        "helmet":{"kind":"Rune","lines":["Old full helmet effect"]}}}]});
    let csd = json!({"files":{GLOBAL:{"path":GLOBAL,"records":[
        description("test_life","{0:+d} to maximum Life"),description("test_mana","{0:+d} to maximum Mana")]}}});
    let csd_hash = write(&dir.join("descriptions.json"), &csd);
    let manifest_hash = write(
        &dir.join("manifest.json"),
        &json!({"format":"repoe-native-descriptions/v1",
        "source_identity":{"game_patch":"0.5.5.5"},"files":{"descriptions.json":{
        "sha256":csd_hash,"bytes":fs::metadata(dir.join("descriptions.json")).unwrap().len()}}}),
    );
    let legacy_hash = write(&dir.join("legacy.json"), &old);
    let effects = json!([{"category":"All","normal":[["test_life",10]],"bonded":[["test_mana",4]]},
        {"category":"Helmet","normal":[["test_life",-2]],"bonded":[]}]);
    let policy = json!({"schema":"pobr-ordinary-socketables-policy/v1","version":"0.5.5.5",
        "source_sha256":legacy_hash,"descriptions_sha256":csd_hash,"descriptions_manifest_sha256":manifest_hash,
        "categories":{"All":["weapon"],"Helmet":["helmet"]},"category_item_classes":{"All":[],"Helmet":["Helmet"]},"limit_groups":{"per-item":null},"compatibility_slot_aliases":{},"import_aliases":{},"excluded":{},
        "compatibility":{"Metadata/TestRune":{"name":"Old Name","effects_sha256":hash(&effects).unwrap()}},"statement":"Synthetic explicit whole-entry compatibility"});
    let policy_hash = write(&dir.join("policy.json"), &policy);
    let args = Args {
        domain: Domain::Socketables,
        item_descriptions: Some(dir.join("descriptions.json")),
        codec: Some(CodecArgs {
            source: dir.join("legacy.json"),
            source_sha256: legacy_hash,
            policy: dir.join("policy.json"),
            policy_sha256: policy_hash,
        }),
        ..f.args()
    };
    (f, args)
}
fn write(path: &Path, value: &Value) -> String {
    let bytes = reader::json_bytes(value).unwrap();
    fs::write(path, &bytes).unwrap();
    sha256_hex(&bytes)
}
fn reseal(f: &mut Fixture, args: &mut Args) {
    f.seal();
    args.expected_manifest_sha256 = f.args().expected_manifest_sha256;
}
fn result(f: &Fixture) -> Value {
    crate::read_json(&f.root.join("out/overlay/runes.json")).unwrap()
}
fn rejects(f: &Fixture, args: Args, expected: &str) {
    let error = run(args).unwrap_err();
    assert!(error.contains(expected), "expected {expected}, got {error}");
    assert!(
        !f.root.join("out").exists(),
        "validation wrote partial output"
    );
}

#[test]
fn target_name_flags_values_and_bonded_packets_feed_existing_dto() {
    let (f, args) = fixture();
    run(args).unwrap();
    let out = result(&f);
    let data: RunesDef = serde_json::from_value(out).unwrap();
    let rune = &data.runes[0];
    assert_eq!(rune.name, "Source Rune");
    assert_eq!(
        rune.slots["weapon"].lines,
        ["+10 to maximum Life", "Bonded: +4 to maximum Mana"]
    );
    assert_eq!(rune.slots["helmet"].lines, ["-2 to maximum Life"]);
    assert_eq!(rune.slots["weapon"].limit, Some(2));
    assert_eq!(
        rune.slots["weapon"].limit_id, None,
        "generic limit is per augment, not shared"
    );
}

#[test]
fn changed_source_name_values_restrictions_and_slot_membership_propagate() {
    let (mut f, mut args) = fixture();
    f.values("BaseItemTypes", 0)["Name"] = json!("New Name");
    f.values("SoulCoreStats", 0)["StatsValues"] = json!([31]);
    f.values("SoulCoreStats", 1)["StatCategory"] = json!(0);
    f.values("SoulCores", 0)["RequiredLevel"] = json!(27);
    f.values("SoulCores", 0)["IsSocketBound"] = json!(true);
    f.values("SoulCores", 0)["CanSocketInUniqueItems"] = json!(false);
    reseal(&mut f, &mut args);
    run(args).unwrap();
    let out = result(&f);
    let rune = &out["runes"][0];
    assert_eq!(rune["name"], "New Name");
    assert!(rune["slots"]["helmet"].is_null());
    assert_eq!(
        rune["slots"]["weapon"]["lines"],
        json!([
            "+31 to maximum Life",
            "Bonded: +4 to maximum Mana",
            "-2 to maximum Life"
        ])
    );
    assert_eq!(rune["slots"]["weapon"]["required_level"], 27);
    assert_eq!(rune["slots"]["weapon"]["socket_bound"], true);
    assert_eq!(rune["slots"]["weapon"]["can_socket_in_unique_items"], false);
}

#[test]
fn missing_stat_partner_invalid_fk_and_absent_flag_reject_before_output() {
    for (table, key, value, error) in [
        ("SoulCoreStats", "StatsValues", json!([]), "cardinality"),
        (
            "SoulCoreStats",
            "SoulCore",
            json!(900),
            "unresolved physical row",
        ),
        ("SoulCores", "IsSocketBound", Value::Null, "boolean"),
        (
            "SoulCoreStats",
            "StatsValues",
            json!([2147483648_i64]),
            "signed",
        ),
    ] {
        let (mut f, mut args) = fixture();
        f.values(table, 0)[key] = value;
        reseal(&mut f, &mut args);
        rejects(&f, args, error);
    }
}

#[test]
fn wrong_foreign_key_schema_is_not_silently_interpreted() {
    let (mut f, mut args) = fixture();
    let doc = f.documents.get_mut("tables/SoulCoreStats.json").unwrap();
    doc["columns"][0]["schema"]["references"]["table"] = json!("Stats");
    reseal(&mut f, &mut args);
    rejects(&f, args, "schema");
}

#[test]
fn whole_socketable_fallback_never_keeps_a_partial_target_packet() {
    let (mut f, mut args) = fixture();
    f.values("Stats", 1)["Id"] = json!("unsupported");
    let policy_path = f.root.join("aux/policy.json");
    let mut p: Value = crate::read_json(&policy_path).unwrap();
    let packets = json!([{"category":"All","normal":[["test_life",10]],"bonded":[["unsupported",4]]},
        {"category":"Helmet","normal":[["test_life",-2]],"bonded":[]}]);
    p["compatibility"]["Metadata/TestRune"]["effects_sha256"] = json!(hash(&packets).unwrap());
    args.codec.as_mut().unwrap().policy_sha256 = write(&policy_path, &p);
    reseal(&mut f, &mut args);
    run(args).unwrap();
    let out = result(&f);
    assert_eq!(
        out["runes"][0]["slots"]["weapon"]["lines"],
        json!(["Old full effect"])
    );
    let receipt: Value = crate::read_json(&f.root.join("out/producer-receipt.json")).unwrap();
    assert_eq!(
        receipt["conversion"]["records"][0]["selected_descriptions"],
        json!([])
    );
}

#[test]
fn absent_compound_context_is_never_zero_and_explicit_source_zero_can_render() {
    for mode in 0..3 {
        let (mut f, mut args) = fixture();
        let csd_path = args.item_descriptions.as_ref().unwrap();
        let mut csd: Value = crate::read_json(csd_path).unwrap();
        let record = &mut csd["files"][GLOBAL]["records"][0];
        record["ids"] = json!(["test_life", "test_context"]);
        record["groups"][0]["branches"][0]["condition_tokens_raw"] = json!(["#", "0"]);
        let csd_hash = write(csd_path, &csd);
        let manifest_path = csd_path.with_file_name("manifest.json");
        let mut manifest: Value = crate::read_json(&manifest_path).unwrap();
        manifest["files"]["descriptions.json"] =
            json!({"sha256":csd_hash,"bytes":fs::metadata(csd_path).unwrap().len()});
        let manifest_hash = write(&manifest_path, &manifest);
        let codec = args.codec.as_mut().unwrap();
        let mut policy: Value = crate::read_json(&codec.policy).unwrap();
        policy["descriptions_sha256"] = json!(csd_hash);
        policy["descriptions_manifest_sha256"] = json!(manifest_hash);
        codec.policy_sha256 = write(&codec.policy, &policy);
        let stats = f.documents.get_mut("tables/Stats.json").unwrap();
        stats["rows"]
            .as_array_mut()
            .unwrap()
            .push(json!({"_row":2,"values":{"Id":"test_context"}}));
        stats["validation"]["rows"] = json!(3);
        if mode == 1 {
            f.values("SoulCoreStats", 0)["StatsValues"] = json!([11]);
        } else if mode == 2 {
            for row in 0..2 {
                f.values("SoulCoreStats", row)["Stats"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!(2));
                f.values("SoulCoreStats", row)["StatsValues"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!(0));
            }
        }
        reseal(&mut f, &mut args);
        if mode == 1 {
            rejects(&f, args, "unrenderable socketable effects changed");
            continue;
        }
        run(args).unwrap();
        let out = result(&f);
        let receipt: Value = crate::read_json(&f.root.join("out/producer-receipt.json")).unwrap();
        let record = &receipt["conversion"]["records"][0];
        if mode == 0 {
            assert_eq!(
                out["runes"][0]["slots"]["weapon"]["lines"],
                json!(["Old full effect"])
            );
            assert_eq!(
                out["runes"][0]["slots"]["helmet"]["lines"],
                json!(["Old full helmet effect"])
            );
            assert_eq!(record["selected_descriptions"], json!([]));
            assert!(
                record["unrenderable"][0]["error"]
                    .as_str()
                    .unwrap()
                    .contains("incomplete compound CSD member test_context")
            );
            assert_eq!(
                record["packets"][0][1]["normal"],
                json!([["test_life", 10]])
            );
        } else {
            assert_eq!(
                out["runes"][0]["slots"]["weapon"]["lines"],
                json!(["+10 to maximum Life", "Bonded: +4 to maximum Mana"])
            );
            assert_eq!(record["mode"], "target-csd");
            assert_eq!(
                record["packets"][0][1]["normal"],
                json!([["test_life", 10], ["test_context", 0]])
            );
        }
    }
}

#[test]
fn unsupported_changed_values_cannot_reuse_old_text() {
    let (mut f, mut args) = fixture();
    f.values("Stats", 1)["Id"] = json!("unsupported");
    reseal(&mut f, &mut args);
    rejects(&f, args, "effects changed");
}

#[test]
fn no_existing_output_or_unpinned_description_is_overwritten() {
    let (f, mut args) = fixture();
    args.item_descriptions = Some(f.root.join("aux/legacy.json"));
    rejects(&f, args, "SHA256 mismatch");
    let (f, args) = fixture();
    fs::create_dir(f.root.join("out")).unwrap();
    let error = run(args).unwrap_err();
    assert!(error.contains("existing"), "{error}");
    assert_eq!(fs::read_dir(f.root.join("out")).unwrap().count(), 0);
}

#[test]
fn new_source_member_does_not_require_a_prepared_catalog_entry() {
    let (mut f, mut args) = fixture();
    for (table, mut value) in [
        ("BaseItemTypes", f.values("BaseItemTypes", 0).clone()),
        ("SoulCores", f.values("SoulCores", 0).clone()),
        ("SoulCoreStats", f.values("SoulCoreStats", 0).clone()),
    ] {
        match table {
            "BaseItemTypes" => {
                value["Id"] = json!("Metadata/NewRune");
                value["Name"] = json!("New Source Rune");
            }
            "SoulCores" => value["BaseItemType"] = json!(1),
            _ => value["SoulCore"] = json!(1),
        }
        let doc = f
            .documents
            .get_mut(&format!("tables/{table}.json"))
            .unwrap();
        let rows = doc["rows"].as_array_mut().unwrap();
        let row = rows.len();
        rows.push(json!({"_row":row,"values":value}));
        doc["validation"]["rows"] = json!(row + 1);
    }
    reseal(&mut f, &mut args);
    run(args).unwrap();
    let out = result(&f);
    assert_eq!(out["runes"].as_array().unwrap().len(), 2);
    assert_eq!(out["runes"][0]["name"], "New Source Rune");
    assert_eq!(
        out["runes"][0]["slots"]["weapon"]["lines"],
        json!(["+10 to maximum Life", "Bonded: +4 to maximum Mana"])
    );
}

#[test]
fn changed_category_members_and_unknown_category_are_explicit_errors() {
    let (mut f, mut args) = fixture();
    f.values("SoulCoreStatCategories", 0)["TargetItemClasses"] = json!([0]);
    reseal(&mut f, &mut args);
    rejects(&f, args, "item-class binding changed");
    let (mut f, mut args) = fixture();
    f.values("SoulCoreStatCategories", 0)["Id"] = json!("New Category");
    reseal(&mut f, &mut args);
    rejects(&f, args, "item-class binding changed");
}

#[test]
fn unchanged_prepared_combined_slot_expands_without_losing_any_line() {
    let (mut f, mut args) = fixture();
    f.values("Stats", 1)["Id"] = json!("unsupported");
    let codec = args.codec.as_mut().unwrap();
    let mut old: Value = crate::read_json(&codec.source).unwrap();
    old["runes"][0]["slots"]["combined"] = old["runes"][0]["slots"]["weapon"].take();
    old["runes"][0]["slots"]
        .as_object_mut()
        .unwrap()
        .remove("weapon");
    codec.source_sha256 = write(&codec.source, &old);
    let mut p: Value = crate::read_json(&codec.policy).unwrap();
    p["source_sha256"] = json!(codec.source_sha256);
    p["categories"]["All"] = json!(["weapon", "wand", "staff"]);
    p["compatibility_slot_aliases"] = json!({"combined":["weapon","wand","staff"]});
    p["compatibility"]["Metadata/TestRune"]["effects_sha256"] = json!(
        hash(&json!([
        {"category":"All","normal":[["test_life",10]],"bonded":[["unsupported",4]]},
        {"category":"Helmet","normal":[["test_life",-2]],"bonded":[]}]))
        .unwrap()
    );
    codec.policy_sha256 = write(&codec.policy, &p);
    reseal(&mut f, &mut args);
    run(args).unwrap();
    let out = result(&f);
    for slot in ["weapon", "wand", "staff"] {
        assert_eq!(
            out["runes"][0]["slots"][slot]["lines"],
            json!(["Old full effect"])
        );
    }
}

#[test]
fn declared_import_alias_keeps_target_effects_and_rejects_name_collision() {
    for collide in [false, true] {
        let (mut f, mut args) = fixture();
        let codec = args.codec.as_mut().unwrap();
        let mut p: Value = crate::read_json(&codec.policy).unwrap();
        p["import_aliases"] = json!({"Metadata/TestRune":["Old Name"]});
        codec.policy_sha256 = write(&codec.policy, &p);
        if collide {
            f.values("BaseItemTypes", 0)["Name"] = json!("Old Name");
        }
        reseal(&mut f, &mut args);
        if collide {
            rejects(&f, args, "duplicate socketable import alias");
        } else {
            run(args).unwrap();
            let out = result(&f);
            let rows = out["runes"].as_array().unwrap();
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0]["name"], "Old Name");
            assert_eq!(rows[1]["name"], "Source Rune");
            assert_eq!(rows[0]["slots"], rows[1]["slots"]);
        }
    }
}
