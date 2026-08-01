//! The environment-finalize stage dispatch framework.
//!
//! PoB2's model is "the first half of perform keeps writing to modDB, the
//! second half's defence→offence only reads and aggregates" (the
//! CalcPerform.lua stage tree). This module provides a fixed 7-stage
//! dispatch at the start of `perform`, before offence/defence: mechanics
//! like buff/aura/curse/enemy ailments are implemented in their own
//! dedicated modules by their owning track and hooked into the corresponding
//! stage slot; this file is only responsible for the **ordering**.
//!
//! Framework constraints (D1):
//! - Each stage is a local, pure `pub fn xxx(env: &mut Env)` procedure
//!   (only writes `env.player.mod_db` / `env.enemy.mod_db` /
//!   `env.cfg.conditions`), introducing no shared mutable state; every
//!   written modifier carries `SourceId` attribution (see SourceKind's
//!   `ConfigOption`/`Buff`/`Flask`/`GrantedKeystone` in pobr-data's source.rs).
//! - Each stage defaults to **no-op safe**: with no buff spec / no flask /
//!   no EnemyModifier, every output value is unchanged (a migration invariant anchor).
//! - The offence/defence order is not being adjusted here; all new
//!   mechanics happen before both of them.
//!
//! At T0, every stage was a no-op stub; each track rewrites its call body once its own module is implemented.

use std::collections::HashMap;

use pobr_data::prelude::*;

use crate::Modifier;

use super::Env;

/// The environment-finalize dispatch entry point (the sole call site, at the
/// start of `perform`). Stage order mirrors PoB2's perform stage tree
/// (omitting the unimplemented Banner/Warcry/party stages); **must not be reordered**.
pub fn env_finalize(env: &mut Env) {
    // Stage 1 (T5): merges mod-granted keystones (including flask/buff grants, idempotent deduplication).
    merge_keystones(env);
    // Stage 2 (T3): forwards the player(+minion) db's EnemyModifier LIST into the enemy db.
    forward_enemy_modifiers(env);
    // Stage 2.5: expands Mageblood legacies (vendor CalcPerform.lua:1502-1528,
    // located before the flask effect section `:1531`). Aggregates the
    // `LegacyOf*` BASE + `MagebloodEquipped` flag marker mods into real
    // armour/evasion/resistance etc. mods; a no-op without Mageblood.
    super::mageblood::apply_mageblood_legacies(env);
    // Stage 3 (T4): merges flask/charm mods per the active configuration (gated by mode_combat).
    merge_flasks_charms(env);
    // Stage 4 (T3): the nine-way buff dispatch (aura factor / curse priority+limit / debuff→enemy).
    buff_pass(env);
    // Stage 5 (T5): a second keystone merge pass (for keystones granted by buffs/flasks).
    merge_keystones(env);
    // Stage 6 (T2): equivalent to doActorMisc (flag → buff_definitions → mods).
    expand_misc_buffs(env);
    // Stage 7 (T4): applies non-damaging ailments (Chill/Shock → enemy db).
    apply_nondamaging_ailments(env);
    // Stage 7.5: bridges enemy-side condition flags → cfg conditions (vendor
    // ModStore's GetCondition semantics: a condition's truth value = the
    // conditions table **or** the aggregated modDB `Condition:<X>` FLAG --
    // a condition state applied to the enemy by a mod (e.g. body armour's
    // "Enemies in your Presence are Intimidated" → the enemy db's
    // `Condition:Intimidated` flag) is equivalent to a config checkbox.
    // pobr routes all condition consumption through `cfg.conditions`, so
    // this copies the flag aggregation result back after every enemy-side
    // injection source (item EnemyModifier forwarding / buff_pass / ailments) has landed.
    // ponytail: the allowlist currently only has Intimidated (the only
    // condition with a consumer for the enemy base condition pair); other
    // `Condition:*` flags already have their own dedicated bridge
    // (config/ailments), extend one by one as needed.
    bridge_enemy_condition_flags(env);
    // Stage 8: exposure reduction (vendor CalcPerform.lua:3214-3247 "Apply
    // exposures", after the buff loop / before offence) -- reduces every
    // `<El>Exposure BASE` in the enemy db (from config injection + buff_pass's
    // Debuff path, e.g. Frost Bomb) down to the single strongest one, folded
    // into `<El>Resist BASE -magnitude`. **The sole reduction point**:
    // `CalculationSession::apply_enemy_exposure` only injects, never reduces
    // (reducing twice = double-deducting resistance). A no-op without any exposure mod (every value unchanged).
    super::setup_env::reduce_enemy_exposure(&mut env.enemy.mod_db, &env.player.mod_db, &env.cfg);
}

