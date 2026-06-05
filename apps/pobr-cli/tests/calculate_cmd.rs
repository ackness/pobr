//! 集成测试：直接调用 `pobr_cli` 库函数（纯函数，IO 无关）。

use pobr_cli::{
    CalculateRequest, ParseItemRequest, calculate, decode_code, encode_code, parse_item, parse_mod,
};

fn base_input() -> pobr_core::calc::MinimalInput {
    pobr_core::calc::MinimalInput {
        base_life: 1000.0,
        ..Default::default()
    }
}

#[test]
fn calculate_applies_base_life_modifier_text() {
    // Arrange：base_life=1000，加一条可解析的 "+50 to maximum Life" → 期望 life=1050。
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
    // "mirrored" 在 parser 中是 Unsupported 状态（不报错），应被收集。
    let req = CalculateRequest {
        input: base_input(),
        modifier_texts: vec!["mirrored".to_string()],
    };

    let result = calculate(&req).expect("calculate succeeds even with unsupported text");

    assert_eq!(result.output.life, 1000.0);
    assert_eq!(result.unsupported, vec!["mirrored".to_string()]);
}

#[test]
fn calculate_errors_on_unparseable_modifier_text() {
    let req = CalculateRequest {
        input: base_input(),
        modifier_texts: vec!["garbage zzz".to_string()],
    };

    let err = calculate(&req).expect_err("garbage modifier text errors");
    let msg = format!("{err}");
    assert!(
        msg.contains("garbage zzz"),
        "error should mention input: {msg}"
    );
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
fn parse_mod_errors_on_unparseable() {
    let err = parse_mod("garbage zzz").expect_err("unparseable text errors");
    assert!(format!("{err}").contains("garbage zzz"));
}

#[test]
fn parse_item_parses_rare_ring() {
    // parse_item 现已调用真实解析器（item_text::parse_item_text + item::ingest_item）。
    let req = ParseItemRequest {
        text: "Rarity: Rare\nGloom Coil\nIron Ring\n--------\n+40 to maximum Life".to_string(),
    };
    let report = parse_item(&req).expect("parse_item succeeds");
    assert_eq!(report.base, "Iron Ring");
    assert_eq!(report.rarity, "Rare");
    assert_eq!(report.quality, 0);
    // "+40 to maximum Life" 是可解析词条，应出现在 modifiers 中。
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
    // "mirrored" 是 Unsupported，应收集进 unsupported 而不阻断解析。
    let req = ParseItemRequest {
        text: "Rarity: Normal\nIron Ring\n--------\nmirrored".to_string(),
    };
    let report = parse_item(&req).expect("parse_item succeeds even with unsupported mods");
    assert_eq!(report.base, "Iron Ring");
    assert_eq!(report.unsupported, vec!["mirrored".to_string()]);
}

#[test]
fn parse_item_errors_on_missing_rarity() {
    // 缺少 Rarity: 头，返回结构性错误。
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
    // URL-safe base64 不应含 '+' 或 '/'。
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
