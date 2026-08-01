//! `base/enemy_presets.json` load tests.
//!
//! Migration-invariant checks: fields with a Rust source of truth
//! (`pobr_data::monster::EnemyTier`'s methods + `setup_env.rs`'s injected
//! values) are asserted **value-for-value, tier by tier** against the
//! existing Rust table; vendor-only fields
//! (KnockbackDistanceOnSelf / MinimumMovementSpeed / PoiseThreshold
//! 213/838 / player_mods / chaos_damage_div / speed/crit placeholders) are
//! asserted against hardcoded values per vendor
//! `ConfigOptions.lua` (commit 2df5a74) line numbers.

use pobr_data::catalog::enemy_presets::{EnemyPresetsTable, EnemyTierPreset};
use pobr_data::monster::{EnemyTier, MAX_ENEMY_LEVEL, MONSTER_BASE_CRIT_DAMAGE_BONUS};
use pobr_gamedata::{GameData, repo_data_root};

fn version() -> String {
    pobr_gamedata::data_version()
}

fn load() -> EnemyPresetsTable {
    GameData::new(repo_data_root().join(version()))
        .enemy_presets()
        .expect("enemy_presets 可加载")
}

/// Gets a given tier (also checks the JSON tier order = None → Boss → Pinnacle → Uber).
fn tier<'a>(t: &'a EnemyPresetsTable, id: &str) -> &'a EnemyTierPreset {
    t.tiers
        .iter()
        .find(|p| p.id == id)
        .unwrap_or_else(|| panic!("缺少档位 {id}"))
}

/// Finds an entry in a tier's mod group (matched by name + value, since
/// PoiseThreshold has two entries sharing a name).
fn find_mod<'a>(
    mods: &'a [pobr_data::catalog::enemy_presets::EnemyPresetMod],
    name: &str,
    value: f64,
) -> &'a pobr_data::catalog::enemy_presets::EnemyPresetMod {
    mods.iter()
        .find(|m| m.name == name && m.value == value)
        .unwrap_or_else(|| panic!("缺少 mod {name} = {value}"))
}

/// All four tiers present, in an order matching vendor's list / pobr's
/// `EnemyTier` enum order.
#[test]
fn four_tiers_in_canonical_order() {
    let t = load();
    let ids: Vec<&str> = t.tiers.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, ["None", "Boss", "Pinnacle", "Uber"]);
    // Default tier = Pinnacle (vendor defaultIndex = 3; pobr's `EnemyTier::default()`).
    let default_id = format!("{:?}", EnemyTier::default());
    for p in &t.tiers {
        assert_eq!(p.is_default, p.id == default_id, "档位 {} 默认位", p.id);
    }
}

/// Top-level shared defaults: value-equal to pobr's source of truth plus a
/// vendor-only spot check.
#[test]
fn top_level_defaults_match_rust_sources() {
    let t = load();
    // pobr's source of truth: monster.rs::MAX_ENEMY_LEVEL / MONSTER_BASE_CRIT_DAMAGE_BONUS.
    assert_eq!(t.max_enemy_level, MAX_ENEMY_LEVEL);
    assert_eq!(
        t.default_enemy_crit_damage_bonus,
        MONSTER_BASE_CRIT_DAMAGE_BONUS
    );
    // pobr's source of truth: the 1.5 inlined in
    // EnemyTierDefaults::compute's `damage * 1.5 * dps_mult`.
    assert_eq!(t.ehp_base_damage_mult, 1.5);
    // vendor-only: ConfigOptions.lua L1965 (enemySpeed placeholder 700) /
    // L1966 (crit 5%).
    assert_eq!(t.default_enemy_speed, 700.0);
    assert_eq!(t.default_enemy_crit_chance, 5.0);
}

