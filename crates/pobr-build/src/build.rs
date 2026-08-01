//! `Build`: in-memory state of a PoB Build.
//!
//! Aggregates all the calculable inputs of a build: character basics (level / class /
//! ascendancy), passive tree ([`PassiveTreeSpec`]), equipment ([`Item`] keyed by
//! [`EquipmentSlot`]), skill gem groups ([`SocketGroup`]), Build-level config
//! ([`BuildConfig`]), and the current view ([`ViewMode`]).
//!
//! Types are always the REAL authoritative definitions (`pobr_data`'s `PassiveTreeSpec` /
//! `Item` / `ViewMode`). `SocketGroup` is this crate's simplified equivalent (no
//! dependency on SANDBOX-only types), carrying the stable id and enabled state of an
//! "active skill + support gems" group.

use std::collections::HashMap;

use pobr_data::build_config::ViewMode;
use pobr_data::item::{EquipmentSlot, Item};
use pobr_data::passive_tree::PassiveTreeSpec;

use crate::build_config::BuildConfig;

/// A granted-effect reference for one gem (`<Gem skillId>` + `<Gem level>` +
/// `<Gem quality>` + `<Gem statSetIndex>`), active or support. Classified by the calc
/// side using the data table (`is_support`).
///
/// Contract C4 scope: `quality` is the T1 share (default 0 = no quality),
/// `stat_set_index` is the T5 share (form selection).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GemSkillRef {
    /// Granted effect id (PoB `<Gem skillId>`, e.g. `SupportAddedLightningDamagePlayer`).
    pub skill_id: String,
    /// Gem level (PoB `<Gem level>`).
    pub gem_level: u32,
    /// Gem quality (PoB `<Gem quality>`, 0-23; 0 = no quality). The consumer applies the
    /// quality stat as `trunc(per_quality_rate × quality)` (see `BuildData::effect_stats`,
    /// matching PoB2 CalcTools.lua `buildSkillInstanceStats`).
    pub quality: u32,
    /// statSet form selection (PoB `<Gem statSetIndex>`, **1-based**, indexes the
    /// statSets list exported by PoB2 = `StatSetDef::vendor_set_index` semantics; vendor
    /// SkillsTab.lua:354 reads / :489 writes). `None` = unspecified (defaults to the
    /// primary set; PoB2 serializes the default state as the literal `"nil"`, which
    /// parsing normalizes to `None`). `statSetIndexCalcs` (a separate selection on the
    /// calcs page) is not handled and is ignored by the parser.
    pub stat_set_index: Option<u32>,
    /// PoB `<Gem nameSpec>` display name — only present when the XML lacks
    /// `skillId`/`gemId` (the serialized form of a lineage support like Atziri's
    /// Communion), in which case `skill_id` is an empty string. The orchestrator's
    /// `stage_build_view` matches this display name against granted_effects to backfill
    /// `skill_id` (mirroring how PoB2 SkillsTab looks up a gem by nameSpec). References
    /// that fail to resolve are silently skipped at every consumption point because
    /// `granted_effects.get("")` comes up empty.
    pub name_spec: Option<String>,
}

