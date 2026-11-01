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
    refresh::project(
        &input,
        &facts,
        &policy,
        old,
        r,
        &renderer,
        Some(refresh::Supplement {
            ascendancy: &source,
            input: extra,
        }),
    )
}

fn unlock_fixture() -> (Fixture, Value, Value, Value, selected::Input) {
    let (mut f, mut policy, old, reference, extra) = fixture();
    let doc = f.documents.get_mut("tables/PassiveSkills.json").unwrap();
    for (field, kind, array, target) in [
        ("UnlockedBy", "row", true, "PassiveSkills"),
        ("VisibleForAscendancy", "foreignrow", false, "Ascendancy"),
        ("Requirement", "foreignrow", false, "ClientStrings"),
    ] {
        let n = doc["columns"].as_array().unwrap().len();
        doc["columns"].as_array_mut().unwrap().push(json!({"source_column":n,"physical_fields":[field],
            "schema":{"name":field,"type":kind,"array":array,"interval":false,"references":{"table":target}}}));
        doc["physical_fields"]
            .as_array_mut()
            .unwrap()
            .push(json!(field));
        for row in doc["rows"].as_array_mut().unwrap() {
            row["values"][field] = if array { json!([]) } else { Value::Null };
        }
    }
    policy["schema"] = json!("pobr-ordinary-target-tree-policy/v3");
    policy["base_policy"]["qualification"]["context_sha256"] = json!(sha256_hex(
        &reader::json_bytes(&super::super::source::context(f.values("PassiveSkills", 0)).unwrap())
            .unwrap()
    ));
    for row in doc_rows(&f) {
        let raw = &row["values"];
        let mut context = refresh::context(raw).unwrap();
        for field in ["Ascendancy", "UnlockedBy", "VisibleForAscendancy"] {
            context.as_object_mut().unwrap().remove(field);
        }
        policy["compatibility_bindings"][raw["Id"].as_str().unwrap()]["context_sha256"] =
            json!(sha256_hex(&reader::json_bytes(&context).unwrap()));
    }
    (f, policy, old, reference, extra)
}

fn doc_rows(f: &Fixture) -> &[Value] {
    f.documents["tables/PassiveSkills.json"]["rows"]
        .as_array()
        .unwrap()
}

fn text_fixture() -> (Fixture, Value, Value, Value, selected::Input) {
    let (mut f, mut policy, old, reference, mut extra) = unlock_fixture();
    let doc = f.documents.get_mut("tables/PassiveSkills.json").unwrap();
    for (field, kind, array, target, value) in [
        (
            "GrantedSkill",
            "foreignrow",
            false,
            Some("SkillGems"),
            Value::Null,
        ),
        ("SkillPointsGranted", "i32", false, None, json!(0)),
        ("WeaponPointsGranted", "i32", false, None, json!(0)),
        (
            "PassiveSkillBuffs",
            "foreignrow",
            true,
            Some("BuffTemplates"),
            json!([]),
        ),
    ] {
        let n = doc["columns"].as_array().unwrap().len();
        doc["columns"].as_array_mut().unwrap().push(json!({"source_column":n,"physical_fields":[field],
            "schema":{"name":field,"type":kind,"array":array,"interval":false,"references":target.map(|t|json!({"table":t}))}}));
        doc["physical_fields"]
            .as_array_mut()
            .unwrap()
            .push(json!(field));
        for row in doc["rows"].as_array_mut().unwrap() {
            row["values"][field] = value.clone();
        }
    }
    extra.documents.insert(
        "tables/SkillGems.json".into(),
        table(
            "SkillGems",
            &[("BaseItemType", "foreignrow", false, Some("BaseItemTypes"))],
            json!([{"BaseItemType":1},{"BaseItemType":0}]),
        ),
    );
    extra.documents.insert("tables/BaseItemTypes.json".into(), table("BaseItemTypes", &[
        ("Id", "string", false, None),("Name", "string", false, None)], json!([
            {"Id":"Metadata/OtherSkill","Name":"Other source skill"},{"Id":"Metadata/GrantedSkill","Name":"Source skill {0}"}])));
    extra.documents.insert(
        "tables/ClientStrings.json".into(),
        table(
            "ClientStrings",
            &[
                ("Id", "string", false, None),
                ("Text", "string", false, None),
            ],
            json!([
            {"Id":"PassiveNodeGrantsPassivePoint","Text":"Grants {0} Passive Skill Point"},
            {"Id":"PassiveNodeGrantsPassivePoints","Text":"Grants {0} Passive Skill Points"},
            {"Id":"PassiveNodeGrantsSpecialisationPoint","Text":"{0} point becomes a weapon point"},
            {"Id":"PassiveNodeGrantsSpecialisationPoints","Text":"{0} points become weapon points"},
            {"Id":"ItemDisplayGrantedSkillNoScaling","Text":"Grants Skill: <underline>{{{0}}}"}]),
        ),
    );
    for table in ["ClientStrings", "SkillGems", "BaseItemTypes"] {
        let resource = format!("Data/Balance/{table}.datc64");
        extra.manifest["source_identity"]["raw_resources"][&resource] =
            f.manifest["source_identity"]["raw_resources"][&resource].clone();
    }
    policy["schema"] = json!("pobr-ordinary-target-tree-policy/v4");
    policy["non_stat_manifest_sha256"] = json!(sha256_hex(&extra.manifest_bytes));
    bind_text_contexts(&mut f, &mut policy);
    (f, policy, old, reference, extra)
}

