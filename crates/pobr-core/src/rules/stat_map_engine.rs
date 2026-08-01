//! SkillStatMap data engine: translates `overlay/skill_stat_map.json`
//! ([`pobr_data::catalog::stat_map`], a deterministic extraction of vendor
//! `Data/SkillStatMap.lua`'s 954 global entries plus per-statSet overrides)
//! into PoBR [`Modifier`] injection items, replacing the 751-line suffix
//! heuristic in `pobr-build::skill_stat_map`.
//!
//! Pure functions + zero I/O (injection style): the catalog is loaded by
//! pobr-gamedata and injected by pobr-build; this layer only does table
//! lookup, the merge formula, and name/tag translation.
//!
//! ## Merge formula (matches vendor `Modules/CalcActiveSkill.lua:112` line for line)
//!
//! ```text
//! injected value = (entry.value or stat value) × (entry.mult or 1) × scalar / (entry.div or 1) + (entry.base or 0)
//! ```
//!
//! A group element (a nested mod list with no name) uses group-level
//! parameters instead of entry-level ones (CalcActiveSkill.lua:117). `scalar`
//! (`checkForScalarMultiplier`, :53-66, which needs a mod_db lookup of
//! `Multiplier:<var>`) is fixed at 1.0; **any entry that requires a scalar is
//! reported wholesale as [`MappedOutcome::Unsupported`]** (counted, never
//! miscomputed).
//!
//! ## Support boundary (first batch: skip rather than miscompute)
//!
//! - **tag**: no tag / `Condition` / `Multiplier` / `PerStat` (mapped onto
//!   PoBR's existing [`ModTag`] system); other tag kinds (GlobalEffect /
//!   DistanceRamp / SkillType / actor-related...) make the whole entry
//!   Unsupported -- matching legacy's "skip conservatively" behaviour so the
//!   two engines stay comparable.
//! - **ModName translation layer**: a Rust constant table from PoB2 names to
//!   PoBR names (framework-level; the rationale is that names track game
//!   mechanics, not versions -- see the architecture owner's ruling). The
//!   first pass was reverse-derived from the existing mapping in
//!   `pobr-build::skill_stat_map`; unknown names are reported as
//!   [`UnsupportedReason::UnknownModName`] and filled in as the dual-run diff
//!   turns them up.
//! - **skill_data** (vendor `skill(key, …)` constructor): base damage keys
//!   (`FireMin`/`PhysicalMax` etc.) translate to `<Type>DamageMin/Max` BASE
//!   modifiers (PoBR has no skillData table, so base damage flows through the
//!   modifier pipeline instead, matching legacy behaviour); `duration`
//!   produces [`MappedItem::SkillData`] (callers may ignore it until a
//!   consumer is wired up); other keys are counted as Unsupported.
//! - **flag constructor** (vendor `flag(name)`, a skill behaviour switch such
//!   as `projectile`): only names with a ModDb flag consumer are translated;
//!   the rest are Unsupported (see [`is_consumable_flag`]).

use std::collections::BTreeMap;

use pobr_data::catalog::stat_map::{SkillStatMapDef, StatMapEntry, StatMapMod, StatMapValue};
use pobr_data::modifier::{KeywordFlags, ModFlags, ModType};
use pobr_data::skill::SkillTypes;

use crate::modifier::{ModTag, Modifier};

/// Fixed scalar value (the vendor `checkForScalarMultiplier` mod_db lookup
/// isn't wired up yet; entries with a scalar field are Unsupported wholesale.
/// This constant only exists to keep the formula's shape complete).
const SCALAR_FIXED: f64 = 1.0;

/// statmap lookup catalog (a combined view of global entries and
/// per-statSet overrides).
///
/// Built from the [`SkillStatMapDef`] deserialized from
/// `overlay/skill_stat_map.json`; lookup semantics: a per-set hit wins, a
/// miss falls back to global (equivalent to the `__index` chain on the
/// vendor `Data.lua:835-847` statMap metatable).
#[derive(Debug, Clone)]
pub struct StatMapCatalog {
    def: SkillStatMapDef,
}

impl StatMapCatalog {
    /// Constructs from a deserialized overlay document.
    pub fn new(def: SkillStatMapDef) -> Self {
        Self { def }
    }

    /// Looks up an entry: when `set_key` is given, checks
    /// `per_stat_set[effect_id][set_key]` first, falling back to `global` on
    /// a miss.
    ///
    /// `set_key = None` means the caller hasn't made a statSet selection, so
    /// this uses the override table for the **default set "1"**: PoB2's
    /// statSetIndex always defaults to 1 (vendor `SkillsTab.lua:354`
    /// `gemInstance.statSet = { index = tonumber(child.attrib.statSetIndex) or 1 }`,
    /// `CalcActiveSkill.lua:171` `statSet = …statSets[activeEffect.statSet.index]`),
    /// and a miss on the selected set's statMap falls back to the global
    /// table through the metatable chain (the `Data.lua` statMap `__index`).
    /// Overrides on sets other than "1" only get hit with an explicit
    /// `set_key` (statSetIndex selection wiring is planned for the T5
    /// multi-statSet model).
    fn lookup(&self, effect_id: &str, set_key: Option<&str>, stat: &str) -> Option<&StatMapEntry> {
        let key = set_key.unwrap_or("1");
        if let Some(entry) = self
            .def
            .per_stat_set
            .get(effect_id)
            .and_then(|sets| sets.get(key))
            .and_then(|map| map.get(stat))
        {
            return Some(entry);
        }
        self.def.global.get(stat)
    }

    /// Number of entries in the global section (used by dual-run reporting).
    pub fn global_len(&self) -> usize {
        self.def.global.len()
    }

    /// Iterates the stat ids in the global section.
    pub fn global_stats(&self) -> impl Iterator<Item = &str> {
        self.def.global.keys().map(String::as_str)
    }

    /// Whether a per-statSet override exists for (effect, set, stat). Used by
    /// dual-run L1: only stats with an override need a separate diff line per
    /// effect context; other stats dedupe on the global line.
    pub fn has_per_set_override(&self, effect_id: &str, set_key: &str, stat: &str) -> bool {
        self.def
            .per_stat_set
            .get(effect_id)
            .and_then(|sets| sets.get(set_key))
            .is_some_and(|map| map.contains_key(stat))
    }

    /// Iterates the per-statSet section: `(effect_id, set_key, stat, entry)`.
    pub fn per_set_entries(&self) -> impl Iterator<Item = (&str, &str, &str, &StatMapEntry)> {
        self.def.per_stat_set.iter().flat_map(|(effect, sets)| {
            sets.iter().flat_map(move |(set_key, map)| {
                map.iter().map(move |(stat, entry)| {
                    (effect.as_str(), set_key.as_str(), stat.as_str(), entry)
                })
            })
        })
    }

    /// Iterates entries in the global section (used for oracle sampling).
    pub fn global_entries(&self) -> impl Iterator<Item = (&str, &StatMapEntry)> {
        self.def.global.iter().map(|(k, v)| (k.as_str(), v))
    }
}

impl From<SkillStatMapDef> for StatMapCatalog {
    fn from(def: SkillStatMapDef) -> Self {
        Self::new(def)
    }
}

/// A single translation output: either an injected modifier or a skill data
/// key/value pair.
#[derive(Debug, Clone, PartialEq)]
pub enum MappedItem {
    /// A PoBR modifier ready to go straight into ModDb (name translated, tag
    /// mapped; boxed to offset the size difference with the SkillData
    /// variant).
    Modifier(Box<Modifier>),
    /// A skill data key/value (vendor `skill(key, …)`; e.g. `duration`, in
    /// seconds). Consumers wire this up as needed; callers with no consumer
    /// can just ignore it (it doesn't participate in calculation, so
    /// ignoring it can't cause a miscalculation).
    SkillData {
        /// The vendor skillData key name, verbatim.
        key: String,
        /// The value produced by the merge formula.
        value: f64,
    },
}

/// Classification of why an entry is unsupported (the aggregation dimension
/// for dual-run L1 reporting).
#[derive(Debug, Clone, PartialEq)]
pub enum UnsupportedReason {
    /// An entry with a distorted extraction (a vendor function value or
    /// malformed construct, `_unextractable: true`).
    Unextractable,
    /// The entry needs a scalar (scalar is fixed at 1.0, so the whole entry
    /// is skipped to avoid miscomputing).
    ScalarMultiplier,
    /// The PoB2 ModName isn't in the translation table (includes behaviour
    /// switch names from flag constructors).
    UnknownModName(String),
    /// The mod constructor is missing a type (a vendor typo entry, kept
    /// faithfully as extracted).
    MissingModType,
    /// An aggregation type outside the first batch (e.g. a `LIST` that isn't
    /// skill_data).
    UnsupportedModType(String),
    /// A tag kind outside the first batch (GlobalEffect / DistanceRamp /
    /// actor-related...).
    UnsupportedTag(String),
    /// A ModFlag combination that can't be translated to PoBR ModName
    /// semantics.
    UnsupportedFlags(String),
    /// A KeywordFlag whose semantics can't be conservatively dropped (only
    /// the base-damage family is allowed to drop it, matching legacy).
    UnsupportedKeywordFlags(String),
    /// A skill_data key outside the first-batch whitelist.
    UnsupportedSkillDataKey(String),
    /// The entry carries a `skillFlag` (consumed by the PoB2 statSet flags
    /// path, not by the merge formula).
    SkillFlag(String),
    /// An unknown element kind (outside the extractor's set of conventions).
    UnsupportedKind(String),
}

impl UnsupportedReason {
    /// A stable classification tag (the aggregation key for dual-run reports).
    pub fn category(&self) -> &'static str {
        match self {
            Self::Unextractable => "unextractable",
            Self::ScalarMultiplier => "scalar",
            Self::UnknownModName(_) => "unknown_mod_name",
            Self::MissingModType => "missing_mod_type",
            Self::UnsupportedModType(_) => "mod_type",
            Self::UnsupportedTag(_) => "tag",
            Self::UnsupportedFlags(_) => "flags",
            Self::UnsupportedKeywordFlags(_) => "keyword_flags",
            Self::UnsupportedSkillDataKey(_) => "skill_data_key",
            Self::SkillFlag(_) => "skill_flag",
            Self::UnsupportedKind(_) => "kind",
        }
    }
}

/// The result of `map_stat` (contract C3).
#[derive(Debug, Clone, PartialEq)]
pub enum MappedOutcome {
    /// The entry was found and every element could be translated -- yields
    /// the list of injection items.
    Mapped(Vec<MappedItem>),
    /// The entry was found but has semantics outside the first batch, so the
    /// **whole entry** is skipped (skip rather than miscompute).
    Unsupported(UnsupportedReason),
    /// The catalog has no entry for this stat (miss on both global and
    /// per-set).
    Unknown,
}

/// Translates one skill stat through the statmap data into PoBR injection
/// items.
///
/// - `effect_id` + `set_key`: locate a per-statSet override (`set_key` is the
///   decimal string of the vendor `statSets` 1-based index); `set_key = None`
///   or a miss falls back to global;
/// - `stat_value`: the stat's runtime value (after per-level stat + quality
///   are applied);
/// - see the module doc for the merge formula and support boundary.
pub fn map_stat(
    catalog: &StatMapCatalog,
    effect_id: &str,
    set_key: Option<&str>,
    stat: &str,
    stat_value: f64,
) -> MappedOutcome {
    let Some(entry) = catalog.lookup(effect_id, set_key, stat) else {
        return MappedOutcome::Unknown;
    };
    map_entry(entry, stat_value)
}

/// Entry-level translation (pure merge + translation after lookup; shared by
/// `map_stat` and unit tests).
fn map_entry(entry: &StatMapEntry, stat_value: f64) -> MappedOutcome {
    if entry.unextractable {
        return MappedOutcome::Unsupported(UnsupportedReason::Unextractable);
    }
    if let Some(flag) = &entry.skill_flag {
        return MappedOutcome::Unsupported(UnsupportedReason::SkillFlag(flag.clone()));
    }
    let entry_params = MergeParams {
        div: entry.div,
        mult: entry.mult,
        base: entry.base,
        value: entry.value,
    };
    let mut items = Vec::new();
    for element in &entry.mods {
        if let Err(reason) = collect_element(element, &entry_params, stat_value, &mut items) {
            // Any unsupported element skips the whole entry -- a half-injected
            // entry would break PoB2's grouped semantics.
            return MappedOutcome::Unsupported(reason);
        }
    }
    MappedOutcome::Mapped(items)
}

//  global-only merge for an unselected statSet

/// Equivalent to vendor `isGlobalEffect` (`Modules/CalcActiveSkill.lua:68-80`):
/// a modOrGroup counts as global if **any** of its mods carries a
/// `type == "GlobalEffect"` tag.
///
/// Vendor shape: `local modList = modOrGroup.name and { modOrGroup } or modOrGroup`
/// -- having a name means a single mod (check its own tags), no name means a
/// group (check each member's tags). The extraction layer records the two
/// shapes as `kind == "mod"/"flag"/"skill_data"` (tags attached directly) and
/// `kind == "group"` (members live in [`StatMapMod::mods`]); vendor never
/// nests groups, so this recursion is a faithful, conservative
/// generalization (a hit at any level counts as global).
///
/// Judged at the granularity of the **whole modOrGroup** (vendor `:103`
/// judges each map element once): if any group member carries the tag, the
/// entire group is injected as global (including members without the tag) --
/// there's no member-level split.
pub fn is_global_effect(element: &StatMapMod) -> bool {
    if element.kind == "group" {
        return element.mods.iter().any(is_global_effect);
    }
    element
        .tags
        .iter()
        .any(|tag| matches!(tag.get("type"), Some(StatMapValue::Text(t)) if t == "GlobalEffect"))
}

/// Entry-level global bookkeeping probe: whether the statmap entry for
/// (effect, set, stat) contains **any** global modOrGroup.
///
/// Corresponds to the `selectedGlobalStats[stat] = true` bookkeeping vendor
/// does when merging the selected set (`CalcActiveSkill.lua:104-106`:
/// `if isGlobal and not onlyGlobals`) -- callers call this function for every
/// stat of the **selected set** to build the set, and the global-only merge
/// for the unselected set skips stats in that set wholesale (`:107`
/// `not (onlyGlobals and selectedGlobalStats[stat])`; `selectedGlobalStats`
/// doesn't change again during the onlyGlobals phase, so this stat-level skip
/// is equivalent to vendor's element-level condition). A missing entry or an
/// `_unextractable` one (empty mods) is treated as having no global mods.
pub fn stat_has_global_mods(
    catalog: &StatMapCatalog,
    effect_id: &str,
    set_key: Option<&str>,
    stat: &str,
) -> bool {
    catalog
        .lookup(effect_id, set_key, stat)
        .is_some_and(|entry| entry.mods.iter().any(is_global_effect))
}

/// **Global-only** mapping for an unselected statSet (vendor
/// `mergeStatSet(set, onlyGlobals=true)`, `CalcActiveSkill.lua:92-141`'s call
/// at `:124-129` plus the injection condition at `:103-107`): only the
/// modOrGroup elements that hit [`is_global_effect`] go through the merge
/// formula and translation; non-global elements are **silently skipped**
/// (same as vendor -- a local mod on an unselected set was never meant to be
/// injected, so this isn't an Unsupported case). If filtering leaves no
/// global elements, the result is `Mapped(empty)`.
///
/// `set_key` should be the vendor index of the **unselected** set (a per-set
/// override is looked up through that set's own statMap chain, falling back
/// to global on a miss -- same semantics as the vendor `set.statMap`
/// metatable `__index`). Stats already accounted for as global on the
/// selected set are skipped by the caller (see [`stat_has_global_mods`]).
///
/// **First-batch boundary**: the `GlobalEffect` tag itself is still outside
/// the tag translation boundary (the buff domain arrives with buff_pass) --
/// so right now every global element gets
/// reported wholesale via [`UnsupportedReason::UnsupportedTag`] and injects
/// nothing (skip rather than miscompute). Once the GlobalEffect tag is wired
/// up, this path will **automatically** start producing injection items with
/// no further changes to this function.
pub fn map_stat_global_only(
    catalog: &StatMapCatalog,
    effect_id: &str,
    set_key: Option<&str>,
    stat: &str,
    stat_value: f64,
) -> MappedOutcome {
    let Some(entry) = catalog.lookup(effect_id, set_key, stat) else {
        return MappedOutcome::Unknown;
    };
    if entry.unextractable {
        // Distorted extraction: content unknown (mods is empty), reported as
        // Unsupported for visibility (injects nothing either way).
        return MappedOutcome::Unsupported(UnsupportedReason::Unextractable);
    }
    // Note: when an entry carries `skill_flag`, that flag isn't a modOrGroup
    // (it's consumed by the vendor statSet flags path), so from the
    // global-only viewpoint there's no global element and it naturally falls
    // into `Mapped(empty)` -- matching vendor, where flags on an unselected
    // set don't apply (flags only take effect on the selected set).
    let entry_params = MergeParams {
        div: entry.div,
        mult: entry.mult,
        base: entry.base,
        value: entry.value,
    };
    let mut items = Vec::new();
    for element in entry.mods.iter().filter(|e| is_global_effect(e)) {
        if let Err(reason) = collect_element(element, &entry_params, stat_value, &mut items) {
            // Same rule as map_entry: any retained element that can't be
            // translated skips the whole entry (grouped semantics).
            return MappedOutcome::Unsupported(reason);
        }
    }
    MappedOutcome::Mapped(items)
}

//  curse domain (GlobalEffect effectType=Curse) enemy-side mapping

/// Whether an element is a **curse buff payload** (vendor `GlobalEffect` tag
/// with `effectType == "Curse"` -- `CalcActiveSkill.lua:976-1041` moves
/// matching elements out of skillModList into `buff.modList` (buff.type =
/// "Curse"), and `CalcPerform.lua:2286-2316` writes them to enemyDB at
/// :2969-2984 after the CurseEffect multiplier zone `ScaleAddList`).
/// A hit on any group member counts for the whole group (the same
/// conservative generalization as [`is_global_effect`]).
fn is_curse_effect(element: &StatMapMod) -> bool {
    if element.kind == "group" {
        return element.mods.iter().any(is_curse_effect);
    }
    element.tags.iter().any(|tag| {
        matches!(tag.get("type"), Some(StatMapValue::Text(t)) if t == "GlobalEffect")
            && matches!(tag.get("effectType"), Some(StatMapValue::Text(t)) if t == "Curse")
    })
}

/// Translates one curse skill stat through the statmap data into
/// **enemy-side** PoBR injection items (the BuffSpec.mods read channel,
/// consumed by the buff_pass curse path).
///
/// Differences from [`map_stat`]:
/// - Only elements hitting [`is_curse_effect`] are kept (non-curse elements
///   are technique-local mods that go through the main skill injection
///   channel, so they're **silently skipped** here -- matching vendor: a mod
///   without GlobalEffect stays in skillModList). If filtering leaves no
///   curse elements, the result is `Mapped(empty)` (the stat isn't a curse
///   payload).
/// - ModName goes through the enemy-side translation table
///   [`translate_curse_mod_name`] (enemy db aggregate names pass through
///   vendor's enemyDB names directly; `ElementalResist` expands to the three
///   elements -- pobr's enemy-side resistance aggregation only reads
///   `<Type>Resist`, and vendor's `enemyDB:Sum(.. type.."Resist", "ElementalResist")`
///   collects both names, so the expansion is equivalent). Unknown names
///   (pobr has no enemy-side consumer yet, e.g. `TemporalChainsActionSpeed` /
///   `FreezeBuildup`) make the whole entry
///   [`UnsupportedReason::UnknownModName`] (skip rather than miscompute; the
///   caller records the visibility report in Compare).
/// - The `GlobalEffect` tag itself is stripped (routing metadata, not part
///   of the match); a tag with keys outside the convention (`effectCond` /
///   `modCond` / `effectStackVar`... carry extra gating semantics) makes the
///   whole entry Unsupported. Other tags are translated directly via
///   [`translate_tag`] -- a curse mod lands in the enemy db, so a
///   `Condition`'s var is the enemy's own state (e.g. Enfeeble's `Unique`,
///   which is the same name and meaning as the `Unique` cfg condition set by
///   the orchestration layer for boss tiers -- **no** Enemy prefix is added).
/// - flags / keyword_flags must be empty in the first batch (all curse
///   payload data has no flags; a non-empty one means scope semantics that
///   aren't modeled yet, so it's Unsupported).
pub fn map_curse_stat(
    catalog: &StatMapCatalog,
    effect_id: &str,
    set_key: Option<&str>,
    stat: &str,
    stat_value: f64,
) -> MappedOutcome {
    let Some(entry) = catalog.lookup(effect_id, set_key, stat) else {
        return MappedOutcome::Unknown;
    };
    if entry.unextractable {
        return MappedOutcome::Unsupported(UnsupportedReason::Unextractable);
    }
    let entry_params = MergeParams {
        div: entry.div,
        mult: entry.mult,
        base: entry.base,
        value: entry.value,
    };
    let mut items = Vec::new();
    for element in entry.mods.iter().filter(|e| is_curse_effect(e)) {
        if let Err(reason) = collect_curse_element(element, &entry_params, stat_value, &mut items) {
            // Same rule as map_entry: any curse element that can't be
            // translated skips the whole entry (grouped semantics).
            return MappedOutcome::Unsupported(reason);
        }
    }
    MappedOutcome::Mapped(items)
}

/// Whether this stat carries a **curse buff payload** (any element hits
/// [`is_curse_effect`]).
///
/// Lets the orchestration layer mirror vendor's curse registration
/// precondition: vendor only moves mods carrying the `GlobalEffect` tag into
/// `activeSkill.buffList` (`CalcActiveSkill.lua:976-1041`), and curse table
/// entries are built only from buffList (`CalcPerform.lua:2286-2316`) -- so a
/// curse skill whose statMap data has **no** `GlobalEffect effectType=Curse`
/// entry at all (e.g. Repulsion `CurseOfRepulsionPlayer`, whose per-set
/// statMap is entirely empty) has an always-empty buffList: it never
/// registers as a curse, never occupies a curse slot, and never counts toward
/// `Multiplier:CurseOnEnemy` (:2969 `#curseSlots`). Counterexample: Freezing
/// Mark, where vendor data deliberately supplies a `Dummy INC`
/// (GlobalEffect Curse) placeholder payload so it takes a slot
/// (`act_int.lua:8645`).
///
/// Differs from [`map_curse_stat`]: this only checks **existence**, not
/// translatability -- payloads outside the allow-list (Temporal Chains
/// `TemporalChainsActionSpeed` / `Dummy`) still count as a curse payload
/// (vendor counts them toward the slot too). An `unextractable` entry has
/// empty mods, so it's treated as having no payload (the extractor can
/// extract every `mod()` construct in curse statMap data; the current data
/// has no case that fails this).
pub fn has_curse_payload(
    catalog: &StatMapCatalog,
    effect_id: &str,
    set_key: Option<&str>,
    stat: &str,
) -> bool {
    catalog
        .lookup(effect_id, set_key, stat)
        .is_some_and(|entry| entry.mods.iter().any(is_curse_effect))
}

/// (Backlog #7-1) Reads the **skill-local effect multiplier zone** for curse
/// skills: if a stat maps to a `CurseEffect` INC/MORE **without a
/// GlobalEffect tag** (a skill-local mod that stays in skillModList -- vendor
/// reads the curse multiplier zone at CalcPerform.lua:2423/:2427 via
/// `skillModList:Sum/More(skillCfg, "CurseEffect")`), returns its
/// `(inc increment, more factor)`. Typical sources: the curse gem's own
/// quality `curse_effect_+%` (EW 0.5/q), the Heightened Curse support in the
/// same group (constantStats +25), and the Atziri's Allure lineage
/// (`support_atziri_curse_effect_+%_final` MORE -20).
///
/// Conservative rule: only bare `CurseEffect` with `kind=="mod"`, no tag, and
/// no flag counts (Mark-gated variants carry a SkillType tag and don't count
/// -- unblocked once the mark domain gets its own modeling). Every other stat
/// or shape returns `(0.0, 1.0)` (zero contribution, never a miscalculation).
pub fn curse_local_effect(
    catalog: &StatMapCatalog,
    effect_id: &str,
    set_key: Option<&str>,
    stat: &str,
    stat_value: f64,
) -> (f64, f64) {
    let (mut inc, mut more) = (0.0, 1.0);
    let Some(entry) = catalog.lookup(effect_id, set_key, stat) else {
        return (inc, more);
    };
    if entry.unextractable {
        return (inc, more);
    }
    let params = MergeParams {
        div: entry.div,
        mult: entry.mult,
        base: entry.base,
        value: entry.value,
    };
    for element in &entry.mods {
        if element.kind != "mod"
            || element.name.as_deref() != Some("CurseEffect")
            || !element.tags.is_empty()
            || !element.flags.is_empty()
            || !element.keyword_flags.is_empty()
            || element.scalar.is_some()
        {
            continue;
        }
        let merged = params.merge(stat_value);
        match element.mod_type.as_deref() {
            Some("INC") => inc += merged,
            Some("MORE") => more *= 1.0 + merged / 100.0,
            _ => {}
        }
    }
    (inc, more)
}

