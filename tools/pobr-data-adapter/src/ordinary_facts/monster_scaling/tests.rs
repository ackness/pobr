use std::collections::BTreeMap;
use std::fs;

use pobr_data::catalog::monster_scaling::{MONSTER_SCALING_TABLE_LEN, MonsterScalingTable};
use serde_json::{Value, json};

use super::super::stat_set_codec::tests::table;
use super::super::tests::Fixture;
use super::super::{Args, Domain, parse_args, reader, run};
use super::{FLOATS, INTEGERS, PAYLOAD, TABLE};
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
    let fields: Vec<_> = std::iter::once(("DisplayLevel", "string", false, None))
        .chain(
            INTEGERS
                .iter()
                .map(|(field, _)| (*field, "i32", false, None)),
        )
        .chain(FLOATS.iter().map(|(field, _)| (*field, "f32", false, None)))
        .chain([
            ("Experience", "i32", false, None),
            ("Difficulty", "i32", false, None),
        ])
        .collect();
    let values: Vec<_> = (1..=MONSTER_SCALING_TABLE_LEN)
        .rev()
        .map(|level| {
            json!({"DisplayLevel":level.to_string(),"Accuracy":level,"Evasion":level+100,
            "Armour":level+200,"MonsterLife":level+300,"AllyLife":level+400,
            "AilmentThreshold":level+500,"PoiseThreshold":level+600,
            "Damage":level as f64+0.125,"MinionDamage":-(level as f64)-0.125,
            "Experience":-42,"Difficulty":0})
        })
        .collect();
    f.documents
        .insert(PAYLOAD.into(), table(TABLE, &fields, json!(values)));
    f.seal();
    for path in old {
        fs::remove_file(f.root.join("input").join(path)).unwrap();
    }
    f
}

pub(in crate::ordinary_facts) fn args(f: &Fixture) -> Args {
    Args {
        domain: Domain::MonsterScaling,
        ..f.args()
    }
}
fn read(f: &Fixture, path: &str) -> Value {
    crate::read_json(&f.root.join("out").join(path)).unwrap()
}
fn rejects(f: &Fixture, expected: &str) {
    let error = run(args(f)).unwrap_err();
    assert!(
        error.contains(expected),
        "expected {expected:?}, got {error:?}"
    );
    assert!(!f.root.join("out").exists());
}
fn replace_payload(f: &mut Fixture, bytes: &[u8]) {
    fs::write(f.root.join("input").join(PAYLOAD), bytes).unwrap();
    f.manifest["files"][PAYLOAD] = json!({"bytes":bytes.len(),"sha256":sha256_hex(bytes)});
    fs::write(
        f.root.join("input/manifest.json"),
        reader::json_bytes(&f.manifest).unwrap(),
    )
    .unwrap();
}

#[test]
fn original_nine_arrays_follow_levels_not_physical_rows_and_bind_all_values() {
    let f = fixture();
    run(args(&f)).unwrap();
    let doc = read(&f, "base/monster_scaling.json");
    let dto: MonsterScalingTable = serde_json::from_value(doc.clone()).unwrap();
    let c = &read(&f, "producer-receipt.json")["conversion"];
    for level in 1..=MONSTER_SCALING_TABLE_LEN {
        let i = level - 1;
        assert_eq!(dto.accuracy[i], level as u32);
        assert_eq!(dto.evasion[i], (level + 100) as u32);
        assert_eq!(dto.armour[i], (level + 200) as u32);
        assert_eq!(dto.life[i], (level + 300) as u32);
        assert_eq!(dto.ally_life[i], (level + 400) as u32);
        assert_eq!(dto.ailment_threshold[i], (level + 500) as u32);
        assert_eq!(dto.poise_threshold[i], (level + 600) as u32);
        assert_eq!(dto.damage[i], level as f64 + 0.13);
        assert_eq!(dto.ally_damage[i], -(level as f64) - 0.13);
        let r = &c["source_records"][i];
        assert_eq!(r["level"], level);
        assert_eq!(r["source_row"], MONSTER_SCALING_TABLE_LEN - level);
        assert_eq!(r["output_index"], i);
        assert_eq!(r["display_level"], level.to_string());
        assert_eq!(r["raw"].as_object().unwrap().len(), 9);
        assert_eq!(r["output"].as_object().unwrap().len(), 9);
        let source = &f.documents[PAYLOAD]["rows"][MONSTER_SCALING_TABLE_LEN - level]["values"];
        for (raw, out) in INTEGERS.iter().chain(FLOATS.iter()) {
            assert_eq!(r["raw"][*raw], source[*raw]);
            assert_eq!(r["output"][*out], doc[*out][i]);
        }
        for (j, (raw, out)) in FLOATS.iter().enumerate() {
            assert_eq!(
                r["normalizations"][j],
                json!({"source_field":raw,"output_field":out,"raw":source[*raw],"normalized":doc[*out][i]})
            );
        }
    }
    for (_, field) in INTEGERS.iter().chain(FLOATS.iter()) {
        assert_eq!(
            doc[*field].as_array().unwrap().len(),
            MONSTER_SCALING_TABLE_LEN
        );
    }
    assert_eq!(doc.as_object().unwrap().len(), 10); // nine required arrays + target meta
    let columns = c["unconsumed_source"][0]["columns"].as_array().unwrap();
    assert_eq!(
        columns
            .iter()
            .map(|c| c["physical_fields"].clone())
            .collect::<Vec<_>>(),
        vec![json!(["Experience"]), json!(["Difficulty"])]
    );
    assert_eq!(
        c["unconsumed_source"][0]["source_row_range"],
        json!({"start":0,"end_exclusive":MONSTER_SCALING_TABLE_LEN})
    );
    // Original DTO clamp/lookup methods operate on these supplied arrays unchanged.
    assert_eq!(dto.accuracy_at(0), 1);
    assert_eq!(dto.life_at(101), 400);
}

