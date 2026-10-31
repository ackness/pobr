use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use pobr_data::catalog::NativeRuntimeContract;
use pobr_data::catalog::native_player::{NATIVE_GENERIC_OT, NATIVE_PLAYER_ID};
use pobr_data::source::SourceKind;
use pobr_gamedata::{GameData, manifest::native_assembly_key};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::*;
use crate::native_document::{NativeActorPolicyPin, NativeBuildSchema};
use crate::{Build, BuildConfig, CharacterIdentity, SocketGroup};

const CONTEXT: &str = "base/native_player_context.json";
const BINDING: &str = "base/native_actor_binding.json";
const CONTRACT: &str = "base/native_runtime.json";
const SETS: &str = "base/granted_effect_stat_sets.json";
const LEVELS: &str = "base/granted_effect_levels.json";

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn edit(files: &mut BTreeMap<String, Vec<u8>>, path: &str, change: impl FnOnce(&mut Value)) {
    let mut value = serde_json::from_slice(&files[path]).unwrap();
    change(&mut value);
    files.insert(path.into(), serde_json::to_vec(&value).unwrap());
}

fn read_pack(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let bytes = std::fs::read(root.join("manifest.json")).unwrap();
    let manifest: Value = serde_json::from_slice(&bytes).unwrap();
    let mut files: BTreeMap<_, _> = manifest["files"]
        .as_object()
        .unwrap()
        .keys()
        .map(|p| (p.clone(), std::fs::read(root.join(p)).unwrap()))
        .collect();
    files.insert("manifest.json".into(), bytes);
    files
}

fn v1_pack() -> BTreeMap<String, Vec<u8>> {
    read_pack(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../pobr-gamedata/tests/fixtures/native_runtime"),
    )
}

fn reseal(files: &mut BTreeMap<String, Vec<u8>>) {
    let context: Value = serde_json::from_slice(&files[CONTEXT]).unwrap();
    let context_sha = hash(&files[CONTEXT]);
    edit(files, BINDING, |v| {
        v["context_sha256"] = context_sha.clone().into()
    });
    let binding_sha = hash(&files[BINDING]);
    edit(files, CONTRACT, |v| {
        v["player"] = json!({"context_sha256":context_sha, "actor_binding_sha256":binding_sha,
            "source_manifest_sha256":context["source_manifest_sha256"],
            "source_lock_sha256":context["source_lock_sha256"],
            "parent_manifest_sha256":context["parent_manifest_sha256"],
            "parent_source_lock_sha256":context["parent_source_lock_sha256"]});
        v["tree"]["source_manifest_sha256"] = context["parent_manifest_sha256"].clone();
        v["allowed_source_manifests"] = json!([
            context["source_manifest_sha256"],
            context["parent_manifest_sha256"]
        ]);
    });
    let contract: NativeRuntimeContract = serde_json::from_slice(&files[CONTRACT]).unwrap();
    edit(files, CONTRACT, |v| {
        v["assembly_key"] = native_assembly_key(&contract).into()
    });
    let hashes: BTreeMap<_, _> = files
        .iter()
        .filter(|(p, _)| p.starts_with("base/"))
        .map(|(p, bytes)| (p.clone(), hash(bytes)))
        .collect();
    edit(files, "manifest.json", |v| {
        v["files"] = serde_json::to_value(hashes).unwrap()
    });
}

fn pack() -> BTreeMap<String, Vec<u8>> {
    // Reuse the source worker's complete synthetic player archive rather than copy 6.5k lines.
    let player: Value = serde_json::from_slice(include_bytes!(
        "../../../../tools/pobr-data-adapter/tests/fixtures/native-runtime/player-synthetic.json"
    ))
    .unwrap();
    let skill: Value = serde_json::from_slice(include_bytes!("tests/skill-facts.json")).unwrap();
    let mut files = v1_pack();
    for name in [
        "base_items",
        "skill_gems",
        "granted_effects",
        "granted_effect_levels",
        "granted_effect_stat_sets",
        "cost_types",
    ] {
        files.insert(
            format!("base/{name}.json"),
            serde_json::to_vec(&skill[name]).unwrap(),
        );
    }
    files.insert(
        CONTEXT.into(),
        serde_json::to_vec(&player["context"]).unwrap(),
    );
    files.insert(
        BINDING.into(),
        serde_json::to_vec(&player["binding"]).unwrap(),
    );
    for name in [
        "native_input_lock",
        "native_normalization",
        "native_rule_requirements",
        "native_acquisition",
    ] {
        files.insert(format!("base/{name}.json"), b"{}".to_vec());
    }
    edit(&mut files, CONTRACT, |v| {
        v["schema_version"] = 2.into();
        v["output_profile"] = "conditional-skill-hit-v1".into();
        for profile in ["normalization", "rules"] {
            v[profile]["version"] = 2.into();
            v[profile]["sha256"] = hash(b"{}").into();
        }
        v["source_lock_sha256"] = hash(b"{}").into();
        v["acquisition"]["evidence_sha256"] = hash(b"{}").into();
    });
    edit(&mut files, "manifest.json", |v| {
        for name in [
            "native_player_context",
            "native_actor_binding",
            "native_input_lock",
            "native_normalization",
            "native_rule_requirements",
            "native_acquisition",
        ] {
            v["domains"]["base"]
                .as_array_mut()
                .unwrap()
                .push(name.into());
        }
    });
    reseal(&mut files);
    files
}

