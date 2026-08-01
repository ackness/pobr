//! stat_map — StatMap/curse/debuff/exposure/player_buff mapping + the STAT_MAP_CTX dual-run collector.
//!
//! **Dual-run context**: [`mapped_stat_modifiers`] is a free function; its three call
//! sites (skill_base / quality / support) don't hold the orchestrator options — per the
//! §3.2 sharing rule (only touch `mapped_stat_modifiers` + an `OrchestratorOptions`
//! field, ≤3 lines of wiring in the main flow), mode and catalog are passed through the
//! thread-local context [`STAT_MAP_CTX`]: installed at the start of
//! `calculate_with_data`, reset by a guard when it goes out of scope. A single
//! calculation runs on one thread, and install/reset is deterministic, so this doesn't
//! constitute shared mutable state.

use super::*;

use pobr_core::Modifier;
use pobr_core::rules::stat_map_engine::{self, MappedItem, MappedOutcome, StatMapCatalog};
use pobr_data::source::{ModifierSource, SourceId, SourceKind};

use crate::build::{Build, SocketGroup};
use crate::build_data::BuildData;

use std::cell::RefCell;

thread_local! {
    pub(crate) static STAT_MAP_CTX: RefCell<StatMapCtx> = RefCell::new(StatMapCtx::default());
}

#[derive(Default)]
pub(crate) struct StatMapCtx {
    mode: StatMapMode,
    pub(crate) catalog: Option<std::sync::Arc<StatMapCatalog>>,
    /// Compare mode's mapping-level outcome observation records (outlives the guard,
    /// retrieved via [`take_stat_map_compare_records`]).
    compare_records: Vec<StatMapCompareRecord>,
}

/// A single mapping-level outcome observation record produced by Compare mode (one per stat).
#[derive(Debug, Clone)]
pub struct StatMapCompareRecord {
    /// The stat's stable id.
    pub stat: String,
    /// The call site's label (skill / gem.<id>.qN / support id).
    pub label: String,
    /// Classification: `mapped` / `unsupported` / `unknown` (an observation of the data
    /// channel's outcome; before Legacy was removed this was a five-way dual-run diff).
    pub classification: &'static str,
    /// Detail (the list of injected items / the Unsupported category).
    pub detail: String,
}

/// Installs this calculation's statmap context, returning a guard that resets it automatically when it goes out of scope.
pub(crate) fn install_stat_map_context(
    mode: StatMapMode,
    catalog: Option<std::sync::Arc<StatMapCatalog>>,
) -> StatMapCtxGuard {
    STAT_MAP_CTX.with(|ctx| {
        let mut ctx = ctx.borrow_mut();
        ctx.mode = mode;
        ctx.catalog = catalog;
    });
    StatMapCtxGuard
}

pub(crate) struct StatMapCtxGuard;

impl Drop for StatMapCtxGuard {
    fn drop(&mut self) {
        STAT_MAP_CTX.with(|ctx| {
            let mut ctx = ctx.borrow_mut();
            ctx.mode = StatMapMode::default();
            ctx.catalog = None;
            // compare_records is kept — the caller takes it after calculate returns.
        });
    }
}

/// Takes (and clears) the current thread's accumulated Compare mode outcome observation records.
pub fn take_stat_map_compare_records() -> Vec<StatMapCompareRecord> {
    STAT_MAP_CTX.with(|ctx| std::mem::take(&mut ctx.borrow_mut().compare_records))
}

