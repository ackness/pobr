use pobr_core::calc::{CalculationSession, MinimalInput};
use pobr_core::item::{ItemIngest, ingest_item};
use pobr_core::{CalcConfig, ModDb};
use pobr_data::prelude::*;

fn helmet(texts: &[&str]) -> Item {
    Item {
        base: ItemBaseId::from("Helmet"),
        rarity: ItemRarity::Rare,
        quality: 20,
        implicit_texts: Vec::new(),
        modifier_texts: texts.iter().map(|t| (*t).to_string()).collect(),
        enchant_texts: Vec::new(),
        rolled_defence: RolledDefence::default(),
        parsed_stats: Vec::new(),
    }
}

#[test]
fn ingest_item_parses_texts_into_modifiers_with_item_affix_source() {
    let item = helmet(&["+40 to maximum Life", "+30% to Fire Resistance"]);

    let ItemIngest {
        modifiers,
        unsupported,
    } = ingest_item(EquipmentSlot::Helmet, &item).unwrap();

    assert!(unsupported.is_empty());
    // 品质不再注入 modifier（逐属性 base 缩放在编排层处理），仅 2 个 explicit 词条。
    assert_eq!(modifiers.len(), 2);

    // 仅检查 explicit 词条（SourceKind::ItemAffix）的归因字段。
    let explicit_mods: Vec<_> = modifiers
        .iter()
        .filter(|m| {
            m.origin
                .as_ref()
                .map(|o| o.source_id.kind == SourceKind::ItemAffix)
                .unwrap_or(false)
        })
        .collect();
    assert_eq!(explicit_mods.len(), 2);

    for modifier in &explicit_mods {
        let origin = modifier
            .origin
            .as_ref()
            .expect("item modifier carries an origin");
        // explicit 词条归因到 ItemAffix，SourceId 含槽位与 section 后缀。
        assert_eq!(origin.source_id.kind, SourceKind::ItemAffix);
        assert_eq!(origin.source_id.id, "item.helmet.explicit");
        assert_eq!(origin.slot.as_deref(), Some("helmet"));
        // 原始词条文本必须保留，以便 breakdown 展示与 PoB 对比。
        assert!(origin.raw_text.is_some());
    }

    // stat_id / mod_type 由 with_origin 从 modifier 回填。
    let life = modifiers
        .iter()
        .find(|m| m.name == ModName::from("MaximumLife"))
        .unwrap();
    assert_eq!(
        life.origin.as_ref().unwrap().stat_id,
        Some(ModName::from("MaximumLife"))
    );
    assert_eq!(life.origin.as_ref().unwrap().mod_type, Some(ModType::Base));

    // 品质不再注入任何 ItemQuality modifier。
    let quality_mod = modifiers.iter().find(|m| {
        m.origin
            .as_ref()
            .map(|o| o.source_id.kind == SourceKind::ItemQuality)
            .unwrap_or(false)
    });
    assert!(
        quality_mod.is_none(),
        "品质不再作为 modifier 注入（逐属性 base 缩放在编排层处理）"
    );
}

#[test]
fn ingest_item_distinguishes_implicit_explicit_enchant_sections() {
    // 三个 section 使用不同 stat，便于唯一定位各自的 origin。
    let item = Item {
        base: ItemBaseId::from("Helmet"),
        rarity: ItemRarity::Rare,
        quality: 0,
        implicit_texts: vec!["+30% to Fire Resistance".to_string()],
        modifier_texts: vec!["+40 to maximum Life".to_string()],
        enchant_texts: vec!["+25% to Cold Resistance".to_string()],
        rolled_defence: RolledDefence::default(),
        parsed_stats: Vec::new(),
    };

    let ingest = ingest_item(EquipmentSlot::Helmet, &item).unwrap();
    assert!(ingest.unsupported.is_empty());

    let origin_of = |name: &str| {
        ingest
            .modifiers
            .iter()
            .find(|m| m.name == ModName::from(name))
            .unwrap_or_else(|| panic!("modifier {name} present"))
            .origin
            .clone()
            .expect("origin present")
    };

    let implicit = origin_of("FireResistance");
    assert_eq!(implicit.source_id.kind, SourceKind::ItemImplicit);
    assert_eq!(implicit.source_id.id, "item.helmet.implicit");
    assert_eq!(implicit.slot.as_deref(), Some("helmet"));

    let explicit = origin_of("MaximumLife");
    assert_eq!(explicit.source_id.kind, SourceKind::ItemAffix);
    assert_eq!(explicit.source_id.id, "item.helmet.explicit");
    assert_eq!(explicit.slot.as_deref(), Some("helmet"));

    let enchant = origin_of("ColdResistance");
    assert_eq!(enchant.source_id.kind, SourceKind::ItemEnchant);
    assert_eq!(enchant.source_id.id, "item.helmet.enchant");
    assert_eq!(enchant.slot.as_deref(), Some("helmet"));
}

