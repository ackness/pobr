//! What a skill's stats grant, written from the game's own data.
//!
//! Each rule is keyed by the game's stat id and states, in the engine's own
//! terms, what that stat does. Its provenance is the stat's description text
//! in the game and the skill's data under `data/<version>/base/`. A stat with
//! a rule here is not looked up in the stat map extracted from PoB2: this is
//! where that map is replaced, one stat at a time.

use pobr_data::catalog::SkillDamageStat;
use pobr_data::prelude::*;

use crate::{ModTag, Modifier};

/// The ailment each element builds up on its own.
const ELEMENTAL_AILMENTS: [(&str, &str); 3] = [
    ("Fire", "Ignite"),
    ("Cold", "Freeze"),
    ("Lightning", "Shock"),
];

/// The stat behind "Benefits from consuming Charges for Supported Skill have
/// {0}% chance to be doubled", on a skill or on a support that applies to it.
pub const CONSUMED_CHARGE_BENEFIT_DOUBLE_CHANCE: &str =
    "chance_%_to_double_effect_of_removing_charges";

/// What the rules for a buff skill's stats need to know about that skill.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BuffSkill {
    /// The chance (%) that the benefits of the charges the skill consumes are
    /// doubled: [`CONSUMED_CHARGE_BENEFIT_DOUBLE_CHANCE`] summed over the
    /// skill and its supports.
    pub consumed_charge_benefit_double_chance: f64,
}

impl BuffSkill {
    /// The average benefit of a consumed charge, relative to a charge whose
    /// benefit is never doubled.
    fn consumed_charge_benefit(self) -> f64 {
        1.0 + self.consumed_charge_benefit_double_chance.clamp(0.0, 100.0) / 100.0
    }
}

/// The stat behind "Each Cursed Ground area from Supported Skills spawns a
/// Witchtoad every {0} seconds" (Doedre's Undoing).
const WITCHTOAD_SPAWN_INTERVAL_MS: &str = "doedre_cursed_ground_toad_spawn_interval_ms";

/// The stats that say a skill inflicts Withered: on the skill itself, or on a
/// support for the skills it supports. "Minions have {0}% chance to inflict
/// Withered on Hit" is not among them: it is about the minions.
const INFLICTS_WITHERED: [&str; 9] = [
    // "{0}% chance to inflict Withered on Hit" and its fixed-duration forms.
    "withered_on_hit_chance_%",
    "withered_on_hit_for_2_seconds_%_chance",
    "withered_on_chaos_damage_hit_chance_%",
    "wither_on_hit_chance_rollovercapped",
    // Withering Touch: a chance per 100% of the target's Ailment Threshold
    // dealt as Chaos damage.
    "withered_on_hit_chance_%_for_every_100%_target_ailment_threshold_dealt_as_chaos_damage",
    // "Inflicts {0} Withered on Hit" (Chaos Bolt granted by a weapon).
    "apply_x_wither_on_hit",
    "archon_of_chayula_withered_on_hit_for_4_seconds_%_chance",
    // Withering Presence: "Every {0} seconds, inflicts Withered on enemies
    // in your Presence".
    "skill_withering_presence_frequency_ms",
    // Wither, His Foul Emergence: the duration of the Withered they inflict.
    "active_skill_withered_base_duration_ms",
];

/// Whether the stat `stat`, on a skill or on a support that applies to it,
/// says the skill inflicts Withered. A character that uses such a skill can
/// have Withered on the enemy, whichever skill is being calculated: the
/// build is searched for one (`skill_env::skill_inflicting_withered`), and
/// the stat grants nothing to the skill itself.
pub fn inflicts_withered(stat: &str) -> bool {
    INFLICTS_WITHERED.contains(&stat)
}

