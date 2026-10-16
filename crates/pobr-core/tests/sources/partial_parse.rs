//! A line the parser read only in part is not applied by any ingest path.
//!
//! The parser can return `Parsed` with leftover text: it recognised a prefix of the line
//! and the modifier it built for that prefix can mean something else than the whole line.
//! Measured on the benchmark and tuned builds (flask lines):
//!
//! - "1% more Life Flask Recovery" reads as +1% more `MaximumLife`, leftover "Flask Recovery";
//! - "50% of Chaos damage you prevent when Hit Recouped as Life and Mana during effect" reads
//!   as +50 `ChaosDamage`, leftover "you prevent when Hit Recouped as Life and Mana";
//! - "Removes 15% of Life Recovered from Mana when used" reads as -15 `FlaskRecovery`
//!   (the flask's own recovery, not the Life recovered from Mana);
//! - "88% more Recovery if used while on Low Mana" reads as 88% more `FlaskLifeRecoveryLowLife`
//!   (Low Life, not Low Mana), leftover "if used";
//! - "1% increased speed of Recoup Effects" reads as +1% increased `LocalEffect`.
//!
//! Every ingest path goes through `ParseOutcome::into_applicable`: such a line applies
//! nothing and is reported in the ingest's `unsupported` list (and the session's
//! `unsupported_modifier_texts`), exactly like a line no rule matched.

use pobr_core::Modifier;
use pobr_core::calc::MinimalInput;
use pobr_core::item::{
    FLASK_BUFF_LIST_NAME, ItemIngest, ingest_flask_charm_with_ctx, ingest_item_with_ctx,
};
use pobr_core::mod_parser::ParseStatus;
use pobr_core::passive::{AllocatedNode, ingest_passive_nodes_with_ctx};
use pobr_core::skill_source::{
    ActiveSkillSpec, GemModSource, SupportGemSpec, ingest_active_gem_with_ctx, ingest_gem_with_ctx,
    ingest_support_gem_with_ctx,
};
use pobr_data::prelude::*;
use std::collections::HashSet;

use crate::support::{ctx, parse_mod, session};

/// Reads as +1% more `MaximumLife`, leftover "Flask Recovery".
const LIFE_FLASK_MORE: &str = "1% more Life Flask Recovery";
/// Reads as +1% increased `LocalEffect`, leftover "speed of Recoup s".
const RECOUP_SPEED: &str = "1% increased speed of Recoup Effects";
/// Reads as +50 `ChaosDamage`, leftover "you prevent when Hit Recouped as Life and Mana".
const CHAOS_PREVENTED: &str =
    "50% of Chaos damage you prevent when Hit Recouped as Life and Mana during effect";
/// Reads as -15 `FlaskRecovery`, leftover "% of  from Mana when used".
const REMOVES_FROM_MANA: &str = "Removes 15% of Life Recovered from Mana when used";
/// Reads as 88% more `FlaskLifeRecoveryLowLife`, leftover "if used".
const LOW_MANA_MORE: &str = "88% more Recovery if used while on Low Mana";

fn item(base: &str, rarity: ItemRarity, explicits: &[&str]) -> Item {
    Item {
        base: ItemBaseId::from(base),
        rarity,
        quality: 0,
        corrupted: false,
        implicit_texts: Vec::new(),
        modifier_texts: explicits.iter().map(|t| (*t).to_string()).collect(),
        enchant_texts: Vec::new(),
        rolled_defence: RolledDefence::default(),
        socketed: Vec::new(),
        parsed_stats: Vec::new(),
    }
}

fn node(id: u32, texts: &[&str]) -> AllocatedNode {
    AllocatedNode {
        node_id: NodeId(id),
        ascendancy: false,
        modifier_texts: texts.iter().map(|t| (*t).to_string()).collect(),
    }
}

fn has_name(mods: &[Modifier], name: &str) -> bool {
    mods.iter().any(|m| m.name.as_str() == name)
}

