use super::{refresh, render::Renderer, source::Facts};
use crate::native_facts::reader::{self, VerifiedFacts};
use crate::ordinary_facts::{stat_set_codec::tests::table, tests::Fixture};
use crate::raw_source::sha256_hex;
use serde_json::{Value, json};
use std::fs;

fn hash(v: &impl serde::Serialize) -> String {
    sha256_hex(&reader::json_bytes(v).unwrap())
}
fn descriptor(ids: &[&str], template: &str) -> Value {
    json!({"kind":"description","ids":ids,"raw_slice_sha256":"fixture","groups":[{
        "language_origin":"implicit-default","declared_language":null,"branches":[{
            "condition_tokens_raw":ids.iter().map(|_|"#").collect::<Vec<_>>(),"template_raw":template,
            "template_syntax_diagnostics":[],"quantifiers_lexical":[],"quantifier_tokens_raw":[],"restrictions_raw":[],"span":{}}]}]})
}
pub(super) fn descriptions() -> Value {
    json!({"files":{
        "Data/StatDescriptions/passive_skill_stat_descriptions.csd":{
            "path":"Data/StatDescriptions/passive_skill_stat_descriptions.csd","records":[
                {"kind":"include","target":"Data/StatDescriptions/stat_descriptions.csd"}]},
        "Data/StatDescriptions/stat_descriptions.csd":{
            "path":"Data/StatDescriptions/stat_descriptions.csd","records":[
                descriptor(&["base_fire_damage_resistance_%"],"{0:+d}% to Fire Resistance"),
                descriptor(&["spell_damage_+%"],"{0}% increased Spell Damage")]}}})
}
pub(super) fn fixture() -> (Fixture, Value, Value, Value) {
    let mut f = super::tests::fixture();
    let passive = f.documents.get_mut("tables/PassiveSkills.json").unwrap();
    for field in [
        "IsNotable",
        "IsKeystone",
        "IsJewelSocket",
        "IsAscendancyStartingNode",
        "IsJustIcon",
    ] {
        let n = passive["columns"].as_array().unwrap().len();
        passive["columns"].as_array_mut().unwrap().push(json!({"source_column":n,"physical_fields":[field],"schema":{"name":field,"type":"bool","array":false,"interval":false,"references":null}}));
        passive["physical_fields"]
            .as_array_mut()
            .unwrap()
            .push(json!(field));
        for row in passive["rows"].as_array_mut().unwrap() {
            row["values"][field] = json!(false);
        }
    }
    // Use a second distinct stat: repeated IDs must never become partial native packets.
    let stat = f.documents.get_mut("tables/Stats.json").unwrap();
    stat["rows"]
        .as_array_mut()
        .unwrap()
        .push(json!({"_row":2,"values":{"Id":"spell_damage_+%"}}));
    stat["validation"]["rows"] = json!(3);
    f.values("PassiveSkills", 2)["Stats"] = json!([0, 2]);
    f.documents.insert(
        "tables/PassiveSkillTrees.json".into(),
        table(
            "PassiveSkillTrees",
            &[
                ("Id", "string", false, None),
                ("PassiveSkillGraph", "string", false, None),
            ],
            json!([{"Id":"Default","PassiveSkillGraph":"Metadata/PassiveSkillGraph"}]),
        ),
    );
    let instances:Vec<_>=[0,1,2].into_iter().map(|i| {
        let r=&f.documents["tables/PassiveSkills.json"]["rows"][i]["values"];
        json!({"definition":{"row":i,"id":r["Id"]},"graph_id":r["PassiveSkillGraphId"],"group":0,"radius":0,"position":0,
            "connections":if i==0 {vec![303]}else{vec![]},"splines":if i==0 {vec![0]}else{vec![]}})
    }).collect();
    f.documents.insert(
        "passives.json".into(),
        json!({"definitions":[],"class_overrides":[],"default_graph_instances":instances}),
    );
    let graph_nodes: Vec<_> = instances
        .iter()
        .map(|i| {
            let mut n = i.clone();
            n.as_object_mut().unwrap().remove("definition");
            n.as_object_mut().unwrap().remove("group");
            n
        })
        .collect();
    f.documents.insert("graph.json".into(),json!({"tree":{"id":"Default","row":0},"raw_resource":"Metadata/PassiveSkillGraph.psg","header":[3,0],
        "skills_per_orbit":[1],"roots":[{"graph_id":101,"unknown":0}],"groups":[{"group":0,"x":10.0,"y":20.0,"flag":0,"unknown1":0,"unknown2":0,"nodes":graph_nodes}],
        "parsed_bytes":f.manifest["source_identity"]["raw_resources"]["Metadata/PassiveSkillGraph.psg"]["bytes"],"radius_semantics":"opaque","warnings":[]}));
    f.seal();
    let mut base: Value =
        serde_json::from_slice(&fs::read(f.root.join("policy/policy.json")).unwrap()).unwrap();
    base["target"]["facts_manifest_sha256"] = json!(f.args().expected_manifest_sha256);
    base["qualification"]["context_sha256"] = json!(hash(
        &super::source::context(f.values("PassiveSkills", 0)).unwrap()
    ));
    base["legacy_id_bindings"] = json!([]);
    base["legacy_connection_exceptions"] = json!([]);
    let mut legacy = json!([
        {"skill":101,"id":"maximum_resistances1","name":"old","kind":"normal","stats":["+5% to Fire Resistance"],"connections":[],"unknown":{"keep":true}},
        {"skill":202,"id":"Unknown","name":"Unknown","kind":"normal","stats":["unparsed old complete effect"],"connections":[]},
        {"skill":303,"id":"Multi","name":"Multi","kind":"normal","stats":["stale old stat"],"connections":[]}
    ]);
    for n in legacy.as_array_mut().unwrap() {
        n["group"] = json!(1);
        n["orbit"] = json!(0);
        n["orbit_index"] = json!(0);
        n["x"] = json!(10.0);
        n["y"] = json!(20.0);
    }
    let bindings:serde_json::Map<_,_>=f.documents["tables/PassiveSkills.json"]["rows"].as_array().unwrap().iter().map(|r|{
        let raw=&r["values"];let stats:Vec<_>=raw["Stats"].as_array().unwrap().iter().enumerate().map(|(i,s)|
            json!([f.documents["tables/Stats.json"]["rows"][s.as_u64().unwrap() as usize]["values"]["Id"],raw[format!("Stat{}Value",i+1)]])).collect();
        (raw["Id"].as_str().unwrap().to_owned(),json!({"context_sha256":hash(&refresh::context(raw).unwrap()),"stats_sha256":hash(&stats)}))
    }).collect();
    let policy = json!({"schema":"pobr-ordinary-target-tree-policy/v1","base_policy":base,"reference_sha256":"0".repeat(64),
        "descriptions_sha256":"0".repeat(64),"descriptions_manifest_sha256":"0".repeat(64),"compatibility_bindings":bindings,"excluded":{},"class_bindings":[],"statement":"fixture compatibility"});
    let mut reference = json!({"tree":"Default","nodes":{},"groups":{"1":{"x":10.0,"y":20.0}}});
    for id in [101, 202, 303] {
        reference["nodes"][id.to_string()] =
            json!({"skill":id,"group":1,"orbit":0,"orbitIndex":0,"x":10.0,"y":20.0});
    }
    (f, policy, legacy, reference)
}
pub(super) fn input(f: &mut Fixture) -> VerifiedFacts {
    f.seal();
    let mut input = reader::open(
        &f.root.join("input"),
        &f.args().expected_manifest_sha256,
        "0.5.5.5",
        &super::REFRESH_TABLES,
    )
    .unwrap();
    reader::read_tree(&f.root.join("input"), &mut input).unwrap();
    input
}
fn project(
    f: &mut Fixture,
    policy: &Value,
    old: &Value,
    reference: &Value,
) -> Result<(Value, Value, Value), String> {
    let input = input(f);
    let policy: refresh::Policy = serde_json::from_value(policy.clone()).unwrap();
    let facts = Facts::read(&input, &policy.base_policy)?;
    let descriptions = descriptions();
    let renderer = Renderer::new(&descriptions)?;
    refresh::project(&input, &facts, &policy, old, reference, &renderer, None)
}
fn edit_node(f: &mut Fixture, id: u64, edit: impl Fn(&mut Value)) {
    for n in f.documents.get_mut("graph.json").unwrap()["groups"][0]["nodes"]
        .as_array_mut()
        .unwrap()
    {
        if n["graph_id"] == id {
            edit(n);
        }
    }
    for n in f.documents.get_mut("passives.json").unwrap()["default_graph_instances"]
        .as_array_mut()
        .unwrap()
    {
        if n["graph_id"] == id {
            edit(n);
        }
    }
}

