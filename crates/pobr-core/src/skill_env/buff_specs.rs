//! Buff/warcry spec assembly + support-modifier injection (engine-semantics layer).
//!
//! Moved from `pobr-build`'s `skill/buffs.rs` + `skill/mods.rs`: builds `BuffSpec` /
//! `WarcrySpec` lists and the compatible-support modifier payload from plain lookup
//! traits + `SocketGroupView` + `StatMapCtx`, without `Build`/`BuildData`/
//! `CalculationContext`.

use pobr_data::modifier::ModType;
use pobr_data::source::{ModifierSource, SourceId, SourceKind};

use crate::Modifier;
use crate::calc::{BuffKind, BuffSpec, WarcrySpec};
use crate::rules::stat_map_engine;
use crate::skill_env::buff_stat_map::map_aura_buff_stat;
use crate::skill_env::buffs::buff_skill_name;
use crate::skill_env::lookup::{EffectLookup, SocketGroupView, StatMapLookup, StatSetLookup};
use crate::skill_env::mods::GemPropertyBonus;
use crate::skill_env::mods::skill_type_bits;
use crate::skill_env::resolve::{
    GemPropertyLookup, SkillLevelLookup, additional_gem_levels, resolve_skill_level,
    support_granted_gem_levels, support_stat_set_index,
};
use crate::skill_env::stat_map::{
    StatMapCtx, curse_stat_modifiers, debuff_stat_modifiers, mapped_stat_modifiers,
    player_buff_stat_modifiers,
};
use crate::skill_env::support::{GroupSupportJudgement, SupportCandidate, judge_group_supports};
use crate::skill_env::{EnabledGroup, GemInput};

/// The lookup surface the buff/warcry spec builders need.
pub trait BuffEnv: StatSetLookup + EffectLookup + StatMapLookup + GemPropertyLookup {}
impl<T: StatSetLookup + EffectLookup + StatMapLookup + GemPropertyLookup> BuffEnv for T {}

/// Runs the group support judgement for `skill_id` over an [`EnabledGroup`]:
/// assembles candidates from the group's gems + additional granted effects
/// (the engine-semantics counterpart of `pobr-build`'s `judge_group_supports`).
pub fn group_judgement<'a>(
    group: &EnabledGroup<'a>,
    data: &'a (dyn BuffEnv + 'a),
    skill_id: &str,
) -> GroupSupportJudgement {
    use std::collections::HashSet;

    let active_skill_types: HashSet<String> = data
        .effect(skill_id)
        .map(|e| e.skill_types.iter().cloned().collect())
        .unwrap_or_default();
    let cannot_be_supported = data.effect(skill_id).is_some_and(|e| e.cannot_be_supported);

    let candidates: Vec<SupportCandidate<'a>> = group
        .gems
        .iter()
        .enumerate()
        .flat_map(|(i, g)| {
            std::iter::once(g.skill_id.as_str())
                .chain(
                    data.additional_effects(&g.skill_id)
                        .iter()
                        .map(String::as_str),
                )
                .filter_map(move |id| {
                    data.effect(id)
                        .filter(|e| e.is_support)
                        .map(|effect| SupportCandidate {
                            gem_index: i,
                            effect_id: id,
                            effect,
                        })
                })
        })
        .collect();

    judge_group_supports(
        &active_skill_types,
        cannot_be_supported,
        group.from_gem,
        &candidates,
    )
}

