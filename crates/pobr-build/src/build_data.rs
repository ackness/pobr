//! [`BuildData`]: collapses [`GameData`]'s per-domain query results into the in-memory
//! indexes the orchestrator needs.
//!
//! [`crate::calc_orchestrator::calculate_with_data`] needs to resolve the stable ids
//! stored in a Build (passive node `skill` id, gem id, class name) into calculable
//! modifier sources. That resolution depends on game data, and file I/O is confined to
//! [`pobr_gamedata::GameData`]. This module, once the caller has **already loaded**
//! [`GameData`], projects the needed domains into in-memory indexes all at once (node
//! table / gem table / class base attributes), so the orchestrator can query them with
//! zero additional I/O.
//!
//! Design constraint: this crate holds no file I/O itself — [`GameData`] is constructed
//! and passed in by the caller; this module only reads its per-domain loaders and lands
//! the result as a deterministic in-memory structure.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};

use pobr_core::HighPrecisionRules;
use pobr_core::rules::stat_map_engine::StatMapCatalog;
use pobr_data::catalog::buffs::BuffDef;
use pobr_data::catalog::curse_priority::CursePriorityDef;
use pobr_data::catalog::item_granted_skills::ItemGrantedSkillVariant;
use pobr_data::catalog::jewel_radii::JewelRadiiDef;
use pobr_data::catalog::local_mods::LocalModsDef;
use pobr_data::catalog::{
    BaseItemDef, CostTypeDef, GemEffectDef, GrantedEffectDef, PassiveNodeDef, QualityStat,
    RuntimeConstants, SkillGemDef, SkillLevelDef, SkillStatSetDef, TriggerConfigDef,
};
use pobr_data::minion::MinionDef;
use pobr_gamedata::ruleset::ConfigCatalog;
use pobr_gamedata::{GameData, LoadError};

mod equipment;
mod passives;
mod rules;
mod skills;

/// Class base attributes (PoE2 starting str/dex/int), used to derive [`pobr_core::CharacterBase`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassBaseAttributes {
    pub strength: i32,
    pub dexterity: i32,
    pub intelligence: i32,
}

/// Calc-relevant parameters resolved for an active skill at a given level (all time
/// units are seconds). Re-exported from `pobr-core::skill_env` (the engine-semantics
/// layer owns the resolution; `BuildData` implements its lookup traits).
pub use pobr_core::skill_env::ResolvedSkillLevel;

/// A granted effect's mappable stats at a given (gem level, quality), split into
/// `base`/`quality` segments by source (contract C1). Re-exported from
/// `pobr-core::skill_env`.
pub use pobr_core::skill_env::EffectStats;

/// A stat snapshot for one **unselected statSet** of a granted effect (the data
/// source for global-only merge). Re-exported from `pobr-core::skill_env`.
pub use pobr_core::skill_env::UnselectedSetStats;

/// One resolved skill resource cost. Re-exported from `pobr-core::skill_env`.
pub use pobr_core::skill_env::ResolvedCost;

