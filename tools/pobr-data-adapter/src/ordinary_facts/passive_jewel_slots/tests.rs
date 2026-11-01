use super::*;
use crate::ordinary_facts::{
    Args, Domain, parse_args, run,
    stat_set_codec::tests::table,
    tests::{Fixture, supplement},
};
use std::collections::BTreeMap;
use std::fs;

fn fixture() -> (Fixture, Value) {
    let mut f = Fixture::new();
    f.documents.insert(
        "tables/PassiveSkills.json".into(),
        table(
            "PassiveSkills",
            &[
                ("Id", "string", false, None),
                ("PassiveSkillGraphId", "u16", false, None),
                ("IsJewelSocket", "bool", false, None),
            ],
            json!([{"Id":"Retired ","PassiveSkillGraphId":12,"IsJewelSocket":true},
            {"Id":"Current","PassiveSkillGraphId":34,"IsJewelSocket":true}]),
        ),
    );
    f.seal();
    let slots = table(
        "PassiveJewelSlots",
        &[
            ("Slot", "foreignrow", false, Some("PassiveSkills")),
            (
                "ClusterJewelSize",
                "foreignrow",
                false,
                Some("PassiveTreeExpansionJewelSizes"),
            ),
            ("ProxySlot", "foreignrow", false, Some("PassiveSkills")),
            ("ReplacesSlot", "row", false, Some("PassiveJewelSlots")),
            ("StartIndices", "i32", true, None),
            ("SinisterJewelSocket", "bool", false, None),
        ],
        json!([
        {"Slot":1,"ClusterJewelSize":null,"ProxySlot":null,"ReplacesSlot":null,"StartIndices":[],"SinisterJewelSocket":false},
        {"Slot":0,"ClusterJewelSize":null,"ProxySlot":null,"ReplacesSlot":null,"StartIndices":[],"SinisterJewelSocket":true}]),
    );
    (f, json!({"PassiveJewelSlots":slots}))
}
fn args(f: &Fixture, tables: &Value) -> Args {
    Args {
        domain: Domain::PassiveJewelSlots,
        supplement: Some(supplement(f, tables)),
        ..f.args()
    }
}
fn loaded(f: &Fixture) -> PassiveJewelSlotsDef {
    let data = pobr_gamedata::GameData::from_memory(BTreeMap::from([
        (
            "overlay/passive_jewel_slots.json".into(),
            fs::read(f.root.join("out/overlay/passive_jewel_slots.json")).unwrap(),
        ),
        (
            "overlay-common/passive_jewel_slots.json".into(),
            br#"{"slots":[]}"#.to_vec(),
        ),
    ]));
    data.passive_jewel_slots().unwrap()
}
#[test]
fn all_rows_use_physical_order_and_target_ids_without_tree_filtering() {
    let (f, t) = fixture();
    run(args(&f, &t)).unwrap();
    let data = loaded(&f);
    assert_eq!(
        data.passive_id_for_wegame_socket("PassiveJewel_0"),
        Some("Current")
    );
    assert_eq!(
        data.passive_id_for_wegame_socket("PassiveJewel_1"),
        Some("Retired ")
    );
    assert_eq!(data.slots[1].passive_skill_graph_id, 12);
    assert!(data.slots[1].sinister_jewel_socket);
    assert_eq!(data.passive_id_for_wegame_socket("PassiveJewel_2"), None);
}
#[test]
fn source_changes_propagate_to_import_lookup_and_new_membership() {
    let (mut f, mut t) = fixture();
    f.values("PassiveSkills", 0)["Id"] = json!("Renamed");
    f.values("PassiveSkills", 0)["PassiveSkillGraphId"] = json!(65535);
    f.seal();
    t["PassiveJewelSlots"]["rows"][0]["values"]["Slot"] = json!(0);
    t["PassiveJewelSlots"]["rows"][1]["values"]["SinisterJewelSocket"] = json!(false);
    let mut row = t["PassiveJewelSlots"]["rows"][0].clone();
    row["_row"] = json!(2);
    t["PassiveJewelSlots"]["rows"]
        .as_array_mut()
        .unwrap()
        .push(row);
    t["PassiveJewelSlots"]["validation"]["rows"] = json!(3);
    run(args(&f, &t)).unwrap();
    let data = loaded(&f);
    assert_eq!(
        data.passive_id_for_wegame_socket("PassiveJewel_2"),
        Some("Renamed")
    );
    assert_eq!(data.slots.len(), 3);
    assert!(
        data.slots
            .iter()
            .all(|s| s.passive_skill_graph_id == 65535 && !s.sinister_jewel_socket)
    );
}
#[test]
fn removed_membership_is_not_refilled_from_common_data() {
    let (f, mut t) = fixture();
    t["PassiveJewelSlots"]["rows"] = json!([]);
    t["PassiveJewelSlots"]["validation"]["rows"] = json!(0);
    t["PassiveJewelSlots"]["validation"]["actual_row_bytes"] = json!(0);
    run(args(&f, &t)).unwrap();
    assert!(loaded(&f).slots.is_empty());
}
#[test]
fn dangling_or_unrepresentable_slot_semantics_fail_before_output() {
    for mode in 0..9 {
        let (mut f, mut t) = fixture();
        match mode {
            0 => t["PassiveJewelSlots"]["rows"][0]["values"]["Slot"] = json!(99),
            1 => f.values("PassiveSkills", 1)["IsJewelSocket"] = json!(false),
            2 => f.values("PassiveSkills", 1)["PassiveSkillGraphId"] = json!(65536),
            3 => t["PassiveJewelSlots"]["rows"][0]["values"]["SinisterJewelSocket"] = json!(0),
            4 => t["PassiveJewelSlots"]["rows"][0]["values"]["ProxySlot"] = json!(0),
            5 => t["PassiveJewelSlots"]["rows"][0]["values"]["ReplacesSlot"] = json!(1),
            6 => t["PassiveJewelSlots"]["rows"][0]["values"]["ClusterJewelSize"] = json!(0),
            7 => t["PassiveJewelSlots"]["rows"][0]["values"]["StartIndices"] = json!([0]),
            _ => {
                t["PassiveJewelSlots"]["columns"][0]["schema"]["references"] =
                    json!({"table":"Stats"})
            }
        }
        f.seal();
        assert!(run(args(&f, &t)).is_err(), "mode {mode}");
        assert!(!f.root.join("out").exists());
    }
}
#[test]
fn both_exact_producers_work_but_unknown_producer_patch_and_locale_fail() {
    for mode in 0..5 {
        let (f, t) = fixture();
        let mut a = args(&f, &t);
        let extra = a.supplement.as_mut().unwrap();
        let p = extra.path.join("manifest.json");
        let mut m: Value = crate::read_json(&p).unwrap();
        match mode {
            0 => {
                m["producer"] = serde_json::from_slice(include_bytes!(
                    "../../../tests/fixtures/ordinary-facts/selected-tables-producer.json"
                ))
                .unwrap()
            }
            1 => {}
            2 => m["producer"]["RePoE/native_tables.py"] = json!("f".repeat(64)),
            3 => m["source_identity"]["game_patch"] = json!("0.5.5.6"),
            _ => m["format"] = json!("repoe-localized-tables/v1"),
        }
        let bytes = crate::native_facts::reader::json_bytes(&m).unwrap();
        fs::write(p, &bytes).unwrap();
        extra.sha256 = sha256_hex(&bytes);
        assert_eq!(run(a).is_ok(), mode < 2, "mode {mode}");
    }
}
#[test]
fn cli_requires_paired_supplement_and_rejects_irrelevant_policies() {
    let base = [
        "--ordinary-facts",
        "/f",
        "--facts-manifest-sha256",
        "pin",
        "--version",
        "0.5.5.5",
        "--out",
        "/o",
        "--ordinary-domain",
        "passive-jewel-slots",
    ];
    assert!(parse_args(base.iter().map(|s| s.to_string())).is_err());
    let mut a = base.to_vec();
    a.extend(["--supplement", "/s", "--supplement-manifest-sha256", "p"]);
    assert_eq!(
        parse_args(a.iter().map(|s| s.to_string())).unwrap().domain,
        Domain::PassiveJewelSlots
    );
    a.extend(["--codec-source", "/x"]);
    assert!(parse_args(a.iter().map(|s| s.to_string())).is_err());
}
