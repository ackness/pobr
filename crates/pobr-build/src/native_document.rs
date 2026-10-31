//! Native build documents reuse [`Build`] and bind exact runtime bytes and selections.
//!
//! Decoding or matching a dataset never admits calculation. A native consumer must
//! resolve the request again against its immutable native catalogs and model policy.
//! The compatibility XML/code importers do not consume this document format.

use std::fmt;

pub use pobr_data::catalog::native_player::NativeEvaluationKind;
use pobr_data::catalog::native_runtime::{NativeGame, NativeRealm};
use pobr_data::item::Item;
use pobr_gamedata::{GameData, LoadError};
use serde::{Deserialize, Deserializer, Serialize, de};
use serde_json::{Map, Number, Value};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{Build, build_config::BuildConfig};

#[cfg(test)]
mod tests;

/// A bounded document input, independently of compressed PoB build-code limits.
pub const MAX_NATIVE_BUILD_BYTES: usize = 16 * 1024 * 1024;

/// Explicit native document discriminator. Unknown versions never become XML/code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NativeBuildSchema {
    #[serde(rename = "pobr-native-build/v1")]
    V1,
}

/// Exact dataset binding, distinct from display language and compatibility tags.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeDatasetBinding {
    pub game: NativeGame,
    pub realm: NativeRealm,
    pub game_patch: String,
    pub tree_resource_sha256: String,
    pub assembly_key: String,
    pub runtime_identity_sha256: String,
}

impl NativeDatasetBinding {
    /// Binds a supplied immutable native pack. It never searches CURRENT or admits it.
    pub fn from_game_data(data: &GameData) -> Result<Self, NativeDocumentError> {
        let contract = data.native_runtime()?.ok_or_else(|| {
            NativeDocumentError::Invalid("native document requires a native dataset".into())
        })?;
        let binding = Self {
            game: contract.game,
            realm: contract.realm,
            game_patch: contract.game_patch.clone(),
            tree_resource_sha256: contract.tree.resource_sha256.clone(),
            assembly_key: contract.assembly_key.clone(),
            runtime_identity_sha256: data.identity()?.sha256.clone(),
        };
        binding.validate()?;
        Ok(binding)
    }

    /// Rejects missing/different datasets, including same-patch replacements.
    pub fn matches_game_data(&self, data: &GameData) -> Result<(), NativeDocumentError> {
        self.validate()?;
        if self != &Self::from_game_data(data)? {
            return invalid("native dataset differs from the saved exact binding");
        }
        Ok(())
    }

    fn validate(&self) -> Result<(), NativeDocumentError> {
        let parts: Vec<_> = self.game_patch.split('.').collect();
        if parts.len() < 2
            || !parts
                .iter()
                .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        {
            return invalid("native dataset requires an explicit dotted game patch");
        }
        for pin in [
            &self.tree_resource_sha256,
            &self.assembly_key,
            &self.runtime_identity_sha256,
        ] {
            digest(pin)?;
        }
        Ok(())
    }
}

/// Stable actor identity; the mirrored Build level must agree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeActorSelection {
    pub id: String,
    pub level: u32,
}

/// Location is zero-based; identities and exact levels do not use PoB ordinals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeGemSelection {
    pub group_index: usize,
    pub gem_index: usize,
    pub gem_id: String,
    pub effect_id: String,
    pub stat_set_id: String,
    pub level: u32,
    pub quality: u32,
}

/// Named model policy and exact artifact digest, independent of game-source approval.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeActorPolicyPin {
    pub id: String,
    pub version: u32,
    pub sha256: String,
}

/// Editable/saveable native request. This public DTO is deliberately not admission.
/// Every edit and reopen must pass the later native catalog/profile admission again.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeBuildRequest {
    pub schema: NativeBuildSchema,
    pub dataset: NativeDatasetBinding,
    pub actor: NativeActorSelection,
    pub skill: NativeGemSelection,
    pub actor_policy: NativeActorPolicyPin,
    pub evaluation: NativeEvaluationKind,
    pub build: Build,
}

