use pobr_build::usage::{LineStatus, ModifierStatus};
use pobr_build::{DataOrchestratorOptions, diagnose, parse_build};
use pobr_core::display_value;

#[test]
fn charge_durations_scale_game_base_and_reconstruct_without_changing_other_outputs() {
    let data = crate::test_data();
    // The game's Charges tooltip gives all three kinds a 15-second base.
    // Generic and kind-specific increases add; more modifiers multiply.
    let lines = [
        "20% increased Endurance, Frenzy and Power Charge Duration",
        "25% increased Power Charge Duration",
        "25% reduced Endurance Charge Duration",
        "25% increased Frenzy Charge Duration",
        "10% increased Charge Duration",
        "20% more Power Charge Duration",
        "10% more Charge Duration",
        "5% increased Frenzy Charge Duration per Frenzy Charge",
    ];
    for reference in [false, true] {
        for count in [0, 3] {
            let enabled = count > 0;
            let build = parse_build(&format!(r#"<PathOfBuilding2>
<Build level="80" className="Witch" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="FireballPlayer" level="20" enabled="true"/></Skill></Skills>
<Config><Input name="useFrenzyCharges" boolean="{enabled}"/><Input name="overrideFrenzyCharges" number="{count}"/></Config>
</PathOfBuilding2>"#)).unwrap();
            let mut options = DataOrchestratorOptions::default();
            if reference {
                options = options.on_reference_basis();
            }
            let baseline = diagnose(&build, &data, &options).unwrap();
            options.extra_modifier_texts = lines.iter().map(|line| (*line).into()).collect();
            let result = diagnose(&build, &data, &options).unwrap();
            let session = &result.session;
            assert!(session.unsupported_modifier_texts().is_empty());
            // Power: 15 * (1 + (20 + 25 + 10)/100) * 1.2 * 1.1 = 30.69.
            // Frenzy: 15 * (1 + (20 + 25 + 10 + 5*count)/100) * 1.1.
            // Endurance: 15 * (1 + (20 - 25 + 10)/100) * 1.1 = 17.325.
            for (stat, expected) in [
                ("PowerChargesDuration", 30.69),
                (
                    "FrenzyChargesDuration",
                    if count == 3 { 28.05 } else { 25.575 },
                ),
                ("EnduranceChargesDuration", 17.325),
            ] {
                assert_eq!(display_value(baseline.session.output(), stat), 15.0);
                assert!(
                    (display_value(session.output(), stat) - expected).abs() < 1e-9,
                    "{stat}: got {}, expected {expected}; count {count}, reference {reference}",
                    display_value(session.output(), stat)
                );
                let detail = session.derived_detail(stat).unwrap();
                assert_eq!(
                    detail.formula,
                    "max(0, BaseChargeDuration * (1 + ChargeDurationIncreased / 100) * ChargeDurationMore)"
                );
                let input = |id| detail.inputs.iter().find(|(name, _)| name == id).unwrap().1;
                let reconstructed = (input("BaseChargeDuration")
                    * (1.0 + input("ChargeDurationIncreased") / 100.0)
                    * input("ChargeDurationMore"))
                .max(0.0);
                assert!((reconstructed - expected).abs() < 1e-9);
            }
            let mut unchanged = session.output().clone();
            unchanged.charge_power_duration = 15.0;
            unchanged.charge_frenzy_duration = 15.0;
            unchanged.charge_endurance_duration = 15.0;
            assert_eq!(&unchanged, baseline.session.output());
            let usage = pobr_build::usage::line_usage(&result, &build, &data);
            for text in lines {
                let row = usage
                    .lines
                    .iter()
                    .find(|row| row.text.as_deref() == Some(text))
                    .unwrap();
                let expected = if count == 0 && text.ends_with("per Frenzy Charge") {
                    ModifierStatus::Inactive
                } else {
                    assert_eq!(row.status, LineStatus::Applied, "{row:?}");
                    ModifierStatus::Applied
                };
                assert!(!row.modifiers.is_empty(), "{row:?}");
                assert!(
                    row.modifiers
                        .iter()
                        .all(|modifier| modifier.status == expected),
                    "{row:?}"
                );
            }
            for (reduction, expected) in [(50, 7.5), (150, 0.0)] {
                options.extra_modifier_texts = vec![format!(
                    "{reduction}% reduced Endurance, Frenzy and Power Charge Duration"
                )];
                let reduced = diagnose(&build, &data, &options).unwrap();
                for stat in [
                    "PowerChargesDuration",
                    "FrenzyChargesDuration",
                    "EnduranceChargesDuration",
                ] {
                    assert_eq!(display_value(reduced.session.output(), stat), expected);
                }
            }
        }
    }
}
