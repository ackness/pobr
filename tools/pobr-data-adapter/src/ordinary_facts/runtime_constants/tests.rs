use super::super::stat_set_codec::tests::table;
use super::super::tests::Fixture;
use super::super::{Args, CodecArgs, Domain, parse_args, reader, run};
use crate::raw_source::sha256_hex;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;

const MANIFEST: &str = "snapshot/manifest.json";
const POLICY: &str = "policy/policy.json";
fn fixture() -> Fixture {
    let mut f = Fixture::new();
    let old: Vec<_> = f
        .documents
        .keys()
        .filter(|p| p.starts_with("tables/"))
        .cloned()
        .collect();
    f.documents.retain(|p, _| p == "source-lock.json");
    let p: Value = serde_json::from_slice(include_bytes!(
        "../../../../../pipeline/runtime-constants/0.5.5.5-facts-compat-policy.json"
    ))
    .unwrap();
    let ids: BTreeSet<_> = p["bindings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["source_id"].as_str().unwrap())
        .collect();
    let mut rows = vec![
        json!({"Id":"LineageSupportGemGoldPrice ","Value":9,"Divisor":0,"Future":"unused zero divisor, not evaluated"}),
    ];
    for (index, id) in ids.into_iter().rev().enumerate() {
        rows.push(json!({"Id":id,"Value":index as i32*17-100,"Divisor":7,"Future":"unmapped"}));
    }
    for row in &mut rows {
        let values = match row["Id"].as_str().unwrap() {
            "BaseShockMagnitude" => (17, 1),
            "BaseWardRegenerationPercentPerMinute" => (777, 1),
            "HeavyStunModifierDuration" => (16500, 1000),
            "BleedingHitDamagePercentPerMinute" => (1, 3),
            "LightStunMinimumChance" => (-21, -3),
            "PassiveTreeJewelDistanceMultiplier" => (-120, 100),
            _ => continue,
        };
        row["Value"] = json!(values.0);
        row["Divisor"] = json!(values.1);
    }
    f.documents.insert(
        "tables/GameConstants.json".into(),
        table(
            "GameConstants",
            &[
                ("Id", "string", false, None),
                ("Value", "i32", false, None),
                ("Divisor", "i32", false, None),
                ("Future", "string", false, None),
            ],
            json!(rows),
        ),
    );
    f.seal();
    for path in old {
        if !f.documents.contains_key(&path) {
            fs::remove_file(f.root.join("input").join(path)).unwrap();
        }
    }
    fs::create_dir_all(f.root.join("snapshot/base")).unwrap();
    fs::create_dir(f.root.join("policy")).unwrap();
    let templates: BTreeMap<String, Value> = serde_json::from_slice(include_bytes!(
        "../../../tests/fixtures/ordinary-facts/runtime-constant-templates.json"
    ))
    .unwrap();
    for (name, v) in templates {
        write(&f, &format!("snapshot/base/{name}"), &v);
    }
    write(
        &f,
        MANIFEST,
        &json!({"schema_version":3,"poe_version":"0.5.5.4.2","files":{"overlay/not-provided.json":"a".repeat(64)},"future":{"preserve":"manifest bytes"}}),
    );
    write(&f, POLICY, &p);
    reseal(&mut f);
    f
}
fn doc(f: &Fixture, path: &str) -> Value {
    crate::read_json(&f.root.join(path)).unwrap()
}
fn write(f: &Fixture, path: &str, v: &Value) {
    let mut b = b" \n\t".to_vec();
    b.extend(serde_json::to_vec(v).unwrap());
    b.extend(b"\n\n");
    fs::write(f.root.join(path), b).unwrap();
}
fn hash(v: &impl serde::Serialize) -> String {
    sha256_hex(&reader::json_bytes(v).unwrap())
}
fn reseal(f: &mut Fixture) {
    f.seal();
    let mut p = doc(f, POLICY);
    let mut m = doc(f, MANIFEST);
    for selected in p["source_files"].as_array_mut().unwrap() {
        let path = selected["relative_path"].as_str().unwrap();
        let pin = sha256_hex(&fs::read(f.root.join("snapshot").join(path)).unwrap());
        m["files"][path] = json!(pin);
        selected["sha256"] = json!(pin);
    }
    write(f, MANIFEST, &m);
    p["source_snapshot"]["sha256"] = json!(sha256_hex(&fs::read(f.root.join(MANIFEST)).unwrap()));
    p["target"]["facts_manifest_sha256"] = json!(f.args().expected_manifest_sha256);
    write(f, POLICY, &p);
}
fn repin_manifest(f: &Fixture) {
    let mut p = doc(f, POLICY);
    p["source_snapshot"]["sha256"] = json!(sha256_hex(&fs::read(f.root.join(MANIFEST)).unwrap()));
    write(f, POLICY, &p);
}
fn args(f: &Fixture) -> Args {
    Args {
        domain: Domain::RuntimeConstants,
        codec: Some(CodecArgs {
            source: f.root.join(MANIFEST),
            source_sha256: sha256_hex(&fs::read(f.root.join(MANIFEST)).unwrap()),
            policy: f.root.join(POLICY),
            policy_sha256: sha256_hex(&fs::read(f.root.join(POLICY)).unwrap()),
        }),
        ..f.args()
    }
}
fn rejects(f: &Fixture, needle: &str) {
    let e = run(args(f)).unwrap_err();
    assert!(e.contains(needle), "expected {needle:?}, got {e:?}");
    assert!(!f.root.join("out").exists());
}
fn raw_row(f: &Fixture, id: &str) -> usize {
    f.documents["tables/GameConstants.json"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .position(|r| r["values"]["Id"] == id)
        .unwrap()
}
fn output(f: &Fixture, file: &str) -> Value {
    doc(f, &format!("out/base/{file}"))
}
fn receipt(f: &Fixture) -> Value {
    doc(f, "out/producer-receipt.json")
}

#[test]
fn three_original_dtos_use_target_values_and_preserve_every_other_raw_value() {
    use pobr_data::catalog::{
        character_constants::CharacterConstantsDef, game_constants::GameConstantsDef,
        jewel_radii::JewelRadiiDef,
    };
    let f = fixture();
    run(args(&f)).unwrap();
    let p = doc(&f, POLICY);
    let game: GameConstantsDef = serde_json::from_value(output(&f, "game_constants.json")).unwrap();
    let character: CharacterConstantsDef =
        serde_json::from_value(output(&f, "character_constants.json")).unwrap();
    let jewels: JewelRadiiDef = serde_json::from_value(output(&f, "jewel_radii.json")).unwrap();
    assert_eq!(game.game.light_stun_minimum_chance, 7.0);
    assert_eq!(game.game.heavy_stun_modifier_duration, 16.5);
    assert_eq!(game.game.shock_min_effect, 17.0);
    assert_eq!(game.game.shock_default_effect, 17.0 / 100.0);
    assert_eq!(character.ward_regeneration_per_minute_pct, 777.0);
    assert_eq!(jewels.distance_multiplier, -1.2);
    assert_eq!(jewels.tree_versions["old"][0].label, "Second");
    assert_eq!(jewels.tree_versions["old"][1].label, "First");
    for selected in p["source_files"].as_array().unwrap() {
        let file = selected["file"].as_str().unwrap();
        let mut source = doc(&f, &format!("snapshot/base/{file}"));
        let out = output(&f, file);
        for b in p["bindings"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|b| b["file"] == file)
        {
            let ptr = b["target_pointer"].as_str().unwrap();
            let (parent, leaf) = super::leaf_parent(&mut source, ptr).unwrap();
            parent.insert(leaf, out.pointer(ptr).unwrap().clone());
        }
        assert_eq!(source, out, "{file}");
        assert!(out.get("_meta").is_none());
    }
    assert!(
        output(&f, "game_constants.json")["game"]
            .get("pin_base_duration")
            .is_none()
    );
    assert!(
        doc(&f, "snapshot/base/character_constants.json")
            .get("ward_regeneration_per_minute_pct")
            .is_none()
    );
    let r = receipt(&f);
    assert_eq!(r["conversion"]["bindings"].as_array().unwrap().len(), 31);
    let ward = &r["conversion"]["bindings"][29];
    assert_eq!(ward["legacy"]["present"], false);
    assert_eq!(ward["legacy"]["value"], Value::Null);
    assert_eq!(ward["legacy"]["equals_normalized_f64"], Value::Null);
    let shared = r["conversion"]["source_occurrences"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == "BaseShockMagnitude")
        .unwrap();
    assert_eq!(shared["binding_positions"], json!([11, 24]));
    assert_eq!(
        r["conversion"]["unconsumed_source"]["rows"][0]["id"],
        "LineageSupportGemGoldPrice "
    );
    assert_eq!(
        r["conversion"]["unconsumed_source"]["rows"][0]["evaluated_as_model"],
        false
    );
    assert_eq!(
        r["conversion"]["unconsumed_source"]["columns"][0]["physical_fields"],
        json!(["Future"])
    );
}

#[test]
fn signed_zero_changed_values_and_ordered_scaling_have_no_old_value_gate() {
    for (raw, divisor) in [(0, 1), (-71, 3), (71, -3), (i32::MIN, i32::MAX)] {
        let mut f = fixture();
        let row = raw_row(&f, "BaseWardRegenerationPercentPerMinute");
        f.values("GameConstants", row)["Value"] = json!(raw);
        f.values("GameConstants", row)["Divisor"] = json!(divisor);
        reseal(&mut f);
        run(args(&f)).unwrap();
        assert_eq!(
            output(&f, "character_constants.json")["ward_regeneration_per_minute_pct"]
                .as_f64()
                .unwrap(),
            f64::from(raw) / f64::from(divisor)
        );
    }
    let f = fixture();
    run(args(&f)).unwrap();
    let expected = ((1.0_f64 / 3.0) / 60.0) / 100.0;
    assert_ne!(
        expected.to_bits(),
        (1.0_f64 / (3.0 * 60.0 * 100.0)).to_bits()
    );
    assert_eq!(
        output(&f, "game_constants.json")["game"]["bleed_base_fraction"]
            .as_f64()
            .unwrap()
            .to_bits(),
        expected.to_bits()
    );
    let r = receipt(&f);
    let b = &r["conversion"]["bindings"][26];
    assert_eq!(b["normalization_steps"][0]["divisor"], 3);
    assert_eq!(b["normalization_steps"][1]["divisor"], 60.0);
    assert_eq!(b["normalization_steps"][2]["divisor"], 100.0);
    assert_eq!(b["legacy"]["equals_normalized_f64"], false);
}

#[test]
fn known_numeric_shape_is_not_a_default_input_or_an_unknown_field_backdoor() {
    for mode in [
        "ward-not-allowed",
        "unknown",
        "unknown-existing",
        "array",
        "object",
        "missing-parent",
        "missing-owned-default",
        "null-ward",
        "string-owned",
        "non-owned-bad-type",
    ] {
        let mut f = fixture();
        let mut p = doc(&f, POLICY);
        let mut game = doc(&f, "snapshot/base/game_constants.json");
        let mut character = doc(&f, "snapshot/base/character_constants.json");
        match mode {
            "ward-not-allowed" => {
                p["bindings"][29]
                    .as_object_mut()
                    .unwrap()
                    .remove("allow_missing_leaf");
            }
            "unknown" => {
                p["bindings"][29]["target_pointer"] = json!("/not_a_dto_field");
            }
            "unknown-existing" => {
                p["bindings"][29]["target_pointer"] = json!("/future_numeric");
                character["future_numeric"] = json!(42);
            }
            "array" => {
                p["bindings"][30]["target_pointer"] = json!("/tree_versions/old/0/outer");
            }
            "object" => {
                p["bindings"][0]["target_pointer"] = json!("/game");
            }
            "missing-parent" => {
                game.as_object_mut().unwrap().remove("game");
            }
            "missing-owned-default" => {
                game["game"]
                    .as_object_mut()
                    .unwrap()
                    .remove("pin_damage_scale");
            }
            "null-ward" => character["ward_regeneration_per_minute_pct"] = Value::Null,
            "string-owned" => game["game"]["shock_min_effect"] = json!("17"),
            _ => character["base_life_constant"] = json!(false),
        }
        write(&f, "snapshot/base/game_constants.json", &game);
        write(&f, "snapshot/base/character_constants.json", &character);
        write(&f, POLICY, &p);
        reseal(&mut f);
        rejects(&f, "");
    }
    let mut f = fixture();
    let mut character = doc(&f, "snapshot/base/character_constants.json");
    character["ward_regeneration_per_minute_pct"] = json!(-1234);
    write(&f, "snapshot/base/character_constants.json", &character);
    reseal(&mut f);
    run(args(&f)).unwrap();
    assert_eq!(
        output(&f, "character_constants.json")["ward_regeneration_per_minute_pct"],
        777.0
    );
    assert_eq!(
        receipt(&f)["conversion"]["bindings"][29]["legacy"]["value"],
        -1234
    );
}

#[test]
fn source_ids_are_exact_and_allow_used_and_unused_trailing_whitespace() {
    let mut f = fixture();
    let row = raw_row(&f, "BaseShockMagnitude");
    f.values("GameConstants", row)["Id"] = json!("BaseShockMagnitude ");
    let mut p = doc(&f, POLICY);
    p["bindings"][11]["source_id"] = json!("BaseShockMagnitude ");
    p["bindings"][24]["source_id"] = json!("BaseShockMagnitude ");
    write(&f, POLICY, &p);
    reseal(&mut f);
    run(args(&f)).unwrap();
    assert_eq!(
        receipt(&f)["conversion"]["bindings"][11]["source_id"],
        "BaseShockMagnitude "
    );
    assert_eq!(
        output(&f, "game_constants.json")["game"]["shock_min_effect"],
        17.0
    );
    let mut f = fixture();
    let row = raw_row(&f, "BaseShockMagnitude");
    f.values("GameConstants", row)["Id"] = json!("BaseShockMagnitude ");
    reseal(&mut f);
    rejects(&f, "missing GameConstants Id");
}

#[test]
fn evaluated_rows_require_exact_i32_metadata_keys_and_nonzero_divisors() {
    for field in ["Value", "Divisor"] {
        for value in [
            Value::Null,
            json!(false),
            json!("1"),
            json!(1.25),
            json!(2147483648_u64),
            json!(-2147483649_i64),
        ] {
            let mut f = fixture();
            let row = raw_row(&f, "BaseShockMagnitude");
            f.values("GameConstants", row)[field] = value;
            reseal(&mut f);
            rejects(&f, field);
        }
    }
    for mode in [
        "zero",
        "missing-id",
        "duplicate-id",
        "empty-id",
        "physical-key",
        "missing-value",
        "schema-type",
        "schema-array",
        "schema-fk",
    ] {
        let mut f = fixture();
        let row = raw_row(&f, "BaseShockMagnitude");
        match mode {
            "zero" => f.values("GameConstants", row)["Divisor"] = json!(0),
            "missing-id" => f.values("GameConstants", row)["Id"] = json!("Gone"),
            "duplicate-id" => f.values("GameConstants", 0)["Id"] = json!("BaseShockMagnitude"),
            "empty-id" => f.values("GameConstants", 0)["Id"] = json!(""),
            "physical-key" => {
                f.documents.get_mut("tables/GameConstants.json").unwrap()["rows"][1]["_row"] =
                    json!(0)
            }
            "missing-value" => {
                f.values("GameConstants", row)
                    .as_object_mut()
                    .unwrap()
                    .remove("Value");
            }
            "schema-type" => {
                f.documents.get_mut("tables/GameConstants.json").unwrap()["columns"][1]["schema"]
                    ["type"] = json!("u32")
            }
            "schema-array" => {
                f.documents.get_mut("tables/GameConstants.json").unwrap()["columns"][1]["schema"]
                    ["array"] = json!(true)
            }
            _ => {
                f.documents.get_mut("tables/GameConstants.json").unwrap()["columns"][1]["schema"]
                    ["references"] = json!({"table":"Stats"})
            }
        }
        reseal(&mut f);
        rejects(&f, "");
    }
}

#[test]
fn declared_divisors_must_be_finite_nonzero_and_keep_signed_order() {
    for divisor in [json!(0), Value::Null, json!("100"), json!(false)] {
        let f = fixture();
        let mut p = doc(&f, POLICY);
        p["bindings"][0]["additional_divisors"] = json!([divisor]);
        write(&f, POLICY, &p);
        rejects(&f, "");
    }
    for invalid in [f64::INFINITY, f64::NEG_INFINITY, f64::NAN, 0.0, -0.0] {
        assert!(super::normalize(1, 1, &[invalid]).is_err());
    }
    assert!(super::normalize(i32::MAX, 1, &[f64::MIN_POSITIVE]).is_err());
    let f = fixture();
    let mut p = doc(&f, POLICY);
    p["bindings"][0]["additional_divisors"] = json!([-2, 5]);
    write(&f, POLICY, &p);
    run(args(&f)).unwrap();
    assert_eq!(
        output(&f, "game_constants.json")["game"]["light_stun_minimum_chance"]
            .as_f64()
            .unwrap(),
        7.0 / -2.0 / 5.0
    );
    let f = fixture();
    let mut p = doc(&f, POLICY);
    p["bindings"][0]["additional_divisors"] = json!([1e-320]);
    write(&f, POLICY, &p);
    rejects(&f, "nonfinite normalized");
}

#[test]
fn policy_rejects_unknown_schemas_targets_paths_and_duplicate_destinations() {
    for (pointer, value) in [
        ("/schema", json!("future")),
        ("/normalization", json!("multiply")),
        ("/target/game_patch", json!("old")),
        ("/target/facts_manifest_sha256", json!("0".repeat(64))),
        ("/source_snapshot/schema_version", json!(4)),
        ("/source_snapshot/snapshot_version", json!("elsewhere")),
        ("/source_snapshot/path", json!("../manifest.json")),
        ("/source_snapshot/sha256", json!("0".repeat(64))),
        ("/source_files/0/file", json!("enemy_presets.json")),
        (
            "/source_files/0/relative_path",
            json!("../game_constants.json"),
        ),
        ("/source_files/0/schema", json!("unknown")),
        ("/source_files/0/sha256", json!("bad")),
        ("/bindings/0/source_id", json!("")),
        ("/bindings/0/file", json!("unselected.json")),
        (
            "/bindings/0/target_pointer",
            json!("game/light_stun_minimum_chance"),
        ),
        ("/bindings/29/allow_missing_leaf", json!(1)),
        ("/evidence/source_check/sha256", json!("bad")),
    ] {
        let f = fixture();
        let mut p = doc(&f, POLICY);
        *p.pointer_mut(pointer).unwrap() = value;
        write(&f, POLICY, &p);
        rejects(&f, "");
    }
    for mode in [
        "duplicate-file",
        "missing-file",
        "duplicate-target",
        "unknown-key",
    ] {
        let f = fixture();
        let mut p = doc(&f, POLICY);
        match mode {
            "duplicate-file" => {
                let v = p["source_files"][0].clone();
                p["source_files"].as_array_mut().unwrap().push(v);
            }
            "missing-file" => {
                p["source_files"].as_array_mut().unwrap().pop();
            }
            "duplicate-target" => {
                let v = p["bindings"][0].clone();
                p["bindings"].as_array_mut().unwrap().push(v);
            }
            _ => p["unknown"] = json!(true),
        }
        write(&f, POLICY, &p);
        rejects(&f, "");
    }
}

#[test]
fn supplied_manifest_and_each_selected_file_are_independently_pinned_before_parse() {
    for path in [
        POLICY,
        MANIFEST,
        "snapshot/base/game_constants.json",
        "snapshot/base/character_constants.json",
        "snapshot/base/jewel_radii.json",
    ] {
        let f = fixture();
        let a = args(&f);
        fs::write(f.root.join(path), b"not JSON").unwrap();
        assert!(run(a).unwrap_err().contains("SHA256 mismatch"));
        assert!(!f.root.join("out").exists());
    }
    for path in [POLICY, MANIFEST, "snapshot/base/game_constants.json"] {
        let mut f = fixture();
        fs::write(f.root.join(path), b"{\"duplicate\":1,\"duplicate\":2}").unwrap();
        if path == MANIFEST {
            repin_manifest(&f);
        } else if path != POLICY {
            reseal(&mut f);
        }
        rejects(&f, "duplicate");
    }
    for mode in [
        "schema",
        "patch",
        "files-array",
        "object-sha",
        "bad-sha",
        "missing-sha",
        "pin-mismatch",
    ] {
        let f = fixture();
        let mut m = doc(&f, MANIFEST);
        match mode {
            "schema" => m["schema_version"] = json!(4),
            "patch" => m["poe_version"] = json!("0.5.5.5"),
            "files-array" => m["files"] = json!([]),
            "object-sha" => {
                m["files"]["base/game_constants.json"] = json!({"sha256":"a".repeat(64)})
            }
            "bad-sha" => m["files"]["base/game_constants.json"] = json!("bad"),
            "missing-sha" => {
                m["files"]
                    .as_object_mut()
                    .unwrap()
                    .remove("base/game_constants.json");
            }
            _ => m["files"]["base/game_constants.json"] = json!("0".repeat(64)),
        }
        write(&f, MANIFEST, &m);
        repin_manifest(&f);
        rejects(&f, "");
    }
    let f = fixture();
    assert!(!f.root.join("snapshot/overlay/not-provided.json").exists());
    run(args(&f)).unwrap();
    assert_eq!(
        receipt(&f)["conversion"]["legacy_snapshot"]["file_read_and_hash_verified"],
        true
    );
    // A selected path cannot escape its explicitly supplied snapshot directory through a link.
    let f = fixture();
    fs::rename(
        f.root.join("snapshot/base/jewel_radii.json"),
        f.root.join("outside.json"),
    )
    .unwrap();
    std::os::unix::fs::symlink(
        f.root.join("outside.json"),
        f.root.join("snapshot/base/jewel_radii.json"),
    )
    .unwrap();
    rejects(&f, "escapes");
}

#[test]
fn twelve_files_two_payloads_hashes_and_retained_value_evidence_replay() {
    let f = fixture();
    run(args(&f)).unwrap();
    run(Args {
        out: f.root.join("again"),
        ..args(&f)
    })
    .unwrap();
    let r = receipt(&f);
    assert_eq!(r["files"].as_object().unwrap().len(), 11);
    assert_eq!(
        r["input"]["selected_payloads"].as_object().unwrap().len(),
        2
    );
    assert_eq!(r["complete_runtime"], false);
    assert_eq!(r["adoption"], "not-adopted");
    assert_eq!(
        doc(&f, "out/_domain_candidate.json")["domain"],
        "ordinary-runtime-constants"
    );
    for absent in ["overlay", "manifest.json", "CURRENT", "native_runtime"] {
        assert!(!f.root.join("out").join(absent).exists());
    }
    for (input, output) in [
        (MANIFEST, "legacy-manifest.json"),
        (POLICY, "codec-policy.json"),
        (
            "snapshot/base/game_constants.json",
            "legacy-constants/game_constants.json",
        ),
        (
            "snapshot/base/character_constants.json",
            "legacy-constants/character_constants.json",
        ),
        (
            "snapshot/base/jewel_radii.json",
            "legacy-constants/jewel_radii.json",
        ),
    ] {
        assert_eq!(
            fs::read(f.root.join(input)).unwrap(),
            fs::read(f.root.join("out").join(output)).unwrap()
        );
    }
    for (path, id) in r["files"].as_object().unwrap() {
        let bytes = fs::read(f.root.join("out").join(path)).unwrap();
        assert_eq!(id["sha256"], sha256_hex(&bytes));
        assert_eq!(id["bytes"], bytes.len());
        assert_eq!(bytes, fs::read(f.root.join("again").join(path)).unwrap());
    }
    assert_eq!(
        fs::read(f.root.join("out/producer-receipt.json")).unwrap(),
        fs::read(f.root.join("again/producer-receipt.json")).unwrap()
    );
    for file in r["conversion"]["files"].as_array().unwrap() {
        let name = file["declaration"]["file"].as_str().unwrap();
        let mut retained = doc(&f, &format!("snapshot/base/{name}"));
        for ptr in file["owned_pointers"].as_array().unwrap() {
            let (parent, leaf) = super::leaf_parent(&mut retained, ptr.as_str().unwrap()).unwrap();
            parent.remove(&leaf);
        }
        assert_eq!(file["retained_minus_owned_value_sha256"], hash(&retained));
    }
    for b in r["conversion"]["bindings"].as_array().unwrap() {
        let row = b["source_row"].as_u64().unwrap() as usize;
        let raw = &f.documents["tables/GameConstants.json"]["rows"][row]["values"];
        assert_eq!(b["source_id"], raw["Id"]);
        assert_eq!(b["raw_value"], raw["Value"]);
        assert_eq!(
            b["source_payload_sha256"],
            sha256_hex(&fs::read(f.root.join("input/tables/GameConstants.json")).unwrap())
        );
        assert_eq!(
            b["source_pointers"]["value"],
            format!("/rows/{row}/values/Value")
        );
        assert_eq!(
            b["source_pointers"]["divisor"],
            format!("/rows/{row}/values/Divisor")
        );
    }
}

#[test]
fn cli_source_and_output_boundaries_fail_before_reservation() {
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
        "runtime-constants",
        "--codec-source",
        "manifest",
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
        ["--raw", "path"],
        ["--native-facts", "F"],
        ["--ordinary-domain", "other"],
    ] {
        assert!(parse_args(cli.into_iter().chain(pair).map(String::from)).is_err());
    }
    let f = fixture();
    for parent in ["input", "snapshot", "snapshot/base", "policy"] {
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
    fs::write(f.root.join("input/tables/GameConstants.json"), b"{}").unwrap();
    rejects(&f, "");
}

#[test]
fn legacy_archive_layer_uses_the_existing_directory_handle_guards() {
    use crate::native_facts::Output;
    let f = fixture();
    let mut out = Output::prepare_paths(&f.root.join("input"), &f.root.join("guarded")).unwrap();
    out.reserve().unwrap();
    out.write_new("_domain_candidate.json", b"{}").unwrap();
    assert!(
        out.write_legacy_constants(&BTreeMap::from([("../escape.json".into(), b"{}".to_vec())]))
            .is_err()
    );
    assert!(!f.root.join("guarded/legacy-constants").exists());
    std::os::unix::fs::symlink(
        f.root.join("input"),
        f.root.join("guarded/legacy-constants"),
    )
    .unwrap();
    assert!(
        out.write_legacy_constants(&BTreeMap::from([(
            "game_constants.json".into(),
            b"{}".to_vec()
        )]))
        .is_err()
    );
    assert!(!f.root.join("input/game_constants.json").exists());
}

#[test]
fn prior_domains_keep_files_and_receipts_without_runtime_constants_archives() {
    for (domain, bytes, count) in [
        (Domain::StatsMods, None, 5),
        (Domain::PassiveTree, None, 7),
        (Domain::GemQuality, None, 5),
        (Domain::GrantedEffectMinions, None, 6),
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
            Domain::PassiveTree => super::super::passive_tree::tests::fixture(),
            Domain::GemQuality => super::super::quality::tests::fixture(),
            Domain::GrantedEffectMinions => super::super::minion_links::tests::fixture(),
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
            Domain::PassiveTree => super::super::passive_tree::tests::args(&f),
            Domain::GemQuality => super::super::quality::tests::args(&f),
            Domain::GrantedEffectMinions => super::super::minion_links::tests::args(&f),
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
        let r = doc(&f, "out/producer-receipt.json");
        assert_eq!(r.as_object().unwrap().len(), 10);
        assert_eq!(r["files"].as_object().unwrap().len(), count);
        assert!(!f.root.join("out/legacy-constants").exists());
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

#[test]
fn an_unowned_missing_defaulted_leaf_is_not_materialized() {
    let f = fixture();
    let mut p = doc(&f, POLICY);
    p["bindings"].as_array_mut().unwrap().remove(29);
    write(&f, POLICY, &p);
    run(args(&f)).unwrap();
    assert!(
        output(&f, "character_constants.json")
            .get("ward_regeneration_per_minute_pct")
            .is_none()
    );
    assert_eq!(
        output(&f, "character_constants.json"),
        doc(&f, "snapshot/base/character_constants.json")
    );
}
