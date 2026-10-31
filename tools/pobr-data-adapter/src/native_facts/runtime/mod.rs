//! Guarded, scoped runtime projection of a verified native archive.
//!
//! This produces queryable catalogs, not calculation admission or source approval.

mod inputs;
mod player;
mod player_policy;
mod skills;
mod tree;

#[cfg(test)]
mod player_tests;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use pobr_data::catalog::native_runtime::*;
use pobr_data::catalog::*;
use pobr_gamedata::GameData;
use serde_json::{Value, json};

use super::{Profile, convert, json_bytes, output, trust};
use crate::raw_source::sha256_hex;

pub(crate) struct Args {
    facts: super::Args,
    descriptions: PathBuf,
    descriptions_sha256: String,
    tree: PathBuf,
    tree_sha256: String,
    compiler_revision: String,
    player: Option<player::Args>,
}

pub(crate) fn parse_args(args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut args = args;
    let mut values = BTreeMap::new();
    while let Some(flag) = args.next() {
        trust::require(
            [
                "--native-runtime",
                "--expected-manifest-sha256",
                "--source-lock",
                "--out",
                "--descriptions",
                "--descriptions-manifest-sha256",
                "--official-tree",
                "--official-tree-sha256",
                "--compiler-revision",
                "--player-context",
                "--player-manifest-sha256",
                "--player-source-lock-sha256",
            ]
            .contains(&flag.as_str()),
            &format!("unsupported native-runtime argument: {flag}"),
        )?;
        let value = args.next().ok_or_else(|| format!("{flag} needs a value"))?;
        trust::require(
            !value.starts_with("--") && values.insert(flag.clone(), value).is_none(),
            &format!("missing value or duplicate argument: {flag}"),
        )?;
    }
    let player = match (values.remove("--player-context"), values.remove("--player-manifest-sha256"), values.remove("--player-source-lock-sha256")) {
        (None,None,None) => None,
        (Some(root),Some(manifest_sha256),Some(source_lock_sha256)) => Some(player::Args { root: root.into(), manifest_sha256, source_lock_sha256 }),
        _ => return Err("--player-context requires both external --player-manifest-sha256 and --player-source-lock-sha256".into()),
    };
    let mut take = |flag: &str| values.remove(flag).ok_or_else(|| format!("missing {flag}"));
    Ok(Args {
        facts: super::Args {
            input: take("--native-runtime")?.into(),
            expected_manifest_sha256: take("--expected-manifest-sha256")?,
            source_lock: take("--source-lock")?.into(),
            out: take("--out")?.into(),
            profile: Profile::ItemsActorsBc,
        },
        descriptions: take("--descriptions")?.into(),
        descriptions_sha256: take("--descriptions-manifest-sha256")?,
        tree: take("--official-tree")?.into(),
        tree_sha256: take("--official-tree-sha256")?,
        compiler_revision: take("--compiler-revision")?,
        player,
    })
}

pub(crate) fn run(args: Args) -> Result<String, String> {
    trust::require_absent(&args.facts.out)?;
    let mut output = output::Output::prepare(&args.facts)?;
    output.protect(&args.descriptions)?;
    if let Some(player) = &args.player {
        output.protect(&player.root)?;
    }
    output.protect(
        args.tree
            .parent()
            .ok_or("official tree needs a parent directory")?,
    )?;
    trust::require(
        args.compiler_revision.len() == 40
            && args
                .compiler_revision
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
        "invalid compiler revision",
    )?;
    let descriptions = inputs::descriptions(&args.descriptions, &args.descriptions_sha256)?;
    let official_tree = inputs::pinned_json(&args.tree, &args.tree_sha256, 16 * 1024 * 1024)?;
    let input = trust::verify(&args.facts)?;
    let candidate = convert::normalize(&input)?;
    let player = args
        .player
        .as_ref()
        .map(|args| player::verify(args, &input))
        .transpose()?;
    let patch = input.manifest["source_identity"]["game_patch"]
        .as_str()
        .ok_or("missing game patch")?;
    trust::require(
        descriptions.manifest["source_identity"]["game_patch"] == patch,
        "descriptions and native facts patches differ",
    )?;
    let (base, mut receipt) = assemble(
        &input,
        &candidate,
        &official_tree,
        &descriptions,
        player.as_ref(),
        &args,
    )?;
    // Every byte and every loader-visible domain is checked before reserving output.
    let mut memory = base
        .iter()
        .map(|(name, bytes)| (format!("base/{name}"), bytes.clone()))
        .collect::<BTreeMap<_, _>>();
    let manifest = manifest(patch, &base)?;
    memory.insert("manifest.json".into(), manifest.clone());
    let identity = validate_runtime(memory)?;
    if player.is_some() {
        receipt["format"] = "pobr-native-runtime-assembly-receipt/v2".into();
        receipt["data_identity"] = serde_json::to_value(&identity).map_err(|e| e.to_string())?;
        receipt["actor_policy"] = "selected-player-generic-ot-v1".into();
        receipt["evaluation"] = "conditional-skill-hit-v1".into();
    }
    let receipt_bytes = json_bytes(&receipt)?;
    output.reserve()?;
    output.write_base(&base)?;
    output.write_new("manifest.json", &manifest)?;
    output.write_new("assembly-receipt.json", &receipt_bytes)?;
    output.verify_namespace()?;
    Ok(format!(
        "guarded native runtime catalog: {}; receipt sha256 {} (calculation not admitted)",
        args.facts.out.display(),
        sha256_hex(&receipt_bytes)
    ))
}

