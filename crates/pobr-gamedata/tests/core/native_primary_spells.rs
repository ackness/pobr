//! Portable synthetic selection/evidence tests, without real source certification.

use super::*;
use pobr_gamedata::NativeData;

mod fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/native_primary_spells.rs"
    ));
}

const PROJECTION: &str = "base/native_skill_projection.json";
const FIRE: &str = "Metadata/Items/Gem/SkillGemFireball";
const SPARK: &str = "Metadata/Items/Gems/SkillGemSpark";

fn primary_pack() -> BTreeMap<String, Vec<u8>> {
    fixture::extend(player_pack())
}

fn rejects(path: &str, change: impl FnOnce(&mut Value)) {
    let mut files = primary_pack();
    fixture::edit(&mut files, path, change);
    fixture::reseal(&mut files);
    assert!(
        NativeData::load(&GameData::from_memory(files)).is_err(),
        "{path}"
    );
}

#[test]
fn primary_inventory_loads_disk_and_memory_and_preserves_authored_evidence() {
    let files = primary_pack();
    let disk = DiskPack::new(&files);
    for data in [
        GameData::new(disk.version()),
        GameData::from_memory(files.clone()),
        GameData::from_memory_releasing(files, true),
    ] {
        let native = NativeData::load(&data).unwrap();
        let p = native.primary_spells().unwrap();
        assert_eq!(p.selections.len(), 3);
        assert_eq!(p.selections[0].gem_id, FIRE);
        assert_eq!(p.selections[2].gem_levels, [1, 2, 10, 40]);
        assert_eq!(
            p.retained_evidence[FIRE]["active_reverse_granted_effect"],
            ""
        );
        assert_eq!(
            p.retained_evidence[SPARK]["additional_stat_sets"],
            json!(["SparkColdInfusedPlayer"])
        );
        assert_eq!(
            p.retained_evidence[FIRE]["retained_outside_projection"]["CooldownGroup"],
            67
        );
        assert!(data.validate_calculation_admission().is_err());
    }
    assert!(
        NativeData::load(&GameData::from_memory(player_pack()))
            .unwrap()
            .primary_spells()
            .is_none()
    );
}

#[test]
fn primary_inventory_rejects_catalog_identity_and_complete_level_disagreement() {
    for (path, pointer, value) in [
        (
            PROJECTION,
            "/format",
            json!("pobr-native-skill-projection/v1"),
        ),
        (PROJECTION, "/primary_only", json!(false)),
        (PROJECTION, "/quality", json!(1)),
        (PROJECTION, "/selections/0/gem_id", json!("display name")),
        (PROJECTION, "/selections/0/gem_levels", json!([1, 2, 10])),
        (
            PROJECTION,
            "/selections/0/gem_levels",
            json!([0, 1, 2, 10, 40]),
        ),
        (
            PROJECTION,
            "/selections/0/gem_levels",
            json!([1, 2, 2, 10, 40]),
        ),
        ("base/skill_gems.json", "/0/gem_effect_id", json!("Spark")),
        (
            "base/skill_gems.json",
            "/0/granted_effect_id",
            json!("SparkPlayer"),
        ),
        (
            "base/granted_effects.json",
            "/0/active_skill",
            json!("spark"),
        ),
        ("base/granted_effects.json", "/0/stat_set", json!(31)),
        (
            "base/granted_effect_stat_sets.json",
            "/0/sets/0/set_id",
            json!("FireballExplosionPlayer"),
        ),
        (
            "base/granted_effect_levels.json",
            "/FireballPlayer/1/level",
            json!(1),
        ),
        (
            "base/granted_effect_stat_sets.json",
            "/0/sets/0/levels/1/gem_level",
            json!(1),
        ),
        (
            "base/native_input_lock.json",
            "/primary_gems",
            json!([SPARK]),
        ),
        (
            "base/native_reference_index.json",
            "/skill_selection",
            json!([]),
        ),
        (
            "base/native_coverage.json",
            "/projected/primary_spells",
            json!([]),
        ),
    ] {
        rejects(path, |v| *v.pointer_mut(pointer).unwrap() = value);
    }
    for path in [
        "base/base_items.json",
        "base/skill_gems.json",
        "base/granted_effects.json",
        "base/granted_effect_stat_sets.json",
    ] {
        rejects(path, |v| {
            let row = v[0].clone();
            v.as_array_mut().unwrap().push(row);
        });
    }
    rejects("base/granted_effect_levels.json", |v| {
        v["FireballPlayer"].as_array_mut().unwrap().pop();
    });
    rejects("base/granted_effect_stat_sets.json", |v| {
        v[0]["sets"][0]["levels"].as_array_mut().unwrap().pop();
    });
    rejects("base/granted_effect_stat_sets.json", |v| {
        let extra = v[0]["sets"][0].clone();
        v[0]["sets"].as_array_mut().unwrap().push(extra);
    });
}

