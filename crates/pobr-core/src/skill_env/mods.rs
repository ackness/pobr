//! Pure skill-group → `Modifier` translations (no context, no `Build`/`BuildData`).
//!
//! These are the engine-semantics helpers extracted from `pobr-build`'s
//! `skill/mods.rs`: they take plain data types and produce `Vec<Modifier>`,
//! letting the same logic be reused by WASM, CLI, and test harnesses.

use pobr_data::catalog::DotFlags;
use pobr_data::modifier::ModType;
use pobr_data::source::{ModifierSource, SourceId, SourceKind};

use crate::Modifier;

/// Maps the selected statSet's dotIs* flags into `FLAG` modifiers.
///
/// Vendor semantics: entries like statSet `baseMods`'s `skill("dotIsArea", true)` hang
/// directly on skillData. PoBR injects a FLAG under the same name as the stat-driven
/// channel (the dotIs* skill_data keys from `stat_map_engine::collect_skill_data`) —
/// `calc::skill_dot::DotIsFlags::from_db` is the unified consumption point for both
/// paths. Returns empty when all flags are false.
pub fn dot_flag_modifiers(flags: DotFlags, skill_id: &str) -> Vec<Modifier> {
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

/// Whether a base_damage stat is the off-hand weapon's physical damage (already
/// counted into `base_hit_min/max` as a **weapon source** by
/// `non_weapon_attack_contribution`, so it must not also be injected as
/// `PhysicalDamageMin/Max` BASE via stat-map).
pub fn is_off_hand_weapon_base_stat(stat: &str) -> bool {
    matches!(
        stat,
        "off_hand_weapon_minimum_physical_damage" | "off_hand_weapon_maximum_physical_damage"
    )
}

/// Resolves the enemy level for calculations (matching vendor `CalcSetup.lua:529`'s
/// `enemyLevel = m_min(data.misc.MaxEnemyLevel, config.enemyLevel or characterLevel)`).
///
/// Priority: orchestrator option → config `enemyLevel` → `min(maxEnemyLevel, characterLevel)`.
pub fn resolved_enemy_level(
    option_enemy_level: u32,
    config_enemy_level: Option<u32>,
    character_level: u32,
    max_enemy_level: u32,
) -> u32 {
    if option_enemy_level != 0 {
        option_enemy_level
    } else {
        config_enemy_level
            .unwrap_or_else(|| character_level.min(max_enemy_level))
            .min(max_enemy_level)
    }
}

/// A resolved skill's base parameters → `BASE`/`MORE` modifiers.
///
/// This is the **pure** half of `pobr-build`'s `skill_base_modifiers`: the
/// cooldown/stored-uses/mana-cost/crit-chance/attack-speed-more translations that
/// don't need the stat-map catalog. The caller appends the stat-map-mapped
/// `base_damage` segment separately.
///
/// `skill_id` is used only for `SourceId` attribution labels.
pub fn skill_base_modifiers(
    skill_id: &str,
    cooldown_s: Option<f64>,
    stored_uses: Option<u32>,
    mana_cost: Option<f64>,
    crit_chance: Option<f64>,
    attack_speed_more: Option<f64>,
) -> Vec<Modifier> {
    let mut mods = Vec::new();
    let mk = |stat: &str, value: f64, label: &str| {
        let origin =
            ModifierSource::new(SourceId::new(SourceKind::SkillGem, format!("skill.{stat}")))
                .with_raw_text(label);
        Modifier::number(stat, ModType::Base, value).with_origin(origin)
    };
    if let Some(cd) = cooldown_s
        && cd > 0.0
    {
        mods.push(mk("SkillCooldownBase", cd, "main skill base cooldown"));
    }
    if let Some(stored) = stored_uses
        && stored > 1
    {
        mods.push(mk(
            "SkillStoredUsesBase",
            f64::from(stored),
            "main skill stored uses",
        ));
    }
    if let Some(mc) = mana_cost
        && mc > 0.0
    {
        mods.push(mk("SkillManaCostBase", mc, "main skill base mana cost"));
    }
    if let Some(cc) = crit_chance
        && cc > 0.0
    {
        mods.push(mk("SkillBaseCritChance", cc, "main skill base crit chance"));
    }
    if let Some(more) = attack_speed_more
        && more != 0.0
    {
        let origin =
            ModifierSource::new(SourceId::new(SourceKind::SkillGem, "skill.AttackSpeedMore"))
                .with_raw_text("main skill statSet base attack speed MORE");
        mods.push(Modifier::number("AttackSpeed", ModType::More, more).with_origin(origin));
    }
    let _ = skill_id; // reserved for future per-skill attribution
    mods
}

/// A compatible support's per-level cost multiplier → `SupportManaMultiplier` `MORE`
/// (matching PoB2's `CalcActiveSkill.lua:689-691`:
/// `NewMod("SupportManaMultiplier","MORE", level.manaMultiplier, modSource)`).
///
/// Only injected for the **compatible list** — a rejected support's multiplier
/// doesn't apply. Consumed by `skill_mechanics::calc_skill_cost` (the multipliers
/// are chained and truncated to 4 decimal places, then applied to base cost before
/// the inc/more chain).
///
/// Returns `None` when the effect has no level table or the multiplier is zero.
pub fn support_mana_multiplier_modifier(
    effect_id: &str,
    mana_multiplier: Option<f64>,
) -> Option<Modifier> {
    let mm = mana_multiplier.filter(|&v| v != 0.0)?;
    let origin = ModifierSource::new(SourceId::new(
        SourceKind::SupportGem,
        format!("support.{effect_id}.manaMultiplier"),
    ))
    .with_raw_text(format!("support {effect_id} cost multiplier {mm}%"));
    Some(Modifier::number("SupportManaMultiplier", ModType::More, mm).with_origin(origin))
}

/// Skill type name (`ActiveSkillType.Id`) → `SkillTypes` bitset.
///
/// Used by the orchestrator to translate a granted effect's `skill_types` list into
/// the bitset consumed by `stat_map_engine::collect_skill_data`.
pub fn skill_type_bits(skill_types: &[String]) -> pobr_data::skill::SkillTypes {
    let mut bits = pobr_data::skill::SkillTypes::NONE;
    for t in skill_types {
        match pobr_data::skill::SkillTypes::from_pob2_name(t) {
            Some(st) => bits |= st,
            None => debug_assert!(false, "unknown SkillType name: {t}"),
        }
    }
    bits
}

/// Skill type name (`ActiveSkillType.Id`) → cfg damage flags.
///
/// Used by damage aggregation to pull `<Projectile|Area|Spell|Melee>Damage` boosts by
/// skill category. A hit skill → `ModFlag.Hit` (matching vendor
/// `CalcActiveSkill.lua:176`'s `skillFlags.hit = … or skillTypes[Attack] or
/// skillTypes[Damage] or skillTypes[Projectile]`).
pub fn skill_type_flags(skill_types: &[String]) -> pobr_data::modifier::ModFlags {
    let mut flags = pobr_data::modifier::ModFlags::NONE;
    for t in skill_types {
        match t.as_str() {
            "Attack" => flags |= pobr_data::modifier::ModFlags::ATTACK,
            "Spell" => flags |= pobr_data::modifier::ModFlags::SPELL,
            "Melee" => flags |= pobr_data::modifier::ModFlags::MELEE,
            "Projectile" | "ProjectilesFromUser" => {
                flags |= pobr_data::modifier::ModFlags::PROJECTILE
            }
            "Area" | "AreaSpell" => flags |= pobr_data::modifier::ModFlags::AREA,
            _ => {}
        }
    }
    if skill_types
        .iter()
        .any(|t| matches!(t.as_str(), "Attack" | "Damage" | "Projectile"))
    {
        flags |= pobr_data::modifier::ModFlags::HIT;
    }
    flags
}

/// Vendor `math.floor(value * scale * 100 + 0.5) / 100` scaling (PoB2's `modValue`
/// rounding for radius jewel grants). Returns the truncated-to-integer result.
pub fn vendor_scale_mod_value(value: f64, scale: f64) -> f64 {
    let rounded = (value * scale * 100.0).round() / 100.0;
    rounded.trunc()
}

/// Determines an attribute-choice node (PoBR's equivalent of PoB2 tree.lua's
/// `isAttribute=true` nodes): its mod is the "+N to any [Attributes|Attribute]"
/// three-way-choice form. The catalog carries no isAttribute flag, so this is
/// determined from the node's mod text (matching the text form used by pobr-tree's
/// attribute-choice rewrite).
pub fn is_attribute_node(def: &pobr_data::catalog::PassiveNodeDef) -> bool {
    def.stats.iter().any(|s| {
        let lower = s.to_ascii_lowercase();
        lower.contains(" to any ") && lower.contains("attribute")
    })
}

/// The parsed result of a GemProperty mod (matching vendor's
/// `mod("GemProperty", "LIST", { keyword, key, value, gemRequirements })`,
/// ModParser.lua:3468-3497).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GemPropertyBonus {
    pub value: u32,
    pub kind: GemPropertyKind,
    /// The category (lowercase; empty = a bare "all Skills" matches unconditionally).
    pub category: String,
    /// Attribute requirement filter (matching vendor's
    /// `gemRequirements[reqStr|reqDex|reqInt] ≥ 1`, the `with a <Attr> requirement`
    /// suffix): Some("str"|"dex"|"int").
    pub attr_req: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GemPropertyKind {
    Level,
    Quality,
}

/// Strips PoB item mod `{tag}` markers and `[a|b]` variant brackets, returning the
/// resolved text (matching vendor's `modLib.parseMod` pre-processing).
pub fn clean_grant_text(text: &str) -> String {
    let no_braces = crate::skill_env::clean_item_text(text);
    if !no_braces.contains('[') {
        return no_braces;
    }
    let mut out = String::with_capacity(no_braces.len());
    let mut chars = no_braces.chars();
    while let Some(c) = chars.next() {
        if c == '[' {
            let mut inner = String::new();
            for ic in chars.by_ref() {
                if ic == ']' {
                    break;
                }
                inner.push(ic);
            }
            out.push_str(inner.rsplit('|').next().unwrap_or(&inner));
        } else {
            out.push(c);
        }
    }
    out
}

/// Parses a `+N to [Level|Quality] of all <category> Skills [with a <Attr> requirement]`
/// mod into a [`GemPropertyBonus`]. Returns `None` for any other form.
pub fn parse_gem_property_bonus(text: &str) -> Option<GemPropertyBonus> {
    let clean = clean_grant_text(text);
    let body = clean.strip_prefix('+')?;
    let (num, rest) = body.split_once(" to ")?;
    let num = num.strip_suffix('%').unwrap_or(num);
    let value: u32 = num.trim().parse().ok()?;
    let (kind, rest) = if let Some(r) = rest.strip_prefix("level of all") {
        (GemPropertyKind::Level, r)
    } else {
        (
            GemPropertyKind::Quality,
            rest.strip_prefix("quality of all")?,
        )
    };
    let mut rest = rest.trim();
    let mut attr_req = None;
    if let Some((head, req)) = rest.split_once(" with a ") {
        attr_req = Some(match req.trim() {
            "strength requirement" => "str",
            "dexterity requirement" => "dex",
            "intelligence requirement" => "int",
            _ => return None,
        });
        rest = head.trim_end();
    }
    let category = rest.strip_suffix("skills").unwrap_or(rest).trim();
    Some(GemPropertyBonus {
        value,
        kind,
        category: category.to_string(),
        attr_req,
    })
}

/// Compatibility shim for the old call surface: `+N to Level of all <category> Skills`
/// (no attribute-requirement suffix) → `(N, category)`. Delegates to
/// [`parse_gem_property_bonus`].
pub fn parse_gem_level_bonus(text: &str) -> Option<(u32, String)> {
    let bonus = parse_gem_property_bonus(text)?;
    (bonus.kind == GemPropertyKind::Level && bonus.attr_req.is_none())
        .then_some((bonus.value, bonus.category))
}

/// Whether a gem-level-bonus's `<category>` applies to the main skill. Matches PoB2
/// semantics (`ModParser.lua:3480-3496`'s GemProperty construction +
/// `CalcSetup.lua:404-435`'s `applyGemMods` + `CalcTools.lua:113-126`'s `gemIsType`):
/// - a bare "all skills"/"skill gems" matches unconditionally;
/// - the whole string = a skill name (PoB2's `gemIdLookup` match branch) → matches by
///   the main skill's name (derived from the granted effect id);
/// - otherwise, split on whitespace (PoB2's multi-word category = `keywordList`):
///   **every** token must hit the main skill's `skill_types`.
pub fn gem_level_category_matches(category: &str, skill_types: &[String], skill_id: &str) -> bool {
    if category.is_empty() || category == "skill gems" {
        return true;
    }
    if category == skill_name_from_id(skill_id) {
        return true;
    }
    category
        .split_whitespace()
        .all(|tok| skill_types.iter().any(|t| t.eq_ignore_ascii_case(tok)))
}

/// Derives a skill's display name from its granted effect id (lowercase, CamelCase
/// split): strips the `Player` suffix, then inserts a space at each uppercase boundary
/// (`ShieldWallPlayer` → `shield wall`). Matches PoB2's exported skillId naming
/// convention (`Export/Scripts/skills.lua`: id = display name with spaces stripped +
/// an actor suffix), used by `gem_level_category_matches`'s skill-name category branch.
pub fn skill_name_from_id(skill_id: &str) -> String {
    let stem = skill_id.strip_suffix("Player").unwrap_or(skill_id);
    let mut out = String::with_capacity(stem.len() + 4);
    for (i, ch) in stem.chars().enumerate() {
        if ch.is_ascii_uppercase() && i > 0 {
            out.push(' ');
        }
        out.push(ch.to_ascii_lowercase());
    }
    out
}

/// Builds a trigger `BASE` mod (SkillGem attribution, `trigger.<stat>` source id).
pub fn mk_trigger_mod(stat: &str, value: f64, label: &str) -> Modifier {
    let origin = ModifierSource::new(SourceId::new(
        SourceKind::SkillGem,
        format!("trigger.{stat}"),
    ))
    .with_raw_text(label);
    Modifier::number(stat, ModType::Base, value).with_origin(origin)
}

/// Builds a trigger `FLAG` mod (SkillGem attribution, `trigger.<name>` source id).
pub fn mk_trigger_flag(name: &str, label: &str) -> Modifier {
    let origin = ModifierSource::new(SourceId::new(
        SourceKind::SkillGem,
        format!("trigger.{name}"),
    ))
    .with_raw_text(label);
    Modifier::flag(name).with_origin(origin)
}

/// Finds the "N% increased Effect of Jewel Socket Passive Skills containing Corrupted
/// Magic Jewels" value on a unique jewel (the Adorned). Returns `None` when no unique
/// jewel carries the suffix.
pub fn adorned_corrupted_magic_jewel_inc(jewels: &[pobr_data::item::Item]) -> Option<f64> {
    const SUFFIX: &str =
        "% increased Effect of Jewel Socket Passive Skills containing Corrupted Magic Jewels";
    for jewel in jewels {
        if jewel.rarity != pobr_data::item::ItemRarity::Unique {
            continue;
        }
        let joined = jewel.modifier_texts.join(" ");
        if let Some(pos) = joined.find(SUFFIX) {
            let head = &joined[..pos];
            let num_start = head
                .rfind(|c: char| !c.is_ascii_digit())
                .map_or(0, |i| i + 1);
            if let Ok(v) = head[num_start..].parse::<f64>() {
                return Some(v);
            }
        }
    }
    None
}

/// Vendor's `ModStore:ScaleAddMod` numeric scaling semantics (ModStore.lua:70-79):
/// `m_modf(round(v × scale, 2))` — rounds to 2 decimal places first, then truncates.
pub fn scale_trunc_2dp(value: f64, scale: f64) -> f64 {
    ((value * scale * 100.0).round() / 100.0).trunc()
}

/// A gem's socketed-skill reference (the `gem_skills` domain).
#[derive(Debug, Clone)]
pub struct GemInput {
    /// The granted effect id this gem provides.
    pub skill_id: String,
    /// The gem's level.
    pub gem_level: u32,
    /// The gem's quality.
    pub quality: u32,
    /// The gem's selected statSet index (0-based; `None` = default).
    pub stat_set_index: Option<u32>,
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
pub fn crossbow_reload_modifiers(
    effects: &dyn crate::skill_env::EffectLookup,
    weapons: &dyn crate::skill_env::WeaponBaseLookup,
    equipment: &dyn crate::skill_env::EquipmentView,
    stat_sets: &dyn crate::skill_env::StatSetLookup,
    gems: &[GemInput],
    skill_id: &str,
) -> Vec<Modifier> {
    let Some(effect) = effects.effect(skill_id) else {
        return Vec::new();
    };
    let has_type = |t: &str| effect.skill_types.iter().any(|x| x == t);
    if !has_type("CrossbowSkill") || has_type("Grenade") || has_type("CrossbowAmmoSkill") {
        return Vec::new();
    }
    // Weapon reload base value (main-hand only; matching vendor's `actor.weaponData1.ReloadTime`).
    let Some(reload_ms) = equipment
        .item(pobr_data::item::EquipmentSlot::Weapon1)
        .and_then(|item| weapons.weapon_base(&item.base.to_string()))
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
    let ammo = gems.iter().find_map(|g| {
        let mut candidates: Vec<&str> = vec![g.skill_id.as_str()];
        candidates.extend(
            effects
                .additional_effects(&g.skill_id)
                .iter()
                .map(String::as_str),
        );
        candidates
            .into_iter()
            .find(|eid| {
                effects
                    .effect(eid)
                    .is_some_and(|e| e.skill_types.iter().any(|t| t == "CrossbowAmmoSkill"))
            })
            .map(|eid| (eid.to_string(), g.gem_level))
    });
    if let Some((ammo_id, gem_level)) = ammo {
        let bolts: f64 = stat_sets
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

/// Whether a granted effect is a damaging skill (attack or spell, not Meta).
pub fn is_damage_skill(effect: &pobr_data::catalog::GrantedEffectDef) -> bool {
    (effect.is_attack() || effect.is_spell()) && !effect.skill_types.iter().any(|t| t == "Meta")
}

/// Selects the main skill `(skill_id, gem_level, stat_set_index)` within a single gem
/// group (matching PoB's `socketGroupSkillList` + `mainActiveSkill` semantics):
///
/// 1. Collects **non-support** gems (order preserved, includes meta shells);
/// 2. Selects the Nth one using `main_active_skill` (1-based, defaults to 1, clamped);
/// 3. If the selected entry is a damaging skill → uses it directly; otherwise (a meta
///    shell / non-damaging) pierces through to the group's first damaging skill;
/// 4. When every gem is non-damaging, expands via `gem_effects`'s additional granted
///    effects (the meta/composite gem foreign key) — takes the first additional
///    damaging effect (e.g. ShockwaveTotem → ShockwaveTotemQuakePlayer);
/// 5. Fallback: when `gems` is empty (constructed by the builder's `with_active_skill`),
///    uses `active_skill_id` (the builder path has no statSetIndex concept).
///
/// Returns `None` when the group has no damaging skill candidate at all.
pub fn pick_group_main_skill<'a>(
    gems: &'a [GemInput],
    effects: &'a (dyn crate::skill_env::EffectLookup + 'a),
    main_active_skill: Option<usize>,
    active_skill_id: Option<&'a str>,
    active_gem_level: Option<u32>,
) -> Option<(&'a str, u32, Option<u32>)> {
    // The group's non-support gem list (meta shells count).
    let actives: Vec<&GemInput> = gems
        .iter()
        .filter(|g| {
            effects
                .effect(&g.skill_id)
                .map(|e| !e.is_support)
                .unwrap_or(true)
        })
        .collect();

    if !actives.is_empty() {
        let idx = main_active_skill
            .unwrap_or(1)
            .saturating_sub(1)
            .min(actives.len() - 1);
        let chosen = actives[idx];

        if effects
            .effect(&chosen.skill_id)
            .is_some_and(is_damage_skill)
        {
            return Some((&chosen.skill_id, chosen.gem_level, chosen.stat_set_index));
        }
        if let Some(dmg) = actives
            .iter()
            .find(|g| effects.effect(&g.skill_id).is_some_and(is_damage_skill))
        {
            return Some((&dmg.skill_id, dmg.gem_level, dmg.stat_set_index));
        }
        // T5.6: every gem is non-damaging → expand additional granted effects.
        if let Some(expanded) = actives.iter().find_map(|g| {
            effects
                .additional_effects(&g.skill_id)
                .iter()
                .find(|eid| effects.effect(eid).is_some_and(is_damage_skill))
                .map(|eid| (eid.as_str(), g.gem_level, g.stat_set_index))
        }) {
            return Some(expanded);
        }
        return None;
    }

    active_skill_id.map(|id| (id, active_gem_level.unwrap_or(1), None))
}
