//! Pure skill-group → `Modifier` translations (no context, no `Build`/`BuildData`).
//!
//! These are the engine-semantics helpers extracted from `pobr-build`'s
//! `skill/mods.rs`: they take plain data types and produce `Vec<Modifier>`,
//! letting the same logic be reused by WASM, CLI, and test harnesses.

use pobr_data::catalog::DotFlags;
use pobr_data::modifier::{KeywordFlags, ModType};
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
    ) || ["fire", "cold", "lightning", "chaos"].iter().any(|kind| {
        ["minimum", "maximum"].iter().any(|bound| {
            stat == format!("off_hand_weapon_{bound}_{kind}_damage")
                || stat.starts_with(&format!("off_hand_{bound}_added_{kind}_damage_per_"))
                    && stat.contains("_shield_")
        })
    })
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

/// The keywords a skill of these types answers to: what "Damage with
/// Warcries", "with Fire Skills", "of Movement Skills" or "Curse Skills"
/// ask of the skill. A skill is a Fire skill when it has the Fire type, a
/// Warcry when it has the Warcry type, and so on. Attack and Spell are damage
/// flags ([`skill_type_flags`]); Hit, Arrow and Totem are set where the hit,
/// the arrows and the totem are known.
pub fn skill_keyword_flags(types: pobr_data::skill::SkillTypes) -> KeywordFlags {
    const BY_TYPE: [(&str, KeywordFlags); 12] = [
        ("Aura", KeywordFlags::AURA),
        ("AppliesCurse", KeywordFlags::CURSE),
        ("Warcry", KeywordFlags::WARCRY),
        ("Movement", KeywordFlags::MOVEMENT),
        ("Vaal", KeywordFlags::VAAL),
        ("Physical", KeywordFlags::PHYSICAL),
        ("Fire", KeywordFlags::FIRE),
        ("Cold", KeywordFlags::COLD),
        ("Lightning", KeywordFlags::LIGHTNING),
        ("Chaos", KeywordFlags::CHAOS),
        ("Trapped", KeywordFlags::TRAP),
        ("RemoteMined", KeywordFlags::MINE),
    ];
    BY_TYPE
        .iter()
        .filter(|(name, _)| {
            pobr_data::skill::SkillTypes::from_pob2_name(name).is_some_and(|t| types.intersects(t))
        })
        .fold(KeywordFlags::NONE, |flags, (_, keyword)| flags | *keyword)
}

/// The game's stat on a stat set whose hits are a projectile's: "Projectile
/// Damage" is damage of such a hit.
pub const STAT_IS_PROJECTILE: &str = "base_is_projectile";
/// The game's stat on a stat set whose damage is area damage.
pub const STAT_IS_AREA_DAMAGE: &str = "is_area_damage";

