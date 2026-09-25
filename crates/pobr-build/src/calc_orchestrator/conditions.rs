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
/// Derived from [`weapon_type_info`] and the same `weapon_types.json` table as
/// [`weapon_type_conditions`]. Unlike grip conditions (which consider both hands),
/// these per-skill matching bits describe the main-hand weapon source; the per-hand
/// attack pass can replace them with its offhand bits. An empty main hand →
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

/// Equipped weapon categories → weapon type / grip conditions. PoE2's GGG
/// item class for Quarterstaff is `Warstaff`. Grip and melee use vendor's
/// `weaponTypeInfo` predicates (CalcPerform.lua:305-351), independently of
/// `ModFlags::Weapon1H/2H` used by per-skill weapon matching.
pub(crate) fn weapon_type_conditions(build: &Build, data: &BuildData) -> Vec<&'static str> {
    let Some(item) = build.items.get(&EquipmentSlot::Weapon1) else {
        return Vec::new();
    };
    let Some(def) = data.base_items.get(&item.base.to_string()) else {
        return Vec::new();
    };
    // An unmapped caster Staff has no main-hand type; it must not become
    // a one-handed dual-wield source, but a valid offhand still has conditions.
    let main = weapon_type_info(data, &def.item_class);
    let mut vars = Vec::new();
    // The vendor considers both hands for Using* conditions. Only the subset of
    // flag conditions consumed by PoBR is surfaced here; grip/melee comes from
    // the table, not from an item-class string predicate.
    for info in [
        main,
        build.items.get(&EquipmentSlot::Weapon2).and_then(|off| {
            let off_def = data.base_items.get(&off.base.to_string())?;
            data.weapon_base(&off.base.to_string())?;
            weapon_type_info(data, &off_def.item_class)
        }),
    ]
    .into_iter()
    .flatten()
    {
        let category = match info.flag.as_str() {
            "Staff" => Some("UsingQuarterstaff"),
            "Mace" => Some("UsingMace"),
            "Crossbow" => Some("UsingCrossbow"),
            "Bow" => Some("UsingBow"),
            "Spear" => Some("UsingSpear"),
            "Dagger" => Some("UsingDagger"),
            _ => None,
        };
        if let Some(var) = category
            && !vars.contains(&var)
        {
            vars.push(var);
        }
        if info.melee {
            let grip = if info.one_hand {
                "UsingOneHandedMelee"
            } else {
                "UsingTwoHandedMelee"
            };
            if !vars.contains(&grip) {
                vars.push(grip);
            }
        }
    }
    // A valid second one-handed weapon is required for a dual-wield attack;
    // shields, quivers, and unmapped bases aren't second weapon sources.
    if main.is_some_and(|w| w.one_hand)
        && build.items.get(&EquipmentSlot::Weapon2).is_some_and(|off| {
            data.weapon_base(&off.base.to_string()).is_some()
                && data
                    .base_items
                    .get(&off.base.to_string())
                    .is_some_and(|def| {
                        weapon_type_info(data, &def.item_class).is_some_and(|w| w.one_hand)
                    })
        })
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
    pobr_core::skill_env::skill_type_bits(skill_types)
}

/// Skill type name (`ActiveSkillType.Id`) → cfg damage flags. Used by damage
/// aggregation to pull `<Projectile|Area|Spell|Melee>Damage` boosts by skill category.
pub(crate) fn skill_type_flags(skill_types: &[String]) -> ModFlags {
    pobr_core::skill_env::skill_type_flags(skill_types)
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

#[cfg(test)]
mod weapon_condition_tests {
    use super::*;
    use pobr_data::catalog::{BaseItemDef, WeaponBaseStats};
    use pobr_data::item::{Item, ItemBaseId, ItemRarity, RolledDefence};

    fn equip(build: Build, data: &mut BuildData, slot: EquipmentSlot, class: &str) -> Build {
        let name = format!("Test {class}");
        data.base_items.insert(
            name.clone(),
            BaseItemDef {
                id: format!("Test/{class}"),
                name: name.clone(),
                item_class: class.into(),
                req_str: 0,
                req_dex: 0,
                req_int: 0,
                drop_level: 1,
                width: 1,
                height: 1,
                tags: vec![],
                implicits: vec![],
                mod_domain: 1,
                weapon: Some(WeaponBaseStats {
                    physical_min: 2,
                    physical_max: 5,
                    speed_ms: 600,
                    crit_chance: 500,
                    range: 0,
                    reload_time_ms: None,
                }),
                armour: None,
                spirit: None,
                charm_buff: vec![],
            },
        );
        build.set_item(
            slot,
            Item {
                base: ItemBaseId::from(name.as_str()),
                rarity: ItemRarity::Normal,
                quality: 0,
                corrupted: false,
                implicit_texts: vec![],
                modifier_texts: vec![],
                enchant_texts: vec![],
                rolled_defence: RolledDefence::default(),
                parsed_stats: vec![],
            },
        )
    }

    #[test]
    fn vendor_grip_conditions_and_dual_wield_require_known_one_hand_sources() {
        let mut data = BuildData::empty();
        for (class, expected) in [
            ("Bow", vec!["UsingBow"]),
            ("Crossbow", vec!["UsingCrossbow"]),
            ("Wand", vec![]),
            ("Warstaff", vec!["UsingQuarterstaff", "UsingTwoHandedMelee"]),
            ("Staff", vec![]),
            ("Talisman", vec!["UsingTwoHandedMelee"]),
            ("FishingRod", vec!["UsingTwoHandedMelee"]),
        ] {
            let build = equip(Build::new(), &mut data, EquipmentSlot::Weapon1, class);
            assert_eq!(weapon_type_conditions(&build, &data), expected, "{class}");
            let dual = equip(build, &mut data, EquipmentSlot::Weapon2, "Dagger");
            let conditions = weapon_type_conditions(&dual, &data);
            assert_eq!(
                conditions.contains(&"DualWielding"),
                class == "Wand",
                "{class}"
            );
            assert!(
                conditions.contains(&"UsingDagger"),
                "offhand category: {class}"
            );
        }
        let main = equip(
            Build::new(),
            &mut data,
            EquipmentSlot::Weapon1,
            "One Hand Mace",
        );
        let dual = equip(main, &mut data, EquipmentSlot::Weapon2, "Dagger");
        assert_eq!(
            weapon_type_conditions(&dual, &data),
            [
                "UsingMace",
                "UsingOneHandedMelee",
                "UsingDagger",
                "DualWielding"
            ]
        );
        let swapped = equip(dual, &mut data, EquipmentSlot::Weapon2, "Bow");
        assert!(!weapon_type_conditions(&swapped, &data).contains(&"DualWielding"));
    }
}
