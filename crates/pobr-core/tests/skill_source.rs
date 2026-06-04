use pobr_core::calc::{CalculationSession, MinimalInput};
use pobr_core::skill_source::{GemIngest, GemModSource, ingest_gem};
use pobr_core::{CalcConfig, ModDb};
use pobr_data::prelude::*;

#[test]
fn ingest_active_gem_attributes_to_skill_gem_source() {
    let gem = GemModSource::active("fireball", ["20% increased maximum Life"]);

    let GemIngest {
        modifiers,
        unsupported,
    } = ingest_gem(&gem).unwrap();

    assert!(unsupported.is_empty());
    assert_eq!(modifiers.len(), 1);

    let origin = modifiers[0]
        .origin
        .as_ref()
        .expect("gem modifier carries an origin");
    assert_eq!(origin.source_id.kind, SourceKind::SkillGem);
    assert_eq!(origin.source_id.id, "gem.fireball");
    assert!(origin.raw_text.is_some());
    // 主动宝石没有父技能来源。
    assert!(origin.parent_source_id.is_none());
}

#[test]
fn ingest_support_gem_attributes_to_support_gem_source_with_parent() {
    let gem =
        GemModSource::support("added_fire", ["10% increased maximum Life"]).supporting("fireball");

    let ingest = ingest_gem(&gem).unwrap();

    assert_eq!(ingest.modifiers.len(), 1);
    let origin = ingest.modifiers[0].origin.as_ref().unwrap();
    assert_eq!(origin.source_id.kind, SourceKind::SupportGem);
    assert_eq!(origin.source_id.id, "support.added_fire");

    // support gem 的 modifier 关联到所支援的主动技能 source。
    let parent = origin
        .parent_source_id
        .as_ref()
        .expect("support gem links to the active skill it supports");
    assert_eq!(parent.kind, SourceKind::SkillGem);
    assert_eq!(parent.id, "gem.fireball");
}

#[test]
fn ingest_gem_collects_unsupported_texts() {
    let gem = GemModSource::active("fireball", ["20% increased maximum Life", "mirrored"]);

    let ingest = ingest_gem(&gem).unwrap();

    assert_eq!(ingest.modifiers.len(), 1);
    assert_eq!(ingest.unsupported, vec!["mirrored".to_string()]);
}

#[test]
fn ingested_gem_modifiers_attribute_back_through_contributions() {
    let gem = GemModSource::active("fireball", ["20% increased maximum Life"]);
    let ingest = ingest_gem(&gem).unwrap();

    let mut db = ModDb::new();
    db.add_list(ingest.modifiers);
    let cfg = CalcConfig::new();

    let contributions = db.contributions(ModType::Inc, &cfg, &[ModName::from("MaximumLife")]);
    assert_eq!(contributions.len(), 1);
    let origin = contributions[0]
        .origin
        .as_ref()
        .expect("contribution carries the gem origin");
    assert_eq!(origin.source_id.kind, SourceKind::SkillGem);
    assert_eq!(origin.source_id.id, "gem.fireball");
}

#[test]
fn session_add_skill_gem_feeds_minimal_calc() {
    let input = MinimalInput {
        base_life: 100.0,
        ..MinimalInput::default()
    };
    let mut session = CalculationSession::new(input);

    let active = GemModSource::active(
        "fireball",
        [
            "+40 to maximum Life",
            "20% increased maximum Life",
            "mirrored",
        ],
    );
    session.add_skill_gem(&active).unwrap();

    let output = session.perform_minimal();

    // (100 base + 40 gem) * (1 + 20/100) = 168。
    assert_eq!(output.life, 168.0);
    assert_eq!(session.unsupported_modifier_texts(), ["mirrored"]);
}

#[test]
fn session_add_support_gem_feeds_minimal_calc() {
    let input = MinimalInput {
        base_life: 100.0,
        ..MinimalInput::default()
    };
    let mut session = CalculationSession::new(input);

    let support =
        GemModSource::support("added_fire", ["+50 to maximum Life"]).supporting("fireball");
    session.add_support_gem(&support).unwrap();

    let output = session.perform_minimal();
    assert_eq!(output.life, 150.0);
}