/// Injects the **compatible** support gems' modifiers for `active_skill_id` in `group`
/// (the support-side half of the skill-group → modifier translation).
///
/// The current scope is **global** (correct semantics under a single-main-skill build:
/// every support's multiplier applies to the one skill being calculated); per-skill tag
/// isolation for multiple main skills is deferred until the flag system is wired up.
pub fn support_modifiers(
    ctx: &mut StatMapCtx<'_>,
    group: &EnabledGroup<'_>,
    data: &dyn BuffEnv,
    active_skill_id: &str,
) -> Vec<Modifier> {
    let judgement = group_judgement(group, data, active_skill_id);
    let mut mods = Vec::new();
    for sup in &judgement.compatible {
        let gem = &group.gems[sup.gem_index];
        let set_index = support_stat_set_index(sup, group.gems);
        // TODO(T1, add after rebasing post-T3.6 merge): change the quality argument to
        // gem.quality — supports have no quality table entries (PoB2 skips them at
        // export), so this segment is currently always empty and passing 0 is
        // equivalent to passing gem.quality.
        let stats = data.effect_stats(&sup.effect_id, gem.gem_level, 0, set_index);
        // A support's set_key is taken from its own selected set (per-set overrides are
        // located by the support's effect id). Note: vendor doesn't pass a statSet for
        // support effects (CalcActiveSkill.lua:130 does a full merge across all sets) —
        // the full merge for a multi-set support's additional sets is a current gap.
        let set_key = data.selected_set_key(&sup.effect_id, set_index);
        mods.extend(mapped_stat_modifiers(
            ctx,
            &stats.base,
            SourceKind::SupportGem,
            &sup.effect_id,
            &sup.effect_id,
            set_key.as_deref(),
        ));
        // A compatible support's per-level cost multiplier → `SupportManaMultiplier`
        // MORE (matching PoB2's `CalcActiveSkill.lua:689-691`:
        // `NewMod("SupportManaMultiplier","MORE", level.manaMultiplier, modSource)`).
        // Only injected for the **compatible list** — a rejected support's multiplier
        // doesn't apply, matching PoB2's rejection. Consumed by
        // `skill_mechanics::calc_skill_cost` (the multipliers are chained and truncated
        // to 4 decimal places, then applied to base cost before the inc/more chain).
        if let Some(m) = crate::skill_env::support_mana_multiplier_modifier(
            &sup.effect_id,
            data.effect_level_row(&sup.effect_id, gem.gem_level)
                .and_then(|row| row.mana_multiplier),
        ) {
            mods.push(m);
        }
    }
    mods
}

