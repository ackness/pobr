use std::collections::BTreeMap;
use std::fs;

use pobr_data::catalog::StatSetLabelsDef;
use serde_json::{Value, json};

use super::super::tests::Fixture;
use super::super::{Args, Domain, parse_args, run};
use crate::raw_source::sha256_hex;

pub(in crate::ordinary_facts) fn table(
    name: &str,
    fields: &[(&str, &str, bool, Option<&str>)],
    values: Value,
) -> Value {
    let rows: Vec<_> = values
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(i, v)| json!({"_row":i,"values":v}))
        .collect();
    json!({"table":name,"raw_resource":format!("Data/Balance/{name}.datc64"),"row_key":"_row",
        "physical_fields":fields.iter().map(|c|c.0).collect::<Vec<_>>(),
        "columns":fields.iter().enumerate().map(|(i,(name,kind,array,target))|json!({
            "source_column":i,"physical_fields":[name],"schema":{"name":name,"type":kind,
                "array":array,"interval":false,"references":target.map(|t|json!({"table":t}))}
        })).collect::<Vec<_>>(),
        "validation":{"rows":rows.len(),"schema_row_bytes":8,"actual_row_bytes":8,"warnings":[]},"rows":rows})
}

pub(in crate::ordinary_facts) fn fixture() -> Fixture {
    let mut f = Fixture::new();
    let old: Vec<_> = f
        .documents
        .keys()
        .filter(|p| p.starts_with("tables/"))
        .cloned()
        .collect();
    f.documents.retain(|p, _| p == "source-lock.json");
    for (name, doc) in [
        (
            "GrantedEffects",
            table(
                "GrantedEffects",
                &[
                    ("Id", "string", false, None),
                    (
                        "StatSet",
                        "foreignrow",
                        false,
                        Some("GrantedEffectStatSets"),
                    ),
                    (
                        "AdditionalStatSets",
                        "foreignrow",
                        true,
                        Some("GrantedEffectStatSets"),
                    ),
                    ("_index", "i32", false, None),
                ],
                json!([
                    {"Id":"ZOwner","StatSet":1,"AdditionalStatSets":[0,0,0],"_index":999},
                    {"Id":"AOwner","StatSet":2,"AdditionalStatSets":[],"_index":998},
                    {"Id":"NoCodecOwner","StatSet":2,"AdditionalStatSets":[],"_index":997}
                ]),
            ),
        ),
        (
            "GrantedEffectStatSets",
            table(
                "GrantedEffectStatSets",
                &[
                    ("Id", "string", false, None),
                    ("Unused", "i32", false, None),
                ],
                json!([
                    {"Id":"Additional","Unused":-9},{"Id":"ZPrimary","Unused":0},{"Id":"APrimary","Unused":7}
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
    fs::create_dir(f.root.join("codec")).unwrap();
    fs::create_dir(f.root.join("policy")).unwrap();
    let mut p: Value = serde_json::from_slice(include_bytes!(
        "../../../../../pipeline/skill-codec/0.5.5.5-legacy-stat-set-policy.json"
    ))
    .unwrap();
    let ghost = json!({"skill":"AOwner","set_id":"GhostSet","set_index":1});
    p["raw_ownership_exceptions"] = json!([{"key":ghost,"reason":"Synthetic pre-merge unresolved legacy record; preserve unchanged"}]);
    p["evidence"]["bound_before_prepared_merges_in_both"] = json!(3);
    p["evidence"]["unbound_before_prepared_merges_in_both"] = json!([ghost]);
    let doc = json!({"_meta":p["source"]["metadata"],"labels":[
        {"skill":"ZOwner","set_id":"Additional","set_index":7,"label":""},
        {"skill":"ZOwner","set_id":"ZPrimary","set_index":3,"label":"Old primary"},
        {"skill":"AOwner","set_id":"GhostSet","set_index":1,"label":"Unbound legacy"},
        {"skill":"AOwner","set_id":"APrimary","set_index":4,"label":"Legacy label"}
    ]});
    write_codec(&f, &doc);
    write_policy(&f, &p);
    reseal(&mut f);
    f
}

fn codec(f: &Fixture) -> Value {
    crate::read_json(&f.root.join("codec/source.json")).unwrap()
}
fn policy(f: &Fixture) -> Value {
    crate::read_json(&f.root.join("policy/policy.json")).unwrap()
}
fn write_codec(f: &Fixture, doc: &Value) {
    // Noncanonical whitespace and source order must survive the producer byte-for-byte.
    let mut bytes = b" \n\t".to_vec();
    bytes.extend(serde_json::to_vec(doc).unwrap());
    bytes.extend(b"\n\n");
    fs::write(f.root.join("codec/source.json"), bytes).unwrap();
}
fn write_policy(f: &Fixture, p: &Value) {
    fs::write(
        f.root.join("policy/policy.json"),
        serde_json::to_vec_pretty(p).unwrap(),
    )
    .unwrap();
}
fn reseal(f: &mut Fixture) {
    f.seal();
    let mut p = policy(f);
    p["target"]["facts_manifest_sha256"] = f.args().expected_manifest_sha256.into();
    p["source"]["sha256"] = sha256_hex(&fs::read(f.root.join("codec/source.json")).unwrap()).into();
    write_policy(f, &p);
}

pub(in crate::ordinary_facts) fn args(f: &Fixture) -> Args {
    let source = f.root.join("codec/source.json");
    let policy = f.root.join("policy/policy.json");
    Args {
        domain: Domain::StatSetCodec,
        codec: Some(super::super::CodecArgs {
            source_sha256: sha256_hex(&fs::read(&source).unwrap()),
            source,
            policy_sha256: sha256_hex(&fs::read(&policy).unwrap()),
            policy,
        }),
        ..f.args()
    }
}
fn rejects(f: &Fixture, expected: &str) {
    let error = run(args(f)).unwrap_err();
    assert!(
        error.contains(expected),
        "expected {expected:?}, got {error:?}"
    );
    assert!(!f.root.join("out").exists());
}
fn read(f: &Fixture, file: &str) -> Value {
    crate::read_json(&f.root.join("out").join(file)).unwrap()
}

#[test]
fn codec_is_verbatim_with_empty_labels_export_indices_and_ordered_raw_references() {
    let f = fixture();
    let original = fs::read(f.root.join("codec/source.json")).unwrap();
    run(args(&f)).unwrap();
    assert_eq!(
        original,
        fs::read(f.root.join("out/overlay/stat_set_labels.json")).unwrap()
    );
    let dto: StatSetLabelsDef = serde_json::from_slice(&original).unwrap();
    assert_eq!(
        dto.labels.iter().map(|r| r.set_index).collect::<Vec<_>>(),
        [7, 3, 1, 4]
    );
    assert_eq!(dto.labels[0].label, "");
    assert_eq!(dto.labels[2].set_id, "GhostSet");
    let receipt = read(&f, "producer-receipt.json");
    let c = &receipt["conversion"];
    assert_eq!(
        (
            c["codec_records"].as_u64(),
            c["codec_owners"].as_u64(),
            c["raw_owned_records"].as_u64(),
            c["exception_records"].as_u64()
        ),
        (Some(4), Some(2), Some(3), Some(1))
    );
    assert_eq!(c["empty_labels"], 1);
    assert_eq!(c["legacy_source"]["metadata"], codec(&f)["_meta"]);
    assert_eq!(c["legacy_source"]["snapshot_version"], "0.5.5.4.2");
    assert_eq!(receipt["target"]["version"], "0.5.5.5");
    let members = &c["target_membership"];
    assert_eq!(members["owners"]["ZOwner"]["effect_source_row"], 0);
    assert_eq!(
        members["owners"]["ZOwner"]["primary"],
        json!({"source_row":1,"set_id":"ZPrimary"})
    );
    assert_eq!(
        members["owners"]["ZOwner"]["additional"],
        json!([
            {"source_row":0,"set_id":"Additional"},{"source_row":0,"set_id":"Additional"},{"source_row":0,"set_id":"Additional"}
        ])
    );
    assert_eq!(members["exceptions"][0]["codec_source_row"], 2);
    assert_eq!(
        members["exceptions"][0]["current_owner"]["effect_source_row"],
        1
    );
    assert_eq!(
        members["exceptions"][0]["current_owner"]["primary"]["set_id"],
        "APrimary"
    );
    assert_eq!(members["records"][0]["key"]["set_index"], 7); // Not raw slot/order.
    assert_eq!(
        c["historical_binding_evidence"]["unbound_before_prepared_merges_in_both"][0]["set_id"],
        "GhostSet"
    );
    assert_eq!(
        c["unmapped_source_columns"][0]["columns"][0]["physical_fields"],
        json!(["_index"])
    );
    assert_eq!(
        c["unmapped_source_columns"][1]["source_row_range"]["end_exclusive"],
        3
    );
    assert_eq!(receipt["complete_runtime"], false);
    assert_eq!(receipt["adoption"], "not-adopted");
    assert_eq!(
        read(&f, "_domain_candidate.json")["domain"],
        "ordinary-stat-set-codec"
    );
    assert!(!f.root.join("out/base").exists());
    assert!(!f.root.join("out/manifest.json").exists());
    assert_eq!(
        fs::read(f.root.join("out/codec-policy.json")).unwrap(),
        fs::read(f.root.join("policy/policy.json")).unwrap()
    );
    assert_eq!(
        receipt["input"]["selected_payloads"]
            .as_object()
            .unwrap()
            .len(),
        3
    );
    let pins = receipt["files"].as_object().unwrap();
    assert_eq!(pins.len(), 5);
    for (path, pin) in pins {
        let b = fs::read(f.root.join("out").join(path)).unwrap();
        assert_eq!(pin["sha256"], sha256_hex(&b));
        assert_eq!(pin["bytes"], b.len());
    }
    run(Args {
        out: f.root.join("again"),
        ..args(&f)
    })
    .unwrap();
    for path in pins
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

#[test]
fn codec_rejects_duplicate_identity_keys_and_invalid_fields_without_dropping_records() {
    for (pointer, value, error) in [
        ("/labels/1/set_index", json!(7), "duplicate (skill,index)"),
        (
            "/labels/1/set_id",
            json!("Additional"),
            "duplicate (skill,index)",
        ),
        ("/labels/0/skill", json!(""), "nonempty skill/set"),
        ("/labels/0/set_id", json!(""), "nonempty skill/set"),
        ("/labels/0/set_index", json!(0), "positive u32"),
        ("/labels/0/set_index", json!(-1), "invalid value"),
        (
            "/labels/0/set_index",
            json!(4294967296_u64),
            "invalid value",
        ),
        ("/labels/0/set_index", json!(1.5), "invalid type"),
        ("/labels/0/label", json!(null), "invalid type"),
        ("/labels/0/label", json!(7), "invalid type"),
        (
            "/labels/0/skill",
            json!("MissingOwner"),
            "unknown target owner",
        ),
        (
            "/labels/0/set_id",
            json!("UnknownSet"),
            "unlisted raw ownership exception",
        ),
    ] {
        let mut f = fixture();
        let mut doc = codec(&f);
        *doc.pointer_mut(pointer).unwrap() = value;
        write_codec(&f, &doc);
        reseal(&mut f);
        rejects(&f, error);
    }
    let mut f = fixture();
    let mut doc = codec(&f);
    doc["labels"][0].as_object_mut().unwrap().remove("label");
    write_codec(&f, &doc);
    reseal(&mut f);
    rejects(&f, "missing field");
}

#[test]
fn codec_exceptions_are_exact_complete_and_rejected_when_duplicate_stale_or_unused() {
    for case in [
        "unlisted",
        "duplicate",
        "stale",
        "wrong-index",
        "missing-row",
        "empty-reason",
    ] {
        let mut f = fixture();
        let mut p = policy(&f);
        match case {
            "unlisted" => p["raw_ownership_exceptions"] = json!([]),
            "duplicate" => {
                let e = p["raw_ownership_exceptions"][0].clone();
                p["raw_ownership_exceptions"]
                    .as_array_mut()
                    .unwrap()
                    .push(e);
            }
            "stale" => {
                f.values("GrantedEffectStatSets", 2)["Id"] = json!("GhostSet");
                let mut d = codec(&f);
                d["labels"][3]["set_id"] = json!("GhostSet");
                d["labels"].as_array_mut().unwrap().remove(3);
                write_codec(&f, &d);
            }
            "wrong-index" => p["raw_ownership_exceptions"][0]["key"]["set_index"] = json!(2),
            "missing-row" => {
                let mut d = codec(&f);
                d["labels"].as_array_mut().unwrap().remove(2);
                write_codec(&f, &d);
            }
            "empty-reason" => p["raw_ownership_exceptions"][0]["reason"] = json!(" "),
            _ => unreachable!(),
        }
        write_policy(&f, &p);
        reseal(&mut f);
        rejects(
            &f,
            match case {
                "duplicate" | "empty-reason" => "duplicate codec exception or empty reason",
                "stale" | "missing-row" => "unused/stale",
                _ => "unlisted raw ownership exception",
            },
        );
    }
}

#[test]
fn codec_validates_all_consumed_physical_fields_and_foreign_keys_before_output() {
    for case in [
        "primary-null",
        "primary-negative",
        "additional-null",
        "bad-additional",
        "unreferenced-owner",
        "duplicate-id",
        "physical-key",
        "column-type",
        "column-target",
        "missing-column",
    ] {
        let mut f = fixture();
        match case {
            "primary-null" => f.values("GrantedEffects", 0)["StatSet"] = json!(null),
            "primary-negative" => f.values("GrantedEffects", 0)["StatSet"] = json!(-1),
            "additional-null" => {
                f.values("GrantedEffects", 0)["AdditionalStatSets"] = json!([null])
            }
            "bad-additional" => f.values("GrantedEffects", 0)["AdditionalStatSets"] = json!([999]),
            "unreferenced-owner" => f.values("GrantedEffects", 2)["StatSet"] = json!(999),
            "duplicate-id" => f.values("GrantedEffectStatSets", 1)["Id"] = json!("Additional"),
            "physical-key" => {
                f.documents.get_mut("tables/GrantedEffects.json").unwrap()["rows"][1]["_row"] =
                    json!(0)
            }
            "column-type" => {
                f.documents.get_mut("tables/GrantedEffects.json").unwrap()["columns"][1]["schema"]
                    ["type"] = json!("i32")
            }
            "column-target" => {
                f.documents.get_mut("tables/GrantedEffects.json").unwrap()["columns"][2]["schema"]
                    ["references"]["table"] = json!("Stats")
            }
            "missing-column" => {
                f.values("GrantedEffects", 0)
                    .as_object_mut()
                    .unwrap()
                    .remove("AdditionalStatSets");
            }
            _ => unreachable!(),
        }
        reseal(&mut f);
        assert!(run(args(&f)).is_err(), "{case}");
        assert!(!f.root.join("out").exists(), "{case}");
    }
}

#[test]
fn codec_policy_checks_target_legacy_identity_and_declared_evidence_without_opening_it() {
    for (pointer, value, error) in [
        ("/schema", json!("unknown"), "policy schema"),
        (
            "/target/game_patch",
            json!("old"),
            "target patch / facts manifest",
        ),
        (
            "/target/facts_manifest_sha256",
            json!("0".repeat(64)),
            "target patch / facts manifest",
        ),
        (
            "/source/sha256",
            json!("0".repeat(64)),
            "pin differs from policy",
        ),
        (
            "/source/snapshot_version",
            json!("0.5.5.5"),
            "legacy version/path",
        ),
        (
            "/source/metadata/vendor_commit",
            json!("0".repeat(40)),
            "metadata differs",
        ),
        (
            "/source/metadata/schema",
            json!("unknown"),
            "metadata schema",
        ),
        (
            "/source/metadata/vendor",
            json!("different"),
            "metadata schema",
        ),
        ("/evidence/source_check/sha256", json!("bad"), "hexadecimal"),
        (
            "/evidence/bound_before_prepared_merges_in_both",
            json!(0),
            "binding evidence",
        ),
        (
            "/evidence/unbound_before_prepared_merges_in_both/0/set_id",
            json!("not-in-codec"),
            "binding evidence",
        ),
    ] {
        let f = fixture();
        let mut p = policy(&f);
        *p.pointer_mut(pointer).unwrap() = value;
        write_policy(&f, &p);
        rejects(&f, error);
    }
    let f = fixture();
    let mut p = policy(&f);
    p["extra"] = json!(true);
    write_policy(&f, &p);
    rejects(&f, "unknown field");
    let mut f = fixture();
    let mut d = codec(&f);
    d["_meta"]["schema"] = json!("unknown");
    write_codec(&f, &d);
    reseal(&mut f);
    rejects(&f, "metadata schema");
    // Evidence files are not input dependencies; none of these paths exists.
    let f = fixture();
    let mut p = policy(&f);
    for field in [
        "source_check",
        "legacy_runtime_manifest",
        "legacy_base",
        "target_base",
    ] {
        p["evidence"][field]["path"] = json!("/not-present/no-implicit-read");
    }
    write_policy(&f, &p);
    run(args(&f)).unwrap();
}

#[test]
fn codec_pin_and_duplicate_json_failures_happen_before_reserving_an_output() {
    for kind in ["source", "policy"] {
        let f = fixture();
        let a = args(&f);
        let path = if kind == "source" {
            "codec/source.json"
        } else {
            "policy/policy.json"
        };
        fs::write(f.root.join(path), b"invalid JSON").unwrap();
        assert!(run(a).unwrap_err().contains("checked before parsing"));
        assert!(!f.root.join("out").exists());
        let mut f = fixture();
        fs::write(f.root.join(path), br#"{"duplicate":{"x":1,"x":2}}"#).unwrap();
        if kind == "source" {
            reseal(&mut f);
        }
        rejects(&f, "duplicate JSON key");
    }
    for kind in ["manifest", "payload", "version", "bad-hex"] {
        let f = fixture();
        let mut a = args(&f);
        match kind {
            "manifest" => a.expected_manifest_sha256 = "0".repeat(64),
            "payload" => fs::write(f.root.join("input/tables/GrantedEffects.json"), b"{}").unwrap(),
            "version" => a.version = "wrong".into(),
            "bad-hex" => a.codec.as_mut().unwrap().policy_sha256 = "BAD".into(),
            _ => unreachable!(),
        }
        assert!(run(a).is_err(), "{kind}");
        assert!(!f.root.join("out").exists());
    }
}

#[test]
fn codec_requires_all_explicit_flags_and_rejects_cross_domain_or_repeated_options() {
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
    let flags = [
        "--codec-source",
        "source",
        "--codec-source-sha256",
        "pin",
        "--codec-policy",
        "policy",
        "--codec-policy-sha256",
        "pin",
    ];
    let domain = ["--ordinary-domain", "stat-set-codec"];
    assert_eq!(
        parse_args(
            base.into_iter()
                .chain(domain)
                .chain(flags)
                .map(String::from)
        )
        .unwrap()
        .domain,
        Domain::StatSetCodec
    );
    assert!(matches!(
        crate::parse_args_from(
            base.into_iter()
                .chain(domain)
                .chain(flags)
                .map(String::from)
        ),
        Ok(crate::Mode::OrdinaryFacts(_))
    ));
    for i in (0..flags.len()).step_by(2) {
        let incomplete = flags
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i && *j != i + 1)
            .map(|(_, s)| *s);
        assert!(
            parse_args(
                base.into_iter()
                    .chain(domain)
                    .chain(incomplete)
                    .map(String::from)
            )
            .is_err()
        );
        assert!(
            parse_args(
                base.into_iter()
                    .chain(domain)
                    .chain(flags)
                    .chain([flags[i], "again"])
                    .map(String::from)
            )
            .is_err()
        );
        for other in ["stats-mods", "base-items", "skills", "gem-quality"] {
            assert!(
                parse_args(
                    base.into_iter()
                        .chain(["--ordinary-domain", other, flags[i], "value"])
                        .map(String::from)
                )
                .is_err()
            );
        }
    }
    for extra in [
        ["--quality-policy", "q"],
        ["--quality-policy-sha256", "pin"],
        ["--ordinary-domain", "stat-set-codec"],
        ["--raw", "old"],
    ] {
        assert!(
            parse_args(
                base.into_iter()
                    .chain(domain)
                    .chain(flags)
                    .chain(extra)
                    .map(String::from)
            )
            .is_err()
        );
    }
    assert!(
        parse_args(
            base.into_iter()
                .chain(domain)
                .chain(flags)
                .chain(["--codec-source"])
                .map(String::from)
        )
        .is_err()
    );
}

#[test]
fn codec_protects_both_input_parents_production_and_fresh_output() {
    let f = fixture();
    for out in [
        f.root.join("codec/new"),
        f.root.join("policy/new"),
        f.root.join("input/new"),
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data")
            .canonicalize()
            .unwrap()
            .join(format!(
                "codec-forbidden-{}",
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
fn codec_leaves_existing_base_domains_bytes_and_receipt_shapes_unchanged() {
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
            f.documents.retain(|p, _| p == "source-lock.json");
            f.documents.extend(
                tables
                    .into_iter()
                    .map(|(t, d)| (format!("tables/{t}.json"), d)),
            );
            f.seal();
        }
        run(Args { domain, ..f.args() }).unwrap();
        let c = fixture();
        run(args(&c)).unwrap();
        run(Args {
            domain,
            out: f.root.join("again"),
            ..f.args()
        })
        .unwrap();
        let receipt = read(&f, "producer-receipt.json");
        let files = receipt["files"].as_object().unwrap();
        assert_eq!(files.len(), count + 3);
        assert_eq!(receipt.as_object().unwrap().len(), 10);
        assert!(!f.root.join("out/codec-policy.json").exists());
        for path in files
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
