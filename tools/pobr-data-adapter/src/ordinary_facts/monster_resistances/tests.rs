use std::collections::BTreeMap;
use std::fs;

use pobr_data::catalog::{MonsterResistanceDef, MonsterResistancesDef};
use serde_json::{Value, json};

use super::super::stat_set_codec::tests::table;
use super::super::tests::Fixture;
use super::super::{Args, Domain, parse_args, run};
use crate::raw_source::sha256_hex;

pub(in crate::ordinary_facts) fn fixture() -> Fixture {
    let mut f = Fixture::new();
    let old: Vec<_> = f
        .documents
        .keys()
        .filter(|p| p.starts_with("tables/"))
        .cloned()
        .collect();
    f.documents.retain(|p, _| p == "source-lock.json");
    for (name, fields, values) in [
        (
            "MonsterVarieties",
            vec![
                ("Id", "string", false, None),
                ("MonsterType", "foreignrow", false, Some("MonsterTypes")),
                ("Name", "string", false, None),
            ],
            json!([
                {"Id":"ZMixed","MonsterType":1,"Name":"[DNT] still retained"},
                {"Id":"AEmpty","MonsterType":0,"Name":"Empty references"},
                {"Id":"Repeated","MonsterType":2,"Name":"Unknown"},
                {"Id":"AllZero","MonsterType":3,"Name":""},
                {"Id":"Negative","MonsterType":4,"Name":"Vulnerable"}
            ]),
        ),
        (
            "MonsterTypes",
            vec![
                ("Id", "string", false, None),
                (
                    "MonsterResistances",
                    "foreignrow",
                    true,
                    Some("MonsterResistances"),
                ),
                ("_index", "i32", false, None),
            ],
            json!([
                {"Id":"ZEmptyType","MonsterResistances":[],"_index":999},
                {"Id":"AMixedType","MonsterResistances":[2,0,1],"_index":998},
                {"Id":"RepeatedType","MonsterResistances":[1,0,1],"_index":997},
                {"Id":"ZeroType","MonsterResistances":[3],"_index":996},
                {"Id":"NegativeType","MonsterResistances":[1],"_index":995},
                {"Id":"UnusedType","MonsterResistances":[],"_index":994}
            ]),
        ),
        (
            "MonsterResistances",
            vec![
                ("Id", "string", false, None),
                ("Fire1", "i32", true, None),
                ("Cold1", "i32", true, None),
                ("Lightning1", "i32", true, None),
                ("Chaos1", "i32", true, None),
                ("Fire2", "i32", true, None),
            ],
            json!([
                {"Id":"ZFire","Fire1":[30],"Cold1":[-5],"Lightning1":[0],"Chaos1":[0],"Fire2":[999,12]},
                {"Id":"ACold","Fire1":[-10],"Cold1":[5],"Lightning1":[-30],"Chaos1":[7],"Fire2":[888]},
                {"Id":"Mixed","Fire1":[0],"Cold1":[40],"Lightning1":[0],"Chaos1":[-2],"Fire2":[]},
                {"Id":"Zero","Fire1":[0],"Cold1":[0],"Lightning1":[0],"Chaos1":[0],"Fire2":[777]},
                {"Id":"Unused","Fire1":[0],"Cold1":[0],"Lightning1":[0],"Chaos1":[0],"Fire2":[666]}
            ]),
        ),
    ] {
        f.documents
            .insert(format!("tables/{name}.json"), table(name, &fields, values));
    }
    f.seal();
    for path in old {
        if !f.documents.contains_key(&path) {
            fs::remove_file(f.root.join("input").join(path)).unwrap();
        }
    }
    f
}

pub(in crate::ordinary_facts) fn args(f: &Fixture) -> Args {
    Args {
        domain: Domain::MonsterResistances,
        ..f.args()
    }
}
fn read(f: &Fixture, file: &str) -> Value {
    crate::read_json(&f.root.join("out").join(file)).unwrap()
}
fn rejects(f: &Fixture, expected: &str) {
    let error = run(args(f)).unwrap_err();
    assert!(
        error.contains(expected),
        "expected {expected:?}, got {error:?}"
    );
    assert!(!f.root.join("out").exists());
}