/// Maps a set of resolved stats into modifiers attributed with `source_kind` — the
/// statmap channel's dispatch point: Data goes through the
/// [`stat_map_engine::map_stat`] data engine; Compare = the Data computation + recording
/// a mapping outcome observation per stat (**output identical to Data**, pure
/// observation that doesn't change the result; records are retrieved via
/// [`take_stat_map_compare_records`]). Stats that can't be mapped (Unsupported /
/// Unknown) are silently skipped; zero values are skipped.
///
/// `effect_id`: the granted effect the stat belongs to, for per-statSet override lookup.
/// `set_key`: the decimal string of the **selected** statSet's vendor 1-based export
/// index ([`BuildData::selected_set_key`]); `None` = the engine automatically uses the
/// default set "1" override (matching PoB2's default statSetIndex=1, vendor
/// `SkillsTab.lua:354`; across the 18 ninja builds, statSetIndex is always nil, which
/// is equivalent to None). The global-only merge for unselected sets goes through
/// [`unselected_set_global_modifiers`], not this dispatch point.
pub(crate) fn mapped_stat_modifiers(
    stats: &[pobr_data::catalog::SkillDamageStat],
    source_kind: SourceKind,
    label_prefix: &str,
    effect_id: &str,
    set_key: Option<&str>,
) -> Vec<Modifier> {
    let (mode, catalog) =
        STAT_MAP_CTX.with(|ctx| (ctx.borrow().mode, ctx.borrow().catalog.clone()));
    match mode {
        StatMapMode::Data => data_mapped_stat_modifiers(
            stats,
            source_kind,
            label_prefix,
            effect_id,
            set_key,
            catalog.as_deref(),
        ),
        StatMapMode::Compare => {
            record_stat_map_observation(
                stats,
                label_prefix,
                effect_id,
                set_key,
                catalog.as_deref(),
            );
            data_mapped_stat_modifiers(
                stats,
                source_kind,
                label_prefix,
                effect_id,
                set_key,
                catalog.as_deref(),
            )
        }
    }
}

/// Data channel: the statmap data engine. See [`mapped_stat_modifiers`]'s doc for the
/// effect context + selected-set override key; `SkillData` items have no consumer yet
/// and are ignored (don't participate in the calculation, can't cause miscalculation);
/// Unsupported / Unknown are silently skipped (classification observation goes through Compare mode).
pub(crate) fn data_mapped_stat_modifiers(
    stats: &[pobr_data::catalog::SkillDamageStat],
    source_kind: SourceKind,
    label_prefix: &str,
    effect_id: &str,
    set_key: Option<&str>,
    catalog: Option<&StatMapCatalog>,
) -> Vec<Modifier> {
    let Some(catalog) = catalog else {
        return Vec::new(); // No catalog injected: the data channel misses entirely (the blueprint's Data mode always carries a catalog).
    };
    let mut mods = Vec::new();
    for ds in stats {
        if ds.value == 0.0 {
            continue; // Skip zero-value stats (no information, matching historical semantics).
        }
        let MappedOutcome::Mapped(items) =
            stat_map_engine::map_stat(catalog, effect_id, set_key, &ds.stat, ds.value)
        else {
            continue;
        };
        for item in items {
            let MappedItem::Modifier(modifier) = item else {
                continue; // SkillData: no consumer in the first batch.
            };
            let origin = ModifierSource::new(SourceId::new(
                source_kind.clone(),
                format!("{label_prefix}.{}", ds.stat),
            ))
            .with_raw_text(format!("{label_prefix} {} ({})", ds.stat, ds.value));
            mods.push(modifier.with_origin(origin));
        }
    }
    mods
}

/// Fetches the statmap catalog (thread-local context first — injected via the
/// orchestrator options installed by `calculate_with_data`; falls back to
/// `data.stat_map_catalog` outside that context — e.g. test/tool paths that call
/// [`buff_skill_specs`] directly — both point to the same Arc in the main orchestration flow).
pub(crate) fn resolve_stat_map_catalog(data: &BuildData) -> Option<std::sync::Arc<StatMapCatalog>> {
    STAT_MAP_CTX
        .with(|ctx| ctx.borrow().catalog.clone())
        .or_else(|| data.stat_map_catalog.clone())
}

