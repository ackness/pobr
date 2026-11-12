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
    if attacks_with_weapon(skill_types)
        && let Some(item) = build.items.get(&EquipmentSlot::Weapon1)
        && let Some(base) = data.base_item(&item.base.to_string())
        && let Some(row) = weapon_type_info(data, &base.item_class)
    {
        names.push(row.damage_keyword());
    }
    names
}

/// Whether a skill attacks with the weapon it is used with: an attack, and
/// not one with its own base damage in place of a weapon's (`NonWeaponAttack`,
/// Shield Wall's "replace off hand unarmed attack stats with shield type").
/// "Damage with Maces" and the other weapon-type modifiers are about the
/// weapon's hits, so a spell or a shield attack does not take them.
pub(crate) fn attacks_with_weapon(skill_types: &[String]) -> bool {
    skill_types.iter().any(|t| t == "Attack") && !skill_types.iter().any(|t| t == "NonWeaponAttack")
}

/// Resolve the equipment class through the table's shared compatibility aliases.
pub(crate) fn weapon_type_info<'a>(
    data: &'a BuildData,
    item_class: &str,
) -> Option<&'a pobr_data::catalog::WeaponTypeDef> {
    data.constants.weapon_types.get_by_item_class(item_class)
}

/// Main-hand weapon bits from the same row as the build's Using conditions.
/// The per-hand attack pass substitutes the offhand row's bits when needed.
/// Empty main hand is unarmed and has no generic weapon/grip/range bits.
pub(crate) fn weapon_cfg_flags(build: &Build, data: &BuildData) -> ModFlags {
    let Some(item) = build.items.get(&EquipmentSlot::Weapon1) else {
        return ModFlags::weapon_flags("None", "Unarmed", true, true);
    };
    let Some(def) = data.base_item(&item.base.to_string()) else {
        return ModFlags::NONE;
    };
    weapon_type_info(data, &def.item_class)
        .map(|w| ModFlags::weapon_flags(&w.id, &w.flag, w.one_hand, w.melee))
        .unwrap_or(ModFlags::NONE)
}

/// Partial type-bit support is visible through the existing unsupported channel.
pub(crate) fn weapon_support_limits(build: &Build, data: &BuildData) -> Vec<String> {
    [EquipmentSlot::Weapon1, EquipmentSlot::Weapon2]
        .into_iter()
        .filter_map(|slot| {
            let item = build.items.get(&slot)?;
            let base = data.base_item(&item.base.to_string())?;
            let info = weapon_type_info(data, &base.item_class)?;
            ModFlags::weapon_type_bit(&info.flag).is_none().then(|| format!(
                "Weapon {}: unsupported weapon flag {}; generic weapon classification and Using{} conditions remain available",
                slot.id(), info.flag, info.flag
            ))
        })
        .collect()
}