/// Builds every enabled active skill's buff spec (aura defensive / player-side buff /
/// enemy-side debuff / curse), matching vendor's buffList assembly
/// (`CalcActiveSkill.lua:976-1046` → `CalcPerform.lua:1949-2316`).
///
/// `gem_level_bonuses` = the build's GemProperty Level bonuses (assembled once by the
/// caller via [`crate::skill_env::gem_property_bonuses`]); `ctx` carries the statmap
/// catalog + Compare-mode sink.
pub fn buff_skill_specs(
    ctx: &mut StatMapCtx<'_>,
    groups: &dyn SocketGroupView,
    data: &dyn BuffEnv,
    gem_level_bonuses: &[GemPropertyBonus],
) -> Vec<BuffSpec> {
    use std::collections::HashSet;
    let mut specs = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    groups.for_each_enabled_group(&mut |group| {
        for (idx, gem) in group.gems.iter().enumerate() {
            // Primary granted effect + additional granted effects (the
            // overlay/gem_effects.json foreign key; vendor builds an independent
            // activeSkill for each gem's additionalGrantedEffectId1..N — e.g. a
            // banner's buff-side DefianceBannerPlayer (Aura) is in the additional slot,
            // while the primary slot is the reservation-side ReservationPlayer). Level/
            // quality/set follow the host gem instance (PoB2's additional effects share
            // the same gemInstance as the host).
            let effect_ids: Vec<&str> = std::iter::once(gem.skill_id.as_str())
                .chain(
                    data.additional_effects(&gem.skill_id)
                        .iter()
                        .map(String::as_str),
                )
                .collect();
            for skill_id in effect_ids {
                let Some(effect) = data.effect(skill_id) else {
                    continue;
                };
                if effect.is_support {
                    continue;
                }
                let has_type = |t: &str| effect.skill_types.iter().any(|x| x == t);
                let is_aura = has_type("Aura");
                let is_mark = has_type("Mark");
                let is_curse = is_mark || has_type("AppliesCurse");
                let socket_index = (idx + 1) as u32;
                if !is_aura && !is_curse {
                    // Debuff branch + player-side Buff branch (see the orchestrator's
                    // doc for the vendor mapping; both payloads are empty for most skills).
                    let es =
                        data.effect_stats(skill_id, gem.gem_level, gem.quality, gem.stat_set_index);
                    let set_key = data.selected_set_key(skill_id, gem.stat_set_index);
                    let debuff_mods = debuff_stat_modifiers(ctx, &es, skill_id, set_key.as_deref());
                    // Player-side Buff payload: the fetch level = gem level + any
                    // applicable `+N to Level of all <X> Skills` (vendor's applyGemMods
                    // applies to every gem effect, CalcSetup.lua:410-435; confirmed
                    // with Sigil: 20→32). The level granted by a support isn't modeled;
                    // noted as a residual gap.
                    let buff_level =
                        gem.gem_level + additional_gem_levels(gem_level_bonuses, data, skill_id);
                    let es_buff = if buff_level == gem.gem_level {
                        es
                    } else {
                        data.effect_stats(skill_id, buff_level, gem.quality, gem.stat_set_index)
                    };
                    let buff_mods =
                        player_buff_stat_modifiers(ctx, &es_buff, skill_id, set_key.as_deref());
                    if (debuff_mods.is_empty() && buff_mods.is_empty())
                        || !seen.insert(skill_id.to_string())
                    {
                        continue;
                    }
                    if !debuff_mods.is_empty() {
                        specs.push(BuffSpec {
                            name: buff_skill_name(data, skill_id),
                            kind: BuffKind::Debuff,
                            skill_id: skill_id.to_string(),
                            mods: debuff_mods,
                            magnitude: 1.0,
                            slot: group.slot.map(str::to_string),
                            socket_index,
                            is_mark: false,
                            ignore_curse_limit: false,
                            local_effect_inc: 0.0,
                            local_effect_more: 1.0,
                            skill_types: pobr_data::skill::SkillTypes::NONE,
                        });
                    }
                    if !buff_mods.is_empty() {
                        specs.push(BuffSpec {
                            name: buff_skill_name(data, skill_id),
                            kind: BuffKind::Buff,
                            skill_id: skill_id.to_string(),
                            mods: buff_mods,
                            magnitude: 1.0,
                            slot: group.slot.map(str::to_string),
                            socket_index,
                            is_mark: false,
                            ignore_curse_limit: false,
                            local_effect_inc: 0.0,
                            local_effect_more: 1.0,
                            skill_types: pobr_data::skill::SkillTypes::NONE,
                        });
                    }
                    continue;
                }
                if !seen.insert(skill_id.to_string()) {
                    continue;
                }
                if is_aura {
                    // Aura defensive buff: the same stat→mod mapping and SkillGem
                    // attribution as aura_buff_modifiers (buff_pass scaling preserves
                    // origin, not dropped in trace).
                    let es =
                        data.effect_stats(skill_id, gem.gem_level, gem.quality, gem.stat_set_index);
                    let mut mods = Vec::new();
                    for ds in es.all() {
                        for mapped in map_aura_buff_stat(&ds.stat) {
                            if ds.value == 0.0 {
                                continue;
                            }
                            let origin = ModifierSource::new(SourceId::new(
                                SourceKind::SkillGem,
                                format!("aura.{}.{}", gem.skill_id, ds.stat),
                            ))
                            .with_raw_text(format!(
                                "aura {} {} ({})",
                                gem.skill_id, ds.stat, ds.value
                            ));
                            mods.push(
                                Modifier::number(
                                    mapped.mod_name.as_str(),
                                    mapped.mod_type,
                                    ds.value,
                                )
                                .with_origin(origin),
                            );
                        }
                    }
                    // statmap buff domain's supplementary channel: the player-side
                    // allowlist's (Accuracy) GlobalEffect Buff/Aura payload (e.g. War
                    // Banner's `base_skill_buff_banner_accuracy_+%_to_apply` → Accuracy
                    // INC, with the Condition:BannerPlanted tag preserved as-is).
                    let set_key = data.selected_set_key(skill_id, gem.stat_set_index);
                    mods.extend(player_buff_stat_modifiers(
                        ctx,
                        &es,
                        skill_id,
                        set_key.as_deref(),
                    ));
                    specs.push(BuffSpec {
                        name: buff_skill_name(data, skill_id),
                        kind: BuffKind::Aura,
                        skill_id: skill_id.to_string(),
                        mods,
                        magnitude: 1.0,
                        slot: group.slot.map(str::to_string),
                        socket_index,
                        is_mark: false,
                        ignore_curse_limit: false,
                        local_effect_inc: 0.0,
                        local_effect_more: 1.0,
                        // vendor's per-skill skillCfg (buff_pass's multiplier zone
                        // matches domain-scoped mods — e.g. the SkillTypes(Banner) tag
                        // on "Banner Skills have N% increased Aura Magnitudes" — against
                        // this effect's own type bits).
                        skill_types: skill_type_bits(&effect.skill_types),
                    });
                } else {
                    // Curse effect mods: statset stats mapped through the statmap curse
                    // domain into enemy-side modifiers, applied by buff_pass's
                    // CurseEffect multiplier zone + Condition:Effective before entering
                    // the enemy db. Fetch level = gem level + any applicable `+N to
                    // Level of all <X> Skills` (vendor's applyGemMods applies to every
                    // gem effect — confirmed with EW: 19+8→27, payload -58→-66).
                    let curse_level =
                        gem.gem_level + additional_gem_levels(gem_level_bonuses, data, skill_id);
                    let es =
                        data.effect_stats(skill_id, curse_level, gem.quality, gem.stat_set_index);
                    let set_key = data.selected_set_key(skill_id, gem.stat_set_index);
                    // Vendor's registration precondition: buffList is built purely
                    // from GlobalEffect payloads, and a curse table entry is only
                    // constructed from buffList — a skill with **no** curse payload at
                    // all in the statMap data doesn't register as a curse: it doesn't
                    // take a slot and doesn't count toward `Multiplier:CurseOnEnemy`.
                    // Without a catalog (old data pack), keeps the existing behavior
                    // (always registers).
                    if let Some(catalog) = ctx.catalog
                        && !es.all().any(|ds| {
                            stat_map_engine::has_curse_payload(
                                catalog,
                                skill_id,
                                set_key.as_deref(),
                                &ds.stat,
                            )
                        })
                    {
                        continue;
                    }
                    let mods = curse_stat_modifiers(ctx, &es, skill_id, set_key.as_deref());
                    // The skill-local CurseEffect segment (vendor's curse multiplier
                    // zone, reads skillModList): the curse gem's own quality segment +
                    // the **compatible** supports in the group's payload, pre-scaled
                    // via the statmap global segment `curse_local_effect`.
                    let (local_effect_inc, local_effect_more) =
                        curse_local_effect_scale(ctx, &group, data, gem, skill_id, curse_level);
                    specs.push(BuffSpec {
                        name: buff_skill_name(data, skill_id),
                        kind: BuffKind::Curse,
                        skill_id: skill_id.to_string(),
                        mods,
                        magnitude: 1.0,
                        slot: group.slot.map(str::to_string),
                        socket_index,
                        is_mark,
                        ignore_curse_limit: false,
                        local_effect_inc,
                        local_effect_more,
                        skill_types: pobr_data::skill::SkillTypes::NONE,
                    });
                }
            }
        }
    });
    specs
}

