//! skill/minions — summoning-gem recognition and minion wiring.
//!
//! Thin wrapper: the semantics moved to `pobr_core::skill_env::spawn_minions` (the
//! engine-semantics layer); this assembles the GemProperty level bonuses from
//! `Build`/`BuildData` and delegates. What the character grants its minions
//! is read from its modifiers, which every source has written by then.

use pobr_core::calc::CalculationSession;

use super::super::skill::resolve::gem_property_bonuses;
use crate::build::Build;
use crate::build_data::BuildData;

pub(crate) fn spawn_minions(session: &mut CalculationSession, build: &Build, data: &BuildData) {
    let bonuses = gem_property_bonuses(build, data);
    pobr_core::skill_env::spawn_minions(session, build, data, &bonuses, build);
    // "... equal to ... your main hand Weapon's damage" includes its local
    // elemental and chaos damage, as well as physical damage.
    if let Some(item) =
        pobr_core::skill_env::EquipmentView::item(build, pobr_data::item::EquipmentSlot::Weapon1)
        && let Some(weapon) = pobr_core::skill_env::weapon_item_contribution(item, data)
    {
        use pobr_data::constants::DamageType;
        let mut damage = vec![(DamageType::Physical, weapon.phys_min, weapon.phys_max)];
        for kind in [
            DamageType::Fire,
            DamageType::Cold,
            DamageType::Lightning,
            DamageType::Chaos,
        ] {
            let suffix = format!("{kind:?} damage").to_lowercase();
            let (min, max) = pobr_core::skill_env::weapon_mod_texts(item)
                .filter_map(|text| {
                    pobr_core::skill_env::parse_adds_with_suffix(
                        &pobr_core::skill_env::clean_item_text(text),
                        &suffix,
                    )
                })
                .fold((0.0, 0.0), |(min, max), (lo, hi)| (min + lo, max + hi));
            if min != 0.0 || max != 0.0 {
                damage.push((kind, min, max));
            }
        }
        session.set_main_hand_weapon_damage_components(damage);
    }
}