/// Per-tier scalars are value-equal to pobr's `EnemyTier` methods (the
/// core assertion for the migration invariant).
#[test]
fn tier_scalars_match_enemy_tier_methods() {
    let t = load();
    for (id, rust_tier) in [
        ("None", EnemyTier::None),
        ("Boss", EnemyTier::Boss),
        ("Pinnacle", EnemyTier::Pinnacle),
        ("Uber", EnemyTier::Uber),
    ] {
        let p = tier(&t, id);
        assert_eq!(p.min_level, rust_tier.min_level(), "{id} min_level");
        assert_eq!(
            p.elemental_resist_bonus,
            rust_tier.elemental_resist_bonus(),
            "{id} elemental_resist_bonus"
        );
        assert_eq!(
            p.chaos_resist_bonus,
            rust_tier.chaos_resist_bonus(),
            "{id} chaos_resist_bonus"
        );
        // ExactRatio::value() requires being **bit-identical** to the Rust
        // source of truth (the fraction form guarantees serde_json's
        // default float parsing is lossless, see the schema doc).
        assert_eq!(
            p.armour_mult_pct.value(),
            rust_tier.armour_mult_pct(),
            "{id} armour_mult_pct"
        );
        assert_eq!(
            p.evasion_mult_pct.value(),
            rust_tier.evasion_mult_pct(),
            "{id} evasion_mult_pct"
        );
        assert_eq!(p.pen, rust_tier.pen(), "{id} pen");
        assert_eq!(p.dps_mult.value(), rust_tier.dps_mult(), "{id} dps_mult");
    }
}

/// Condition states match pobr's `setup_env.rs` injection
/// (Unique/RareOrUnique; Pinnacle/Uber add PinnacleBoss).
#[test]
fn conditions_match_setup_env_injection() {
    let t = load();
    assert!(tier(&t, "None").conditions.is_empty());
    assert_eq!(tier(&t, "Boss").conditions, ["Unique", "RareOrUnique"]);
    for id in ["Pinnacle", "Uber"] {
        assert_eq!(
            tier(&t, id).conditions,
            ["Unique", "RareOrUnique", "PinnacleBoss"],
            "{id} conditions"
        );
    }
}

/// The boss-shared mods pobr already injects
/// (setup_env.rs::inject_enemy_mods) are value-equal tier by tier:
/// Curse/Exposure/Slow `MORE -50` (Effective-gated) + `PoiseThreshold MORE
/// 500` (ungated).
#[test]
fn boss_common_mods_match_setup_env_values() {
    let t = load();
    assert!(tier(&t, "None").enemy_mods.is_empty(), "None 档无 mod 组");
    for id in ["Boss", "Pinnacle", "Uber"] {
        let mods = &tier(&t, id).enemy_mods;
        for name in [
            "CurseEffectOnSelf",
            "ExposureEffectOnSelf",
            "SlowEffectOnSelf",
        ] {
            let m = find_mod(mods, name, -50.0);
            assert_eq!(m.mod_type, "MORE", "{id} {name} 类型");
            assert!(
                m.effective_only,
                "{id} {name} 应带 Effective 门控（pobr 现状）"
            );
        }
        let poise = find_mod(mods, "PoiseThreshold", 500.0);
        assert_eq!(poise.mod_type, "MORE", "{id} PoiseThreshold 类型");
        // pobr's setup_env.rs injects via push_enemy_number (no Effective
        // gate); vendor L2005 has the gate — the discrepancy is already
        // recorded in the schema doc's TODO(parity); asserted here per
        // pobr's current behavior.
        assert!(
            !poise.effective_only,
            "{id} PoiseThreshold 500 按 pobr 现状不带门控"
        );
    }
}

