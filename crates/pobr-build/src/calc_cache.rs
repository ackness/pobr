//! 计算结果缓存：以 [`BuildSnapshot::content_hash`] 为键缓存 [`OutputTable`]。
//!
//! 计算编排昂贵；只要计算相关输入不变（内容哈希相同），就直接复用上次结果。
//! 缓存是简单的内存 map（LRU 容量上限可选），确定性、无共享可变全局状态：
//! 调用方持有 [`CalcCache`] 实例并显式 `get_or_compute`。

use std::collections::HashMap;

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
        let key = BuildSnapshot::from_build(build).content_hash();
        if let Some(cached) = self.entries.get(&key) {
            self.hits += 1;
            return Ok(cached.clone());
        }

        self.misses += 1;
        let output = calculate(build, options)?;
        self.entries.insert(key, output.clone());
        Ok(output)
    }

    /// 直接按内容哈希查缓存（不触发计算）。
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
    fn clear_resets() {
        let mut cache = CalcCache::new();
        cache.get_or_compute(&build(90), &opts()).expect("calc");
        cache.clear();
        assert!(cache.is_empty());
        assert_eq!(cache.hits(), 0);
    }
}