/// A group of skill gems in the same socket (active skill + its supports). A simplified
/// equivalent represented by stable gem ids.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SocketGroup {
    /// PoB `<Skill source>` (item-granted skills use `Item:<id>:<name>`). `None` means a
    /// regular skill group the user created manually; the orchestrator uses this to tell
    /// manual groups apart from granted ones in the same slot with the same skill.
    pub source: Option<String>,
    /// Which slot it's socketed in (e.g. `"weapon1"`). `None` means unspecified / a
    /// slotless source such as a soul core.
    pub slot: Option<String>,
    /// Whether this group participates in the calculation (PoB lets each group be
    /// enabled/disabled individually).
    pub enabled: bool,
    /// Stable gem ids for this group (active skill first, then support gems; matches
    /// PoB's Gem list order).
    pub gem_ids: Vec<String>,
    /// The active skill's granted effect id (PoB `<Gem skillId>`, e.g.
    /// `ExplosiveGrenadePlayer`), used to look up this skill's per-level parameters
    /// (cast/attack time, cost, cooldown). `None` = unknown.
    pub active_skill_id: Option<String>,
    /// The active skill gem's level (PoB `<Gem level>`), used to index the per-level
    /// parameter arrays.
    pub active_gem_level: Option<u32>,
    /// The active skill gem's quality (PoB `<Gem quality>`), kept in sync with
    /// [`GemSkillRef::quality`] (T1.4: the data-fetch side actually looks up quality via
    /// `gem_skills`; this field exists to keep the builder/snapshot views consistent).
    pub active_gem_quality: Option<u32>,
    /// Granted-effect references for **every enabled gem** in this group (in PoB Gem
    /// list order, active and support included); used to resolve support gems' per-level
    /// stats (multipliers/added damage) and inject them into the supported skill.
    pub gem_skills: Vec<GemSkillRef>,
    /// PoB `<Skill mainActiveSkill="N">` (**1-based**): points to the Nth entry in this
    /// group's **non-support skill list** (meta/trigger shells count, supports don't).
    /// Used to pick the correct main skill in groups with multiple active skills (e.g.
    /// Cast on Crit + Comet); `None` = unspecified (falls back to the group's first
    /// damaging skill).
    pub main_active_skill: Option<usize>,
}

impl SocketGroup {
    pub fn new() -> Self {
        Self {
            source: None,
            slot: None,
            enabled: true,
            gem_ids: Vec::new(),
            active_skill_id: None,
            active_gem_level: None,
            active_gem_quality: None,
            gem_skills: Vec::new(),
            main_active_skill: None,
        }
    }

    pub fn with_slot(mut self, slot: impl Into<String>) -> Self {
        self.slot = Some(slot.into());
        self
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    pub fn with_gem(mut self, gem_id: impl Into<String>) -> Self {
        self.gem_ids.push(gem_id.into());
        self
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Sets the active skill's granted effect id + gem level (the key for looking up
    /// per-level parameters).
    pub fn with_active_skill(mut self, skill_id: impl Into<String>, gem_level: u32) -> Self {
        self.active_skill_id = Some(skill_id.into());
        self.active_gem_level = Some(gem_level);
        self
    }

    /// Appends a granted-effect reference for a gem (active or support; in PoB Gem list
    /// order). Quality defaults to 0 (no quality); use
    /// [`Self::with_gem_skill_quality`] to set it.
    pub fn with_gem_skill(self, skill_id: impl Into<String>, gem_level: u32) -> Self {
        self.with_gem_skill_quality(skill_id, gem_level, 0)
    }

    /// Appends a granted-effect reference for a gem with quality (T1.4, contract C4).
    pub fn with_gem_skill_quality(
        mut self,
        skill_id: impl Into<String>,
        gem_level: u32,
        quality: u32,
    ) -> Self {
        self.gem_skills.push(GemSkillRef {
            skill_id: skill_id.into(),
            gem_level,
            quality,
            stat_set_index: None,
            name_spec: None,
        });
        self
    }

    /// Appends a granted-effect reference for a gem with a statSet form selection (T5.4, contract C4).
    pub fn with_gem_skill_stat_set(
        mut self,
        skill_id: impl Into<String>,
        gem_level: u32,
        stat_set_index: u32,
    ) -> Self {
        self.gem_skills.push(GemSkillRef {
            skill_id: skill_id.into(),
            gem_level,
            quality: 0,
            stat_set_index: Some(stat_set_index),
            name_spec: None,
        });
        self
    }

    /// Sets PoB `mainActiveSkill` (1-based, indexing this group's non-support skill list).
    pub fn with_main_active_skill(mut self, n: usize) -> Self {
        self.main_active_skill = Some(n);
        self
    }
}

/// Character identity (a subset of the header fields in PoB Build XML's `<Build>`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CharacterIdentity {
    pub level: u32,
    pub class_name: String,
    pub ascendancy_name: String,
}

/// Geometric expansion input for a radius jewel.
///
/// PoB2's `... Passive Skills in Radius also grant <mod>` mods need to inject a global
/// mod scaled by the count of matching **allocated** nodes within the jewel socket's
/// radius times the granted value. This struct carries the minimum info that geometry
/// needs: the tree node id the socket sits on, the radius tier text (`Radius:` line),
/// and the `also grant` lines (raw text, expanded line by line downstream).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RadiusJewel {
    /// The `skill` id of the tree socket the jewel sits in (`<Socket nodeId>`).
    pub socket_node: u32,
    /// Radius tier text (raw `Radius:` line, e.g. `Small`/`Medium`/`Large`/`Very Large`).
    /// `None` when missing (PoB defaults to a jewel radius approximated as Large).
    pub radius_label: Option<String>,
    /// `... Passive Skills in Radius also grant <mod>` lines (raw text, including the
    /// node-type prefix).
    pub grant_lines: Vec<String>,
    /// `N% increased Effect of Notable Passive Skills in Radius` (Time-Lost jewels;
    /// vendor ModParser.lua:6847 → `JewelNotablePassiveSkillEffect` INC, consumed at
    /// CalcSetup.lua:246-275 which applies a ScaleAddList to the modList of Notable
    /// nodes within radius). When a jewel has multiple such lines, the last one wins
    /// (vendor overwrites via `localNotableIncEffect = mod.value`).
    pub notable_effect_inc: u32,
}

