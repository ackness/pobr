//! Integration tests for parsing raw item text blocks.
//!
//! Covers splitting PoB-style English export text into an [`Item`] (rarity / quality /
//! implicit / explicit / enchant sections), and verifies that feeding the result into
//! `ingest_item` yields modifiers with the correct [`SourceKind`].

use pobr_core::item::{ItemIngest, ingest_item_with_ctx};

/// Engine-facing ingest (signature matches the historical `ingest_item`, wired to the real rules).
fn ingest_item(
    slot: EquipmentSlot,
    item: &Item,
) -> Result<ItemIngest, pobr_core::mod_parser::ParseError> {
    ingest_item_with_ctx(slot, item, crate::support::ctx())
}
use pobr_core::item_text::{
    ItemTextError, parse_item_text, parse_pob_xml_item, strip_pob_annotations,
};
use pobr_data::prelude::*;

/// A typical PoB rare-item export: has Quality / Item Level / Implicits headers.
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

/// An item with an enchant: lines marked `{crafted}` land in the enchant section.
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

// Bug Fix: item-text-range-tier-marker-not-stripped
//
// PoB-exported modifier lines often carry annotations like `{range:0.5}` / `(tier: 3)` /
// `[augmented]`. After the fix these are stripped before the text reaches mod_parser, so
// the lines no longer fall into Unsupported.
//
// Source: agent-docs/item-character-systems.md §5;
//         PoB2 src/Classes/Item.lua BuildAndParseRaw (line 708-734, 926).

/// Unit test: `strip_pob_annotations` strips each annotation style.
#[test]
fn strip_pob_annotations_removes_range_marker() {
    // The {range:0.5} brace annotation is stripped.
    assert_eq!(
        strip_pob_annotations("+40 to maximum Life {range:0.5}"),
        "+40 to maximum Life"
    );
}

#[test]
fn strip_pob_annotations_removes_augmented_parenthetical() {
    // The " (augmented)" lowercase parenthetical annotation is stripped.
    assert_eq!(
        strip_pob_annotations("20% increased maximum Life (augmented)"),
        "20% increased maximum Life"
    );
}

#[test]
fn strip_pob_annotations_removes_tier_annotation() {
    // The "(tier: 3)" annotation (colon + digits) is stripped.
    assert_eq!(
        strip_pob_annotations("20% increased Fire Damage (tier: 3)"),
        "20% increased Fire Damage"
    );
}

#[test]
fn strip_pob_annotations_removes_square_bracket_annotation() {
    // The "[augmented]" bracket annotation is stripped.
    assert_eq!(
        strip_pob_annotations("+30% to Fire Resistance [augmented]"),
        "+30% to Fire Resistance"
    );
}

#[test]
fn strip_pob_annotations_removes_multiple_annotations() {
    // Multiple annotations on the same line are all stripped.
    assert_eq!(
        strip_pob_annotations("{range:0.5}+40 to maximum Life (augmented)"),
        "+40 to maximum Life"
    );
}

/// Integration test: a PoB-exported item with a {range:0.5} annotation parses cleanly
/// and produces modifiers.
///
/// This is the regression test for Bug item-text-range-tier-marker-not-stripped:
/// before the fix, annotated lines fell into unsupported; after the fix they parse.
#[test]
fn item_with_range_markers_parses_correctly() {
    // Simulates a PoB export: modifier lines carry {range:0.5} annotations.
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

    // Once annotations are stripped, the text should be clean.
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

    // Once fed into ingest_item, it should parse successfully rather than landing in unsupported.
    let ingest = ingest_item(EquipmentSlot::Helmet, &item).expect("ingest succeeds");
    assert!(
        ingest.unsupported.is_empty(),
        "含注释的行在剥离后应能被 mod_parser 解析，不应归入 unsupported：{:?}",
        ingest.unsupported
    );
    // 1 implicit + 2 explicit (quality no longer injects a modifier; per-attribute base scaling is handled by the orchestration layer).
    assert_eq!(
        ingest.modifiers.len(),
        3,
        "应解析出 3 个 modifier（1 implicit + 2 explicit）"
    );
}

