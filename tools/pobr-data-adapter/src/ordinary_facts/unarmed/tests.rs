use super::*;
use crate::ordinary_facts::{reader, stat_set_codec::tests::table, tests::Fixture};
use pobr_core::skill_env::{UnarmedDataLookup, unarmed_contribution};

fn fixture() -> (Fixture, Policy, Value, NativeMetadataFile) {
    let mut f = Fixture::new();
    f.documents.insert("tables/Characters.json".into(),table("Characters",&[
        ("Id","string",false,None),("Name","string",false,None),("IntegerId","i32",false,None),
        ("WeaponSpeed","i32",false,None),("MinDamage","i32",false,None),("MaxDamage","i32",false,None)],
        json!([
            {"Id":"Metadata/A","Name":"A","IntegerId":0,"WeaponSpeed":606,"MinDamage":2,"MaxDamage":8},
            {"Id":"Metadata/B","Name":"B","IntegerId":1,"WeaponSpeed":500,"MinDamage":0,"MaxDamage":4}])));
    for name in ["PassiveSkills", "ClassPassiveSkillOverrides"] {
        f.documents.insert(
            format!("tables/{name}.json"),
            table(name, &[("Id", "string", false, None)], json!([])),
        );
    }
    for name in ["PassiveSkills", "ClassPassiveSkillOverrides"] {
        f.documents.get_mut(&format!("tables/{name}.json")).unwrap()["validation"]["actual_row_bytes"] =
            json!(0);
    }
    f.seal();
    let historical = json!({"class_id":0,"class_name":"Historical","weapon_type":"None","attack_rate":1.65,"crit_chance":0.05,"physical_min":2.0,"physical_max":6.0});
    let old = json!([
        {"class_id":9,"class_name":"Old A","weapon_type":"None","attack_rate":999.0,"crit_chance":0.2,"physical_min":100.0,"physical_max":200.0,"future":[null,4]},
        historical.clone()]);
    let p:Policy=serde_json::from_value(json!({"schema":"pobr-ordinary-unarmed-policy/v1","version":"0.5.5.5","source_sha256":"0".repeat(64),
        "player_manifest_sha256":"0".repeat(64),"player_source_lock_sha256":"0".repeat(64),
        "legacy_bindings":{"Old A":"Metadata/A"},"legacy_only":{"Historical":historical},"statement":"Synthetic explicit ordinary field binding"})).unwrap();
    let metadata:NativeMetadataFile=serde_json::from_value(json!({"format":"synthetic","path":NATIVE_GENERIC_OT,"bytes":0,"encoding":"UTF-16LE","merge_policy":"none",
        "records":[],"extends":[],"diagnostics":[],"assignments":[{"key_raw":CRIT,"value_raw":"700","line":1,"scope":[{"id":1,"name_raw":"Stats","open_line":1}],
        "plain_stats_scope":true,"value_byte_start":0,"value_byte_end":0}]})).unwrap();
    (f, p, old, metadata)
}
fn facts(f: &Fixture) -> VerifiedFacts {
    let a = f.args();
    reader::open(&a.input, &a.expected_manifest_sha256, &a.version, &TABLES).unwrap()
}
fn load(value: &Value) -> Vec<UnarmedWeaponDef> {
    pobr_gamedata::GameData::from_memory(BTreeMap::from([(
        "base/unarmed_data.json".into(),
        super::super::pretty_bytes(value).unwrap(),
    )]))
    .unarmed_data()
    .unwrap()
}
struct Lookup(Vec<UnarmedWeaponDef>);
impl UnarmedDataLookup for Lookup {
    fn unarmed_for_class(&self, name: &str) -> Option<&UnarmedWeaponDef> {
        self.0.iter().find(|r| r.class_name == name)
    }
}

#[test]
fn source_fields_reach_ordinary_loader_and_existing_weapon_contribution() {
    let (f, p, old, metadata) = fixture();
    let (out, records, crit) = project(&facts(&f), &old, &p, &metadata).unwrap();
    assert_eq!(out[0]["future"], old[0]["future"]);
    assert_eq!(out[2], old[1]);
    assert_eq!(records.len(), 3);
    assert_eq!(crit["raw_integer"], 700);
    let data = Lookup(load(&out));
    let a = unarmed_contribution(&data, "A");
    assert_eq!(
        (a.phys_min, a.phys_max, a.attack_rate),
        (2.0, 8.0, 1000.0 / 606.0)
    );
    assert!((a.crit_chance - 7.0).abs() < 1e-12);
    let b = unarmed_contribution(&data, "B");
    assert_eq!((b.phys_min, b.phys_max, b.attack_rate), (0.0, 4.0, 2.0));
    assert_eq!(data.0[0].class_id, 0);
    assert_eq!(data.0[2].class_id, 0); // Historical ID remains explicitly separate by name.
}

