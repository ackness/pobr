use super::*;
use crate::ordinary_facts::{reader, stat_set_codec::tests::table, tests::Fixture};

fn fixture() -> (Fixture, Policy, NativeMetadataFile, BTreeMap<String, Value>) {
    let mut f = Fixture::new();
    f.documents.insert("tables/Characters.json".into(), table("Characters", &[
        ("Id","string",false,None),("BaseMaxLife","i32",false,None),("BaseMaxMana","i32",false,None)],
        json!([{"Id":"First","BaseMaxLife":41,"BaseMaxMana":55},{"Id":"Second","BaseMaxLife":41,"BaseMaxMana":55}])));
    for name in ["PassiveSkills", "ClassPassiveSkillOverrides"] {
        let mut value = table(name, &[("Id", "string", false, None)], json!([]));
        value["validation"]["actual_row_bytes"] = json!(0);
        f.documents.insert(format!("tables/{name}.json"), value);
    }
    f.seal();
    let p: Value = serde_json::from_slice(include_bytes!(
        "../../../../../../pipeline/runtime-constants/0.5.5.5-with-player-baselines-policy.json"
    ))
    .unwrap();
    let policy: Policy = serde_json::from_value(p["player"].clone()).unwrap();
    let ids: BTreeSet<_> = policy
        .bindings
        .iter()
        .filter_map(|b| match &b.source {
            Source::GenericStat { id } => Some(id.as_str()),
            _ => None,
        })
        .collect();
    let assignments:Vec<_>=ids.into_iter().map(|id|json!({"key_raw":id,"value_raw":"19","line":1,
        "scope":[{"id":1,"name_raw":"Stats","open_line":1}],"plain_stats_scope":true,"value_byte_start":0,"value_byte_end":0})).collect();
    let metadata:NativeMetadataFile=serde_json::from_value(json!({"format":"synthetic","path":NATIVE_GENERIC_OT,"bytes":0,"encoding":"UTF-16LE","merge_policy":"none",
        "records":[],"extends":[],"diagnostics":[],"assignments":assignments})).unwrap();
    let mut docs: BTreeMap<String, Value> = serde_json::from_slice(include_bytes!(
        "../../../../tests/fixtures/ordinary-facts/runtime-constant-templates.json"
    ))
    .unwrap();
    docs.get_mut("character_constants.json").unwrap()["unknown"] = json!([null, {"raw":7}]);
    (f, policy, metadata, docs)
}
fn facts(f: &Fixture) -> VerifiedFacts {
    let a = f.args();
    reader::open(
        &a.input,
        &a.expected_manifest_sha256,
        &a.version,
        &super::super::super::unarmed::TABLES,
    )
    .unwrap()
}
fn run_project(
    f: &Fixture,
    p: &Policy,
    m: &NativeMetadataFile,
    d: &mut BTreeMap<String, Value>,
) -> Result<Vec<Value>, String> {
    let shapes = d
        .iter()
        .map(|(name, value)| Ok((name.clone(), super::super::numeric_fields(name, value)?)))
        .collect::<Result<_, String>>()?;
    project(&facts(f), m, p, d, &shapes)
}
#[test]
fn source_coefficients_reach_existing_loader_and_character_formulas() {
    let (f, p, m, mut d) = fixture();
    let original = d.clone();
    assert_eq!(run_project(&f, &p, &m, &mut d).unwrap().len(), 16);
    let g = pobr_gamedata::GameData::from_memory(
        d.iter()
            .map(|(name, value)| (format!("base/{name}"), serde_json::to_vec(value).unwrap()))
            .collect(),
    );
    let c = g.character_constants().unwrap();
    let actor = pobr_core::CharacterBase {
        level: 10,
        strength: 0.0,
        dexterity: 0.0,
        intelligence: 0.0,
    };
    assert_eq!(actor.base_life(&c), 231.0);
    assert_eq!(actor.base_mana(&c), 245.0);
    assert_eq!(actor.base_accuracy(&c), 171.0);
    assert_eq!(c.base_evasion, 19.0);
    assert_eq!(
        g.game_constants()
            .unwrap()
            .character
            .base_critical_hit_damage_bonus,
        19.0
    );
    let mut retained = original;
    for b in &p.bindings {
        let (o, k) =
            super::super::leaf_parent(retained.get_mut(&b.file).unwrap(), &b.target_pointer)
                .unwrap();
        o.insert(k, d[&b.file].pointer(&b.target_pointer).unwrap().clone());
    }
    assert_eq!(retained, d);
}
#[test]
fn uniform_class_changes_and_source_stat_changes_propagate_without_old_value_gate() {
    let (mut f, p, mut m, mut d) = fixture();
    for row in 0..2 {
        f.values("Characters", row)["BaseMaxLife"] = json!(99);
    }
    f.seal();
    m.assignments
        .iter_mut()
        .find(|a| a.key_raw == "life_per_level")
        .unwrap()
        .value_raw = "27".into();
    run_project(&f, &p, &m, &mut d).unwrap();
    assert_eq!(d["character_constants.json"]["base_life_constant"], 99.0);
    assert_eq!(d["character_constants.json"]["life_per_level"], 27.0);
    f.values("Characters", 1)["BaseMaxLife"] = json!(100);
    f.seal();
    assert!(
        run_project(&f, &p, &m, &mut d)
            .unwrap_err()
            .contains("disagreement")
    );
}
#[test]
fn missing_duplicate_nonliteral_and_non_plain_assignments_reject() {
    for change in 0..4 {
        let (f, p, mut m, mut d) = fixture();
        let index = m
            .assignments
            .iter()
            .position(|a| a.key_raw == "life_per_level")
            .unwrap();
        match change {
            0 => {
                m.assignments.remove(index);
            }
            1 => m.assignments.push(m.assignments[index].clone()),
            2 => m.assignments[index].value_raw = "12 + level".into(),
            _ => m.assignments[index].plain_stats_scope = false,
        }
        assert!(run_project(&f, &p, &m, &mut d).is_err());
    }
}
#[test]
fn policy_cannot_overlap_game_bindings_or_invent_destinations() {
    let (f, mut p, m, mut d) = fixture();
    let files = BTreeSet::from(["character_constants.json", "game_constants.json"]);
    let first = &p.bindings[0];
    let mut dest = BTreeSet::from([(&first.file, &first.target_pointer)]);
    assert!(
        p.validate(&files, &mut dest)
            .unwrap_err()
            .contains("duplicate")
    );
    p.bindings[0].target_pointer = "/unknown/numeric".into();
    assert!(
        run_project(&f, &p, &m, &mut d)
            .unwrap_err()
            .contains("unknown/non-numeric")
    );
}
