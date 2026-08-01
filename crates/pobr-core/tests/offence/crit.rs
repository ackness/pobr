//! Unit tests for the effective-crit pipeline (resolve_crit) -- pinned item by item
//! against PoB2 `CalcOffence.lua`'s crit section.
//!
//! Covers gaps: crit-resolve-refactor / crit-chance-cap-queryable / crit-mode-effective /
//! crit-flag-no-crit-multiplier / crit-flag-lucky / crit-flag-bifurcate /
//! crit-flag-inevitable / crit-enemy-selfcrit / crit-traced-inc-more-untraced.

use pobr_core::calc::crit::resolve_crit;
use pobr_core::calc::{
    MinimalInput, calculate_minimal, calculate_minimal_traced, calculate_minimal_vs_enemy,
    resolve_crit_traced,
};
use pobr_core::trace::TraceGraph;
use pobr_core::{CalcConfig, ModDb, Modifier};
use pobr_data::prelude::*;

const EPS: f64 = 1e-9;

fn approx(a: f64, b: f64) {
    assert!((a - b).abs() < EPS, "expected {b}, got {a}");
}

/// A player db containing only a CriticalStrikeChance BASE mod.
fn player_with_base_crit(pct: f64) -> ModDb {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("CriticalStrikeChance", ModType::Base, pct));
    db
}

// Baseline / cap

/// With no flags / enemy mods / mode_effective, resolve_crit matches the legacy
/// formula exactly: crit_chance = base/100, cap=100%,
/// crit_mult = 1 + (100+0)/100 = 2.0.
#[test]
fn baseline_matches_legacy_formula() {
    let db = player_with_base_crit(40.0);
    let enemy = ModDb::new();
    let cfg = CalcConfig::attack();

    let crit = resolve_crit(&db, &enemy, &cfg, 1.0, 0.0, false);
    approx(crit.chance, 0.40);
    approx(crit.pre_effective_chance, 0.40);
    approx(crit.multiplier, 2.0);
    // crit_effect = (1-0.4) + 0.4*2.0 = 1.4
    approx(crit.effect, 1.4);
}

/// CritChanceCap: defaults to 100%; injecting an Override makes the cap take
/// effect (gap crit-chance-cap-queryable).
#[test]
fn crit_chance_cap_default_100_and_override() {
    let cfg = CalcConfig::attack();
    let enemy = ModDb::new();

    // A raw 200% chance is capped at 100% by default -> chance=1.0.
    let db = player_with_base_crit(200.0);
    let crit = resolve_crit(&db, &enemy, &cfg, 1.0, 0.0, false);
    approx(crit.chance, 1.0);
    approx(crit.pre_effective_chance, 1.0);

    // Override CritChanceCap = 50 -> cap becomes 50%.
    let mut capped = player_with_base_crit(200.0);
    capped.add_mod(Modifier::number("CritChanceCap", ModType::Override, 50.0));
    let crit = resolve_crit(&capped, &enemy, &cfg, 1.0, 0.0, false);
    approx(crit.chance, 0.50);
    approx(crit.pre_effective_chance, 0.50);
}

/// CritChanceBase OVERRIDE (vendor CalcOffence.lua:3667-3676 baseCritOverride):
/// directly **replaces** the base item's base crit chance (e.g. Blood Mage's
/// "Sunder the Flesh" = 15%), then stacks normally with BASE/INC.
#[test]
fn crit_chance_base_override_replaces_weapon_base() {
    let cfg = CalcConfig::attack();
    let enemy = ModDb::new();

    let mut db = ModDb::new();
    db.add_mod(Modifier::number("CritChanceBase", ModType::Override, 15.0));
    db.add_mod(Modifier::number(
        "CriticalStrikeChance",
        ModType::Inc,
        100.0,
    ));

    // The base item's 7% (gem base crit) is overridden to 15%: 15 x (1+100/100) = 30%.
    let crit = resolve_crit(&db, &enemy, &cfg, 1.0, 0.07, false);
    approx(crit.chance, 0.30);

    // Without OVERRIDE, the base item value is kept: 7 x 2 = 14%.
    let mut plain = ModDb::new();
    plain.add_mod(Modifier::number(
        "CriticalStrikeChance",
        ModType::Inc,
        100.0,
    ));
    let crit = resolve_crit(&plain, &enemy, &cfg, 1.0, 0.07, false);
    approx(crit.chance, 0.14);
}

// mode_effective hit-chance downgrade

