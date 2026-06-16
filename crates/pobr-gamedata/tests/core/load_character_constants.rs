//! `base/character_constants.json` 加载 + 搬迁不变式回归。
//!
//! 断言策略（架构文档 20 §1.1 搬迁不变式）：
//! - 有 pobr Rust 准源的 10 个值（`crates/pobr-core/src/character.rs` 常量），
//!   逐值断言与准源字面量相等——pobr-gamedata 不依赖 pobr-core，无法直接引用
//!   私有常量，故把准源值写死在测试里并注明常量名；准源若改值本测试即红。
//! - vendor-only 的 3 个值抽样断言，值写死并引用 vendor 文件:行号
//!   （commit `2df5a74`，见 `vendor/.pob2-version.txt`）。

use pobr_gamedata::{GameData, repo_data_root};

const VERSION: &str = "4.5.0.3.4";

fn game_data() -> GameData {
    GameData::new(repo_data_root().join(VERSION))
}

/// manifest v2 已把本域注册在 base 段。
#[test]
fn manifest_registers_character_constants_in_base() {
    let manifest = game_data().manifest().expect("manifest 可加载");
    assert!(
        manifest
            .domains
            .base
            .iter()
            .any(|d| d == "character_constants"),
        "manifest.domains.base 应包含 character_constants"
    );
}

/// 逐值断言 = pobr-core/src/character.rs 现有常量（搬迁不变式：JSON 必须与
/// 既有 Rust 数值相等；行为对齐 vendor 是后续独立 commit 的事）。
#[test]
fn values_match_pobr_core_character_rs_constants() {
    let c = game_data()
        .character_constants()
        .expect("character_constants 可加载");

    // 生命：character.rs BASE_LIFE_CONSTANT / LIFE_PER_LEVEL / LIFE_PER_STRENGTH
    assert_eq!(c.base_life_constant, 16.0);
    assert_eq!(c.life_per_level, 12.0);
    assert_eq!(c.life_per_strength, 2.0);

    // 魔力：character.rs BASE_MANA_CONSTANT / MANA_PER_LEVEL / MANA_PER_INTELLIGENCE
    assert_eq!(c.base_mana_constant, 30.0);
    assert_eq!(c.mana_per_level, 4.0);
    assert_eq!(c.mana_per_intelligence, 2.0);

    // 精准：character.rs BASE_ACCURACY_CONSTANT / ACCURACY_PER_LEVEL / ACCURACY_PER_DEXTERITY
    assert_eq!(c.base_accuracy_constant, -6.0);
    assert_eq!(c.accuracy_per_level, 6.0);
    assert_eq!(c.accuracy_per_dexterity, 6.0);

    // 闪避：character.rs BASE_EVASION
    assert_eq!(c.base_evasion, 7.0);
}

/// vendor-only 字段抽样断言（pobr Rust 无此值，源 vendor PoB2 commit 2df5a74）。
#[test]
fn vendor_only_per_level_attributes_match_vendor() {
    let c = game_data().character_constants().unwrap();

    // vendor/PathOfBuilding-PoE2/src/Data/Misc.lua:157 ["strength_per_level"] = 0
    assert_eq!(c.strength_per_level, 0.0);
    // vendor/PathOfBuilding-PoE2/src/Data/Misc.lua:158 ["dexterity_per_level"] = 0
    assert_eq!(c.dexterity_per_level, 0.0);
    // vendor/PathOfBuilding-PoE2/src/Data/Misc.lua:159 ["intelligence_per_level"] = 0
    assert_eq!(c.intelligence_per_level, 0.0);
}

/// 派生公式复算 oracle 实证值（character.rs 模块 doc 注：L99 Life base 1204 =
/// 12×99+16、Mana base 426 = 4×99+30；L1 Accuracy base 0 = 6×1−6）——确认本表
/// 数值喂入既有公式后输出不变。
#[test]
fn derived_formulas_reproduce_documented_oracle_values() {
    let c = game_data().character_constants().unwrap();

    let life_l99 = c.base_life_constant + c.life_per_level * 99.0;
    assert_eq!(life_l99, 1204.0, "L99 固有生命应为 1204");

    let mana_l99 = c.base_mana_constant + c.mana_per_level * 99.0;
    assert_eq!(mana_l99, 426.0, "L99 固有魔力应为 426");

    let accuracy_l1 = c.base_accuracy_constant + c.accuracy_per_level * 1.0;
    assert_eq!(accuracy_l1, 0.0, "L1 固有精准应为 0");
}

/// JSON 与 schema 往返一致（serde 字段无遗漏 / 无多余）。
#[test]
fn json_roundtrips_through_schema() {
    use pobr_data::catalog::character_constants::CharacterConstantsDef;

    let c = game_data().character_constants().unwrap();
    let text = serde_json::to_string(&c).unwrap();
    let back: CharacterConstantsDef = serde_json::from_str(&text).unwrap();
    assert_eq!(c, back);
}