fn request(data: &NativeData) -> NativeBuildRequest {
    let c = data.contract();
    let mut group = SocketGroup::new()
        .with_gem(scope::GEM_ID)
        .with_active_skill(scope::EFFECT_ID, 1)
        .with_gem_skill_quality(scope::EFFECT_ID, 1, 0);
    group.active_gem_quality = Some(0);
    NativeBuildRequest {
        schema: NativeBuildSchema::V1,
        dataset: NativeDatasetBinding {
            game: c.game,
            realm: c.realm,
            game_patch: c.game_patch.clone(),
            tree_resource_sha256: c.tree.resource_sha256.clone(),
            assembly_key: c.assembly_key.clone(),
            runtime_identity_sha256: data.identity().sha256.clone(),
        },
        actor: NativeActorSelection {
            id: NATIVE_PLAYER_ID.into(),
            level: 1,
        },
        skill: NativeGemSelection {
            group_index: 0,
            gem_index: 0,
            gem_id: scope::GEM_ID.into(),
            effect_id: scope::EFFECT_ID.into(),
            stat_set_id: scope::EFFECT_ID.into(),
            level: 1,
            quality: 0,
        },
        actor_policy: NativeActorPolicyPin {
            id: "selected-player-generic-ot-v1".into(),
            version: 1,
            sha256: data
                .actor_binding_sha256()
                .map_or_else(|| "f".repeat(64), String::from),
        },
        evaluation: NativeEvaluationKind::ConditionalSkillHitV1,
        build: Build::new()
            .with_character(CharacterIdentity {
                level: 1,
                class_name: "Sorceress".into(),
                ascendancy_name: String::new(),
            })
            .with_config(
                BuildConfig::new().with_campaign_progress(pobr_core::CampaignProgress::Act1),
            )
            .add_socket_group(group),
    }
}

fn load(files: BTreeMap<String, Vec<u8>>) -> NativeData {
    NativeData::load(&GameData::from_memory(files))
        .expect("valid independently sealed synthetic native pack")
}

fn result(files: BTreeMap<String, Vec<u8>>) -> NativeSkillResult {
    let data = load(files);
    calculate_native_skill(&admit_native_skill(&data, &request(&data)).unwrap()).unwrap()
}

fn lightning(result: &NativeSkillResult) -> &NativeHitDamage {
    result
        .damage
        .iter()
        .find(|row| row.damage_type == DamageType::Lightning)
        .unwrap()
}

fn near(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-10, "{actual} != {expected}");
}