#[test]
fn complete_multistat_values_and_name_reach_ordinary_text_without_changing_math() {
    let (mut f, p, old, r) = fixture();
    f.values("PassiveSkills", 2)["Name"] = json!("Renamed from source");
    f.values("PassiveSkills", 2)["Stat1Value"] = json!(19);
    f.values("PassiveSkills", 2)["Stat2Value"] = json!(31);
    let (tree, _, receipt) = project(&mut f, &p, &old, &r).unwrap();
    assert_eq!(tree[2]["name"], "Renamed from source");
    assert_eq!(
        tree[2]["stats"],
        json!(["+19% to Fire Resistance", "31% increased Spell Damage"])
    );
    assert!(tree[2]["native_stats"].is_null());
    assert_eq!(tree[0]["unknown"], old[0]["unknown"]);
    assert_eq!(receipt["target_multistat_nodes"], 1);
}
#[test]
fn target_edges_membership_and_group_centres_propagate() {
    let (mut f, p, old, r) = fixture();
    edit_node(&mut f, 101, |n| {
        n["connections"] = json!([202, 303]);
        n["splines"] = json!([0, 0]);
    });
    f.documents.get_mut("graph.json").unwrap()["groups"][0]["x"] = json!(35.0);
    let (tree, _, _) = project(&mut f, &p, &old, &r).unwrap();
    assert_eq!(tree[0]["connections"], json!([202, 303]));
    assert_eq!(tree[0]["x"], 35.0);
    // Remove an unallocated source member and its edge; the prepared node must not survive.
    edit_node(&mut f, 101, |n| {
        n["connections"] = json!([303]);
        n["splines"] = json!([0]);
    });
    f.documents.get_mut("graph.json").unwrap()["groups"][0]["nodes"]
        .as_array_mut()
        .unwrap()
        .retain(|n| n["graph_id"] != 202);
    f.documents.get_mut("passives.json").unwrap()["default_graph_instances"]
        .as_array_mut()
        .unwrap()
        .retain(|n| n["graph_id"] != 202);
    let (tree, _, receipt) = project(&mut f, &p, &old, &r).unwrap();
    assert_eq!(tree.as_array().unwrap().len(), 2);
    assert_eq!(receipt["removed_prepared_skills"], json!([202]));
}
#[test]
fn new_normal_source_member_requires_explicit_geometry_and_then_is_emitted() {
    let (mut f, p, old, mut r) = fixture();
    // A previously uninstantiated physical definition with ordinary context joins the graph.
    let n = json!({"graph_id":1304,"radius":0,"position":0,"connections":[101],"splines":[0]});
    f.documents.get_mut("graph.json").unwrap()["groups"][0]["nodes"]
        .as_array_mut()
        .unwrap()
        .push(n.clone());
    let mut instance = n;
    instance["group"] = json!(0);
    instance["definition"] = json!({"id":"Trailing ","row":12});
    f.documents.get_mut("passives.json").unwrap()["default_graph_instances"]
        .as_array_mut()
        .unwrap()
        .push(instance);
    assert!(
        project(&mut f, &p, &old, &r)
            .unwrap_err()
            .contains("missing physical orbit-offset")
    );
    r["nodes"]["1304"] = json!({"skill":1304,"group":1,"orbit":0,"orbitIndex":0,"x":10.0,"y":20.0});
    let (tree, _, _) = project(&mut f, &p, &old, &r).unwrap();
    assert_eq!(tree.as_array().unwrap().len(), 4);
    assert_eq!(tree[3]["id"], "Trailing ");
    assert_eq!(tree[3]["stats"], json!(["-7% to Fire Resistance"]));
    assert_eq!(tree[3]["x"], 10.0);
}

