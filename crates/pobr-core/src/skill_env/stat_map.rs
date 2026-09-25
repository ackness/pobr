//! Stat-map domain fetch points (engine-semantics layer): maps an effect's statSet
//! stats through the data engine's domain channels (primary / curse / debuff /
//! player-buff / global-only) into attributed `Modifier`s.
//!
//! Moved from `pobr-build`'s `stat_map.rs`: each function takes the catalog + an
//! optional Compare-mode record sink instead of `CalculationContext`, so the same
//! mapping can be reused without the orchestrator.

use pobr_data::source::{ModifierSource, SourceId, SourceKind};

use crate::Modifier;
use crate::rules::stat_map_engine::{self, MappedItem, MappedOutcome, StatMapCatalog};
use crate::skill_env::EffectStats;

/// The statmap mapping channel.
///
/// A runtime enum rather than a cargo feature: a dual-run completes within a single
/// process, making reporting easy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatMapMode {
    /// The data engine (`overlay/skill_stat_map.json` + `rules/stat_map_engine`).
    /// **Default**.
    #[default]
    Data,
    /// Observation comparison: the Data computation + recording a mapping outcome per
    /// stat (**output identical to Data**; pure observation that changes no computed
    /// result). Kept as a long-term comparison framework — config / parser dual-runs
    /// reuse the same pattern.
    Compare,
}

/// A single mapping-level outcome observation record produced by Compare mode (one per stat).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatMapCompareRecord {
    /// The stat's stable id.
    pub stat: String,
    /// The call site's label (skill / gem.<id>.qN / support id).
    pub label: String,
    /// Classification: `mapped` / `unsupported` / `unknown` (an observation of the data
    /// channel's outcome).
    pub classification: &'static str,
    /// Detail (the list of injected items / the Unsupported category).
    pub detail: String,
}

/// The Compare-mode record sink: `Some(&mut Vec)` collects observations, `None` = Data
/// mode (pure mapping, zero overhead). Bundled with the catalog so every fetch point
/// has a single context argument.
pub struct StatMapCtx<'a> {
    /// The stat-map catalog (`None` = old data pack; every channel misses entirely).
    pub catalog: Option<&'a StatMapCatalog>,
    /// The mapping channel mode.
    pub mode: StatMapMode,
    /// The Compare-mode record sink (only written when `mode == Compare`).
    pub records: Option<&'a mut Vec<StatMapCompareRecord>>,
}

impl<'a> StatMapCtx<'a> {
    /// A data-mode context over an optional catalog (no observation records).
    pub fn data(catalog: Option<&'a StatMapCatalog>) -> Self {
        Self {
            catalog,
            mode: StatMapMode::Data,
            records: None,
        }
    }

    fn record(&mut self, stat: &str, label: String, classification: &'static str, detail: String) {
        if self.mode != StatMapMode::Compare {
            return;
        }
        if let Some(records) = self.records.as_deref_mut() {
            records.push(StatMapCompareRecord {
                stat: stat.to_string(),
                label,
                classification,
                detail,
            });
        }
    }
}