#[test]
fn ingest_item_collects_unsupported_texts_across_sections() {
    let item = Item {
        base: ItemBaseId::from("Helmet"),
        rarity: ItemRarity::Rare,
        quality: 0,
        implicit_texts: vec!["split".to_string()],
        modifier_texts: vec!["+40 to maximum Life".to_string(), "mirrored".to_string()],
        enchant_texts: Vec::new(),
        rolled_defence: RolledDefence::default(),
        parsed_stats: Vec::new(),
    };

    let ingest = ingest_item(EquipmentSlot::Helmet, &item).unwrap();

    assert_eq!(ingest.modifiers.len(), 1);
    // 各 section 中无法解析的行都被收集进 unsupported。
    assert_eq!(ingest.unsupported.len(), 2);
    assert!(ingest.unsupported.contains(&"split".to_string()));
    assert!(ingest.unsupported.contains(&"mirrored".to_string()));
}

#[test]
fn ingested_item_modifiers_attribute_back_to_slot() {
    let ring = Item {
        base: ItemBaseId::from("Iron Ring"),
        rarity: ItemRarity::Rare,
        quality: 0,
        implicit_texts: Vec::new(),
        modifier_texts: vec!["+25 to maximum Life".to_string()],
        enchant_texts: Vec::new(),
        rolled_defence: RolledDefence::default(),
        parsed_stats: Vec::new(),
    };

    let ingest = ingest_item(EquipmentSlot::Ring1, &ring).unwrap();

    let mut db = ModDb::new();
    db.add_list(ingest.modifiers);
    let cfg = CalcConfig::new();

    let contributions = db.contributions(ModType::Base, &cfg, &[ModName::from("MaximumLife")]);
    assert_eq!(contributions.len(), 1);
    let origin = contributions[0]
        .origin
        .as_ref()
        .expect("contribution carries the item origin");
    assert_eq!(origin.source_id.kind, SourceKind::ItemAffix);
    assert_eq!(origin.source_id.id, "item.ring1.explicit");
    assert_eq!(origin.slot.as_deref(), Some("ring1"));
}

#[test]
fn ingested_implicit_attributes_to_item_implicit_source() {
    let amulet = Item {
        base: ItemBaseId::from("Amber Amulet"),
        rarity: ItemRarity::Rare,
        quality: 0,
        implicit_texts: vec!["+25 to maximum Life".to_string()],
        modifier_texts: Vec::new(),
        enchant_texts: Vec::new(),
        rolled_defence: RolledDefence::default(),
        parsed_stats: Vec::new(),
    };

    let ingest = ingest_item(EquipmentSlot::Amulet, &amulet).unwrap();

    let mut db = ModDb::new();
    db.add_list(ingest.modifiers);
    let cfg = CalcConfig::new();

    let contributions = db.contributions(ModType::Base, &cfg, &[ModName::from("MaximumLife")]);
    assert_eq!(contributions.len(), 1);
    let origin = contributions[0].origin.as_ref().unwrap();
    assert_eq!(origin.source_id.kind, SourceKind::ItemImplicit);
    assert_eq!(origin.source_id.id, "item.amulet.implicit");
    assert_eq!(origin.slot.as_deref(), Some("amulet"));
}

