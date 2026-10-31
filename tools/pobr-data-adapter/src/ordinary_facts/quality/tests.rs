use std::collections::BTreeMap;
use std::fs;

use pobr_data::catalog::GemQualityStatsDef;
use serde_json::{Value, json};

use super::super::tests::Fixture;
use super::super::{Args, Domain, parse_args, run};
use crate::native_facts::Output;
use crate::raw_source::sha256_hex;

fn table(name: &str, columns: &[(&str, &str, bool, Option<&str>)], values: Value) -> Value {
    let rows: Vec<_> = values
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(i, v)| json!({"_row":i,"values":v}))
        .collect();
    json!({
        "table":name, "raw_resource":format!("Data/Balance/{name}.datc64"), "row_key":"_row",
        "physical_fields":columns.iter().map(|c| c.0).collect::<Vec<_>>(),
        "columns":columns.iter().enumerate().map(|(i, (field, kind, array, target))| json!({
            "source_column":i, "physical_fields":[field],
            "schema":{"name":field, "type":kind,"array":array,"interval":false,
                "references":target.map(|t| json!({"table":t}))}
        })).collect::<Vec<_>>(),
        "validation":{"rows":rows.len(),"schema_row_bytes":8,"actual_row_bytes":8,"warnings":[]},
        "rows":rows
    })
}

fn fixture() -> Fixture {
    let mut f = Fixture::new();
    let old: Vec<_> = f
        .documents
        .keys()
        .filter(|p| p.starts_with("tables/"))
        .cloned()
        .collect();
    f.documents.retain(|path, _| path == "source-lock.json");
    for (name, doc) in [
        (
            "GrantedEffects",
            table(
                "GrantedEffects",
                &[
                    ("Id", "string", false, None),
                    ("IsSupport", "bool", false, None),
                    ("_index", "i32", false, None),
                ],
                json!([
                    {"Id":"z_attack","IsSupport":false,"_index":999},
                    {"Id":"a_non_gem_support","IsSupport":true,"_index":998},
                    {"Id":"excluded","IsSupport":false,"_index":997}
                ]),
            ),
        ),
        (
            "Stats",
            table(
                "Stats",
                &[("Id", "string", false, None)],
                json!([{"Id":"damage"},{"Id":"cost"}]),
            ),
        ),
        (
            "GrantedEffectQualityStats",
            table(
                "GrantedEffectQualityStats",
                &[
                    ("GrantedEffect", "foreignrow", false, Some("GrantedEffects")),
                    ("Stats", "foreignrow", true, Some("Stats")),
                    ("StatsValuesPermille", "i32", true, None),
                    ("ApplyToStatSets", "i32", true, None),
                    ("AltStats", "foreignrow", true, Some("Stats")),
                    ("AltStatValuesPermille", "i32", true, None),
                    ("AltApplyToStatSets", "i32", true, None),
                    ("AddTypes", "foreignrow", true, Some("ActiveSkillType")),
                    (
                        "AddMinionTypes",
                        "foreignrow",
                        true,
                        Some("ActiveSkillType"),
                    ),
                ],
                json!([
                    {"GrantedEffect":1,"Stats":[0,1,0],"StatsValuesPermille":[550,-50,0],
                     "ApplyToStatSets":[0,2,0],"AltStats":[1],"AltStatValuesPermille":[100],
                     "AltApplyToStatSets":[0,1],"AddTypes":[73],"AddMinionTypes":[]},
                    {"GrantedEffect":0,"Stats":[1],"StatsValuesPermille":[-1000],
                     "ApplyToStatSets":[],"AltStats":[],"AltStatValuesPermille":[],
                     "AltApplyToStatSets":[],"AddTypes":[],"AddMinionTypes":[91]},
                    {"GrantedEffect":2,"Stats":[0],"StatsValuesPermille":[1000],
                     "ApplyToStatSets":[0],"AltStats":[1],"AltStatValuesPermille":[-250],
                     "AltApplyToStatSets":[2],"AddTypes":[],"AddMinionTypes":[]}
                ]),
            ),
        ),
    ] {
        f.documents.insert(format!("tables/{name}.json"), doc);
    }
    f.seal();
    for path in old {
        if !f.documents.contains_key(&path) {
            fs::remove_file(f.root.join("input").join(path)).unwrap();
        }
    }
    fs::create_dir(f.root.join("policy")).unwrap();
    let mut policy: Value = serde_json::from_slice(include_bytes!(
        "../../../../../pipeline/gem-quality/0.5.5.5-facts-policy.json"
    ))
    .unwrap();
    policy["target"]["facts_manifest_sha256"] = f.args().expected_manifest_sha256.into();
    policy["compatibility"]["effects"] = json!(["z_attack", "a_non_gem_support"]);
    policy["compatibility"]["excluded_effects"] = json!({"excluded":"Synthetic compatibility gap"});
    write_policy(&f, &policy);
    f
}

