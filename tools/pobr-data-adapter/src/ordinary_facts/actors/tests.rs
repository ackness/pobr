use std::collections::BTreeMap;
use std::fs;

use pobr_data::catalog::actors::MinionsDef;
use serde_json::{Value, json};

use super::super::stat_set_codec::tests::table;
use super::super::tests::Fixture;
use super::super::{Args, CodecArgs, Domain, parse_args, reader, run};
use super::{Catalog, projection::OWNED_FIELDS};
use crate::raw_source::sha256_hex;

const SOURCE: &str = "legacy/source.json";
const POLICY: &str = "policy/policy.json";

pub(in crate::ordinary_facts) fn fixture(catalog: Catalog) -> Fixture {
    let mut f = Fixture::new();
    let old: Vec<_> = f
        .documents
        .keys()
        .filter(|p| p.starts_with("tables/"))
        .cloned()
        .collect();
    f.documents.retain(|p, _| p == "source-lock.json");
    let mut varieties = table(
        "MonsterVarieties",
        &[
            ("Id", "string", false, None),
            ("Name", "string", false, None),
            ("MonsterType", "foreignrow", false, Some("MonsterTypes")),
            ("Tags", "foreignrow", true, Some("Tags")),
            ("LifeMultiplier", "i32", false, None),
            ("DamageMultiplier", "i32", false, None),
            ("AttackSpeed", "i32", false, None),
            ("MaximumAttackDistance", "i32", false, None),
            ("MovementSpeed", "i32", false, None),
            ("AttackCrit", "f32", false, None),
            ("ExperienceMultiplier", "i32", false, None),
            ("Accuracy", "i32", false, None),
        ],
        json!([
            {"Id":"Unselected","Name":"Unused creature","MonsterType":0,"Tags":[],"LifeMultiplier":100,"DamageMultiplier":100,"AttackSpeed":1000,"MaximumAttackDistance":1,"MovementSpeed":40,"AttackCrit":0.05,"ExperienceMultiplier":100,"Accuracy":999},
            {"Id":"VarZ","Name":"Target Z","MonsterType":1,"Tags":[1,0,1],"LifeMultiplier":-75,"DamageMultiplier":0,"AttackSpeed":-1250,"MaximumAttackDistance":-9,"MovementSpeed":-30,"AttackCrit":-0.00005,"ExperienceMultiplier":110,"Accuracy":999},
            {"Id":"VarA","Name":"","MonsterType":0,"Tags":[],"LifeMultiplier":100,"DamageMultiplier":250,"AttackSpeed":2500,"MaximumAttackDistance":0,"MovementSpeed":25,"AttackCrit":0.05000000074505806,"ExperienceMultiplier":0,"Accuracy":999}
        ]),
    );
    for c in varieties["columns"].as_array_mut().unwrap() {
        let description = match c["schema"]["name"].as_str().unwrap() {
            "AttackSpeed" => Some("in ms"),
            "LifeMultiplier" | "DamageMultiplier" | "ExperienceMultiplier" => Some("in percent"),
            _ => None,
        };
        if let Some(description) = description {
            c["schema"]["description"] = json!(description);
        }
    }
    let mut resistance_fields = vec![("Id", "string", false, None)];
    for field in [
        "Fire1",
        "Cold1",
        "Lightning1",
        "Chaos1",
        "Fire3",
        "Cold3",
        "Lightning3",
        "Chaos3",
    ] {
        resistance_fields.push((field, "i32", true, None));
    }
    for (name, doc) in [
        ("MonsterVarieties", varieties),
        (
            "MonsterTypes",
            table(
                "MonsterTypes",
                &[
                    ("Id", "string", false, None),
                    ("DamageSpread", "i32", false, None),
                    ("Armour", "i32", false, None),
                    ("Evasion", "i32", false, None),
                    ("EnergyShieldFromLife", "i32", false, None),
                    (
                        "MonsterResistances",
                        "foreignrow",
                        true,
                        Some("MonsterResistances"),
                    ),
                    ("Flag0", "bool", false, None),
                ],
                json!([
                    {"Id":"ZeroType","DamageSpread":0,"Armour":0,"Evasion":0,"EnergyShieldFromLife":0,"MonsterResistances":[0],"Flag0":true},
                    {"Id":"SignedType","DamageSpread":-25,"Armour":-50,"Evasion":20,"EnergyShieldFromLife":0,"MonsterResistances":[1,0,1],"Flag0":false}
                ]),
            ),
        ),
        (
            "MonsterResistances",
            table(
                "MonsterResistances",
                &resistance_fields,
                json!([
                    {"Id":"Zero","Fire1":[0],"Cold1":[0],"Lightning1":[0],"Chaos1":[0],"Fire3":[0],"Cold3":[0],"Lightning3":[0],"Chaos3":[0]},
                    {"Id":"First","Fire1":[-11,101],"Cold1":[0,102],"Lightning1":[23,103],"Chaos1":[-4,104],"Fire3":[-31,201],"Cold3":[42,202],"Lightning3":[0,203],"Chaos3":[54,204]}
                ]),
            ),
        ),
        (
            "Tags",
            table(
                "Tags",
                &[("Id", "string", false, None)],
                json!([{"Id":"TagZ"},{"Id":"TagA"}]),
            ),
        ),
        (
            "GrantedEffects",
            table(
                "GrantedEffects",
                &[("Id", "string", false, None)],
                json!([{"Id":"SkillZ"},{"Id":"SkillA"},{"Id":"UnusedSkill"}]),
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
    for dir in ["legacy", "policy"] {
        fs::create_dir(f.root.join(dir)).unwrap();
    }
    let policy: Value = serde_json::from_slice(match catalog {
        Catalog::Minions => {
            include_bytes!("../../../../../pipeline/actors/0.5.5.5-minions-policy.json").as_slice()
        }
        Catalog::Spectres => {
            include_bytes!("../../../../../pipeline/actors/0.5.5.5-spectres-policy.json").as_slice()
        }
    })
    .unwrap();
    let mut first = json!({"id":if catalog==Catalog::Minions{"AliasZ"}else{"VarZ"},"name":"Old name","monster_tags":["old"],
        "life":999.0,"damage":999.0,"damage_spread":999.0,"attack_time":999.0,"attack_range":999.0,"accuracy":2.5,"crit_chance":999.0,"base_movement_speed":999.0});
    for fragment in [
        json!({"armour":9.0,"evasion":9.0,"energy_shield":9.0,"fire_resist":999.0,"cold_resist":999.0,"lightning_resist":999.0,"chaos_resist":999.0,
        "spectre_reservation":999.0,"companion_reservation":31.2,"companion_fire_resist":999.0,"companion_cold_resist":999.0,"companion_lightning_resist":999.0,"companion_chaos_resist":999.0,
        }),
        json!({"base_damage_ignores_attack_speed":true,"monster_category":"CommunityCategory","limit":"UninterpretedLimit","weapon_type1":"FutureWeapon","weapon_type2":"OtherWeapon",
        "skill_list":["SkillA","SkillZ","SkillA"],"extra_flags":["unknown_flag"],"spawn_location":["archived location"],
        "mod_list":[{"name":"UnknownMod","type":"FUTURE","value":-3.0,"flags":"UnsupportedFlag","keyword_flags":0.0,"tags":[{"type":"FutureTag","limit":false}],"future_constructor":{"nested":[null,4]}}],
        "future/~field":{"nested":[null,0,"unknown"]}}),
    ] {
        first
            .as_object_mut()
            .unwrap()
            .extend(fragment.as_object().unwrap().clone());
    }
    if catalog == Catalog::Minions {
        first["monster_variety_id"] = json!("VarZ");
    }
    let mut last = first.clone();
    last["id"] = json!(if catalog == Catalog::Minions {
        "Solo"
    } else {
        "VarA"
    });
    if catalog == Catalog::Minions {
        last["monster_variety_id"] = json!("VarA");
    }
    for field in [
        "skill_list",
        "mod_list",
        "limit",
        "base_damage_ignores_attack_speed",
        "spawn_location",
        "extra_flags",
    ] {
        last.as_object_mut().unwrap().remove(field);
    }
    let mut entries = vec![first.clone()];
    if catalog == Catalog::Minions {
        first["id"] = json!("AliasA");
        first["base_damage_ignores_attack_speed"] = json!(false);
        entries.push(first);
    }
    entries.push(last);
    write(
        &f,
        SOURCE,
        &json!({"_meta":policy["source"]["metadata"],"future_catalog":{"preserved":null},"minions":entries}),
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
    write(f, POLICY, &p);
}
pub(in crate::ordinary_facts) fn args(f: &Fixture, catalog: Catalog) -> Args {
    Args {
        domain: match catalog {
            Catalog::Minions => Domain::Minions,
            Catalog::Spectres => Domain::Spectres,
        },
        codec: Some(CodecArgs {
            source: f.root.join(SOURCE),
            source_sha256: sha256_hex(&fs::read(f.root.join(SOURCE)).unwrap()),
            policy: f.root.join(POLICY),
            policy_sha256: sha256_hex(&fs::read(f.root.join(POLICY)).unwrap()),
        }),
        ..f.args()
    }
}
fn read(f: &Fixture, path: &str) -> Value {
    doc(f, &format!("out/{path}"))
}
fn overlay(f: &Fixture, catalog: Catalog) -> Value {
    read(f, &format!("overlay/{}", catalog.file()))
}
fn rejects(f: &Fixture, catalog: Catalog, expected: &str) {
    let e = run(args(f, catalog)).unwrap_err();
    assert!(e.contains(expected), "expected {expected:?}, got {e:?}");
    assert!(!f.root.join("out").exists());
}

#[test]
fn both_catalogs_project_original_dto_fields_and_preserve_authored_membership() {
    for catalog in [Catalog::Minions, Catalog::Spectres] {
        let f = fixture(catalog);
        run(args(&f, catalog)).unwrap();
        let out = overlay(&f, catalog);
        let source = doc(&f, SOURCE);
        let dto: MinionsDef = serde_json::from_value(out.clone()).unwrap();
        assert_eq!(
            dto.minions.len(),
            if catalog == Catalog::Minions { 3 } else { 2 }
        );
        let first = &dto.minions[0];
        assert_eq!(first.name, "Target Z");
        assert_eq!(first.monster_tags, ["TagA", "TagZ", "TagA"]);
        assert_eq!(
            (first.life, first.damage, first.damage_spread),
            (-0.75, 0.0, -0.25)
        );
        assert_eq!(
            (
                first.attack_time,
                first.attack_range,
                first.base_movement_speed
            ),
            (-1.25, -9.0, -30.0)
        );
        assert_eq!(first.crit_chance, 0.0);
        assert_eq!(first.armour, Some(-0.5));
        assert_eq!(first.evasion, Some(0.2));
        assert_eq!(first.energy_shield, None);
        assert_eq!(
            (
                first.fire_resist,
                first.cold_resist,
                first.lightning_resist,
                first.chaos_resist
            ),
            (-11.0, 0.0, 23.0, -4.0)
        );
        assert_eq!(
            (
                first.companion_fire_resist,
                first.companion_cold_resist,
                first.companion_lightning_resist,
                first.companion_chaos_resist
            ),
            (Some(-31.0), Some(42.0), Some(0.0), Some(54.0))
        );
        assert_eq!(first.spectre_reservation, 53.0);
        assert_eq!(first.companion_reservation, 1.04 * 30.0);
        assert_ne!(first.companion_reservation, 31.2);
        assert_eq!(first.accuracy, 2.5);
        assert!(first.base_damage_ignores_attack_speed);
        let last = out["minions"].as_array().unwrap().last().unwrap();
        assert_eq!(last["name"], "");
        assert_eq!(last["monster_tags"], json!([]));
        assert_eq!(last["crit_chance"], 5.0);
        assert_eq!(last["spectre_reservation"], 0.0);
        assert_eq!(last["companion_reservation"], 0.0);
        for field in [
            "armour",
            "evasion",
            "energy_shield",
            "skill_list",
            "mod_list",
            "limit",
            "base_damage_ignores_attack_speed",
            "spawn_location",
            "extra_flags",
        ] {
            assert!(last.get(field).is_none(), "{field}");
        }
        assert_eq!(out["future_catalog"], source["future_catalog"]);
        for (old, new) in source["minions"]
            .as_array()
            .unwrap()
            .iter()
            .zip(out["minions"].as_array().unwrap())
        {
            for (field, value) in old.as_object().unwrap() {
                if !OWNED_FIELDS.contains(&field.as_str()) {
                    assert_eq!(new[field], *value, "{catalog:?}/{field}");
                }
            }
        }
        if catalog == Catalog::Minions {
            assert_eq!(dto.minions[0].id, "AliasZ");
            assert_eq!(dto.minions[1].id, "AliasA");
            assert!(!dto.minions[1].base_damage_ignores_attack_speed);
        }
        assert_eq!(
            out["_meta"]["target_fields"]["source_identity"]["game_patch"],
            "0.5.5.5"
        );
        assert_eq!(
            out["_meta"]["legacy_compatibility"]["source"]["metadata"],
            source["_meta"]
        );
    }
}

#[test]
fn receipt_binds_aliases_every_owned_omission_and_ordered_unapplied_references() {
    let f = fixture(Catalog::Minions);
    run(args(&f, Catalog::Minions)).unwrap();
    let r = read(&f, "producer-receipt.json");
    let c = &r["conversion"];
    let first = &c["source_records"][0];
    assert_eq!(c["projected_field_occurrences"], 61);
    assert_eq!(c["retained_skill_occurrences"], 6);
    assert_eq!(
        c["variety_aliases"]["VarZ"],
        json!([{"id":"AliasZ","source_row":0},{"id":"AliasA","source_row":1}])
    );
    assert_eq!(first["projection"]["variety"]["source_row"], 1);
    assert_eq!(first["projection"]["monster_type"]["source_row"], 1);
    assert_eq!(
        first["projection"]["tag_occurrences"],
        json!([{"position":0,"source_row":1,"id":"TagA"},{"position":1,"source_row":0,"id":"TagZ"},{"position":2,"source_row":1,"id":"TagA"}])
    );
    assert_eq!(
        first["skill_occurrences"],
        json!([{"position":0,"source_row":1,"id":"SkillA","table":"GrantedEffects"},{"position":1,"source_row":0,"id":"SkillZ","table":"GrantedEffects"},{"position":2,"source_row":1,"id":"SkillA","table":"GrantedEffects"}])
    );
    let refs = &first["projection"]["resistance_occurrences"];
    assert_eq!(refs.as_array().unwrap().len(), 3);
    assert_eq!(refs[0]["source_row"], 1);
    assert_eq!(refs[2]["source_row"], 1);
    assert_eq!(refs[2]["selected"], false);
    assert_eq!(refs[0]["arrays"]["Fire1"]["raw"], json!([-11, 101]));
    assert_eq!(refs[0]["arrays"]["Fire1"]["selected_element_index"], 0);
    assert_eq!(
        refs[0]["arrays"]["Fire1"]["unapplied_element_range"],
        json!({"start":1,"end_exclusive":2})
    );
    assert_eq!(
        refs[2]["arrays"]["Fire1"]["unapplied_element_range"],
        json!({"start":0,"end_exclusive":2})
    );
    assert_eq!(first["owned_fields"].as_array().unwrap().len(), 22);
    let omitted = first["owned_fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["field"] == "energy_shield")
        .unwrap();
    assert_eq!(omitted["present"], false);
    assert_eq!(omitted["output"], Value::Null);
    let source = doc(&f, SOURCE);
    for record in c["source_records"].as_array().unwrap() {
        for retained in record["retained_fields"].as_array().unwrap() {
            let original = source
                .pointer(retained["source_pointer"].as_str().unwrap())
                .unwrap();
            assert_eq!(
                retained["value_sha256"],
                sha256_hex(&reader::json_bytes(original).unwrap())
            );
        }
    }
    assert!(
        c["legacy_f64_differences"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["field"] == "companion_reservation")
    );
    assert_eq!(c["unconsumed_source"].as_array().unwrap().len(), 5);
}

#[test]
fn changed_target_fields_and_tag_order_replace_old_values_without_admitting_new_actors() {
    for catalog in [Catalog::Minions, Catalog::Spectres] {
        let mut f = fixture(catalog);
        f.values("MonsterVarieties", 1)["LifeMultiplier"] = json!(i32::MIN);
        f.values("MonsterVarieties", 1)["AttackCrit"] = json!(0.0500000001);
        f.values("MonsterVarieties", 1)["Tags"] = json!([0, 1]);
        f.values("Tags", 0)["Id"] = json!("ChangedTag");
        f.values("MonsterTypes", 1)["Armour"] = json!(0);
        f.values("MonsterTypes", 1)["EnergyShieldFromLife"] = json!(-100);
        f.values("MonsterResistances", 1)["Fire1"] = json!([77, -900]);
        reseal(&mut f);
        run(args(&f, catalog)).unwrap();
        let out = overlay(&f, catalog);
        assert_eq!(out["minions"][0]["life"], f64::from(i32::MIN) / 100.0);
        assert_eq!(
            out["minions"][0]["monster_tags"],
            json!(["ChangedTag", "TagA"])
        );
        assert_eq!(out["minions"][0]["crit_chance"], 5.0);
        assert!(out["minions"][0].get("armour").is_none());
        assert_eq!(out["minions"][0]["energy_shield"], -1.0);
        assert_eq!(out["minions"][0]["fire_resist"], 77.0);
        assert_eq!(
            out["minions"].as_array().unwrap().len(),
            if catalog == Catalog::Minions { 3 } else { 2 }
        );
        assert!(
            out["minions"]
                .as_array()
                .unwrap()
                .iter()
                .all(|r| r["id"] != "Unselected")
        );
    }
}

#[test]
fn negative_halfway_crit_and_reservation_order_remain_archived_semantics() {
    for (raw, expected) in [
        (-0.00005, 0.0),
        (-0.03125, -3.12),
        (0.00005, 0.01),
        (0.03125, 3.13),
        (-0.02, -2.0),
        (2.0, 200.0),
    ] {
        let mut f = fixture(Catalog::Minions);
        f.values("MonsterVarieties", 1)["AttackCrit"] = json!(raw);
        reseal(&mut f);
        run(args(&f, Catalog::Minions)).unwrap();
        assert_eq!(
            overlay(&f, Catalog::Minions)["minions"][0]["crit_chance"],
            expected,
            "raw {raw}"
        );
    }
    let mut f = fixture(Catalog::Spectres);
    f.values("MonsterVarieties", 1)["ExperienceMultiplier"] = json!(25);
    reseal(&mut f);
    run(args(&f, Catalog::Spectres)).unwrap();
    let out = overlay(&f, Catalog::Spectres);
    assert_eq!(out["minions"][0]["spectre_reservation"], 17.0);
    assert_eq!(out["minions"][0]["companion_reservation"], 15.0);
}

#[test]
fn catalog_first_reference_projection_does_not_change_normal_resistance_sum() {
    let mut f = fixture(Catalog::Minions);
    for field in ["Fire1", "Cold1", "Lightning1", "Chaos1"] {
        let first = f.values("MonsterResistances", 1)[field][0].clone();
        f.values("MonsterResistances", 1)[field] = json!([first]);
    }
    reseal(&mut f);
    run(args(&f, Catalog::Minions)).unwrap();
    run(Args {
        domain: Domain::MonsterResistances,
        out: f.root.join("normal"),
        ..f.args()
    })
    .unwrap();
    assert_eq!(
        overlay(&f, Catalog::Minions)["minions"][0]["fire_resist"],
        -11.0
    );
    let normal = doc(&f, "normal/base/monster_resistances.json");
    assert_eq!(normal["monsters"]["VarZ"]["fire"], -22);
    assert_eq!(normal["monsters"]["VarZ"]["lightning"], 46);
}

#[test]
fn missing_invalid_or_out_of_range_facts_never_use_legacy_values() {
    for (table, row, field, value) in [
        ("MonsterVarieties", 1, "Name", Value::Null),
        ("MonsterVarieties", 1, "LifeMultiplier", Value::Null),
        (
            "MonsterVarieties",
            1,
            "LifeMultiplier",
            json!(2147483648_i64),
        ),
        (
            "MonsterVarieties",
            1,
            "DamageMultiplier",
            json!(-2147483649_i64),
        ),
        ("MonsterVarieties", 1, "AttackSpeed", json!(1.0)),
        ("MonsterVarieties", 1, "MaximumAttackDistance", json!("8")),
        ("MonsterVarieties", 1, "MovementSpeed", json!(false)),
        ("MonsterVarieties", 1, "AttackCrit", json!("0.05")),
        (
            "MonsterVarieties",
            1,
            "AttackCrit",
            json!(f64::from(f32::MAX) * 2.0),
        ),
        ("MonsterVarieties", 1, "ExperienceMultiplier", json!(-1)),
        ("MonsterVarieties", 1, "MonsterType", json!(99)),
        ("MonsterVarieties", 1, "MonsterType", json!(-1)),
        ("MonsterVarieties", 1, "Tags", Value::Null),
        ("MonsterVarieties", 1, "Tags", json!([1, 99])),
        ("MonsterTypes", 1, "Armour", json!(2147483648_i64)),
        ("MonsterTypes", 1, "MonsterResistances", json!([])),
        ("MonsterTypes", 1, "MonsterResistances", json!([1, null])),
        ("MonsterTypes", 1, "MonsterResistances", json!([1, 99])),
        ("MonsterResistances", 1, "Fire1", json!([])),
        ("MonsterResistances", 1, "Fire3", json!([0, "bad"])),
        ("MonsterResistances", 0, "Chaos3", json!([2147483648_i64])), // unapplied later reference is still checked
        ("MonsterVarieties", 1, "Id", json!("MissingVarZ")),
        ("Tags", 1, "Id", json!("TagZ")),
        ("GrantedEffects", 1, "Id", json!("RenamedSkill")),
    ] {
        let mut f = fixture(Catalog::Minions);
        f.values(table, row)[field] = value;
        reseal(&mut f);
        rejects(&f, Catalog::Minions, "");
    }
    let mut f = fixture(Catalog::Spectres);
    f.values("MonsterVarieties", 1)["AttackCrit"] = json!(-f64::from(f32::MAX));
    f.values("MonsterVarieties", 1)["ExperienceMultiplier"] = json!(i32::MAX);
    reseal(&mut f);
    run(args(&f, Catalog::Spectres)).unwrap();
    let out = overlay(&f, Catalog::Spectres);
    assert!(
        out["minions"][0]["crit_chance"]
            .as_f64()
            .unwrap()
            .is_finite()
    );
    assert!(
        out["minions"][0]["companion_reservation"]
            .as_f64()
            .unwrap()
            .is_finite()
    );
}

#[test]
fn source_ids_keep_whitespace_when_unused_or_referenced_exactly() {
    for catalog in [Catalog::Minions, Catalog::Spectres] {
        let mut f = fixture(catalog);
        f.values("MonsterVarieties", 0)["Id"] = json!("Unused variety ");
        reseal(&mut f);
        run(args(&f, catalog)).unwrap();
        assert_eq!(
            overlay(&f, catalog)["minions"].as_array().unwrap().len(),
            doc(&f, SOURCE)["minions"].as_array().unwrap().len()
        );

        let mut f = fixture(catalog);
        f.values("MonsterVarieties", 1)["Id"] = json!("VarZ ");
        f.values("Tags", 1)["Id"] = json!("TagA ");
        f.values("GrantedEffects", 1)["Id"] = json!("SkillA ");
        let mut source = doc(&f, SOURCE);
        for entry in source["minions"].as_array_mut().unwrap() {
            if entry[catalog.identity_field()] == "VarZ" {
                entry[catalog.identity_field()] = json!("VarZ ");
                entry["skill_list"] = json!(["SkillA ", "SkillZ", "SkillA "]);
            }
        }
        write(&f, SOURCE, &source);
        reseal(&mut f);
        run(args(&f, catalog)).unwrap();
        let out = overlay(&f, catalog);
        assert_eq!(out["minions"][0][catalog.identity_field()], "VarZ ");
        assert_eq!(
            out["minions"][0]["monster_tags"],
            json!(["TagA ", "TagZ", "TagA "])
        );
        assert_eq!(
            out["minions"][0]["skill_list"],
            json!(["SkillA ", "SkillZ", "SkillA "])
        );
        let receipt = read(&f, "producer-receipt.json");
        let record = &receipt["conversion"]["source_records"][0];
        assert_eq!(record["projection"]["variety"]["id"], "VarZ ");
        assert_eq!(record["projection"]["variety"]["source_row"], 1);
        assert_eq!(record["skill_occurrences"][0]["id"], "SkillA ");

        source["minions"][0][catalog.identity_field()] = json!("VarZ");
        write(&f, SOURCE, &source);
        reseal(&mut f);
        let a = Args {
            out: f.root.join("mismatch"),
            ..args(&f, catalog)
        };
        assert!(run(a).unwrap_err().contains("missing target variety"));
        assert!(!f.root.join("mismatch").exists());
    }
}

#[test]
fn consumed_metadata_and_physical_and_catalog_keys_are_strict() {
    for mode in [
        "missing",
        "physical-key",
        "schema-type",
        "schema-array",
        "schema-fk",
        "schema-unit",
        "duplicate-id",
        "empty-id",
        "missing-variety",
        "bad-skill",
        "null-skill-list",
    ] {
        let mut f = fixture(Catalog::Minions);
        let mut source = doc(&f, SOURCE);
        match mode {
            "missing" => {
                f.values("MonsterVarieties", 1)
                    .as_object_mut()
                    .unwrap()
                    .remove("LifeMultiplier");
            }
            "physical-key" => {
                f.documents.get_mut("tables/MonsterVarieties.json").unwrap()["rows"][1]["_row"] =
                    json!(0)
            }
            "schema-type" => {
                f.documents.get_mut("tables/MonsterVarieties.json").unwrap()["columns"][9]["schema"]
                    ["type"] = json!("i32")
            }
            "schema-array" => {
                f.documents.get_mut("tables/MonsterVarieties.json").unwrap()["columns"][3]["schema"]
                    ["array"] = json!(false)
            }
            "schema-fk" => {
                f.documents.get_mut("tables/MonsterTypes.json").unwrap()["columns"][5]["schema"]["references"]
                    ["table"] = json!("Tags")
            }
            "schema-unit" => {
                f.documents.get_mut("tables/MonsterVarieties.json").unwrap()["columns"][6]["schema"]
                    ["description"] = json!("in seconds")
            }
            "duplicate-id" => source["minions"][1]["id"] = source["minions"][0]["id"].clone(),
            "empty-id" => source["minions"][0]["id"] = json!(""),
            "missing-variety" => {
                source["minions"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("monster_variety_id");
            }
            "bad-skill" => source["minions"][0]["skill_list"] = json!(["SkillA", "UnknownSkill"]),
            _ => source["minions"][0]["skill_list"] = Value::Null,
        }
        write(&f, SOURCE, &source);
        reseal(&mut f);
        rejects(&f, Catalog::Minions, "");
    }
    for value in [json!("WrongVariety"), Value::Null] {
        let mut f = fixture(Catalog::Spectres);
        let mut source = doc(&f, SOURCE);
        source["minions"][0]["monster_variety_id"] = value;
        write(&f, SOURCE, &source);
        reseal(&mut f);
        rejects(&f, Catalog::Spectres, "differs");
    }
    let mut f = fixture(Catalog::Spectres);
    let mut source = doc(&f, SOURCE);
    source["minions"][0]["monster_variety_id"] = json!("VarZ");
    write(&f, SOURCE, &source);
    reseal(&mut f);
    run(args(&f, Catalog::Spectres)).unwrap();
    assert_eq!(
        overlay(&f, Catalog::Spectres)["minions"][0]["monster_variety_id"],
        "VarZ"
    );
}

#[test]
fn policy_binds_domain_recipe_owned_fields_and_target_without_whitelisting_unknown_fields() {
    for catalog in [Catalog::Minions, Catalog::Spectres] {
        let f = fixture(catalog);
        let other = if catalog == Catalog::Minions {
            Catalog::Spectres
        } else {
            Catalog::Minions
        };
        rejects(&f, other, "catalog");
    }
    for mode in [
        "schema",
        "projection",
        "patch",
        "manifest",
        "pin",
        "output",
        "identity",
        "version",
        "path",
        "metadata",
        "owned",
        "duplicate-owned",
        "reference",
        "duplicate-reference",
        "retained-overlap",
        "unknown",
    ] {
        let f = fixture(Catalog::Minions);
        let mut p = doc(&f, POLICY);
        match mode {
            "schema" => p["schema"] = json!("future"),
            "projection" => p["projection_schema"] = json!("new-law"),
            "patch" => p["target"]["game_patch"] = json!("old"),
            "manifest" => p["target"]["facts_manifest_sha256"] = json!("0".repeat(64)),
            "pin" => p["source"]["sha256"] = json!("0".repeat(64)),
            "output" => p["catalog"]["output_file"] = json!("spectres.json"),
            "identity" => p["catalog"]["variety_identity_field"] = json!("id"),
            "version" => p["source"]["snapshot_version"] = json!("0.5.5.5"),
            "path" => p["source"]["path"] = json!("data/other.json"),
            "metadata" => p["source"]["metadata"]["schema"] = json!("spectres/v1"),
            "owned" => {
                p["owned_fields"].as_array_mut().unwrap().pop();
            }
            "duplicate-owned" => p["owned_fields"]
                .as_array_mut()
                .unwrap()
                .push(json!("name")),
            "reference" => p["projection_references"][0]["sha256"] = json!("0".repeat(64)),
            "duplicate-reference" => {
                let extra = p["projection_references"][0].clone();
                p["projection_references"]
                    .as_array_mut()
                    .unwrap()
                    .push(extra);
            }
            "retained-overlap" => p["retained_compatibility_fields_observed"]
                .as_array_mut()
                .unwrap()
                .push(json!("life")),
            _ => p["unknown_policy"] = json!(true),
        }
        write(&f, POLICY, &p);
        rejects(&f, Catalog::Minions, "");
    }
}

#[test]
fn pins_precede_strict_json_and_original_source_metadata_stays_archival() {
    for path in [SOURCE, POLICY] {
        let f = fixture(Catalog::Minions);
        let a = args(&f, Catalog::Minions);
        fs::write(f.root.join(path), b"not json").unwrap();
        let e = run(a).unwrap_err();
        assert!(e.contains("SHA256 mismatch"), "{path}: {e}");
        assert!(!f.root.join("out").exists());
        let mut f = fixture(Catalog::Minions);
        fs::write(f.root.join(path), b"{\"duplicate\":1,\"duplicate\":2}").unwrap();
        if path == SOURCE {
            reseal(&mut f);
        }
        rejects(&f, Catalog::Minions, "duplicate");
    }
    let mut f = fixture(Catalog::Spectres);
    let mut source = doc(&f, SOURCE);
    source["_meta"]["vendor_commit"] = json!("0".repeat(40));
    write(&f, SOURCE, &source);
    reseal(&mut f);
    rejects(&f, Catalog::Spectres, "metadata differs");
    let f = fixture(Catalog::Minions);
    let mut a = args(&f, Catalog::Minions);
    a.codec.as_mut().unwrap().source_sha256 = "ABC".into();
    assert!(run(a).is_err());
    assert!(!f.root.join("out").exists());
    let mut f = fixture(Catalog::Minions);
    let bytes = String::from_utf8(fs::read(f.root.join(SOURCE)).unwrap())
        .unwrap()
        .replace("999.0", "1e999");
    fs::write(f.root.join(SOURCE), bytes).unwrap();
    reseal(&mut f);
    rejects(&f, Catalog::Minions, "");
}

#[test]
fn both_modes_write_seven_replayable_files_and_six_authenticated_payloads() {
    for catalog in [Catalog::Minions, Catalog::Spectres] {
        let f = fixture(catalog);
        run(args(&f, catalog)).unwrap();
        run(Args {
            out: f.root.join("again"),
            ..args(&f, catalog)
        })
        .unwrap();
        let r = read(&f, "producer-receipt.json");
        assert_eq!(r["files"].as_object().unwrap().len(), 6);
        assert_eq!(
            r["input"]["selected_payloads"].as_object().unwrap().len(),
            6
        );
        assert_eq!(r["complete_runtime"], false);
        assert_eq!(r["adoption"], "not-adopted");
        assert_eq!(
            read(&f, "_domain_candidate.json")["domain"],
            format!("ordinary-{}", catalog.name())
        );
        assert!(!f.root.join("out/base").exists());
        assert!(!f.root.join("out/manifest.json").exists());
        for (input, output) in [
            (SOURCE, "legacy-actors.json"),
            (POLICY, "codec-policy.json"),
        ] {
            assert_eq!(
                fs::read(f.root.join(input)).unwrap(),
                fs::read(f.root.join("out").join(output)).unwrap()
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
    }
}

#[test]
fn actor_cli_requires_four_pins_and_rejects_branch_quality_and_duplicate_options() {
    for catalog in [Catalog::Minions, Catalog::Spectres] {
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
            catalog.name(),
            "--codec-source",
            "legacy",
            "--codec-source-sha256",
            "sha",
            "--codec-policy",
            "policy",
            "--codec-policy-sha256",
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
            ["--branch-codec", "branch"],
            ["--branch-codec-sha256", "sha"],
            ["--quality-policy", "p"],
            ["--quality-policy-sha256", "sha"],
            ["--ordinary-domain", catalog.name()],
            ["--raw", "old"],
            ["--native-facts", "F"],
        ] {
            assert!(parse_args(base.into_iter().chain(pair).map(String::from)).is_err());
        }
        assert!(parse_args(base.into_iter().chain(["--codec-source"]).map(String::from)).is_err());
    }
}

#[test]
fn actor_output_guards_protect_sources_policy_production_and_existing_paths() {
    for catalog in [Catalog::Minions, Catalog::Spectres] {
        let f = fixture(catalog);
        for parent in ["legacy", "policy", "input"] {
            let out = f.root.join(parent).join("forbidden");
            assert!(
                run(Args {
                    out: out.clone(),
                    ..args(&f, catalog)
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
                "actors-forbidden-{}",
                f.root.file_name().unwrap().to_string_lossy()
            ));
        assert!(
            run(Args {
                out: out.clone(),
                ..args(&f, catalog)
            })
            .is_err()
        );
        assert!(!out.exists());
        for mode in ["pin", "patch"] {
            let mut a = args(&f, catalog);
            if mode == "pin" {
                a.expected_manifest_sha256 = "0".repeat(64);
            } else {
                a.version = "old".into();
            }
            assert!(run(a).is_err());
            assert!(!f.root.join("out").exists());
        }
        std::os::unix::fs::symlink(f.root.join("absent"), f.root.join("out")).unwrap();
        assert!(run(args(&f, catalog)).is_err());
        fs::remove_file(f.root.join("out")).unwrap();
        run(args(&f, catalog)).unwrap();
        let before = fs::read(f.root.join("out/producer-receipt.json")).unwrap();
        assert!(run(args(&f, catalog)).is_err());
        assert_eq!(
            before,
            fs::read(f.root.join("out/producer-receipt.json")).unwrap()
        );
        let f = fixture(catalog);
        let a = args(&f, catalog);
        fs::write(f.root.join("input/tables/MonsterVarieties.json"), b"{}").unwrap();
        assert!(run(a).is_err());
        assert!(!f.root.join("out").exists());
    }
}

#[test]
fn eleven_previous_domains_keep_shapes_without_actor_sidecars() {
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
        (Domain::SkillOverrides, None, 7),
    ] {
        let mut f = match domain {
            Domain::StatSetCodec => super::super::stat_set_codec::tests::fixture(),
            Domain::GemEffects => super::super::gem_effects::tests::fixture(),
            Domain::MonsterResistances => super::super::monster_resistances::tests::fixture(),
            Domain::TotemVarieties => super::super::totem_varieties::tests::fixture(),
            Domain::MonsterScaling => super::super::monster_scaling::tests::fixture(),
            Domain::SkillStatMap => super::super::skill_stat_map::tests::fixture(),
            Domain::SkillOverrides => super::super::skill_overrides::tests::fixture(),
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
            Domain::SkillOverrides => super::super::skill_overrides::tests::args(&f),
            _ => Args { domain, ..f.args() },
        };
        run(a()).unwrap();
        for catalog in [Catalog::Minions, Catalog::Spectres] {
            let actor = fixture(catalog);
            run(args(&actor, catalog)).unwrap();
        }
        run(Args {
            out: f.root.join("again"),
            ..a()
        })
        .unwrap();
        let r = read(&f, "producer-receipt.json");
        assert_eq!(r.as_object().unwrap().len(), 10);
        assert_eq!(r["files"].as_object().unwrap().len(), count);
        assert!(!f.root.join("out/legacy-actors.json").exists());
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