#[test]
fn session_add_item_feeds_minimal_calc() {
    let input = MinimalInput {
        base_life: 100.0,
        ..MinimalInput::default()
    };
    let mut session = CalculationSession::new(input);

    let body = Item {
        base: ItemBaseId::from("Plate Vest"),
        rarity: ItemRarity::Rare,
        quality: 0,
        implicit_texts: Vec::new(),
        modifier_texts: vec![
            "+40 to maximum Life".to_string(),
            "20% increased maximum Life".to_string(),
            "mirrored".to_string(),
        ],
        enchant_texts: Vec::new(),
        rolled_defence: RolledDefence::default(),
        parsed_stats: Vec::new(),
    };

    session.add_item(EquipmentSlot::BodyArmour, &body).unwrap();
    let output = session.perform_minimal();

    // (100 base + 40 item) * (1 + 20/100) = 168。
    assert_eq!(output.life, 168.0);
    // 无法解析的词条仍被保留。
    assert_eq!(session.unsupported_modifier_texts(), ["mirrored"]);
}

// ── 品质不作为 More modifier 注入（finding 02-05 修正）─────────────────────
//
// PoB2 的物品品质是**逐属性 base 缩放**（武器仅物理；护甲 armour/evasion/ES 各自
// 独立 × (1 + quality/100)），不是一个全局 `more` modifier。把品质作为
// `LocalPhysicalDamageMore` / `LocalDefencesMore` 全局 More 注入会错误地波及全局
// 伤害 / 全部防御。实际缩放由编排层 `pobr-build::calc_orchestrator`（item_rolled_defence
// 与武器 physical_min/max）逐件、逐属性处理，与 PoB2 对齐。故 ingest 不注入品质 modifier。
//
// 出处：PoB2 src/Classes/Item.lua BuildModListForSlotNum 1751-1756（武器）、
//       1812-1819（护甲，逐属性独立缩放）。

/// 辅助：断言一件物品 ingest 后**不含**任何 `ItemQuality` 归因的 modifier。
fn assert_no_quality_modifier(slot: EquipmentSlot, item: &Item) {
    let ingest = ingest_item(slot, item).unwrap();
    let quality_mod = ingest.modifiers.iter().find(|m| {
        m.origin
            .as_ref()
            .map(|o| o.source_id.kind == SourceKind::ItemQuality)
            .unwrap_or(false)
    });
    assert!(
        quality_mod.is_none(),
        "{} 品质不应作为 modifier 注入（逐属性 base 缩放在编排层处理）",
        slot.id()
    );
}

fn bare_item(base: &str, slot_quality: u8) -> Item {
    Item {
        base: ItemBaseId::from(base),
        rarity: ItemRarity::Rare,
        quality: slot_quality,
        implicit_texts: Vec::new(),
        modifier_texts: Vec::new(),
        enchant_texts: Vec::new(),
        rolled_defence: RolledDefence::default(),
        parsed_stats: Vec::new(),
    }
}

#[test]
fn armour_quality_does_not_inject_modifier() {
    assert_no_quality_modifier(EquipmentSlot::Helmet, &bare_item("Iron Helmet", 20));
    assert_no_quality_modifier(EquipmentSlot::BodyArmour, &bare_item("Plate Vest", 10));
}

#[test]
fn weapon_quality_does_not_inject_modifier() {
    assert_no_quality_modifier(EquipmentSlot::Weapon1, &bare_item("Iron Sword", 15));
}

#[test]
fn accessory_quality_does_not_inject_modifier() {
    // 戒指 / 腰带品质走催化剂机制（getCatalystScalar），尚未建模——见下方 defer 说明。
    assert_no_quality_modifier(EquipmentSlot::Ring1, &bare_item("Iron Ring", 20));
    assert_no_quality_modifier(EquipmentSlot::Belt, &bare_item("Heavy Belt", 20));
}

#[test]
fn zero_quality_does_not_inject_modifier() {
    assert_no_quality_modifier(EquipmentSlot::Helmet, &bare_item("Iron Helmet", 0));
}

