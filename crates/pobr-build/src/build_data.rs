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

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use pobr_core::HighPrecisionRules;
use pobr_core::rules::stat_map_engine::StatMapCatalog;
use pobr_data::catalog::buffs::BuffDef;
use pobr_data::catalog::curse_priority::CursePriorityDef;
use pobr_data::catalog::jewel_radii::JewelRadiiDef;
use pobr_data::catalog::local_mods::LocalModsDef;
use pobr_data::catalog::{
    ArmourBaseStats, BaseItemDef, CostTypeDef, GemEffectDef, GrantedEffectDef, PassiveNodeDef,
    QualityStat, RuntimeConstants, SkillDamageStat, SkillGemDef, SkillLevelDef, SkillStatSetDef,
    StatSetDef, TriggerConfigDef, WeaponBaseStats,
};
use pobr_data::minion::MinionDef;
use pobr_gamedata::ruleset::ConfigCatalog;
use pobr_gamedata::{GameData, LoadError};

/// Class base attributes (PoE2 starting str/dex/int), used to derive [`pobr_core::CharacterBase`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassBaseAttributes {
    pub strength: i32,
    pub dexterity: i32,
    pub intelligence: i32,
}

/// Calc-relevant parameters resolved for an active skill at a given level (all time
/// units are seconds).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ResolvedSkillLevel {
    /// Use time (seconds): attack time for attack skills, otherwise cast time. `None` =
    /// determined by weapon/default.
    pub use_time_s: Option<f64>,
    /// Cooldown (seconds). `None` = no cooldown.
    pub cooldown_s: Option<f64>,
    /// Mana cost (resource = `Mana`). `None` = no mana cost (may still cost Life/ES/etc, see `costs`).
    pub mana_cost: Option<f64>,
    /// The skill's resolved **base damage stat** at this level (e.g.
    /// `spell_minimum_base_fire_damage` → value). Mapped by the calc side into
    /// `<Type>DamageMin/Max` BASE mod injection. Empty = no stat-set damage data.
    pub base_damage: Vec<SkillDamageStat>,
    /// All resource costs (resource name resolved via `CostTypes`, amount already
    /// divided by the divisor). Covers Mana/Life/ES/Rage/Ward etc. and per-second costs.
    /// Empty = no CostTypes data or no cost.
    pub costs: Vec<ResolvedCost>,
    /// Skill damage multiplier (PoB `baseMultiplier`; scales an attack skill's
    /// weapon + added damage). `1.0` = none.
    pub damage_multiplier: f64,
    /// Attack speed multiplier (PoB `attackSpeedMultiplier`, percentage points, can be
    /// negative). Applies to weapon attack rate as `AttackRate × (1 + v/100)` (e.g.
    /// Flicker -50). `None` = none (weapon rate unchanged).
    pub attack_speed_multiplier: Option<f64>,
    /// Skill's base crit chance (PoB `critChance`, percentage points; e.g. Comet
    /// 13.0=13%). An inherent crit source for spells; attack skills fall back to the
    /// weapon base crit chance when `None`. `None` = data missing (old data pack, or the
    /// skill has no critChance row).
    pub crit_chance: Option<f64>,
    /// statSet `baseMods`' inherent **attack speed MORE**
    /// (PoB2 `mod("Speed","MORE",N,ModFlag.Attack)`, percentage points; e.g. Flicker
    /// Strike=285). Injected as an `AttackSpeed` MORE mod in the speed bucket (consumed
    /// only by attack skills). `None` = none.
    pub skill_attack_speed_more: Option<f64>,
    /// Number of stored uses (PoB `storedUses`, e.g. grenade=3). `None` = 0 (no storage).
    /// Injected on the consumer side via a `SkillStoredUsesBase` BASE mod —
    /// `calc_cooldown` uses it to decide whether the cooldown should round up to a
    /// server frame (PoB2 CalcOffence.lua:340: no rounding when stored uses >1).
    pub stored_uses: Option<u32>,
}

