//! Read-only calculation snapshot of a Build plus its content hash.
//!
//! [`BuildSnapshot`] collapses the parts of a [`Build`] that **affect the calculation
//! result** into a deterministic, hashable view. [`BuildSnapshot::content_hash`] gives a
//! stable 64-bit content hash used as the key for [`crate::calc_cache::CalcCache`]: as
//! long as the calc-relevant inputs don't change, the hash doesn't change, so the cache hits.
//!
//! The hash is implemented as FNV-1a over normalized fields written to a stable string
//! (no third-party hashing dependency, consistent across platforms / processes; doesn't
//! rely on `HashMap`'s randomized iteration order).

use pobr_data::item::EquipmentSlot;

use crate::build::Build;

/// Read-only snapshot of calculation input. Field order is fixed and iteration is
/// deterministic, so it is safe to use for content hashing.
///
/// **Scope warning**: this snapshot only covers the inputs to **text-only `calculate`**
/// (level / class / allocated nodes / item mod text / socket group gem ids / config
/// keys). It is **not sufficient** as a cache key for
/// [`crate::calc_orchestrator::calculate_with_data`] — that path also consumes gem
/// level/quality, jewel radius effects, and other runtime data this snapshot doesn't
/// capture. `calculate_with_data`'s cache needs its own key over
/// `(build_hash, options, data...)`; never use
/// [`content_hash`](BuildSnapshot::content_hash) directly as that cache key.
#[derive(Debug, Clone, PartialEq)]
pub struct BuildSnapshot {
    pub level: u32,
    pub class_name: String,
    pub ascendancy_name: String,
    pub allocated_nodes: Vec<u32>,
    /// (slot id, normalized mod text) pairs of equipped items, sorted by slot id.
    pub items: Vec<(String, Vec<String>)>,
    /// gem id sequences of enabled skill socket groups, order preserved within each group.
    pub socket_groups: Vec<Vec<String>>,
    /// Normalized config key-values (attack / spell / conditions / multipliers), stably sorted.
    pub config_keys: Vec<String>,
}

impl BuildSnapshot {
    /// Extracts the calc-relevant inputs from a [`Build`] and normalizes them into a deterministic snapshot.
    pub fn from_build(build: &Build) -> Self {
        let mut allocated_nodes: Vec<u32> =
            build.tree.allocated_nodes.iter().map(|n| n.0).collect();
        allocated_nodes.sort_unstable();

        let items = collect_items(build);
        let socket_groups = build
            .enabled_socket_groups()
            .map(|g| g.gem_ids.clone())
            .collect();
        let config_keys = collect_config_keys(build);

        Self {
            level: build.character.level,
            class_name: build.character.class_name.clone(),
            ascendancy_name: build.character.ascendancy_name.clone(),
            allocated_nodes,
            items,
            socket_groups,
            config_keys,
        }
    }

    /// Deterministic 64-bit content hash (FNV-1a, stable across platforms).
    ///
    /// Only covers the inputs to text-only `calculate`; when used as a
    /// [`crate::calc_cache::CalcCache`] key it must be combined with the hash of
    /// [`OrchestratorOptions`](crate::calc_orchestrator::OrchestratorOptions) (see
    /// `calc_cache`). **Do not** use it alone as the cache key for
    /// `calculate_with_data` (gem level/quality, jewels, etc. aren't captured) — see the
    /// scope warning on [`BuildSnapshot`].
    pub fn content_hash(&self) -> u64 {
        let mut hasher = Fnv1a::new();
        hasher.write_u32(self.level);
        hasher.write_str(&self.class_name);
        hasher.write_str(&self.ascendancy_name);

        hasher.write_str("|nodes|");
        for node in &self.allocated_nodes {
            hasher.write_u32(*node);
        }

        hasher.write_str("|items|");
        for (slot, texts) in &self.items {
            hasher.write_str(slot);
            for text in texts {
                hasher.write_str(text);
            }
        }

        hasher.write_str("|skills|");
        for group in &self.socket_groups {
            hasher.write_str("(");
            for gem in group {
                hasher.write_str(gem);
            }
            hasher.write_str(")");
        }

        hasher.write_str("|config|");
        for key in &self.config_keys {
            hasher.write_str(key);
        }

        hasher.finish()
    }
}