/// In-memory indexes projected from [`GameData`] for the orchestrator's calculations.
///
/// The results of each domain's lazy resolution are **pre-resolved into in-memory
/// structures** here (the caller loads once and reuses many times):
/// - `passive_nodes`: `skill id -> node definition` (for [`pobr_tree::collect_allocated_mods`]);
/// - `skill_gems`: `gem id -> gem definition` (for active/support classification);
/// - `class_attributes`: `class name -> base attributes` (for deriving CharacterBase).
#[derive(Debug, Clone)]
pub struct BuildData {
    /// Snapshot version for binding optional native packets; absent in text-only data.
    pub native_data_version: Option<String>,
    /// Passive node table, keyed by the numeric `skill` id.
    pub passive_nodes: HashMap<u32, PassiveNodeDef>,
    /// Node table for historical league tree versions (`treeVersion -> skill id -> node
    /// definition`; a minimal-field extract from vendor `TreeData/<v>/tree.lua`, see
    /// `base/passive_trees/`). The current default version is **not** in this table —
    /// [`Self::passive_nodes_for`] falls back to [`Self::passive_nodes`] on a miss; once
    /// a league updates, the old default version just needs extracting into this table.
    pub versioned_passive_nodes: HashMap<String, HashMap<u32, PassiveNodeDef>>,
    /// Skill gem table, keyed by stable gem id (e.g. `Metadata/Items/Gem/...`).
    pub skill_gems: HashMap<String, SkillGemDef>,
    /// Class base attribute table, keyed by the English canonical class name (e.g. `Ranger`).
    pub class_attributes: HashMap<String, ClassBaseAttributes>,
    /// Granted effect table, keyed by `GrantedEffects.Id` (e.g.
    /// `ExplosiveGrenadePlayer`) — the target that PoB's `<Gem skillId>` points to, used
    /// to resolve an active skill's cast/cost.
    pub granted_effects: HashMap<String, GrantedEffectDef>,
    /// Per-level parameter table for granted effects, keyed by `GrantedEffects.Id` (level array, ascending).
    pub granted_effect_levels: HashMap<String, Vec<SkillLevelDef>>,
    /// Per-level **damage stat sets** for granted effects, keyed by `GrantedEffects.Id`
    /// (resolved damage stats for each level).
    pub skill_stat_sets: HashMap<String, SkillStatSetDef>,
    /// Whether the stat sets list the game's valueless stats, so that a set
    /// says what its hits are (`base_is_projectile`, `is_area_damage`). False
    /// for a snapshot adapted before the adapter read them: there the types
    /// of the skill are all there is to go by.
    pub stat_sets_mark_hits: bool,
    /// Gem quality stat slopes (`overlay/gem_quality_stats.json`), keyed by
    /// `GrantedEffects.Id`. Empty when an old data pack lacks this overlay domain
    /// (quality produces no stats, backward compatible).
    pub gem_quality_stats: HashMap<String, Vec<QualityStat>>,
    /// Gem → granted-effect links, keyed by the **primary effect id**
    /// (`granted_effect_id`) — used by meta/composite gem expansion (T5.6) to look up
    /// additional effects forward from a socket group's gem `skill_id` (= the primary
    /// effect id). Built from the gems' own links (the game's `GemEffects` table), with
    /// `overlay/gem_effects.json` overriding where it lists an effect; empty when a data
    /// pack has neither (no expansion, backward compatible).
    pub gem_effects: HashMap<String, GemEffectDef>,
    /// Cost resource type table (indexed by `CostTypes`, ascending; empty means an old
    /// data pack lacks this domain).
    pub cost_types: Vec<CostTypeDef>,
    /// Item base table, keyed by the English canonical name (used to resolve an
    /// equipped item's `Item.base` name to its weapon/armour base stats).
    pub base_items: HashMap<String, BaseItemDef>,
    /// Runtime constants injected into calc (the injection pipeline): assembled by
    /// merging the domains `GameData::load_ruleset()` has data-driven; domains not yet
    /// data-driven, or with a missing file, fall back to `Default` (value-for-value
    /// equal to the JSON). Injected into pobr-core by `calculate_with_data` via
    /// `CalculationSession::set_constants`.
    pub constants: Arc<RuntimeConstants>,
    /// Radius jewel ring tier table (`base/jewel_radii.json`): distance multipliers +
    /// tier label→inner/outer. Consumed by this crate's tree geometry
    /// (`radius_jewel_grant_modifiers` → pobr-tree's
    /// `compute_radius_jewel_effect_with_radii`), doesn't flow into pobr-core through
    /// `RuntimeConstants`. Falls back to `Default` when data is missing (value-for-value
    /// equal to the JSON).
    pub jewel_radii: JewelRadiiDef,
    pub passive_jewels: pobr_data::catalog::passive_jewels::PassiveJewelData,
    /// Local mod allowlist (`overlay/local_mods.json`).
    /// Falls back to the built-in [`LocalModsDef::default`] when a data pack lacks this
    /// overlay file (a mirror that matches the JSON value-for-value, no behavior change).
    pub local_mods: LocalModsDef,
    /// Names of the base items whose granted skills reserve no Spirit
    /// (`overlay/item_granted_skills.json`): the Lament, Portent and Absent
    /// Amulets. Empty when a data pack lacks the file.
    pub reservation_free_grant_bases: HashSet<String>,
    /// Proven item-specific effect identities and their exported aliases.
    pub item_granted_skill_variants: Vec<ItemGrantedSkillVariant>,
    /// statmap data catalog (`overlay/skill_stat_map.json` → [`StatMapCatalog`], the data
    /// source for the [`crate::StatMapMode::Data`] channel once switched to default).
    /// `calculate_with_data` falls back to this when the orchestrator options don't
    /// explicitly inject a catalog; `None` when the overlay file is missing (old data
    /// pack) — the data channel misses entirely and injects no mapping.
    pub stat_map_catalog: Option<Arc<StatMapCatalog>>,
    /// Built-in buff definition table (`overlay/buff_definitions.json`).
    /// Injected by `calculate_with_data` via `CalculationSession::set_buff_definitions`,
    /// consumed at env_finalize stage 6 (the doActorMisc equivalent) — the whole stage is
    /// gated by `cfg.mode_combat` (default false), so the injection itself is a zero
    /// behavior change. Empty when the overlay file is missing (old data pack) — no
    /// built-in buff expansion, backward compatible.
    pub buff_definitions: Vec<BuffDef>,
    /// Curse priority data table (`overlay/curse_priority.json`).
    /// Injected by `calculate_with_data` via `CalculationSession::set_curse_priority`,
    /// consumed by env_finalize stage 4 (buff_pass)'s curse priority/limit logic — the
    /// whole stage is gated by `cfg.mode_buffs` (default false), so the injection itself
    /// is a zero behavior change. `None` when the overlay file is missing (old data
    /// pack) — the consumer falls back to all weights being 0 (tolerant of a missing table).
    pub curse_priority: Option<CursePriorityDef>,
    /// config option catalog (`overlay/config_options.json`).
    /// Consumed by `calculate_with_data` via `crate::config_resolve::resolve_config` —
    /// `Some` goes through the `config_interpreter::interpret` primary path; `None` (old
    /// data pack / [`BuildData::empty`]) falls back to the legacy parse_config output
    /// (tolerant of a missing table).
    pub config_catalog: Option<Arc<ConfigCatalog>>,
    /// Trigger config recognition index: granted effect id (from `match_effect_ids`) →
    /// `overlay/trigger_configs.json` entry (transcribed from vendor
    /// CalcTriggers.lua's configTable). The orchestrator's trigger stage looks up this
    /// table by a socket group's gem / main skill id to recognize gem-link/triggeredBy
    /// relationships. Empty when the overlay file is missing (old data pack) — no
    /// recognition coverage, no behavior change; entries with an unmapped PoE2 id
    /// (mostly PoE1 uniques) aren't indexed.
    pub trigger_configs: HashMap<String, TriggerConfigDef>,
    /// Rounding-precision rules (`overlay/high_precision_mods.json` → `RuleSet` →
    /// pobr-core's [`HighPrecisionRules`], deduplicated wiring). Injected by
    /// `calculate_with_data` via `CalculationSession::set_high_precision_rules`,
    /// consumed by buff_pass / merge_flasks_charms's ScaleAddMod scaling (T1's write
    /// primitive uses the same rule set). Falls back to
    /// [`HighPrecisionRules::default`] when the overlay file is missing (old data pack)
    /// (no exception table).
    pub high_precision: HighPrecisionRules,
    /// Minion / spectre definition table, keyed by minion id. `minions.json` takes
    /// priority, falling back to `spectres.json` on a miss (spectre key = full metadata
    /// path). Empty when the overlay file is missing (old data pack) — no minions,
    /// backward compatible. Only queried by consumers through [`Self::minion_def`].
    pub minions: HashMap<String, MinionDef>,
    /// What can be put in an item's sockets (Runes, Idols, Soul Cores), keyed
    /// by name (`overlay/runes.json`): its kind and the lines it gives each
    /// class of item. The calculation reads an item's modifiers from the item's
    /// own text; this table says which of them belong to an Idol
    /// (`calc_orchestrator::item::bonded`). Empty when the overlay file is
    /// missing: no line is attributed to an Idol.
    pub augments: HashMap<String, pobr_data::catalog::RuneDef>,
    /// Data-driven ModParser engine rules (the sole parser): `overlay/mod_parser_rules.json`
    /// compiled via [`CompiledParserRules::compile_with_special`] (the special channel
    /// reuses `special_mods` + `special_derived`, concatenated). Injected by
    /// `calculate_with_data` via [`CalculationSession::set_parser_rules`]; every ingested
    /// mod is parsed through the data-driven scan engine. `None` when the
    /// `mod_parser_rules` domain is missing (old data pack) — no parser: every mod is
    /// collected wholesale as Unsupported.
    ///
    /// [`CompiledParserRules::compile_with_special`]: pobr_core::mod_parser::CompiledParserRules::compile_with_special
    /// [`CalculationSession::set_parser_rules`]: pobr_core::calc::CalculationSession::set_parser_rules
    pub parser_rules: Option<Arc<pobr_core::mod_parser::CompiledParserRules>>,
    /// The monster each totem-summoning skill places, keyed by its
    /// `GrantedEffects.Id` (`overlay-common/totem_varieties.json`, curated from
    /// the game's `ActiveSkills` / `SkillTotemVariations` / `MonsterVarieties`),
    /// with its base resistances from `base/monster_resistances.json`.
    pub totems: HashMap<String, TotemDef>,
    /// Lookup tables derived from the fields above, built on first use. See
    /// [`DerivedIndexes`].
    pub derived: DerivedIndexes,
}