fn bind_text_contexts(f: &mut Fixture, policy: &mut Value) {
    policy["base_policy"]["qualification"]["context_sha256"] = json!(sha256_hex(
        &reader::json_bytes(&super::super::source::context(f.values("PassiveSkills", 0)).unwrap())
            .unwrap()
    ));
    for row in doc_rows(f) {
        let raw = &row["values"];
        let mut context = refresh::context(raw).unwrap();
        for field in ["Ascendancy", "UnlockedBy", "VisibleForAscendancy"] {
            context.as_object_mut().unwrap().remove(field);
        }
        let binding = &mut policy["compatibility_bindings"][raw["Id"].as_str().unwrap()];
        binding["context_sha256"] = json!(sha256_hex(&reader::json_bytes(&context).unwrap()));
        for field in super::super::nonstat::OWNED_FIELDS {
            context.as_object_mut().unwrap().remove(field);
        }
        binding["render_context_sha256"] =
            json!(sha256_hex(&reader::json_bytes(&context).unwrap()));
    }
}

#[test]
fn complete_stats_points_and_skill_name_grants_propagate_together() {
    let (mut f, p, old, r, mut extra) = text_fixture();
    f.values("PassiveSkills", 2)["SkillPointsGranted"] = json!(2);
    f.values("PassiveSkills", 2)["WeaponPointsGranted"] = json!(1);
    f.values("PassiveSkills", 2)["GrantedSkill"] = json!(0);
    let (tree, _, receipt) = project(&mut f, &p, &old, &r, &extra).unwrap();
    assert_eq!(
        tree[2]["stats"],
        json!([
            "-7% to Fire Resistance",
            "22% increased Spell Damage",
            "Grants 2 Passive Skill Points",
            "1 point becomes a weapon point",
            "Grants Skill: <underline>{Source skill {0}}"
        ])
    );
    assert_eq!(
        receipt["source_records"][2]["text"]["owner"],
        "target-csd-and-client-strings"
    );
    assert_eq!(
        receipt["source_records"][2]["text"]["non_stat"][2]["base_item_row"],
        1
    );
    value(&mut extra, "BaseItemTypes", 0)["Name"] = json!("Renamed source skill");
    f.values("PassiveSkills", 2)["GrantedSkill"] = json!(1);
    f.values("PassiveSkills", 2)["SkillPointsGranted"] = json!(1);
    f.values("PassiveSkills", 2)["WeaponPointsGranted"] = json!(0);
    let (tree, _, _) = project(&mut f, &p, &old, &r, &extra).unwrap();
    assert_eq!(
        tree[2]["stats"],
        json!([
            "-7% to Fire Resistance",
            "22% increased Spell Damage",
            "Grants 1 Passive Skill Point",
            "Grants Skill: <underline>{Renamed source skill}"
        ])
    );
    f.values("PassiveSkills", 2)["GrantedSkill"] = Value::Null;
    f.values("PassiveSkills", 2)["SkillPointsGranted"] = json!(0);
    let (tree, _, _) = project(&mut f, &p, &old, &r, &extra).unwrap();
    assert_eq!(
        tree[2]["stats"],
        json!(["-7% to Fire Resistance", "22% increased Spell Damage"])
    );
}

#[test]
fn buffs_or_unknown_stats_keep_whole_text_and_reject_changed_grants() {
    for row in [1, 2] {
        let (mut f, mut p, old, r, extra) = text_fixture();
        f.values("PassiveSkills", row)["GrantedSkill"] = json!(0);
        if row == 2 {
            f.values("PassiveSkills", row)["PassiveSkillBuffs"] = json!([0]);
        }
        bind_text_contexts(&mut f, &mut p);
        let (tree, _, receipt) = project(&mut f, &p, &old, &r, &extra).unwrap();
        assert_eq!(tree[row]["stats"], old[row]["stats"]);
        assert_eq!(
            receipt["source_records"][row]["text"]["owner"],
            "prepared-whole-text"
        );
        f.values("PassiveSkills", row)["GrantedSkill"] = json!(1);
        let error = project(&mut f, &p, &old, &r, &extra).unwrap_err();
        assert!(
            error.contains("stale whole-text compatibility rejected"),
            "row {row}: {error}"
        );
    }
}