#[test]
fn source_owned_mapping_shared_core_and_provenance_are_retained() {
    let files = pack();
    let data = load(files.clone());
    let request = request(&data);
    let admitted = admit_native_skill(&data, &request).unwrap();
    assert_eq!(admitted.request_sha256(), request.input_sha256().unwrap());
    let out = calculate_native_skill(&admitted).unwrap();
    let row = lightning(&out);
    assert_eq!(
        row.non_critical,
        NativeDamageRange {
            min: 1.0,
            max: 10.0,
            mean: 5.5
        }
    );
    // The reused fixture's lexical source token is 37.5, intentionally unlike the actual pack.
    // The common critical resolver rounds to nine decimal places, retaining 1.375.
    near(out.critical_multiplier, 1.375);
    near(out.critical_probability, 0.09);
    near(row.critical.mean, 5.5 * 1.375);
    near(out.conditional_weighted_mean, 5.5 * (0.91 + 0.09 * 1.375));
    assert_eq!(out.analytical_base_use_ms, 700);
    assert_eq!(out.resource_costs[0].resource, "Mana");
    assert_eq!(out.resource_costs[0].amount_per_use, 5.0);
    assert_eq!(out.total_projectiles, 4.0);
    assert_eq!(out.projectile_lifetime_ms, 2000.0);
    assert_eq!(out.skill_facts.len(), 16);
    assert_eq!(out.model_status, NativeModelStatus::ConditionalModel);
    assert_eq!(out.provenance.contract, *data.contract());
    assert_eq!(
        out.provenance.actor_disposition_counts,
        NativeDispositionCounts::from_facts(&data.actor_binding().unwrap().dispositions)
    );
    assert_eq!(
        out.provenance.actor_dispositions_source,
        reference("base/native_actor_binding.json", "/dispositions")
    );
    assert!(!out.critical_trace.nodes.is_empty());
    assert!(!out.critical_trace.edges.is_empty());
    for evidence in &out.modifiers {
        let origin = evidence.origin.as_ref().unwrap();
        assert_eq!(origin.source_id.kind, SourceKind::SkillGem);
        assert_eq!(
            origin.source_id.id,
            format!("native.skill.{}", evidence.native_stat_id)
        );
        assert_eq!(origin.stat_id.as_ref().unwrap().as_str(), evidence.name);
        assert_eq!(origin.mod_type, Some(evidence.mod_type));
        assert!(
            origin
                .raw_text
                .as_ref()
                .unwrap()
                .contains(&evidence.native_stat_id)
        );
        assert!(evidence.counted);
        if evidence.intrinsic_reader.is_none() {
            assert!(!evidence.queries.is_empty(), "{evidence:?}");
        }
        assert_eq!(evidence.runtime_source.artifact, SETS);
    }
    for fact in &out.skill_facts {
        let raw: Value = serde_json::from_slice(&files[&fact.source.artifact]).unwrap();
        assert!(raw.pointer(&fact.source.pointer).is_some(), "{fact:?}");
    }
    for source in out.provenance.selection_sources.iter().chain([
        &out.provenance.critical_chance_source,
        &out.provenance.critical_bonus_source,
        &out.provenance.base_use_source,
    ]) {
        let raw: Value = serde_json::from_slice(&files[&source.artifact]).unwrap();
        assert!(raw.pointer(&source.pointer).is_some(), "{source:?}");
    }
    let encoded = serde_json::to_value(out).unwrap();
    assert_eq!(encoded["model_status"], "conditional-model");
    assert_eq!(
        encoded["provenance"]["contract"]["acquisition"]["status"],
        "unreviewed-acquisition-draft"
    );
    assert!(encoded.get("dps").is_none());
    assert!(encoded.get("pools").is_none());
    assert!(encoded["unavailable"].as_array().unwrap().len() >= 9);
}