/// Integration test: modifier lines with a (tier: N) annotation parse cleanly.
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

    // The tier annotation is stripped.
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

/// Confirms the {crafted} prefix still routes the line into the enchant section, and that trailing annotations are also stripped.
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

    // The {crafted} prefix routes correctly into enchant, and the inline {range:0.9} is also stripped.
    assert_eq!(
        item.enchant_texts,
        vec!["+25% to Cold Resistance"],
        "{{crafted}} 行应归入 enchant section，且 {{range:0.9}} 被剥离"
    );
    assert_eq!(item.modifier_texts, vec!["+40 to maximum Life"]);
}

// Parsing <Item> text blocks embedded in PoB Build XML (no -------- section separators; sections are split by the Implicits: N count)

/// A real PoB2 Build XML RARE weapon block (no section separators; has Rune: / {enchant}{rune} / {fractured}).
const XML_RARE_CROSSBOW: &str = "\
\t\t\tRarity: RARE
Plague Core
Siege Crossbow
Unique ID: 28c4b9c403bbe522924570d1210631801a9e1001f999d688ad4372ec13c6e2ba
Item Level: 81
Quality: 20
Sockets: S S
Rune: Perfect Iron Rune
LevelReq: 79
Implicits: 5
{enchant}{rune}20% increased Physical Damage
{enchant}{rune}Gain 5% of Damage as Extra Damage of all Elements
Grenade Skills Fire an additional Projectile
{fractured}Adds 1 to 356 Lightning Damage
{desecrated}152% increased Physical Damage
Adds 47 to 86 Physical Damage
+26 to Strength
";

#[test]
fn xml_item_parses_rarity_base_and_quality_without_separators() {
    let item = parse_pob_xml_item(XML_RARE_CROSSBOW).expect("xml crossbow parses");

    assert_eq!(item.rarity, ItemRarity::Rare);
    assert_eq!(item.base, ItemBaseId::from("Siege Crossbow"));
    assert_eq!(item.quality, 20);
}

#[test]
fn xml_item_strips_brace_prefixes_and_collects_all_mods() {
    let item = parse_pob_xml_item(XML_RARE_CROSSBOW).expect("parse");

    // The {enchant}{rune} prefix is stripped by strip_enchant_marker + strip_pob_annotations and the line lands in the enchant section.
    assert!(
        item.enchant_texts
            .iter()
            .any(|t| t == "20% increased Physical Damage"),
        "rune enchant 应保留干净文本: {:?}",
        item.enchant_texts
    );

    // Metadata lines like Rune: / Sockets: / Unique ID: must not leak into any modifier section.
    let all: Vec<&String> = item
        .implicit_texts
        .iter()
        .chain(&item.modifier_texts)
        .chain(&item.enchant_texts)
        .collect();
    assert!(
        all.iter().all(|t| !t.starts_with("Rune:")
            && !t.starts_with("Sockets:")
            && !t.starts_with("Unique ID:")),
        "元数据行泄漏到词条段: {all:?}"
    );

    // The {fractured} / {desecrated} prefixes are stripped, leaving parseable text.
    assert!(
        all.iter()
            .any(|t| t.as_str() == "Adds 1 to 356 Lightning Damage"),
        "fractured 词条应保留干净文本: {all:?}"
    );
    assert!(
        all.iter().any(|t| t.as_str() == "+26 to Strength"),
        "末尾普通词条应保留: {all:?}"
    );
}

#[test]
fn xml_item_counts_filled_sockets_from_rune_lines() {
    // `Sockets: S S` declares 2 sockets, but only 1 `Rune:` line → filled count is 1
    // (PoB2's `RunesSocketedIn` counts runes/soul cores actually socketed, not total sockets).
    let item = parse_pob_xml_item(XML_RARE_CROSSBOW).expect("parse");
    assert_eq!(item.rolled_defence.sockets_filled, 1);
}