/// Uber-only: `DamageTaken MORE -70` (pobr's
/// `EnemyTier::damage_taken_more()`; vendor L2087, no Effective gate); must
/// not appear in any other tier.
#[test]
fn uber_damage_taken_matches_rust_source() {
    let t = load();
    let m = find_mod(
        &tier(&t, "Uber").enemy_mods,
        "DamageTaken",
        EnemyTier::Uber.damage_taken_more(),
    );
    assert_eq!(m.mod_type, "MORE");
    assert!(!m.effective_only);
    for (id, rust_tier) in [
        ("None", EnemyTier::None),
        ("Boss", EnemyTier::Boss),
        ("Pinnacle", EnemyTier::Pinnacle),
    ] {
        assert!(
            !tier(&t, id)
                .enemy_mods
                .iter()
                .any(|m| m.name == "DamageTaken"),
            "{id} 档不应有 DamageTaken"
        );
        assert_eq!(rust_tier.damage_taken_more(), 0.0, "{id} Rust 准源亦为 0");
    }
}

/// Spot checks on vendor-only enemy mods (not previously implemented in
/// pobr, values hardcoded from vendor):
/// - `KnockbackDistanceOnSelf MORE -75` (ConfigOptions.lua L2002/L2044/L2084)
/// - `MinimumMovementSpeed BASE 20` (L2004/L2046/L2086)
/// - `PoiseThreshold MORE 213 "Map Boss"` (Boss tier only, L2006)
/// - `PoiseThreshold MORE 838 "Xesht"` (Pinnacle/Uber tiers, L2048/L2089)
#[test]
fn vendor_only_enemy_mods_sampled() {
    let t = load();
    for id in ["Boss", "Pinnacle", "Uber"] {
        let mods = &tier(&t, id).enemy_mods;
        let kb = find_mod(mods, "KnockbackDistanceOnSelf", -75.0);
        assert_eq!(kb.mod_type, "MORE");
        assert!(kb.effective_only);
        let mms = find_mod(mods, "MinimumMovementSpeed", 20.0);
        assert_eq!(mms.mod_type, "BASE");
        assert!(mms.effective_only);
    }
    let map_boss = find_mod(&tier(&t, "Boss").enemy_mods, "PoiseThreshold", 213.0);
    assert_eq!(map_boss.source_label, "Map Boss");
    for id in ["Pinnacle", "Uber"] {
        let xesht = find_mod(&tier(&t, id).enemy_mods, "PoiseThreshold", 838.0);
        assert_eq!(xesht.source_label, "Xesht", "{id} Xesht poise");
        assert!(
            !tier(&t, id).enemy_mods.iter().any(|m| m.value == 213.0),
            "{id} 不应有 Map Boss 213"
        );
    }
}

/// vendor-only player mods: the Boss/Pinnacle/Uber tiers all inject
/// `WarcryPower BASE 20` + `Multiplier:EnemyPower BASE 20`
/// (L2007-2008/L2049-2050/L2090-2091).
#[test]
fn vendor_only_player_mods_sampled() {
    let t = load();
    assert!(tier(&t, "None").player_mods.is_empty());
    for id in ["Boss", "Pinnacle", "Uber"] {
        let mods = &tier(&t, id).player_mods;
        assert_eq!(mods.len(), 2, "{id} player_mods 条数");
        for name in ["WarcryPower", "Multiplier:EnemyPower"] {
            let m = find_mod(mods, name, 20.0);
            assert_eq!(m.mod_type, "BASE", "{id} {name} 类型");
            assert_eq!(m.source_label, "Boss", "{id} {name} 来源标签");
            assert!(!m.effective_only, "{id} {name} 无 Effective 门控");
        }
    }
}

/// vendor-only chaos-damage divisor: None/Boss/Pinnacle = 2.5
/// (L1987/L2028/L2070), Uber = 4 (L2111).
#[test]
fn chaos_damage_divisor_sampled_from_vendor() {
    let t = load();
    for id in ["None", "Boss", "Pinnacle"] {
        assert_eq!(tier(&t, id).chaos_damage_div, 2.5, "{id} chaos div");
    }
    assert_eq!(tier(&t, "Uber").chaos_damage_div, 4.0);
}
