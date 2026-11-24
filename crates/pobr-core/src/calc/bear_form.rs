//! Animal shapeshift forms: Bear, Wolf, and Wyvern.
//!
//! A skill of the game types `Shapeshift` and `Bear`/`Wolf`/`Wyvern`
//! shapeshifts its user into that form while active:
//!
//! - **Bear form**: +10 Armour per level ([`BEAR_FORM_ARMOUR_PER_LEVEL`]) and
//!   30% of Armour also applies to Elemental Damage Types
//!   ([`BEAR_FORM_ARMOUR_ALSO_APPLIES_TO_ELEMENTAL`]).
//! - **Wolf form**: 30% increased Movement Speed
//!   ([`WOLF_FORM_MOVEMENT_SPEED_INC`]).
//! - **Wyvern form**: 50% faster start of Energy Shield Recharge, 50% increased
//!   Stun Threshold, and 50% increased Ailment Threshold
//!   ([`WYVERN_FORM_ES_RECHARGE_FASTER_INC`], [`WYVERN_FORM_STUN_THRESHOLD_INC`],
//!   [`WYVERN_FORM_AILMENT_THRESHOLD_INC`]).

use pobr_data::constants::{
    BEAR_FORM_ARMOUR_ALSO_APPLIES_TO_ELEMENTAL, BEAR_FORM_ARMOUR_PER_LEVEL,
    WOLF_FORM_MOVEMENT_SPEED_INC, WYVERN_FORM_AILMENT_THRESHOLD_INC,
    WYVERN_FORM_ES_RECHARGE_FASTER_INC, WYVERN_FORM_STUN_THRESHOLD_INC,
};
use pobr_data::prelude::*;

use crate::{ModTag, Modifier};

use super::Env;

/// The condition of a character in Bear form.
pub const BEAR_FORM: &str = "BearForm";
/// The condition of a character in Wolf form.
pub const WOLF_FORM: &str = "WolfForm";
/// The condition of a character in Wyvern form.
pub const WYVERN_FORM: &str = "WyvernForm";

const BEAR_FORM_SOURCE: &str = "BearForm";
const WOLF_FORM_SOURCE: &str = "WolfForm";
const WYVERN_FORM_SOURCE: &str = "WyvernForm";

fn inject_form(
    env: &mut Env,
    condition: &str,
    guard_stat: &str,
    source_id: &str,
    buff_id: &str,
    raw_text: &str,
    modifiers: impl IntoIterator<Item = Modifier>,
) {
    if !env.cfg.condition(condition)
        || env
            .player
            .mod_db
            .named(guard_stat)
            .any(|modifier| modifier.source.as_deref() == Some(source_id))
    {
        return;
    }
    let origin =
        ModifierSource::new(SourceId::new(SourceKind::Buff, buff_id)).with_raw_text(raw_text);
    env.player.mod_db.add_list(
        modifiers
            .into_iter()
            .map(|modifier| modifier.with_source(source_id).with_origin(origin.clone())),
    );
}

/// Grants a character in Bear form what the form gives. Does nothing when it
/// is not in the form, or already has the form's modifiers.
pub fn apply_bear_form(env: &mut Env) {
    inject_form(
        env,
        BEAR_FORM,
        "ArmourAppliesToElementalDamageTaken",
        BEAR_FORM_SOURCE,
        "buff.BearForm",
        "Bear form",
        [
            Modifier::number("Armour", ModType::Base, BEAR_FORM_ARMOUR_PER_LEVEL)
                .with_tag(ModTag::multiplier("Level", 1.0, None)),
            Modifier::number(
                "ArmourAppliesToElementalDamageTaken",
                ModType::Base,
                BEAR_FORM_ARMOUR_ALSO_APPLIES_TO_ELEMENTAL,
            ),
        ],
    );
}

/// Grants a character in Wolf form its inherent movement speed bonus.
pub fn apply_wolf_form(env: &mut Env) {
    inject_form(
        env,
        WOLF_FORM,
        "MovementSpeed",
        WOLF_FORM_SOURCE,
        "buff.WolfForm",
        "Wolf form",
        [Modifier::number(
            "MovementSpeed",
            ModType::Inc,
            WOLF_FORM_MOVEMENT_SPEED_INC,
        )],
    );
}

/// Grants a character in Wyvern form its inherent ES recharge, stun, and
/// ailment threshold bonuses.
pub fn apply_wyvern_form(env: &mut Env) {
    inject_form(
        env,
        WYVERN_FORM,
        "EnergyShieldRechargeFaster",
        WYVERN_FORM_SOURCE,
        "buff.WyvernForm",
        "Wyvern form",
        [
            Modifier::number(
                "EnergyShieldRechargeFaster",
                ModType::Inc,
                WYVERN_FORM_ES_RECHARGE_FASTER_INC,
            ),
            Modifier::number(
                "StunThreshold",
                ModType::Inc,
                WYVERN_FORM_STUN_THRESHOLD_INC,
            ),
            Modifier::number(
                "AilmentThreshold",
                ModType::Inc,
                WYVERN_FORM_AILMENT_THRESHOLD_INC,
            ),
        ],
    );
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
            .sum_by_name(ModType::Base, &env.cfg, &[name])
    }

    fn inc(env: &Env, name: &str) -> f64 {
        env.player
            .mod_db
            .sum_by_name(ModType::Inc, &env.cfg, &[name])
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

    #[test]
    fn wolf_form_gives_movement_speed_once() {
        let mut inactive = Env::new(Actor::new(1, ActorBaseStats::default()));
        inactive.cfg = inactive.cfg.with_condition(WOLF_FORM, false);
        apply_wolf_form(&mut inactive);
        assert_eq!(inc(&inactive, "MovementSpeed"), 0.0);

        let mut active = Env::new(Actor::new(1, ActorBaseStats::default()));
        active.cfg = active.cfg.with_condition(WOLF_FORM, true);
        apply_wolf_form(&mut active);
        apply_wolf_form(&mut active);
        assert_eq!(inc(&active, "MovementSpeed"), 30.0);
    }

    #[test]
    fn wyvern_form_gives_es_recharge_stun_and_ailment_threshold_once() {
        let mut inactive = Env::new(Actor::new(1, ActorBaseStats::default()));
        inactive.cfg = inactive.cfg.with_condition(WYVERN_FORM, false);
        apply_wyvern_form(&mut inactive);
        assert_eq!(inc(&inactive, "EnergyShieldRechargeFaster"), 0.0);
        assert_eq!(inc(&inactive, "StunThreshold"), 0.0);
        assert_eq!(inc(&inactive, "AilmentThreshold"), 0.0);

        let mut active = Env::new(Actor::new(1, ActorBaseStats::default()));
        active.cfg = active.cfg.with_condition(WYVERN_FORM, true);
        apply_wyvern_form(&mut active);
        apply_wyvern_form(&mut active);
        assert_eq!(inc(&active, "EnergyShieldRechargeFaster"), 50.0);
        assert_eq!(inc(&active, "StunThreshold"), 50.0);
        assert_eq!(inc(&active, "AilmentThreshold"), 50.0);
    }
}