#[test]
fn primary_evidence_rejects_unknown_duplicate_physical_and_retained_identities() {
    for (field, value) in [
        ("active_reverse_granted_effect", json!("SparkPlayer")),
        ("additional_stat_sets", json!(["FireballPlayer"])),
        ("additional_stat_sets", json!(["x", "x"])),
        ("additional_stat_sets", json!([""])),
        ("additional_stat_sets", json!([7431])),
        ("unknown", json!(0)),
    ] {
        rejects(PROJECTION, |v| v["retained_evidence"][FIRE][field] = value);
    }
    for (pointer, value) in [
        ("/rows/0/payload", json!("tables/SkillGems.json")),
        ("/rows/5/pointer", json!("/rows/31/values")),
        ("/levels/0/gem_level", json!(2)),
        ("/levels/1/rows/0/pointer", json!("/rows/1/values")),
        ("/levels/0/rows/0/unknown", json!(0)),
    ] {
        rejects(PROJECTION, |v| {
            let evidence = &mut v["retained_evidence"][FIRE];
            if pointer.ends_with("unknown") {
                evidence["levels"][0]["rows"][0]["unknown"] = value;
            } else {
                *evidence.pointer_mut(pointer).unwrap() = value;
            }
        });
    }
    rejects(PROJECTION, |v| {
        v["retained_evidence"][FIRE]["levels"]
            .as_array_mut()
            .unwrap()
            .pop();
    });
    rejects(PROJECTION, |v| {
        v["retained_evidence"][SPARK]["rows"][3]["pointer"] = "/rows/0/values".into()
    });
    let mut files = primary_pack();
    let text = String::from_utf8(files[PROJECTION].clone()).unwrap();
    files.insert(
        PROJECTION.into(),
        text.replacen("\"quality\":0", "\"quality\":0,\"qu\\u0061lity\":0", 1)
            .into_bytes(),
    );
    fixture::reseal(&mut files);
    assert!(NativeData::load(&GameData::from_memory(files)).is_err());
}

#[test]
fn primary_forward_joins_allow_shared_active_rows_and_empty_excluded_inventory() {
    let mut files = primary_pack();
    fixture::edit(&mut files, PROJECTION, |v| {
        for selection in v["selections"].as_array_mut().unwrap() {
            selection["active_skill_id"] = "shared-active".into();
        }
        for e in v["retained_evidence"].as_object_mut().unwrap().values_mut() {
            e["rows"][4]["pointer"] = "/rows/9/values".into();
            e["active_reverse_granted_effect"] = "".into();
            e["additional_stat_sets"] = json!([]);
        }
    });
    fixture::edit(&mut files, "base/granted_effects.json", |v| {
        for e in v.as_array_mut().unwrap() {
            e["active_skill"] = "shared-active".into();
        }
    });
    let p: Value = serde_json::from_slice(&files[PROJECTION]).unwrap();
    fixture::edit(&mut files, "base/native_reference_index.json", |v| {
        v["skill_selection"] = p["selections"].clone()
    });
    fixture::edit(&mut files, "base/native_coverage.json", |v| {
        v["projected"]["primary_spells"] = p["selections"].clone()
    });
    fixture::reseal(&mut files);
    NativeData::load(&GameData::from_memory(files.clone())).unwrap();
    fixture::edit(&mut files, PROJECTION, |v| {
        v["retained_evidence"][FIRE]["active_reverse_granted_effect"] = "FireballPlayer".into();
    });
    fixture::reseal(&mut files);
    assert!(
        NativeData::load(&GameData::from_memory(files)).is_err(),
        "shared physical ActiveSkills row must retain one reverse-evidence value"
    );
}

#[test]
fn primary_profile_keeps_strict_passive_validator_and_exact_policy_tuple() {
    rejects("base/passive_tree.json", |v| {
        v[0]["native_stats"]["stats"][0]["provenance"]["stat_row"] = 1001.into()
    });
    rejects("base/passive_tree.json", |v| {
        v[0]["native_stats"]["stats"][0]["provenance"]["passive_row"] = 12.into()
    });
    for (path, pointer, value) in [
        (
            NATIVE_RUNTIME_PATH,
            "/output_profile",
            json!("conditional-skill-hit-v1"),
        ),
        (
            NATIVE_RUNTIME_PATH,
            "/input_profile",
            json!("sorceress-spark-level1-q0-ordinary-damage-passives-v1"),
        ),
        (
            NATIVE_RUNTIME_PATH,
            "/normalization/profile",
            json!("spark-level1-q0"),
        ),
        (
            NATIVE_RUNTIME_PATH,
            "/rules/profile",
            json!("spark-level1-q0-ordinary-damage-passives"),
        ),
        (
            "base/native_actor_binding.json",
            "/policy",
            json!("selected-player-generic-ot-v1"),
        ),
        (
            "base/native_actor_binding.json",
            "/evaluation",
            json!("conditional-skill-hit-v1"),
        ),
        (
            "base/native_actor_binding.json",
            "/required_conditions/1",
            json!("primary-SparkPlayer-level1-q0"),
        ),
    ] {
        rejects(path, |v| *v.pointer_mut(pointer).unwrap() = value);
    }
    let mut old = player_pack();
    fixture::edit(&mut old, NATIVE_RUNTIME_PATH, |v| {
        v["output_profile"] = "conditional-skill-hit-v2".into()
    });
    reseal_player(&mut old);
    assert!(NativeData::load(&GameData::from_memory(old)).is_err());
}
