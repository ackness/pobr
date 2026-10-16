//! skill_mods — modifier injection for skill base mods / DoT / corpse explosion / crossbow reload / quality / unselected sets.

use super::super::DataOrchestratorOptions;
use super::super::context::CalculationContext;
use super::super::skill::resolve::config_enemy_level;
use super::super::stat_map::mapped_stat_modifiers;

use pobr_core::Modifier;
use pobr_core::rules::config_interpreter::ConfigInputValue;
use pobr_core::rules::stat_map_engine::{self, MappedItem, MappedOutcome};
use pobr_data::modifier::ModType;
use pobr_data::monster::EnemyTier;
use pobr_data::source::{ModifierSource, SourceId, SourceKind};

use crate::build::{Build, SocketGroup};
use crate::build_data::{BuildData, ResolvedSkillLevel};

/// Builds the main skill's per-level parameters (cost / cooldown / **stat set**) into
/// SkillGem-attributed modifiers: cost/cooldown are read by `fill_skill_mechanics` via
/// `SkillManaCostBase` / `SkillCooldownBase`; the stat set is mapped via
/// [`map_skill_stat`] (base damage BASE, `damage_+%` INC, `_final` MORE), feeding into
/// offence's damage component pipeline.
///
/// Use time isn't handled here (it goes through `base_input.base_action_rate`, see
/// [`calculate_with_data`]). `set_key` = the selected statSet's per-set override key
/// (wired through, see [`mapped_stat_modifiers`]).
pub(crate) fn skill_base_modifiers(
    context: &mut CalculationContext,
    skill: &ResolvedSkillLevel,
    skill_id: &str,
    set_key: Option<&str>,
) -> Vec<Modifier> {
    let mut mods = pobr_core::skill_env::skill_base_modifiers(
        skill_id,
        skill.cooldown_s,
        skill.stored_uses,
        skill.mana_cost,
        skill.crit_chance,
        skill.skill_attack_speed_more,
    );
    mods.extend(pobr_core::skill_env::additional_resource_cost_modifiers(
        &skill.costs,
    ));
    mods.extend(pobr_core::calc::channel::skill_modifiers(
        skill_id,
        skill.use_time_s,
        skill
            .base_damage
            .iter()
            .any(|stat| stat.stat == "aftershock_delay_ms"),
    ));
    // The skill's stats (base damage + its own damage% scaling) are injected via
    // SkillStatMap mapping. Exception: `off_hand_weapon_*physical_damage` (base hit
    // damage for a non-weapon attack) is already counted into `base_hit_min/max` as a
    // **weapon source** by `non_weapon_attack_contribution` (× baseMultiplier), and must
    // not also be injected as `PhysicalDamageMin/Max` BASE via stat-map (or it would be double-counted).
    let base_damage: Vec<_> = skill
        .base_damage
        .iter()
        .filter(|ds| !is_off_hand_weapon_base_stat(&ds.stat))
        .cloned()
        .collect();
    mods.extend(mapped_stat_modifiers(
        context,
        &base_damage,
        SourceKind::SkillGem,
        "skill",
        skill_id,
        set_key,
    ));
    mods
}