#[test]
fn target_name_values_and_membership_changes_are_not_gated_on_old_values() {
    let (mut f, p, old, mut metadata) = fixture();
    f.values("Characters", 0)["Name"] = json!("Renamed A");
    f.values("Characters", 0)["IntegerId"] = json!(7);
    f.values("Characters", 0)["WeaponSpeed"] = json!(800);
    f.values("Characters", 0)["MaxDamage"] = json!(17);
    metadata.assignments[0].value_raw = "0".into();
    f.seal();
    let (out, _, _) = project(&facts(&f), &old, &p, &metadata).unwrap();
    let data = Lookup(load(&out));
    let a = unarmed_contribution(&data, "Renamed A");
    assert_eq!(
        (a.phys_max, a.attack_rate, a.crit_chance),
        (17.0, 1.25, 0.0)
    );
    assert_eq!(data.0[0].class_id, 7);
    assert!(data.unarmed_for_class("Old A").is_none());
    // A source-only class is emitted; deleting a legacy-bound source is an error.
    f.values("Characters", 0)["Id"] = json!("Metadata/Replaced");
    f.seal();
    assert!(
        project(&facts(&f), &old, &p, &metadata)
            .unwrap_err()
            .contains("missing")
    );
}

#[test]
fn invalid_numeric_source_and_ambiguous_names_are_rejected() {
    for (field, value) in [
        ("WeaponSpeed", json!(0)),
        ("WeaponSpeed", json!(-2)),
        ("MinDamage", json!(9)),
        ("MaxDamage", json!(null)),
        ("IntegerId", json!(1)),
        ("Name", json!("B")),
    ] {
        let (mut f, p, old, metadata) = fixture();
        f.values("Characters", 0)[field] = value;
        f.seal();
        assert!(project(&facts(&f), &old, &p, &metadata).is_err(), "{field}");
    }
}

#[test]
fn no_missing_nonliteral_scoped_or_duplicate_crit_defaults() {
    for change in 0..6 {
        let (f, p, old, mut metadata) = fixture();
        match change {
            0 => metadata.assignments.clear(),
            1 => metadata.assignments.push(metadata.assignments[0].clone()),
            2 => metadata.assignments[0].value_raw = "500 + level".into(),
            3 => metadata.assignments[0].value_raw = "-1".into(),
            4 => metadata.assignments[0].plain_stats_scope = false,
            _ => metadata.path = "Metadata/Other.ot".into(),
        }
        assert!(project(&facts(&f), &old, &p, &metadata).is_err());
    }
}

#[test]
fn legacy_entries_require_exact_exhaustive_single_dispositions() {
    for change in 0..4 {
        let (f, mut p, mut old, metadata) = fixture();
        match change {
            0 => {
                p.legacy_bindings.clear();
            }
            1 => {
                p.legacy_only.insert("Old A".into(), old[0].clone());
            }
            2 => {
                old[1]["physical_max"] = json!(7);
            }
            _ => {
                p.legacy_bindings
                    .insert("Unused".into(), "Metadata/NotHere".into());
            }
        }
        assert!(project(&facts(&f), &old, &p, &metadata).is_err());
    }
}

#[test]
fn player_context_flags_are_paired_and_scoped_to_the_ordinary_unarmed_domain() {
    let base = [
        "--ordinary-facts",
        "facts",
        "--facts-manifest-sha256",
        "pin",
        "--version",
        "0.5.5.5",
        "--out",
        "out",
        "--codec-source",
        "legacy",
        "--codec-source-sha256",
        "pin",
        "--codec-policy",
        "policy",
        "--codec-policy-sha256",
        "pin",
    ];
    let domain = ["--ordinary-domain", "unarmed-data"];
    let context = [
        "--player-context",
        "player",
        "--player-manifest-sha256",
        "pin",
    ];
    assert!(
        super::super::parse_args(
            base.into_iter()
                .chain(domain)
                .chain(context)
                .map(str::to_owned)
        )
        .unwrap()
        .player_context
        .is_some()
    );
    for flags in [&context[..0], &context[..2], &context[2..]] {
        assert!(
            super::super::parse_args(
                base.into_iter()
                    .chain(domain)
                    .chain(flags.iter().copied())
                    .map(str::to_owned)
            )
            .is_err()
        );
    }
    assert!(
        super::super::parse_args(
            base.into_iter()
                .chain(["--ordinary-domain", "base-items"])
                .chain(context)
                .map(str::to_owned)
        )
        .is_err()
    );
}
