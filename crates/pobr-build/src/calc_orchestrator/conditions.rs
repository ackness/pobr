//! conditions — skill_types→flags/conditions conversion, damage keyword derivation, weapon type conditions.

use pobr_core::CalcConfig;
use pobr_data::item::EquipmentSlot;
use pobr_data::modifier::ModFlags;
use pobr_data::skill::SkillTypes;

use crate::build::Build;
use crate::build_data::BuildData;

/// Main skill keyword + main weapon category → extra damage-scaling ModName
/// (`GrenadeDamage`/`CrossbowDamage` etc). Makes skill/weapon-scoped damage boosts like
/// `increased Grenade Damage` / `Damage with Crossbows` take effect.
pub(crate) fn damage_keywords(
    build: &Build,
    data: &BuildData,
    skill_types: &[String],
) -> Vec<String> {
    let mut names = Vec::new();
    // Skill keyword (a non-flag damage keyword, e.g. Grenade).
    if skill_types.iter().any(|t| t == "Grenade") {
        names.push("GrenadeDamage".to_string());
    }
    // Main weapon category → weapon-type damage.
    if let Some(item) = build.items.get(&EquipmentSlot::Weapon1)
        && let Some(def) = data.base_items.get(&item.base.to_string())
    {
        // An allowlist mapping flag → damage ModName (an L4 code-side derivation): only
        // covers weapon categories pobr already has a consumer for (equivalent per
        // category to the old contains-based check). TODO(parity): vendor also has
        // Sword/Axe/Claw/... flags, but pobr currently has no matching `<X>Damage`
        // consumer chain, so those aren't derived.
        let kw = weapon_type_info(data, &def.item_class).and_then(|w| match w.flag.as_str() {
            "Crossbow" => Some("CrossbowDamage"),
            "Bow" => Some("BowDamage"),
            // vendor records the Quarterstaff's flag as "Staff" (label = Quarterstaff).
            "Staff" => Some("QuarterstaffDamage"),
            "Mace" => Some("MaceDamage"),
            "Spear" => Some("SpearDamage"),
            _ => None,
        });
        if let Some(k) = kw {
            names.push(k.to_string());
        }
    }
    names
}

/// Looks up an entry in the injected weapon type table (`data.constants.weapon_types`,
/// sourced from vendor's `data.weaponTypeInfo`) by GGG's `item_class`. The key-space
/// mismatch (documented in the schema doc) is resolved here:
///
/// - GGG records the quarterstaff's item_class as `Warstaff`, but vendor's table key is
///   `Staff` (`label = "Quarterstaff"`);
/// - GGG's `Staff` (the staff base type, 17 entries in the repo data) has **no matching
///   weapon type entry** in vendor's PoE2 base data — it must not be mismatched to the
///   table key `Staff` (that's the quarterstaff), nor mapped to the legacy `Warstaff`
///   entry; returns `None` (matching the old scattered predicates' behavior: staves
///   aren't melee, have no Using*/damage keywords);
/// - GGG's `FishingRod` → table key `Fishing Rod` (with a space).
pub(crate) fn weapon_type_info<'a>(
    data: &'a BuildData,
    item_class: &str,
) -> Option<&'a pobr_data::catalog::WeaponTypeDef> {
    let key = match item_class {
        "Warstaff" => "Staff",
        "Staff" => return None,
        "FishingRod" => "Fishing Rod",
        other => other,
    };
    data.constants.weapon_types.get(key)
}

/// Main-hand weapon → cfg weapon bits (matching vendor's `getWeaponFlags`,
/// `CalcActiveSkill.lua:274-309`; introduced in commit-2, permanent from the switch
/// commit on): used so mod-side weapon-bit matching (mod.flags ⊆ cfg.flags subset
/// match) can hit.
///
/// Derived from the same source as [`weapon_type_conditions`]
/// ([`weapon_type_info`], the same `weapon_types.json` table): for every dual-written
/// mod, the bit channel's determination is implied by the condition channel (the two
/// channels ANDed together ≡ the old single condition channel), see
/// weapon_type_conditions's guard-semantics comparison. An empty main hand → matching
/// vendor's `weaponData.type = "None"` → only the `Unarmed` bit.
pub(crate) fn weapon_cfg_flags(build: &Build, data: &BuildData) -> ModFlags {
    let Some(item) = build.items.get(&EquipmentSlot::Weapon1) else {
        return ModFlags::weapon_flags("None", "Unarmed", true, true);
    };
    let Some(def) = data.base_items.get(&item.base.to_string()) else {
        return ModFlags::NONE;
    };
    weapon_type_info(data, &def.item_class)
        .map(|w| ModFlags::weapon_flags(&w.id, &w.flag, w.one_hand, w.melee))
        .unwrap_or(ModFlags::NONE)
}