/// In-memory state of a PoB Build.
///
/// Immutable updates: every `with_*` method returns a new copy; `set_item` /
/// `add_socket_group` etc. keep the same "construct a new value" semantics (following
/// the builder pattern to avoid in-place shared mutable state).
#[derive(Debug, Clone, Default)]
pub struct Build {
    /// Character identity.
    pub character: CharacterIdentity,
    /// Current view (PoB UI state, compatible with Build XML's `viewMode`).
    pub view_mode: ViewMode,
    /// Allocated passive tree.
    pub tree: PassiveTreeSpec,
    /// The PoB passive tree version recorded for this build (`<Spec treeVersion>`, e.g.
    /// `"0_5"`). `None` = an old save with no annotation. **Purely reconciliation
    /// metadata**: calc still interprets allocated nodes against whatever tree data is
    /// currently loaded (matching PoB2's "recompute against the current pool"). The
    /// actual symptom of a tree version mismatch is allocated nodes not present in the
    /// loaded tree; use [`crate::diagnose_tree_version`] to surface it explicitly (calc
    /// itself currently skips such nodes silently — see pobr-tree node.rs).
    pub tree_version: Option<String>,
    /// Equipment, indexed by [`EquipmentSlot`]. Iterated in `slot.id()` lexical order for
    /// determinism (`EquipmentSlot` only implements `Hash`, not `Ord`, and we can't
    /// change the REAL type).
    pub items: HashMap<EquipmentSlot, Item>,
    /// Jewels (passive tree / abyss sockets, no fixed [`EquipmentSlot`]). Their mods are
    /// injected as global (most jewels are global; radius jewels are currently
    /// approximated as global too).
    pub jewels: Vec<Item>,
    /// Geometric expansion input for radius jewels (`... in Radius also grant <mod>`).
    /// Coexists with `jewels`: `jewels` injects the jewel's **own** global mods, while
    /// this list additionally expands `also grant` lines by radius geometry into global
    /// mods scaled by "count of allocated nodes of the matching type within radius ×
    /// grant" (see `calc_orchestrator`).
    pub radius_jewels: Vec<RadiusJewel>,
    /// **Active** flask/charm slot-name + item pairs:
    /// `("Flask 1"|"Charm 1..3", Item)`, in XML document order, only slots with
    /// `active="true"` are included (matches vendor CalcSetup.lua:1014-1028's
    /// `slot.active` gate). The slot name feeds the flask/charm structured pipeline's
    /// `SourceId(Flask, "flask.<slot>")` attribution and flask/charm classification, and
    /// enters the calculation via `ingest_flask_charm` → env_finalize stage 3
    /// `merge_flasks_charms`.
    pub utility_slots: Vec<(String, Item)>,
    /// Skill gem groups.
    pub socket_groups: Vec<SocketGroup>,
    /// Main skill group index (PoB `<Build mainSocketGroup>`, **1-based**, indexes
    /// `socket_groups`). `None` = unspecified (falls back to a heuristic pick of the
    /// first damaging skill).
    pub main_socket_group: Option<usize>,
    /// Build-level config.
    pub config: BuildConfig,
}