/// Stages 1/5 (T5 implementation): injects keystones granted by
/// `Env::keystone_mods` into the player modDB (mirrors CalcPerform.lua:66-76's
/// mergeKeystones, matching `env.keystonesAdded`'s dedup semantics).
/// Implementation lives in [`super::keystone_merge`]; zero writes with an
/// empty map / no granting mod.
pub fn merge_keystones(env: &mut Env) {
    super::keystone_merge::merge_keystones(env);
}

/// Stage 2 (T3-C4-3): forwards the player(+minions) db's `EnemyModifier` LIST into the enemy db.
///
/// Mirrors vendor `CalcPerform.lua:486-500 applyEnemyModifiers` (verified against commit `2df5a74`):
/// - `:491`'s `actor.modDB:Tabulate(nil, nil, "EnemyModifier")` takes each
///   `value.mod` (inner); pobr's equivalent = filters outer entries matching
///   `name == "EnemyModifier" && mod_type == List && matches(cfg)` and
///   unwraps the [`crate::ModValue::NestedMods`] payload (this goes through
///   `iter_mods` directly, rather than [`crate::ModDb::list_nested`]'s
///   pass-through semantics, in order to keep the outer context for the source fallback below).
/// - `:495`'s `local source = mod.source or value.mod.source`: when inner
///   lacks a source, it falls back to the **outer** entry's `source`/`origin`
///   -- the parse layer only attaches `SourceId` attribution to the outer
///   entry (item/passive/gem ingest), so after forwarding, inner can still
///   be traced back to the original source (attribution passes through).
/// - `:487-498`'s `actor.appliedEnemyModifiers` instance cache (triggered
///   multiple times at call sites `:762` / `:1107-1111`; an already-forwarded
///   mod isn't re-injected, but distinct instances with equal values are
///   each kept). pobr's stateless equivalent within a single perform: the
///   enemy db's existing modifier identities form a **multiset** -- a
///   candidate first deducts against an existing entry with the same
///   identity, and only what's left over gets injected; repeated calls are
///   idempotent and don't swallow multiple equal-valued sources. Identity =
///   full-field equality of inner after the source fallback ([`DedupSeed`]'s
///   semantic-scalar bucketing plus the bucket's `Modifier`-derived
///   `PartialEq` for the final call, covering name/type/value/tags/flags/
///   source/origin; colliding with a native enemy db injection would
///   require every field to match, and setup_env's source strings like
///   `enemy <id>` can never collide with a mod's raw text).
///
/// No-op safe: writes no state without any `EnemyModifier` entry (a D1
/// migration invariant anchor). inner always carries `Condition:Effective`
/// (attached by the parse layer), so it doesn't match under the panel view
/// (`mode_effective == false`) aggregation -- forwarding by itself never changes existing output.
pub fn forward_enemy_modifiers(env: &mut Env) {
    let enemy_modifier = ModName::from("EnemyModifier");

    // The enemy db's existing "bucket key → (representative mod, remaining
    // count)" multiset (the base for idempotent deduction). Bucket key =
    // semantic scalars (see [`DedupSeed`]); the final identity within a
    // bucket is decided by `Modifier`'s derived `PartialEq` (covering every
    // field including tags/origin), equivalent to the old full-field Debug
    // fingerprint but without coupling to Debug text.
    let mut existing: HashMap<DedupSeed, Vec<(Modifier, usize)>> = HashMap::new();
    for m in env.enemy.mod_db.iter_mods() {
        let bucket = existing.entry(dedup_seed(m)).or_default();
        match bucket.iter_mut().find(|(rep, _)| *rep == *m) {
            Some(entry) => entry.1 += 1,
            None => bucket.push((m.clone(), 1)),
        }
    }

    // Collects read-only first (player + minions), then writes to enemy all
    // at once (separating borrows + a deterministic order).
    let mut forwarded: Vec<Modifier> = Vec::new();
    let source_dbs =
        std::iter::once(&env.player.mod_db).chain(env.minions.iter().map(|m| &m.mod_db));
    for db in source_dbs {
        for outer in db.iter_mods() {
            if outer.name != enemy_modifier
                || outer.mod_type != ModType::List
                || !outer.matches(&env.cfg)
            {
                continue;
            }
            let Some(nested) = outer.value.as_nested_mods() else {
                continue;
            };
            for inner in nested {
                let mut fwd = inner.clone();
                // vendor :495 source fallback: inner inherits the outer entry's source/origin when it lacks its own.
                if fwd.source.is_none() {
                    fwd.source = outer.source.clone();
                }
                if fwd.origin.is_none() {
                    fwd.origin = outer.origin.clone();
                }
                // Idempotent deduction: when a representative with the same
                // identity (PartialEq) and remaining count exists in the
                // bucket → deducts, no re-injection; otherwise forwarded as a new entry.
                let abated = existing
                    .get_mut(&dedup_seed(&fwd))
                    .and_then(|bucket| {
                        bucket
                            .iter_mut()
                            .find(|(rep, count)| *count > 0 && *rep == fwd)
                    })
                    .map(|entry| entry.1 -= 1)
                    .is_some();
                if !abated {
                    forwarded.push(fwd);
                }
            }
        }
    }
    for m in forwarded {
        env.enemy.mod_db.add_mod(m);
    }
}