/// The main skill's selected statSet's dotIs* flags → `DotIs<X>` FLAG modifiers.
///
/// Vendor semantics: entries like statSet `baseMods`'s `skill("dotIsArea", true)` hang
/// directly on skillData (in the full 4.5.0.3.4 data, only TornadoShot's "Tornado" set
/// has one). PoBR fetches them via catalog [`pobr_data::catalog::DotFlags`] (merged in
/// through the skill_overrides overlay), and injects a FLAG under the same name as the
/// stat-driven channel (the dotIs* skill_data keys from
/// `stat_map_engine::collect_skill_data`) — `calc::skill_dot::DotIsFlags::from_db` is
/// the unified consumption point for both paths. Returns empty (zero injection) when
/// all flags are false (unverified/no flags).
pub(crate) fn dot_flag_modifiers(
    group: &SocketGroup,
    data: &BuildData,
    skill_id: &str,
) -> Vec<Modifier> {
    let set_index = super::resolve::selected_group_effect(data, group)
        .filter(|effect| effect.skill_id == skill_id)
        .and_then(|effect| effect.stat_set_index);
    let flags = data.selected_set_dot_flags(skill_id, set_index);
    let mut mods = pobr_core::skill_env::dot_flag_modifiers(flags, skill_id);
    if skill_id == "TriggeredDecayingHexPlayer" {
        mods.push(Modifier::flag("ProfaneDecay"));
    }
    if let Some(set) = data.select_stat_set(skill_id, set_index) {
        // The game gives Ignited Ground its own fire-damage stat set. The
        // resulting ailment is already calculated; this only identifies its view.
        if matches!(
            set.set_id.as_str(),
            "BallLightningIgnitedGround"
                | "IncinerateGroundPlayer"
                | "OilGrenadeOilGroundPlayer"
                | "SpearOfSolarisGroundPlayer"
        ) {
            mods.push(Modifier::flag("IgnitedGround"));
        }
        if set.set_id == "CGEMonsterModAbyssVolatileRockBurningGroundPlayer" {
            mods.push(Modifier::flag("BurningGround"));
        }
    }
    mods
}

/// Corpse explosion base damage: the skill deals a share of the corpse's
/// maximum Life as base damage of a type, both given by its stat
/// ([`skill_stats::corpse_explosion_damage`]).
///
/// - Gate = the selected statSet's `explodeCorpse` (catalog
///   `StatSetDef::explode_corpse`, from the skill_overrides overlay; e.g.
///   DetonateDeadPlayer);
/// - Corpse life = the explicit corpse-Life assumption, otherwise
///   `monster_scaling.life_at(enemy_level)`;
/// - Injection = `<Type>DamageMin/Max` BASE, added to the skill's base.
///
/// Returns empty (zero injection) for a non-corpse skill or a skill without
/// such a stat.
pub(crate) fn corpse_explosion_modifiers(
    build: &Build,
    data: &BuildData,
    enemy_level: u32,
    group: &SocketGroup,
    skill: &ResolvedSkillLevel,
    skill_id: &str,
) -> Vec<Modifier> {
    let set_index = super::resolve::selected_group_effect(data, group)
        .filter(|effect| effect.skill_id == skill_id)
        .and_then(|effect| effect.stat_set_index);
    if !data.selected_set_explode_corpse(skill_id, set_index) {
        return Vec::new();
    }
    let configured_life = match build.config.raw_inputs.values.get("detonateDeadCorpseLife") {
        Some(ConfigInputValue::Number(value)) if value.is_finite() && *value >= 0.0 => Some(*value),
        _ => None,
    };
    let corpse_life = configured_life
        .unwrap_or_else(|| f64::from(data.constants.monster_scaling.life_at(enemy_level)));
    let mut mods = Vec::new();
    // A stat set that only poisons with the corpse's Life (Decompose's
    // cloud) and deals no damage of its own does not hit, so it cannot
    // critically hit; its explosion, with base Fire damage, does.
    let without_hit = skill.base_damage.iter().any(|ds| {
        ds.value != 0.0 && pobr_core::rules::skill_stats::corpse_damage_without_hit(&ds.stat)
    }) && !skill.base_damage.iter().any(|ds| {
        ds.value != 0.0 && ds.stat.contains("minimum_base_") && ds.stat.ends_with("_damage")
    });
    if without_hit {
        mods.push(
            Modifier::flag("NeverCrit").with_origin(
                ModifierSource::new(SourceId::new(
                    SourceKind::SkillGem,
                    format!("skill.{skill_id}.corpseExplosion"),
                ))
                .with_raw_text("poisons as though dealing damage: no hit"),
            ),
        );
    }
    for ds in &skill.base_damage {
        if ds.value == 0.0 {
            continue;
        }
        let Some((damage_type, share)) =
            pobr_core::rules::skill_stats::corpse_explosion_damage(&ds.stat, ds.value)
        else {
            continue;
        };
        let bonus = corpse_life * share;
        let origin = ModifierSource::new(SourceId::new(
            SourceKind::SkillGem,
            format!("skill.{skill_id}.corpseExplosion"),
        ))
        .with_raw_text(format!(
            "corpse explosion {bonus:.1} (corpse life {corpse_life} x {share}; {})",
            if configured_life.is_some() {
                "detonateDeadCorpseLife"
            } else {
                "monster life at enemy level"
            }
        ));
        for bound in ["Min", "Max"] {
            mods.push(
                Modifier::number(
                    format!("{damage_type:?}Damage{bound}"),
                    ModType::Base,
                    bonus,
                )
                .with_origin(origin.clone()),
            );
        }
    }
    mods
}