/// The damage flags of a stat set that has the valueless `stats`, from the
/// flags the types of its skill give (`by_types`).
///
/// A skill is of the `Projectile` or `Area` type when some part of it is:
/// Fireball flies and explodes, and only the explosion is area damage;
/// Falling Thunder slams and then fires projectiles. The game marks each
/// stat set, so the set decides, also where the skill has neither type:
/// the arrows of Rain of Arrows fall instead of flying, and their hits are
/// projectile hits. Spell, attack and melee still come from the types.
pub fn stat_set_hit_flags(
    by_types: pobr_data::modifier::ModFlags,
    stats: &[String],
) -> pobr_data::modifier::ModFlags {
    use pobr_data::modifier::ModFlags;
    let has = |stat: &str| stats.iter().any(|s| s == stat);
    let mut flags = by_types
        .without(ModFlags::PROJECTILE)
        .without(ModFlags::AREA);
    if has(STAT_IS_PROJECTILE) {
        flags |= ModFlags::PROJECTILE;
    }
    if has(STAT_IS_AREA_DAMAGE) {
        flags |= ModFlags::AREA;
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
    /// "all Corrupted ... Skill Gems": only a corrupted gem gains the bonus.
    pub corrupted_only: bool,
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
/// mod into a [`GemPropertyBonus`]. Returns `None` for any other form. The game
/// writes the kind either way, "all Cold Skills" or "all Nova Skill Gems"; both
/// leave the kind alone as the category. Named skills also omit "all":
/// Cursecarver's `despair_gem_level_+` is "+N to Level of Despair Skills".
pub fn parse_gem_property_bonus(text: &str) -> Option<GemPropertyBonus> {
    let clean = clean_grant_text(text);
    let body = clean.strip_prefix('+')?;
    let (num, rest) = body.split_once(" to ")?;
    let num = num.strip_suffix('%').unwrap_or(num);
    let value: u32 = num.trim().parse().ok()?;
    let (kind, rest) = if let Some(r) = rest.strip_prefix("level of ") {
        let target = if let Some(category) = r.strip_prefix("all ") {
            category
        } else {
            // Named skills have a complete Skills/Skill Gems suffix. Local
            // socketed and supported-gem bonuses need their own scope.
            if !(r.ends_with(" skills") || r.ends_with(" skill gems"))
                || r.starts_with("socketed ")
                || r.starts_with("supported ")
            {
                return None;
            }
            r
        };
        (GemPropertyKind::Level, target)
    } else {
        (
            GemPropertyKind::Quality,
            rest.strip_prefix("quality of all")?,
        )
    };
    let mut rest = rest.trim();
    let mut attr_req = None;
    // "with an Intelligence requirement", "with a Strength requirement".
    if let Some((head, req)) = rest
        .split_once(" with an ")
        .or_else(|| rest.split_once(" with a "))
    {
        attr_req = Some(match req.trim() {
            "strength requirement" => "str",
            "dexterity requirement" => "dex",
            "intelligence requirement" => "int",
            _ => return None,
        });
        rest = head.trim_end();
    }
    // "Corrupted" names a state of the gem, not a kind of skill.
    let (corrupted_only, rest) = match rest.strip_prefix("corrupted ") {
        Some(rest) => (true, rest),
        None => (false, rest),
    };
    let category = rest
        .strip_suffix("skill gems")
        .or_else(|| rest.strip_suffix("skills"))
        .unwrap_or(rest)
        .trim();
    Some(GemPropertyBonus {
        value,
        kind,
        category: category.to_string(),
        corrupted_only,
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

/// Whether the gem that grants the skill `skill_id`, with the game's
/// `gem_tags`, is of the kind a gem bonus names (`category`, lowercase):
/// - no kind ("all Skills", "all Corrupted Skill Gems") is every gem;
/// - the name of the skill is that skill ("all Shield Wall Skills");
/// - otherwise each word names a tag and the gem has to carry all of them
///   ("all Cold Spell Skills": `cold` and `spell`).
///
/// The game ties each tag to the stat that raises its gems (`GemTags`:
/// `projectile` to `projectile_skill_gem_level_+`, "+# to Level of all
/// Projectile Skills"), so the tag decides, whatever the types of the skills
/// the gem grants: Rain of Arrows is tagged `projectile` and its arrows are
/// not of the `Projectile` skill type. "Elemental" is not a tag: the game's
/// `elemental_skill_gem_level_+` is for the gems of the three elements.
pub fn gem_tags_match_category(category: &str, gem_tags: &[String], skill_id: &str) -> bool {
    if category.is_empty() {
        return true;
    }
    if names_skill(category, skill_id) {
        return true;
    }
    let tagged = |tag: &str| gem_tags.iter().any(|gem_tag| gem_tag == tag);
    category.split_whitespace().all(|word| match word {
        "elemental" => tagged("fire") || tagged("cold") || tagged("lightning"),
        tag => tagged(tag),
    })
}

/// [`gem_tags_match_category`] for a skill whose gem has no tags to go by: a
/// skill no gem in the data grants, or a snapshot adapted before gems
/// carried their tags. The game's `skill_types` of the skill stand in for
/// the tags (see [`kind_skill_types`]).
pub fn gem_level_category_matches(category: &str, skill_types: &[String], skill_id: &str) -> bool {
    if category.is_empty() {
        return true;
    }
    if names_skill(category, skill_id) {
        return true;
    }
    category
        .split_whitespace()
        .all(|word| match kind_skill_types(word) {
            Some(named) => skill_types
                .iter()
                .any(|skill_type| named.contains(&skill_type.as_str())),
            None => skill_types
                .iter()
                .any(|skill_type| skill_type.eq_ignore_ascii_case(word)),
        })
}

/// The skill types a kind of gem goes by where the word a line uses is not the
/// type's own name: a "Totem" gem is one that summons a totem, a "Curse" one
/// that applies a curse, a "Strike" a melee attack on a single target, and an
/// "Elemental" skill is of any of the three elements. Every other kind a line
/// names (Nova, Cold, Spell, Herald, …) is a skill type of that name.
fn kind_skill_types(word: &str) -> Option<&'static [&'static str]> {
    match word {
        "totem" => Some(&["SummonsTotem"]),
        "curse" => Some(&["AppliesCurse"]),
        "strike" => Some(&["MeleeSingleTarget"]),
        "crossbow" => Some(&["CrossbowSkill", "CrossbowAmmoSkill"]),
        "elemental" => Some(&["Fire", "Cold", "Lightning"]),
        _ => None,
    }
}

/// The names lines use for a skill whose name is not its id's
/// (lowercase): Prism of Belief's "+# to Level of all {1} Skills"
/// (`unique_jewel_specific_skill_level_+_skill`) names the skill a tamed
/// beast's gem grants "Tamed Companion".
const SKILL_NAMES: &[(&str, &str)] = &[("SummonBeastPlayer", "tamed companion")];

/// Whether `category` is the name of the skill `skill_id`.
fn names_skill(category: &str, skill_id: &str) -> bool {
    category == skill_name_from_id(skill_id)
        || SKILL_NAMES
            .iter()
            .any(|(id, name)| *id == skill_id && *name == category)
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

/// The Adorned: "N% increased Effect of Jewel Socket Passive Skills containing
/// Corrupted Magic Jewels" or "... Corrupted Rare Jewels". Each applies to the
/// corrupted jewels of that rarity (vendor ModParser.lua:5644,
/// CalcSetup.lua:120-126).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CorruptedJewelEffect {
    pub magic: Option<f64>,
    pub rare: Option<f64>,
}

impl CorruptedJewelEffect {
    const MIDDLE: &str = "% increased Effect of Jewel Socket Passive Skills containing Corrupted ";

    /// `(rarity, N)` when `line` is the whole modifier. Item text wraps it
    /// after "Passive Skills", so a caller joins the two halves with a space.
    pub fn parse_line(line: &str) -> Option<(pobr_data::item::ItemRarity, f64)> {
        let (number, rest) = line.trim().split_once(Self::MIDDLE)?;
        let rarity = match rest {
            "Magic Jewels" => pobr_data::item::ItemRarity::Magic,
            "Rare Jewels" => pobr_data::item::ItemRarity::Rare,
            _ => return None,
        };
        Some((rarity, number.parse().ok()?))
    }

    /// The effects the unique jewels among `jewels` carry.
    pub fn from_jewels<'a>(jewels: impl IntoIterator<Item = &'a pobr_data::item::Item>) -> Self {
        let mut effect = Self::default();
        for jewel in jewels {
            if jewel.rarity != pobr_data::item::ItemRarity::Unique {
                continue;
            }
            let lines = &jewel.modifier_texts;
            let wrapped = lines.windows(2).map(|pair| pair.join(" "));
            for line in lines.iter().cloned().chain(wrapped) {
                match Self::parse_line(&line) {
                    Some((pobr_data::item::ItemRarity::Magic, n)) => effect.magic = Some(n),
                    Some((_, n)) => effect.rare = Some(n),
                    None => {}
                }
            }
        }
        effect
    }

    /// The percent increase to `jewel`'s own modifiers: it must be corrupted
    /// and of a rarity an effect names.
    pub fn increase_for(&self, jewel: &pobr_data::item::Item) -> Option<f64> {
        self.increase(jewel.rarity, jewel.corrupted)
    }

    /// The percent increase for a jewel of this rarity and corruption.
    pub fn increase(&self, rarity: pobr_data::item::ItemRarity, corrupted: bool) -> Option<f64> {
        if !corrupted {
            return None;
        }
        match rarity {
            pobr_data::item::ItemRarity::Magic => self.magic,
            pobr_data::item::ItemRarity::Rare => self.rare,
            _ => None,
        }
    }
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
    /// Global levels resolved in this gem's weapon set, if different from the
    /// calculation's current set. Kept separate from the base gem level.
    pub global_level_bonus: Option<u32>,
    /// The gem's quality.
    pub quality: u32,
    /// The gem's selected statSet index (0-based; `None` = default).
    pub stat_set_index: Option<u32>,
    /// Whether the gem is corrupted.
    pub corrupted: bool,
    /// The monster the gem summons when the player chooses it (a Companion's
    /// beast, a Spectre's monster): a monster variety id.
    pub minion: Option<String>,
    /// Which of its monster's skills the minion uses, counting from 1;
    /// `None` is the first.
    pub minion_skill: Option<u32>,
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
/// generic modifier bus, aggregated on the calc side (`fill_crossbow_reload`). The
/// weapon's own reload speed lines are not among them: they are local
/// (`local_mods.weapon`) and stripped from the global pool.
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
    // The main-hand weapon's reload time.
    let Some(weapon) = equipment.item(pobr_data::item::EquipmentSlot::Weapon1) else {
        return Vec::new();
    };
    let Some(reload_ms) = weapons
        .weapon_base(&weapon.base.to_string())
        .and_then(|w| w.reload_time_ms)
        .filter(|&ms| ms > 0)
    else {
        return Vec::new();
    };
    // The weapon's own reload speed (the game's `local_reload_speed_+%`) and
    // its own attack speed, which changes reload time as attack speed does,
    // change the weapon's reload time before the character's reload and
    // attack speed apply to it (`calc::skill_use_time::crossbow_reload_time`).
    let local_speed = super::item_text::weapon_local_reload_speed(weapon)
        + super::item_text::weapon_local_attack_speed(weapon);
    let local_multiplier = 1.0 + local_speed / 100.0;
    let mut reload_s = f64::from(reload_ms) / 1000.0;
    if local_multiplier > 0.0 {
        reload_s /= local_multiplier;
    }
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
        reload_s,
        format!("crossbow weapon reload {reload_ms}ms, {local_speed:+}% on the weapon"),
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
            .map(|eid| (eid.to_string(), g.gem_level, g.quality))
    });
    if let Some((ammo_id, gem_level, quality)) = ammo {
        let stats = stat_sets.effect_stats(&ammo_id, gem_level, quality, None);
        // The reload action owns both the magazine and its reload-speed stats;
        // selecting the shot must retain these parts of the ammunition cycle.
        let reload_stat = "active_skill_reload_speed_+%_final";
        let reload_speed: f64 = stats
            .all()
            .filter(|s| s.stat == reload_stat)
            .map(|s| s.value)
            .sum();
        if reload_speed != 0.0 {
            mods.push(
                Modifier::number("ReloadSpeed", ModType::More, reload_speed).with_origin(
                    ModifierSource::new(SourceId::new(
                        SourceKind::SkillGem,
                        format!("skill.{ammo_id}.{reload_stat}"),
                    ))
                    .with_raw_text(reload_stat),
                ),
            );
        }
        let bolts: f64 = stats
            .all()
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
    // Requiem explicitly fires freely without ammunition. Neither a one-bolt
    // fallback magazine nor a reload belongs in its active firing cadence.
    if stat_sets
        .effect_stats(skill_id, 1, 0, None)
        .all()
        .any(|s| s.stat == "crossbow_skill_has_no_ammo" && s.value != 0.0)
    {
        mods.push(
            Modifier::flag("CrossbowHasNoAmmo").with_origin(
                ModifierSource::new(SourceId::new(
                    SourceKind::SkillGem,
                    format!("skill.{skill_id}.crossbow_skill_has_no_ammo"),
                ))
                .with_raw_text("crossbow_skill_has_no_ammo"),
            ),
        );
    }
    mods
}

/// Whether a granted effect is a damaging skill (attack or spell, not Meta).
pub fn is_damage_skill(effect: &pobr_data::catalog::GrantedEffectDef) -> bool {
    (effect.is_attack() || effect.is_spell()) && !effect.skill_types.iter().any(|t| t == "Meta")
}

/// One non-support effect in PoB's complete, 1-based `socketGroupSkillList`.
/// Additional effects retain the originating gem's level, quality and stat set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveEffect<'a> {
    pub active_skill_index: usize,
    pub gem_index: usize,
    pub skill_id: &'a str,
    pub gem_level: u32,
    pub quality: u32,
    pub stat_set_index: Option<u32>,
}

/// Enumerates each gem's active effects in its granted-effect display order.
/// Support primaries are omitted, but their granted active effects are not.
/// Unknown effects remain visible so callers can report missing data, not silently
/// renumber or substitute a different skill.
pub fn group_active_effects<'a>(
    gems: impl Iterator<Item = (&'a str, u32, u32, Option<u32>)> + 'a,
    effects: &'a dyn crate::skill_env::EffectLookup,
) -> impl Iterator<Item = ActiveEffect<'a>> + 'a {
    gems.enumerate()
        .flat_map(
            move |(gem_index, (id, gem_level, quality, stat_set_index))| {
                let additional = effects.additional_effects(id);
                let order = effects.effect_display_order(id);
                order
                    .iter()
                    .copied()
                    .enumerate()
                    .filter(move |&(position, index)| {
                        index <= additional.len() && !order[..position].contains(&index)
                    })
                    .map(|(_, index)| index)
                    .chain((0..=additional.len()).filter(move |index| !order.contains(index)))
                    .map(move |index| {
                        if index == 0 {
                            id
                        } else {
                            additional[index - 1].as_str()
                        }
                    })
                    .filter(move |id| {
                        !id.is_empty() && effects.effect(id).is_none_or(|effect| !effect.is_support)
                    })
                    .map(move |skill_id| ActiveEffect {
                        active_skill_index: 0,
                        gem_index,
                        skill_id,
                        gem_level,
                        quality,
                        stat_set_index,
                    })
            },
        )
        .enumerate()
        .map(|(index, mut effect)| {
            effect.active_skill_index = index + 1;
            effect
        })
}

/// Selects an exact complete active-effect ordinal, defaulting to the first.
/// Invalid ordinals are not clamped and utility effects are not pierced.
pub fn pick_group_main_skill<'a>(
    gems: &'a [GemInput],
    effects: &'a (dyn crate::skill_env::EffectLookup + 'a),
    main_active_skill: Option<usize>,
    active_skill_id: Option<&'a str>,
    active_gem_level: Option<u32>,
) -> Option<(&'a str, u32, Option<u32>)> {
    let index = main_active_skill.unwrap_or(1).checked_sub(1)?;
    let builder = active_skill_id
        .filter(|_| gems.is_empty())
        .map(|id| (id, active_gem_level.unwrap_or(1), 0, None));
    let gems = gems.iter().map(|gem| {
        (
            gem.skill_id.as_str(),
            gem.gem_level,
            gem.quality,
            gem.stat_set_index,
        )
    });
    group_active_effects(gems.chain(builder), effects)
        .nth(index)
        .map(|effect| (effect.skill_id, effect.gem_level, effect.stat_set_index))
}