/// The modifiers the stat `stat` of a skill, or of a support that applies to
/// it, grants that skill at `value`, when this table defines the stat. `None`
/// leaves the stat to the extracted stat map.
pub fn skill_modifiers(stat: &str, value: f64) -> Option<Vec<Modifier>> {
    match stat {
        stat if inflicts_withered(stat) => Some(Vec::new()),
        // Efficiency: "Supported Skills have {0}% more Cost" / "less Cost",
        // of whatever the skill costs.
        "support_inspiration_cost_+%_final" => {
            Some(vec![Modifier::number("Cost", ModType::More, value)])
        }
        // "Supported Skills have {0}% increased Mana Cost Efficiency".
        "base_mana_cost_efficiency_+%" => Some(vec![Modifier::number(
            "ManaCostEfficiency",
            ModType::Inc,
            value,
        )]),
        // "Supported Skills have {0}% of Mana Costs Converted to Life Costs".
        "base_skill_cost_life_instead_of_mana_%" => Some(vec![Modifier::number(
            "HybridManaAndLifeCost_Life",
            ModType::Base,
            value,
        )]),
        // Says nothing about the curse the support applies to: the Witchtoads
        // use the skill the support grants (see `granted_skill_modifiers`).
        WITCHTOAD_SPAWN_INTERVAL_MS => Some(Vec::new()),
        // "Spells Cast by Totems have {0}% increased Cast Speed" and "Attacks
        // used by Totems have {0}% increased Attack Speed" (Urgent Totems):
        // for the skills a totem uses, as the same line on a passive is.
        "totem_skill_cast_speed_+%" => Some(vec![
            Modifier::number("CastSpeed", ModType::Inc, value)
                .with_flags(ModFlags::SPELL)
                .with_keyword_flags(KeywordFlags::TOTEM),
        ]),
        "totem_skill_attack_speed_+%" => Some(vec![
            Modifier::number("AttackSpeed", ModType::Inc, value)
                .with_keyword_flags(KeywordFlags::TOTEM),
        ]),
        // Rain of Arrows: "One arrow lands every {0} seconds", with
        // "Modifiers to Projectile speed apply to arrow landing frequency"
        // (`calc::rain`).
        "rain_of_arrows_delay_per_arrow" => Some(vec![Modifier::number(
            crate::calc::rain::LANDING_INTERVAL,
            ModType::Base,
            value,
        )]),
        // "Can Hit each enemy only once every {0} seconds", of what a rain or
        // a storm drops. Only a rain whose landings the engine knows reads it.
        "rain_hit_delay_ms" => Some(vec![Modifier::number(
            crate::calc::rain::HIT_INTERVAL,
            ModType::Base,
            value,
        )]),
        // Rigwald's Ferocity: increased Attack Speed and more Damage for
        // Supported Skills, each only while using one of the two weapon sets.
        "support_rigwald_attack_speed_+%_in_weapon_set_one" => Some(vec![
            Modifier::number("AttackSpeed", ModType::Inc, value)
                .with_tag(while_using_weapon_set(1)),
        ]),
        "support_rigwald_attack_speed_+%_in_weapon_set_two" => Some(vec![
            Modifier::number("AttackSpeed", ModType::Inc, value)
                .with_tag(while_using_weapon_set(2)),
        ]),
        "support_rigwald_damage_+%_final_in_weapon_set_one" => Some(vec![
            Modifier::number("Damage", ModType::More, value).with_tag(while_using_weapon_set(1)),
        ]),
        "support_rigwald_damage_+%_final_in_weapon_set_two" => Some(vec![
            Modifier::number("Damage", ModType::More, value).with_tag(while_using_weapon_set(2)),
        ]),
        _ => None,
    }
}

/// "while using Weapon Set `set`": the condition the build sets for the
/// weapon set in use, `WeaponSet1` or `WeaponSet2`.
fn while_using_weapon_set(set: u8) -> ModTag {
    ModTag::condition(format!("WeaponSet{set}"), false)
}