/// Whether an element is an **exposure-infliction capability** payload (used
/// for host detection, not for reading a value): vendor
/// `flag("InflictExposure", …)` (SkillStatMap.lua:1692-1715, in its
/// on_shock / on_cold_crit / on_ignite / on_hit forms) or
/// `<El>ExposureChance BASE` (:1689-1690 / :1704-1707). Corresponds to the
/// Config exposure-source host test in CalcPerform.lua:3196-3200
/// `getSkillExposureEffect`: `HasMod("BASE", cfg, el.."ExposureChance") or
/// HasMod("FLAG", "InflictExposure")`. PoBR approximates by ignoring gating
/// tags on the flag (on-Ignited and similar conditions -- vendor's `HasMod`
/// with a cfg is likewise a loose existence check that ignores conditions).
fn is_exposure_inflict(element: &StatMapMod) -> bool {
    if element.kind == "group" {
        return element.mods.iter().any(is_exposure_inflict);
    }
    element
        .name
        .as_deref()
        .is_some_and(|n| n == "InflictExposure" || n.ends_with("ExposureChance"))
}

/// Whether a stat carries an exposure-infliction payload
/// ([`is_exposure_inflict`]; an **existence** check with the same rule as
/// [`has_curse_payload`] -- doesn't require it to be on the allow-list).
/// Lets the orchestration layer detect an exposure host: only when the
/// host's exposure capability comes from a support (Fire Exposure
/// `inflict_exposure_for_x_ms_on_ignite`) does an exposure-effect support in
/// the same group (Potent Exposure) get its `<El>ExposureEffect` injected
/// globally.
pub fn has_exposure_inflict_payload(
    catalog: &StatMapCatalog,
    effect_id: &str,
    set_key: Option<&str>,
    stat: &str,
) -> bool {
    catalog
        .lookup(effect_id, set_key, stat)
        .is_some_and(|entry| entry.mods.iter().any(is_exposure_inflict))
}

/// Enemy-side ModName translation table (curse domain, PoB2 enemyDB name →
/// PoBR enemy db aggregate name).
///
/// The allow-list is checked one by one against pobr's current enemy-side
/// consumers (skip rather than miscompute):
/// - `<Type>Resist` BASE: the resistance-mitigation section of
///   `offence::enemy_damage_multiplier` (Despair's `ChaosResist`);
///   `ElementalResist` (Elemental Weakness) expands to the three fire/cold/
///   lightning lines (the consumer only reads `<Type>Resist`, which is
///   equivalent to vendor collecting both names).
/// - `Damage` INC/MORE: the enemy's outgoing-damage multiplier zone
///   (Enfeeble), consumed by `ehp::assemble_enemy_damage`
///   (CalcDefence.lua:2133 enemyDamageMult).
/// - `SelfCritMultiplier` BASE: bonus to crits taken by the enemy
///   (Sniper's Mark), consumed by the enemy-side section of `crit.rs`
///   (CalcOffence.lua:3814-3825).
/// - `BuffExpireFaster` MORE: how fast effects on the enemy expire
///   (Temporal Chains's
///   `base_temporal_chains_other_buff_time_passed_+%_to_apply` → a negative
///   MORE means "expire slower"), consumed by
///   `ailment::debuff_duration_mult` (CalcOffence.lua:1833-1835
///   `debuffDurationMult = 1 / max(
///   BuffExpirationSlowCap, calcLib.mod(enemyDB, cfg, "BuffExpireFaster"))`,
///   folded into ailment duration at :5040).
///
/// Everything else (`TemporalChainsActionSpeed` / `FreezeBuildup` /
/// `ElectrocuteBuildup` / `IgnoreArmour` / `Dummy`...) has no enemy-side
/// consumer in pobr yet, so it's reported as `UnknownModName` and added to
/// the list once a consumer lands.
fn translate_curse_mod_name(name: &str) -> Result<Vec<&'static str>, UnsupportedReason> {
    match name {
        "FireResist" => Ok(vec!["FireResist"]),
        "ColdResist" => Ok(vec!["ColdResist"]),
        "LightningResist" => Ok(vec!["LightningResist"]),
        "ChaosResist" => Ok(vec!["ChaosResist"]),
        "ElementalResist" => Ok(vec!["FireResist", "ColdResist", "LightningResist"]),
        "Damage" => Ok(vec!["Damage"]),
        "SelfCritMultiplier" => Ok(vec!["SelfCritMultiplier"]),
        "BuffExpireFaster" => Ok(vec!["BuffExpireFaster"]),
        other => Err(UnsupportedReason::UnknownModName(other.to_string())),
    }
}

/// Translates a curse element (group recursion + mod constructor; flag/
/// skill_data with a curse tag doesn't occur in the data, so any occurrence
/// is Unsupported -- curse semantics for non-mod payloads aren't modeled).
fn collect_curse_element(
    element: &StatMapMod,
    params: &MergeParams,
    stat_value: f64,
    items: &mut Vec<MappedItem>,
) -> Result<(), UnsupportedReason> {
    if element.scalar.is_some() {
        return Err(UnsupportedReason::ScalarMultiplier);
    }
    match element.kind.as_str() {
        "group" => {
            let group_params = MergeParams {
                div: element.div,
                mult: element.mult,
                base: element.base,
                value: match &element.value {
                    Some(StatMapValue::Number(v)) => Some(*v),
                    Some(_) => {
                        return Err(UnsupportedReason::UnsupportedKind(
                            "group 非数值 value".to_string(),
                        ));
                    }
                    None => None,
                },
            };
            for nested in element.mods.iter().filter(|e| is_curse_effect(e)) {
                collect_curse_element(nested, &group_params, stat_value, items)?;
            }
            Ok(())
        }
        "mod" => collect_curse_mod(element, params.merge(stat_value), items),
        other => Err(UnsupportedReason::UnsupportedKind(format!(
            "curse 非 mod 载荷：{other}"
        ))),
    }
}

/// Translates a curse `mod()` constructor: enemy-side allow-list name +
/// GlobalEffect stripped + other tags translated directly.
fn collect_curse_mod(
    element: &StatMapMod,
    merged_value: f64,
    items: &mut Vec<MappedItem>,
) -> Result<(), UnsupportedReason> {
    let Some(name) = element.name.as_deref() else {
        return Err(UnsupportedReason::UnknownModName("<missing name>".into()));
    };
    let Some(mod_type) = element.mod_type.as_deref() else {
        return Err(UnsupportedReason::MissingModType);
    };
    let mod_type = match mod_type {
        "BASE" => ModType::Base,
        "INC" => ModType::Inc,
        "MORE" => ModType::More,
        "FLAG" => ModType::Flag,
        "OVERRIDE" => ModType::Override,
        other => return Err(UnsupportedReason::UnsupportedModType(other.to_string())),
    };
    // First batch: curse payloads have no flag / keyword_flag (enemy-side cfg
    // doesn't derive scope bits, so attaching one would silently undercount);
    // a non-empty one reports the whole entry.
    if !element.flags.is_empty() {
        return Err(UnsupportedReason::UnsupportedFlags(element.flags.join("|")));
    }
    if !element.keyword_flags.is_empty() {
        return Err(UnsupportedReason::UnsupportedKeywordFlags(
            element.keyword_flags.join("|"),
        ));
    }
    // tag: GlobalEffect is stripped (a key outside the convention means extra
    // gating semantics, reported wholesale); everything else is translated
    // directly.
    let mut tags = Vec::new();
    for tag in &element.tags {
        let is_global =
            matches!(tag.get("type"), Some(StatMapValue::Text(t)) if t == "GlobalEffect");
        if is_global {
            if !tag
                .keys()
                .all(|k| matches!(k.as_str(), "type" | "effectType"))
            {
                return Err(UnsupportedReason::UnsupportedTag(format!(
                    "GlobalEffect 含约定外键：{:?}",
                    tag.keys().collect::<Vec<_>>()
                )));
            }
            continue;
        }
        tags.push(translate_tag(tag)?);
    }
    for translated in translate_curse_mod_name(name)? {
        let mut modifier = if mod_type == ModType::Flag {
            Modifier::flag(translated)
        } else {
            Modifier::number(translated, mod_type, merged_value)
        };
        for tag in &tags {
            modifier = modifier.with_tag(tag.clone());
        }
        items.push(MappedItem::Modifier(Box::new(modifier)));
    }
    Ok(())
}

//  player buff domain (GlobalEffect effectType=Buff/Aura) player-side mapping

/// Whether an element is a **player-side buff payload** (vendor
/// `GlobalEffect` tag with `effectType ∈ {Buff, Aura}` --
/// `CalcActiveSkill.lua:976-1041` moves matching elements into
/// `buff.modList`, and `CalcPerform.lua:1949-1962` (Buff) / :2086-2120 (Aura)
/// write them to the player modDB after the BuffEffect/AuraEffect multiplier
/// zone `ScaleAddList`). A hit on any group member counts for the whole group
/// (the same conservative generalization as [`is_curse_effect`]).
fn is_player_buff_effect(element: &StatMapMod) -> bool {
    if element.kind == "group" {
        return element.mods.iter().any(is_player_buff_effect);
    }
    element.tags.iter().any(|tag| {
        matches!(tag.get("type"), Some(StatMapValue::Text(t)) if t == "GlobalEffect")
            && matches!(tag.get("effectType"), Some(StatMapValue::Text(t)) if t == "Buff" || t == "Aura")
    })
}

/// Translates one stat of a buff-granting skill (or its support) through the
/// statmap data into **player-side** PoBR injection items (the BuffSpec.mods
/// read channel, consumed by the buff_pass Buff/Aura path).
///
/// Structured like [`map_curse_stat`] (the curse domain's precedent):
/// - Only elements hitting [`is_player_buff_effect`] are kept (non-buff
///   elements are skill-local mods that go through the main skill injection
///   channel, so they're **silently skipped** here). If filtering leaves no
///   buff elements, the result is `Mapped(empty)`.
/// - ModName goes through the player-side allow-list
///   [`translate_player_buff_mod_name`] -- the first batch is just
///   `Accuracy` (Precision I/II support `sup_dex.lua:4181-4250` / War
///   Banner's `base_skill_buff_banner_accuracy_+%_to_apply`, feeding the
///   offence accuracy aggregate at CalcOffence.lua:2555-2572). This
///   **doesn't overlap** with the defensive allow-list (ES/resistance
///   family) already covered by the static `map_aura_buff_stat` mapping, to
///   avoid double-injecting through the aura path.
/// - `GlobalEffect` tag is stripped; besides the curse domain's convention
///   keys, `effectName` is also allowed (the buff's display name, used by
///   vendor only to name AffectedBy conditions, no gating semantics).
/// - flags / keyword_flags must be empty in the first batch (every payload
///   on the allow-list has no flags).
///
/// **Each element is handled independently** (unlike map_curse_stat's
/// "skip the whole entry" rule): vendor's merge loop translates each
/// modOrGroup of an entry into modList **independently**
/// (CalcActiveSkill.lua:96-117 does `mergeStat` per element, with no grouped
/// coupling between elements), so a single untranslatable element only skips
/// itself, not its siblings -- a concrete case is Pinnacle of Power's
/// (other.lua:12503) `elemental_power_elemental_damage_+%_final_per_
/// power_charge` entry: its first element, `Damage MORE` with a scalar
/// Multiplier (outside the ScalarMultiplier boundary), shouldn't drag down
/// the same entry's six `<El>Can<Ailment>` flag payloads.
/// Visibility: if every matching element fails (zero injections), the first
/// failure reason is still reported as Unsupported; a partial success yields
/// `Mapped(the successful subset)` (failed elements just inject nothing --
/// skip rather than miscompute).
pub fn map_player_buff_stat(
    catalog: &StatMapCatalog,
    effect_id: &str,
    set_key: Option<&str>,
    stat: &str,
    stat_value: f64,
) -> MappedOutcome {
    let Some(entry) = catalog.lookup(effect_id, set_key, stat) else {
        return MappedOutcome::Unknown;
    };
    if entry.unextractable {
        return MappedOutcome::Unsupported(UnsupportedReason::Unextractable);
    }
    let entry_params = MergeParams {
        div: entry.div,
        mult: entry.mult,
        base: entry.base,
        value: entry.value,
    };
    let mut items = Vec::new();
    let mut first_failure: Option<UnsupportedReason> = None;
    for element in entry.mods.iter().filter(|e| is_player_buff_effect(e)) {
        // Each element is handled independently (see the function doc): a
        // failed element injects nothing and doesn't drag down its siblings.
        // Per-element scratch vec guards against a half-injected group (some
        // members already pushed before a later member fails).
        let mut element_items = Vec::new();
        match collect_player_buff_element(element, &entry_params, stat_value, &mut element_items) {
            Ok(()) => items.append(&mut element_items),
            Err(reason) => {
                first_failure.get_or_insert(reason);
            }
        }
    }
    if items.is_empty()
        && let Some(reason) = first_failure
    {
        // Every matching element failed -> report Unsupported.
        return MappedOutcome::Unsupported(reason);
    }
    MappedOutcome::Mapped(items)
}

/// Player-side ModName allow-list (buff domain). Checked family by family
/// against their consumers before admission:
/// - `Accuracy` INC: the accuracy section in `offence.rs`
///   (CalcOffence.lua:2555-2572 `skillModList:Sum("INC", cfg, "Accuracy")`) --
///   first batch.
/// - `ManaRegen` INC (Clarity I/II, vendor sup_int.txt:305-315): consumed by
///   `calc::survivability::calc_regen` (vendor CalcDefence.lua:1642
///   `Sum("INC", nil, resource.."Regen", resource.."RecoveryRate")`).
/// - `LifeRegenPercent` BASE (Vitality I/II, vendor sup_str.txt:1791-1802,
///   per-minute div 60): same consumer as above (CalcDefence.lua:1658
///   `pool × Sum("BASE", resource.."RegenPercent")/100`).
///
/// **Not admitted** (already surveyed against the 18-build corpus and
/// recorded):
/// - The `base_skill_buff_*_to_apply` defensive family (Purity/Impurity/
///   Discipline's FireResistance/ChaosResistance/EnergyShield...) -- already
///   injected via `map_aura_buff_stat`'s static allow-list (the aura
///   channel), so admitting them here would double-inject.
/// - Mysticism's `Damage INC + ModFlag.Spell + Condition:FullEnergyShield`
///   (sup_int.txt:1250-1251) -- belongs to the damage-vector line, and a
///   non-empty flags set is reported wholesale under this domain's
///   convention anyway.
/// - The self-buff ailment-duration family (Coolheaded/Warmblooded/
///   StrongHearted's `*_duration_on_self_+%_final`), the flask domain
///   (Herbalism/Alchemist's Boon), and non-mod rage/incision payloads
///   (kind=flag/scalar) -- no consumer yet, so they stay reported as
///   `UnknownModName`/`UnsupportedKind` (skip rather than miscompute).
fn translate_player_buff_mod_name(name: &str) -> Result<Vec<&'static str>, UnsupportedReason> {
    match name {
        "Accuracy" => Ok(vec!["Accuracy"]),
        "ManaRegen" => Ok(vec!["ManaRegen"]),
        "LifeRegen" => Ok(vec!["LifeRegen"]),
        "LifeRegenPercent" => Ok(vec!["LifeRegenPercent"]),
        // Defensive buff family (Gemling ascendancy's Virtuous Barrier
        // per-Mote INC: `gem_barrier_green_grants_*` → Armour/Evasion/
        // EnergyShield INC ×Mote, `gem_barrier_red_grants_maximum_life_+%` →
        // Life INC ×Mote). Consumer = `calc::defence` (Armour/Evasion/
        // EnergyShield aggregation) + the life pool. The Multiplier tag
        // (StrengthMoteSkillCount/DexterityMoteSkillCount) is provisioned by
        // the orchestration layer.
        "Armour" => Ok(vec!["Armour"]),
        "Evasion" => Ok(vec!["Evasion"]),
        "EnergyShield" => Ok(vec!["EnergyShield"]),
        // PoBR's life-pool aggregate name is `MaximumLife` (the parser's
        // name_map normalizes "maximum Life" to this, and scaled_pool looks
        // up the same name) -- the vendor name `Life` must be normalized to
        // `MaximumLife`, otherwise barrier's per-Mote Life INC lands in a
        // dead bucket named `Life` and the life pool never reads it
        // (Armour/Evasion/EnergyShield don't have this problem because their
        // canonical names already match vendor's). This is the root cause of
        // gemling Virtuous Barrier's 24% Life INC gap.
        "Life" => Ok(vec!["MaximumLife"]),
        // Damage-vector family (Sigil of Power's
        // `circle_of_power_spell_damage_+%_final_per_stage` → Damage MORE
        // Spell; Elemental Conflux's
        // `skill_elemental_conflux_active_element_damage_+%_final` →
        // <El>Damage MORE). Consumer = the damage-bucket aggregation
        // (`calc::damage`'s `Damage`/`<El>Damage` INC/MORE queries, same
        // names as vendor CalcOffence).
        "Damage" => Ok(vec!["Damage"]),
        "FireDamage" => Ok(vec!["FireDamage"]),
        "ColdDamage" => Ok(vec!["ColdDamage"]),
        "LightningDamage" => Ok(vec!["LightningDamage"]),
        // Refraction I/II support (`sup_str.lua:5984/6023` Refractive Plating
        // buff, `support_tempered_valour_deflection_rating_%_of_evasion_rating`
        // → BASE 20). Consumer = `calc::defence_panels::calc_deflection`
        // (CalcDefence.lua:1516 `Evasion × ΣBASE EvasionGainAsDeflection / 100`).
        "EvasionGainAsDeflection" => Ok(vec!["EvasionGainAsDeflection"]),
        // The same buff's
        // `support_tempered_valour_%_armour_to_apply_to_elemental_damage` →
        // ArmourAppliesTo<El>DamageTaken BASE 30 (Refraction II). Consumer =
        // `calc::taken::armour_applies_pct` (vendor CalcDefence.lua:2361-2368
        // `percentOfArmourApplies` → `effectiveAppliedArmour`, feeding
        // per-type DamageReduction / MaximumHit / EHP). Tag shape matches the
        // deflection payload (GlobalEffect + MultiplierThreshold
        // RefractionMinimumValour statically resolves to 0).
        "ArmourAppliesToFireDamageTaken" => Ok(vec!["ArmourAppliesToFireDamageTaken"]),
        "ArmourAppliesToColdDamageTaken" => Ok(vec!["ArmourAppliesToColdDamageTaken"]),
        "ArmourAppliesToLightningDamageTaken" => Ok(vec!["ArmourAppliesToLightningDamageTaken"]),
        // Sigil of Power's `circle_of_power_max_stages` → player
        // `Multiplier:SigilOfPowerMaxStages` BASE (vendor's consumption point
        // is the dynamic cap in GetMultiplier, ModStore.lua:369; PoBR's
        // orchestration layer bridges `Multiplier:` BASE from buff payloads
        // into cfg.multipliers -- see the buff-specs injection point in
        // calc_orchestrator).
        "Multiplier:SigilOfPowerMaxStages" => Ok(vec!["Multiplier:SigilOfPowerMaxStages"]),
        // (0.5.4b #5) Blazing Critical support (sup_int.lua:959): 0.22.0 added
        // a GlobalEffect/Buff tag to
        // `support_blazing_crits_gain_%_fire_damage_with_attacks_on_critical_hit`
        // -- the 15% `DamageGainAsFire` BASE (ModFlag.Attack +
        // Condition:CritRecently) went from "a dead mod that only sits on the
        // supported skill" to a global player buff ("imbue all of your
        // Attacks"). Consumer = `calc::damage`'s gain-as matrix
        // (buildGainTable, `DamageGainAs<To>` BASE queries); ignite's fire
        // source scales up quadratically as a result (chance ∝ fire/threshold,
        // magnitude ∝ fire).
        "DamageGainAsFire" => Ok(vec!["DamageGainAsFire"]),
        // (Backlog #7-1) Archmage (act_int.lua:229-231):
        // `archmage_all_damage_%_to_gain_as_lightning_to_grant_to_non_
        // channelling_spells_per_100_max_mana` → `DamageGainAsLightning` BASE
        // 4 (GlobalEffect/Buff + SkillType Channel negated + SkillType Spell
        // + PerStat Mana div 100). Same consumer as DamageGainAsFire =
        // `calc::damage`'s gain-as matrix; the Mana denominator is
        // pre-loaded by the orchestration layer's `inject_per_x_multipliers`
        // (cfg.multipliers["Mana"] = the full-pipeline pool value). Root
        // cause of monk-invoker-frost-bomb's 0.66x TotalDPS (missing 80%
        // lightning gain-as).
        "DamageGainAsLightning" => Ok(vec!["DamageGainAsLightning"]),
        // (#10-2) Barrage buff (BarragePlayer `empower_barrage_*`,
        // act_dex.lua:216-224): `BarrageRepeats` BASE / `BarrageRepeatDamage`
        // MORE. Consumer = the Barrage-repeats DPS multiplier zone in
        // `calc::scaled_damage::dps_end_factors` (vendor CalcOffence.lua:962-976,
        // gated by Barrageable + SequentialProjectiles).
        "BarrageRepeats" => Ok(vec!["BarrageRepeats"]),
        "BarrageRepeatDamage" => Ok(vec!["BarrageRepeatDamage"]),
        // (#12 companion allies layer) Loyalty support's (SupportLoyaltyPlayer)
        // `companion_takes_%_damage_before_you_from_support` → BASE 10
        // (GlobalEffect/Buff/unscalable, SkillStatMap.lua:2559-2561). Consumer
        // = perform's `inject_companion_life` gate + `pool_setup::build_pool_state`'s
        // companion-first-absorbs-damage layer (CalcDefence.lua:2961-2965 /
        // :3656-3663).
        "TakenFromCompanionBeforeYou" => Ok(vec!["TakenFromCompanionBeforeYou"]),
        other => Err(UnsupportedReason::UnknownModName(other.to_string())),
    }
}

/// Translates a player buff element (group recursion + mod constructor; same
/// structure as the curse domain).
fn collect_player_buff_element(
    element: &StatMapMod,
    params: &MergeParams,
    stat_value: f64,
    items: &mut Vec<MappedItem>,
) -> Result<(), UnsupportedReason> {
    if element.scalar.is_some() {
        return Err(UnsupportedReason::ScalarMultiplier);
    }
    match element.kind.as_str() {
        "group" => {
            let group_params = MergeParams {
                div: element.div,
                mult: element.mult,
                base: element.base,
                value: match &element.value {
                    Some(StatMapValue::Number(v)) => Some(*v),
                    Some(_) => {
                        return Err(UnsupportedReason::UnsupportedKind(
                            "group 非数值 value".to_string(),
                        ));
                    }
                    None => None,
                },
            };
            for nested in element.mods.iter().filter(|e| is_player_buff_effect(e)) {
                collect_player_buff_element(nested, &group_params, stat_value, items)?;
            }
            Ok(())
        }
        "mod" => collect_player_buff_mod(element, params.merge(stat_value), items),
        "flag" => collect_player_buff_flag(element, items),
        other => Err(UnsupportedReason::UnsupportedKind(format!(
            "player buff 非 mod 载荷：{other}"
        ))),
    }
}