impl Build {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_character(mut self, character: CharacterIdentity) -> Self {
        self.character = character;
        self
    }

    pub fn with_view_mode(mut self, view_mode: ViewMode) -> Self {
        self.view_mode = view_mode;
        self
    }

    /// Sets the main skill group index (PoB `mainSocketGroup`, 1-based).
    pub fn with_main_socket_group(mut self, group: usize) -> Self {
        self.main_socket_group = Some(group);
        self
    }

    pub fn with_tree(mut self, tree: PassiveTreeSpec) -> Self {
        self.tree = tree;
        self
    }

    /// Sets this build's PoB passive tree version annotation (`<Spec treeVersion>`), returns a new copy.
    pub fn with_tree_version(mut self, version: Option<String>) -> Self {
        self.tree_version = version;
        self
    }

    pub fn with_config(mut self, config: BuildConfig) -> Self {
        self.config = config;
        self
    }

    /// Sets the item in a slot, returns a new copy.
    pub fn set_item(mut self, slot: EquipmentSlot, item: Item) -> Self {
        self.items.insert(slot, item);
        self
    }

    /// Sets the jewel list, returns a new copy.
    pub fn with_jewels(mut self, jewels: Vec<Item>) -> Self {
        self.jewels = jewels;
        self
    }

    /// Sets the radius jewel geometric expansion list, returns a new copy.
    pub fn with_radius_jewels(mut self, radius_jewels: Vec<RadiusJewel>) -> Self {
        self.radius_jewels = radius_jewels;
        self
    }

    /// Sets the active flask/charm slot-name list (see [`Self::utility_slots`]), returns a new copy.
    pub fn with_utility_slots(mut self, slots: Vec<(String, Item)>) -> Self {
        self.utility_slots = slots;
        self
    }

    /// Appends a skill gem group, returns a new copy.
    pub fn add_socket_group(mut self, group: SocketGroup) -> Self {
        self.socket_groups.push(group);
        self
    }

    /// Iterates equipped items (slot + item) in deterministic `slot.id()` lexical order.
    pub fn equipped_items(&self) -> Vec<(EquipmentSlot, &Item)> {
        let mut entries: Vec<(EquipmentSlot, &Item)> = self
            .items
            .iter()
            .map(|(slot, item)| (*slot, item))
            .collect();
        entries.sort_by_key(|(slot, _)| slot.id());
        entries
    }

    /// Enabled skill gem groups.
    pub fn enabled_socket_groups(&self) -> impl Iterator<Item = &SocketGroup> {
        self.socket_groups.iter().filter(|g| g.enabled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pobr_data::item::{ItemBaseId, ItemRarity, RolledDefence};

    fn sample_item() -> Item {
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
        }
    }

    #[test]
    fn build_accumulates_items_deterministically() {
        let build = Build::new()
            .set_item(EquipmentSlot::Ring1, sample_item())
            .set_item(EquipmentSlot::Amulet, sample_item());
        let slots: Vec<_> = build.equipped_items().into_iter().map(|(s, _)| s).collect();
        assert_eq!(slots.len(), 2);
        assert!(slots.contains(&EquipmentSlot::Ring1));
        assert!(slots.contains(&EquipmentSlot::Amulet));
    }

    #[test]
    fn enabled_socket_groups_filters_disabled() {
        let build = Build::new()
            .add_socket_group(SocketGroup::new().with_gem("a").with_enabled(true))
            .add_socket_group(SocketGroup::new().with_gem("b").with_enabled(false));
        let enabled: Vec<_> = build.enabled_socket_groups().collect();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].gem_ids, vec!["a".to_string()]);
    }
}