/// A curse skill's **skill-local** CurseEffect multiplier-zone segment (matching vendor
/// CalcPerform.lua:2423's `skillModList:Sum("INC", skillCfg, "CurseEffect")` + :2427's
/// `More(...)`): the curse gem's own effect stats (the quality segment carries
/// `curse_effect_+%`) + the effect stats of **compatible** supports in the group.
///
/// stat → (INC, MORE) conversion goes through statmap data
/// ([`stat_map_engine::curse_local_effect`]). No catalog → (0, 1).
fn curse_local_effect_scale(
    ctx: &mut StatMapCtx<'_>,
    group: &EnabledGroup<'_>,
    data: &dyn BuffEnv,
    gem: &GemInput,
    skill_id: &str,
    curse_level: u32,
) -> (f64, f64) {
    let Some(catalog) = ctx.catalog else {
        return (0.0, 1.0);
    };
    let (mut inc, mut more) = (0.0, 1.0);
    let mut absorb = |effect_id: &str, level: u32, quality: u32, set_index: Option<u32>| {
        let es = data.effect_stats(effect_id, level, quality, set_index);
        let set_key = data.selected_set_key(effect_id, set_index);
        for ds in es.all() {
            if ds.value == 0.0 {
                continue;
            }
            let (di, dm) = stat_map_engine::curse_local_effect(
                catalog,
                effect_id,
                set_key.as_deref(),
                &ds.stat,
                ds.value,
            );
            inc += di;
            more *= dm;
        }
    };
    absorb(skill_id, curse_level, gem.quality, gem.stat_set_index);
    let judgement = group_judgement(group, data, skill_id);
    for sup in &judgement.compatible {
        let host = &group.gems[sup.gem_index];
        absorb(
            &sup.effect_id,
            host.gem_level,
            host.quality,
            support_stat_set_index(sup, group.gems),
        );
    }
    (inc, more)
}

