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
    assert_eq!(modifiers.len(), 2);

    for modifier in &modifiers {
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
        parsed_stats: Vec::new(),
    };

    session.add_item(EquipmentSlot::BodyArmour, &body).unwrap();
    let output = session.perform_minimal();

    // (100 base + 40 item) * (1 + 20/100) = 168。
    assert_eq!(output.life, 168.0);
    // 无法解析的词条仍被保留。
    assert_eq!(session.unsupported_modifier_texts(), ["mirrored"]);
}
