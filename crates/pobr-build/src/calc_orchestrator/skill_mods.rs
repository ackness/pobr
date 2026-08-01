//! skill_mods — modifier injection for skill base mods / DoT / corpse explosion / crossbow reload / quality / unselected sets.

use super::*;

use pobr_core::Modifier;
use pobr_core::rules::stat_map_engine::{self, MappedItem, MappedOutcome};
use pobr_data::item::EquipmentSlot;
use pobr_data::modifier::ModType;
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
    skill: &ResolvedSkillLevel,
    skill_id: &str,
    set_key: Option<&str>,
) -> Vec<Modifier> {
    let mut mods = Vec::new();
    let mk = |stat: &str, value: f64, label: &str| {
        let origin =
            ModifierSource::new(SourceId::new(SourceKind::SkillGem, format!("skill.{stat}")))
                .with_raw_text(label);
        Modifier::number(stat, ModType::Base, value).with_origin(origin)
    };
    if let Some(cd) = skill.cooldown_s
        && cd > 0.0
    {
        mods.push(mk("SkillCooldownBase", cd, "main skill base cooldown"));
    }
    // Number of stored uses (PoB's `skillData.storedUses`, e.g. grenade=3) →
    // SkillStoredUsesBase BASE. Consumed by `calc_cooldown` / `apply_cooldown_cap`: when
    // stored uses > 1, cooldown does **not** round up to a server frame (vendor
    // CalcOffence.lua:338-345).
    if let Some(stored) = skill.stored_uses
        && stored > 1
    {
        mods.push(mk(
            "SkillStoredUsesBase",
            f64::from(stored),
            "main skill stored uses",
        ));
    }
    if let Some(mc) = skill.mana_cost
        && mc > 0.0
    {
        mods.push(mk("SkillManaCostBase", mc, "main skill base mana cost"));
    }
    // The skill's inherent base crit chance (percentage points, e.g. Comet 13.0) →
    // SkillBaseCritChance BASE (the **base-material bucket**, distinct from the mod
    // bucket CriticalStrikeChance — vendor keeps `baseCrit = source.CritChance` and
    // `Sum BASE CritChance` as two separate buckets, CalcOffence.lua:3665-3689;
    // CritChanceBase OVERRIDE only replaces the base-material bucket). A spell's base
    // crit comes from the skill itself (not the weapon); for attack skills this field is
    // None, and base crit is instead injected from the weapon (see calc's main flow 1c).
    if let Some(cc) = skill.crit_chance
        && cc > 0.0
    {
        mods.push(mk("SkillBaseCritChance", cc, "main skill base crit chance"));
    }
    // statSet baseMods' inherent attack speed MORE (PoB2's
    // `mod("Speed","MORE",N,ModFlag.Attack)`; e.g. Flicker 285). Injected as
    // `AttackSpeed` MORE — the attack speed multiplier zone reads AttackSpeed by
    // ModName (attack chain only), matching PoB2's `skillModList:More(cfg,"Speed")`.
    // Spells never read AttackSpeed, so they're naturally unaffected.
    if let Some(more) = skill.skill_attack_speed_more
        && more != 0.0
    {
        let origin =
            ModifierSource::new(SourceId::new(SourceKind::SkillGem, "skill.AttackSpeedMore"))
                .with_raw_text("main skill statSet base attack speed MORE");
        mods.push(Modifier::number("AttackSpeed", ModType::More, more).with_origin(origin));
    }
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
    let set_index = group
        .gem_skills
        .iter()
        .find(|g| g.skill_id == skill_id)
        .and_then(|g| g.stat_set_index);
    let flags = data.selected_set_dot_flags(skill_id, set_index);
    let pairs = [
        ("DotIsArea", flags.area),
        ("DotIsProjectile", flags.projectile),
        ("DotIsSpell", flags.spell),
        ("DotIsAttack", flags.attack),
        ("DotIsHit", flags.hit),
    ];
    pairs
        .iter()
        .filter(|(_, on)| *on)
        .map(|(name, _)| {
            let origin = ModifierSource::new(SourceId::new(
                SourceKind::SkillGem,
                format!("skill.{skill_id}.{name}"),
            ))
            .with_raw_text(format!("statSet dot flag {name}"));
            Modifier::flag(*name).with_origin(origin)
        })
        .collect()
}

