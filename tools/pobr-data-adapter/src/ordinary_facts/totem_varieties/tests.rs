use std::collections::BTreeMap;
use std::fs;

use pobr_data::catalog::totem_varieties::{TotemVarietiesDef, TotemVarietyDef};
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
            "GrantedEffects",
            vec![
                ("Id", "string", false, None),
                ("ActiveSkill", "foreignrow", false, Some("ActiveSkills")),
            ],
            json!([
                {"Id":"ZPlayerLike","ActiveSkill":1},{"Id":"ARogueAlias","ActiveSkill":1},
                {"Id":"UntaggedNpc","ActiveSkill":0},{"Id":"UnresolvedSummoner","ActiveSkill":2},
                {"Id":"UnmatchedOther","ActiveSkill":3},{"Id":"NullActive","ActiveSkill":null},
                {"Id":"NegativeSkinOnly","ActiveSkill":4},{"Id":"NullTwo","ActiveSkill":null},
                {"Id":"NullThree","ActiveSkill":null},{"Id":"UnmatchedTwo","ActiveSkill":3},
                {"Id":"UnmatchedThree","ActiveSkill":3}
            ]),
        ),
        (
            "ActiveSkills",
            vec![
                ("Id", "string", false, None),
                ("SkillTotemId", "enumrow", false, Some("SkillTotems")),
                (
                    "ActiveSkillTypes",
                    "foreignrow",
                    true,
                    Some("ActiveSkillType"),
                ),
                ("GrantedEffect", "string", false, None),
            ],
            json!([
                {"Id":"ZZeroSelector","SkillTotemId":0,"ActiveSkillTypes":[],"GrantedEffect":"not a physical FK"},
                {"Id":"APercentages","SkillTotemId":500,"ActiveSkillTypes":[1,0,1],"GrantedEffect":""},
                {"Id":"Unresolved","SkillTotemId":90,"ActiveSkillTypes":[0,1],"GrantedEffect":"ZPlayerLike"},
                {"Id":"NonSummoner","SkillTotemId":901,"ActiveSkillTypes":[],"GrantedEffect":""},
                {"Id":"SkinOnly","SkillTotemId":12,"ActiveSkillTypes":[1],"GrantedEffect":""},
                {"Id":"UnusedActive","SkillTotemId":700,"ActiveSkillTypes":[],"GrantedEffect":""}
            ]),
        ),
        (
            "ActiveSkillType",
            vec![("Id", "string", false, None)],
            json!([{"Id":"Spell"},{"Id":"SummonsTotem"}]),
        ),
        (
            "SkillTotemVariations",
            vec![
                ("SkillTotemsKey", "enumrow", false, Some("SkillTotems")),
                ("TotemSkinId", "i32", false, None),
                (
                    "MonsterVarietiesKey",
                    "foreignrow",
                    false,
                    Some("MonsterVarieties"),
                ),
            ],
            json!([
                {"SkillTotemsKey":500,"TotemSkinId":2,"MonsterVarietiesKey":1},
                {"SkillTotemsKey":0,"TotemSkinId":0,"MonsterVarietiesKey":0},
                {"SkillTotemsKey":500,"TotemSkinId":0,"MonsterVarietiesKey":2},
                {"SkillTotemsKey":12,"TotemSkinId":-1,"MonsterVarietiesKey":1},
                {"SkillTotemsKey":777,"TotemSkinId":0,"MonsterVarietiesKey":1}
            ]),
        ),
        (
            "MonsterVarieties",
            vec![
                ("Id", "string", false, None),
                ("MonsterType", "foreignrow", false, Some("MonsterTypes")),
                ("LifeMultiplier", "i32", false, None),
                ("_index", "i32", false, None),
            ],
            json!([
                {"Id":"ZZero","MonsterType":0,"LifeMultiplier":0,"_index":999},
                {"Id":"ASkinnedSigned","MonsterType":1,"LifeMultiplier":-3,"_index":998},
                {"Id":"SelectedPercentage","MonsterType":2,"LifeMultiplier":125,"_index":997}
            ]),
        ),
        (
            "MonsterTypes",
            vec![
                ("Id", "string", false, None),
                ("Armour", "i32", false, None),
                ("Evasion", "i32", false, None),
                ("EnergyShieldFromLife", "i32", false, None),
            ],
            json!([
                {"Id":"ZZeroType","Armour":0,"Evasion":0,"EnergyShieldFromLife":0},
                {"Id":"SignedType","Armour":-4,"Evasion":-5,"EnergyShieldFromLife":-6},
                {"Id":"ASelectedType","Armour":80,"Evasion":50,"EnergyShieldFromLife":7}
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
        domain: Domain::TotemVarieties,
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
fn original_dto_uses_opaque_keys_and_physical_links_with_all_aliases_and_raw_percentages() {
    let f = fixture();
    run(args(&f)).unwrap();
    let dto: TotemVarietiesDef =
        crate::read_json(&f.root.join("out/overlay/totem_varieties.json")).unwrap();
    let percentage = |effect: &str| TotemVarietyDef {
        granted_effect: effect.into(),
        active_skill: "APercentages".into(),
        skill_totem_id: 500,
        monster_variety: "SelectedPercentage".into(),
        life_multiplier: 125,
        armour: 80,
        evasion: 50,
        energy_shield_from_life: 7,
    };
    assert_eq!(
        dto.totems,
        vec![
            percentage("ARogueAlias"),
            TotemVarietyDef {
                granted_effect: "UntaggedNpc".into(),
                active_skill: "ZZeroSelector".into(),
                skill_totem_id: 0,
                monster_variety: "ZZero".into(),
                life_multiplier: 0,
                armour: 0,
                evasion: 0,
                energy_shield_from_life: 0,
            },
            percentage("ZPlayerLike")
        ]
    );
    // No SkillTotems payload or old overlay is present in the fixture.
    assert!(!f.root.join("input/tables/SkillTotems.json").exists());
    let c = &read(&f, "producer-receipt.json")["conversion"];
    let r = &c["source_records"][0];
    assert_eq!(
        r["source"]["effect"],
        json!({"source_row":1,"id":"ARogueAlias"})
    );
    assert_eq!(
        r["source"]["active_skill"],
        json!({"source_row":1,"id":"APercentages"})
    );
    assert_eq!(r["variation"]["source_row"], 2);
    assert_eq!(r["variation"]["selector"], 500);
    assert_eq!(
        r["variation"]["variety"],
        json!({"source_row":2,"id":"SelectedPercentage"})
    );
    assert_eq!(
        r["variation"]["monster_type"],
        json!({"source_row":2,"id":"ASelectedType"})
    );
    assert_eq!(
        r["source"]["active_skill_types"],
        json!([
            {"position":0,"target":{"source_row":1,"id":"SummonsTotem"}},
            {"position":1,"target":{"source_row":0,"id":"Spell"}},
            {"position":2,"target":{"source_row":1,"id":"SummonsTotem"}}
        ])
    );
    assert_eq!(c["selector_join"]["namespace"], "SkillTotems");
    assert_eq!(
        c["unconsumed_source"][1]["columns"][0]["physical_fields"],
        json!(["GrantedEffect"])
    );
    assert_eq!(
        c["unconsumed_source"][4]["columns"][0]["physical_fields"],
        json!(["_index"])
    );
}

#[test]
fn unresolved_tagged_sources_and_compact_unmatched_null_ranges_never_get_fallback_numbers() {
    let f = fixture();
    run(args(&f)).unwrap();
    let c = &read(&f, "producer-receipt.json")["conversion"];
    let unresolved = c["unresolved_summons_totem"].as_array().unwrap();
    assert_eq!(unresolved.len(), 2);
    assert_eq!(
        unresolved[0]["source"]["effect"],
        json!({"source_row":3,"id":"UnresolvedSummoner"})
    );
    assert_eq!(unresolved[0]["source"]["selector"], 90);
    assert_eq!(unresolved[1]["source"]["effect"]["id"], "NegativeSkinOnly");
    assert!(unresolved.iter().all(|r| r.get("variation").is_none()));
    assert_eq!(
        c["null_active_effects"],
        json!({"count":3,"ranges":[[5,6],[7,9]]})
    );
    assert_eq!(
        c["unmatched_selectors"],
        json!({
            "12":{"count":1,"ranges":[[6,7]]},"90":{"count":1,"ranges":[[3,4]]},
            "901":{"count":3,"ranges":[[4,5],[9,11]]}
        })
    );
    let variations = &c["variation_records"];
    assert_eq!(variations[0]["status"], "other-skin-unapplied");
    assert_eq!(variations[0]["source"]["life_multiplier"], -3);
    assert_eq!(variations[4]["status"], "unreferenced-skin-zero");
    assert_eq!(variations[4]["source"]["armour"], -4);
    assert_eq!(variations[3]["source"]["skin_id"], -1); // signed source, not an invented u32 skin rule
    assert_eq!(
        read(&f, "overlay/totem_varieties.json")["totems"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}

#[test]
fn only_unique_skin_zero_matches_apply_and_other_skins_are_preserved() {
    for key in [500, 777] {
        let mut f = fixture();
        f.values("SkillTotemVariations", 0)["SkillTotemsKey"] = json!(key);
        f.values("SkillTotemVariations", 0)["TotemSkinId"] = json!(0);
        f.seal();
        rejects(&f, "ambiguous skin-zero"); // even a key unused by all granted effects
    }
    let mut f = fixture();
    f.values("SkillTotemVariations", 2)["TotemSkinId"] = json!(1);
    f.seal();
    run(args(&f)).unwrap();
    assert_eq!(
        read(&f, "overlay/totem_varieties.json")["totems"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        read(&f, "producer-receipt.json")["conversion"]["unresolved_summons_totem"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    // Duplicate nonzero skins remain independent physical source records; no new key law is imposed.
    let mut f = fixture();
    f.values("SkillTotemVariations", 3)["SkillTotemsKey"] = json!(500);
    f.values("SkillTotemVariations", 3)["TotemSkinId"] = json!(2);
    f.seal();
    run(args(&f)).unwrap();
    assert_eq!(
        read(&f, "producer-receipt.json")["conversion"]["variation_records"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
}

#[test]
fn selected_numbers_require_u32_and_all_signed_source_values_keep_their_schema() {
    for (table, field, output) in [
        ("MonsterVarieties", "LifeMultiplier", "life_multiplier"),
        ("MonsterTypes", "Armour", "armour"),
        ("MonsterTypes", "Evasion", "evasion"),
        (
            "MonsterTypes",
            "EnergyShieldFromLife",
            "energy_shield_from_life",
        ),
    ] {
        let mut f = fixture();
        f.values(table, 2)[field] = json!(-1);
        f.seal();
        rejects(&f, &format!("selected {field} is outside u32 range"));
        for value in [
            json!(null),
            json!("100"),
            json!(100.0),
            json!(2147483648_i64),
            json!(-2147483649_i64),
            json!(4294967296_u64),
        ] {
            let mut f = fixture();
            f.values(table, 1)[field] = value;
            f.seal();
            rejects(&f, "signed i32 required"); // unused/skinned rows still validate physical representation
        }
        let mut f = fixture();
        f.values(table, 2)[field] = json!(i32::MAX);
        f.seal();
        run(args(&f)).unwrap();
        assert_eq!(
            read(&f, "overlay/totem_varieties.json")["totems"][0][output],
            i32::MAX
        );
    }
    // A previously unused signed object cannot turn into an unsigned DTO via clamping.
    let mut f = fixture();
    f.values("ActiveSkills", 3)["SkillTotemId"] = json!(777);
    f.seal();
    rejects(&f, "selected LifeMultiplier");
}

#[test]
fn all_consumed_references_metadata_and_identity_keys_validate_before_output() {
    for (table, row, field) in [
        ("GrantedEffects", 0, "ActiveSkill"),
        ("ActiveSkills", 5, "SkillTotemId"),
        ("SkillTotemVariations", 0, "SkillTotemsKey"),
        ("SkillTotemVariations", 0, "MonsterVarietiesKey"),
        ("MonsterVarieties", 1, "MonsterType"),
    ] {
        for value in [json!(-1), json!(1.0), json!("0"), json!(4294967296_u64)] {
            let mut f = fixture();
            f.values(table, row)[field] = value;
            f.seal();
            assert!(run(args(&f)).is_err(), "{table}.{field}");
            assert!(!f.root.join("out").exists());
        }
    }
    for (table, row, field, value) in [
        ("ActiveSkills", 5, "ActiveSkillTypes", json!([999])),
        ("ActiveSkills", 5, "ActiveSkillTypes", json!([null])),
        ("ActiveSkills", 5, "ActiveSkillTypes", json!(null)),
        ("ActiveSkills", 5, "ActiveSkillTypes", json!([-1])),
        ("ActiveSkills", 5, "SkillTotemId", json!(null)),
        ("SkillTotemVariations", 0, "SkillTotemsKey", json!(null)),
        (
            "SkillTotemVariations",
            0,
            "MonsterVarietiesKey",
            json!(null),
        ),
        ("MonsterVarieties", 1, "MonsterType", json!(null)),
        ("SkillTotemVariations", 0, "TotemSkinId", json!(null)),
        ("SkillTotemVariations", 0, "TotemSkinId", json!(0.5)),
        (
            "SkillTotemVariations",
            0,
            "TotemSkinId",
            json!(2147483648_i64),
        ),
    ] {
        let mut f = fixture();
        f.values(table, row)[field] = value;
        f.seal();
        assert!(run(args(&f)).is_err(), "{table}.{field}");
        assert!(!f.root.join("out").exists());
    }
    for table in [
        "GrantedEffects",
        "ActiveSkills",
        "ActiveSkillType",
        "MonsterVarieties",
        "MonsterTypes",
    ] {
        let mut f = fixture();
        let id = f.values(table, 0)["Id"].clone();
        f.values(table, 1)["Id"] = id;
        f.seal();
        rejects(&f, "duplicate");
    }
    for case in [
        "empty-id",
        "physical-key",
        "missing-field",
        "missing-column",
        "enum-type",
        "namespace",
        "foreign-target",
        "array",
        "numeric-type",
    ] {
        let mut f = fixture();
        match case {
            "empty-id" => f.values("GrantedEffects", 0)["Id"] = json!(""),
            "physical-key" => {
                f.documents
                    .get_mut("tables/SkillTotemVariations.json")
                    .unwrap()["rows"][1]["_row"] = json!(0)
            }
            "missing-field" => {
                f.values("ActiveSkills", 5)
                    .as_object_mut()
                    .unwrap()
                    .remove("SkillTotemId");
            }
            "missing-column" => {
                f.documents
                    .get_mut("tables/SkillTotemVariations.json")
                    .unwrap()["columns"]
                    .as_array_mut()
                    .unwrap()
                    .remove(0);
            }
            "enum-type" => {
                f.documents.get_mut("tables/ActiveSkills.json").unwrap()["columns"][1]["schema"]["type"] =
                    json!("foreignrow")
            }
            "namespace" => {
                f.documents
                    .get_mut("tables/SkillTotemVariations.json")
                    .unwrap()["columns"][0]["schema"]["references"]["table"] = json!("ActiveSkills")
            }
            "foreign-target" => {
                f.documents.get_mut("tables/GrantedEffects.json").unwrap()["columns"][1]["schema"]
                    ["references"]["table"] = json!("SkillTotemVariations")
            }
            "array" => {
                f.documents.get_mut("tables/ActiveSkills.json").unwrap()["columns"][2]["schema"]["array"] =
                    json!(false)
            }
            "numeric-type" => {
                f.documents.get_mut("tables/MonsterVarieties.json").unwrap()["columns"][2]["schema"]
                    ["type"] = json!("u32")
            }
            _ => unreachable!(),
        }
        f.seal();
        assert!(run(args(&f)).is_err(), "{case}");
        assert!(!f.root.join("out").exists());
    }
}

#[test]
fn target_overlay_replays_with_only_six_pinned_inputs_and_incomplete_receipt() {
    let f = fixture();
    run(args(&f)).unwrap();
    run(Args {
        out: f.root.join("again"),
        ..args(&f)
    })
    .unwrap();
    let r = read(&f, "producer-receipt.json");
    assert_eq!(r["complete_runtime"], false);
    assert_eq!(r["adoption"], "not-adopted");
    assert_eq!(r["files"].as_object().unwrap().len(), 4);
    assert_eq!(
        r["input"]["selected_payloads"].as_object().unwrap().len(),
        7
    );
    assert_eq!(
        read(&f, "_domain_candidate.json")["domain"],
        "ordinary-totem-varieties"
    );
    let meta = &read(&f, "overlay/totem_varieties.json")["_meta"];
    assert_eq!(meta["poe_version"], "0.5.5.5");
    assert_eq!(
        meta["facts_manifest_sha256"],
        args(&f).expected_manifest_sha256
    );
    assert_eq!(meta["inputs"].as_object().unwrap().len(), 6);
    assert!(!f.root.join("out/base").exists());
    assert!(!f.root.join("out/manifest.json").exists());
    assert!(!f.root.join("out/quality-policy.json").exists());
    assert!(!f.root.join("out/codec-policy.json").exists());
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
    assert_eq!(
        fs::read(f.root.join("input/manifest.json")).unwrap(),
        fs::read(f.root.join("out/source-manifest.json")).unwrap()
    );
}

#[test]
fn pins_patch_cli_and_protected_output_reject_before_any_candidate() {
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
        "totem-varieties",
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
        ["--ordinary-domain", "totem-varieties"],
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
            _ => fs::write(f.root.join("input/tables/SkillTotemVariations.json"), b"{}").unwrap(),
        }
        assert!(run(a).is_err());
        assert!(!f.root.join("out").exists());
    }
    let mut f = fixture();
    let path = "tables/SkillTotemVariations.json";
    let bytes = br#"{"table":"SkillTotemVariations","table":"duplicate"}"#;
    fs::write(f.root.join("input").join(path), bytes).unwrap();
    f.manifest["files"][path] = json!({"bytes":bytes.len(),"sha256":sha256_hex(bytes)});
    fs::write(
        f.root.join("input/manifest.json"),
        super::super::reader::json_bytes(&f.manifest).unwrap(),
    )
    .unwrap();
    rejects(&f, "duplicate");
    let f = fixture();
    for out in [
        f.root.join("input/new"),
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/overlay-common")
            .canonicalize()
            .unwrap()
            .join(format!(
                "totem-forbidden-{}",
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
fn previous_base_and_codec_domains_keep_bytes_and_receipt_shapes() {
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
        (Domain::MonsterResistances, None, 4),
    ] {
        let mut f = match domain {
            Domain::StatSetCodec => super::super::stat_set_codec::tests::fixture(),
            Domain::GemEffects => super::super::gem_effects::tests::fixture(),
            Domain::MonsterResistances => super::super::monster_resistances::tests::fixture(),
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
            Domain::MonsterResistances => super::super::monster_resistances::tests::args(&f),
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
        assert!(!f.root.join("out/overlay/totem_varieties.json").exists());
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