/// Data channel: the statmap data engine. Maps a stat list through
/// [`stat_map_engine::map_stat`] into `Modifier`s with `SourceId` attribution.
///
/// `SkillData` items have no consumer yet and are ignored (don't participate in the
/// calculation, can't cause miscalculation); `Unsupported` / `Unknown` are silently
/// skipped (classification observation goes through Compare mode).
///
/// Returns empty when `catalog` is `None` (old data pack — the data channel misses
/// entirely, matching `mapped_stat_modifiers`'s semantics).
pub fn data_mapped_stat_modifiers(
    stats: &[pobr_data::catalog::SkillDamageStat],
    source_kind: SourceKind,
    label_prefix: &str,
    effect_id: &str,
    set_key: Option<&str>,
    catalog: Option<&crate::rules::stat_map_engine::StatMapCatalog>,
) -> Vec<Modifier> {
    let mut ctx = crate::skill_env::StatMapCtx::data(catalog);
    crate::skill_env::mapped_stat_modifiers(
        &mut ctx,
        stats,
        source_kind,
        label_prefix,
        effect_id,
        set_key,
    )
}

/// The CostTypes channel is already divided into points or percent per use
/// or per second. Mana is injected by the existing base-parameter adapter.
pub fn additional_resource_cost_modifiers(costs: &[super::ResolvedCost]) -> Vec<Modifier> {
    costs
        .iter()
        .filter_map(|cost| {
            if cost.resource == "Mana" && !cost.per_second {
                return None;
            }
            let key = cost.resource.replace("PerMinute", "PerSecond");
            if !matches!(
                key.as_str(),
                "Life"
                    | "ES"
                    | "Ward"
                    | "Rage"
                    | "ManaPercent"
                    | "LifePercent"
                    | "ESPercent"
                    | "WardPercent"
                    | "UnreservedManaPercent"
                    | "ManaPerSecond"
                    | "LifePerSecond"
                    | "ESPerSecond"
                    | "WardPerSecond"
                    | "RagePerSecond"
                    | "ManaPercentPerSecond"
                    | "LifePercentPerSecond"
                    | "ESPercentPerSecond"
            ) {
                return None;
            }
            Some(
                Modifier::number(format!("Skill{key}CostBase"), ModType::Base, cost.amount)
                    .with_origin(ModifierSource::new(SourceId::new(
                        SourceKind::SkillGem,
                        format!("skill.{key}Cost"),
                    ))),
            )
        })
        .collect()
}