#[test]
fn changed_unrenderable_values_or_context_cannot_reuse_old_numbers() {
    let (mut f, p, old, r) = fixture();
    assert_eq!(
        project(&mut f, &p, &old, &r).unwrap().0[1]["stats"],
        old[1]["stats"]
    );
    f.values("PassiveSkills", 1)["Stat1Value"] = json!(99);
    assert!(
        project(&mut f, &p, &old, &r)
            .unwrap_err()
            .contains("stale whole-text")
    );
    f.values("PassiveSkills", 1)["Stat1Value"] = json!(-7);
    f.values("PassiveSkills", 2)["Opaque"] = json!(999);
    assert!(
        project(&mut f, &p, &old, &r)
            .unwrap_err()
            .contains("context change")
    );
}
#[test]
fn topology_and_geometry_conflicts_are_explicit_errors() {
    let (mut f, p, old, r) = fixture();
    edit_node(&mut f, 101, |n| {
        n["connections"] = json!([65500]);
        n["splines"] = json!([0]);
    });
    assert!(
        project(&mut f, &p, &old, &r)
            .unwrap_err()
            .contains("unresolved graph edge")
    );
    edit_node(&mut f, 101, |n| {
        n["connections"] = json!([]);
        n["splines"] = json!([]);
        n["radius"] = json!(3);
    });
    assert!(
        project(&mut f, &p, &old, &r)
            .unwrap_err()
            .contains("changed orbit/slot")
    );
}
#[test]
fn exclusions_are_source_bound_and_edges_to_layout_are_recorded() {
    let (mut f, mut p, old, r) = fixture();
    let raw = &f.documents["tables/PassiveSkills.json"]["rows"][2]["values"];
    p["excluded"]["Multi"] =
        json!({"definition_sha256":hash(raw),"reason":"fixture non-allocatable"});
    let (tree, _, receipt) = project(&mut f, &p, &old, &r).unwrap();
    assert_eq!(tree.as_array().unwrap().len(), 2);
    assert_eq!(tree[0]["connections"], json!([]));
    assert_eq!(
        receipt["source_records"][0]["excluded_connections"],
        json!([303])
    );
    f.values("PassiveSkills", 2)["Name"] = json!("now real");
    assert!(
        project(&mut f, &p, &old, &r)
            .unwrap_err()
            .contains("non-allocatable compatibility binding changed")
    );
}
#[test]
fn target_notable_category_replaces_prepared_normal() {
    let (mut f, p, old, r) = fixture();
    f.values("PassiveSkills", 2)["IsNotable"] = json!(true);
    assert_eq!(
        project(&mut f, &p, &old, &r).unwrap().0[2]["kind"],
        "notable"
    );
}
#[test]
fn renderer_compound_child_scope_and_unknown_handlers_are_whole_node() {
    let mut d = descriptions();
    d["files"]["Data/StatDescriptions/passive_skill_stat_descriptions.csd"]["records"]
        .as_array_mut()
        .unwrap()
        .push(descriptor(
            &["base_fire_damage_resistance_%", "spell_damage_+%"],
            "{0:+d}% resistance and {1}% spell damage",
        ));
    let r = Renderer::new(&d).unwrap();
    let stats = vec![
        ("base_fire_damage_resistance_%".into(), 7),
        ("spell_damage_+%".into(), 12),
    ];
    assert_eq!(
        r.render(&stats).unwrap().0,
        vec!["+7% resistance and 12% spell damage"]
    );
    assert!(
        r.render(&stats[..1])
            .unwrap_err()
            .contains("incomplete compound")
    );
    d["files"]["Data/StatDescriptions/passive_skill_stat_descriptions.csd"]["records"][1]["groups"]
        [0]["branches"][0]["quantifiers_lexical"] =
        json!([{"name_raw":"mystery","argument_tokens_raw":["1"]}]);
    d["files"]["Data/StatDescriptions/passive_skill_stat_descriptions.csd"]["records"][1]["groups"]
        [0]["branches"][0]["quantifier_tokens_raw"] = json!(["mystery", "1"]);
    assert!(
        Renderer::new(&d)
            .unwrap()
            .render(&stats)
            .unwrap_err()
            .contains("unsupported CSD handler")
    );
}
#[test]
fn renderer_negatives_zero_and_unmatched_branch_are_explicit() {
    let mut d = descriptions();
    let b = &mut d["files"]["Data/StatDescriptions/stat_descriptions.csd"]["records"][1]["groups"]
        [0]["branches"];
    let mut negative = b[0].clone();
    negative["condition_tokens_raw"] = json!(["#|-1"]);
    negative["template_raw"] = json!("{0}% reduced Spell Damage");
    negative["quantifiers_lexical"] = json!([{"name_raw":"negate","argument_tokens_raw":["1"]}]);
    negative["quantifier_tokens_raw"] = json!(["negate", "1"]);
    b[0]["condition_tokens_raw"] = json!(["1|#"]);
    b.as_array_mut().unwrap().push(negative);
    let r = Renderer::new(&d).unwrap();
    assert_eq!(
        r.render(&[("spell_damage_+%".into(), -17)]).unwrap().0,
        vec!["17% reduced Spell Damage"]
    );
    assert!(
        r.render(&[("spell_damage_+%".into(), 0)])
            .unwrap_err()
            .contains("no CSD branch")
    );
    assert_eq!(
        r.render(&[("base_fire_damage_resistance_%".into(), 0)])
            .unwrap()
            .0,
        vec!["+0% to Fire Resistance"]
    );
    d["files"]["Data/StatDescriptions/stat_descriptions.csd"]["records"][0]["groups"][0]["branches"]
        [0]["quantifier_tokens_raw"] = json!(["unparsed_token"]);
    assert!(
        Renderer::new(&d)
            .unwrap()
            .render(&[("base_fire_damage_resistance_%".into(), 0)])
            .unwrap_err()
            .contains("unaccounted CSD handler tokens")
    );
}