/// Corpse explosion base damage (vendor `CalcOffence.lua:2211-2217`):
///
/// ```lua
/// local monsterLife = skillData.corpseLife or data.monsterLifeTable[env.enemyLevel]
/// if skillData.explodeCorpse then
///     skillData[type.."BonusMin"] = monsterLife * skillData.corpseExplosionLifeMultiplier
/// ```
///
/// - Gate = the selected statSet's `explodeCorpse` baseMod (catalog
///   `StatSetDef::explode_corpse`, merged in via the skill_overrides overlay; e.g.
///   DetonateDeadPlayer, act_int.lua:5287);
/// - Multiplier = the selected set's per-level stat, via the statmap skill_data channel
///   (`corpse_explosion_monster_life_%` div=100 /
///   `corpse_explosion_monster_life_permillage_physical` div=1000 →
///   `corpseExplosionLifeMultiplier`, SkillStatMap.lua:309-316);
/// - Monster life = `monster_scaling.life_at(enemy level)` (enemy level resolved in the
///   same order as setup_enemy: orchestrator option → config enemyLevel →
///   min(MaxEnemyLevel, character level), CalcSetup.lua:529);
/// - Injection = `PhysicalDamageMin/Max` BASE (vendor's `corpseExplosionDamageType`
///   defaults to Physical, and the 4.5.0.3.4 statmap has no damage-type override
///   entry), matching vendor's semantics of directly adding to base via
///   `source[type.."BonusMin"]` (CalcOffence.lua:3910-3911; the Detonate Dead family has
///   baseMultiplier=1, so there's no added-mult interaction).
///
/// Returns empty (zero injection) for a non-corpse skill / no multiplier stat / no catalog.
pub(crate) fn corpse_explosion_modifiers(
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
    group: &SocketGroup,
    skill: &ResolvedSkillLevel,
    skill_id: &str,
) -> Vec<Modifier> {
    let set_index = group
        .gem_skills
        .iter()
        .find(|g| g.skill_id == skill_id)
        .and_then(|g| g.stat_set_index);
    if !data.selected_set_explode_corpse(skill_id, set_index) {
        return Vec::new();
    }
    let catalog = STAT_MAP_CTX
        .with(|ctx| ctx.borrow().catalog.clone())
        .or_else(|| data.stat_map_catalog.clone());
    let Some(catalog) = catalog else {
        return Vec::new(); // No catalog (old data pack): the multiplier is unavailable, conservatively zero injection.
    };
    // Fetching the multiplier: the selected set's per-level stat goes through the same
    // mapping engine as skill_base_modifiers, only harvesting skill_data output (the
    // Modifier output is already injected by the primary channel, not duplicated here).
    let set_key = data.selected_set_key(skill_id, set_index);
    let mut multiplier = 0.0;
    for ds in &skill.base_damage {
        if ds.value == 0.0 {
            continue;
        }
        if let MappedOutcome::Mapped(items) =
            stat_map_engine::map_stat(&catalog, skill_id, set_key.as_deref(), &ds.stat, ds.value)
        {
            for item in items {
                if let MappedItem::SkillData { key, value } = item
                    && key == "corpseExplosionLifeMultiplier"
                {
                    multiplier += value;
                }
            }
        }
    }
    if multiplier == 0.0 {
        return Vec::new();
    }
    let enemy_level = resolved_enemy_level(build, data, options);
    let monster_life = f64::from(data.constants.monster_scaling.life_at(enemy_level));
    let bonus = monster_life * multiplier;
    let mk = |name: &str| {
        let origin = ModifierSource::new(SourceId::new(
            SourceKind::SkillGem,
            format!("skill.{skill_id}.corpseExplosion"),
        ))
        .with_raw_text(format!(
            "corpse explosion {bonus:.1} (monster life {monster_life} x {multiplier})"
        ));
        Modifier::number(name, ModType::Base, bonus).with_origin(origin)
    };
    vec![mk("PhysicalDamageMin"), mk("PhysicalDamageMax")]
}

