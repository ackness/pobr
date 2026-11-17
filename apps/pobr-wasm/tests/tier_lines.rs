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