fn write_policy(f: &Fixture, policy: &Value) {
    fs::write(
        f.root.join("policy/policy.json"),
        serde_json::to_vec_pretty(policy).unwrap(),
    )
    .unwrap();
}

fn policy(f: &Fixture) -> Value {
    crate::read_json(&f.root.join("policy/policy.json")).unwrap()
}

fn args(f: &Fixture) -> Args {
    let path = f.root.join("policy/policy.json");
    Args {
        domain: Domain::GemQuality,
        quality_policy: Some(super::Args {
            expected_sha256: sha256_hex(&fs::read(&path).unwrap()),
            path,
        }),
        ..f.args()
    }
}

fn reseal(f: &mut Fixture) {
    f.seal();
    let mut policy = policy(f);
    policy["target"]["facts_manifest_sha256"] = f.args().expected_manifest_sha256.into();
    write_policy(f, &policy);
}

fn rejects(f: &Fixture, expected: &str) {
    let error = run(args(f)).unwrap_err();
    assert!(
        error.contains(expected),
        "expected {expected:?}, got {error:?}"
    );
    assert!(!f.root.join("out").exists(), "failure reserved output");
}

fn read(f: &Fixture, file: &str) -> Value {
    crate::read_json(&f.root.join("out").join(file)).unwrap()
}

#[test]
fn physical_quality_keeps_original_dto_rates_order_alt_scope_and_non_gem_support() {
    let f = fixture();
    run(args(&f)).unwrap();
    let doc = read(&f, "overlay/gem_quality_stats.json");
    let dto: GemQualityStatsDef = serde_json::from_value(doc.clone()).unwrap();
    assert_eq!(dto.effects.len(), 2);
    assert_eq!(
        doc["effects"],
        json!([
            {"effect_id":"a_non_gem_support","stats":[
                {"stat":"damage","per_quality_rate":0.55},
                {"stat":"cost","per_quality_rate":-0.05},
                {"stat":"damage","per_quality_rate":0.0},
                {"stat":"cost","per_quality_rate":0.1,"alt":true}]},
            {"effect_id":"z_attack","stats":[{"stat":"cost","per_quality_rate":-1.0}]}
        ])
    );
    assert_eq!(doc["_meta"]["poe_version"], "0.5.5.5");
    assert_eq!(doc["_meta"]["model_reference"]["game_patch"], "0.5.5.4.2");
    assert_eq!(
        doc["_meta"]["unapplied_stat_set_scopes"],
        json!({
            "a_non_gem_support":{"main":[0,2,0],"alt":[0,1]},
            "excluded":{"main":[0],"alt":[2]}
        })
    );
    let receipt = read(&f, "producer-receipt.json");
    let conversion = &receipt["conversion"];
    assert_eq!(
        conversion["owners"][0],
        json!({"effect_id":"a_non_gem_support","source_row":0,"effect_source_row":1})
    );
    assert_eq!(
        conversion["excluded_effects"][0]["raw_fields"]["AltStatValuesPermille"],
        json!([-250])
    );
    assert_eq!(
        conversion["excluded_effects"][0]["reason"],
        "Synthetic compatibility gap"
    );
    assert_eq!(
        conversion["unmapped_source_columns"][1]["table"],
        "GrantedEffectQualityStats"
    );
    assert_eq!(
        conversion["unmapped_source_columns"][1]["columns"][0]["physical_fields"],
        json!(["AddTypes"])
    );
    assert_eq!(
        conversion["unmapped_source_columns"][1]["source_row_range"]["end_exclusive"],
        3
    );
    assert_eq!(receipt["complete_runtime"], false);
    assert_eq!(receipt["adoption"], "not-adopted");
    assert_eq!(
        receipt["input"]["selected_payloads"]
            .as_object()
            .unwrap()
            .len(),
        4 // Three selected tables plus the verified source lock.
    );
    assert!(!f.root.join("out/base").exists());
    assert!(!f.root.join("out/manifest.json").exists());
    assert_eq!(
        read(&f, "_domain_candidate.json")["domain"],
        "ordinary-gem-quality"
    );
    assert_eq!(
        fs::read(f.root.join("out/quality-policy.json")).unwrap(),
        fs::read(f.root.join("policy/policy.json")).unwrap()
    );
    let pins = receipt["files"].as_object().unwrap();
    assert_eq!(pins.len(), 5); // Plus the final receipt, which cannot fingerprint itself.
    for (path, pin) in pins {
        let bytes = fs::read(f.root.join("out").join(path)).unwrap();
        assert_eq!(pin["sha256"], sha256_hex(&bytes), "{path}");
        assert_eq!(pin["bytes"], bytes.len(), "{path}");
    }
    assert_eq!(
        conversion["quality_policy"]["sha256"],
        pins["quality-policy.json"]["sha256"]
    );
    let second = Args {
        out: f.root.join("second"),
        ..args(&f)
    };
    run(second).unwrap();
    for path in pins
        .keys()
        .map(String::as_str)
        .chain(["producer-receipt.json"])
    {
        assert_eq!(
            fs::read(f.root.join("out").join(path)).unwrap(),
            fs::read(f.root.join("second").join(path)).unwrap()
        );
    }
    let before = fs::read(f.root.join("out/producer-receipt.json")).unwrap();
    assert!(run(args(&f)).is_err());
    assert_eq!(
        before,
        fs::read(f.root.join("out/producer-receipt.json")).unwrap()
    );
}

