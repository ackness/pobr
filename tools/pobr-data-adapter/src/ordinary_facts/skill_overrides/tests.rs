use std::collections::BTreeMap;
use std::fs;

use pobr_data::catalog::skill_overrides::SkillOverridesDef;
use serde_json::{Value, json};

use super::super::stat_set_codec::tests::table;
use super::super::tests::Fixture;
use super::super::{Args, BranchCodecArgs, CodecArgs, Domain, parse_args, reader, run};
use crate::raw_source::sha256_hex;

const SOURCE: &str = "legacy/source.json";
const CODEC: &str = "branch/source.json";
const POLICY: &str = "policy/policy.json";
const OVERLAY: &str = "overlay/skill_overrides.json";

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
            "Stats",
            table(
                "Stats",
                &[("Id", "string", false, None)],
                json!([{"Id":"implicit_a"},{"Id":"implicit_b"}]),
            ),
        ),
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
                ],
                json!([
                    {"Id":"ZOwner","StatSet":1,"AdditionalStatSets":[0,0,0]},
                    {"Id":"Alias","StatSet":1,"AdditionalStatSets":[]},
                    {"Id":"Constant","StatSet":2,"AdditionalStatSets":[]},
                    {"Id":"UnboundPresent","StatSet":2,"AdditionalStatSets":[]}
                ]),
            ),
        ),
        (
            "GrantedEffectStatSets",
            table(
                "GrantedEffectStatSets",
                &[("Id", "string", false, None)],
                json!([
                    {"Id":"Extra"},{"Id":"Main"},{"Id":"ConstantSet"}
                ]),
            ),
        ),
        (
            "GrantedEffectStatSetsPerLevel",
            table(
                "GrantedEffectStatSetsPerLevel",
                &[
                    (
                        "StatSet",
                        "foreignrow",
                        false,
                        Some("GrantedEffectStatSets"),
                    ),
                    ("GemLevel", "i32", false, None),
                    ("BaseMultiplier", "i32", false, None),
                    ("ActorLevel", "i32", false, None),
                ],
                json!([
                    {"StatSet":1,"GemLevel":3,"BaseMultiplier":1000,"ActorLevel":99},
                    {"StatSet":1,"GemLevel":1,"BaseMultiplier":-10000,"ActorLevel":98},
                    {"StatSet":2,"GemLevel":2,"BaseMultiplier":-2000,"ActorLevel":97},
                    {"StatSet":2,"GemLevel":1,"BaseMultiplier":-2000,"ActorLevel":96},
                    {"StatSet":1,"GemLevel":0,"BaseMultiplier":7000,"ActorLevel":95},
                    {"StatSet":0,"GemLevel":2,"BaseMultiplier":-8000,"ActorLevel":94}
                ]),
            ),
        ),
        (
            "GrantedEffectsPerLevel",
            table(
                "GrantedEffectsPerLevel",
                &[
                    ("GrantedEffect", "foreignrow", false, Some("GrantedEffects")),
                    ("Level", "i32", false, None),
                ],
                json!([
                    {"GrantedEffect":0,"Level":3},{"GrantedEffect":0,"Level":1},
                    {"GrantedEffect":1,"Level":1},{"GrantedEffect":2,"Level":1},{"GrantedEffect":3,"Level":1}
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
    for dir in ["legacy", "branch", "policy"] {
        fs::create_dir(f.root.join(dir)).unwrap();
    }
    let mut policy: Value = serde_json::from_slice(include_bytes!(
        "../../../../../pipeline/skill-overrides/0.5.5.5-facts-compat-policy.json"
    ))
    .unwrap();
    policy["legacy_numeric_exceptions"] = json!([{"source_row":3,"key":{"skill":"Ghost","stat":"base_multiplier"},"reason":"Fixture virtual effect; retain as community-only."}]);
    policy["unbound_retained_entries"] = json!([
        {"source_row":7,"key":{"skill":"Ghost","stat":"explode_corpse","stat_set":1},"codec_present":false,"target_effect_present":false,"action":"retain unchanged compatibility entry; no inferred target branch ownership"},
        {"source_row":8,"key":{"skill":"UnboundPresent","stat":"implicit_stat","stat_set":1,"stat_id":"implicit_b"},"codec_present":false,"target_effect_present":true,"action":"retain unchanged compatibility entry; no inferred target branch ownership"}
    ]);
    write(
        &f,
        SOURCE,
        &json!({"_meta":policy["source"]["metadata"],"future_top":{"preserve":null},"overrides":[
            {"skill":"ZOwner","stat":"base_multiplier","per_level":[[1,123.0],[2,999.0]],"future":{"nullable":null,"opaque":true}},
            {"skill":"Alias","stat":"base_multiplier","value":70.0},
            {"skill":"Constant","stat":"base_multiplier","value":999.0},
            {"skill":"Ghost","stat":"base_multiplier","value":7.0,"unknown":"archive"},
            {"skill":"ZOwner","stat":"implicit_stat","stat_set":7,"stat_id":"implicit_a","unsupported":{"mods":[{"kind":"future","value":-2,"tag":null}]}},
            {"skill":"Alias","stat":"explode_corpse","stat_set":4,"value":1.0},
            {"skill":"Constant","stat":"skill_attack_speed_more","stat_set":3,"value":-25.0},
            {"skill":"Ghost","stat":"explode_corpse","stat_set":1,"value":1.0},
            {"skill":"UnboundPresent","stat":"implicit_stat","stat_set":1,"stat_id":"implicit_b"},
            {"skill":"ZOwner","stat":"dot_is_area","stat_set":7,"value":0.0}
        ]}),
    );
    write(
        &f,
        CODEC,
        &json!({"_meta":policy["branch_codec"]["metadata"],"labels":[
            {"skill":"ZOwner","set_id":"Extra","set_index":7,"label":""},
            {"skill":"ZOwner","set_id":"Main","set_index":9,"label":"not an index source"},
            {"skill":"Alias","set_id":"Main","set_index":4,"label":"Alias"},
            {"skill":"Constant","set_id":"ConstantSet","set_index":3,"label":"Constant"},
            {"skill":"UnusedArchive","set_id":"UnacquiredSet","set_index":12,"label":""}
        ]}),
    );
    write(&f, POLICY, &policy);
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
    p["branch_codec"]["sha256"] = sha256_hex(&fs::read(f.root.join(CODEC)).unwrap()).into();
    write(f, POLICY, &p);
}
pub(in crate::ordinary_facts) fn args(f: &Fixture) -> Args {
    Args {
        domain: Domain::SkillOverrides,
        codec: Some(CodecArgs {
            source: f.root.join(SOURCE),
            source_sha256: sha256_hex(&fs::read(f.root.join(SOURCE)).unwrap()),
            policy: f.root.join(POLICY),
            policy_sha256: sha256_hex(&fs::read(f.root.join(POLICY)).unwrap()),
        }),
        branch_codec: Some(BranchCodecArgs {
            path: f.root.join(CODEC),
            sha256: sha256_hex(&fs::read(f.root.join(CODEC)).unwrap()),
        }),
        ..f.args()
    }
}
fn read(f: &Fixture, path: &str) -> Value {
    doc(f, &format!("out/{path}"))
}
fn rejects(f: &Fixture, expected: &str) {
    let e = run(args(f)).unwrap_err();
    assert!(e.contains(expected), "expected {expected:?}, got {e:?}");
    assert!(!f.root.join("out").exists());
}
fn append(f: &mut Fixture, table: &str, row: Value) {
    let d = f
        .documents
        .get_mut(&format!("tables/{table}.json"))
        .unwrap();
    let rows = d["rows"].as_array_mut().unwrap();
    rows.push(json!({"_row":rows.len(),"values":row}));
    d["validation"]["rows"] = json!(d["rows"].as_array().unwrap().len());
}

#[test]
fn target_primary_numbers_rebuild_every_level_and_preserve_raw_prepared_values() {
    let f = fixture();
    run(args(&f)).unwrap();
    let raw = doc(&f, SOURCE);
    let out = read(&f, OVERLAY);
    let dto: SkillOverridesDef = serde_json::from_value(out.clone()).unwrap();
    assert_eq!(dto.overrides.len(), 10);
    assert_eq!(
        out["overrides"][0]["per_level"],
        json!([[1, 0.0], [3, 1.1]])
    ); // no old L2 filler
    assert_eq!(
        out["overrides"][1]["per_level"],
        out["overrides"][0]["per_level"]
    );
    assert!(out["overrides"][1].get("value").is_none());
    assert_eq!(out["overrides"][2]["value"], json!(0.8));
    assert!(out["overrides"][2].get("per_level").is_none());
    for index in 3..10 {
        assert_eq!(
            out["overrides"][index], raw["overrides"][index],
            "entry {index}"
        );
    }
    assert_eq!(out["overrides"][0]["future"], raw["overrides"][0]["future"]);
    assert_eq!(out["future_top"], raw["future_top"]);
    let receipt = read(&f, "producer-receipt.json");
    let c = &receipt["conversion"];
    assert_eq!(c["target_numeric_entries"], 3);
    assert_eq!(c["legacy_only_numeric_entries"], 1);
    assert_eq!(c["retained_interpretation_entries"], 6);
    let r = &c["entry_records"];
    assert_eq!(r[0]["binding"]["target_owner"]["primary"]["source_row"], 1);
    assert_eq!(
        r[0]["binding"]["target_owner"]["additional"],
        json!([
            {"source_row":0,"set_id":"Extra"},{"source_row":0,"set_id":"Extra"},{"source_row":0,"set_id":"Extra"}
        ])
    );
    let levels = &r[0]["binding"]["numbers"]["source_level_rows"];
    assert_eq!(levels[0]["gem_level"], 0);
    assert_eq!(levels[0]["emitted"], false);
    assert_eq!(levels[1]["source_row"], 1);
    assert_eq!(levels[1]["raw_base_multiplier"], -10000);
    assert_eq!(levels[1]["gpl_pair"]["source_row"], 1);
    assert_eq!(levels[2]["gem_level"], 3);
    assert_eq!(levels[2]["source_row"], 0);
    assert_eq!(levels[2]["legacy_comparison"]["value"], Value::Null);
    assert_eq!(
        r[2]["binding"]["numbers"]["source_level_rows"][1]["gpl_pair"],
        Value::Null
    );
    assert_eq!(r[4]["binding"]["branch"]["codec_record"]["set_index"], 7);
    assert_eq!(r[4]["binding"]["branch"]["codec_record"]["label"], "");
    assert_eq!(r[4]["binding"]["implicit_stat"]["source_row"], 0);
    assert_eq!(r[8]["binding"]["branch"]["status"], "policy-unbound");
    assert_eq!(
        r[8]["binding"]["branch"]["target_owner"]["effect_source_row"],
        3
    );
    for record in r.as_array().unwrap() {
        let source = raw
            .pointer(record["source_pointer"].as_str().unwrap())
            .unwrap();
        assert_eq!(
            record["source_entry_sha256"],
            sha256_hex(&reader::json_bytes(source).unwrap())
        );
        let index = record["source_row"].as_u64().unwrap() as usize;
        assert_eq!(
            record["output_entry_sha256"],
            sha256_hex(&reader::json_bytes(&out["overrides"][index]).unwrap())
        );
    }
    assert_eq!(
        out["_meta"]["target_numeric"]["source_identity"]["game_patch"],
        "0.5.5.5"
    );
    assert_eq!(
        out["_meta"]["legacy_compatibility"]["source"]["snapshot_version"],
        "0.5.5.4.2"
    );
    assert_eq!(
        out["_meta"]["legacy_compatibility"]["source"]["metadata"],
        raw["_meta"]
    );
}

#[test]
fn changed_target_values_and_new_levels_ignore_legacy_numbers_and_gpl_whitelists() {
    let mut f = fixture();
    f.values("GrantedEffectStatSetsPerLevel", 0)["BaseMultiplier"] = json!(-12345);
    f.values("GrantedEffectStatSetsPerLevel", 2)["BaseMultiplier"] = json!(3000);
    append(
        &mut f,
        "GrantedEffectStatSetsPerLevel",
        json!({"StatSet":1,"GemLevel":55,"BaseMultiplier":i32::MAX,"ActorLevel":1}),
    );
    let mut legacy = doc(&f, SOURCE);
    legacy["overrides"][0]["per_level"] = json!([[77, -1234567.0]]);
    write(&f, SOURCE, &legacy);
    reseal(&mut f);
    run(args(&f)).unwrap();
    let out = read(&f, OVERLAY);
    assert_eq!(
        out["overrides"][0]["per_level"],
        json!([
            [1, 0.0],
            [3, 1.0 + f64::from(-12345) / 10000.0],
            [55, 1.0 + f64::from(i32::MAX) / 10000.0]
        ])
    );
    assert_eq!(
        out["overrides"][2]["per_level"],
        json!([[1, 0.8], [2, 1.3]])
    );
    assert!(out["overrides"][2].get("value").is_none());
    let r = read(&f, "producer-receipt.json");
    let rows = r["conversion"]["entry_records"][0]["binding"]["numbers"]["source_level_rows"]
        .as_array()
        .unwrap();
    assert_eq!(rows.last().unwrap()["gem_level"], 55);
    assert!(rows.last().unwrap()["gpl_pair"].is_null());
    assert_eq!(rows[2]["legacy_comparison"]["f64_equal"], Value::Null);
    // A legacy per-level entry stays per-level even if target rows become equal.
    let mut f = fixture();
    f.values("GrantedEffectStatSetsPerLevel", 0)["BaseMultiplier"] = json!(-10000);
    reseal(&mut f);
    run(args(&f)).unwrap();
    assert_eq!(
        read(&f, OVERLAY)["overrides"][0]["per_level"],
        json!([[1, 0.0], [3, 0.0]])
    );
}

#[test]
fn missing_primary_source_never_falls_back_to_old_constant_or_points() {
    for mode in [
        "empty",
        "missing-constant",
        "nonpositive",
        "bad-primary",
        "null-primary",
    ] {
        let mut f = fixture();
        match mode {
            "empty" => {
                for i in [0, 1, 4] {
                    f.values("GrantedEffectStatSetsPerLevel", i)["StatSet"] = json!(0);
                }
            }
            "nonpositive" => {
                f.values("GrantedEffectStatSetsPerLevel", 0)["GemLevel"] = json!(-3);
                f.values("GrantedEffectStatSetsPerLevel", 1)["GemLevel"] = json!(-1);
            }
            "missing-constant" => {
                f.values("GrantedEffects", 2)["StatSet"] = json!(0);
                f.values("GrantedEffectStatSetsPerLevel", 5)["StatSet"] = json!(2);
                f.values("GrantedEffectStatSetsPerLevel", 5)["GemLevel"] = json!(5);
            }
            "bad-primary" => f.values("GrantedEffects", 0)["StatSet"] = json!(99),
            _ => f.values("GrantedEffects", 0)["StatSet"] = Value::Null,
        }
        reseal(&mut f);
        rejects(
            &f,
            if matches!(mode, "empty" | "missing-constant") {
                "missing primary"
            } else if mode == "nonpositive" {
                "no positive"
            } else {
                "StatSet"
            },
        );
    }
}

#[test]
fn virtual_and_unbound_exceptions_are_exact_and_fully_consumed() {
    for mode in [
        "missing-numeric",
        "wrong-key",
        "wrong-row",
        "stale-numeric",
        "missing-unbound",
        "wrong-presence",
        "stale-unbound",
        "unused",
        "new-retained",
    ] {
        let mut f = fixture();
        let mut p = doc(&f, POLICY);
        let mut legacy = doc(&f, SOURCE);
        match mode {
            "missing-numeric" => p["legacy_numeric_exceptions"] = json!([]),
            "wrong-key" => {
                p["legacy_numeric_exceptions"][0]["key"]["skill"] = json!("AnotherGhost")
            }
            "wrong-row" => p["legacy_numeric_exceptions"][0]["source_row"] = json!(4),
            "stale-numeric" => append(
                &mut f,
                "GrantedEffects",
                json!({"Id":"Ghost","StatSet":2,"AdditionalStatSets":[]}),
            ),
            "missing-unbound" => {
                p["unbound_retained_entries"]
                    .as_array_mut()
                    .unwrap()
                    .remove(0);
            }
            "wrong-presence" => {
                p["unbound_retained_entries"][1]["target_effect_present"] = json!(false)
            }
            "stale-unbound" => {
                let mut codec = doc(&f, CODEC);
                codec["labels"].as_array_mut().unwrap().push(json!({"skill":"UnboundPresent","set_id":"ConstantSet","set_index":1,"label":""}));
                write(&f, CODEC, &codec);
            }
            "unused" => {
                let mut extra = p["unbound_retained_entries"][0].clone();
                extra["source_row"] = json!(99);
                extra["key"]["skill"] = json!("UnusedGhost");
                p["unbound_retained_entries"]
                    .as_array_mut()
                    .unwrap()
                    .push(extra);
            }
            _ => {
                legacy["overrides"].as_array_mut().unwrap().push(json!({"skill":"AnotherGhost","stat":"explode_corpse","stat_set":2,"value":1.0}));
            }
        };
        write(&f, POLICY, &p);
        write(&f, SOURCE, &legacy);
        reseal(&mut f);
        rejects(
            &f,
            if mode == "stale-unbound" || mode == "stale-numeric" || mode == "unused" {
                "stale"
            } else if mode == "wrong-presence"
                || mode == "missing-unbound"
                || mode == "new-retained"
            {
                "unbound"
            } else {
                "numeric exception"
            },
        );
    }
}

#[test]
fn retained_branches_require_actual_codec_membership_and_implicit_ids() {
    for mode in [
        "wrong-set",
        "missing-effect",
        "missing-codec",
        "missing-stat",
        "duplicate-index",
        "duplicate-set",
        "empty-id",
    ] {
        let mut f = fixture();
        let mut codec = doc(&f, CODEC);
        match mode {
            "wrong-set" => codec["labels"][0]["set_id"] = json!("ConstantSet"),
            "missing-effect" => f.values("GrantedEffects", 1)["Id"] = json!("NoAlias"),
            "missing-codec" => {
                codec["labels"].as_array_mut().unwrap().remove(0);
            }
            "missing-stat" => f.values("Stats", 0)["Id"] = json!("renamed"),
            "duplicate-index" => {
                let extra = codec["labels"][0].clone();
                codec["labels"].as_array_mut().unwrap().push(extra);
            }
            "duplicate-set" => codec["labels"][1]["set_id"] = json!("Extra"),
            _ => codec["labels"][0]["skill"] = json!(""),
        }
        write(&f, CODEC, &codec);
        reseal(&mut f);
        rejects(
            &f,
            match mode {
                "wrong-set" => "not target raw-owned",
                "missing-stat" => "policy conflict",
                "missing-effect" => "numeric exception",
                "missing-codec" => "unbound",
                "empty-id" => "canonical",
                _ => "duplicate",
            },
        );
    }
}

#[test]
fn bad_physical_values_keys_and_consumed_metadata_fail_before_reserving_output() {
    for (table, row, field, value) in [
        (
            "GrantedEffectStatSetsPerLevel",
            0,
            "BaseMultiplier",
            Value::Null,
        ),
        (
            "GrantedEffectStatSetsPerLevel",
            0,
            "BaseMultiplier",
            json!("1000"),
        ),
        (
            "GrantedEffectStatSetsPerLevel",
            0,
            "BaseMultiplier",
            json!(1.0),
        ),
        (
            "GrantedEffectStatSetsPerLevel",
            0,
            "BaseMultiplier",
            json!(2147483648_i64),
        ),
        (
            "GrantedEffectStatSetsPerLevel",
            0,
            "BaseMultiplier",
            json!(-2147483649_i64),
        ),
        ("GrantedEffectStatSetsPerLevel", 0, "GemLevel", json!(1)),
        (
            "GrantedEffectStatSetsPerLevel",
            0,
            "GemLevel",
            json!(2147483648_i64),
        ),
        ("GrantedEffectStatSetsPerLevel", 0, "StatSet", Value::Null),
        ("GrantedEffectStatSetsPerLevel", 0, "StatSet", json!(-1)),
        ("GrantedEffectStatSetsPerLevel", 0, "StatSet", json!(99)),
        ("GrantedEffectsPerLevel", 0, "Level", json!(1)),
        ("GrantedEffectsPerLevel", 0, "Level", json!(true)),
        ("GrantedEffectsPerLevel", 0, "GrantedEffect", json!(99)),
        ("GrantedEffects", 0, "AdditionalStatSets", json!([null])),
        ("GrantedEffects", 0, "AdditionalStatSets", json!([0, 99])),
        ("GrantedEffects", 0, "AdditionalStatSets", json!(0)),
        ("Stats", 1, "Id", json!("implicit_a")),
        ("GrantedEffects", 0, "Id", json!(" Alias ")),
    ] {
        let mut f = fixture();
        f.values(table, row)[field] = value;
        reseal(&mut f);
        rejects(&f, "");
    }
    for mode in [
        "missing-value",
        "physical-key",
        "metadata-type",
        "metadata-array",
        "metadata-fk",
    ] {
        let mut f = fixture();
        match mode {
            "missing-value" => {
                f.values("GrantedEffectStatSetsPerLevel", 0)
                    .as_object_mut()
                    .unwrap()
                    .remove("BaseMultiplier");
            }
            "physical-key" => {
                f.documents
                    .get_mut("tables/GrantedEffectStatSetsPerLevel.json")
                    .unwrap()["rows"][1]["_row"] = json!(0)
            }
            "metadata-type" => {
                f.documents
                    .get_mut("tables/GrantedEffectStatSetsPerLevel.json")
                    .unwrap()["columns"][2]["schema"]["type"] = json!("f32")
            }
            "metadata-array" => {
                f.documents.get_mut("tables/GrantedEffects.json").unwrap()["columns"][2]["schema"]
                    ["array"] = json!(false)
            }
            _ => {
                f.documents
                    .get_mut("tables/GrantedEffectsPerLevel.json")
                    .unwrap()["columns"][0]["schema"]["references"]["table"] = json!("Stats")
            }
        }
        reseal(&mut f);
        rejects(&f, "");
    }
}

#[test]
fn override_shapes_duplicates_new_types_and_policy_schema_are_strict() {
    for mode in [
        "duplicate-entry",
        "duplicate-level",
        "null-value",
        "both",
        "empty-levels",
        "bad-value",
        "bad-tuple",
        "zero-level",
        "null-index",
        "unknown-stat",
        "implicit-value",
        "missing-stat-id",
    ] {
        let mut f = fixture();
        let mut s = doc(&f, SOURCE);
        match mode {
            "duplicate-entry" => {
                let extra = s["overrides"][0].clone();
                s["overrides"].as_array_mut().unwrap().push(extra);
            }
            "duplicate-level" => s["overrides"][0]["per_level"] = json!([[1, 0.0], [1, 1.0]]),
            "null-value" => s["overrides"][1]["value"] = Value::Null,
            "both" => s["overrides"][0]["value"] = json!(1.0),
            "empty-levels" => s["overrides"][0]["per_level"] = json!([]),
            "bad-value" => s["overrides"][1]["value"] = json!("not a number"),
            "bad-tuple" => s["overrides"][0]["per_level"] = json!([[1, 1.0, 7]]),
            "zero-level" => s["overrides"][0]["per_level"] = json!([[0, 1.0]]),
            "null-index" => s["overrides"][4]["stat_set"] = Value::Null,
            "unknown-stat" => s["overrides"][9]["stat"] = json!("future_mechanic"),
            "implicit-value" => s["overrides"][4]["value"] = json!(0),
            _ => {
                s["overrides"][4].as_object_mut().unwrap().remove("stat_id");
            }
        }
        write(&f, SOURCE, &s);
        reseal(&mut f);
        rejects(&f, "");
    }
    for mode in [
        "schema",
        "patch",
        "target-pin",
        "source-pin",
        "branch-pin",
        "version",
        "path",
        "meta",
        "unknown-field",
        "duplicate-exception",
        "duplicate-unbound",
        "bad-action",
        "categories",
    ] {
        let f = fixture();
        let mut p = doc(&f, POLICY);
        match mode {
            "schema" => p["schema"] = json!("future"),
            "patch" => p["target"]["game_patch"] = json!("old"),
            "target-pin" => p["target"]["facts_manifest_sha256"] = json!("0".repeat(64)),
            "source-pin" => p["source"]["sha256"] = json!("0".repeat(64)),
            "branch-pin" => p["branch_codec"]["sha256"] = json!("0".repeat(64)),
            "version" => p["source"]["snapshot_version"] = json!("0.5.5.5"),
            "path" => p["branch_codec"]["path"] = json!("data/other.json"),
            "meta" => p["source"]["metadata"]["schema"] = json!("stat_set_labels/v1"),
            "unknown-field" => p["future_policy"] = json!(true),
            "duplicate-exception" => {
                let e = p["legacy_numeric_exceptions"][0].clone();
                p["legacy_numeric_exceptions"]
                    .as_array_mut()
                    .unwrap()
                    .push(e);
            }
            "duplicate-unbound" => {
                let e = p["unbound_retained_entries"][0].clone();
                p["unbound_retained_entries"]
                    .as_array_mut()
                    .unwrap()
                    .push(e);
            }
            "bad-action" => p["unbound_retained_entries"][0]["action"] = json!("infer primary"),
            _ => p["retained_interpretation_stats"] = json!(["implicit_stat"]),
        }
        write(&f, POLICY, &p);
        rejects(&f, "");
    }
}

#[test]
fn every_auxiliary_pin_precedes_parsing_and_original_meta_is_required() {
    for path in [SOURCE, CODEC, POLICY] {
        let f = fixture();
        let a = args(&f);
        fs::write(f.root.join(path), b"not json").unwrap();
        let e = run(a).unwrap_err();
        assert!(e.contains("SHA256 mismatch"), "{path}: {e}");
        assert!(!f.root.join("out").exists());
        let mut f = fixture();
        fs::write(f.root.join(path), b"{\"duplicate\":1,\"duplicate\":2}").unwrap();
        if path != POLICY {
            reseal(&mut f);
        }
        rejects(&f, "duplicate");
    }
    for path in [SOURCE, CODEC] {
        let mut f = fixture();
        let mut d = doc(&f, path);
        d["_meta"]["vendor_commit"] = json!("0".repeat(40));
        write(&f, path, &d);
        reseal(&mut f);
        rejects(&f, "metadata differs");
    }
    let f = fixture();
    let a = args(&f);
    fs::remove_file(f.root.join(CODEC)).unwrap();
    assert!(run(a).is_err());
    assert!(!f.root.join("out").exists());
    // Even repinned invalid numeric JSON must fail, not acquire a fallback/default.
    let mut f = fixture();
    let s = String::from_utf8(fs::read(f.root.join(SOURCE)).unwrap())
        .unwrap()
        .replace("999.0", "1e999");
    fs::write(f.root.join(SOURCE), s).unwrap();
    reseal(&mut f);
    rejects(&f, "");
}

#[test]
fn eight_files_bind_separate_provenance_and_replay_without_adoption() {
    let f = fixture();
    run(args(&f)).unwrap();
    run(Args {
        out: f.root.join("again"),
        ..args(&f)
    })
    .unwrap();
    let r = read(&f, "producer-receipt.json");
    assert_eq!(r["files"].as_object().unwrap().len(), 7);
    assert_eq!(
        r["input"]["selected_payloads"].as_object().unwrap().len(),
        5
    );
    assert_eq!(r["complete_runtime"], false);
    assert_eq!(r["adoption"], "not-adopted");
    assert_eq!(
        read(&f, "_domain_candidate.json")["domain"],
        "ordinary-skill-overrides"
    );
    assert!(!f.root.join("out/base").exists());
    assert!(!f.root.join("out/manifest.json").exists());
    for (path, sidecar) in [
        (SOURCE, "legacy-skill-overrides.json"),
        (CODEC, "branch-codec.json"),
        (POLICY, "codec-policy.json"),
    ] {
        assert_eq!(
            fs::read(f.root.join(path)).unwrap(),
            fs::read(f.root.join("out").join(sidecar)).unwrap()
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
    for name in ["legacy_source", "branch_codec", "codec_policy"] {
        let id = &r["conversion"][name];
        assert_eq!(
            id["sha256"],
            r["files"][id["file"].as_str().unwrap()]["sha256"]
        );
    }
}

#[test]
fn all_six_auxiliary_flags_are_explicit_and_cross_domain_flags_fail() {
    let base = [
        "--ordinary-facts",
        "F",
        "--facts-manifest-sha256",
        "sha",
        "--version",
        "0.5.5.5",
        "--out",
        "fresh",
        "--ordinary-domain",
        "skill-overrides",
        "--codec-source",
        "source",
        "--codec-source-sha256",
        "sha",
        "--codec-policy",
        "policy",
        "--codec-policy-sha256",
        "sha",
        "--branch-codec",
        "branch",
        "--branch-codec-sha256",
        "sha",
    ];
    assert!(matches!(
        crate::parse_args_from(base.into_iter().map(String::from)),
        Ok(crate::Mode::OrdinaryFacts(_))
    ));
    for index in (10..base.len()).step_by(2) {
        let mut args = base.to_vec();
        args.drain(index..index + 2);
        assert!(parse_args(args.into_iter().map(String::from)).is_err());
        assert!(
            parse_args(
                base.into_iter()
                    .chain([base[index], "duplicate"])
                    .map(String::from)
            )
            .is_err()
        );
    }
    for pair in [
        ["--quality-policy", "p"],
        ["--quality-policy-sha256", "sha"],
        ["--raw", "old"],
        ["--native-facts", "F"],
        ["--ordinary-domain", "skill-overrides"],
    ] {
        assert!(parse_args(base.into_iter().chain(pair).map(String::from)).is_err());
    }
    assert!(parse_args(base.into_iter().chain(["--branch-codec"]).map(String::from)).is_err());
    for domain in [
        "stats-mods",
        "base-items",
        "skills",
        "gem-quality",
        "stat-set-codec",
        "gem-effects",
        "monster-resistances",
        "totem-varieties",
        "monster-scaling",
    ] {
        let e = parse_args(
            base[..8]
                .iter()
                .copied()
                .chain(["--ordinary-domain", domain, "--branch-codec", "branch"])
                .map(String::from),
        )
        .err()
        .unwrap();
        assert!(e.contains("branch codec arguments"), "{domain}: {e}");
    }
}

#[test]
fn source_and_output_guards_hold_for_the_new_domain() {
    let f = fixture();
    for parent in ["legacy", "branch", "policy", "input"] {
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
        .join(format!(
            "overrides-forbidden-{}",
            f.root.file_name().unwrap().to_string_lossy()
        ));
    assert!(
        run(Args {
            out: out.clone(),
            ..args(&f)
        })
        .is_err()
    );
    assert!(!out.exists());
    for mode in ["pin", "patch"] {
        let mut a = args(&f);
        if mode == "pin" {
            a.expected_manifest_sha256 = "0".repeat(64);
        } else {
            a.version = "old".into();
        }
        assert!(run(a).is_err());
        assert!(!f.root.join("out").exists());
    }
    std::os::unix::fs::symlink(f.root.join("missing"), f.root.join("out")).unwrap();
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
    let a = args(&f);
    fs::write(
        f.root
            .join("input/tables/GrantedEffectStatSetsPerLevel.json"),
        b"{}",
    )
    .unwrap();
    assert!(run(a).is_err());
    assert!(!f.root.join("out").exists());
}

#[test]
fn previous_non_quality_domains_keep_files_and_receipt_shapes() {
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
    ] {
        let mut f = match domain {
            Domain::StatSetCodec => super::super::stat_set_codec::tests::fixture(),
            Domain::GemEffects => super::super::gem_effects::tests::fixture(),
            Domain::MonsterResistances => super::super::monster_resistances::tests::fixture(),
            Domain::TotemVarieties => super::super::totem_varieties::tests::fixture(),
            Domain::MonsterScaling => super::super::monster_scaling::tests::fixture(),
            Domain::SkillStatMap => super::super::skill_stat_map::tests::fixture(),
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
            _ => Args { domain, ..f.args() },
        };
        run(a()).unwrap();
        let new = fixture();
        run(args(&new)).unwrap();
        run(Args {
            out: f.root.join("again"),
            ..a()
        })
        .unwrap();
        let r = read(&f, "producer-receipt.json");
        assert_eq!(r.as_object().unwrap().len(), 10);
        assert_eq!(r["files"].as_object().unwrap().len(), count);
        assert!(!f.root.join("out/legacy-skill-overrides.json").exists());
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
