//! Immutable native catalogs. Loading verifies facts and policy; it never admits calculation.

use std::collections::{BTreeMap, BTreeSet};

use pobr_data::catalog::native_player::{
    NATIVE_ACTOR_BINDING_PATH, NATIVE_GENERIC_OT, NATIVE_PLAYER_AO, NATIVE_PLAYER_CONTEXT_PATH,
    NATIVE_PLAYER_ID, NativeActorBinding, NativeActorPolicy, NativeEvaluationKind,
    NativeModelStatus, NativePlayerContextDef, NativePlayerOrigin,
};
use pobr_data::catalog::{
    BaseItemDef, CostTypeDef, DataManifest, GrantedEffectDef, NativeRuntimeContract,
    PassiveNodeDef, PassiveTreeMeta, SkillGemDef, SkillLevelDef, SkillStatSetDef,
};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::{DataIdentity, GameData, LoadError, manifest::sha256_hex};

/// Validated native input snapshot with private state and borrowed typed catalog access.
/// There is no default, deserialization, legacy constants bundle or calculation authority.
#[derive(Debug)]
pub struct NativeData {
    identity: DataIdentity,
    contract: NativeRuntimeContract,
    player_context: Option<NativePlayerContextDef>,
    actor_binding: Option<NativeActorBinding>,
    passive_nodes: Vec<PassiveNodeDef>,
    passive_tree_meta: PassiveTreeMeta,
    base_items: Vec<BaseItemDef>,
    skill_gems: Vec<SkillGemDef>,
    granted_effects: Vec<GrantedEffectDef>,
    granted_effect_levels: BTreeMap<String, Vec<SkillLevelDef>>,
    skill_stat_sets: Vec<SkillStatSetDef>,
    cost_types: Vec<CostTypeDef>,
}

impl NativeData {
    /// Loads all projected domains using the existing disk or memory backend.
    ///
    /// # Errors
    /// Rejects legacy packs, malformed native artifacts, changed bytes and patch overlays.
    /// Contract v1 remains queryable with absent player context and binding.
    pub fn load(data: &GameData) -> Result<Self, LoadError> {
        let manifest = data.validate_manifest()?;
        let contract = data
            .native_runtime()?
            .ok_or_else(|| invalid(data, "NativeData requires a native schema-4 snapshot"))?
            .clone();
        reject_patches(data, &manifest)?;
        let player = validate_player(data, &manifest, &contract)?;
        let (player_context, actor_binding) = match player {
            Some((context, binding)) => (Some(context), Some(binding)),
            None => (None, None),
        };
        Ok(Self {
            identity: data.identity()?.clone(),
            contract,
            player_context,
            actor_binding,
            passive_nodes: sealed(data, &manifest, "base/passive_tree.json")?,
            passive_tree_meta: sealed(data, &manifest, "base/passive_tree_meta.json")?,
            base_items: sealed(data, &manifest, "base/base_items.json")?,
            skill_gems: sealed(data, &manifest, "base/skill_gems.json")?,
            granted_effects: sealed(data, &manifest, "base/granted_effects.json")?,
            granted_effect_levels: sealed(data, &manifest, "base/granted_effect_levels.json")?,
            skill_stat_sets: sealed(data, &manifest, "base/granted_effect_stat_sets.json")?,
            cost_types: sealed(data, &manifest, "base/cost_types.json")?,
        })
    }