/// Lookup tables that are pure functions of domains fixed once the data is loaded,
/// but that one calculation needs several times (each build of one walks every
/// base item or granted effect). Built on first use and kept for the life of the
/// [`BuildData`]. Loaded data is not edited, so they never go stale; a caller that
/// does edit `passive_nodes`, `granted_effects`, `gem_effects`, `base_items` or
/// `item_granted_skill_variants` after a calculation has used the data must call
/// [`BuildData::reset_derived_indexes`] (a value built with `..BuildData::empty()`
/// starts with none).
#[derive(Debug, Clone, Default)]
pub struct DerivedIndexes {
    skill_id_by_gem_name: OnceLock<HashMap<String, String>>,
    effect_id_by_normalized_name: OnceLock<HashMap<String, String>>,
    notable_skill_by_name: OnceLock<HashMap<String, u32>>,
    class_start_nodes: OnceLock<std::collections::BTreeMap<String, u32>>,
}

/// A name as `nameSpec` lookups compare it: lowercase letters and digits only.
pub(crate) fn normalize_gem_name(name: &str) -> String {
    name.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

impl BuildData {
    /// Forgets the lookup tables derived from the domains, so the next use rebuilds
    /// them. Only needed after editing a domain they are built from (see
    /// [`DerivedIndexes`]).
    pub fn reset_derived_indexes(&mut self) {
        self.derived = DerivedIndexes::default();
    }

    /// Gem display name (lowercase) to primary effect id (same source as the gem
    /// catalog: base items' gem name joined with the gem effects' gem id link).
    pub fn skill_id_by_gem_name(&self) -> &HashMap<String, String> {
        self.derived.skill_id_by_gem_name.get_or_init(|| {
            let gem_to_skill: HashMap<&str, &str> = self
                .gem_effects
                .iter()
                .map(|(skill_id, def)| (def.gem_id.as_str(), skill_id.as_str()))
                .collect();
            let mut by_name: HashMap<String, String> = self
                .base_items
                .iter()
                .filter_map(|(name, def)| {
                    gem_to_skill
                        .get(def.id.as_str())
                        .map(|skill_id| (name.to_lowercase(), (*skill_id).to_string()))
                })
                .collect();
            // The base-item name index collapses ordinary and unique skill gems.
            // Without an item attachment, a shared name means the ordinary effect.
            for variant in &self.item_granted_skill_variants {
                if self.gem_effects.contains_key(&variant.exported_effect_id) {
                    by_name.insert(
                        variant.skill_name.to_lowercase(),
                        variant.exported_effect_id.clone(),
                    );
                }
            }
            by_name
        })
    }

    /// Each class's starting node in the passive tree `tree_version` selects, by the
    /// ids the passive-jewel catalog names (the lowest node id when several share
    /// one). Depends on the tree only, so the current tree's answer is kept.
    pub(crate) fn passive_class_starts(
        &self,
        tree_version: Option<&str>,
    ) -> std::borrow::Cow<'_, std::collections::BTreeMap<String, u32>> {
        let nodes = self.passive_nodes_for(tree_version);
        let find = || {
            let mut starts = std::collections::BTreeMap::new();
            for (class, id) in &self.passive_jewels.class_starts {
                if let Some(node) = nodes
                    .values()
                    .filter(|n| &n.id == id)
                    .min_by_key(|n| n.skill)
                {
                    starts.insert(class.clone(), node.skill);
                }
            }
            starts
        };
        if std::ptr::eq(nodes, &self.passive_nodes) {
            std::borrow::Cow::Borrowed(self.derived.class_start_nodes.get_or_init(find))
        } else {
            std::borrow::Cow::Owned(find())
        }
    }

    /// The Notable named `name` (ASCII case ignored) in the passive tree `tree_version`
    /// selects; when several nodes share the name, the one with the lowest id.
    pub(crate) fn notable_named(
        &self,
        tree_version: Option<&str>,
        name: &str,
    ) -> Option<&PassiveNodeDef> {
        use pobr_data::catalog::PassiveNodeKind;
        let nodes = self.passive_nodes_for(tree_version);
        if !std::ptr::eq(nodes, &self.passive_nodes) {
            // A historical tree is rare, and a name is looked up in it only a few times.
            return nodes
                .values()
                .filter(|def| def.kind == PassiveNodeKind::Notable)
                .filter(|def| {
                    def.name
                        .as_ref()
                        .is_some_and(|n| n.eq_ignore_ascii_case(name))
                })
                .min_by_key(|def| def.skill);
        }
        let index = self.derived.notable_skill_by_name.get_or_init(|| {
            let mut index: HashMap<String, u32> = HashMap::new();
            for def in nodes.values() {
                if def.kind != PassiveNodeKind::Notable {
                    continue;
                }
                if let Some(name) = &def.name {
                    index
                        .entry(name.to_ascii_lowercase())
                        .and_modify(|skill| *skill = (*skill).min(def.skill))
                        .or_insert(def.skill);
                }
            }
            index
        });
        index
            .get(&name.to_ascii_lowercase())
            .and_then(|skill| nodes.get(skill))
    }

    /// Granted effect id by [`normalize_gem_name`] of its display stem: the `Player`
    /// suffix and a `Support` prefix are dropped, so `SupportAtzirisCommunionPlayer`
    /// answers to "atziriscommunion", the normal form of "Atziri's Communion".
    ///
    /// Some stems name two effects (`Unleash` is both an active gem and a support gem,
    /// `UnleashPlayer` and `SupportUnleashPlayer`; 86 stems do). The name alone cannot
    /// say which a gem entry means, so the choice is fixed by [`outranks_for_name`]
    /// rather than left to the order the map happens to iterate in, which differs from
    /// one process to the next.
    pub(crate) fn effect_id_by_normalized_name(&self) -> &HashMap<String, String> {
        self.derived.effect_id_by_normalized_name.get_or_init(|| {
            let mut lookup: HashMap<String, String> = HashMap::new();
            for id in self.granted_effects.keys() {
                let stem = id.strip_suffix("Player").unwrap_or(id);
                let stem = stem.strip_prefix("Support").unwrap_or(stem);
                match lookup.entry(normalize_gem_name(stem)) {
                    std::collections::hash_map::Entry::Occupied(mut held) => {
                        if outranks_for_name(id, held.get()) {
                            held.insert(id.clone());
                        }
                    }
                    std::collections::hash_map::Entry::Vacant(slot) => {
                        slot.insert(id.clone());
                    }
                }
            }
            lookup
        })
    }
}