/// The **player-side buff** granted by a support → [`BuffSpec`] (kind =
/// [`BuffKind::Buff`], applied by buff_pass's Buff branch, which applies the
/// BuffEffect multiplier zone before merging into the player db).
///
/// Vendor semantics: a support's own statSet's statMap produces a `GlobalEffect
/// effectType=Buff` mod, which applies to the player as the supported Persistent Buff
/// skill (Herald/Malice/Banner…) activates. Applicability is data-driven:
/// [`judge_group_supports`] is checked against every enabled active skill in the
/// group, and the support is injected if any is compatible; the same support effect
/// appearing in multiple groups is deduplicated by id.
pub fn support_buff_specs(
    ctx: &mut StatMapCtx<'_>,
    groups: &dyn SocketGroupView,
    data: &dyn BuffEnv,
) -> Vec<BuffSpec> {
    use std::collections::HashSet;
    let mut specs = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    groups.for_each_enabled_group(&mut |group| {
        // The group's enabled active skills (known effect and non-support). Includes
        // additional granted effects (vendor builds an independent activeSkill for
        // each additionalGrantedEffectId1..N, and a support is judged against each
        // individually).
        let active_ids: Vec<&str> = group
            .gems
            .iter()
            .flat_map(|g| {
                std::iter::once(g.skill_id.as_str()).chain(
                    data.additional_effects(&g.skill_id)
                        .iter()
                        .map(String::as_str),
                )
            })
            .filter(|id| data.effect(id).is_some_and(|e| !e.is_support))
            .collect();
        if active_ids.is_empty() {
            return;
        }
        // Included if compatible with any active skill (vendor: a support is judged
        // against each active skill in the group individually).
        let mut compatible: HashSet<(usize, String)> = HashSet::new();
        for active_id in &active_ids {
            for sup in group_judgement(&group, data, active_id).compatible {
                compatible.insert((sup.gem_index, sup.effect_id));
            }
        }
        let mut entries: Vec<(usize, String)> = compatible.into_iter().collect();
        entries.sort_unstable();
        for (idx, effect_id) in entries {
            let gem = &group.gems[idx];
            if !seen.insert(effect_id.clone()) {
                continue;
            }
            // An additionally-granted support half doesn't reuse the gem instance's
            // statSetIndex (only meaningful for the primary effect).
            let set_index = (gem.skill_id == effect_id)
                .then_some(gem.stat_set_index)
                .flatten();
            let es = data.effect_stats(&effect_id, gem.gem_level, gem.quality, set_index);
            let set_key = data.selected_set_key(&effect_id, set_index);
            let mods = player_buff_stat_modifiers(ctx, &es, &effect_id, set_key.as_deref());
            if mods.is_empty() {
                continue;
            }
            specs.push(BuffSpec {
                name: buff_skill_name(data, &effect_id),
                kind: BuffKind::Buff,
                skill_id: effect_id.clone(),
                mods,
                magnitude: 1.0,
                slot: group.slot.map(str::to_string),
                socket_index: (idx + 1) as u32,
                is_mark: false,
                ignore_curse_limit: false,
                local_effect_inc: 0.0,
                local_effect_more: 1.0,
                skill_types: pobr_data::skill::SkillTypes::NONE,
            });
        }
    });
    specs
}

