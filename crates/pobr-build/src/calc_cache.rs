//! Calculation result cache: keys [`OutputTable`] by the combination of
//! [`BuildSnapshot::content_hash`] and an [`OrchestratorOptions`] hash.
//!
//! Calc orchestration is expensive; as long as the calc-relevant inputs don't change, we
//! just reuse the previous result. **Note**: `calculate`'s output is determined not only
//! by [`Build`] but also by `options` (the base MinimalInput in `base_input` plus
//! `extra_modifier_texts`). The cache key must therefore cover both — otherwise the same
//! Build with different options would incorrectly hit the first cached result (audit HIGH-4).
//!
//! The cache is a plain in-memory map (LRU capacity is optional), deterministic and with
//! no shared mutable global state: the caller owns a [`CalcCache`] instance and calls
//! `get_or_compute` explicitly.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use pobr_core::calc::OutputTable;

use crate::build::Build;
use crate::calc_orchestrator::{OrchestratorOptions, calculate};
use crate::error::BuildError;
use crate::snapshot::BuildSnapshot;

/// In-memory calculation cache. Keyed by content hash, values are computed [`OutputTable`]s.
#[derive(Debug, Default)]
pub struct CalcCache {
    entries: HashMap<u64, OutputTable>,
    /// Hit / miss counters, useful for diagnosing cache effectiveness.
    hits: u64,
    misses: u64,
}

impl CalcCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the cached result on a hit; otherwise computes, stores, then returns it.
    pub fn get_or_compute(
        &mut self,
        build: &Build,
        options: &OrchestratorOptions,
    ) -> Result<OutputTable, BuildError> {
        let snapshot_hash = BuildSnapshot::from_build(build).content_hash();
        let key = combined_key(snapshot_hash, options_hash(options));
        if let Some(cached) = self.entries.get(&key) {
            self.hits += 1;
            return Ok(cached.clone());
        }

        self.misses += 1;
        let output = calculate(build, options)?;
        self.entries.insert(key, output.clone());
        Ok(output)
    }

    /// Looks up the cache directly by the combined cache key, without triggering a
    /// computation. The key is derived from the build content hash and the options hash.
    pub fn peek(&self, key: u64) -> Option<&OutputTable> {
        self.entries.get(&key)
    }

    /// Clears the cache and its stats.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.hits = 0;
        self.misses = 0;
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn hits(&self) -> u64 {
        self.hits
    }

    pub fn misses(&self) -> u64 {
        self.misses
    }
}

/// Deterministic hash of `OrchestratorOptions`.
///
/// Neither `OrchestratorOptions` nor `MinimalInput` derive `Serialize` or `Hash` (the
/// latter can't be derived directly because of its `f64` fields), so we feed the hasher
/// field by field:
/// - `extra_modifier_texts` (`Vec<String>` already implements `Hash`) is hashed directly;
/// - each `f64` field of `base_input` is fed via `to_bits()` (bitwise equality, working
///   around `f64` not implementing `Eq`/`Hash`).
///
/// **Maintenance note**: whenever `MinimalInput` gets a new field, it must be added here
/// too, or the new field won't be part of the cache key and could cause cache aliasing.
/// If these types ever derive `Serialize`, this could switch to feeding a stable
/// serialization (e.g. `serde_json::to_vec`) into the hasher, covering all fields automatically.
fn options_hash(options: &OrchestratorOptions) -> u64 {
    let mut hasher = DefaultHasher::new();
    options.extra_modifier_texts.hash(&mut hasher);

    let bi = &options.base_input;
    for bits in [
        bi.base_life.to_bits(),
        bi.base_mana.to_bits(),
        bi.base_fire_resistance.to_bits(),
        bi.base_cold_resistance.to_bits(),
        bi.base_lightning_resistance.to_bits(),
        bi.base_accuracy.to_bits(),
        bi.enemy_evasion.to_bits(),
        bi.base_hit_min.to_bits(),
        bi.base_hit_max.to_bits(),
        bi.base_action_rate.to_bits(),
    ] {
        hasher.write_u64(bits);
    }
    hasher.finish()
}

/// Combines the build content hash and the options hash into the final cache key.
fn combined_key(snapshot_hash: u64, options_hash: u64) -> u64 {
    let mut hasher = DefaultHasher::new();
    hasher.write_u64(snapshot_hash);
    hasher.write_u64(options_hash);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::CharacterIdentity;
    use pobr_core::calc::MinimalInput;

    fn opts() -> OrchestratorOptions {
        OrchestratorOptions {
            base_input: MinimalInput {
                base_life: 100.0,
                ..MinimalInput::default()
            },
            extra_modifier_texts: vec![],
        }
    }

    fn build(level: u32) -> Build {
        Build::new().with_character(CharacterIdentity {
            level,
            class_name: "Ranger".into(),
            ascendancy_name: String::new(),
        })
    }

    #[test]
    fn second_call_hits_cache() {
        let mut cache = CalcCache::new();
        let b = build(90);
        let o = opts();
        let first = cache.get_or_compute(&b, &o).expect("calc");
        let second = cache.get_or_compute(&b, &o).expect("calc");
        assert_eq!(first, second);
        assert_eq!(cache.hits(), 1);
        assert_eq!(cache.misses(), 1);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn different_builds_miss() {
        let mut cache = CalcCache::new();
        let o = opts();
        cache.get_or_compute(&build(90), &o).expect("calc");
        cache.get_or_compute(&build(91), &o).expect("calc");
        assert_eq!(cache.misses(), 2);
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn same_build_different_options_miss() {
        // The same Build with different options (different extra_modifier_texts) must
        // be computed separately and must not incorrectly hit the first cache entry
        // (audit HIGH-4 regression guard).
        let mut cache = CalcCache::new();
        let b = build(90);
        let mut o2 = opts();
        o2.extra_modifier_texts = vec!["+50 to maximum Life".into()];
        cache.get_or_compute(&b, &opts()).expect("calc");
        cache.get_or_compute(&b, &o2).expect("calc");
        assert_eq!(cache.misses(), 2);
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn clear_resets() {
        let mut cache = CalcCache::new();
        cache.get_or_compute(&build(90), &opts()).expect("calc");
        cache.clear();
        assert!(cache.is_empty());
        assert_eq!(cache.hits(), 0);
    }
}