/// One level for corpse Life and enemy mitigation. Explicit options override
/// the build's input/placeholder. Otherwise ordinary monsters follow the
/// character, while Pinnacle/Uber use their preset's default/minimum level.
/// Every path respects the preset's level floor and the maximum enemy level.
pub(crate) fn resolved_enemy_level(
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
    tier: EnemyTier,
) -> u32 {
    let presets = &data.constants.enemy_presets;
    let min_level = presets
        .tier_for(tier)
        .map_or_else(|| tier.min_level(), |preset| preset.min_level);
    let default_level = match tier {
        EnemyTier::Pinnacle | EnemyTier::Uber => min_level,
        EnemyTier::None | EnemyTier::Boss => build.character.level,
    };
    let level = if options.enemy_level != 0 {
        options.enemy_level
    } else {
        config_enemy_level(build).unwrap_or(default_level)
    };
    level.max(min_level).min(presets.max_enemy_level)
}

/// Crossbow reload data channel (matching vendor `CalcOffence.lua:1118-1122`'s
/// skillData assembly + `:283-320`'s calcCrossbowAmmoStats/calcCrossbowReloadTime data
/// fetching):
///
/// - Gate = main skill's `skill_types` includes `CrossbowSkill` and excludes `Grenade` /
///   `CrossbowAmmoSkill` (matching vendor `:1118`'s same three predicates; grenades
///   don't consume ammo);
/// - `CrossbowReloadTimeBase` BASE (seconds) ← the main-hand weapon's
///   `weapon.reload_time_ms` (WeaponTypes' ReloadTime; falls back to the overlay's
///   base_item_overrides, 33 crossbows already cataloged), divided by
///   `1 + (local reload speed + local attack speed) / 100` of that weapon. When the
///   weapon has no reload data (not holding a crossbow) → returns empty entirely
///   (matching vendor's baseReloadTime nil semantics);
/// - `CrossbowBoltCount` BASE ← the stat `base_number_of_crossbow_bolts` from the
///   sibling ammo skill in the same group (either directly in the group, or linked via
///   gem_effects' additional effects) (matching vendor's ammo skill modList transfer
///   `:303-307`). Not injected when there's no ammo data (the calc side falls back to a
///   minimum magazine of 1).
///
/// `ReloadSpeed`/`ChanceToNotConsumeAmmo`/`InstantReloadChance` mods go through the
/// generic modifier bus, aggregated on the calc side (`fill_crossbow_reload`).
pub(crate) fn crossbow_reload_modifiers(
    build: &Build,
    data: &BuildData,
    group: &SocketGroup,
    skill_id: &str,
) -> Vec<Modifier> {
    let gems = group.gem_inputs();
    pobr_core::skill_env::crossbow_reload_modifiers(data, data, build, data, &gems, skill_id)
}

