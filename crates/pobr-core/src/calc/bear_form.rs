//! Bear form: the shape a character takes to use a Bear skill.
//!
//! The rules, as the game defines the form ("While in Bear form, you gain"):
//!
//! - "+10 to Armour per level" ([`BEAR_FORM_ARMOUR_PER_LEVEL`]): flat Armour,
//!   which the character's increased and more Armour scale like any other.
//!   PoB2 gives 10 more (10 + 10 per level), which the game's text does not
//!   state, so a build's reference Armour comes out slightly higher.
//! - "30% of Armour also applies to Elemental Damage Types"
//!   ([`BEAR_FORM_ARMOUR_ALSO_APPLIES_TO_ELEMENTAL`]).
//! - "an Endurance Charge for every 15 Rage you spend" is not modelled: the
//!   calculation does not follow Rage being spent.
//!
//! A skill of the game types `Shapeshift` and `Bear` shapeshifts its user into
//! the form, so the character is in it while that skill is the one in use: the
//! build states the `BearForm` condition from the main skill's types.

use pobr_data::constants::{
    BEAR_FORM_ARMOUR_ALSO_APPLIES_TO_ELEMENTAL, BEAR_FORM_ARMOUR_PER_LEVEL,
};
use pobr_data::prelude::*;

use crate::{ModTag, Modifier};

use super::Env;

/// The condition of a character in Bear form.
pub const BEAR_FORM: &str = "BearForm";

/// Source of the injected modifiers; also the guard against applying them twice.
const BEAR_FORM_SOURCE: &str = "BearForm";

/// Grants a character in Bear form what the form gives. Does nothing when it
/// is not in the form, or already has the form's modifiers.
pub fn apply_bear_form(env: &mut Env) {
    if !env.cfg.condition(BEAR_FORM)
        || env
            .player
            .mod_db
            .named("ArmourAppliesToElementalDamageTaken")
            .any(|modifier| modifier.source.as_deref() == Some(BEAR_FORM_SOURCE))
    {
        return;
    }
    let origin = ModifierSource::new(SourceId::new(SourceKind::Buff, "buff.BearForm"))
        .with_raw_text("Bear form");
    let modifiers = [
        Modifier::number("Armour", ModType::Base, BEAR_FORM_ARMOUR_PER_LEVEL)
            .with_tag(ModTag::multiplier("Level", 1.0, None)),
        Modifier::number(
            "ArmourAppliesToElementalDamageTaken",
            ModType::Base,
            BEAR_FORM_ARMOUR_ALSO_APPLIES_TO_ELEMENTAL,
        ),
    ];
    env.player.mod_db.add_list(modifiers.map(|modifier| {
        modifier
            .with_source(BEAR_FORM_SOURCE)
            .with_origin(origin.clone())
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calc::{Actor, ActorBaseStats};

    /// A level 98 character, in Bear form or not.
    fn run(in_form: bool) -> Env {
        let mut env = Env::new(Actor::new(1, ActorBaseStats::default()));
        env.cfg = env
            .cfg
            .with_multiplier("Level", 98.0)
            .with_condition(BEAR_FORM, in_form);
        apply_bear_form(&mut env);
        env
    }

    fn base(env: &Env, name: &str) -> f64 {
        env.player
            .mod_db
            .sum(ModType::Base, &env.cfg, &[ModName::from(name)])
    }

    #[test]
    fn bear_form_gives_armour_per_level_and_armour_against_the_elements() {
        let env = run(true);
        assert_eq!(base(&env, "Armour"), 980.0);
        assert_eq!(base(&env, "ArmourAppliesToElementalDamageTaken"), 30.0);
    }

    #[test]
    fn a_character_not_in_bear_form_gains_nothing() {
        let env = run(false);
        assert_eq!(base(&env, "Armour"), 0.0);
        assert_eq!(base(&env, "ArmourAppliesToElementalDamageTaken"), 0.0);
    }

    #[test]
    fn bear_form_applies_once() {
        let mut env = run(true);
        apply_bear_form(&mut env);
        assert_eq!(base(&env, "Armour"), 980.0);
        assert_eq!(base(&env, "ArmourAppliesToElementalDamageTaken"), 30.0);
    }
}
