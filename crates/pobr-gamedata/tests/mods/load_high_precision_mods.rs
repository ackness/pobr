//! `overlay/high_precision_mods.json` 加载测试。
//!
//! 本表当前**零消费方**（pobr 未实现 ScaleAddMod / MORE 精度例外分支，见
//! audits/rearchitecture-2026-06-10/10-mod-system.md Gap 6），值为 vendor
//! PoB2 `src/Modules/Data.lua:413-530` 的忠实转录（vendor commit `2df5a74`）；
//! 唯一有 pobr 准源的字段是 `more_default_round_decimals`
//! （= `pobr-core::mod_db::round_more` 固定的 2 位，逐值一致）。

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

/// 取某 mod 名 + mod type 的精度位数（缺条目即 panic）。
fn precision(def: &HighPrecisionModsDef, name: &str, mod_type: &str) -> u32 {
    *def.mods
        .get(name)
        .unwrap_or_else(|| panic!("存在 {name} 条目"))
        .get(mod_type)
        .unwrap_or_else(|| panic!("{name} 存在 {mod_type} 精度"))
}

/// 默认值逐值：`defaultHighPrecision = 1`（vendor `Data.lua:413`）；
/// MORE 默认取整 2 位（pobr 准源 `mod_db::round_more` = vendor
/// `ModList.lua:144` `round(modResult, 2)`，搬迁不变式）。
#[test]
fn default_precisions_match_sources() {
    let def = load();
    assert_eq!(def.default_high_precision, 1);
    assert_eq!(def.more_default_round_decimals, 2);
}

/// 例外表条目总数 = vendor `highPrecisionMods` 的 38 条（`Data.lua:415-530`）。
#[test]
fn exception_table_has_vendor_entry_count() {
    assert_eq!(load().mods.len(), 38);
}

/// vendor 条目抽样逐值：BASE 1 / BASE 2 / MORE 4 三档各取代表。
#[test]
fn vendor_entries_spot_checks() {
    let def = load();
    // BASE 精度 2：暴击 / 每秒回复百分比 / leech 族。
    assert_eq!(precision(&def, "CritChance", "BASE"), 2);
    assert_eq!(precision(&def, "SelfCritChance", "BASE"), 2);
    assert_eq!(precision(&def, "LifeRegenPercent", "BASE"), 2);
    assert_eq!(precision(&def, "DamageLifeLeech", "BASE"), 2);
    assert_eq!(precision(&def, "ChaosDamageEnergyShieldLeech", "BASE"), 2);
    // BASE 精度 1：flat 回复/损耗族。
    assert_eq!(precision(&def, "LifeRegen", "BASE"), 1);
    assert_eq!(precision(&def, "RageRegen", "BASE"), 1);
    assert_eq!(precision(&def, "EnergyShieldDegen", "BASE"), 1);
    // MORE 精度 4：支援宝石/保留倍率（仅有的两条 MORE 例外）。
    assert_eq!(precision(&def, "SupportManaMultiplier", "MORE"), 4);
    assert_eq!(precision(&def, "ReservationMultiplier", "MORE"), 4);
}

/// 结构合法性：mod type 仅 `BASE`/`MORE`，精度位数在 1..=4，
/// 且每个条目至少有一个 mod type（防转录走样）。
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
