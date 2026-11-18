//! Integration tests for affix tier annotation (real data).
//!
//! The active data version has mods' `group`/`spawn_weights` plus the
//! StatDescriptions overlay, so a rare item's explicit lines carry the `tier`
//! field.

use pobr_gamedata::repo_data_root;
use serde_json::{Value, json};

const RARE_ITEM: &str = "\
Rarity: RARE
Apocalypse Pelt
Falconer's Jacket
Item Level: 81
+190 to maximum Life
+34% to Cold Resistance";

const HYBRID_GLOVES: &str = "\
Rarity: RARE
Display Fixture
Runeforged Secured Wraps
Item Level: 81
Rune: Kolr's Hunt
Rune: The Greatwolf's Rune of Claws
{rune}Can roll Marksman modifiers
+74 to Evasion Rating
+25 to maximum Energy Shield
56% increased Evasion and Energy Shield
+2 to Level of all Projectile Skills
28% increased Critical Damage Bonus
Projectiles move at 38% increased Speed";

#[path = "support/staged_pack.rs"]
mod staged_pack;

fn classify(version_dir: &str) -> Vec<Value> {
    pobr_wasm::init_data_from_dir(version_dir).expect("init data");
    classified_lines(RARE_ITEM)
}

fn classified_lines(text: &str) -> Vec<Value> {
    let json = pobr_wasm::classify_item_lines_json(text).expect("classify");
    serde_json::from_str(&json).expect("parse lines json")
}

fn assert_glove_tiers(lines: &[Value]) {
    for (text, affix, tier, total) in [
        ("+74 to Evasion Rating", "prefix", 1, 4),
        ("+25 to maximum Energy Shield", "prefix", 1, 4),
        ("+2 to Level of all Projectile Skills", "suffix", 1, 2),
        ("Projectiles move at 38% increased Speed", "prefix", 1, 3),
    ] {
        let line = lines.iter().find(|line| line["text"] == text).unwrap();
        assert_eq!(line["affix"], affix, "{text}");
        assert_eq!(line["tier"], tier, "{text}");
        assert_eq!(line["tier_total"], total, "{text}");
        assert!(line.get("modifier_sources").is_none(), "{text}");
    }
    let rune = lines
        .iter()
        .find(|line| line["text"] == "Can roll Marksman modifiers")
        .unwrap();
    assert_eq!(rune["kind"], "rune");
    assert!(rune.get("tier").is_none());
}

#[test]
fn modifier_sources_preserve_supported_annotations_without_game_data() {
    for rarity in ["NORMAL", "MAGIC", "RARE", "UNIQUE", "RELIC"] {
        for source in ["crafted", "fractured", "desecrated", "mutated", "custom"] {
            let title = if matches!(rarity, "RARE" | "UNIQUE" | "RELIC") {
                "Annotation Fixture\n"
            } else {
                ""
            };
            let text = format!(
                "Rarity: {rarity}\n{title}Synthetic Base\nItem Level: 1\n{{{source}}}+40 to maximum Life"
            );
            let lines = classified_lines(&text);
            assert_eq!(
                lines.last().unwrap(),
                &json!({
                    "text": "+40 to maximum Life",
                    "kind": "explicit",
                    "modifier_sources": [source],
                }),
                "{rarity}, {source}"
            );
            assert!(
                lines[..lines.len() - 1]
                    .iter()
                    .all(|line| line.get("modifier_sources").is_none())
            );
        }
    }
}

#[test]
fn modifier_sources_stay_with_their_line_and_category() {
    let text = "\
Rarity: RARE
Annotation Fixture
Synthetic Base
Implicits: 3
{fractured}+40 to maximum Life
{rune}{crafted}+40 to maximum Life
{enchant}{desecrated}+40 to maximum Life
+40 to maximum Life
{custom}{mutated}{desecrated}{fractured}{crafted}{crafted}+40 to maximum Life
+40 to maximum Life
Corrupted
{fractured}Corrupted";
    let lines = classified_lines(text);
    assert_eq!(
        lines[3..],
        [
            json!({
                "text": "+40 to maximum Life",
                "kind": "implicit",
                "modifier_sources": ["fractured"],
            }),
            json!({
                "text": "+40 to maximum Life",
                "kind": "rune",
                "modifier_sources": ["crafted"],
            }),
            json!({
                "text": "+40 to maximum Life",
                "kind": "enchant",
                "modifier_sources": ["desecrated"],
            }),
            json!({"text": "+40 to maximum Life", "kind": "explicit"}),
            json!({
                "text": "+40 to maximum Life",
                "kind": "explicit",
                "modifier_sources": ["crafted", "fractured", "desecrated", "mutated", "custom"],
            }),
            json!({"text": "+40 to maximum Life", "kind": "explicit"}),
            json!({"text": "Corrupted", "kind": "struct"}),
            json!({
                "text": "Corrupted",
                "kind": "explicit",
                "modifier_sources": ["fractured"],
            }),
        ]
    );
    assert_eq!(classified_lines(text), lines);
}