/// Translates a player buff `flag()` constructor: the buff domain's flag
/// allow-list is the cross-type infliction `<Type>Can<Ailment>` family
/// ([`is_cross_type_ailment_flag`]).
///
/// Consumer: the `{type_prefix}Can{ailment}` flag gate in
/// `calc::ailment::{cross_type_source_hit_at_roll, stored_source_at_roll}`
/// (vendor CalcOffence.lua:4791-4825 `canDoAilment` + :5453-5456
/// `type.."Can"..ailment`). Typical source = Pinnacle of Power (granted by
/// the Adonia's Ego weapon, other.lua:12503)'s six `<El>Can<Ailment>` FLAGs
/// (all carrying the GlobalEffect/Buff tag; vendor writes them globally
/// through the buff loop).
///
/// Flag names outside the allow-list are still reported as unknown (same
/// rule as the main channel's [`is_consumable_flag`]: a wrong injection would
/// pollute ModDb flag queries); tag handling matches
/// [`collect_player_buff_mod`] (GlobalEffect stripped + convention-key check
/// + everything else translated directly).
fn collect_player_buff_flag(
    element: &StatMapMod,
    items: &mut Vec<MappedItem>,
) -> Result<(), UnsupportedReason> {
    let name = element.name.as_deref().unwrap_or("?");
    // `SequentialProjectiles` (Barrage buff, act_dex.lua:219): consumer = the
    // Barrage-repeats gate in `dps_end_factors` (vendor CalcOffence.lua:962).
    if !is_cross_type_ailment_flag(name) && name != "SequentialProjectiles" {
        return Err(UnsupportedReason::UnknownModName(format!("flag:{name}")));
    }
    if !element.flags.is_empty() {
        return Err(UnsupportedReason::UnsupportedFlags(element.flags.join("|")));
    }
    if !element.keyword_flags.is_empty() {
        return Err(UnsupportedReason::UnsupportedKeywordFlags(
            element.keyword_flags.join("|"),
        ));
    }
    let mut modifier = Modifier::flag(name);
    for tag in &element.tags {
        let is_global =
            matches!(tag.get("type"), Some(StatMapValue::Text(t)) if t == "GlobalEffect");
        if is_global {
            if !tag
                .keys()
                .all(|k| matches!(k.as_str(), "type" | "effectType" | "effectName"))
            {
                return Err(UnsupportedReason::UnsupportedTag(format!(
                    "GlobalEffect 含约定外键：{:?}",
                    tag.keys().collect::<Vec<_>>()
                )));
            }
            continue;
        }
        modifier = modifier.with_tag(translate_tag(tag)?);
    }
    items.push(MappedItem::Modifier(Box::new(modifier)));
    Ok(())
}

/// Recognizes the `<Type>Can<Ailment>` cross-type infliction flag family:
/// type ∈ the five damage-type prefixes (the same table as
/// `calc::ailment::type_prefix`), ailment ∈ the seven ailment names the ModDb
/// consumer recognizes (the same table as `calc::ailment::ailment_mod_name`).
/// The consumer builds the name as `format!("{prefix}Can{ailment}")`, and
/// this check matches that literally.
fn is_cross_type_ailment_flag(name: &str) -> bool {
    let Some(rest) = ["Physical", "Fire", "Cold", "Lightning", "Chaos"]
        .iter()
        .find_map(|p| name.strip_prefix(p))
    else {
        return false;
    };
    let Some(ailment) = rest.strip_prefix("Can") else {
        return false;
    };
    matches!(
        ailment,
        "Bleed" | "Ignite" | "Poison" | "Shock" | "Chill" | "Freeze" | "Electrocute"
    )
}

/// Translates a player buff `mod()` constructor: the name must be on the
/// player-side allow-list, `GlobalEffect` is stripped (`effectName` is allowed
/// through as well), and everything else is translated directly.
fn collect_player_buff_mod(
    element: &StatMapMod,
    merged_value: f64,
    items: &mut Vec<MappedItem>,
) -> Result<(), UnsupportedReason> {
    let Some(name) = element.name.as_deref() else {
        return Err(UnsupportedReason::UnknownModName("<missing name>".into()));
    };
    let Some(mod_type) = element.mod_type.as_deref() else {
        return Err(UnsupportedReason::MissingModType);
    };
    let mod_type = match mod_type {
        "BASE" => ModType::Base,
        "INC" => ModType::Inc,
        "MORE" => ModType::More,
        "FLAG" => ModType::Flag,
        "OVERRIDE" => ModType::Override,
        other => return Err(UnsupportedReason::UnsupportedModType(other.to_string())),
    };
    // flags go through the ModFlag subset direct translation (allowed
    // through: Sigil of Power's Damage MORE carries a `Spell` flag -- vendor
    // has flags=ModFlag.Spell, so the matching semantics agree on both
    // sides; tokens outside the subset are still reported wholesale). The
    // `Hit` ModFlag routes to the HIT keyword (see translate_mod_flags).
    let (flags, kw_from_flags) = translate_mod_flags(&element.flags)?;
    if !element.keyword_flags.is_empty() {
        return Err(UnsupportedReason::UnsupportedKeywordFlags(
            element.keyword_flags.join("|"),
        ));
    }
    // tag: GlobalEffect is stripped (an extra gating key reports the whole
    // entry; `effectName` = the buff's display name, no gating semantics, so
    // it's allowed; `unscalable` = a marker that the buff effect multiplier
    // zone is exempt -- PoBR's buff_pass doesn't model the scaling-exemption
    // dimension, but there's no difference when the multiplier zone is 1, so
    // it's allowed through and logged); everything else is translated
    // directly.
    let mut tags = Vec::new();
    for tag in &element.tags {
        let is_global =
            matches!(tag.get("type"), Some(StatMapValue::Text(t)) if t == "GlobalEffect");
        if is_global {
            if !tag.keys().all(|k| {
                matches!(
                    k.as_str(),
                    "type" | "effectType" | "effectName" | "unscalable"
                )
            }) {
                return Err(UnsupportedReason::UnsupportedTag(format!(
                    "GlobalEffect 含约定外键：{:?}",
                    tag.keys().collect::<Vec<_>>()
                )));
            }
            continue;
        }
        tags.push(translate_tag(tag)?);
    }
    for translated in translate_player_buff_mod_name(name)? {
        let mut modifier = if mod_type == ModType::Flag {
            Modifier::flag(translated)
        } else {
            Modifier::number(translated, mod_type, merged_value)
        };
        modifier.flags = flags;
        modifier.keyword_flags = modifier.keyword_flags | kw_from_flags;
        for tag in &tags {
            modifier = modifier.with_tag(tag.clone());
        }
        items.push(MappedItem::Modifier(Box::new(modifier)));
    }
    Ok(())
}

// #12: minion domain (MinionModifier LIST payload -> inner minion mod)

/// Translates one support/skill stat through statmap into a **minion-side**
/// list of inner modifiers (the `MinionModifierEntry.inner` payload;
/// consumer = the orchestration layer's spawn_minions ->
/// `build_minion_context` channel 1).
///
/// Vendor semantics: statmap's
/// `mod("MinionModifier","LIST",{ mod = mod(<inner>) })` merges into the
/// supported skill's skillModList along with the support (the
/// CalcActiveSkill.lua merge loop), and `addMinionModifiers`
/// (CalcPerform.lua:1668-1686) then injects the inner mod into **that
/// skill's** minion modDB -- the scope is the minions of the supported skill
/// in that group, not global. Typical case: the Loyalty support's
/// `support_trusty_companion_minion_life_+%_final` -> Life MORE -30 (the
/// wolf-pack companion's life 3231 × 0.7 = 2262 MORE factor, pinned by the
/// oracle).
///
/// The first batch's allowed inner mod is `Life` (BASE/INC/MORE; the vendor
/// name `Life` is normalized to PoBR's life-pool aggregate name
/// `MaximumLife`, the same rule as the buff domain's
/// [`translate_player_buff_mod_name`]). Other inner mods (damage/speed
/// families) aren't admitted yet -- minion DPS isn't on the parity panel, so
/// skip rather than miscompute. Conservative gate: an entry whose outer
/// layer carries flags/keyword_flags/tags/scalar, or whose inner layer has
/// keys outside kind/mod_type/name, is skipped wholesale (returns empty, not
/// reported -- this is a narrow channel and unadmitted shapes have no
/// consumer).
pub fn map_minion_life_stat(
    catalog: &StatMapCatalog,
    effect_id: &str,
    set_key: Option<&str>,
    stat: &str,
    stat_value: f64,
) -> Vec<Modifier> {
    let Some(entry) = catalog.lookup(effect_id, set_key, stat) else {
        return Vec::new();
    };
    if entry.unextractable {
        return Vec::new();
    }
    let params = MergeParams {
        div: entry.div,
        mult: entry.mult,
        base: entry.base,
        value: entry.value,
    };
    let mut out = Vec::new();
    for element in &entry.mods {
        if element.kind != "mod"
            || element.name.as_deref() != Some("MinionModifier")
            || element.mod_type.as_deref() != Some("LIST")
            || !element.flags.is_empty()
            || !element.keyword_flags.is_empty()
            || !element.tags.is_empty()
            || element.scalar.is_some()
        {
            continue;
        }
        let Some(StatMapValue::Table(wrapper)) = &element.value else {
            continue;
        };
        let Some(StatMapValue::Table(inner)) = wrapper.get("mod") else {
            continue;
        };
        // The inner layer only accepts the bare shape
        // `{ kind:"mod", mod_type, name:"Life" }` (an inner layer with
        // tags/flags carries extra gating semantics, so it's skipped).
        if wrapper.len() != 1
            || inner
                .keys()
                .any(|k| !matches!(k.as_str(), "kind" | "mod_type" | "name"))
        {
            continue;
        }
        if !matches!(inner.get("name"), Some(StatMapValue::Text(n)) if n == "Life") {
            continue;
        }
        let mod_type = match inner.get("mod_type") {
            Some(StatMapValue::Text(t)) => match t.as_str() {
                "BASE" => ModType::Base,
                "INC" => ModType::Inc,
                "MORE" => ModType::More,
                _ => continue,
            },
            _ => continue,
        };
        out.push(Modifier::number(
            "MaximumLife",
            mod_type,
            params.merge(stat_value),
        ));
    }
    out
}

//  debuff domain (GlobalEffect effectType=Debuff) enemy-side mapping

/// Whether an element is an **enemy-side debuff payload** (vendor
/// `GlobalEffect` tag with `effectType == "Debuff"` --
/// `CalcActiveSkill.lua:976-1041` moves matching elements into
/// `buff.modList` (buff.type = "Debuff"), and `CalcPerform.lua:2219-2285`
/// writes them to enemyDB via mergeBuff into the `debuffs` table after the
/// DebuffEffect multiplier zone `ScaleAddList`). A hit on any group member
/// counts for the whole group (the same conservative generalization as
/// [`is_curse_effect`]).
fn is_debuff_effect(element: &StatMapMod) -> bool {
    if element.kind == "group" {
        return element.mods.iter().any(is_debuff_effect);
    }
    element.tags.iter().any(|tag| {
        matches!(tag.get("type"), Some(StatMapValue::Text(t)) if t == "GlobalEffect")
            && matches!(tag.get("effectType"), Some(StatMapValue::Text(t)) if t == "Debuff")
    })
}

/// Translates one debuff skill stat through the statmap data into
/// **enemy-side** PoBR injection items (the BuffSpec.mods read channel,
/// consumed by the buff_pass Debuff path).
///
/// Structured like [`map_curse_stat`] (the curse domain's precedent):
/// - Only elements hitting [`is_debuff_effect`] are kept (non-debuff
///   elements are skill-local mods that go through the main skill injection
///   channel, so they're **silently skipped** here). If filtering leaves no
///   debuff elements, the result is `Mapped(empty)`.
/// - ModName goes through the enemy-side allow-list
///   [`translate_debuff_mod_name`] -- the first batch is the elemental
///   exposure family (Frost Bomb's
///   `active_skill_all_elemental_exposure_magnitude` → `<El>Exposure BASE`,
///   vendor SkillStatMap.lua:1721-1725; consumer =
///   `calc::reduce_enemy_exposure`'s exposure reduction, CalcPerform.lua:3214-3247
///   "Apply exposures", which folds the strongest of enemyDB's `<El>Exposure`
///   into `<El>Resist BASE -magnitude`). Unknown names report the whole
///   entry as `UnknownModName` (skip rather than miscompute).
/// - `GlobalEffect` tag is stripped (same convention-key check as the curse
///   domain); everything else is translated directly.
/// - flags / keyword_flags must be empty in the first batch.
pub fn map_debuff_stat(
    catalog: &StatMapCatalog,
    effect_id: &str,
    set_key: Option<&str>,
    stat: &str,
    stat_value: f64,
) -> MappedOutcome {
    let Some(entry) = catalog.lookup(effect_id, set_key, stat) else {
        return MappedOutcome::Unknown;
    };
    if entry.unextractable {
        return MappedOutcome::Unsupported(UnsupportedReason::Unextractable);
    }
    let entry_params = MergeParams {
        div: entry.div,
        mult: entry.mult,
        base: entry.base,
        value: entry.value,
    };
    let mut items = Vec::new();
    for element in entry.mods.iter().filter(|e| is_debuff_effect(e)) {
        if let Err(reason) = collect_debuff_element(element, &entry_params, stat_value, &mut items)
        {
            // Same rule as map_curse_stat: any debuff element that can't be
            // translated skips the whole entry (grouped semantics).
            return MappedOutcome::Unsupported(reason);
        }
    }
    MappedOutcome::Mapped(items)
}

/// Enemy-side ModName allow-list (debuff domain). The first batch is
/// elemental exposure (consumer = `calc::reduce_enemy_exposure`, which reads
/// enemy db `<El>Exposure` BASE).
///
/// Other debuff payload names (`ColdDamageTaken`/`MovementSpeed`...) have no
/// enemy-side consumer in pobr yet after checking one by one, so they're
/// reported as `UnknownModName` and added to the list once a consumer lands.
fn translate_debuff_mod_name(name: &str) -> Result<Vec<&'static str>, UnsupportedReason> {
    match name {
        "FireExposure" => Ok(vec!["FireExposure"]),
        "ColdExposure" => Ok(vec!["ColdExposure"]),
        "LightningExposure" => Ok(vec!["LightningExposure"]),
        other => Err(UnsupportedReason::UnknownModName(other.to_string())),
    }
}

/// Translates a debuff element (group recursion + mod constructor; same
/// structure as the curse domain).
fn collect_debuff_element(
    element: &StatMapMod,
    params: &MergeParams,
    stat_value: f64,
    items: &mut Vec<MappedItem>,
) -> Result<(), UnsupportedReason> {
    if element.scalar.is_some() {
        return Err(UnsupportedReason::ScalarMultiplier);
    }
    match element.kind.as_str() {
        "group" => {
            let group_params = MergeParams {
                div: element.div,
                mult: element.mult,
                base: element.base,
                value: match &element.value {
                    Some(StatMapValue::Number(v)) => Some(*v),
                    Some(_) => {
                        return Err(UnsupportedReason::UnsupportedKind(
                            "group 非数值 value".to_string(),
                        ));
                    }
                    None => None,
                },
            };
            for nested in element.mods.iter().filter(|e| is_debuff_effect(e)) {
                collect_debuff_element(nested, &group_params, stat_value, items)?;
            }
            Ok(())
        }
        "mod" => collect_debuff_mod(element, params.merge(stat_value), items),
        other => Err(UnsupportedReason::UnsupportedKind(format!(
            "debuff 非 mod 载荷：{other}"
        ))),
    }
}

/// Translates a debuff `mod()` constructor: enemy-side allow-list name +
/// GlobalEffect stripped + other tags translated directly.
fn collect_debuff_mod(
    element: &StatMapMod,
    merged_value: f64,
    items: &mut Vec<MappedItem>,
) -> Result<(), UnsupportedReason> {
    let Some(name) = element.name.as_deref() else {
        return Err(UnsupportedReason::UnknownModName("<missing name>".into()));
    };
    let Some(mod_type) = element.mod_type.as_deref() else {
        return Err(UnsupportedReason::MissingModType);
    };
    let mod_type = match mod_type {
        "BASE" => ModType::Base,
        "INC" => ModType::Inc,
        "MORE" => ModType::More,
        "FLAG" => ModType::Flag,
        "OVERRIDE" => ModType::Override,
        other => return Err(UnsupportedReason::UnsupportedModType(other.to_string())),
    };
    // First batch: debuff's allowed payloads have no flag / keyword_flag; a
    // non-empty one reports the whole entry.
    if !element.flags.is_empty() {
        return Err(UnsupportedReason::UnsupportedFlags(element.flags.join("|")));
    }
    if !element.keyword_flags.is_empty() {
        return Err(UnsupportedReason::UnsupportedKeywordFlags(
            element.keyword_flags.join("|"),
        ));
    }
    // tag: GlobalEffect is stripped (a key outside the convention means
    // extra gating semantics, reported wholesale); everything else is
    // translated directly.
    let mut tags = Vec::new();
    for tag in &element.tags {
        let is_global =
            matches!(tag.get("type"), Some(StatMapValue::Text(t)) if t == "GlobalEffect");
        if is_global {
            if !tag
                .keys()
                .all(|k| matches!(k.as_str(), "type" | "effectType"))
            {
                return Err(UnsupportedReason::UnsupportedTag(format!(
                    "GlobalEffect 含约定外键：{:?}",
                    tag.keys().collect::<Vec<_>>()
                )));
            }
            continue;
        }
        tags.push(translate_tag(tag)?);
    }
    for translated in translate_debuff_mod_name(name)? {
        let mut modifier = if mod_type == ModType::Flag {
            Modifier::flag(translated)
        } else {
            Modifier::number(translated, mod_type, merged_value)
        };
        for tag in &tags {
            modifier = modifier.with_tag(tag.clone());
        }
        items.push(MappedItem::Modifier(Box::new(modifier)));
    }
    Ok(())
}

/// Entry- or group-level merge parameters (vendor `map.div/mult/base/value`).
struct MergeParams {
    div: Option<f64>,
    mult: Option<f64>,
    base: Option<f64>,
    value: Option<f64>,
}

impl MergeParams {
    /// The merge formula itself (matches CalcActiveSkill.lua:112 line for
    /// line; scalar is fixed at 1.0, and entries with a scalar are already
    /// Unsupported wholesale before reaching this formula).
    fn merge(&self, stat_value: f64) -> f64 {
        match self.value {
            Some(v) => v,
            None => {
                stat_value * self.mult.unwrap_or(1.0) * SCALAR_FIXED / self.div.unwrap_or(1.0)
                    + self.base.unwrap_or(0.0)
            }
        }
    }
}

/// Translates a single element (mod / flag / skill_data / group). Groups
/// recurse, and the nested mods use group-level parameters
/// (CalcActiveSkill.lua:117).
fn collect_element(
    element: &StatMapMod,
    params: &MergeParams,
    stat_value: f64,
    items: &mut Vec<MappedItem>,
) -> Result<(), UnsupportedReason> {
    if element.scalar.is_some() {
        return Err(UnsupportedReason::ScalarMultiplier);
    }
    match element.kind.as_str() {
        "group" => {
            let group_params = MergeParams {
                div: element.div,
                mult: element.mult,
                base: element.base,
                value: None, // group-level value is stored by the extraction layer in StatMapMod.value (StatMapValue) -- see below
            };
            // group-level literal value (vendor `modOrGroup.value`): only a
            // numeric value overrides the formula.
            let group_params = match &element.value {
                Some(StatMapValue::Number(v)) => MergeParams {
                    value: Some(*v),
                    ..group_params
                },
                Some(_) => {
                    return Err(UnsupportedReason::UnsupportedKind(
                        "group 非数值 value".to_string(),
                    ));
                }
                None => group_params,
            };
            for nested in &element.mods {
                collect_element(nested, &group_params, stat_value, items)?;
            }
            Ok(())
        }
        "mod" => collect_mod(element, params.merge(stat_value), items),
        "flag" => {
            // Vendor flag(name) is mostly a skill behaviour switch
            // (projectile / unarmedMelee...) that PoBR has no consumer for,
            // so it's reported as unknown by default; the **whitelist**
            // translates names that have a ModDb flag consumer (the
            // crit/lucky family, consumed at calc::crit::resolve_crit and
            // calc::damage::lucky_hit_chance); tags are translated as usual
            // (e.g. Garukhan's Resolve's `attacks_roll_crits_twice` →
            // flag("BifurcateCrit", SkillType.Attack),
            // SkillStatMap.lua:1011-1013).
            let name = element.name.as_deref().unwrap_or("?");
            if !is_consumable_flag(name) {
                return Err(UnsupportedReason::UnknownModName(format!("flag:{name}")));
            }
            let mut modifier = Modifier::flag(name);
            for tag in &element.tags {
                modifier = modifier.with_tag(translate_tag(tag)?);
            }
            items.push(MappedItem::Modifier(Box::new(modifier)));
            Ok(())
        }
        "skill_data" => collect_skill_data(element, params.merge(stat_value), items),
        other => Err(UnsupportedReason::UnsupportedKind(other.to_string())),
    }
}

/// Whitelist of statmap `flag()` constructor names: flags that have a
/// `flag()` consumer in PoBR's ModDb (the crit/lucky family --
/// `calc::crit::resolve_crit` steps 4/5/6/crit damage plus
/// `calc::damage::lucky_hit_chance`; the mod-conversion family --
/// `calc::perform::apply_projectile_speed_to_damage`,
/// CalcOffence.lua:840-845). Flag names outside the whitelist are still
/// reported as unknown (mostly skill behaviour switches; a wrong injection
/// would pollute ModDb flag queries).
fn is_consumable_flag(name: &str) -> bool {
    matches!(
        name,
        "BifurcateCrit"
            | "CritChanceLucky"
            | "InevitableCriticalHits"
            | "NoCritMultiplier"
            | "LuckyHits"
            | "CritLucky"
            | "ElementalLuckHits"
            | "ProjectileSpeedAppliesToProjectileDamage"
            //  Ailment-stacking switch (Escalating Poison's
            // `number_of_additional_poison_stacks` is injected paired with
            // `PoisonStacks BASE`, sup_dex.lua:2188-2191). Consumer = the
            // maxStacks flag gate in `calc::perform::resolve_stack_config`
            // (vendor CalcOffence.lua:5022-5025).
            | "PoisonCanStack"
            | "BleedCanStack"
            | "IgniteCanStack"
    )
}

/// Translates a `mod()` constructor: the name (including flag-semantics
/// dispatch) maps to a PoBR ModName, and tags map to [`ModTag`].
fn collect_mod(
    element: &StatMapMod,
    merged_value: f64,
    items: &mut Vec<MappedItem>,
) -> Result<(), UnsupportedReason> {
    let Some(name) = element.name.as_deref() else {
        return Err(UnsupportedReason::UnknownModName("<missing name>".into()));
    };
    let Some(mod_type) = element.mod_type.as_deref() else {
        // A vendor typo entry (e.g. sup_str's CorruptingCry is missing
        // type); kept faithfully as extracted, so it's skipped here.
        return Err(UnsupportedReason::MissingModType);
    };
    let mod_type = match mod_type {
        "BASE" => ModType::Base,
        "INC" => ModType::Inc,
        "MORE" => ModType::More,
        "FLAG" => ModType::Flag,
        "OVERRIDE" => ModType::Override,
        //  Vendor's "CHANCE" bucket is consumed as
        // `Sum("CHANCE", cfg, name)` and clamped at the consumption point
        // (CalcOffence.lua:4145 HitsInvertEleResChance) -- its summation
        // semantics match BASE, and translate_mod_name's allow-list still
        // gates it name by name (the current data has only one such entry,
        // `treat_enemy_resistances_as_negated_...`, so there's no case of
        // BASE and CHANCE sharing a bucket under the same name).
        "CHANCE" => ModType::Base,
        other => return Err(UnsupportedReason::UnsupportedModType(other.to_string())),
    };
    let translated = translate_mod_name(name, &element.flags, &element.keyword_flags)?;
    let mut modifier = if mod_type == ModType::Flag {
        // A FLAG mod's merged value only carries Lua truthiness semantics
        // (any number, including 0, is truthy) -> Bool(true).
        Modifier::flag(translated.name)
    } else {
        Modifier::number(translated.name, mod_type, merged_value)
    };
    modifier = modifier
        .with_flags(translated.flags)
        .with_keyword_flags(translated.keyword_flags);
    for tag in &element.tags {
        modifier = modifier.with_tag(translate_tag(tag)?);
    }
    items.push(MappedItem::Modifier(Box::new(modifier)));
    Ok(())
}

