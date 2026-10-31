use std::collections::BTreeMap;
use std::fs;

use pobr_data::catalog::GemEffectsDef;
use serde_json::{Value, json};

use super::super::stat_set_codec::tests::table;
use super::super::tests::Fixture;
use super::super::{Args, CodecArgs, Domain, parse_args, run};
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
            "BaseItemTypes",
            vec![
                ("Id", "string", false, None),
                ("Name", "string", false, None),
            ],
            json!([
                {"Id":"ZLegacy","Name":"Unknown"},{"Id":"AFirst","Name":"Attack"},
                {"Id":"ZNew","Name":"Support"},{"Id":"ANew","Name":"Unknown"},
                {"Id":"BCompound","Name":"Minion compound"},{"Id":"Dev","Name":"[DNT] Placeholder"}
            ]),
        ),
        (
            "SkillGems",
            vec![
                ("BaseItemType", "foreignrow", false, Some("BaseItemTypes")),
                ("GemEffects", "foreignrow", true, Some("GemEffects")),
            ],
            json!([
                {"BaseItemType":4,"GemEffects":[4]},{"BaseItemType":0,"GemEffects":[0]},
                {"BaseItemType":3,"GemEffects":[3]},{"BaseItemType":2,"GemEffects":[2]},
                {"BaseItemType":1,"GemEffects":[1]},{"BaseItemType":5,"GemEffects":[5]},
                {"BaseItemType":null,"GemEffects":[5]}
            ]),
        ),
        (
            "GemEffects",
            vec![
                ("Id", "string", false, None),
                ("GrantedEffect", "foreignrow", false, Some("GrantedEffects")),
                (
                    "AdditionalGrantedEffects",
                    "foreignrow",
                    true,
                    Some("GrantedEffects"),
                ),
                ("_index", "i32", false, None),
            ],
            json!([
                {"Id":"VariantZ","GrantedEffect":0,"AdditionalGrantedEffects":[],"_index":999},
                {"Id":"VariantA","GrantedEffect":0,"AdditionalGrantedEffects":[],"_index":998},
                {"Id":"NewZ","GrantedEffect":1,"AdditionalGrantedEffects":[],"_index":997},
                {"Id":"NewA","GrantedEffect":1,"AdditionalGrantedEffects":[],"_index":996},
                {"Id":"Compound","GrantedEffect":2,"AdditionalGrantedEffects":[3,0,3],"_index":995},
                {"Id":"Excluded","GrantedEffect":null,"AdditionalGrantedEffects":[],"_index":994}
            ]),
        ),
        (
            "GrantedEffects",
            vec![
                ("Id", "string", false, None),
                (
                    "AdditionalStatSets",
                    "foreignrow",
                    true,
                    Some("GrantedEffectStatSets"),
                ),
            ],
            json!([
                {"Id":"SharedPrimary","AdditionalStatSets":[]},{"Id":"NewPrimary","AdditionalStatSets":[]},
                {"Id":"CompoundPrimary","AdditionalStatSets":[0,1,0]},{"Id":"Additional","AdditionalStatSets":[]}
            ]),
        ),
        (
            "GrantedEffectStatSets",
            vec![("Id", "string", false, None)],
            json!([
                {"Id":"SetZ"},{"Id":"SetA"}
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
    fs::create_dir(f.root.join("codec")).unwrap();
    fs::create_dir(f.root.join("policy")).unwrap();
    let p: Value = serde_json::from_slice(include_bytes!(
        "../../../../../pipeline/skill-codec/0.5.5.5-gem-effects-policy.json"
    ))
    .unwrap();
    write_codec(
        &f,
        &json!({"_meta":p["source"]["metadata"],"gems":[
            {"gem_id":"ZLegacy","variant_id":"VariantZ","granted_effect_id":"SharedPrimary"},
            {"gem_id":"BCompound","variant_id":"Compound","granted_effect_id":"CompoundPrimary",
             "additional_granted_effect_ids":["Additional","SharedPrimary","Additional"],
             "additional_stat_set_ids":["SetZ","SetA","SetZ"],"granted_effect_display_order":[2,0]}
        ]}),
    );
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
fn write_codec(f: &Fixture, value: &Value) {
    let mut bytes = b" \n\t".to_vec();
    bytes.extend(serde_json::to_vec(value).unwrap());
    bytes.extend(b"\n\n");
    fs::write(f.root.join("codec/source.json"), bytes).unwrap();
}
fn write_policy(f: &Fixture, value: &Value) {
    fs::write(
        f.root.join("policy/policy.json"),
        serde_json::to_vec_pretty(value).unwrap(),
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
        domain: Domain::GemEffects,
        codec: Some(CodecArgs {
            source_sha256: sha256_hex(&fs::read(&source).unwrap()),
            source,
            policy_sha256: sha256_hex(&fs::read(&policy).unwrap()),
            policy,
        }),
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
fn target_links_keep_new_gems_duplicates_partial_display_and_existing_owner_precedence() {
    let f = fixture();
    run(args(&f)).unwrap();
    let dto: GemEffectsDef =
        crate::read_json(&f.root.join("out/overlay/gem_effects.json")).unwrap();
    assert_eq!(
        dto.gems
            .iter()
            .map(|g| g.gem_id.as_str())
            .collect::<Vec<_>>(),
        ["AFirst", "ZNew", "ANew", "BCompound", "ZLegacy"]
    );
    let by_gem: BTreeMap<_, _> = dto.gems.iter().map(|g| (g.gem_id.as_str(), g)).collect();
    assert_eq!(by_gem.len(), 5); // All three non-legacy gems remain, including shared-primary rows.
    let compound = by_gem["BCompound"];
    assert_eq!(
        compound.additional_granted_effect_ids,
        ["Additional", "SharedPrimary", "Additional"]
    );
    assert_eq!(compound.additional_stat_set_ids, ["SetZ", "SetA", "SetZ"]);
    assert_eq!(compound.granted_effect_display_order, [2, 0]); // Partial, not sorted or filled.
    assert_eq!(by_gem["ZLegacy"].variant_id, "VariantZ"); // row zero is not an ID.
    assert!(by_gem["ANew"].granted_effect_display_order.is_empty());

    // Reproduce only the unchanged consumers' map operations, no new runtime algorithm.
    let mut before = BTreeMap::new();
    for g in by_gem.values() {
        before
            .entry(g.granted_effect_id.as_str())
            .or_insert(g.gem_id.as_str());
    }
    assert_eq!(before["SharedPrimary"], "AFirst");
    let mut expected: BTreeMap<String, String> = before
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    for g in serde_json::from_value::<GemEffectsDef>(codec(&f))
        .unwrap()
        .gems
    {
        expected.insert(g.granted_effect_id, g.gem_id);
    }
    let actual: BTreeMap<_, _> = dto
        .gems
        .iter()
        .map(|g| (g.granted_effect_id.clone(), g.gem_id.clone()))
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(actual["SharedPrimary"], "ZLegacy"); // Explicit codec owner wins over base-first AFirst.
    assert_eq!(actual["NewPrimary"], "ANew"); // No codec: existing ID-sorted first owner wins.
    let receipt = read(&f, "producer-receipt.json");
    let c = &receipt["conversion"];
    assert_eq!(c["gems"], 5);
    assert_eq!(c["new_gems_without_legacy_codec"], 3);
    assert_eq!(c["shared_primary_owners"].as_array().unwrap().len(), 2);
    assert_eq!(c["filtered_placeholders"].as_array().unwrap().len(), 2);
    let record = c["source_records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["gem_id"] == "BCompound")
        .unwrap();
    assert_eq!(record["source"]["skill_gem_source_row"], 0);
    assert_eq!(
        record["source"]["base"],
        json!({"source_row":4,"id":"BCompound"})
    );
    assert_eq!(
        record["source"]["additional_stat_sets"],
        json!([
            {"position":0,"target":{"source_row":0,"id":"SetZ"}},
            {"position":1,"target":{"source_row":1,"id":"SetA"}},
            {"position":2,"target":{"source_row":0,"id":"SetZ"}}
        ])
    );
    assert_eq!(
        c["unconsumed_source"][2]["columns"][0]["physical_fields"],
        json!(["_index"])
    );
}

#[test]
fn target_output_has_separate_verbatim_provenance_hashes_and_replays() {
    let f = fixture();
    run(args(&f)).unwrap();
    assert!(!f.root.join("out/base").exists());
    assert!(!f.root.join("out/manifest.json").exists());
    for (output, input) in [
        ("legacy-gem-effects.json", "codec/source.json"),
        ("codec-policy.json", "policy/policy.json"),
        ("source-manifest.json", "input/manifest.json"),
        ("source-lock.json", "input/source-lock.json"),
    ] {
        assert_eq!(
            fs::read(f.root.join("out").join(output)).unwrap(),
            fs::read(f.root.join(input)).unwrap()
        );
    }
    let r = read(&f, "producer-receipt.json");
    assert_eq!(r["complete_runtime"], false);
    assert_eq!(r["adoption"], "not-adopted");
    assert_eq!(
        r["input"]["selected_payloads"].as_object().unwrap().len(),
        6
    );
    assert_eq!(r["files"].as_object().unwrap().len(), 6);
    assert_eq!(
        read(&f, "_domain_candidate.json")["domain"],
        "ordinary-gem-effects"
    );
    let meta = read(&f, "overlay/gem_effects.json");
    assert_eq!(meta["_meta"]["game_patch"], "0.5.5.5");
    assert_eq!(
        meta["_meta"]["legacy_codec"]["snapshot_version"],
        "0.5.5.4.2"
    );
    run(Args {
        out: f.root.join("again"),
        ..args(&f)
    })
    .unwrap();
    for (file, pin) in r["files"].as_object().unwrap() {
        let bytes = fs::read(f.root.join("out").join(file)).unwrap();
        assert_eq!(pin["sha256"], sha256_hex(&bytes));
        assert_eq!(pin["bytes"], bytes.len());
        assert_eq!(bytes, fs::read(f.root.join("again").join(file)).unwrap());
    }
    assert_eq!(
        fs::read(f.root.join("out/producer-receipt.json")).unwrap(),
        fs::read(f.root.join("again/producer-receipt.json")).unwrap()
    );
}

#[test]
fn every_legacy_tuple_component_and_identity_must_match_without_exceptions() {
    for (field, value) in [
        ("variant_id", json!("different")),
        ("granted_effect_id", json!("different")),
        (
            "additional_granted_effect_ids",
            json!(["SharedPrimary", "Additional", "Additional"]),
        ),
        ("additional_stat_set_ids", json!(["SetZ", "SetZ", "SetA"])),
    ] {
        let mut f = fixture();
        let mut d = codec(&f);
        d["gems"][1][field] = value;
        write_codec(&f, &d);
        reseal(&mut f);
        rejects(&f, &format!("target tuple mismatch: {field}"));
    }
    for field in ["gem_id", "variant_id", "granted_effect_id"] {
        let mut f = fixture();
        let mut d = codec(&f);
        d["gems"][0][field] = json!("");
        write_codec(&f, &d);
        reseal(&mut f);
        rejects(&f, "empty link identity");
    }
    let mut f = fixture();
    let mut d = codec(&f);
    d["gems"][0]["gem_id"] = json!("absent");
    write_codec(&f, &d);
    reseal(&mut f);
    rejects(&f, "absent from target");
}

#[test]
fn codec_duplicate_identities_and_bad_display_orders_fail_but_ordered_refs_do_not() {
    for case in [
        "gem",
        "primary",
        "position-duplicate",
        "position-range",
        "position-negative",
        "position-null",
    ] {
        let mut f = fixture();
        let mut d = codec(&f);
        match case {
            "gem" => d["gems"][1]["gem_id"] = json!("ZLegacy"),
            "primary" => d["gems"][1]["granted_effect_id"] = json!("SharedPrimary"),
            "position-duplicate" => d["gems"][1]["granted_effect_display_order"] = json!([0, 0]),
            "position-range" => d["gems"][1]["granted_effect_display_order"] = json!([4]),
            "position-negative" => d["gems"][1]["granted_effect_display_order"] = json!([-1]),
            _ => d["gems"][1]["granted_effect_display_order"] = json!([null]),
        }
        write_codec(&f, &d);
        reseal(&mut f);
        assert!(run(args(&f)).is_err(), "{case}");
        assert!(!f.root.join("out").exists());
    }
}

#[test]
fn all_consumed_physical_values_keys_and_metadata_validate_before_filtering() {
    for case in [
        "duplicate-id",
        "physical-key",
        "duplicate-base",
        "base-fk",
        "base-type",
        "name-null",
        "no-variant",
        "multiple-variants",
        "variant-negative",
        "variant-null",
        "missing-primary",
        "primary-fk",
        "additional-fk",
        "set-fk",
        "null-array",
        "placeholder-fk",
        "unreferenced-fk",
        "column-type",
        "column-target",
        "column-missing",
    ] {
        let mut f = fixture();
        match case {
            "duplicate-id" => f.values("GrantedEffects", 3)["Id"] = json!("SharedPrimary"),
            "physical-key" => {
                f.documents.get_mut("tables/GemEffects.json").unwrap()["rows"][1]["_row"] = json!(0)
            }
            "duplicate-base" => f.values("SkillGems", 4)["BaseItemType"] = json!(0),
            "base-fk" => f.values("SkillGems", 0)["BaseItemType"] = json!(999),
            "base-type" => f.values("SkillGems", 0)["BaseItemType"] = json!(4.0),
            "name-null" => f.values("BaseItemTypes", 5)["Name"] = json!(null),
            "no-variant" => f.values("SkillGems", 0)["GemEffects"] = json!([]),
            "multiple-variants" => f.values("SkillGems", 0)["GemEffects"] = json!([4, 4]),
            "variant-negative" => f.values("SkillGems", 0)["GemEffects"] = json!([-1]),
            "variant-null" => f.values("SkillGems", 0)["GemEffects"] = json!([null]),
            "missing-primary" => f.values("GemEffects", 4)["GrantedEffect"] = json!(null),
            "primary-fk" => f.values("GemEffects", 4)["GrantedEffect"] = json!(999),
            "additional-fk" => f.values("GemEffects", 4)["AdditionalGrantedEffects"] = json!([999]),
            "set-fk" => f.values("GrantedEffects", 2)["AdditionalStatSets"] = json!([999]),
            "null-array" => f.values("GrantedEffects", 2)["AdditionalStatSets"] = json!(null),
            "placeholder-fk" => f.values("SkillGems", 5)["GemEffects"] = json!([999]),
            "unreferenced-fk" => f.values("GrantedEffects", 3)["AdditionalStatSets"] = json!([-1]),
            "column-type" => {
                f.documents.get_mut("tables/GemEffects.json").unwrap()["columns"][1]["schema"]["type"] =
                    json!("i32")
            }
            "column-target" => {
                f.documents.get_mut("tables/GemEffects.json").unwrap()["columns"][1]["schema"]["references"]
                    ["table"] = json!("Stats")
            }
            "column-missing" => {
                f.values("GemEffects", 0)
                    .as_object_mut()
                    .unwrap()
                    .remove("AdditionalGrantedEffects");
            }
            _ => unreachable!(),
        }
        reseal(&mut f);
        assert!(run(args(&f)).is_err(), "{case}");
        assert!(!f.root.join("out").exists(), "{case}");
    }
}

#[test]
fn policy_requires_exact_target_legacy_metadata_and_existing_precedence() {
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
            "/source/metadata/extracted_files",
            json!(["other.lua"]),
            "metadata schema",
        ),
        (
            "/owner_precedence",
            json!("deduplicate"),
            "owner precedence",
        ),
        ("/evidence/source_check/sha256", json!("BAD"), "hexadecimal"),
    ] {
        let f = fixture();
        let mut p = policy(&f);
        *p.pointer_mut(pointer).unwrap() = value;
        write_policy(&f, &p);
        rejects(&f, error);
    }
    let f = fixture();
    let mut p = policy(&f);
    p["exceptions"] = json!([]);
    write_policy(&f, &p);
    rejects(&f, "unknown field");
    let f = fixture();
    let mut p = policy(&f);
    for field in [
        "source_check",
        "legacy_runtime_manifest",
        "target_skill_gems",
    ] {
        p["evidence"][field]["path"] = json!("/absent/not-a-producer-input");
    }
    write_policy(&f, &p);
    run(args(&f)).unwrap();
}

#[test]
fn pins_duplicate_json_and_patch_fail_before_output() {
    for file in ["codec/source.json", "policy/policy.json"] {
        let f = fixture();
        let a = args(&f);
        fs::write(f.root.join(file), b"not json").unwrap();
        assert!(run(a).unwrap_err().contains("SHA256 mismatch"));
        assert!(!f.root.join("out").exists());
        let mut f = fixture();
        fs::write(f.root.join(file), br#"{"duplicate":1,"duplicate":2}"#).unwrap();
        if file.starts_with("codec") {
            reseal(&mut f);
        }
        rejects(&f, "duplicate");
    }
    for kind in ["manifest", "payload", "version"] {
        let f = fixture();
        let mut a = args(&f);
        match kind {
            "manifest" => a.expected_manifest_sha256 = "0".repeat(64),
            "version" => a.version = "0.5.5.4.2".into(),
            _ => fs::write(f.root.join("input/tables/SkillGems.json"), b"{}").unwrap(),
        }
        assert!(run(a).is_err(), "{kind}");
        assert!(!f.root.join("out").exists());
    }
}

#[test]
fn gem_effects_cli_uses_only_the_four_explicit_codec_options() {
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
    let domain = ["--ordinary-domain", "gem-effects"];
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
        assert!(
            parse_args(
                base.into_iter()
                    .chain(domain)
                    .chain(
                        flags
                            .iter()
                            .enumerate()
                            .filter(|(j, _)| *j != i && *j != i + 1)
                            .map(|(_, s)| *s)
                    )
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
                        .chain(["--ordinary-domain", other])
                        .chain(flags)
                        .map(String::from)
                )
                .is_err()
            );
        }
    }
    for extra in [
        ["--quality-policy", "q"],
        ["--quality-policy-sha256", "pin"],
        ["--ordinary-domain", "gem-effects"],
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
fn output_rejects_input_parents_production_symlinks_and_overwrite() {
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
                "gem-effects-forbidden-{}",
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
fn gem_effects_preserves_prior_base_domains_and_stat_set_codec_bytes_and_shapes() {
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
        (Domain::StatSetCodec, None, 1),
    ] {
        let mut f = if domain == Domain::StatSetCodec {
            super::super::stat_set_codec::tests::fixture()
        } else {
            Fixture::new()
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
        let old_args = || {
            if domain == Domain::StatSetCodec {
                super::super::stat_set_codec::tests::args(&f)
            } else {
                Args { domain, ..f.args() }
            }
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
        assert_eq!(
            r["files"].as_object().unwrap().len(),
            count + 3 + usize::from(domain == Domain::StatSetCodec)
        );
        assert!(!f.root.join("out/legacy-gem-effects.json").exists());
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