/// Whether the off-hand slot has a shield equipped (PoB2's `Condition:UsingShield`).
/// Determined from the `Weapon2` slot's base `item_class` in the currently active
/// equipment group (of `Shield`/`Buckler`/`Focus`, only the Shield category counts).
/// Generic, not specialized.
pub(crate) fn main_hand_offhand_is_shield(build: &Build, data: &BuildData) -> bool {
    let Some(item) = build.items.get(&EquipmentSlot::Weapon2) else {
        return false;
    };
    let Some(def) = data.base_item(&item.base.to_string()) else {
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
    cfg
}

/// Equipped weapon categories → weapon type / grip conditions. The game's
/// item class for a Quarterstaff is `Warstaff`; the caster `Staff` class is
/// not a weapon type and sets nothing. Whether a weapon is one- or two-handed
/// and melee comes from the weapon type table (`weapon_types.json`),
/// independently of `ModFlags::Weapon1H/2H` used by per-skill weapon matching.
pub(crate) fn weapon_type_conditions(build: &Build, data: &BuildData) -> Vec<String> {
    let Some(item) = build.items.get(&EquipmentSlot::Weapon1) else {
        return Vec::new();
    };
    let Some(def) = data.base_item(&item.base.to_string()) else {
        return Vec::new();
    };
    // An unmapped caster Staff has no main-hand type; it must not become
    // a one-handed dual-wield source, but a valid offhand still has conditions.
    let main = weapon_type_info(data, &def.item_class);
    let mut vars = Vec::new();
    // Both equipped hands supply conditions from the loaded rows.
    for info in [
        main,
        build.items.get(&EquipmentSlot::Weapon2).and_then(|off| {
            let off_def = data.base_item(&off.base.to_string())?;
            data.weapon_base(&off.base.to_string())?;
            weapon_type_info(data, &off_def.item_class)
        }),
    ]
    .into_iter()
    .flatten()
    {
        for var in info.conditions() {
            if !vars.contains(&var) {
                vars.push(var);
            }
        }
        if info.melee {
            if !vars.iter().any(|var| var == "UsingMeleeWeapon") {
                vars.push("UsingMeleeWeapon".into());
            }
            let grip = if info.one_hand {
                "UsingOneHandedMelee"
            } else {
                "UsingTwoHandedMelee"
            };
            if !vars.iter().any(|var| var == grip) {
                vars.push(grip.into());
            }
        }
    }
    // A valid second one-handed weapon is required for a dual-wield attack;
    // shields, quivers, and unmapped bases aren't second weapon sources.
    // Giant's Blood is settled after source injection, including keystones
    // granted by an item. Category/grip conditions above retain base identity.
    if pobr_core::skill_env::can_dual_wield(build, data, false) {
        vars.push("DualWielding".into());
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

/// Combat conditions that follow from the character using the main skill, by its
/// final skill types:
///
/// - a skill that hits → `HitRecently`, and a spell that hits also
///   `HitSpellRecently` ("if you've Hit an Enemy Recently"): the hits of the
///   skills the character uses or triggers are its own, those of its mines and
///   totems are not;
/// - **Exemption**: a triggered skill (`Triggered`/`InbuiltTrigger`, the same
///   determination as `trigger_modifiers`) or a mine (`RemoteMined`) is not the
///   character using it, and gives nothing below. Traps have no skill type in
///   the game data.
/// - an attack → `AttackedRecently`, otherwise a spell → `CastSpellRecently`;
/// - `Movement` → `UsedMovementSkillRecently`;
/// - `Minion` without `Duration` → `UsedMinionSkillRecently`;
/// - `Vaal` → `UsedVaalSkillRecently` (no skill in the game data has the type);
/// - `Channel` → `Channelling` (read by offence's channelling branch);
/// - `Shapeshift`, a skill that shapeshifts its user → `Shapeshifted`, and with
///   `Bear`, the form it takes → `BearForm` (what the form gives:
///   [`pobr_core::calc::bear_form`]).
///
/// A skill a totem uses ([`used_by_totem`]) is not the character attacking or
/// casting. For it to be in use the character has summoned the totem, so it
/// has a totem and has summoned one recently.
pub(crate) fn combat_conditions(
    skill_types: &[String],
    skill_flags: ModFlags,
) -> Vec<&'static str> {
    let has = |t: &str| skill_types.iter().any(|x| x == t);
    if used_by_totem(skill_types) {
        return vec!["HaveTotem", "SummonedTotemRecently"];
    }
    if has("RemoteMined") {
        return Vec::new();
    }
    let mut conds = Vec::new();
    if skill_flags.intersects(ModFlags::HIT) {
        conds.push("HitRecently");
        if skill_flags.intersects(ModFlags::SPELL) {
            conds.push("HitSpellRecently");
        }
    }
    if has("Triggered") || has("InbuiltTrigger") {
        return conds;
    }
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
    if has("Shapeshift") {
        conds.push("Shapeshifted");
        if has("Bear") {
            conds.push(pobr_core::calc::bear_form::BEAR_FORM);
        }
    }
    conds
}

/// Whether a totem uses the skill with these final types: the skill of a totem
/// gem itself, a skill socketed in a meta totem gem (whose support half adds
/// the game's `UsedByTotem`), or the skill that places the totem.
pub(crate) fn used_by_totem(skill_types: &[String]) -> bool {
    skill_types
        .iter()
        .any(|skill_type| skill_type == "UsedByTotem" || skill_type == "SummonsTotem")
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
                utility: None,
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
                socketed: Vec::new(),
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
            ("Wand", vec!["UsingWand"]),
            (
                "Warstaff",
                vec![
                    "UsingQuarterstaff",
                    "UsingStaff",
                    "UsingMeleeWeapon",
                    "UsingTwoHandedMelee",
                ],
            ),
            ("Staff", vec![]),
            (
                "Talisman",
                vec!["UsingTalisman", "UsingMeleeWeapon", "UsingTwoHandedMelee"],
            ),
            (
                "FishingRod",
                vec!["UsingFishing", "UsingMeleeWeapon", "UsingTwoHandedMelee"],
            ),
        ] {
            let build = equip(Build::new(), &mut data, EquipmentSlot::Weapon1, class);
            assert_eq!(weapon_type_conditions(&build, &data), expected, "{class}");
            let dual = equip(build, &mut data, EquipmentSlot::Weapon2, "Dagger");
            let conditions = weapon_type_conditions(&dual, &data);
            assert_eq!(
                conditions.iter().any(|var| var == "DualWielding"),
                class == "Wand",
                "{class}"
            );
            assert!(
                conditions.iter().any(|var| var == "UsingDagger"),
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
                "UsingMeleeWeapon",
                "UsingOneHandedMelee",
                "UsingDagger",
                "DualWielding"
            ]
        );
        let swapped = equip(dual, &mut data, EquipmentSlot::Weapon2, "Bow");
        assert!(
            !weapon_type_conditions(&swapped, &data)
                .iter()
                .any(|var| var == "DualWielding")
        );
    }
    /// A new row needs no Rust weapon-name entry for matching or classification.
    #[test]
    fn new_weapon_row_preserves_flags_and_applies_wielding_condition() {
        use super::super::{DataOrchestratorOptions, calculate_with_data_session};
        use pobr_data::catalog::WeaponTypeDef;
        use pobr_gamedata::GameData;
        use std::sync::Arc;

        let game = GameData::new(pobr_gamedata::current_data_dir());
        let mut data = BuildData::load(&game).unwrap();
        let rapier: WeaponTypeDef =
            serde_json::from_str(r#"{"id":"Rapier","flag":"Rapier","one_hand":true,"melee":true}"#)
                .unwrap();
        Arc::make_mut(&mut data.constants)
            .weapon_types
            .0
            .push(rapier);
        let build = equip(Build::new(), &mut data, EquipmentSlot::Weapon1, "Rapier");
        let conditions = weapon_type_conditions(&build, &data);
        assert!(conditions.iter().any(|name| name == "UsingRapier"));
        assert!(conditions.iter().any(|name| name == "UsingOneHandedMelee"));
        let flags = ModFlags::WEAPON | ModFlags::WEAPON_1H | ModFlags::WEAPON_MELEE;
        assert_eq!(weapon_cfg_flags(&build, &data), flags);
        let contribution = pobr_core::skill_env::weapon_item_contribution(
            &build.items[&EquipmentSlot::Weapon1],
            &data,
        )
        .unwrap();
        assert_eq!(contribution.flags, flags);
        assert_eq!(
            damage_keywords(&build, &data, &["Attack".into()]),
            ["RapierDamage"]
        );
        assert!(damage_keywords(&build, &data, &["Spell".into()]).is_empty());
        assert!(super::super::diagnosis::condition_set_from_build(
            "UsingRapier",
            &data
        ));

        let doc = game.mod_parser_rules().unwrap().unwrap();
        data.parser_rules = Some(Arc::new(
            pobr_core::mod_parser::CompiledParserRules::compile_with_weapon_types(
                &doc,
                &[],
                &data.constants.weapon_types,
            )
            .unwrap(),
        ));
        let options = DataOrchestratorOptions::default();
        let baseline = calculate_with_data_session(&build, &data, &options).unwrap();
        assert!(baseline.config().condition("UsingRapier"));
        assert!(flags.is_subset_of(baseline.config().flags));
        assert!(
            baseline
                .unsupported_modifier_texts()
                .iter()
                .any(|text| text.contains("unsupported weapon flag Rapier"))
        );
        let boosted = calculate_with_data_session(
            &build,
            &data,
            &DataOrchestratorOptions {
                extra_modifier_texts: vec!["+37 to maximum Life while wielding a Rapier".into()],
                ..options
            },
        )
        .unwrap();
        assert_eq!(boosted.output().life, baseline.output().life + 37.0);
        assert!(
            !boosted
                .unsupported_modifier_texts()
                .iter()
                .any(|text| text.contains("+37"))
        );

        let mixed = equip(build, &mut data, EquipmentSlot::Weapon2, "Dagger");
        let unsupported = calculate_with_data_session(
            &mixed,
            &data,
            &DataOrchestratorOptions {
                extra_modifier_texts: vec!["37% increased Damage with Rapiers".into()],
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            unsupported
                .unsupported_modifier_texts()
                .iter()
                .any(|text| text.contains("Rapiers"))
        );
    }
}