/// Translates a `skill()` constructor: base damage keys become
/// `<Type>DamageMin/Max` BASE modifiers; `duration` becomes
/// [`MappedItem::SkillData`]; other keys are Unsupported in the first batch.
fn collect_skill_data(
    element: &StatMapMod,
    merged_value: f64,
    items: &mut Vec<MappedItem>,
) -> Result<(), UnsupportedReason> {
    // skill_data's key lands in the extraction layer's value table `{key, value}`.
    let key = match &element.value {
        Some(StatMapValue::Table(t)) => match t.get("key") {
            Some(StatMapValue::Text(k)) => k.as_str(),
            _ => {
                return Err(UnsupportedReason::UnsupportedKind(
                    "skill_data 缺 key".into(),
                ));
            }
        },
        _ => {
            return Err(UnsupportedReason::UnsupportedKind(
                "skill_data 缺 key".into(),
            ));
        }
    };
    // Translate tags first (a skill_data with an unsupported tag is also
    // skipped wholesale).
    let mut tags = Vec::new();
    for tag in &element.tags {
        tags.push(translate_tag(tag)?);
    }
    // Base damage keys (vendor writes a skill's base damage into skillData;
    // PoBR has no skillData table, so it's consumed as `<Type>DamageMin/Max`
    // BASE through the modifier pipeline instead, matching legacy's
    // `map_base_damage` behaviour).
    if let Some(mod_name) = damage_bound_mod_name(key) {
        let mut modifier = Modifier::number(mod_name, ModType::Base, merged_value);
        for tag in tags {
            modifier = modifier.with_tag(tag);
        }
        items.push(MappedItem::Modifier(Box::new(modifier)));
        return Ok(());
    }
    //  Skill DoT base value keys (vendor `skill("<Type>Dot", …)`, sourced from
    // the `base_<type>_damage_to_deal_per_minute` stat, with the entry-level
    // div=60 already converting per-minute to per-second) become a
    // same-named `<Type>Dot` BASE modifier (PoBR has no skillData table, so
    // it's consumed through the modifier pipeline, matching the existing
    // convention for the base-damage family `<Type>DamageMin/Max`; consumer =
    // `calc::skill_dot`, aggregated by dotTypeCfg).
    if let Some(mod_name) = dot_base_mod_name(key) {
        let mut modifier = Modifier::number(mod_name, ModType::Base, merged_value);
        for tag in tags {
            modifier = modifier.with_tag(tag);
        }
        items.push(MappedItem::Modifier(Box::new(modifier)));
        return Ok(());
    }
    //  `dotIs*` boolean keys (vendor `skill("dotIsSpell", true)` and similar,
    // sourced from stats like `spell_damage_modifiers_apply_to_skill_dot`)
    // become `DotIs<X>` FLAG modifiers (`calc::skill_dot` keeps the
    // corresponding dotCfg bit when set and strips it otherwise --
    // CalcOffence.lua:5839-5856). Note: the current `.dat` ingest has no
    // value-less boolean stats, so this channel never fires against the
    // present data; the dotIs* that hang directly off statSet baseMods
    // (TornadoShot) go through catalog `DotFlags` -> the orchestration layer
    // injects the same-named FLAG (the same consumption path).
    if let Some(flag_name) = dot_is_flag_mod_name(key) {
        if !tags.is_empty() {
            return Err(UnsupportedReason::UnsupportedTag(
                "skill_data 带 tag".into(),
            ));
        }
        items.push(MappedItem::Modifier(Box::new(Modifier::flag(flag_name))));
        return Ok(());
    }
    // skill_data whitelist (produces [`MappedItem::SkillData`], consumed by
    // the orchestration layer by key):
    // - duration (vendor `skill("duration", …)`; the entry-level div=1000
    //   already converts ms to s in the merge formula);
    // - corpseExplosionLifeMultiplier (vendor SkillStatMap.lua:309-316:
    //   `corpse_explosion_monster_life_%` div=100 /
    //   `_permillage_physical` div=1000 -> the corpse life multiplier;
    //   consumer = the orchestration layer's corpse-explosion base-damage
    //   injection, CalcOffence.lua:2211-2217).
    // Other keys are counted as unsupported.
    if matches!(key, "duration" | "corpseExplosionLifeMultiplier") {
        if !tags.is_empty() {
            return Err(UnsupportedReason::UnsupportedTag(
                "skill_data 带 tag".into(),
            ));
        }
        items.push(MappedItem::SkillData {
            key: key.to_string(),
            value: merged_value,
        });
        return Ok(());
    }
    Err(UnsupportedReason::UnsupportedSkillDataKey(key.to_string()))
}

/// `<Type>Dot` skill_data key -> the same-named BASE ModName (vendor
/// SkillStatMap's `base_<type>_damage_to_deal_per_minute` entries, across all
/// five damage types).
fn dot_base_mod_name(key: &str) -> Option<String> {
    DAMAGE_TYPES.iter().find_map(|ty| {
        (key == format!("{ty}Dot")).then(|| key.to_string()) // ModName matches the key
    })
}

/// `dotIs*` skill_data key -> `DotIs*` FLAG ModName (shares the same set of
/// FLAG names as the orchestration layer's injection for catalog `DotFlags`,
/// the booleans that hang directly off statSet baseMods).
fn dot_is_flag_mod_name(key: &str) -> Option<&'static str> {
    Some(match key {
        "dotIsArea" => "DotIsArea",
        "dotIsProjectile" => "DotIsProjectile",
        "dotIsSpell" => "DotIsSpell",
        "dotIsAttack" => "DotIsAttack",
        "dotIsHit" => "DotIsHit",
        _ => return None,
    })
}

/// The five damage types (PoB2 naming matches PoBR's PascalCase directly).
const DAMAGE_TYPES: [&str; 5] = ["Physical", "Fire", "Cold", "Lightning", "Chaos"];

/// `<Type>Min` / `<Type>Max` -> `<Type>DamageMin/Max` (shared by mod and
/// skill_data: vendor uses this same set of key names for base damage on
/// both channels).
pub(crate) fn damage_bound_mod_name(name: &str) -> Option<String> {
    for ty in DAMAGE_TYPES {
        if let Some(bound) = name.strip_prefix(ty)
            && (bound == "Min" || bound == "Max")
        {
            return Some(format!("{ty}Damage{bound}"));
        }
    }
    None
}

/// The output of name translation: a PoBR ModName plus the directly
/// translated ModFlags / KeywordFlags.
///
/// Public so the dual-run oracle comparison can use it (the modList injected
/// by vendor's `mergeSkillInstanceMods` uses PoB2 names, normalized through
/// this translation layer before comparison; see
/// `pobr-build/tests/statmap_dual_run.rs`).
#[derive(Debug, Clone, PartialEq)]
pub struct TranslatedName {
    /// The PoBR ModName.
    pub name: String,
    /// The directly translated scope flags (PoB2's
    /// `band(cfg.flags, mod.flags) == mod.flags` has the same semantics as
    /// PoBR's [`ModFlags::is_subset_of`], bit for bit).
    pub flags: ModFlags,
    /// The directly translated keyword flags (PoB2's `MatchKeywordFlags` ANY
    /// semantics match [`KeywordFlags::matches_context`]).
    pub keyword_flags: KeywordFlags,
}

impl TranslatedName {
    fn bare(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            flags: ModFlags::NONE,
            keyword_flags: KeywordFlags::NONE,
        }
    }
}

/// Direct translation of ModFlag tokens (PoBR's [`ModFlags`] already ports
/// this subset; matching semantics agree on both sides -- PoB2's
/// `ModList.lua` subset check equals PoBR `Modifier::matches`'s
/// `is_subset_of`). Tokens outside the subset (Thorns/Weapon/weapon-type...)
/// make the entry Unsupported (PoBR cfg never sets these bits, so attaching
/// one would make the mod never match -- a silent undercount -- so it's
/// reported wholesale instead).
///
/// Returns `(mod_flags, keyword_flags)`: `Hit` is vendor's `ModFlag.Hit`, but
/// PoBR's hit-scoping **goes through the KeywordFlag.Hit channel** (`offence`
/// ORs cfg with `KeywordFlags::HIT` on a hit, `skill_dot`/`ailment` strip
/// `KeywordFlags::HIT`) -- cfg.flags never sets `ModFlags::HIT`. So the `Hit`
/// ModFlag is routed to the HIT keyword: the matching semantics agree with
/// vendor bit for bit (applies on a hit, not on ailments/DoT), using the same
/// rule as [`translate_keyword_flags`]'s handling of `KeywordFlag.Hit`.
fn translate_mod_flags(tokens: &[String]) -> Result<(ModFlags, KeywordFlags), UnsupportedReason> {
    let mut flags = ModFlags::NONE;
    let mut keyword_flags = KeywordFlags::NONE;
    for token in tokens {
        match token.as_str() {
            "Attack" => flags |= ModFlags::ATTACK,
            "Spell" => flags |= ModFlags::SPELL,
            "Melee" => flags |= ModFlags::MELEE,
            "Projectile" => flags |= ModFlags::PROJECTILE,
            "Area" => flags |= ModFlags::AREA,
            //  The Dot bit -- once dotCfg (`calc::skill_dot`) sets the DOT
            // bit, this kind of mod (e.g.
            // `support_rapid_decay_damage_over_time_+%_final` -> Damage MORE
            // Dot) only matches the DoT aggregation (kept permanently once
            // PoB2's full bit table is switched over).
            "Dot" => flags |= ModFlags::DOT,
            // Vendor's `ModFlag.Hit` routes to PoBR's keyword HIT channel
            // (see the function doc).
            "Hit" => keyword_flags = keyword_flags | KeywordFlags::HIT,
            _ => return Err(UnsupportedReason::UnsupportedFlags(tokens.join("|"))),
        }
    }
    Ok((flags, keyword_flags))
}

/// Names whose scope is already implied by the name itself and that PoBR has
/// no consumer for: their KeywordFlag is purely a redundant scope qualifier
/// (e.g. vendor `SkillStatMap.lua:556`'s
/// `mod("WarcrySpeed","INC",nil,0,KeywordFlag.Warcry)` -- the name
/// "WarcrySpeed" already implies the Warcry scope), so it's safe to drop:
/// nothing in PoBR queries this name, so dropping the flag can't change any
/// existing calculation.
const SCOPE_NAMED_INERT: [&str; 2] = ["WarcrySpeed", "TotemPlacementSpeed"];

/// Direct translation of KeywordFlag tokens (bit values agree with PoB2's
/// `Global.lua:263-292` on both sides).
///
/// Returns `(keyword_flags, extra_mod_flags)`: `KeywordFlag.Attack` /
/// `KeywordFlag.Spell` fold into [`ModFlags::ATTACK`] / [`ModFlags::SPELL`] --
/// PoB2's ANY match on these two keywords (`MatchKeywordFlags`,
/// cfg.keywordFlags derived from skill type includes Attack/Spell) is
/// equivalent to PoBR cfg.flags's ATTACK/SPELL subset-match gate (cfg.flags
/// is likewise derived from skill_types, `calc_orchestrator.rs:1125-1129`).
/// Example: `support_attack_skills_elemental_damage_+%_final` (vendor
/// `sup_str.lua:2825-2827`
/// `mod("ElementalDamage","MORE",nil,0,KeywordFlag.Attack)`).
fn translate_keyword_flags(
    tokens: &[String],
    name: &str,
) -> Result<(KeywordFlags, ModFlags), UnsupportedReason> {
    let mut flags = KeywordFlags::NONE;
    let mut extra_mod_flags = ModFlags::NONE;
    for token in tokens {
        // A redundant keyword on an inert scope name (see
        // [`SCOPE_NAMED_INERT`]) is dropped.
        if matches!(token.as_str(), "Warcry" | "Totem") && SCOPE_NAMED_INERT.contains(&name) {
            continue;
        }
        // Attack/Spell keyword -> the equivalent ModFlags gate (see the
        // function doc).
        if token == "Attack" {
            extra_mod_flags |= ModFlags::ATTACK;
            continue;
        }
        if token == "Spell" {
            extra_mod_flags |= ModFlags::SPELL;
            continue;
        }
        let bit = match token.as_str() {
            "Aura" => KeywordFlags::AURA,
            "Curse" => KeywordFlags::CURSE,
            "Hit" => KeywordFlags::HIT,
            "Ailment" => KeywordFlags::AILMENT,
            "Poison" => KeywordFlags::POISON,
            "Bleed" => KeywordFlags::BLEED,
            "Ignite" => KeywordFlags::IGNITE,
            "PhysicalDot" => KeywordFlags::PHYSICAL_DOT,
            "LightningDot" => KeywordFlags::LIGHTNING_DOT,
            "ColdDot" => KeywordFlags::COLD_DOT,
            "FireDot" => KeywordFlags::FIRE_DOT,
            "ChaosDot" => KeywordFlags::CHAOS_DOT,
            _ => {
                return Err(UnsupportedReason::UnsupportedKeywordFlags(tokens.join("|")));
            }
        };
        flags = flags | bit;
    }
    Ok((flags, extra_mod_flags))
}

/// ModName translation layer (PoB2 name + ModFlag/KeywordFlag combination ->
/// PoBR name + flags).
///
/// Framework-level semantics, tier L4 (names track game mechanics, not
/// versions, so this stays a Rust constant table). The first pass's coverage
/// was reverse-derived from the mapping families already in legacy
/// `pobr-build::skill_stat_map`, filled in as the dual-run diff turns up
/// gaps; unknown names are reported as
/// [`UnsupportedReason::UnknownModName`].
///
/// Flag handling has two layers:
/// - **Name dispatch** (speed/damage buckets that PoBR expresses as separate
///   ModNames): `Speed` + Attack -> `AttackSpeed`; + Cast -> `CastSpeed`;
///   bare -> `SkillSpeed`. `Damage`'s single Attack/Spell/Area flag dispatches
///   the same way as legacy (PoBR's `calc::damage` reads the
///   `AttackDamage`/`AreaDamage` buckets by cfg flag as well, so the two
///   representations are equivalent);
/// - **Direct flag translation** (every other combination): attached to the
///   Modifier via [`translate_mod_flags`], with matching semantics agreeing
///   bit for bit with PoB2's subset check (e.g.
///   `support_melee_physical_damage_+%_final`'s per-set entry
///   `mod("PhysicalDamage","MORE",nil,ModFlag.Melee)` ->
///   `PhysicalDamage` MORE + `MELEE`, with cfg taking flags derived from
///   skill_types).
///
/// The base-damage family (`<Type>Min/Max`) still drops the Attack/Spell
/// flag and KeywordFlag (same rule as legacy: injected globally for a single
/// main skill, so scope qualification makes no difference).
pub fn translate_mod_name(
    name: &str,
    flags: &[String],
    keyword_flags: &[String],
) -> Result<TranslatedName, UnsupportedReason> {
    // Base-damage family: allowed to drop the Attack/Spell flag and
    // KeywordFlag (scope qualification makes no difference for a single main
    // skill; legacy is likewise flag-blind here).
    if let Some(bound_name) = damage_bound_mod_name(name) {
        let droppable = |f: &String| f == "Attack" || f == "Spell";
        if !flags.iter().all(droppable) {
            return Err(UnsupportedReason::UnsupportedFlags(flags.join("|")));
        }
        if !keyword_flags.iter().all(|f| f == "Attack" || f == "Spell") {
            return Err(UnsupportedReason::UnsupportedKeywordFlags(
                keyword_flags.join("|"),
            ));
        }
        return Ok(TranslatedName::bare(bound_name));
    }
    // Name-dispatch family: the flag consumed by dispatch is removed from
    // the direct-translation set; remaining flags are translated as usual.
    let (base_name, remaining_flags): (&str, Vec<String>) = match name {
        "Speed" => {
            if let Some(pos) = flags.iter().position(|f| f == "Attack") {
                let mut rest = flags.to_vec();
                rest.remove(pos);
                ("AttackSpeed", rest)
            } else if let Some(pos) = flags.iter().position(|f| f == "Cast") {
                let mut rest = flags.to_vec();
                rest.remove(pos);
                ("CastSpeed", rest)
            } else {
                ("SkillSpeed", flags.to_vec())
            }
        }
        "Damage" => match flags {
            [f] if f == "Attack" => ("AttackDamage", Vec::new()),
            [f] if f == "Spell" => ("Damage", Vec::new()),
            [f] if f == "Area" => ("AreaDamage", Vec::new()),
            _ => ("Damage", flags.to_vec()),
        },
        _ => (name, flags.to_vec()),
    };
    // Name translation (base_name after dispatch).
    let translated: String = match base_name {
        "CritChance" => "CriticalStrikeChance".to_string(),
        "CritMultiplier" => "CriticalStrikeMultiplier".to_string(),
        // Crit chance cap: same name as vendor (e.g. Garukhan's Resolve's
        // `maximum_critical_strike_chance_is_%` -> CritChanceCap OVERRIDE 50);
        // consumer = `calc::crit::crit_chance_cap`.
        "CritChanceCap" => base_name.to_string(),
        // Same-name pass-through family (names either consumed directly by
        // PoBR calc or scope names with no consumer).
        "Damage"
        | "AttackDamage"
        | "AreaDamage"
        | "AttackSpeed"
        | "CastSpeed"
        | "SkillSpeed"
        | "PhysicalDamage"
        | "FireDamage"
        | "ColdDamage"
        | "LightningDamage"
        | "ChaosDamage"
        | "ElementalDamage"
        // Interval-end MORE family (vendor
        // `mod("Max<Type>Damage"/"Min<Type>Damage","MORE",…)`, e.g. Heft's
        // `support_heft_maximum_physical_damage_+%_final` ->
        // `MaxPhysicalDamage` MORE, sup_str.lua:4222-4223). Consumer =
        // `calc::damage::scale_with_path` (CalcOffence.lua:138-139,153-154's
        // separate `Min/Max<Type>Damage` MORE multiplier zone, which only
        // scales one end of the interval).
        | "MaxPhysicalDamage"
        | "MaxFireDamage"
        | "MaxColdDamage"
        | "MaxLightningDamage"
        | "MaxChaosDamage"
        | "MinPhysicalDamage"
        | "MinFireDamage"
        | "MinColdDamage"
        | "MinLightningDamage"
        | "MinChaosDamage"
        | "FirePenetration"
        | "ColdPenetration"
        | "LightningPenetration"
        | "ChaosPenetration"
        | "ElementalPenetration"
        | "TotalCastTime"
        | "TotalAttackTime"
        // Vendor `SkillStatMap.lua:554-557` (skill_speed_+% shares the entry
        // with three mods) and `:2400-2401` (summon_totem_cast_speed_+%):
        // separate names for warcry/totem speed. PoBR has no consumer yet
        // (an inert injection -- the name itself is the scope, so it can't
        // cause a miscalculation); the PoB2 original name is kept so legacy
        // doesn't wrongly merge TotemPlacementSpeed into CastSpeed (a legacy
        // mismapping).
        | "WarcrySpeed"
        | "TotemPlacementSpeed"
        //  Cooldown recovery rate pass-through (vendor
        // `base_cooldown_speed_+%`/quality section/
        // `support_cooldown_reduction_cooldown_recovery_+%` -> CooldownRecovery).
        // Consumer = `calc::skill_mechanics::calc_cooldown` /
        // `offence::apply_cooldown_cap` (the whole cooldown recovery-rate
        // chain).
        | "CooldownRecovery"
        //  Exposure effect pass-through (vendor `exposure_effect_+%` -> the
        // three elemental `<El>ExposureEffect` INC, SkillStatMap.lua:1731-1735,
        // the Potent Exposure support payload). Consumer =
        // `calc::reduce_enemy_exposure` (CalcPerform.lua:3223 exposure
        // effect INC; vendor scopes this per skill, PoBR approximates with a
        // flat db global sum -- see the registered TODO(parity) in
        // reduce_enemy_exposure's doc).
        | "FireExposureEffect"
        | "ColdExposureEffect"
        | "LightningExposureEffect" => base_name.to_string(),
        //  Treats the enemy's elemental resistance as inverted on hit
        // (Rakiata's Flow's
        // `treat_enemy_resistances_as_negated_on_elemental_damage_hit_%_chance`
        // -> CHANCE, SkillStatMap.lua:941-944, entry div=100 -> a fraction).
        // Consumer = the resistance section of
        // `offence::enemy_damage_multiplier` (CalcOffence.lua:4145-4148
        // `resist = resist - 2 * invertChance * resist`).
        "HitsInvertEleResChance" => base_name.to_string(),
        //  Grenade's chance to detonate twice (vendor
        // SkillStatMap.lua:2795-2797's
        // `grenade_skill_%_chance_to_explode_twice` -> GrenadeActivateTwice
        // BASE; only a SupportPayload produces this stat). Consumer =
        // `calc::scaled_damage::dps_end_factors` (vendor
        // CalcOffence.lua:1124-1127 folds it into a DPS MORE).
        "GrenadeActivateTwice" => base_name.to_string(),
        //  Damage-ailment family pass-through (consumer = `calc::ailment` +
        // `calc::perform::fill_ailments`). Infliction chance: bleed/poison's
        // intrinsic `<Ailment>Chance` (vendor
        // `base_chance_to_inflict_bleeding_%`/`base_chance_to_poison_on_hit_%`,
        // the SkillStatMap.lua:1267/:1311 families), derived ignite/shock
        // chance `Enemy<Ailment>Chance` (`base_chance_to_ignite_%` etc.);
        // magnitude: `AilmentMagnitude` (Deadly Poison/Ignites's
        // `*_effect_+%_final`, keyword-scoped via direct keyword
        // translation) plus shock/chill magnitude
        // (`EnemyShockMagnitude`/`EnemyChillMagnitude`, consumed by
        // `shock_traced`/`chill_traced`); stacking: `<Ailment>Stacks`
        // (Escalating Poison's `number_of_additional_poison_stacks`,
        // consumed by `resolve_stack_config`, paired with the
        // `<Ailment>CanStack` flag).
        "PoisonChance" | "BleedChance" | "EnemyIgniteChance" | "EnemyShockChance"
        | "AilmentMagnitude" | "EnemyShockMagnitude" | "EnemyChillMagnitude" | "PoisonStacks"
        | "BleedStacks" | "IgniteStacks" => base_name.to_string(),
        // (k3 backlog) Ailment stack-rate rateMod name pass-through (vendor
        // `faster_burn_%`/`faster_poison_%`/`faster_bleed_%`/
        // `damaging_ailments_deal_damage_+%_faster` -> `<Ailment>Faster` INC,
        // SkillStatMap.lua:843-848/:1255/:1479-1483). Consumer =
        // `calc::ailment::ailment_rate_mod` (CalcOffence.lua:5036 rateMod,
        // calcLib.mod's combined INC+MORE set under the same name).
        "BleedFaster" | "PoisonFaster" | "IgniteFaster" => base_name.to_string(),
        // Backlog #9: warcry uptime-machinery pass-through family (vendor
        // `warcry_empowers_per_X_monster_power[_mp_cap]` -> WarcryPowerPer/Cap
        // (SkillStatMap.lua:608-613), Infernal Cry's per-set
        // `infernal_cry_exerted_attack_all_damage_%_to_gain_as_fire_%` ->
        // InfernalExtraFireDamageMultiplier (act_str.lua:7729-7731)). Consumer
        // = `calc::warcry` (empower-count math CalcPerform.lua:2121-2123 +
        // the uptime-scaled DamageGainAsFire injection CalcOffence.lua:3251-3254).
        "WarcryPowerPer" | "WarcryPowerCap" | "InfernalExtraFireDamageMultiplier" => {
            base_name.to_string()
        }
        //  Ailment duration -- vendor's infliction-side mod names carry an
        // Enemy prefix (the debuff duration applied to the enemy,
        // CalcOffence.lua:5037's durationMod reads
        // `Enemy<Ailment>Duration`/`EnemyAilmentDuration`/`DamagingAilmentDuration`),
        // while PoBR's `ailment_duration`/`scale_duration` aggregate under
        // `<Ailment>Duration`/`AilmentDuration` (the same family in
        // mod_parser) -> translation normalizes between the two.
        "EnemyPoisonDuration" => "PoisonDuration".to_string(),
        "EnemyBleedDuration" => "BleedDuration".to_string(),
        "EnemyIgniteDuration" => "IgniteDuration".to_string(),
        "EnemyAilmentDuration" | "DamagingAilmentDuration" => "AilmentDuration".to_string(),
        other => {
            // Conversion / gain-as family (`Skill<From>DamageConvertTo<To>` /
            // `[Skill]<From>DamageGainAs<To>`): PoB2 and PoBR use the same
            // naming, so it passes through by shape.
            if is_conversion_mod_name(other) {
                other.to_string()
            } else {
                return Err(UnsupportedReason::UnknownModName(other.to_string()));
            }
        }
    };
    let (kw, extra_mod_flags) = translate_keyword_flags(keyword_flags, &translated)?;
    let (mod_flags, kw_from_flags) = translate_mod_flags(&remaining_flags)?;
    Ok(TranslatedName {
        flags: mod_flags | extra_mod_flags,
        keyword_flags: kw | kw_from_flags,
        name: translated,
    })
}

