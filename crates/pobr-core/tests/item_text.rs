//! raw 物品文本块解析的集成测试。
//!
//! 覆盖 PoB 风格英文导出文本 → [`Item`] 的分段切分（rarity / quality /
//! implicit / explicit / enchant），并验证产物喂给 `ingest_item` 后能得到
//! 带正确 [`SourceKind`] 的 modifier。

use pobr_core::item::ingest_item;
use pobr_core::item_text::{ItemTextError, parse_item_text, strip_pob_annotations};
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

// ── Bug Fix: item-text-range-tier-marker-not-stripped ──────────────────────
//
// PoB 导出的词条行常带 `{range:0.5}` / `(tier: 3)` / `[augmented]` 等元注释。
// 修复后这些注释在喂给 mod_parser 前被剥离，解析不再归入 Unsupported。
//
// 出处：agent-docs/item-character-systems.md §5；
//       PoB2 src/Classes/Item.lua BuildAndParseRaw (line 708-734, 926)。

/// 单元测试：`strip_pob_annotations` 剥离各类注释。
#[test]
fn strip_pob_annotations_removes_range_marker() {
    // {range:0.5} 花括号注释被剥离。
    assert_eq!(
        strip_pob_annotations("+40 to maximum Life {range:0.5}"),
        "+40 to maximum Life"
    );
}

#[test]
fn strip_pob_annotations_removes_augmented_parenthetical() {
    // " (augmented)" 全小写括号注释被剥离。
    assert_eq!(
        strip_pob_annotations("20% increased maximum Life (augmented)"),
        "20% increased maximum Life"
    );
}

#[test]
fn strip_pob_annotations_removes_tier_annotation() {
    // "(tier: 3)" 注释（含冒号/数字）被剥离。
    assert_eq!(
        strip_pob_annotations("20% increased Fire Damage (tier: 3)"),
        "20% increased Fire Damage"
    );
}

#[test]
fn strip_pob_annotations_removes_square_bracket_annotation() {
    // "[augmented]" 方括号注释被剥离。
    assert_eq!(
        strip_pob_annotations("+30% to Fire Resistance [augmented]"),
        "+30% to Fire Resistance"
    );
}

#[test]
fn strip_pob_annotations_removes_multiple_annotations() {
    // 同一行多个注释同时剥离。
    assert_eq!(
        strip_pob_annotations("{range:0.5}+40 to maximum Life (augmented)"),
        "+40 to maximum Life"
    );
}

/// 集成测试：带 {range:0.5} 注释的 PoB 导出物品能正常解析并产出 modifier。
///
/// 这是 Bug item-text-range-tier-marker-not-stripped 的回归测试：
/// 修复前，带注释的行会归入 unsupported；修复后，解析成功。
#[test]
fn item_with_range_markers_parses_correctly() {
    // 模拟 PoB 导出：词条行带 {range:0.5} 标注。
    let raw = "\
Rarity: RARE
Storm Visage
Lion Pelt
--------
Quality: +20% (augmented)
--------
Item Level: 84
--------
Implicits: 1
+30% to Fire Resistance {range:0.8}
--------
+40 to maximum Life {range:0.5}
20% increased maximum Life (augmented)
--------
";
    let item = parse_item_text(raw).expect("item with range markers parses");

    // 注释被剥离后文本应是干净的。
    assert_eq!(
        item.implicit_texts,
        vec!["+30% to Fire Resistance"],
        "implicit 行的 {{range:0.8}} 应被剥离"
    );
    assert_eq!(
        item.modifier_texts,
        vec!["+40 to maximum Life", "20% increased maximum Life"],
        "explicit 行的 {{range:0.5}} 和 (augmented) 应被剥离"
    );

    // 喂给 ingest_item 后应该能成功解析，不归入 unsupported。
    let ingest = ingest_item(EquipmentSlot::Helmet, &item).expect("ingest succeeds");
    assert!(
        ingest.unsupported.is_empty(),
        "含注释的行在剥离后应能被 mod_parser 解析，不应归入 unsupported：{:?}",
        ingest.unsupported
    );
    // 1 implicit + 2 explicit + 1 quality modifier（quality=20 护甲注入 LocalDefencesMore）。
    assert_eq!(
        ingest.modifiers.len(),
        4,
        "应解析出 4 个 modifier（1 implicit + 2 explicit + 1 quality）"
    );
}

/// 集成测试：带 (tier: N) 注释的词条行能正常解析。
#[test]
fn item_with_tier_annotation_parses_correctly() {
    let raw = "\
Rarity: RARE
Ruin Crown
Rusted Helmet
--------
Item Level: 75
--------
Implicits: 0
--------
20% increased Fire Damage (tier: 3)
+30% to Cold Resistance (tier: 5)
--------
";
    let item = parse_item_text(raw).expect("item with tier annotations parses");

    // tier 注释被剥离。
    assert_eq!(
        item.modifier_texts,
        vec!["20% increased Fire Damage", "+30% to Cold Resistance"],
        "tier 注释应被剥离"
    );

    let ingest = ingest_item(EquipmentSlot::Helmet, &item).expect("ingest succeeds");
    assert!(
        ingest.unsupported.is_empty(),
        "tier 注释剥离后应能解析：{:?}",
        ingest.unsupported
    );
}

/// 确认 {crafted} 前缀仍然正确触发 enchant section 归类，且后续注释也被剥离。
#[test]
fn crafted_prefix_still_triggers_enchant_section_after_annotation_strip() {
    let raw = "\
Rarity: MAGIC
Iron Boots
--------
Item Level: 60
--------
{crafted}+25% to Cold Resistance {range:0.9}
--------
+40 to maximum Life
--------
";
    let item = parse_item_text(raw).expect("crafted item parses");

    // {crafted} 前缀正确归入 enchant，且行内 {range:0.9} 也被剥离。
    assert_eq!(
        item.enchant_texts,
        vec!["+25% to Cold Resistance"],
        "{{crafted}} 行应归入 enchant section，且 {{range:0.9}} 被剥离"
    );
    assert_eq!(item.modifier_texts, vec!["+40 to maximum Life"]);
}
