use super::BuildData;
use pobr_data::catalog::{ArmourBaseStats, BaseItemDef, WeaponBaseStats};

impl BuildData {
    /// Resolve an imported item name without rewriting it. PoB's normal/magic
    /// item blocks can retain a quality prefix or affixes instead of a separate
    /// base line: `Exceptional Ornate Plate`, `Squire's Structured Hammer`.
    /// Exact catalog names win. Otherwise the base must end the name before its
    /// optional ` of ...` suffix, on a word boundary; a base mentioned only in
    /// the suffix must never identify the item. Bases can themselves contain
    /// ` of `, so match the longest complete catalog name before discarding a
    /// suffix. This also distinguishes `Emerald Ring` from `Emerald` or `Ring`.
    pub fn base_item(&self, name: &str) -> Option<&BaseItemDef> {
        if let Some(base) = self.base_items.get(name) {
            return Some(base);
        }
        if name.ends_with(" of ") {
            return None;
        }
        let first_of = name.find(" of ").unwrap_or(name.len());
        std::iter::once(0)
            .chain(name.match_indices(' ').map(|(index, _)| index + 1))
            .take_while(|start| *start < first_of)
            .flat_map(|start| {
                let tail = &name[start..];
                std::iter::once(tail).chain(
                    tail.match_indices(" of ")
                        .filter(|(index, _)| !tail[index + 4..].trim().is_empty())
                        .map(|(index, _)| &tail[..index]),
                )
            })
            .filter_map(|candidate| self.base_items.get(candidate))
            .max_by_key(|base| base.name.len())
    }

    /// Looks up weapon base stats by base name (`Item.base` → `WeaponBaseStats`);
    /// returns `None` for a non-weapon / unknown base.
    pub fn weapon_base(&self, base_name: &str) -> Option<&WeaponBaseStats> {
        self.base_item(base_name).and_then(|b| b.weapon.as_ref())
    }

    /// Looks up armour base stats by base name (`Item.base` → `ArmourBaseStats`);
    /// returns `None` for a non-armour / unknown base.
    pub fn armour_base(&self, base_name: &str) -> Option<&ArmourBaseStats> {
        self.base_item(base_name).and_then(|b| b.armour.as_ref())
    }
}

impl pobr_core::skill_env::ArmourBaseLookup for BuildData {
    fn armour_base(&self, base_name: &str) -> Option<&ArmourBaseStats> {
        self.armour_base(base_name)
    }
}

impl pobr_core::skill_env::BaseItemLookup for BuildData {
    fn base_item(&self, base_name: &str) -> Option<&BaseItemDef> {
        self.base_item(base_name)
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
