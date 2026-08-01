//! The nine-way buff dispatch.
//!
//! Mirrors PoB2 `Modules/CalcPerform.lua` (verified against vendor commit `2df5a74`):
//! - **The aura self path** `:2085-2120` (`buff.type == "Aura"`, the self factor `:2102-2105`);
//! - **The debuff path** `:2219-2285` (`buff.type == "Debuff"`, the DebuffEffect factor `:2274-2278`);
//! - **The curse path** `:2286-2337` (curse table entry construction + the CurseEffect factor);
//! - **Curse priority** `:454-485` (`determineCursePriority`, data table =
//!   `overlay/curse_priority.json`, schema [`CursePriorityDef`]);
//! - **Curse limit + slot assignment** `:2829-2920` (`output.EnemyCurseLimit`
//!   `:2830`, slot filling `:2853-2876`, `ignoreCurseLimit` appending beyond
//!   slots `:2882-2896`, enemy-side application `:2969-2984`);
//! - **ScaleAddList rounding** `Classes/ModStore.lua:45-79` (`ScaleAddMod`,
//!   see [`scale_value`]) + `Modules/Data.lua:413-530`
//!   (`defaultHighPrecision` / `highPrecisionMods`);
//! - **mergeBuff same-name strongest-wins** `Modules/CalcPerform.lua:41-63`.
//!
//! Gating (D5): the whole section is gated on `cfg.mode_buffs` (pobr-build's
//! orchestration entry point always sets this true for the MAIN view,
//! matching vendor's non-CALCS-mode buffMode always being `"EFFECTIVE"`,
//! CalcSetup.lua:583-597; still defaults to false -- every existing caller
//! that doesn't explicitly set it is unaffected value-for-value); the
//! curse/debuff sections are additionally gated on `cfg.mode_effective`, matching vendor.
//!
//! After the C5 switchover (every value across the 18-build display was
//! unchanged), this path is the sole channel for aura -- the orchestration
//! layer's static direct-injection of `aura_buff_modifiers` and the
//! `buff-pass-aura` feature gate have both been removed; the fallback
//! channel would be to revert the switchover/deletion commit.
//!
//! ## Semantic simplification checklist
//!
//! - (a) `skill_db` = `player.mod_db`'s global aggregate (pobr has no
//!   per-skill modlist layering). The gap: an "AuraEffect that only applies
//!   to a specific aura" mod (rare, fix per-build when it comes up).
//! - (b) The ally-strongest-wins branch (`allyBuffs`, `:2105`) is always
//!   empty -- party isn't implemented, this branch isn't implemented.
//! - (c) `auraCannotAffectSelf` (`:2102`): the granted_effect data column
//!   isn't landed, always treated as false.
//! - (d) The `ExtraAuraEffect` extra mod list (`:2089-2101`) isn't migrated
//!   -- pobr's parse layer doesn't produce this ModName yet, add per the C5
//!   diff report when it comes up.
//! - (e) `highPrecisionMods` (Data.lua:415-530): after deduplication, this
//!   is **data-driven** -- [`scale_value`] directly consumes the T1 write
//!   primitive [`crate::ModDb::scale_add_mod`], with precision exceptions
//!   looked up via `Env::high_precision` (injected from
//!   `overlay/high_precision_mods.json` by the orchestration layer; not
//!   injected = falls back to no exception table). The earlier hardcoded
//!   name-family mirror table has been removed. `mod.unscalable`
//!   (ModStore.lua:46-52) has no corresponding bit in pobr's mod model, not
//!   implemented (recorded with the same semantics on the T1 primitive side).
//! - (f) Debuff's `stackVar`/`stackLimit` (`:2221-2230`): the `BuffSpec`
//!   contract (frozen at T0) has no stack field, treated as `stackCount = 1`
//!   (vendor's `skillData.stackCount or 1` default).
//! - (g) Curse priority's source weight: pobr doesn't model "equipment
//!   implicit curse / aura-applied curse" (`socketGroup.source` / the
//!   Blasphemy family), the orchestration layer always passes
//!   [`CurseSourceWeight::None`]; [`determine_curse_priority`] keeps the
//!   parameter, and the table-driven tests cover it with vendor's numeric samples.
//! - (h) The `SelfCast<name>` condition (`:2288`), `socketedCursesHexLimit`
//!   (`:2294`/`:2899-2914`), and `CurseBuff`'s buffModList secondary scaling
//!   (`:2318-2330`) are absent -- `CurseBuff` and every other unconsumed kind
//!   take the "inject raw value directly" compatibility path.
//! - (i) The buff name table: curse names are derived by the orchestration
//!   layer from the `active_skill`'s snake_case name (`snipers_mark` →
//!   `Snipers Mark`); when this doesn't match vendor's apostrophe name
//!   (`Sniper's Mark`), `curse_base` fails to look it up → base value 0
//!   (matching vendor's `data.cursePriority[curseName] or 0` fallback semantics).
//! - (j) Curse effect mods (the original "mods always empty" simplification
//!   **has since been implemented**): the orchestration layer's
//!   `buff_skill_specs` maps statset stats to enemy-side modifiers into
//!   `spec.mods` via the statmap curse domain
//!   (`stat_map_engine::map_curse_stat`, mirroring each curse statSet's
//!   `GlobalEffect effectType=Curse` entries in vendor); this path applies
//!   the CurseEffect factor (`:2295-2305`) plus `Condition:Effective`, and
//!   then writes the slotted curse into the enemy db (`:2969-2984`).
//!   **Remaining gap**: the enemy-side ModName allowlist = pobr's existing
//!   consumers (`<Type>Resist` / `Damage` / `SelfCritMultiplier` /
//!   `BuffExpireFaster`; `ElementalResist` expands into equal-value fire/cold/
//!   lightning entries), and names with no consumer
//!   (`TemporalChainsActionSpeed` / `FreezeBuildup` / `ElectrocuteBuildup` /
//!   `IgnoreArmour` / `Dummy`) are recorded as whole-line Unsupported in the
//!   Compare visibility report and not injected; `GlobalEffect` entries
//!   carrying extra gating keys like `effectCond`/`modCond` are likewise skipped from reporting.
//!
//! Attribution: aura/curse/debuff scaled output preserves its original
//! `origin` (never dropped from the trace); a mod with no origin falls back
//! to `(SourceKind::Buff, "aura.<skill_id>" / "curse.<skill_id>" /
//! "buff.<skill_id>")`; the scaling multiplier is recorded in `raw_text`.

use std::collections::BTreeMap;

use pobr_data::catalog::curse_priority::CursePriorityDef;
use pobr_data::prelude::*;
use pobr_data::source::SourceKind;

use crate::{HighPrecisionRules, ModDb, ModTag, ModValue, Modifier};

use super::Env;
use super::session::BuffKind;
use super::survivability::{ChargeKind, charge_maximum};