/// Complete, collision-free equality of the Build input to data-backed calculation.
/// Keep each field explicit so new Build fields require review at this boundary.
pub(crate) fn same_build(a: &Build, b: &Build) -> bool {
    // No `..`: adding an input field must update this cache boundary.
    let Build {
        character,
        view_mode,
        tree,
        tree_version,
        items,
        jewels,
        granted_socket_jewels,
        radius_jewels,
        utility_slots,
        socket_groups,
        main_socket_group,
        config,
    } = a;
    let pobr_data::passive_tree::PassiveTreeSpec {
        allocated_nodes,
        mastery_effects,
        attribute_overrides,
    } = tree;
    character == &b.character
        && view_mode == &b.view_mode
        && allocated_nodes == &b.tree.allocated_nodes
        && mastery_effects == &b.tree.mastery_effects
        && attribute_overrides == &b.tree.attribute_overrides
        && tree_version == &b.tree_version
        && same_item_map(items, &b.items)
        && same_items(jewels, &b.jewels)
        && granted_socket_jewels.len() == b.granted_socket_jewels.len()
        && granted_socket_jewels
            .iter()
            .zip(&b.granted_socket_jewels)
            .all(|(a, b)| a.0 == b.0 && same_item(&a.1, &b.1) && a.2 == b.2)
        && radius_jewels == &b.radius_jewels
        && utility_slots.len() == b.utility_slots.len()
        && utility_slots
            .iter()
            .zip(&b.utility_slots)
            .all(|(a, b)| a.0 == b.0 && same_item(&a.1, &b.1))
        && socket_groups == &b.socket_groups
        && main_socket_group == &b.main_socket_group
        && same_config(config, &b.config)
}

fn same_item_map(
    a: &std::collections::HashMap<EquipmentSlot, pobr_data::item::Item>,
    b: &std::collections::HashMap<EquipmentSlot, pobr_data::item::Item>,
) -> bool {
    a.len() == b.len()
        && a.iter()
            .all(|(slot, item)| b.get(slot).is_some_and(|other| same_item(item, other)))
}

fn same_items(a: &[pobr_data::item::Item], b: &[pobr_data::item::Item]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_item(a, b))
}

fn same_item(a: &pobr_data::item::Item, b: &pobr_data::item::Item) -> bool {
    fn bits(a: Option<f64>, b: Option<f64>) -> bool {
        a.map(f64::to_bits) == b.map(f64::to_bits)
    }
    let pobr_data::item::Item {
        base,
        rarity,
        quality,
        corrupted,
        implicit_texts,
        modifier_texts,
        enchant_texts,
        rolled_defence,
        parsed_stats,
    } = a;
    let pobr_data::item::RolledDefence {
        armour,
        evasion,
        energy_shield,
        spirit,
        ward,
        sockets_filled,
    } = rolled_defence;
    base == &b.base
        && rarity == &b.rarity
        && quality == &b.quality
        && corrupted == &b.corrupted
        && implicit_texts == &b.implicit_texts
        && modifier_texts == &b.modifier_texts
        && enchant_texts == &b.enchant_texts
        && bits(*armour, b.rolled_defence.armour)
        && bits(*evasion, b.rolled_defence.evasion)
        && bits(*energy_shield, b.rolled_defence.energy_shield)
        && bits(*spirit, b.rolled_defence.spirit)
        && bits(*ward, b.rolled_defence.ward)
        && sockets_filled == &b.rolled_defence.sockets_filled
        && parsed_stats == &b.parsed_stats
}