type Files = BTreeMap<String, Vec<u8>>;

fn assemble(
    input: &trust::VerifiedInput,
    candidate: &super::model::Candidate,
    official: &Value,
    descriptions: &inputs::Descriptions,
    player: Option<&player::Player>,
    args: &Args,
) -> Result<(Files, Value), String> {
    let source_sha = sha256_hex(&input.manifest_bytes);
    let patch = input.manifest["source_identity"]["game_patch"]
        .as_str()
        .ok_or("missing patch")?;
    let projected_tree = tree::project(input, candidate, official, &source_sha, patch)?;
    let mut base = skills::project(input)?;
    base.insert(
        "passive_tree.json".into(),
        json_bytes(&projected_tree.nodes)?,
    );
    base.insert(
        "passive_tree_meta.json".into(),
        json_bytes(&projected_tree.meta)?,
    );
    let implementation = implementation_sha256();
    let mut lock = json!({
        "format":"pobr-native-runtime-input-lock/v1",
        "game":"poe2", "realm":"INTERNATIONAL", "game_patch":patch,
        "facts":{"manifest_sha256":source_sha,"source_lock_sha256":sha256_hex(&input.lock_bytes)},
        "descriptions":{"manifest_sha256":args.descriptions_sha256,
            "source_lock_sha256":descriptions.manifest["source_identity"]["source_lock_sha256"]},
        "official_tree":{"sha256":args.tree_sha256,"game_patch":null,
            "fields":["coordinates"],"match_policy":"exact-ids-directed-edges-orbits-class-pairs; rounded-group-centres"},
        "compiler":{"revision_declaration":args.compiler_revision,"implementation_sha256":implementation},
        "profile":"sorceress-spark-level1-q0-v1"
    });
    let mut coverage = json!({
        "format":"pobr-native-runtime-coverage/v1", "production_ready":false,
        "calculation_admitted":false, "source_status":"unreviewed-acquisition-draft",
        "facts":{"tables":77,"passive_definitions":candidate.passives.len(),
            "graph_instances":candidate.instances.len(),"class_relations":candidate.class_relations.len()},
        "projected":{"tree_instances":projected_tree.nodes.len(),
            "native_passive_packets":projected_tree.nodes.iter().filter(|node| node.native_stats.is_some()).count(),
            "gem":"Metadata/Items/Gems/SkillGemSpark","effect":"SparkPlayer",
            "branch":"SparkPlayer","gem_level":1,"quality":0},
        "missing_or_excluded":[
            "Calculation admission, actor/innate constants, exact build/save/export dataset binding.",
            "General stat description selection/formatting; tree modifier text is intentionally absent.",
            "Other passive semantics, variants, ascendancy ownership, unlock constraints and attribute choices.",
            "Cold-infused Spark and other gems/levels/quality/supports/equipment/encounter conditions.",
            "Jewel radius semantics, projectile collision/re-hit counts, effective server cadence and CN numerical facts."
        ],
        "policy":"All unprojected data remains in the exact referenced canonical archive; absence is not effectless."
    });
    let mut references = json!({
        "format":"pobr-native-runtime-reference-index/v1",
        "archive":{"manifest_sha256":source_sha,"files":input.manifest["files"]},
        "descriptions":{"manifest_sha256":args.descriptions_sha256,"files":descriptions.manifest["files"],
            "status":"pinned but no formatter or branch selection performed"},
        "passive_definitions":candidate.passives,
        "graph_instances":candidate.instances,
        "class_relations":candidate.class_relations,
        "skill_selection":{"gem":{"table":"SkillGems","base_id":"Metadata/Items/Gems/SkillGemSpark"},
            "effect":"SparkPlayer","stat_set":"SparkPlayer","level":1,
            "evidence":"Physical stable-ID joins; BaseResolvedValues, never FloatStatsValues, supplies resolved damage."}
    });
    let mut normalization = json!({
        "format":"pobr-native-normalization/v1", "profile":"spark-level1-q0", "version":1,
        "rules":[
            "Stable-ID gem -> GemEffects -> GrantedEffects -> primary StatSet; no additional-set merging.",
            "Level 1 BaseResolvedValues aligns exactly with FloatStats; AdditionalStatsValues aligns exactly with AdditionalStats.",
            "Retain constant and implicit stats in separate channels; do not multiply stored damage by effectiveness again.",
            "CastTime, Cooldown and AttackTime remain milliseconds in runtime catalogs; zero durations become absent.",
            "SpellCritChance / 100 gives percentage points; BaseMultiplier / 10000 + 1 gives damage multiplier.",
            "Native raw base_number_of_projectiles is a total, not an additional count.",
            "Native raw base_skill_effect_duration is milliseconds; Rust semantics converts to seconds.",
            "No vendor ordinal or compatibility statSetIndex is assigned; the branch retains its stable set_id.",
            "Native passive packets require one complete stat, the reviewed ordinary raw-context cohort, and an existing exact core rule; no numerical meaning is inferred from node labels."
        ]
    });
    let mut rules = json!({"format":"pobr-native-rule-requirements/v1", "profile":"spark-level1-q0","version":1,
        "calculation_admitted":false,
        "required_own_stat_rules":["spell_minimum_base_lightning_damage","spell_maximum_base_lightning_damage",
            "base_number_of_projectiles","base_skill_effect_duration"],
        "passive_rule":"base_fire_damage_resistance_%",
        "passive_rules":projected_tree.nodes.iter().filter_map(|node| node.native_stats.as_ref())
            .flat_map(|packet| packet.stats.iter().map(|stat| stat.id.as_str())).collect::<BTreeSet<_>>(),
        "policy":"This is a required-rule declaration, not certification of engine coverage or game correctness."});
    let mut acquisition = json!({"facts":input.manifest["source_identity"]["acquisition"],
        "descriptions":descriptions.manifest["source_identity"]["acquisition"]});
    let player_pins = if let Some(player) = player {
        let context = &player.context;
        let context_bytes = json_bytes(context)?;
        let context_sha = sha256_hex(&context_bytes);
        let binding = player_policy::bind(context, context_sha.clone())?;
        let binding_bytes = json_bytes(&binding)?;
        let pins = NativePlayerArtifacts {
            context_sha256: context_sha,
            actor_binding_sha256: sha256_hex(&binding_bytes),
            source_manifest_sha256: context.source_manifest_sha256.clone(),
            source_lock_sha256: context.source_lock_sha256.clone(),
            parent_manifest_sha256: context.parent_manifest_sha256.clone(),
            parent_source_lock_sha256: context.parent_source_lock_sha256.clone(),
        };
        base.insert("native_player_context.json".into(), context_bytes);
        base.insert("native_actor_binding.json".into(), binding_bytes);
        lock["format"] = "pobr-native-runtime-input-lock/v2".into();
        lock["player"] = json!({"manifest_sha256":pins.source_manifest_sha256,
            "source_lock_sha256":pins.source_lock_sha256,"parent_manifest_sha256":pins.parent_manifest_sha256,
            "parent_source_lock_sha256":pins.parent_source_lock_sha256,
            "producer_revision":player::PRODUCER_REVISION,
            "actor_policy":"selected-player-generic-ot-v1", "actor_binding_sha256":pins.actor_binding_sha256});
        coverage["format"] = "pobr-native-runtime-coverage/v2".into();
        coverage["player"] = json!({"context_sha256":pins.context_sha256,
            "actor_binding_sha256":pins.actor_binding_sha256,"model_status":"conditional-model",
            "raw_stats":context.facts.metadata_stats.len(),"starting_relations":context.facts.all_starting_relations.len(),
            "policy":"Only selected generic OT critical bonus is normalized; every fact is retained with a finite profile disposition. Loading is not admission."});
        coverage["missing_or_excluded"][0] = "Build calculation admission/persistence, actor pools/accuracy/requirements and effective cadence remain unavailable.".into();
        references["format"] = "pobr-native-runtime-reference-index/v2".into();
        references["player"] = json!({"manifest_sha256":pins.source_manifest_sha256,"files":player.manifest["files"],
            "parent_manifest_sha256":pins.parent_manifest_sha256,"origin_policy":"player and parent namespaces are resolved separately",
            "retained_archive":"base/native_player_context.json#/source_archive"});
        normalization["format"] = "pobr-native-normalization/v2".into();
        normalization["version"] = 2.into();
        normalization["actor_policy"] = json!({"id":"selected-player-generic-ot-v1","artifact_sha256":pins.actor_binding_sha256,
            "rule":"Parse the sole plain generic OT base_critical_hit_damage_bonus lexical token as a finite explicit extra-damage bonus. Never infer AO-to-OT inheritance or full constants."});
        rules["format"] = "pobr-native-rule-requirements/v2".into();
        rules["version"] = 2.into();
        rules["actor_policy_sha256"] = pins.actor_binding_sha256.clone().into();
        rules["evaluation"] = "conditional-skill-hit-v1".into();
        acquisition["player"] = player.manifest["source_identity"]["acquisition"].clone();
        Some(pins)
    } else {
        None
    };
    for (name, value) in [
        ("native_input_lock", lock),
        ("native_coverage", coverage),
        ("native_reference_index", references),
        ("native_normalization", normalization),
        ("native_rule_requirements", rules),
        ("native_acquisition", acquisition),
        ("native_tree_match", projected_tree.match_evidence),
    ] {
        base.insert(format!("{name}.json"), json_bytes(&value)?);
    }
    let digest = |name: &str| sha256_hex(&base[&format!("{name}.json")]);
    let tree_resource = "Metadata/PassiveSkillGraph.psg";
    let mut contract = NativeRuntimeContract {
        schema_version: if player_pins.is_some() {
            NATIVE_PLAYER_CONTRACT_VERSION
        } else {
            NATIVE_RUNTIME_CONTRACT_VERSION
        },
        game: NativeGame::Poe2,
        realm: NativeRealm::International,
        game_patch: patch.into(),
        assembly_key: String::new(),
        source_lock_sha256: digest("native_input_lock"),
        descriptions_manifest_sha256: args.descriptions_sha256.clone(),
        compiler_revision: args.compiler_revision.clone(),
        tree: NativeTreeIdentity {
            source_manifest_sha256: source_sha.clone(),
            resource: tree_resource.into(),
            resource_sha256:
                input.manifest["source_identity"]["raw_resources"][tree_resource]["sha256"]
                    .as_str()
                    .ok_or("missing raw graph hash")?
                    .into(),
            compatibility_tag: "0_5".into(),
        },
        allowed_source_manifests: player_pins
            .iter()
            .flat_map(|p| {
                [
                    p.source_manifest_sha256.clone(),
                    args.descriptions_sha256.clone(),
                ]
            })
            .chain([source_sha])
            .collect(),
        normalization: NativeProfilePin {
            profile: NativeProfile::SparkLevel1Q0,
            version: if player_pins.is_some() { 2 } else { 1 },
            sha256: digest("native_normalization"),
        },
        rules: NativeProfilePin {
            profile: NativeProfile::SparkLevel1Q0,
            version: if player_pins.is_some() { 2 } else { 1 },
            sha256: digest("native_rule_requirements"),
        },
        input_profile: NativeInputProfile::SorceressSparkLevel1Q0V1,
        output_profile: if player_pins.is_some() {
            NativeOutputProfile::ConditionalSkillHitV1
        } else {
            NativeOutputProfile::SparkSingleHitCastCostV1
        },
        coverage_sha256: digest("native_coverage"),
        reference_index_sha256: digest("native_reference_index"),
        acquisition: NativeAcquisition {
            status: NativeAcquisitionStatus::UnreviewedAcquisitionDraft,
            evidence_sha256: digest("native_acquisition"),
        },
        player: player_pins,
        supplements: vec![NativeTreeSupplement {
            sha256: args.tree_sha256.clone(),
            game_patch: None,
            fields: BTreeSet::from([NativeTreeSupplementField::Coordinates]),
            match_evidence_sha256: digest("native_tree_match"),
        }],
    };
    contract.assembly_key = pobr_gamedata::manifest::native_assembly_key(&contract);
    base.insert("native_runtime.json".into(), json_bytes(&contract)?);
    let receipt = json!({"format":"pobr-native-runtime-assembly-receipt/v1", "production_ready":false,
        "calculation_admitted":false, "assembly_key":contract.assembly_key,
        "compiler":{"revision_declaration":args.compiler_revision,"implementation_sha256":implementation},
        "manifest_sha256":sha256_hex(&manifest(patch,&base)?),
        "input_lock_sha256":contract.source_lock_sha256,
        "scope":"Queryable scoped catalogs only; no active data/default switch, source approval or calculation admission."});
    Ok((base, receipt))
}

