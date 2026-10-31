// Explicitly synthetic normalized catalogs and source references for portable tests.
// This fixture does not certify a real source package, producer or calculation result.

use pobr_data::catalog::NativeRuntimeContract;
use pobr_data::catalog::native_player::{
    NATIVE_ALLOCATION_ASSUMPTION, NATIVE_GEM_LEVEL_ASSUMPTION, NATIVE_OTHER_ALLOCATIONS_EXCLUSION,
    NATIVE_PASSIVES_ACTIVE_CONDITION, NATIVE_PRIMARY_SPELL_CONDITION,
};
use pobr_gamedata::manifest::native_assembly_key;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const CONTEXT: &str = "base/native_player_context.json";
const BINDING: &str = "base/native_actor_binding.json";
const CONTRACT: &str = "base/native_runtime.json";
const PROJECTION: &str = "base/native_skill_projection.json";
const INDEX: &str = "base/native_reference_index.json";

pub const MINIMUM_STAT_ID: &str = "object_innate_minimum_number_of_projectiles_to_fire_is_1";
pub const MINIMUM_REASON: &str = "The generic-OT minimum is retained without a projectile-count applicability law. This policy does not derive a missing intrinsic count or apply a projectile adjustment; the one-successful-hit output is independent of count.";

pub fn edit(files: &mut BTreeMap<String, Vec<u8>>, path: &str, change: impl FnOnce(&mut Value)) {
    let mut value = serde_json::from_slice(&files[path]).unwrap();
    change(&mut value);
    files.insert(path.into(), serde_json::to_vec(&value).unwrap());
}

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn reference(table: &str, row: usize) -> Value {
    json!({"payload":format!("tables/{table}.json"),"pointer":format!("/rows/{row}/values")})
}

