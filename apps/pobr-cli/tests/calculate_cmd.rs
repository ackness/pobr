//! Integration tests: calling `pobr_cli` library functions directly (pure functions, no IO).

use pobr_cli::{
    CalculateBuildRequest, CalculateRequest, ParseItemRequest, calculate, calculate_build,
    decode_code, encode_code, parse_item, parse_mod,
};
use pobr_data::monster::EnemyTier;

fn base_input() -> pobr_core::calc::MinimalInput {
    pobr_core::calc::MinimalInput {
        base_life: 1000.0,
        ..Default::default()
    }
}

#[test]
fn calculate_applies_base_life_modifier_text() {
    // Arrange: base_life=1000, plus a parseable "+50 to maximum Life" -> expect life=1050.
    let req = CalculateRequest {
        input: base_input(),
        modifier_texts: vec!["+50 to maximum Life".to_string()],
    };

    // Act
    let result = calculate(&req).expect("calculate succeeds");

    // Assert
    assert_eq!(result.output.life, 1050.0);
    assert!(result.unsupported.is_empty());
}

#[test]
fn calculate_collects_unsupported_modifier_texts() {
    // "mirrored" is Unsupported in the parser (no error), and should be collected.
    let req = CalculateRequest {
        input: base_input(),
        modifier_texts: vec!["mirrored".to_string()],
    };

    let result = calculate(&req).expect("calculate succeeds even with unsupported text");

    assert_eq!(result.output.life, 1000.0);
    assert_eq!(result.unsupported, vec!["mirrored".to_string()]);
}

#[test]
fn calculate_collects_unparseable_modifier_text_as_unsupported() {
    // The engine never errors on unrecognized text — the whole line goes into the unsupported collection.
    let req = CalculateRequest {
        input: base_input(),
        modifier_texts: vec!["garbage zzz".to_string()],
    };

    let result = calculate(&req).expect("engine never errors on unknown text");
    assert_eq!(result.output.life, 1000.0);
    assert_eq!(result.unsupported, vec!["garbage zzz".to_string()]);
}

#[test]
fn calculate_increased_life_scales_correctly() {
    // (1000 + 100) * (1 + 50/100) = 1650
    let req = CalculateRequest {
        input: base_input(),
        modifier_texts: vec![
            "+100 to maximum Life".to_string(),
            "50% increased maximum Life".to_string(),
        ],
    };

    let result = calculate(&req).expect("calculate succeeds");
    assert_eq!(result.output.life, 1650.0);
}

#[test]
fn parse_mod_returns_parsed_for_known_text() {
    let report = parse_mod("50% increased maximum Life").expect("parse succeeds");

    assert_eq!(report.status, "Parsed");
    assert_eq!(report.mods.len(), 1);
    assert_eq!(report.mods[0].name, "MaximumLife");
    assert_eq!(report.mods[0].mod_type, "Inc");
    assert_eq!(report.mods[0].value, Some(50.0));
    assert!(report.unparsed.is_none());
}

#[test]
fn parse_mod_returns_unsupported_for_mirrored() {
    let report = parse_mod("mirrored").expect("parse does not error on unsupported");

    assert_eq!(report.status, "Unsupported");
    assert!(report.mods.is_empty());
    assert_eq!(report.unparsed.as_deref(), Some("mirrored"));
}

#[test]
fn parse_mod_reports_unsupported_for_unparseable() {
    // The engine never errors on unrecognized text — Unsupported, with the raw text going into unparsed.
    let report = parse_mod("garbage zzz").expect("engine never errors on unknown text");
    assert_eq!(report.status, "Unsupported");
    assert_eq!(report.unparsed.as_deref(), Some("garbage zzz"));
}

#[test]
fn parse_item_parses_rare_ring() {
    // parse_item now calls the real parser (item_text::parse_item_text + item::ingest_item).
    let req = ParseItemRequest {
        text: "Rarity: Rare\nGloom Coil\nIron Ring\n--------\n+40 to maximum Life".to_string(),
    };
    let report = parse_item(&req).expect("parse_item succeeds");
    assert_eq!(report.base, "Iron Ring");
    assert_eq!(report.rarity, "Rare");
    assert_eq!(report.quality, 0);
    // "+40 to maximum Life" is a parseable mod line and should show up in modifiers.
    assert!(
        report
            .modifiers
            .iter()
            .any(|m| m.name.contains("MaximumLife")),
        "expected MaximumLife modifier, got: {:?}",
        report.modifiers
    );
    assert!(
        report.unsupported.is_empty(),
        "no unsupported: {:?}",
        report.unsupported
    );
}