/// The curse-effect mod fetch point: maps every stat in a curse skill's statset,
/// through [`stat_map_engine::map_curse_stat`] (the curse domain's data channel), into
/// a list of **enemy-side** modifiers (BuffSpec.mods payload, consumed by buff_pass's
/// curse path).
///
/// - Catalog fetch: thread-local context first (injected by the orchestrator options
///   installed by `calculate_with_data`), falls back to `data.stat_map_catalog` outside
///   that context (e.g. test/tool paths that call [`buff_skill_specs`] directly) — both
///   point to the same Arc in the main orchestration flow.
/// - Attribution: `(SkillGem, "curse.<skill_id>.<stat>")` (same semantics as the aura
///   path), buff_pass scaling preserves origin (not dropped in trace).
/// - Visibility (not silent): Compare mode records each stat's curse payload as
///   `mapped` / `unsupported:<category>` into [`StatMapCompareRecord`] (label =
///   `curse.<skill_id>`); `Mapped(empty)` (not a curse payload, goes through the main
///   skill channel) and `Unknown` (no catalog entry) aren't recorded — they aren't
///   curse semantics, so recording them would just be noise. Data mode silently skips,
///   matching the statmap primary channel's semantics (classification observation goes
///   through Compare).
pub(crate) fn curse_stat_modifiers(
    data: &BuildData,
    stats: &crate::build_data::EffectStats,
    skill_id: &str,
    set_key: Option<&str>,
) -> Vec<Modifier> {
    let mode = STAT_MAP_CTX.with(|ctx| ctx.borrow().mode);
    let Some(catalog) = resolve_stat_map_catalog(data) else {
        return Vec::new(); // No catalog (old data pack): curse mods miss entirely (matching the primary channel's semantics).
    };
    let mut mods = Vec::new();
    for ds in stats.all() {
        if ds.value == 0.0 {
            continue; // Skip zero-value stats (matching the primary channel's semantics).
        }
        let outcome =
            stat_map_engine::map_curse_stat(&catalog, skill_id, set_key, &ds.stat, ds.value);
        // Compare mode visibility recording (a line specific to curse payloads).
        if mode == StatMapMode::Compare {
            let record = match &outcome {
                MappedOutcome::Mapped(items) if !items.is_empty() => {
                    let injected: Vec<(String, &'static str, f64)> = items
                        .iter()
                        .filter_map(|item| match item {
                            MappedItem::Modifier(m) => Some((
                                m.name.to_string(),
                                m.mod_type.as_trace_label(),
                                m.value.as_number().unwrap_or(0.0),
                            )),
                            MappedItem::SkillData { .. } => None,
                        })
                        .collect();
                    Some(("mapped", format!("curse={injected:?}")))
                }
                MappedOutcome::Unsupported(reason) => {
                    Some(("unsupported", format!("unsupported:{}", reason.category())))
                }
                _ => None, // Mapped(empty)/Unknown = not a curse payload, not recorded.
            };
            if let Some((classification, detail)) = record {
                STAT_MAP_CTX.with(|ctx| {
                    ctx.borrow_mut().compare_records.push(StatMapCompareRecord {
                        stat: ds.stat.clone(),
                        label: format!("curse.{skill_id}"),
                        classification,
                        detail,
                    });
                });
            }
        }
        let MappedOutcome::Mapped(items) = outcome else {
            continue;
        };
        for item in items {
            let MappedItem::Modifier(modifier) = item else {
                continue;
            };
            let origin = ModifierSource::new(SourceId::new(
                SourceKind::SkillGem,
                format!("curse.{skill_id}.{}", ds.stat),
            ))
            .with_raw_text(format!("curse {skill_id} {} ({})", ds.stat, ds.value));
            mods.push(modifier.with_origin(origin));
        }
    }
    mods
}

/// The debuff-effect mod fetch point: maps every stat in a debuff skill's statset,
/// through [`stat_map_engine::map_debuff_stat`] (the debuff domain's data channel, whose
/// enemy-side allowlist is currently the elemental exposure family), into a list of
/// **enemy-side** modifiers (BuffSpec.mods payload, consumed by buff_pass's Debuff
/// path). Isomorphic to [`curse_stat_modifiers`]:
/// - Catalog fetch: thread-local context first, falls back to `data.stat_map_catalog`;
/// - Attribution: `(SkillGem, "debuff.<skill_id>.<stat>")`, buff_pass scaling preserves origin;
/// - Visibility: Compare mode records each stat into [`StatMapCompareRecord`] (label =
///   `debuff.<skill_id>`); `Mapped(empty)` / `Unknown` aren't recorded (not a debuff payload).
pub(crate) fn debuff_stat_modifiers(
    data: &BuildData,
    stats: &crate::build_data::EffectStats,
    skill_id: &str,
    set_key: Option<&str>,
) -> Vec<Modifier> {
    let (mode, ctx_catalog) =
        STAT_MAP_CTX.with(|ctx| (ctx.borrow().mode, ctx.borrow().catalog.clone()));
    let catalog = ctx_catalog.or_else(|| data.stat_map_catalog.clone());
    let Some(catalog) = catalog else {
        return Vec::new(); // No catalog (old data pack): debuff mods miss entirely (matching the primary channel's semantics).
    };
    let mut mods = Vec::new();
    for ds in stats.all() {
        if ds.value == 0.0 {
            continue; // Skip zero-value stats (matching the primary channel's semantics).
        }
        let outcome =
            stat_map_engine::map_debuff_stat(&catalog, skill_id, set_key, &ds.stat, ds.value);
        if mode == StatMapMode::Compare {
            let record = match &outcome {
                MappedOutcome::Mapped(items) if !items.is_empty() => {
                    let injected: Vec<(String, &'static str, f64)> = items
                        .iter()
                        .filter_map(|item| match item {
                            MappedItem::Modifier(m) => Some((
                                m.name.to_string(),
                                m.mod_type.as_trace_label(),
                                m.value.as_number().unwrap_or(0.0),
                            )),
                            MappedItem::SkillData { .. } => None,
                        })
                        .collect();
                    Some(("mapped", format!("debuff={injected:?}")))
                }
                MappedOutcome::Unsupported(reason) => {
                    Some(("unsupported", format!("unsupported:{}", reason.category())))
                }
                _ => None, // Mapped(empty)/Unknown = not a debuff payload, not recorded.
            };
            if let Some((classification, detail)) = record {
                STAT_MAP_CTX.with(|ctx| {
                    ctx.borrow_mut().compare_records.push(StatMapCompareRecord {
                        stat: ds.stat.clone(),
                        label: format!("debuff.{skill_id}"),
                        classification,
                        detail,
                    });
                });
            }
        }
        let MappedOutcome::Mapped(items) = outcome else {
            continue;
        };
        for item in items {
            let MappedItem::Modifier(modifier) = item else {
                continue;
            };
            let origin = ModifierSource::new(SourceId::new(
                SourceKind::SkillGem,
                format!("debuff.{skill_id}.{}", ds.stat),
            ))
            .with_raw_text(format!("debuff {skill_id} {} ({})", ds.stat, ds.value));
            mods.push(modifier.with_origin(origin));
        }
    }
    mods
}

/// Whether a group has a debuff exposure payload (the host-detection step of
/// [`exposure_support_modifiers`]): uses the same fetch chain as
/// [`debuff_stat_modifiers`] but is **purely read-only** (doesn't record into Compare —
/// the same stat is already recorded by buff_skill_specs's Debuff branch, so a duplicate
/// record from this probe would just be noise).
pub(crate) fn has_debuff_payload(
    data: &BuildData,
    stats: &crate::build_data::EffectStats,
    skill_id: &str,
    set_key: Option<&str>,
) -> bool {
    let ctx_catalog = STAT_MAP_CTX.with(|ctx| ctx.borrow().catalog.clone());
    let Some(catalog) = ctx_catalog.or_else(|| data.stat_map_catalog.clone()) else {
        return false;
    };
    stats.all().any(|ds| {
        ds.value != 0.0
            && matches!(
                stat_map_engine::map_debuff_stat(&catalog, skill_id, set_key, &ds.stat, ds.value),
                MappedOutcome::Mapped(items) if !items.is_empty()
            )
    })
}

/// Whether an effect's statset carries an **exposure-inflicting** payload
/// (`InflictExposure` flag / `<El>ExposureChance` BASE, determined by
/// [`stat_map_engine::has_exposure_inflict_payload`]'s existence check) — the second
/// criterion for [`exposure_support_modifiers`]'s host detection: also holds when the
/// host's exposure capability comes from a support (Fire Exposure's
/// `inflict_exposure_for_x_ms_on_ignite` → `flag("InflictExposure", on-Ignited)`, vendor
/// SkillStatMap.lua:1701-1703) rather than its own debuff payload (vendor
/// CalcPerform.lua:3196-3200's Config exposure-source criterion
/// `HasMod("FLAG", "InflictExposure")` checks the skillModList after supports are
/// merged in). A zero-value stat doesn't count (same semantics as [`has_debuff_payload`]).
pub(crate) fn has_exposure_inflict_stats(
    data: &BuildData,
    stats: &crate::build_data::EffectStats,
    skill_id: &str,
    set_key: Option<&str>,
) -> bool {
    let ctx_catalog = STAT_MAP_CTX.with(|ctx| ctx.borrow().catalog.clone());
    let Some(catalog) = ctx_catalog.or_else(|| data.stat_map_catalog.clone()) else {
        return false;
    };
    stats.all().any(|ds| {
        ds.value != 0.0
            && stat_map_engine::has_exposure_inflict_payload(&catalog, skill_id, set_key, &ds.stat)
    })
}

/// The injection surface for exposure-effect supports outside the main group (noted as
/// h3, same root cause as Potent Exposure).
///
/// Vendor: a support's mods are merged into the host skill's skillModList
/// (CalcActiveSkill.lua:210-214's effectList); when exposure is applied, the
/// `<El>ExposureEffect` INC is taken **per source skill**
/// (CalcPerform.lua:3193-3211's getSkillExposureEffect, :3226-3231 scales each
/// exposure source independently) — a Potent Exposure support
/// (`exposure_effect_+%`, SkillStatMap.lua:1731-1735) outside the main group also
/// applies to its own host (e.g. the chronomancer ascendancy's Frost Bomb in a
/// secondary group). PoBR's exposure reduction (`reduce_enemy_exposure`) reads a flat
/// sum from the player db (a noted approximation); the equivalent injection surface is
/// to globally inject the compatible supports' `<El>ExposureEffect` mods from
/// **whichever group the exposure source is in** into the player db:
/// - Only scans exposure-host groups — either criterion holding (a group with no
///   exposure source doesn't have its exposure-effect mods apply globally, keeping the
///   smallest extension of vendor's scoping semantics):
///   1. The active skill itself produces a debuff exposure payload
///      ([`has_debuff_payload`], the shape of Frost Bomb's
///      `active_skill_all_elemental_exposure_magnitude`);
///   2. The active skill or a compatible support carries an exposure-inflicting payload
///      ([`has_exposure_inflict_stats`]: `InflictExposure` flag /
///      `<El>ExposureChance`, the shape of the Fire Exposure support's
///      `inflict_exposure_for_x_ms_on_ignite` — vendor's Config exposure-source
///      criterion CalcPerform.lua:3196-3200 checks the skillModList after supports are merged in).
/// - **Skips the main group** (its supports are already fully injected by
///   [`support_modifiers`], including this name family, to avoid double injection);
/// - Only keeps `<El>ExposureEffect`-named mods (every other support mod is still
///   skill-local semantics and must not leak from a non-main group into the global bucket).
///
/// Noted approximation (a multi-source scenario; every corpus sample is single-source):
/// vendor scales each exposure source independently as `global + that source skill's
/// skill INC` and takes the max (:3226-3231); PoBR does a flat global sum — if multiple
/// exposure-host groups each carry an exposure-effect support, PoBR's sum would
/// over-count (vendor takes each source's own value). Elemental Equilibrium skipping
/// exposure on an already-hit element (:3216-3219) and setting a
/// `Condition:Has<El>Exposure` flag (:3242-3244) aren't implemented (no corpus sample
/// combines EE + exposure, and there's no consumer for that condition).
pub(crate) fn exposure_support_modifiers(
    build: &Build,
    data: &BuildData,
    main_group: Option<&SocketGroup>,
) -> Vec<Modifier> {
    use std::collections::BTreeSet;
    let mut mods = Vec::new();
    for group in build.enabled_socket_groups() {
        if main_group.is_some_and(|mg| std::ptr::eq(mg, group)) {
            continue;
        }
        // Exposure-source host: the group's active skill itself produces a debuff
        // exposure payload, or itself/a compatible support carries an
        // exposure-inflicting payload → that group's compatible support list.
        let mut support_entries: BTreeSet<(usize, String)> = BTreeSet::new();
        for gem in &group.gem_skills {
            let Some(effect) = data.granted_effects.get(&gem.skill_id) else {
                continue;
            };
            if effect.is_support {
                continue;
            }
            let es = data.effect_stats(
                &gem.skill_id,
                gem.gem_level,
                gem.quality,
                gem.stat_set_index,
            );
            let set_key = data.selected_set_key(&gem.skill_id, gem.stat_set_index);
            let judgement = judge_group_supports(group, data, &gem.skill_id);
            let is_host = has_debuff_payload(data, &es, &gem.skill_id, set_key.as_deref())
                || has_exposure_inflict_stats(data, &es, &gem.skill_id, set_key.as_deref())
                || judgement.compatible.iter().any(|sup| {
                    let host = &group.gem_skills[sup.gem_index];
                    // Quality passed as 0, matching support_modifiers's semantics.
                    let set_index = sup.stat_set_index(group);
                    let sup_stats = data.effect_stats(&sup.effect_id, host.gem_level, 0, set_index);
                    let sup_key = data.selected_set_key(&sup.effect_id, set_index);
                    has_exposure_inflict_stats(data, &sup_stats, &sup.effect_id, sup_key.as_deref())
                });
            if !is_host {
                continue;
            }
            for sup in judgement.compatible {
                support_entries.insert((sup.gem_index, sup.effect_id));
            }
        }
        for (idx, effect_id) in support_entries {
            let gem = &group.gem_skills[idx];
            let set_index = (gem.skill_id == effect_id)
                .then_some(gem.stat_set_index)
                .flatten();
            // Quality passed as 0, matching support_modifiers's semantics (supports have no quality table entries).
            let stats = data.effect_stats(&effect_id, gem.gem_level, 0, set_index);
            let set_key = data.selected_set_key(&effect_id, set_index);
            mods.extend(
                mapped_stat_modifiers(
                    &stats.base,
                    SourceKind::SupportGem,
                    &effect_id,
                    &effect_id,
                    set_key.as_deref(),
                )
                .into_iter()
                .filter(|m| m.name.as_str().ends_with("ExposureEffect")),
            );
        }
    }
    mods
}

/// The player-side buff mod fetch point: maps every stat in a buff granted effect's
/// (support / aura skill) statset, through [`stat_map_engine::map_player_buff_stat`]
/// (the buff domain's data channel, the player-side allowlist), into a list of
/// **player-side** modifiers (BuffSpec.mods payload, consumed by buff_pass's Buff/Aura
/// path). Isomorphic to [`curse_stat_modifiers`]:
/// - Catalog fetch: thread-local context first, falls back to `data.stat_map_catalog`;
/// - Attribution: `(SkillGem, "buff.<skill_id>.<stat>")`, buff_pass scaling preserves origin;
/// - Visibility: Compare mode records each stat into [`StatMapCompareRecord`] (label =
///   `buff.<skill_id>`); `Mapped(empty)` / `Unknown` aren't recorded (not a buff payload).
pub(crate) fn player_buff_stat_modifiers(
    data: &BuildData,
    stats: &crate::build_data::EffectStats,
    skill_id: &str,
    set_key: Option<&str>,
) -> Vec<Modifier> {
    let (mode, ctx_catalog) =
        STAT_MAP_CTX.with(|ctx| (ctx.borrow().mode, ctx.borrow().catalog.clone()));
    let catalog = ctx_catalog.or_else(|| data.stat_map_catalog.clone());
    let Some(catalog) = catalog else {
        return Vec::new(); // No catalog (old data pack): buff mods miss entirely (matching the primary channel's semantics).
    };
    // Same-named stats are added together first (matching vendor CalcTools.lua:138-200's
    // buildSkillInstanceStats `stats[stat] += value`: when the quality segment and level
    // segment share a name, they're unified before a mod is built). Without merging,
    // two mods with the same (name/type/flags/tags) would result, and buff_pass's
    // merge_buff "same-name keeps the stronger one" (matching vendor's mergeBuff
    // CalcPerform.lua:41-63) would drop the smaller one — Elemental Conflux's q20
    // quality segment +10 used to get silently swallowed this way.
    let mut merged: Vec<(String, f64)> = Vec::new();
    for ds in stats.all() {
        match merged.iter_mut().find(|(stat, _)| *stat == ds.stat) {
            Some((_, value)) => *value += ds.value,
            None => merged.push((ds.stat.clone(), ds.value)),
        }
    }
    let mut mods = Vec::new();
    for (stat, value) in &merged {
        if *value == 0.0 {
            continue; // Skip zero-value stats (matching the primary channel's semantics).
        }
        let outcome =
            stat_map_engine::map_player_buff_stat(&catalog, skill_id, set_key, stat, *value);
        if mode == StatMapMode::Compare {
            let record = match &outcome {
                MappedOutcome::Mapped(items) if !items.is_empty() => {
                    let injected: Vec<(String, &'static str, f64)> = items
                        .iter()
                        .filter_map(|item| match item {
                            MappedItem::Modifier(m) => Some((
                                m.name.to_string(),
                                m.mod_type.as_trace_label(),
                                m.value.as_number().unwrap_or(0.0),
                            )),
                            MappedItem::SkillData { .. } => None,
                        })
                        .collect();
                    Some(("mapped", format!("buff={injected:?}")))
                }
                MappedOutcome::Unsupported(reason) => {
                    Some(("unsupported", format!("unsupported:{}", reason.category())))
                }
                _ => None, // Mapped(empty)/Unknown = not a player-side buff payload, not recorded.
            };
            if let Some((classification, detail)) = record {
                STAT_MAP_CTX.with(|ctx| {
                    ctx.borrow_mut().compare_records.push(StatMapCompareRecord {
                        stat: stat.clone(),
                        label: format!("buff.{skill_id}"),
                        classification,
                        detail,
                    });
                });
            }
        }
        let MappedOutcome::Mapped(items) = outcome else {
            continue;
        };
        for item in items {
            let MappedItem::Modifier(modifier) = item else {
                continue;
            };
            let origin = ModifierSource::new(SourceId::new(
                SourceKind::SkillGem,
                format!("buff.{skill_id}.{stat}"),
            ))
            .with_raw_text(format!("buff {skill_id} {stat} ({value})"));
            mods.push(modifier.with_origin(origin));
        }
    }
    mods
}

/// Compare mode: records the data channel's mapping outcome observation per stat
/// (classified as `mapped` / `unsupported:<category>` / `unknown`) into the thread-local
/// buffer. The Legacy heuristic has been removed (T2.4); this function is kept as a
/// long-term comparison/observation framework — config / parser dual-runs reuse the same
/// pattern (a deliberate decision to keep the enum and reporting framework around).
pub(crate) fn record_stat_map_observation(
    stats: &[pobr_data::catalog::SkillDamageStat],
    label_prefix: &str,
    effect_id: &str,
    set_key: Option<&str>,
    catalog: Option<&StatMapCatalog>,
) {
    for ds in stats {
        if ds.value == 0.0 {
            continue;
        }
        // The data channel's outcome (effect context + selected-set override key, matching the Data channel's semantics).
        let outcome = match catalog {
            Some(c) => stat_map_engine::map_stat(c, effect_id, set_key, &ds.stat, ds.value),
            None => MappedOutcome::Unknown,
        };
        let (classification, detail): (&'static str, String) = match &outcome {
            MappedOutcome::Mapped(items) => {
                let mut injected: Vec<(String, &'static str, f64)> = items
                    .iter()
                    .filter_map(|item| match item {
                        MappedItem::Modifier(m) => Some((
                            m.name.to_string(),
                            m.mod_type.as_trace_label(),
                            m.value.as_number().unwrap_or(0.0),
                        )),
                        MappedItem::SkillData { .. } => None, // No consumer, not counted
                    })
                    .collect();
                injected.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                ("mapped", format!("data={injected:?}"))
            }
            MappedOutcome::Unsupported(reason) => {
                ("unsupported", format!("unsupported:{}", reason.category()))
            }
            MappedOutcome::Unknown => ("unknown", String::new()),
        };
        STAT_MAP_CTX.with(|ctx| {
            ctx.borrow_mut().compare_records.push(StatMapCompareRecord {
                stat: ds.stat.clone(),
                label: label_prefix.to_string(),
                classification,
                detail,
            });
        });
    }
}