#[test]
fn identity_alias_requires_original_entry_and_keeps_new_target_name() {
    let (mut f, mut p, mut old, r) = fixture();
    old[2]["id"] = json!("303");
    p["base_policy"]["legacy_id_bindings"] = json!([{
        "legacy_id":"303","skill":303,"target_passive_id":"Multi",
        "expected_name":"Multi","source_entry_sha256":hash(&old[2]),"role":"fixture alias"
    }]);
    f.values("PassiveSkills", 2)["Name"] = json!("New target name");
    let (tree, _, _) = project(&mut f, &p, &old, &r).unwrap();
    assert_eq!(tree[2]["id"], "Multi");
    assert_eq!(tree[2]["name"], "New target name");
    old[2]["stats"] = json!(["tampered"]);
    assert!(
        project(&mut f, &p, &old, &r)
            .unwrap_err()
            .contains("stale prepared identity alias")
    );
}

#[test]
fn source_class_replacement_value_name_and_removal_propagate_with_extensions() {
    let (mut f, mut p, mut old, r) = fixture();
    f.values("ClassPassiveSkillOverrides", 0)["SkillToOverride"] = json!(2);
    f.values("PassiveSkills", 5)["Stat1Value"] = json!(37);
    f.values("PassiveSkills", 5)["Name"] = json!("Target class choice");
    p["class_bindings"] = json!([{"base_id":"Multi","class":"Witch"}]);
    old[2]["variants"] = json!([{"class":"Witch","variant_skill":606,"name":"old","stats":["+1% to Fire Resistance"],"future":{"keep":true}}]);
    let (tree, _, _) = project(&mut f, &p, &old, &r).unwrap();
    assert_eq!(tree[2]["variants"][0]["name"], "Target class choice");
    assert_eq!(
        tree[2]["variants"][0]["stats"],
        json!(["+37% to Fire Resistance"])
    );
    assert_eq!(
        tree[2]["variants"][0]["future"],
        old[2]["variants"][0]["future"]
    );
    let table = f
        .documents
        .get_mut("tables/ClassPassiveSkillOverrides.json")
        .unwrap();
    table["rows"] = json!([]);
    table["validation"]["rows"] = json!(0);
    table["validation"]["actual_row_bytes"] = json!(0);
    assert!(
        project(&mut f, &p, &old, &r).unwrap().0[2]
            .get("variants")
            .is_none()
    );
}