#[test]
fn modifier_sources_are_omitted_without_line_annotation_evidence() {
    let text = "\
Rarity: RARE
Crafted Fixture
Synthetic Base
Crafted: true
Note: crafted fractured desecrated mutated custom
Implicits: 2
{rune}+40 to maximum Life
{enchant}+10 to Dexterity
+190 to maximum Life
Unknown effect: still needs review
{exotic}Unknown annotated effect
{tags:crafted,fractured,desecrated,mutated,custom}+20 to Strength
{range:0.5}{corruptedRange:0.5}{variant:1}{unscalable}+(10-20) to Intelligence
+20 to Intelligence (crafted)";
    let lines = classified_lines(text);
    assert!(
        lines
            .iter()
            .all(|line| line.get("modifier_sources").is_none())
    );
    assert!(lines.contains(&json!({
        "text": "Unknown effect: still needs review",
        "kind": "explicit",
    })));
    assert!(lines.contains(&json!({
        "text": "+20 to Intelligence (crafted)",
        "kind": "explicit",
    })));
}

#[test]
fn current_data_annotates_explicit_tiers() {
    let dir = repo_data_root().join(pobr_gamedata::data_version());
    let lines = classify(dir.to_str().unwrap());
    assert!(
        lines
            .iter()
            .all(|line| line.get("modifier_sources").is_none())
    );
    let explicits: Vec<&Value> = lines.iter().filter(|l| l["kind"] == "explicit").collect();
    assert_eq!(explicits.len(), 2, "should be two explicit lines");
    let tiered: Vec<&&Value> = explicits
        .iter()
        .filter(|l| l.get("tier").is_some())
        .collect();
    assert!(
        !tiered.is_empty(),
        "the current data version should have tagged at least one explicit line with a tier: {explicits:?}"
    );
    for l in &tiered {
        let tier = l["tier"].as_u64().unwrap();
        let total = l["tier_total"].as_u64().unwrap();
        assert!(
            tier >= 1 && tier <= total,
            "tier {tier} should fall within 1..={total}"
        );
        let affix = l["affix"].as_str().unwrap();
        assert!(affix == "prefix" || affix == "suffix");
    }
}

#[test]
fn hybrid_and_category_scoped_special_affixes_share_native_tiers() {
    pobr_wasm::init_data_from_dir(pobr_gamedata::current_data_dir().to_str().unwrap()).unwrap();
    let lines = classified_lines(HYBRID_GLOVES);
    assert_glove_tiers(&lines);
    assert_eq!(classified_lines(HYBRID_GLOVES), lines);

    let ring = classified_lines(&HYBRID_GLOVES.replace("Runeforged Secured Wraps", "Gold Ring"));
    for text in [
        "+2 to Level of all Projectile Skills",
        "Projectiles move at 38% increased Speed",
    ] {
        let line = ring.iter().find(|line| line["text"] == text).unwrap();
        assert!(line.get("tier").is_none(), "{line}");
        assert!(line.get("affix").is_none(), "{line}");
    }
}

#[test]
fn ambiguous_merged_defence_and_life_lines_have_no_tier_or_affix() {
    pobr_wasm::init_data_from_dir(pobr_gamedata::current_data_dir().to_str().unwrap()).unwrap();
    let text = "\
Rarity: RARE
Display Fixture
Runeforged Secured Wraps
Item Level: 81
56% increased Evasion and Energy Shield
+50 to maximum Life";
    let lines = classified_lines(text);
    for modifier in [
        "56% increased Evasion and Energy Shield",
        "+50 to maximum Life",
    ] {
        let line = lines.iter().find(|line| line["text"] == modifier).unwrap();
        assert_eq!(line["kind"], "explicit", "{line}");
        assert!(line.get("tier").is_none(), "{line}");
        assert!(line.get("tier_total").is_none(), "{line}");
        assert!(line.get("affix").is_none(), "{line}");
    }
}

