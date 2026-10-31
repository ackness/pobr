use super::*;
use crate::build::{GemSkillRef, JewelSocket, WeaponSet};
use crate::party::PartyMember;
use pobr_core::{CampaignProgress, rules::config_interpreter::ConfigInputValue};
use pobr_data::{
    item::{EquipmentSlot, ItemBaseId, ItemRarity, LimbSlot, RolledDefence},
    passive_tree::{AttributeChoice, MasterySelection, NodeId},
};
use std::collections::BTreeMap;

fn item(label: &str) -> Item {
    Item {
        base: ItemBaseId::from(label),
        rarity: ItemRarity::Rare,
        quality: 13,
        corrupted: true,
        implicit_texts: vec!["保留中文原文".into()],
        modifier_texts: vec!["Rarity: Rare\n--------\nunknown modifier <not XML>".into()],
        enchant_texts: vec!["+7 to something unmodelled".into()],
        rolled_defence: RolledDefence {
            armour: Some(13.527611300083607),
            spirit: Some(-0.0),
            ..Default::default()
        },
        socketed: vec!["unevaluated soul core".into()],
        parsed_stats: vec!["old attribution label".into()],
    }
}

fn request() -> NativeBuildRequest {
    let gem_id = "Metadata/Items/Gems/SkillGemSpark".to_owned();
    let mut build = Build::new();
    build.character.level = 1;
    build.character.class_name = "Sorceress".into();
    build.tree_version = Some("0_5".into());
    build.socket_groups.push(crate::SocketGroup {
        enabled: true,
        gem_ids: vec![gem_id.clone()],
        active_skill_id: Some("SparkPlayer".into()),
        active_gem_level: Some(1),
        active_gem_quality: Some(0),
        gem_skills: vec![GemSkillRef {
            skill_id: "SparkPlayer".into(),
            gem_level: 1,
            ..Default::default()
        }],
        ..Default::default()
    });
    build.config.campaign_progress = Some(CampaignProgress::Act1);
    NativeBuildRequest {
        schema: NativeBuildSchema::V1,
        dataset: NativeDatasetBinding {
            game: NativeGame::Poe2,
            realm: NativeRealm::International,
            game_patch: "0.5.5.5".into(),
            tree_resource_sha256: "a".repeat(64),
            assembly_key: "b".repeat(64),
            runtime_identity_sha256: "c".repeat(64),
        },
        actor: NativeActorSelection {
            id: "Metadata/Characters/Int/IntFourb".into(),
            level: 1,
        },
        skill: NativeGemSelection {
            group_index: 0,
            gem_index: 0,
            gem_id,
            effect_id: "SparkPlayer".into(),
            stat_set_id: "SparkPlayer".into(),
            level: 1,
            quality: 0,
        },
        actor_policy: NativeActorPolicyPin {
            id: "selected-player-generic-ot-v1".into(),
            version: 1,
            sha256: "d".repeat(64),
        },
        evaluation: NativeEvaluationKind::ConditionalSkillHitV1,
        build,
    }
}

