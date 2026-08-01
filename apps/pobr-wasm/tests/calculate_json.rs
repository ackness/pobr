//! Integration tests for `calculate_json` (default features, run on the host).
//!
//! Covers: the standard life pipeline `(base + Σbase) * (1 + Σinc/100)`,
//! empty-input defaults, resistance pass-through, unparseable modifiers
//! landing in `unsupported_modifiers`, and errors on invalid JSON.

use serde_json::Value;

use pobr_wasm::calculate_json;

/// Initializes game data (modifier parsing needs the engine rules; idempotent within the process).
fn init_data() {
    pobr_wasm::init_data_from_dir(&pobr_gamedata::current_data_dir().to_string_lossy())
        .expect("init game data from repo data dir");
}

/// Parses `calculate_json`'s output into a JSON value, asserting success and returning it.
fn run(input: &str) -> Value {
    let json = calculate_json(input).expect("calculate_json should succeed");
    serde_json::from_str(&json).expect("output is valid json")
}

#[test]
fn computes_life_with_base_and_increased_modifiers() {
    init_data();
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
    init_data();
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
    init_data();
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
fn collects_unparseable_modifier_as_unsupported() {
    // The engine never errors on unrecognized text — the whole line goes into unsupported_modifiers.
    init_data();
    let input = r#"{
        "base_life": 100,
        "modifiers": ["this is not a real modifier"]
    }"#;

    // Act
    let out = run(input);

    // Assert
    assert_eq!(out["life"].as_f64().unwrap(), 100.0);
    let unsupported = out["unsupported_modifiers"].as_array().unwrap();
    assert_eq!(unsupported.len(), 1);
}

#[test]
fn errors_on_modifiers_without_initialized_data() {
    // Non-empty modifiers with uninitialized data -> an explicit error
    // (fail-fast, never silently dropping mod lines).
    // Note: this test case relies on nextest running each test in its own
    // process (other test cases sharing a process may have already called init).
    let input = r#"{ "base_life": 100, "modifiers": ["+10 to maximum life"] }"#;
    if pobr_wasm::is_data_ready() {
        return; // Already initialized by a shared process — skip (cargo test's single-process mode).
    }
    let result = calculate_json(input);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("game data"));
}
