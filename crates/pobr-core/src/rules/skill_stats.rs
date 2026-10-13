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

/// The modifiers the stat `stat` of a skill, or of a support that applies to
/// it, grants that skill at `value`, when this table defines the stat. `None`
/// leaves the stat to the extracted stat map.
pub fn skill_modifiers(stat: &str, value: f64) -> Option<Vec<Modifier>> {
    match stat {
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
        _ => None,
    }
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
        // Not a buff on the character (see `granted_skill_modifiers`).
        WITCHTOAD_SPAWN_INTERVAL_MS => Some(Vec::new()),
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
        _ => None,
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
