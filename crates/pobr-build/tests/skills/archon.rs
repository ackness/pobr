//! Archon shared bonuses are opt-in and excluded from exporter-reference runs.
use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data_session, parse_build};
use pobr_core::display_value;
use pobr_gamedata::{GameData, data_version, repo_data_root};

#[test]
fn archon_config_applies_shared_bonuses_and_reference_basis_omits_absent_rules() {
    let data = BuildData::load(&GameData::new(repo_data_root().join(data_version()))).unwrap();
    let options = DataOrchestratorOptions {
        extra_modifier_texts: [
            "Archon Buffs also grant 10% increased Movement Speed",
            "Archon Buffs also grant +20% to all Elemental Resistances",
            "10% increased effect of Archon Buffs on you",
            "30% increased Archon Buff duration",
            "Archon recovery period expires 10% faster",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
        ..Default::default()
    };
    for active in [false, true] {
        let build = parse_build(&format!(
            r#"<PathOfBuilding2>
<Build level="80" className="Witch"/>
<Config><Input name="conditionAffectedByArchon" boolean="{active}"/></Config>
</PathOfBuilding2>"#
        ))
        .unwrap();
        let game = calculate_with_data_session(&build, &data, &options).unwrap();
        let reference =
            calculate_with_data_session(&build, &data, &options.clone().on_reference_basis())
                .unwrap();
        assert!(game.unsupported_modifier_texts().is_empty());
        let expected_movement = if active { 1.11 } else { 1.0 };
        assert!(
            (display_value(game.output(), "EffectiveMovementSpeedMod") - expected_movement).abs()
                < 1e-9
        );
        assert_eq!(
            display_value(reference.output(), "EffectiveMovementSpeedMod"),
            1.0
        );
        assert_eq!(display_value(game.output(), "ArchonDuration"), 13.0);
        assert_eq!(display_value(reference.output(), "ArchonDuration"), 10.0);
        for stat in ["FireResist", "ColdResist", "LightningResist"] {
            let bonus =
                display_value(game.output(), stat) - display_value(reference.output(), stat);
            assert_eq!(bonus, if active { 22.0 } else { 0.0 });
        }
    }
}
