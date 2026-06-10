//! `base/jewel_radii.json` 加载测试。
//!
//! 搬迁不变式校验：4 个具名档（Small/Medium/Large/Very Large）与距离乘数**逐值等于**
//! pobr 现有 Rust 准源 `crates/pobr-tree/src/radius_jewel.rs` 的常量；
//! vendor-only 部分（inner / colour / 8 个 Variable 档）按 vendor PoB2
//! `src/Modules/Data.lua:595-611` 写死期望值抽样断言。

use pobr_data::catalog::jewel_radii::{JewelRadiiDef, JewelRadiusBandDef};
use pobr_gamedata::{GameData, repo_data_root};
use pobr_tree::{
    JEWEL_RADIUS_LARGE, JEWEL_RADIUS_MEDIUM, JEWEL_RADIUS_SMALL, JEWEL_RADIUS_VERY_LARGE,
    JewelRadius, PASSIVE_TREE_JEWEL_DISTANCE_MULTIPLIER,
};

const VERSION: &str = "4.5.0.3.4";

fn load() -> JewelRadiiDef {
    GameData::new(repo_data_root().join(VERSION))
        .jewel_radii()
        .expect("jewel_radii 可加载")
}

/// 取 `0_1` 树版本中指定具名档（具名档 label 唯一）。
fn named_band<'a>(def: &'a JewelRadiiDef, label: &str) -> &'a JewelRadiusBandDef {
    def.tree_versions["0_1"]
        .iter()
        .find(|b| b.label == label)
        .unwrap_or_else(|| panic!("存在 {label} 档"))
}

/// 距离乘数逐值等于 pobr-tree 准源常量（= 1.2，源 GameConstants.dat）。
#[test]
fn distance_multiplier_matches_pobr_tree_constant() {
    let def = load();
    assert_eq!(
        def.distance_multiplier,
        PASSIVE_TREE_JEWEL_DISTANCE_MULTIPLIER
    );
    assert_eq!(def.distance_multiplier, 1.2);
}

/// 4 个具名档的 outer × 乘数逐值等于 pobr-tree 的 `JEWEL_RADIUS_*` 常量，
/// 且与 `JewelRadius::units()` 行为一致（搬迁不变式）。
#[test]
fn named_bands_match_pobr_tree_radius_constants() {
    let def = load();
    let m = def.distance_multiplier;
    let cases: [(&str, f64, JewelRadius); 4] = [
        ("Small", JEWEL_RADIUS_SMALL, JewelRadius::Small),
        ("Medium", JEWEL_RADIUS_MEDIUM, JewelRadius::Medium),
        ("Large", JEWEL_RADIUS_LARGE, JewelRadius::Large),
        (
            "Very Large",
            JEWEL_RADIUS_VERY_LARGE,
            JewelRadius::VeryLarge,
        ),
    ];
    for (label, expect_units, radius) in cases {
        let band = named_band(&def, label);
        assert_eq!(
            f64::from(band.outer) * m,
            expect_units,
            "{label} 档 outer×乘数应等于 pobr-tree 常量"
        );
        assert_eq!(f64::from(band.outer) * m, radius.units());
        // 具名档为实心圆，inner=0（vendor Data.lua:597-600 同为 0）。
        assert_eq!(band.inner, 0, "{label} 档 inner 应为 0");
    }
}

/// 具名档 outer 基础值抽样（vendor `Modules/Data.lua:597-600`；
/// 与 pobr-tree 常量定义里的 1000/1150/1300/1500 一致）。
#[test]
fn named_band_outer_base_values() {
    let def = load();
    assert_eq!(named_band(&def, "Small").outer, 1000);
    assert_eq!(named_band(&def, "Medium").outer, 1150);
    assert_eq!(named_band(&def, "Large").outer, 1300);
    assert_eq!(named_band(&def, "Very Large").outer, 1500);
}

/// vendor-only：8 个 Variable 环形档的 inner/outer 逐值等于
/// vendor `Modules/Data.lua:602-609`（pobr 现无 Rust 准源，期望值写死）。
#[test]
fn variable_bands_match_vendor_data_lua() {
    let def = load();
    let bands = &def.tree_versions["0_1"];
    assert_eq!(bands.len(), 12, "0_1 共 4 具名档 + 8 Variable 档");

    let variables: Vec<(u32, u32)> = bands
        .iter()
        .filter(|b| b.label == "Variable")
        .map(|b| (b.inner, b.outer))
        .collect();
    assert_eq!(
        variables,
        vec![
            (650, 950),
            (800, 1100),
            (950, 1250),
            (1100, 1400),
            (1250, 1550),
            (1400, 1700),
            (1650, 1950),
            (1800, 2100),
        ],
        "Variable 档 inner/outer 应与 vendor Data.lua:602-609 逐值一致（保持书写顺序）"
    );
    // Variable 档环宽恒为 300（vendor 表的结构性质，防手误改值）。
    assert!(variables.iter().all(|(inner, outer)| outer - inner == 300));
}

/// vendor-only：颜色码抽样（`Modules/Data.lua:597` Small `^xBB6600`、
/// `:609` 最末 Variable 档 `^x0099FF`）。
#[test]
fn colour_codes_sampled_from_vendor() {
    let def = load();
    let bands = &def.tree_versions["0_1"];
    assert_eq!(named_band(&def, "Small").colour, "^xBB6600");
    assert_eq!(named_band(&def, "Very Large").colour, "^xC100FF");
    assert_eq!(bands.last().unwrap().colour, "^x0099FF");
}