/// With mode_effective=true, effective crit = crit chance x hit chance;
/// PreEffective retains the value before the downgrade.
#[test]
fn mode_effective_downgrades_crit_by_hit_chance() {
    let db = player_with_base_crit(50.0);
    let enemy = ModDb::new();
    let cfg = CalcConfig::attack().with_mode_effective(true);

    let crit = resolve_crit(&db, &enemy, &cfg, 0.6, 0.0, true);
    // effective = 50% x 0.6 = 30%.
    approx(crit.chance, 0.30);
    // PreEffective is the post-cap value before the downgrade = 50%.
    approx(crit.pre_effective_chance, 0.50);
}

/// With mode_effective=false, no hit-chance downgrade is applied (panel semantics,
/// for backward compatibility).
#[test]
fn non_effective_skips_hit_downgrade() {
    let db = player_with_base_crit(50.0);
    let enemy = ModDb::new();
    let cfg = CalcConfig::attack();

    let crit = resolve_crit(&db, &enemy, &cfg, 0.3, 0.0, false);
    approx(crit.chance, 0.50);
}

// NoCritMultiplier

/// NoCritMultiplier flag -> crit damage = 1.0, so crit_effect = 1 regardless of crit
/// chance (total hit == non-crit).
#[test]
fn no_crit_multiplier_flag_neutralizes_bonus() {
    let mut db = player_with_base_crit(50.0);
    db.add_mod(Modifier::number(
        "CriticalStrikeMultiplier",
        ModType::Base,
        80.0,
    ));
    db.add_mod(Modifier::flag("NoCritMultiplier"));
    let enemy = ModDb::new();
    let cfg = CalcConfig::attack();

    let crit = resolve_crit(&db, &enemy, &cfg, 1.0, 0.0, false);
    approx(crit.multiplier, 1.0);
    approx(crit.effect, 1.0);
}

/// End-to-end: with NoCritMultiplier, total_hit_avg == the non-crit average hit.
#[test]
fn no_crit_multiplier_end_to_end_total_hit_equals_non_crit() {
    let mut db = player_with_base_crit(75.0);
    db.add_mod(Modifier::flag("NoCritMultiplier"));
    let input = MinimalInput {
        base_hit_min: 100.0,
        base_hit_max: 100.0,
        base_action_rate: 1.0,
        ..MinimalInput::default()
    };
    let out = calculate_minimal(&db, &CalcConfig::attack(), &input);
    let non_crit: f64 = out.damage_components.iter().map(|c| c.avg()).sum();
    approx(out.total_hit_avg, non_crit);
}

// Lucky

/// CritChanceLucky flag (mode_effective): 30% -> 1-(1-0.3)^2 = 0.51.
#[test]
fn lucky_crit_chance_30_to_51() {
    let mut db = player_with_base_crit(30.0);
    db.add_mod(Modifier::flag("CritChanceLucky"));
    let enemy = ModDb::new();
    let cfg = CalcConfig::spell().with_mode_effective(true);

    // Spells always hit -> hit_chance 1.0, Lucky is applied after the downgrade.
    let crit = resolve_crit(&db, &enemy, &cfg, 1.0, 0.0, true);
    approx(crit.chance, 0.51);
}

/// Lucky only takes effect under mode_effective (PoB2 gate).
#[test]
fn lucky_inactive_without_mode_effective() {
    let mut db = player_with_base_crit(30.0);
    db.add_mod(Modifier::flag("CritChanceLucky"));
    let enemy = ModDb::new();
    let cfg = CalcConfig::spell();

    let crit = resolve_crit(&db, &enemy, &cfg, 1.0, 0.0, false);
    approx(crit.chance, 0.30);
}

// Bifurcate

/// BifurcateCrit flag (mode_effective): 50% -> 1-(1-0.5)^2 = 0.75.
#[test]
fn bifurcate_crit_chance_50_to_75() {
    let mut db = player_with_base_crit(50.0);
    db.add_mod(Modifier::flag("BifurcateCrit"));
    let enemy = ModDb::new();
    let cfg = CalcConfig::spell().with_mode_effective(true);

    let crit = resolve_crit(&db, &enemy, &cfg, 1.0, 0.0, true);
    approx(crit.chance, 0.75);
}