fn same_config(a: &crate::build_config::BuildConfig, b: &crate::build_config::BuildConfig) -> bool {
    let crate::build_config::BuildConfig {
        is_attack,
        is_spell,
        damage_type,
        bandit,
        conditions,
        multipliers,
        global_modifier_texts,
        campaign_progress,
        enemy_tier,
        raw_inputs,
    } = a;
    is_attack == &b.is_attack
        && is_spell == &b.is_spell
        && damage_type == &b.damage_type
        && bandit == &b.bandit
        && conditions == &b.conditions
        && multipliers.len() == b.multipliers.len()
        && multipliers.iter().all(|(key, v)| {
            b.multipliers
                .get(key)
                .is_some_and(|other| v.to_bits() == other.to_bits())
        })
        && global_modifier_texts == &b.global_modifier_texts
        && campaign_progress == &b.campaign_progress
        && enemy_tier == &b.enemy_tier
        && same_raw_inputs(raw_inputs, &b.raw_inputs)
}

fn same_raw_inputs(
    a: &pobr_core::rules::config_interpreter::RawConfigInputs,
    b: &pobr_core::rules::config_interpreter::RawConfigInputs,
) -> bool {
    use pobr_core::rules::config_interpreter::{ConfigInputValue, RawConfigInputs};
    fn same_values(
        a: &std::collections::BTreeMap<String, ConfigInputValue>,
        b: &std::collections::BTreeMap<String, ConfigInputValue>,
    ) -> bool {
        a.len() == b.len()
            && a.iter().all(|(key, value)| {
                b.get(key).is_some_and(|other| match value {
                    ConfigInputValue::Bool(v) => {
                        matches!(other, ConfigInputValue::Bool(o) if v == o)
                    }
                    ConfigInputValue::Number(v) => {
                        matches!(other, ConfigInputValue::Number(o) if v.to_bits() == o.to_bits())
                    }
                    ConfigInputValue::Text(v) => {
                        matches!(other, ConfigInputValue::Text(o) if v == o)
                    }
                })
            })
    }
    let RawConfigInputs {
        values,
        placeholders,
    } = a;
    same_values(values, &b.values) && same_values(placeholders, &b.placeholders)
}

fn collect_items(build: &Build) -> Vec<(String, Vec<String>)> {
    build
        .equipped_items()
        .into_iter()
        .map(|(slot, item)| {
            let mut texts: Vec<String> = Vec::new();
            texts.extend(item.enchant_texts.iter().cloned());
            texts.extend(item.implicit_texts.iter().cloned());
            texts.extend(item.modifier_texts.iter().cloned());
            (slot_tag(slot).to_string(), texts)
        })
        .collect()
}

fn collect_config_keys(build: &Build) -> Vec<String> {
    let cfg = &build.config;
    let mut keys = vec![
        format!("attack={}", cfg.is_attack),
        format!("spell={}", cfg.is_spell),
        format!("dt={:?}", cfg.damage_type),
        format!("bandit={:?}", cfg.bandit),
    ];
    let mut conds: Vec<String> = cfg
        .conditions
        .iter()
        .map(|(k, v)| format!("c:{k}={v}"))
        .collect();
    conds.sort();
    keys.extend(conds);
    let mut mults: Vec<String> = cfg
        .multipliers
        .iter()
        .map(|(k, v)| format!("m:{k}={}", v.to_bits()))
        .collect();
    mults.sort();
    keys.extend(mults);
    // Raw config input (the primary data source): the interpreter consumes raw_inputs
    // to drive behavior beyond conditions/multipliers (enemy overrides / customMods,
    // enabled per-category, etc.), so it must go into the content hash to avoid cache
    // aliasing. BTreeMap iteration is already deterministically ordered.
    keys.extend(
        cfg.raw_inputs
            .values
            .iter()
            .map(|(k, v)| format!("r:{k}={v:?}")),
    );
    keys
}

fn slot_tag(slot: EquipmentSlot) -> &'static str {
    slot.id()
}

/// 64-bit FNV-1a hasher (deterministic, no random seed).
struct Fnv1a {
    state: u64,
}

impl Fnv1a {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    fn new() -> Self {
        Self {
            state: Self::OFFSET,
        }
    }

    fn write_byte(&mut self, byte: u8) {
        self.state ^= byte as u64;
        self.state = self.state.wrapping_mul(Self::PRIME);
    }

    fn write_str(&mut self, s: &str) {
        for b in s.as_bytes() {
            self.write_byte(*b);
        }
        // Length separator to avoid concatenation ambiguity ("ab"+"c" vs "a"+"bc").
        self.write_byte(0xff);
    }

