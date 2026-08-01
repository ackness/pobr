use pobr_core::calc::MinimalInput;
use pobr_core::item::{ItemIngest, ingest_item_with_ctx};

/// Engine-backed ingest (keeps the historical `ingest_item` signature, wires in the
/// real rules).
fn ingest_item(
    slot: EquipmentSlot,
    item: &Item,
) -> Result<ItemIngest, pobr_core::mod_parser::ParseError> {
    ingest_item_with_ctx(slot, item, crate::support::ctx())
}
use pobr_core::{CalcConfig, ModDb, ModTag};
use pobr_data::prelude::*;

fn helmet(texts: &[&str]) -> Item {
    Item {
        base: ItemBaseId::from("Helmet"),
        rarity: ItemRarity::Rare,
        quality: 20,
        corrupted: false,
        implicit_texts: Vec::new(),
        modifier_texts: texts.iter().map(|t| (*t).to_string()).collect(),
        enchant_texts: Vec::new(),
        rolled_defence: RolledDefence::default(),
        parsed_stats: Vec::new(),
    }
}

#[test]
fn ingest_substitutes_slotname_in_per_socket_multiplier() {
    // `per Socket filled` -> `Multiplier{var:"RunesSocketedIn{SlotName}"}`; ingest
    // substitutes `{SlotName}` with the slot ID (the orchestration layer pre-fills
    // `RunesSocketedIn<slot>` for the lookup).
    let item = helmet(&["+14 to Spirit per Socket filled"]);
    let ingest = ingest_item(EquipmentSlot::BodyArmour, &item).unwrap();
    let spirit = ingest
        .modifiers
        .iter()
        .find(|m| m.name.as_str() == "Spirit")
        .expect("Spirit mod should be parsed");
    assert!(
        spirit.tags.iter().any(|t| matches!(
            t,
            ModTag::Multiplier { var, .. } if var == "RunesSocketedInbodyarmour"
        )),
        "{{SlotName}} should be replaced with the slot ID: {:?}",
        spirit.tags
    );
}

#[test]
fn ingest_item_parses_texts_into_modifiers_with_item_affix_source() {
    let item = helmet(&["+40 to maximum Life", "+30% to Fire Resistance"]);

    let ItemIngest {
        modifiers,
        unsupported,
    } = ingest_item(EquipmentSlot::Helmet, &item).unwrap();

    assert!(unsupported.is_empty());
    // Quality no longer injects a modifier (per-stat base scaling is handled at the
    // orchestration layer); only the 2 explicit mods remain.
    assert_eq!(modifiers.len(), 2);

    // Only check the origin fields of explicit mods (SourceKind::ItemAffix).
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
        // Explicit mods attribute to ItemAffix; the SourceId includes the slot and
        // section suffix.
        assert_eq!(origin.source_id.kind, SourceKind::ItemAffix);
        assert_eq!(origin.source_id.id, "item.helmet.explicit");
        assert_eq!(origin.slot.as_deref(), Some("helmet"));
        // The raw mod text must be preserved for breakdown display and comparison
        // against PoB.
        assert!(origin.raw_text.is_some());
    }

    // stat_id / mod_type get backfilled from the modifier by with_origin.
    let life = modifiers
        .iter()
        .find(|m| m.name == ModName::from("MaximumLife"))
        .unwrap();
    assert_eq!(
        life.origin.as_ref().unwrap().stat_id,
        Some(ModName::from("MaximumLife"))
    );
    assert_eq!(life.origin.as_ref().unwrap().mod_type, Some(ModType::Base));

    // Quality no longer injects any ItemQuality modifier.
    let quality_mod = modifiers.iter().find(|m| {
        m.origin
            .as_ref()
            .map(|o| o.source_id.kind == SourceKind::ItemQuality)
            .unwrap_or(false)
    });
    assert!(
        quality_mod.is_none(),
        "quality no longer injects as a modifier (per-attribute base scaling is handled by the orchestration layer)"
    );
}