#[test]
fn physical_quality_rejects_bad_pairs_references_types_and_scope_without_clamping() {
    for (row, field, value, expected) in [
        (0, "GrantedEffect", json!(999), "dangling GrantedEffect"),
        (0, "GrantedEffect", json!(null), "invalid type"),
        (0, "Stats", json!([0, 999, 0]), "dangling stat"),
        (0, "Stats", json!([0, null, 0]), "invalid type"),
        (
            0,
            "StatsValuesPermille",
            json!([550, -50]),
            "length mismatch",
        ),
        (0, "AltStatValuesPermille", json!([]), "length mismatch"),
        (
            0,
            "StatsValuesPermille",
            json!([0, 2147483648_i64, 0]),
            "invalid value",
        ),
        (0, "StatsValuesPermille", json!([0, 0.5, 0]), "invalid type"),
        (0, "StatsValuesPermille", json!([0, "0", 0]), "invalid type"),
        (0, "ApplyToStatSets", json!([-1]), "invalid value"),
        (0, "ApplyToStatSets", json!([null]), "invalid type"),
        (
            0,
            "AltApplyToStatSets",
            json!([2147483648_u64]),
            "scope exceeds source i32",
        ),
        (2, "Stats", json!([999]), "dangling stat"), // Excluded still validated.
        (2, "AltStats", json!([999]), "dangling stat"),
        (1, "GrantedEffect", json!(1), "duplicate effect"),
    ] {
        let mut f = fixture();
        f.values("GrantedEffectQualityStats", row)[field] = value;
        reseal(&mut f);
        rejects(&f, expected);
    }
    let mut f = fixture();
    for field in [
        "Stats",
        "StatsValuesPermille",
        "AltStats",
        "AltStatValuesPermille",
    ] {
        f.values("GrantedEffectQualityStats", 0)[field] = json!([]);
    }
    reseal(&mut f);
    rejects(&f, "reviewed effect has no quality stats");
}

