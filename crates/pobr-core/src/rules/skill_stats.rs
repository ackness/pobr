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

/// The player modifiers a buff skill's stat grants at `value`, when this
/// table defines the stat. `None` leaves the stat to the extracted stat map.
pub fn player_buff_modifiers(stat: &str, value: f64) -> Option<Vec<Modifier>> {
    match stat {
        // Pinnacle of Power: "{0}% more Elemental Damage per Power Charge
        // Consumed" and "Buff causes all Elemental Damage to Contribute to
        // Flammability and Ignite Magnitudes, Shock chance, and Freeze
        // buildup". The skill consumes every Power Charge the character can
        // lose, so the count is the charges above the minimum. The damage is
        // by damage type: Fire, Cold and Lightning, whatever the skill.
        "elemental_power_elemental_damage_+%_final_per_power_charge" => Some(
            std::iter::once(
                Modifier::number("ElementalDamage", ModType::More, value)
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

    #[test]
    fn pinnacle_of_power_is_more_elemental_damage_per_consumed_power_charge() {
        let mods = player_buff_modifiers(
            "elemental_power_elemental_damage_+%_final_per_power_charge",
            15.0,
        )
        .expect("a rule");
        let mut db = ModDb::new();
        db.add_list(mods);

        let elemental = [ModName::from("ElementalDamage")];
        let four_consumed = CalcConfig::new().with_multiplier("RemovablePowerCharge", 4.0);
        assert!((db.more(&four_consumed, &elemental) - 1.6).abs() < 1e-12);
        // No charge to consume, no damage. The buff is by damage type, so
        // nothing lands on damage in general.
        assert_eq!(db.more(&CalcConfig::new(), &elemental), 1.0);
        assert_eq!(db.more(&four_consumed, &[ModName::from("Damage")]), 1.0);
    }

    #[test]
    fn pinnacle_of_power_lets_each_element_contribute_to_the_other_ailments() {
        let mods = player_buff_modifiers(
            "elemental_power_elemental_damage_+%_final_per_power_charge",
            15.0,
        )
        .expect("a rule");
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
        assert!(player_buff_modifiers("base_skill_effect_duration", 5000.0).is_none());
    }
}