// ── Defer: 催化剂（accessory quality → catalyst）尚未建模 ───────────────────
//
// PoB2 `getCatalystScalar(catalystId, mod, quality)`（src/Classes/Item.lua 33-58）
// 按催化剂的 tag 集（catalystTags：life/mana/defences/physical/attack/caster…）与
// **每条词缀的 GGG modTags** 求交，命中则将该词缀值缩放 (100 + quality)/100。
//
// PoBR 当前缺两样东西，故无法建模，明确 defer：
//   1. 数据模型：`pobr_data::Item` 无 `catalyst` / `catalyst_quality` 字段
//      （item_text 已把 `Catalyst:` / `CatalystQuality:` 行作为元数据丢弃）。
//   2. 词缀 tag 分类：PoBR 解析出的 `Modifier` 不携带 GGG modTags
//      （life/defences/physical…），`getCatalystScalar` 无从匹配。
//
// 补齐顺序应为：先在 mod 数据管线引入词缀 tag 分类，再加 Item 催化剂字段，
// 最后在 ingest（或编排层）按 tag 命中缩放词缀值。此处仅占位记录。
#[test]
fn catalyst_scaling_is_deferred_no_field_in_model() {
    // 文档化 defer：accessory 即便有品质也不缩放词条（催化剂未建模），
    // 与 accessory_quality_does_not_inject_modifier 一致——不引入虚假缩放。
    let ring = bare_item("Topaz Ring", 20);
    let ingest = ingest_item(EquipmentSlot::Ring1, &ring).unwrap();
    assert!(
        ingest.modifiers.is_empty(),
        "催化剂未建模：accessory 不应因品质产生任何额外 modifier"
    );
}

// ── flask / charm 载荷接入（M3-T4 D2，item.rs flask 分支）────────────────────

use pobr_core::item::{
    CHARM_BUFF_LIST_NAME, FLASK_BUFF_LIST_NAME, LOCAL_UTILITY_EFFECT_NAME, UtilityItemKind,
    classify_utility_item, ingest_flask_charm,
};

fn utility_item(base: &str, implicits: &[&str], explicits: &[&str]) -> Item {
    Item {
        base: ItemBaseId::from(base),
        rarity: ItemRarity::Magic,
        quality: 0,
        implicit_texts: implicits.iter().map(|t| (*t).to_string()).collect(),
        modifier_texts: explicits.iter().map(|t| (*t).to_string()).collect(),
        enchant_texts: Vec::new(),
        rolled_defence: RolledDefence::default(),
        parsed_stats: Vec::new(),
    }
}

/// charm 词条 → CharmBuff 载荷（List 嵌套），归因 SourceId(Flask, "flask.charm1")；
/// 载荷对聚合零影响（List 不参与 sum——未合并前逐值不变的微观锚点）。
#[test]
fn ingest_charm_wraps_mods_into_list_payload_with_flask_attribution() {
    let charm = utility_item(
        "Sapphire Charm of Lightning",
        &["Used when you take Cold damage from a Hit"],
        &["+15% to Lightning Resistance"],
    );
    assert_eq!(classify_utility_item(&charm), UtilityItemKind::Charm);

    let ingest = ingest_flask_charm("Charm 1", &charm);
    assert_eq!(
        ingest.unsupported,
        vec!["Used when you take Cold damage from a Hit".to_string()],
        "触发行不可解析 → unsupported 原行收集"
    );
    assert_eq!(ingest.modifiers.len(), 1, "仅一个载荷 mod");
    let carrier = &ingest.modifiers[0];
    assert_eq!(carrier.name, ModName::from(CHARM_BUFF_LIST_NAME));
    assert_eq!(carrier.mod_type, ModType::List);
    assert_eq!(
        carrier.source.as_deref(),
        Some("Sapphire Charm of Lightning")
    );
    let origin = carrier.origin.as_ref().expect("carrier origin");
    assert_eq!(
        origin.source_id,
        SourceId::new(SourceKind::Flask, "flask.charm1")
    );

    let nested = carrier.value.as_nested_mods().expect("nested payload");
    assert_eq!(nested.len(), 1);
    assert_eq!(nested[0].name, ModName::from("LightningResistance"));
    assert_eq!(
        nested[0].origin.as_ref().unwrap().source_id,
        SourceId::new(SourceKind::Flask, "flask.charm1"),
        "内层 mod 各自带 Flask 归因"
    );

    // List 载荷对聚合零影响（未合并前逐值不变）。
    let mut db = ModDb::new();
    db.add_mod(carrier.clone());
    assert_eq!(
        db.sum(
            ModType::Base,
            &CalcConfig::new(),
            &[ModName::from("LightningResistance")]
        ),
        0.0
    );
}

