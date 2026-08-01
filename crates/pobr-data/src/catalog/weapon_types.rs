//! Weapon type domain schema (`base/weapon_types.json`).
//!
//! Source table: PoB2's `data.weaponTypeInfo`
//! (`vendor/PathOfBuilding-PoE2/src/Modules/Data.lua:532-551`, 19 entries).
//! This table is a **line-for-line migration** (a migration invariant):
//! field values match vendor exactly; discrepancies between pobr's existing
//! scattered Rust judgments (see below) and vendor are only recorded, not
//! fixed here.
//!
//! Key-space note: `id` is the PoB base item's `type` name (vendor
//! `Data/Bases/*.lua`'s `type` field), **not** GGG's `ItemClasses.Id`
//! (pobr's `BaseItemDef::item_class`). The two mostly share names; known
//! discrepancies:
//! - PoE2's quarterstaff base has `type = "Staff"`, `subType = "Warstaff"`
//!   (`Data/Bases/staff.lua:159-167`), corresponding to this table's
//!   `id = "Staff"` (`label = "Quarterstaff"`); whereas GGG's item_class
//!   records the quarterstaff as `Warstaff` and the caster staff as
//!   `Staff`. This table's `id = "Warstaff"` entry currently has no base
//!   item using it in vendor's data (a legacy entry — `subType` only shows
//!   up as `Warstaff`).
//! - Fishing rod: this table's `id = "Fishing Rod"` (with a space), GGG's
//!   item_class is `FishingRod`.
//!
//! Known discrepancies with pobr's existing Rust judgments (recorded only;
//! bringing behavior into alignment is a separate follow-up commit):
//! - TODO(parity): `pobr-build::calc_orchestrator::weapon_type_conditions`'s
//!   melee-class list (the `matches!` branch) doesn't include `Talisman` /
//!   `FishingRod`, while vendor treats both `Talisman` and `Fishing Rod` as
//!   `melee = true`.
//! - TODO(parity): the same function's `two_handed` predicate
//!   (`starts_with("Two Hand") || "Warstaff" || "Staff"`) evaluates to
//!   false (i.e. treated as one-handed) for `Bow` / `Crossbow` /
//!   `Talisman` / `FishingRod`, while vendor has `oneHand = false` for all
//!   of these types.
//!
//! Ranged derivation: vendor has no separate range field; ranged =
//! `!melee`; deriving `ModFlags` bits from `flag` stays in code.

use serde::{Deserialize, Serialize};

/// A weapon type definition (corresponds to one entry of vendor's
/// `data.weaponTypeInfo`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeaponTypeDef {
    /// Weapon type name (the PoB base item's `type`, e.g. `One Hand Axe`;
    /// `None` = unarmed).
    pub id: String,
    /// Whether it's one-handed (vendor's `oneHand`; used for dual-wield /
    /// grip condition checks).
    pub one_hand: bool,
    /// Whether it's melee (vendor's `melee`; ranged = `!melee`, vendor has
    /// no separate range field).
    pub melee: bool,
    /// ModFlag name (vendor's `flag`, e.g. `Bow`/`Axe`/`Unarmed`) — the
    /// weapon-type bit (a `ModFlags` extension bit) is derived from this;
    /// the bit enum itself stays in code (L4).
    pub flag: String,
    /// Display alias (vendor's `label`; defaults to displaying `id` when
    /// absent). Two known entries: `Staff` → `Quarterstaff`,
    /// `Thrusting One Hand Sword` → `One Hand Sword`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// The full weapon-type table (the domain
/// [`crate::catalog::RuntimeConstants`] injects).
///
/// `#[serde(transparent)]`: the JSON shape matches
/// `base/weapon_types.json` (an array). `Default` = the fallback full table
/// (value-equal to the JSON, a migration invariant).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WeaponTypeTable(pub Vec<WeaponTypeDef>);

impl Default for WeaponTypeTable {
    fn default() -> Self {
        Self(WeaponTypeDef::default_table())
    }
}

impl WeaponTypeTable {
    /// Looks up an entry by weapon type name (the PoB base `type`, e.g.
    /// `One Hand Axe`); returns `None` for an unknown type. Key-space trap
    /// (see the module doc): GGG's `Warstaff` (the quarterstaff's
    /// item_class) corresponds to this table's `Staff`, and GGG's
    /// `FishingRod` corresponds to `Fishing Rod` — mapping item_class to a
    /// table key is the caller's job (the consumer side, see pobr-build's
    /// `calc_orchestrator`).
    pub fn get(&self, id: &str) -> Option<&WeaponTypeDef> {
        self.0.iter().find(|w| w.id == id)
    }
}

impl WeaponTypeDef {
    /// The full-table fallback (the injection pipeline): the default value
    /// [`crate::catalog::RuntimeConstants`] uses when there's no GameData /
    /// the data directory is missing `base/weapon_types.json`.
    ///
    /// Migration invariant: value-equal to the JSON (locked by a
    /// `pobr-gamedata` test). Values sourced from vendor's
    /// `data.weaponTypeInfo` (`Data.lua:532-551`, all 19 entries copied
    /// verbatim); pobr's old Rust side has no complete equivalent table
    /// (scattered predicates instead — see the module doc's
    /// TODO(parity) for the discrepancies), so this is a literal table with
    /// sourcing docs.
    pub fn default_table() -> Vec<Self> {
        /// Builds a single entry (see the struct field docs for `label`;
        /// only two entries have a non-empty one).
        fn entry(
            id: &str,
            one_hand: bool,
            melee: bool,
            flag: &str,
            label: Option<&str>,
        ) -> WeaponTypeDef {
            WeaponTypeDef {
                id: id.to_string(),
                one_hand,
                melee,
                flag: flag.to_string(),
                label: label.map(str::to_string),
            }
        }
        // Ascending by id (matches the JSON's sort order, so the Vec stays value-equal).
        vec![
            entry("Bow", false, false, "Bow", None),
            entry("Claw", true, true, "Claw", None),
            entry("Crossbow", false, false, "Crossbow", None),
            entry("Dagger", true, true, "Dagger", None),
            entry("Fishing Rod", false, true, "Fishing", None),
            entry("Flail", true, true, "Flail", None),
            entry("None", true, true, "Unarmed", None),
            entry("One Hand Axe", true, true, "Axe", None),
            entry("One Hand Mace", true, true, "Mace", None),
            entry("One Hand Sword", true, true, "Sword", None),
            entry("Spear", true, true, "Spear", None),
            entry("Staff", false, true, "Staff", Some("Quarterstaff")),
            entry("Talisman", false, true, "Talisman", None),
            entry(
                "Thrusting One Hand Sword",
                true,
                true,
                "Sword",
                Some("One Hand Sword"),
            ),
            entry("Two Hand Axe", false, true, "Axe", None),
            entry("Two Hand Mace", false, true, "Mace", None),
            entry("Two Hand Sword", false, true, "Sword", None),
            entry("Wand", true, false, "Wand", None),
            entry("Warstaff", false, true, "Warstaff", None),
        ]
    }
}
