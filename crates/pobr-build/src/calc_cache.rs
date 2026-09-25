//! Caller-owned caches for text-only and data-backed calculations.
//! A data cache borrows its complete data and options for its lifetime; neither
//! input can be mutated while the cache exists. No identity is inferred from a
//! version string or address.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use pobr_core::calc::OutputTable;

use crate::build::Build;
use crate::build_data::BuildData;
use crate::calc_orchestrator::{
    DataOrchestratorOptions, OrchestratorOptions, calculate, calculate_with_data,
};
use crate::error::BuildError;
use crate::snapshot::{BuildSnapshot, same_build};

/// Text-only cache with a default capacity of 64 results.
/// `peek` preserves the historic hash lookup for callers that know its key;
/// unlike `get_or_compute`, it cannot disambiguate a digest collision.
#[derive(Debug)]
pub struct CalcCache {
    entries: VecDeque<(u64, Build, OrchestratorOptions, OutputTable)>,
    capacity: usize,
    hits: u64,
    misses: u64,
}

impl Default for CalcCache {
    fn default() -> Self {
        Self::with_capacity(64)
    }
}

impl CalcCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            capacity,
            hits: 0,
            misses: 0,
        }
    }

    pub fn get_or_compute(
        &mut self,
        build: &Build,
        options: &OrchestratorOptions,
    ) -> Result<OutputTable, BuildError> {
        let key = combined_key(
            BuildSnapshot::from_build(build).content_hash(),
            options_hash(options),
        );
        if let Some(pos) = self.entries.iter().position(|(hash, b, o, _)| {
            *hash == key && same_build(b, build) && same_options(o, options)
        }) {
            self.hits += 1;
            let entry = self.entries.remove(pos).expect("matched entry");
            let result = entry.3.clone();
            self.entries.push_back(entry);
            return Ok(result);
        }
        self.misses += 1;
        let output = calculate(build, options)?;
        if self.capacity != 0 {
            if self.entries.len() == self.capacity {
                self.entries.pop_front();
            }
            self.entries
                .push_back((key, build.clone(), options.clone(), output.clone()));
        }
        Ok(output)
    }
    pub fn peek(&self, key: u64) -> Option<&OutputTable> {
        self.entries
            .iter()
            .rev()
            .find(|(hash, _, _, _)| *hash == key)
            .map(|entry| &entry.3)
    }
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

/// Bounded cache for complete data-backed outputs. The build is cloned on insertion
/// and structurally compared on lookup, so a 64-bit digest collision cannot alias.
/// This caches only `OutputTable`: callers requiring session state or Compare-mode
/// diagnostics continue using the uncached report/session entry points.
pub struct DataCalcCache<'a> {
    data: &'a BuildData,
    options: &'a DataOrchestratorOptions,
    entries: VecDeque<(Build, OutputTable)>,
    trigger_memo: Rc<RefCell<TriggerMemo>>,
    capacity: usize,
    hits: u64,
    misses: u64,
}

impl<'a> DataCalcCache<'a> {
    pub fn new(data: &'a BuildData, options: &'a DataOrchestratorOptions, capacity: usize) -> Self {
        Self {
            data,
            options,
            entries: VecDeque::new(),
            trigger_memo: Rc::new(RefCell::new(TriggerMemo::new(capacity))),
            capacity,
            hits: 0,
            misses: 0,
        }
    }