#[test]
fn only_active_native_lines_reserve_affix_slots() {
    pobr_wasm::init_data_from_dir(pobr_gamedata::current_data_dir().to_str().unwrap()).unwrap();
    let text = "\
Rarity: RARE
Display Fixture
Runeforged Secured Wraps
Item Level: 81
Variant: First
Variant: Second
Selected Variant: 1

56% increased Evasion and Energy Shield
+50 to maximum Life";
    let inactive = classified_lines(&format!("{text}\n{{variant:2}}+30 to maximum Mana"));
    let custom = classified_lines(&format!("{text}\n{{custom}}+30 to maximum Mana"));
    let expected = [
        json!({"text": "56% increased Evasion and Energy Shield", "kind": "explicit"}),
        json!({"text": "+50 to maximum Life", "kind": "explicit"}),
    ];
    assert_eq!(inactive[inactive.len() - 3..inactive.len() - 1], expected);
    assert_eq!(custom[custom.len() - 3..custom.len() - 1], expected);
    assert_eq!(
        inactive.last().unwrap(),
        &json!({"text": "+30 to maximum Mana", "kind": "explicit"})
    );
    assert_eq!(
        custom.last().unwrap(),
        &json!({"text": "+30 to maximum Mana", "kind": "explicit", "modifier_sources": ["custom"]})
    );

    // Without an explicit selection the last variant is active in the engine.
    let selected =
        format!("{text}\n{{variant:2}}+30 to maximum Mana").replace("Selected Variant: 1\n", "");
    let selected = classified_lines(&selected);
    assert_eq!(selected[selected.len() - 3]["tier"], 4);
    assert_eq!(selected[selected.len() - 2]["tier"], 6);
    assert_eq!(selected.last().unwrap()["affix"], "prefix");
}

fn assert_variant_rolls(headers: &str, rolls: [&str; 2], active: [bool; 2]) {
    let header = RARE_ITEM.split_once("\n+190 to maximum Life").unwrap().0;
    let values = ["+190 to maximum Life", "+34% to Cold Resistance"];
    let text = format!(
        "{header}\n{headers}\n{}{}\n{}{}",
        rolls[0], values[0], rolls[1], values[1]
    );
    let reference = classified_lines(RARE_ITEM);
    let lines = classified_lines(&text);
    let explicit: Vec<_> = lines
        .iter()
        .filter(|line| line["kind"] == "explicit")
        .collect();
    assert_eq!(explicit.len(), 2, "{text}: {lines:?}");
    for (index, value) in values.into_iter().enumerate() {
        let expected = reference.iter().find(|line| line["text"] == value).unwrap();
        assert!(expected.get("tier").is_some(), "{expected}");
        if active[index] {
            assert_eq!(explicit[index], expected, "{text}");
        } else {
            assert_eq!(
                explicit[index],
                &json!({"text": value, "kind": "explicit"}),
                "{text}"
            );
        }
    }
    let item = pobr_core::item_text::parse_pob_xml_item(&text).unwrap();
    let active_values: Vec<_> = values
        .into_iter()
        .zip(active)
        .filter_map(|(value, active)| active.then_some(value))
        .collect();
    assert_eq!(item.modifier_texts, active_values, "{text}");
}

#[test]
fn all_legacy_alt_slots_follow_their_enable_flag() {
    pobr_wasm::init_data_from_dir(pobr_gamedata::current_data_dir().to_str().unwrap()).unwrap();
    for suffix in ["", " Two", " Three", " Four", " Five"] {
        for enabled in [false, true] {
            assert_variant_rolls(
                &format!(
                    "Variant: First\nVariant: Second\nSelected Variant: 1\nHas Alt Variant{suffix}: {enabled}\nSelected Alt Variant{suffix}: 2"
                ),
                ["{variant:1}", "{variant:2}"],
                [true, enabled],
            );
        }
    }
}

#[test]
fn version_base_and_group_selection_match_calculation() {
    pobr_wasm::init_data_from_dir(pobr_gamedata::current_data_dir().to_str().unwrap()).unwrap();
    for selected in [1, 2] {
        let active = [selected == 1, selected == 2];
        for (label, rolls) in [
            ("Version", ["{version:1}", "{version:2}"]),
            ("Base Variant", ["{base:1}", "{base:2}"]),
        ] {
            assert_variant_rolls(
                &format!("{label}: First\n{label}: Second\nSelected {label}: {selected}"),
                rolls,
                active,
            );
        }
        assert_variant_rolls(
            &format!(
                "Version: Current\nVariant: First\nVariant: Second\nSelected Variant Group: 1={selected}"
            ),
            [
                "{version:1}{group:1}{variant:1}",
                "{version:1}{group:1}{variant:2}",
            ],
            active,
        );
    }
    assert_variant_rolls(
        "Version: Current\nVariant: First\nVariant: Second\nSelected Variant: 1\nHas Alt Variant: true\nSelected Alt Variant: 2",
        ["{variant:1}", "{variant:2}"],
        [true, false],
    );
}