#[test]
fn quality_physical_identity_and_consumed_column_metadata_are_checked() {
    for case in [
        "physical-key",
        "stable-id",
        "id-type",
        "missing-column",
        "type",
        "array",
        "fk-target",
    ] {
        let mut f = fixture();
        match case {
            "physical-key" => {
                f.documents.get_mut("tables/GrantedEffects.json").unwrap()["rows"][1]["_row"] =
                    json!(0)
            }
            "stable-id" => f.values("GrantedEffects", 1)["Id"] = json!("z_attack"),
            "id-type" => f.values("Stats", 0)["Id"] = json!(0),
            "missing-column" => {
                f.values("GrantedEffectQualityStats", 0)
                    .as_object_mut()
                    .unwrap()
                    .remove("AltStats");
            }
            "type" => {
                f.documents
                    .get_mut("tables/GrantedEffectQualityStats.json")
                    .unwrap()["columns"][2]["schema"]["type"] = json!("f32")
            }
            "array" => {
                f.documents
                    .get_mut("tables/GrantedEffectQualityStats.json")
                    .unwrap()["columns"][1]["schema"]["array"] = json!(false)
            }
            "fk-target" => {
                f.documents
                    .get_mut("tables/GrantedEffectQualityStats.json")
                    .unwrap()["columns"][1]["schema"]["references"]["table"] =
                    json!("GrantedEffects")
            }
            _ => unreachable!(),
        }
        reseal(&mut f);
        assert!(run(args(&f)).is_err(), "{case}");
        assert!(!f.root.join("out").exists(), "{case}");
    }
}

#[test]
fn quality_policy_requires_complete_disjoint_classification_and_exact_target_evidence() {
    for (pointer, value, expected) in [
        ("/schema", json!("unknown"), "policy schema"),
        ("/extra", json!(true), "unknown field"),
        (
            "/target/game_patch",
            json!("0.5.5.4.2"),
            "target patch / facts manifest",
        ),
        (
            "/target/facts_manifest_sha256",
            json!("0".repeat(64)),
            "target patch / facts manifest",
        ),
        (
            "/compatibility/reference_sha256",
            json!("not-sha"),
            "hexadecimal",
        ),
        (
            "/reference/policy_sha256",
            json!("F".repeat(64)),
            "hexadecimal",
        ),
        (
            "/target_applicability/baseline_sha256",
            json!("0".repeat(64)),
            "applicability evidence",
        ),
        (
            "/compatibility/scope_policy",
            json!("apply_sets"),
            "unsupported quality scope",
        ),
        (
            "/compatibility/effects",
            json!(["z_attack", "z_attack"]),
            "overlapping",
        ),
        (
            "/compatibility/effects",
            json!(["z_attack", ""]),
            "overlapping",
        ),
        (
            "/compatibility/effects",
            json!(["z_attack", "excluded"]),
            "overlapping",
        ),
        (
            "/compatibility/effects",
            json!(["z_attack"]),
            "no reviewed compatibility decision",
        ),
        (
            "/compatibility/effects",
            json!(["z_attack", "a_non_gem_support", "gone"]),
            "disappeared",
        ),
        (
            "/compatibility/excluded_effects",
            json!({"excluded":" "}),
            "overlapping",
        ),
        (
            "/compatibility/excluded_effects",
            json!({}),
            "no reviewed compatibility decision",
        ),
    ] {
        let f = fixture();
        let mut p = policy(&f);
        if pointer == "/extra" {
            p["extra"] = value;
        } else {
            *p.pointer_mut(pointer).unwrap() = value;
        }
        write_policy(&f, &p);
        rejects(&f, expected);
    }
    let mut f = fixture();
    f.values("GrantedEffects", 2)["Id"] = json!("new_owner");
    reseal(&mut f);
    rejects(&f, "no reviewed compatibility decision");
    let f = fixture();
    fs::write(
        f.root.join("policy/policy.json"),
        b"{\"schema\":1,\"schema\":2}",
    )
    .unwrap();
    rejects(&f, "duplicate JSON key");
    fs::write(
        f.root.join("policy/policy.json"),
        br#"{"compatibility":{"excluded_effects":{"excluded":"one","excluded":"two"}}}"#,
    )
    .unwrap();
    rejects(&f, "duplicate JSON key");
}

