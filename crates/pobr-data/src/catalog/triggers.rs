//! Mirage config domain schema (`overlay/mirage_configs.json`).
//!
//! Data source: vendor PoB2 `Modules/CalcMirages.lua`'s five branches
//! (Mirage Archer / Saviour Mirage Warriors / Tawhoa's Chosen / Sacred Wisps
//! / General's Cry) — the branch bodies are procedural closures that can't
//! be serialized by luajit, so these 5 configs are stored by
//! `sync-pob-catalog gen-mirage-configs` **embedding them in the tool's
//! source** (satisfying "overlay files are never hand-edited, only
//! tool-regenerated"; vendor drift is flagged by a coarse-grained
//! fingerprint of CalcMirages.lua recorded in `_meta` — open question 2).
//!
//! Genuinely special branch logic (Tawhoa's trigger-cooldown model,
//! General's Cry's exert rewrite, etc.) goes through `handler_id`
//! (registered in `pobr-core::rules::registry`, subject to doc 20 §5's
//! <100 total handler count monitor); this module only defines the serde
//! shape, no logic.
//!
//! This file also holds the schema for `trigger_configs.json` (extended in
//! the same file when the time comes, with schema ownership unified under
//! catalog).

use serde::{Deserialize, Serialize};

/// A mirage's trigger condition (the hit condition for this branch in
/// vendor `calcs.mirages`'s if-elseif chain; the two fields are mutually
/// exclusive alternatives).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MirageTriggerDef {
    /// A trigger flag on the main skill's `skillData` (e.g.
    /// `triggeredByMirageArcher`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_data_flag: Option<String>,
    /// Exact match on the main skill's granted-effect name (e.g. Saviour's
    /// `Reflection`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub granted_effect_name: Option<String>,
}

/// Filter for a mirage's source skill (the data-representable part of
/// vendor's `config.compareFunc`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MirageSourceFilterDef {
    /// Requires this main-hand weapon type (e.g. `Bow` / `Wand`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weapon_type: Option<String>,
    /// Requires all of these skill types to match (e.g. `["Attack"]`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skill_types: Vec<String>,
    /// Excludes the skill if any of these types match (e.g.
    /// `["Totem", "SummonsTotem"]`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude_skill_types: Vec<String>,
    /// Requires the skill's cfg flags to include all of these bit names
    /// (e.g. Saviour's `["Sword", "Weapon1H"]`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub weapon_flags: Vec<String>,
    /// Excludes skills already used by another mirage (recursion guard,
    /// vendor's `usedByMirage` condition).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub exclude_used_by_mirage: bool,
    /// Source-skill selection strategy: `main_skill` (the mirage copies the
    /// main skill itself) or `best_dps` (scans the skill list for the
    /// highest DPS, vendor's GlobalCache path).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub select: Option<String>,
}

/// The config for one kind of mirage (corresponds to one branch of
/// `Modules/CalcMirages.lua`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MirageConfigDef {
    /// Stable id (snake_case, e.g. `mirage_archer`).
    pub mirage_id: String,
    /// Trigger condition.
    pub trigger: MirageTriggerDef,
    /// Source-skill filter.
    #[serde(default)]
    pub source_skill_filter: MirageSourceFilterDef,
    /// Stat name for the mirage count (aggregated via `Sum("BASE", …)`,
    /// e.g. `MirageArcherMaxCount`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count_stat: Option<String>,
    /// Stat name for the "less damage" penalty (injects a `Damage MORE`,
    /// e.g. `MirageArcherLessDamage`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub less_damage_stat: Option<String>,
    /// Stat name for the "less attack speed" penalty (injects a `Speed MORE`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub less_attack_speed_stat: Option<String>,
    /// Stat name for the cast chance (Sacred Wisps: `Speed MORE (chance-100)`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cast_chance_stat: Option<String>,
    /// The mirage's sub-environment inherits the main skill's `storedUses`
    /// (vendor `mirageUses = storedUses`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub uses_stored_uses: bool,
    /// The main skill's offence panel keeps being computed on its own
    /// (vendor `calcMainSkillOffence`; false = the mirage's output entirely
    /// replaces the main skill's output, e.g. Saviour / Tawhoa).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub calc_main_skill_offence: bool,
    /// Stable handler id for genuinely special branch logic that can't be
    /// data-driven (e.g. Tawhoa's trigger-cooldown model); `None` means
    /// pure config can drive it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handler_id: Option<String>,
    /// Vendor origin (a `Modules/CalcMirages.lua` line range, an anchor for
    /// manual cross-checking).
    pub vendor_ref: String,
}

