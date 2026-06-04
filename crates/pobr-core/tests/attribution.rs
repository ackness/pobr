//! AttributionReport（来源贡献总账）集成测试。
//!
//! 对照 `devs/docs/architecture/10-pob-parity-and-attribution.md` §6-7：
//! - Direct contribution：从 TraceGraph 直接读某 source 对某输出的直接贡献。
//! - Marginal contribution：`final(build) - final(build_without_source)`。
//! - Interaction：`final - base - Σ(individual marginal deltas)`。
//!
//! 用一个含多来源（两件装备各 +Life、装备 + 天赋）的小场景驱动，
//! 通过按 `SourceId` 过滤的 `ModDb` 重算 + `calculate_minimal` 实现 marginal 重算。

use pobr_core::attribution::{
    AttributionEntry, AttributionMode, AttributionReport, AttributionRequest, attribute,
};
use pobr_core::calc::{MinimalInput, calculate_minimal, calculate_minimal_traced};
use pobr_core::{CalcConfig, ModDb, Modifier};
use pobr_data::prelude::*;

const WEAPON: &str = "item.weapon";
const HELMET: &str = "item.helmet";
const PASSIVE: &str = "node.541";

fn life_source(kind: SourceKind, id: &str) -> ModifierSource {
    ModifierSource::new(SourceId::new(kind, id))
}

/// 构造一个三来源 Life 场景：
/// - weapon: +60 Life (Base)
/// - helmet: +40 Life (Base)
/// - passive node: +30% increased Life (Inc)
fn life_db() -> ModDb {
    let mut db = ModDb::new();
    db.add_mod(
        Modifier::number("MaximumLife", ModType::Base, 60.0)
            .with_origin(life_source(SourceKind::Item, WEAPON)),
    );
    db.add_mod(
        Modifier::number("MaximumLife", ModType::Base, 40.0)
            .with_origin(life_source(SourceKind::Item, HELMET)),
    );
    db.add_mod(
        Modifier::number("MaximumLife", ModType::Inc, 30.0)
            .with_origin(life_source(SourceKind::PassiveNode, PASSIVE)),
    );
    db
}

/// 复制一个剔除 `excluded` 中任一 source 的 ModDb，用于"移除某 source 后重算"。
fn db_without(db: &ModDb, excluded: &[SourceId]) -> ModDb {
    db.filtered(|modifier| {
        modifier
            .origin
            .as_ref()
            .map(|origin| !excluded.contains(&origin.source_id))
            .unwrap_or(true)
    })
}

#[test]
fn marginal_delta_matches_recompute_without_source() {
    let db = life_db();
    let input = MinimalInput {
        base_life: 100.0,
        ..MinimalInput::default()
    };
    let cfg = CalcConfig::new();

    // final: (100 + 60 + 40) * 1.30 = 260
    let final_life = calculate_minimal(&db, &cfg, &input).life;
    assert_eq!(final_life, 260.0);

    // remove weapon: (100 + 40) * 1.30 = 182 → marginal 78
    let weapon = SourceId::new(SourceKind::Item, WEAPON);
    let without_weapon = calculate_minimal(
        &db_without(&db, std::slice::from_ref(&weapon)),
        &cfg,
        &input,
    )
    .life;
    assert_eq!(without_weapon, 182.0);

    let request = AttributionRequest::new(DisplayStatId::from("Life"))
        .with_mode(AttributionMode::Marginal)
        .with_sources([
            weapon.clone(),
            SourceId::new(SourceKind::Item, HELMET),
            SourceId::new(SourceKind::PassiveNode, PASSIVE),
        ]);

    let report = attribute(&request, final_life, None, |excluded| {
        calculate_minimal(&db_without(&db, excluded), &cfg, &input).life
    });

    assert_eq!(report.output, DisplayStatId::from("Life"));
    assert_eq!(report.final_value, 260.0);

    let weapon_entry = report
        .entries
        .iter()
        .find(|entry| entry.source == weapon)
        .expect("weapon entry present");
    assert_eq!(weapon_entry.marginal_delta, Some(78.0));
    // marginal_percent = delta / final
    assert!((weapon_entry.marginal_percent.unwrap() - 78.0 / 260.0).abs() < 1e-9);
}