/// Maps the main skill gem's **quality stat segment** into `SourceKind::GemQuality`
/// attributed modifiers via [`mapped_stat_modifiers`] (T1.7, matching PoB2's
/// `buildSkillInstanceStats` up-front quality stacking, CalcTools.lua:140-145:
/// `stats[stat] += math.modf(rate × quality)`).
///
/// Quality and stat-set provenance follow the exact selected active effect,
/// including additional effects granted by support gems. The
/// attribution id prefix is `gem.<effect id>.q<Q>`, matching `skill_source.rs`'s
/// existing convention (`quality_source_id`); [`mapped_stat_modifiers`] appends
/// `.<stat>` to further split it to a single stat. `use_alt_quality` is the build's
/// GemlingQuality flag. Returns empty for quality 0 / no
/// quality table entry (e.g. supports, which are skipped at export).
pub(crate) fn main_skill_quality_modifiers(
    context: &mut CalculationContext,
    group: &SocketGroup,
    data: &BuildData,
    skill_id: &str,
    use_alt_quality: bool,
) -> Vec<Modifier> {
    let Some(gem) = super::resolve::selected_group_effect(data, group)
        .filter(|effect| effect.skill_id == skill_id)
    else {
        return Vec::new();
    };
    if gem.quality == 0 {
        return Vec::new();
    }
    let mut stats = data.effect_stats(gem.skill_id, gem.gem_level, gem.quality, gem.stat_set_index);
    // GemlingQuality builds also apply the gem's altQualityStats
    // (CalcTools.lua:169-174 `includeAltQualityStats`).
    if use_alt_quality {
        stats
            .quality
            .extend(data.alt_quality_stats(gem.skill_id, gem.quality));
    }
    // The quality segment belongs to the selected set's stats table (PoB2 stacks then
    // maps), the per-set override key matches the primary path.
    let set_key = data.selected_set_key(gem.skill_id, gem.stat_set_index);
    let label = format!("gem.{skill_id}.q{}", gem.quality);
    let mut map = |stats: &[pobr_data::catalog::SkillDamageStat]| {
        mapped_stat_modifiers(
            context,
            stats,
            SourceKind::GemQuality,
            &label,
            skill_id,
            set_key.as_deref(),
        )
    };
    // PoB2 adds quality onto the stat before mapping (CalcTools.lua:163-175), so a
    // statmap `base` offset applies once per stat. A quality stat the set already
    // carries is therefore mapped as the difference it makes on top of the set's
    // value; mapping it alone would apply the offset a second time (e.g.
    // `base_number_of_projectiles`, `base = -1`, SkillStatMap.lua:1889-1892).
    let mut mods = Vec::new();
    for quality in &stats.quality {
        let in_set: f64 = stats
            .base
            .iter()
            .filter(|base| base.stat == quality.stat)
            .map(|base| base.value)
            .sum();
        if in_set == 0.0 {
            mods.extend(map(std::slice::from_ref(quality)));
            continue;
        }
        let stat = |value: f64| pobr_data::catalog::SkillDamageStat {
            stat: quality.stat.clone(),
            value,
        };
        let without = map(&[stat(in_set)]);
        for (index, modifier) in map(&[stat(in_set + quality.value)]).into_iter().enumerate() {
            let before = without
                .get(index)
                .filter(|other| other.name == modifier.name && other.mod_type == modifier.mod_type)
                .and_then(|other| other.value.as_number());
            match (modifier.value.as_number(), before) {
                (Some(after), Some(before)) => {
                    let mut modifier = modifier;
                    modifier.value = pobr_core::ModValue::Number(after - before);
                    mods.push(modifier);
                }
                _ => mods.push(modifier),
            }
        }
    }
    mods
}