#[test]
fn tree_text_requires_same_raw_source_pinned_supplement_and_resolved_skill_fk() {
    for mode in 0..4 {
        let (mut f, p, old, r, mut extra) = text_fixture();
        match mode {
            0 => {
                extra.manifest["source_identity"]["raw_resources"]["Data/Balance/SkillGems.datc64"]
                    ["sha256"] = json!("0".repeat(64))
            }
            1 => extra.manifest_bytes = b"another source identity".to_vec(),
            2 => f.values("PassiveSkills", 2)["GrantedSkill"] = json!(99),
            _ => f.values("PassiveSkills", 2)["SkillPointsGranted"] = json!(-1),
        }
        assert!(
            project(&mut f, &p, &old, &r, &extra).is_err(),
            "mode {mode}"
        );
    }
}

#[test]
fn source_unlock_members_order_visibility_and_removal_propagate() {
    let (mut f, p, mut old, r, mut extra) = unlock_fixture();
    old[2]["unlock_constraint"] = json!({"nodes":[101],"ascendancy":"Witch3","future":false});
    f.values("PassiveSkills", 2)["UnlockedBy"] = json!([1, 0]);
    f.values("PassiveSkills", 2)["VisibleForAscendancy"] = json!(0);
    value(&mut extra, "Ascendancy", 0)["Id"] = json!("SourceAscendancy");
    let (tree, _, receipt) = project(&mut f, &p, &old, &r, &extra).unwrap();
    assert_eq!(
        tree[2]["unlock_constraint"],
        json!({"nodes":[202,101],"ascendancy":"SourceAscendancy","future":false})
    );
    assert_eq!(
        receipt["source_records"][2]["unlock"]["nodes"][0]["target_row"],
        1
    );
    assert!(tree[2]["native_stats"].is_null());
    let doc = extra.documents.get_mut("tables/Ascendancy.json").unwrap();
    doc["rows"].as_array_mut().unwrap().push(json!({"_row":1,"values":{
        "Id":"OtherSourceAscendancy","Name":"Other source owner","Character":0,"Disabled":false,"BaseAscendancy":null}}));
    doc["validation"]["rows"] = json!(2);
    f.values("PassiveSkills", 2)["UnlockedBy"] = json!([0]);
    f.values("PassiveSkills", 2)["VisibleForAscendancy"] = json!(1);
    let (tree, _, _) = project(&mut f, &p, &old, &r, &extra).unwrap();
    assert_eq!(
        tree[2]["unlock_constraint"],
        json!({"nodes":[101],"ascendancy":"OtherSourceAscendancy","future":false})
    );
    f.values("PassiveSkills", 2)["UnlockedBy"] = json!([]);
    f.values("PassiveSkills", 2)["VisibleForAscendancy"] = Value::Null;
    let (tree, _, _) = project(&mut f, &p, &old, &r, &extra).unwrap();
    assert_eq!(tree[2]["unlock_constraint"], json!({"future":false}));
    old[2].as_object_mut().unwrap().remove("unlock_constraint");
    let (tree, _, _) = project(&mut f, &p, &old, &r, &extra).unwrap();
    assert!(tree[2].get("unlock_constraint").is_none());
}

#[test]
fn unresolved_unlock_nodes_owners_and_wrong_column_types_reject() {
    for mode in 0..5 {
        let (mut f, p, old, r, extra) = unlock_fixture();
        match mode {
            0 => f.values("PassiveSkills", 2)["UnlockedBy"] = json!([100]),
            // Physical row exists, but its graph node is not an emitted member.
            1 => f.values("PassiveSkills", 2)["UnlockedBy"] = json!([3]),
            2 => f.values("PassiveSkills", 2)["VisibleForAscendancy"] = json!(100),
            3 => f.values("PassiveSkills", 2)["UnlockedBy"] = json!(null),
            _ => {
                let doc = f.documents.get_mut("tables/PassiveSkills.json").unwrap();
                let column = doc["columns"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|c| c["physical_fields"] == json!(["UnlockedBy"]))
                    .unwrap();
                column["schema"]["references"]["table"] = json!("ClientStrings");
            }
        }
        assert!(
            project(&mut f, &p, &old, &r, &extra).is_err(),
            "mode {mode}"
        );
    }
}

#[test]
fn requirement_and_unknown_context_never_become_unlock_predicates() {
    for field in ["Requirement", "Opaque"] {
        let (mut f, p, old, r, extra) = unlock_fixture();
        f.values("PassiveSkills", 2)[field] = json!(99);
        let error = project(&mut f, &p, &old, &r, &extra).unwrap_err();
        assert!(
            error.contains("unsupported source context change"),
            "{field}: {error}"
        );
    }
}

#[test]
fn source_unlock_ownership_never_waives_unrepresentable_variant_context() {
    for field in ["UnlockedBy", "VisibleForAscendancy"] {
        let (mut f, p, old, r, extra) = unlock_fixture();
        f.values("PassiveSkills", 5)[field] = if field == "UnlockedBy" {
            json!([0])
        } else {
            json!(0)
        };
        let error = project(&mut f, &p, &old, &r, &extra).unwrap_err();
        assert!(
            error.contains("variant unlock context cannot be represented"),
            "{field}: {error}"
        );
    }
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