/// Builds every **enabled warcry active skill** into a [`WarcrySpec`], injected via
/// `session.add_warcry_skill` and consumed by `calc::warcry` (before perform's hand
/// pass), scaled by uptime (matching vendor CalcOffence.lua:3203-3256 +
/// CalcPerform.lua:2116-2142).
///
/// Spec assembly (all through existing data channels, zero per-skill hardcoding):
/// - **skill-local mods** = the skill's own statSet stats (including the quality
///   segment) mapped via statmap + the group's **compatible support** payload +
///   `WarcryCastTime BASE` (the effect's `cast_time`);
/// - **Fetch level** = gem level + any applicable `+N to Level of ...` +
///   support-granted levels;
/// - cooldown / storedUses = the granted_effect_levels row (`resolve_skill_level`).
///
/// The same effect appearing in multiple groups is deduplicated by id.
/// The lookup surface [`warcry_skill_specs`] needs (buff env + skill-level rows).
pub trait WarcryEnv: BuffEnv + SkillLevelLookup {}
impl<T: BuffEnv + SkillLevelLookup> WarcryEnv for T {}

pub fn warcry_skill_specs(
    ctx: &mut StatMapCtx<'_>,
    groups: &dyn SocketGroupView,
    data: &dyn WarcryEnv,
    gem_level_bonuses: &[GemPropertyBonus],
) -> Vec<WarcrySpec> {
    use std::collections::HashSet;
    let mut specs = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    groups.for_each_enabled_group(&mut |group| {
        for gem in group.gems {
            let Some(effect) = data.effect(&gem.skill_id) else {
                continue;
            };
            if effect.is_support || !effect.skill_types.iter().any(|t| t == "Warcry") {
                continue;
            }
            if !seen.insert(gem.skill_id.clone()) {
                continue;
            }
            // Global +N gem levels (applyGemMods) + the same group's compatible
            // supports' granted levels (smith's Fire Mastery
            // `supported_fire_skill_gem_level_+` → Infernal Cry 21→22 level, gain
            // 51+q11=62, matching oracle exactly).
            let judgement = group_judgement(&group, data, &gem.skill_id);
            let level = gem.gem_level
                + additional_gem_levels(gem_level_bonuses, data, &gem.skill_id)
                + support_granted_gem_levels(&judgement, group.gems, data);
            let es = data.effect_stats(&gem.skill_id, level, gem.quality, gem.stat_set_index);
            let set_key = data.selected_set_key(&gem.skill_id, gem.stat_set_index);
            let stats: Vec<pobr_data::catalog::SkillDamageStat> = es.all().cloned().collect();
            if crate::dbg_env!("POBR_DBG_WARCRY").is_some() {
                eprintln!(
                    "[POBR_DBG_WARCRY] specs {} level={level} q={} set_key={set_key:?} stats={:?}",
                    gem.skill_id,
                    gem.quality,
                    stats
                        .iter()
                        .map(|s| (s.stat.as_str(), s.value))
                        .collect::<Vec<_>>()
                );
            }
            let mut mods = mapped_stat_modifiers(
                ctx,
                &stats,
                SourceKind::SkillGem,
                &gem.skill_id,
                &gem.skill_id,
                set_key.as_deref(),
            );
            mods.extend(support_modifiers(ctx, &group, data, &gem.skill_id));
            if let Some(ms) = effect.cast_time {
                mods.push(
                    Modifier::number("WarcryCastTime", ModType::Base, f64::from(ms) / 1000.0)
                        .with_source("Base"),
                );
            }
            let (cooldown_base_s, stored_uses) =
                resolve_skill_level(data, &gem.skill_id, level, None)
                    .map(|r| {
                        (
                            r.cooldown_s.unwrap_or(0.0),
                            // Matching vendor's `skillData.storedUses or 0` (CalcOffence.lua:3236).
                            r.stored_uses.map_or(0.0, f64::from),
                        )
                    })
                    .unwrap_or((0.0, 0.0));
            // Warcry key name (matching vendor CalcPerform.lua:2124's gsub chain:
            // strips `" Cry"`/`"'s"`/spaces entirely): "Infernal Cry" → `Infernal`.
            let name = buff_skill_name(data, &gem.skill_id)
                .replace(" Cry", "")
                .replace("'s", "")
                .replace(' ', "");
            specs.push(WarcrySpec {
                name,
                skill_id: gem.skill_id.clone(),
                cooldown_base_s,
                stored_uses,
                skill_types: skill_type_bits(&effect.skill_types),
                mods,
            });
        }
    });
    specs
}