/// The "bucket key" used for forwarding dedup (see [`forward_enemy_modifiers`]'s docs).
///
/// Replaces the old `format!("{m:?}")`: the old implementation coupled dedup
/// behavior to `Modifier`'s **non-contractual** `Debug` text, and heap-
/// allocated a full Debug String per mod on the forwarding hot path.
/// This instead builds a `Hash+Eq` bucket key from semantic scalar fields;
/// the final identity within a bucket is decided by `Modifier`'s own derived
/// `PartialEq` (covering every field including tags/origin) -- this avoids
/// laying a global `Hash/Eq` over `Modifier` (f64 plus semantics too broad
/// for that), while staying robust to future field additions (identity
/// resolution always goes through the derived `PartialEq`, so it can never
/// silently mismatch due to a hand-written mirror missing a field).
/// `name`/`mod_type`/`value` (f64→bits)/`source`/`flags`/`keyword_flags` are
/// only used for bucketing; collisions are resolved by `PartialEq` within the bucket.
#[derive(PartialEq, Eq, Hash)]
struct DedupSeed {
    name: ModName,
    mod_type: ModType,
    value: ValueSeed,
    source: Option<String>,
    flags: u64,
    keyword_flags: u64,
}

/// [`DedupSeed`]'s value bucketing projection: f64 uses `to_bits` (exact
/// bucketing, working around f64 not being `Eq`); nested payloads don't
/// enter the key (they almost never appear on the forwarding path), and all fall into the same bucket for `PartialEq` to resolve.
#[derive(PartialEq, Eq, Hash)]
enum ValueSeed {
    Number(u64),
    Bool(bool),
    Text(String),
    Nested,
}

fn dedup_seed(m: &Modifier) -> DedupSeed {
    let value = match &m.value {
        crate::ModValue::Number(n) => ValueSeed::Number(n.to_bits()),
        crate::ModValue::Bool(b) => ValueSeed::Bool(*b),
        crate::ModValue::Text(t) => ValueSeed::Text(t.clone()),
        crate::ModValue::NestedMods(_) => ValueSeed::Nested,
    };
    DedupSeed {
        name: m.name.clone(),
        mod_type: m.mod_type,
        value,
        source: m.source.clone(),
        flags: m.flags.bits(),
        keyword_flags: m.keyword_flags.bits(),
    }
}

