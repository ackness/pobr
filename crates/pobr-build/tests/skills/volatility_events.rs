//! Instability restores only the explicit explosion snapshot; never loops events.
use pobr_build::{
    Build, BuildData, DataOrchestratorOptions, calculate_with_data_session, parse_build,
};
use pobr_core::calc::{
    CalculationSession, CompletedCalculationSession, MinimalInput,
    volatility_events::{QUANTITY, RATE, STATS},
};
use pobr_core::rules::config_interpreter::ConfigInputValue;
use pobr_core::{CalcConfig, Modifier, display_value};
use pobr_data::prelude::*;
use pobr_gamedata::{GameData, data_version, repo_data_root};

use crate::test_data as data;
fn build(config: &str, source: bool, cap: bool) -> Build {
    let nodes = if source { "43944" } else { "" };
    let mut build = parse_build(&format!(
        r#"<PathOfBuilding2>
<Build level="90" className="Ranger" mainSocketGroup="1"/>
<Tree activeSpec="1"><Spec treeVersion="0_4" nodes="{nodes}"/></Tree>
<Skills><Skill><Gem skillId="FireballPlayer" level="20"/></Skill></Skills>
<Config>{config}</Config></PathOfBuilding2>"#
    ))
    .unwrap();
    if cap {
        build
            .config
            .global_modifier_texts
            .push("Maximum Volatility is 30".into());
    }
    build
}
fn run(build: &Build, data: &BuildData) -> CompletedCalculationSession {
    calculate_with_data_session(build, data, &DataOrchestratorOptions::default()).unwrap()
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-8, "{a} != {b}");
}
fn native(
    quantity: Option<f64>,
    rate: Option<f64>,
    chance: Option<f64>,
    cap: Option<f64>,
    combat: bool,
) -> CalculationSession {
    let mut cfg = CalcConfig::attack().with_mode_combat(combat);
    if let Some(n) = quantity {
        cfg = cfg.with_multiplier(QUANTITY, n);
    }
    if let Some(n) = rate {
        cfg = cfg.with_multiplier(RATE, n);
    }
    let mut session = CalculationSession::new(MinimalInput {
        base_hit_min: 100.0,
        base_hit_max: 100.0,
        ..Default::default()
    })
    .with_config(cfg);
    if let Some(n) = chance {
        session.add_modifiers([Modifier::number(
            "VolatilityRefreshChance",
            ModType::Base,
            n,
        )]);
    }
    if let Some(n) = cap {
        session.add_modifiers([Modifier::number("VolatilityMaximum", ModType::Override, n)]);
    }
    session.track_modifier_reads();
    session.perform_minimal().unwrap();
    session
}

#[test]
fn volatility_native_boundaries_caps_and_no_recursive_rate() {
    let baseline = native(None, None, None, None, true);
    for (quantity, chance, cap, expected) in [
        (20.0, 25.0, None, 5.0),
        (20.0, 0.0, None, 0.0),
        (20.0, 100.0, None, 20.0),
        (20.0, 150.0, None, 20.0),
        (20.0, -20.0, None, 0.0),
        (250.0, 25.0, None, 50.0),
        (250.0, 25.0, Some(30.0), 7.5),
        (20.0, 100.0, Some(0.0), 0.0),
    ] {
        let session = native(Some(quantity), Some(2.0), Some(chance), cap, true);
        close(
            display_value(session.output(), "VolatilityRestoredPerExplosion"),
            expected,
        );
        close(
            display_value(session.output(), "VolatilityRestoredPerSecond"),
            2.0 * expected,
        );
        close(session.output().dps, baseline.output().dps);
        close(session.output().life, baseline.output().life);
        close(session.output().armour, baseline.output().armour);
        close(session.output().action_rate, baseline.output().action_rate);
    }
    for rate in [
        None,
        Some(0.0),
        Some(-1.0),
        Some(f64::NAN),
        Some(f64::INFINITY),
        Some(f64::MAX),
    ] {
        let session = native(Some(20.0), rate, Some(100.0), None, true);
        close(
            display_value(session.output(), "VolatilityRestoredPerExplosion"),
            20.0,
        );
        close(
            display_value(session.output(), "VolatilityRestoredPerSecond"),
            0.0,
        );
    }
    let fractional_rate = native(Some(20.0), Some(0.5), Some(25.0), None, true);
    close(
        display_value(fractional_rate.output(), "VolatilityRestoredPerSecond"),
        2.5,
    );
}