    pub fn get_or_compute(&mut self, build: &Build) -> Result<OutputTable, BuildError> {
        // Compare is observational: never skip its stat-map record collection.
        if self.options.stat_map_mode == crate::calc_orchestrator::StatMapMode::Compare {
            self.misses += 1;
            return calculate_with_data(build, self.data, self.options);
        }
        if let Some(pos) = self
            .entries
            .iter()
            .position(|(key, _)| same_build(key, build))
        {
            self.hits += 1;
            let entry = self.entries.remove(pos).expect("matched entry");
            let output = entry.1.clone();
            self.entries.push_back(entry);
            return Ok(output);
        }
        self.misses += 1;
        let output = crate::calc_orchestrator::calculate_with_data_memo(
            build,
            self.data,
            self.options,
            Rc::clone(&self.trigger_memo),
        )?;
        if self.capacity != 0 {
            if self.entries.len() == self.capacity {
                self.entries.pop_front();
            }
            self.entries.push_back((build.clone(), output.clone()));
        }
        Ok(output)
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
    pub fn clear(&mut self) {
        self.entries.clear();
        self.trigger_memo.borrow_mut().clear();
        self.hits = 0;
        self.misses = 0;
    }
}

/// Child computations only: a source build is the exact post-selection input.
/// No failed result is inserted. The recursive child context has memo disabled.
pub(crate) struct TriggerMemo {
    entries: VecDeque<(Build, pobr_core::calc::TriggerSourceStats)>,
    capacity: usize,
    pub hits: u64,
    pub misses: u64,
}

impl TriggerMemo {
    fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            capacity,
            hits: 0,
            misses: 0,
        }
    }
    pub fn get(&mut self, build: &Build) -> Option<pobr_core::calc::TriggerSourceStats> {
        if let Some(pos) = self
            .entries
            .iter()
            .position(|(key, _)| same_build(key, build))
        {
            self.hits += 1;
            let entry = self.entries.remove(pos).expect("matched entry");
            let stats = entry.1;
            self.entries.push_back(entry);
            Some(stats)
        } else {
            self.misses += 1;
            None
        }
    }
    pub fn insert(&mut self, build: Build, stats: pobr_core::calc::TriggerSourceStats) {
        if self.capacity == 0 {
            return;
        }
        if self.entries.len() == self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back((build, stats));
    }
    fn clear(&mut self) {
        self.entries.clear();
        self.hits = 0;
        self.misses = 0;
    }
}

fn same_options(a: &OrchestratorOptions, b: &OrchestratorOptions) -> bool {
    let OrchestratorOptions {
        base_input: _,
        extra_modifier_texts,
    } = a;
    extra_modifier_texts == &b.extra_modifier_texts && option_bits(a) == option_bits(b)
}

fn option_bits(options: &OrchestratorOptions) -> [u64; 10] {
    let pobr_core::calc::MinimalInput {
        base_life,
        base_mana,
        base_fire_resistance,
        base_cold_resistance,
        base_lightning_resistance,
        base_accuracy,
        enemy_evasion,
        base_hit_min,
        base_hit_max,
        base_action_rate,
    } = &options.base_input;
    [
        base_life.to_bits(),
        base_mana.to_bits(),
        base_fire_resistance.to_bits(),
        base_cold_resistance.to_bits(),
        base_lightning_resistance.to_bits(),
        base_accuracy.to_bits(),
        enemy_evasion.to_bits(),
        base_hit_min.to_bits(),
        base_hit_max.to_bits(),
        base_action_rate.to_bits(),
    ]
}