/// The main skill's **unselected statSet** global-only merge (matching PoB2's
/// `calcs.mergeSkillInstanceMods`, `Modules/CalcActiveSkill.lua:124-140`): for every
/// vendor-exported set other than the selected one, only its stats' statmap entries
/// carrying a `GlobalEffect` tag (`isGlobalEffect`, `:68-80`) get injected as a
/// modOrGroup; a stat that's already accounted for globally by the selected set is
/// skipped entirely (`selectedGlobalStats`, `:104-107`).
///
/// Data source = [`BuildData::unselected_set_stats`] (matching buildSkillInstanceStats
/// table semantics, quality stacked per set, same stats merged); mapping =
/// `stat_map_engine::map_stat_global_only` (the per-set override chain is looked up by
/// **this unselected set's** set_key).
///
/// **First-batch boundary**: the `GlobalEffect` tag itself is still outside the tag
/// translation boundary (the buff domain gets wired up with buff_pass) —
/// currently a global entry is entirely Unsupported and injects nothing; this
/// wiring is the structural groundwork. Once it's connected, injections will
/// automatically be produced (FlameWall's projectile buff etc.). Zero values
/// are skipped (matching every other fetch point's semantics).
pub(crate) fn unselected_set_global_modifiers(
    context: &mut CalculationContext,
    group: &SocketGroup,
    data: &BuildData,
    skill_id: &str,
) -> Vec<Modifier> {
    let Some(gem) = super::resolve::selected_group_effect(data, group)
        .filter(|effect| effect.skill_id == skill_id)
    else {
        return Vec::new();
    };
    let unselected =
        data.unselected_set_stats(gem.skill_id, gem.gem_level, gem.quality, gem.stat_set_index);
    if unselected.is_empty() {
        return Vec::new();
    }
    let Some(catalog) = context.catalog.clone() else {
        return Vec::new(); // No catalog: the data channel misses entirely, matching mapped_stat_modifiers's semantics.
    };
    // selectedGlobalStats accounting (:104-106): a stat in the selected set's stats
    // table that's already accounted for globally isn't re-injected from the
    // unselected sets (the onlyGlobals stage's accounting is unchanged, the stat-level
    // skip is equivalent to :107).
    let selected_key = data.selected_set_key(gem.skill_id, gem.stat_set_index);
    let selected_stats =
        data.effect_stats(gem.skill_id, gem.gem_level, gem.quality, gem.stat_set_index);
    let selected_globals: std::collections::HashSet<&str> = selected_stats
        .all()
        .filter(|ds| {
            stat_map_engine::stat_has_global_mods(
                &catalog,
                skill_id,
                selected_key.as_deref(),
                &ds.stat,
            )
        })
        .map(|ds| ds.stat.as_str())
        .collect();
    let mut mods = Vec::new();
    for set in &unselected {
        for ds in &set.stats {
            if ds.value == 0.0 || selected_globals.contains(ds.stat.as_str()) {
                continue;
            }
            let MappedOutcome::Mapped(items) = stat_map_engine::map_stat_global_only(
                &catalog,
                skill_id,
                Some(&set.set_key),
                &ds.stat,
                ds.value,
            ) else {
                continue; // Unsupported (including the GlobalEffect tag's first-batch boundary) / Unknown: skipped.
            };
            for item in items {
                let MappedItem::Modifier(modifier) = item else {
                    continue; // SkillData: no consumer.
                };
                let origin = ModifierSource::new(SourceId::new(
                    SourceKind::SkillGem,
                    format!("skill.{skill_id}.set{}.{}", set.set_key, ds.stat),
                ))
                .with_raw_text(format!(
                    "unselected statSet {} global {} ({})",
                    set.set_id, ds.stat, ds.value
                ));
                mods.push(modifier.with_origin(origin));
            }
        }
    }
    mods
}

/// Whether this is a non-weapon-attack off-hand weapon base damage stat (consumed as a
/// weapon source by `non_weapon_attack_contribution`, so excluded from the stat-map
/// injection path to avoid double-counting).
pub(crate) fn is_off_hand_weapon_base_stat(stat: &str) -> bool {
    pobr_core::skill_env::is_off_hand_weapon_base_stat(stat)
}