/// Whether the off-hand slot has a shield equipped (PoB2's `Condition:UsingShield`).
/// Determined from the `Weapon2` slot's base `item_class` in the currently active
/// equipment group (of `Shield`/`Buckler`/`Focus`, only the Shield category counts).
/// Generic, not specialized.
pub(crate) fn main_hand_offhand_is_shield(build: &Build, data: &BuildData) -> bool {
    let Some(item) = build.items.get(&EquipmentSlot::Weapon2) else {
        return false;
    };
    let Some(def) = data.base_items.get(&item.base.to_string()) else {
        return false;
    };
    def.item_class.as_str().contains("Shield")
}

/// PoB2 condition implications (ConfigOptions.lua's `implyCond`/`implyCondList`): a
/// parent condition being true sets its child condition too. Only covers chains related
/// to offence aggregation that PoBR's mods can already parse as condition tags;
/// generic, independent of build/skill.
pub(crate) fn apply_condition_implications(mut cfg: CalcConfig) -> CalcConfig {
    // An ignited enemy must also be burning (PoB2's `conditionEnemyIgnited` implyCond `Burning`).
    if cfg.condition("EnemyIgnited") {
        cfg = cfg.with_condition("EnemyBurning", true);
    }
    // A frozen enemy must also be chilled (PoB2's `conditionEnemyFrozen` implyCond `Chilled`).
    if cfg.condition("EnemyFrozen") {
        cfg = cfg.with_condition("EnemyChilled", true);
    }
    cfg
}

/// Main-hand weapon category → weapon type / grip condition vars (for tree/mods like
/// "... with <weapon class>" or "while dual wielding"). PoE2's internal class name:
/// Quarterstaff = `Warstaff`. Returns the list of condition vars to set true.
pub(crate) fn weapon_type_conditions(build: &Build, data: &BuildData) -> Vec<&'static str> {
    let Some(item) = build.items.get(&EquipmentSlot::Weapon1) else {
        return Vec::new();
    };
    let Some(def) = data.base_items.get(&item.base.to_string()) else {
        return Vec::new();
    };
    let cls = def.item_class.as_str();
    // Grip/melee determination moved from scattered string predicates to the injected
    // weapon_types table (`data.constants.weapon_types` ← `base/weapon_types.json`,
    // sourced from vendor's `data.weaponTypeInfo`; see [`weapon_type_info`] for the GGG
    // item_class → table key mapping).
    let info = weapon_type_info(data, cls);
    let mut vars = Vec::new();
    // Weapon type condition var: an allowlist mapping table flag → the `Using*`
    // conditions pobr already consumes (an L4 code-side derivation, equivalent per
    // category to the old contains-based check). TODO(parity): vendor also has
    // Sword/Axe/Claw/Flail/Wand etc. flags, but pobr currently has no matching `Using*`
    // consumer, so those aren't set.
    if let Some(var) = info.and_then(|w| match w.flag.as_str() {
        // vendor records the Quarterstaff's flag as "Staff" (label = Quarterstaff).
        "Staff" => Some("UsingQuarterstaff"),
        "Mace" => Some("UsingMace"),
        "Crossbow" => Some("UsingCrossbow"),
        "Bow" => Some("UsingBow"),
        "Spear" => Some("UsingSpear"),
        "Dagger" => Some("UsingDagger"),
        _ => None,
    }) {
        vars.push(var);
    }
    // Melee / two-handed classification (table's melee / !one_hand). A ported invariant
    // guard (zero behavior change): TODO(parity): vendor records melee=true for
    // Talisman / Fishing Rod, and oneHand=false for Bow/Crossbow/Talisman/Fishing Rod;
    // pobr's old predicates instead treated them as non-melee / one-handed
    // respectively (affecting the Using<X>HandedMelee and DualWielding
    // determinations) — this guard pins down the old behavior (same TODO as the
    // schema doc); aligning with vendor's data is left for its own behavior commit.
    let melee = info.is_some_and(|w| w.melee) && !matches!(cls, "Talisman" | "FishingRod");
    let two_handed = match cls {
        // The old predicate treats these classes as one-handed (vendor's
        // oneHand=false, see the TODO(parity) above).
        "Bow" | "Crossbow" | "Talisman" | "FishingRod" => false,
        // GGG's staff class: vendor's table has no entry (see weapon_type_info), and the old predicate treats it as two-handed.
        "Staff" => true,
        _ => info.is_some_and(|w| !w.one_hand),
    };
    if melee {
        vars.push(if two_handed {
            "UsingTwoHandedMelee"
        } else {
            "UsingOneHandedMelee"
        });
    }
    // Dual wielding: the off-hand is also a weapon base (not a shield/quiver/foci off-hand).
    if !two_handed
        && let Some(off) = build.items.get(&EquipmentSlot::Weapon2)
        && data.weapon_base(&off.base.to_string()).is_some()
    {
        vars.push("DualWielding");
    }
    vars
}

