use super::*;
use crate::ordinary_facts::{
    Args, CodecArgs, Domain, parse_args, reader, run, stat_set_codec::tests::table, tests::Fixture,
};
use std::fs;

mod links;

fn fixture() -> (Fixture, Value) {
    let mut f = Fixture::new();
    f.documents.insert(
        "tables/BaseItemTypes.json".into(),
        table(
            "BaseItemTypes",
            &[
                ("Id", "string", false, None),
                ("Name", "string", false, None),
            ],
            json!([
        {"Id":"A","Name":"New A"},{"Id":"B","Name":"New B"},{"Id":"Gem","Name":"Gem"}]),
        ),
    );
    f.documents.insert(
        "tables/SkillGems.json".into(),
        table(
            "SkillGems",
            &[("BaseItemType", "foreignrow", false, Some("BaseItemTypes"))],
            json!([{"BaseItemType":2}]),
        ),
    );
    f.documents.insert("tables/ItemInherentSkills.json".into(),table("ItemInherentSkills",&[("BaseItemType","foreignrow",false,Some("BaseItemTypes")),("SkillsGranted","foreignrow",true,Some("SkillGems")),("IsWeapon","bool",false,None)],json!([
        {"BaseItemType":0,"SkillsGranted":[0,0],"IsWeapon":true},{"BaseItemType":1,"SkillsGranted":[],"IsWeapon":false}])));
    f.seal();
    let old = json!({"_meta":{"old":"exact"},"reservation_free_bases":["Stale"],"future":[null,{"key":7}],"variants":[{
        "item_name":"Unique","base_names":["Armour"],"skill_name":"Skill","granted_effect_id":"Native","exported_effect_id":"Historic","game_mod_id":"Mod"}]});
    (f, old)
}
fn facts(f: &Fixture) -> VerifiedFacts {
    let a = f.args();
    reader::open(&a.input, &a.expected_manifest_sha256, &a.version, &TABLES).unwrap()
}
#[test]
fn complete_membership_replaces_old_list_and_preserves_all_other_values() {
    let (f, old) = fixture();
    let (doc, records) = project(&facts(&f), &old).unwrap();
    let mut expected = old;
    expected["reservation_free_bases"] = json!(["New A"]);
    assert_eq!(doc, expected);
    assert_eq!(records.len(), 2);
    assert_eq!(records[0]["skills"].as_array().unwrap().len(), 2);
    let data = pobr_gamedata::GameData::from_memory(BTreeMap::from([
        (
            "overlay/item_granted_skills.json".into(),
            super::super::pretty_bytes(&doc).unwrap(),
        ),
        (
            "overlay-common/item_granted_skills.json".into(),
            br#"{"reservation_free_bases":["Stale"]}"#.to_vec(),
        ),
    ]));
    let loaded = data.item_granted_skills().unwrap();
    assert_eq!(loaded.reservation_free_bases, vec!["New A"]);
    assert_eq!(loaded.variants.len(), 1);
}
#[test]
fn source_rename_true_false_and_empty_membership_propagate() {
    let (mut f, old) = fixture();
    f.values("BaseItemTypes", 0)["Name"] = json!("Renamed");
    f.values("ItemInherentSkills", 1)["IsWeapon"] = json!(true);
    f.seal();
    assert_eq!(
        project(&facts(&f), &old).unwrap().0["reservation_free_bases"],
        json!(["New B", "Renamed"])
    );
    for i in 0..2 {
        f.values("ItemInherentSkills", i)["IsWeapon"] = json!(false);
    }
    f.seal();
    assert_eq!(
        project(&facts(&f), &old).unwrap().0["reservation_free_bases"],
        json!([])
    );
}
#[test]
fn dangling_duplicate_ambiguous_and_malformed_physical_values_reject() {
    for case in 0..7 {
        let (mut f, old) = fixture();
        match case {
            0 => f.values("ItemInherentSkills", 0)["BaseItemType"] = json!(99),
            1 => f.values("ItemInherentSkills", 1)["BaseItemType"] = json!(0),
            2 => f.values("ItemInherentSkills", 0)["SkillsGranted"] = json!([99]),
            3 => f.values("ItemInherentSkills", 0)["IsWeapon"] = json!(1),
            4 => f.values("BaseItemTypes", 1)["Name"] = json!("New A"),
            5 => f.values("BaseItemTypes", 0)["Name"] = json!(""),
            _ => {
                f.documents
                    .get_mut("tables/ItemInherentSkills.json")
                    .unwrap()["columns"][2]["source_column"] = json!(1)
            }
        };
        f.seal();
        assert!(project(&facts(&f), &old).is_err(), "case {case}");
    }
}
fn args(f: &Fixture, old: &Value) -> Args {
    let dir = f.root.join("legacy");
    fs::create_dir(&dir).unwrap();
    let pol = f.root.join("policy");
    fs::create_dir(&pol).unwrap();
    let bytes = super::super::pretty_bytes(old).unwrap();
    fs::write(dir.join("old.json"), &bytes).unwrap();
    let mut policy: Value = serde_json::from_slice(include_bytes!(
        "../../../../../pipeline/item-grants/0.5.5.5-community-semantics-policy.json"
    ))
    .unwrap();
    policy["source_sha256"] = json!(sha256_hex(&bytes));
    let pb = super::super::pretty_bytes(&policy).unwrap();
    fs::write(pol.join("policy.json"), &pb).unwrap();
    Args {
        domain: Domain::ItemGrants,
        codec: Some(CodecArgs {
            source: dir.join("old.json"),
            source_sha256: sha256_hex(&bytes),
            policy: pol.join("policy.json"),
            policy_sha256: sha256_hex(&pb),
        }),
        ..f.args()
    }
}
#[test]
fn full_converter_archives_both_sources_and_retains_legacy_metadata() {
    let (f, old) = fixture();
    let a = args(&f, &old);
    let raw = fs::read(&a.codec.as_ref().unwrap().source).unwrap();
    run(a).unwrap();
    let doc: Value =
        crate::read_json(&f.root.join("out/overlay/item_granted_skills.json")).unwrap();
    assert_eq!(doc["_meta"]["legacy_metadata"], old["_meta"]);
    assert_eq!(doc["variants"], old["variants"]);
    assert_eq!(doc["future"], old["future"]);
    assert_eq!(
        fs::read(f.root.join("out/legacy-item-grants.json")).unwrap(),
        raw
    );
    let receipt: Value = crate::read_json(&f.root.join("out/producer-receipt.json")).unwrap();
    assert_eq!(
        receipt["conversion"]["interpretation_policy"]["raw_field"],
        "IsWeapon"
    );
    assert_eq!(
        receipt["conversion"]["interpretation_policy"]["community_schema"]["spec_name"],
        "NoReservation"
    );
}
#[test]
fn policy_interpretation_and_source_pins_are_not_implicit() {
    for field in [
        "raw_schema_revision",
        "raw_field",
        "interpreted_semantic",
        "source_sha256",
    ] {
        let (f, old) = fixture();
        let mut a = args(&f, &old);
        let c = a.codec.as_mut().unwrap();
        let mut p: Value = crate::read_json(&c.policy).unwrap();
        p[field] = json!("wrong");
        let bytes = super::super::pretty_bytes(&p).unwrap();
        fs::write(&c.policy, &bytes).unwrap();
        c.policy_sha256 = sha256_hex(&bytes);
        assert!(run(a).unwrap_err().contains("identity/interpretation"));
        assert!(!f.root.join("out").exists());
    }
    let (f, old) = fixture();
    let mut a = args(&f, &old);
    a.codec.as_mut().unwrap().source_sha256 = "0".repeat(64);
    assert!(run(a).is_err());
}
#[test]
fn cli_requires_explicit_source_and_policy() {
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
        "item-grants",
    ];
    assert!(parse_args(base.iter().map(|s| s.to_string())).is_err());
    let mut all = base.to_vec();
    all.extend([
        "--codec-source",
        "/s",
        "--codec-source-sha256",
        "p",
        "--codec-policy",
        "/p",
        "--codec-policy-sha256",
        "p",
    ]);
    assert_eq!(
        parse_args(all.iter().map(|s| s.to_string()))
            .unwrap()
            .domain,
        Domain::ItemGrants
    );
}