#[test]
fn influence_ailment_duration_tiers_exclude_alloy_from_the_same_category() {
    pobr_wasm::init_data_from_dir(pobr_gamedata::current_data_dir().to_str().unwrap()).unwrap();
    let modifier = "10% increased Duration of Damaging Ailments on Enemies";
    let text = format!(
        "Rarity: RARE\nSource Fixture\nRuneforged Secured Wraps\nItem Level: 81\n{modifier}"
    );
    let lines = classified_lines(&text);
    let duration = lines.iter().find(|line| line["text"] == modifier).unwrap();
    assert_eq!(duration["kind"], "explicit");
    assert_eq!(duration["tier"], 2);
    assert_eq!(duration["tier_total"], 2);
    assert!(duration.get("modifier_sources").is_none());
    assert_eq!(classified_lines(&text), lines);
}

#[test]
fn tagged_catalyst_roll_uses_original_tier_and_keeps_display_values() {
    pobr_wasm::init_data_from_dir(pobr_gamedata::current_data_dir().to_str().unwrap()).unwrap();
    let text = "\
Rarity: RARE
Display Fixture
Gold Ring
Item Level: 79
Quality (Cold Modifiers): +20%
Implicits: 0
{tags:Damage,cold,damage,attack}{unrelated}Adds 26 to 38 Cold damage to Attacks";
    let lines = classified_lines(text);
    let line = lines.last().unwrap();
    assert_eq!(line["text"], "Adds 26 to 38 Cold damage to Attacks");
    assert_eq!(line["mod_tags"], json!(["attack", "cold", "damage"]));
    assert_eq!(line["affix"], "prefix");
    assert_eq!(line["tier"], 1);
    let draft = pobr_item::ItemDraft::parse(text).unwrap();
    assert_eq!(
        draft,
        pobr_item::ItemDraft::parse(&draft.build_raw()).unwrap()
    );
    let calc_item = pobr_core::ingest::item_text::parse_pob_xml_item(text).unwrap();
    assert_eq!(
        calc_item.modifier_texts,
        ["Adds 26 to 38 Cold damage to Attacks"]
    );
    let unscalable = classified_lines(&text.replace("{unrelated}", "{unscalable}"));
    assert!(unscalable.last().unwrap().get("tier").is_none());
    let old = classified_lines(&text.replace("Quality (Cold Modifiers)", "Quality"));
    assert!(old.last().unwrap().get("tier").is_none());
}

#[test]
fn named_catalyst_keeps_competing_tiers_unknown() {
    pobr_wasm::init_data_from_dir(pobr_gamedata::current_data_dir().to_str().unwrap()).unwrap();
    let item =
        "Rarity: RARE\nDisplay Fixture\nGold Ring\nItem Level: 79\nQuality (Cold Modifiers): +20%";
    let roll = "Adds 24 to 36 Cold damage to Attacks";
    let untagged = classified_lines(&format!("{item}\n{roll}"));
    assert_eq!(
        untagged.last().unwrap(),
        &json!({"text": roll, "kind": "explicit"})
    );

    let tagged = classified_lines(&format!("{item}\n{{tags:cold}}{roll}"));
    let cold = tagged.last().unwrap();
    assert_eq!(cold["text"], roll);
    assert_eq!(cold["mod_tags"], json!(["cold"]));
    assert_eq!(cold["affix"], "prefix");
    assert_eq!(cold["tier"], 2);

    let unscalable = classified_lines(&format!("{item}\n{{unscalable}}{roll}"));
    let raw = unscalable.last().unwrap();
    assert_eq!(raw["text"], roll);
    assert_eq!(raw["affix"], "prefix");
    assert_eq!(raw["tier"], 1);
}