/// The modifiers the stat `stat` of a support grants, at `value`, to the
/// active skill that the same gem grants. `stats` is every stat of the
/// support. Empty when this table has nothing to say about the stat.
pub fn granted_skill_modifiers(stat: &str, value: f64, stats: &[SkillDamageStat]) -> Vec<Modifier> {
    let sibling = |id: &str| stats.iter().find(|ds| ds.stat == id).map(|ds| ds.value);
    match stat {
        // Doedre's Undoing: "Each Cursed Ground area from Supported Skills
        // spawns a Witchtoad every {0} seconds", with "Limit {0} areas of
        // Cursed Ground". The skill the gem grants, Dark Consequences, "is
        // Triggered when Witchtoads leap at enemies": it is used once per
        // Witchtoad, so at the rate they spawn, whatever the character's
        // cast speed. The enemy is taken to stand in every area the
        // character may have.
        WITCHTOAD_SPAWN_INTERVAL_MS if value > 0.0 => {
            let areas = sibling("maximum_curse_zones_allowed")
                .unwrap_or(1.0)
                .max(1.0);
            vec![Modifier::number(
                "SkillUseRate",
                ModType::Base,
                areas * 1000.0 / value,
            )]
        }
        _ => Vec::new(),
    }
}

/// The player modifiers the stat of the buff skill `skill` grants at `value`,
/// when this table defines the stat. `None` leaves the stat to the extracted
/// stat map.
pub fn player_buff_modifiers(stat: &str, value: f64, skill: BuffSkill) -> Option<Vec<Modifier>> {
    match stat {
        // Not a buff on the character (see `inflicts_withered`).
        stat if inflicts_withered(stat) => Some(Vec::new()),
        // Cast on Critical, from its quality on a character whose gem quality
        // grants the additional effect: "Buff grants {0}% increased Critical
        // Hit chance".
        "cast_on_crit_global_critical_hit_chance_granted_+%" => Some(vec![Modifier::number(
            "CriticalStrikeChance",
            ModType::Inc,
            value,
        )]),
        // Archmage: "Non-Channelling Spells cost an additional {0}% of your
        // maximum Mana". The stat is in hundredths of a percent. The cost is
        // added to the spell's own after its supports' cost multipliers,
        // which are multipliers of the gem's cost.
        "archmage_max_mana_permyriad_to_add_to_non_channelled_spell_mana_cost" => {
            let spell = SkillTypes::from_pob2_name("Spell")?;
            let channel = SkillTypes::from_pob2_name("Channel")?;
            Some(vec![
                Modifier::number("ManaCostNoMult", ModType::Base, value / 100.0)
                    .with_tag(ModTag::SkillTypes(spell))
                    .with_tag(ModTag::SkillTypesNeg(channel))
                    .with_tag(ModTag::PercentStat {
                        stat: "Mana".into(),
                        percent: Some(1.0),
                    }),
            ])
        }
        // Sorcery Ward (see `sorcery_ward_barrier`).
        SORCERY_WARD_BARRIER => Some(sorcery_ward_barrier(value)),
        // Not a buff on the character (see `granted_skill_modifiers`).
        WITCHTOAD_SPAWN_INTERVAL_MS => Some(Vec::new()),
        // Charge Regulation: "{0}% more Armour, Evasion and Energy Shield
        // while you have an Endurance Charge".
        "charge_mastery_armour_evasion_energy_shield_+%_final_with_endurance_charges" => Some(
            ["Armour", "Evasion", "EnergyShield"]
                .into_iter()
                .map(|defence| {
                    Modifier::number(defence, ModType::More, value)
                        .with_tag(while_holding("EnduranceCharge"))
                })
                .collect(),
        ),
        // "{0}% more Critical Hit Chance while you have a Power Charge".
        "charge_mastery_crit_chance_+%_final_with_power_charges" => Some(vec![
            Modifier::number("CriticalStrikeChance", ModType::More, value)
                .with_tag(while_holding("PowerCharge")),
        ]),
        // "{0}% increased Skill Speed while you have a Frenzy Charge".
        "charge_mastery_skill_speed_+%_with_frenzy_charges" => Some(
            SKILL_SPEED
                .into_iter()
                .map(|speed| {
                    Modifier::number(speed, ModType::Inc, value)
                        .with_tag(while_holding("FrenzyCharge"))
                })
                .collect(),
        ),
        // From quality: "Buff grants {0}% increased damage per Charge", of
        // any kind.
        "charge_regulation_damage_per_charge_granted_+%" => Some(
            ["PowerCharge", "FrenzyCharge", "EnduranceCharge"]
                .into_iter()
                .map(|charge| {
                    Modifier::number("Damage", ModType::Inc, value)
                        .with_tag(ModTag::multiplier(charge, 1.0, None))
                })
                .collect(),
        ),
        // "Consumes one of each Charge every {0} seconds": upkeep, with no
        // effect on what the character has while the buff is up.
        "consume_frenzy_power_and_endurance_charge_every_x_ms" => Some(Vec::new()),
        // Pinnacle of Power: "{0}% more Elemental Damage per Power Charge
        // Consumed" and "Buff causes all Elemental Damage to Contribute to
        // Flammability and Ignite Magnitudes, Shock chance, and Freeze
        // buildup". The skill consumes every Power Charge the character can
        // lose, so the count is the charges above the minimum. The damage is
        // by damage type: Fire, Cold and Lightning, whatever the skill.
        "elemental_power_elemental_damage_+%_final_per_power_charge" => Some(
            std::iter::once(
                Modifier::number(
                    "ElementalDamage",
                    ModType::More,
                    value * skill.consumed_charge_benefit(),
                )
                .with_tag(ModTag::multiplier("RemovablePowerCharge", 1.0, None)),
            )
            .chain(every_element_contributes_to_every_elemental_ailment())
            .collect(),
        ),
        // Charged Staff: "Buff grants {0} to {1} Added Quarterstaff Attack
        // Lightning damage per Power Charge Consumed, up to your maximum
        // Power Charges".
        "charged_staff_attack_minimum_added_lightning_damage_per_stack" => {
            Some(vec![charged_staff_added_lightning(
                "LightningDamageMin",
                value,
                skill,
            )])
        }
        "charged_staff_attack_maximum_added_lightning_damage_per_stack" => {
            Some(vec![charged_staff_added_lightning(
                "LightningDamageMax",
                value,
                skill,
            )])
        }
        // "Buff duration is {0} seconds per Power Charge Consumed": how long
        // the buff lasts, with no effect on what it grants while it is up.
        "charged_staff_buff_duration_per_stack_ms" => Some(Vec::new()),
        _ => None,
    }
}