/// Resolves the enemy level (in the same order as `calculate_with_data` step 5's
/// setup_enemy, vendor CalcSetup.lua:529): orchestrator option's explicit level → build
/// config's `enemyLevel` (Input/Placeholder) → `min(MaxEnemyLevel, character level)`.
pub(crate) fn resolved_enemy_level(
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
) -> u32 {
    if options.enemy_level != 0 {
        options.enemy_level
    } else {
        let cap = data.constants.enemy_presets.max_enemy_level;
        config_enemy_level(build)
            .unwrap_or_else(|| build.character.level.min(cap))
            .min(cap)
    }
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
///   base_item_overrides, 33 crossbows already cataloged). When the weapon has no
///   reload data (not holding a crossbow) → returns empty entirely (matching vendor's
///   baseReloadTime nil semantics);
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
    let Some(effect) = data.granted_effects.get(skill_id) else {
        return Vec::new();
    };
    let has_type = |t: &str| effect.skill_types.iter().any(|x| x == t);
    if !has_type("CrossbowSkill") || has_type("Grenade") || has_type("CrossbowAmmoSkill") {
        return Vec::new();
    }
    // Weapon reload base value (main-hand only; matching vendor's `actor.weaponData1.ReloadTime`).
    let Some(reload_ms) = build
        .items
        .get(&EquipmentSlot::Weapon1)
        .and_then(|item| data.weapon_base(&item.base.to_string()))
        .and_then(|w| w.reload_time_ms)
        .filter(|&ms| ms > 0)
    else {
        return Vec::new();
    };
    let mk = |name: &str, value: f64, label: String| {
        let origin = ModifierSource::new(SourceId::new(
            SourceKind::SkillGem,
            format!("skill.{skill_id}.{name}"),
        ))
        .with_raw_text(label);
        Modifier::number(name, ModType::Base, value).with_origin(origin)
    };
    let mut mods = vec![mk(
        "CrossbowReloadTimeBase",
        f64::from(reload_ms) / 1000.0,
        format!("crossbow weapon reload {reload_ms}ms"),
    )];
    // Magazine capacity from the sibling ammo skill: the first `CrossbowAmmoSkill`
    // among the group's own gems or their additional granted effects, taking its
    // selected level's `base_number_of_crossbow_bolts` stat.
    let ammo = group.gem_skills.iter().find_map(|g| {
        let mut candidates: Vec<&str> = vec![g.skill_id.as_str()];
        if let Some(link) = data.gem_effects.get(&g.skill_id) {
            candidates.extend(
                link.additional_granted_effect_ids
                    .iter()
                    .map(String::as_str),
            );
        }
        candidates
            .into_iter()
            .find(|eid| {
                data.granted_effects
                    .get(*eid)
                    .is_some_and(|e| e.skill_types.iter().any(|t| t == "CrossbowAmmoSkill"))
            })
            .map(|eid| (eid.to_string(), g.gem_level))
    });
    if let Some((ammo_id, gem_level)) = ammo {
        let bolts: f64 = data
            .effect_stats(&ammo_id, gem_level, 0, None)
            .base
            .iter()
            .filter(|ds| ds.stat == "base_number_of_crossbow_bolts")
            .map(|ds| ds.value)
            .sum();
        if bolts > 0.0 {
            mods.push(mk(
                "CrossbowBoltCount",
                bolts,
                format!("ammo skill {ammo_id} bolt count"),
            ));
        }
    }
    mods
}