/// Errors preserve JSON/schema failures separately from exact-binding failures.
#[derive(Debug, Error)]
pub enum NativeDocumentError {
    #[error("invalid native build document: {0}")]
    Invalid(String),
    #[error("invalid native build JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("native dataset is unavailable: {0}")]
    Data(#[from] LoadError),
}

fn invalid<T>(message: impl Into<String>) -> Result<T, NativeDocumentError> {
    Err(NativeDocumentError::Invalid(message.into()))
}

fn digest(value: &str) -> Result<(), NativeDocumentError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return invalid("expected a full lowercase SHA-256 binding");
    }
    Ok(())
}

fn stable_id(value: &str) -> Result<(), NativeDocumentError> {
    if value.trim().is_empty() || value.len() > 1024 || value.chars().any(char::is_control) {
        return invalid("native identity must be a nonempty stable ID");
    }
    Ok(())
}

impl NativeBuildRequest {
    /// Checks structural/selection agreement only, without loading rules or defaults.
    pub fn validate_document(&self) -> Result<(), NativeDocumentError> {
        self.dataset.validate()?;
        for id in [
            &self.actor.id,
            &self.skill.gem_id,
            &self.skill.effect_id,
            &self.skill.stat_set_id,
            &self.actor_policy.id,
        ] {
            stable_id(id)?;
        }
        digest(&self.actor_policy.sha256)?;
        if self.actor_policy.version == 0 || self.actor.level == 0 || self.skill.level == 0 {
            return invalid(
                "native levels and model-policy version must be explicit positive values",
            );
        }
        if self.actor.level != self.build.character.level {
            return invalid("native actor level conflicts with the Build");
        }
        let group = self
            .build
            .socket_groups
            .get(self.skill.group_index)
            .ok_or_else(|| {
                NativeDocumentError::Invalid("selected native group is absent".into())
            })?;
        let gem = group
            .gem_skills
            .get(self.skill.gem_index)
            .ok_or_else(|| NativeDocumentError::Invalid("selected native gem is absent".into()))?;
        if group.gem_ids.get(self.skill.gem_index) != Some(&self.skill.gem_id)
            || gem.skill_id != self.skill.effect_id
            || gem.gem_level != self.skill.level
            || gem.quality != self.skill.quality
        {
            return invalid("native gem identity/level/quality conflicts with the Build");
        }
        finite_build(&self.build)
    }

    /// Deterministic JSON: complete existing Build content, stable object ordering.
    /// Inactive/unsupported state is retained; this does not admit its calculation.
    pub fn to_json(&self) -> Result<Vec<u8>, NativeDocumentError> {
        self.validate_document()?;
        let mut value = serde_json::to_value(self)?;
        value.sort_all_objects();
        let mut bytes = serde_json::to_vec(&value)?;
        bytes.push(b'\n');
        if bytes.len() > MAX_NATIVE_BUILD_BYTES {
            return invalid("native build document exceeds the byte limit");
        }
        Ok(bytes)
    }

    /// Input fingerprint, not an engine/result-cache identity. A cache must also
    /// bind the executing engine/model implementation or recompute the result.
    pub fn input_sha256(&self) -> Result<String, NativeDocumentError> {
        Ok(Sha256::digest(self.to_json()?)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect())
    }

    /// Strict decoding never repairs, imports XML, or selects another snapshot.
    pub fn from_json(bytes: &[u8]) -> Result<Self, NativeDocumentError> {
        if bytes.len() > MAX_NATIVE_BUILD_BYTES {
            return invalid("native build document exceeds the byte limit");
        }
        let original = serde_json::from_slice::<UniqueValue>(bytes)?.0;
        let document: Self = serde_json::from_value(original.clone())?;
        // Existing Party transport deliberately has compatibility defaults. A
        // native document instead requires every emitted field, including nulls,
        // and rejects any ignored nested field rather than silently losing it.
        exact_shape(&original, &serde_json::to_value(&document)?, "$")?;
        document.validate_document()?;
        Ok(document)
    }
}