#[test]
fn parse_outcome_separates_complete_partial_and_unsupported_lines() {
    // Complete: parsed, nothing left over -> the modifiers are applicable.
    let complete = parse_mod("+40 to maximum Life").unwrap();
    assert!(complete.is_complete());
    let mods = complete.into_applicable("+40 to maximum Life").unwrap();
    assert_eq!(mods.len(), 1);
    assert_eq!(mods[0].name.as_str(), "MaximumLife");
    assert_eq!(mods[0].value.as_number(), Some(40.0));

    // Partial: status is `Parsed` and a modifier exists, but "Flask Recovery" is left over.
    let partial = parse_mod(LIFE_FLASK_MORE).unwrap();
    assert_eq!(partial.status, ParseStatus::Parsed);
    assert_eq!(partial.unparsed.as_deref(), Some("Flask Recovery"));
    assert_eq!(partial.mods.len(), 1);
    assert_eq!(partial.mods[0].name.as_str(), "MaximumLife");
    assert_eq!(partial.mods[0].value.as_number(), Some(1.0));
    assert!(!partial.is_complete());
    // Nothing is applicable; the whole line is what gets reported.
    assert_eq!(
        partial.into_applicable(LIFE_FLASK_MORE),
        Err(LIFE_FLASK_MORE.to_string())
    );

    // Unsupported: no rule matched; the parser's own remainder is reported.
    let unsupported = parse_mod("not a real modifier").unwrap();
    assert_eq!(unsupported.status, ParseStatus::Unsupported);
    assert!(!unsupported.is_complete());
    assert!(
        unsupported
            .into_applicable("not a real modifier")
            .is_err_and(|text| !text.is_empty())
    );
}

#[test]
fn item_section_does_not_apply_a_partially_parsed_line() {
    let helmet = item(
        "Helmet",
        ItemRarity::Rare,
        &["+40 to maximum Life", LIFE_FLASK_MORE],
    );

    let ItemIngest {
        modifiers,
        unsupported,
    } = ingest_item_with_ctx(EquipmentSlot::Helmet, &helmet, ctx()).unwrap();

    // Only the complete line is applied: one +40 MaximumLife modifier, no +1% more.
    let life: Vec<_> = modifiers
        .iter()
        .filter(|m| m.name.as_str() == "MaximumLife")
        .collect();
    assert_eq!(life.len(), 1, "{modifiers:?}");
    assert_eq!(life[0].mod_type, ModType::Base);
    assert_eq!(life[0].value.as_number(), Some(40.0));
    assert_eq!(unsupported, vec![LIFE_FLASK_MORE.to_string()]);
}

#[test]
fn flask_ingest_does_not_apply_partially_parsed_lines() {
    let flask = item(
        "Quicksilver Flask",
        ItemRarity::Magic,
        &[
            "10% increased Movement Speed during effect",
            REMOVES_FROM_MANA,
            LOW_MANA_MORE,
            CHAOS_PREVENTED,
        ],
    );

    let ingest = ingest_flask_charm_with_ctx("Flask 1", &flask, ctx());

    // The three partial lines are reported as written, in order.
    assert_eq!(
        ingest.unsupported,
        vec![
            REMOVES_FROM_MANA.to_string(),
            LOW_MANA_MORE.to_string(),
            CHAOS_PREVENTED.to_string(),
        ]
    );
    // The payload holds only the complete line: one MovementSpeed +10% increased.
    let carrier = &ingest.modifiers[0];
    assert_eq!(carrier.name, ModName::from(FLASK_BUFF_LIST_NAME));
    let payload = carrier.value.as_nested_mods().unwrap();
    assert_eq!(payload.len(), 1, "{payload:?}");
    assert_eq!(payload[0].name.as_str(), "MovementSpeed");
    assert_eq!(payload[0].mod_type, ModType::Inc);
    assert_eq!(payload[0].value.as_number(), Some(10.0));
    for wrong in ["FlaskRecovery", "FlaskLifeRecoveryLowLife", "ChaosDamage"] {
        assert!(!has_name(payload, wrong), "{wrong} must not be applied");
    }
}

#[test]
fn charm_ingest_does_not_apply_a_partially_parsed_line() {
    let charm = item(
        "Sapphire Charm of Lightning",
        ItemRarity::Magic,
        &["+15% to Lightning Resistance", CHAOS_PREVENTED],
    );

    let ingest = ingest_flask_charm_with_ctx("Charm 1", &charm, ctx());

    assert_eq!(ingest.unsupported, vec![CHAOS_PREVENTED.to_string()]);
    let payload = ingest.modifiers[0].value.as_nested_mods().unwrap();
    assert_eq!(payload.len(), 1, "{payload:?}");
    assert_eq!(payload[0].name.as_str(), "LightningResistance");
    assert_eq!(payload[0].value.as_number(), Some(15.0));
}