/// Top level of `overlay/mirage_configs.json` (the consumer ignores `_meta`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct MirageConfigsDef {
    /// Config list, ascending by `mirage_id`.
    pub configs: Vec<MirageConfigDef>,
}

//  trigger_configs (61 entries from vendor CalcTriggers.lua's configTable)

/// A trigger config's match key (vendor `CalcTriggers.lua:1452-1455`'s
/// four-level lookup: skill name → triggeredBy name → normalized awakened
/// name → unique item name, all lowercase).
///
/// `kind` marks which source category this key belongs to in vendor (used
/// by the consumer to pick the right join path); the normalized awakened
/// form (`gsub("^awakened ", "")`) shares a name with `triggered_by` and
/// isn't listed separately.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerKeyDef {
    /// Key category: `skill` (matches the main skill's name) /
    /// `triggered_by` (matches the name of the triggering support/meta gem)
    /// / `unique_item` (matches a unique item's trigger name).
    pub kind: String,
    /// The literal key from vendor's configTable (lowercase).
    pub name: String,
}

/// A restricted skill predicate (**capped at three fields**, field
/// references plus any/all/not, no free-form expressions; extending its
/// capability requires ≥20 entries to benefit — doc 20 §5's gate, otherwise
/// the entry falls back to `handler_id`).
///
/// Semantics, compared against the vendor closures:
/// - `any_skill_types`: `skill.skillTypes[A] or skill.skillTypes[B]` (any match);
/// - `all_mod_flags`: a transcription of the **intent** behind vendor's
///   multi-flag pattern `band(flags, bor(Mace, Weapon1H)) > 0`, which is
///   literally an any-of — but a 1H weapon attack's skillCfg.flags carries
///   both a "weapon class" bit and a "grip" bit at once, so all-of better
///   matches the entry's intent (e.g. Mjolner requires a **one-handed**
///   mace specifically); for single-flag entries the two semantics are
///   equivalent;
/// - `not_skill_types`: `not skill.skillTypes[X]` (excluded if any match).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerSkillCondDef {
    /// Passes if any of these skill types match (e.g. `["Melee", "Attack"]`).
    /// Empty means unconstrained.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub any_skill_types: Vec<String>,
    /// The skill's cfg flags must include all of these bit names (e.g.
    /// `["Claw"]` / `["Bow"]`). Empty means unconstrained.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub all_mod_flags: Vec<String>,
    /// Excluded if any of these skill types match (e.g. `["SummonsTotem"]`).
    /// Empty means nothing excluded.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub not_skill_types: Vec<String>,
}

impl TriggerSkillCondDef {
    /// Whether the predicate is empty (constrains nothing).
    pub fn is_empty(&self) -> bool {
        self.any_skill_types.is_empty()
            && self.all_mod_flags.is_empty()
            && self.not_skill_types.is_empty()
    }
}

