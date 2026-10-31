use super::super::stat_set_codec::tests::table;
use super::super::tests::Fixture;
use super::super::{Args, CodecArgs, Domain, parse_args, reader, run};
use crate::raw_source::sha256_hex;
use pobr_data::catalog::{PassiveNodeDef, PassiveTreeMeta};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;

const SOURCE: &str = "legacy/source.json";
const POLICY: &str = "policy/policy.json";

pub(in crate::ordinary_facts) fn fixture() -> Fixture {
    let mut f = Fixture::new();
    let old: Vec<_> = f
        .documents
        .keys()
        .filter(|p| p.starts_with("tables/"))
        .cloned()
        .collect();
    f.documents.retain(|p, _| p == "source-lock.json");
    let ids = [
        "maximum_resistances1",
        "Unknown",
        "Multi",
        "Context",
        "OverrideBase",
        "OverrideTarget",
        "Variant",
        "Asc",
        "Unlock",
        "Armour",
        "AliasTarget",
        "Outside",
        "Trailing ",
    ];
    let skills = [
        101, 202, 303, 404, 505, 606, 707, 808, 909, 1001, 1102, 1203, 1304,
    ];
    let rows: Vec<_> = ids
        .iter()
        .zip(skills)
        .map(|(id, skill)| {
            json!({"Id":id,"Name":id,"Icon_DDSFile":"icon.dds","PassiveSkillGraphId":skill,
        "Stats":[0],"Stat1Value":-7,"Stat2Value":0,"Opaque":7})
        })
        .collect();
    f.documents.insert(
        "tables/PassiveSkills.json".into(),
        table(
            "PassiveSkills",
            &[
                ("Id", "string", false, None),
                ("Name", "string", false, None),
                ("Icon_DDSFile", "string", false, None),
                ("PassiveSkillGraphId", "u16", false, None),
                ("Stats", "foreignrow", true, Some("Stats")),
                ("Stat1Value", "i32", false, None),
                ("Stat2Value", "i32", false, None),
                ("Opaque", "i32", false, None),
            ],
            json!(rows),
        ),
    );
    f.values("PassiveSkills", 1)["Stats"] = json!([1]);
    f.values("PassiveSkills", 2)["Stats"] = json!([0, 0]);
    f.values("PassiveSkills", 2)["Stat2Value"] = json!(22);
    f.values("PassiveSkills", 3)["Opaque"] = json!(9);
    f.documents.insert(
        "tables/Stats.json".into(),
        table(
            "Stats",
            &[("Id", "string", false, None)],
            json!([
        {"Id":"base_fire_damage_resistance_%"},{"Id":"unknown_stat"}]),
        ),
    );
    f.documents.insert("tables/Characters.json".into(),table("Characters",&[
        ("Id","string",false,None),("Name","string",false,None),("BaseStrength","i32",false,None),("BaseDexterity","i32",false,None),("BaseIntelligence","i32",false,None)
    ],json!([{"Id":"WarriorId","Name":"Warrior","BaseStrength":0,"BaseDexterity":-3,"BaseIntelligence":17},
        {"Id":"WitchId","Name":"Witch","BaseStrength":19,"BaseDexterity":23,"BaseIntelligence":29}])));
    f.documents.insert(
        "tables/ClassPassiveSkillOverrides.json".into(),
        table(
            "ClassPassiveSkillOverrides",
            &[
                (
                    "CharacterToOverrideFor",
                    "foreignrow",
                    false,
                    Some("Characters"),
                ),
                (
                    "SkillToOverride",
                    "foreignrow",
                    false,
                    Some("PassiveSkills"),
                ),
                ("Override", "foreignrow", false, Some("PassiveSkills")),
            ],
            json!([{"CharacterToOverrideFor":1,"SkillToOverride":4,"Override":5}]),
        ),
    );
    f.seal();
    for path in old {
        if !f.documents.contains_key(&path) {
            fs::remove_file(f.root.join("input").join(path)).unwrap();
        }
    }
    fs::create_dir(f.root.join("legacy")).unwrap();
    fs::create_dir(f.root.join("policy")).unwrap();
    let mut p: Value = serde_json::from_slice(include_bytes!(
        "../../../../../pipeline/passive-tree/0.5.5.5-prepared-compat-policy.json"
    ))
    .unwrap();
    p["legacy_metadata"]["value"] = json!({"tree":"Default","unknown_meta":[null,0,false],"classes":[
        {"name":"Witch","base_str":1,"base_dex":2,"base_int":3,"ascendancies":[{"id":"Witch3","name":"Lich","future":17}],"extra":"keep"},
        {"name":"Warrior","base_str":4,"base_dex":5,"base_int":6,"ascendancies":[]} ]});
    let mut nodes:Vec<_>=ids[..11].iter().zip(skills).map(|(id,skill)|json!({"skill":skill,"id":id,"name":id,"kind":"normal","stats":["+5% to Fire Resistance"],"connections":[]})).collect();
    nodes[0]["connections"] = json!([707, 1203, 707, 1203, 2001]);
    nodes[0]["group"] = json!(4);
    nodes[0]["orbit"] = json!(2);
    nodes[0]["orbit_index"] = json!(0);
    nodes[0]["x"] = json!(-1.25);
    nodes[0]["y"] = json!(0.0);
    nodes[0]["unknown/~field"] = json!({"keep":[null,0,false]});
    nodes[6]["variants"] = json!([{"class":"Witch","variant_skill":2001,"name":"Alternate","stats":["+9 to Intelligence","unparsed line"],"unknown":7}]);
    nodes[7]["ascendancy_id"] = json!("Witch3");
    nodes[7]["kind"] = json!("notable");
    nodes[8]["unlock_constraint"] = json!({"nodes":[101,101],"ascendancy":"Witch3","future":false});
    nodes[9]["apply_to_armour"] = json!(true);
    nodes[9]["stats"] = json!(["first", "second"]);
    nodes[10]["id"] = json!("1102");
    let mut alias = p["legacy_id_bindings"][0].clone();
    alias["legacy_id"] = json!("1102");
    alias["skill"] = json!(1102);
    alias["target_passive_id"] = json!("AliasTarget");
    alias["expected_name"] = json!("AliasTarget");
    alias["source_entry_sha256"] = json!(hash(&nodes[10]));
    p["legacy_id_bindings"] = json!([alias]);
    let seed = p["legacy_connection_exceptions"][0].clone();
    p["legacy_connection_exceptions"] = json!([1, 3].map(|position| {
        let mut c = seed.clone();
        c["source_row"] = json!(0);
        c["skill"] = json!(101);
        c["id"] = json!(ids[0]);
        c["source_entry_sha256"] = json!(hash(&nodes[0]));
        c["source_pointer"] = json!(format!("/0/connections/{position}"));
        c["position"] = json!(position);
        c["target_skill"] = json!(1203);
        c["target_passive_id"] = json!("Outside");
        c["expected_target_name"] = json!("Outside");
        c["target_row_evidence"] = json!(11);
        c
    }));
    p["qualification"]["context_sha256"] = json!(hash(
        &super::source::context(f.values("PassiveSkills", 0)).unwrap()
    ));
    write(&f, SOURCE, &json!(nodes));
    write(&f, POLICY, &p);
    reseal(&mut f);
    f
}
fn hash(value: &impl serde::Serialize) -> String {
    sha256_hex(&reader::json_bytes(value).unwrap())
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
    p["source"]["sha256"] = json!(sha256_hex(&fs::read(f.root.join(SOURCE)).unwrap()));
    write(f, POLICY, &p);
}
pub(in crate::ordinary_facts) fn args(f: &Fixture) -> Args {
    Args {
        domain: Domain::PassiveTree,
        codec: Some(CodecArgs {
            source: f.root.join(SOURCE),
            source_sha256: sha256_hex(&fs::read(f.root.join(SOURCE)).unwrap()),
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
fn output(f: &Fixture) -> Value {
    doc(f, "out/base/passive_tree.json")
}
fn conversion(f: &Fixture) -> Value {
    doc(f, "out/producer-receipt.json")["conversion"].clone()
}
fn refresh_entry_declarations(f: &Fixture) {
    let s = doc(f, SOURCE);
    let mut p = doc(f, POLICY);
    for c in p["legacy_connection_exceptions"].as_array_mut().unwrap() {
        c["source_entry_sha256"] = json!(hash(&s[c["source_row"].as_u64().unwrap() as usize]));
    }
    for b in p["legacy_id_bindings"].as_array_mut().unwrap() {
        let entry = s
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == b["legacy_id"])
            .unwrap();
        b["source_entry_sha256"] = json!(hash(entry));
    }
    write(f, POLICY, &p);
}

#[test]
fn complete_tree_and_metadata_keep_raw_fields_order_presence_and_compatibility() {
    let f = fixture();
    run(args(&f)).unwrap();
    let original = doc(&f, SOURCE);
    let out = output(&f);
    let dto: Vec<PassiveNodeDef> = serde_json::from_value(out.clone()).unwrap();
    assert_eq!(dto.len(), 11);
    pobr_tree::PassiveTree::from_json(
        &String::from_utf8(fs::read(f.root.join("out/base/passive_tree.json")).unwrap()).unwrap(),
    )
    .unwrap();
    for (before, after) in original
        .as_array()
        .unwrap()
        .iter()
        .zip(out.as_array().unwrap())
    {
        let mut retained = after.clone();
        retained.as_object_mut().unwrap().remove("native_stats");
        assert_eq!(*before, retained);
    }
    assert_eq!(out[0]["connections"], json!([707, 1203, 707, 1203, 2001]));
    assert!(dto[9].apply_to_armour);
    assert_eq!(dto[8].unlock_constraint.as_ref().unwrap().nodes, [101, 101]);
    assert_eq!(
        dto[6].variants[0].stats,
        ["+9 to Intelligence", "unparsed line"]
    );
    let meta = doc(&f, "out/base/passive_tree_meta.json");
    let typed: PassiveTreeMeta = serde_json::from_value(meta.clone()).unwrap();
    assert_eq!(
        (
            typed.classes[0].base_str,
            typed.classes[0].base_dex,
            typed.classes[0].base_int
        ),
        (19, 23, 29)
    );
    assert_eq!(
        (
            typed.classes[1].base_str,
            typed.classes[1].base_dex,
            typed.classes[1].base_int
        ),
        (0, -3, 17)
    );
    let mut original_meta = doc(&f, POLICY)["legacy_metadata"]["value"].clone();
    for (before, after) in original_meta["classes"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .zip(meta["classes"].as_array().unwrap())
    {
        for field in ["base_str", "base_dex", "base_int"] {
            before[field] = after[field].clone();
        }
    }
    assert_eq!(original_meta, meta);
    let r = conversion(&f);
    assert_eq!(r["native_packets"], 1);
    assert_eq!(r["connection_exceptions"].as_array().unwrap().len(), 2);
    assert_eq!(
        r["class_override_inventory"][0]["definitions"][0]["source_row"],
        4
    );
    assert_eq!(r["metadata_authoring"]["file_read_by_producer"], false);
    for rec in r["source_records"].as_array().unwrap() {
        let row = rec["source_row"].as_u64().unwrap() as usize;
        assert_eq!(rec["source_entry_sha256"], hash(&original[row]));
        assert_eq!(rec["output_entry_sha256"], hash(&out[row]));
        for field in rec["retained_fields"].as_array().unwrap() {
            assert_eq!(
                field["value_sha256"],
                hash(
                    original
                        .pointer(field["source_pointer"].as_str().unwrap())
                        .unwrap()
                )
            );
        }
    }
}

#[test]
fn target_signed_zero_values_and_provenance_use_shared_selection_without_text_rewrite() {
    use pobr_core::ingest::passive::{AllocatedNode, NativePassiveInput, select_native_passive};
    use pobr_data::passive_tree::NodeId;
    for value in [-11, 0, 13] {
        let mut f = fixture();
        f.values("PassiveSkills", 0)["Stat1Value"] = json!(value);
        reseal(&mut f);
        run(args(&f)).unwrap();
        let dto: Vec<PassiveNodeDef> = serde_json::from_value(output(&f)).unwrap();
        let packet = dto[0].native_stats.clone().unwrap();
        assert_eq!(packet.stats[0].value, value);
        assert_eq!(packet.modifier_texts, ["+5% to Fire Resistance"]);
        assert_eq!(packet.tree_version.as_deref(), Some("0_5"));
        assert_eq!(packet.data_version, "0.5.5.5");
        let provenance = &packet.stats[0].provenance;
        assert_eq!((provenance.passive_row, provenance.stat_row), (0, 0));
        assert_eq!(
            provenance.source_manifest_sha256,
            args(&f).expected_manifest_sha256
        );
        assert_eq!(
            provenance.references[0].pointer,
            "/rows/0/values/Stat1Value"
        );
        assert_eq!(provenance.references[1].pointer, "/rows/0/values/Id");
        assert_ne!(
            provenance.references[0].resource,
            provenance.references[1].resource
        );
        let mut allocated = AllocatedNode {
            node_id: NodeId(101),
            ascendancy: false,
            modifier_texts: dto[0].stats.clone(),
            native: Some(NativePassiveInput {
                packet,
                ineligible: None,
            }),
        };
        assert!(select_native_passive(&allocated).unwrap().is_ok());
        allocated.native.as_mut().unwrap().packet.modifier_texts = vec!["stale".into()];
        assert_eq!(
            select_native_passive(&allocated)
                .unwrap()
                .unwrap_err()
                .fallback_texts,
            dto[0].stats
        );
        allocated.native = None;
        assert!(select_native_passive(&allocated).is_none());
    }
}

#[test]
fn unknown_multi_context_class_relation_transformed_and_alias_nodes_keep_complete_text() {
    let f = fixture();
    run(args(&f)).unwrap();
    let out = output(&f);
    let r = conversion(&f);
    for i in 1..11 {
        assert!(out[i].get("native_stats").is_none(), "row {i}");
        assert!(
            !r["source_records"][i]["native"]["reasons"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
    assert_eq!(
        r["source_records"][2]["ordered_raw_stats"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(r["source_records"][2]["ordered_raw_stats"][1]["value"], 22);
    assert_eq!(
        r["source_records"][3]["context"]["differing_fields"],
        json!(["Opaque"])
    );
    assert_eq!(out[10]["id"], "1102");
    assert_eq!(r["source_records"][10]["target"]["id"], "AliasTarget");
    assert_eq!(
        r["source_records"][4]["context"]["class_override_rows"],
        json!([0])
    );
    assert_eq!(
        r["source_records"][5]["context"]["class_override_rows"],
        json!([0])
    );
}

#[test]
fn old_packets_never_fallback_and_display_whitespace_is_never_normalized() {
    let f = fixture();
    run(args(&f)).unwrap();
    let packet = output(&f)[0]["native_stats"].clone();
    for line in [
        "",
        "  +5% to Fire Resistance",
        "+5% to Fire Resistance\n",
        "line\nsecond",
    ] {
        let mut f = fixture();
        let mut s = doc(&f, SOURCE);
        s[0]["stats"] = json!([line]);
        s[0]["native_stats"] = packet.clone();
        write(&f, SOURCE, &s);
        refresh_entry_declarations(&f);
        reseal(&mut f);
        run(args(&f)).unwrap();
        assert_eq!(output(&f)[0]["stats"], json!([line]));
        assert!(output(&f)[0].get("native_stats").is_none());
    }
    let mut f = fixture();
    let mut s = doc(&f, SOURCE);
    s[1]["native_stats"] = packet;
    write(&f, SOURCE, &s);
    reseal(&mut f);
    run(args(&f)).unwrap();
    assert!(output(&f)[1].get("native_stats").is_none());
}

#[test]
fn connection_occurrences_require_exact_source_and_target_and_consume_once() {
    for mode in [
        "undeclared",
        "extra",
        "target",
        "position",
        "hash",
        "duplicate",
        "unused",
        "target-id",
        "target-row",
        "target-name",
        "target-graph",
        "source-id",
        "source-skill",
        "pointer",
    ] {
        let mut f = fixture();
        let mut s = doc(&f, SOURCE);
        let mut p = doc(&f, POLICY);
        match mode {
            "undeclared" => p["legacy_connection_exceptions"] = json!([]),
            "extra" => {
                s[1]["connections"] = json!([1203]);
            }
            "target" => p["legacy_connection_exceptions"][0]["target_skill"] = json!(1304),
            "position" => {
                p["legacy_connection_exceptions"][0]["position"] = json!(5);
                p["legacy_connection_exceptions"][0]["source_pointer"] = json!("/0/connections/5");
            }
            "hash" => {
                p["legacy_connection_exceptions"][0]["source_entry_sha256"] = json!("0".repeat(64))
            }
            "duplicate" => {
                let case = p["legacy_connection_exceptions"][0].clone();
                p["legacy_connection_exceptions"]
                    .as_array_mut()
                    .unwrap()
                    .push(case);
            }
            "unused" => {
                let mut case = p["legacy_connection_exceptions"][0].clone();
                case["source_row"] = json!(99);
                case["source_pointer"] = json!("/99/connections/1");
                p["legacy_connection_exceptions"]
                    .as_array_mut()
                    .unwrap()
                    .push(case);
            }
            "target-id" => {
                p["legacy_connection_exceptions"][0]["target_passive_id"] = json!("Outside ")
            }
            "target-row" => p["legacy_connection_exceptions"][0]["target_row_evidence"] = json!(0),
            "target-name" => {
                p["legacy_connection_exceptions"][0]["expected_target_name"] = json!("changed")
            }
            "target-graph" => f.values("PassiveSkills", 11)["PassiveSkillGraphId"] = json!(1304),
            "source-id" => p["legacy_connection_exceptions"][0]["id"] = json!("changed"),
            "source-skill" => p["legacy_connection_exceptions"][0]["skill"] = json!(0),
            _ => p["legacy_connection_exceptions"][0]["source_pointer"] = json!("/0/connections/3"),
        }
        write(&f, SOURCE, &s);
        write(&f, POLICY, &p);
        reseal(&mut f);
        rejects(&f, "");
    }
    for mode in ["emitted", "variant"] {
        let mut f = fixture();
        let mut s = doc(&f, SOURCE);
        if mode == "emitted" {
            s.as_array_mut()
                .unwrap()
                .push(json!({"id":"Outside","skill":1203,"kind":"normal","stats":["line"]}));
        } else {
            s[6]["variants"][0]["variant_skill"] = json!(1203);
            s[0]["connections"] = json!([707, 1203, 707, 1203]);
        }
        write(&f, SOURCE, &s);
        refresh_entry_declarations(&f);
        reseal(&mut f);
        rejects(&f, "stale connection");
    }
    // Empty/default exceptions allow ordinary internal references, not a wildcard waiver.
    let mut f = fixture();
    let mut s = doc(&f, SOURCE);
    s[0]["connections"] = json!([707, 707, 2001]);
    write(&f, SOURCE, &s);
    let mut p = doc(&f, POLICY);
    p.as_object_mut()
        .unwrap()
        .remove("legacy_connection_exceptions");
    write(&f, POLICY, &p);
    reseal(&mut f);
    run(args(&f)).unwrap();
    assert!(
        conversion(&f)["connection_exceptions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn explicit_identity_alias_is_fixed_and_never_a_numeric_or_name_fallback() {
    for mode in [
        "missing",
        "hash",
        "skill",
        "name",
        "target",
        "target-name",
        "duplicate",
        "unused",
    ] {
        let mut f = fixture();
        let mut p = doc(&f, POLICY);
        match mode {
            "missing" => p["legacy_id_bindings"] = json!([]),
            "hash" => p["legacy_id_bindings"][0]["source_entry_sha256"] = json!("0".repeat(64)),
            "skill" => p["legacy_id_bindings"][0]["skill"] = json!(1),
            "name" => p["legacy_id_bindings"][0]["expected_name"] = json!("wrong"),
            "target" => p["legacy_id_bindings"][0]["target_passive_id"] = json!("1102"),
            "target-name" => f.values("PassiveSkills", 10)["Name"] = json!("changed"),
            "duplicate" => {
                let b = p["legacy_id_bindings"][0].clone();
                p["legacy_id_bindings"].as_array_mut().unwrap().push(b);
            }
            _ => {
                let mut b = p["legacy_id_bindings"][0].clone();
                b["legacy_id"] = json!("no-node");
                b["skill"] = json!(65500);
                p["legacy_id_bindings"].as_array_mut().unwrap().push(b);
            }
        }
        write(&f, POLICY, &p);
        reseal(&mut f);
        rejects(&f, "");
    }
    let mut f = fixture();
    f.values("PassiveSkills", 10)["Id"] = json!("1102");
    reseal(&mut f);
    rejects(&f, "stale alias");
}

#[test]
fn prepared_reference_shape_duplicates_and_exact_ids_fail_without_dropping_rows() {
    for mode in [
        "duplicate-skill",
        "duplicate-id",
        "empty-id",
        "missing-id",
        "graph",
        "unknown-asc",
        "unknown-unlock",
        "unknown-selector",
        "duplicate-selector",
        "alias-collision",
        "coordinates",
        "null-stats",
        "bad-native",
    ] {
        let mut f = fixture();
        let mut s = doc(&f, SOURCE);
        match mode {
            "duplicate-skill" => s[1]["skill"] = json!(101),
            "duplicate-id" => s[1]["id"] = json!("maximum_resistances1"),
            "empty-id" => s[1]["id"] = json!(""),
            "missing-id" => s[1]["id"] = json!("Unknown "),
            "graph" => s[1]["skill"] = json!(999),
            "unknown-asc" => s[7]["ascendancy_id"] = json!("UnknownAsc"),
            "unknown-unlock" => s[8]["unlock_constraint"]["nodes"] = json!([1203]),
            "unknown-selector" => s[6]["variants"][0]["class"] = json!("UnknownClass"),
            "duplicate-selector" => {
                let v = s[6]["variants"][0].clone();
                s[6]["variants"].as_array_mut().unwrap().push(v);
            }
            "alias-collision" => s[6]["variants"][0]["variant_skill"] = json!(202),
            "coordinates" => s[1]["x"] = json!(0),
            "null-stats" => s[1]["stats"] = Value::Null,
            _ => s[1]["native_stats"] = json!({}),
        }
        write(&f, SOURCE, &s);
        reseal(&mut f);
        rejects(&f, "");
    }
    let mut f = fixture();
    let mut s = doc(&f, SOURCE);
    s[1]["id"] = json!("Unknown ");
    f.values("PassiveSkills", 1)["Id"] = json!("Unknown ");
    write(&f, SOURCE, &s);
    reseal(&mut f);
    run(args(&f)).unwrap();
    assert_eq!(output(&f)[1]["id"], "Unknown ");
}

#[test]
fn physical_rows_metadata_exact_values_fk_and_complete_slots_are_strict() {
    for (table_name, row, field) in [
        ("PassiveSkills", 1, "Stat1Value"),
        ("Characters", 0, "BaseStrength"),
    ] {
        for value in [
            Value::Null,
            json!(1.5),
            json!("0"),
            json!(2147483648_u64),
            json!(-2147483649_i64),
        ] {
            let mut f = fixture();
            f.values(table_name, row)[field] = value;
            reseal(&mut f);
            rejects(&f, field);
        }
    }
    for mode in [
        "bad-fk",
        "null-ref",
        "missing-slot",
        "extra-stat",
        "graph-range",
        "bad-row",
        "duplicate-id",
        "duplicate-name",
        "override-character",
        "override-base",
        "override-target",
        "duplicate-override-row",
        "type",
        "array",
        "reference",
        "missing-column",
        "missing-character",
    ] {
        let mut f = fixture();
        match mode {
            "bad-fk" => f.values("PassiveSkills", 1)["Stats"] = json!([999]),
            "null-ref" => f.values("PassiveSkills", 1)["Stats"] = json!([null]),
            "missing-slot" => {
                f.values("PassiveSkills", 1)
                    .as_object_mut()
                    .unwrap()
                    .remove("Stat2Value");
            }
            "extra-stat" => f.values("PassiveSkills", 1)["Stats"] = json!([0, 0, 0]),
            "graph-range" => f.values("PassiveSkills", 1)["PassiveSkillGraphId"] = json!(65536),
            "bad-row" => {
                f.documents.get_mut("tables/PassiveSkills.json").unwrap()["rows"][1]["_row"] =
                    json!(0)
            }
            "duplicate-id" => f.values("Stats", 1)["Id"] = json!("base_fire_damage_resistance_%"),
            "duplicate-name" => f.values("Characters", 1)["Name"] = json!("Warrior"),
            "override-character" => {
                f.values("ClassPassiveSkillOverrides", 0)["CharacterToOverrideFor"] = json!(999)
            }
            "override-base" => {
                f.values("ClassPassiveSkillOverrides", 0)["SkillToOverride"] = Value::Null
            }
            "override-target" => f.values("ClassPassiveSkillOverrides", 0)["Override"] = json!(999),
            "duplicate-override-row" => {
                let d = f
                    .documents
                    .get_mut("tables/ClassPassiveSkillOverrides.json")
                    .unwrap();
                let row = d["rows"][0].clone();
                d["rows"].as_array_mut().unwrap().push(row);
                d["validation"]["rows"] = json!(2);
            }
            "type" => {
                f.documents.get_mut("tables/PassiveSkills.json").unwrap()["columns"][5]["schema"]
                    ["type"] = json!("u32")
            }
            "array" => {
                f.documents.get_mut("tables/PassiveSkills.json").unwrap()["columns"][4]["schema"]
                    ["array"] = json!(false)
            }
            "reference" => {
                f.documents.get_mut("tables/PassiveSkills.json").unwrap()["columns"][4]["schema"]
                    ["references"] = json!({"table":"Characters"})
            }
            "missing-column" => {
                let d = f.documents.get_mut("tables/PassiveSkills.json").unwrap();
                d["physical_fields"][6] = json!("Other");
                d["columns"][6]["physical_fields"] = json!(["Other"]);
                d["columns"][6]["schema"]["name"] = json!("Other");
                for row in d["rows"].as_array_mut().unwrap() {
                    row["values"].as_object_mut().unwrap().remove("Stat2Value");
                    row["values"]["Other"] = json!(0);
                }
            }
            _ => f.values("Characters", 0)["Name"] = json!("UnprovidedClass"),
        }
        reseal(&mut f);
        rejects(&f, "");
    }
}

#[test]
fn policy_context_source_pins_and_embedded_metadata_guards_precede_reserve() {
    for (pointer, value) in [
        ("/schema", json!("future")),
        ("/target/game_patch", json!("old")),
        ("/target/facts_manifest_sha256", json!("0".repeat(64))),
        ("/source/sha256", json!("0".repeat(64))),
        ("/source/schema", json!("object")),
        ("/source/path", json!("../tree")),
        ("/catalog/tree_compatibility_tag", json!("future")),
        ("/catalog/domain", json!("skills")),
        ("/catalog/meta_file", json!("other.json")),
        ("/qualification/context_sha256", json!("0".repeat(64))),
        ("/qualification/reference_passive_id", json!("other")),
        (
            "/qualification/variable_fields",
            json!(["Id", "Name", "Stats"]),
        ),
        ("/legacy_metadata/value/tree", json!("Other")),
        ("/legacy_metadata/value/classes/1/name", json!("Witch")),
        (
            "/legacy_metadata/value/classes/0/ascendancies/0/id",
            json!(""),
        ),
        (
            "/legacy_metadata/source_declaration/role",
            json!("verified file"),
        ),
        ("/legacy_metadata/source_declaration/sha256", json!("bad")),
    ] {
        let f = fixture();
        let mut p = doc(&f, POLICY);
        *p.pointer_mut(pointer).unwrap() = value;
        write(&f, POLICY, &p);
        rejects(&f, "");
    }
    for path in [SOURCE, POLICY] {
        let f = fixture();
        let a = args(&f);
        fs::write(f.root.join(path), b"not json").unwrap();
        assert!(run(a).unwrap_err().contains("SHA256 mismatch"));
        assert!(!f.root.join("out").exists());
        let mut f = fixture();
        fs::write(f.root.join(path), b"{\"duplicate\":1,\"duplicate\":2}").unwrap();
        if path == SOURCE {
            reseal(&mut f);
        }
        rejects(&f, "duplicate");
    }
    let mut f = fixture();
    f.values("PassiveSkills", 0)["Opaque"] = json!(8);
    reseal(&mut f);
    rejects(&f, "reference context SHA256 mismatch");
    let mut f = fixture();
    f.values("PassiveSkills", 0)["Stats"] = json!([1]);
    reseal(&mut f);
    rejects(&f, "one fire-resistance stat");
    let f = fixture();
    let mut p = doc(&f, POLICY);
    p["source"]["snapshot_version"] = json!("not-provided");
    p["source"]["path"] = json!("data/not-provided/base/passive_tree.json");
    p["legacy_metadata"]["source_declaration"]["snapshot_version"] = json!("not-provided");
    p["legacy_metadata"]["source_declaration"]["path"] =
        json!("data/not-provided/base/passive_tree_meta.json");
    write(&f, POLICY, &p);
    run(args(&f)).unwrap();
    assert_eq!(
        conversion(&f)["metadata_authoring"]["file_hash_verified_by_producer"],
        false
    );
    assert!(!f.root.join("data/not-provided").exists());
}

#[test]
fn eight_files_five_payloads_raw_archives_and_receipt_replay_without_overlay() {
    let f = fixture();
    run(args(&f)).unwrap();
    run(Args {
        out: f.root.join("again"),
        ..args(&f)
    })
    .unwrap();
    let r = doc(&f, "out/producer-receipt.json");
    assert_eq!(r["files"].as_object().unwrap().len(), 7);
    assert_eq!(
        r["input"]["selected_payloads"].as_object().unwrap().len(),
        5
    );
    assert_eq!(r["complete_runtime"], false);
    assert_eq!(r["adoption"], "not-adopted");
    assert_eq!(
        doc(&f, "out/_domain_candidate.json")["domain"],
        "ordinary-passive-tree"
    );
    for absent in ["overlay", "manifest.json", "CURRENT", "native_runtime"] {
        assert!(!f.root.join("out").join(absent).exists());
    }
    for (input, output) in [
        (SOURCE, "legacy-passive-tree.json"),
        (POLICY, "codec-policy.json"),
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
}

#[test]
fn cli_and_paths_reject_inapplicable_options_pins_version_and_overwrites() {
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
        "passive-tree",
        "--codec-source",
        "source",
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
        ["--raw", "old"],
        ["--native-facts", "F"],
        ["--ordinary-domain", "unknown"],
    ] {
        assert!(parse_args(cli.into_iter().chain(pair).map(String::from)).is_err());
    }
    assert!(parse_args(cli.into_iter().chain(["--codec-source"]).map(String::from)).is_err());
    let f = fixture();
    for parent in ["input", "legacy", "policy"] {
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
    fs::write(f.root.join("input/tables/PassiveSkills.json"), b"{}").unwrap();
    rejects(&f, "");
}

#[test]
fn existing_class_variant_selection_still_replaces_the_complete_text() {
    use pobr_data::passive_tree::{NodeId, PassiveTreeSpec};
    let f = fixture();
    run(args(&f)).unwrap();
    let nodes: Vec<PassiveNodeDef> = serde_json::from_value(output(&f)).unwrap();
    let nodes = nodes.into_iter().map(|n| (n.skill, n)).collect();
    let spec = PassiveTreeSpec {
        allocated_nodes: vec![NodeId(707)],
        ..Default::default()
    };
    let collected = pobr_tree::collect_allocated_mods_for_class(
        &spec,
        &nodes,
        pobr_tree::ClassContext {
            class_name: "Witch",
            ascendancy_name: "",
        },
    );
    assert_eq!(
        collected[0].modifier_texts,
        ["+9 to Intelligence", "unparsed line"]
    );
}

#[test]
fn prior_domains_keep_files_and_receipts_without_passive_tree_sidecars() {
    for (domain, bytes, count) in [
        (Domain::StatsMods, None, 5),
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
        assert!(!f.root.join("out/legacy-passive-tree.json").exists());
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
fn repeated_class_relation_occurrences_are_preserved_without_inventing_variant_rules() {
    let mut f = fixture();
    let d = f
        .documents
        .get_mut("tables/ClassPassiveSkillOverrides.json")
        .unwrap();
    let mut row = d["rows"][0].clone();
    row["_row"] = json!(1);
    d["rows"].as_array_mut().unwrap().push(row);
    d["validation"]["rows"] = json!(2);
    reseal(&mut f);
    run(args(&f)).unwrap();
    let r = conversion(&f);
    assert_eq!(r["class_override_inventory"].as_array().unwrap().len(), 2);
    assert_eq!(
        r["source_records"][4]["context"]["class_override_rows"],
        json!([0, 1])
    );
    assert!(output(&f)[4].get("native_stats").is_none());
}

#[test]
fn empty_stats_and_changed_nonfirst_slots_never_create_partial_native_packets() {
    for mode in ["empty", "unused-slot"] {
        let mut f = fixture();
        if mode == "empty" {
            f.values("PassiveSkills", 1)["Stats"] = json!([]);
        } else {
            f.values("PassiveSkills", 1)["Stats"] = json!([0]);
            f.values("PassiveSkills", 1)["Stat2Value"] = json!(-9);
        }
        reseal(&mut f);
        run(args(&f)).unwrap();
        assert!(output(&f)[1].get("native_stats").is_none());
        assert_eq!(output(&f)[1]["stats"], doc(&f, SOURCE)[1]["stats"]);
    }
}
