//! Base item domain schema (`base/base_items.json`, sourced from
//! `BaseItemTypes.dat` etc.).

use serde::{Deserialize, Serialize};

/// A base item definition (from `BaseItemTypes.dat` plus foreign-key
/// resolution).
///
/// `name` is English canonical; names in other languages go through the
/// `i18n/<lang>/base_items.json` sidecar (`id -> localized name`).
/// Weapon/armour numbers (e.g. PhysicalMin/Max) come from the separate
/// `WeaponTypes` / `ArmourTypes` tables, wired in by a later slice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaseItemDef {
    /// Stable ID, i.e. the `.dat`'s `Id` (e.g.
    /// `Metadata/Items/Weapons/.../FourOneHandAxe1`).
    pub id: String,
    /// English canonical name.
    pub name: String,
    /// Item class (resolves `ItemClasses.Id`, e.g. `One Hand Axe`).
    pub item_class: String,
    /// Drop level.
    pub drop_level: u32,
    /// Inventory width / height.
    pub width: u8,
    pub height: u8,
    /// Tags (resolves `Tags.Id`, e.g. `ezomyte_basetype`).
    pub tags: Vec<String>,
    /// Stable mod IDs for implicit mods (resolves `Mods.Id`).
    pub implicits: Vec<String>,
    /// Mod domain (a raw GGG enum value, used to decide which mods can
    /// apply to this base).
    pub mod_domain: u32,
    /// Weapon base stats (from `WeaponTypes.dat`; `None` for non-weapons) —
    /// the base for attack damage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weapon: Option<WeaponBaseStats>,
    /// Armour base stats (from `ArmourTypes.dat`; `None` for non-armour) —
    /// the local base for armour/evasion/ES.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub armour: Option<ArmourBaseStats>,
    /// The base's inherent Spirit (e.g. a sceptre's `spirit = 100`; sourced
    /// from `ItemSpirit.dat`'s `SpiritGranted`).
    ///
    /// This table's bundle has been pruned from the CDN at the pinned
    /// patch (the `.dat` route is unavailable), so it's currently filled in
    /// via a gamedata merge from `overlay/base_item_overrides.json`
    /// (deterministically extracted from vendor `Data/Bases/*.lua` by
    /// `sync-pob-catalog extract-bases`) — the dual-route fallback from §6
    /// open question 2. `None` for bases without Spirit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spirit: Option<u32>,
    /// A charm base's inherent buff mod (vendor `Data/Bases/flask.lua`'s
    /// `charm.buff`, e.g. a Ruby Charm's `"+25% to Fire Resistance"`,
    /// Sapphire/Topaz's cold/lightning resist, Amethyst's mixed resist, and
    /// the other immunity-type charms). This buff **isn't in the item's
    /// text** — it's an inherent property of the base; when the charm is
    /// active, `pobr_core::ingest::item::ingest_flask_charm` folds it into
    /// the `CharmBuff` payload (vendor `Item.lua:838-844` runs each line of
    /// `base.charm.buff` through parseMod into `buffModList`). No GGG
    /// `.dat` column for this — filled in via a gamedata merge from
    /// `overlay/base_item_overrides.json` (extracted from vendor
    /// `Data/Bases/flask.lua` by `sync-pob-catalog extract-bases`) — the
    /// same dual-route fallback as [`BaseItemDef::spirit`]. Empty for
    /// non-charms / charms without a buff.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub charm_buff: Vec<String>,
    /// The base's attribute requirements (vendor `Data/Bases/*.lua`'s
    /// `req = { str/dex/int }`; the corresponding GGG `.dat` table's bundle
    /// is unavailable, so this is extracted and merged via
    /// `overlay/base_item_overrides.json` — the same fallback as
    /// [`Self::spirit`]). Consumed by the equipment-requirement snapshot
    /// `<Attr>RequirementsOn<slot>` (vendor CalcPerform.lua:1848-1857,
    /// Smith's "Gain Armour equal to 150% of total Strength Requirements
    /// …"). 0 when there's no requirement.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub req_str: u32,
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub req_dex: u32,
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub req_int: u32,
}