/// A single trigger config (corresponds to one entry of vendor
/// `Modules/CalcTriggers.lua:881-1417`'s configTable; all 61 entries are
/// transcribed, with drift guarded against by the tool's key-scan
/// reconciliation).
///
/// ~90% of entries are declarative facts (trigger name / predicate /
/// cooldown override / rate-cap override / global flag / disable
/// condition); a few carry real logic (Mjolner's dual source, Snipe's
/// charge-up, etc.) → `handler_id` (count-monitored <100, doc 20 §5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TriggerConfigDef {
    /// Match key (the literal key from the four-level lookup, plus its category).
    pub key: TriggerKeyDef,
    /// Trigger display-name override (vendor `triggerName`, e.g.
    /// Limbsplit's "Gore Shockwave").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger_name: Option<String>,
    /// The source rate uses use/cast-triggered accounting (vendor
    /// `triggerOnUse`: the source triggers on every **use**, not folded
    /// through hit/crit chance).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub trigger_on_use: bool,
    /// The source rate uses the cast rate (vendor `useCastRate`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub use_cast_rate: bool,
    /// Predicate on the **triggering source skill** (a restricted
    /// transcription of vendor's `triggerSkillCond`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_skill_cond: Option<TriggerSkillCondDef>,
    /// Predicate on the **triggered skill** (a restricted transcription of
    /// vendor's `triggeredSkillCond`; vendor's inherent `triggeredBy*`
    /// skillData flag plus the same-socket-group requirement are this
    /// predicate's default semantics and aren't re-encoded here).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub triggered_skill_cond: Option<TriggerSkillCondDef>,
    /// The source skill matched by **exact granted-effect name** (vendor's
    /// closure does `skill.activeEffect.grantedEffect.name == X`, e.g.
    /// Automation/Spellslinger).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_skill_name: Option<String>,
    /// This entry only applies when the main skill's name equals this value
    /// (vendor gates the entry with `env.player.mainSkill...name == X`,
    /// e.g. The Rippling Thoughts).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_main_skill_name: Option<String>,
    /// Stat name for the trigger chance (vendor
    /// `triggerChance = modDB:Sum("BASE", nil, X)` / a skillData field name,
    /// e.g. Kitava's `KitavaTriggerChance`, Cast when Stunned's
    /// `chanceToTriggerOnStun`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger_chance_stat: Option<String>,
    /// Stat name for the trigger's source rate (vendor
    /// `trigRate = modDB:Sum("BASE", nil, X)`, e.g. Intuitive Link's
    /// `IntuitiveLinkSourceRate`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_rate_stat: Option<String>,
    /// Cooldown override (seconds; vendor overrides
    /// `skillData.cooldown = N` in place, e.g. Lioneye's Paws).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cooldown_override_s: Option<f64>,
    /// Trigger rate cap override (uses/second; vendor
    /// `skillData.triggerRateCapOverride`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger_rate_cap_override: Option<f64>,
    /// Global trigger (vendor `skillFlags.globalTrigger`: doesn't depend on
    /// the source skill's rate — EffectiveSourceRate takes the
    /// TriggerRateCap instead).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub global_trigger: bool,
    /// The source is the triggered skill itself (vendor
    /// `return {source = env.player.mainSkill}`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub source_is_self: bool,
    /// The source rate isn't folded through any further accounting (vendor
    /// `skillData.sourceRateIsFinal`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub source_rate_is_final: bool,
    /// The trigger isn't rounded to the server tick rate (vendor
    /// `skillData.ignoresTickRate`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ignores_tick_rate: bool,
    /// Assumes "every hit kills" (vendor `assumingEveryHitKills`, for
    /// on-kill triggers).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub assuming_every_hit_kills: bool,
    /// Ignores the source-rate gate (vendor `ignoreSourceRate`, Avenging Flame).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ignore_source_rate: bool,
    /// The trigger chance is folded into the **source's crit chance**
    /// (vendor `skillData.triggerOnCrit`, the CoC path; sourced from
    /// SkillStatMap's fixed flag for triggeredByCoc skills, transcribed
    /// here as a plain fact).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub trigger_on_crit: bool,
    /// A precondition that must hold for this to apply (vendor
    /// `modDB:Flag(nil, "Condition:X")` gate, e.g. The Hidden Blade's
    /// `Phasing`, Cast on Melee Kill's `KilledRecently`); vendor disables
    /// the trigger, or falls back to self-cast, when unmet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_condition: Option<String>,
    /// PoBR-side join key: the PoE2 granted-effect id this entry
    /// corresponds to (`GrantedEffects.Id`, e.g. `MetaCastOnCritPlayer`).
    /// The vendor key is a PoB display name (PoBR's data model has no
    /// display name), so wiring identification matches by this field
    /// against gems in a socket group / the main skill; empty means not
    /// mapped yet (mostly PoE1 uniques with no PoE2 counterpart).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub match_effect_ids: Vec<String>,
    /// Stable handler id for entries with real logic (`trigger:` prefix;
    /// count-monitored <100, doc 20 §5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handler_id: Option<String>,
    /// Transcription note (vendor details the restricted predicate can't
    /// express / was deliberately omitted; an anchor for manual
    /// cross-checking).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Vendor origin (a `Modules/CalcTriggers.lua` line range).
    pub vendor_ref: String,
    /// Whether this has been manually verified against vendor's behavior
    /// (stored as false by default, flipped to true once verified).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub verified: bool,
}

/// Top level of `overlay/trigger_configs.json` (the consumer ignores `_meta`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct TriggerConfigsDef {
    /// Config list, ascending by `key.name`; entry count is reconciled
    /// against vendor's configTable (= 61).
    pub configs: Vec<TriggerConfigDef>,
}