#[test]
fn passive_ingest_does_not_apply_a_partially_parsed_line() {
    let nodes = vec![node(2001, &["+40 to maximum Life", RECOUP_SPEED])];

    let ingest = ingest_passive_nodes_with_ctx(&nodes, ctx()).unwrap();

    assert_eq!(ingest.modifiers.len(), 1, "{:?}", ingest.modifiers);
    assert_eq!(ingest.modifiers[0].name.as_str(), "MaximumLife");
    assert!(!has_name(&ingest.modifiers, "LocalEffect"));
    assert_eq!(ingest.unsupported, vec![RECOUP_SPEED.to_string()]);
}

#[test]
fn gem_ingest_does_not_apply_a_partially_parsed_line() {
    // Active and support gems from the text-only entry point.
    for gem in [
        GemModSource::active("fireball", ["+40 to maximum Life", LIFE_FLASK_MORE]),
        GemModSource::support("added_fire", ["+40 to maximum Life", LIFE_FLASK_MORE])
            .supporting("fireball"),
    ] {
        let ingest = ingest_gem_with_ctx(&gem, ctx()).unwrap();
        let life: Vec<_> = ingest
            .modifiers
            .iter()
            .filter(|m| m.name.as_str() == "MaximumLife")
            .collect();
        assert_eq!(life.len(), 1, "{:?}", ingest.modifiers);
        assert_eq!(life[0].value.as_number(), Some(40.0));
        assert_eq!(ingest.unsupported, vec![LIFE_FLASK_MORE.to_string()]);
    }
}

#[test]
fn active_and_support_gem_specs_do_not_apply_a_partially_parsed_line() {
    let active = ActiveSkillSpec::new(
        "fireball",
        SkillTypes::SPELL,
        ["+40 to maximum Life", LIFE_FLASK_MORE],
    );
    let ingest = ingest_active_gem_with_ctx(&active, ctx()).unwrap();
    assert_eq!(ingest.modifiers.len(), 1, "{:?}", ingest.modifiers);
    assert_eq!(ingest.modifiers[0].value.as_number(), Some(40.0));
    assert_eq!(ingest.unsupported, vec![LIFE_FLASK_MORE.to_string()]);

    let support = SupportGemSpec::new("added_fire", ["+40 to maximum Life", LIFE_FLASK_MORE])
        .supporting("fireball");
    let ingest = ingest_support_gem_with_ctx(&support, &HashSet::new(), ctx()).unwrap();
    let life: Vec<_> = ingest
        .modifiers
        .iter()
        .filter(|m| m.name.as_str() == "MaximumLife")
        .collect();
    assert_eq!(life.len(), 1, "{:?}", ingest.modifiers);
    assert_eq!(life[0].value.as_number(), Some(40.0));
    assert_eq!(ingest.unsupported, vec![LIFE_FLASK_MORE.to_string()]);
}

#[test]
fn session_add_modifier_texts_reports_a_partially_parsed_line() {
    let mut session = session(MinimalInput {
        base_life: 100.0,
        ..MinimalInput::default()
    });
    session
        .add_modifier_texts(["+40 to maximum Life", LIFE_FLASK_MORE, RECOUP_SPEED])
        .unwrap();

    // Life = 100 base + 40; the "+1% more" modifier the leftover-text parse built is absent.
    let output = session.perform_minimal().expect("perform");
    assert_eq!(output.life, 140.0);
    assert_eq!(
        session.unsupported_modifier_texts(),
        [LIFE_FLASK_MORE, RECOUP_SPEED]
    );
    assert!(session.mods_named("LocalEffect").is_empty());
}

#[test]
fn session_entry_points_report_partially_parsed_lines() {
    let mut session = session(MinimalInput::default());
    session
        .add_item(
            EquipmentSlot::Helmet,
            &item("Helmet", ItemRarity::Rare, &[LIFE_FLASK_MORE]),
        )
        .unwrap();
    session.add_flask_charm(
        "Flask 1",
        &item("Quicksilver Flask", ItemRarity::Magic, &[CHAOS_PREVENTED]),
    );
    session
        .add_passive_nodes(&[node(1, &[RECOUP_SPEED])])
        .unwrap();
    session
        .add_skill_gem(&GemModSource::active("fireball", [REMOVES_FROM_MANA]))
        .unwrap();

    assert_eq!(
        session.unsupported_modifier_texts(),
        [
            LIFE_FLASK_MORE,
            CHAOS_PREVENTED,
            RECOUP_SPEED,
            REMOVES_FROM_MANA
        ]
    );
    assert!(session.mods_named("MaximumLife").is_empty());
    assert!(session.mods_named("ChaosDamage").is_empty());
    assert!(session.mods_named("LocalEffect").is_empty());
    assert!(session.mods_named("FlaskRecovery").is_empty());
}