fn manifest(patch: &str, base: &Files) -> Result<Vec<u8>, String> {
    json_bytes(&DataManifest {
        schema_version: NATIVE_CATALOG_SCHEMA_VERSION,
        poe_version: patch.into(),
        languages: vec![],
        domains: DomainSections {
            base: base
                .keys()
                .map(|name| name.trim_end_matches(".json").into())
                .collect(),
            ..Default::default()
        },
        files: base
            .iter()
            .map(|(name, bytes)| (format!("base/{name}"), sha256_hex(bytes)))
            .collect(),
    })
}

fn validate_runtime(files: Files) -> Result<pobr_gamedata::DataIdentity, String> {
    let data = GameData::from_memory(files);
    let check = || -> Result<(), pobr_gamedata::LoadError> {
        data.validate_manifest()?;
        pobr_gamedata::NativeData::load(&data)?;
        data.passive_nodes()?;
        data.passive_tree_meta()?;
        data.base_items()?;
        data.skill_gems()?;
        data.granted_effects()?;
        data.granted_effect_levels()?;
        data.skill_stat_sets()?;
        data.cost_types()?;
        Ok(())
    };
    check().map_err(|e| format!("assembled catalog validation: {e}"))?;
    trust::require(
        data.validate_calculation_admission().is_err(),
        "native calculation must remain unadmitted",
    )?;
    data.identity().cloned().map_err(|e| e.to_string())
}

