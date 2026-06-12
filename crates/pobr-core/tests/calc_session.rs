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

/// 资源池最终总量（vendor PerStat 分母 = actor output，ModStore.lua:440-460）：
/// `pool_total` 必须吃满 base×(1+inc)×more 全管线，与 perform 内 offence 池值
/// 同源——BASE-only（`base_sum`）会漏掉 inc/more 缩放。
#[test]
fn pool_total_applies_full_pool_pipeline() {
    // Arrange
    let input = MinimalInput {
        base_mana: 100.0,
        ..MinimalInput::default()
    };
    let mut session = CalculationSession::new(input);
    session
        .add_modifier_texts(["+200 to maximum Mana", "50% increased maximum Mana"])
        .unwrap();

    // Act + Assert：(100 + 200) × 1.5 = 450（base_sum 只会给 200）。
    assert_eq!(session.pool_total("MaximumMana"), 450.0);
    assert_eq!(session.base_sum("MaximumMana"), 200.0);

    // 池值与 perform 输出同源（同一 scaled_pool 管线）。
    let output = session.perform_minimal();
    assert_eq!(output.mana, 450.0);
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