struct DiskPack(PathBuf);
impl DiskPack {
    fn new(files: &BTreeMap<String, Vec<u8>>) -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "pobr-native-admission-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        let pack = Self(path);
        for (name, bytes) in files {
            let path = pack.0.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
        pack
    }
}
impl Drop for DiskPack {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn exact_bound_save_reopen_fresh_disk_and_memory_instances() {
    let files = pack();
    let disk = DiskPack::new(&files);
    let first = GameData::new(&disk.0);
    let native = NativeData::load(&first).unwrap();
    let mut request = request(&native);
    request.build.tree_version = Some("retained UI tag; never a dataset selector".into());
    request.build.character.class_name = "Witch display text; never actor authority".into();
    request.build.socket_groups[0].slot = Some("Skills UI".into());
    request.build.socket_groups[0].gem_requirement_levels = vec![1];
    let expected = serde_json::to_value(
        calculate_native_skill(&admit_native_skill(&native, &request).unwrap()).unwrap(),
    )
    .unwrap();
    let bytes = request.to_json().unwrap();
    let saved = disk.0.join("saved-build.json");
    std::fs::write(&saved, &bytes).unwrap();
    drop(first);
    drop(native);
    let reopened = NativeBuildRequest::from_json(&std::fs::read(saved).unwrap()).unwrap();
    assert_eq!(reopened.to_json().unwrap(), bytes);
    for data in [
        GameData::new(&disk.0),
        GameData::from_memory(files.clone()),
        GameData::from_memory_prechecked(files),
    ] {
        let native = NativeData::load(&data).unwrap();
        reopened.dataset.matches_game_data(&data).unwrap();
        let admitted = admit_native_skill(&native, &reopened).unwrap();
        let actual = calculate_native_skill(&admitted).unwrap();
        assert_eq!(
            actual.compatibility_class_name,
            request.build.character.class_name
        );
        assert_eq!(actual.gem_requirement_levels, [1]);
        assert_eq!(serde_json::to_value(actual).unwrap(), expected);
    }
}

#[test]
fn agreeing_redundant_mirrors_and_neutral_controls_preserve_source_outputs() {
    let data = load(pack());
    let mut r = request(&data);
    let baseline = calculate_native_skill(&admit_native_skill(&data, &r).unwrap()).unwrap();
    r.build.config.is_spell = true;
    r.build.config.damage_type = Some(DamageType::Lightning);
    r.build.main_socket_group = Some(1);
    r.build.weapon_set = crate::WeaponSet::Second;
    let group = &mut r.build.socket_groups[0];
    group.main_active_skill = Some(1);
    group.weapon_set = Some(2);
    group.group_count = Some(1.0);
    group.gem_skills[0].count = Some(1.0);
    group.gem_requirement_levels = vec![1];
    let admitted = admit_native_skill(&data, &r).unwrap();
    // Subsequent edits cannot modify the already-admitted owned request or facts.
    r.build.config.conditions.insert("Moving".into(), true);
    assert!(admit_native_skill(&data, &r).is_err());
    drop(data);
    let actual = calculate_native_skill(&admitted).unwrap();
    assert_eq!(
        actual.conditional_weighted_mean,
        baseline.conditional_weighted_mean
    );
    assert_eq!(actual.total_projectiles, baseline.total_projectiles);
    assert_ne!(actual.request_sha256, baseline.request_sha256);
}

// Change the fixture's lexical source, typed projection, retained archive, and all pins.
// This is a synthetic fixture mutation, not a producer or decoder reimplementation.
fn bonus(files: &mut BTreeMap<String, Vec<u8>>, token: &str) {
    edit(files, CONTEXT, |c| {
        let stat = c["facts"]["metadata_stats"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|s| s["key_raw"] == "base_critical_hit_damage_bonus")
            .unwrap();
        let index = stat["assignment"].as_u64().unwrap() as usize;
        stat["value_raw"] = token.into();
        let file = &mut c["metadata"]["files"][NATIVE_GENERIC_OT];
        let assignment = &mut file["assignments"][index];
        let line = assignment["line"].clone();
        let old = assignment["value_raw"].as_str().unwrap().to_string();
        assert_eq!(
            old.len(),
            token.len(),
            "fixture mutation preserves lexical spans"
        );
        assignment["value_raw"] = token.into();
        for record in file["records"].as_array_mut().unwrap() {
            if record["line"] == line {
                record["literal"] = record["literal"]
                    .as_str()
                    .unwrap()
                    .replace(&old, token)
                    .into();
            }
        }
        for (name, value) in [
            ("player_context.json", c["facts"].clone()),
            ("metadata.json", c["metadata"].clone()),
        ] {
            c["source_archive"][name] = serde_json::to_string(&value).unwrap().into();
        }
        let mut manifest: Value =
            serde_json::from_str(c["source_archive"]["manifest.json"].as_str().unwrap()).unwrap();
        for name in ["player_context.json", "metadata.json"] {
            let bytes = c["source_archive"][name].as_str().unwrap().as_bytes();
            manifest["files"][name]["sha256"] = hash(bytes).into();
            manifest["files"][name]["bytes"] = bytes.len().into();
        }
        let manifest = serde_json::to_string(&manifest).unwrap();
        c["source_manifest_sha256"] = hash(manifest.as_bytes()).into();
        c["source_archive"]["manifest.json"] = manifest.into();
    });
    edit(files, BINDING, |b| {
        b["critical_bonus"]["bonus_percent"] = token.parse::<f64>().unwrap().into()
    });
    reseal(files);
}

#[test]
fn resolved_damage_chance_bonus_and_intrinsics_flow_independently() {
    let baseline = result(pack());
    let mut damage = pack();
    edit(&mut damage, SETS, |v| {
        v[0]["sets"][0]["levels"][0]["stats"][0]["value"] = 7.into();
        v[0]["sets"][0]["levels"][0]["stats"][1]["value"] = 21.into();
    });
    reseal(&mut damage);
    let changed = result(damage);
    assert_eq!(
        (
            lightning(&changed).non_critical.min,
            lightning(&changed).non_critical.max
        ),
        (7.0, 21.0)
    );
    assert_eq!(changed.critical_probability, baseline.critical_probability);
    assert_eq!(changed.critical_multiplier, baseline.critical_multiplier);
    let mut chance = pack();
    edit(&mut chance, LEVELS, |v| {
        v["SparkPlayer"][0]["crit_chance"] = 25.into()
    });
    reseal(&mut chance);
    let changed = result(chance);
    assert_eq!(changed.critical_probability, 0.25);
    assert_eq!(
        lightning(&changed).non_critical,
        lightning(&baseline).non_critical
    );
    assert_eq!(changed.critical_multiplier, baseline.critical_multiplier);
    for token in ["25.0", "75.0"] {
        let mut files = pack();
        bonus(&mut files, token);
        let changed = result(files);
        near(
            changed.critical_multiplier,
            1.0 + token.parse::<f64>().unwrap() / 100.0,
        );
        assert_eq!(changed.critical_probability, baseline.critical_probability);
        assert_eq!(
            lightning(&changed).non_critical,
            lightning(&baseline).non_critical
        );
    }
    let mut intrinsics = pack();
    edit(&mut intrinsics, SETS, |v| {
        v[0]["sets"][0]["levels"][0]["stats"][2]["value"] = 6.into();
        v[0]["sets"][0]["constant_stats"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|s| s["stat"] == "base_skill_effect_duration")
            .unwrap()["value"] = 1500.into();
    });
    edit(&mut intrinsics, "base/granted_effects.json", |v| {
        v[0]["cast_time"] = 813.into();
        v[0]["cost_types"] = json!([1]);
    });
    edit(
        &mut intrinsics,
        "base/cost_types.json",
        |v| *v = json!([{"id":"Unused","divisor":1},{"id":"Mana","divisor":2}]),
    );
    edit(&mut intrinsics, LEVELS, |v| {
        v["SparkPlayer"][0]["cost_amounts"] = json!([13])
    });
    reseal(&mut intrinsics);
    let changed = result(intrinsics);
    assert_eq!(changed.total_projectiles, 6.0);
    assert_eq!(changed.projectile_lifetime_ms, 1500.0);
    assert_eq!(changed.analytical_base_use_ms, 813);
    assert_eq!(changed.resource_costs[0].amount_per_use, 6.5);
    assert_eq!(
        changed.conditional_weighted_mean,
        baseline.conditional_weighted_mean
    );
}

#[test]
fn wrong_dataset_assembly_tree_and_policy_never_fall_back() {
    let data = load(pack());
    let original = request(&data);
    let mut changes = Vec::new();
    let mut r = original.clone();
    r.dataset.game_patch = "0.5.5.6".into();
    changes.push(r);
    let mut r = original.clone();
    r.dataset.runtime_identity_sha256 = "a".repeat(64);
    changes.push(r);
    let mut r = original.clone();
    r.dataset.assembly_key = "b".repeat(64);
    changes.push(r);
    let mut r = original.clone();
    r.dataset.tree_resource_sha256 = "c".repeat(64);
    changes.push(r);
    let mut r = original.clone();
    r.actor_policy.sha256 = "d".repeat(64);
    changes.push(r);
    let mut r = original.clone();
    r.actor_policy.id = "other-policy".into();
    changes.push(r);
    let mut r = original.clone();
    r.actor_policy.version = 2;
    changes.push(r);
    for r in changes {
        assert!(admit_native_skill(&data, &r).is_err(), "{r:?}");
    }
    let mut same_patch = pack();
    edit(&mut same_patch, "base/granted_effects.json", |v| {
        v[0]["displayed_name"] = "Changed snapshot".into()
    });
    reseal(&mut same_patch);
    let same_patch = load(same_patch);
    assert_eq!(same_patch.contract().game_patch, data.contract().game_patch);
    assert_eq!(
        same_patch.contract().assembly_key,
        data.contract().assembly_key
    );
    assert!(matches!(
        admit_native_skill(&same_patch, &original),
        Err(NativeAdmissionError::DatasetMismatch {
            field: "runtime_identity_sha256"
        })
    ));
    let v1 = load(v1_pack());
    assert!(admit_native_skill(&v1, &request(&v1)).is_err());
}

fn rejects_request(change: impl FnOnce(&mut Value)) {
    let data = load(pack());
    let mut value = serde_json::to_value(request(&data)).unwrap();
    change(&mut value);
    let request: NativeBuildRequest =
        serde_json::from_value(value.clone()).expect("well-typed scope regression input");
    assert!(
        admit_native_skill(&data, &request).is_err(),
        "unexpected admission: {value}"
    );
}

#[test]
fn actor_branch_level_quality_mirrors_unknown_fields_and_gem_overrides_reject() {
    for (pointer, value) in [
        ("/actor/id", json!("Metadata/Characters/Int/Int")),
        ("/actor/level", json!(2)),
        ("/skill/level", json!(2)),
        ("/skill/quality", json!(1)),
        ("/skill/stat_set_id", json!("SparkColdInfusedPlayer")),
        ("/skill/group_index", json!(1)),
        ("/skill/gem_index", json!(1)),
        ("/build/character/ascendancy_name", json!("Stormweaver")),
        ("/build/socket_groups/0/active_skill_id", Value::Null),
        ("/build/socket_groups/0/active_gem_level", Value::Null),
        ("/build/socket_groups/0/active_gem_quality", Value::Null),
        ("/build/socket_groups/0/active_gem_quality", json!(1)),
        ("/build/socket_groups/0/enabled", json!(false)),
        ("/build/socket_groups/0/weapon_set", json!(2)),
        ("/build/socket_groups/0/source", json!("")),
        (
            "/build/socket_groups/0/reservation_free_skill",
            json!("SparkPlayer"),
        ),
        ("/build/socket_groups/0/granted_level", json!(1)),
        ("/build/socket_groups/0/gem_requirement_levels", json!([2])),
        ("/build/socket_groups/0/main_active_skill", json!(2)),
        ("/build/main_socket_group", json!(2)),
        ("/build/socket_groups/0/include_in_full_dps", json!(true)),
        ("/build/socket_groups/0/group_count", json!(0)),
        ("/build/socket_groups/0/gem_skills/0/enabled", json!(false)),
        ("/build/socket_groups/0/gem_skills/0/count", json!(2)),
        (
            "/build/socket_groups/0/gem_skills/0/enable_global_1",
            json!(false),
        ),
        (
            "/build/socket_groups/0/gem_skills/0/enable_global_2",
            json!(false),
        ),
        (
            "/build/socket_groups/0/gem_skills/0/global_level_bonus",
            json!(0),
        ),
        (
            "/build/socket_groups/0/gem_skills/0/stat_set_index",
            json!(1),
        ),
        (
            "/build/socket_groups/0/gem_skills/0/name_spec",
            json!("Spark"),
        ),
        ("/build/socket_groups/0/gem_skills/0/corrupted", json!(true)),
        ("/build/socket_groups/0/gem_skills/0/minion", json!("x")),
        ("/build/socket_groups/0/gem_skills/0/minion_skill", json!(1)),
    ] {
        rejects_request(|v| *v.pointer_mut(pointer).unwrap() = value);
    }
    for pointer in [
        "",
        "/build",
        "/build/config",
        "/build/party",
        "/build/socket_groups/0",
        "/build/socket_groups/0/gem_skills/0",
    ] {
        let data = load(pack());
        let mut value = serde_json::to_value(request(&data)).unwrap();
        value
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unknown".into(), json!(0));
        assert!(NativeBuildRequest::from_json(&serde_json::to_vec(&value).unwrap()).is_err());
    }
    for (pointer, value) in [
        ("/dataset/game", json!("poe1")),
        ("/dataset/realm", json!("CN")),
        ("/evaluation", json!("full-build")),
    ] {
        let data = load(pack());
        let mut document = serde_json::to_value(request(&data)).unwrap();
        *document.pointer_mut(pointer).unwrap() = value;
        assert!(NativeBuildRequest::from_json(&serde_json::to_vec(&document).unwrap()).is_err());
    }
    rejects_request(|v| {
        v["actor"]["level"] = 2.into();
        v["build"]["character"]["level"] = 2.into();
    });
    // Mirrors can agree with a requested unsupported level/quality and still fail.
    rejects_request(|v| {
        v["skill"]["level"] = 2.into();
        v["build"]["socket_groups"][0]["active_gem_level"] = 2.into();
        v["build"]["socket_groups"][0]["gem_skills"][0]["gem_level"] = 2.into();
    });
    rejects_request(|v| {
        v["skill"]["quality"] = 1.into();
        v["build"]["socket_groups"][0]["active_gem_quality"] = 1.into();
        v["build"]["socket_groups"][0]["gem_skills"][0]["quality"] = 1.into();
    });
}

#[test]
fn all_extra_sources_and_raw_false_zero_config_are_rejected() {
    let item = json!({"base":"Iron Ring","rarity":"normal","quality":0,"corrupted":false,
        "implicit_texts":[],"modifier_texts":[],"enchant_texts":[],"rolled_defence":{},"socketed":[],"parsed_stats":[]});
    for (pointer, value) in [
        ("/build/items", json!({"ring1":item})),
        ("/build/jewels", json!([["tree", item]])),
        ("/build/granted_socket_jewels", json!([[1, item, null]])),
        (
            "/build/radius_jewels",
            json!([{"socket_node":1,"radius_label":null,"grant_lines":[],"notable_effect_inc":0,"small_effect_inc":0,"tree_texts":[],"rarity":"normal","corrupted":false,"socket":"tree"}]),
        ),
        ("/build/utility_slots", json!([["Flask 1", item]])),
        ("/build/inactive_utility_slots", json!([["Flask 1", item]])),
        ("/build/limbs", json!({"arm1":item})),
        ("/build/other_weapons", json!({"weapon1":item})),
        ("/build/other_weapon_set_nodes", json!([])),
        ("/build/tree/allocated_nodes", json!([2779])),
        (
            "/build/tree/mastery_effects",
            json!({"1":{"effect_text":""}}),
        ),
        ("/build/tree/attribute_overrides", json!({"1":"strength"})),
        (
            "/build/party/members",
            json!([{"id":"1","name":"disabled","enabled":false,"blocks":{},"warnings":[]}]),
        ),
        ("/build/party/exports", json!({"Auras":""})),
        ("/build/config/conditions", json!({"Moving":false})),
        ("/build/config/multipliers", json!({"PowerCharge":0})),
        ("/build/config/global_modifier_texts", json!([""])),
        ("/build/config/bandit", json!("Alira")),
        ("/build/config/campaign_progress", Value::Null),
        ("/build/config/enemy_tier", json!("Pinnacle")),
        ("/build/config/is_attack", json!(true)),
        ("/build/config/damage_type", json!("Cold")),
        (
            "/build/config/raw_inputs/values",
            json!({"x":{"bool":false}}),
        ),
        (
            "/build/config/raw_inputs/placeholders",
            json!({"x":{"number":0}}),
        ),
    ] {
        rejects_request(|v| *v.pointer_mut(pointer).unwrap() = value);
    }
    rejects_request(|v| {
        let mut extra = v["build"]["socket_groups"][0].clone();
        extra["enabled"] = false.into();
        v["build"]["socket_groups"]
            .as_array_mut()
            .unwrap()
            .push(extra);
    });
    rejects_request(|v| {
        let mut extra = v["build"]["socket_groups"][0]["gem_skills"][0].clone();
        extra["enabled"] = false.into();
        v["build"]["socket_groups"][0]["gem_skills"]
            .as_array_mut()
            .unwrap()
            .push(extra);
    });
    // Typed state avoids a malformed JSON enum accidentally satisfying the rejection test.
    let data = load(pack());
    let mut r = request(&data);
    r.build.config.raw_inputs.values.insert(
        "x".into(),
        pobr_core::rules::config_interpreter::ConfigInputValue::Bool(false),
    );
    assert!(admit_native_skill(&data, &r).is_err());
    let mut r = request(&data);
    r.build.config.raw_inputs.placeholders.insert(
        "x".into(),
        pobr_core::rules::config_interpreter::ConfigInputValue::Number(0.0),
    );
    assert!(admit_native_skill(&data, &r).is_err());
    let mut r = request(&data);
    r.build.socket_groups[0].gem_skills[0].count = Some(f64::NAN);
    assert!(admit_native_skill(&data, &r).is_err());
}

fn rejects_source(path: &str, change: impl FnOnce(&mut Value)) {
    let mut files = pack();
    edit(&mut files, path, change);
    reseal(&mut files);
    let native = load(files);
    assert!(
        admit_native_skill(&native, &request(&native)).is_err(),
        "{path}"
    );
}

#[test]
fn exact_joins_invalid_stats_and_unmodelled_channels_fail_before_calculation() {
    for path in [
        "base/base_items.json",
        "base/skill_gems.json",
        "base/granted_effects.json",
        SETS,
    ] {
        rejects_source(path, |v| {
            let duplicate = v[0].clone();
            v.as_array_mut().unwrap().push(duplicate);
        });
        rejects_source(path, |v| *v = json!([]));
    }
    rejects_source("base/skill_gems.json", |v| {
        v[0]["granted_effect_id"] = "SparkColdInfusedPlayer".into()
    });
    rejects_source(SETS, |v| {
        let duplicate = v[0]["sets"][0].clone();
        v[0]["sets"].as_array_mut().unwrap().push(duplicate);
    });
    rejects_source(SETS, |v| {
        let duplicate = v[0]["sets"][0]["levels"][0].clone();
        v[0]["sets"][0]["levels"]
            .as_array_mut()
            .unwrap()
            .push(duplicate);
    });
    rejects_source(LEVELS, |v| {
        let duplicate = v["SparkPlayer"][0].clone();
        v["SparkPlayer"].as_array_mut().unwrap().push(duplicate);
    });
    rejects_source(LEVELS, |v| v["SparkPlayer"][0]["level"] = 2.into());
    rejects_source(SETS, |v| {
        v[0]["sets"][0]["levels"][0]["gem_level"] = 2.into()
    });
    rejects_source(SETS, |v| {
        v[0]["sets"][0]["set_id"] = "SparkColdInfusedPlayer".into()
    });
    rejects_source(SETS, |v| v[0]["sets"][0]["vendor_set_index"] = 1.into());
    rejects_source(SETS, |v| {
        v[0]["sets"][0]["levels"][0]["stats"][0]["stat"] = "unknown_new_stat".into()
    });
    rejects_source(SETS, |v| {
        v[0]["sets"][0]["constant_stats"]
            .as_array_mut()
            .unwrap()
            .push(json!({"stat":"unknown_zero_stat","value":0}));
    });
    rejects_source(SETS, |v| {
        v[0]["sets"][0]["levels"][0]["stats"][0]["value"] = (-1).into()
    });
    rejects_source(SETS, |v| {
        v[0]["sets"][0]["levels"][0]["stats"][0]["value"] = 11.into()
    });
    rejects_source(SETS, |v| {
        v[0]["sets"][0]["levels"][0]["stats"][2]["value"] = 0.into()
    });
    rejects_source(SETS, |v| {
        v[0]["sets"][0]["levels"][0]["stats"][2]["value"] = 1.5.into()
    });
    rejects_source(SETS, |v| {
        let duplicate = v[0]["sets"][0]["levels"][0]["stats"][0].clone();
        v[0]["sets"][0]["constant_stats"]
            .as_array_mut()
            .unwrap()
            .push(duplicate);
    });
    rejects_source(SETS, |v| {
        v[0]["sets"][0]["implicit_stats"]
            .as_array_mut()
            .unwrap()
            .push("unmodelled_implicit".into())
    });
    rejects_source(SETS, |v| {
        v[0]["sets"][0]["levels"][0]["damage_multiplier"] = 2.into()
    });
    rejects_source(LEVELS, |v| v["SparkPlayer"][0]["crit_chance"] = 101.into());
    rejects_source(LEVELS, |v| {
        v["SparkPlayer"][0]
            .as_object_mut()
            .unwrap()
            .remove("crit_chance");
    });
    rejects_source(LEVELS, |v| v["SparkPlayer"][0]["cooldown_ms"] = 500.into());
    rejects_source(LEVELS, |v| {
        v["SparkPlayer"][0]["spirit_reservation_flat"] = 1.into()
    });
    rejects_source("base/granted_effects.json", |v| {
        v[0]["skill_types"]
            .as_array_mut()
            .unwrap()
            .push("Attack".into())
    });
    let mut zero = pack();
    edit(&mut zero, SETS, |v| {
        v[0]["sets"][0]["levels"][0]["stats"][0]["value"] = 0.into()
    });
    reseal(&mut zero);
    let out = result(zero);
    assert_eq!(lightning(&out).non_critical.min, 0.0);
    assert!(
        out.skill_facts
            .iter()
            .any(|f| f.value == 0.0 && f.reason.contains("zero minimum explicitly accounted"))
    );
    assert_eq!(
        out.modifiers
            .iter()
            .filter(|m| m.intrinsic_reader.is_none())
            .count(),
        1
    );
}

#[test]
fn complete_cost_pairs_unique_resource_positive_divisor_and_instantaneous_only() {
    rejects_source(LEVELS, |v| v["SparkPlayer"][0]["cost_amounts"] = json!([]));
    rejects_source(LEVELS, |v| {
        v["SparkPlayer"][0]["cost_amounts"] = json!([5, 8])
    });
    rejects_source("base/granted_effects.json", |v| {
        v[0]["cost_types"] = json!([9])
    });
    rejects_source("base/cost_types.json", |v| *v = json!([]));
    rejects_source("base/cost_types.json", |v| v[0]["id"] = "Life".into());
    rejects_source("base/cost_types.json", |v| v[0]["divisor"] = 0.into());
    rejects_source("base/cost_types.json", |v| v[0]["per_minute"] = true.into());
    rejects_source("base/cost_types.json", |v| {
        let duplicate = v[0].clone();
        v.as_array_mut().unwrap().push(duplicate);
    });
}

#[test]
fn relevant_unresolved_actor_fact_blocks_even_with_valid_snapshot_and_policy_pin() {
    rejects_source(BINDING, |v| {
        let fact = v["dispositions"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|f| f["relevant_to_conditional_hit"] == true)
            .unwrap();
        fact["disposition"] = "unresolved".into();
    });
}

/// Explicit root-supplied actual pack proof. Missing input fails; no silent skip or user path.
#[test]
#[ignore = "requires POBR_NATIVE_SKILL_PACK pointing to the supplied immutable actual v2 pack"]
fn supplied_actual_pack_save_reopen_and_conditional_outputs() {
    let path = std::env::var_os("POBR_NATIVE_SKILL_PACK")
        .expect("supply the immutable actual v2 pack explicitly");
    let files = read_pack(Path::new(&path));
    let data = NativeData::load(&GameData::new(Path::new(&path))).unwrap();
    let r = request(&data);
    let saved = r.to_json().unwrap();
    let reopened = NativeBuildRequest::from_json(&saved).unwrap();
    let first = calculate_native_skill(&admit_native_skill(&data, &r).unwrap()).unwrap();
    drop(data);
    let fresh = load(files);
    let out = calculate_native_skill(&admit_native_skill(&fresh, &reopened).unwrap()).unwrap();
    assert_eq!(
        serde_json::to_value(&out).unwrap(),
        serde_json::to_value(first).unwrap()
    );
    assert_eq!(
        lightning(&out).non_critical,
        NativeDamageRange {
            min: 1.0,
            max: 10.0,
            mean: 5.5
        }
    );
    assert_eq!(
        lightning(&out).critical,
        NativeDamageRange {
            min: 2.0,
            max: 20.0,
            mean: 11.0
        }
    );
    near(out.critical_probability, 0.09);
    near(out.critical_multiplier, 2.0);
    near(out.conditional_weighted_mean, 5.995);
    assert_eq!(out.analytical_base_use_ms, 700);
    assert_eq!(out.resource_costs[0].resource, "Mana");
    assert_eq!(out.resource_costs[0].amount_per_use, 5.0);
    assert_eq!(out.total_projectiles, 4.0);
    assert_eq!(out.projectile_lifetime_ms, 2000.0);
}
