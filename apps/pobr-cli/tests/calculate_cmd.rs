//! Integration tests: calling `pobr_cli` library functions directly (pure functions, no IO).

use pobr_cli::{
    BuildDetail, CalculateBuildRequest, CalculateRequest, ParseItemRequest, calculate,
    calculate_build, calculate_build_detailed, decode_code, encode_code, parse_item, parse_mod,
    render_breakdowns,
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
/// A Comet in a Cast on Critical, filled by Spark.
const GEMLING_COMET_CODE: &str = include_str!(
    "../../../examples/wasm-benchmark/builds/ninja-2026-10-11-mercenary-gemling-legionnaire-comet-c60ba778fabb/code.txt"
);

/// A Rain of Arrows with 169% increased Projectile Speed.
const RAIN_OF_ARROWS_CODE: &str = include_str!(
    "../../../examples/reference-builds/tuned/builds/ninja-2026-10-13-huntress-amazon-rain-of-arrows-0826116e6c98/code.txt"
);

/// A Spirit Walker whose main skill is a tamed beast's (Zekoa).
const TAMED_BEAST_CODE: &str = include_str!(
    "../../../examples/reference-builds/tuned/builds/ninja-2026-10-13-huntress-spirit-walker-companion-zekoa-the-headcrusher-14d04f6ecc92/code.txt"
);

fn repo_data_dir() -> std::path::PathBuf {
    pobr_gamedata::repo_data_root().join(pobr_gamedata::data_version())
}

#[test]
fn full_build_and_marginal_reports_preserve_rejected_modifiers() {
    let unknown = "Synthetic unsupported architecture audit modifier";
    let xml = decode_code(DEADEYE_CODE).unwrap();
    assert!(xml.contains("</Item>"));
    let code = encode_code(&xml.replacen("</Item>", &format!("\n{unknown}\n</Item>"), 1)).unwrap();
    let report = calculate_build(&CalculateBuildRequest {
        code: code.clone(),
        data_dir: repo_data_dir(),
        enemy_level: 80,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: true,
    })
    .unwrap();
    assert!(
        report
            .unsupported_modifiers
            .iter()
            .any(|text| text == unknown)
    );
    assert_eq!(report.data_version, pobr_gamedata::data_version());
    let data = pobr_gamedata::GameData::new(repo_data_dir());
    assert_eq!(report.provenance.engine_version, env!("CARGO_PKG_VERSION"));
    assert_eq!(
        report.provenance.data_version.as_deref(),
        Some(report.data_version.as_str())
    );
    assert_eq!(
        report.provenance.data_sha256,
        data.identity().unwrap().sha256
    );
    let extra = "Another unsupported candidate modifier";
    let marginal = pobr_cli::marginal_contribution(&pobr_cli::MarginalRequest {
        build_code: code,
        data_dir: repo_data_dir(),
        enemy_level: 80,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: true,
        mod_texts: vec![extra.into()],
    })
    .unwrap();
    assert!(
        marginal
            .baseline_unsupported_modifiers
            .iter()
            .any(|text| text == unknown)
    );
    assert!(
        marginal
            .unsupported_modifiers
            .iter()
            .any(|text| text == extra)
    );
    assert!(pobr_cli::render_marginal(&marginal).contains(extra));
    assert_eq!(marginal.data_version, report.data_version);
    assert_eq!(marginal.provenance, report.provenance);
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

/// A build whose gloves cannot be read is still calculated, and says so.
#[test]
fn calculate_build_reports_an_item_it_could_not_read() {
    let request = |code: String| CalculateBuildRequest {
        code,
        data_dir: repo_data_dir(),
        enemy_level: 0,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: true,
    };
    let intact = calculate_build(&request(DEADEYE_CODE.to_string())).unwrap();
    assert!(intact.missing_items.is_empty());

    // Item 9 is in the Gloves slot of this code.
    let xml = decode_code(DEADEYE_CODE).unwrap();
    let gloves = xml.find("<Item id=\"9\">").expect("item 9");
    let broken = format!(
        "{}{}",
        &xml[..gloves],
        xml[gloves..].replacen("Rarity: ", "Rarity: UNREADABLE ", 1)
    );
    let report = calculate_build(&request(encode_code(&broken).unwrap())).unwrap();

    assert_eq!(report.missing_items.len(), 1);
    let missing = &report.missing_items[0];
    assert_eq!(missing.item_id, 9);
    assert_eq!(missing.used_in, ["gloves"]);
    assert!(missing.reason.contains("rarity"), "{}", missing.reason);
    assert!(!missing.name.is_empty());
    assert_eq!(
        report.build.equipped_item_count + 1,
        intact.build.equipped_item_count
    );
}

/// The code's own exported stats are compared only under the conditions they
/// were exported with.
#[test]
fn calculate_build_compares_with_the_reference_the_code_carries() {
    let exported = CalculateBuildRequest {
        code: DEADEYE_CODE.to_string(),
        data_dir: repo_data_dir(),
        enemy_level: 0,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: true,
    };
    let reference = calculate_build(&exported)
        .unwrap()
        .reference
        .expect("the code carries PlayerStat values");
    assert!(reference.compared > 20, "{}", reference.compared);
    assert_eq!(
        reference.agreeing + reference.differing.len(),
        reference.compared
    );
    assert!(
        reference
            .differing
            .iter()
            .all(|row| row.pobr != row.reference)
    );

    for other in [
        CalculateBuildRequest {
            mode_effective: false,
            ..exported.clone()
        },
        CalculateBuildRequest {
            enemy_level: 80,
            ..exported.clone()
        },
        CalculateBuildRequest {
            enemy_tier: EnemyTier::Boss,
            ..exported.clone()
        },
    ] {
        assert!(calculate_build(&other).unwrap().reference.is_none());
    }
}

/// A spell in a Cast on Critical is reported at the rate the gem triggers it,
/// and compared with the code's reference at its own rate of use: the
/// exporter has no Energy model.
#[test]
fn calculate_build_reports_a_triggered_spell_at_the_rate_of_triggers() {
    let report = calculate_build(&CalculateBuildRequest {
        code: GEMLING_COMET_CODE.trim().to_string(),
        data_dir: repo_data_dir(),
        enemy_level: 0,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: true,
    })
    .unwrap();
    let trigger = report
        .output
        .energy_trigger
        .as_ref()
        .expect("the main skill is in a Cast on Critical");
    // Comet alone: 100 for its second of base cast time, 200 for the second
    // it adds to its total cast time.
    assert_eq!(trigger.maximum_energy, 300.0);
    assert_eq!(trigger.source_skill.as_deref(), Some("SparkPlayer"));
    assert!(trigger.energy_per_critical_hit > 0.0);
    assert!(trigger.triggers_per_second > 0.0);
    assert_eq!(report.output.action_rate, trigger.triggers_per_second);

    let reference = report
        .reference
        .expect("the code carries PlayerStat values");
    assert!(reference.at_own_rate_of_use);
    // The exporter's Speed is the Comet's own cast rate, and so is the value
    // compared with it.
    assert!(
        reference
            .differing
            .iter()
            .all(|row| row.stat != "ActionRate"),
        "{:?}",
        reference.differing
    );

    // A skill used by hand is compared as it is reported.
    let by_hand = calculate_build(&CalculateBuildRequest {
        code: DEADEYE_CODE.to_string(),
        data_dir: repo_data_dir(),
        enemy_level: 0,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: true,
    })
    .unwrap();
    assert!(by_hand.output.energy_trigger.is_none());
    assert!(!by_hand.reference.unwrap().at_own_rate_of_use);
}

/// A rain is reported at the hits its landing arrows give, and compared with
/// the code's reference by the exporter's count: a hit whenever the enemy can
/// be hit again.
#[test]
fn calculate_build_reports_a_rain_by_its_landing_arrows() {
    let report = calculate_build(&CalculateBuildRequest {
        code: RAIN_OF_ARROWS_CODE.trim().to_string(),
        data_dir: repo_data_dir(),
        enemy_level: 0,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: true,
    })
    .unwrap();
    let rain = report
        .output
        .rain
        .as_ref()
        .expect("the main skill is a rain");
    // 169% increased and Projectile Acceleration III's 40% more: an arrow
    // every 0.08 / (2.69 × 1.4) s, and the sixth after a hit is the first to
    // land once 0.12 s have passed.
    assert!((rain.landing_interval - 0.08 / (2.69 * 1.4)).abs() < 1e-9);
    assert_eq!(rain.hit_interval, 0.12);
    assert_eq!(rain.arrows_per_hit, 6.0);
    assert_eq!(report.output.action_rate, rain.hits_per_second);

    let reference = report
        .reference
        .expect("the code carries PlayerStat values");
    assert!(reference.rain_hits_whenever_allowed);
    assert!(!reference.at_own_rate_of_use);
    // The exporter's hit rate, 1 / 0.12 s, is the one compared with it.
    assert!(
        reference
            .differing
            .iter()
            .all(|row| row.stat != "ActionRate"),
        "{:?}",
        reference.differing
    );
}

/// `--breakdown` lists the stats asked for in that order, each with the
/// steps and lines of the calculation behind the output; the default report
/// carries none.
#[test]
fn calculate_build_breaks_down_the_stats_asked_for() {
    let request = CalculateBuildRequest {
        code: DEADEYE_CODE.to_string(),
        data_dir: repo_data_dir(),
        enemy_level: 0,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: true,
    };
    let detail = BuildDetail {
        breakdown: vec![
            "TotalDPS".into(),
            "Life".into(),
            "NothingIsNamedThis".into(),
        ],
        ..BuildDetail::default()
    };
    let report = calculate_build_detailed(&request, &detail).unwrap();

    let stats: Vec<&str> = report
        .breakdowns
        .iter()
        .map(|entry| entry.stat.as_str())
        .collect();
    assert_eq!(stats, ["TotalDPS", "Life", "NothingIsNamedThis"]);

    let life = &report.breakdowns[1];
    assert_eq!(life.value, Some(report.output.life));
    assert_eq!(life.pob_key.as_deref(), Some("Life"));
    // The code carries the exporter's Life and the report compares with it.
    assert!(life.reference.is_some());
    let breakdown = life.breakdown.as_ref().expect("Life has lines");
    let formula = breakdown.formula.as_ref().expect("Life has steps");
    assert!(
        (formula.value - report.output.life).abs() <= 1e-6 * report.output.life.max(1.0),
        "steps end at {} for Life {}",
        formula.value,
        report.output.life
    );
    // The lines that apply are the totals the steps use.
    let active_base: f64 = breakdown
        .mods
        .iter()
        .filter(|line| line.active && line.mod_type == "BASE")
        .filter_map(|line| line.value)
        .sum();
    assert!((active_base - breakdown.base_total).abs() < 1e-6);

    let dps = &report.breakdowns[0];
    assert_eq!(dps.value, Some(report.output.dps));
    assert_eq!(dps.pob_key.as_deref(), Some("TotalDPS"));

    let unknown = &report.breakdowns[2];
    assert!(unknown.value.is_none());
    assert!(unknown.pob_key.is_none());
    assert!(unknown.breakdown.is_none());

    let main = report
        .main_skill
        .as_ref()
        .expect("the build has a main skill");
    assert!(!main.skill_id.is_empty());
    assert_eq!(main.hit_dps, report.output.dps);

    let text = render_breakdowns(&report);
    assert!(
        text.contains(&format!("main skill: {}", main.skill_id)),
        "{text}"
    );
    for stat in &stats {
        assert!(text.contains(&format!("== {stat}")), "{text}");
    }
    assert!(
        text.contains("no modifier lines or steps for this stat"),
        "{text}"
    );
    if breakdown.mods.iter().any(|line| !line.active) {
        assert!(text.contains("inactive"), "{text}");
    }

    // The default report is as before: no breakdowns, no main skill.
    let plain = calculate_build(&request).unwrap();
    assert!(plain.breakdowns.is_empty());
    assert!(plain.main_skill.is_none());
    assert_eq!(plain.output.life, report.output.life);
    let json = serde_json::to_value(&plain).unwrap();
    for key in ["breakdowns", "main_skill", "reference_basis"] {
        assert!(json.get(key).is_none(), "{key}");
    }
}

/// On the reference's basis the whole report leaves out the models the
/// exporter lacks: a Comet in a Cast on Critical is at its own rate of use,
/// in the output and in its breakdown.
#[test]
fn calculate_build_on_the_reference_basis_answers_the_exporters_question() {
    let request = CalculateBuildRequest {
        code: GEMLING_COMET_CODE.trim().to_string(),
        data_dir: repo_data_dir(),
        enemy_level: 0,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: true,
    };
    let triggered = calculate_build(&request).unwrap();
    let report = calculate_build_detailed(
        &request,
        &BuildDetail {
            breakdown: vec!["ActionRate".into()],
            reference_basis: true,
            ..BuildDetail::default()
        },
    )
    .unwrap();
    assert!(report.reference_basis);
    assert!(report.output.energy_trigger.is_none());
    assert_ne!(report.output.action_rate, triggered.output.action_rate);
    let reference = report
        .reference
        .as_ref()
        .expect("the code carries PlayerStat values");
    assert!(!reference.at_own_rate_of_use);
    assert!(
        reference
            .differing
            .iter()
            .all(|row| row.stat != "ActionRate"),
        "{:?}",
        reference.differing
    );
    assert_eq!(report.breakdowns[0].value, Some(report.output.action_rate));
}

/// `--usage` adds what the calculation did with each line of the build and
/// changes nothing else. On the reference's basis the ledger is still of the
/// calculation with every model on, whose readers of lines that basis lacks.
#[test]
fn calculate_build_reports_what_became_of_each_line() {
    let request = CalculateBuildRequest {
        code: GEMLING_COMET_CODE.trim().to_string(),
        data_dir: repo_data_dir(),
        enemy_level: 0,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: true,
    };
    let plain = calculate_build(&request).unwrap();
    assert!(plain.usage.is_none());
    let usage = BuildDetail {
        usage: true,
        ..BuildDetail::default()
    };
    let report = calculate_build_detailed(&request, &usage).unwrap();
    assert_eq!(json(&report.output), json(&plain.output));
    let ledger = report.usage.as_ref().expect("asked for");
    assert!(
        ledger.lines.iter().any(|line| {
            line.status == pobr_build::usage::LineStatus::Applied
                && line.used_in.contains(&"TotalHitAvg")
        }),
        "no line reaches the average hit"
    );
    let text = render_breakdowns(&report);
    assert!(text.contains("== line usage"), "{text}");

    let on_basis = calculate_build_detailed(
        &request,
        &BuildDetail {
            usage: true,
            reference_basis: true,
            ..BuildDetail::default()
        },
    )
    .unwrap();
    assert_ne!(json(&on_basis.output), json(&report.output));
    assert_eq!(json(&on_basis.usage), json(&report.usage));
}

/// A value as the JSON report carries it, for comparing.
fn json<T: serde::Serialize>(value: &T) -> serde_json::Value {
    serde_json::to_value(value).unwrap()
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

/// A build whose main skill summons a minion: its minions are in the output,
/// the code's `<MinionStat>` values are compared with the main skill's
/// minion, and `Minion:<stat>` breaks down that minion's stat.
#[test]
fn calculate_build_reports_the_minion_of_the_main_skill() {
    let request = CalculateBuildRequest {
        code: TAMED_BEAST_CODE.trim().to_string(),
        data_dir: repo_data_dir(),
        enemy_level: 0,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: true,
    };
    let report = calculate_build_detailed(
        &request,
        &BuildDetail {
            breakdown: vec!["Minion:Life".into(), "Minion:FireResist".into()],
            ..BuildDetail::default()
        },
    )
    .unwrap();
    let beast = "Metadata/Monsters/Quadrilla/QuadrillaBossMinion2";
    let minion = report
        .output
        .minions
        .iter()
        .find(|minion| minion.minion_id == beast)
        .expect("the main skill's beast");
    assert_eq!(minion.summoned_by, "SummonBeastPlayer");
    for id in pobr_core::calc::minion::MINION_RESISTANCE_STATS {
        assert!(
            minion.stats.contains_key(id),
            "missing CLI minion stat {id}"
        );
    }
    let resistance = &report.breakdowns[1];
    assert_eq!(resistance.value, Some(minion.stats["FireResist"]));
    assert_eq!(
        resistance
            .breakdown
            .as_ref()
            .unwrap()
            .formula
            .as_ref()
            .unwrap()
            .value,
        minion.stats["FireResist"]
    );

    let entry = &report.breakdowns[0];
    assert_eq!(entry.stat, "Minion:Life");
    assert_eq!(entry.pob_key.as_deref(), Some("Minion:Life"));
    assert_eq!(entry.value, Some(minion.life));
    assert!(
        entry.reference.is_some(),
        "the code carries the minion's Life"
    );
    let breakdown = entry.breakdown.as_ref().expect("the minion's Life lines");
    assert!(breakdown.formula.is_some());
    assert!(
        breakdown
            .mods
            .iter()
            .any(|line| line.origin_id.as_deref() == Some("minion.base.life")),
        "{breakdown:#?}"
    );
    let reference = report.reference.as_ref().expect("compared");
    assert!(
        reference
            .differing
            .iter()
            .all(|row| row.stat != "MinionLife"),
        "the beast's Life agrees: {reference:#?}"
    );
}

#[test]
fn periodic_spells_compare_on_continuous_exposure_despite_short_duration() {
    let xml = r#"<PathOfBuilding2>
<Build level="90" className="Sorceress" ascendClassName="None" mainSocketGroup="1">
  <PlayerStat stat="Speed" value="1.25"/>
  <PlayerStat stat="HitSpeed" value="0.7142857142857143"/>
</Build>
<Skills activeSkillSet="1"><SkillSet id="1"><Skill enabled="true" mainActiveSkill="1">
  <Gem gemId="Metadata/Items/Gem/SkillGemSolarOrb" skillId="SolarOrbPlayer" level="20" quality="0" enabled="true"/>
</Skill></SkillSet></Skills>
<Config><Input name="customMods" string="99% reduced Skill Effect Duration"/></Config>
</PathOfBuilding2>"#;
    let request = CalculateBuildRequest {
        code: encode_code(xml).unwrap(),
        data_dir: repo_data_dir(),
        enemy_level: 0,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: true,
    };
    let game = calculate_build(&request).unwrap();
    assert!((game.output.action_rate - 0.125 / 1.4).abs() < 1e-6);
    let reference = game.reference.as_ref().unwrap();
    assert!(reference.periodic_hits_assume_sustained);
    assert!(reference.compared > 0);
    assert!(
        reference
            .differing
            .iter()
            .all(|row| row.stat != "ActionRate")
    );
    let on_basis = calculate_build_detailed(
        &request,
        &BuildDetail {
            reference_basis: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert!((on_basis.output.action_rate - 1.0 / 1.4).abs() < 1e-6);
    assert!(render_breakdowns(&game).contains("exporter's basis"));
}

#[test]
fn full_dps_is_opt_in_and_the_json_contributions_sum_to_it() {
    let xml = r#"<PathOfBuilding2><Build level="90" className="Witch"/>
        <Skills><Skill includeInFullDPS="true"><Gem skillId="SparkPlayer" level="20" count="3"/></Skill></Skills>
        </PathOfBuilding2>"#;
    let request = CalculateBuildRequest {
        code: pobr_build::encode_pob_code(xml).unwrap(),
        data_dir: repo_data_dir(),
        enemy_level: 0,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: true,
    };
    assert!(
        calculate_build_detailed(&request, &BuildDetail::default())
            .unwrap()
            .output
            .full_dps
            .is_none()
    );
    let report = calculate_build_detailed(
        &request,
        &BuildDetail {
            breakdown: vec!["FullDPS".into(), "FullDotDPS".into()],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(report.output.full_dps_skills.len(), 1);
    assert_eq!(report.output.full_dps_skills[0].count, 3.0);
    assert_eq!(
        report.output.full_dps,
        Some(
            report
                .output
                .full_dps_skills
                .iter()
                .map(|row| row.dps)
                .sum()
        )
    );
    assert_eq!(report.breakdowns[0].value, report.output.full_dps);
}

#[test]
fn calculate_build_exports_crossbow_firing_and_reload_rates() {
    let xml = r#"<PathOfBuilding2><Build className="Witch" level="90" mainSocketGroup="1"/>
<Skills><Skill mainActiveSkill="1"><Gem skillId="FragmentationRoundsAmmoPlayer" level="20"/></Skill></Skills>
<Items activeItemSet="1"><Item id="1">Rarity: NORMAL
Makeshift Crossbow
</Item><ItemSet id="1"><Slot name="Weapon 1" itemId="1"/></ItemSet></Items></PathOfBuilding2>"#;
    let report = calculate_build(&CalculateBuildRequest {
        code: encode_code(xml).unwrap(),
        data_dir: repo_data_dir(),
        enemy_level: 0,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: true,
    })
    .unwrap();
    assert!((report.output.firing_rate - 1.6).abs() < 1e-9);
    assert!((report.output.reload_time - 0.8).abs() < 1e-9);
    let json = serde_json::to_value(report).unwrap();
    assert!(json["output"]["firing_rate"].as_f64().unwrap() > 0.0);
    assert!(json["output"]["reload_time"].as_f64().unwrap() > 0.0);
}

#[test]
fn exporter_absent_rules_are_reported_without_changing_reference_agreement() {
    let request = CalculateBuildRequest {
        code: GEMLING_COMET_CODE.trim().to_string(),
        data_dir: repo_data_dir(),
        enemy_level: 0,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: true,
    };
    let report = calculate_build_detailed(
        &request,
        &BuildDetail {
            usage: true,
            reference_basis: true,
            ..BuildDetail::default()
        },
    )
    .unwrap();
    let reference = report.reference.unwrap();
    assert!(reference.exporter_absent_rules_omitted);
    assert!(reference.differing.iter().all(|row| row.stat != "ManaCost"));
    assert!(
        report
            .beyond_pob2
            .iter()
            .any(|rule| rule.rule_id == "spell_mana_cost_efficiency" && rule.changed_rows > 0)
    );
    let usage = serde_json::to_string(&report.usage.unwrap()).unwrap();
    assert!(
        usage.contains("ManaCostEfficiency"),
        "game-mode usage keeps the absent rule"
    );
}

// --lang: mod and item lines in one of the data pack's locales.

fn zh_cn() -> pobr_cli::Locale {
    pobr_cli::Locale::load(&repo_data_dir(), "zh-CN").expect("the data pack declares zh-CN")
}

fn has_han(text: &str) -> bool {
    text.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
}

#[test]
fn lang_reads_a_localized_mod_line_as_its_english_line() {
    let locale = zh_cn();
    let english = pobr_cli::parse_mod_json("+50 to maximum Life", &repo_data_dir(), None).unwrap();
    let localized =
        pobr_cli::parse_mod_json("+50 生命上限", &repo_data_dir(), Some(&locale)).unwrap();
    assert_eq!(localized, english);
    assert!(english.contains("\"Parsed\""), "{english}");
    // An English line given with --lang is read as it is.
    let mixed =
        pobr_cli::parse_mod_json("+50 to maximum Life", &repo_data_dir(), Some(&locale)).unwrap();
    assert_eq!(mixed, english);

    let request = |lines: &[&str]| CalculateRequest {
        input: base_input(),
        modifier_texts: lines.iter().map(|line| line.to_string()).collect(),
    };
    let english = pobr_cli::calculate_json(
        &request(&["+50 to maximum Life", "10% increased maximum Life"]),
        None,
    )
    .unwrap();
    let localized = pobr_cli::calculate_json(
        &request(&["+50 生命上限", "生命上限提高 10%"]),
        Some(&locale),
    )
    .unwrap();
    assert_eq!(localized, english);
    let value: serde_json::Value = serde_json::from_str(&localized).unwrap();
    assert_eq!(value["output"]["life"], 1155.0);
}

#[test]
fn lang_reads_localized_item_text_and_shows_the_base_in_the_locale() {
    let locale = zh_cn();
    let english =
        "Rarity: Rare\nGloom Coil\nSapphire Ring\n--------\n+40 to maximum Life\nmirrored";
    let localized = "Rarity: Rare\nGloom Coil\n蓝玉戒指\n--------\n+40 生命上限\nmirrored";
    let as_json = |text: &str, locale| -> serde_json::Value {
        let json = pobr_cli::parse_item_json(
            &ParseItemRequest {
                text: text.to_string(),
            },
            locale,
        )
        .unwrap();
        serde_json::from_str(&json).unwrap()
    };
    let english = as_json(english, None);
    let localized = as_json(localized, Some(&locale));
    assert_eq!(english["base"], "Sapphire Ring");
    assert_eq!(localized["base"], "蓝玉戒指");
    assert_eq!(localized["modifiers"], english["modifiers"]);
    assert_eq!(localized["unsupported"], english["unsupported"]);

    // Unsupported item lines come back as typed: an unrecognised localized
    // line, and a recognised one the parser does not support.
    let unsupported = as_json(
        "Rarity: Rare\nGloom Coil\n蓝玉戒指\n--------\n+40 生命上限\n不认识的词条\n1 个稀有怪物被替换为一个随机地图首领",
        Some(&locale),
    );
    assert_eq!(
        unsupported["unsupported"],
        serde_json::json!(["不认识的词条", "1 个稀有怪物被替换为一个随机地图首领"])
    );
    assert_eq!(unsupported["modifiers"], english["modifiers"]);
    assert!(
        english["modifiers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["name"].as_str().unwrap().contains("MaximumLife")),
        "{english}"
    );
}

#[test]
fn lang_shows_a_build_reports_mod_lines_in_the_locale() {
    let request = CalculateBuildRequest {
        code: DEADEYE_CODE.to_string(),
        data_dir: repo_data_dir(),
        enemy_level: 0,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: true,
    };
    let detail = BuildDetail {
        breakdown: vec!["Life".into()],
        usage: true,
        ..BuildDetail::default()
    };
    let english = calculate_build_detailed(&request, &detail).unwrap();
    let mut localized = english.clone();
    zh_cn().localize_build(&mut localized);

    // Numbers are the language's business nowhere.
    assert_eq!(
        serde_json::to_value(&localized.output).unwrap(),
        serde_json::to_value(&english.output).unwrap()
    );
    let lines = |report: &pobr_cli::CalculateBuildReport| -> Vec<(Option<String>, Option<f64>)> {
        report.breakdowns[0]
            .breakdown
            .as_ref()
            .expect("Life has lines")
            .mods
            .iter()
            .map(|line| (line.source_text.clone(), line.value))
            .collect()
    };
    let (before, after) = (lines(&english), lines(&localized));
    assert_eq!(before.len(), after.len());
    assert!(before.iter().zip(&after).all(|(b, a)| b.1 == a.1));
    let flat_life = before
        .iter()
        .zip(&after)
        .find(|(b, _)| {
            b.0.as_deref()
                .is_some_and(|text| text.ends_with(" to maximum Life") && text.starts_with('+'))
        })
        .expect("the build has a flat Life line");
    let shown = flat_life.1.0.as_deref().unwrap();
    assert!(shown.ends_with(" 生命上限"), "{shown}");
    assert!(render_breakdowns(&localized).contains(shown));

    let usage = localized.usage.as_ref().expect("usage was asked for");
    assert!(
        usage
            .lines
            .iter()
            .filter_map(|line| line.text.as_deref())
            .any(has_han),
        "usage lines are shown in the locale"
    );
}

#[test]
fn lang_marks_the_translation_repositorys_own_text() {
    let request = CalculateBuildRequest {
        code: DEADEYE_CODE.to_string(),
        data_dir: repo_data_dir(),
        enemy_level: 0,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: true,
    };
    let detail = BuildDetail {
        breakdown: vec!["Life".into()],
        ..BuildDetail::default()
    };
    let english = calculate_build_detailed(&request, &detail).unwrap();

    // Today's pack is official text only: nothing is marked, and the report
    // and its text form are what they were.
    let mut official = english.clone();
    zh_cn().localize_build(&mut official);
    assert!(official.origins.is_empty());
    let json = serde_json::to_value(&official).unwrap();
    assert!(json.get("origins").is_none(), "{json}");
    assert_eq!(
        pobr_cli::render_build_text(official.clone()),
        render_breakdowns(&official)
    );

    // A pack whose flat Life row is a supplement, and one official row.
    let stat_lines = serde_json::json!([
        {"src": "{0:+d} 生命上限", "en": "{0:+d} to maximum Life", "origin": "agent"},
        {"src": "生命上限提高 {0}%", "en": "{0}% increased maximum Life"}
    ]);
    let pack = pobr_i18n::LocalePack::from_files(
        "zh-CN",
        [("stat_lines.json", stat_lines.to_string().into_bytes())],
    )
    .unwrap();
    let locale = pobr_cli::Locale::from_packs("zh-CN", vec![pack], vec![]);
    let mut localized = english.clone();
    locale.localize_build(&mut localized);
    let shown: Vec<String> = localized.breakdowns[0]
        .breakdown
        .as_ref()
        .unwrap()
        .mods
        .iter()
        .filter_map(|line| line.source_text.clone())
        .filter(|text| text.ends_with(" 生命上限") && text.starts_with('+'))
        .collect();
    assert!(!shown.is_empty(), "the build has a flat Life line");
    for text in &shown {
        assert_eq!(
            localized.origins.get(text),
            Some(&pobr_i18n::TextOrigin::Agent)
        );
    }
    assert!(
        localized
            .origins
            .keys()
            .all(|text| text.ends_with(" 生命上限")),
        "only the supplement's lines: {:?}",
        localized.origins
    );
    let json = serde_json::to_value(&localized).unwrap();
    assert_eq!(json["origins"][&shown[0]], "agent");
    let text = pobr_cli::render_build_text(localized.clone());
    assert!(
        text.contains(&format!("{}{}", shown[0], pobr_cli::SUPPLEMENT_MARKER)),
        "{text}"
    );
    assert!(text.ends_with(&format!("{}\n", pobr_cli::SUPPLEMENT_FOOTNOTE)));
    // Official lines are not marked; JSON lines are never marked.
    assert_eq!(
        text.matches(pobr_cli::SUPPLEMENT_MARKER).count(),
        shown.len() + 1
    );
    assert!(
        !serde_json::to_string(&json)
            .unwrap()
            .contains(pobr_cli::SUPPLEMENT_MARKER)
    );
}

#[test]
fn lang_reads_a_description_written_on_two_lines_together() {
    let locale = zh_cn();
    // Mind Over Matter: the pack keys the two lines as one description, and
    // its second line alone would read as a different line.
    let english = [
        "All Damage is taken from Mana before Life",
        "50% less Mana Recovery Rate",
        "+50 to maximum Life",
    ];
    let typed = [
        "受到的所有伤害在扣除生命前优先从魔力扣除",
        "魔力回复速度总降 50%",
        "+50 生命上限",
    ];
    let lines = |lines: &[&str]| {
        lines
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(locale.parse_lines(&lines(&typed)).lines, english);
    let request = |texts: &[&str]| CalculateRequest {
        input: base_input(),
        modifier_texts: lines(texts),
    };
    assert_eq!(
        pobr_cli::calculate_json(&request(&typed), Some(&locale)).unwrap(),
        pobr_cli::calculate_json(&request(&english), None).unwrap()
    );
}

#[test]
fn lang_never_joins_lines_whose_repeated_values_differ() {
    let locale = zh_cn();
    // The pack has "附加 {0} - {1} 冰霜伤害\n玩家及其召唤生物附加 {0} - {1} 冰霜伤害"
    // (one value pair on both lines); these lines carry two different pairs.
    let typed = [
        "附加 5 - 10 冰霜伤害",
        "玩家及其召唤生物附加 3 - 4 冰霜伤害",
    ];
    let request = |texts: &[&str]| CalculateRequest {
        input: pobr_core::calc::MinimalInput {
            base_hit_min: 1.0,
            base_hit_max: 1.0,
            base_action_rate: 1.0,
            ..base_input()
        },
        modifier_texts: texts.iter().map(|line| line.to_string()).collect(),
    };
    let hit = |json: String| {
        serde_json::from_str::<serde_json::Value>(&json).unwrap()["output"]["total_hit_avg"]
            .as_f64()
            .unwrap()
    };
    let localized = hit(pobr_cli::calculate_json(&request(&typed), Some(&locale)).unwrap());
    let first_alone = hit(pobr_cli::calculate_json(&request(&typed[..1]), Some(&locale)).unwrap());
    assert_eq!(localized, first_alone);
    assert_eq!(localized, 8.5);
    // A blank line holds no modifier and is not reported.
    let json = pobr_cli::calculate_json(&request(&["", "Adds 5 to 10 Cold Damage"]), None).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["unsupported"], serde_json::json!([]));
}

#[test]
fn lang_reports_unsupported_lines_as_typed() {
    let locale = zh_cn();
    // Recognised by the zh-CN templates (so it reaches the parser in English),
    // but not supported by the parser.
    let recognised = "1 个稀有怪物被替换为一个随机地图首领";
    assert_eq!(
        locale.parse_line(recognised),
        "1 Rare Monster is replaced by a random Map Boss"
    );
    let lines = ["+50 生命上限", "不认识的词条", recognised, "mirrored"];
    let json = pobr_cli::calculate_json(
        &CalculateRequest {
            input: base_input(),
            modifier_texts: lines.iter().map(|line| line.to_string()).collect(),
        },
        Some(&locale),
    )
    .unwrap();
    let result: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(result["output"]["life"], 1050.0);
    assert_eq!(
        result["unsupported"],
        serde_json::json!(["不认识的词条", recognised, "mirrored"])
    );
}

#[test]
fn lang_reports_a_broken_data_pack_as_a_data_error() {
    let root = std::env::temp_dir().join(format!("pobr-cli-lang-{}", std::process::id()));
    let corrupt = root.join("corrupt");
    std::fs::create_dir_all(&corrupt).unwrap();
    std::fs::write(corrupt.join("manifest.json"), "{ not json").unwrap();
    for dir in [root.join("missing"), corrupt] {
        let error = pobr_cli::Locale::load(&dir, "zh-CN").unwrap_err();
        assert!(
            matches!(error, pobr_cli::CliError::GameData(_)),
            "{dir:?}: {error}"
        );
        assert!(!error.to_string().contains("unknown language"), "{error}");
    }
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn lang_unknown_language_lists_the_available_ones() {
    let error = pobr_cli::Locale::load(&repo_data_dir(), "xx-XX").unwrap_err();
    let message = error.to_string();
    assert!(message.contains("\"xx-XX\""), "{message}");
    for code in pobr_cli::available_languages(&repo_data_dir()) {
        assert!(message.contains(&code), "{message}");
    }
    assert!(
        message.contains("en-US") && message.contains("zh-CN"),
        "{message}"
    );

    // English, the parser's language, translates nothing.
    let english = pobr_cli::Locale::load(&repo_data_dir(), pobr_cli::SOURCE_LANGUAGE).unwrap();
    assert!(!english.has_templates());
    assert_eq!(english.parse_line("+50 生命上限"), "+50 生命上限");
    assert_eq!(
        english.render_line("+50 to maximum Life"),
        "+50 to maximum Life"
    );
}