fn options_hash(options: &OrchestratorOptions) -> u64 {
    let mut hasher = DefaultHasher::new();
    options.extra_modifier_texts.hash(&mut hasher);
    for bits in option_bits(options) {
        hasher.write_u64(bits);
    }
    hasher.finish()
}
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
    fn text_cache_checks_full_identity_and_eviction() {
        let mut cache = CalcCache::with_capacity(1);
        let first = build(90);
        let mut second = first.clone();
        second
            .config
            .global_modifier_texts
            .push("+50 to maximum Life".into());
        assert_eq!(
            BuildSnapshot::from_build(&first).content_hash(),
            BuildSnapshot::from_build(&second).content_hash()
        );
        for candidate in [&first, &second] {
            assert_eq!(
                cache.get_or_compute(candidate, &opts()).unwrap(),
                calculate(candidate, &opts()).unwrap()
            );
        }
        assert_eq!((cache.hits(), cache.misses(), cache.len()), (0, 2, 1));
        cache.get_or_compute(&first, &opts()).unwrap();
        assert_eq!(cache.misses(), 3);
    }

    #[test]
    fn data_cache_equivalence_invalidation_and_eviction() {
        use crate::build::RadiusJewel;
        use crate::build::SocketGroup;
        let data = BuildData::load(&pobr_gamedata::GameData::new(
            pobr_gamedata::current_data_dir(),
        ))
        .expect("committed data");
        let options = DataOrchestratorOptions::default();
        let original = build(90);
        let mut gem = original.clone();
        gem.socket_groups
            .push(SocketGroup::new().with_gem_skill_quality("SparkPlayer", 10, 20));
        let mut quality = gem.clone();
        quality.socket_groups[0].gem_skills[0].quality = 21;
        let mut radius = original.clone();
        radius.radius_jewels.push(RadiusJewel {
            socket_node: 123,
            radius_label: Some("Small".into()),
            grant_lines: vec!["+5 to maximum Life".into()],
            notable_effect_inc: 0,
            small_effect_inc: 0,
            tree_texts: vec![],
        });
        let mut item = original.clone();
        item.items.insert(
            pobr_data::item::EquipmentSlot::Helmet,
            pobr_data::item::Item {
                base: "Iron Hat".into(),
                rarity: pobr_data::item::ItemRarity::Normal,
                quality: 0,
                corrupted: false,
                implicit_texts: vec![],
                modifier_texts: vec![],
                enchant_texts: vec![],
                rolled_defence: Default::default(),
                parsed_stats: vec![],
            },
        );
        let mut jewel = original.clone();
        jewel
            .jewels
            .push(item.items[&pobr_data::item::EquipmentSlot::Helmet].clone());
        let mut config = original.clone();
        config
            .config
            .global_modifier_texts
            .push("+10 to maximum Life".into());
        let mut tree = original.clone();
        tree.tree
            .allocated_nodes
            .push(pobr_data::passive_tree::NodeId(123));
        let mut cache = DataCalcCache::new(&data, &options, 2);
        for candidate in [
            &original, &gem, &quality, &radius, &item, &jewel, &config, &tree,
        ] {
            let uncached = calculate_with_data(candidate, &data, &options).expect("uncached");
            assert_eq!(cache.get_or_compute(candidate).expect("cached"), uncached);
        }
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.misses(), 8);
        assert_eq!(
            cache.get_or_compute(&tree).expect("hit"),
            calculate_with_data(&tree, &data, &options).unwrap()
        );
        assert_eq!(cache.hits(), 1);
        cache.get_or_compute(&original).expect("evicted");
        assert_eq!(cache.misses(), 9);
        let different_options = DataOrchestratorOptions {
            enemy_level: 99,
            ..Default::default()
        };
        let mut other = DataCalcCache::new(&data, &different_options, 1);
        assert_eq!(
            other.get_or_compute(&original).unwrap(),
            calculate_with_data(&original, &data, &different_options).unwrap()
        );
        assert_eq!(other.misses(), 1);
        let catalog_options = DataOrchestratorOptions {
            stat_map_catalog: Some(std::sync::Arc::new(
                pobr_core::rules::stat_map_engine::StatMapCatalog::new(
                    serde_json::from_str(r#"{"global":{}}"#).unwrap(),
                ),
            )),
            ..Default::default()
        };
        let mut catalog_cache = DataCalcCache::new(&data, &catalog_options, 1);
        assert_eq!(
            catalog_cache.get_or_compute(&gem).unwrap(),
            calculate_with_data(&gem, &data, &catalog_options).unwrap()
        );
        let mut other_data = data.clone();
        other_data.class_attributes.remove("Ranger");
        let mut data_cache = DataCalcCache::new(&other_data, &options, 1);
        assert_eq!(
            data_cache.get_or_compute(&original).unwrap(),
            calculate_with_data(&original, &other_data, &options).unwrap()
        );
        assert_eq!(data_cache.misses(), 1);
        let compare_options = DataOrchestratorOptions {
            stat_map_mode: crate::calc_orchestrator::StatMapMode::Compare,
            ..Default::default()
        };
        let mut compare_cache = DataCalcCache::new(&data, &compare_options, 2);
        compare_cache.get_or_compute(&original).unwrap();
        compare_cache.get_or_compute(&original).unwrap();
        assert_eq!(
            (
                compare_cache.hits(),
                compare_cache.misses(),
                compare_cache.len()
            ),
            (0, 2, 0)
        );
    }

    #[test]
    fn placeholders_invalidate_output_and_trigger_keys() {
        use crate::build::SocketGroup;
        use pobr_core::rules::config_interpreter::ConfigInputValue;
        let data = BuildData::load(&pobr_gamedata::GameData::new(
            pobr_gamedata::current_data_dir(),
        ))
        .unwrap();
        let options = DataOrchestratorOptions {
            mode_effective: true,
            ..Default::default()
        };
        let mut first = build(80)
            .add_socket_group(SocketGroup::new().with_gem_skill("ArmourBreakerPlayer", 10))
            .with_main_socket_group(1);
        first
            .config
            .raw_inputs
            .placeholders
            .insert("enemyLevel".into(), ConfigInputValue::Number(1.0));
        let mut second = first.clone();
        second
            .config
            .raw_inputs
            .placeholders
            .insert("enemyLevel".into(), ConfigInputValue::Number(85.0));
        let mut third = second.clone();
        third
            .config
            .raw_inputs
            .placeholders
            .insert("enemyDistance".into(), ConfigInputValue::Number(50.0));
        let mut cache = DataCalcCache::new(&data, &options, 4);
        for candidate in [&first, &second, &third] {
            assert_eq!(
                cache.get_or_compute(candidate).unwrap(),
                calculate_with_data(candidate, &data, &options).unwrap()
            );
        }
        assert_eq!((cache.hits(), cache.misses()), (0, 3));
        assert_ne!(
            cache.get_or_compute(&first).unwrap().hit_chance,
            cache.get_or_compute(&second).unwrap().hit_chance
        );
        let mut memo = TriggerMemo::new(4);
        memo.insert(
            first,
            pobr_core::calc::TriggerSourceStats {
                action_rate: 1.0,
                hit_chance: 100.0,
                crit_chance: 5.0,
            },
        );
        assert!(memo.get(&second).is_none());
        assert!(memo.get(&third).is_none());
    }

    #[test]
    fn trigger_memo_reuses_exact_source_selection_only() {
        use crate::build::SocketGroup;
        let data = BuildData::load(&pobr_gamedata::GameData::new(
            pobr_gamedata::current_data_dir(),
        ))
        .expect("data");
        let options = DataOrchestratorOptions::default();
        let first = build(80)
            .add_socket_group(
                SocketGroup::new()
                    .with_gem_skill("ArmourBreakerPlayer", 10)
                    .with_gem_skill("MetaCastOnCritPlayer", 10)
                    .with_gem_skill("FireballPlayer", 10)
                    .with_gem_skill("FireballPlayer", 10)
                    .with_main_active_skill(3),
            )
            .with_main_socket_group(1);
        let mut second = first.clone();
        second.socket_groups[0].main_active_skill = Some(4);
        let mut cache = DataCalcCache::new(&data, &options, 4);
        for candidate in [&first, &second] {
            let expected = calculate_with_data(candidate, &data, &options).unwrap();
            assert_eq!(cache.get_or_compute(candidate).unwrap(), expected);
        }
        assert_eq!(cache.misses(), 2);
        assert!(
            cache.trigger_memo.borrow().hits > 0,
            "second selected skill should reuse identical source subcalc"
        );
        let previous_hits = cache.trigger_memo.borrow().hits;
        let mut altered = second.clone();
        altered.socket_groups[0].gem_skills[0].gem_level += 1;
        let expected = calculate_with_data(&altered, &data, &options).unwrap();
        assert_eq!(cache.get_or_compute(&altered).unwrap(), expected);
        assert_eq!(
            cache.trigger_memo.borrow().hits,
            previous_hits,
            "source gem level invalidates subcalc"
        );
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