/// Stage 3: merges flask/charm mods -- the carrier List mod's nested mods
/// are scaled by the effect factor and merged into the player db (mirrors
/// vendor `CalcPerform.lua:1429-1663` mergeFlasks/mergeCharms; line numbers verified against source).
///
/// The input is the `FlaskBuff`/`CharmBuff` payload produced by
/// [`crate::item::ingest_flask_charm`] (`ModValue::NestedMods`; only
/// **active** slots are wired up by the build layer -- vendor
/// CalcSetup.lua:1014-1028's `slot.active` gates env.flasks/charms, with flasks and charms sharing the same semantics).
///
/// Vendor mirror (line numbers):
/// - `:1656-1663` gates the whole section on `env.mode_combat` →
///   `cfg.mode_combat` (D5, default false = zero behavior);
/// - `:1405`/`:1495` flask effect = `Σ INC FlaskEffect +
///   item.flaskData.effectInc` (the local share is the payload's
///   `LocalUtilityEffect`); `:1587`/`:1602` mirror this for charms (`CharmEffect`);
/// - `:1589`'s `charmLimit = min(Override(CharmLimit) or Σ BASE CharmLimit, 3)`,
///   with the cap injected via `cfg.constants.game().charm_limit_cap` (no
///   new magic numbers allowed); `:1640-1643` charms over the limit aren't
///   merged (deducted by payload insertion order -- vendor's `pairs()`
///   order is itself undefined, so a deterministic order is used here instead);
/// - `:1493-1530`'s `ScaleAddList(modList, effectMod)`: numeric mod scaling
///   goes through [`super::buff_pass::scale_value`] (deduplicated: reuses
///   the same rounding kernel as the T1 write primitive
///   `ModDb::scale_add_mod` -- precision exceptions are looked up via
///   `Env::high_precision`, non-integer raw values follow
///   `defaultHighPrecision`'s floor, default `m_modf(round(v×scale, 2))` truncation, ModStore.lua:69-76); flags aren't scaled;
/// - `:41-63`'s `mergeBuff`: mods with the same params within the same group
///   (same base) take the **max** value rather than stacking;
/// - `:1535-1542`/`:1646-1647` conditions: `UsingFlask`/`UsingCharm` +
///   `Using<base name with spaces stripped>` + `UsingLifeFlask` (base name
///   contains "Life Flask" and lacks `CannotRecoverLifeOutsideLeech`) / `UsingManaFlask`;
/// - `:1561`'s `FlasksDoNotApplyToPlayer` → the flask's buff and conditions
///   don't land on the player at all.
///
/// Known differences: the charge/duration/recovery model
/// (flaskData.duration/charges, calcFlaskRecovery, the Mageblood special
/// case `:1387-1403`) isn't built; `MagicUtilityFlaskEffect`/`MagicCharmEffect`
/// (`:1406`/`:1588`, needs a rarity channel) and the minion-side application
/// (`:1568-1586`) aren't implemented (the earlier "highPrecisionMods'
/// per-mod precision override table isn't implemented" gap has since been
/// eliminated during deduplication -- vendor's ScaleAddMod already looks up
/// that table, so the earlier name-blind scaling was the actual deviation);
/// a charm base's always-on buff (vendor `item.base.charm.buff`, e.g. Ruby
/// Charm's `+25% to Fire Resistance`) depends on a base data column (this
/// gap is tracked in drill-findings-m3.md F8), so currently only the item's own text mod is included.
///
/// Attribution: both the carrier and its inner mods carry
/// `SourceId(SourceKind::Flask, "flask.<slot>")` (attached during ingest),
/// so every mod remains traceable after being merged and injected.
/// Idempotent: skipped when a non-List Flask-sourced mod already exists in
/// the player db (repeated perform within the same Env doesn't re-merge).
/// No-op safe: writes no state without any payload / when `mode_combat == false` (a D1 migration invariant anchor).
pub fn merge_flasks_charms(env: &mut Env) {
    use std::collections::BTreeMap;

    use crate::ModValue;
    use crate::item::{CHARM_BUFF_LIST_NAME, FLASK_BUFF_LIST_NAME, LOCAL_UTILITY_EFFECT_NAME};

    /// Vendor's mergeBuff (CalcPerform.lua:41-63): when an entry with the
    /// same params (name/type/flags/keywordFlags/tags) already exists, takes
    /// the max value rather than stacking; otherwise appends.
    fn merge_buff_max(group: &mut Vec<Modifier>, candidate: Modifier) {
        let same_params = |a: &Modifier, b: &Modifier| {
            a.name == b.name
                && a.mod_type == b.mod_type
                && a.flags == b.flags
                && a.keyword_flags == b.keyword_flags
                && a.tags == b.tags
        };
        if candidate.mod_type != ModType::List
            && let Some(existing) = group.iter_mut().find(|m| same_params(m, &candidate))
        {
            if let (Some(new), Some(old)) =
                (candidate.value.as_number(), existing.value.as_number())
                && new > old
            {
                *existing = candidate;
            }
            return;
        }
        group.push(candidate);
    }

    if !env.cfg.mode_combat {
        return;
    }
    // Idempotency guard: an existing non-List Flask-sourced mod means this Env has already merged them in.
    let already_merged = env.player.mod_db.iter_mods().any(|m| {
        m.mod_type != ModType::List
            && m.origin
                .as_ref()
                .is_some_and(|o| o.source_id.kind == SourceKind::Flask)
    });
    if already_merged {
        return;
    }

    let flask_list = ModName::from(FLASK_BUFF_LIST_NAME);
    let charm_list = ModName::from(CHARM_BUFF_LIST_NAME);
    let local_effect = ModName::from(LOCAL_UTILITY_EFFECT_NAME);

    let carriers: Vec<Modifier> = env
        .player
        .mod_db
        .iter_mods()
        .filter(|m| {
            m.mod_type == ModType::List
                && (m.name == flask_list || m.name == charm_list)
                && m.matches(&env.cfg)
                && m.value.as_nested_mods().is_some()
        })
        .cloned()
        .collect();
    if carriers.is_empty() {
        return;
    }
    // Determinism note: when charms exceed `charm_limit`, "which one gets
    // dropped" follows payload insertion order -- every charm payload shares
    // the single `charm_list` ModName, landing in one Vec bucket in the
    // ModDb, laid out consecutively in ingest slot order (not scattered
    // across HashMap buckets), so the deduction order is deterministic. See
    // the `charm_limit_caps_number_of_active_charms` test for this pinned insertion-order semantics.
    let db = &env.player.mod_db;
    let cfg = &env.cfg;
    // Rounding precision rules (deduplicated: the ScaleAddMod primitive's
    // exception table; vendor's ScaleAddList → ScaleAddMod already looks up
    // highPrecisionMods per mod, ModStore.lua:69).
    let rules = &env.high_precision;
    let flask_effect_inc = db.sum(ModType::Inc, cfg, &[ModName::from("FlaskEffect")]);
    let charm_effect_inc = db.sum(ModType::Inc, cfg, &[ModName::from("CharmEffect")]);
    let charm_limit_name = ModName::from("CharmLimit");
    let mut charm_budget = db
        .override_(cfg, charm_limit_name.clone())
        .unwrap_or_else(|| db.sum(ModType::Base, cfg, &[charm_limit_name]))
        .min(cfg.constants.game().charm_limit_cap);
    let flasks_do_not_apply = db.flag(cfg, ModName::from("FlasksDoNotApplyToPlayer"));
    let cannot_recover_life = db.flag(cfg, ModName::from("CannotRecoverLifeOutsideLeech"));

    // mergeBuff grouping: `(is_charm, base name)` → same group, same params take the max (BTreeMap for a deterministic order).
    let mut groups: BTreeMap<(bool, String), Vec<Modifier>> = BTreeMap::new();
    let mut conditions: Vec<String> = Vec::new();
    for carrier in &carriers {
        let is_charm = carrier.name == charm_list;
        if is_charm {
            if charm_budget < 1.0 {
                continue; // :1640-1643 over the charm limit.
            }
            charm_budget -= 1.0;
        } else if flasks_do_not_apply {
            continue; // :1561 (buff and conditions share this gate).
        }
        let nested = carrier.value.as_nested_mods().unwrap_or(&[]);
        let local_inc: f64 = nested
            .iter()
            .filter(|m| m.name == local_effect)
            .filter_map(|m| m.value.as_number())
            .sum();
        let global_inc = if is_charm {
            charm_effect_inc
        } else {
            flask_effect_inc
        };
        let effect_mod = 1.0 + (global_inc + local_inc) / 100.0;

        let base_name = carrier.source.clone().unwrap_or_default();
        conditions.push(if is_charm { "UsingCharm" } else { "UsingFlask" }.to_string());
        if !base_name.is_empty() {
            let compact: String = base_name.chars().filter(|c| !c.is_whitespace()).collect();
            conditions.push(format!("Using{compact}"));
        }
        if !is_charm {
            if base_name.contains("Life Flask") && !cannot_recover_life {
                conditions.push("UsingLifeFlask".to_string());
            }
            if base_name.contains("Mana Flask") {
                conditions.push("UsingManaFlask".to_string());
            }
        }

        let group = groups.entry((is_charm, base_name)).or_default();
        for inner in nested.iter().filter(|m| m.name != local_effect) {
            let mut scaled = inner.clone();
            if let Some(value) = scaled.value.as_number() {
                scaled.value = ModValue::Number(super::buff_pass::scale_value(
                    rules,
                    scaled.name.as_str(),
                    scaled.mod_type,
                    value,
                    effect_mod,
                ));
            }
            merge_buff_max(group, scaled);
        }
    }

    for group in groups.into_values() {
        for modifier in group {
            env.player.mod_db.add_mod(modifier);
        }
    }
    for condition in conditions {
        env.cfg.conditions.insert(condition, true);
    }
}