#[test]
fn native_document_roundtrip_preserves_existing_build_and_inactive_state() {
    let mut original = request();
    let build = &mut original.build;
    build
        .items
        .insert(EquipmentSlot::Ring1, item("active raw base"));
    build
        .other_weapons
        .insert(EquipmentSlot::Weapon1, item("inactive weapon"));
    build
        .jewels
        .push((JewelSocket::Granted, item("granted jewel")));
    build
        .granted_socket_jewels
        .push((88, item("inactive granted jewel"), None));
    build
        .utility_slots
        .push(("Charm 1".into(), item("active charm")));
    build
        .inactive_utility_slots
        .push(("Flask 1".into(), item("inactive flask")));
    build.limbs.insert(LimbSlot::Arm1, item("extra limb"));
    build.other_weapon_set_nodes = Some(vec![NodeId(42)]);
    build.weapon_set = WeaponSet::Second;
    build.tree.allocated_nodes = vec![NodeId(62518), NodeId(91)];
    build
        .tree
        .attribute_overrides
        .insert(NodeId(91), AttributeChoice::Intelligence);
    build.tree.mastery_effects.insert(
        NodeId(92),
        MasterySelection {
            effect_text: "unparsed choice".into(),
        },
    );
    build
        .config
        .conditions
        .insert("UnknownInactiveState".into(), false);
    build
        .config
        .multipliers
        .insert("UnknownAmount".into(), 2.25);
    build
        .config
        .global_modifier_texts
        .push("保留未知词条".into());
    build.config.raw_inputs.values.insert(
        "native-text".into(),
        ConfigInputValue::Text("opaque".into()),
    );
    build
        .config
        .raw_inputs
        .placeholders
        .insert("fallback".into(), ConfigInputValue::Number(3.125));
    build.party.members.push(PartyMember {
        enabled: false,
        name: "不生效但保留".into(),
        ..Default::default()
    });
    let bytes = original.to_json().unwrap();
    let reopened = NativeBuildRequest::from_json(&bytes).unwrap();
    assert!(crate::snapshot::same_build(
        &original.build,
        &reopened.build
    ));
    assert_eq!(reopened.dataset, original.dataset);
    assert_eq!(reopened.actor_policy, original.actor_policy);
    assert_eq!(reopened.skill, original.skill);
    assert_eq!(reopened.to_json().unwrap(), bytes);
    assert_eq!(
        reopened.input_sha256().unwrap(),
        original.input_sha256().unwrap()
    );
    // Persistence of unsupported sources is not numerical admission of them.
    assert_eq!(reopened.build.items.len(), 1);
}

#[test]
fn native_document_canonicalizes_maps_without_reordering_gems_or_nodes() {
    let mut a = request();
    a.build.config.conditions.insert("z-last".into(), true);
    a.build.config.conditions.insert("a-first".into(), false);
    let mut b = a.clone();
    b.build.config.conditions.clear();
    b.build.config.conditions.insert("a-first".into(), false);
    b.build.config.conditions.insert("z-last".into(), true);
    assert_eq!(a.to_json().unwrap(), b.to_json().unwrap());
    a.build.tree.allocated_nodes = vec![NodeId(1), NodeId(2)];
    b.build.tree.allocated_nodes = vec![NodeId(2), NodeId(1)];
    assert_ne!(a.input_sha256().unwrap(), b.input_sha256().unwrap());
}

#[test]
fn native_document_hash_binds_dataset_policy_actor_and_stable_branch() {
    let original = request();
    let hash = original.input_sha256().unwrap();
    for field in [
        "runtime", "assembly", "tree", "patch", "policy", "actor", "branch", "input",
    ] {
        let mut changed = original.clone();
        match field {
            "runtime" => changed.dataset.runtime_identity_sha256 = "e".repeat(64),
            "assembly" => changed.dataset.assembly_key = "e".repeat(64),
            "tree" => changed.dataset.tree_resource_sha256 = "e".repeat(64),
            "patch" => changed.dataset.game_patch = "0.5.5.6".into(),
            "policy" => changed.actor_policy.sha256 = "e".repeat(64),
            "actor" => changed.actor.id = "Metadata/Characters/Other".into(),
            "branch" => changed.skill.stat_set_id = "SparkColdInfusedPlayer".into(),
            _ => changed.build.socket_groups[0].gem_skills[0].count = Some(2.0),
        }
        assert_ne!(changed.input_sha256().unwrap(), hash, "{field}");
    }
}

#[test]
fn native_document_rejects_conflicting_selected_locations_and_mirrors() {
    for case in [
        "actor-level",
        "group",
        "gem",
        "gem-id",
        "effect",
        "level",
        "quality",
    ] {
        let mut changed = request();
        match case {
            "actor-level" => changed.actor.level = 2,
            "group" => changed.skill.group_index = 1,
            "gem" => changed.skill.gem_index = 1,
            "gem-id" => changed.skill.gem_id = "missing".into(),
            "effect" => changed.skill.effect_id = "missing".into(),
            "level" => changed.skill.level = 2,
            _ => changed.skill.quality = 20,
        }
        assert!(changed.to_json().is_err(), "{case}");
    }
}

