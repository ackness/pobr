//! What a skill's stats grant, written from the game's own data.
//!
//! Each rule is keyed by the game's stat id and states, in the engine's own
//! terms, what that stat does. Its provenance is the stat's description text
//! in the game and the skill's data under `data/<version>/base/`. A stat with
//! a rule here is not looked up in the stat map extracted from PoB2: this is
//! where that map is replaced, one stat at a time.

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

/// The player modifiers the stat of the buff skill `skill` grants at `value`,
/// when this table defines the stat. `None` leaves the stat to the extracted
/// stat map.
pub fn player_buff_modifiers(stat: &str, value: f64, skill: BuffSkill) -> Option<Vec<Modifier>> {
    match stat {
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
    }
}
