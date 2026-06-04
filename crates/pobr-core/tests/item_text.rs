//! raw 物品文本块解析的集成测试。
//!
//! 覆盖 PoB 风格英文导出文本 → [`Item`] 的分段切分（rarity / quality /
//! implicit / explicit / enchant），并验证产物喂给 `ingest_item` 后能得到
//! 带正确 [`SourceKind`] 的 modifier。

use pobr_core::item::ingest_item;
use pobr_core::item_text::{ItemTextError, parse_item_text};
use pobr_data::prelude::*;

/// 典型 PoB 稀有物品导出：含 Quality / Item Level / Implicits 头。
const RARE_HELMET: &str = "\
Rarity: RARE
Hate Cowl
Lion Pelt
--------
Quality: +20% (augmented)
--------
Armour: 360
--------
Requirements:
Level: 70
--------
Item Level: 84
--------
Implicits: 1
+30% to Fire Resistance
--------
+40 to maximum Life
20% increased maximum Life
--------
";

#[test]
fn parses_rarity_base_and_quality() {
    let item = parse_item_text(RARE_HELMET).expect("rare helmet parses");

    assert_eq!(item.rarity, ItemRarity::Rare);
    assert_eq!(item.base, ItemBaseId::from("Lion Pelt"));
    assert_eq!(item.quality, 20);
}

#[test]
fn splits_implicit_and_explicit_sections() {
    let item = parse_item_text(RARE_HELMET).expect("rare helmet parses");

    assert_eq!(item.implicit_texts, vec!["+30% to Fire Resistance"]);
    assert_eq!(
        item.modifier_texts,
        vec!["+40 to maximum Life", "20% increased maximum Life"]
    );
    assert!(item.enchant_texts.is_empty());
}

#[test]
fn parsed_item_feeds_ingest_item_with_correct_sources() {
    let item = parse_item_text(RARE_HELMET).expect("rare helmet parses");
    let ingest = ingest_item(EquipmentSlot::Helmet, &item).expect("ingest succeeds");

    let kind_of = |name: &str| {
        ingest
            .modifiers
            .iter()
            .find(|m| m.name == ModName::from(name))
            .unwrap_or_else(|| panic!("modifier {name} present"))
            .origin
            .as_ref()
            .expect("origin present")
            .source_id
            .kind
            .clone()
    };

    assert_eq!(kind_of("FireResistance"), SourceKind::ItemImplicit);
    assert_eq!(kind_of("MaximumLife"), SourceKind::ItemAffix);
}

/// 带附魔的物品：`{crafted}` 标记的行落入 enchant 段。
const ENCHANTED_BOOTS: &str = "\
Rarity: MAGIC
Goathide Boots
--------
Item Level: 60
--------
{crafted}+25% to Cold Resistance
--------
Implicits: 0
--------
+40 to maximum Life
--------
";

#[test]
fn enchant_lines_land_in_enchant_section() {
    let item = parse_item_text(ENCHANTED_BOOTS).expect("enchanted boots parse");

    assert_eq!(item.rarity, ItemRarity::Magic);
    assert_eq!(item.enchant_texts, vec!["+25% to Cold Resistance"]);
    assert_eq!(item.modifier_texts, vec!["+40 to maximum Life"]);
    assert!(item.implicit_texts.is_empty());

    let ingest = ingest_item(EquipmentSlot::Boots, &item).expect("ingest succeeds");
    let cold = ingest
        .modifiers
        .iter()
        .find(|m| m.name == ModName::from("ColdResistance"))
        .expect("cold resistance present");
    assert_eq!(
        cold.origin.as_ref().unwrap().source_id.kind,
        SourceKind::ItemEnchant
    );
}

#[test]
fn missing_rarity_is_structural_error() {
    let raw = "Just Some Name\n--------\n+10 to maximum Life\n";
    let err = parse_item_text(raw).unwrap_err();
    assert!(matches!(err, ItemTextError::MissingRarity));
}

#[test]
fn empty_input_is_structural_error() {
    let err = parse_item_text("   \n  \n").unwrap_err();
    assert!(matches!(err, ItemTextError::Empty));
}

#[test]
fn normal_item_without_quality_defaults_to_zero() {
    let raw = "\
Rarity: NORMAL
Iron Greaves
--------
Item Level: 1
--------
";
    let item = parse_item_text(raw).expect("normal item parses");
    assert_eq!(item.rarity, ItemRarity::Normal);
    assert_eq!(item.base, ItemBaseId::from("Iron Greaves"));
    assert_eq!(item.quality, 0);
    assert!(item.modifier_texts.is_empty());
}