/// Player's inherent curse limit baseline (vendor `CalcSetup.lua:648`'s
/// `NewMod("EnemyCurseLimit","BASE",1,"Base")`; data mirror =
/// `base/base_player_mods.json::enemy_curse_limit`, value-for-value tested
/// in `pobr-gamedata/tests/load_base_player_mods.rs`).
pub const DEFAULT_ENEMY_CURSE_LIMIT: f64 = 1.0;

/// Player's inherent mark limit baseline (vendor `CalcSetup.lua:649`; data mirror = `base_player_mods.json::enemy_mark_limit`).
pub const DEFAULT_ENEMY_MARK_LIMIT: f64 = 1.0;

/// The ceiling on socket order entering priority (vendor `CalcPerform.lua:465`'s
/// `m_min(k, 8)` -- avoids colliding with the `CurseFromEquipment` weight range).
const SOCKET_INDEX_CAP: u32 = 8;

/// The ModName set aggregated for the aura self factor (vendor
/// `CalcPerform.lua:2103-2104`, INC/MORE share the same name set).
const AURA_SELF_EFFECT_NAMES: [&str; 6] = [
    "AuraEffect",
    "BuffEffect",
    "BuffEffectOnSelf",
    "AuraEffectOnSelf",
    "AuraBuffEffect",
    "SkillAuraEffectOnSelf",
];

/// Curse priority's source weight dimension (vendor
/// `determineCursePriority` `:473-478`: an aura source → `CurseFromAura`, an
/// equipment source → `CurseFromEquipment`). The orchestration layer doesn't
/// model either source, always passing [`Self::None`] (simplification (g));
/// the dimension is kept so table-driven tests can cover it with vendor's numeric samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurseSourceWeight {
    /// Cast by an ordinary gem (vendor `source == ""` and not an aura, weight 0).
    None,
    /// Applied by an aura (the Blasphemy family, always the highest weight tier).
    Aura,
    /// Equipment implicit curse (Ring 2/3 slot weight folds back to Ring 1, `:480-483`).
    Equipment,
}

/// Curse panel output (produced by `buff_pass`, copied back into
/// [`super::OutputTable`]'s `enemy_curse_limit` / `curse_slots` at the end
/// of `perform` via [`Env::curse_pass_output`]; "not extending
/// display_catalog, only OutputTable fields").
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CursePassOutput {
    /// `output.EnemyCurseLimit` (vendor `:2830`).
    pub enemy_curse_limit: f64,
    /// Curse slot occupancy list (order = vendor `curseSlots`'s merge order:
    /// hex slots → mark slots → `ignoreCurseLimit` appended beyond slots, `:2878-2896`).
    pub curse_slots: Vec<String>,
}

/// A curse candidate that has passed the gate (pobr's equivalent of vendor `:2290-2298`'s `curse` table entry).
#[derive(Debug, Clone)]
struct CurseEntry {
    name: String,
    priority: i64,
    is_mark: bool,
    ignore_curse_limit: bool,
    mods: Vec<Modifier>,
}

/// Priority calculation (vendor `determineCursePriority`, CalcPerform.lua:454-485):
/// `base + min(socket_index, 8) × SocketPriorityBase + slot_weight + source_weight`.
///
/// - `slot` has its `" (Swap)"` / `" Swap"` suffix stripped before the table
///   lookup (vendor `:471`'s `slot:gsub(" (Swap)","")`; the parentheses in
///   the Lua pattern are a capture group, so it actually matches the literal
///   `" Swap"` -- both spellings are stripped for compatibility); a failed lookup = 0.
/// - `socket_index` is 1-based (vendor's `ipairs` order, defaults to 1); clamped to `[1, 8]`.
/// - When the source is equipment and the slot weight is Ring 2/Ring 3, it
///   folds back to Ring 1 (`:480-483`, equipment implicit curses don't distinguish rings).
/// - A failed `curse_base` lookup = 0 (vendor `data.cursePriority[curseName] or 0`).
pub fn determine_curse_priority(
    data: &CursePriorityDef,
    curse_name: &str,
    slot: Option<&str>,
    socket_index: u32,
    source: CurseSourceWeight,
) -> i64 {
    let base = data.curse_base.get(curse_name).copied().unwrap_or(0);
    let socket = i64::from(socket_index.clamp(1, SOCKET_INDEX_CAP)) * data.socket_priority_base;
    let slot_key = slot
        .map(|s| {
            s.trim_end_matches(" (Swap)")
                .trim_end_matches(" Swap")
                .to_string()
        })
        .unwrap_or_default();
    let mut slot_weight = data.slot_weights.get(&slot_key).copied().unwrap_or(0);
    if source == CurseSourceWeight::Equipment {
        let ring = |name: &str| data.slot_weights.get(name).copied();
        if Some(slot_weight) == ring("Ring 2") || Some(slot_weight) == ring("Ring 3") {
            slot_weight = ring("Ring 1").unwrap_or(slot_weight);
        }
    }
    let source_weight = match source {
        CurseSourceWeight::None => 0,
        CurseSourceWeight::Aura => data.curse_from_aura,
        CurseSourceWeight::Equipment => data.curse_from_equipment,
    };
    base + socket + slot_weight + source_weight
}

/// ScaleAddMod's value scaling semantics (vendor `ModStore.lua:45-79`) --
/// deduplicated: **directly consumes the T1 write primitive**
/// [`ModDb::scale_add_mod`] (the single implementation of this vendor
/// section, covering the precision exception lookup / non-integer raw value
/// `defaultHighPrecision` floor / default `m_modf(round(·,2))` truncation
/// three branches). `rules` = `overlay/high_precision_mods.json`'s
/// data-driven exception table (injected via [`Env::high_precision`];
/// replaces the earlier hardcoded name-family mirror).
///
/// Implementation: goes through a single-mod scratch db running the
/// primitive, then reads the value back -- under the no-code-movement
/// constraint, mod_db's rounding kernel exposes no value-level entry point,
/// so a scratch db is a "consume only, never mutate" reuse channel (a single
/// bucket, single mod, deterministic read-back; buff/flask scaling isn't a
/// hot path, so the overhead is negligible). `scale == 1` returns the raw
/// value directly (`:54`, an early exit with the same semantics as the primitive, avoiding the scratch overhead).
pub fn scale_value(
    rules: &HighPrecisionRules,
    name: &str,
    mod_type: ModType,
    value: f64,
    scale: f64,
) -> f64 {
    if scale == 1.0 {
        return value;
    }
    let mut db = ModDb::new();
    db.scale_add_mod(Modifier::number(name, mod_type, value), scale, rules);
    db.iter_mods()
        .next()
        .and_then(|m| m.value.as_number())
        .unwrap_or(value)
}

