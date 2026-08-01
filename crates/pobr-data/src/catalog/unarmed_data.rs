//! Unarmed base domain schema (`base/unarmed_data.json`, per-class unarmed
//! physical/attack-speed/crit bases).
//!
//! Corresponds to PoB2's `data.unarmedWeaponData`:
//! `vendor/PathOfBuilding-PoE2/src/Modules/Data.lua:553-563` (indexed by
//! PoE2 classId, 9 class entries); the crit constant is sourced from
//! `src/Data/Misc.lua:155`
//! (`characterConstants["unarmed_base_critical_strike_chance"] = 500`,
//! divided by 100 on vendor's side to get the percentage `5`).
//!
//! Migration invariant: values are migrated verbatim from pobr's existing
//! Rust source of truth,
//! `pobr-build::calc_orchestrator::unarmed_contribution`
//! (`phys_min` / `phys_max` / `attack_rate` / `crit_chance`); `class_id` /
//! `weapon_type` are vendor-only fields (pobr currently matches by
//! `class_name` instead).

use serde::{Deserialize, Serialize};

/// A single class's unarmed weapon base — the weaponData source for attack
/// skills when there's no main-hand weapon (matches PoB2
/// `CalcSetup.lua:1578`'s
/// `copyTable(env.data.unarmedWeaponData[env.classId])`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnarmedWeaponDef {
    /// PoE2 classId (the table key in vendor `Data.lua:554-562`): 0=Scion
    /// (a PoE2 legacy placeholder), 1=Witch, 2=Ranger, 6=Warrior,
    /// 7=Sorceress, 8=Huntress, 9=Mercenary, 10=Monk, 11=Druid. Vendor-only
    /// (pobr has no classId channel currently).
    pub class_id: u32,
    /// English class name (from vendor's trailing comment on the same
    /// line; the match key for pobr's `Build.character.class_name`).
    pub class_name: String,
    /// Weapon type (vendor's `type = "None"`, corresponding to
    /// `data.weaponTypeInfo["None"]` → the `Unarmed` flag, see
    /// `Data.lua:533`). Vendor-only.
    pub weapon_type: String,
    /// Base attack rate (per second; vendor's `AttackRate`, 1.65 for every class).
    pub attack_rate: f64,
    /// Base crit chance. pobr's current value is `0.05` (the decimal form
    /// matching `unarmed_contribution`'s comment saying "5% crit"), copied
    /// verbatim per the migration invariant.
    ///
    /// TODO(parity): vendor's same field is the percentage `5`
    /// (`Data.lua:554-562` = `Misc.lua:155`'s 500 / 100), and pobr's own
    /// weapon path (`weapon_contribution`'s `raw crit / 100`) produces
    /// `5.0` — the unarmed and armed paths disagree on units. This task
    /// only migrates the value without changing it; bringing behavior into
    /// alignment is a separate follow-up commit.
    pub crit_chance: f64,
    /// Base physical damage minimum (vendor's `PhysicalMin`, 2 for every class).
    pub physical_min: f64,
    /// Base physical damage maximum (vendor's `PhysicalMax`, by class:
    /// Warrior 8, Scion/Mercenary/Druid 6, everyone else 5; matches pobr's
    /// `unarmed_contribution` `class_name` match).
    pub physical_max: f64,
}

/// The full unarmed-base table (the domain
/// [`crate::catalog::RuntimeConstants`] injects).
///
/// `#[serde(transparent)]`: the JSON shape matches
/// `base/unarmed_data.json` (an array). `Default` = the fallback full table
/// (value-equal to the JSON, a migration invariant).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UnarmedDataTable(pub Vec<UnarmedWeaponDef>);

impl Default for UnarmedDataTable {
    fn default() -> Self {
        Self(UnarmedWeaponDef::default_table())
    }
}

impl UnarmedDataTable {
    /// Looks up the unarmed base entry by English class name; returns
    /// `None` for an unknown class (the caller falls back to a generic value).
    pub fn for_class(&self, class_name: &str) -> Option<&UnarmedWeaponDef> {
        self.0.iter().find(|e| e.class_name == class_name)
    }
}

impl UnarmedWeaponDef {
    /// The full-table fallback (the injection pipeline): the default value
    /// [`crate::catalog::RuntimeConstants`] uses when there's no GameData /
    /// the data directory is missing `base/unarmed_data.json`.
    ///
    /// Migration invariant: value-equal to the JSON (locked by a
    /// `pobr-gamedata` test); values sourced from pobr's old Rust source of
    /// truth, `pobr-build::calc_orchestrator::unarmed_contribution` (which
    /// lives in an upper-layer crate unreachable in the dependency
    /// direction, so it's transcribed here as literals with sourcing docs)
    /// plus vendor `Data.lua:554-562` (class_id / weapon_type and other
    /// vendor-only fields).
    pub fn default_table() -> Vec<Self> {
        /// Builds a single entry: weapon_type is `"None"`, attack_rate is
        /// 1.65, crit_chance is 0.05, and physical_min is 2 for every class
        /// (all from the same vendor source) — only class_id/class name/
        /// physical_max vary by class.
        fn entry(class_id: u32, class_name: &str, physical_max: f64) -> UnarmedWeaponDef {
            UnarmedWeaponDef {
                class_id,
                class_name: class_name.to_string(),
                weapon_type: "None".to_string(),
                attack_rate: 1.65,
                crit_chance: 0.05,
                physical_min: 2.0,
                physical_max,
            }
        }
        // Ascending by class_id (matches the JSON's sort order, so the Vec stays value-equal).
        vec![
            entry(0, "Scion", 6.0),
            entry(1, "Witch", 5.0),
            entry(2, "Ranger", 5.0),
            entry(6, "Warrior", 8.0),
            entry(7, "Sorceress", 5.0),
            entry(8, "Huntress", 5.0),
            entry(9, "Mercenary", 6.0),
            entry(10, "Monk", 5.0),
            entry(11, "Druid", 6.0),
        ]
    }
}