/// Skill type name (`ActiveSkillType.Id`) → `cfg.skill_types` (the attack/spell
/// classification bits).
///
/// Previously the orchestrator only set `ModFlags` and never filled in
/// `CalcConfig::skill_types`, which made `cfg.is_attack()`/`cfg.is_spell()` always
/// return false for every build — spells were incorrectly subjected to
/// accuracy/evasion hit checks (vendor CalcOffence.lua:2611-2612: `if not isAttack then
/// output.AccuracyHitChance = 100`, spells/non-attacks always hit), and under the
/// affected semantics this also incorrectly under-scaled crit (`:3700`'s crit
/// double-hit-check only multiplies `AccuracyHitChance`).
///
/// (Data-driven A1) Sets bits **fully**: every type name a skill carries is mapped
/// through the single source `SkillTypes::from_pob2_name` (a generated table covering
/// all 290 enum values from vendor's Global.lua), isomorphic to vendor's
/// `activeSkill.skillTypes` (all types set true) — the tag side
/// (`ModTag::SkillTypes` in template.rs / special_mod.rs) went fully data-driven in the
/// same commit; both sides must be opened together — narrowing just one side would
/// break existing mods that currently pass by "dropping the tag makes it apply
/// globally". An unknown name (the data is a subset of the enum; a miss means corrupt
/// data) panics in debug builds, ignored in release.
pub(crate) fn skill_type_bits(skill_types: &[String]) -> SkillTypes {
    let mut bits = SkillTypes::NONE;
    for t in skill_types {
        match SkillTypes::from_pob2_name(t) {
            Some(st) => bits |= st,
            None => debug_assert!(false, "unknown SkillType name: {t}"),
        }
    }
    bits
}

/// Skill type name (`ActiveSkillType.Id`) → cfg damage flags. Used by damage
/// aggregation to pull `<Projectile|Area|Spell|Melee>Damage` boosts by skill category.
pub(crate) fn skill_type_flags(skill_types: &[String]) -> ModFlags {
    let mut flags = ModFlags::NONE;
    for t in skill_types {
        match t.as_str() {
            "Attack" => flags |= ModFlags::ATTACK,
            "Spell" => flags |= ModFlags::SPELL,
            "Melee" => flags |= ModFlags::MELEE,
            "Projectile" | "ProjectilesFromUser" => flags |= ModFlags::PROJECTILE,
            "Area" | "AreaSpell" => flags |= ModFlags::AREA,
            _ => {}
        }
    }
    // A hit skill → ModFlag.Hit (matching vendor CalcActiveSkill.lua:176's
    // `skillFlags.hit = … or skillTypes[Attack] or skillTypes[Damage] or
    // skillTypes[Projectile]` + :523-525's `skillModFlags |= ModFlag.Hit`).
    // Makes mods carrying the HIT flag (e.g. the "Spell Hits Gain …" family) apply to
    // hit skills; DoT cfg already strips this bit, matching vendor
    // (calc::skill_dot's `flags.without(HIT)`).
    if skill_types
        .iter()
        .any(|t| matches!(t.as_str(), "Attack" | "Damage" | "Projectile"))
    {
        flags |= ModFlags::HIT;
    }
    flags
}