/// Applies the effect factor (equivalent to ScaleAddMod) to a buff mod and
/// sorts out its attribution: preserves the original `origin` (never dropped
/// from the trace), falling back to `(SourceKind::Buff, fallback_source_id)`
/// when there's no origin; the scaling multiplier is recorded in `raw_text`.
/// Non-numeric payloads (Flag/Text/NestedMods) pass through unchanged
/// (vendor only scales `value.mod` for table payloads; pobr buff mods are always numeric/Flag).
fn scale_buff_mod(
    rules: &HighPrecisionRules,
    modifier: &Modifier,
    mult: f64,
    fallback_source_id: &str,
) -> Modifier {
    let mut out = modifier.clone();
    if let ModValue::Number(v) = out.value {
        out.value = ModValue::Number(scale_value(rules, out.name.as_str(), out.mod_type, v, mult));
    }
    let scale_note = format!("buff effect ×{mult:.4}");
    match out.origin.as_mut() {
        Some(origin) => {
            if mult != 1.0 {
                origin.raw_text = Some(match origin.raw_text.take() {
                    Some(text) => format!("{text} ({scale_note})"),
                    None => scale_note,
                });
            }
        }
        None => {
            out.origin = Some(
                ModifierSource::new(SourceId::new(
                    SourceKind::Buff,
                    fallback_source_id.to_string(),
                ))
                .with_raw_text(scale_note),
            );
        }
    }
    out
}

/// mergeBuff's same-name strongest-wins merge (vendor `CalcPerform.lua:41-63`):
/// within the same buff-name bucket, when "the mod's params match"
/// (name/type/flags/keyword_flags/tags) and both are numeric, takes the
/// larger; LIST payloads are always appended (`:48`'s `mod.type ~= "LIST"`).
fn merge_buff(dest: &mut Vec<Modifier>, src: Vec<Modifier>) {
    for incoming in src {
        if incoming.mod_type != ModType::List
            && let Some(existing) = dest.iter_mut().find(|m| {
                m.name == incoming.name
                    && m.mod_type == incoming.mod_type
                    && m.flags == incoming.flags
                    && m.keyword_flags == incoming.keyword_flags
                    && m.tags == incoming.tags
            })
        {
            if let (Some(old), Some(new)) = (existing.value.as_number(), incoming.value.as_number())
                && new > old
            {
                *existing = incoming;
            }
            continue;
        }
        dest.push(incoming);
    }
}

/// Ensures a mod carries the `Condition:Effective` tag.
fn ensure_effective_tag(modifier: &mut Modifier) {
    let already = modifier.tags.iter().any(|tag| {
        matches!(tag, ModTag::Condition { var, negated: false, actor: None } if var == "Effective")
    });
    if !already {
        modifier.tags.push(ModTag::condition("Effective", false));
    }
}

/// Buff name → `AffectedBy<name with spaces stripped>` condition name (vendor `:2110`'s `buff.name:gsub(" ","")`).
fn affected_by_condition(name: &str) -> String {
    format!("AffectedBy{}", name.replace(' ', ""))
}