/// One end of the Lightning damage Charged Staff adds to Quarterstaff
/// attacks, `value` per Power Charge consumed: `LightningDamageMin` or
/// `LightningDamageMax`, shaped as the line "Adds # to # Lightning Damage to
/// Quarterstaff Attacks" parses. The skill consumes every Power Charge the
/// character can lose, so the count is the charges above the minimum. The
/// text caps it at the maximum Power Charges, so a charge that only counts
/// as consumed ("Quarterstaff Skills that consume Power Charges count as
/// consuming an additional Power Charge") adds nothing at the maximum;
/// below it, that charge is not modelled.
fn charged_staff_added_lightning(name: &str, value: f64, skill: BuffSkill) -> Modifier {
    let per_charge = value * skill.consumed_charge_benefit();
    Modifier::number(name, ModType::Base, per_charge)
        .with_flags(ModFlags::STAFF)
        .with_tag(ModTag::DamageType(DamageType::Lightning))
        .with_tag(ModTag::condition("UsingQuarterstaff", false))
        .with_tag(ModTag::multiplier("RemovablePowerCharge", 1.0, None))
}

/// The stat behind Sorcery Ward's "Barrier can take Elemental Damage equal to
/// {0}% of your Armour and Evasion Rating".
const SORCERY_WARD_BARRIER: &str = "aegis_unique_shield_max_value_from_%_armour_evasion";

/// The condition "Grants Skill: Sorcery Ward" sets (`Condition:SorceryWard`,
/// bridged by `CalculationSession::bridge_player_conditions`). Sorcery Ward is
/// an Ascendancy skill: the character has it only while something grants it
/// (the Witchhunter's Obsessive Rituals), so a skill group that lists the gem
/// without the grant gives no Barrier.
pub const SORCERY_WARD: &str = "SorceryWard";

/// The condition "Sorcery Ward's Barrier can also take Physical and Chaos
/// Damage from Hits" sets (`Condition:CeremonialAblution`, bridged by
/// `CalculationSession::bridge_player_conditions`).
pub const CEREMONIAL_ABLUTION: &str = "CeremonialAblution";