/// Whether any enabled skill summons a companion (`SkillType.CreatesCompanion`) —
/// equivalent to vendor's `companionInPresence` config option's `ifSkillType` gate
/// (ConfigOptions.lua:1012). Determined generically by skill type token, never targets a
/// specific skill id.
pub(crate) fn build_has_companion_skill(build: &Build, data: &BuildData) -> bool {
    build.enabled_socket_groups().any(|group| {
        group.gem_skills.iter().any(|gem| {
            data.granted_effects.get(&gem.skill_id).is_some_and(|e| {
                !e.is_support && e.skill_types.iter().any(|t| t == "CreatesCompanion")
            })
        })
    })
}

/// Counts the number of distinct granted effects among enabled active skills with
/// `SkillType.Grenade`, deduplicated (matching vendor CalcPerform.lua:1238-1242: walks
/// activeSkillList, deduplicating by grantedEffect.id →
/// `env.modDB.multipliers["GrenadeTypes"]`). The Multiplier limitVar denominator for the
/// Demolitionist ascendancy's "for every different Grenade fired …".
pub(crate) fn grenade_type_count(build: &Build, data: &BuildData) -> f64 {
    let mut seen = std::collections::HashSet::new();
    for group in build.enabled_socket_groups() {
        for gem in &group.gem_skills {
            if seen.contains(gem.skill_id.as_str()) {
                continue;
            }
            let Some(effect) = data.granted_effects.get(&gem.skill_id) else {
                continue;
            };
            if !effect.is_support && effect.skill_types.iter().any(|t| t == "Grenade") {
                seen.insert(gem.skill_id.as_str());
            }
        }
    }
    seen.len() as f64
}

/// Combat conditions derived from the main skill (read directly from vendor
/// `CalcPerform.lua:242-266`, the `if env.mode_combat` section).
///
/// Line-by-line comparison against vendor:
/// - **Exemption** (:248 `not skillData.triggered and not trap/mine/totem`): the PoE2
///   data-token equivalents are `Triggered`/`InbuiltTrigger` (triggered, same
///   determination as `trigger_modifiers`), `RemoteMined` (mine), `SummonsTotem`
///   (totem); trap has no token in PoE2's cataloged data (0 hits), so there's no
///   matching exemption for it.
/// - attack → `AttackedRecently` **else if** spell → `CastSpellRecently` (:249-253,
///   the mutually-exclusive branches carried over verbatim);
/// - `SkillType.Movement` → `UsedMovementSkillRecently` (:254-256);
/// - minion and not duration → `UsedMinionSkillRecently` (:257-259, matching vendor's
///   `skillFlags.minion and not skillFlags.duration` → the `Minion` token present and no
///   `Duration` token);
/// - `SkillType.Vaal` → `UsedVaalSkillRecently` (:260-262; PoE2's cataloged data has no
///   `Vaal` token, kept to match vendor, but never actually triggers);
/// - `SkillType.Channel` → `Channelling` (:264-266, an existing consumer on offence's
///   channelling branch in `offence.rs`).
pub(crate) fn combat_conditions(
    skill_types: &[String],
    skill_flags: ModFlags,
) -> Vec<&'static str> {
    let has = |t: &str| skill_types.iter().any(|x| x == t);
    if has("Triggered") || has("InbuiltTrigger") || has("RemoteMined") || has("SummonsTotem") {
        return Vec::new();
    }
    let mut conds = Vec::new();
    if skill_flags.intersects(ModFlags::ATTACK) {
        conds.push("AttackedRecently");
    } else if skill_flags.intersects(ModFlags::SPELL) {
        conds.push("CastSpellRecently");
    }
    if has("Movement") {
        conds.push("UsedMovementSkillRecently");
    }
    if has("Minion") && !has("Duration") {
        conds.push("UsedMinionSkillRecently");
    }
    if has("Vaal") {
        conds.push("UsedVaalSkillRecently");
    }
    if has("Channel") {
        conds.push("Channelling");
    }
    conds
}
