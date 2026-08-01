//! Defensive keystone switch registry (13-G6 / 13-G16).
//!
//! Consolidates the "data flag -> a fixed set of stable branches" switches
//! into a one-time snapshot struct, [`DefenceKeystones`]: calc code reads
//! this struct read-only everywhere, instead of each site reading keystone
//! flags separately.
//!
//! Data vs. logic split (conclusion of 13-defence §5): the switch itself is
//! data (a tree modifier -> `Modifier::flag`, landed in the ModDb by
//! mod_parser / passive ingest); the behavior is logic (the finite set of
//! fields enumerated by this registry, plus each consumption point's
//! branching). **No** per-unique hardcoding.
//!
//! Vendor cross-reference (`vendor/PathOfBuilding-PoE2/src/Modules/CalcDefence.lua`,
//! line numbers verified 2026-06-11):
//! - `ChaosInoculation`: :85 (flag read out) / :120-123 (Life=1 + FullLife) /
//!   :2537-2539 (stun threshold uses pre-CI Life).
//! - `EnergyShieldProtectsMana` (EB): :597-603 (ES nests inside the MoMEBPool
//!   that protects Mana) / :2726-2820 (MoM/EB pool assembly).
//! - `EternalLife`: :588-594 (ES deduction branches are mutually exclusive).
//! - `IronReflexes`/`Unbreakable`: :806-808 and :1235-1237 (when both flags
//!   hold, Body Armour's evasion base ×2); `Unbreakable` alone doubles Body
//!   Armour's armour base (:790-795 / :1216-1221).
//! - `DoubleBodyArmourDefence`: :1150-1290 (Body Armour's ward/ES/armour/evasion
//!   bases all ×2).
//! - `EnergyShieldToWard`: :1160-1192 (ES's increased% is lent to Ward; ES
//!   itself no longer aggregates it).
//! - `WardNotBreak`: :560-575 (ward is refunded after being deducted) /
//!   :3030 (the EHP = infinity branch).
//! - `BloodMagic`: :172-350 (reservation routed through life instead, wired
//!   to reservation; this phase only reserves the field).

use crate::{CalcConfig, ModDb};
use pobr_data::prelude::*;

/// Threshold for full ES->Mana conversion (`EnergyShieldConvertToMana` BASE
/// accumulating to ≥ 100 counts as the Eldritch Battery-style "convert all
/// ES to Mana", matching PoB2 resourceList's cap-100 semantics).
const ES_TO_MANA_FULL_CONVERSION_PCT: f64 = 100.0;

/// A one-time snapshot of defensive keystone switches, read from the ModDb
/// once; calc code reads this struct read-only everywhere instead of
/// reading flags separately.
///
/// Constructed via [`DefenceKeystones::from_db`]. All fields are `bool` and
/// `Copy`, passed by value; `Default` = everything off (matches the
/// behavior of a build with no keystones, same as before this struct
/// existed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DefenceKeystones {
    /// Chaos Inoculation: Life=1, ES becomes the life pool, chaos immunity
    /// (mod_parser already parses `Maximum Life is 1` -> Override + flag,
    /// ~L537).
    pub chaos_inoculation: bool,
    /// Eldritch Battery-style full ES->Mana conversion:
    /// `EnergyShieldConvertToMana` BASE ≥ 100 (folds in the existing
    /// `es_to_mana_rate` "full conversion" semantics; partial conversion
    /// goes through the conversion matrix's numeric channel instead).
    pub eldritch_battery_es_to_mana: bool,
    /// The EB flag (`EnergyShieldProtectsMana`, a W0.1 modifier): ES
    /// protects Mana instead of Life.
    pub energy_shield_protects_mana: bool,
    /// Eternal Life: the ES-deduction branches are mutually exclusive
    /// (CalcDefence.lua:588-594).
    pub eternal_life: bool,
    /// Iron Reflexes: the data-level expansion of `EvasionConvertToArmour`
    /// 100 still goes through the conversion matrix; this flag only feeds
    /// the Unbreakable interaction (Body Armour evasion base ×2).
    pub iron_reflexes: bool,
    /// Unbreakable: Body Armour armour base ×2; when combined with
    /// IronReflexes, evasion base is also ×2.
    pub unbreakable: bool,
    /// Body Armour's ward/ES/armour/evasion bases are all ×2
    /// (CalcDefence.lua:1150-1290).
    pub double_body_armour_defence: bool,
    /// ES's increased% is lent to Ward, and ES itself no longer aggregates
    /// it (CalcDefence.lua:1160-1192, consumed by Track D).
    pub energy_shield_to_ward: bool,
    /// Ward is refunded after being deducted, instead of breaking
    /// (CalcDefence.lua:560-575, consumed by Track A/F).
    pub ward_not_break: bool,
    /// Blood Magic: reservation routed through life instead (reserved
    /// field, wired to reservation).
    pub blood_magic: bool,
}

