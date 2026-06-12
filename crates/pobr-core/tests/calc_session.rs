use pobr_core::calc::{CalculationSession, MinimalInput};

#[test]
fn session_parses_modifier_texts_and_calculates_minimal_output() {
    let input = MinimalInput {
        base_life: 1_000.0,
        base_mana: 100.0,
        base_fire_resistance: 0.0,
        base_cold_resistance: 0.0,
        base_lightning_resistance: 0.0,
        base_accuracy: 0.0,
        enemy_evasion: 0.0,
        base_hit_min: 100.0,
        base_hit_max: 200.0,
        base_action_rate: 2.0,
    };

    let mut session = CalculationSession::new(input);
    session
        .add_modifier_texts([
            "+50 to maximum Life",
            "20% increased maximum Life",
            "+35% to Fire Resistance",
            "Attacks deal 50% increased Physical Damage",
            "20% more Physical Damage",
            "10% increased Attack Speed",
        ])
        .unwrap();

    let output = session.perform_minimal();

    assert_eq!(output.life, 1_260.0);
    assert_eq!(output.fire_resistance, 35.0);
    assert_eq!(output.total_hit_avg, 270.0);
    assert_eq!(output.action_rate, 2.2);
    assert_eq!(output.dps, 594.0);
}

#[test]
fn session_preserves_accuracy_inputs_for_hit_chance_and_dps() {
    let input = MinimalInput {
        base_life: 1.0,
        base_mana: 1.0,
        base_fire_resistance: 0.0,
        base_cold_resistance: 0.0,
        base_lightning_resistance: 0.0,
        base_accuracy: 400.0,
        enemy_evasion: 1_000.0,
        base_hit_min: 100.0,
        base_hit_max: 100.0,
        base_action_rate: 1.0,
    };
    let mut session = CalculationSession::new(input);
    session
        .add_modifier_texts(["+200 to Accuracy Rating"])
        .unwrap();

    let output = session.perform_minimal();
    let expected_hit_chance = pobr_core::calc::hit_chance(1_000.0, 600.0);

    assert_eq!(output.total_hit_avg, 100.0);
    assert_eq!(output.hit_chance, expected_hit_chance);
    assert_eq!(output.dps, 100.0 * expected_hit_chance);
}

/// 属性最终总量（PoB2 `calculateAttributes`，CalcPerform.lua:381-388）：
/// `round((class_base + Σbase) × (1 + Σinc/100) × Πmore)`，下限 0。
/// `N% increased Dexterity` 类词条必须缩放含职业起始在内的全部 BASE。
#[test]
fn attribute_total_applies_increased_attribute_modifiers() {
    // Arrange
    let mut session = CalculationSession::new(MinimalInput::default());
    session
        .add_modifier_texts(["+100 to Dexterity", "8% increased Dexterity"])
        .unwrap();

    // Act + Assert：round((20 + 100) × 1.08) = round(129.6) = 130。
    assert_eq!(session.attribute_total("Dexterity", 20.0), 130.0);
    // 无 INC 词条的属性 = class_base + Σbase 直通（Strength 无任何词条）。
    assert_eq!(session.attribute_total("Strength", 7.0), 7.0);
}

#[test]
fn session_preserves_unsupported_modifier_texts() {
    let mut session = CalculationSession::new(MinimalInput::default());
    session.add_modifier_texts(["Mirrored"]).unwrap();

    assert_eq!(session.unsupported_modifier_texts(), ["Mirrored"]);
}

#[test]
fn session_returns_parse_error_for_unknown_modifier_text() {
    let mut session = CalculationSession::new(MinimalInput::default());

    let error = session
        .add_modifier_texts(["not a real modifier"])
        .unwrap_err();

    assert_eq!(error.input, "not a real modifier");
}