/// Injects the **exposure supports** of every non-main group: a group whose active
/// skill produces a debuff-exposure payload (or carries an exposure-inflicting
/// payload through itself/a compatible support) contributes its compatible supports'
/// `*ExposureEffect` mods (matching vendor's exposure-source criterion
/// `HasMod("FLAG", "InflictExposure")`, CalcPerform.lua:3196-3200).
///
/// `main_group_index` = the index of the main skill's group in the enabled-group
/// enumeration order (skipped — the main group's own supports are already injected by
/// the primary support path).
pub fn exposure_support_modifiers(
    ctx: &mut StatMapCtx<'_>,
    groups: &dyn SocketGroupView,
    data: &dyn BuffEnv,
    main_group_index: Option<usize>,
) -> Vec<Modifier> {
    use std::collections::BTreeSet;
    let mut mods = Vec::new();
    let mut group_index = 0usize;
    groups.for_each_enabled_group(&mut |group| {
        let idx = group_index;
        group_index += 1;
        if main_group_index == Some(idx) {
            return;
        }
        // Exposure-source host: the group's active skill itself produces a debuff
        // exposure payload, or itself/a compatible support carries an
        // exposure-inflicting payload → that group's compatible support list.
        let mut support_entries: BTreeSet<(usize, String)> = BTreeSet::new();
        for gem in group.gems {
            let Some(effect) = data.effect(&gem.skill_id) else {
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
            let judgement = group_judgement(&group, data, &gem.skill_id);
            let is_host = crate::skill_env::stat_map::has_debuff_payload(
                ctx.catalog,
                &es,
                &gem.skill_id,
                set_key.as_deref(),
            ) || crate::skill_env::stat_map::has_exposure_inflict_stats(
                ctx.catalog,
                &es,
                &gem.skill_id,
                set_key.as_deref(),
            ) || judgement.compatible.iter().any(|sup| {
                let host = &group.gems[sup.gem_index];
                // Quality passed as 0, matching support_modifiers's semantics.
                let set_index = support_stat_set_index(sup, group.gems);
                let sup_stats = data.effect_stats(&sup.effect_id, host.gem_level, 0, set_index);
                let sup_key = data.selected_set_key(&sup.effect_id, set_index);
                crate::skill_env::stat_map::has_exposure_inflict_stats(
                    ctx.catalog,
                    &sup_stats,
                    &sup.effect_id,
                    sup_key.as_deref(),
                )
            });
            if !is_host {
                continue;
            }
            for sup in judgement.compatible {
                support_entries.insert((sup.gem_index, sup.effect_id));
            }
        }
        for (idx, effect_id) in support_entries {
            let gem = &group.gems[idx];
            let set_index = (gem.skill_id == effect_id)
                .then_some(gem.stat_set_index)
                .flatten();
            // Quality passed as 0, matching support_modifiers's semantics (supports have no quality table entries).
            let stats = data.effect_stats(&effect_id, gem.gem_level, 0, set_index);
            let set_key = data.selected_set_key(&effect_id, set_index);
            mods.extend(
                mapped_stat_modifiers(
                    ctx,
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
    });
    mods
}