/// Whether effect id `candidate` is the better reading of a gem name than `held`, when
/// both normalize to it: a support effect first (a socket group has many supports and
/// one active skill, and the entries that name a gem without its effect id are the
/// ones the importer could not look up), then the smaller id.
pub(crate) fn outranks_for_name(candidate: &str, held: &str) -> bool {
    let key = |id: &str| (id.starts_with("Support"), std::cmp::Reverse(id.to_owned()));
    key(candidate) > key(held)
}

/// A totem monster's game base stats ([`BuildData::totems`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TotemDef {
    /// `MonsterVarieties.LifeMultiplier` as a factor (1.2 for the Ancestral Warrior's Slam Totem).
    pub life_multiplier: f64,
    /// Base Fire, Cold, Lightning and Chaos resistances of the variety's monster type.
    pub resistances: [f64; 4],
    /// `MonsterTypes.Armour` / `Evasion` as factors of the level's
    /// `DefaultMonsterStats.Armour` / `Evasion` (1 for the Spell Totem's armour).
    pub armour: f64,
    pub evasion: f64,
    /// `MonsterTypes.EnergyShieldFromLife` in percent of maximum Life.
    pub energy_shield_from_life: f64,
}

fn totem_defs(
    def: pobr_data::catalog::totem_varieties::TotemVarietiesDef,
    resistances: &pobr_data::catalog::MonsterResistancesDef,
) -> HashMap<String, TotemDef> {
    def.totems
        .into_iter()
        .map(|row| {
            let res = resistances
                .monsters
                .get(&row.monster_variety)
                .map_or([0.0; 4], |b| {
                    [b.fire, b.cold, b.lightning, b.chaos].map(f64::from)
                });
            (
                row.granted_effect,
                TotemDef {
                    life_multiplier: f64::from(row.life_multiplier) / 100.0,
                    resistances: res,
                    armour: f64::from(row.armour) / 100.0,
                    evasion: f64::from(row.evasion) / 100.0,
                    energy_shield_from_life: f64::from(row.energy_shield_from_life),
                },
            )
        })
        .collect()
}

/// One `stats` entry per text line. The tree export packs a node's lines into
/// a single newline-joined entry for some nodes (most keystones, some notables)
/// and writes one entry per line for the rest; every reader of `stats` parses
/// an entry as one modifier line, so a packed entry parsed as nothing.
fn one_stat_per_line(mut node: PassiveNodeDef) -> PassiveNodeDef {
    if node.stats.iter().any(|stat| stat.contains('\n')) {
        node.stats = node
            .stats
            .iter()
            .flat_map(|stat| stat.lines())
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(String::from)
            .collect();
    }
    node
}