/// Weapon base stats (`WeaponTypes.dat` foreign-key resolution; the base
/// for attack-skill damage, matching PoB2 `CalcSetup.lua`'s weaponData
/// assembly). Values are all raw `.dat` integers; unit conversion happens
/// on the calc side.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeaponBaseStats {
    /// Base physical damage min/max (`DamageMin`/`DamageMax`).
    pub physical_min: u32,
    pub physical_max: u32,
    /// Attack interval (`Speed`, milliseconds); attack rate = `1000 / speed_ms`.
    pub speed_ms: u32,
    /// Base crit chance (`CritChance` raw value; crit% = `crit_chance / 100`,
    /// e.g. `500` = 5%).
    pub crit_chance: u32,
    /// Attack range (`RangeMax`).
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub range: u32,
    /// Crossbow reload time (milliseconds). The real source is
    /// `WeaponTypes.dat`'s `ReloadTime` column (vendor
    /// `Export/spec.lua:62483`, `Export/Scripts/bases.lua:268-269`'s
    /// `ReloadTimeBase = ReloadTime/1000`, exported only when >0). While
    /// the local pipeline/tables snapshot is missing it (drill F3/F8),
    /// it's filled in via a gamedata merge from
    /// `overlay/base_item_overrides.json` (vendor
    /// `Data/Bases/crossbow.lua`'s `ReloadTimeBase` seconds value ×1000,
    /// via `sync-pob-catalog extract-bases`) — the same dual-route
    /// fallback as [`BaseItemDef::spirit`]. `None` for non-crossbows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reload_time_ms: Option<u32>,
}

/// Armour base stats (`ArmourTypes.dat` foreign-key resolution; the local
/// base for armour/evasion/ES/ward).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArmourBaseStats {
    pub armour: u32,
    pub evasion: u32,
    pub energy_shield: u32,
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub ward: u32,
    /// A shield's base block chance (%; sourced from `ShieldTypes.dat`'s
    /// `Block` column, matching PoB2 `Export/Scripts/bases.lua:277-279`).
    /// This table's bundle has been pruned from the CDN at the pinned
    /// patch, so it's currently filled in via a gamedata merge from
    /// `overlay/base_item_overrides.json` (the dual-route fallback, see
    /// [`BaseItemDef::spirit`]'s note). `None` for non-shields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_chance: Option<f64>,
    /// Movement speed penalty (a fraction, e.g. `0.03` = 3% slower;
    /// `ArmourTypes.dat`'s `IncreasedMovementSpeed` raw value converted per
    /// PoB2's convention as `-raw/10000`, matching
    /// `Export/Scripts/bases.lua:298-300`). `None` when there's no penalty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub movement_penalty: Option<f64>,
}

/// serde predicate to skip a zero u32 (keeps diffs clean).
fn is_zero_u32(v: &u32) -> bool {
    *v == 0
}

#[cfg(test)]
mod m4_t4_reload_tests {
    use super::WeaponBaseStats;

    /// `reload_time_ms` round-trips through serde losslessly; `None` isn't
    /// written out (zero diff against existing base JSON), and old JSON
    /// (missing the key) deserializes to `None` (schema backward
    /// compatible).
    #[test]
    fn reload_time_round_trip_and_backward_compatible() {
        let weapon = WeaponBaseStats {
            physical_min: 7,
            physical_max: 12,
            speed_ms: 625,
            crit_chance: 500,
            range: 120,
            reload_time_ms: Some(800),
        };
        let json = serde_json::to_string(&weapon).unwrap();
        assert!(json.contains(r#""reload_time_ms":800"#));
        let parsed: WeaponBaseStats = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, weapon, "serde round trip must be lossless");

        let none = WeaponBaseStats {
            reload_time_ms: None,
            ..weapon
        };
        let json = serde_json::to_string(&none).unwrap();
        assert!(
            !json.contains("reload_time_ms"),
            "None must not be serialized: {json}"
        );
        let legacy: WeaponBaseStats = serde_json::from_str(&json).unwrap();
        assert_eq!(
            legacy.reload_time_ms, None,
            "missing key in legacy JSON must fall back to None"
        );
    }
}
