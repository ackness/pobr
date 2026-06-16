//! `base/monster_scaling.json` 加载测试。
//!
//! 搬迁不变式校验：有 Rust 准源（`pobr_data::monster`）的七张表
//! **循环逐值断言全部 100 级**；vendor-only 的 ally 两张表按
//! vendor `Misc.lua` 行号做抽样断言（值写死）。

use pobr_data::monster::{
    MONSTER_ACCURACY_TABLE, MONSTER_AILMENT_THRESHOLD_TABLE, MONSTER_ARMOUR_TABLE,
    MONSTER_DAMAGE_TABLE, MONSTER_EVASION_TABLE, MONSTER_LIFE_TABLE, MONSTER_POISE_THRESHOLD_TABLE,
    MONSTER_TABLE_LEN,
};
use pobr_gamedata::{GameData, repo_data_root};

const VERSION: &str = "4.5.0.3.4";

fn game_data() -> GameData {
    GameData::new(repo_data_root().join(VERSION))
}

/// 九张表均应恒为 100 项（等级 1..=100）。
#[test]
fn all_tables_have_100_levels() {
    let t = game_data()
        .monster_scaling()
        .expect("monster_scaling 可加载");
    assert_eq!(t.accuracy.len(), MONSTER_TABLE_LEN, "accuracy 长度");
    assert_eq!(t.evasion.len(), MONSTER_TABLE_LEN, "evasion 长度");
    assert_eq!(t.armour.len(), MONSTER_TABLE_LEN, "armour 长度");
    assert_eq!(t.life.len(), MONSTER_TABLE_LEN, "life 长度");
    assert_eq!(t.ally_life.len(), MONSTER_TABLE_LEN, "ally_life 长度");
    assert_eq!(t.damage.len(), MONSTER_TABLE_LEN, "damage 长度");
    assert_eq!(t.ally_damage.len(), MONSTER_TABLE_LEN, "ally_damage 长度");
    assert_eq!(
        t.ailment_threshold.len(),
        MONSTER_TABLE_LEN,
        "ailment_threshold 长度"
    );
    assert_eq!(
        t.poise_threshold.len(),
        MONSTER_TABLE_LEN,
        "poise_threshold 长度"
    );
}

/// 搬迁不变式：六张整型表逐值等于 `pobr_data::monster` 现有 Rust 表（全 100 级循环断言）。
#[test]
fn integer_tables_match_rust_source_at_every_level() {
    let t = game_data().monster_scaling().unwrap();
    for lv in 1..=MONSTER_TABLE_LEN {
        let i = lv - 1;
        assert_eq!(
            t.accuracy[i], MONSTER_ACCURACY_TABLE[i],
            "lv{lv} accuracy 与 MONSTER_ACCURACY_TABLE 不等"
        );
        assert_eq!(
            t.evasion[i], MONSTER_EVASION_TABLE[i],
            "lv{lv} evasion 与 MONSTER_EVASION_TABLE 不等"
        );
        assert_eq!(
            t.armour[i], MONSTER_ARMOUR_TABLE[i],
            "lv{lv} armour 与 MONSTER_ARMOUR_TABLE 不等"
        );
        assert_eq!(
            t.life[i], MONSTER_LIFE_TABLE[i],
            "lv{lv} life 与 MONSTER_LIFE_TABLE 不等"
        );
        assert_eq!(
            t.ailment_threshold[i], MONSTER_AILMENT_THRESHOLD_TABLE[i],
            "lv{lv} ailment_threshold 与 MONSTER_AILMENT_THRESHOLD_TABLE 不等"
        );
        assert_eq!(
            t.poise_threshold[i], MONSTER_POISE_THRESHOLD_TABLE[i],
            "lv{lv} poise_threshold 与 MONSTER_POISE_THRESHOLD_TABLE 不等"
        );
    }
}

/// 搬迁不变式：damage 表逐值（位级）等于 Rust 表（全 100 级循环断言）。
///
/// Rust 表与 JSON 均为 2 位小数口径（vendor f32 噪声值 round 到 2 位），
/// 二者应位级相等，故用 `==` 而非容差比较。
#[test]
fn damage_table_matches_rust_source_at_every_level() {
    let t = game_data().monster_scaling().unwrap();
    for lv in 1..=MONSTER_TABLE_LEN {
        let i = lv - 1;
        assert_eq!(
            t.damage[i], MONSTER_DAMAGE_TABLE[i],
            "lv{lv} damage 与 MONSTER_DAMAGE_TABLE 不等"
        );
    }
}