impl BuildData {
    /// Loads and projects every domain the orchestrator needs from an already-constructed [`GameData`].
    ///
    /// This is the only entry point that triggers [`GameData`] I/O; returns
    /// [`LoadError`] on failure (missing file / parse error). Callers should cache the
    /// return value rather than reloading the same version directory repeatedly.
    pub fn load(data: &GameData) -> Result<Self, LoadError> {
        data.validate_manifest()?;
        data.validate_calculation_admission()?;
        // Pin report provenance alongside the immutable data used by this build.
        data.identity()?;
        // A domain's file is released as soon as it is parsed (a data pack staged in memory to
        // release them; any other keeps its files): its bytes are not resident beside what
        // was parsed from them. Only files nothing reads after this load are released here.
        let passive_nodes = data
            .passive_nodes()?
            .into_iter()
            .map(|node| (node.skill, one_stat_per_line(node)))
            .collect();
        data.release_domain("passive_tree.json");

        // Historical league trees (mirroring PoB2's multi-version TreeData):
        // `base/passive_trees/*.json` is fully preloaded (~0.5-0.8MB per version; a
        // missing directory = empty table, zero cost).
        let mut versioned_passive_nodes: HashMap<String, HashMap<u32, PassiveNodeDef>> =
            HashMap::new();
        for version in data.available_tree_versions() {
            if let Some(nodes) = data.passive_nodes_versioned(&version)? {
                versioned_passive_nodes.insert(
                    version.clone(),
                    nodes
                        .into_iter()
                        .map(|node| (node.skill, one_stat_per_line(node)))
                        .collect(),
                );
                data.release_domain(&format!("passive_trees/{version}.json"));
            }
        }

        let skill_gems: HashMap<String, SkillGemDef> = data
            .skill_gems()?
            .into_iter()
            .map(|gem| (gem.id.clone(), gem))
            .collect();
        data.release_domain("skill_gems.json");

        let class_attributes = data
            .passive_tree_meta()?
            .classes
            .into_iter()
            .map(|class| {
                (
                    class.name,
                    ClassBaseAttributes {
                        strength: class.base_str,
                        dexterity: class.base_dex,
                        intelligence: class.base_int,
                    },
                )
            })
            .collect();

        let granted_effects = data
            .granted_effects()?
            .into_iter()
            .map(|effect| (effect.id.clone(), effect))
            .collect();
        data.release_domain("granted_effects.json");

        let granted_effect_levels = data.granted_effect_levels()?.into_iter().collect();
        data.release_domain("granted_effect_levels.json");

        let skill_stat_sets: HashMap<String, SkillStatSetDef> = data
            .skill_stat_sets()?
            .into_iter()
            .map(|set| (set.effect_id.clone(), set))
            .collect();
        data.release_domain("granted_effect_stat_sets.json");
        let stat_sets_mark_hits = skill_stat_sets
            .values()
            .flat_map(|def| &def.sets)
            .any(|set| {
                set.implicit_stats
                    .iter()
                    .any(|stat| stat == pobr_core::skill_env::STAT_IS_PROJECTILE)
            });

        // Quality stat slopes (overlay domain): missing file = empty table (quality produces no stats, backward compatible).
        let gem_quality_stats = data
            .gem_quality_stats()?
            .map(|def| {
                def.effects
                    .into_iter()
                    .map(|e| (e.effect_id, e.stats))
                    .collect()
            })
            .unwrap_or_default();

        // Gem → effect links, indexed by primary effect id (the forward lookup key for meta
        // expansion). A gem comes with the game's link; an effect several gems grant is
        // indexed under the first of them. `overlay/gem_effects.json` holds the links of a
        // snapshot adapted before the adapter joined the game's `GemEffects` table, and
        // where it lists an effect its gem stands for it.
        let mut linked_gems: Vec<&SkillGemDef> = skill_gems.values().collect();
        linked_gems.sort_by(|a, b| a.id.cmp(&b.id));
        let mut gem_effects: HashMap<String, GemEffectDef> = HashMap::new();
        for gem in linked_gems {
            let Some(effect_id) = &gem.granted_effect_id else {
                continue;
            };
            gem_effects
                .entry(effect_id.clone())
                .or_insert_with(|| GemEffectDef {
                    gem_id: gem.id.clone(),
                    variant_id: gem.gem_effect_id.clone().unwrap_or_default(),
                    granted_effect_id: effect_id.clone(),
                    additional_granted_effect_ids: gem.additional_granted_effect_ids.clone(),
                    granted_effect_display_order: Vec::new(),
                    additional_stat_set_ids: Vec::new(),
                });
        }
        if let Some(def) = data.gem_effects()? {
            for link in def.gems {
                gem_effects.insert(link.granted_effect_id.clone(), link);
            }
        }

        let cost_types = data.cost_types()?;

        let base_items = data
            .base_items()?
            .into_iter()
            .map(|b| (b.name.clone(), b))
            .collect();

        // Merge the RuleSet's already data-driven domains into the constants bundle;
        // domains that are None keep the Default fallback (equal to the JSON
        // value-for-value, so the injected and fallback paths produce the same output).
        let ruleset = data.load_ruleset()?;
        let mut constants = RuntimeConstants::default();
        if let Some(game_constants) = ruleset.game_constants {
            constants.game_constants = game_constants;
        }
        if let Some(character_constants) = ruleset.character_constants {
            constants.character_constants = character_constants;
        }
        if let Some(monster_scaling) = ruleset.monster_scaling {
            constants.monster_scaling = monster_scaling;
        }
        if let Some(enemy_presets) = ruleset.enemy_presets {
            constants.enemy_presets = enemy_presets;
        }
        // Curated game rows until the adapter exports MonsterVarieties.PoiseThreshold.
        // Old packs retain the game-sourced defaults; malformed files still fail loading.
        match data.boss_poise_thresholds() {
            Ok(def) => constants.boss_poise_thresholds = def,
            Err(LoadError::Io { ref source, .. })
                if source.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        if let Some(unarmed_data) = ruleset.unarmed_data {
            constants.unarmed_data = unarmed_data;
        }
        if let Some(weapon_types) = ruleset.weapon_types {
            constants.weapon_types = weapon_types;
        }
        // Radius jewel tier table: overridden when the data-driven domain is Some, falls back to Default when None (equal to the JSON value-for-value).
        let jewel_radii = ruleset.jewel_radii.unwrap_or_default();
        // Rounding-precision exception table: missing overlay file = Default (no exception table).
        let high_precision = ruleset
            .high_precision_mods
            .map(HighPrecisionRules::from_def)
            .unwrap_or_default();

        // Local mod allowlist: degrades to the built-in fallback (matches the JSON
        // value-for-value) when the overlay file is missing (old data pack); every other
        // load/parse error still propagates as normal, never silenced.
        let local_mods = match data.local_mods() {
            Ok(def) => def,
            Err(LoadError::Io { ref source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                LocalModsDef::default()
            }
            Err(e) => return Err(e),
        };

        // The same tolerance for the rules of item-granted skills: a data pack
        // without the file knows no base whose granted skills reserve nothing.
        let item_granted_skills = match data.item_granted_skills() {
            Ok(def) => def,
            Err(LoadError::Io { ref source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                Default::default()
            }
            Err(e) => return Err(e),
        };
        let reservation_free_grant_bases = item_granted_skills
            .reservation_free_bases
            .into_iter()
            .collect();
        let item_granted_skill_variants = item_granted_skills.variants;

        // statmap data catalog (the data source for the default Data channel). `None`
        // when the overlay file is missing (old data pack) — the data channel misses
        // entirely; every other load/parse error still propagates as normal.
        let stat_map_catalog = data
            .skill_stat_map()?
            .map(|def| Arc::new(StatMapCatalog::new(def)));

        // Built-in buff definitions: missing file = empty table (no built-in buff
        // expansion, backward compatible); every other load/parse error still propagates as normal.
        let buff_definitions = data
            .buff_definitions()?
            .map(|def| def.buffs)
            .unwrap_or_default();

        // Curse priority table: missing file = None (the consumer falls back to all
        // weights being 0); every other load/parse error still propagates as normal.
        let curse_priority = data.curse_priority()?;

        // config option catalog: missing file = `None` (the consumer falls back to the
        // legacy parse_config output, tolerant of a missing table); every other
        // load/parse error still propagates as normal.
        let config_catalog = ruleset.config_catalog.map(Arc::new);

        // Trigger config recognition index: expanded by match_effect_ids into effect id
        // → entry. Missing overlay file (old data pack) = empty table (no recognition coverage, no behavior change).
        let trigger_configs = data
            .trigger_configs()?
            .map(|def| {
                def.configs
                    .into_iter()
                    .flat_map(|config| {
                        config
                            .match_effect_ids
                            .clone()
                            .into_iter()
                            .map(move |id| (id, config.clone()))
                            .collect::<Vec<_>>()
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Minion / spectre definitions: minions get priority in the index, spectres fill
        // in the gaps (spectre key = metadata path, doesn't collide with minion id).
        // Missing file = empty table (no minions, backward compatible); every other
        // load/parse error still propagates as normal.
        let mut minions: HashMap<String, MinionDef> = HashMap::new();
        let resistances = data.monster_resistances()?;
        // Curated game rows until the adapter exports the totem tables; a data
        // pack without the file keeps the same rows (`TotemVarietiesDef::default`).
        let totems = totem_defs(
            match data.totem_varieties() {
                Ok(def) => def,
                Err(LoadError::Io { ref source, .. })
                    if source.kind() == std::io::ErrorKind::NotFound =>
                {
                    Default::default()
                }
                Err(e) => return Err(e),
            },
            &resistances,
        );
        let minion_def = |entry: &pobr_data::catalog::actors::MinionEntryDef| {
            let mut def = MinionDef::from_entry(entry);
            let variety = entry.monster_variety_id.as_deref().unwrap_or(&entry.id);
            if let Some(base) = resistances.monsters.get(variety) {
                def.fire_resist = f64::from(base.fire);
                def.cold_resist = f64::from(base.cold);
                def.lightning_resist = f64::from(base.lightning);
                def.chaos_resist = f64::from(base.chaos);
            }
            def
        };
        if let Some(spectres) = data.spectres()? {
            for entry in &spectres.minions {
                minions.insert(entry.id.clone(), minion_def(entry));
            }
        }
        if let Some(minion_defs) = data.minions()? {
            // minions overrides spectres on a shared id (they don't actually overlap in
            // practice; the override just guarantees minions-takes-priority semantics).
            for entry in &minion_defs.minions {
                minions.insert(entry.id.clone(), minion_def(entry));
            }
        }
        let augments = data
            .runes()?
            .map(|table| {
                table
                    .runes
                    .into_iter()
                    .map(|augment| (augment.name.clone(), augment))
                    .collect()
            })
            .unwrap_or_default();
        // The special entry array (RuleSet's three-source concatenation) — compiled into the engine's special channel.
        let special_entries = ruleset.special_mods.unwrap_or_default();

        // Compiles the data-driven ModParser engine rules (the sole parser).
        // gamedata only loads the `mod_parser_rules.json` doc; compilation happens in
        // core (the I/O boundary). Missing doc (old data pack) = None (no parser: mods
        // are collected wholesale as Unsupported); compile failure still propagates as normal.
        let parser_rules = match data.mod_parser_rules()? {
            Some(doc) => Some(Arc::new(
                pobr_core::mod_parser::CompiledParserRules::compile_with_special(
                    &doc,
                    &special_entries,
                )
                .map_err(|e| LoadError::Overlay {
                    path: "overlay/mod_parser_rules.json".into(),
                    message: format!("parser rule compilation failed: {e}"),
                })?,
            )),
            None => None,
        };

        Ok(Self {
            native_data_version: data.identity()?.data_version.clone(),
            passive_nodes,
            versioned_passive_nodes,
            skill_gems,
            class_attributes,
            granted_effects,
            granted_effect_levels,
            skill_stat_sets,
            stat_sets_mark_hits,
            gem_quality_stats,
            gem_effects,
            cost_types,
            base_items,
            constants: Arc::new(constants),
            jewel_radii,
            passive_jewels: data.passive_jewels()?.unwrap_or_default(),
            local_mods,
            reservation_free_grant_bases,
            item_granted_skill_variants,
            stat_map_catalog,
            buff_definitions,
            curse_priority,
            config_catalog,
            trigger_configs,
            high_precision,
            minions,
            augments,
            parser_rules,
            totems,
            derived: DerivedIndexes::default(),
        })
    }

    /// Constructs an empty [`BuildData`] (no domain data at all; the local mod
    /// allowlist uses the built-in fallback — it's a classification rule rather than
    /// content data, and an empty table would break weapon-local stripping). Used for
    /// tests or the text-only path fallback.
    pub fn empty() -> Self {
        Self {
            native_data_version: None,
            passive_nodes: HashMap::new(),
            versioned_passive_nodes: HashMap::new(),
            skill_gems: HashMap::new(),
            class_attributes: HashMap::new(),
            granted_effects: HashMap::new(),
            granted_effect_levels: HashMap::new(),
            skill_stat_sets: HashMap::new(),
            stat_sets_mark_hits: false,
            gem_quality_stats: HashMap::new(),
            gem_effects: HashMap::new(),
            cost_types: Vec::new(),
            base_items: HashMap::new(),
            constants: Arc::default(),
            jewel_radii: JewelRadiiDef::default(),
            passive_jewels: Default::default(),
            local_mods: LocalModsDef::default(),
            reservation_free_grant_bases: HashSet::new(),
            item_granted_skill_variants: Vec::new(),
            stat_map_catalog: None,
            buff_definitions: Vec::new(),
            curse_priority: None,
            config_catalog: None,
            trigger_configs: HashMap::new(),
            high_precision: HighPrecisionRules::default(),
            minions: HashMap::new(),
            augments: HashMap::new(),
            parser_rules: None,
            totems: HashMap::new(),
            derived: DerivedIndexes::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pobr_gamedata::repo_data_root;

    /// The version directory under the repo's built-in data directory (shared with the orchestrator tests).
    pub(crate) fn repo_version_dir() -> std::path::PathBuf {
        repo_data_root().join(pobr_gamedata::data_version())
    }

    fn effect(id: &str, is_support: bool) -> pobr_data::catalog::GrantedEffectDef {
        pobr_data::catalog::GrantedEffectDef {
            id: id.into(),
            is_support,
            active_skill: None,
            displayed_name: None,
            cast_time: None,
            require_skill_types: vec![],
            add_skill_types: vec![],
            exclude_skill_types: vec![],
            cannot_be_supported: false,
            support_gems_only: false,
            stat_set: None,
            additional_stat_set_ids: vec![],
            cost_types: vec![],
            minion_list: vec![],
            add_minion_list: vec![],
            minion_uses: vec![],
            minion_has_item_set: false,
            ignore_minion_types: false,
            skill_types: vec![],
        }
    }

    /// `Unleash` names two effects. The pick must not depend on the order a map
    /// iterates in, which differs between processes: build the table from many
    /// differently seeded maps and require one answer.
    #[test]
    fn a_name_two_effects_share_resolves_the_same_way_every_time() {
        let ids = [
            ("UnleashPlayer", false),
            ("SupportUnleashPlayer", true),
            ("BlasphemyPlayer", false),
            ("SupportBlasphemyPlayer", true),
            ("DoLiterallyNothing", false),
            ("DoLiterallyNothingPlayer", false),
            ("FirebolPlayer", false),
        ];
        for _ in 0..40 {
            let mut data = BuildData::empty();
            for (id, support) in ids {
                data.granted_effects.insert(id.into(), effect(id, support));
            }
            let lookup = data.effect_id_by_normalized_name();
            assert_eq!(lookup["unleash"], "SupportUnleashPlayer");
            assert_eq!(lookup["blasphemy"], "SupportBlasphemyPlayer");
            assert_eq!(lookup["doliterallynothing"], "DoLiterallyNothing");
            assert_eq!(lookup["firebol"], "FirebolPlayer");
        }
    }

    #[test]
    fn loads_repo_data_domains() {
        let data = GameData::new(repo_version_dir());
        let bd = BuildData::load(&data).expect("load repo data");
        assert!(!bd.passive_nodes.is_empty(), "passive nodes loaded");
        assert!(!bd.skill_gems.is_empty(), "skill gems loaded");
        assert!(!bd.class_attributes.is_empty(), "class attrs loaded");
    }

    #[test]
    fn resolves_known_class_attributes() {
        let data = GameData::new(repo_version_dir());
        let bd = BuildData::load(&data).expect("load");
        let ranger = bd.class_attributes("Ranger").expect("Ranger present");
        // Ranger starting attributes: 7 str / 15 dex / 7 int (passive_tree_meta).
        assert_eq!(ranger.dexterity, 15);
        assert!(bd.class_attributes("NoSuchClass").is_none());
    }

    /// T5.5 (final contract C1): statSet form selection — `set_index` selects a set by
    /// vendor's export index; None / a miss falls back to the primary set (conservative).
    #[test]
    fn effect_stats_selects_stat_set_by_vendor_index() {
        use pobr_data::catalog::{SkillDamageStat, SkillStatSetLevel, StatSetDef};
        fn set(set_id: &str, vendor_idx: Option<u32>, stat: &str, value: f64) -> StatSetDef {
            StatSetDef {
                set_id: set_id.into(),
                label: None,
                vendor_set_index: vendor_idx,
                base_effectiveness: 0.0,
                user_level_scaling: None,
                constant_stats: Vec::new(),
                skill_attack_speed_more: None,
                dot_flags: Default::default(),
                explode_corpse: false,
                implicit_stats: Vec::new(),
                levels: vec![SkillStatSetLevel {
                    gem_level: 1,
                    damage_multiplier: 1.0,
                    stats: vec![SkillDamageStat {
                        stat: stat.into(),
                        value,
                    }],
                    per_effectiveness: Vec::new(),
                }],
            }
        }
        let mut skill_stat_sets = HashMap::new();
        skill_stat_sets.insert(
            "Synth".to_string(),
            pobr_data::catalog::SkillStatSetDef {
                effect_id: "Synth".into(),
                sets: vec![
                    set("SynthMain", Some(1), "spell_minimum_base_fire_damage", 10.0),
                    set("SynthAlt", Some(2), "spell_minimum_base_cold_damage", 99.0),
                ],
            },
        );
        let bd = BuildData {
            skill_stat_sets,
            ..BuildData::empty()
        };
        // Default (None) → the primary set.
        let main = bd.effect_stats("Synth", 1, 0, None);
        assert_eq!(main.base[0].stat, "spell_minimum_base_fire_damage");
        // statSetIndex=2 → the secondary set at vendor index 2 (XML parse → consume round trip).
        let alt = bd.effect_stats("Synth", 1, 0, Some(2));
        assert_eq!(alt.base[0].stat, "spell_minimum_base_cold_damage");
        assert_eq!(alt.base[0].value, 99.0);
        // Index miss (not exported by vendor / out of range) → falls back to the primary set, no wrong pick.
        let miss = bd.effect_stats("Synth", 1, 0, Some(7));
        assert_eq!(miss.base[0].stat, "spell_minimum_base_fire_damage");
        // Same semantics through the resolve chain: Synth isn't in the granted_effects
        // table (resolve returns None); the select path is already covered by
        // effect_stats — this just anchors that the signature is usable.
        assert!(
            bd.resolve_skill_level_with_set("Synth", 1, Some(2))
                .is_none()
        );
    }

    /// The unselected-set snapshot — the selection determination matches effect_stats;
    /// vendor-unexported sets are excluded; the quality segment is stacked per set
    /// (truncated); same stats add together, deterministic lexical order.
    #[test]
    fn unselected_set_stats_snapshot_semantics() {
        use pobr_data::catalog::{QualityStat, SkillDamageStat, SkillStatSetLevel, StatSetDef};
        fn set(
            set_id: &str,
            vendor_idx: Option<u32>,
            level_stats: Vec<(&str, f64)>,
            constant_stats: Vec<(&str, f64)>,
        ) -> StatSetDef {
            let ds = |v: Vec<(&str, f64)>| {
                v.into_iter()
                    .map(|(stat, value)| SkillDamageStat {
                        stat: stat.into(),
                        value,
                    })
                    .collect::<Vec<_>>()
            };
            StatSetDef {
                set_id: set_id.into(),
                label: None,
                vendor_set_index: vendor_idx,
                base_effectiveness: 0.0,
                user_level_scaling: None,
                constant_stats: ds(constant_stats),
                skill_attack_speed_more: None,
                dot_flags: Default::default(),
                explode_corpse: false,
                implicit_stats: Vec::new(),
                levels: vec![SkillStatSetLevel {
                    gem_level: 1,
                    damage_multiplier: 1.0,
                    stats: ds(level_stats),
                    per_effectiveness: Vec::new(),
                }],
            }
        }
        let mut bd = BuildData::empty();
        bd.skill_stat_sets.insert(
            "Synth".to_string(),
            pobr_data::catalog::SkillStatSetDef {
                effect_id: "Synth".into(),
                sets: vec![
                    set("SynthMain", Some(1), vec![("alpha", 10.0)], vec![]),
                    // The per-level row and the constant share a stat (beta) → added together to 5+2=7.
                    set(
                        "SynthAlt",
                        Some(2),
                        vec![("beta", 5.0)],
                        vec![("beta", 2.0), ("gamma", 1.0)],
                    ),
                    // Not exported by vendor (skipped by template curation) → excluded from global merge.
                    set("SynthHidden", None, vec![("hidden", 99.0)], vec![]),
                ],
            },
        );
        // The quality table is shared per effect: it's stacked first for unselected sets too (trunc(0.55×19)=10).
        bd.gem_quality_stats.insert(
            "Synth".into(),
            vec![QualityStat {
                stat: "beta".into(),
                per_quality_rate: 0.55,
                alt: false,
            }],
        );

        // Default (None) → the primary set is selected, unselected = only SynthAlt (Hidden excluded).
        let unsel = bd.unselected_set_stats("Synth", 1, 19, None);
        assert_eq!(unsel.len(), 1);
        assert_eq!(unsel[0].set_key, "2");
        assert_eq!(unsel[0].set_id, "SynthAlt");
        // beta = quality 10 + per-level 5 + constant 2 = 17; gamma = 1; lexical order.
        assert_eq!(
            unsel[0]
                .stats
                .iter()
                .map(|s| (s.stat.as_str(), s.value))
                .collect::<Vec<_>>(),
            vec![("beta", 17.0), ("gamma", 1.0)]
        );

        // Explicitly selecting set 2 → unselected = the primary set (quality is stacked into its snapshot too).
        let unsel = bd.unselected_set_stats("Synth", 1, 0, Some(2));
        assert_eq!(unsel.len(), 1);
        assert_eq!(unsel[0].set_key, "1");
        assert_eq!(unsel[0].set_id, "SynthMain");
        assert_eq!(
            unsel[0]
                .stats
                .iter()
                .map(|s| (s.stat.as_str(), s.value))
                .collect::<Vec<_>>(),
            vec![("alpha", 10.0)]
        );

        // Index miss (out of range) → selection falls back to the primary set, unselected matches the default case; unknown effect → empty.
        assert_eq!(
            bd.unselected_set_stats("Synth", 1, 19, Some(7)),
            bd.unselected_set_stats("Synth", 1, 19, None)
        );
        assert!(bd.unselected_set_stats("NoSuch", 1, 20, None).is_empty());
    }

    /// Gem → effect links get merged into SkillGemDef via overlay/gem_effects.json,
    /// and the meta-expansion index (gem_effects) is built keyed by primary effect id.
    #[test]
    fn gem_effect_links_loaded_from_overlay() {
        let data = GameData::new(repo_version_dir());
        let bd = BuildData::load(&data).expect("load");
        let ice = bd
            .skill_gems
            .get("Metadata/Items/Gems/SkillGemIceNova")
            .expect("IceNova gem present");
        assert_eq!(ice.granted_effect_id.as_deref(), Some("IceNovaPlayer"));
        // The meta-expansion index is queryable by primary effect id (GemSkillRef.skill_id = the primary effect id).
        assert!(bd.gem_effects.contains_key("IceNovaPlayer"));
        // Additional granted-effect foreign key (18-G5): the Blasphemy gem's primary
        // effect BlasphemyPlayer carries SupportBlasphemyPlayer alongside it (vendor
        // Gems.lua's additionalGrantedEffectId1).
        let blasphemy = bd.gem_effects.get("BlasphemyPlayer").expect("Blasphemy");
        assert_eq!(
            blasphemy.additional_granted_effect_ids,
            ["SupportBlasphemyPlayer"]
        );
    }

    #[test]
    fn classifies_support_gem() {
        let data = GameData::new(repo_version_dir());
        let bd = BuildData::load(&data).expect("load");
        // Pick any known support gem and any active gem to assert classification.
        let any_support = bd.skill_gems.values().find(|g| g.is_support);
        let any_active = bd.skill_gems.values().find(|g| !g.is_support);
        if let Some(g) = any_support {
            assert_eq!(bd.is_support_gem(&g.id), Some(true));
        }
        if let Some(g) = any_active {
            assert_eq!(bd.is_support_gem(&g.id), Some(false));
        }
        assert_eq!(bd.is_support_gem("Metadata/Items/Gem/DoesNotExist"), None);
    }
}
