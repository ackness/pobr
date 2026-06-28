//! 计算结果缓存：以「[`BuildSnapshot::content_hash`] + [`OrchestratorOptions`] 哈希」
//! 的组合为键缓存 [`OutputTable`]。
//!
//! 计算编排昂贵；只要计算相关输入不变，就直接复用上次结果。**注意**：`calculate`
//! 的输出不仅由 [`Build`] 决定，还由 `options`（`base_input` 的基础 MinimalInput +
//! `extra_modifier_texts`）决定。因此缓存键必须同时覆盖二者——否则同一个 Build 配上
//! 不同 options 会错误命中首次缓存结果（audit HIGH-4）。
//!
//! 缓存是简单的内存 map（LRU 容量上限可选），确定性、无共享可变全局状态：
//! 调用方持有 [`CalcCache`] 实例并显式 `get_or_compute`。

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use pobr_core::calc::OutputTable;

use crate::build::Build;
use crate::calc_orchestrator::{OrchestratorOptions, calculate};
use crate::error::BuildError;
use crate::snapshot::BuildSnapshot;

/// 内存计算缓存。键为内容哈希，值为已算出的 [`OutputTable`]。
#[derive(Debug, Default)]
pub struct CalcCache {
    entries: HashMap<u64, OutputTable>,
    /// 命中 / 未命中统计，便于诊断缓存有效性。
    hits: u64,
    misses: u64,
}

impl CalcCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// 命中则返回缓存结果；否则计算、存入、再返回。
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

    /// 直接按组合缓存键查缓存（不触发计算）。键由 build 内容哈希与 options 哈希合成。
    pub fn peek(&self, key: u64) -> Option<&OutputTable> {
        self.entries.get(&key)
    }

    /// 清空缓存与统计。
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

/// `OrchestratorOptions` 的确定性哈希。
///
/// `OrchestratorOptions` / `MinimalInput` 都未派生 `Serialize` 或 `Hash`（后者含
/// `f64` 字段无法直接派生），故逐字段手动喂入：
/// - `extra_modifier_texts`（`Vec<String>` 已实现 `Hash`）直接 `hash`；
/// - `base_input` 的每个 `f64` 字段取 `to_bits()` 后喂入（位级相等，规避 `f64`
///   不实现 `Eq`/`Hash` 的限制）。
///
/// **维护提示**：`MinimalInput` 新增字段时必须在此同步追加，否则新字段不会进入
/// 缓存键、可能造成缓存别名。若将来这些类型派生了 `Serialize`，可改用稳定序列化
/// （如 `serde_json::to_vec`）喂入哈希器，自动覆盖全部字段。
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

/// 把 build 内容哈希与 options 哈希合成最终缓存键。
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
        // 同一 Build、不同 options（extra_modifier_texts 不同）必须各算各的，
        // 不得错误命中首次缓存（audit HIGH-4 回归保护）。
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