/// Validates the shape of a conversion / gain-as ModName:
/// `[Skill]<From>DamageConvertTo<To>` / `[Skill]<From>DamageGainAs<To>`,
/// where `<From>` may be empty (meaning all damage sources) and `<To>` must
/// be a damage type. Matches the names consumed by PoBR `calc::damage`
/// literally.
fn is_conversion_mod_name(name: &str) -> bool {
    let core = name.strip_prefix("Skill").unwrap_or(name);
    for marker in ["DamageConvertTo", "DamageGainAs"] {
        if let Some((from, to)) = core.split_once(marker) {
            let from_ok = from.is_empty() || DAMAGE_TYPES.contains(&from);
            let to_ok = DAMAGE_TYPES.contains(&to);
            return from_ok && to_ok;
        }
    }
    false
}

/// Tag translation (first batch: Condition / ActorCondition(enemy) /
/// Multiplier / PerStat -> PoBR [`ModTag`]).
///
/// Other tag types are Unsupported wholesale; a supported type with **keys
/// outside the convention** is likewise Unsupported (skip rather than
/// miscompute -- an extra key usually carries extra semantics, and silently
/// dropping it would be a miscalculation). Public so the dual-run oracle
/// comparison can use it (vendor's modList tags are compared after going
/// through the same normalizing translation).
pub fn translate_tag(tag: &BTreeMap<String, StatMapValue>) -> Result<ModTag, UnsupportedReason> {
    let tag_type = match tag.get("type") {
        Some(StatMapValue::Text(t)) => t.as_str(),
        _ => return Err(UnsupportedReason::UnsupportedTag("<missing type>".into())),
    };
    let text = |key: &str| match tag.get(key) {
        Some(StatMapValue::Text(v)) => Some(v.clone()),
        _ => None,
    };
    let number = |key: &str| match tag.get(key) {
        Some(StatMapValue::Number(v)) => Some(*v),
        _ => None,
    };
    let keys_subset_of = |allowed: &[&str]| tag.keys().all(|k| allowed.contains(&k.as_str()));
    match tag_type {
        "Condition" => {
            if !keys_subset_of(&["type", "var", "neg"]) {
                return Err(UnsupportedReason::UnsupportedTag(format!(
                    "Condition 含约定外键：{:?}",
                    tag.keys().collect::<Vec<_>>()
                )));
            }
            let Some(var) = text("var") else {
                // Variants like varList aren't supported in the first batch.
                return Err(UnsupportedReason::UnsupportedTag("Condition 缺 var".into()));
            };
            let negated = matches!(tag.get("neg"), Some(StatMapValue::Bool(true)));
            Ok(ModTag::condition(var, negated))
        }
        // Enemy state condition (vendor e.g. `SkillStatMap.lua:1119`
        // `{ type = "ActorCondition", actor = "enemy", var = "Burning" }`):
        // PoBR's existing convention folds enemy conditions into an
        // `Enemy<Var>` condition variable (`mod_parser.rs:950-964`, injected
        // by the orchestration layer via build config, e.g. `EnemyBurning`),
        // so this translates to a same-named Condition tag. actor values
        // other than enemy (player/parent...) aren't supported in the first
        // batch.
        "ActorCondition" => {
            if !keys_subset_of(&["type", "actor", "var", "neg"]) {
                return Err(UnsupportedReason::UnsupportedTag(format!(
                    "ActorCondition 含约定外键：{:?}",
                    tag.keys().collect::<Vec<_>>()
                )));
            }
            if text("actor").as_deref() != Some("enemy") {
                return Err(UnsupportedReason::UnsupportedTag(
                    "ActorCondition 非 enemy actor".into(),
                ));
            }
            let Some(var) = text("var") else {
                return Err(UnsupportedReason::UnsupportedTag(
                    "ActorCondition 缺 var".into(),
                ));
            };
            let negated = matches!(tag.get("neg"), Some(StatMapValue::Bool(true)));
            Ok(ModTag::condition(format!("Enemy{var}"), negated))
        }
        "Multiplier" => {
            if !keys_subset_of(&[
                "type",
                "var",
                "div",
                "limit",
                "limitVar",
                "invert",
                "limitTotal",
            ]) {
                return Err(UnsupportedReason::UnsupportedTag(format!(
                    "Multiplier 含约定外键：{:?}",
                    tag.keys().collect::<Vec<_>>()
                )));
            }
            let Some(var) = text("var") else {
                return Err(UnsupportedReason::UnsupportedTag(
                    "Multiplier 缺 var".into(),
                ));
            };
            // limitVar (vendor ModStore.lua:369's dynamic cap, e.g. Sigil of
            // Power's `SigilOfPowerStage` capped by `SigilOfPowerMaxStages`),
            // invert (:378-380's reciprocal scaling, e.g. Elemental Conflux
            // splitting evenly across the three elements via
            // `ElementalConflux<El>Effect` as 1/N), and limitTotal (:370-371 +
            // 402-404's total cap, e.g. "each poison stack gives +N% damage,
            // up to +M% total") translate directly into ModTag fields.
            Ok(ModTag::Multiplier {
                var,
                div: number("div").unwrap_or(1.0),
                limit: number("limit"),
                actor: None,
                limit_var: text("limitVar"),
                limit_actor: None,
                invert: matches!(tag.get("invert"), Some(StatMapValue::Bool(true))),
                limit_total: matches!(tag.get("limitTotal"), Some(StatMapValue::Bool(true))),
            })
        }
        // Threshold gate (vendor ModStore.lua:429-459): `mult =
        // GetMultiplier(var)`, `threshold = tag.threshold or
        // GetMultiplier(thresholdVar)`, skipped when it falls on the wrong
        // side. A `thresholdVar` shape is only admitted for variables
        // **verified to have zero setters across the whole vendor tree**
        // (GetMultiplier always returns 0 for an unset variable -> the
        // threshold is always 0, so statically folding it is lossless);
        // variables with a setter (e.g. Attrition's `AttritionCullSeconds`)
        // stay Unsupported -- statically folding to 0 would open the gate at
        // the wrong point. actor/thresholdActor/scalar/equals variants are
        // blocked by the keys_subset check.
        "MultiplierThreshold" => {
            if !keys_subset_of(&["type", "var", "threshold", "thresholdVar", "upper"]) {
                return Err(UnsupportedReason::UnsupportedTag(format!(
                    "MultiplierThreshold 含约定外键：{:?}",
                    tag.keys().collect::<Vec<_>>()
                )));
            }
            let Some(var) = text("var") else {
                return Err(UnsupportedReason::UnsupportedTag(
                    "MultiplierThreshold 缺 var".into(),
                ));
            };
            let threshold = match (number("threshold"), text("thresholdVar")) {
                (Some(t), None) => t,
                // Refraction I/II's `RefractionMinimumValour`
                // (sup_str.lua:5978-6024): no setter anywhere in the vendor
                // tree -> always 0 (ValourStacks ≥ 0 always passes the gate,
                // matching vendor's default-config behaviour).
                (None, Some(v)) if v == "RefractionMinimumValour" => 0.0,
                (None, Some(v)) => {
                    return Err(UnsupportedReason::UnsupportedTag(format!(
                        "MultiplierThreshold thresholdVar 非零 setter 未核实：{v}"
                    )));
                }
                _ => {
                    return Err(UnsupportedReason::UnsupportedTag(
                        "MultiplierThreshold 缺 threshold".into(),
                    ));
                }
            };
            Ok(ModTag::MultiplierThreshold {
                var,
                threshold,
                upper: matches!(tag.get("upper"), Some(StatMapValue::Bool(true))),
            })
        }
        "PerStat" => {
            if !keys_subset_of(&["type", "stat", "div", "limit", "limitTotal"]) {
                return Err(UnsupportedReason::UnsupportedTag(format!(
                    "PerStat 含约定外键：{:?}",
                    tag.keys().collect::<Vec<_>>()
                )));
            }
            let Some(stat) = text("stat") else {
                return Err(UnsupportedReason::UnsupportedTag("PerStat 缺 stat".into()));
            };
            // PoB2's PerStat reads an actor output stat; PoBR injects the
            // same-named variable through cfg.multipliers (abbreviations are
            // normalized to PoBR resource names). If the variable was never
            // injected it multiplies by 0 -> contributes 0 (safe undercount).
            let var = match stat.as_str() {
                "Str" => "Strength".to_string(),
                "Dex" => "Dexterity".to_string(),
                "Int" => "Intelligence".to_string(),
                other => other.to_string(),
            };
            // limit / limitTotal (vendor ModStore.lua:461-468 + :402-404;
            // e.g. Atalui's Bloodletting's
            // `PerStat{stat=LifeCost,div=20,limit=40,limitTotal}` -- +1% per
            // 20 life cost, capped at +40% total).
            let mut mtag = ModTag::multiplier(var, number("div").unwrap_or(1.0), number("limit"));
            if let (ModTag::Multiplier { limit_total, .. }, Some(StatMapValue::Bool(true))) =
                (&mut mtag, tag.get("limitTotal"))
            {
                *limit_total = true;
            }
            Ok(mtag)
        }
        // Skill-type qualifier (vendor `{ type = "SkillType", skillType =
        // SkillType.X, [neg = true] }`, e.g. Garukhan's
        // `attacks_roll_crits_twice`'s Attack qualifier, or the Archmage
        // buff's "non-channelling Spells" = Channel neg + Spell) ->
        // [`ModTag::SkillTypes`] / [`ModTag::SkillTypesNeg`] (`Modifier::matches`
        // tests via `cfg.skill_types` intersection / negated intersection).
        // Type names go through the single-source `SkillTypes::from_pob2_name`
        // (the full 290-entry enum table from A1) -- the orchestration
        // layer's `skill_type_bits` already sets every bit (conditions.rs),
        // so the old whitelist limited to Attack/Spell is obsolete. Names
        // outside the enum stay Unsupported.
        "SkillType" => {
            if !keys_subset_of(&["type", "skillType", "neg"]) {
                return Err(UnsupportedReason::UnsupportedTag(format!(
                    "SkillType 含约定外键：{:?}",
                    tag.keys().collect::<Vec<_>>()
                )));
            }
            let name = text("skillType");
            let Some(bits) = name.as_deref().and_then(SkillTypes::from_pob2_name) else {
                return Err(UnsupportedReason::UnsupportedTag(format!(
                    "SkillType 未支持类型：{name:?}"
                )));
            };
            if matches!(tag.get("neg"), Some(StatMapValue::Bool(true))) {
                Ok(ModTag::SkillTypesNeg(bits))
            } else {
                Ok(ModTag::SkillTypes(bits))
            }
        }
        // Distance interpolation (vendor `{ type = "DistanceRamp", ramp =
        // {{d,m},...} }`, e.g. Close Combat's
        // `support_close_combat_attack_damage_+%_final_from_distance`) ->
        // [`ModTag::DistanceRamp`] (linearly interpolated against
        // `enemyDistance` at evaluation time, ModStore.lua:574-590).
        "DistanceRamp" => {
            if !keys_subset_of(&["type", "ramp"]) {
                return Err(UnsupportedReason::UnsupportedTag(format!(
                    "DistanceRamp 含约定外键：{:?}",
                    tag.keys().collect::<Vec<_>>()
                )));
            }
            let Some(StatMapValue::List(points)) = tag.get("ramp") else {
                return Err(UnsupportedReason::UnsupportedTag(
                    "DistanceRamp 缺 ramp 点列".into(),
                ));
            };
            let mut ramp = Vec::with_capacity(points.len());
            for point in points {
                // Each point must be a `[distance, multiplier]` pair.
                let StatMapValue::List(pair) = point else {
                    return Err(UnsupportedReason::UnsupportedTag(
                        "DistanceRamp ramp 点非数组".into(),
                    ));
                };
                let (Some(StatMapValue::Number(d)), Some(StatMapValue::Number(m))) =
                    (pair.first(), pair.get(1))
                else {
                    return Err(UnsupportedReason::UnsupportedTag(
                        "DistanceRamp ramp 点非 [距离,倍率] 数对".into(),
                    ));
                };
                ramp.push((*d, *m));
            }
            if ramp.is_empty() {
                return Err(UnsupportedReason::UnsupportedTag(
                    "DistanceRamp ramp 点列为空".into(),
                ));
            }
            Ok(ModTag::DistanceRamp { ramp })
        }
        other => Err(UnsupportedReason::UnsupportedTag(other.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pobr_data::catalog::stat_map::StatMapEntry;

    /// Convenience constructor: a single-mod entry.
    fn entry_json(json: &str) -> StatMapEntry {
        serde_json::from_str(json).expect("测试条目 JSON 合法")
    }

    fn catalog_json(json: &str) -> StatMapCatalog {
        StatMapCatalog::new(serde_json::from_str(json).expect("测试 catalog JSON 合法"))
    }

    fn expect_modifiers(outcome: MappedOutcome) -> Vec<Modifier> {
        match outcome {
            MappedOutcome::Mapped(items) => items
                .into_iter()
                .map(|item| match item {
                    MappedItem::Modifier(m) => *m,
                    other => panic!("期望 Modifier，得到 {other:?}"),
                })
                .collect(),
            other => panic!("期望 Mapped，得到 {other:?}"),
        }
    }

    // flag constructor whitelist

    /// Whitelisted flag (BifurcateCrit) + SkillType tag -> FLAG modifier +
    /// `ModTag::SkillTypes(ATTACK)` (mirrors Garukhan's
    /// `attacks_roll_crits_twice`, SkillStatMap.lua:1011-1013).
    #[test]
    fn flag_kind_whitelist_translates_with_skill_type_tag() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "flag", "name": "BifurcateCrit", "mod_type": "FLAG",
                 "value": true,
                 "tags": [ { "type": "SkillType", "skillType": "Attack" } ] } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 1.0));
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].name.as_str(), "BifurcateCrit");
        assert_eq!(mods[0].mod_type, ModType::Flag);
        assert!(
            mods[0]
                .tags
                .iter()
                .any(|t| matches!(t, ModTag::SkillTypes(st) if st.contains(SkillTypes::ATTACK)))
        );
    }

    // damage-ailment family whitelist

    /// Mirrors Escalating Poison (sup_dex.lua:2188-2191's
    /// `number_of_additional_poison_stacks` -> `PoisonStacks BASE +
    /// PoisonCanStack flag` injected as a pair): both elements translate
    /// successfully (the flag through the whitelist, the mod through the
    /// pass-through family).
    #[test]
    fn poison_stacks_pair_translates() {
        let entry = entry_json(
            r#"{ "mods": [
                 { "kind": "mod", "name": "PoisonStacks", "mod_type": "BASE" },
                 { "kind": "flag", "name": "PoisonCanStack", "mod_type": "FLAG", "value": true } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 1.0));
        assert_eq!(mods.len(), 2);
        assert_eq!(mods[0].name.as_str(), "PoisonStacks");
        assert_eq!(mods[0].mod_type, ModType::Base);
        assert_eq!(mods[1].name.as_str(), "PoisonCanStack");
        assert_eq!(mods[1].mod_type, ModType::Flag);
    }

    /// Ailment-duration normalization: vendor's `EnemyPoisonDuration` (the
    /// infliction side's debuff duration on the enemy, CalcOffence.lua:5037)
    /// -> PoBR's aggregate name `PoisonDuration` (mirrors Escalating
    /// Poison's `support_multi_poison_poison_duration_+%_final` MORE -20).
    #[test]
    fn enemy_poison_duration_renames_to_pobr_bucket() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "EnemyPoisonDuration", "mod_type": "MORE" } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, -20.0));
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].name.as_str(), "PoisonDuration");
        assert_eq!(mods[0].mod_type, ModType::More);
        assert_eq!(mods[0].value.as_number(), Some(-20.0));
    }

    /// Direct translation of a magnitude mod's keyword scope: Deadly
    /// Poison's `support_deadly_poison_poison_effect_+%_final` ->
    /// `AilmentMagnitude MORE kw=Poison` (sup_dex.lua:1748-1750). Hits on the
    /// consumer side once `ailment_scoped_cfg` sets the bit and the
    /// ANY-overlap check passes.
    #[test]
    fn ailment_magnitude_with_poison_keyword_translates() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "AilmentMagnitude", "mod_type": "MORE",
                 "keyword_flags": ["Poison"] } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 75.0));
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].name.as_str(), "AilmentMagnitude");
        assert!(mods[0].keyword_flags.intersects(KeywordFlags::POISON));
    }

    /// Infliction-chance pass-through: Envenom/Bleed III's
    /// `<Ailment>Chance BASE` (SkillStatMap.lua:1267 / sup_str.lua:932).
    #[test]
    fn ailment_chance_passthrough() {
        for name in ["PoisonChance", "BleedChance", "EnemyIgniteChance"] {
            let entry = entry_json(&format!(
                r#"{{ "mods": [ {{ "kind": "mod", "name": "{name}", "mod_type": "BASE" }} ] }}"#
            ));
            let mods = expect_modifiers(map_entry(&entry, 60.0));
            assert_eq!(mods[0].name.as_str(), name, "{name} 应直通");
        }
    }

    /// (k3) Ailment stack-rate rateMod name pass-through (vendor's
    /// `faster_burn_%` family -> `<Ailment>Faster` INC,
    /// SkillStatMap.lua:843-848; consumer = ailment_rate_mod).
    #[test]
    fn ailment_faster_passthrough() {
        for name in ["BleedFaster", "PoisonFaster", "IgniteFaster"] {
            let entry = entry_json(&format!(
                r#"{{ "mods": [ {{ "kind": "mod", "name": "{name}", "mod_type": "INC" }} ] }}"#
            ));
            let mods = expect_modifiers(map_entry(&entry, 25.0));
            assert_eq!(mods[0].name.as_str(), name, "{name} 应直通");
            assert_eq!(mods[0].mod_type, ModType::Inc);
        }
    }

    ///  The CHANCE bucket maps to Base summation semantics (Rakiata's
    /// Flow's `treat_enemy_resistances_as_negated_…` -> HitsInvertEleResChance,
    /// SkillStatMap.lua:941-944, entry div=100; the consumption-point clamp
    /// lives in `offence::enemy_damage_multiplier`).
    #[test]
    fn chance_mod_type_maps_to_base_for_invert_ele_res() {
        let entry = entry_json(
            r#"{ "div": 100.0, "mods": [ { "kind": "mod",
                 "name": "HitsInvertEleResChance", "mod_type": "CHANCE" } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 100.0));
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].name.as_str(), "HitsInvertEleResChance");
        assert_eq!(mods[0].mod_type, ModType::Base);
        assert_eq!(mods[0].value.as_number(), Some(1.0), "div=100 → 分数");
    }

    /// A flag outside the whitelist is still reported as unknown; a
    /// SkillType name outside the enum skips the whole entry (names inside
    /// the enum are admitted in full via `SkillTypes::from_pob2_name` --
    /// backlog #7-1).
    #[test]
    fn flag_kind_outside_whitelist_or_unknown_skill_type_unsupported() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "flag", "name": "projectile", "mod_type": "FLAG" } ] }"#,
        );
        assert!(matches!(
            map_entry(&entry, 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::UnknownModName(_))
        ));

        let entry = entry_json(
            r#"{ "mods": [ { "kind": "flag", "name": "BifurcateCrit", "mod_type": "FLAG",
                 "tags": [ { "type": "SkillType", "skillType": "NotARealSkillType" } ] } ] }"#,
        );
        assert!(matches!(
            map_entry(&entry, 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::UnsupportedTag(_))
        ));
    }

    /// (Backlog #7-1) SkillType tag: every name inside the enum is admitted
    /// via the single-source `SkillTypes::from_pob2_name`; `neg = true` ->
    /// [`ModTag::SkillTypesNeg`] (vendor ModStore.lua:829-833's negated
    /// match, e.g. Archmage's non-channelling qualifier).
    #[test]
    fn skill_type_tag_full_enum_and_neg_translate() {
        use pobr_data::skill::SkillTypes;
        let mut tag = BTreeMap::new();
        tag.insert("type".into(), StatMapValue::Text("SkillType".into()));
        tag.insert("skillType".into(), StatMapValue::Text("Minion".into()));
        assert_eq!(
            translate_tag(&tag).unwrap(),
            ModTag::SkillTypes(SkillTypes::MINION)
        );
        tag.insert("skillType".into(), StatMapValue::Text("Channel".into()));
        tag.insert("neg".into(), StatMapValue::Bool(true));
        assert_eq!(
            translate_tag(&tag).unwrap(),
            ModTag::SkillTypesNeg(SkillTypes::CHANNEL)
        );
    }

    /// CritChanceCap pass-through (Garukhan's constant stat 50 -> OVERRIDE).
    #[test]
    fn crit_chance_cap_override_passthrough() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "CritChanceCap", "mod_type": "OVERRIDE" } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 50.0));
        assert_eq!(mods[0].name.as_str(), "CritChanceCap");
        assert_eq!(mods[0].mod_type, ModType::Override);
        assert_eq!(mods[0].value.as_number(), Some(50.0));
    }

    // full coverage of the merge formula's four parameters

    /// No parameters: injected value = the stat value.
    #[test]
    fn merge_defaults_to_stat_value() {
        let entry =
            entry_json(r#"{ "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "INC" } ] }"#);
        let mods = expect_modifiers(map_entry(&entry, 42.0));
        assert_eq!(mods[0].name.as_str(), "Damage");
        assert_eq!(mods[0].mod_type, ModType::Inc);
        assert_eq!(mods[0].value.as_number(), Some(42.0));
    }

    /// div: the total_cast_time_+_ms shape (1000ms -> 1.0s).
    #[test]
    fn merge_div_scales_down() {
        let entry = entry_json(
            r#"{ "div": 1000.0,
                 "mods": [ { "kind": "mod", "name": "TotalCastTime", "mod_type": "BASE" } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 1000.0));
        assert_eq!(mods[0].name.as_str(), "TotalCastTime");
        assert_eq!(mods[0].value.as_number(), Some(1.0));
    }

    /// mult + base: injected value = stat × mult + base.
    #[test]
    fn merge_mult_and_base() {
        let entry = entry_json(
            r#"{ "mult": 2.0, "base": 5.0,
                 "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "INC" } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 10.0));
        assert_eq!(mods[0].value.as_number(), Some(25.0));
    }

    /// value: a constant override that ignores the stat value (the
    /// global_bleed_on_hit = 100 shape).
    #[test]
    fn merge_value_overrides_stat() {
        let entry = entry_json(
            r#"{ "value": 100.0,
                 "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "BASE" } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 7.0));
        assert_eq!(mods[0].value.as_number(), Some(100.0));
    }

    /// All four parameters combined: value takes highest priority (vendor's
    /// `map.value or …` short-circuits).
    #[test]
    fn merge_value_wins_over_other_params() {
        let entry = entry_json(
            r#"{ "value": 3.0, "div": 2.0, "mult": 10.0, "base": 99.0,
                 "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "INC" } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 8.0));
        assert_eq!(mods[0].value.as_number(), Some(3.0));
    }

    /// group: nested mods use group-level parameters (CalcActiveSkill.lua:117);
    /// entry-level parameters don't leak in.
    #[test]
    fn group_params_apply_to_nested_mods() {
        let entry = entry_json(
            r#"{ "div": 7.0,
                 "mods": [ { "kind": "group", "div": 2.0, "mods": [
                     { "kind": "mod", "name": "FireDamage", "mod_type": "MORE" },
                     { "kind": "mod", "name": "ColdDamage", "mod_type": "MORE" } ] } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 10.0));
        assert_eq!(mods.len(), 2);
        assert_eq!(mods[0].value.as_number(), Some(5.0)); // 10/2, not 10/7
        assert_eq!(mods[1].name.as_str(), "ColdDamage");
    }

    // scalar / distorted extraction / unknown name

    /// An element-level scalar (on an entry) skips the whole entry as
    /// Unsupported.
    #[test]
    fn scalar_entry_is_unsupported() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "group", "scalar": "ConsumedPowerChargeEffect", "mods": [
                     { "kind": "mod", "name": "Damage", "mod_type": "MORE" } ] } ] }"#,
        );
        assert_eq!(
            map_entry(&entry, 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::ScalarMultiplier)
        );
    }

    /// A distorted-extraction entry -> Unsupported(Unextractable).
    #[test]
    fn unextractable_entry_is_unsupported() {
        let entry = entry_json(r#"{ "_unextractable": true }"#);
        assert_eq!(
            map_entry(&entry, 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::Unextractable)
        );
    }

    /// An unknown ModName is reported as Unsupported(UnknownModName).
    #[test]
    fn unknown_mod_name_is_reported() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "WeaponRangeMetre", "mod_type": "BASE" } ] }"#,
        );
        assert_eq!(
            map_entry(&entry, 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::UnknownModName(
                "WeaponRangeMetre".into()
            ))
        );
    }

    /// A missing mod_type (a vendor typo entry) -> Unsupported(MissingModType).
    #[test]
    fn missing_mod_type_is_unsupported() {
        let entry = entry_json(r#"{ "mods": [ { "kind": "mod", "name": "Damage" } ] }"#);
        assert_eq!(
            map_entry(&entry, 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::MissingModType)
        );
    }

    /// Any unsupported element skips the whole entry (never a half injection).
    #[test]
    fn one_bad_element_rejects_whole_entry() {
        let entry = entry_json(
            r#"{ "mods": [
                 { "kind": "mod", "name": "Damage", "mod_type": "INC" },
                 { "kind": "mod", "name": "SomethingNovel", "mod_type": "INC" } ] }"#,
        );
        assert!(matches!(
            map_entry(&entry, 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::UnknownModName(_))
        ));
    }

    // name translation / flag dispatch

    /// Speed dispatches on ModFlag: Attack -> AttackSpeed / Cast -> CastSpeed
    /// / bare -> SkillSpeed.
    #[test]
    fn speed_dispatches_on_flags() {
        for (flags, expect) in [
            (r#"["Attack"]"#, "AttackSpeed"),
            (r#"["Cast"]"#, "CastSpeed"),
            (r#"[]"#, "SkillSpeed"),
        ] {
            let entry = entry_json(&format!(
                r#"{{ "mods": [ {{ "kind": "mod", "name": "Speed", "mod_type": "INC", "flags": {flags} }} ] }}"#
            ));
            let mods = expect_modifiers(map_entry(&entry, 15.0));
            assert_eq!(mods[0].name.as_str(), expect);
        }
        // An unknown flag combination -> Unsupported.
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "Speed", "mod_type": "INC", "flags": ["Warcry"] } ] }"#,
        );
        assert!(matches!(
            map_entry(&entry, 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::UnsupportedFlags(_))
        ));
    }

    /// Damage dispatches on ModFlag; CritChance/CritMultiplier translate
    /// directly.
    #[test]
    fn damage_and_crit_translation() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "INC", "flags": ["Attack"] } ] }"#,
        );
        assert_eq!(
            expect_modifiers(map_entry(&entry, 1.0))[0].name.as_str(),
            "AttackDamage"
        );
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "CritChance", "mod_type": "MORE" } ] }"#,
        );
        assert_eq!(
            expect_modifiers(map_entry(&entry, 1.0))[0].name.as_str(),
            "CriticalStrikeChance"
        );
    }

    /// Base-damage family (mod shape, with KeywordFlag.Spell): the flag is
    /// dropped and `<Type>DamageMin` translates directly.
    #[test]
    fn damage_bound_mod_drops_scoping_flags() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "PhysicalMin", "mod_type": "BASE",
                             "keyword_flags": ["Spell"] } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 12.0));
        assert_eq!(mods[0].name.as_str(), "PhysicalDamageMin");
        assert_eq!(mods[0].value.as_number(), Some(12.0));
        // A keyword flag that can't be dropped (e.g. Warcry) -> Unsupported.
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "PhysicalMin", "mod_type": "BASE",
                             "keyword_flags": ["Warcry"] } ] }"#,
        );
        assert!(matches!(
            map_entry(&entry, 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::UnsupportedKeywordFlags(_))
        ));
    }

    /// Direct ModFlag translation: the ported subset attaches to the
    /// Modifier (matching semantics agree with PoB2's subset check on both
    /// sides). Mirrors vendor `sup_str.lua`'s Melee Physical Damage statMap:
    /// `mod("PhysicalDamage","MORE",nil,ModFlag.Melee)`.
    #[test]
    fn supported_mod_flags_are_attached() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "PhysicalDamage", "mod_type": "MORE", "flags": ["Melee"] } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 25.0));
        assert_eq!(mods[0].name.as_str(), "PhysicalDamage");
        assert_eq!(mods[0].flags, ModFlags::MELEE);
        // Vendor's `ModFlag.Hit` routes to PoBR's keyword HIT channel
        // (hit-scoping goes through KeywordFlag; cfg.flags never sets
        // ModFlags::HIT): the mod is produced, ModFlags is empty, and the
        // keyword contains HIT.
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "PhysicalDamage", "mod_type": "MORE", "flags": ["Hit"] } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 25.0));
        assert_eq!(mods[0].name.as_str(), "PhysicalDamage");
        assert_eq!(mods[0].flags, ModFlags::NONE);
        assert_eq!(mods[0].keyword_flags, KeywordFlags::HIT);
    }

    /// gain-as + ModFlag.Attack (mirrors vendor `SkillStatMap.lua:1116`'s
    /// `non_skill_base_all_damage_%_to_gain_as_chaos_with_attacks`): the name
    /// passes through and the ATTACK flag attaches (applies under an attack
    /// cfg, not a spell -- matching PoB2's scoping).
    #[test]
    fn gain_as_with_attack_flag_translates() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "DamageGainAsChaos", "mod_type": "BASE", "flags": ["Attack"] } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 30.0));
        assert_eq!(mods[0].name.as_str(), "DamageGainAsChaos");
        assert_eq!(mods[0].flags, ModFlags::ATTACK);
        assert_eq!(mods[0].value.as_number(), Some(30.0));
    }

    /// After Speed's dispatch consumes Attack, the remaining flag still
    /// translates directly as usual (Attack+Melee -> AttackSpeed+MELEE).
    #[test]
    fn speed_dispatch_keeps_remaining_flags() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "Speed", "mod_type": "INC", "flags": ["Attack", "Melee"] } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 10.0));
        assert_eq!(mods[0].name.as_str(), "AttackSpeed");
        assert_eq!(mods[0].flags, ModFlags::MELEE);
    }

    /// The `skill_speed_+%` entry's full set of three mods (vendor
    /// `SkillStatMap.lua:554-557`): Speed -> SkillSpeed;
    /// WarcrySpeed/TotemPlacementSpeed pass through as inert names
    /// (WarcrySpeed's redundant KeywordFlag.Warcry is safely dropped).
    #[test]
    fn skill_speed_entry_maps_all_three_mods() {
        let entry = entry_json(
            r#"{ "mods": [
                 { "kind": "mod", "name": "Speed", "mod_type": "INC" },
                 { "kind": "mod", "name": "WarcrySpeed", "mod_type": "INC", "keyword_flags": ["Warcry"] },
                 { "kind": "mod", "name": "TotemPlacementSpeed", "mod_type": "INC" } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 20.0));
        assert_eq!(
            mods.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(),
            vec!["SkillSpeed", "WarcrySpeed", "TotemPlacementSpeed"]
        );
        assert!(mods.iter().all(|m| m.value.as_number() == Some(20.0)));
        // A Warcry keyword on a non-inert name is still rejected (no
        // corresponding KeywordFlags bit).
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "INC", "keyword_flags": ["Warcry"] } ] }"#,
        );
        assert!(matches!(
            map_entry(&entry, 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::UnsupportedKeywordFlags(_))
        ));
    }

    /// A ported KeywordFlag bit attaches via direct translation (e.g.
    /// KeywordFlag.Bleed).
    #[test]
    fn ported_keyword_flags_are_attached() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "INC", "keyword_flags": ["Bleed"] } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 15.0));
        assert_eq!(mods[0].keyword_flags, KeywordFlags::BLEED);
    }

    /// KeywordFlag.Attack/Spell -> the equivalent ModFlags gate (mirrors
    /// vendor `sup_str.lua:2825-2827`'s Elemental Armament:
    /// `mod("ElementalDamage","MORE",nil,0,KeywordFlag.Attack)`; PoB2's ANY
    /// keyword match and PoBR's cfg.flags ATTACK subset match both mean
    /// "only applies to attack skills").
    #[test]
    fn attack_spell_keywords_become_mod_flags() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "ElementalDamage", "mod_type": "MORE", "keyword_flags": ["Attack"] } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 25.0));
        assert_eq!(mods[0].name.as_str(), "ElementalDamage");
        assert_eq!(mods[0].flags, ModFlags::ATTACK);
        assert_eq!(mods[0].keyword_flags, KeywordFlags::NONE);
    }

    /// ActorCondition(enemy) -> `Enemy<Var>` Condition (mirrors vendor
    /// `SkillStatMap.lua:1119` plus PoBR `mod_parser.rs:950-964`'s enemy
    /// condition naming convention).
    #[test]
    fn enemy_actor_condition_translates() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "DamageGainAsFire", "mod_type": "BASE",
                 "tags": [ { "type": "ActorCondition", "actor": "enemy", "var": "Burning" } ] } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 40.0));
        assert_eq!(mods[0].tags, vec![ModTag::condition("EnemyBurning", false)]);
        // An actor other than enemy -> Unsupported.
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "INC",
                 "tags": [ { "type": "ActorCondition", "actor": "parent", "var": "Stationary" } ] } ] }"#,
        );
        assert!(matches!(
            map_entry(&entry, 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::UnsupportedTag(_))
        ));
    }

    /// set_key=None automatically uses the default set "1" override (PoB2
    /// defaults statSetIndex to 1, `SkillsTab.lua:354` /
    /// `CalcActiveSkill.lua:166-171`).
    #[test]
    fn none_set_key_uses_default_set_one() {
        let catalog = catalog_json(
            r#"{
              "global": {
                "damage_+%": { "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "INC" } ] }
              },
              "per_stat_set": {
                "FooPlayer": { "1": {
                  "damage_+%": { "mods": [ { "kind": "mod", "name": "FireDamage", "mod_type": "INC" } ] }
                } },
                "BarPlayer": { "2": {
                  "damage_+%": { "mods": [ { "kind": "mod", "name": "ColdDamage", "mod_type": "INC" } ] }
                } }
              }
            }"#,
        );
        // Hits the default set "1" override.
        let outcome = map_stat(&catalog, "FooPlayer", None, "damage_+%", 10.0);
        assert_eq!(expect_modifiers(outcome)[0].name.as_str(), "FireDamage");
        // A set override other than "1" isn't selected by default (needs an
        // explicit set_key, wired up with T5).
        let outcome = map_stat(&catalog, "BarPlayer", None, "damage_+%", 10.0);
        assert_eq!(expect_modifiers(outcome)[0].name.as_str(), "Damage");
        let outcome = map_stat(&catalog, "BarPlayer", Some("2"), "damage_+%", 10.0);
        assert_eq!(expect_modifiers(outcome)[0].name.as_str(), "ColdDamage");
    }

    /// Conversion / gain-as names pass through; an invalid type word doesn't.
    #[test]
    fn conversion_names_pass_through() {
        for name in [
            "SkillPhysicalDamageConvertToFire",
            "SkillDamageGainAsChaos",
            "PhysicalDamageGainAsCold",
        ] {
            let entry = entry_json(&format!(
                r#"{{ "mods": [ {{ "kind": "mod", "name": "{name}", "mod_type": "BASE" }} ] }}"#
            ));
            assert_eq!(
                expect_modifiers(map_entry(&entry, 30.0))[0].name.as_str(),
                name
            );
        }
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "SkillFooDamageConvertToBar", "mod_type": "BASE" } ] }"#,
        );
        assert!(matches!(
            map_entry(&entry, 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::UnknownModName(_))
        ));
    }

    // tag first batch

    /// Condition tag -> ModTag::Condition (including neg).
    #[test]
    fn condition_tag_translates() {
        let entry = entry_json(
            r#"{ "value": 100.0, "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "MORE",
                 "tags": [ { "type": "Condition", "var": "Leeching", "neg": true } ] } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 1.0));
        assert_eq!(mods[0].tags, vec![ModTag::condition("Leeching", true)]);
    }

    /// Multiplier / PerStat tag -> ModTag::Multiplier (PerStat abbreviations
    /// are normalized).
    #[test]
    fn multiplier_and_per_stat_tags_translate() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "INC",
                 "tags": [ { "type": "Multiplier", "var": "PowerCharge", "limit": 5.0 } ] } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 1.0));
        assert_eq!(
            mods[0].tags,
            vec![ModTag::multiplier("PowerCharge", 1.0, Some(5.0))]
        );
        let entry =
            entry_json(r#"{ "mods": [ { "kind": "skill_data", "value": { "key": "Damage" } } ] }"#);
        // (The line above just constructs a legal-JSON counterexample --
        // skill_data's Damage key isn't on the whitelist.)
        assert!(matches!(
            map_entry(&entry, 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::UnsupportedSkillDataKey(_))
        ));
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "INC",
                 "tags": [ { "type": "PerStat", "stat": "Int", "div": 10.0 } ] } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 1.0));
        assert_eq!(
            mods[0].tags,
            vec![ModTag::multiplier("Intelligence", 10.0, None)]
        );
    }

    /// A tag outside the first batch (GlobalEffect) skips the whole entry as
    /// Unsupported.
    #[test]
    fn unsupported_tag_types_reject_entry() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "MORE",
                 "tags": [ { "type": "GlobalEffect", "effectType": "Buff" } ] } ] }"#,
        );
        assert!(matches!(
            map_entry(&entry, 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::UnsupportedTag(_))
        ));
    }

    /// `DistanceRamp` tag (Close Combat's `..._final_from_distance`) ->
    /// [`ModTag::DistanceRamp`], with the ramp point list translated verbatim
    /// (vendor ModStore.lua:574-590).
    #[test]
    fn distance_ramp_tag_translates() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "MORE",
                 "tags": [ { "type": "DistanceRamp", "ramp": [[10,1],[35,0]] } ] } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 30.0));
        assert_eq!(
            mods[0].tags,
            vec![ModTag::DistanceRamp {
                ramp: vec![(10.0, 1.0), (35.0, 0.0)],
            }]
        );
    }

    /// DistanceRamp with a key outside the convention skips the whole entry
    /// as Unsupported (an extra key usually carries extra semantics).
    #[test]
    fn distance_ramp_rejects_extra_keys() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "MORE",
                 "tags": [ { "type": "DistanceRamp", "ramp": [[10,1]], "var": "x" } ] } ] }"#,
        );
        assert!(matches!(
            map_entry(&entry, 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::UnsupportedTag(_))
        ));
    }

    /// `Multiplier{limitTotal}` (vendor ModStore.lua:370-371 + 402-404's total
    /// cap): `limit` doesn't truncate the multiplier count, it caps the
    /// final contribution after `value × mult`. Shaped like "each poison
    /// stack gives +15% damage, up to +75% total" (var=PoisonStacks,
    /// limit=75, limitTotal).
    #[test]
    fn multiplier_limit_total_caps_final_contribution() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "INC",
                 "tags": [ { "type": "Multiplier", "var": "PoisonStacks", "limit": 75.0,
                             "limitTotal": true } ] } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 15.0));
        assert_eq!(
            mods[0].tags,
            vec![ModTag::Multiplier {
                var: "PoisonStacks".into(),
                div: 1.0,
                limit: Some(75.0),
                actor: None,
                limit_var: None,
                limit_actor: None,
                invert: false,
                limit_total: true,
            }]
        );
        // 3 stacks: 15×3 = 45 ≤ 75 -> 45 (cap not hit).
        let cfg3 = crate::CalcConfig::new().with_multiplier("PoisonStacks", 3.0);
        assert_eq!(mods[0].effective_number(&cfg3), Some(45.0));
        // 8 stacks: 15×8 = 120 -> capped to the 75 total (capping the count
        // instead would give 15×min(8,75)=120, a miscalculation).
        let cfg8 = crate::CalcConfig::new().with_multiplier("PoisonStacks", 8.0);
        assert_eq!(mods[0].effective_number(&cfg8), Some(75.0));
    }

    // skill_data / flag constructors

    /// A skill_data base-damage key -> a `<Type>DamageMin/Max` BASE modifier.
    #[test]
    fn skill_data_damage_bounds_become_modifiers() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "skill_data", "value": { "key": "FireMin" } } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 19.0));
        assert_eq!(mods[0].name.as_str(), "FireDamageMin");
        assert_eq!(mods[0].mod_type, ModType::Base);
        assert_eq!(mods[0].value.as_number(), Some(19.0));
    }

    /// skill_data duration -> a SkillData item (entry div=1000 converts ms to s).
    #[test]
    fn skill_data_duration_emits_skill_data() {
        let entry = entry_json(
            r#"{ "div": 1000.0,
                 "mods": [ { "kind": "skill_data", "value": { "key": "duration" } } ] }"#,
        );
        match map_entry(&entry, 4000.0) {
            MappedOutcome::Mapped(items) => assert_eq!(
                items,
                vec![MappedItem::SkillData {
                    key: "duration".into(),
                    value: 4.0
                }]
            ),
            other => panic!("期望 Mapped，得到 {other:?}"),
        }
    }

    /// A flag constructor (a skill behaviour switch) with no consumer in the
    /// first batch is reported as Unsupported.
    #[test]
    fn flag_ctor_is_unsupported_in_first_batch() {
        let entry = entry_json(r#"{ "mods": [ { "kind": "flag", "name": "projectile" } ] }"#);
        assert_eq!(
            map_entry(&entry, 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::UnknownModName("flag:projectile".into()))
        );
    }

    /// An entry with skillFlag (consumed by the statSet flags path) ->
    /// Unsupported(SkillFlag).
    #[test]
    fn entry_skill_flag_is_unsupported() {
        let entry = entry_json(r#"{ "skill_flag": "arrow" }"#);
        assert_eq!(
            map_entry(&entry, 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::SkillFlag("arrow".into()))
        );
    }

    /// A FLAG-typed mod (vendor `mod(…, "FLAG", …)`) -> Modifier::flag (Lua
    /// truthiness semantics).
    #[test]
    fn flag_typed_mod_becomes_bool_modifier() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "FLAG" } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 0.0));
        assert_eq!(mods[0].mod_type, ModType::Flag);
        assert_eq!(mods[0].value.as_bool(), Some(true));
    }

    // catalog lookup semantics

    /// A per-set override wins; a miss falls back to global; a miss on both
    /// -> Unknown.
    #[test]
    fn per_set_overrides_global_then_falls_back() {
        let catalog = catalog_json(
            r#"{
              "global": {
                "damage_+%": { "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "INC" } ] }
              },
              "per_stat_set": {
                "IceNovaPlayer": { "2": {
                  "damage_+%": { "mods": [ { "kind": "mod", "name": "ColdDamage", "mod_type": "INC" } ] }
                } }
              }
            }"#,
        );
        // Hits the per-set override.
        let outcome = map_stat(&catalog, "IceNovaPlayer", Some("2"), "damage_+%", 10.0);
        assert_eq!(expect_modifiers(outcome)[0].name.as_str(), "ColdDamage");
        // A set miss falls back to global.
        let outcome = map_stat(&catalog, "IceNovaPlayer", Some("1"), "damage_+%", 10.0);
        assert_eq!(expect_modifiers(outcome)[0].name.as_str(), "Damage");
        // No set context -> global.
        let outcome = map_stat(&catalog, "Other", None, "damage_+%", 10.0);
        assert_eq!(expect_modifiers(outcome)[0].name.as_str(), "Damage");
        // A miss on both -> Unknown.
        assert_eq!(
            map_stat(&catalog, "Other", None, "nonexistent_stat", 1.0),
            MappedOutcome::Unknown
        );
    }

    /// (Backlog #7-1) Reading the curse skill-local effect multiplier zone:
    /// a bare `CurseEffect` INC/MORE counts, while elements with a
    /// GlobalEffect / other tag, and unrelated stats, contribute zero.
    #[test]
    fn curse_local_effect_collects_bare_curse_effect_only() {
        let catalog = catalog_json(
            r#"{
              "global": {
                "curse_effect_+%": { "mods": [ { "kind": "mod", "name": "CurseEffect", "mod_type": "INC" } ] },
                "support_atziri_curse_effect_+%_final": { "mods": [ { "kind": "mod", "name": "CurseEffect", "mod_type": "MORE" } ] },
                "mark_effect_+%": { "mods": [ { "kind": "mod", "name": "CurseEffect", "mod_type": "INC",
                    "tags": [ { "type": "SkillType", "skillType": "Mark" } ] } ] }
              },
              "per_stat_set": {}
            }"#,
        );
        assert_eq!(
            curse_local_effect(&catalog, "X", None, "curse_effect_+%", 25.0),
            (25.0, 1.0)
        );
        assert_eq!(
            curse_local_effect(
                &catalog,
                "X",
                None,
                "support_atziri_curse_effect_+%_final",
                -20.0
            ),
            (0.0, 0.8)
        );
        // A tagged variant (Mark-gated) is conservatively not counted.
        assert_eq!(
            curse_local_effect(&catalog, "X", None, "mark_effect_+%", 30.0),
            (0.0, 1.0)
        );
        // An unrelated stat / a missing entry -> zero contribution.
        assert_eq!(
            curse_local_effect(&catalog, "X", None, "nope", 1.0),
            (0.0, 1.0)
        );
    }

    //  isGlobalEffect / global-only merge

    /// Equivalent to isGlobalEffect (CalcActiveSkill.lua:68-80): a single mod
    /// checks its own tags; a hit on any group member makes the group
    /// global; flag/skill_data shapes are judged by tags the same way.
    #[test]
    fn is_global_effect_matches_vendor_predicate() {
        let m = |json: &str| -> StatMapMod { serde_json::from_str(json).expect("mod JSON 合法") };
        // A single mod: with / without the GlobalEffect tag.
        assert!(is_global_effect(&m(
            r#"{ "kind": "mod", "name": "Damage", "mod_type": "INC",
                 "tags": [ { "type": "GlobalEffect", "effectType": "Buff" } ] }"#
        )));
        assert!(!is_global_effect(&m(
            r#"{ "kind": "mod", "name": "Damage", "mod_type": "INC",
                 "tags": [ { "type": "Condition", "var": "Leeching" } ] }"#
        )));
        assert!(!is_global_effect(&m(
            r#"{ "kind": "mod", "name": "Damage", "mod_type": "INC" }"#
        )));
        // group: a hit on any member -> the whole group is global; no
        // member hit -> not global.
        assert!(is_global_effect(&m(r#"{ "kind": "group", "mods": [
                 { "kind": "mod", "name": "Damage", "mod_type": "INC" },
                 { "kind": "mod", "name": "CastSpeed", "mod_type": "INC",
                   "tags": [ { "type": "GlobalEffect", "effectType": "Buff" } ] } ] }"#)));
        assert!(!is_global_effect(&m(r#"{ "kind": "group", "mods": [
                 { "kind": "mod", "name": "Damage", "mod_type": "INC" },
                 { "kind": "mod", "name": "ColdDamage", "mod_type": "MORE" } ] }"#)));
        // skill_data shape (vendor's skill() constructor can likewise carry
        // a GlobalEffect tag).
        assert!(is_global_effect(&m(
            r#"{ "kind": "skill_data", "value": { "key": "duration" },
                 "tags": [ { "type": "GlobalEffect", "effectType": "Buff" } ] }"#
        )));
    }

    /// global-only dual-set case (mirrors `CalcActiveSkill.lua:124-140`): the
    /// selected set does a full merge plus global bookkeeping; the
    /// unselected set only lets global elements participate, silently
    /// skipping non-global ones (Mapped empty), and a stat already accounted
    /// for is skipped wholesale by the caller.
    #[test]
    fn global_only_merge_dual_set_semantics() {
        // A synthetic dual-set catalog (mirrors the DemonForm shape: global =
        // an INC with a GlobalEffect Buff tag, see other.lua:4384-4386; local
        // = an ordinary per-level stat):
        // set "1" (selected): alpha (a mixed global+local entry), beta (pure
        // local);
        // set "2" (unselected): alpha (a per-set override, global), beta
        // (pure local), gamma (global, visible only through the global
        // table).
        let catalog = catalog_json(
            r#"{
              "global": {
                "alpha": { "mods": [
                    { "kind": "mod", "name": "Damage", "mod_type": "INC" },
                    { "kind": "mod", "name": "CastSpeed", "mod_type": "INC",
                      "tags": [ { "type": "GlobalEffect", "effectType": "Buff" } ] } ] },
                "beta": { "mods": [ { "kind": "mod", "name": "ColdDamage", "mod_type": "INC" } ] },
                "gamma": { "mods": [ { "kind": "mod", "name": "AttackSpeed", "mod_type": "INC",
                      "tags": [ { "type": "GlobalEffect", "effectType": "Buff" } ] } ] }
              },
              "per_stat_set": {
                "Foo": { "2": {
                  "alpha": { "mods": [ { "kind": "mod", "name": "SkillSpeed", "mod_type": "INC",
                      "tags": [ { "type": "GlobalEffect", "effectType": "Buff" } ] } ] }
                } }
              }
            }"#,
        );
        // (1) Bookkeeping when merging the selected set "1" (:104-106): alpha
        //     has a global element -> accounted for; beta is pure local ->
        //     not accounted for.
        assert!(stat_has_global_mods(&catalog, "Foo", Some("1"), "alpha"));
        assert!(!stat_has_global_mods(&catalog, "Foo", Some("1"), "beta"));
        // From the unselected set "2"'s viewpoint: alpha goes through the
        // per-set override chain and is likewise global.
        assert!(stat_has_global_mods(&catalog, "Foo", Some("2"), "alpha"));
        // (2) Global-only on the unselected set: a pure-local entry ->
        //     silently injects nothing (Mapped empty, not Unsupported --
        //     vendor's :107 simply doesn't collect it, which isn't the same
        //     as "unsupported").
        assert_eq!(
            map_stat_global_only(&catalog, "Foo", Some("2"), "beta", 10.0),
            MappedOutcome::Mapped(Vec::new())
        );
        // (3) Global elements are still kept for translation -- the
        //     GlobalEffect tag itself is still outside the first batch's
        //     translation boundary (the buff domain), so the whole entry is
        //     reported Unsupported and injects nothing (skip rather than
        //     miscompute).
        for stat in ["alpha", "gamma"] {
            assert!(
                matches!(
                    map_stat_global_only(&catalog, "Foo", Some("2"), stat, 10.0),
                    MappedOutcome::Unsupported(UnsupportedReason::UnsupportedTag(_))
                ),
                "global 元素应整条上报（M3 前注入为零）：{stat}"
            );
        }
        // (4) The caller's bookkeeping-skip semantics: alpha was already
        //     accounted for as global on the selected set, so the unselected
        //     set never calls map_stat_global_only for it again (the
        //     stat-level equivalent of vendor's selectedGlobalStats -- see
        //     stat_has_global_mods's doc). The bookkeeping probe plus
        //     global-only together implement all of :107's condition.
        // (5) A miss on both -> Unknown.
        assert_eq!(
            map_stat_global_only(&catalog, "Foo", Some("2"), "nonexistent", 1.0),
            MappedOutcome::Unknown
        );
    }

    /// Global-only's element granularity and skill_flag behaviour: a group
    /// is retained as a whole when global (including members without the
    /// tag -- vendor's :103 granularity is the modOrGroup); a skill_flag
    /// entry has no modOrGroup, so it naturally comes out Mapped empty under
    /// global-only (flags only take effect on the selected set).
    #[test]
    fn global_only_group_granularity_and_skill_flag() {
        let entry: StatMapEntry = serde_json::from_str(
            r#"{ "mods": [
                 { "kind": "group", "mods": [
                     { "kind": "mod", "name": "Damage", "mod_type": "INC" },
                     { "kind": "mod", "name": "CastSpeed", "mod_type": "INC",
                       "tags": [ { "type": "GlobalEffect", "effectType": "Buff" } ] } ] },
                 { "kind": "mod", "name": "ColdDamage", "mod_type": "MORE" } ] }"#,
        )
        .expect("条目 JSON 合法");
        // A global hit on any group member -> the whole group is retained;
        // a sibling ordinary mod is unaffected.
        assert!(is_global_effect(&entry.mods[0]));
        assert!(!is_global_effect(&entry.mods[1]));
        // A skill_flag entry has no modOrGroup -> Mapped empty under
        // global-only.
        let catalog =
            catalog_json(r#"{ "global": { "skill_can_fire_arrows": { "skill_flag": "arrow" } } }"#);
        assert_eq!(
            map_stat_global_only(&catalog, "Any", None, "skill_can_fire_arrows", 1.0),
            MappedOutcome::Mapped(Vec::new())
        );
        // A distorted-extraction entry is reported as Unsupported (content
        // unknown, visibility comes first; injects nothing either way).
        let catalog = catalog_json(r#"{ "global": { "broken": { "_unextractable": true } } }"#);
        assert!(matches!(
            map_stat_global_only(&catalog, "Any", None, "broken", 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::Unextractable)
        ));
    }

    //  curse domain enemy-side mapping

    /// Mirrors Despair's shape (a per-set `ChaosResist` BASE with a
    /// GlobalEffect Curse tag): the enemy-side name passes through,
    /// GlobalEffect is stripped, and the merged value is the raw stat value
    /// (negative = a resistance reduction).
    #[test]
    fn curse_chaos_resist_maps_to_enemy_name() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "DespairPlayer": { "1": {
                 "base_skill_buff_chaos_damage_resistance_%_to_apply": {
                   "mods": [ { "kind": "mod", "name": "ChaosResist", "mod_type": "BASE",
                               "tags": [ { "type": "GlobalEffect", "effectType": "Curse" } ] } ] } } } } }"#,
        );
        let MappedOutcome::Mapped(items) = map_curse_stat(
            &catalog,
            "DespairPlayer",
            None,
            "base_skill_buff_chaos_damage_resistance_%_to_apply",
            -35.0,
        ) else {
            panic!("期望 Mapped");
        };
        assert_eq!(items.len(), 1);
        let MappedItem::Modifier(m) = &items[0] else {
            panic!("期望 Modifier");
        };
        assert_eq!(m.name.as_str(), "ChaosResist");
        assert_eq!(m.mod_type, ModType::Base);
        assert_eq!(m.value.as_number(), Some(-35.0));
        assert!(m.tags.is_empty(), "GlobalEffect 路由 tag 已剥除");
    }

    /// Mirrors Elemental Weakness's shape: `ElementalResist` expands to the
    /// three fire/cold/lightning lines (pobr's enemy-side aggregation only
    /// reads `<Type>Resist`, equivalent to vendor collecting both names).
    #[test]
    fn curse_elemental_resist_expands_to_three_types() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "ElementalWeaknessPlayer": { "1": {
                 "base_skill_buff_all_elements_resistance_%_to_apply": {
                   "mods": [ { "kind": "mod", "name": "ElementalResist", "mod_type": "BASE",
                               "tags": [ { "type": "GlobalEffect", "effectType": "Curse" } ] } ] } } } } }"#,
        );
        let MappedOutcome::Mapped(items) = map_curse_stat(
            &catalog,
            "ElementalWeaknessPlayer",
            None,
            "base_skill_buff_all_elements_resistance_%_to_apply",
            -59.0,
        ) else {
            panic!("期望 Mapped");
        };
        let names: Vec<&str> = items
            .iter()
            .map(|i| match i {
                MappedItem::Modifier(m) => m.name.as_str(),
                other => panic!("期望 Modifier，得到 {other:?}"),
            })
            .collect();
        assert_eq!(names, vec!["FireResist", "ColdResist", "LightningResist"]);
    }

    /// Mirrors Enfeeble's shape: `Damage` MORE + Condition(Unique[, neg])
    /// translates directly and is kept (a curse mod lands in the enemy db,
    /// so the var is the enemy's own state, with no Enemy prefix added).
    #[test]
    fn curse_damage_more_keeps_condition_tags() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "EnfeeblePlayer": { "1": {
                 "base_skill_buff_damage_+%_final_to_apply": {
                   "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "MORE",
                               "tags": [ { "type": "GlobalEffect", "effectType": "Curse" },
                                         { "type": "Condition", "var": "Unique", "neg": true } ] } ] } } } } }"#,
        );
        let MappedOutcome::Mapped(items) = map_curse_stat(
            &catalog,
            "EnfeeblePlayer",
            None,
            "base_skill_buff_damage_+%_final_to_apply",
            -21.0,
        ) else {
            panic!("期望 Mapped");
        };
        let MappedItem::Modifier(m) = &items[0] else {
            panic!("期望 Modifier");
        };
        assert_eq!(m.name.as_str(), "Damage");
        assert_eq!(m.mod_type, ModType::More);
        assert_eq!(m.value.as_number(), Some(-21.0));
        assert_eq!(
            m.tags,
            vec![crate::ModTag::condition("Unique", true)],
            "Condition Unique(neg) 直译保留，GlobalEffect 剥除"
        );
    }

    /// An unknown enemy-side name (pobr has no consumer yet, e.g.
    /// TemporalChainsActionSpeed) is reported as Unsupported(UnknownModName)
    /// (for visibility, not injected silently).
    #[test]
    fn curse_unknown_enemy_name_is_reported() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "TemporalChainsPlayer": { "1": {
                 "base_skill_debuff_action_speed_+%_final_to_inflict": {
                   "mods": [ { "kind": "mod", "name": "TemporalChainsActionSpeed", "mod_type": "INC",
                               "tags": [ { "type": "GlobalEffect", "effectType": "Curse" } ] } ] } } } } }"#,
        );
        assert_eq!(
            map_curse_stat(
                &catalog,
                "TemporalChainsPlayer",
                None,
                "base_skill_debuff_action_speed_+%_final_to_inflict",
                -25.0,
            ),
            MappedOutcome::Unsupported(UnsupportedReason::UnknownModName(
                "TemporalChainsActionSpeed".into()
            ))
        );
    }

    /// Temporal Chains's second (admitted) payload: a negative
    /// `BuffExpireFaster MORE` (meaning "expire slower") passes through to
    /// the same enemy-side name (consumer =
    /// `ailment::debuff_duration_mult`, CalcOffence.lua:1833-1835 / :5040).
    #[test]
    fn curse_buff_expire_faster_maps_to_enemy_name() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "TemporalChainsPlayer": { "1": {
                 "base_temporal_chains_other_buff_time_passed_+%_to_apply": {
                   "mods": [ { "kind": "mod", "name": "BuffExpireFaster", "mod_type": "MORE",
                               "tags": [ { "type": "GlobalEffect", "effectType": "Curse" } ] } ] } } } } }"#,
        );
        let MappedOutcome::Mapped(items) = map_curse_stat(
            &catalog,
            "TemporalChainsPlayer",
            None,
            "base_temporal_chains_other_buff_time_passed_+%_to_apply",
            -25.0,
        ) else {
            panic!("期望 Mapped");
        };
        assert_eq!(items.len(), 1);
        let MappedItem::Modifier(m) = &items[0] else {
            panic!("期望 Modifier");
        };
        assert_eq!(m.name.as_str(), "BuffExpireFaster");
        assert_eq!(m.mod_type, ModType::More);
        assert_eq!(m.value.as_number(), Some(-25.0));
        assert!(m.tags.is_empty(), "GlobalEffect 路由 tag 已剥除");
    }

    /// A non-curse payload (a global entry with no Curse tag, e.g. a skill's
    /// own duration/AoE) -> Mapped(empty): silently skipped (goes through the
    /// main skill injection channel instead, not Unsupported).
    #[test]
    fn non_curse_payload_yields_empty_mapped() {
        let catalog = catalog_json(
            r#"{ "global": { "base_skill_effect_duration": {
                   "div": 1000.0,
                   "mods": [ { "kind": "skill_data", "value": { "key": "duration" } } ] } } }"#,
        );
        assert_eq!(
            map_curse_stat(
                &catalog,
                "DespairPlayer",
                None,
                "base_skill_effect_duration",
                6000.0,
            ),
            MappedOutcome::Mapped(Vec::new())
        );
        // A catalog miss -> Unknown (same rule as map_stat).
        assert_eq!(
            map_curse_stat(&catalog, "DespairPlayer", None, "no_such_stat", 1.0),
            MappedOutcome::Unknown
        );
    }

    /// Existence check for an exposure-infliction payload:
    /// `flag("InflictExposure", …)` (mirrors Fire Exposure's
    /// `inflict_exposure_for_x_ms_on_ignite`, SkillStatMap.lua:1701-1703,
    /// where the gating tag doesn't affect the check) and
    /// `<El>ExposureChance BASE` (:1689-1690) both hit; an ordinary payload
    /// / a catalog miss -> doesn't exist.
    #[test]
    fn has_exposure_inflict_payload_matches_flag_and_chance() {
        let catalog = catalog_json(
            r#"{ "global": {
                 "inflict_exposure_for_x_ms_on_ignite": {
                   "mods": [ { "kind": "mod", "name": "ExposureDuration", "mod_type": "BASE",
                               "tags": [ { "type": "ActorCondition", "actor": "enemy", "var": "Ignited" } ] },
                             { "kind": "flag", "name": "InflictExposure", "mod_type": "FLAG", "value": true,
                               "tags": [ { "type": "ActorCondition", "actor": "enemy", "var": "Ignited" } ] } ] },
                 "base_inflict_fire_exposure_on_hit_%_chance": {
                   "mods": [ { "kind": "mod", "name": "FireExposureChance", "mod_type": "BASE" } ] },
                 "plain_damage": {
                   "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "INC" } ] } },
                 "per_stat_set": {} }"#,
        );
        assert!(has_exposure_inflict_payload(
            &catalog,
            "X",
            None,
            "inflict_exposure_for_x_ms_on_ignite"
        ));
        assert!(has_exposure_inflict_payload(
            &catalog,
            "X",
            None,
            "base_inflict_fire_exposure_on_hit_%_chance"
        ));
        assert!(!has_exposure_inflict_payload(
            &catalog,
            "X",
            None,
            "plain_damage"
        ));
        assert!(!has_exposure_inflict_payload(
            &catalog,
            "X",
            None,
            "no_such_stat"
        ));
    }

    /// The **existence** check for a curse payload (vendor's buffList
    /// registration precondition, CalcActiveSkill.lua:976-1041): a payload
    /// outside the allow-list (TemporalChainsActionSpeed / a Dummy
    /// placeholder) still counts as existing (vendor counts it toward the
    /// slot too); a non-curse entry / a catalog miss -> doesn't exist.
    #[test]
    fn has_curse_payload_is_presence_not_translatability() {
        let catalog = catalog_json(
            r#"{ "global": { "base_skill_effect_duration": {
                   "div": 1000.0,
                   "mods": [ { "kind": "skill_data", "value": { "key": "duration" } } ] } },
                 "per_stat_set": { "TemporalChainsPlayer": { "1": {
                 "base_skill_debuff_action_speed_+%_final_to_inflict": {
                   "mods": [ { "kind": "mod", "name": "TemporalChainsActionSpeed", "mod_type": "INC",
                               "tags": [ { "type": "GlobalEffect", "effectType": "Curse" } ] } ] } } } } }"#,
        );
        // Outside the allow-list (map_curse_stat would say Unsupported) but
        // the payload exists -> true.
        assert!(has_curse_payload(
            &catalog,
            "TemporalChainsPlayer",
            None,
            "base_skill_debuff_action_speed_+%_final_to_inflict",
        ));
        // A non-curse entry (a global duration, the shape of all of
        // Repulsion's stats) -> false.
        assert!(!has_curse_payload(
            &catalog,
            "CurseOfRepulsionPlayer",
            None,
            "base_skill_effect_duration",
        ));
        // A catalog miss -> false.
        assert!(!has_curse_payload(
            &catalog,
            "CurseOfRepulsionPlayer",
            None,
            "no_such_stat",
        ));
    }

    /// GlobalEffect with a key outside the convention (effectCond etc.,
    /// carrying extra gating semantics) skips the whole entry as Unsupported.
    #[test]
    fn curse_global_effect_extra_keys_unsupported() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "X": { "1": {
                 "some_stat": {
                   "mods": [ { "kind": "mod", "name": "ChaosResist", "mod_type": "BASE",
                               "tags": [ { "type": "GlobalEffect", "effectType": "Curse",
                                           "effectCond": "Stationary" } ] } ] } } } } }"#,
        );
        assert!(matches!(
            map_curse_stat(&catalog, "X", Some("1"), "some_stat", -10.0),
            MappedOutcome::Unsupported(UnsupportedReason::UnsupportedTag(_))
        ));
    }

    /// Mirrors Frost Bomb's shape (SkillStatMap.lua:1721-1725):
    /// `active_skill_all_elemental_exposure_magnitude` -> the three elemental
    /// `<El>Exposure BASE` (GlobalEffect Debuff is stripped, consumed by
    /// enemy-side exposure reduction).
    #[test]
    fn debuff_exposure_magnitude_maps_three_elements() {
        let catalog = catalog_json(
            r#"{ "global": { "active_skill_all_elemental_exposure_magnitude": {
                 "mods": [
                   { "kind": "mod", "name": "FireExposure", "mod_type": "BASE",
                     "tags": [ { "type": "GlobalEffect", "effectType": "Debuff" } ] },
                   { "kind": "mod", "name": "ColdExposure", "mod_type": "BASE",
                     "tags": [ { "type": "GlobalEffect", "effectType": "Debuff" } ] },
                   { "kind": "mod", "name": "LightningExposure", "mod_type": "BASE",
                     "tags": [ { "type": "GlobalEffect", "effectType": "Debuff" } ] } ] } } }"#,
        );
        let MappedOutcome::Mapped(items) = map_debuff_stat(
            &catalog,
            "FrostBombPlayer",
            None,
            "active_skill_all_elemental_exposure_magnitude",
            20.0,
        ) else {
            panic!("期望 Mapped");
        };
        let mods: Vec<(&str, ModType, Option<f64>)> = items
            .iter()
            .map(|i| match i {
                MappedItem::Modifier(m) => (m.name.as_str(), m.mod_type, m.value.as_number()),
                other => panic!("期望 Modifier，得到 {other:?}"),
            })
            .collect();
        assert_eq!(
            mods,
            vec![
                ("FireExposure", ModType::Base, Some(20.0)),
                ("ColdExposure", ModType::Base, Some(20.0)),
                ("LightningExposure", ModType::Base, Some(20.0)),
            ]
        );
        for item in &items {
            let MappedItem::Modifier(m) = item else {
                unreachable!()
            };
            assert!(m.tags.is_empty(), "GlobalEffect Debuff tag 剥除");
        }
    }

    /// An unknown enemy-side debuff name (outside the allow-list) is
    /// reported as Unsupported(UnknownModName); a non-debuff payload (no
    /// Debuff tag) -> Mapped(empty), silently skipped (goes through the main
    /// skill channel instead).
    #[test]
    fn debuff_unknown_name_reported_and_non_debuff_empty() {
        let catalog = catalog_json(
            r#"{ "global": {
                 "hinder_debuff_movement_speed_+%": {
                   "mods": [ { "kind": "mod", "name": "MovementSpeed", "mod_type": "INC",
                               "tags": [ { "type": "GlobalEffect", "effectType": "Debuff" } ] } ] },
                 "spell_minimum_base_cold_damage": {
                   "mods": [ { "kind": "mod", "name": "ColdMin", "mod_type": "BASE" } ] } } }"#,
        );
        assert_eq!(
            map_debuff_stat(
                &catalog,
                "X",
                None,
                "hinder_debuff_movement_speed_+%",
                -30.0
            ),
            MappedOutcome::Unsupported(UnsupportedReason::UnknownModName("MovementSpeed".into()))
        );
        assert_eq!(
            map_debuff_stat(&catalog, "X", None, "spell_minimum_base_cold_damage", 7.0),
            MappedOutcome::Mapped(Vec::new())
        );
        assert_eq!(
            map_debuff_stat(&catalog, "X", None, "no_such_stat", 1.0),
            MappedOutcome::Unknown
        );
    }

    /// (#12) Mirrors Loyalty's shape (per_stat_set["SupportLoyaltyPlayer"]["1"]):
    /// `MinionModifier LIST { mod = Life MORE }` -> the minion-side inner
    /// `MaximumLife MORE` (name normalized, the same rule as the buff
    /// domain's Life -> MaximumLife). An inner mod outside the allow-list
    /// (Damage) and a tagged outer layer both skip the whole entry (return
    /// empty).
    #[test]
    fn minion_life_stat_maps_loyalty_more_life() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "SupportLoyaltyPlayer": { "1": {
                 "support_trusty_companion_minion_life_+%_final": {
                   "mods": [ { "kind": "mod", "name": "MinionModifier", "mod_type": "LIST",
                               "value": { "mod": { "kind": "mod", "mod_type": "MORE",
                                                   "name": "Life" } } } ] },
                 "minion_damage_+%": {
                   "mods": [ { "kind": "mod", "name": "MinionModifier", "mod_type": "LIST",
                               "value": { "mod": { "kind": "mod", "mod_type": "INC",
                                                   "name": "Damage" } } } ] } } } } }"#,
        );
        let mods = map_minion_life_stat(
            &catalog,
            "SupportLoyaltyPlayer",
            None,
            "support_trusty_companion_minion_life_+%_final",
            -30.0,
        );
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].name.as_str(), "MaximumLife");
        assert_eq!(mods[0].mod_type, ModType::More);
        assert_eq!(mods[0].value.as_number(), Some(-30.0));
        // An inner mod other than Life (Damage): not admitted by the narrow
        // channel -> empty.
        assert!(
            map_minion_life_stat(
                &catalog,
                "SupportLoyaltyPlayer",
                None,
                "minion_damage_+%",
                20.0
            )
            .is_empty()
        );
        // An unknown stat -> empty.
        assert!(
            map_minion_life_stat(&catalog, "SupportLoyaltyPlayer", None, "no_such", 1.0).is_empty()
        );
    }

    /// Mirrors Precision II's shape (sup_dex.lua:4216-4250): `Accuracy INC` +
    /// GlobalEffect Buff (including the effectName key, no gating semantics)
    /// -> a player-side Accuracy INC with tags fully stripped.
    #[test]
    fn player_buff_precision_accuracy_inc_maps() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "SupportPrecisionPlayerTwo": { "1": {
                 "support_precision_accuracy_rating_+%": {
                   "mods": [ { "kind": "mod", "name": "Accuracy", "mod_type": "INC",
                               "tags": [ { "type": "GlobalEffect", "effectType": "Buff",
                                           "effectName": "Precision II" } ] } ] } } } } }"#,
        );
        let MappedOutcome::Mapped(items) = map_player_buff_stat(
            &catalog,
            "SupportPrecisionPlayerTwo",
            None,
            "support_precision_accuracy_rating_+%",
            50.0,
        ) else {
            panic!("期望 Mapped");
        };
        assert_eq!(items.len(), 1);
        let MappedItem::Modifier(m) = &items[0] else {
            panic!("期望 Modifier");
        };
        assert_eq!(m.name.as_str(), "Accuracy");
        assert_eq!(m.mod_type, ModType::Inc);
        assert_eq!(m.value.as_number(), Some(50.0));
        assert!(m.tags.is_empty(), "GlobalEffect（含 effectName）剥除");
    }

    /// Mirrors War Banner's shape (GlobalEffect effectType=Aura + Condition
    /// BannerPlanted): Aura-type player-side buffs are collected the same
    /// way, and the Condition tag translates directly and is kept.
    #[test]
    fn player_buff_banner_accuracy_keeps_condition() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "WarBannerPlayer": { "1": {
                 "base_skill_buff_banner_accuracy_+%_to_apply": {
                   "mods": [ { "kind": "mod", "name": "Accuracy", "mod_type": "INC",
                               "tags": [ { "type": "Condition", "var": "BannerPlanted" },
                                         { "type": "GlobalEffect", "effectType": "Aura" } ] } ] } } } } }"#,
        );
        let MappedOutcome::Mapped(items) = map_player_buff_stat(
            &catalog,
            "WarBannerPlayer",
            None,
            "base_skill_buff_banner_accuracy_+%_to_apply",
            130.0,
        ) else {
            panic!("期望 Mapped");
        };
        let MappedItem::Modifier(m) = &items[0] else {
            panic!("期望 Modifier");
        };
        assert_eq!(m.name.as_str(), "Accuracy");
        assert_eq!(m.value.as_number(), Some(130.0));
        assert_eq!(
            m.tags,
            vec![crate::ModTag::condition("BannerPlanted", false)],
            "Condition 直译保留，GlobalEffect 剥除"
        );
    }

    /// Mirrors Clarity II's shape (vendor sup_int.txt:305-315:
    /// `support_clarity_mana_regeneration_rate_+%` -> `ManaRegen INC` +
    /// GlobalEffect Buff effectName "Clarity II") -> a player-side
    /// ManaRegen INC with tags fully stripped. Pinned by the oracle
    /// (pob2-oracle sorceress-stormweaver-comet): Clarity II lv1's payload of
    /// 50 plus the rest of the bare-name INC 25 -> calcsOutput.ManaRegenInc
    /// = 75.
    #[test]
    fn player_buff_clarity_mana_regen_inc_maps() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "SupportClarityPlayerTwo": { "1": {
                 "support_clarity_mana_regeneration_rate_+%": {
                   "mods": [ { "kind": "mod", "name": "ManaRegen", "mod_type": "INC",
                               "tags": [ { "type": "GlobalEffect", "effectType": "Buff",
                                           "effectName": "Clarity II" } ] } ] } } } } }"#,
        );
        let MappedOutcome::Mapped(items) = map_player_buff_stat(
            &catalog,
            "SupportClarityPlayerTwo",
            None,
            "support_clarity_mana_regeneration_rate_+%",
            50.0,
        ) else {
            panic!("期望 Mapped");
        };
        assert_eq!(items.len(), 1);
        let MappedItem::Modifier(m) = &items[0] else {
            panic!("期望 Modifier");
        };
        assert_eq!(m.name.as_str(), "ManaRegen");
        assert_eq!(m.mod_type, ModType::Inc);
        assert_eq!(m.value.as_number(), Some(50.0));
        assert!(m.tags.is_empty(), "GlobalEffect（含 effectName）剥除");
    }

    /// Mirrors Vitality II's shape (vendor sup_str.txt:1791-1802:
    /// `support_vitality_life_regeneration_rate_per_minute_%` div=60 ->
    /// `LifeRegenPercent BASE`) -> a per-minute percentage converted to
    /// per-second (120/60 = 2.0). Pinned by the oracle (pob2-oracle
    /// warrior-smith-of-kitava-shield-wall): Vitality II's 2.0 plus the rest
    /// of 6.1 -> calcsOutput.LifeRegenPercent = 8.1.
    #[test]
    fn player_buff_vitality_life_regen_percent_div60() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "SupportVitalityPlayerTwo": { "1": {
                 "support_vitality_life_regeneration_rate_per_minute_%": {
                   "div": 60.0,
                   "mods": [ { "kind": "mod", "name": "LifeRegenPercent", "mod_type": "BASE",
                               "tags": [ { "type": "GlobalEffect", "effectType": "Buff",
                                           "effectName": "Vitality II" } ] } ] } } } } }"#,
        );
        let MappedOutcome::Mapped(items) = map_player_buff_stat(
            &catalog,
            "SupportVitalityPlayerTwo",
            None,
            "support_vitality_life_regeneration_rate_per_minute_%",
            120.0,
        ) else {
            panic!("期望 Mapped");
        };
        assert_eq!(items.len(), 1);
        let MappedItem::Modifier(m) = &items[0] else {
            panic!("期望 Modifier");
        };
        assert_eq!(m.name.as_str(), "LifeRegenPercent");
        assert_eq!(m.mod_type, ModType::Base);
        assert_eq!(m.value.as_number(), Some(2.0), "120/min ÷ 60 = 2.0/s");
        assert!(m.tags.is_empty());
    }

    /// A name outside the player-side allow-list (e.g. AttackSpeed) is
    /// reported as Unsupported(UnknownModName); a non-buff payload (no
    /// GlobalEffect Buff/Aura tag) -> Mapped(empty), silently skipped.
    #[test]
    fn player_buff_unknown_name_reported_and_non_buff_empty() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "X": { "1": {
                 "buff_attack_speed": {
                   "mods": [ { "kind": "mod", "name": "AttackSpeed", "mod_type": "INC",
                               "tags": [ { "type": "GlobalEffect", "effectType": "Buff" } ] } ] },
                 "local_damage": {
                   "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "INC" } ] } } } } }"#,
        );
        assert_eq!(
            map_player_buff_stat(&catalog, "X", Some("1"), "buff_attack_speed", 10.0),
            MappedOutcome::Unsupported(UnsupportedReason::UnknownModName("AttackSpeed".into()))
        );
        assert_eq!(
            map_player_buff_stat(&catalog, "X", Some("1"), "local_damage", 10.0),
            MappedOutcome::Mapped(Vec::new())
        );
    }

    /// Mirrors Pinnacle of Power's shape (other.lua:12503
    /// `elemental_power_elemental_damage_+%_final_per_power_charge`): the
    /// first element, `Damage MORE` with a scalar Multiplier (outside the
    /// boundary, injects nothing), does **not** drag down the same entry's
    /// six `<El>Can<Ailment>` flags -- with each element handled
    /// independently, all the flag payloads still come out and the
    /// GlobalEffect tag is fully stripped. This is the cross-type pass for
    /// stormweaver-comet's IgniteDPS 1911 (pinned by the oracle at
    /// `Skill:PinnacleOfPowerPlayer`).
    #[test]
    fn player_buff_pinnacle_flags_survive_scalar_sibling() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "PinnacleOfPowerPlayer": { "1": {
                 "elemental_power_elemental_damage_+%_final_per_power_charge": {
                   "mods": [
                     { "kind": "mod", "name": "Damage", "mod_type": "MORE",
                       "tags": [ { "type": "SkillType", "skillTypeList": ["Cold","Fire","Lightning"] },
                                 { "type": "Multiplier", "var": "RemovablePowerCharge",
                                   "scalar": "ConsumedPowerChargeEffect" },
                                 { "type": "GlobalEffect", "effectType": "Buff" } ] },
                     { "kind": "flag", "name": "ColdCanIgnite", "mod_type": "FLAG", "value": true,
                       "tags": [ { "type": "GlobalEffect", "effectType": "Buff" } ] },
                     { "kind": "flag", "name": "ColdCanShock", "mod_type": "FLAG", "value": true,
                       "tags": [ { "type": "GlobalEffect", "effectType": "Buff" } ] },
                     { "kind": "flag", "name": "FireCanFreeze", "mod_type": "FLAG", "value": true,
                       "tags": [ { "type": "GlobalEffect", "effectType": "Buff" } ] },
                     { "kind": "flag", "name": "FireCanShock", "mod_type": "FLAG", "value": true,
                       "tags": [ { "type": "GlobalEffect", "effectType": "Buff" } ] },
                     { "kind": "flag", "name": "LightningCanFreeze", "mod_type": "FLAG", "value": true,
                       "tags": [ { "type": "GlobalEffect", "effectType": "Buff" } ] },
                     { "kind": "flag", "name": "LightningCanIgnite", "mod_type": "FLAG", "value": true,
                       "tags": [ { "type": "GlobalEffect", "effectType": "Buff" } ] } ] } } } } }"#,
        );
        let MappedOutcome::Mapped(items) = map_player_buff_stat(
            &catalog,
            "PinnacleOfPowerPlayer",
            None,
            "elemental_power_elemental_damage_+%_final_per_power_charge",
            15.0,
        ) else {
            panic!("期望 Mapped（scalar 元素不连坐 flag）");
        };
        let flags: Vec<(&str, ModType)> = items
            .iter()
            .map(|item| {
                let MappedItem::Modifier(m) = item else {
                    panic!("期望 Modifier");
                };
                assert!(m.tags.is_empty(), "GlobalEffect Buff tag 剥除");
                (m.name.as_str(), m.mod_type)
            })
            .collect();
        assert_eq!(
            flags,
            vec![
                ("ColdCanIgnite", ModType::Flag),
                ("ColdCanShock", ModType::Flag),
                ("FireCanFreeze", ModType::Flag),
                ("FireCanShock", ModType::Flag),
                ("LightningCanFreeze", ModType::Flag),
                ("LightningCanIgnite", ModType::Flag),
            ],
            "六枚 <El>Can<Ailment> flag 全部产出；scalar Damage MORE 零注入"
        );
    }

    /// A flag outside the `<Type>Can<Ailment>` family (e.g. the behaviour
    /// switch projectile) stays reported as unknown; when every matching
    /// element fails (zero injections), the Unsupported visibility doesn't
    /// degrade.
    #[test]
    fn player_buff_flag_outside_family_reported() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "X": { "1": {
                 "some_behaviour_stat": {
                   "mods": [ { "kind": "flag", "name": "projectile", "mod_type": "FLAG",
                               "value": true,
                               "tags": [ { "type": "GlobalEffect", "effectType": "Buff" } ] } ] } } } } }"#,
        );
        assert_eq!(
            map_player_buff_stat(&catalog, "X", Some("1"), "some_behaviour_stat", 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::UnknownModName("flag:projectile".into()))
        );
    }

    /// Mirrors Sigil of Power's shape (vendor other.lua's
    /// SigilOfPowerPlayer statMap
    /// `circle_of_power_spell_damage_+%_final_per_stage`): Damage MORE +
    /// Spell flag + Multiplier{var=SigilOfPowerStage,
    /// limitVar=SigilOfPowerMaxStages}. Pinned by the oracle (varashta): eff
    /// level 32 -> per-stage 17, stage=1 -> modDB
    /// `Damage MORE 17 | Skill:SigilOfPowerPlayer`.
    #[test]
    fn player_buff_sigil_per_stage_damage_more_with_limit_var() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "SigilOfPowerPlayer": { "1": {
                 "circle_of_power_spell_damage_+%_final_per_stage": {
                   "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "MORE",
                               "flags": [ "Spell" ],
                               "tags": [ { "type": "GlobalEffect", "effectType": "Buff" },
                                         { "type": "Multiplier", "var": "SigilOfPowerStage",
                                           "limitVar": "SigilOfPowerMaxStages" } ] } ] } } } } }"#,
        );
        let MappedOutcome::Mapped(items) = map_player_buff_stat(
            &catalog,
            "SigilOfPowerPlayer",
            None,
            "circle_of_power_spell_damage_+%_final_per_stage",
            17.0,
        ) else {
            panic!("期望 Mapped");
        };
        let MappedItem::Modifier(m) = &items[0] else {
            panic!("期望 Modifier");
        };
        assert_eq!(m.name.as_str(), "Damage");
        assert_eq!(m.mod_type, ModType::More);
        assert_eq!(m.value.as_number(), Some(17.0));
        assert_eq!(m.flags, ModFlags::SPELL, "Spell flag 直译");
        assert_eq!(
            m.tags,
            vec![crate::ModTag::Multiplier {
                var: "SigilOfPowerStage".into(),
                div: 1.0,
                limit: None,
                actor: None,
                limit_var: Some("SigilOfPowerMaxStages".into()),
                limit_actor: None,
                invert: false,
                limit_total: false,
            }],
            "Multiplier limitVar 直译，GlobalEffect 剥除"
        );
        // Evaluation: stage=1, maxStages=4 -> ×1; stage=9 is capped to 4 by
        // maxStages.
        let cfg = crate::CalcConfig::new()
            .with_multiplier("SigilOfPowerStage", 1.0)
            .with_multiplier("SigilOfPowerMaxStages", 4.0);
        assert_eq!(m.effective_number(&cfg), Some(17.0));
        let cfg9 = crate::CalcConfig::new()
            .with_multiplier("SigilOfPowerStage", 9.0)
            .with_multiplier("SigilOfPowerMaxStages", 4.0);
        assert_eq!(m.effective_number(&cfg9), Some(68.0));
    }

    /// Mirrors Refraction II's shape (vendor sup_str.lua:6023-6025:
    /// `support_tempered_valour_deflection_rating_%_of_evasion_rating` ->
    /// `EvasionGainAsDeflection BASE 20` + GlobalEffect Buff "Refractive
    /// Plating" + MultiplierThreshold
    /// ValourStacks/thresholdVar=RefractionMinimumValour) -> a player-side
    /// BASE, with the threshold statically folded to 0 (that var has zero
    /// setters across the whole vendor tree), active under the default cfg.
    /// Pinned by the oracle (wolf-pack): tree's 28 + this payload's 20 =
    /// 48%, rating 11791.52.
    #[test]
    fn player_buff_refraction_evasion_gain_as_deflection_maps() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "SupportRefractionPlayerTwo": { "1": {
                 "support_tempered_valour_deflection_rating_%_of_evasion_rating": {
                   "mods": [ { "kind": "mod", "name": "EvasionGainAsDeflection",
                               "mod_type": "BASE",
                               "tags": [ { "type": "GlobalEffect", "effectType": "Buff",
                                           "effectName": "Refractive Plating" },
                                         { "type": "MultiplierThreshold", "var": "ValourStacks",
                                           "thresholdVar": "RefractionMinimumValour" } ] } ] } } } } }"#,
        );
        let MappedOutcome::Mapped(items) = map_player_buff_stat(
            &catalog,
            "SupportRefractionPlayerTwo",
            None,
            "support_tempered_valour_deflection_rating_%_of_evasion_rating",
            20.0,
        ) else {
            panic!("期望 Mapped");
        };
        let MappedItem::Modifier(m) = &items[0] else {
            panic!("期望 Modifier");
        };
        assert_eq!(m.name.as_str(), "EvasionGainAsDeflection");
        assert_eq!(m.mod_type, ModType::Base);
        assert_eq!(m.value.as_number(), Some(20.0));
        assert_eq!(
            m.tags,
            vec![crate::ModTag::MultiplierThreshold {
                var: "ValourStacks".into(),
                threshold: 0.0,
                upper: false,
            }],
            "thresholdVar 静态折 0，GlobalEffect 剥除"
        );
        // Under the default cfg (ValourStacks not injected = 0): 0 ≥ 0 ->
        // active, matching vendor's default.
        assert!(m.matches(&crate::CalcConfig::new()));
    }

    /// The same buff's armour-conversion payload (vendor sup_str.lua:6019-6021:
    /// `support_tempered_valour_%_armour_to_apply_to_elemental_damage` ->
    /// ArmourAppliesTo{Fire,Cold,Lightning}DamageTaken BASE 30, with the same
    /// tag shape as the deflection payload) -> three player-side BASE
    /// modifiers, consumed by `calc::taken::armour_applies_pct`. Pinned by
    /// the oracle (wolf-pack): tree's 84 + this payload's 30 = 114%,
    /// FireEffectiveAppliedArmour 21181.2.
    #[test]
    fn player_buff_refraction_armour_applies_to_elements_maps() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "SupportRefractionPlayerTwo": { "1": {
                 "support_tempered_valour_%_armour_to_apply_to_elemental_damage": {
                   "mods": [ { "kind": "mod", "name": "ArmourAppliesToFireDamageTaken",
                               "mod_type": "BASE",
                               "tags": [ { "type": "GlobalEffect", "effectType": "Buff",
                                           "effectName": "Refractive Plating" },
                                         { "type": "MultiplierThreshold", "var": "ValourStacks",
                                           "thresholdVar": "RefractionMinimumValour" } ] },
                             { "kind": "mod", "name": "ArmourAppliesToColdDamageTaken",
                               "mod_type": "BASE",
                               "tags": [ { "type": "GlobalEffect", "effectType": "Buff",
                                           "effectName": "Refractive Plating" },
                                         { "type": "MultiplierThreshold", "var": "ValourStacks",
                                           "thresholdVar": "RefractionMinimumValour" } ] },
                             { "kind": "mod", "name": "ArmourAppliesToLightningDamageTaken",
                               "mod_type": "BASE",
                               "tags": [ { "type": "GlobalEffect", "effectType": "Buff",
                                           "effectName": "Refractive Plating" },
                                         { "type": "MultiplierThreshold", "var": "ValourStacks",
                                           "thresholdVar": "RefractionMinimumValour" } ] } ] } } } } }"#,
        );
        let MappedOutcome::Mapped(items) = map_player_buff_stat(
            &catalog,
            "SupportRefractionPlayerTwo",
            None,
            "support_tempered_valour_%_armour_to_apply_to_elemental_damage",
            30.0,
        ) else {
            panic!("期望 Mapped");
        };
        let names: Vec<&str> = items
            .iter()
            .map(|item| {
                let MappedItem::Modifier(m) = item else {
                    panic!("期望 Modifier");
                };
                assert_eq!(m.mod_type, ModType::Base);
                assert_eq!(m.value.as_number(), Some(30.0));
                // Under the default cfg (ValourStacks not injected = 0):
                // 0 ≥ 0 -> active.
                assert!(m.matches(&crate::CalcConfig::new()));
                m.name.as_str()
            })
            .collect();
        assert_eq!(
            names,
            vec![
                "ArmourAppliesToFireDamageTaken",
                "ArmourAppliesToColdDamageTaken",
                "ArmourAppliesToLightningDamageTaken",
            ]
        );
    }

    /// A MultiplierThreshold thresholdVar with a setter (Attrition's
    /// `AttritionCullSeconds`, set by act_str.lua:1258's Multiplier) --
    /// statically folding it to 0 would open the gate at the wrong point, so
    /// it stays reported as Unsupported wholesale.
    #[test]
    fn player_buff_threshold_var_with_setter_reported() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "AttritionPlayer": { "1": {
                 "attrition_cull_payload": {
                   "mods": [ { "kind": "mod", "name": "EvasionGainAsDeflection",
                               "mod_type": "BASE",
                               "tags": [ { "type": "GlobalEffect", "effectType": "Buff" },
                                         { "type": "MultiplierThreshold", "var": "EnemyPresenceSeconds",
                                           "thresholdVar": "AttritionCullSeconds" } ] } ] } } } } }"#,
        );
        assert!(matches!(
            map_player_buff_stat(
                &catalog,
                "AttritionPlayer",
                None,
                "attrition_cull_payload",
                1.0,
            ),
            MappedOutcome::Unsupported(UnsupportedReason::UnsupportedTag(_))
        ));
    }

    /// Sigil of Power's max-stages payload (`circle_of_power_max_stages` ->
    /// `Multiplier:SigilOfPowerMaxStages` BASE, with GlobalEffect carrying
    /// the unscalable marker -- admitted through; the orchestration layer
    /// bridges this BASE into cfg.multipliers as the limitVar denominator).
    #[test]
    fn player_buff_sigil_max_stages_multiplier_base() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "SigilOfPowerPlayer": { "1": {
                 "circle_of_power_max_stages": {
                   "mods": [ { "kind": "mod", "name": "Multiplier:SigilOfPowerMaxStages",
                               "mod_type": "BASE",
                               "tags": [ { "type": "GlobalEffect", "effectType": "Buff",
                                           "unscalable": true } ] } ] } } } } }"#,
        );
        let MappedOutcome::Mapped(items) = map_player_buff_stat(
            &catalog,
            "SigilOfPowerPlayer",
            None,
            "circle_of_power_max_stages",
            4.0,
        ) else {
            panic!("期望 Mapped");
        };
        let MappedItem::Modifier(m) = &items[0] else {
            panic!("期望 Modifier");
        };
        assert_eq!(m.name.as_str(), "Multiplier:SigilOfPowerMaxStages");
        assert_eq!(m.mod_type, ModType::Base);
        assert_eq!(m.value.as_number(), Some(4.0));
        assert!(m.tags.is_empty(), "GlobalEffect（含 unscalable 键）剥除");
    }

    /// Mirrors Elemental Conflux's shape (vendor SkillStatMap's
    /// `skill_elemental_conflux_active_element_damage_+%_final`): three
    /// elemental MORE mods, each with an invert Multiplier
    /// (`ElementalConflux<El>Effect`; the config's Average setting = 3 ->
    /// splits ×1/3, and locking to a single element = 1/0 -> ×1/×0). Pinned
    /// by the oracle (varashta): 73 -> three `<El>Damage MORE 24.33`.
    #[test]
    fn player_buff_conflux_inverted_element_more() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "ElementalConfluxPlayer": { "1": {
                 "skill_elemental_conflux_active_element_damage_+%_final": {
                   "mods": [
                     { "kind": "mod", "name": "LightningDamage", "mod_type": "MORE",
                       "tags": [ { "type": "GlobalEffect", "effectType": "Buff",
                                   "effectName": "Elemental Conflux" },
                                 { "type": "Multiplier",
                                   "var": "ElementalConfluxLightningEffect",
                                   "invert": true } ] },
                     { "kind": "mod", "name": "ColdDamage", "mod_type": "MORE",
                       "tags": [ { "type": "GlobalEffect", "effectType": "Buff",
                                   "effectName": "Elemental Conflux" },
                                 { "type": "Multiplier",
                                   "var": "ElementalConfluxColdEffect",
                                   "invert": true } ] } ] } } } } }"#,
        );
        let MappedOutcome::Mapped(items) = map_player_buff_stat(
            &catalog,
            "ElementalConfluxPlayer",
            None,
            "skill_elemental_conflux_active_element_damage_+%_final",
            73.0,
        ) else {
            panic!("期望 Mapped");
        };
        assert_eq!(items.len(), 2);
        let MappedItem::Modifier(lightning) = &items[0] else {
            panic!("期望 Modifier");
        };
        assert_eq!(lightning.name.as_str(), "LightningDamage");
        assert_eq!(lightning.mod_type, ModType::More);
        assert_eq!(lightning.value.as_number(), Some(73.0));
        // Average setting: multiplier 3 -> invert ×1/3 = 24.33 (matches
        // vendor's Tabulate).
        let avg = crate::CalcConfig::new().with_multiplier("ElementalConfluxLightningEffect", 3.0);
        let v = lightning.effective_number(&avg).expect("数值");
        assert!((v - 73.0 / 3.0).abs() < 1e-9, "invert 1/3 均摊，得 {v}");
        // Locked to another element: multiplier 0 -> invert stays 0 -> this
        // element's payload is 0.
        let locked =
            crate::CalcConfig::new().with_multiplier("ElementalConfluxLightningEffect", 0.0);
        assert_eq!(lightning.effective_number(&locked), Some(0.0));
    }

    /// Unsupported classification tags stay stable (the aggregation key for
    /// dual-run reports).
    #[test]
    fn unsupported_categories_are_stable() {
        assert_eq!(UnsupportedReason::Unextractable.category(), "unextractable");
        assert_eq!(UnsupportedReason::ScalarMultiplier.category(), "scalar");
        assert_eq!(
            UnsupportedReason::UnknownModName(String::new()).category(),
            "unknown_mod_name"
        );
        assert_eq!(
            UnsupportedReason::UnsupportedTag(String::new()).category(),
            "tag"
        );
    }
}