#[test]
fn supplemental_cli_pair_and_tree_payload_pins_are_enforced() {
    let (mut f, _, _, _) = fixture();
    let error = crate::ordinary_facts::parse_args(
        vec!["--tree-reference".into(), "reference.json".into()].into_iter(),
    )
    .err()
    .unwrap();
    assert!(error.contains("both tree-reference and tree-descriptions"));
    let mut verified = input(&mut f);
    fs::write(f.root.join("input/graph.json"), b"{}").unwrap();
    assert!(
        reader::read_tree(&f.root.join("input"), &mut verified)
            .unwrap_err()
            .contains("fingerprint mismatch")
    );
}

#[test]
fn target_centres_translate_prepared_offsets_without_promoting_coordinate_less_nodes() {
    let (mut f, p, mut old, r) = fixture();
    old[0]["x"] = json!(9.0); // Deliberately different from reference physical offset.
    old[1].as_object_mut().unwrap().remove("x");
    old[1].as_object_mut().unwrap().remove("y");
    f.documents.get_mut("graph.json").unwrap()["groups"][0]["x"] = json!(110.0);
    let (tree, _, _) = project(&mut f, &p, &old, &r).unwrap();
    assert_eq!(tree[0]["x"], 109.0);
    assert_eq!(tree[0]["y"], 20.0);
    assert!(!tree[1].as_object().unwrap().contains_key("x"));
    assert!(!tree[1].as_object().unwrap().contains_key("y"));
}