#[test]
fn normal_dto_uses_all_signed_occurrences_and_keeps_every_variety_and_known_zero() {
    let f = fixture();
    run(args(&f)).unwrap();
    let dto: MonsterResistancesDef =
        crate::read_json(&f.root.join("out/base/monster_resistances.json")).unwrap();
    assert_eq!(dto.monsters.len(), 5);
    assert_eq!(
        dto.monsters["ZMixed"],
        MonsterResistanceDef {
            fire: 20,
            cold: 40,
            lightning: -30,
            chaos: 5
        }
    );
    assert_eq!(
        dto.monsters["Repeated"],
        MonsterResistanceDef {
            fire: 10,
            cold: 5,
            lightning: -60,
            chaos: 14
        }
    );
    assert_eq!(
        dto.monsters["Negative"],
        MonsterResistanceDef {
            fire: -10,
            cold: 5,
            lightning: -30,
            chaos: 7
        }
    );
    assert_eq!(dto.monsters["AEmpty"], MonsterResistanceDef::default());
    assert_eq!(dto.monsters["AllZero"], MonsterResistanceDef::default());
    assert_eq!(
        dto.monsters.keys().map(String::as_str).collect::<Vec<_>>(),
        ["AEmpty", "AllZero", "Negative", "Repeated", "ZMixed"]
    );
    let receipt = read(&f, "producer-receipt.json");
    let records = receipt["conversion"]["source_records"].as_array().unwrap();
    assert_eq!(records[0]["id"], "ZMixed");
    assert_eq!(records[0]["variety_source_row"], 0);
    assert_eq!(
        records[0]["monster_type"],
        json!({"id":"AMixedType","source_row":1})
    );
    assert_eq!(
        records[2]["resistance_occurrences"],
        json!([
            {"position":0,"source_row":1,"id":"ACold","normal_values":{"Fire1":[-10],"Cold1":[5],"Lightning1":[-30],"Chaos1":[7]}},
            {"position":1,"source_row":0,"id":"ZFire","normal_values":{"Fire1":[30],"Cold1":[-5],"Lightning1":[0],"Chaos1":[0]}},
            {"position":2,"source_row":1,"id":"ACold","normal_values":{"Fire1":[-10],"Cold1":[5],"Lightning1":[-30],"Chaos1":[7]}}
        ])
    );
    assert_eq!(records[1]["resistance_occurrences"], json!([]));
    let omitted = &receipt["conversion"]["unconsumed_source"];
    assert_eq!(omitted[0]["columns"][0]["physical_fields"], json!(["Name"]));
    assert_eq!(
        omitted[1]["columns"][0]["physical_fields"],
        json!(["_index"])
    );
    assert_eq!(
        omitted[2]["columns"][0]["physical_fields"],
        json!(["Fire2"])
    );
    assert_eq!(
        omitted[2]["source_row_range"],
        json!({"start":0,"end_exclusive":5})
    );
}

#[test]
fn required_singletons_and_i32_values_validate_even_on_unused_rows() {
    for field in super::NORMAL_COLUMNS {
        for invalid in [
            json!([]),
            json!([0, 1]),
            json!(null),
            json!([null]),
            json!(0),
            json!([true]),
            json!([1.0]),
            json!(["0"]),
            json!([2147483648_i64]),
            json!([-2147483649_i64]),
        ] {
            let mut f = fixture();
            f.values("MonsterResistances", 4)[field] = invalid;
            f.seal();
            assert!(run(args(&f)).is_err(), "{field}");
            assert!(!f.root.join("out").exists());
        }
    }
}

#[test]
fn signed_overflow_in_each_component_returns_error_before_writing() {
    for field in super::NORMAL_COLUMNS {
        for (edge, step) in [(i32::MAX, 1), (i32::MIN, -1)] {
            let mut f = fixture();
            f.values("MonsterResistances", 2)[field] = json!([0]);
            f.values("MonsterResistances", 0)[field] = json!([edge]);
            f.values("MonsterResistances", 1)[field] = json!([step]);
            f.seal();
            rejects(&f, &format!("{field} i32 sum overflow"));
        }
    }
}

