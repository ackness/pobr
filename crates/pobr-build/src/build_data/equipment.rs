use super::BuildData;
use pobr_data::catalog::{ArmourBaseStats, BaseItemDef, WeaponBaseStats};

impl BuildData {
    /// Looks up weapon base stats by base name (`Item.base` → `WeaponBaseStats`);
    /// returns `None` for a non-weapon / unknown base.
    pub fn weapon_base(&self, base_name: &str) -> Option<&WeaponBaseStats> {
        self.base_items
            .get(base_name)
            .and_then(|b| b.weapon.as_ref())
    }

    /// Looks up armour base stats by base name (`Item.base` → `ArmourBaseStats`);
    /// returns `None` for a non-armour / unknown base.
    pub fn armour_base(&self, base_name: &str) -> Option<&ArmourBaseStats> {
        self.base_items
            .get(base_name)
            .and_then(|b| b.armour.as_ref())
    }
}

impl pobr_core::skill_env::ArmourBaseLookup for BuildData {
    fn armour_base(&self, base_name: &str) -> Option<&ArmourBaseStats> {
        self.armour_base(base_name)
    }
}

impl pobr_core::skill_env::BaseItemLookup for BuildData {
    fn base_item(&self, base_name: &str) -> Option<&BaseItemDef> {
        self.base_items.get(base_name)
    }
}

impl pobr_core::skill_env::WeaponTypeLookup for BuildData {
    fn weapon_type_info(&self, item_class: &str) -> Option<&pobr_data::catalog::WeaponTypeDef> {
        let key = match item_class {
            "Warstaff" => "Staff",
            "Staff" => return None,
            "FishingRod" => "Fishing Rod",
            other => other,
        };
        self.constants.weapon_types.get(key)
    }
}

impl pobr_core::skill_env::UnarmedDataLookup for BuildData {
    fn unarmed_for_class(&self, class_name: &str) -> Option<&pobr_data::catalog::UnarmedWeaponDef> {
        self.constants.unarmed_data.for_class(class_name)
    }
}

impl pobr_core::skill_env::WeaponBaseLookup for BuildData {
    fn weapon_base(&self, base_name: &str) -> Option<&WeaponBaseStats> {
        self.weapon_base(base_name)
    }
}
