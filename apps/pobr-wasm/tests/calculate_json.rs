//! `calculate_json` 的集成测试（默认 features，宿主运行）。
//!
//! 覆盖：life 标准管线 `(base + Σbase) * (1 + Σinc/100)`、空输入默认值、
//! 抗性透传、无法解析的 modifier 进入 `unsupported_modifiers`、非法 JSON 报错。

use serde_json::Value;

use pobr_wasm::calculate_json;

/// 解析 `calculate_json` 的输出为 JSON 值，断言成功并返回。
fn run(input: &str) -> Value {
    let json = calculate_json(input).expect("calculate_json should succeed");
    serde_json::from_str(&json).expect("output is valid json")
}

#[test]
fn computes_life_with_base_and_increased_modifiers() {
    // Arrange: base 100 +50 flat, +20% increased.
    let input = r#"{
        "base_life": 100,
        "modifiers": [
            "+50 to maximum life",
            "20% increased maximum life"
        ]
    }"#;

    // Act
    let out = run(input);

    // Assert: (100 + 50) * 1.20 = 180.
    assert_eq!(out["life"].as_f64().unwrap(), 180.0);
}

#[test]
fn passes_base_life_through_when_no_modifiers() {
    // Arrange
    let input = r#"{ "base_life": 75 }"#;

    // Act
    let out = run(input);

    // Assert
    assert_eq!(out["life"].as_f64().unwrap(), 75.0);
}

#[test]
fn applies_more_multiplier_on_top_of_increased() {
    // Arrange: (100 + 0) * 1.10 * 1.10 = 121.
    let input = r#"{
        "base_life": 100,
        "modifiers": [
            "10% increased maximum life",
            "10% more maximum life"
        ]
    }"#;

    // Act
    let out = run(input);

    // Assert
    let life = out["life"].as_f64().unwrap();
    assert!((life - 121.0).abs() < 1e-9, "expected 121, got {life}");
}

#[test]
fn passes_resistances_through() {
    // Arrange
    let input = r#"{
        "base_fire_resistance": 30,
        "base_cold_resistance": 20,
        "base_lightning_resistance": 10
    }"#;

    // Act
    let out = run(input);

    // Assert
    assert_eq!(out["fire_resistance"].as_f64().unwrap(), 30.0);
    assert_eq!(out["cold_resistance"].as_f64().unwrap(), 20.0);
    assert_eq!(out["lightning_resistance"].as_f64().unwrap(), 10.0);
}

#[test]
fn defaults_to_zero_for_empty_object() {
    // Arrange
    let input = "{}";

    // Act
    let out = run(input);

    // Assert
    assert_eq!(out["life"].as_f64().unwrap(), 0.0);
    assert_eq!(out["mana"].as_f64().unwrap(), 0.0);
    assert!(out["unsupported_modifiers"].as_array().unwrap().is_empty());
}

#[test]
fn collects_unsupported_modifiers_without_failing() {
    // Arrange: "mirrored" is recognized as a non-error unsupported modifier.
    let input = r#"{
        "base_life": 100,
        "modifiers": ["mirrored", "+10 to maximum life"]
    }"#;

    // Act
    let out = run(input);

    // Assert: the supported mod still applies, the unsupported one is reported.
    assert_eq!(out["life"].as_f64().unwrap(), 110.0);
    let unsupported = out["unsupported_modifiers"].as_array().unwrap();
    assert_eq!(unsupported.len(), 1);
    assert_eq!(unsupported[0].as_str().unwrap(), "mirrored");
}

#[test]
fn errors_on_invalid_json() {
    // Arrange
    let input = "{ not json";

    // Act
    let result = calculate_json(input);

    // Assert
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("invalid input json"));
}

#[test]
fn errors_on_unknown_field() {
    // Arrange: deny_unknown_fields guards the input contract.
    let input = r#"{ "base_life": 100, "bogus": 1 }"#;

    // Act
    let result = calculate_json(input);

    // Assert
    assert!(result.is_err());
}

#[test]
fn errors_on_unparseable_modifier() {
    // Arrange: a modifier form the parser rejects (hard ParseError, not Unsupported).
    let input = r#"{
        "base_life": 100,
        "modifiers": ["this is not a real modifier"]
    }"#;

    // Act
    let result = calculate_json(input);

    // Assert
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("failed to parse modifier"));
}