#[test]
fn parse_item_collects_unsupported_modifiers() {
    // "mirrored" is Unsupported, and should be collected into unsupported without blocking parsing.
    let req = ParseItemRequest {
        text: "Rarity: Normal\nIron Ring\n--------\nmirrored".to_string(),
    };
    let report = parse_item(&req).expect("parse_item succeeds even with unsupported mods");
    assert_eq!(report.base, "Iron Ring");
    assert_eq!(report.unsupported, vec!["mirrored".to_string()]);
}

#[test]
fn parse_item_errors_on_missing_rarity() {
    // Missing the Rarity: header, returns a structural error.
    let req = ParseItemRequest {
        text: "Iron Ring\n--------\n+40 to maximum Life".to_string(),
    };
    let err = parse_item(&req).expect_err("missing rarity should error");
    assert!(
        format!("{err}").contains("Rarity") || format!("{err}").contains("item text"),
        "error should mention rarity or item text: {err}"
    );
}

#[test]
fn encode_then_decode_roundtrips_xml() {
    let xml = "<PathOfBuilding><Build level=\"90\"/></PathOfBuilding>";

    let code = encode_code(xml).expect("encode succeeds");
    assert!(!code.is_empty());
    // URL-safe base64 should never contain '+' or '/'.
    assert!(!code.contains('+'));
    assert!(!code.contains('/'));

    let decoded = decode_code(&code).expect("decode succeeds");
    assert_eq!(decoded, xml);
}

#[test]
fn decode_errors_on_invalid_code() {
    let err = decode_code("!!!not-valid-base64!!!").expect_err("invalid code errors");
    assert!(!format!("{err}").is_empty());
}

// calculate-build: PoB Build Code -> a full Build -> end-to-end attributed calculation

const DEADEYE_CODE: &str = include_str!("../../../examples/demo-bd-test/ninja-bd-deadeye.txt");

fn repo_data_dir() -> std::path::PathBuf {
    pobr_gamedata::repo_data_root().join(pobr_gamedata::data_version())
}

#[test]
fn calculate_build_summarizes_and_computes_from_code() {
    let req = CalculateBuildRequest {
        code: DEADEYE_CODE.to_string(),
        data_dir: repo_data_dir(),
        enemy_level: 80,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: true,
    };
    let report = calculate_build(&req).expect("calculate-build succeeds");

    // Build summary: identity and per-source counts are recovered by the production parser.
    assert_eq!(report.build.level, 98);
    assert_eq!(report.build.class_name, "Ranger");
    assert!(report.build.ascendancy_name.contains("Deadeye"));
    assert!(report.build.allocated_node_count > 0);
    assert!(report.build.equipped_item_count > 0);
    assert!(report.build.socket_group_count > 0);

    // Output: CharacterBase plus equipment plus passives raise life; hit chance falls within a valid range.
    let char_base_life = 28.0 + 12.0 * 98.0 + 2.0 * 7.0;
    assert!(
        report.output.life > char_base_life,
        "life {} should exceed char base {}",
        report.output.life,
        char_base_life
    );
    assert!(report.output.life.is_finite());
    assert!(report.output.hit_chance >= 0.0 && report.output.hit_chance <= 1.0);
    // PoE2's base crit multiplier is 2.0 (no additional damage-increase mod lines).
    assert_eq!(report.output.crit_multiplier, 2.0);

    // gap B: the report carries passive-tree version reconciliation
    // diagnostics (a real PoB2 build always has <Spec treeVersion>). Doesn't
    // pin the exact unknown-node count (data-version dependent), only checks
    // that it's captured and self-consistent.
    let diag = &report.tree_version;
    assert!(
        diag.build_tree_version.is_some(),
        "build's treeVersion should be captured"
    );
    assert_eq!(
        diag.unknown_node_count,
        diag.unknown_nodes.len(),
        "unknown_node_count should be self-consistent with unknown_nodes"
    );
}

#[test]
fn calculate_build_panel_vs_effective_hit_chance() {
    let base = CalculateBuildRequest {
        code: DEADEYE_CODE.to_string(),
        data_dir: repo_data_dir(),
        enemy_level: 80,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: false,
    };
    let panel = calculate_build(&base).expect("panel");
    let effective = calculate_build(&CalculateBuildRequest {
        mode_effective: true,
        ..base.clone()
    })
    .expect("effective");

    // The panel basis ignores enemy evasion, so hit chance should be >= the effective basis.
    assert!(
        panel.output.hit_chance >= effective.output.hit_chance,
        "panel {} >= effective {}",
        panel.output.hit_chance,
        effective.output.hit_chance
    );
}

#[test]
fn calculate_build_errors_on_invalid_code() {
    let req = CalculateBuildRequest {
        code: "!!!not-valid!!!".to_string(),
        data_dir: repo_data_dir(),
        enemy_level: 0,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: true,
    };
    let err = calculate_build(&req).expect_err("invalid code should error");
    assert!(!format!("{err}").is_empty());
}