/// A granted effect's mappable stats at a given (gem level, quality), split into two
/// segments by source (contract C1): `base` = stat-set per-level row + level-independent
/// constants; `quality` = the quality-stacking segment
/// (`trunc(per_quality_rate × quality)`, matching PoB2 CalcTools.lua:140-145's
/// `buildSkillInstanceStats` up-front quality stacking).
///
/// The split preserves attribution granularity (a PoBR-specific asset, 20-target §1.1):
/// when the quality segment is injected via `mapped_stat_modifiers`, it's tagged with
/// `SourceKind::GemQuality` (id prefix `gem.<effect id>.q<Q>`). Consumers that don't
/// care about attribution can iterate the merged view via [`Self::all`] (equivalent to
/// PoB2's numeric semantics of adding quality into the same stats table up front — same
/// stat's BASE/INC add together in mod_db regardless, so this matches merge-then-map).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EffectStats {
    /// stat-set per-level row + level-independent constants (the existing base segment).
    pub base: Vec<SkillDamageStat>,
    /// Quality-stacking segment (empty when quality = 0 or there's no quality table entry).
    pub quality: Vec<SkillDamageStat>,
}

impl EffectStats {
    /// A merged view chaining base + quality in order (for consumers that don't need attribution).
    pub fn all(&self) -> impl Iterator<Item = &SkillDamageStat> {
        self.base.iter().chain(self.quality.iter())
    }
}

/// A stat snapshot for one **unselected statSet** of a granted effect (the data source
/// for global-only merge, see [`BuildData::unselected_set_stats`]).
#[derive(Debug, Clone, PartialEq)]
pub struct UnselectedSetStats {
    /// statmap per-set override lookup key = the decimal string of vendor's 1-based
    /// export index (following the key convention of
    /// [`pobr_data::catalog::stat_map::SkillStatMapDef::per_stat_set`], fed directly as
    /// `stat_map_engine::map_stat_global_only`'s `set_key`).
    pub set_key: String,
    /// statSet's stable id (for attribution labels / debugging, e.g.
    /// `FlameWallProjectileBuffPlayer`).
    pub set_id: String,
    /// This set's stat table at (gem_level, quality) — same stats already merged
    /// additively, sorted by stat name (matching vendor's `buildSkillInstanceStats`
    /// table semantics, CalcTools.lua:138-200).
    pub stats: Vec<SkillDamageStat>,
}

/// One resolved skill resource cost.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedCost {
    /// Resource id (`Mana` / `Life` / `ES` / `Rage` / `Ward` / `ManaPercent` / `ManaPerMinute` …).
    pub resource: String,
    /// Cost amount (already divided by `CostTypes.Divisor`: a per-minute resource is ÷60 to get the per-second amount).
    pub amount: f64,
    /// Whether this is an ongoing per-second cost.
    pub per_second: bool,
}

/// In-memory indexes projected from [`GameData`] for the orchestrator's calculations.
///
/// The results of each domain's lazy resolution are **pre-resolved into in-memory
/// structures** here (the caller loads once and reuses many times):
/// - `passive_nodes`: `skill id -> node definition` (for [`pobr_tree::collect_allocated_mods`]);
/// - `skill_gems`: `gem id -> gem definition` (for active/support classification);
/// - `class_attributes`: `class name -> base attributes` (for deriving CharacterBase).
#[derive(Debug, Clone)]
pub struct BuildData {
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
    /// Gem quality stat slopes (`overlay/gem_quality_stats.json`), keyed by
    /// `GrantedEffects.Id`. Empty when an old data pack lacks this overlay domain
    /// (quality produces no stats, backward compatible).
    pub gem_quality_stats: HashMap<String, Vec<QualityStat>>,
    /// Gem → granted-effect links (`overlay/gem_effects.json`), keyed by the **primary
    /// effect id** (`granted_effect_id`) — used by meta/composite gem expansion (T5.6)
    /// to look up additional effects forward from a socket group's gem `skill_id` (= the
    /// primary effect id). Empty when an old data pack lacks this overlay domain (no
    /// expansion, backward compatible).
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
    pub constants: RuntimeConstants,
    /// Radius jewel ring tier table (`base/jewel_radii.json`): distance multipliers +
    /// tier label→inner/outer. Consumed by this crate's tree geometry
    /// (`radius_jewel_grant_texts` → pobr-tree's
    /// `compute_radius_jewel_effect_with_radii`), doesn't flow into pobr-core through
    /// `RuntimeConstants`. Falls back to `Default` when data is missing (value-for-value
    /// equal to the JSON).
    pub jewel_radii: JewelRadiiDef,
    /// Local mod allowlist (`overlay/local_mods.json`).
    /// Falls back to the built-in [`LocalModsDef::default`] when a data pack lacks this
    /// overlay file (a mirror that matches the JSON value-for-value, no behavior change).
    pub local_mods: LocalModsDef,
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
}