/// Sorcery Ward: "Elemental Damage from Hits is taken from the Barrier before
/// your Life, Mana or Energy Shield. Barrier can take Elemental Damage equal
/// to {0}% of your Armour and Evasion Rating, up to a maximum of 32,000".
/// With "Sorcery Ward's Barrier can also take Physical and Chaos Damage from
/// Hits" the same Barrier takes every type of damage. The engine's aegis pools
/// take the strongest source, so a second Sorcery Ward does not add to it. The
/// maximum of 32,000 is not applied: no tag caps a percentage of a stat.
fn sorcery_ward_barrier(percent: f64) -> Vec<Modifier> {
    let of_armour_and_evasion = || ModTag::PercentStat {
        stat: "Armour|Evasion".into(),
        percent: Some(1.0),
    };
    vec![
        Modifier::number("ElementalAegisValue", ModType::Base, percent)
            .with_tag(ModTag::condition(SORCERY_WARD, false))
            .with_tag(ModTag::condition(CEREMONIAL_ABLUTION, true))
            .with_tag(of_armour_and_evasion()),
        Modifier::number("AegisValue", ModType::Base, percent)
            .with_tag(ModTag::condition(SORCERY_WARD, false))
            .with_tag(ModTag::condition(CEREMONIAL_ABLUTION, false))
            .with_tag(of_armour_and_evasion()),
    ]
}

/// What "Skill Speed" is: the speed of attacks and casts, of Warcries and of
/// placing Totems.
pub(crate) const SKILL_SPEED: [&str; 3] = ["SkillSpeed", "WarcrySpeed", "TotemPlacementSpeed"];

/// "while you have a `charge`": at least one charge of that kind
/// (`PowerCharge`, `FrenzyCharge` or `EnduranceCharge`).
fn while_holding(charge: &str) -> ModTag {
    ModTag::MultiplierThreshold {
        var: charge.into(),
        threshold: 1.0,
        upper: false,
    }
}