/// Classifies one mapping outcome for Compare-mode recording (shared shape across the
/// domain fetch points): `mapped` (with the domain-prefixed injected list),
/// `unsupported:<category>`, or `None` (Mapped-empty / Unknown — not recorded).
fn classify_outcome(outcome: &MappedOutcome, domain: &str) -> Option<(&'static str, String)> {
    match outcome {
        MappedOutcome::Mapped(items) if !items.is_empty() => {
            let mut injected: Vec<(String, &'static str, f64)> = items
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
            injected.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            Some(("mapped", format!("{domain}={injected:?}")))
        }
        MappedOutcome::Unsupported(reason) => {
            Some(("unsupported", format!("unsupported:{}", reason.category())))
        }
        _ => None,
    }
}

/// Maps a set of resolved stats into modifiers attributed with `source_kind` — the
/// statmap channel's dispatch point: Data goes through
/// [`stat_map_engine::map_stat`]; Compare additionally records a mapping outcome
/// observation per stat (output identical to Data). Stats that can't be mapped
/// (Unsupported / Unknown) are silently skipped; zero values are skipped.
///
/// `effect_id`: the granted effect the stat belongs to, for per-statSet override
/// lookup. `set_key`: the decimal string of the **selected** statSet's vendor 1-based
/// export index; `None` = the engine automatically uses the default set "1" override.
pub fn mapped_stat_modifiers(
    ctx: &mut StatMapCtx<'_>,
    stats: &[pobr_data::catalog::SkillDamageStat],
    source_kind: SourceKind,
    label_prefix: &str,
    effect_id: &str,
    set_key: Option<&str>,
) -> Vec<Modifier> {
    if ctx.mode == StatMapMode::Compare {
        for ds in stats {
            if ds.value == 0.0 {
                continue;
            }
            let outcome = match ctx.catalog {
                Some(c) => stat_map_engine::map_stat(c, effect_id, set_key, &ds.stat, ds.value),
                None => MappedOutcome::Unknown,
            };
            let record = match &outcome {
                MappedOutcome::Mapped(items) => {
                    let mut injected: Vec<(String, &'static str, f64)> = items
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
                    injected.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                    Some(("mapped", format!("data={injected:?}")))
                }
                MappedOutcome::Unsupported(reason) => {
                    Some(("unsupported", format!("unsupported:{}", reason.category())))
                }
                MappedOutcome::Unknown => Some(("unknown", String::new())),
            };
            if let Some((classification, detail)) = record {
                ctx.record(&ds.stat, label_prefix.to_string(), classification, detail);
            }
        }
    }
    data_mapped_stat_modifiers(
        stats,
        source_kind,
        label_prefix,
        effect_id,
        set_key,
        ctx.catalog,
    )
}

/// Data channel: the statmap data engine. See [`mapped_stat_modifiers`]'s doc for the
/// effect context + selected-set override key; `SkillData` items have no consumer yet
/// and are ignored; Unsupported / Unknown are silently skipped.
pub fn data_mapped_stat_modifiers(
    stats: &[pobr_data::catalog::SkillDamageStat],
    source_kind: SourceKind,
    label_prefix: &str,
    effect_id: &str,
    set_key: Option<&str>,
    catalog: Option<&StatMapCatalog>,
) -> Vec<Modifier> {
    crate::skill_env::data_mapped_stat_modifiers(
        stats,
        source_kind,
        label_prefix,
        effect_id,
        set_key,
        catalog,
    )
}

/// The curse-effect mod fetch point: maps every stat in a curse skill's statset,
/// through [`stat_map_engine::map_curse_stat`] (the curse domain's data channel), into
/// a list of **enemy-side** modifiers (BuffSpec.mods payload, consumed by buff_pass's
/// curse path).
///
/// - Attribution: `(SkillGem, "curse.<skill_id>.<stat>")`, buff_pass scaling preserves origin;
/// - Visibility: Compare mode records each stat's curse payload as `mapped` /
///   `unsupported:<category>` (label = `curse.<skill_id>`); `Mapped(empty)` / `Unknown`
///   aren't recorded. Data mode silently skips.
pub fn curse_stat_modifiers(
    ctx: &mut StatMapCtx<'_>,
    stats: &EffectStats,
    skill_id: &str,
    set_key: Option<&str>,
) -> Vec<Modifier> {
    let Some(catalog) = ctx.catalog else {
        return Vec::new(); // No catalog (old data pack): curse mods miss entirely.
    };
    let mut mods = Vec::new();
    for ds in stats.all() {
        if ds.value == 0.0 {
            continue;
        }
        let outcome =
            stat_map_engine::map_curse_stat(catalog, skill_id, set_key, &ds.stat, ds.value);
        if let Some((classification, detail)) = classify_outcome(&outcome, "curse") {
            ctx.record(
                &ds.stat,
                format!("curse.{skill_id}"),
                classification,
                detail,
            );
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
/// through [`stat_map_engine::map_debuff_stat`] (the debuff domain's data channel),
/// into a list of **enemy-side** modifiers. Isomorphic to [`curse_stat_modifiers`].
pub fn debuff_stat_modifiers(
    ctx: &mut StatMapCtx<'_>,
    stats: &EffectStats,
    skill_id: &str,
    set_key: Option<&str>,
) -> Vec<Modifier> {
    let Some(catalog) = ctx.catalog else {
        return Vec::new();
    };
    let mut mods = Vec::new();
    for ds in stats.all() {
        if ds.value == 0.0 {
            continue;
        }
        let outcome =
            stat_map_engine::map_debuff_stat(catalog, skill_id, set_key, &ds.stat, ds.value);
        if let Some((classification, detail)) = classify_outcome(&outcome, "debuff") {
            ctx.record(
                &ds.stat,
                format!("debuff.{skill_id}"),
                classification,
                detail,
            );
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

/// Whether a stat snapshot has a debuff exposure payload — purely read-only (doesn't
/// record into Compare; the same stat is already recorded by the Debuff branch).
pub fn has_debuff_payload(
    catalog: Option<&StatMapCatalog>,
    stats: &EffectStats,
    skill_id: &str,
    set_key: Option<&str>,
) -> bool {
    let Some(catalog) = catalog else {
        return false;
    };
    stats.all().any(|ds| {
        ds.value != 0.0
            && matches!(
                stat_map_engine::map_debuff_stat(catalog, skill_id, set_key, &ds.stat, ds.value),
                MappedOutcome::Mapped(items) if !items.is_empty()
            )
    })
}

/// Whether an effect's statset carries an **exposure-inflicting** payload
/// (`InflictExposure` flag / `<El>ExposureChance` BASE, determined by
/// [`stat_map_engine::has_exposure_inflict_payload`]'s existence check).
pub fn has_exposure_inflict_stats(
    catalog: Option<&StatMapCatalog>,
    stats: &EffectStats,
    skill_id: &str,
    set_key: Option<&str>,
) -> bool {
    let Some(catalog) = catalog else {
        return false;
    };
    stats.all().any(|ds| {
        ds.value != 0.0
            && stat_map_engine::has_exposure_inflict_payload(catalog, skill_id, set_key, &ds.stat)
    })
}

/// The player-side buff mod fetch point: maps every stat in a buff granted effect's
/// (support / aura skill) statset, through
/// [`stat_map_engine::map_player_buff_stat`] (the buff domain's data channel), into a
/// list of **player-side** modifiers. Isomorphic to [`curse_stat_modifiers`].
///
/// Same-named stats are added together first (matching vendor CalcTools.lua:138-200's
/// `stats[stat] += value`: when the quality segment and level segment share a name,
/// they're unified before a mod is built). Without merging, two mods with the same
/// (name/type/flags/tags) would result, and buff_pass's merge_buff "same-name keeps
/// the stronger one" would drop the smaller — Elemental Conflux's q20 quality segment
/// +10 used to get silently swallowed this way.
pub fn player_buff_stat_modifiers(
    ctx: &mut StatMapCtx<'_>,
    stats: &EffectStats,
    skill_id: &str,
    set_key: Option<&str>,
) -> Vec<Modifier> {
    let Some(catalog) = ctx.catalog else {
        return Vec::new();
    };
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
            continue;
        }
        let outcome =
            stat_map_engine::map_player_buff_stat(catalog, skill_id, set_key, stat, *value);
        if let Some((classification, detail)) = classify_outcome(&outcome, "buff") {
            ctx.record(stat, format!("buff.{skill_id}"), classification, detail);
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