fn implementation_sha256() -> String {
    let mut source = super::implementation_sha256().into_bytes();
    for (name, bytes) in [
        ("runtime/mod.rs", include_bytes!("mod.rs").as_slice()),
        ("runtime/inputs.rs", include_bytes!("inputs.rs").as_slice()),
        ("runtime/tree.rs", include_bytes!("tree.rs").as_slice()),
        (
            "core/rules/passive_stats.rs",
            include_bytes!("../../../../../crates/pobr-core/src/rules/passive_stats.rs").as_slice(),
        ),
        ("runtime/skills.rs", include_bytes!("skills.rs").as_slice()),
        ("runtime/player.rs", include_bytes!("player.rs").as_slice()),
        (
            "player-producer-manifest.json",
            include_bytes!("../../../tests/fixtures/native-facts/player-context-manifest.json")
                .as_slice(),
        ),
        (
            "runtime/player_policy.rs",
            include_bytes!("player_policy.rs").as_slice(),
        ),
        (
            "runtime/player-stat-policy.json",
            include_bytes!("player-stat-policy.json").as_slice(),
        ),
        (
            "catalog/native_player.rs",
            include_bytes!("../../../../../crates/pobr-data/src/catalog/native_player.rs")
                .as_slice(),
        ),
        (
            "gamedata/native.rs",
            include_bytes!("../../../../../crates/pobr-gamedata/src/native.rs").as_slice(),
        ),
        (
            "gamedata/native/unique_json.rs",
            include_bytes!("../../../../../crates/pobr-gamedata/src/native/unique_json.rs")
                .as_slice(),
        ),
        ("main.rs", include_bytes!("../../main.rs").as_slice()),
    ] {
        source.extend_from_slice(name.as_bytes());
        source.push(0);
        source.extend_from_slice(bytes);
        source.push(0);
    }
    sha256_hex(&source)
}