#[test]
fn volatility_source_off_noncombat_and_invalid_inputs_are_inactive() {
    for quantity in [
        None,
        Some(0.0),
        Some(-2.0),
        Some(1.5),
        Some(f64::NAN),
        Some(f64::INFINITY),
    ] {
        let session = native(quantity, Some(2.0), Some(25.0), Some(30.0), true);
        close(
            display_value(session.output(), "VolatilityRefreshChance"),
            25.0,
        );
        for id in &STATS[1..] {
            close(display_value(session.output(), id), 0.0);
        }
        assert!(session.read_variables().multipliers.contains(QUANTITY));
        assert!(session.read_variables().multipliers.contains(RATE));
        assert!(
            session
                .read_modifier_names()
                .iter()
                .any(|n| n.as_str() == "VolatilityRefreshChance")
        );
        close(session.output().volatility_events.maximum, 30.0);
    }
    for (chance, combat) in [(None, true), (Some(25.0), false), (Some(f64::NAN), true)] {
        let session = native(Some(20.0), Some(2.0), chance, Some(30.0), combat);
        close(
            display_value(session.output(), "VolatilityRefreshChance"),
            if !combat { 25.0 } else { 0.0 },
        );
        for id in &STATS[1..] {
            close(display_value(session.output(), id), 0.0);
        }
        if chance.is_none() {
            assert!(
                !session
                    .read_modifier_names()
                    .iter()
                    .any(|n| n.as_str() == "VolatilityMaximum")
            );
        }
    }
    let absent = native(None, None, None, None, true);
    assert!(absent.read_variables().multipliers.contains(QUANTITY));
    assert!(absent.read_variables().multipliers.contains(RATE));
}

#[test]
fn volatility_passive_source_xml_json_native_and_reference_basis_agree() {
    let data = data();
    let xml = r#"<Input name="volatilityExplosionQuantity" number="40"/><Input name="volatilityExplosionsPerSecond" number="0.5"/>"#;
    let with_source = build(xml, true, true);
    let session = run(&with_source, &data);
    assert!(
        session
            .unsupported_modifier_texts()
            .iter()
            .all(|line| !line.to_lowercase().contains("volatility")),
        "{:?}",
        session.unsupported_modifier_texts()
    );
    close(
        display_value(session.output(), "VolatilityRefreshChance"),
        25.0,
    );
    close(
        display_value(session.output(), "VolatilityExplosionQuantity"),
        30.0,
    );
    close(
        display_value(session.output(), "VolatilityRestoredPerExplosion"),
        7.5,
    );
    close(
        display_value(session.output(), "VolatilityRestoredPerSecond"),
        3.75,
    );
    let mut native = build("", true, true);
    native.config.multipliers.insert(QUANTITY.into(), 40.0);
    native.config.multipliers.insert(RATE.into(), 0.5);
    close(
        display_value(run(&native, &data).output(), "VolatilityRestoredPerSecond"),
        3.75,
    );
    native.config.raw_inputs.values.insert(
        "volatilityExplosionQuantity".into(),
        ConfigInputValue::Number(40.0),
    );
    native.config.raw_inputs.values.insert(
        "volatilityExplosionsPerSecond".into(),
        ConfigInputValue::Number(0.5),
    );
    close(
        display_value(run(&native, &data).output(), "VolatilityRestoredPerSecond"),
        3.75,
    );
    let reference = calculate_with_data_session(
        &with_source,
        &data,
        &DataOrchestratorOptions::default().on_reference_basis(),
    )
    .unwrap();
    for id in STATS {
        close(display_value(reference.output(), id), 0.0);
    }
    let no_source = run(&build(xml, false, true), &data);
    for id in STATS {
        close(display_value(no_source.output(), id), 0.0);
    }
    // Even an explicit rate never creates an event quantity.
    close(
        display_value(
            run(
                &build(
                    r#"<Input name="volatilityExplosionsPerSecond" number="2"/>"#,
                    true,
                    false,
                ),
                &data,
            )
            .output(),
            "VolatilityRestoredPerSecond",
        ),
        0.0,
    );
}