/// Bifurcate extra crit damage: the conditional probability "both hits crit given
/// at least one crits" adds an extra weighting of `extra` (vendor CalcOffence.lua's
/// `conditionalBifurcateChance`).
/// base_crit=50%, base extra = (100)/100 = 1.0;
/// bifurcateMultiChance = 50^2/100 = 25; effective crit = 75;
/// conditional = 25/75 = 1/3; extra' = 1.0x(1+1/3) -> crit_mult = 2.3333.
#[test]
fn bifurcate_adds_extra_crit_multiplier() {
    let mut db = player_with_base_crit(50.0);
    db.add_mod(Modifier::flag("BifurcateCrit"));
    let enemy = ModDb::new();
    let cfg = CalcConfig::spell().with_mode_effective(true);

    let crit = resolve_crit(&db, &enemy, &cfg, 1.0, 0.0, true);
    approx(crit.multiplier, 1.0 + 1.0 + 25.0 / 75.0);
}

// Inevitable

/// InevitableCriticalHits flag (mode_effective): forces crit to 100%, and applies a
/// geometric-series discount to crit damage as a less modifier.
/// effective crit=50% (spell, hit 1.0) -> lessMore = round((1 - m)*-100) = -27 (see
/// PoB2's 4-term truncation).
/// crit_mult = 1 + (100/100)*(1 + (-27)/100) = 1 + 0.73 = 1.73.
#[test]
fn inevitable_forces_100_and_geometric_less_bonus() {
    let mut db = player_with_base_crit(50.0);
    db.add_mod(Modifier::flag("InevitableCriticalHits"));
    let enemy = ModDb::new();
    let cfg = CalcConfig::spell().with_mode_effective(true);

    let crit = resolve_crit(&db, &enemy, &cfg, 1.0, 0.0, true);
    approx(crit.chance, 1.0);
    // +-0.1% tolerance (per the spec).
    assert!(
        (crit.multiplier - 1.73).abs() < 1e-3,
        "inevitable crit_mult expected ~1.73, got {}",
        crit.multiplier
    );
    // crit_effect = (1-1) + 1*1.73 = 1.73.
    assert!((crit.effect - 1.73).abs() < 1e-3);
}

/// Inevitable applies no discount at 100% effective crit (lessMore=0 -> crit_mult=2.0).
#[test]
fn inevitable_at_full_crit_no_penalty() {
    let mut db = player_with_base_crit(100.0);
    db.add_mod(Modifier::flag("InevitableCriticalHits"));
    let enemy = ModDb::new();
    let cfg = CalcConfig::spell().with_mode_effective(true);

    let crit = resolve_crit(&db, &enemy, &cfg, 1.0, 0.0, true);
    approx(crit.chance, 1.0);
    approx(crit.multiplier, 2.0);
}

// enemy SelfCrit

/// Enemy SelfCritChance (a crit-weakness debuff) adds onto the base crit chance
/// (mode_effective): player base 20% + enemy 10% = 30% -> chance 0.30.
#[test]
fn enemy_self_crit_chance_raises_base() {
    let db = player_with_base_crit(20.0);
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("SelfCritChance", ModType::Base, 10.0));
    let cfg = CalcConfig::spell().with_mode_effective(true);

    let crit = resolve_crit(&db, &enemy, &cfg, 1.0, 0.0, true);
    approx(crit.chance, 0.30);
}

/// Enemy SelfCritChance has no effect when mode_effective=false (panel semantics).
#[test]
fn enemy_self_crit_chance_ignored_when_not_effective() {
    let db = player_with_base_crit(20.0);
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("SelfCritChance", ModType::Base, 10.0));
    let cfg = CalcConfig::spell();

    let crit = resolve_crit(&db, &enemy, &cfg, 1.0, 0.0, false);
    approx(crit.chance, 0.20);
}

/// Enemy SelfCritMultiplier (e.g. from a mark, a BASE percentage) raises crit
/// damage: base extra=1.0; + enemy 50/100 = 1.5 -> crit_mult = 2.5.
#[test]
fn enemy_self_crit_multiplier_raises_bonus() {
    let db = player_with_base_crit(100.0);
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("SelfCritMultiplier", ModType::Base, 50.0));
    let cfg = CalcConfig::spell().with_mode_effective(true);

    let crit = resolve_crit(&db, &enemy, &cfg, 1.0, 0.0, true);
    approx(crit.multiplier, 2.5);
}

/// End-to-end (vs enemy): under mode_effective, enemy SelfCritChance raises
/// total_hit_avg.
#[test]
fn enemy_self_crit_chance_increases_total_hit_end_to_end() {
    let db = player_with_base_crit(0.0);
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("SelfCritChance", ModType::Base, 100.0));
    let input = MinimalInput {
        base_hit_min: 100.0,
        base_hit_max: 100.0,
        base_action_rate: 1.0,
        ..MinimalInput::default()
    };
    let cfg = CalcConfig::spell().with_mode_effective(true);
    let out = calculate_minimal_vs_enemy(&db, &enemy, &cfg, &input);
    // 100% enemy crit-weakness -> always crits, crit_mult=2.0, total_hit_avg =
    // 100*2 = 200.
    approx(out.crit_chance, 1.0);
    approx(out.total_hit_avg, 200.0);
}