/// Stage 4 (T3 implementation): the nine-way dispatch for `Env::buff_skills`
/// (mirrors CalcPerform.lua:1831-2984; aura factor `:2102-2105` / curse
/// priority `:454-485` + limit `:2829-2833`), the whole section gated on
/// `cfg.mode_buffs`. Implementation in [`super::buff_pass`]; a no-op (every
/// value unchanged) when `mode_buffs == false` (the default) or there's no buff spec.
pub fn buff_pass(env: &mut Env) {
    super::buff_pass::buff_pass(env);
}

/// Stage 6: equivalent to doActorMisc -- built-in buff flags are expanded
/// via `Env::buff_definitions` (injected from `overlay/buff_definitions.json`)
/// into mods written back to `env.player.mod_db`, with accompanying
/// conditions written to `env.cfg.conditions` (mirrors CalcPerform.lua:503-765,
/// the whole section gated on `cfg.mode_combat` -- default false means a
/// no-op, a migration invariant anchor; B4's automatic mode_combat activation is a separate behavior commit).
///
/// Attribution: `(SourceKind::Buff, "buff.<id>")`; a def whose id has
/// already been expanded (a repeated `perform` within the same Env) is skipped, guaranteeing idempotency with no double-counting.
pub fn expand_misc_buffs(env: &mut Env) {
    use pobr_data::source::SourceKind;

    use crate::rules::buff_expander::{self, BuffExpandState};

    if !env.cfg.mode_combat || env.buff_definitions.is_empty() {
        return;
    }

    // Idempotency guard: excludes defs already expanded for this Env (decided by attribution id `buff.<id>`).
    let expanded_ids: std::collections::BTreeSet<&str> = env
        .player
        .mod_db
        .iter_mods()
        .filter_map(|m| m.origin.as_ref())
        .filter(|o| o.source_id.kind == SourceKind::Buff)
        .map(|o| o.source_id.id.as_str())
        .collect();
    let pending: Vec<_> = env
        .buff_definitions
        .iter()
        .filter(|def| !expanded_ids.contains(format!("buff.{}", def.id).as_str()))
        .cloned()
        .collect();
    if pending.is_empty() {
        return;
    }

    let expansion = buff_expander::expand_misc_buffs(
        &BuffExpandState {
            db: &env.player.mod_db,
            enemy_db: Some(&env.enemy.mod_db),
            cfg: &env.cfg,
            mode_combat: env.cfg.mode_combat,
            // Main skill context: Env has no main-skill snapshot field yet,
            // so this is None until wired up -- handlers depending on it
            // (buff:fanaticism) conservatively produce zero output.
            main_skill: None,
        },
        &pending,
        &env.buff_handler_registry,
    );
    env.player.mod_db.add_list(expansion.mods);
    env.enemy.mod_db.add_list(expansion.enemy_mods);
    for condition in expansion.conditions_set {
        env.cfg.conditions.insert(condition, true);
    }
    // Handler scalars → additively merged into cfg.multipliers (vendor's
    // `modDB.multipliers[var] += v` shape, e.g. Fortify's BuffOnSelf; see the registry::HandlerOutcome docs).
    for (var, value) in expansion.multipliers {
        *env.cfg.multipliers.entry(var).or_insert(0.0) += value;
    }
}