#[test]
fn class_target_swap_cannot_borrow_the_previous_targets_fallback_text() {
    let (mut f, mut p, mut old, r) = fixture();
    f.values("PassiveSkills", 6)["Stats"] = json!([1]);
    p["compatibility_bindings"]["Variant"]["stats_sha256"] =
        json!(hash(&json!([["unknown_stat", -7]])));
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
            json!([
                {"CharacterToOverrideFor":1,"SkillToOverride":0,"Override":6},
                {"CharacterToOverrideFor":1,"SkillToOverride":2,"Override":5}
            ]),
        ),
    );
    p["class_bindings"] = json!([
        {"base_id":"maximum_resistances1","class":"Witch"},
        {"base_id":"Multi","class":"Witch"}
    ]);
    old[0]["variants"] =
        json!([{"class":"Witch","variant_skill":707,"stats":["unknown target's complete text"]}]);
    old[2]["variants"] =
        json!([{"class":"Witch","variant_skill":606,"stats":["+5% to Fire Resistance"]}]);
    let baseline = project(&mut f, &p, &old, &r).unwrap().0;
    assert_eq!(
        baseline[0]["variants"][0]["stats"],
        old[0]["variants"][0]["stats"]
    );
    // Both targets have valid semantic bindings, but their old text is not interchangeable.
    f.values("ClassPassiveSkillOverrides", 0)["Override"] = json!(5);
    f.values("ClassPassiveSkillOverrides", 1)["Override"] = json!(6);
    assert!(
        project(&mut f, &p, &old, &r)
            .unwrap_err()
            .contains("no complete compatibility text")
    );
    // A changed target with complete source rendering remains legal.
    let table = f
        .documents
        .get_mut("tables/ClassPassiveSkillOverrides.json")
        .unwrap();
    table["rows"].as_array_mut().unwrap().pop();
    table["validation"]["rows"] = json!(1);
    let changed = project(&mut f, &p, &old, &r).unwrap().0;
    assert_eq!(changed[0]["variants"][0]["variant_skill"], 606);
    assert_eq!(
        changed[0]["variants"][0]["stats"],
        json!(["-7% to Fire Resistance"])
    );
}

#[test]
fn compound_csd_cannot_consume_a_members_child_owner_or_ambiguity() {
    let mut d = descriptions();
    let child = "Data/StatDescriptions/passive_skill_stat_descriptions.csd";
    let parent = "Data/StatDescriptions/stat_descriptions.csd";
    d["files"][parent]["records"] = json!([descriptor(&["a", "b"], "{0} and {1}")]);
    let mut override_b = descriptor(&["b"], "{0} child");
    override_b["groups"][0]["branches"][0]["quantifiers_lexical"] =
        json!([{"name_raw":"unknown","argument_tokens_raw":["1"]}]);
    override_b["groups"][0]["branches"][0]["quantifier_tokens_raw"] = json!(["unknown", "1"]);
    d["files"][child]["records"]
        .as_array_mut()
        .unwrap()
        .push(override_b.clone());
    for stats in [
        vec![("a".into(), 1), ("b".into(), 7)],
        vec![("b".into(), 7), ("a".into(), 1)],
    ] {
        assert!(
            Renderer::new(&d).unwrap().render(&stats).is_err(),
            "child ownership must hold for both stat orders"
        );
    }
    // Even a supported child or ambiguous child cannot be silently consumed by its parent.
    override_b["groups"][0]["branches"][0]["quantifiers_lexical"] = json!([]);
    override_b["groups"][0]["branches"][0]["quantifier_tokens_raw"] = json!([]);
    d["files"][child]["records"][1] = override_b.clone();
    assert!(
        Renderer::new(&d)
            .unwrap()
            .render(&[("a".into(), 1), ("b".into(), 7)])
            .is_err()
    );
    d["files"][child]["records"]
        .as_array_mut()
        .unwrap()
        .push(override_b);
    assert!(
        Renderer::new(&d)
            .unwrap()
            .render(&[("a".into(), 1), ("b".into(), 7)])
            .is_err()
    );
}