#[test]
fn ingest_item_distinguishes_implicit_explicit_enchant_sections() {
    // Each of the three sections uses a different stat so each origin can be
    // uniquely located.
    let item = Item {
        base: ItemBaseId::from("Helmet"),
        rarity: ItemRarity::Rare,
        quality: 0,
        corrupted: false,
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
        corrupted: false,
        implicit_texts: vec!["split".to_string()],
        modifier_texts: vec!["+40 to maximum Life".to_string(), "mirrored".to_string()],
        enchant_texts: Vec::new(),
        rolled_defence: RolledDefence::default(),
        parsed_stats: Vec::new(),
    };

    let ingest = ingest_item(EquipmentSlot::Helmet, &item).unwrap();

    assert_eq!(ingest.modifiers.len(), 1);
    // Unparseable lines from every section get collected into unsupported.
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
        corrupted: false,
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
        corrupted: false,
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
    let mut session = crate::support::session(input);

    let body = Item {
        base: ItemBaseId::from("Plate Vest"),
        rarity: ItemRarity::Rare,
        quality: 0,
        corrupted: false,
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

    // (100 base + 40 item) * (1 + 20/100) = 168.
    assert_eq!(output.life, 168.0);
    // Unparseable mods are still retained (as unsupported text).
    assert_eq!(session.unsupported_modifier_texts(), ["mirrored"]);
}

// Quality is not injected as a More modifier (finding 02-05 fix)
//
// PoB2's item quality is a **per-stat base scaling** (weapons: physical only;
// armour: armour/evasion/ES each independently x (1 + quality/100)), not a single
// global `more` modifier. Injecting quality as a global More mod
// (`LocalPhysicalDamageMore` / `LocalDefencesMore`) would incorrectly spill over
// into global damage / all defences. The actual scaling is handled per-item,
// per-stat by the orchestration layer's `pobr-build::calc_orchestrator`
// (item_rolled_defence and weapon physical_min/max), matching PoB2. So ingest
// injects no quality modifier.
//
// Source: PoB2 src/Classes/Item.lua BuildModListForSlotNum 1751-1756 (weapons),
//         1812-1819 (armour, per-stat independent scaling).

/// Helper: asserts that ingesting an item produces **no** modifier attributed to
/// `ItemQuality`.
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
        "{} quality should not inject as a modifier (per-attribute base scaling is handled by the orchestration layer)",
        slot.id()
    );
}

