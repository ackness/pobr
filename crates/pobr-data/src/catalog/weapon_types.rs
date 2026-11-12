//! Weapon type domain schema (`base/weapon_types.json`).
//!
//! Rows supply weapon classification and the vocabulary used by conditions and
//! modifier text. New flags work without a new engine bit; unknown type bits
//! retain generic classification and are reported by the build orchestrator.
//!
//! The table's ids use the imported base type vocabulary. The legacy aliases
//! between that vocabulary and item classes are resolved by `get_by_item_class`.

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
    /// Stable weapon flag (e.g. `Bow`/`Axe`/`Unarmed`), used for conditions
    /// and damage vocabulary. Known flags also have a fixed `ModFlags` bit;
    /// unknown flags keep generic classification without an invented type bit.
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
/// `base/weapon_types.json` (an array). `Default` supplies a compatibility
/// fallback for callers without game data; loaded rows are authoritative.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WeaponTypeTable(pub Vec<WeaponTypeDef>);

impl Default for WeaponTypeTable {
    fn default() -> Self {
        Self(WeaponTypeDef::default_table())
    }
}

impl WeaponTypeTable {
    /// Resolve legacy base-type aliases once for all equipment consumers.
    /// Caster staves have no weapon row; `Staff` denotes the quarterstaff.
    pub fn get_by_item_class(&self, item_class: &str) -> Option<&WeaponTypeDef> {
        self.get(match item_class {
            "Warstaff" => "Staff",
            "Staff" => return None,
            "FishingRod" => "Fishing Rod",
            other => other,
        })
    }

    /// Looks up an entry by weapon type name (the PoB base `type`, e.g.
    /// `One Hand Axe`); returns `None` for an unknown type. Key-space trap
    /// (see the module doc): GGG's `Warstaff` (the quarterstaff's
    /// item_class) corresponds to this table's `Staff`, and GGG's
    /// `FishingRod` corresponds to `Fishing Rod`. Equipment consumers use
    /// [`Self::get_by_item_class`] to resolve those aliases.
    pub fn get(&self, id: &str) -> Option<&WeaponTypeDef> {
        self.0.iter().find(|w| w.id == id)
    }
}

impl WeaponTypeDef {
    /// The established scope spelling for this flag.
    pub fn scope_name(&self) -> &str {
        if self.flag == "Staff" {
            "Quarterstaff"
        } else {
            &self.flag
        }
    }

    /// Conditions follow the flag, with the historical Quarterstaff alias.
    pub fn conditions(&self) -> Vec<String> {
        let mut names = vec![format!("Using{}", self.scope_name())];
        if self.scope_name() != self.flag {
            names.push(format!("Using{}", self.flag));
        }
        names
    }

    /// Match the orchestration's condition vocabulary without allocating a list.
    pub fn matches_condition(&self, condition: &str) -> bool {
        condition
            .strip_prefix("Using")
            .is_some_and(|name| name == self.flag || name == self.scope_name())
    }

    /// These historical damage names did not carry a per-hand type bit. Keep
    /// their main-hand keyword routing so existing calculations stay unchanged.
    /// Other weapon names retain their type bits when normalized.
    pub fn uses_legacy_damage_keyword(&self) -> bool {
        matches!(
            self.flag.as_str(),
            "Spear" | "Crossbow" | "Bow" | "Mace" | "Staff"
        )
    }

    /// Damage vocabulary keeps the existing Quarterstaff spelling.
    pub fn damage_keyword(&self) -> String {
        format!("{}Damage", self.scope_name())
    }

    /// Compatibility fallback for callers without `weapon_types.json`.
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
        // Keep the fallback deterministic.
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