/// Stage 7 (T4 implementation): applies non-damaging ailments -- folds
/// Chill/Shock's Val/Base/Override and writes the result to the enemy db
/// (mirrors CalcPerform.lua:3076-3180). Implementation in
/// [`super::ailment_apply`]; a no-op without any source mod (the no-op-safe invariant holds).
pub fn apply_nondamaging_ailments(env: &mut Env) {
    super::ailment_apply::apply_nondamaging_ailments(env);
}

/// Stage 7.5: bridges enemy-side `Condition:<X>` FLAGs → `cfg.conditions["Enemy<X>"]`.
///
/// Vendor semantics (ModStore.lua's `GetCondition`): a condition's truth
/// value = the conditions table **or** the aggregated modDB
/// `Condition:<X>` FLAG -- a condition state applied to the enemy by a mod
/// (e.g. body armour's "Enemies in your Presence are Intimidated" → the
/// enemy db's `Condition:Intimidated` flag) is equivalent to a config
/// checkbox. pobr routes all condition consumption through `cfg.conditions`,
/// so this copies the aggregated flag result back
/// (`matches(cfg)` already includes gates like the Effective/EnemyInPresence tags).
fn bridge_enemy_condition_flags(env: &mut Env) {
    // ponytail: the allowlist currently only has Intimidated (the only
    // condition with a consumer for the enemy base condition pair);
    // generalizing to all `Condition:*` flags would light up every enemy
    // condition mod at once -- left for parity to call out and extend one by one.
    const BRIDGED: &[&str] = &["Intimidated"];
    for cond in BRIDGED {
        let flag = ModName::from(format!("Condition:{cond}"));
        if env.enemy.mod_db.flag(&env.cfg, flag) {
            env.cfg.conditions.insert(format!("Enemy{cond}"), true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Actor, ActorBaseStats};

    /// No-op-safe invariant (originally a full T0 no-op assertion; once
    /// stage 2 landed, its semantics upgraded to "every output value is
    /// unchanged without EnemyModifier / a buff spec / a flask", a D1 anchor).
    #[test]
    fn env_finalize_is_noop_in_t0() {
        let mut env = Env::new(Actor::new(1, ActorBaseStats::default()));
        let player_mods_before = env.player.mod_db.iter_mods().count();
        let enemy_mods_before = env.enemy.mod_db.iter_mods().count();
        let conditions_before = env.cfg.conditions.clone();

        env_finalize(&mut env);

        assert_eq!(env.player.mod_db.iter_mods().count(), player_mods_before);
        assert_eq!(env.enemy.mod_db.iter_mods().count(), enemy_mods_before);
        assert_eq!(env.cfg.conditions, conditions_before);
    }
}

/// Stage 2 `forward_enemy_modifiers` (T3-C4-3): forwarding / attribution pass-through / idempotent dedup / end-to-end.
#[cfg(test)]
mod forward_enemy_modifiers_tests {
    use super::*;
    use crate::calc::{Actor, ActorBaseStats};
    use crate::{CalcConfig, ModTag, ModValue};

    fn condition_tag(var: &str) -> ModTag {
        ModTag::condition(var, false)
    }

    /// An outer EnemyModifier (with Item source attribution) plus an inner mod (no origin, simulating the parse layer's output).
    fn curse_take_outer() -> Modifier {
        let inner = Modifier::number("DamageTaken", ModType::Inc, 6.0)
            .with_source("Enemies you Curse take 6% increased Damage")
            .with_tag(condition_tag("EnemyCursed"))
            .with_tag(condition_tag("Effective"));
        Modifier::new(
            "EnemyModifier",
            ModType::List,
            ModValue::NestedMods(vec![inner]),
        )
        .with_source("Enemies you Curse take 6% increased Damage")
        .with_origin(ModifierSource::new(SourceId::new(
            SourceKind::ItemEnchant,
            "item.helmet.enchant",
        )))
    }

    /// An effective-view cfg (enemy cursed + effective).
    fn effective_cursed_cfg() -> CalcConfig {
        CalcConfig::new()
            .with_condition("EnemyCursed", true)
            .with_mode_effective(true)
    }

    /// Forwarding baseline: the player db's EnemyModifier inner mod lands in
    /// the enemy db, and enemy-side aggregation matches under both
    /// conditions; the panel view (mode_effective=false) doesn't match
    /// (a micro-anchor for the ninja_parity invariant).
    #[test]
    fn forwards_nested_mods_to_enemy_db() {
        let mut env = Env::new(Actor::new(1, ActorBaseStats::default()));
        env.player.mod_db.add_mod(curse_take_outer());

        forward_enemy_modifiers(&mut env);

        let names = [ModName::from("DamageTaken")];
        assert_eq!(
            env.enemy
                .mod_db
                .sum(ModType::Inc, &effective_cursed_cfg(), &names),
            6.0,
            "EnemyCursed + Effective → 敌侧受伤链命中"
        );
        let panel_cfg = CalcConfig::new().with_condition("EnemyCursed", true);
        assert_eq!(
            env.enemy.mod_db.sum(ModType::Inc, &panel_cfg, &names),
            0.0,
            "面板口径（无 Effective）→ 不命中"
        );
    }

    /// Attribution pass-through: inner has no origin → falls back to the
    /// outer SourceId when forwarded (vendor `:495`'s source fallback), and
    /// the enemy db's aggregated contribution can still be traced back to the original Item source.
    #[test]
    fn forwarding_preserves_source_id_attribution() {
        let mut env = Env::new(Actor::new(1, ActorBaseStats::default()));
        env.player.mod_db.add_mod(curse_take_outer());

        forward_enemy_modifiers(&mut env);

        let contributions = env.enemy.mod_db.contributions(
            ModType::Inc,
            &effective_cursed_cfg(),
            &[ModName::from("DamageTaken")],
        );
        assert_eq!(contributions.len(), 1);
        let origin = contributions[0].origin.as_ref().expect("origin 回退外层");
        assert_eq!(
            origin.source_id,
            SourceId::new(SourceKind::ItemEnchant, "item.helmet.enchant")
        );
        // inner isn't overwritten when it has its own origin: adds another outer entry whose inner has an independent origin.
        let own_origin = ModifierSource::new(SourceId::new(SourceKind::PassiveNode, "node.123"));
        let inner = Modifier::number("ActionSpeed", ModType::Inc, -20.0)
            .with_origin(own_origin.clone())
            .with_tag(condition_tag("Effective"));
        env.player.mod_db.add_mod(
            Modifier::new(
                "EnemyModifier",
                ModType::List,
                ModValue::NestedMods(vec![inner]),
            )
            .with_origin(ModifierSource::new(SourceId::new(
                SourceKind::Item,
                "item.boots.explicit",
            ))),
        );
        forward_enemy_modifiers(&mut env);
        let contributions = env.enemy.mod_db.contributions(
            ModType::Inc,
            &CalcConfig::new().with_mode_effective(true),
            &[ModName::from("ActionSpeed")],
        );
        assert_eq!(
            contributions[0].origin.as_ref().unwrap().source_id,
            own_origin.source_id,
            "inner 自带 origin 优先（vendor `mod.source or ...` 短路语义）"
        );
    }

    /// Idempotent dedup (vendor's instance-cache semantics, `:487-498`):
    /// repeated calls don't re-inject; multiple equal-valued source entries (within the same perform) are each kept.
    #[test]
    fn forwarding_is_idempotent_and_keeps_equal_value_duplicates() {
        let mut env = Env::new(Actor::new(1, ActorBaseStats::default()));
        // Two identical source entries within the same build (e.g. two runes with the same mod).
        env.player.mod_db.add_mod(curse_take_outer());
        env.player.mod_db.add_mod(curse_take_outer());

        forward_enemy_modifiers(&mut env);
        let names = [ModName::from("DamageTaken")];
        let cfg = effective_cursed_cfg();
        assert_eq!(
            env.enemy.mod_db.sum(ModType::Inc, &cfg, &names),
            12.0,
            "值相等的两份来源各自保留（vendor 按实例缓存，不按值合并）"
        );

        // Repeated calls (vendor's applyEnemyModifiers triggers at multiple points within perform) → no re-injection.
        forward_enemy_modifiers(&mut env);
        forward_enemy_modifiers(&mut env);
        assert_eq!(
            env.enemy.mod_db.sum(ModType::Inc, &cfg, &names),
            12.0,
            "幂等：已转发条目按指纹多重集抵扣"
        );
        assert_eq!(env.enemy.mod_db.iter_mods().count(), 2);
    }

    /// A minion db's EnemyModifier is forwarded the same way (vendor `:1109`'s `applyEnemyModifiers(env.minion)`).
    #[test]
    fn forwards_from_minion_db() {
        let mut env = Env::new(Actor::new(1, ActorBaseStats::default()));
        let mut minion = Actor::new(1, ActorBaseStats::default());
        minion.mod_db.add_mod(curse_take_outer());
        env.minions.push(minion);

        forward_enemy_modifiers(&mut env);

        assert_eq!(
            env.enemy.mod_db.sum(
                ModType::Inc,
                &effective_cursed_cfg(),
                &[ModName::from("DamageTaken")]
            ),
            6.0
        );
    }

    /// End-to-end (an independent fixture, doesn't touch the ninja
    /// baseline): a session with an enemy-directed mod → perform → the
    /// enemy-side damage-taken chain takes effect under the effective DPS view (×1.06); DPS is unchanged when the condition doesn't hold.
    #[test]
    fn end_to_end_enemy_direction_text_scales_effective_dps() {
        use crate::calc::{CalculationSession, MinimalInput};

        let input = MinimalInput {
            base_life: 100.0,
            base_accuracy: 1000.0,
            enemy_evasion: 0.0,
            base_hit_min: 100.0,
            base_hit_max: 100.0,
            base_action_rate: 1.0,
            ..Default::default()
        };
        let rules = std::sync::Arc::new(crate::mod_parser::test_compiled_rules());
        let dps = |enemy_cursed: bool| {
            let cfg = CalcConfig::attack()
                .with_damage_type(DamageType::Physical)
                .with_mode_effective(true)
                .with_condition("EnemyCursed", enemy_cursed);
            let mut session = CalculationSession::new(input).with_config(cfg);
            session.set_parser_rules(rules.clone());
            session
                .add_modifier_texts(["Enemies you Curse take 6% increased Damage"])
                .expect("parses");
            session.perform_minimal().dps
        };

        let base = dps(false);
        let cursed = dps(true);
        assert!(base > 0.0, "fixture 有非零 DPS 基线");
        assert!(
            (cursed / base - 1.06).abs() < 1e-9,
            "敌侧 DamageTaken INC 6 → 有效 DPS ×1.06（实测 {:.6}）",
            cursed / base
        );
    }
}