#[test]
fn all_foreign_keys_identity_and_column_contracts_are_required() {
    for case in [
        "type-null",
        "type-negative",
        "type-float",
        "type-unknown",
        "refs-null",
        "refs-scalar",
        "ref-null",
        "ref-negative",
        "ref-float",
        "ref-unknown",
        "unused-type-ref",
        "duplicate-variety",
        "duplicate-type",
        "duplicate-resistance",
        "empty-id",
        "physical-key",
        "missing-value",
        "missing-column",
        "type-metadata",
        "array-metadata",
        "target-metadata",
    ] {
        let mut f = fixture();
        match case {
            "type-null" => f.values("MonsterVarieties", 0)["MonsterType"] = json!(null),
            "type-negative" => f.values("MonsterVarieties", 0)["MonsterType"] = json!(-1),
            "type-float" => f.values("MonsterVarieties", 0)["MonsterType"] = json!(1.0),
            "type-unknown" => f.values("MonsterVarieties", 0)["MonsterType"] = json!(999),
            "refs-null" => f.values("MonsterTypes", 1)["MonsterResistances"] = json!(null),
            "refs-scalar" => f.values("MonsterTypes", 1)["MonsterResistances"] = json!(1),
            "ref-null" => f.values("MonsterTypes", 1)["MonsterResistances"] = json!([null]),
            "ref-negative" => f.values("MonsterTypes", 1)["MonsterResistances"] = json!([-1]),
            "ref-float" => f.values("MonsterTypes", 1)["MonsterResistances"] = json!([1.0]),
            "ref-unknown" => f.values("MonsterTypes", 1)["MonsterResistances"] = json!([999]),
            "unused-type-ref" => f.values("MonsterTypes", 5)["MonsterResistances"] = json!([999]),
            "duplicate-variety" => f.values("MonsterVarieties", 1)["Id"] = json!("ZMixed"),
            "duplicate-type" => f.values("MonsterTypes", 1)["Id"] = json!("ZEmptyType"),
            "duplicate-resistance" => f.values("MonsterResistances", 1)["Id"] = json!("ZFire"),
            "empty-id" => f.values("MonsterVarieties", 0)["Id"] = json!(""),
            "physical-key" => {
                f.documents.get_mut("tables/MonsterTypes.json").unwrap()["rows"][1]["_row"] =
                    json!(0)
            }
            "missing-value" => {
                f.values("MonsterTypes", 5)
                    .as_object_mut()
                    .unwrap()
                    .remove("MonsterResistances");
            }
            "missing-column" => {
                f.documents
                    .get_mut("tables/MonsterResistances.json")
                    .unwrap()["columns"]
                    .as_array_mut()
                    .unwrap()
                    .remove(1);
            }
            "type-metadata" => {
                f.documents
                    .get_mut("tables/MonsterResistances.json")
                    .unwrap()["columns"][1]["schema"]["type"] = json!("u32")
            }
            "array-metadata" => {
                f.documents
                    .get_mut("tables/MonsterResistances.json")
                    .unwrap()["columns"][1]["schema"]["array"] = json!(false)
            }
            "target-metadata" => {
                f.documents.get_mut("tables/MonsterVarieties.json").unwrap()["columns"][1]["schema"]
                    ["references"]["table"] = json!("MonsterResistances")
            }
            _ => unreachable!(),
        }
        f.seal();
        assert!(run(args(&f)).is_err(), "{case}");
        assert!(!f.root.join("out").exists(), "{case}");
    }
}