/// env_finalize stage 4 implementation body: `Env::buff_skills`'s nine-way dispatch.
///
/// Consumes the Aura / Curse / Debuff kinds; the remaining kinds take the
/// "inject raw value directly" compatibility path (no mod scaling, no
/// condition set). A no-op for the whole section (every value unchanged)
/// when `cfg.mode_buffs == false` (default) or there's no spec.
pub fn buff_pass(env: &mut Env) {
    if !env.cfg.mode_buffs || env.buff_skills.is_empty() {
        return;
    }
    let specs = env.buff_skills.clone();
    let priority_data = env.curse_priority.clone().unwrap_or_default();
    // Rounding precision rules (the ScaleAddMod primitive's exception table, injected by the orchestration layer; not injected = default).
    let rules = env.high_precision.clone();

    // Collection stage (read-only env)
    // Player-side buffs (aura, etc.) bucketed by buff name (mergeBuff's same-name strongest-wins); BTreeMap keeps a deterministic order.
    let mut player_buffs: BTreeMap<String, Vec<Modifier>> = BTreeMap::new();
    // Enemy-side debuffs bucketed by buff name (vendor's `debuffs` table).
    let mut enemy_debuffs: BTreeMap<String, Vec<Modifier>> = BTreeMap::new();
    // The "inject raw value directly" compatibility path for unconsumed kinds.
    let mut passthrough: Vec<Modifier> = Vec::new();
    let mut curses: Vec<CurseEntry> = Vec::new();
    let mut conditions: Vec<String> = Vec::new();

    for spec in &specs {
        match spec.kind {
            BuffKind::Aura => {
                // vendor :2204-2205: the self factor (simplification (a):
                // skill_db = player.mod_db's global aggregate;
                // (b) ally-strongest-wins is always empty;
                // (c) auraCannotAffectSelf is always false).
                // Per-skill cfg: the spec carries the source effect's skill
                // type bits (vendor's skillCfg), so scoped mods (e.g.
                // "Banner Skills have N% increased Aura Magnitudes"'s
                // SkillTypes(Banner) tag) only match the corresponding aura.
                let aura_cfg = env.cfg.clone().with_skill_types(spec.skill_types);
                let names: Vec<ModName> = AURA_SELF_EFFECT_NAMES
                    .iter()
                    .map(|n| ModName::from(*n))
                    .collect();
                let inc = env.player.mod_db.sum(ModType::Inc, &aura_cfg, &names);
                let more = env.player.mod_db.more(&aura_cfg, &names) * spec.magnitude;
                // `Magnitude` is an independent factor (vendor `:2205`'s
                // trailing `× calcLib.mod(skillCfg, "Magnitude")` -- kept in
                // its own bucket separate from the AuraEffect name set, each
                // forming `(1+Σinc/100)×Πmore` and then **multiplied
                // together**, not summed into the same bucket. E.g. the mod
                // "Aura Skills have N% increased Magnitudes" → Magnitude INC + a SkillTypes(Aura) tag).
                let mag_names = [ModName::from("Magnitude")];
                let mag_mult = (1.0
                    + env.player.mod_db.sum(ModType::Inc, &aura_cfg, &mag_names) / 100.0)
                    * env.player.mod_db.more(&aura_cfg, &mag_names);
                let mult = (1.0 + inc / 100.0) * more * mag_mult;
                // vendor :2107-2110: sets the conditions.
                conditions.push("AffectedByAura".to_string());
                conditions.push(affected_by_condition(&spec.name));
                let source_id = format!("aura.{}", spec.skill_id);
                let scaled = spec
                    .mods
                    .iter()
                    .map(|m| scale_buff_mod(&rules, m, mult, &source_id))
                    .collect();
                merge_buff(player_buffs.entry(spec.name.clone()).or_default(), scaled);
            }
            BuffKind::Buff => {
                // vendor :1949-1962: a player self-buff (e.g. the Precision
                // family of supports) enters the player db through the
                // BuffEffect factor, plus an AffectedBy condition.
                // Simplification (same semantics as Aura's (a)): modStore =
                // player.mod_db's global aggregate;
                // `skillModList:Sum(INC, <name>Effect)` (`:1957`'s dedicated
                // per-buff-name factor, e.g. `PrecisionIIEffect`) has no
                // corresponding ModName produced by pobr's parse layer, not implemented;
                // `applyNotPlayer` / the totem gate (`:1950-1953`) isn't
                // modeled, always treated as applying to the player.
                let inc_names = [
                    ModName::from("BuffEffect"),
                    ModName::from("BuffEffectOnSelf"),
                    ModName::from("BuffEffectOnPlayer"),
                ];
                let more_names = [
                    ModName::from("BuffEffect"),
                    ModName::from("BuffEffectOnSelf"),
                ];
                let inc = env.player.mod_db.sum(ModType::Inc, &env.cfg, &inc_names);
                let more = env.player.mod_db.more(&env.cfg, &more_names) * spec.magnitude;
                let mult = (1.0 + inc / 100.0) * more;
                // vendor :1955: `modDB.conditions["AffectedBy"..buff.name] = true`.
                conditions.push(affected_by_condition(&spec.name));
                let source_id = format!("buff.{}", spec.skill_id);
                let scaled = spec
                    .mods
                    .iter()
                    .map(|m| scale_buff_mod(&rules, m, mult, &source_id))
                    .collect();
                merge_buff(player_buffs.entry(spec.name.clone()).or_default(), scaled);
            }
            BuffKind::Curse => {
                // vendor :2289's gate: `(mode_effective and (not Hexproof or exempt)) or mark`.
                let hexproof = env.enemy.mod_db.flag(&env.cfg, ModName::from("Hexproof"));
                let ignores_hexproof = env
                    .player
                    .mod_db
                    .flag(&env.cfg, ModName::from("CursesIgnoreHexproof"))
                    || spec.ignore_curse_limit;
                if !((env.cfg.mode_effective && (!hexproof || ignores_hexproof)) || spec.is_mark) {
                    continue;
                }
                // vendor :2295-2305: the CurseEffect factor. Originally
                // documented as `INC(CurseEffect, BuffEffect)`, but a direct
                // reading of `:2295` shows
                // `Sum(INC, CurseEffect) + enemyDB Sum(INC, CurseEffectOnSelf)`
                // (no BuffEffect), aligned with the actual vendor source;
                // the aura source's extra AuraEffect INC (`:2296-2298`) isn't
                // implemented (simplification (g): no aura source modeled).
                let curse_effect = [ModName::from("CurseEffect")];
                let curse_effect_on_self = [ModName::from("CurseEffectOnSelf")];
                // spec.local_effect_inc/more = vendor skillModList's
                // skill-local CurseEffect section (gem quality / in-group
                // support, pre-folded by the orchestration layer).
                let inc = env.player.mod_db.sum(ModType::Inc, &env.cfg, &curse_effect)
                    + spec.local_effect_inc
                    + env
                        .enemy
                        .mod_db
                        .sum(ModType::Inc, &env.cfg, &curse_effect_on_self);
                let mut more = env.player.mod_db.more(&env.cfg, &curse_effect)
                    * spec.local_effect_more
                    * spec.magnitude;
                if !spec.is_mark {
                    // vendor :2303-2305: non-marks additionally multiply by the enemy-side CurseEffectOnSelf MORE.
                    more *= env.enemy.mod_db.more(&env.cfg, &curse_effect_on_self);
                }
                let mult = (1.0 + inc / 100.0) * more;
                if dbg_env!("POBR_DBG_CURSE").is_some() {
                    eprintln!(
                        "[POBR_CURSE] entry name={} inc={inc:.2} (local_inc={:.2}) more={more:.4} (local_more={:.4} magnitude={:.4}) mult={mult:.4} mods={:?}",
                        spec.name,
                        spec.local_effect_inc,
                        spec.local_effect_more,
                        spec.magnitude,
                        spec.mods
                            .iter()
                            .map(|m| (&m.name, &m.value))
                            .collect::<Vec<_>>(),
                    );
                }
                let source_id = format!("curse.{}", spec.skill_id);
                let mods = spec
                    .mods
                    .iter()
                    .map(|m| {
                        let mut scaled = scale_buff_mod(&rules, m, mult, &source_id);
                        // Enemy-side writes are gated on Condition:Effective (matching the existing enemy-side semantics).
                        ensure_effective_tag(&mut scaled);
                        scaled
                    })
                    .collect();
                // vendor :2294's `ignoreCurseLimit = (...) and not mark or false`.
                let ignore_curse_limit = (env
                    .player
                    .mod_db
                    .flag(&env.cfg, ModName::from("CursesIgnoreCurseLimit"))
                    || spec.ignore_curse_limit)
                    && !spec.is_mark;
                curses.push(CurseEntry {
                    name: spec.name.clone(),
                    priority: determine_curse_priority(
                        &priority_data,
                        &spec.name,
                        spec.slot.as_deref(),
                        spec.socket_index,
                        CurseSourceWeight::None, // Simplification (g): aura/equipment sources not modeled.
                    ),
                    is_mark: spec.is_mark,
                    ignore_curse_limit,
                    mods,
                });
            }
            BuffKind::Debuff => {
                // vendor :2219-2285 (the Debuff branch): gated on
                // mode_effective + the DebuffEffect factor; stackCount = 1 (simplification (f)).
                if !env.cfg.mode_effective {
                    continue;
                }
                let names = [ModName::from("DebuffEffect")];
                let inc = env.player.mod_db.sum(ModType::Inc, &env.cfg, &names);
                let more = env.player.mod_db.more(&env.cfg, &names) * spec.magnitude;
                let mult = (1.0 + inc / 100.0) * more;
                conditions.push(affected_by_condition(&spec.name));
                let source_id = format!("buff.{}", spec.skill_id);
                let scaled = spec
                    .mods
                    .iter()
                    .map(|m| scale_buff_mod(&rules, m, mult, &source_id))
                    .collect();
                merge_buff(enemy_debuffs.entry(spec.name.clone()).or_default(), scaled);
            }
            // Every other kind (Guard/Warcry/AuraDebuff/CurseBuff/Link):
            // takes the framework's "inject raw value directly"
            // compatibility path (no scaling, no condition set; the
            // orchestration layer doesn't currently construct these kinds,
            // so behavior matches the current state).
            _ => {
                let source_id = format!("buff.{}", spec.skill_id);
                passthrough.extend(
                    spec.mods
                        .iter()
                        .map(|m| scale_buff_mod(&rules, m, 1.0, &source_id)),
                );
            }
        }
    }

    // Curse limit + slot assignment (vendor :2829-2896)
    // limit: override(CurseLimitIsMaximumPowerCharges → PowerChargesMax) else
    // baseline 1 (CalcSetup.lua:648, mirrored in base_player_mods.json) + Σ BASE.
    let curse_limit = if env
        .player
        .mod_db
        .flag(&env.cfg, ModName::from("CurseLimitIsMaximumPowerCharges"))
    {
        f64::from(charge_maximum(
            &env.player.mod_db,
            &env.cfg,
            ChargeKind::Power,
        ))
    } else {
        DEFAULT_ENEMY_CURSE_LIMIT
            + env
                .player
                .mod_db
                .sum(ModType::Base, &env.cfg, &[ModName::from("EnemyCurseLimit")])
    };
    let mark_limit = DEFAULT_ENEMY_MARK_LIMIT
        + env
            .player
            .mod_db
            .sum(ModType::Base, &env.cfg, &[ModName::from("EnemyMarkLimit")]);

    // Slot filling (vendor :2845-2876): curse/mark slots are **separate**;
    // same-name higher priority replaces, lower is skipped; a different name
    // replaces "the last lower-priority slot found while scanning" (a literal mirror of vendor's loop semantics).
    let mut curse_slots: Vec<Option<CurseEntry>> = vec![None; curse_limit.max(0.0) as usize];
    let mut mark_slots: Vec<Option<CurseEntry>> = vec![None; mark_limit.max(0.0) as usize];
    for curse in curses.iter().filter(|c| !c.ignore_curse_limit) {
        let slots = if curse.is_mark {
            &mut mark_slots
        } else {
            &mut curse_slots
        };
        let mut target: Option<usize> = None;
        for (i, slot) in slots.iter().enumerate() {
            match slot {
                None => {
                    target = Some(i);
                    break;
                }
                Some(existing) if existing.name == curse.name => {
                    target = (existing.priority < curse.priority).then_some(i);
                    break;
                }
                Some(existing) => {
                    if existing.priority < curse.priority {
                        target = Some(i);
                    }
                }
            }
        }
        if let Some(i) = target {
            slots[i] = Some(curse.clone());
        }
    }
    // Merge (vendor `:2879`'s `tableConcat(curseSlots, markSlots)`).
    let mut occupied: Vec<CurseEntry> = curse_slots
        .into_iter()
        .chain(mark_slots)
        .flatten()
        .collect();
    // Appended beyond the slots for `ignoreCurseLimit` (vendor :2882-2896: same-name higher replaces, else skipped; a different name is appended).
    for curse in curses.iter().filter(|c| c.ignore_curse_limit) {
        match occupied.iter_mut().find(|c| c.name == curse.name) {
            Some(existing) => {
                if existing.priority < curse.priority {
                    *existing = curse.clone();
                }
            }
            None => occupied.push(curse.clone()),
        }
    }

    // Write stage (only writes player/enemy mod_db + cfg.conditions/multipliers + the output bridge)
    let buff_on_self = player_buffs.len() as f64;
    for (_, mods) in player_buffs {
        env.player.mod_db.add_list(mods);
    }
    if buff_on_self > 0.0 {
        // vendor :2949-2951: each effective buff counts toward multipliers["BuffOnSelf"].
        *env.cfg
            .multipliers
            .entry("BuffOnSelf".to_string())
            .or_insert(0.0) += buff_on_self;
    }
    env.player.mod_db.add_list(passthrough);
    for (_, mods) in enemy_debuffs {
        env.enemy.mod_db.add_list(mods);
    }
    // Curse slot application (vendor :2969-2984): enemy-side conditions + modList injection + the CurseOnEnemy multiplier.
    env.cfg
        .multipliers
        .insert("CurseOnEnemy".to_string(), occupied.len() as f64);
    if dbg_env!("POBR_DBG_CURSE").is_some() {
        eprintln!(
            "[POBR_CURSE] occupied={} limit={curse_limit} names={:?}",
            occupied.len(),
            occupied.iter().map(|s| s.name.clone()).collect::<Vec<_>>()
        );
    }
    let mut slot_names = Vec::with_capacity(occupied.len());
    for slot in occupied {
        conditions.push("EnemyCursed".to_string());
        if slot.is_mark {
            conditions.push("EnemyMarked".to_string());
        }
        env.enemy.mod_db.add_list(slot.mods);
        slot_names.push(slot.name);
    }
    for condition in conditions {
        env.cfg.conditions.insert(condition, true);
    }
    env.curse_pass_output = Some(CursePassOutput {
        enemy_curse_limit: curse_limit,
        curse_slots: slot_names,
    });
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::CalcConfig;
    use crate::calc::{Actor, ActorBaseStats, BuffSpec};

    /// A numeric mirror of vendor `Modules/Data.lua:274`'s `data.cursePriority`
    /// (value-for-value matching `overlay/curse_priority.json`; pobr-core is zero-I/O, so the test builds this inline).
    fn vendor_priority_data() -> CursePriorityDef {
        CursePriorityDef {
            curse_base: BTreeMap::from(
                [
                    ("Temporal Chains", 1),
                    ("Enfeeble", 2),
                    ("Vulnerability", 3),
                    ("Elemental Weakness", 4),
                    ("Flammability", 5),
                    ("Frostbite", 6),
                    ("Conductivity", 7),
                    ("Despair", 8),
                    ("Punishment", 9),
                    ("Warlord's Mark", 10),
                    ("Assassin's Mark", 11),
                    ("Sniper's Mark", 12),
                    ("Poacher's Mark", 13),
                ]
                .map(|(k, v)| (k.to_string(), v)),
            ),
            socket_priority_base: 100,
            slot_weights: BTreeMap::from(
                [
                    ("Weapon 1", 1000),
                    ("Amulet", 2000),
                    ("Helmet", 3000),
                    ("Weapon 2", 4000),
                    ("Body Armour", 5000),
                    ("Gloves", 6000),
                    ("Boots", 7000),
                    ("Ring 1", 8000),
                    ("Ring 2", 9000),
                    ("Ring 3", 10000),
                ]
                .map(|(k, v)| (k.to_string(), v)),
            ),
            curse_from_equipment: 11000,
            curse_from_aura: 20000,
        }
    }

    fn buffed_env() -> Env {
        let mut env = Env::new(Actor::new(1, ActorBaseStats::default()));
        env.cfg = CalcConfig::attack()
            .with_mode_buffs(true)
            .with_mode_effective(true);
        env.curse_priority = Some(vendor_priority_data());
        env
    }

    fn curse_spec(
        name: &str,
        slot: &str,
        socket_index: u32,
        taken_inc: f64,
        is_mark: bool,
        ignore_curse_limit: bool,
    ) -> BuffSpec {
        BuffSpec {
            name: name.to_string(),
            kind: BuffKind::Curse,
            skill_id: format!("{}Player", name.replace([' ', '\''], "")),
            mods: vec![Modifier::number("DamageTaken", ModType::Inc, taken_inc)],
            magnitude: 1.0,
            slot: Some(slot.to_string()),
            socket_index,
            is_mark,
            ignore_curse_limit,
            local_effect_inc: 0.0,
            local_effect_more: 1.0,
            skill_types: pobr_data::skill::SkillTypes::NONE,
        }
    }

    fn aura_spec(name: &str, es: f64) -> BuffSpec {
        BuffSpec {
            name: name.to_string(),
            kind: BuffKind::Aura,
            skill_id: format!("{}Player", name.replace(' ', "")),
            mods: vec![Modifier::number("EnergyShield", ModType::Base, es)],
            magnitude: 1.0,
            slot: Some("Body Armour".to_string()),
            socket_index: 1,
            is_mark: false,
            ignore_curse_limit: false,
            local_effect_inc: 0.0,
            local_effect_more: 1.0,
            skill_types: pobr_data::skill::SkillTypes::NONE,
        }
    }

    // ScaleAddMod rounding semantics (vendor ModStore.lua:45-79, reused via the T1 primitive)

    /// Builds a rule table matching `overlay/high_precision_mods.json`'s
    /// relevant entries value-for-value (an excerpt of Data.lua:415-530: only the names this test touches).
    fn vendor_precision_rules() -> HighPrecisionRules {
        use pobr_data::catalog::high_precision_mods::HighPrecisionModsDef;
        let entry = |ty: &str, p: u32| BTreeMap::from([(ty.to_string(), p)]);
        HighPrecisionRules::from_def(HighPrecisionModsDef {
            default_high_precision: 1,
            more_default_round_decimals: 2,
            mods: BTreeMap::from([
                ("CritChance".to_string(), entry("BASE", 2)),
                ("LifeRegen".to_string(), entry("BASE", 1)),
                ("LifeRegenPercent".to_string(), entry("BASE", 2)),
                ("PhysicalDamageLifeLeech".to_string(), entry("BASE", 2)),
                ("SupportManaMultiplier".to_string(), entry("MORE", 4)),
            ]),
        })
    }

    /// Integer raw values take `m_modf(round(x, 2))` (truncated toward
    /// zero); high-precision entries take floor truncation. Expected values
    /// match the pre-deduplication hardcoded mirror table value-for-value (a migration invariant anchor).
    #[test]
    fn scale_value_matches_vendor_rounding() {
        let rules = vendor_precision_rules();
        // scale == 1 → returns the raw value directly (including non-integers, :54).
        assert_eq!(
            scale_value(&rules, "EnergyShield", ModType::Base, 1.5, 1.0),
            1.5
        );
        // Integer raw value: 100 × 1.2 = 120 (exact); 33 × 1.1 = 36.3 → round(…,2) → trunc 36.
        assert_eq!(
            scale_value(&rules, "EnergyShield", ModType::Base, 100.0, 1.2),
            120.0
        );
        assert_eq!(scale_value(&rules, "Damage", ModType::Inc, 33.0, 1.1), 36.0);
        // Negative values truncate toward zero (m_modf semantics): -33 × 1.1 = -36.3 → -36.
        assert_eq!(
            scale_value(&rules, "ActionSpeed", ModType::Inc, -33.0, 1.1),
            -36.0
        );
        // Non-integer raw value → defaultHighPrecision = 1 (Data.lua:413): 1.5 × 1.3 = 1.95 → 1.9.
        assert_eq!(scale_value(&rules, "Damage", ModType::Inc, 1.5, 1.3), 1.9);
        // CritChance BASE → precision 2 (Data.lua:416-418): 5 × 1.234 = 6.17.
        assert_eq!(
            scale_value(&rules, "CritChance", ModType::Base, 5.0, 1.234),
            6.17
        );
        // LifeRegen BASE → precision 1: 7 × 1.15 = 8.05 → 8.0 (floor truncation).
        assert_eq!(
            scale_value(&rules, "LifeRegen", ModType::Base, 7.0, 1.15),
            8.0
        );
        // LifeRegenPercent BASE → precision 2.
        assert_eq!(
            scale_value(&rules, "LifeRegenPercent", ModType::Base, 1.0, 1.155),
            1.15
        );
        // The Damage*Leech family BASE → precision 2 (Data.lua:460-523).
        assert_eq!(
            scale_value(&rules, "PhysicalDamageLifeLeech", ModType::Base, 2.0, 1.333),
            2.66
        );
        // MORE SupportManaMultiplier → precision 4 (Data.lua:524-526).
        assert_eq!(
            scale_value(
                &rules,
                "SupportManaMultiplier",
                ModType::More,
                130.0,
                1.11111
            ),
            144.4443
        );
    }

    /// Without injected rules (`HighPrecisionRules::default`, no exception
    /// table): the default branch and non-integer raw value fallback are
    /// unchanged; exception entries fall back to the default `round(·,2)` truncation.
    #[test]
    fn scale_value_default_rules_fallback() {
        let rules = HighPrecisionRules::default();
        // Default branch: integer raw value round-trunc.
        assert_eq!(scale_value(&rules, "Damage", ModType::Inc, 33.0, 1.1), 36.0);
        // Non-integer raw values still take default_high_precision = 1 (independent of the exception table).
        assert_eq!(scale_value(&rules, "Damage", ModType::Inc, 1.5, 1.3), 1.9);
        // No exception table → CritChance's integer raw value falls to the default branch (5 × 1.234 = 6.17 → trunc 6).
        assert_eq!(
            scale_value(&rules, "CritChance", ModType::Base, 5.0, 1.234),
            6.0
        );
    }

    // Curse priority (vendor determineCursePriority :454-485, table-driven)

    /// Vendor's numeric samples: `base + min(socket,8)×100 + slot_weight + source_weight`.
    #[test]
    fn curse_priority_matches_vendor_samples() {
        let data = vendor_priority_data();
        // Temporal Chains (1), Ring 1 (8000), socket 2: 1 + 200 + 8000.
        assert_eq!(
            determine_curse_priority(
                &data,
                "Temporal Chains",
                Some("Ring 1"),
                2,
                CurseSourceWeight::None
            ),
            8201
        );
        // Despair (8), "Weapon 1 Swap" with the suffix stripped → Weapon 1 (1000), an aura source (20000).
        assert_eq!(
            determine_curse_priority(
                &data,
                "Despair",
                Some("Weapon 1 Swap"),
                1,
                CurseSourceWeight::Aura
            ),
            8 + 100 + 1000 + 20000
        );
        // An equipment implicit source + Ring 2 → slot weight folds back to Ring 1 (:480-483).
        assert_eq!(
            determine_curse_priority(
                &data,
                "Enfeeble",
                Some("Ring 2"),
                1,
                CurseSourceWeight::Equipment
            ),
            2 + 100 + 8000 + 11000
        );
        // Socket order clamps to 8 (:465); an unknown slot / unknown curse name → falls back to 0.
        assert_eq!(
            determine_curse_priority(&data, "Vulnerability", None, 12, CurseSourceWeight::None),
            3 + 800
        );
        assert_eq!(
            determine_curse_priority(
                &data,
                "Unknown Hex",
                Some("Flask 1"),
                1,
                CurseSourceWeight::None
            ),
            100
        );
    }

    // Gating invariant (D5 / this pass's anchor)

    /// `mode_buffs == false` (default) → the whole section is a no-op even
    /// with a spec present: both dbs / conditions / the output bridge are unchanged value-for-value.
    #[test]
    fn mode_buffs_off_is_value_identical_noop() {
        let mut env = Env::new(Actor::new(1, ActorBaseStats::default()));
        env.cfg = CalcConfig::attack().with_mode_effective(true); // mode_buffs defaults to false
        env.curse_priority = Some(vendor_priority_data());
        env.buff_skills.push(aura_spec("Discipline", 100.0));
        env.buff_skills.push(curse_spec(
            "Temporal Chains",
            "Ring 1",
            1,
            10.0,
            false,
            false,
        ));
        let conditions_before = env.cfg.conditions.clone();

        buff_pass(&mut env);

        assert_eq!(env.player.mod_db.iter_mods().count(), 0);
        assert_eq!(env.enemy.mod_db.iter_mods().count(), 0);
        assert_eq!(env.cfg.conditions, conditions_before);
        assert_eq!(env.curse_pass_output, None);
    }

    // Curse limit / slot assignment (vendor :2829-2896)

    /// 3 hexes into a 1-slot limit → the highest priority takes the slot exclusively; losing mods don't enter the enemy db.
    #[test]
    fn curse_limit_keeps_highest_priority() {
        let mut env = buffed_env();
        // priority: Despair(Boots,socket1)=8+100+7000 < Enfeeble(Ring 1)=2+100+8000
        // < Temporal Chains(Ring 1, socket 3)=1+300+8000.
        env.buff_skills
            .push(curse_spec("Despair", "Boots", 1, 5.0, false, false));
        env.buff_skills
            .push(curse_spec("Enfeeble", "Ring 1", 1, 7.0, false, false));
        env.buff_skills.push(curse_spec(
            "Temporal Chains",
            "Ring 1",
            3,
            11.0,
            false,
            false,
        ));

        buff_pass(&mut env);

        let out = env.curse_pass_output.as_ref().expect("buff_pass 已运行");
        assert_eq!(
            out.enemy_curse_limit, 1.0,
            "基线 limit = 1（CalcSetup.lua:648）"
        );
        assert_eq!(out.curse_slots, vec!["Temporal Chains".to_string()]);
        assert_eq!(
            env.enemy
                .mod_db
                .sum(ModType::Inc, &env.cfg, &[ModName::from("DamageTaken")]),
            11.0,
            "仅 priority 最高者的词条入敌 db"
        );
        assert_eq!(env.cfg.multiplier("CurseOnEnemy"), 1.0);
        assert!(env.cfg.condition("EnemyCursed"));
        assert!(!env.cfg.condition("EnemyMarked"));
    }

    /// A `+1 EnemyCurseLimit` BASE mod extends the slot count: both hexes coexist.
    #[test]
    fn curse_limit_extends_with_base_mods() {
        let mut env = buffed_env();
        env.player
            .mod_db
            .add_mod(Modifier::number("EnemyCurseLimit", ModType::Base, 1.0));
        env.buff_skills
            .push(curse_spec("Despair", "Boots", 1, 5.0, false, false));
        env.buff_skills
            .push(curse_spec("Enfeeble", "Ring 1", 1, 7.0, false, false));

        buff_pass(&mut env);

        let out = env.curse_pass_output.as_ref().unwrap();
        assert_eq!(out.enemy_curse_limit, 2.0);
        assert_eq!(out.curse_slots.len(), 2);
        assert_eq!(
            env.enemy
                .mod_db
                .sum(ModType::Inc, &env.cfg, &[ModName::from("DamageTaken")]),
            12.0
        );
    }

    /// `CurseLimitIsMaximumPowerCharges` override: limit = PowerChargesMax (vendor :2830).
    #[test]
    fn curse_limit_override_uses_power_charges_max() {
        let mut env = buffed_env();
        env.player
            .mod_db
            .add_mod(Modifier::flag("CurseLimitIsMaximumPowerCharges"));
        env.buff_skills
            .push(curse_spec("Despair", "Boots", 1, 5.0, false, false));

        buff_pass(&mut env);

        assert_eq!(
            env.curse_pass_output.as_ref().unwrap().enemy_curse_limit,
            3.0,
            "默认最大充能 3（survivability::DEFAULT_MAX_CHARGES）"
        );
    }

    /// Marks and hexes use separate slots (vendor :2835/:2852-2853): each has its own limit, neither crowds out the other.
    #[test]
    fn marks_and_hexes_use_separate_slots() {
        let mut env = buffed_env();
        env.buff_skills.push(curse_spec(
            "Temporal Chains",
            "Ring 1",
            1,
            10.0,
            false,
            false,
        ));
        env.buff_skills
            .push(curse_spec("Sniper's Mark", "Gloves", 1, 6.0, true, false));

        buff_pass(&mut env);

        let out = env.curse_pass_output.as_ref().unwrap();
        // Merge order = hex slots → mark slots (vendor's tableConcat :2879).
        assert_eq!(
            out.curse_slots,
            vec!["Temporal Chains".to_string(), "Sniper's Mark".to_string()]
        );
        assert_eq!(
            env.enemy
                .mod_db
                .sum(ModType::Inc, &env.cfg, &[ModName::from("DamageTaken")]),
            16.0
        );
        assert!(env.cfg.condition("EnemyMarked"));
        assert_eq!(env.cfg.multiplier("CurseOnEnemy"), 2.0);
    }

    /// `ignore_curse_limit` appends beyond the slots (vendor :2882-2896): still appended even after a limit-1 slot is full.
    #[test]
    fn ignore_curse_limit_appends_beyond_slots() {
        let mut env = buffed_env();
        env.buff_skills
            .push(curse_spec("Enfeeble", "Ring 1", 1, 7.0, false, false));
        env.buff_skills
            .push(curse_spec("Despair", "Boots", 1, 5.0, false, true));

        buff_pass(&mut env);

        let out = env.curse_pass_output.as_ref().unwrap();
        assert_eq!(
            out.curse_slots,
            vec!["Enfeeble".to_string(), "Despair".to_string()]
        );
        assert_eq!(
            env.enemy
                .mod_db
                .sum(ModType::Inc, &env.cfg, &[ModName::from("DamageTaken")]),
            12.0
        );
    }

    // The CurseEffect factor + Condition:Effective semantics (vendor :2295-2316)

    /// 20% inc CurseEffect → hex mods ×1.2; the enemy-side CurseEffectOnSelf
    /// MORE only applies to hexes (not marks, :2303-2305); scaled output carries Condition:Effective (doesn't match the panel view).
    #[test]
    fn curse_effect_scales_hex_not_mark() {
        let mut env = buffed_env();
        env.player
            .mod_db
            .add_mod(Modifier::number("CurseEffect", ModType::Inc, 20.0));
        env.enemy
            .mod_db
            .add_mod(Modifier::number("CurseEffectOnSelf", ModType::More, -50.0));
        env.buff_skills
            .push(curse_spec("Enfeeble", "Ring 1", 1, 10.0, false, false));
        env.buff_skills
            .push(curse_spec("Sniper's Mark", "Gloves", 1, 10.0, true, false));

        buff_pass(&mut env);

        let contributions =
            env.enemy
                .mod_db
                .contributions(ModType::Inc, &env.cfg, &[ModName::from("DamageTaken")]);
        let mut values: Vec<f64> = contributions.iter().map(|c| c.value).collect();
        values.sort_by(f64::total_cmp);
        // hex: 10 × (1.2 × 0.5) = 6; mark: 10 × 1.2 = 12.
        assert_eq!(values, vec![6.0, 12.0]);
        // The panel view (mode_effective=false) doesn't match (gated on Condition:Effective).
        let panel_cfg = CalcConfig::attack().with_mode_buffs(true);
        assert_eq!(
            env.enemy
                .mod_db
                .sum(ModType::Inc, &panel_cfg, &[ModName::from("DamageTaken")]),
            0.0
        );
        // Attribution: a curse mod with no origin falls back to (Buff, "curse.<skill_id>").
        let origin = contributions[0].origin.as_ref().expect("回退归因已附");
        assert_eq!(origin.source_id.kind, pobr_data::source::SourceKind::Buff);
        assert!(origin.source_id.id.starts_with("curse."));
    }

    /// Enemy Hexproof: the whole hex is skipped (vendor :2289's gate), marks are unaffected.
    #[test]
    fn hexproof_blocks_hex_but_not_mark() {
        let mut env = buffed_env();
        env.enemy.mod_db.add_mod(Modifier::flag("Hexproof"));
        env.buff_skills
            .push(curse_spec("Enfeeble", "Ring 1", 1, 10.0, false, false));
        env.buff_skills
            .push(curse_spec("Sniper's Mark", "Gloves", 1, 6.0, true, false));

        buff_pass(&mut env);

        assert_eq!(
            env.curse_pass_output.as_ref().unwrap().curse_slots,
            vec!["Sniper's Mark".to_string()]
        );
    }

    // BuffSpec compatibility path / double-count guard (§6.1)

    /// The Buff branch: BuffEffect factor scaling + an AffectedBy condition.
    /// 50% inc BuffEffect + a buff giving 20% INC → 30% INC.
    #[test]
    fn buff_kind_scales_with_buff_effect_and_sets_condition() {
        let mut env = buffed_env();
        env.player
            .mod_db
            .add_mod(Modifier::number("BuffEffect", ModType::Inc, 50.0));
        env.buff_skills.push(BuffSpec {
            name: "Onslaught".to_string(),
            kind: BuffKind::Buff,
            skill_id: "OnslaughtPlayer".to_string(),
            mods: vec![Modifier::number("ActionSpeed", ModType::Inc, 20.0)],
            magnitude: 1.0,
            slot: None,
            socket_index: 1,
            is_mark: false,
            ignore_curse_limit: false,
            local_effect_inc: 0.0,
            local_effect_more: 1.0,
            skill_types: pobr_data::skill::SkillTypes::NONE,
        });

        buff_pass(&mut env);

        assert_eq!(
            env.player
                .mod_db
                .sum(ModType::Inc, &env.cfg, &[ModName::from("ActionSpeed")]),
            30.0,
            "vendor :1957-1959：mult = 1 + ΣINC(BuffEffect…)/100"
        );
        assert!(env.cfg.condition("AffectedByOnslaught"), "vendor :1955");
    }

    /// An unconsumed kind (e.g. Guard) takes the inject-raw-value path: mods aren't scaled, no condition is set (behavior matches the current state).
    #[test]
    fn unconsumed_kinds_pass_through_unscaled() {
        let mut env = buffed_env();
        env.player
            .mod_db
            .add_mod(Modifier::number("BuffEffect", ModType::Inc, 50.0));
        env.buff_skills.push(BuffSpec {
            name: "Steelskin".to_string(),
            kind: BuffKind::Guard,
            skill_id: "SteelskinPlayer".to_string(),
            mods: vec![Modifier::number("ActionSpeed", ModType::Inc, 20.0)],
            magnitude: 1.0,
            slot: None,
            socket_index: 1,
            is_mark: false,
            ignore_curse_limit: false,
            local_effect_inc: 0.0,
            local_effect_more: 1.0,
            skill_types: pobr_data::skill::SkillTypes::NONE,
        });

        buff_pass(&mut env);

        assert_eq!(
            env.player
                .mod_db
                .sum(ModType::Inc, &env.cfg, &[ModName::from("ActionSpeed")]),
            20.0,
            "原值直注：BuffEffect 乘区不施加"
        );
        assert!(!env.cfg.condition("AffectedBySteelskin"));
    }

    // The aura factor (vendor :2102-2110)

    /// 20% inc AuraEffect + an aura giving 100 ES → 120.
    #[test]
    fn aura_effect_scales_buff_mods() {
        let mut env = buffed_env();
        env.player
            .mod_db
            .add_mod(Modifier::number("AuraEffect", ModType::Inc, 20.0));
        env.buff_skills.push(aura_spec("Discipline", 100.0));

        buff_pass(&mut env);

        assert_eq!(
            env.player
                .mod_db
                .sum(ModType::Base, &env.cfg, &[ModName::from("EnergyShield")]),
            120.0
        );
        assert!(env.cfg.condition("AffectedByAura"));
        assert!(env.cfg.condition("AffectedByDiscipline"));
        assert_eq!(env.cfg.multiplier("BuffOnSelf"), 1.0);
    }

    /// Same fixture, end-to-end: CalculationSession → perform → the defence output's ES = 120.
    #[test]
    fn aura_fixture_end_to_end_session() {
        use crate::calc::{CalculationSession, MinimalInput};

        let mut session = CalculationSession::new(MinimalInput {
            base_life: 100.0,
            ..Default::default()
        })
        .with_config(CalcConfig::attack().with_mode_buffs(true));
        session.add_modifiers([Modifier::number("AuraEffect", ModType::Inc, 20.0)]);
        session.add_buff_skill(aura_spec("Discipline", 100.0));

        session.perform_minimal();

        assert_eq!(session.output().energy_shield, 120.0);
    }

    /// The MORE factor and magnitude both enter mult (vendor :2104's `More(...) × calcLib.mod Magnitude`).
    #[test]
    fn aura_more_and_magnitude_multiply() {
        let mut env = buffed_env();
        env.player
            .mod_db
            .add_mod(Modifier::number("AuraBuffEffect", ModType::More, 50.0));
        let mut spec = aura_spec("Grace", 100.0);
        spec.magnitude = 1.1;
        env.buff_skills.push(spec);

        buff_pass(&mut env);

        // mult = 1.5 × 1.1 = 1.65 → 100 × 1.65 = 165.
        assert_eq!(
            env.player
                .mod_db
                .sum(ModType::Base, &env.cfg, &[ModName::from("EnergyShield")]),
            165.0
        );
    }

    /// mergeBuff's same-name strongest-wins (vendor :41-63): two specs for the same-named aura don't stack, the larger value is taken.
    #[test]
    fn same_name_auras_merge_take_strongest() {
        let mut env = buffed_env();
        env.buff_skills.push(aura_spec("Discipline", 80.0));
        env.buff_skills.push(aura_spec("Discipline", 100.0));

        buff_pass(&mut env);

        assert_eq!(
            env.player
                .mod_db
                .sum(ModType::Base, &env.cfg, &[ModName::from("EnergyShield")]),
            100.0,
            "同名 buff 同参数词条取强不叠加"
        );
    }
}