#[test]
fn untagged_catalysed_ring_matches_supported_rolls_and_retains_ambiguity() {
    pobr_wasm::init_data_from_dir(pobr_gamedata::current_data_dir().to_str().unwrap()).unwrap();
    let text = "\
Rarity: RARE
Display Fixture
Gold Ring
Item Level: 79
Quality (Cold Modifiers): +20%
Implicits: 1
9% increased Rarity of Items found
Adds 10 to 25 Physical Damage to Attacks
Adds 26 to 38 Cold damage to Attacks
+16% to all Elemental Resistances
+43% to Cold Resistance
+35% to Lightning Resistance
{desecrated}30% increased Cold Damage";
    let lines = classified_lines(text);
    for (value, side, tier, tag) in [
        (
            "Adds 10 to 25 Physical Damage to Attacks",
            "prefix",
            2,
            None,
        ),
        (
            "Adds 26 to 38 Cold damage to Attacks",
            "prefix",
            1,
            Some("cold"),
        ),
        ("+43% to Cold Resistance", "suffix", 2, Some("cold")),
        ("+35% to Lightning Resistance", "suffix", 3, None),
    ] {
        let line = lines.iter().find(|line| line["text"] == value).unwrap();
        assert_eq!(line["affix"], side, "{line}");
        assert_eq!(line["tier"], tier, "{line}");
        assert_eq!(line["mod_tags"][0].as_str(), tag, "{line}");
    }
    let all_res = lines
        .iter()
        .find(|line| line["text"] == "+16% to all Elemental Resistances")
        .unwrap();
    assert!(all_res.get("tier").is_none());
    let special = lines.last().unwrap();
    assert_eq!(special["modifier_sources"], json!(["desecrated"]));
    assert!(special.get("tier").is_none());
    assert!(
        lines.iter().any(|line| line["kind"] == "implicit"
            && line["text"] == "9% increased Rarity of Items found")
    );
}

#[test]
fn independent_prefix_rejects_a_merge_that_would_require_four_prefixes() {
    pobr_wasm::init_data_from_dir(pobr_gamedata::current_data_dir().to_str().unwrap()).unwrap();
    let text = "\
Rarity: RARE
Display Fixture
Noble Circlet
Item Level: 79
+44 to maximum Energy Shield
93% increased Energy Shield
+135 to maximum Life
+31% to Fire Resistance
+38% to Cold Resistance";
    let lines = classified_lines(text);
    for (value, tier) in [
        ("+44 to maximum Energy Shield", 3),
        ("93% increased Energy Shield", 1),
        ("+135 to maximum Life", 2),
    ] {
        let line = lines.iter().find(|line| line["text"] == value).unwrap();
        assert_eq!(line["affix"], "prefix", "{line}");
        assert_eq!(line["tier"], tier, "{line}");
    }
}

#[test]
fn out_of_range_ring_damage_is_not_assigned_a_nearest_or_quality_adjusted_tier() {
    pobr_wasm::init_data_from_dir(pobr_gamedata::current_data_dir().to_str().unwrap()).unwrap();
    let text = "Rarity: RARE\nDisplay Fixture\nGold Ring\nItem Level: 79\nQuality: 20\nAdds 26 to 38 Cold damage to Attacks\n+43% to Cold Resistance";
    let lines = classified_lines(text);
    let damage = lines
        .iter()
        .find(|line| line["text"] == "Adds 26 to 38 Cold damage to Attacks")
        .unwrap();
    assert_eq!(damage["kind"], "explicit");
    assert!(damage.get("tier").is_none());
    assert!(damage.get("affix").is_none());
    assert!(damage.get("modifier_sources").is_none());
    assert!(
        lines
            .iter()
            .any(|line| line["text"] == "+43% to Cold Resistance" && line["tier"] == 1)
    );
}

#[test]
fn staged_releasing_data_keeps_special_tiers_available_and_resets_cached_pools() {
    staged_pack::init(None);
    let reference = classified_lines(HYBRID_GLOVES);
    assert_glove_tiers(&reference);
    assert_eq!(classified_lines(HYBRID_GLOVES), reference);

    staged_pack::init(Some((
        "overlay/trade_catalog.json",
        br#"{"bases":[],"search_mods":[]}"#,
    )));
    let without_special = classified_lines(HYBRID_GLOVES);
    let projectile = without_special
        .iter()
        .find(|line| line["text"] == "+2 to Level of all Projectile Skills")
        .unwrap();
    assert!(projectile.get("tier").is_none());
    assert!(
        without_special
            .iter()
            .any(|line| line["text"] == "+74 to Evasion Rating" && line["tier"] == 1)
    );

    staged_pack::init(None);
    assert_eq!(classified_lines(HYBRID_GLOVES), reference);
}
