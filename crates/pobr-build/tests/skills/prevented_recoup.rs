use pobr_build::{DataOrchestratorOptions, calculate_with_data_session, parse_build};

#[test]
fn prevented_recoup_carrier_values_apply_only_on_game_basis() {
    let data = crate::test_data();
    let build = parse_build(
        r#"<PathOfBuilding2><Build level="80" className="Warrior"/></PathOfBuilding2>"#,
    )
    .unwrap();
    let mut made_to_last_rate = 0.0;
    for percent in [5, 50] {
        let options = DataOrchestratorOptions {
            extra_modifier_texts: vec![
                "+10000 to Armour".into(),
                format!("{percent}% of Physical Damage prevented Recouped as Life"),
            ],
            ..Default::default()
        };
        let game = calculate_with_data_session(&build, &data, &options).unwrap();
        let reference =
            calculate_with_data_session(&build, &data, &options.on_reference_basis()).unwrap();
        assert!(game.unsupported_modifier_texts().is_empty());
        assert!(game.output().life_recoup_rate > 0.0);
        assert_eq!(reference.output().life_recoup_rate, 0.0);
        if percent == 5 {
            made_to_last_rate = game.output().life_recoup_rate;
        } else {
            // Wandering Reliquary's 50% returns ten times Made to Last's 5%.
            assert!((game.output().life_recoup_rate - made_to_last_rate * 10.0).abs() < 1e-8);
        }
    }
}