    /// Exact final runtime byte identity, distinct from the assembly key.
    pub fn identity(&self) -> &DataIdentity {
        &self.identity
    }
    /// Sealed assembly and transitive source/policy pins.
    pub fn contract(&self) -> &NativeRuntimeContract {
        &self.contract
    }
    /// Full retained player context; absent for v1, never replaced by defaults.
    pub fn player_context(&self) -> Option<&NativePlayerContextDef> {
        self.player_context.as_ref()
    }
    /// Conditional-model applicability policy; absent for v1, never admission authority.
    pub fn actor_binding(&self) -> Option<&NativeActorBinding> {
        self.actor_binding.as_ref()
    }
    /// Exact policy artifact byte pin; callers bind requests to this and the dataset identity.
    pub fn actor_binding_sha256(&self) -> Option<&str> {
        self.contract
            .player
            .as_ref()
            .map(|pins| pins.actor_binding_sha256.as_str())
    }
    /// Projected passive graph nodes.
    pub fn passive_nodes(&self) -> &[PassiveNodeDef] {
        &self.passive_nodes
    }
    /// Projected tree metadata.
    pub fn passive_tree_meta(&self) -> &PassiveTreeMeta {
        &self.passive_tree_meta
    }
    /// Projected base items.
    pub fn base_items(&self) -> &[BaseItemDef] {
        &self.base_items
    }
    /// Projected gems.
    pub fn skill_gems(&self) -> &[SkillGemDef] {
        &self.skill_gems
    }
    /// Projected granted effects.
    pub fn granted_effects(&self) -> &[GrantedEffectDef] {
        &self.granted_effects
    }
    /// Projected exact effect-level rows.
    pub fn granted_effect_levels(&self) -> &BTreeMap<String, Vec<SkillLevelDef>> {
        &self.granted_effect_levels
    }
    /// Projected stable stat-set branches.
    pub fn skill_stat_sets(&self) -> &[SkillStatSetDef] {
        &self.skill_stat_sets
    }
    /// Explicit source resource identities and divisors.
    pub fn cost_types(&self) -> &[CostTypeDef] {
        &self.cost_types
    }
}

fn invalid(data: &GameData, message: impl Into<String>) -> LoadError {
    LoadError::Integrity {
        path: data.root().join(NATIVE_PLAYER_CONTEXT_PATH),
        message: message.into(),
    }
}

fn sealed<T: DeserializeOwned>(
    data: &GameData,
    manifest: &DataManifest,
    relative: &str,
) -> Result<T, LoadError> {
    let expected = manifest
        .files
        .get(relative)
        .ok_or_else(|| invalid(data, format!("missing sealed native file {relative}")))?;
    let path = data.root().join(relative);
    let bytes = data.read_bytes(&path).map_err(|source| LoadError::Io {
        path: path.clone(),
        source,
    })?;
    if sha256_hex(&bytes) != *expected {
        return Err(invalid(
            data,
            format!("sealed native file changed: {relative}"),
        ));
    }
    serde_json::from_slice(&bytes).map_err(|source| LoadError::Parse { path, source })
}

