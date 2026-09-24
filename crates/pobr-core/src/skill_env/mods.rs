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