#[test]
fn decimal_normalization_preserves_signed_noisy_short_and_halfway_facts() {
    let mut f = fixture();
    let cases = [
        (9.15999984741211, 9.16),
        (-9.15999984741211, -9.16),
        (0.125, 0.13),
        (-0.125, -0.13),
        (0.1, 0.1),
        (0.0, 0.0),
        (-0.0, -0.0),
        (1.234, 1.23),
        (-1.234, -1.23),
    ];
    for (row, (raw, _)) in cases.iter().enumerate() {
        for (field, _) in FLOATS {
            f.values(TABLE, row)[field] = json!(raw);
        }
    }
    f.seal();
    run(args(&f)).unwrap();
    let doc = read(&f, "base/monster_scaling.json");
    for (row, (_, expected)) in cases.iter().enumerate() {
        for (_, field) in FLOATS {
            assert_eq!(
                doc[field][MONSTER_SCALING_TABLE_LEN - 1 - row],
                json!(expected)
            );
        }
    }
    assert!(
        doc["damage"][MONSTER_SCALING_TABLE_LEN - 7]
            .as_f64()
            .unwrap()
            .is_sign_negative()
    );
    // The physical f32 domain is a range check, not a f64 == widened-f32 test.
    assert_ne!(f64::from(0.1_f32), 0.1_f64);
    for raw in [f64::from(f32::MAX), -f64::from(f32::MAX)] {
        let normalized = super::damage(&json!(raw)).unwrap();
        assert!(normalized.is_finite());
        assert_eq!(normalized.is_sign_negative(), raw.is_sign_negative());
    }
}

#[test]
fn levels_are_complete_unique_ascii_decimal_identities_with_no_default_filling() {
    for bad in [
        json!(null),
        json!(1),
        json!(""),
        json!(" 1"),
        json!("1 "),
        json!("+1"),
        json!("-1"),
        json!("1.0"),
        json!("1e0"),
        json!("１"),
        json!("0"),
        json!("101"),
        json!("184467440737095516160"),
    ] {
        let mut f = fixture();
        f.values(TABLE, 0)["DisplayLevel"] = bad;
        f.seal();
        rejects(&f, "DisplayLevel");
    }
    let mut f = fixture();
    f.values(TABLE, 0)["DisplayLevel"] = json!("01");
    f.seal();
    rejects(&f, "duplicate DisplayLevel");
    let mut f = fixture();
    let doc = f.documents.get_mut(PAYLOAD).unwrap();
    doc["rows"].as_array_mut().unwrap().pop();
    doc["validation"]["rows"] = json!(MONSTER_SCALING_TABLE_LEN - 1);
    f.seal();
    rejects(&f, "incomplete DisplayLevel");
    let mut f = fixture();
    let doc = f.documents.get_mut(PAYLOAD).unwrap();
    let mut extra = doc["rows"][0].clone();
    extra["_row"] = json!(MONSTER_SCALING_TABLE_LEN);
    extra["values"]["DisplayLevel"] = json!("101");
    doc["rows"].as_array_mut().unwrap().push(extra);
    doc["validation"]["rows"] = json!(MONSTER_SCALING_TABLE_LEN + 1);
    f.seal();
    rejects(&f, "outside 1..=");
    // Leading zeros remain ASCII decimals, but uniqueness is by parsed level.
    let mut f = fixture();
    f.values(TABLE, MONSTER_SCALING_TABLE_LEN - 1)["DisplayLevel"] = json!("0001");
    f.seal();
    run(args(&f)).unwrap();
    assert_eq!(read(&f, "base/monster_scaling.json")["accuracy"][0], 1);
}