#[test]
fn output_is_only_a_replayable_pinned_normal_domain_candidate() {
    let f = fixture();
    run(args(&f)).unwrap();
    run(Args {
        out: f.root.join("again"),
        ..args(&f)
    })
    .unwrap();
    let receipt = read(&f, "producer-receipt.json");
    assert_eq!(receipt["complete_runtime"], false);
    assert_eq!(receipt["adoption"], "not-adopted");
    assert_eq!(
        receipt["input"]["selected_payloads"]
            .as_object()
            .unwrap()
            .len(),
        4
    );
    assert_eq!(receipt["files"].as_object().unwrap().len(), 4);
    assert_eq!(
        read(&f, "_domain_candidate.json")["domain"],
        "ordinary-monster-resistances"
    );
    let meta = &read(&f, "base/monster_resistances.json")["_meta"];
    assert_eq!(meta["poe_version"], "0.5.5.5");
    assert_eq!(
        meta["facts_manifest_sha256"],
        args(&f).expected_manifest_sha256
    );
    assert_eq!(meta["inputs"].as_object().unwrap().len(), 3);
    assert!(meta.get("source_receipt_sha256").is_none());
    assert!(!f.root.join("out/overlay").exists());
    assert!(!f.root.join("out/manifest.json").exists());
    assert!(!f.root.join("out/quality-policy.json").exists());
    assert!(!f.root.join("out/codec-policy.json").exists());
    for (file, pin) in receipt["files"].as_object().unwrap() {
        let bytes = fs::read(f.root.join("out").join(file)).unwrap();
        assert_eq!(pin["sha256"], sha256_hex(&bytes));
        assert_eq!(pin["bytes"], bytes.len());
        assert_eq!(bytes, fs::read(f.root.join("again").join(file)).unwrap());
    }
    assert_eq!(
        fs::read(f.root.join("out/producer-receipt.json")).unwrap(),
        fs::read(f.root.join("again/producer-receipt.json")).unwrap()
    );
    assert_eq!(
        fs::read(f.root.join("input/manifest.json")).unwrap(),
        fs::read(f.root.join("out/source-manifest.json")).unwrap()
    );
}

#[test]
fn pin_patch_argument_and_output_guards_apply_without_auxiliary_inputs() {
    let base = [
        "--ordinary-facts",
        "F",
        "--facts-manifest-sha256",
        "pin",
        "--version",
        "0.5.5.5",
        "--out",
        "fresh",
        "--ordinary-domain",
        "monster-resistances",
    ];
    assert!(matches!(
        crate::parse_args_from(base.into_iter().map(String::from)),
        Ok(crate::Mode::OrdinaryFacts(_))
    ));
    for extra in [
        ["--quality-policy", "p"],
        ["--quality-policy-sha256", "sha"],
        ["--codec-source", "c"],
        ["--codec-source-sha256", "sha"],
        ["--codec-policy", "p"],
        ["--codec-policy-sha256", "sha"],
        ["--ordinary-domain", "monster-resistances"],
        ["--raw", "old"],
    ] {
        assert!(parse_args(base.into_iter().chain(extra).map(String::from)).is_err());
    }
    assert!(parse_args(base.into_iter().chain(["--out"]).map(String::from)).is_err());
    for kind in ["manifest", "payload", "patch"] {
        let f = fixture();
        let mut a = args(&f);
        match kind {
            "manifest" => a.expected_manifest_sha256 = "0".repeat(64),
            "patch" => a.version = "old".into(),
            _ => fs::write(f.root.join("input/tables/MonsterResistances.json"), b"{}").unwrap(),
        }
        assert!(run(a).is_err());
        assert!(!f.root.join("out").exists());
    }
    let f = fixture();
    for out in [
        f.root.join("input/new"),
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data")
            .canonicalize()
            .unwrap()
            .join(format!(
                "monster-forbidden-{}",
                f.root.file_name().unwrap().to_string_lossy()
            )),
    ] {
        assert!(
            run(Args {
                out: out.clone(),
                ..args(&f)
            })
            .is_err()
        );
        assert!(!out.exists());
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
}

#[test]
fn old_raw_wrapper_shares_the_join_and_preserves_exact_metadata_hashes_and_guards() {
    let f = fixture();
    let raw = f.root.join("raw");
    let base = f.root.join("raw-base");
    fs::create_dir_all(raw.join("English")).unwrap();
    fs::create_dir_all(raw.join(crate::ZH_TW)).unwrap();
    fs::create_dir(&base).unwrap();
    let mut inputs = BTreeMap::new();
    for table in super::TABLES {
        let rows: Vec<_> = f.documents[&format!("tables/{table}.json")]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["values"].clone())
            .collect();
        let bytes = serde_json::to_vec(&rows).unwrap();
        let name = format!("English/{table}.json");
        inputs.insert(name.clone(), sha256_hex(&bytes));
        fs::write(raw.join(name), bytes).unwrap();
    }
    let source = json!({"schema":"official-raw-export/v1","patch":"0.5.5.5","files":inputs});
    let receipt = serde_json::to_vec_pretty(&source).unwrap();
    fs::write(raw.join("export-source.json"), &receipt).unwrap();
    crate::raw_source::verify(&raw, "0.5.5.5").unwrap();
    crate::monsters::write(&raw, &base, "0.5.5.5").unwrap();
    run(args(&f)).unwrap();
    let ordinary = read(&f, "base/monster_resistances.json");
    let old = fs::read(base.join("monster_resistances.json")).unwrap();
    let expected = json!({"_meta":{"schema":"monster-resistances/v1","generator":"pobr-data-adapter","poe_version":"0.5.5.5","source_receipt_sha256":sha256_hex(&receipt),"inputs":inputs},"monsters":ordinary["monsters"]});
    assert_eq!(old, super::super::pretty_bytes(&expected).unwrap());
    // Original CLI still rejects its receipt/patch/hash before reaching any output.
    let old_out = f.root.join("bad-raw-output");
    let wrong_patch = crate::Args {
        raw: raw.clone(),
        out: old_out.clone(),
        patch: "different".into(),
        strict_columns: false,
    };
    assert!(crate::run(wrong_patch).unwrap_err().contains("patch"));
    assert!(!old_out.exists());
    let mut bad = source.clone();
    bad["schema"] = json!("unknown");
    fs::write(raw.join("export-source.json"), bad.to_string()).unwrap();
    assert!(
        crate::raw_source::verify(&raw, "0.5.5.5")
            .unwrap_err()
            .contains("patch")
    );
    fs::write(raw.join("export-source.json"), &receipt).unwrap();
    fs::write(raw.join("English/MonsterTypes.json"), b"[]").unwrap();
    assert!(
        crate::raw_source::verify(&raw, "0.5.5.5")
            .unwrap_err()
            .contains("fingerprint")
    );
    assert!(matches!(
        crate::parse_args_from(
            ["--raw", "old", "--out", "fresh", "--patch", "0.5.5.5"]
                .into_iter()
                .map(String::from)
        ),
        Ok(crate::Mode::BaseItems(_))
    ));
}