#[test]
fn volatility_false_zero_invalid_raw_values_retire_imported_snapshots() {
    let data = data();
    for key in [
        "volatilityExplosionQuantity",
        "volatilityExplosionsPerSecond",
    ] {
        for value in [
            ConfigInputValue::Bool(false),
            ConfigInputValue::Bool(true),
            ConfigInputValue::Number(0.0),
            ConfigInputValue::Number(-1.0),
            ConfigInputValue::Number(f64::NAN),
            ConfigInputValue::Text("off".into()),
            ConfigInputValue::Text("invalid".into()),
        ] {
            let mut build = build("", true, false);
            build.config.multipliers.insert(QUANTITY.into(), 20.0);
            build.config.multipliers.insert(RATE.into(), 2.0);
            build
                .config
                .raw_inputs
                .values
                .insert(key.into(), value.clone());
            let session = run(&build, &data);
            close(
                display_value(session.output(), "VolatilityRestoredPerSecond"),
                0.0,
            );
            close(
                display_value(session.output(), "VolatilityRestoredPerExplosion"),
                if key == "volatilityExplosionQuantity" {
                    0.0
                } else {
                    5.0
                },
            );
        }
    }
    let mut off = build(
        r#"<Input name="volatilityExplosionQuantity" boolean="false"/>"#,
        true,
        false,
    );
    off.config.multipliers.insert(QUANTITY.into(), 20.0);
    off.config.multipliers.insert(RATE.into(), 2.0);
    close(
        display_value(run(&off, &data).output(), "VolatilityRestoredPerSecond"),
        0.0,
    );
}

#[test]
fn volatility_details_rebuild_values_and_retain_source_and_config_wiring() {
    let data = data();
    let session = run(
        &build(
            r#"<Input name="volatilityExplosionQuantity" number="40"/><Input name="volatilityExplosionsPerSecond" number="0.5"/>"#,
            true,
            true,
        ),
        &data,
    );
    let labels_en = include_str!("../../../pobr-i18n/locales/en-US/stats.toml");
    let labels_tw = include_str!("../../../pobr-i18n/locales/zh-TW/stats.toml");
    let labels_web = include_str!("../../../../web/src/lib/detailLabels.ts");
    for id in STATS {
        let entry = pobr_core::display_catalog()
            .into_iter()
            .find(|s| s.id.as_str() == *id)
            .unwrap();
        assert!(entry.pob_key.is_none());
        assert!(labels_en.contains(&format!("{id} =")) && labels_tw.contains(&format!("{id} =")));
        let detail = pobr_build::breakdown::detail::stat_detail(&session, id).unwrap();
        if *id == "VolatilityRefreshChance" {
            assert_eq!(
                detail.kind,
                pobr_build::breakdown::detail::DetailKind::Sources
            );
            assert!(
                detail
                    .sources
                    .iter()
                    .any(|s| s.name == "VolatilityRefreshChance"
                        && s.base_total == 25.0
                        && !s.mods.is_empty())
            );
            continue;
        }
        assert_eq!(
            detail.kind,
            pobr_build::breakdown::detail::DetailKind::Derived
        );
        let d = detail.derived.unwrap();
        let input = |name: &str| d.inputs.iter().find(|i| i.id == name).unwrap().value;
        let rebuilt = match *id {
            "VolatilityRefreshChance" => input("VolatilityRefreshChanceBase").clamp(0.0, 100.0),
            "VolatilityExplosionQuantity" => {
                input("VolatilityRequestedQuantity").min(input("VolatilityMaximum"))
            }
            "VolatilityRestoredPerExplosion" => {
                input("VolatilityExplosionQuantity") * input("VolatilityRefreshChance") / 100.0
            }
            "VolatilityRestoredPerSecond" => {
                input("VolatilityRestoredPerExplosion") * input("VolatilityExplosionsPerSecond")
            }
            _ => unreachable!(),
        };
        close(rebuilt, display_value(session.output(), id));
        for i in d.inputs {
            assert!(labels_web.contains(&format!("{}:", i.id)), "{}", i.id);
        }
    }
    let catalog = GameData::new(repo_data_root().join(data_version()))
        .config_options()
        .unwrap();
    for (var, mult) in [
        ("volatilityExplosionQuantity", QUANTITY),
        ("volatilityExplosionsPerSecond", RATE),
    ] {
        let option = catalog.options.iter().find(|o| o.var == var).unwrap();
        assert!(option.default.is_none());
        assert!(
            option
                .effects
                .iter()
                .any(|effect| effect.name == format!("Multiplier:{mult}"))
        );
    }
}