#[test]
fn native_document_rejects_missing_unknown_and_duplicate_fields_at_every_depth() {
    let bytes = request().to_json().unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    for case in [
        "missing-binding",
        "extra-root",
        "extra-item",
        "extra-party",
        "missing-null",
        "future-schema",
    ] {
        let mut changed = value.clone();
        match case {
            "missing-binding" => {
                changed.as_object_mut().unwrap().remove("dataset");
            }
            "extra-root" => {
                changed["extra"] = Value::Bool(true);
            }
            "extra-item" => {
                changed["build"]["character"]["unknown"] = Value::Bool(true);
            }
            "extra-party" => {
                changed["build"]["party"]["ignored"] = Value::Bool(true);
            }
            "missing-null" => {
                changed["build"]
                    .as_object_mut()
                    .unwrap()
                    .remove("tree_version");
            }
            _ => {
                changed["schema"] = Value::String("pobr-native-build/v999".into());
            }
        }
        assert!(
            NativeBuildRequest::from_json(&serde_json::to_vec(&changed).unwrap()).is_err(),
            "{case}"
        );
    }
    let text = String::from_utf8(bytes).unwrap();
    for duplicate in [
        text.replacen("\"level\":1", "\"level\":1,\"level\":1", 1),
        text.replacen("\"level\":1", "\"level\":1,\"\\u006cevel\":1", 1),
        text.replacen(
            "\"group_index\":0",
            "\"group_index\":0,\"group_index\":0",
            1,
        ),
    ] {
        assert!(
            NativeBuildRequest::from_json(duplicate.as_bytes())
                .unwrap_err()
                .to_string()
                .contains("duplicate JSON key")
        );
    }
}

#[test]
fn native_document_never_serializes_nonfinite_values_as_null() {
    for case in ["item", "count", "group", "multiplier", "raw", "placeholder"] {
        let mut changed = request();
        match case {
            "item" => {
                let mut item = item("bad");
                item.rolled_defence.armour = Some(f64::NAN);
                changed.build.items.insert(EquipmentSlot::Ring1, item);
            }
            "count" => changed.build.socket_groups[0].gem_skills[0].count = Some(f64::INFINITY),
            "group" => changed.build.socket_groups[0].group_count = Some(f64::NEG_INFINITY),
            "multiplier" => {
                changed
                    .build
                    .config
                    .multipliers
                    .insert("bad".into(), f64::NAN);
            }
            "raw" => {
                changed
                    .build
                    .config
                    .raw_inputs
                    .values
                    .insert("bad".into(), ConfigInputValue::Number(f64::INFINITY));
            }
            _ => {
                changed
                    .build
                    .config
                    .raw_inputs
                    .placeholders
                    .insert("bad".into(), ConfigInputValue::Number(f64::NAN));
            }
        }
        assert!(
            changed
                .to_json()
                .unwrap_err()
                .to_string()
                .contains("nonfinite"),
            "{case}"
        );
    }
}

#[test]
fn native_binding_never_uses_current_or_legacy_fallback() {
    let data = GameData::from_memory(BTreeMap::new());
    assert!(request().dataset.matches_game_data(&data).is_err());
    assert!(NativeDatasetBinding::from_game_data(&data).is_err());
    let mut invalid = request();
    invalid.dataset.runtime_identity_sha256 = "latest".into();
    assert!(invalid.to_json().is_err());
}

#[test]
fn native_document_is_not_legacy_xml_code_or_item_text() {
    let mut native = request();
    native
        .build
        .items
        .insert(EquipmentSlot::Ring1, item("Rarity: Rare"));
    native.build.party.exports.insert(
        "embedded-xml".into(),
        "<PathOfBuilding2><Build level='99' className='Wrong'/></PathOfBuilding2>".into(),
    );
    let bytes = native.to_json().unwrap();
    let text = std::str::from_utf8(&bytes).unwrap();
    assert_eq!(crate::detect_import(text), None);
    assert!(crate::parse_build(text).is_err());
    assert!(crate::decode_pob_code(text).is_err());
}