#[test]
fn existing_base_and_codec_domains_keep_bytes_and_receipt_shapes() {
    for (domain, fixture_bytes, file_count) in [
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
    ] {
        let mut f = match domain {
            Domain::StatSetCodec => super::super::stat_set_codec::tests::fixture(),
            Domain::GemEffects => super::super::gem_effects::tests::fixture(),
            _ => Fixture::new(),
        };
        if let Some(bytes) = fixture_bytes {
            let tables: BTreeMap<String, Value> = serde_json::from_slice(bytes).unwrap();
            f.documents.retain(|p, _| p == "source-lock.json");
            f.documents.extend(
                tables
                    .into_iter()
                    .map(|(t, d)| (format!("tables/{t}.json"), d)),
            );
            f.seal();
        }
        let old_args = || match domain {
            Domain::StatSetCodec => super::super::stat_set_codec::tests::args(&f),
            Domain::GemEffects => super::super::gem_effects::tests::args(&f),
            _ => Args { domain, ..f.args() },
        };
        run(old_args()).unwrap();
        let new = fixture();
        run(args(&new)).unwrap();
        run(Args {
            out: f.root.join("again"),
            ..old_args()
        })
        .unwrap();
        let r = read(&f, "producer-receipt.json");
        assert_eq!(r.as_object().unwrap().len(), 10);
        assert_eq!(r["files"].as_object().unwrap().len(), file_count);
        assert!(!f.root.join("out/base/monster_resistances.json").exists());
        for path in r["files"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .chain(["producer-receipt.json"])
        {
            assert_eq!(
                fs::read(f.root.join("out").join(path)).unwrap(),
                fs::read(f.root.join("again").join(path)).unwrap()
            );
        }
    }
}