impl BuildData {
    /// Loads and projects every domain the orchestrator needs from an already-constructed [`GameData`].
    ///
    /// This is the only entry point that triggers [`GameData`] I/O; returns
    /// [`LoadError`] on failure (missing file / parse error). Callers should cache the
    /// return value rather than reloading the same version directory repeatedly.
    pub fn load(data: &GameData) -> Result<Self, LoadError> {
        let passive_nodes = data
            .passive_nodes()?
            .into_iter()
            .map(|node| (node.skill, node))
            .collect();

        // Historical league trees (mirroring PoB2's multi-version TreeData):
        // `base/passive_trees/*.json` is fully preloaded (~0.5-0.8MB per version; a
        // missing directory = empty table, zero cost).
        let mut versioned_passive_nodes: HashMap<String, HashMap<u32, PassiveNodeDef>> =
            HashMap::new();
        for version in data.available_tree_versions() {
            if let Some(nodes) = data.passive_nodes_versioned(&version)? {
                versioned_passive_nodes.insert(
                    version,
                    nodes.into_iter().map(|node| (node.skill, node)).collect(),
                );
            }
        }

        let skill_gems = data
            .skill_gems()?
            .into_iter()
            .map(|gem| (gem.id.clone(), gem))
            .collect();

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

        let granted_effect_levels = data.granted_effect_levels()?.into_iter().collect();

        let skill_stat_sets = data
            .skill_stat_sets()?
            .into_iter()
            .map(|set| (set.effect_id.clone(), set))
            .collect();

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

        // Gem → effect links (overlay domain, T5.1): indexed by primary effect id (the
        // forward lookup key for meta expansion). Missing file = empty table (no expansion, backward compatible).
        let gem_effects = data
            .gem_effects()?
            .map(|def| {
                def.gems
                    .into_iter()
                    .map(|g| (g.granted_effect_id.clone(), g))
                    .collect()
            })
            .unwrap_or_default();

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
        if let Some(spectres) = data.spectres()? {
            for entry in &spectres.minions {
                minions.insert(entry.id.clone(), MinionDef::from_entry(entry));
            }
        }
        if let Some(minion_defs) = data.minions()? {
            // minions overrides spectres on a shared id (they don't actually overlap in
            // practice; the override just guarantees minions-takes-priority semantics).
            for entry in &minion_defs.minions {
                minions.insert(entry.id.clone(), MinionDef::from_entry(entry));
            }
        }
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
                    message: format!("parser 规则编译失败：{e}"),
                })?,
            )),
            None => None,
        };

        Ok(Self {
            passive_nodes,
            versioned_passive_nodes,
            skill_gems,
            class_attributes,
            granted_effects,
            granted_effect_levels,
            skill_stat_sets,
            gem_quality_stats,
            gem_effects,
            cost_types,
            base_items,
            constants,
            jewel_radii,
            local_mods,
            stat_map_catalog,
            buff_definitions,
            curse_priority,
            config_catalog,
            trigger_configs,
            high_precision,
            minions,
            parser_rules,
        })
    }

    /// Constructs an empty [`BuildData`] (no domain data at all; the local mod
    /// allowlist uses the built-in fallback — it's a classification rule rather than
    /// content data, and an empty table would break weapon-local stripping). Used for
    /// tests or the text-only path fallback.
    pub fn empty() -> Self {
        Self {
            passive_nodes: HashMap::new(),
            versioned_passive_nodes: HashMap::new(),
            skill_gems: HashMap::new(),
            class_attributes: HashMap::new(),
            granted_effects: HashMap::new(),
            granted_effect_levels: HashMap::new(),
            skill_stat_sets: HashMap::new(),
            gem_quality_stats: HashMap::new(),
            gem_effects: HashMap::new(),
            cost_types: Vec::new(),
            base_items: HashMap::new(),
            constants: RuntimeConstants::default(),
            jewel_radii: JewelRadiiDef::default(),
            local_mods: LocalModsDef::default(),
            stat_map_catalog: None,
            buff_definitions: Vec::new(),
            curse_priority: None,
            config_catalog: None,
            trigger_configs: HashMap::new(),
            high_precision: HighPrecisionRules::default(),
            minions: HashMap::new(),
            parser_rules: None,
        }
    }

    /// Looks up a minion / spectre definition by minion id; returns `None` for an
    /// unknown id. `minions.json` takes priority, falling back to `spectres.json` on a
    /// miss (spectre key = metadata path).
    pub fn minion_def(&self, id: &str) -> Option<&MinionDef> {
        self.minions.get(id)
    }

    /// Looks up the list of minions a skill summons by its granted effect id (the
    /// merged `minion_list`); returns an empty slice for a non-summon skill / unknown id.
    pub fn effect_minion_list(&self, effect_id: &str) -> &[String] {
        self.granted_effects
            .get(effect_id)
            .map(|e| e.minion_list.as_slice())
            .unwrap_or(&[])
    }

    /// Looks up weapon base stats by base name (`Item.base` → `WeaponBaseStats`);
    /// returns `None` for a non-weapon / unknown base.
    pub fn weapon_base(&self, base_name: &str) -> Option<&WeaponBaseStats> {
        self.base_items
            .get(base_name)
            .and_then(|b| b.weapon.as_ref())
    }

    /// Looks up armour base stats by base name (`Item.base` → `ArmourBaseStats`);
    /// returns `None` for a non-armour / unknown base.
    pub fn armour_base(&self, base_name: &str) -> Option<&ArmourBaseStats> {
        self.base_items
            .get(base_name)
            .and_then(|b| b.armour.as_ref())
    }

    /// Selects a statSet by form (`<Gem statSetIndex>`, 1-based **vendor export
    /// index**); `None` / an index miss (vendor didn't export that index, or an old data
    /// pack has no index sidecar) falls back to the **primary set** — better to default
    /// than to pick the wrong one (conservative; the unselected sets' global-only merge
    /// is left to the caller).
    fn select_stat_set(&self, skill_id: &str, set_index: Option<u32>) -> Option<&StatSetDef> {
        let def = self.skill_stat_sets.get(skill_id)?;
        match set_index {
            Some(n) => def
                .sets
                .iter()
                .find(|s| s.vendor_set_index == Some(n))
                .or_else(|| def.sets.first()),
            None => def.sets.first(),
        }
    }

    /// The selected statSet's statmap **per-set override lookup key** (vendor's 1-based
    /// export index as a decimal string; wired up here): the selection rule matches
    /// [`Self::select_stat_set`] (`set_index` matched against vendor's export index,
    /// falls back to the primary set otherwise). `None` when there's no stat-set data /
    /// the selected set has no vendor index (not exported) — the caller falls back to
    /// the engine's default set `"1"`, equivalent to PoB2's default `statSetIndex=1`
    /// (vendor `SkillsTab.lua:354`).
    pub fn selected_set_key(&self, skill_id: &str, set_index: Option<u32>) -> Option<String> {
        self.select_stat_set(skill_id, set_index)
            .and_then(|s| s.vendor_set_index)
            .map(|i| i.to_string())
    }

    /// The selected statSet's dotIs* flags (booleans hung directly on statSet
    /// `baseMods`, catalog [`pobr_data::catalog::DotFlags`], merged in via the
    /// skill_overrides overlay). Selection rule matches [`Self::select_stat_set`];
    /// returns all-false by default when there's no data (conservatively strips every dotCfg bit).
    pub fn selected_set_dot_flags(
        &self,
        skill_id: &str,
        set_index: Option<u32>,
    ) -> pobr_data::catalog::DotFlags {
        self.select_stat_set(skill_id, set_index)
            .map(|s| s.dot_flags)
            .unwrap_or_default()
    }

    /// The selected statSet's corpse-explosion gate (statSet `baseMods`'
    /// `skill("explodeCorpse", true)`, merged in via the skill_overrides overlay — see
    /// [`pobr_data::catalog::StatSetDef::explode_corpse`]). Selection rule matches
    /// [`Self::select_stat_set`]; `false` when there's no data (no corpse base damage injected).
    pub fn selected_set_explode_corpse(&self, skill_id: &str, set_index: Option<u32>) -> bool {
        self.select_stat_set(skill_id, set_index)
            .map(|s| s.explode_corpse)
            .unwrap_or(false)
    }

    /// Selects the passive node table by the build's `<Spec treeVersion>`: uses the
    /// matching historical tree version (`base/passive_trees/<v>.json` already
    /// extracted) if one exists, otherwise (current default version / not extracted /
    /// not annotated) falls back to the default tree [`Self::passive_nodes`] — PoBR's
    /// counterpart to PoB2's multi-version TreeData. Historical trees only have a
    /// minimal field set (no topology/coordinates), so advanced features like radius
    /// jewel geometry still approximate with the default tree.
    pub fn passive_nodes_for(&self, tree_version: Option<&str>) -> &HashMap<u32, PassiveNodeDef> {
        tree_version
            .and_then(|v| self.versioned_passive_nodes.get(v))
            .unwrap_or(&self.passive_nodes)
    }

    /// Resolves an active skill's parameters at a given level: cast/attack time
    /// (seconds), each resource cost, cooldown (seconds). Uses the default primary
    /// statSet form; use [`Self::resolve_skill_level_with_set`] for a form selection.
    ///
    /// `skill_id` is `GrantedEffects.Id` (PoB's `<Gem skillId>`). Returns `None` if the
    /// skill isn't in the data table or is a support effect (support effects aren't
    /// injected as active skills). Out-of-range levels fall back to the closest existing
    /// level row (the array is sorted ascending by level).
    pub fn resolve_skill_level(
        &self,
        skill_id: &str,
        gem_level: u32,
    ) -> Option<ResolvedSkillLevel> {
        self.resolve_skill_level_with_set(skill_id, gem_level, None)
    }

    /// The statSet form-selecting variant of [`Self::resolve_skill_level`] (T5.5):
    /// `set_index` = PoB's `<Gem statSetIndex>` (1-based vendor export index,
    /// `None`/a miss falls back to the primary set). The selected set determines the
    /// skill's stats (`base_damage`) and damage multiplier (`damage_multiplier`).
    pub fn resolve_skill_level_with_set(
        &self,
        skill_id: &str,
        gem_level: u32,
        set_index: Option<u32>,
    ) -> Option<ResolvedSkillLevel> {
        let effect = self.granted_effects.get(skill_id)?;
        if effect.is_support {
            return None;
        }
        let rows = self.granted_effect_levels.get(skill_id)?;
        if rows.is_empty() {
            return None;
        }
        // Take the highest row with level ≤ gem_level; if every row is above gem_level, take the first.
        let row = rows
            .iter()
            .rfind(|r| r.level <= gem_level)
            .unwrap_or(&rows[0]);

        // Use time: prefer this level's attack time, falling back to the granted effect's cast time (milliseconds → seconds).
        let use_time_ms = row.attack_time_ms.or(effect.cast_time);
        let use_time_s = use_time_ms
            .filter(|&t| t > 0)
            .map(|t| f64::from(t) / 1000.0);
        let cooldown_s = row
            .cooldown_ms
            .filter(|&c| c > 0)
            .map(|c| f64::from(c) / 1000.0);

        // Costs: pair effect.cost_types (resource type indexes) with row.cost_amounts by
        // position, resolved via the CostTypes table into a resource name + amount
        // divided by the divisor (a per-minute resource is ÷60 to get the per-second
        // amount). Falls back to the "index 0 = mana" heuristic when there's no
        // CostTypes data (backward compatible).
        let mut costs = Vec::new();
        for (i, &type_idx) in effect.cost_types.iter().enumerate() {
            let Some(&raw_amount) = row.cost_amounts.get(i) else {
                continue;
            };
            if raw_amount == 0 {
                continue;
            }
            match self.cost_types.get(type_idx as usize) {
                Some(def) if !def.id.is_empty() => costs.push(ResolvedCost {
                    resource: def.id.clone(),
                    amount: f64::from(raw_amount) / f64::from(def.divisor.max(1)),
                    per_second: def.per_minute,
                }),
                _ if type_idx == 0 => costs.push(ResolvedCost {
                    resource: "Mana".into(),
                    amount: f64::from(raw_amount),
                    per_second: false,
                }),
                _ => {}
            }
        }
        // Mana cost (the instantaneous `Mana` resource), read by fill_skill_mechanics's SkillManaCostBase.
        let mana_cost = costs
            .iter()
            .find(|c| c.resource == "Mana" && !c.per_second)
            .map(|c| c.amount);

        // Skill stat (base damage value + damage% scaling): the selected set's per-level
        // row + level-independent constants, for mapping and injection. The quality
        // segment isn't handled here (the main skill's quality is fetched and injected
        // separately by the orchestrator via effect_stats's quality segment, preserving
        // SourceKind::GemQuality attribution granularity), so quality is passed as 0.
        let base_damage = self.effect_stats(skill_id, gem_level, 0, set_index).base;

        // Skill damage multiplier (PoB baseMultiplier): prefers the row from the
        // **selected statSet** (default primary set); falls back to
        // GrantedEffectsPerLevel's base_multiplier when the stat-set is missing (e.g.
        // skills like Flicker whose stat-set is empty) — they're synonymous, PoB carries
        // both tables (grenade's stat-set 7.57 matches per-level, so unaffected).
        let damage_multiplier = self
            .select_stat_set(skill_id, set_index)
            .and_then(|set| {
                set.levels
                    .iter()
                    .rfind(|l| l.gem_level <= gem_level)
                    .or(set.levels.first())
            })
            .map(|l| l.damage_multiplier)
            .or(row.base_multiplier)
            .unwrap_or(1.0);

        // statSet baseMods' inherent attack speed MORE (a PoB2-built-in constant, e.g.
        // Flicker 285). Level-independent; written into the primary set by the overlay
        // merge, so it's absent (None) when a secondary set is selected.
        let skill_attack_speed_more = self
            .select_stat_set(skill_id, set_index)
            .and_then(|set| set.skill_attack_speed_more);

        Some(ResolvedSkillLevel {
            use_time_s,
            cooldown_s,
            mana_cost,
            base_damage,
            costs,
            damage_multiplier,
            attack_speed_multiplier: row.attack_speed_multiplier,
            crit_chance: row.crit_chance,
            skill_attack_speed_more,
            stored_uses: row.stored_uses,
        })
    }

    /// Fetches all mappable stats of a granted effect at a given (gem level, quality,
    /// statSet form) (the final contract-C1 signature after the T1→T5 evolution): the
    /// `base` segment = the **selected set**'s per-level row + level-independent
    /// constants; the `quality` segment = the quality table's slope × quality,
    /// **truncated**.
    ///
    /// `set_index` = PoB's `<Gem statSetIndex>` (1-based vendor export index, see
    /// [`pobr_data::catalog::StatSetDef::vendor_set_index`]); `None` / a miss falls back
    /// to the primary set. **This signature doesn't do global-only merge for unselected
    /// sets** (PoB2's `CalcActiveSkill.lua:124-140` depends on the statmap mod's
    /// GlobalEffect tag for that) — unselected sets' data is fetched via
    /// [`Self::unselected_set_stats`] and filtered/injected by the caller through
    /// `stat_map_engine::map_stat_global_only`.
    ///
    /// Quality semantics match PoB2's `CalcTools.lua:140-145` (`buildSkillInstanceStats`):
    /// `stats[stat] += math.modf(rate × quality)` — `math.modf`'s integer part is
    /// **trunc (toward zero)**, not floor (they differ for negative slopes); the Rust
    /// side uses [`f64::trunc`] to match exactly. The `quality` segment is empty when
    /// quality is 0 or there's no quality table entry.
    ///
    /// Applies equally to active and **support** effects (no `is_support` guard) —
    /// support gems' multiplier / added-damage stats are fetched through here, then
    /// mapped and injected into the supported skill by the statmap data engine
    /// (`pobr-core::rules::stat_map_engine`) (support gems have no quality table entries
    /// at all — PoB2 skips them at export, so the quality segment is naturally empty).
    /// Out-of-range levels fall back to the closest row ≤ the level; the base segment is
    /// empty when there's no stat-set data.
    pub fn effect_stats(
        &self,
        skill_id: &str,
        gem_level: u32,
        quality: u32,
        set_index: Option<u32>,
    ) -> EffectStats {
        let base = self
            .select_stat_set(skill_id, set_index)
            .map(|set| {
                let mut stats = set
                    .levels
                    .iter()
                    .rfind(|l| l.gem_level <= gem_level)
                    .or(set.levels.first())
                    .map(|level| level.stats.clone())
                    .unwrap_or_default();
                stats.extend(set.constant_stats.iter().cloned());
                // Implicit stats: for statSet `stats` entries with no per-level value,
                // vendor folds them in with value 1 (CalcTools.lua:152
                // `statSetLevel[index] or 1`, e.g. Garukhan's Resolve's
                // `attacks_roll_crits_twice` → statmap BifurcateCrit).
                stats.extend(set.implicit_stats.iter().map(|stat| SkillDamageStat {
                    stat: stat.clone(),
                    value: 1.0,
                }));
                stats
            })
            .unwrap_or_default();

        let quality_stats = if quality > 0 {
            self.gem_quality_stats
                .get(skill_id)
                .map(|rows| {
                    rows.iter()
                        // alt quality stats only apply to GemlingQuality builds, added by
                        // the consumer under that flag via [`Self::alt_quality_stats`].
                        .filter(|q| !q.alt)
                        .map(|q| SkillDamageStat {
                            stat: q.stat.clone(),
                            // trunc (toward zero), matching math.modf's integer part.
                            value: (q.per_quality_rate * f64::from(quality)).trunc(),
                        })
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        EffectStats {
            base,
            quality: quality_stats,
        }
    }

    /// The trunc value of an alt quality stat (vendor `altQualityStats`) at a given quality.
    ///
    /// Only folded into the skill's stat set when the build carries the GemlingQuality
    /// flag (the Gemling ascendancy's "Gem Quality grants Socketed Skills an additional
    /// effect", PoB2 `CalcTools.lua:147-152`'s `includeAltQualityStats`) — the consumer
    /// checks the flag and calls this explicitly ([`Self::effect_stats`] never includes
    /// alt rows).
    // ponytail: currently only Spirit reservation efficiency consumes this channel
    // (pinned by gemling parity); wire it up on the offence/statmap side once a fixture
    // forces the deviation (same accessor, zero data changes).
    pub fn alt_quality_stats(&self, skill_id: &str, quality: u32) -> Vec<SkillDamageStat> {
        if quality == 0 {
            return Vec::new();
        }
        self.gem_quality_stats
            .get(skill_id)
            .map(|rows| {
                rows.iter()
                    .filter(|q| q.alt)
                    .map(|q| SkillDamageStat {
                        stat: q.stat.clone(),
                        value: (q.per_quality_rate * f64::from(quality)).trunc(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Gets the stat snapshot of a granted effect's **unselected sets** at a given (gem
    /// level, quality, statSet form) (the data source for global-only merge, matching
    /// PoB2's `CalcActiveSkill.lua:124-140`: every statSet other than the selected one
    /// participates in the merge with `onlyGlobals=true`).
    ///
    /// - The selected-set determination follows the same rule as [`Self::effect_stats`]
    ///   ([`Self::select_stat_set`]: `set_index` matched against vendor's index,
    ///   falling back to the primary set), and returns every other set;
    /// - Sets vendor didn't export (`vendor_set_index = None`, skipped by template
    ///   curation, e.g. IceNovaPlayerOnFrostbolt) aren't in PoB2's
    ///   `grantedEffect.statSets` list and **don't participate** in global merge, so
    ///   they're excluded here too;
    /// - Each set's stats are built following vendor's `buildSkillInstanceStats`
    ///   (CalcTools.lua:138-200) **table semantics**: the quality segment (truncated;
    ///   the quality table is shared per effect and stacked first for every set) + the
    ///   per-level row (the highest row ≤ gem_level, falling back to the first row when
    ///   out of range) + level-independent constants, with **same-stat values added
    ///   together**, keys sorted lexically (deterministic). This differs from
    ///   [`Self::effect_stats`]'s segment-chained view — statmap entries may carry a
    ///   `value` override (non-linear), so global-only lookup must take the already-merged
    ///   single value as input to match vendor's single per-stat merge.
    ///
    /// Returns empty when there's no additional set / the effect is unknown. Note: an
    /// unselected set's `baseMods` (PoBR's distilled field `skill_attack_speed_more`,
    /// which has no GlobalEffect tag) is **never** injected, matching vendor's `:131-135`
    /// semantics of only accepting global baseMods — so this snapshot never carries that field.
    pub fn unselected_set_stats(
        &self,
        skill_id: &str,
        gem_level: u32,
        quality: u32,
        set_index: Option<u32>,
    ) -> Vec<UnselectedSetStats> {
        let Some(def) = self.skill_stat_sets.get(skill_id) else {
            return Vec::new();
        };
        let selected_id = self
            .select_stat_set(skill_id, set_index)
            .map(|s| s.set_id.as_str());
        let mut out = Vec::new();
        for set in &def.sets {
            // Not exported by vendor (no index) → excluded; the selected set itself is skipped.
            let Some(idx) = set.vendor_set_index else {
                continue;
            };
            if Some(set.set_id.as_str()) == selected_id {
                continue;
            }
            // buildSkillInstanceStats table semantics: same-stat values are added together (BTreeMap gives deterministic lexical order).
            let mut acc: BTreeMap<String, f64> = BTreeMap::new();
            if quality > 0
                && let Some(rows) = self.gem_quality_stats.get(skill_id)
            {
                for q in rows.iter().filter(|q| !q.alt) {
                    // trunc (toward zero), same semantics as effect_stats's quality segment.
                    *acc.entry(q.stat.clone()).or_default() +=
                        (q.per_quality_rate * f64::from(quality)).trunc();
                }
            }
            if let Some(level) = set
                .levels
                .iter()
                .rfind(|l| l.gem_level <= gem_level)
                .or(set.levels.first())
            {
                for s in &level.stats {
                    *acc.entry(s.stat.clone()).or_default() += s.value;
                }
            }
            for s in &set.constant_stats {
                *acc.entry(s.stat.clone()).or_default() += s.value;
            }
            out.push(UnselectedSetStats {
                set_key: idx.to_string(),
                set_id: set.set_id.clone(),
                stats: acc
                    .into_iter()
                    .map(|(stat, value)| SkillDamageStat { stat, value })
                    .collect(),
            });
        }
        out
    }

    /// Looks up a class's base attributes (by English canonical name); returns `None` for an unknown class.
    pub fn class_attributes(&self, class_name: &str) -> Option<ClassBaseAttributes> {
        self.class_attributes.get(class_name).copied()
    }

    /// Determines whether a gem id is a support gem; returns `None` for an unknown gem (the caller falls back as needed).
    pub fn is_support_gem(&self, gem_id: &str) -> Option<bool> {
        self.skill_gems.get(gem_id).map(|gem| gem.is_support)
    }

    /// Determines whether a granted effect is an **aura** (`skill_types` includes
    /// `Aura`). Auras apply an ongoing buff to self (and present allies) — their
    /// per-level stats are fetched via [`Self::effect_stats`] and injected by the
    /// defense side. Returns `false` for an unknown effect (conservative, doesn't
    /// invent aura semantics). Curses (which apply to enemies) don't have `Aura` in
    /// `skill_types`, so they're never mistaken for self-buffs.
    pub fn is_aura(&self, skill_id: &str) -> bool {
        self.granted_effects
            .get(skill_id)
            .map(|e| e.skill_types.iter().any(|t| t == "Aura"))
            .unwrap_or(false)
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
                constant_stats: ds(constant_stats),
                skill_attack_speed_more: None,
                dot_flags: Default::default(),
                explode_corpse: false,
                implicit_stats: Vec::new(),
                levels: vec![SkillStatSetLevel {
                    gem_level: 1,
                    damage_multiplier: 1.0,
                    stats: ds(level_stats),
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