#[test]
fn quality_external_pins_and_version_fail_before_reservation_or_policy_parsing() {
    let f = fixture();
    let mut a = args(&f);
    fs::write(f.root.join("policy/policy.json"), b"invalid JSON").unwrap();
    assert!(run(a).unwrap_err().contains("checked before parsing"));
    a = args(&f);
    a.quality_policy.as_mut().unwrap().expected_sha256 = "wrong".into();
    assert!(run(a).unwrap_err().contains("hexadecimal"));
    assert!(!f.root.join("out").exists());
    for case in ["manifest", "payload", "version"] {
        let f = fixture();
        let mut a = args(&f);
        match case {
            "manifest" => a.expected_manifest_sha256 = "0".repeat(64),
            "payload" => fs::write(f.root.join("input/tables/Stats.json"), b"{}").unwrap(),
            "version" => a.version = "wrong".into(),
            _ => unreachable!(),
        }
        assert!(run(a).is_err(), "{case}");
        assert!(!f.root.join("out").exists());
    }
}

#[test]
fn quality_options_are_explicit_exclusive_and_dispatched_by_the_existing_cli() {
    let base = [
        "--ordinary-facts",
        "F",
        "--facts-manifest-sha256",
        "pin",
        "--version",
        "0.5.5.5",
        "--out",
        "fresh",
    ];
    let quality = [
        "--ordinary-domain",
        "gem-quality",
        "--quality-policy",
        "policy",
        "--quality-policy-sha256",
        "pin",
    ];
    let parsed = parse_args(base.into_iter().chain(quality).map(String::from)).unwrap();
    assert_eq!(parsed.domain, Domain::GemQuality);
    assert!(matches!(
        crate::parse_args_from(base.into_iter().chain(quality).map(String::from)),
        Ok(crate::Mode::OrdinaryFacts(_))
    ));
    for suffix in [
        vec!["--ordinary-domain", "gem-quality"],
        vec![
            "--ordinary-domain",
            "gem-quality",
            "--quality-policy",
            "policy",
        ],
        vec![
            "--ordinary-domain",
            "gem-quality",
            "--quality-policy-sha256",
            "pin",
        ],
        vec!["--quality-policy", "policy"],
        vec!["--quality-policy-sha256", "pin"],
        vec![
            "--ordinary-domain",
            "base-items",
            "--quality-policy",
            "policy",
        ],
        vec![
            "--ordinary-domain",
            "skills",
            "--quality-policy-sha256",
            "pin",
        ],
    ] {
        assert!(parse_args(base.into_iter().chain(suffix).map(String::from)).is_err());
    }
    for suffix in [
        vec!["--quality-policy", "again"],
        vec!["--quality-policy-sha256", "again"],
        vec!["--ordinary-domain", "gem-quality"],
        vec!["--quality-source", "old"],
        vec!["--gem-quality", "old"],
    ] {
        assert!(
            parse_args(
                base.into_iter()
                    .chain(quality)
                    .chain(suffix)
                    .map(String::from)
            )
            .is_err()
        );
    }
    assert!(
        crate::parse_args_from(
            [
                "--raw",
                "F",
                "--out",
                "out",
                "--patch",
                "test",
                "--quality-policy",
                "p"
            ]
            .into_iter()
            .map(String::from)
        )
        .is_err()
    );
}

#[test]
fn quality_overlay_uses_the_existing_directory_handle_and_protected_input_boundary() {
    let f = fixture();
    for out in [f.root.join("input/new"), f.root.join("policy/new")] {
        assert!(
            run(Args {
                out: out.clone(),
                ..args(&f)
            })
            .is_err()
        );
        assert!(!out.exists());
    }
    let out = f.root.join("held");
    let mut output = Output::prepare_paths(&f.root.join("input"), &out).unwrap();
    output.reserve().unwrap();
    output.write_new("_domain_candidate.json", b"{}").unwrap();
    assert!(
        output
            .write_overlay(&BTreeMap::from([("../escape.json".into(), vec![])]))
            .is_err()
    );
    output
        .write_overlay(&BTreeMap::from([("quality.json".into(), b"{}".to_vec())]))
        .unwrap();
    fs::rename(out.join("overlay"), out.join("held-overlay")).unwrap();
    fs::create_dir(out.join("overlay")).unwrap();
    assert!(
        output
            .write_new("producer-receipt.json", b"{}")
            .unwrap_err()
            .contains("overlay directory namespace changed")
    );
    assert!(!out.join("producer-receipt.json").exists());
}