#[test]
fn integer_fields_are_exact_i32_checked_to_u32_and_never_optional() {
    for (field, output) in INTEGERS {
        for (bad, reason) in [
            (json!(-1), "nonnegative"),
            (json!(2147483648_i64), "exact signed i32"),
            (json!(-2147483649_i64), "exact signed i32"),
            (json!(1.0), "exact signed i32"),
            (json!("1"), "exact signed i32"),
            (json!(null), "exact signed i32"),
            (json!(true), "exact signed i32"),
        ] {
            let mut f = fixture();
            f.values(TABLE, 0)[field] = bad;
            f.seal();
            rejects(&f, reason);
        }
        let mut f = fixture();
        f.values(TABLE, 0)[field] = json!(i32::MAX);
        f.values(TABLE, 1)[field] = json!(0);
        f.seal();
        run(args(&f)).unwrap();
        let d = read(&f, "base/monster_scaling.json");
        assert_eq!(d[output][99], i32::MAX);
        assert_eq!(d[output][98], 0);
    }
    for (field, _) in INTEGERS.iter().chain(FLOATS.iter()) {
        let mut f = fixture();
        f.values(TABLE, 0).as_object_mut().unwrap().remove(*field);
        f.seal();
        assert!(run(args(&f)).is_err());
        assert!(!f.root.join("out").exists());
    }
}

#[test]
fn bad_float_values_and_nonfinite_json_fail_before_output() {
    for (field, _) in FLOATS {
        for bad in [
            json!(null),
            json!("0.125"),
            json!(true),
            json!([]),
            json!(3.5e38),
            json!(-3.5e38),
        ] {
            let mut f = fixture();
            f.values(TABLE, 0)[field] = bad;
            f.seal();
            rejects(&f, "finite f32-range number");
        }
        for literal in ["NaN", "Infinity", "-Infinity", "1e400"] {
            let mut f = fixture();
            f.values(TABLE, 0)[field] = json!("FLOAT_LITERAL");
            let text = serde_json::to_string(&f.documents[PAYLOAD])
                .unwrap()
                .replace("\"FLOAT_LITERAL\"", literal);
            replace_payload(&mut f, text.as_bytes());
            assert!(run(args(&f)).is_err());
            assert!(!f.root.join("out").exists());
        }
    }
}

#[test]
fn metadata_physical_keys_and_strict_json_are_checked() {
    for case in [
        "physical-key",
        "missing-column",
        "type",
        "integer-type",
        "array",
        "interval",
        "reference",
        "name",
        "missing-level",
    ] {
        let mut f = fixture();
        let doc = f.documents.get_mut(PAYLOAD).unwrap();
        match case {
            "physical-key" => doc["rows"][1]["_row"] = json!(0),
            "missing-column" => {
                doc["columns"].as_array_mut().unwrap().remove(1);
            }
            "type" => doc["columns"][8]["schema"]["type"] = json!("f64"),
            "integer-type" => doc["columns"][1]["schema"]["type"] = json!("u32"),
            "array" => doc["columns"][1]["schema"]["array"] = json!(true),
            "interval" => doc["columns"][1]["schema"]["interval"] = json!(true),
            "reference" => doc["columns"][1]["schema"]["references"] = json!({"table":"Stats"}),
            "name" => doc["columns"][0]["schema"]["name"] = json!("NotDisplayLevel"),
            _ => {
                doc["rows"][0]["values"]
                    .as_object_mut()
                    .unwrap()
                    .remove("DisplayLevel");
            }
        }
        f.seal();
        assert!(run(args(&f)).is_err(), "{case}");
        assert!(!f.root.join("out").exists());
    }
    let mut f = fixture();
    replace_payload(
        &mut f,
        br#"{"table":"DefaultMonsterStats","table":"duplicate"}"#,
    );
    rejects(&f, "duplicate");
}