/// The flags that let each element contribute to the ailments of the other
/// two, e.g. `ColdCanIgnite`.
fn every_element_contributes_to_every_elemental_ailment() -> impl Iterator<Item = Modifier> {
    ELEMENTAL_AILMENTS.iter().flat_map(|(_, ailment)| {
        ELEMENTAL_AILMENTS
            .iter()
            .filter(move |(_, own)| own != ailment)
            .map(move |(element, _)| Modifier::flag(format!("{element}Can{ailment}").as_str()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CalcConfig, ModDb};

    const PER_POWER_CHARGE: &str = "elemental_power_elemental_damage_+%_final_per_power_charge";

    /// The more Elemental Damage Pinnacle of Power grants `skill` after
    /// consuming four Power Charges.
    fn more_elemental_damage(skill: BuffSkill) -> ModDb {
        let mut db = ModDb::new();
        db.add_list(player_buff_modifiers(PER_POWER_CHARGE, 15.0, skill).expect("a rule"));
        db
    }

    fn four_consumed() -> CalcConfig {
        CalcConfig::new().with_multiplier("RemovablePowerCharge", 4.0)
    }

    #[test]
    fn pinnacle_of_power_is_more_elemental_damage_per_consumed_power_charge() {
        let db = more_elemental_damage(BuffSkill::default());
        let elemental = [ModName::from("ElementalDamage")];
        assert!((db.more(&four_consumed(), &elemental) - 1.6).abs() < 1e-12);
        // No charge to consume, no damage. The buff is by damage type, so
        // nothing lands on damage in general.
        assert_eq!(db.more(&CalcConfig::new(), &elemental), 1.0);
        assert_eq!(db.more(&four_consumed(), &[ModName::from("Damage")]), 1.0);
    }

    /// Charge Regulation grants each of its three effects only while the
    /// character holds a charge of the matching kind.
    #[test]
    fn charge_regulation_needs_a_charge_of_the_matching_kind() {
        let buff = |stat: &str, value: f64| {
            let mut db = ModDb::new();
            db.add_list(player_buff_modifiers(stat, value, BuffSkill::default()).expect("a rule"));
            db
        };
        let holding = |charge: &str| CalcConfig::new().with_multiplier(charge, 1.0);
        let none = CalcConfig::new();

        let defences = buff(
            "charge_mastery_armour_evasion_energy_shield_+%_final_with_endurance_charges",
            19.0,
        );
        for name in ["Armour", "Evasion", "EnergyShield"] {
            let names = [ModName::from(name)];
            assert!((defences.more(&holding("EnduranceCharge"), &names) - 1.19).abs() < 1e-12);
            assert_eq!(
                defences.more(&holding("PowerCharge"), &names),
                1.0,
                "{name}"
            );
            assert_eq!(defences.more(&none, &names), 1.0, "{name}");
        }

        let crit = buff(
            "charge_mastery_crit_chance_+%_final_with_power_charges",
            25.0,
        );
        let names = [ModName::from("CriticalStrikeChance")];
        assert!((crit.more(&holding("PowerCharge"), &names) - 1.25).abs() < 1e-12);
        assert_eq!(crit.more(&holding("FrenzyCharge"), &names), 1.0);

        let speed = buff("charge_mastery_skill_speed_+%_with_frenzy_charges", 25.0);
        for name in SKILL_SPEED {
            let names = [ModName::from(name)];
            assert_eq!(
                speed.sum(ModType::Inc, &holding("FrenzyCharge"), &names),
                25.0
            );
            assert_eq!(speed.sum(ModType::Inc, &none, &names), 0.0, "{name}");
        }
    }

    /// Its quality counts every charge the character holds, of any kind.
    #[test]
    fn charge_regulation_quality_is_damage_per_charge_of_any_kind() {
        let mut db = ModDb::new();
        db.add_list(
            player_buff_modifiers(
                "charge_regulation_damage_per_charge_granted_+%",
                2.0,
                BuffSkill::default(),
            )
            .expect("a rule"),
        );
        let cfg = CalcConfig::new()
            .with_multiplier("PowerCharge", 3.0)
            .with_multiplier("EnduranceCharge", 4.0);
        assert_eq!(db.sum(ModType::Inc, &cfg, &[ModName::from("Damage")]), 14.0);
    }

    /// Heightened Charges: a 20% chance to double the benefit is, on
    /// average, 20% more of it. Past 100% there is nothing left to double.
    #[test]
    fn a_chance_to_double_the_benefit_of_consumed_charges_raises_it_on_average() {
        let more = |chance: f64| {
            more_elemental_damage(BuffSkill {
                consumed_charge_benefit_double_chance: chance,
            })
            .more(&four_consumed(), &[ModName::from("ElementalDamage")])
        };
        assert!((more(20.0) - 1.72).abs() < 1e-12);
        assert!((more(100.0) - 2.2).abs() < 1e-12);
        assert!((more(250.0) - 2.2).abs() < 1e-12);
    }

    /// Charged Staff at level 23, 2 to 30 per stack, after consuming six
    /// Power Charges: 12 to 180 Lightning damage for Quarterstaff attacks.
    #[test]
    fn charged_staff_adds_lightning_to_quarterstaff_attacks_per_consumed_power_charge() {
        let mut db = ModDb::new();
        for (stat, value) in [
            (
                "charged_staff_attack_minimum_added_lightning_damage_per_stack",
                2.0,
            ),
            (
                "charged_staff_attack_maximum_added_lightning_damage_per_stack",
                30.0,
            ),
        ] {
            db.add_list(player_buff_modifiers(stat, value, BuffSkill::default()).expect("a rule"));
        }
        let added = |cfg: &CalcConfig| {
            (
                db.sum(ModType::Base, cfg, &[ModName::from("LightningDamageMin")]),
                db.sum(ModType::Base, cfg, &[ModName::from("LightningDamageMax")]),
            )
        };
        let quarterstaff_attack = |consumed: f64| {
            CalcConfig::new()
                .with_flags(ModFlags::ATTACK | ModFlags::HIT | ModFlags::STAFF)
                .with_damage_type(DamageType::Lightning)
                .with_condition("UsingQuarterstaff", true)
                .with_multiplier("RemovablePowerCharge", consumed)
        };
        assert_eq!(added(&quarterstaff_attack(6.0)), (12.0, 180.0));
        // No charge consumed, nothing added.
        assert_eq!(added(&quarterstaff_attack(0.0)), (0.0, 0.0));
        // Only Quarterstaff attacks gain it.
        let other_weapon = quarterstaff_attack(6.0).with_condition("UsingQuarterstaff", false);
        assert_eq!(added(&other_weapon), (0.0, 0.0));
        // The buff's duration grants nothing while the buff is up.
        let duration = player_buff_modifiers(
            "charged_staff_buff_duration_per_stack_ms",
            6000.0,
            BuffSkill::default(),
        );
        assert_eq!(duration, Some(Vec::new()));
    }

    #[test]
    fn pinnacle_of_power_lets_each_element_contribute_to_the_other_ailments() {
        let mods =
            player_buff_modifiers(PER_POWER_CHARGE, 15.0, BuffSkill::default()).expect("a rule");
        let mut flags: Vec<&str> = mods
            .iter()
            .filter(|modifier| modifier.mod_type == ModType::Flag)
            .map(|modifier| modifier.name.as_str())
            .collect();
        flags.sort_unstable();
        assert_eq!(
            flags,
            [
                "ColdCanIgnite",
                "ColdCanShock",
                "FireCanFreeze",
                "FireCanShock",
                "LightningCanFreeze",
                "LightningCanIgnite",
            ]
        );
    }

    #[test]
    fn a_stat_without_a_rule_is_left_to_the_stat_map() {
        let skill = BuffSkill::default();
        assert!(player_buff_modifiers("base_skill_effect_duration", 5000.0, skill).is_none());
        assert!(skill_modifiers("base_skill_effect_duration", 5000.0).is_none());
        assert!(granted_skill_modifiers("base_skill_effect_duration", 5000.0, &[]).is_empty());
    }

    /// The stats of supports that change what the skill they apply to costs.
    #[test]
    fn support_cost_stats_become_the_modifiers_the_cost_reads() {
        let only = |stat: &str, value: f64| {
            let mut mods = skill_modifiers(stat, value).expect("a rule");
            assert_eq!(mods.len(), 1, "{stat}");
            let modifier = mods.remove(0);
            (
                modifier.name.to_string(),
                modifier.mod_type,
                modifier.value.as_number(),
            )
        };
        assert_eq!(
            only("support_inspiration_cost_+%_final", -40.0),
            ("Cost".to_string(), ModType::More, Some(-40.0))
        );
        assert_eq!(
            only("base_mana_cost_efficiency_+%", 25.0),
            ("ManaCostEfficiency".to_string(), ModType::Inc, Some(25.0))
        );
        assert_eq!(
            only("base_skill_cost_life_instead_of_mana_%", 100.0),
            (
                "HybridManaAndLifeCost_Life".to_string(),
                ModType::Base,
                Some(100.0)
            )
        );
    }

    #[test]
    fn cast_on_critical_can_grant_increased_critical_hit_chance() {
        let stat = "cast_on_crit_global_critical_hit_chance_granted_+%";
        let mut db = ModDb::new();
        db.add_list(player_buff_modifiers(stat, 23.0, BuffSkill::default()).expect("a rule"));
        let names = [ModName::from("CriticalStrikeChance")];
        assert_eq!(db.sum(ModType::Inc, &CalcConfig::new(), &names), 23.0);
    }

    /// Archmage at 4.10% of 4000 maximum Mana: 164 more Mana for a spell,
    /// nothing for an attack or a channelled spell.
    #[test]
    fn archmage_adds_a_share_of_maximum_mana_to_the_cost_of_non_channelling_spells() {
        let stat = "archmage_max_mana_permyriad_to_add_to_non_channelled_spell_mana_cost";
        let mut db = ModDb::new();
        db.add_list(player_buff_modifiers(stat, 410.0, BuffSkill::default()).expect("a rule"));
        let added = |types: &[&str]| {
            let mut cfg = CalcConfig::new();
            cfg.stats.insert("Mana".into(), 4000.0);
            for name in types {
                cfg.skill_types |= SkillTypes::from_pob2_name(name).unwrap();
            }
            db.sum(ModType::Base, &cfg, &[ModName::from("ManaCostNoMult")])
        };
        assert_eq!(added(&["Spell"]), 164.0);
        assert_eq!(added(&["Attack"]), 0.0);
        assert_eq!(added(&["Spell", "Channel"]), 0.0);
    }

    /// Sorcery Ward at 31% of 18000 Armour and Evasion Rating: a 5580
    /// Barrier for Elemental Damage, or for every type of damage once
    /// "Sorcery Ward's Barrier can also take Physical and Chaos Damage from
    /// Hits".
    #[test]
    fn sorcery_ward_barrier_is_a_share_of_armour_and_evasion() {
        let mut db = ModDb::new();
        db.add_list(
            player_buff_modifiers(SORCERY_WARD_BARRIER, 31.0, BuffSkill::default())
                .expect("a rule"),
        );
        // (Barrier for Elemental Damage, Barrier for every type of damage).
        let barrier = |cfg: &CalcConfig| {
            let strongest = |name: &str| db.max_of(ModType::Base, cfg, &[ModName::from(name)]);
            (strongest("ElementalAegisValue"), strongest("AegisValue"))
        };
        let granted = || CalcConfig::new().with_condition(SORCERY_WARD, true);
        let armour = granted().with_stat("Armour", 18000.0);
        assert_eq!(barrier(&armour), (5580.0, 0.0));
        let split = granted()
            .with_stat("Armour", 9000.0)
            .with_stat("Evasion", 9000.0);
        assert_eq!(barrier(&split), (5580.0, 0.0));
        let ablution = armour.with_condition(CEREMONIAL_ABLUTION, true);
        assert_eq!(barrier(&ablution), (0.0, 5580.0));
        // No defences, no Barrier.
        assert_eq!(barrier(&granted()), (0.0, 0.0));
        // A skill group that lists the gem without the grant: no Barrier.
        let listed_only = CalcConfig::new().with_stat("Armour", 18000.0);
        assert_eq!(barrier(&listed_only), (0.0, 0.0));
    }

    fn stat(id: &str, value: f64) -> SkillDamageStat {
        SkillDamageStat {
            stat: id.into(),
            value,
        }
    }

    /// The use rate `stats` of Doedre's Undoing give the skill its gem grants.
    fn witchtoad_use_rate(stats: &[SkillDamageStat]) -> f64 {
        let mut db = ModDb::new();
        for ds in stats {
            db.add_list(granted_skill_modifiers(&ds.stat, ds.value, stats));
        }
        db.sum(
            ModType::Base,
            &CalcConfig::new(),
            &[ModName::from("SkillUseRate")],
        )
    }

    /// One Witchtoad every 0.25 seconds in each of two areas: eight a second.
    #[test]
    fn witchtoads_use_the_skill_their_gem_grants_at_the_rate_they_spawn() {
        let two_areas = [
            stat("maximum_curse_zones_allowed", 2.0),
            stat("max_toads_per_doedre_cursed_ground", 4.0),
            stat(WITCHTOAD_SPAWN_INTERVAL_MS, 250.0),
        ];
        assert_eq!(witchtoad_use_rate(&two_areas), 8.0);
        assert_eq!(
            witchtoad_use_rate(&[stat(WITCHTOAD_SPAWN_INTERVAL_MS, 500.0)]),
            2.0
        );
    }

    /// The interval says nothing about the curse the support applies to, and
    /// the Witchtoads are not a buff on the character.
    #[test]
    fn the_witchtoad_interval_grants_the_supported_curse_and_the_character_nothing() {
        assert_eq!(
            skill_modifiers(WITCHTOAD_SPAWN_INTERVAL_MS, 250.0),
            Some(Vec::new())
        );
        let buff = player_buff_modifiers(WITCHTOAD_SPAWN_INTERVAL_MS, 250.0, BuffSkill::default());
        assert_eq!(buff, Some(Vec::new()));
    }
}
