//! `overlay/high_precision_mods.json` load tests.
//!
//! This table currently has **zero consumers** (pobr hasn't implemented
//! ScaleAddMod / the MORE precision exception branch); its values
//! are a faithful transcription of vendor PoB2
//! `src/Modules/Data.lua:413-530` (vendor commit `2df5a74`); the only
//! field with a pobr source of truth is `more_default_round_decimals`
//! (= `pobr-core::mod_db::round_more`'s hardcoded 2, matching value-for-value).

use pobr_data::catalog::high_precision_mods::HighPrecisionModsDef;
use pobr_gamedata::{GameData, repo_data_root};

fn version() -> String {
    pobr_gamedata::data_version()
}

fn load() -> HighPrecisionModsDef {
    GameData::new(repo_data_root().join(version()))
        .high_precision_mods()
        .expect("high_precision_mods 可加载")
}

/// Gets the precision digit count for a given mod name + mod type (panics
/// if the entry is missing).
fn precision(def: &HighPrecisionModsDef, name: &str, mod_type: &str) -> u32 {
    *def.mods
        .get(name)
        .unwrap_or_else(|| panic!("存在 {name} 条目"))
        .get(mod_type)
        .unwrap_or_else(|| panic!("{name} 存在 {mod_type} 精度"))
}

/// Default values, value-for-value: `defaultHighPrecision = 1` (vendor
/// `Data.lua:413`); MORE's default rounding is 2 places (pobr's source of
/// truth `mod_db::round_more` = vendor `ModList.lua:144`'s
/// `round(modResult, 2)`, a migration invariant).
#[test]
fn default_precisions_match_sources() {
    let def = load();
    assert_eq!(def.default_high_precision, 1);
    assert_eq!(def.more_default_round_decimals, 2);
}

/// The exception table's total entry count = vendor's `highPrecisionMods`'
/// 38 entries (`Data.lua:415-530`).
#[test]
fn exception_table_has_vendor_entry_count() {
    assert_eq!(load().mods.len(), 38);
}

/// Value-for-value spot checks on vendor entries: one representative each
/// from the BASE 1 / BASE 2 / MORE 4 tiers.
#[test]
fn vendor_entries_spot_checks() {
    let def = load();
    // BASE precision 2: crit / regen-percent-per-second / the leech family.
    assert_eq!(precision(&def, "CritChance", "BASE"), 2);
    assert_eq!(precision(&def, "SelfCritChance", "BASE"), 2);
    assert_eq!(precision(&def, "LifeRegenPercent", "BASE"), 2);
    assert_eq!(precision(&def, "DamageLifeLeech", "BASE"), 2);
    assert_eq!(precision(&def, "ChaosDamageEnergyShieldLeech", "BASE"), 2);
    // BASE precision 1: the flat regen/degen family.
    assert_eq!(precision(&def, "LifeRegen", "BASE"), 1);
    assert_eq!(precision(&def, "RageRegen", "BASE"), 1);
    assert_eq!(precision(&def, "EnergyShieldDegen", "BASE"), 1);
    // MORE precision 4: support gem/reservation multipliers (the only two
    // MORE exceptions).
    assert_eq!(precision(&def, "SupportManaMultiplier", "MORE"), 4);
    assert_eq!(precision(&def, "ReservationMultiplier", "MORE"), 4);
}

/// Structural validity: mod type is only `BASE`/`MORE`, precision digit
/// count is within 1..=4, and every entry has at least one mod type
/// (guards against transcription drift).
#[test]
fn entries_are_structurally_valid() {
    let def = load();
    for (name, by_type) in &def.mods {
        assert!(!by_type.is_empty(), "{name} 条目不应为空");
        for (mod_type, p) in by_type {
            assert!(
                mod_type == "BASE" || mod_type == "MORE",
                "{name} 的 mod type {mod_type} 不在 BASE/MORE"
            );
            assert!((1..=4).contains(p), "{name} {mod_type} 精度 {p} 超出 1..=4");
        }
    }
}