#[test]
fn replay_binds_one_payload_and_target_meta_without_overlay_or_complete_manifest() {
    let f = fixture();
    run(args(&f)).unwrap();
    run(Args {
        out: f.root.join("again"),
        ..args(&f)
    })
    .unwrap();
    let r = read(&f, "producer-receipt.json");
    let meta = &read(&f, "base/monster_scaling.json")["_meta"];
    assert_eq!(r["complete_runtime"], false);
    assert_eq!(r["adoption"], "not-adopted");
    assert_eq!(r["files"].as_object().unwrap().len(), 4);
    assert_eq!(
        r["input"]["selected_payloads"].as_object().unwrap().len(),
        2
    );
    assert_eq!(meta["poe_version"], "0.5.5.5");
    assert_eq!(
        meta["facts_manifest_sha256"],
        args(&f).expected_manifest_sha256
    );
    assert_eq!(meta["inputs"].as_object().unwrap().len(), 1);
    assert_eq!(
        meta["inputs"][PAYLOAD],
        sha256_hex(&fs::read(f.root.join("input").join(PAYLOAD)).unwrap())
    );
    assert_eq!(
        read(&f, "_domain_candidate.json")["domain"],
        "ordinary-monster-scaling"
    );
    for absent in [
        "overlay",
        "manifest.json",
        "quality-policy.json",
        "codec-policy.json",
    ] {
        assert!(!f.root.join("out").join(absent).exists());
    }
    for (path, pin) in r["files"].as_object().unwrap() {
        let b = fs::read(f.root.join("out").join(path)).unwrap();
        assert_eq!(pin["bytes"], b.len());
        assert_eq!(pin["sha256"], sha256_hex(&b));
        assert_eq!(b, fs::read(f.root.join("again").join(path)).unwrap());
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
fn cli_pins_patch_and_existing_output_protection_remain_strict() {
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
        "monster-scaling",
    ];
    assert!(matches!(
        crate::parse_args_from(base.into_iter().map(String::from)),
        Ok(crate::Mode::OrdinaryFacts(_))
    ));
    for extra in [
        ["--quality-policy", "p"],
        ["--quality-policy-sha256", "p"],
        ["--codec-source", "p"],
        ["--codec-source-sha256", "p"],
        ["--codec-policy", "p"],
        ["--codec-policy-sha256", "p"],
        ["--ordinary-domain", "monster-scaling"],
        ["--raw", "r"],
        ["--native-facts", "r"],
    ] {
        assert!(parse_args(base.into_iter().chain(extra).map(String::from)).is_err());
    }
    assert!(parse_args(base.into_iter().chain(["--out"]).map(String::from)).is_err());
    assert!(
        parse_args(
            base[..8]
                .iter()
                .copied()
                .chain(["--ordinary-domain", "unknown"])
                .map(String::from)
        )
        .is_err()
    );
    for kind in ["manifest", "payload", "patch"] {
        let f = fixture();
        let mut a = args(&f);
        match kind {
            "manifest" => a.expected_manifest_sha256 = "0".repeat(64),
            "patch" => a.version = "old".into(),
            _ => fs::write(f.root.join("input").join(PAYLOAD), b"{}").unwrap(),
        };
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
                "scaling-forbidden-{}",
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
fn previous_non_quality_domains_keep_files_and_receipt_shapes() {
    for (domain, fixture_bytes, count) in [
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
    ] {
        let mut f = match domain {
            Domain::StatSetCodec => super::super::stat_set_codec::tests::fixture(),
            Domain::GemEffects => super::super::gem_effects::tests::fixture(),
            Domain::MonsterResistances => super::super::monster_resistances::tests::fixture(),
            Domain::TotemVarieties => super::super::totem_varieties::tests::fixture(),
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
        let prior_args = || match domain {
            Domain::StatSetCodec => super::super::stat_set_codec::tests::args(&f),
            Domain::GemEffects => super::super::gem_effects::tests::args(&f),
            Domain::MonsterResistances => super::super::monster_resistances::tests::args(&f),
            Domain::TotemVarieties => super::super::totem_varieties::tests::args(&f),
            _ => Args { domain, ..f.args() },
        };
        run(prior_args()).unwrap();
        let new = fixture();
        run(args(&new)).unwrap();
        run(Args {
            out: f.root.join("again"),
            ..prior_args()
        })
        .unwrap();
        let r = read(&f, "producer-receipt.json");
        assert_eq!(r.as_object().unwrap().len(), 10);
        assert_eq!(r["files"].as_object().unwrap().len(), count);
        assert!(!f.root.join("out/base/monster_scaling.json").exists());
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