/// vendor-only：ally_life 抽样断言（pobr 无 Rust 准源）。
///
/// 来源：`vendor/PathOfBuilding-PoE2/src/Data/Misc.lua` L8
/// `data.monsterAllyLifeTable`（Lua 1-indexed = 怪物等级）。
#[test]
fn ally_life_spot_checks_against_vendor() {
    let t = game_data().monster_scaling().unwrap();
    // Misc.lua L8：[1]=51, [10]=382, [20]=886, [50]=3709
    assert_eq!(t.ally_life[0], 51, "lv1 ally_life");
    assert_eq!(t.ally_life[9], 382, "lv10 ally_life");
    assert_eq!(t.ally_life[19], 886, "lv20 ally_life");
    assert_eq!(t.ally_life[49], 3709, "lv50 ally_life");
    // Misc.lua L8：[65]=6282, [85]=11708, [100]=17980
    assert_eq!(t.ally_life[64], 6282, "lv65 ally_life");
    assert_eq!(t.ally_life[84], 11708, "lv85 ally_life");
    assert_eq!(t.ally_life[99], 17980, "lv100 ally_life");
}

/// vendor-only：ally_damage 抽样断言（pobr 无 Rust 准源；2 位小数口径）。
///
/// 来源：`vendor/PathOfBuilding-PoE2/src/Data/Misc.lua` L10
/// `data.monsterAllyDamageTable`，f32 噪声值 round 到 2 位小数
/// （如 [1]=3.1099998950958 → 3.11），与 `damage` 表口径一致。
#[test]
fn ally_damage_spot_checks_against_vendor() {
    let t = game_data().monster_scaling().unwrap();
    // Misc.lua L10：[1]≈3.11, [10]≈18.73, [20]≈50.39, [50]≈367.27
    assert_eq!(t.ally_damage[0], 3.11, "lv1 ally_damage");
    assert_eq!(t.ally_damage[9], 18.73, "lv10 ally_damage");
    assert_eq!(t.ally_damage[19], 50.39, "lv20 ally_damage");
    assert_eq!(t.ally_damage[49], 367.27, "lv50 ally_damage");
    // Misc.lua L10：[65]≈828.94, [85]≈2271.56, [100]≈4661.93
    assert_eq!(t.ally_damage[64], 828.94, "lv65 ally_damage");
    assert_eq!(t.ally_damage[84], 2271.56, "lv85 ally_damage");
    assert_eq!(t.ally_damage[99], 4661.93, "lv100 ally_damage");
}

/// ally 表单调性：召唤物生命/伤害随等级严格递增（结构合理性兜底）。
#[test]
fn ally_tables_strictly_increasing() {
    let t = game_data().monster_scaling().unwrap();
    for lv in 2..=MONSTER_TABLE_LEN {
        let i = lv - 1;
        assert!(
            t.ally_life[i] > t.ally_life[i - 1],
            "ally_life lv{} -> lv{lv} 应递增",
            lv - 1
        );
        assert!(
            t.ally_damage[i] > t.ally_damage[i - 1],
            "ally_damage lv{} -> lv{lv} 应递增",
            lv - 1
        );
    }
}

/// hiddenDamageFixup 派生公式可由本表 + SpectreBeastDamageFixup 复算
/// （PoB2 `CalcActiveSkill.lua:907`）：
/// `fixup = round(allyDamage[lv] / damage[lv] * 1.25, 2) - 1`。
/// 此处验证派生输入齐备且结果有限（具体常量归 game_constants 域，消费接线属后续工作）。
#[test]
fn hidden_damage_fixup_inputs_are_derivable() {
    let t = game_data().monster_scaling().unwrap();
    // SpectreBeastDamageFixup = 1.25（vendor Modules/Data.lua L249，归 game_constants 域）
    const SPECTRE_BEAST_DAMAGE_FIXUP: f64 = 1.25;
    for lv in 1..=MONSTER_TABLE_LEN {
        let i = lv - 1;
        let ratio = t.ally_damage[i] / t.damage[i];
        let fixup = (ratio * SPECTRE_BEAST_DAMAGE_FIXUP * 100.0).round() / 100.0 - 1.0;
        assert!(
            fixup.is_finite(),
            "lv{lv} hiddenDamageFixup 派生输入应有限，得 {fixup}"
        );
    }
    // 抽样：lv100 ratio = 4661.93 / 584.05 ≈ 7.982，fixup = round(9.98, 2) - 1 ≈ 8.98
    let lv100_fixup =
        (t.ally_damage[99] / t.damage[99] * SPECTRE_BEAST_DAMAGE_FIXUP * 100.0).round() / 100.0
            - 1.0;
    assert!(
        (lv100_fixup - 8.98).abs() < 0.01,
        "lv100 hiddenDamageFixup ≈ 8.98，得 {lv100_fixup}"
    );
}