#[test]
fn same_rows_keep_legacy_raw_quality_vectors_scopes_metadata_and_guards() {
    let f = fixture();
    // This is only a tiny authored test fixture. Production never flattens/reopens F.
    let raw = f.root.join("raw");
    fs::create_dir(&raw).unwrap();
    let mut inputs = BTreeMap::new();
    for name in super::TABLES {
        let doc = &f.documents[&format!("tables/{name}.json")];
        let rows: Vec<_> = doc["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                let mut v = r["values"].clone();
                v["_index"] = r["_row"].clone();
                v
            })
            .collect();
        let bytes = serde_json::to_vec_pretty(&rows).unwrap();
        let file = format!("{name}.json");
        fs::write(raw.join(&file), &bytes).unwrap();
        inputs.insert(file, sha256_hex(&bytes));
    }
    let p = policy(&f);
    let source = raw.join("source.json");
    let mut old = json!({"schema":"gem-quality-source/v1","poe_version":"test","provenance":{"kind":"synthetic"},"inputs":inputs,"compatibility":p["compatibility"]});
    fs::write(&source, serde_json::to_vec_pretty(&old).unwrap()).unwrap();
    let raw_args = || crate::skills::quality::QualityArgs {
        raw: raw.clone(),
        source: source.clone(),
        out: f.root.join("legacy"),
        patch: "test".into(),
    };
    crate::skills::quality::run(raw_args()).unwrap();
    run(args(&f)).unwrap();
    let original = fs::read(f.root.join("legacy/test/overlay/gem_quality_stats.json")).unwrap();
    let legacy: Value = serde_json::from_slice(&original).unwrap();
    let new = read(&f, "overlay/gem_quality_stats.json");
    assert_eq!(legacy["effects"], new["effects"]);
    assert_eq!(
        legacy["_meta"]["unapplied_stat_set_scopes"],
        new["_meta"]["unapplied_stat_set_scopes"]
    );
    assert_eq!(
        legacy["_meta"],
        json!({
            "schema":"gem_quality_stats/v1", "generator":"pobr-data-adapter --gem-quality", "poe_version":"test",
            "source_receipt_sha256":sha256_hex(&fs::read(&source).unwrap()),"provenance":old["provenance"],"inputs":inputs,
            "compatibility":p["compatibility"],"unapplied_stat_set_scopes":new["_meta"]["unapplied_stat_set_scopes"],
            "regen_command":"cargo run -p pobr-data-adapter -- --gem-quality pipeline/tables/English --quality-source pipeline/gem-quality/test.json --out data --patch test"
        })
    );
    for (key, value) in [("schema", "wrong"), ("poe_version", "wrong")] {
        let mut bad = old.clone();
        bad[key] = json!(value);
        fs::write(&source, serde_json::to_vec(&bad).unwrap()).unwrap();
        assert!(
            crate::skills::quality::run(raw_args())
                .unwrap_err()
                .contains("schema / game version")
        );
    }
    old["inputs"]["Stats.json"] = json!("0".repeat(64));
    fs::write(&source, serde_json::to_vec(&old).unwrap()).unwrap();
    assert!(
        crate::skills::quality::run(raw_args())
            .unwrap_err()
            .contains("fingerprint differs")
    );
    assert_eq!(
        original,
        fs::read(f.root.join("legacy/test/overlay/gem_quality_stats.json")).unwrap()
    );
}

#[test]
fn quality_does_not_change_existing_domain_outputs_or_add_overlay_fields() {
    for (domain, fixture_bytes, count) in [
        (Domain::StatsMods, None, 2),
        (
            Domain::BaseItems,
            Some(
                include_bytes!("../../../tests/fixtures/ordinary-facts/base-item-tables.json")
                    .as_slice(),
            ),
            1,
        ),
        (
            Domain::Skills,
            Some(
                include_bytes!("../../../tests/fixtures/ordinary-facts/skill-tables.json")
                    .as_slice(),
            ),
            5,
        ),
    ] {
        let mut f = Fixture::new();
        if let Some(bytes) = fixture_bytes {
            let tables: BTreeMap<String, Value> = serde_json::from_slice(bytes).unwrap();
            f.documents.retain(|path, _| path == "source-lock.json");
            f.documents.extend(
                tables
                    .into_iter()
                    .map(|(name, doc)| (format!("tables/{name}.json"), doc)),
            );
            f.seal();
        }
        run(Args { domain, ..f.args() }).unwrap();
        let quality_fixture = fixture();
        run(args(&quality_fixture)).unwrap();
        run(Args {
            domain,
            out: f.root.join("repeat"),
            ..f.args()
        })
        .unwrap();
        let receipt = read(&f, "producer-receipt.json");
        let files = receipt["files"].as_object().unwrap();
        assert_eq!(files.len(), count + 3);
        assert_eq!(receipt.as_object().unwrap().len(), 10);
        assert!(!f.root.join("out/overlay").exists());
        assert!(!f.root.join("out/quality-policy.json").exists());
        for file in files
            .keys()
            .map(String::as_str)
            .chain(["producer-receipt.json"])
        {
            assert_eq!(
                fs::read(f.root.join("out").join(file)).unwrap(),
                fs::read(f.root.join("repeat").join(file)).unwrap(),
                "{file}"
            );
        }
    }
}