    fn write_u32(&mut self, value: u32) {
        for b in value.to_le_bytes() {
            self.write_byte(b);
        }
    }

    fn finish(&self) -> u64 {
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::{Build, CharacterIdentity, SocketGroup};
    use pobr_data::item::{Item, ItemBaseId, ItemRarity, RolledDefence};
    use pobr_data::passive_tree::{NodeId, PassiveTreeSpec};

    fn build_with_level(level: u32) -> Build {
        Build::new().with_character(CharacterIdentity {
            level,
            class_name: "Ranger".into(),
            ascendancy_name: "Deadeye".into(),
        })
    }

    #[test]
    fn data_build_identity_covers_gem_jewel_and_selection() {
        use crate::build::RadiusJewel;
        let a = build_with_level(90);
        let mut b = a.clone();
        assert!(same_build(&a, &b));
        b.main_socket_group = Some(2);
        assert!(!same_build(&a, &b));
        b = a.clone();
        b.radius_jewels.push(RadiusJewel {
            socket_node: 123,
            radius_label: None,
            grant_lines: vec![],
            notable_effect_inc: 0,
            small_effect_inc: 0,
            tree_texts: vec![],
        });
        assert!(!same_build(&a, &b));
        b = a.clone();
        b.socket_groups
            .push(SocketGroup::new().with_gem_skill_quality("SparkPlayer", 1, 20));
        let c = b.clone();
        b.socket_groups[0].gem_skills[0].gem_level = 2;
        assert!(!same_build(&b, &c));
        b = c.clone();
        b.socket_groups[0].gem_skills[0].quality = 21;
        assert!(!same_build(&b, &c));
        b = c.clone();
        b.socket_groups[0].gem_skills[0].name_spec = Some("Spark".into());
        assert!(!same_build(&b, &c));
    }

    #[test]
    fn hash_is_deterministic() {
        let a = BuildSnapshot::from_build(&build_with_level(90));
        let b = BuildSnapshot::from_build(&build_with_level(90));
        assert_eq!(a.content_hash(), b.content_hash());
    }

    #[test]
    fn hash_changes_with_level() {
        let a = BuildSnapshot::from_build(&build_with_level(90)).content_hash();
        let b = BuildSnapshot::from_build(&build_with_level(91)).content_hash();
        assert_ne!(a, b);
    }

    #[test]
    fn hash_insensitive_to_node_order() {
        let mut a = build_with_level(90);
        a = a.with_tree(PassiveTreeSpec {
            allocated_nodes: vec![NodeId(3), NodeId(1), NodeId(2)],
            ..Default::default()
        });
        let mut b = build_with_level(90);
        b = b.with_tree(PassiveTreeSpec {
            allocated_nodes: vec![NodeId(1), NodeId(2), NodeId(3)],
            ..Default::default()
        });
        assert_eq!(
            BuildSnapshot::from_build(&a).content_hash(),
            BuildSnapshot::from_build(&b).content_hash()
        );
    }

    #[test]
    fn hash_changes_with_item() {
        let base = build_with_level(90);
        let with_item = base.clone().set_item(
            EquipmentSlot::Ring1,
            Item {
                base: ItemBaseId::from("Iron Ring"),
                rarity: ItemRarity::Rare,
                quality: 0,
                corrupted: false,
                implicit_texts: vec![],
                modifier_texts: vec!["+10 to maximum Life".into()],
                enchant_texts: vec![],
                rolled_defence: RolledDefence::default(),
                parsed_stats: vec![],
            },
        );
        assert_ne!(
            BuildSnapshot::from_build(&base).content_hash(),
            BuildSnapshot::from_build(&with_item).content_hash()
        );
    }

    #[test]
    fn enabled_groups_affect_hash() {
        let base = build_with_level(90);
        let with_group = base
            .clone()
            .add_socket_group(SocketGroup::new().with_gem("Spark"));
        assert_ne!(
            BuildSnapshot::from_build(&base).content_hash(),
            BuildSnapshot::from_build(&with_group).content_hash()
        );
    }
}