impl DefenceKeystones {
    /// Reads all defensive keystone switches from the ModDb in one pass.
    ///
    /// Except for `eldritch_battery_es_to_mana` (`EnergyShieldConvertToMana`
    /// BASE accumulating to ≥ 100), every other field is read directly from
    /// a like-named `Flag` modifier (`ModDb::flag`, filtered by `cfg`
    /// conditions).
    pub fn from_db(db: &ModDb, cfg: &CalcConfig) -> Self {
        let es_to_mana_pct = db
            .sum(
                ModType::Base,
                cfg,
                &[ModName::from("EnergyShieldConvertToMana")],
            )
            .clamp(0.0, ES_TO_MANA_FULL_CONVERSION_PCT);
        Self {
            chaos_inoculation: db.flag(cfg, ModName::from("ChaosInoculation")),
            eldritch_battery_es_to_mana: es_to_mana_pct >= ES_TO_MANA_FULL_CONVERSION_PCT,
            energy_shield_protects_mana: db.flag(cfg, ModName::from("EnergyShieldProtectsMana")),
            eternal_life: db.flag(cfg, ModName::from("EternalLife")),
            iron_reflexes: db.flag(cfg, ModName::from("IronReflexes")),
            unbreakable: db.flag(cfg, ModName::from("Unbreakable")),
            double_body_armour_defence: db.flag(cfg, ModName::from("DoubleBodyArmourDefence")),
            energy_shield_to_ward: db.flag(cfg, ModName::from("EnergyShieldToWard")),
            ward_not_break: db.flag(cfg, ModName::from("WardNotBreak")),
            blood_magic: db.flag(cfg, ModName::from("BloodMagic")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Modifier;

    /// An empty ModDb -> every switch off (matches `Default`).
    #[test]
    fn from_db_empty_is_all_off() {
        // Arrange
        let db = ModDb::new();
        let cfg = CalcConfig::new();

        // Act
        let ks = DefenceKeystones::from_db(&db, &cfg);

        // Assert
        assert_eq!(ks, DefenceKeystones::default());
        assert!(!ks.chaos_inoculation);
        assert!(!ks.eldritch_battery_es_to_mana);
    }

    /// Each flag modifier drives its matching field individually (a
    /// one-time snapshot; no other flags are read).
    #[test]
    fn from_db_reads_each_flag() {
        // Arrange
        let mut db = ModDb::new();
        db.add_list([
            Modifier::flag("ChaosInoculation"),
            Modifier::flag("EnergyShieldProtectsMana"),
            Modifier::flag("EternalLife"),
            Modifier::flag("IronReflexes"),
            Modifier::flag("Unbreakable"),
            Modifier::flag("DoubleBodyArmourDefence"),
            Modifier::flag("EnergyShieldToWard"),
            Modifier::flag("WardNotBreak"),
            Modifier::flag("BloodMagic"),
        ]);
        let cfg = CalcConfig::new();

        // Act
        let ks = DefenceKeystones::from_db(&db, &cfg);

        // Assert
        assert!(ks.chaos_inoculation);
        assert!(ks.energy_shield_protects_mana);
        assert!(ks.eternal_life);
        assert!(ks.iron_reflexes);
        assert!(ks.unbreakable);
        assert!(ks.double_body_armour_defence);
        assert!(ks.energy_shield_to_ward);
        assert!(ks.ward_not_break);
        assert!(ks.blood_magic);
        // EnergyShieldConvertToMana was never injected -> eldritch stays false.
        assert!(!ks.eldritch_battery_es_to_mana);
    }

    /// `EnergyShieldConvertToMana` only counts as full conversion once it
    /// accumulates to ≥100 (50+50 hits, 50 alone doesn't).
    #[test]
    fn eldritch_battery_requires_full_conversion() {
        // Arrange: 50% partial conversion -> not the full-conversion keystone.
        let mut partial = ModDb::new();
        partial.add_list([Modifier::number(
            "EnergyShieldConvertToMana",
            ModType::Base,
            50.0,
        )]);
        // 50 + 50 = 100 -> full conversion.
        let mut full = ModDb::new();
        full.add_list([
            Modifier::number("EnergyShieldConvertToMana", ModType::Base, 50.0),
            Modifier::number("EnergyShieldConvertToMana", ModType::Base, 50.0),
        ]);
        let cfg = CalcConfig::new();

        // Act / Assert
        assert!(!DefenceKeystones::from_db(&partial, &cfg).eldritch_battery_es_to_mana);
        assert!(DefenceKeystones::from_db(&full, &cfg).eldritch_battery_es_to_mana);
    }
}