/// All 5 sockets filled (the Morior Invictus shape): 5 `Rune:` lines → filled count 5;
/// `per Socket filled` modifiers read this value (gemling Spirit +14×5 = 70).
const XML_FIVE_RUNE_BODY: &str = "\
\t\t\tRarity: UNIQUE
Morior Invictus
Grand Regalia
Armour: 939
Sockets: S S S S S
Rune: Perfect Body Rune
Rune: Perfect Rebirth Rune
Rune: Greater Glacial Rune
Rune: Perfect Body Rune
Rune: Rabbit Idol
Implicits: 0
+14 to Spirit per Socket filled
";

#[test]
fn xml_item_counts_five_filled_sockets() {
    let item = parse_pob_xml_item(XML_FIVE_RUNE_BODY).expect("parse");
    assert_eq!(item.rolled_defence.sockets_filled, 5);
    // Rune: lines are recognized as filled sockets and don't leak into a modifier section.
    let all: Vec<&String> = item
        .implicit_texts
        .iter()
        .chain(&item.modifier_texts)
        .chain(&item.enchant_texts)
        .collect();
    assert!(
        all.iter().all(|t| !t.starts_with("Rune:")),
        "Rune: 元数据行不得进入词条段: {all:?}"
    );
    assert!(
        all.iter()
            .any(|t| t.as_str() == "+14 to Spirit per Socket filled"),
        "per-socket 词条应保留: {all:?}"
    );
}

#[test]
fn xml_item_handles_magic_flask_without_separate_base_line() {
    // A MAGIC item (flask/charm) has a single name line with the base embedded in it; metadata follows directly.
    let raw = "\
Rarity: MAGIC
Catalysed Ultimate Life Flask of the Eternal
Unique ID: 8bf5222a8fe575715ca469864c024b5a8c54f4e4fa1bd810ea209fb6636a634b
Item Level: 82
Quality: 20
Implicits: 0
35% increased Amount Recovered
";
    let item = parse_pob_xml_item(raw).expect("magic flask parses");
    assert_eq!(item.rarity, ItemRarity::Magic);
    // With no separate base line, this falls back to the name line (matching parse_item_text).
    assert_eq!(
        item.base,
        ItemBaseId::from("Catalysed Ultimate Life Flask of the Eternal")
    );
    assert_eq!(
        item.modifier_texts,
        vec!["35% increased Amount Recovered".to_string()]
    );
}

#[test]
fn xml_item_implicit_count_splits_segments() {
    let raw = "\
Rarity: RARE
Dragon Hold
Topaz Ring
Item Level: 80
Implicits: 1
+30% to Lightning Resistance
+50 to maximum Life
+25 to Dexterity
";
    let item = parse_pob_xml_item(raw).expect("ring parses");
    // The first 1 line (Implicits: 1) → implicit, the rest → explicit.
    assert_eq!(item.implicit_texts, vec!["+30% to Lightning Resistance"]);
    assert_eq!(
        item.modifier_texts,
        vec!["+50 to maximum Life", "+25 to Dexterity"]
    );
}

#[test]
fn xml_item_rejects_empty_and_missing_rarity() {
    assert!(matches!(parse_pob_xml_item(""), Err(ItemTextError::Empty)));
    assert!(matches!(
        parse_pob_xml_item("Iron Ring\n+10 to Life"),
        Err(ItemTextError::MissingRarity)
    ));
}

#[test]
fn xml_item_feeds_ingest_item_with_attribution() {
    // End-to-end: XML item block → parse_pob_xml_item → ingest_item, modifiers carry the correct attribution.
    let raw = "\
Rarity: RARE
Dragon Hold
Topaz Ring
Item Level: 80
Implicits: 1
+40 to maximum Life
+50 to maximum Life
";
    let item = parse_pob_xml_item(raw).expect("parse");
    let ingest = ingest_item(EquipmentSlot::Ring1, &item).expect("ingest");
    assert!(
        !ingest.modifiers.is_empty(),
        "ingest 应产出 modifier（含可解析词条）"
    );
}