pub(crate) fn reject_patches(data: &GameData, manifest: &DataManifest) -> Result<(), LoadError> {
    for relative in manifest.files.keys() {
        let path = data.root().join("patch").join(relative);
        match data.read_bytes(&path) {
            Ok(_) => return Err(invalid(data, "native catalogs cannot be patched")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => return Err(LoadError::Io { path, source }),
        }
    }
    Ok(())
}

pub(crate) fn validate_player(
    data: &GameData,
    manifest: &DataManifest,
    contract: &NativeRuntimeContract,
) -> Result<Option<(NativePlayerContextDef, NativeActorBinding)>, LoadError> {
    let Some(pins) = &contract.player else {
        return Ok(None);
    };
    reject_patches(data, manifest)?;
    if !manifest.domains.overlay.is_empty() || !manifest.domains.generated.is_empty() {
        return Err(invalid(
            data,
            "native player catalogs cannot include overlay/generated domains",
        ));
    }
    let c: NativePlayerContextDef = sealed(data, manifest, NATIVE_PLAYER_CONTEXT_PATH)?;
    let b: NativeActorBinding = sealed(data, manifest, NATIVE_ACTOR_BINDING_PATH)?;
    let require = |condition, message: &str| {
        if condition {
            Ok(())
        } else {
            Err(invalid(data, message))
        }
    };
    require(
        c.schema_version == 1
            && b.schema_version == 1
            && c.game == contract.game
            && c.realm == contract.realm
            && c.game_patch == contract.game_patch
            && c.source_manifest_sha256 == pins.source_manifest_sha256
            && c.source_lock_sha256 == pins.source_lock_sha256
            && c.parent_manifest_sha256 == pins.parent_manifest_sha256
            && c.parent_source_lock_sha256 == pins.parent_source_lock_sha256
            && c.parent_manifest_sha256 == contract.tree.source_manifest_sha256
            && b.context_sha256 == pins.context_sha256,
        "native player context/policy identity mismatch",
    )?;
    validate_archive(data, &c)?;
    let facts = &c.facts;
    require(
        facts.format == "repoe-native-player-context-facts/v1"
            && facts.character.source.id.as_deref() == Some(NATIVE_PLAYER_ID)
            && facts.character.source.row == 7
            && facts.character.source.origin == NativePlayerOrigin::Parent
            && facts.character.source.table == "Characters"
            && facts.character.source.payload == "tables/Characters.json"
            && facts.character.source.pointer == "/rows/7/values"
            && facts.character.values.get("Id").and_then(Value::as_str) == Some(NATIVE_PLAYER_ID)
            && facts.character.values.get("AOFile").and_then(Value::as_str)
                == Some(NATIVE_PLAYER_AO)
            && facts.metadata_roots.character_ao == NATIVE_PLAYER_AO
            && facts.metadata_roots.generic_stats_ot == NATIVE_GENERIC_OT
            && facts.selected_class_overrides.is_empty(),
        "native player must bind exact Character7 and separate AO/OT roots",
    )?;
    require(
        b.actor_id == NATIVE_PLAYER_ID
            && b.actor_level == 1
            && b.policy == NativeActorPolicy::SelectedPlayerGenericOtV1
            && b.evaluation == NativeEvaluationKind::ConditionalSkillHitV1
            && b.model_status == NativeModelStatus::ConditionalModel,
        "unsupported native actor policy/profile",
    )?;
    require(
        facts.all_starting_relations.len() == 6
            && facts
                .all_starting_relations
                .iter()
                .map(|r| r.options.len())
                .sum::<usize>()
                == 12
            && facts.all_starting_relations.get(2) == Some(&facts.starting_relation)
            && facts.starting_relation.source.row == 2
            && facts
                .starting_relation
                .options
                .iter()
                .map(|r| r.row)
                .collect::<Vec<_>>()
                == [2779, 1146]
            && facts.allocation_policy
                == "none; starting choices are raw relationships, not automatic grants"
            && facts.binding_policy
                == "none; AO inheritance and independent OT root are not combined or applied",
        "native starting choices cannot become grants or inheritance",
    )?;
    let critical = &b.critical_bonus;
    let assignments = c
        .metadata
        .files
        .iter()
        .flat_map(|(path, file)| {
            file.assignments
                .iter()
                .enumerate()
                .map(move |(i, a)| (path, i, a))
        })
        .filter(|(_, _, a)| a.key_raw == "base_critical_hit_damage_bonus")
        .collect::<Vec<_>>();
    require(
        assignments.len() == 1,
        "duplicate/ambiguous native critical assignment",
    )?;
    let (path, index, assignment) = assignments[0];
    let numeric = assignment.value_raw.parse::<f64>().map_err(|_| {
        invalid(
            data,
            "native critical bonus requires an explicit numeric token",
        )
    })?;
    require(
        numeric.is_finite()
            && critical.bonus_percent.is_finite()
            && numeric == critical.bonus_percent
            && path == NATIVE_GENERIC_OT
            && critical.source.path == *path
            && critical.source.assignment == index
            && assignment.plain_stats_scope
            && assignment.scope.len() == 1
            && assignment.scope[0].name_raw == "Stats"
            && critical.stat.id.as_deref() == Some("base_critical_hit_damage_bonus")
            && facts
                .metadata_stats
                .iter()
                .filter(|s| s.key_raw == "base_critical_hit_damage_bonus")
                .count()
                == 1
            && facts.metadata_stats.iter().any(|s| {
                s.path == *path
                    && s.assignment == index
                    && s.stat.as_ref() == Some(&critical.stat)
                    && s.value_raw == assignment.value_raw
                    && s.scope == assignment.scope
            }),
        "native critical bonus/source projection mismatch",
    )?;
    require(
        critical.meaning.len() == 2,
        "native critical glossary meaning missing",
    )?;
    for (r, id) in critical
        .meaning
        .iter()
        .zip(["Critical", "CriticalDamageBonus"])
    {
        let row = c
            .keyword_popups
            .rows
            .get(r.row)
            .ok_or_else(|| invalid(data, "native critical glossary row missing"))?;
        require(
            r.origin == NativePlayerOrigin::Player
                && r.table == "KeywordPopups"
                && r.payload == "KeywordPopups.json"
                && r.pointer == format!("/rows/{}/values", r.row)
                && r.id.as_deref() == Some(id)
                && row.row == r.row
                && row.values.get("Id").and_then(Value::as_str) == Some(id)
                && row
                    .values
                    .get("Definition")
                    .and_then(Value::as_str)
                    .is_some_and(|s| !s.is_empty()),
            "native critical glossary reference mismatch",
        )?;
    }
    let exclusions = [
        "gear",
        "supports",
        "allocations",
        "ascendancy",
        "infusion",
        "buffs",
        "charges",
        "rampage",
        "quests",
        "PvP",
        "delirium",
        "events",
        "movement",
        "cutscene",
        "external-modifiers",
    ];
    let conditions = [
        "exact-selected-actor-level1",
        "primary-SparkPlayer-level1-q0",
        "ordinary-stationary-self-cast",
        "Act1-no-quests",
        "successful-damaging-hit",
    ];
    require(
        b.excluded_sources.iter().map(String::as_str).eq(exclusions)
            && b.required_conditions
                .iter()
                .map(String::as_str)
                .eq(conditions)
            && !b.assumptions.is_empty(),
        "native policy predicates/exclusions incomplete",
    )?;
    let value = serde_json::to_value(&c).expect("typed source context serializes");
    let mut selectors = BTreeSet::new();
    for d in &b.dispositions {
        require(
            !d.reason.trim().is_empty()
                && value.pointer(&d.selector).is_some()
                && selectors.insert(d.selector.as_str()),
            "native fact disposition missing/ambiguous source",
        )?;
    }
    for i in 0..facts.metadata_stats.len() {
        require(
            selectors.contains(format!("/facts/metadata_stats/{i}").as_str()),
            "native raw Stats disposition missing",
        )?;
    }
    require(
        facts.metadata_stats.len() == 99,
        "native selected raw Stats inventory changed",
    )?;
    for (path, file) in &c.metadata.files {
        let escaped = path.replace('~', "~0").replace('/', "~1");
        let assigned = file
            .assignments
            .iter()
            .map(|a| a.line)
            .collect::<BTreeSet<_>>();
        for (i, record) in file.records.iter().enumerate() {
            if !assigned.contains(&record.line) {
                require(
                    selectors.contains(format!("/metadata/files/{escaped}/records/{i}").as_str()),
                    "native raw record disposition missing",
                )?;
            }
        }
        for i in 0..file.diagnostics.len() {
            require(
                selectors.contains(format!("/metadata/files/{escaped}/diagnostics/{i}").as_str()),
                "native source diagnostic disposition missing",
            )?;
        }
        for (i, a) in file.assignments.iter().enumerate() {
            if !a.plain_stats_scope {
                let path = path.replace('~', "~0").replace('/', "~1");
                require(
                    selectors.contains(format!("/metadata/files/{path}/assignments/{i}").as_str()),
                    "native scoped assignment disposition missing",
                )?;
            }
        }
    }
    for field in facts.character.values.keys() {
        let field = field.replace('~', "~0").replace('/', "~1");
        require(
            selectors.contains(format!("/facts/character/values/{field}").as_str()),
            "native character field disposition missing",
        )?;
    }
    for i in 0..facts.all_starting_relations.len() {
        require(
            selectors.contains(format!("/facts/all_starting_relations/{i}").as_str()),
            "native starting relation disposition missing",
        )?;
    }
    Ok(Some((c, b)))
}

fn validate_archive(data: &GameData, c: &NativePlayerContextDef) -> Result<(), LoadError> {
    let require = |condition, message: &str| {
        if condition {
            Ok(())
        } else {
            Err(invalid(data, message))
        }
    };
    let a = &c.source_archive;
    let raw = |name: &str| -> Result<Value, LoadError> {
        serde_json::from_str(
            a.get(name)
                .ok_or_else(|| invalid(data, format!("missing retained source {name}")))?,
        )
        .map_err(|e| invalid(data, e.to_string()))
    };
    let manifest = raw("manifest.json")?;
    require(
        a.len() == 8
            && sha256_hex(a["manifest.json"].as_bytes()) == c.source_manifest_sha256
            && sha256_hex(
                a.get("source-lock.json")
                    .ok_or_else(|| invalid(data, "missing retained player lock"))?
                    .as_bytes(),
            ) == c.source_lock_sha256
            && sha256_hex(
                a.get("parent-manifest.json")
                    .ok_or_else(|| invalid(data, "missing retained parent manifest"))?
                    .as_bytes(),
            ) == c.parent_manifest_sha256,
        "native retained source identity mismatch",
    )?;
    let files = manifest["files"]
        .as_object()
        .ok_or_else(|| invalid(data, "missing retained source inventory"))?;
    require(
        files.len() == 7,
        "native retained source inventory mismatch",
    )?;
    for (name, fp) in files {
        let bytes = a
            .get(name)
            .ok_or_else(|| invalid(data, "missing retained source payload"))?
            .as_bytes();
        require(
            fp["bytes"] == bytes.len() && fp["sha256"] == sha256_hex(bytes),
            "native retained source payload mismatch",
        )?;
    }
    for (name, value) in [
        ("player_context.json", serde_json::to_value(&c.facts)),
        ("metadata.json", serde_json::to_value(&c.metadata)),
        (
            "StartingPassiveSkills.json",
            serde_json::to_value(&c.starting_passive_skills),
        ),
        (
            "KeywordPopups.json",
            serde_json::to_value(&c.keyword_popups),
        ),
    ] {
        require(
            raw(name)? == value.expect("typed source DTO serializes"),
            "native typed/source archive projection mismatch",
        )?;
    }
    let parent = raw("parent-manifest.json")?;
    let identity = &manifest["source_identity"];
    require(
        manifest["format"] == "repoe-native-player-context/v1"
            && manifest["production_ready"] == false
            && manifest["status"] == "partial-candidate"
            && identity["game"] == "poe2"
            && identity["realm"] == "international"
            && identity["game_patch"] == c.game_patch
            && identity["source_lock_sha256"] == c.source_lock_sha256
            && identity["acquisition"]["status"] == "unreviewed-acquisition-draft"
            && manifest["parent"]["manifest_sha256"] == c.parent_manifest_sha256
            && manifest["parent"]["source_identity"] == parent["source_identity"]
            && parent["source_identity"]["source_lock_sha256"] == c.parent_source_lock_sha256,
        "native retained player/parent source declaration mismatch",
    )?;
    for field in ["game", "realm", "game_patch"] {
        require(
            identity[field] == parent["source_identity"][field],
            "native retained parent realm/patch mismatch",
        )?;
    }
    Ok(())
}
