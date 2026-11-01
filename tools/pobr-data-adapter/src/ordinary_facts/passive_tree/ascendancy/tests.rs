use super::super::{refresh_tests, render::Renderer};
use super::*;
use crate::ordinary_facts::{stat_set_codec::tests::table, tests::Fixture};

fn fixture() -> (Fixture, Value, Value, Value, selected::Input) {
    let (mut f, mut policy, mut old, reference) = refresh_tests::fixture();
    let doc = f.documents.get_mut("tables/PassiveSkills.json").unwrap();
    let n = doc["columns"].as_array().unwrap().len();
    doc["columns"].as_array_mut().unwrap().push(json!({"source_column":n,"physical_fields":["Ascendancy"],"schema":{"name":"Ascendancy","type":"foreignrow","array":false,"interval":false,"references":{"table":"Ascendancy"}}}));
    doc["physical_fields"]
        .as_array_mut()
        .unwrap()
        .push(json!("Ascendancy"));
    for row in doc["rows"].as_array_mut().unwrap() {
        row["values"]["Ascendancy"] = Value::Null;
    }
    policy["schema"] = json!("pobr-ordinary-target-tree-policy/v2");
    policy["ascendancy"] = json!({"excluded":{}});
    policy["base_policy"]["qualification"]["context_sha256"] = json!(sha256_hex(
        &reader::json_bytes(&super::super::source::context(f.values("PassiveSkills", 0)).unwrap())
            .unwrap()
    ));
    f.values("PassiveSkills", 2)["Ascendancy"] = json!(0);
    old[2]["variants"] = json!([{"class":"Lich","variant_skill":606,"name":"old","stats":["old effect"],"future":7}]);
    let a = table(
        "Ascendancy",
        &[
            ("Id", "string", false, None),
            ("Name", "string", false, None),
            ("Character", "foreignrow", false, Some("Characters")),
            ("Disabled", "bool", false, None),
            ("BaseAscendancy", "row", false, Some("Ascendancy")),
        ],
        json!([
            {"Id":"Witch3","Name":"Lich","Character":1,"Disabled":false,"BaseAscendancy":null}
        ]),
    );
    let b = table(
        "AscendancyPassiveSkillOverrides",
        &[
            (
                "AscendancyToOverrideFor",
                "foreignrow",
                false,
                Some("Ascendancy"),
            ),
            (
                "SkillToOverride",
                "foreignrow",
                false,
                Some("PassiveSkills"),
            ),
            ("Override", "foreignrow", false, Some("PassiveSkills")),
        ],
        json!([
            {"AscendancyToOverrideFor":0,"SkillToOverride":2,"Override":5}
        ]),
    );
    let extra = selected::Input {
        manifest: json!({"fixture":true}),
        manifest_bytes: b"fixture".to_vec(),
        documents: BTreeMap::from([
            ("tables/Ascendancy.json".into(), a),
            ("tables/AscendancyPassiveSkillOverrides.json".into(), b),
        ]),
    };
    (f, policy, old, reference, extra)
}
fn value<'a>(extra: &'a mut selected::Input, table: &str, row: usize) -> &'a mut Value {
    &mut extra
        .documents
        .get_mut(&format!("tables/{table}.json"))
        .unwrap()["rows"][row]["values"]
}
fn project(
    f: &mut Fixture,
    p: &Value,
    old: &Value,
    r: &Value,
    extra: &selected::Input,
) -> Result<(Value, Value, Value), String> {
    let input = refresh_tests::input(f);
    let policy: refresh::Policy = serde_json::from_value(p.clone()).unwrap();
    let facts = Facts::read(&input, &policy.base_policy)?;
    let source = Source::read(
        &input,
        &facts,
        extra,
        policy.ascendancy.as_ref().unwrap(),
        &policy.base_policy.legacy_metadata.value,
    )?;
    let descriptions = refresh_tests::descriptions();
    let renderer = Renderer::new(&descriptions)?;
    refresh::project(&input, &facts, &policy, old, r, &renderer, Some(&source))
}
#[test]
fn source_names_parent_node_and_override_values_propagate() {
    let (mut f, p, old, r, mut extra) = fixture();
    value(&mut extra, "Ascendancy", 0)["Name"] = json!("Renamed Lich");
    value(&mut extra, "Ascendancy", 0)["Character"] = json!(0);
    f.values("PassiveSkills", 5)["Stat1Value"] = json!(17);
    f.values("PassiveSkills", 5)["Name"] = json!("Source variant");
    let (tree, meta, receipt) = project(&mut f, &p, &old, &r, &extra).unwrap();
    assert_eq!(tree[2]["ascendancy_id"], "Witch3");
    assert_eq!(
        tree[2]["variants"][0],
        json!({"class":"Renamed Lich","variant_skill":606,"name":"Source variant","stats":["+17% to Fire Resistance"],"future":7})
    );
    assert!(tree[2]["native_stats"].is_null());
    assert!(meta["classes"][0]["ascendancies"].is_null());
    assert_eq!(
        meta["classes"][1]["ascendancies"][0],
        json!({"id":"Witch3","name":"Renamed Lich","future":17})
    );
    assert_eq!(
        receipt["ascendancy"]["relationships"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn relationship_removal_and_new_source_member_are_real() {
    let (mut f, p, old, r, mut extra) = fixture();
    let doc = extra
        .documents
        .get_mut("tables/AscendancyPassiveSkillOverrides.json")
        .unwrap();
    doc["rows"] = json!([]);
    doc["validation"]["rows"] = json!(0);
    doc["validation"]["actual_row_bytes"] = json!(0);
    let doc = extra.documents.get_mut("tables/Ascendancy.json").unwrap();
    doc["rows"].as_array_mut().unwrap().push(json!({"_row":1,"values":{"Id":"NewAsc","Name":"Source new member","Character":0,"Disabled":false,"BaseAscendancy":null}}));
    doc["validation"]["rows"] = json!(2);
    f.values("PassiveSkills", 2)["Ascendancy"] = json!(1);
    let (tree, meta, _) = project(&mut f, &p, &old, &r, &extra).unwrap();
    assert_eq!(tree[2]["ascendancy_id"], "NewAsc");
    assert!(tree[2]["variants"].is_null());
    assert_eq!(meta["classes"][1]["ascendancies"][0]["id"], "NewAsc");
}
#[test]
fn changed_unrenderable_replacement_never_inherits_previous_text() {
    let (mut f, p, old, r, mut extra) = fixture();
    value(&mut extra, "AscendancyPassiveSkillOverrides", 0)["Override"] = json!(1);
    let error = project(&mut f, &p, &old, &r, &extra).unwrap_err();
    assert!(error.contains("no complete compatibility text"), "{error}");
}
#[test]
fn dangling_or_duplicate_relationships_and_unavailable_nodes_reject() {
    for mode in 0..5 {
        let (mut f, p, old, r, mut extra) = fixture();
        match mode {
            0 => value(&mut extra, "Ascendancy", 0)["Character"] = json!(100),
            1 => value(&mut extra, "AscendancyPassiveSkillOverrides", 0)["Override"] = json!(100),
            2 => value(&mut extra, "Ascendancy", 0)["Disabled"] = json!(true),
            3 => f.values("PassiveSkills", 2)["Ascendancy"] = json!(100),
            _ => {
                let doc = extra
                    .documents
                    .get_mut("tables/AscendancyPassiveSkillOverrides.json")
                    .unwrap();
                let mut row = doc["rows"][0].clone();
                row["_row"] = json!(1);
                doc["rows"].as_array_mut().unwrap().push(row);
                doc["validation"]["rows"] = json!(2);
            }
        }
        assert!(
            project(&mut f, &p, &old, &r, &extra).is_err(),
            "mode {mode}"
        );
    }
}
#[test]
fn exact_placeholder_exclusion_cannot_hide_changed_or_removed_source() {
    let (mut f, mut p, old, r, mut extra) = fixture();
    let raw = json!({"Id":"Unused","Name":"[DNT-UNUSED] Placeholder","Character":0,"Disabled":false,"BaseAscendancy":null});
    p["ascendancy"]["excluded"]["Unused"] = json!({"definition_sha256":sha256_hex(&reader::json_bytes(&raw).unwrap()),"reason":"fixed placeholder"});
    let doc = extra.documents.get_mut("tables/Ascendancy.json").unwrap();
    doc["rows"]
        .as_array_mut()
        .unwrap()
        .push(json!({"_row":1,"values":raw}));
    doc["validation"]["rows"] = json!(2);
    project(&mut f, &p, &old, &r, &extra).unwrap();
    value(&mut extra, "Ascendancy", 1)["Name"] = json!("Now released");
    assert!(
        project(&mut f, &p, &old, &r, &extra)
            .unwrap_err()
            .contains("changed ascendancy eligibility")
    );
}