/// M4-m：全部行不可解析的激活 charm **仍产出空载荷**（vendor 对进预算的激活
/// charm 无条件置 UsingCharm/Using<Base> 条件，CalcPerform.lua:1634-1643——
/// 条件置位与 modList 无关；空 NestedMods 在 merge 缩放循环空转）。
#[test]
fn ingest_charm_with_no_parseable_mods_still_emits_empty_carrier() {
    let charm = utility_item(
        "Golden Charm",
        &["Used when you become Stunned"],
        &["40% increased Quantity of Gold Dropped by Slain Enemies"],
    );
    let ingest = ingest_flask_charm("Charm 3", &charm);
    assert_eq!(ingest.modifiers.len(), 1, "空载荷仍产出");
    let carrier = &ingest.modifiers[0];
    assert_eq!(carrier.name, ModName::from(CHARM_BUFF_LIST_NAME));
    assert_eq!(carrier.source.as_deref(), Some("Golden Charm"));
    assert!(
        carrier.value.as_nested_mods().is_some_and(<[_]>::is_empty),
        "载荷为空 NestedMods"
    );
}

/// flask 特殊行：`N% increased effect` → LocalUtilityEffect；`... during effect`
/// 后缀剥除复用 parser（MovementSpeed）。
///
/// **`Grants Onslaught during effect` 刻意归 Unsupported**——PoB2 ModParser 对该行
/// 返回 unsupported（无 mod 产出，`run-parsemod.sh` 核实），其 Onslaught 不进 modDB；
/// PoBR 与之逐行一致，不再越过 PoB2 解析能力无条件发 `flag("Onslaught")`（那会让
/// detonate-dead/coiling/flicker 的 Speed 相对 golden 偏高 +20%）。
#[test]
fn ingest_flask_onslaught_during_effect_is_unsupported_local_effect_still_parses() {
    let flask = utility_item(
        "Quicksilver Flask",
        &[],
        &[
            "Grants Onslaught during effect",
            "25% increased effect",
            "10% increased Movement Speed during effect",
        ],
    );
    assert_eq!(classify_utility_item(&flask), UtilityItemKind::Flask);

    let ingest = ingest_flask_charm("Flask 1", &flask);
    assert_eq!(
        ingest.unsupported,
        vec!["Grants Onslaught during effect".to_string()],
        "Onslaught 行与 PoB2 一致归 Unsupported（不发 Onslaught flag）"
    );
    let carrier = &ingest.modifiers[0];
    assert_eq!(carrier.name, ModName::from(FLASK_BUFF_LIST_NAME));
    let nested = carrier.value.as_nested_mods().unwrap();
    // Onslaught 行不产出 mod → 仅 LocalUtilityEffect + MovementSpeed 两条。
    assert_eq!(nested.len(), 2);
    assert!(
        nested.iter().all(|m| m.name != ModName::from("Onslaught")),
        "不得有 Onslaught flag"
    );
    assert_eq!(nested[0].name, ModName::from(LOCAL_UTILITY_EFFECT_NAME));
    assert_eq!(nested[0].value.as_number(), Some(25.0));
    assert_eq!(nested[1].name, ModName::from("MovementSpeed"));
    assert_eq!(nested[1].mod_type, ModType::Inc);
    assert_eq!(nested[1].value.as_number(), Some(10.0));
}

/// 全部行不可解析 → **仍产出空载荷**（M4-m 行为切换：vendor 条件置位与
/// modList 无关，CalcPerform.lua:1634-1643）；unsupported 照常逐行收集。
#[test]
fn ingest_flask_charm_emits_empty_payload_when_nothing_parses() {
    let charm = utility_item(
        "Thawing Charm",
        &["Used when you become Frozen"],
        &["Energy Shield Recharge starts on use"],
    );
    let ingest = ingest_flask_charm("Charm 2", &charm);
    assert_eq!(ingest.modifiers.len(), 1);
    assert!(
        ingest.modifiers[0]
            .value
            .as_nested_mods()
            .is_some_and(<[_]>::is_empty)
    );
    assert_eq!(ingest.unsupported.len(), 2);
}