/// Maps the main skill gem's **quality stat segment** into `SourceKind::GemQuality`
/// attributed modifiers via [`mapped_stat_modifiers`] (T1.7, matching PoB2's
/// `buildSkillInstanceStats` up-front quality stacking, CalcTools.lua:140-145:
/// `stats[stat] += math.modf(rate × quality)`).
///
/// The main skill's quality is looked up from this group's `gem_skills` by effect id
/// (the result of `resolve_main_skill`'s selection is one of these entries). The
/// attribution id prefix is `gem.<effect id>.q<Q>`, matching `skill_source.rs`'s
/// existing convention (`quality_source_id`); [`mapped_stat_modifiers`] appends
/// `.<stat>` to further split it to a single stat. Returns empty for quality 0 / no
/// quality table entry (e.g. supports, which are skipped at export).
pub(crate) fn main_skill_quality_modifiers(
    group: &SocketGroup,
    data: &BuildData,
    skill_id: &str,
) -> Vec<Modifier> {
    let Some(gem) = group.gem_skills.iter().find(|g| g.skill_id == skill_id) else {
        return Vec::new(); // The builder path (with_active_skill) has no gem_skills: no quality source.
    };
    if gem.quality == 0 {
        return Vec::new();
    }
    let stats = data.effect_stats(
        &gem.skill_id,
        gem.gem_level,
        gem.quality,
        gem.stat_set_index,
    );
    // The quality segment belongs to the selected set's stats table (PoB2 stacks then
    // maps), the per-set override key matches the primary path.
    let set_key = data.selected_set_key(&gem.skill_id, gem.stat_set_index);
    mapped_stat_modifiers(
        &stats.quality,
        SourceKind::GemQuality,
        &format!("gem.{skill_id}.q{}", gem.quality),
        skill_id,
        set_key.as_deref(),
    )
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
/// translation boundary (the buff domain gets wired up with buff_pass, see the switch
/// log §5) — currently a global entry is entirely Unsupported and injects nothing; this
/// wiring is the structural groundwork. Once it's connected, injections will
/// automatically be produced (FlameWall's projectile buff etc., see
/// m1-acceptance-report.md for the measured Q3 impact scope). Zero values are skipped
/// (matching every other fetch point's semantics).
pub(crate) fn unselected_set_global_modifiers(
    group: &SocketGroup,
    data: &BuildData,
    skill_id: &str,
) -> Vec<Modifier> {
    let Some(gem) = group.gem_skills.iter().find(|g| g.skill_id == skill_id) else {
        return Vec::new(); // The builder path (with_active_skill) has no gem_skills: no statSet context.
    };
    let unselected = data.unselected_set_stats(
        &gem.skill_id,
        gem.gem_level,
        gem.quality,
        gem.stat_set_index,
    );
    if unselected.is_empty() {
        return Vec::new();
    }
    let Some(catalog) = STAT_MAP_CTX.with(|ctx| ctx.borrow().catalog.clone()) else {
        return Vec::new(); // No catalog: the data channel misses entirely, matching mapped_stat_modifiers's semantics.
    };
    // selectedGlobalStats accounting (:104-106): a stat in the selected set's stats
    // table that's already accounted for globally isn't re-injected from the
    // unselected sets (the onlyGlobals stage's accounting is unchanged, the stat-level
    // skip is equivalent to :107).
    let selected_key = data.selected_set_key(&gem.skill_id, gem.stat_set_index);
    let selected_stats = data.effect_stats(
        &gem.skill_id,
        gem.gem_level,
        gem.quality,
        gem.stat_set_index,
    );
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
    matches!(
        stat,
        "off_hand_weapon_minimum_physical_damage" | "off_hand_weapon_maximum_physical_damage"
    )
}