/// Maps the **compatible support gems'** per-level stats in the main skill's group
/// through [`map_skill_stat`] into SupportGem-attributed modifiers, injected into the
/// supported skill (e.g. "added lightning damage" → `LightningDamageMin/Max` BASE,
/// "more damage" → `Damage` MORE).
///
/// Before injection, [`judge_group_supports`] produces the compatible list: **a rejected
/// support doesn't participate at all** (neither its numeric values nor its
/// manaMultiplier applies, matching PoB2's `CalcActiveSkill.lua:210-214` semantics of
/// only putting compatible supports into effectList).
///
/// The current scope is **global** (correct semantics under a single-main-skill build:
/// every support's multiplier applies to the one skill being calculated); per-skill tag
/// isolation for multiple main skills (applying only to the supported skill) is deferred
/// until the flag system is wired up. The active main skill's own damage is already
/// injected by [`skill_base_modifiers`]; this only handles supports.
pub(crate) fn support_modifiers(
    context: &mut CalculationContext,
    group: &SocketGroup,
    data: &BuildData,
    active_skill_id: &str,
    host_stat_set_index: Option<u32>,
) -> Vec<Modifier> {
    let gems = group.gem_inputs();
    let eg = group.enabled_group(&gems);
    let mut limits = Vec::new();
    let mut ctx = context.stat_map_ctx();
    let mods = pobr_core::skill_env::support_modifiers_with_limits(
        &mut ctx,
        &eg,
        data,
        active_skill_id,
        host_stat_set_index,
        &mut limits,
    );
    if context.is_trigger_source {
        for limit in &mut limits {
            limit.insert_str(0, "TriggerSource:");
        }
    }
    context.support_limits.extend(limits);
    mods
}

/// The action that places a proxy comes from its summoning skill, not from
/// the attack or spell the proxy performs. CastTime is in milliseconds.
pub(crate) fn skill_timing_modifiers(
    data: &BuildData,
    group: &SocketGroup,
    skill_id: &str,
    final_types: &[String],
) -> Vec<Modifier> {
    let has = |kind: &str| final_types.iter().any(|t| t == kind);
    let main = data.granted_effects.get(skill_id);
    let totem = has("SummonsTotem") || has("UsedByTotem");
    let placement = if totem {
        group
            .gem_skills
            .iter()
            .filter_map(|gem| data.granted_effects.get(&gem.skill_id))
            .find(|effect| effect.skill_types.iter().any(|t| t == "SummonsTotem"))
            .or(main)
            .and_then(|effect| effect.cast_time)
    } else {
        None
    };
    let mut modifiers: Vec<_> = [
        ("SkillTotemPlacementTimeBase", placement),
        (
            "SkillWarcryCastTimeBase",
            main.filter(|_| has("Warcry"))
                .and_then(|effect| effect.cast_time),
        ),
        (
            "SkillTrapThrowingTimeBase",
            main.filter(|_| has("Trap"))
                .and_then(|effect| effect.cast_time),
        ),
        (
            "SkillMineLayingTimeBase",
            main.filter(|_| has("Mine"))
                .and_then(|effect| effect.cast_time),
        ),
    ]
    .into_iter()
    .filter_map(|(name, time)| {
        time.filter(|time| *time > 0).map(|time| {
            Modifier::number(name, ModType::Base, time as f64 / 1000.0).with_origin(
                ModifierSource::new(SourceId::new(SourceKind::SkillGem, format!("skill.{name}"))),
            )
        })
    })
    .collect();
    // A granted meteor's cadence belongs to its originating gem. Retain
    // the parent's level and quality when its active effect is selected.
    if let Some(selected) = super::resolve::selected_group_effect(data, group)
        && selected.skill_id == skill_id
    {
        let gems = group.gem_inputs();
        if let Some(parent) = gems.get(selected.gem_index)
            && parent.skill_id != skill_id
        {
            let stats = data.effect_stats(&parent.skill_id, parent.gem_level, parent.quality, None);
            let all: Vec<_> = stats.all().cloned().collect();
            for stat in all
                .iter()
                .filter(|s| s.stat == "walking_calamity_spawn_meteor_every_ms")
            {
                let origin = ModifierSource::new(SourceId::new(
                    SourceKind::SkillGem,
                    format!("skill.{}.{}", parent.skill_id, stat.stat),
                ))
                .with_raw_text(stat.stat.clone());
                modifiers.extend(
                    pobr_core::rules::skill_stats::granted_skill_modifiers(
                        &stat.stat, stat.value, &all,
                    )
                    .into_iter()
                    .map(|modifier| modifier.with_origin(origin.clone())),
                );
            }
        }
    }
    modifiers
}