// traced attribution

/// The traced path wires CritChance / CritMultiplier's inc/more contributions into
/// the TraceGraph: source_ancestors(crit_node) should be able to find both the inc
/// and more sources.
#[test]
fn traced_records_inc_and_more_sources() {
    let mut db = ModDb::new();
    db.add_mod(
        Modifier::number("CriticalStrikeChance", ModType::Base, 20.0).with_origin(
            ModifierSource::new(SourceId::new(SourceKind::Item, "weapon.base_crit")),
        ),
    );
    db.add_mod(
        Modifier::number("CriticalStrikeChance", ModType::Inc, 50.0).with_origin(
            ModifierSource::new(SourceId::new(SourceKind::PassiveNode, "tree.crit_inc")),
        ),
    );
    db.add_mod(
        Modifier::number("CriticalStrikeMultiplier", ModType::Inc, 30.0).with_origin(
            ModifierSource::new(SourceId::new(SourceKind::PassiveNode, "tree.critmult_inc")),
        ),
    );
    db.add_mod(
        Modifier::number("CriticalStrikeMultiplier", ModType::More, 20.0).with_origin(
            ModifierSource::new(SourceId::new(
                SourceKind::SupportGem,
                "support.critmult_more",
            )),
        ),
    );
    let enemy = ModDb::new();
    let cfg = CalcConfig::attack();
    let mut trace = TraceGraph::new();

    let (_outcome, crit_node) = resolve_crit_traced(&db, &enemy, &cfg, 1.0, 0.0, false, &mut trace);

    let ancestors: Vec<String> = trace
        .source_ancestors(crit_node)
        .iter()
        .map(|s| s.id.clone())
        .collect();
    assert!(
        ancestors.iter().any(|id| id == "tree.crit_inc"),
        "missing CritChance INC source: {ancestors:?}"
    );
    assert!(
        ancestors.iter().any(|id| id == "tree.critmult_inc"),
        "missing CritMultiplier INC source: {ancestors:?}"
    );
    assert!(
        ancestors.iter().any(|id| id == "support.critmult_more"),
        "missing CritMultiplier MORE source: {ancestors:?}"
    );
}

/// traced and plain resolve_crit produce identical values (guards against the two
/// paths drifting apart).
#[test]
fn traced_matches_plain() {
    let mut db = player_with_base_crit(35.0);
    db.add_mod(Modifier::number("CriticalStrikeChance", ModType::Inc, 80.0));
    db.add_mod(Modifier::number(
        "CriticalStrikeMultiplier",
        ModType::Base,
        40.0,
    ));
    db.add_mod(Modifier::number(
        "CriticalStrikeMultiplier",
        ModType::More,
        25.0,
    ));
    let enemy = ModDb::new();
    let cfg = CalcConfig::attack();

    let plain = resolve_crit(&db, &enemy, &cfg, 1.0, 0.0, false);
    let mut trace = TraceGraph::new();
    let (traced, _node) = resolve_crit_traced(&db, &enemy, &cfg, 1.0, 0.0, false, &mut trace);

    approx(traced.chance, plain.chance);
    approx(traced.multiplier, plain.multiplier);
    approx(traced.effect, plain.effect);
}

/// calculate_minimal_traced's overall DPS matches plain (end-to-end, no drift, now
/// that crit is wired in).
#[test]
fn calculate_minimal_traced_dps_unchanged() {
    let mut db = player_with_base_crit(25.0);
    db.add_mod(Modifier::number(
        "CriticalStrikeMultiplier",
        ModType::Inc,
        50.0,
    ));
    let input = MinimalInput {
        base_hit_min: 80.0,
        base_hit_max: 120.0,
        base_action_rate: 1.5,
        ..MinimalInput::default()
    };
    let cfg = CalcConfig::attack();
    let plain = calculate_minimal(&db, &cfg, &input);
    let traced = calculate_minimal_traced(&db, &cfg, &input);
    approx(traced.output.dps, plain.dps);
    approx(traced.output.crit_multiplier, plain.crit_multiplier);
    approx(traced.output.total_hit_avg, plain.total_hit_avg);
}
