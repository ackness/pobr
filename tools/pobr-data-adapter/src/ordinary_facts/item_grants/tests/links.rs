use super::*;
use crate::ordinary_facts::tests::supplement;

fn linked_fixture() -> (Fixture, Value, Value) {
    let (mut f, old) = fixture();
    for (name, fields, values) in [
        (
            "Mods",
            vec![("Id", "string", false, None)],
            json!([{"Id":"Mod"},{"Id":"Unbound"}]),
        ),
        (
            "SkillGems",
            vec![
                ("BaseItemType", "foreignrow", false, Some("BaseItemTypes")),
                ("GemEffects", "foreignrow", true, Some("GemEffects")),
            ],
            json!([{"BaseItemType":2,"GemEffects":[0]}]),
        ),
        (
            "GemEffects",
            vec![
                ("Id", "string", false, None),
                ("GrantedEffect", "foreignrow", false, Some("GrantedEffects")),
            ],
            json!([{"Id":"GemEffect","GrantedEffect":0}]),
        ),
        (
            "GrantedEffects",
            vec![("Id", "string", false, None)],
            json!([{"Id":"TargetEffect"}]),
        ),
    ] {
        f.documents
            .insert(format!("tables/{name}.json"), table(name, &fields, values));
    }
    f.seal();
    let extra = json!({"ModGrantedSkills":table("ModGrantedSkills",&[("Mod","foreignrow",false,Some("Mods")),("Skill","foreignrow",false,Some("SkillGems"))],json!([{"Mod":0,"Skill":0},{"Mod":1,"Skill":0}]))});
    (f, old, extra)
}
fn linked_args(f: &Fixture, old: &Value, t: &Value) -> Args {
    let mut a = args(f, old);
    a.supplement = Some(supplement(f, t));
    a
}
#[test]
fn target_join_replaces_only_owned_fields_and_records_unbound_links() {
    let (f, mut old, t) = linked_fixture();
    old["variants"][0]["unknown"] = json!([null, 7]);
    run(linked_args(&f, &old, &t)).unwrap();
    let doc: Value =
        crate::read_json(&f.root.join("out/overlay/item_granted_skills.json")).unwrap();
    let mut expected = old["variants"].clone();
    expected[0]["skill_name"] = json!("Gem");
    expected[0]["granted_effect_id"] = json!("TargetEffect");
    assert_eq!(doc["variants"], expected);
    assert_eq!(doc["_meta"]["legacy_metadata"], old["_meta"]);
    let r: Value = crate::read_json(&f.root.join("out/producer-receipt.json")).unwrap();
    assert_eq!(
        r["conversion"]["variant_links"]["unbound_source_rows"],
        json!([1])
    );
    assert_eq!(
        r["conversion"]["variant_links"]["records"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let data = pobr_gamedata::GameData::from_memory(BTreeMap::from([(
        "overlay/item_granted_skills.json".into(),
        fs::read(f.root.join("out/overlay/item_granted_skills.json")).unwrap(),
    )]));
    assert_eq!(
        data.item_granted_skills().unwrap().variants[0].granted_effect_id,
        "TargetEffect"
    );
}
#[test]
fn changed_source_effect_and_name_propagate_without_old_value_gate() {
    let (mut f, old, t) = linked_fixture();
    f.values("BaseItemTypes", 2)["Name"] = json!("Changed skill");
    f.values("GrantedEffects", 0)["Id"] = json!("ChangedEffect");
    f.seal();
    run(linked_args(&f, &old, &t)).unwrap();
    let doc: Value =
        crate::read_json(&f.root.join("out/overlay/item_granted_skills.json")).unwrap();
    assert_eq!(doc["variants"][0]["skill_name"], "Changed skill");
    assert_eq!(doc["variants"][0]["granted_effect_id"], "ChangedEffect");
    assert_eq!(doc["variants"][0]["exported_effect_id"], "Historic");
}
#[test]
fn unresolved_removed_ambiguous_or_multi_effect_selected_links_reject() {
    for mode in 0..7 {
        let (mut f, old, mut t) = linked_fixture();
        match mode {
            0 => t["ModGrantedSkills"]["rows"][0]["values"]["Mod"] = json!(99),
            1 => t["ModGrantedSkills"]["rows"][0]["values"]["Skill"] = json!(99),
            2 => f.values("SkillGems", 0)["GemEffects"] = json!([0, 0]),
            3 => f.values("GemEffects", 0)["GrantedEffect"] = json!(null),
            4 => t["ModGrantedSkills"]["rows"][0]["values"]["Mod"] = json!(1),
            5 => t["ModGrantedSkills"]["rows"][1]["values"]["Mod"] = json!(0),
            _ => {
                t["ModGrantedSkills"]["columns"][1]["schema"]["references"] =
                    json!({"table":"Stats"})
            }
        };
        f.seal();
        assert!(run(linked_args(&f, &old, &t)).is_err(), "mode {mode}");
        assert!(!f.root.join("out").exists());
    }
}
#[test]
fn unbound_ordered_multiple_effects_are_evidence_not_invented_variants() {
    let (mut f, old, mut t) = linked_fixture();
    let mut row = f.documents["tables/SkillGems.json"]["rows"][0].clone();
    row["_row"] = json!(1);
    row["values"]["GemEffects"] = json!([0, 0]);
    let doc = f.documents.get_mut("tables/SkillGems.json").unwrap();
    doc["rows"].as_array_mut().unwrap().push(row);
    doc["validation"]["rows"] = json!(2);
    f.seal();
    t["ModGrantedSkills"]["rows"][1]["values"]["Skill"] = json!(1);
    run(linked_args(&f, &old, &t)).unwrap();
    let r: Value = crate::read_json(&f.root.join("out/producer-receipt.json")).unwrap();
    assert_eq!(
        r["conversion"]["variant_links"]["records"][1]["effects"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let doc: Value =
        crate::read_json(&f.root.join("out/overlay/item_granted_skills.json")).unwrap();
    assert_eq!(doc["variants"].as_array().unwrap().len(), 1);
}