#[test]
fn marginal_percent_is_delta_over_final_for_every_source() {
    let db = life_db();
    let input = MinimalInput {
        base_life: 100.0,
        ..MinimalInput::default()
    };
    let cfg = CalcConfig::new();
    let final_life = calculate_minimal(&db, &cfg, &input).life;

    let sources = [
        SourceId::new(SourceKind::Item, WEAPON),
        SourceId::new(SourceKind::Item, HELMET),
        SourceId::new(SourceKind::PassiveNode, PASSIVE),
    ];
    let request = AttributionRequest::new(DisplayStatId::from("Life"))
        .with_mode(AttributionMode::MarginalWithInteraction)
        .with_sources(sources.iter().cloned());

    let report = attribute(&request, final_life, None, |excluded| {
        calculate_minimal(&db_without(&db, excluded), &cfg, &input).life
    });

    for entry in &report.entries {
        let delta = entry.marginal_delta.unwrap();
        let percent = entry.marginal_percent.unwrap();
        assert!((percent - delta / final_life).abs() < 1e-9);
    }
}

#[test]
fn interaction_is_zero_for_single_source() {
    let mut db = ModDb::new();
    db.add_mod(
        Modifier::number("MaximumLife", ModType::Base, 50.0)
            .with_origin(life_source(SourceKind::Item, WEAPON)),
    );
    let input = MinimalInput {
        base_life: 100.0,
        ..MinimalInput::default()
    };
    let cfg = CalcConfig::new();
    let final_life = calculate_minimal(&db, &cfg, &input).life; // 150

    let weapon = SourceId::new(SourceKind::Item, WEAPON);
    let request = AttributionRequest::new(DisplayStatId::from("Life"))
        .with_mode(AttributionMode::MarginalWithInteraction)
        .with_sources([weapon.clone()]);

    let report = attribute(&request, final_life, None, |excluded| {
        calculate_minimal(&db_without(&db, excluded), &cfg, &input).life
    });

    // 单来源：interaction = final - base - Σdelta
    //          base(remove weapon) = 100; delta = 50; final = 150 → interaction 0
    let interaction = report.interaction.as_ref().expect("interaction bucket");
    assert!(interaction.marginal_delta.unwrap().abs() < 1e-9);
}

#[test]
fn interaction_captures_more_inc_synergy() {
    // weapon +100 base, passive +50% inc。两者有交互（inc 放大 base）。
    // final = (100 + 100) * 1.5 = 300
    // remove weapon: 100 * 1.5 = 150 → delta_weapon = 150
    // remove passive: (100 + 100) * 1.0 = 200 → delta_passive = 100
    // baseline (remove both) = 100
    // interaction = 300 - 100 - (150 + 100) = -50
    let mut db = ModDb::new();
    db.add_mod(
        Modifier::number("MaximumLife", ModType::Base, 100.0)
            .with_origin(life_source(SourceKind::Item, WEAPON)),
    );
    db.add_mod(
        Modifier::number("MaximumLife", ModType::Inc, 50.0)
            .with_origin(life_source(SourceKind::PassiveNode, PASSIVE)),
    );
    let input = MinimalInput {
        base_life: 100.0,
        ..MinimalInput::default()
    };
    let cfg = CalcConfig::new();
    let final_life = calculate_minimal(&db, &cfg, &input).life;
    assert_eq!(final_life, 300.0);

    let weapon = SourceId::new(SourceKind::Item, WEAPON);
    let passive = SourceId::new(SourceKind::PassiveNode, PASSIVE);
    let request = AttributionRequest::new(DisplayStatId::from("Life"))
        .with_mode(AttributionMode::MarginalWithInteraction)
        .with_sources([weapon.clone(), passive.clone()]);

    let report = attribute(&request, final_life, None, |excluded| {
        calculate_minimal(&db_without(&db, excluded), &cfg, &input).life
    });

    let interaction = report.interaction.as_ref().expect("interaction bucket");
    assert!((interaction.marginal_delta.unwrap() - (-50.0)).abs() < 1e-9);
}

#[test]
fn direct_contribution_reads_per_source_value_from_trace() {
    let db = life_db();
    let input = MinimalInput {
        base_life: 100.0,
        ..MinimalInput::default()
    };
    let cfg = CalcConfig::new();
    let traced = calculate_minimal_traced(&db, &cfg, &input);
    let life_node = traced
        .node_for(DisplayStatId::from("Life"))
        .expect("Life output node");

    let request = AttributionRequest::new(DisplayStatId::from("Life"))
        .with_mode(AttributionMode::Direct)
        .with_sources([
            SourceId::new(SourceKind::Item, WEAPON),
            SourceId::new(SourceKind::Item, HELMET),
        ]);

    let report = AttributionReport::direct(&request, traced.output.life, &traced.trace, life_node);

    let weapon = SourceId::new(SourceKind::Item, WEAPON);
    let weapon_entry: &AttributionEntry = report
        .entries
        .iter()
        .find(|entry| entry.source == weapon)
        .expect("weapon direct entry");
    // weapon 直接贡献 60 flat life
    assert_eq!(weapon_entry.value, 60.0);
    assert!((weapon_entry.percent_of_final.unwrap() - 60.0 / 260.0).abs() < 1e-9);
}
