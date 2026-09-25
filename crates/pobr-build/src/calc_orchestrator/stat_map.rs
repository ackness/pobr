//! Skill-stat mapping with an explicit, calculation-owned catalog and diagnostics.

use super::context::CalculationContext;

use pobr_core::Modifier;
use pobr_data::source::SourceKind;

use crate::build::{Build, SocketGroup};
use crate::build_data::BuildData;

/// A single mapping-level outcome observation record produced by Compare mode.
/// Re-exported from `pobr-core::skill_env`.
pub use pobr_core::skill_env::StatMapCompareRecord;

/// Maps a set of resolved stats into modifiers attributed with `source_kind` — the
/// statmap channel's dispatch point: Data goes through the
/// [`stat_map_engine::map_stat`] data engine; Compare = the Data computation + recording
/// a mapping outcome observation per stat (**output identical to Data**, pure
/// observation that doesn't change the result; records are retrieved via
/// [`CalculationReport::stat_map_records`]). Stats that can't be mapped (Unsupported /
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
    context: &mut CalculationContext,
    stats: &[pobr_data::catalog::SkillDamageStat],
    source_kind: SourceKind,
    label_prefix: &str,
    effect_id: &str,
    set_key: Option<&str>,
) -> Vec<Modifier> {
    let mut ctx = context.stat_map_ctx();
    pobr_core::skill_env::mapped_stat_modifiers(
        &mut ctx,
        stats,
        source_kind,
        label_prefix,
        effect_id,
        set_key,
    )
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
    context: &mut CalculationContext,
    build: &Build,
    data: &BuildData,
    main_group: Option<&SocketGroup>,
) -> Vec<Modifier> {
    // Resolve the main group's index in the enabled-group enumeration order (the
    // engine-semantics function skips by index rather than pointer identity).
    let main_index = main_group.and_then(|mg| {
        build
            .enabled_socket_groups()
            .position(|g| std::ptr::eq(g, mg))
    });
    let mut ctx = context.stat_map_ctx();
    pobr_core::skill_env::exposure_support_modifiers(&mut ctx, build, data, main_index)
}