#[test]
fn stat_set_codec_does_not_change_quality_files_or_receipt() {
    let f = fixture();
    run(args(&f)).unwrap();
    let codec = super::super::stat_set_codec::tests::fixture();
    run(super::super::stat_set_codec::tests::args(&codec)).unwrap();
    run(Args {
        out: f.root.join("after-codec"),
        ..args(&f)
    })
    .unwrap();
    let receipt = read(&f, "producer-receipt.json");
    assert!(!f.root.join("out/codec-policy.json").exists());
    for path in receipt["files"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .chain(["producer-receipt.json"])
    {
        assert_eq!(
            fs::read(f.root.join("out").join(path)).unwrap(),
            fs::read(f.root.join("after-codec").join(path)).unwrap(),
            "{path}"
        );
    }
}

#[test]
fn gem_effects_does_not_change_quality_files_or_receipt() {
    let f = fixture();
    run(args(&f)).unwrap();
    let links = super::super::gem_effects::tests::fixture();
    run(super::super::gem_effects::tests::args(&links)).unwrap();
    run(Args {
        out: f.root.join("after-links"),
        ..args(&f)
    })
    .unwrap();
    let receipt = read(&f, "producer-receipt.json");
    assert_eq!(receipt["files"].as_object().unwrap().len(), 5);
    assert!(!f.root.join("out/legacy-gem-effects.json").exists());
    for path in receipt["files"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .chain(["producer-receipt.json"])
    {
        assert_eq!(
            fs::read(f.root.join("out").join(path)).unwrap(),
            fs::read(f.root.join("after-links").join(path)).unwrap(),
            "{path}"
        );
    }
}

#[test]
fn monster_resistances_does_not_change_quality_files_or_receipt() {
    let f = fixture();
    run(args(&f)).unwrap();
    let monsters = super::super::monster_resistances::tests::fixture();
    run(super::super::monster_resistances::tests::args(&monsters)).unwrap();
    run(Args {
        out: f.root.join("after-monsters"),
        ..args(&f)
    })
    .unwrap();
    let receipt = read(&f, "producer-receipt.json");
    assert_eq!(receipt["files"].as_object().unwrap().len(), 5);
    assert!(!f.root.join("out/base/monster_resistances.json").exists());
    for path in receipt["files"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .chain(["producer-receipt.json"])
    {
        assert_eq!(
            fs::read(f.root.join("out").join(path)).unwrap(),
            fs::read(f.root.join("after-monsters").join(path)).unwrap(),
            "{path}"
        );
    }
}

#[test]
fn totem_varieties_does_not_change_quality_files_or_receipt() {
    let f = fixture();
    run(args(&f)).unwrap();
    let totems = super::super::totem_varieties::tests::fixture();
    run(super::super::totem_varieties::tests::args(&totems)).unwrap();
    run(Args {
        out: f.root.join("after-totems"),
        ..args(&f)
    })
    .unwrap();
    let receipt = read(&f, "producer-receipt.json");
    assert_eq!(receipt["files"].as_object().unwrap().len(), 5);
    assert!(!f.root.join("out/overlay/totem_varieties.json").exists());
    for path in receipt["files"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .chain(["producer-receipt.json"])
    {
        assert_eq!(
            fs::read(f.root.join("out").join(path)).unwrap(),
            fs::read(f.root.join("after-totems").join(path)).unwrap(),
            "{path}"
        );
    }
}