fn exact_shape(input: &Value, typed: &Value, path: &str) -> Result<(), NativeDocumentError> {
    match (input, typed) {
        (Value::Object(a), Value::Object(b)) => {
            if a.len() != b.len() || a.keys().any(|key| !b.contains_key(key)) {
                return invalid(format!("missing or ignored document field at {path}"));
            }
            for (name, value) in a {
                exact_shape(value, &b[name], &format!("{path}/{name}"))?;
            }
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            for (index, (a, b)) in a.iter().zip(b).enumerate() {
                exact_shape(a, b, &format!("{path}/{index}"))?;
            }
        }
        (Value::Number(_), Value::Number(_))
        | (Value::String(_), Value::String(_))
        | (Value::Bool(_), Value::Bool(_))
        | (Value::Null, Value::Null) => {}
        _ => return invalid(format!("document representation changed at {path}")),
    }
    Ok(())
}

fn finite(value: f64) -> Result<(), NativeDocumentError> {
    if value.is_finite() {
        Ok(())
    } else {
        invalid("nonfinite Build number cannot be saved as JSON null")
    }
}

fn finite_item(item: &Item) -> Result<(), NativeDocumentError> {
    let pobr_data::item::RolledDefence {
        armour,
        evasion,
        energy_shield,
        spirit,
        ward,
    } = item.rolled_defence;
    for value in [armour, evasion, energy_shield, spirit, ward]
        .into_iter()
        .flatten()
    {
        finite(value)?;
    }
    Ok(())
}

fn finite_config(config: &BuildConfig) -> Result<(), NativeDocumentError> {
    for &value in config.multipliers.values() {
        finite(value)?;
    }
    for value in config
        .raw_inputs
        .values
        .values()
        .chain(config.raw_inputs.placeholders.values())
    {
        if let pobr_core::rules::config_interpreter::ConfigInputValue::Number(value) = value {
            finite(*value)?;
        }
    }
    Ok(())
}

fn finite_build(build: &Build) -> Result<(), NativeDocumentError> {
    // Enumerate all Build fields so future model additions revisit this boundary.
    let Build {
        character: _,
        view_mode: _,
        tree: _,
        tree_version: _,
        items,
        jewels,
        granted_socket_jewels,
        radius_jewels: _,
        utility_slots,
        inactive_utility_slots,
        limbs,
        weapon_set: _,
        other_weapons,
        other_weapon_set_nodes: _,
        socket_groups,
        main_socket_group: _,
        config,
        party: _,
    } = build;
    for item in items
        .values()
        .chain(other_weapons.values())
        .chain(limbs.values())
    {
        finite_item(item)?;
    }
    for item in jewels
        .iter()
        .map(|(_, item)| item)
        .chain(utility_slots.iter().map(|(_, item)| item))
        .chain(inactive_utility_slots.iter().map(|(_, item)| item))
    {
        finite_item(item)?;
    }
    for (_, item, _) in granted_socket_jewels {
        finite_item(item)?;
    }
    for group in socket_groups {
        if let Some(value) = group.group_count {
            finite(value)?;
        }
        for gem in &group.gem_skills {
            if let Some(value) = gem.count {
                finite(value)?;
            }
        }
    }
    finite_config(config)
}

/// Duplicate keys must be rejected before serde compatibility defaults can lose them.
struct UniqueValue(Value);

impl<'de> Deserialize<'de> for UniqueValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = UniqueValue;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("JSON with unique object keys")
            }
            fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut values = Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(de::Error::custom(format!("duplicate JSON key {key}")));
                    }
                    values.insert(key, map.next_value::<UniqueValue>()?.0);
                }
                Ok(UniqueValue(Value::Object(values)))
            }
            fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(value) = seq.next_element::<UniqueValue>()? {
                    values.push(value.0);
                }
                Ok(UniqueValue(Value::Array(values)))
            }
            fn visit_bool<E: de::Error>(self, value: bool) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::Bool(value)))
            }
            fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::Number(value.into())))
            }
            fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::Number(value.into())))
            }
            fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
                Number::from_f64(value)
                    .map(|value| UniqueValue(Value::Number(value)))
                    .ok_or_else(|| de::Error::custom("nonfinite JSON number"))
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::String(value.into())))
            }
            fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::String(value)))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::Null))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}