pub fn extend(mut files: BTreeMap<String, Vec<u8>>) -> BTreeMap<String, Vec<u8>> {
    let template: Value = serde_json::from_slice(include_bytes!(
        "../../../pobr-build/src/native_skill/tests/skill-facts.json"
    ))
    .unwrap();
    let mut catalogs = json!({"base_items":[],"skill_gems":[],"granted_effects":[],
        "granted_effect_levels":{},"granted_effect_stat_sets":[],"cost_types":[{"id":"Mana","divisor":1}]});
    let mut selections = Vec::new();
    let mut evidence = serde_json::Map::new();
    for (i, gem_id, gem_effect, effect, active, element, extra) in [
        (
            0,
            "Metadata/Items/Gem/SkillGemFireball",
            "Fireball",
            "FireballPlayer",
            "fireball",
            "Fire",
            vec![
                "FireballExplosionPlayer",
                "FireballSecondaryProjectilePlayer",
            ],
        ),
        (
            1,
            "Metadata/Items/Gems/SkillGemFrostbolt",
            "FrostBolt",
            "FrostboltPlayer",
            "frost_bolt",
            "Cold",
            vec!["FrostboltExplosionPlayer"],
        ),
        (
            2,
            "Metadata/Items/Gems/SkillGemSpark",
            "Spark",
            "SparkPlayer",
            "spark",
            "Lightning",
            vec!["SparkColdInfusedPlayer"],
        ),
    ] {
        let mut base = template["base_items"][0].clone();
        base["id"] = gem_id.into();
        catalogs["base_items"].as_array_mut().unwrap().push(base);
        let mut gem = template["skill_gems"][0].clone();
        gem["id"] = gem_id.into();
        gem["gem_effect_id"] = gem_effect.into();
        gem["granted_effect_id"] = effect.into();
        gem["gem_tags"] = json!([
            "intelligence",
            "grants_active_skill",
            "spell",
            "projectile",
            "repeatable",
            element.to_lowercase(),
            if i == 2 { "duration" } else { "area" }
        ]);
        catalogs["skill_gems"].as_array_mut().unwrap().push(gem);
        let mut eff = template["granted_effects"][0].clone();
        eff["id"] = effect.into();
        eff["active_skill"] = active.into();
        eff["stat_set"] = (30 + i).into();
        eff["cast_time"] = (950 + i * 113).into();
        eff["skill_types"] = json!([
            "Spell",
            "Projectile",
            "ProjectilesFromUser",
            "Damage",
            element,
            "UsableWhileMoving"
        ]);
        if i != 2 {
            eff["skill_types"]
                .as_array_mut()
                .unwrap()
                .push("Area".into());
        }
        catalogs["granted_effects"]
            .as_array_mut()
            .unwrap()
            .push(eff);
        let mut set = template["granted_effect_stat_sets"][0]["sets"][0].clone();
        set["set_id"] = effect.into();
        let mut constants = template["granted_effect_stat_sets"][0]["sets"][0]["constant_stats"]
            .as_array()
            .unwrap()
            .clone();
        if i != 2 {
            constants.retain(|s| {
                s["stat"].as_str().unwrap().starts_with("movement_speed_")
                    || (i == 1 && s["stat"] == "base_skill_effect_duration")
            });
            if i == 0 {
                constants.push(json!({"stat":"projectile_maximum_range_override","value":93}));
            }
            if i == 1 {
                constants[0]["value"] = 4000.into();
            }
        } else {
            constants[0]["value"] = 1650.into();
        }
        set["constant_stats"] = constants.into();
        set["implicit_stats"] = if i == 2 {
            template["granted_effect_stat_sets"][0]["sets"][0]["implicit_stats"].clone()
        } else {
            json!([
                "base_is_projectile",
                "projectile_uses_contact_position",
                "maintain_projectile_direction_when_using_contact_position",
                "can_perform_skill_while_moving",
                "precise_cursor_targeting_uses_contact_point_height_offset"
            ])
        };
        if i == 0 {
            set["implicit_stats"]
                .as_array_mut()
                .unwrap()
                .push("active_skill_consumes_a_fire_infusion".into());
        }
        if i == 1 {
            set["implicit_stats"]
                .as_array_mut()
                .unwrap()
                .push("always_pierce".into());
        }
        let mut levels = Vec::new();
        let mut stat_levels = Vec::new();
        let mut level_evidence = Vec::new();
        for level in [1, 2, 10, 40] {
            levels.push(json!({"level":level,"crit_chance":13+i*7,"cost_amounts":[level*3+i+1]}));
            let mut stats = vec![
                json!({"stat":format!("spell_minimum_base_{}_damage",element.to_lowercase()),"value":(i+2)*level}),
                json!({"stat":format!("spell_maximum_base_{}_damage",element.to_lowercase()),"value":(i+4)*level+1}),
            ];
            if i == 2 {
                stats.push(json!({"stat":"base_number_of_projectiles","value":3+level}));
            }
            if i == 1 {
                stats.push(json!({"stat":"active_skill_chill_effect_+%_final","value":137}));
            }
            stat_levels.push(json!({"gem_level":level,"stats":stats}));
            level_evidence.push(json!({"gem_level":level,"rows":[reference("GrantedEffectsPerLevel",i*100+level),reference("GrantedEffectStatSetsPerLevel",i*100+level)],
                "actor_scaling_coordinate":level as f64 * 3.452,"retained_outside_projection":{"synthetic":true}}));
        }
        set["levels"] = stat_levels.into();
        catalogs["granted_effect_levels"][effect] = levels.into();
        catalogs["granted_effect_stat_sets"]
            .as_array_mut()
            .unwrap()
            .push(json!({"effect_id":effect,"sets":[set]}));
        selections.push(json!({"gem_id":gem_id,"gem_effect_id":gem_effect,"effect_id":effect,"active_skill_id":active,"stat_set_id":effect,"gem_levels":[1,2,10,40]}));
        evidence.insert(gem_id.into(), json!({"rows":[reference("BaseItemTypes",i),reference("SkillGems",i),reference("GemEffects",i),reference("GrantedEffects",i),reference("ActiveSkills",i),reference("GrantedEffectStatSets",30+i),reference("CostTypes",0)],
            "levels":level_evidence,"active_reverse_granted_effect":if i == 2 { effect } else { "" },"additional_stat_sets":extra,
            "retained_outside_projection":{"synthetic":true,"CooldownGroup":67,"CooldownBypassType":4}}));
    }
    for (name, value) in catalogs.as_object().unwrap() {
        files.insert(
            format!("base/{name}.json"),
            serde_json::to_vec(value).unwrap(),
        );
    }
    let projection = json!({"format":"pobr-native-skill-projection/v2","primary_only":true,"quality":0,"selections":selections,"retained_evidence":evidence});
    files.insert(PROJECTION.into(), serde_json::to_vec(&projection).unwrap());
    files.insert("base/native_input_lock.json".into(),serde_json::to_vec(&json!({"primary_gems":selections.iter().map(|s|s["gem_id"].clone()).collect::<Vec<_>>()})).unwrap());
    edit(&mut files, "base/native_coverage.json", |v| {
        v["projected"] = json!({"primary_spells":selections})
    });
    let resources: serde_json::Map<String, Value> = [
        "PassiveSkills",
        "Stats",
        "BaseItemTypes",
        "SkillGems",
        "GemEffects",
        "GrantedEffects",
        "ActiveSkills",
        "GrantedEffectStatSets",
        "CostTypes",
        "GrantedEffectsPerLevel",
        "GrantedEffectStatSetsPerLevel",
    ]
    .into_iter()
    .map(|t| {
        (
            format!("tables/{t}.json"),
            json!({"bytes":2,"sha256":hash(t.as_bytes())}),
        )
    })
    .collect();
    edit(&mut files, CONTEXT, |c| {
        let mut parent: Value = serde_json::from_str(
            c["source_archive"]["parent-manifest.json"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        parent["files"] = resources.clone().into();
        let text = serde_json::to_string(&parent).unwrap();
        let sha = hash(text.as_bytes());
        c["parent_manifest_sha256"] = sha.clone().into();
        c["source_archive"]["parent-manifest.json"] = text.clone().into();
        let mut producer: Value =
            serde_json::from_str(c["source_archive"]["manifest.json"].as_str().unwrap()).unwrap();
        producer["parent"]["manifest_sha256"] = sha.clone().into();
        producer["files"]["parent-manifest.json"] = json!({"sha256":sha,"bytes":text.len()});
        let text = serde_json::to_string(&producer).unwrap();
        c["source_manifest_sha256"] = hash(text.as_bytes()).into();
        c["source_archive"]["manifest.json"] = text.into();
    });
    let context: Value = serde_json::from_slice(&files[CONTEXT]).unwrap();
    let parent_sha = context["parent_manifest_sha256"].clone();
    let mut nodes = Vec::new();
    let mut instances = Vec::new();
    let mut definitions = serde_json::Map::new();
    for (id, stat, row, value) in [
        (11, "spell_damage_+%", 1000, 10),
        (12, "fire_damage_+%", 1001, 12),
        (13, "cold_damage_+%", 1002, 15),
        (14, "lightning_damage_+%", 1003, 20),
        (15, "spell_damage_+%", 1000, 10),
        (16, "base_fire_damage_resistance_%", 1004, 7),
    ] {
        let slug = format!("synthetic-{id}");
        nodes.push(json!({"skill":id,"id":slug,"kind":"normal","native_stats":{"data_version":"0.5.5.5","tree_version":"0_5","node_id":id,"node_slug":slug,"modifier_texts":[],"context":"unconditional_player",
            "stats":[{"id":stat,"value":value,"provenance":{"game_patch":"0.5.5.5","source_manifest_sha256":parent_sha,"passive_id":slug,"passive_row":id,"stat_row":row,"references":[
                {"resource":"tables/PassiveSkills.json","sha256":resources["tables/PassiveSkills.json"]["sha256"],"pointer":format!("/rows/{id}/values/Stat1Value")},
                {"resource":"tables/Stats.json","sha256":resources["tables/Stats.json"]["sha256"],"pointer":format!("/rows/{row}/values/Id")} ]}}]}}));
        instances.push(json!({"graph_id":id,"definition_id":slug,"group":0,"radius_raw":1,"position_raw":0,"connections":[],"splines":[]}));
        definitions.insert(slug,json!({"graph_id_raw":id,"in_default_graph":true,"raw":reference("PassiveSkills",id),"stats":[{"stat_id":stat,"value":value,"slot":1,"value_field":"Stat1Value","raw_stat":reference("Stats",row)}]}));
    }
    files.insert(
        "base/passive_tree.json".into(),
        serde_json::to_vec(&nodes).unwrap(),
    );
    files.insert(INDEX.into(),serde_json::to_vec(&json!({"format":"pobr-native-runtime-reference-index/v2","archive":{"manifest_sha256":parent_sha,"files":resources},"skill_selection":selections,"graph_instances":instances,"passive_definitions":definitions})).unwrap());
    let minimum_index = context["facts"]["metadata_stats"]
        .as_array()
        .unwrap()
        .iter()
        .position(|s| s["key_raw"] == MINIMUM_STAT_ID)
        .unwrap();
    let minimum_selector = format!("/facts/metadata_stats/{minimum_index}");
    edit(&mut files, BINDING, |v| {
        let minimum = v["dispositions"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|d| d["selector"] == minimum_selector)
            .unwrap();
        minimum["reason"] = MINIMUM_REASON.into();
        v["policy"] = "selected-player-generic-ot-primary-spell-hit-passives-v1".into();
        v["evaluation"] = "conditional-skill-hit-v2".into();
        v["excluded_sources"][2] = NATIVE_OTHER_ALLOCATIONS_EXCLUSION.into();
        v["required_conditions"][1] = NATIVE_PRIMARY_SPELL_CONDITION.into();
        v["required_conditions"]
            .as_array_mut()
            .unwrap()
            .push(NATIVE_PASSIVES_ACTIVE_CONDITION.into());
        v["assumptions"].as_array_mut().unwrap().extend([
            NATIVE_ALLOCATION_ASSUMPTION.into(),
            NATIVE_GEM_LEVEL_ASSUMPTION.into(),
        ]);
    });
    edit(&mut files, CONTRACT, |v| {
        v["input_profile"] = "selected-player-primary-spell-hit-q0-passives-v1".into();
        v["output_profile"] = "conditional-skill-hit-v2".into();
        for field in ["normalization", "rules"] {
            v[field]["profile"] = "primary-spell-hit-q0-passives".into();
        }
    });
    edit(&mut files, "manifest.json", |v| {
        let domains = v["domains"]["base"].as_array_mut().unwrap();
        if !domains.iter().any(|d| d == "native_skill_projection") {
            domains.push("native_skill_projection".into());
        }
    });
    reseal(&mut files);
    files
}

pub fn reseal(files: &mut BTreeMap<String, Vec<u8>>) {
    let context: Value = serde_json::from_slice(&files[CONTEXT]).unwrap();
    let context_sha = hash(&files[CONTEXT]);
    edit(files, BINDING, |v| {
        v["context_sha256"] = context_sha.clone().into()
    });
    let binding_sha = hash(&files[BINDING]);
    let lock_sha = hash(&files["base/native_input_lock.json"]);
    let index_sha = hash(&files[INDEX]);
    let coverage_sha = hash(&files["base/native_coverage.json"]);
    edit(files, CONTRACT, |v| {
        v["source_lock_sha256"] = lock_sha.into();
        v["reference_index_sha256"] = index_sha.into();
        v["coverage_sha256"] = coverage_sha.into();
        v["tree"]["source_manifest_sha256"] = context["parent_manifest_sha256"].clone();
        v["allowed_source_manifests"] = json!([
            context["source_manifest_sha256"],
            context["parent_manifest_sha256"]
        ]);
        v["player"] = json!({"context_sha256":context_sha,"actor_binding_sha256":binding_sha,"source_manifest_sha256":context["source_manifest_sha256"],"source_lock_sha256":context["source_lock_sha256"],"parent_manifest_sha256":context["parent_manifest_sha256"],"parent_source_lock_sha256":context["parent_source_lock_sha256"]});
    });
    let contract: NativeRuntimeContract = serde_json::from_slice(&files[CONTRACT]).unwrap();
    edit(files, CONTRACT, |v| {
        v["assembly_key"] = native_assembly_key(&contract).into()
    });
    let hashes: BTreeMap<_, _> = files
        .iter()
        .filter(|(p, _)| p.starts_with("base/"))
        .map(|(p, b)| (p.clone(), hash(b)))
        .collect();
    edit(files, "manifest.json", |v| {
        v["files"] = serde_json::to_value(hashes).unwrap()
    });
}