fn bare_item(base: &str, slot_quality: u8) -> Item {
    Item {
        base: ItemBaseId::from(base),
        rarity: ItemRarity::Rare,
        quality: slot_quality,
        corrupted: false,
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
    // Ring / belt quality goes through the catalyst mechanism (getCatalystScalar),
    // which is not yet modeled -- see the defer note below.
    assert_no_quality_modifier(EquipmentSlot::Ring1, &bare_item("Iron Ring", 20));
    assert_no_quality_modifier(EquipmentSlot::Belt, &bare_item("Heavy Belt", 20));
}

#[test]
fn zero_quality_does_not_inject_modifier() {
    assert_no_quality_modifier(EquipmentSlot::Helmet, &bare_item("Iron Helmet", 0));
}

// Defer: catalyst (accessory quality -> catalyst) is not yet modeled
//
// PoB2's `getCatalystScalar(catalystId, mod, quality)` (src/Classes/Item.lua 33-58)
// intersects the catalyst's tag set (catalystTags: life/mana/defences/physical/
// attack/caster...) with **each affix's GGG modTags**; on a match, it scales that
// affix's value by (100 + quality)/100.
//
// PoBR is currently missing two pieces, so this can't be modeled yet -- explicitly
// deferred:
//   1. Data model: `pobr_data::Item` has no `catalyst` / `catalyst_quality` field
//      (item_text already drops `Catalyst:` / `CatalystQuality:` lines as metadata).
//   2. Affix tag classification: the `Modifier` values PoBR parses don't carry GGG
//      modTags (life/defences/physical...), so `getCatalystScalar` has nothing to
//      match against.
//
// The fill-in order should be: introduce affix tag classification in the mod data
// pipeline first, then add the Item catalyst field, then scale affix values by tag
// match in ingest (or the orchestration layer). This is a placeholder note only.
#[test]
fn catalyst_scaling_is_deferred_no_field_in_model() {
    // Documents the defer: accessories don't scale mods by quality (catalyst is not
    // modeled), consistent with accessory_quality_does_not_inject_modifier -- avoids
    // introducing fake scaling.
    let ring = bare_item("Topaz Ring", 20);
    let ingest = ingest_item(EquipmentSlot::Ring1, &ring).unwrap();
    assert!(
        ingest.modifiers.is_empty(),
        "catalyst is not modeled: an accessory should not produce any extra modifier from quality"
    );
}

// flask / charm payload wiring

use pobr_core::item::{
    CHARM_BUFF_LIST_NAME, FLASK_BUFF_LIST_NAME, LOCAL_UTILITY_EFFECT_NAME, UtilityItemKind,
    classify_utility_item, ingest_flask_charm_with_ctx,
};

/// Engine-backed flask/charm ingest (keeps the historical `ingest_flask_charm`
/// signature).
fn ingest_flask_charm(slot_name: &str, item: &Item) -> ItemIngest {
    ingest_flask_charm_with_ctx(slot_name, item, crate::support::ctx())
}

fn utility_item(base: &str, implicits: &[&str], explicits: &[&str]) -> Item {
    Item {
        base: ItemBaseId::from(base),
        rarity: ItemRarity::Magic,
        quality: 0,
        corrupted: false,
        implicit_texts: implicits.iter().map(|t| (*t).to_string()).collect(),
        modifier_texts: explicits.iter().map(|t| (*t).to_string()).collect(),
        enchant_texts: Vec::new(),
        rolled_defence: RolledDefence::default(),
        parsed_stats: Vec::new(),
    }
}

/// Charm mods -> CharmBuff payload (nested List), attributed to
/// SourceId(Flask, "flask.charm1"); the payload has zero effect on aggregation
/// (List doesn't participate in sum -- a micro-anchor for "values unchanged before
/// merge").
#[test]
fn ingest_charm_wraps_mods_into_list_payload_with_flask_attribution() {
    let charm = utility_item(
        "Sapphire Charm of Lightning",
        &["Used when you take Cold damage from a Hit"],
        &["+15% to Lightning Resistance"],
    );
    assert_eq!(classify_utility_item(&charm), UtilityItemKind::Charm);

    let ingest = ingest_flask_charm("Charm 1", &charm);
    assert!(
        ingest.unsupported.is_empty(),
        "the trigger-condition line is an inherent description of the base, it doesn't land in the unsupported report"
    );
    assert_eq!(ingest.modifiers.len(), 1, "only one payload mod");
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
        "each inner mod carries its own Flask attribution"
    );

    // The List payload has zero effect on aggregation (values unchanged before merge).
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

/// Even mods for effects a charm has that aren't currently modeled must still show
/// up in the unsupported report. `Also grants N Guard` is a real explicit prefix
/// from ModCharm.lua; `Possessed by ...` also comes from a real build -- silently
/// dropping either would make callers think the effect is already in effect.
#[test]
fn ingest_charm_parses_guard_and_possession_effects_via_engine() {
    // The possession line is modeled through the engine's special channel
    // (SpiritPossessionOnUse) and normally produces a mod. The guard line
    // (`Also grants N Guard`) was once modeled by the curated entry
    // also_grants_guard, but the vendor ModParser never actually parses it -- it was
    // a phantom Guard pool relative to PoB2 golden values (removed in backlog item
    // #7, ritualist EHP 1.10x -> 1.00x). Aligning with PoB2 puts it back in the
    // "unmodeled -> must be loudly reported as unsupported" bucket; silently
    // dropping it would be the failure mode.
    let charm = utility_item(
        "Thawing Charm",
        &["Used when you become Frozen"],
        &[
            "Also grants 435 Guard",
            "Possessed by Spirit Of The Cat for 17 seconds on use",
        ],
    );

    let ingest = ingest_flask_charm("Charm 1", &charm);

    assert_eq!(
        ingest.unsupported,
        vec!["Also grants 435 Guard".to_string()],
        "the guard line is unmodeled and should be reported (matching PoB2), the possession line should parse; actual unsupported: {:?}",
        ingest.unsupported
    );
}

/// An activated charm where every line fails to parse **still produces an empty
/// payload** (vendor unconditionally sets the UsingCharm/Using<Base> condition for
/// charms within budget, CalcPerform.lua:1634-1643 -- setting the condition is
/// independent of the modList; an empty NestedMods is simply a no-op in the merge
/// scaling loop).
#[test]
fn ingest_charm_with_no_parseable_mods_still_emits_empty_carrier() {
    let charm = utility_item(
        "Golden Charm",
        &["Used when you become Stunned"],
        &["40% increased Quantity of Gold Dropped by Slain Enemies"],
    );
    let ingest = ingest_flask_charm("Charm 3", &charm);
    assert_eq!(
        ingest.modifiers.len(),
        1,
        "an empty payload still produces output"
    );
    let carrier = &ingest.modifiers[0];
    assert_eq!(carrier.name, ModName::from(CHARM_BUFF_LIST_NAME));
    assert_eq!(carrier.source.as_deref(), Some("Golden Charm"));
    assert!(
        carrier.value.as_nested_mods().is_some_and(<[_]>::is_empty),
        "the payload is an empty NestedMods"
    );
}

/// Flask special lines: `N% increased effect` -> LocalUtilityEffect; the
/// `... during effect` suffix is stripped and re-uses the parser (MovementSpeed).
///
/// **`Grants Onslaught during effect` is deliberately Unsupported** -- PoB2's
/// ModParser returns unsupported for this line (produces no mod, verified via
/// `run-parsemod.sh`), so Onslaught never enters modDB. PoBR matches it line for
/// line and no longer unconditionally emits `flag("Onslaught")` beyond what PoB2 can
/// parse (that inflated Speed for detonate-dead/coiling/flicker by +20% relative to
/// golden).
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
        "the Onslaught line is Unsupported, matching PoB2 (no Onslaught flag emitted)"
    );
    let carrier = &ingest.modifiers[0];
    assert_eq!(carrier.name, ModName::from(FLASK_BUFF_LIST_NAME));
    let nested = carrier.value.as_nested_mods().unwrap();
    // The Onslaught line produces no mod -> only LocalUtilityEffect + MovementSpeed
    // remain.
    assert_eq!(nested.len(), 2);
    assert!(
        nested.iter().all(|m| m.name != ModName::from("Onslaught")),
        "must not have an Onslaught flag"
    );
    assert_eq!(nested[0].name, ModName::from(LOCAL_UTILITY_EFFECT_NAME));
    assert_eq!(nested[0].value.as_number(), Some(25.0));
    assert_eq!(nested[1].name, ModName::from("MovementSpeed"));
    assert_eq!(nested[1].mod_type, ModType::Inc);
    assert_eq!(nested[1].value.as_number(), Some(10.0));
}

/// Every line unparseable -> **still produces an empty payload** (behavior switch:
/// vendor's condition-setting is independent of modList, CalcPerform.lua:1634-1643);
/// unsupported still collects line by line as usual.
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
    // "Used when you become Frozen" is a trigger-description line (silently
    // skipped); "Energy Shield Recharge starts on use" is still an unsupported-mod
    // gap.
    assert_eq!(
        ingest.unsupported,
        vec!["Energy Shield Recharge starts on use".to_string()]
    );
}
