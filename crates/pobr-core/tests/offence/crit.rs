//! 有效暴击管线（resolve_crit）单测——逐项对齐 PoB2 `CalcOffence.lua` 暴击段。
//!
//! 覆盖 gap：crit-resolve-refactor / crit-chance-cap-queryable / crit-mode-effective /
//! crit-flag-no-crit-multiplier / crit-flag-lucky / crit-flag-bifurcate /
//! crit-flag-inevitable / crit-enemy-selfcrit / crit-traced-inc-more-untraced。

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

/// 仅含 CriticalStrikeChance BASE 的玩家 db。
fn player_with_base_crit(pct: f64) -> ModDb {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("CriticalStrikeChance", ModType::Base, pct));
    db
}

// ─────────────────────────── 基线 / cap ───────────────────────────

/// 无任何旗标 / 敌方 / mode_effective 时，resolve_crit 与历史逐字一致：
/// crit_chance = base/100，cap=100%，crit_mult = 1 + (100+0)/100 = 2.0。
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

/// CritChanceCap：默认 100%；注入 Override 后上限生效（gap crit-chance-cap-queryable）。
#[test]
fn crit_chance_cap_default_100_and_override() {
    let cfg = CalcConfig::attack();
    let enemy = ModDb::new();

    // 200% 原始几率默认被 100% cap → chance=1.0。
    let db = player_with_base_crit(200.0);
    let crit = resolve_crit(&db, &enemy, &cfg, 1.0, 0.0, false);
    approx(crit.chance, 1.0);
    approx(crit.pre_effective_chance, 1.0);

    // Override CritChanceCap = 50 → 上限 50%。
    let mut capped = player_with_base_crit(200.0);
    capped.add_mod(Modifier::number("CritChanceCap", ModType::Override, 50.0));
    let crit = resolve_crit(&capped, &enemy, &cfg, 1.0, 0.0, false);
    approx(crit.chance, 0.50);
    approx(crit.pre_effective_chance, 0.50);
}

/// CritChanceBase OVERRIDE（vendor CalcOffence.lua:3667-3676 baseCritOverride）：
/// 直接**替换**底材基础暴击（如 Blood Mage『Sunder the Flesh』= 15%），
/// 与 BASE/INC 正常叠乘。
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

    // 底材 7%（gem 基础暴击）被 15% 覆盖：15 × (1+100/100) = 30%。
    let crit = resolve_crit(&db, &enemy, &cfg, 1.0, 0.07, false);
    approx(crit.chance, 0.30);

    // 无 OVERRIDE 时维持底材：7 × 2 = 14%。
    let mut plain = ModDb::new();
    plain.add_mod(Modifier::number(
        "CriticalStrikeChance",
        ModType::Inc,
        100.0,
    ));
    let crit = resolve_crit(&plain, &enemy, &cfg, 1.0, 0.07, false);
    approx(crit.chance, 0.14);
}

// ─────────────────────────── mode_effective 命中降级 ───────────────────────────

/// mode_effective=true 时有效暴击 = 暴击几率 × 命中率；PreEffective 保留降级前值。
#[test]
fn mode_effective_downgrades_crit_by_hit_chance() {
    let db = player_with_base_crit(50.0);
    let enemy = ModDb::new();
    let cfg = CalcConfig::attack().with_mode_effective(true);

    let crit = resolve_crit(&db, &enemy, &cfg, 0.6, 0.0, true);
    // effective = 50% × 0.6 = 30%。
    approx(crit.chance, 0.30);
    // PreEffective 是降级前的 cap 后值 = 50%。
    approx(crit.pre_effective_chance, 0.50);
}

/// mode_effective=false 时不做命中降级（面板口径，向后兼容）。
#[test]
fn non_effective_skips_hit_downgrade() {
    let db = player_with_base_crit(50.0);
    let enemy = ModDb::new();
    let cfg = CalcConfig::attack();

    let crit = resolve_crit(&db, &enemy, &cfg, 0.3, 0.0, false);
    approx(crit.chance, 0.50);
}

// ─────────────────────────── NoCritMultiplier ───────────────────────────

/// NoCritMultiplier flag → 爆伤 = 1.0，任何暴击几率下 crit_effect = 1（总击中 == 非暴击）。
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

/// 端到端：NoCritMultiplier 下 total_hit_avg == 非暴击平均击中。
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

// ─────────────────────────── Lucky ───────────────────────────

/// CritChanceLucky flag（mode_effective）：30% → 1-(1-0.3)² = 0.51。
#[test]
fn lucky_crit_chance_30_to_51() {
    let mut db = player_with_base_crit(30.0);
    db.add_mod(Modifier::flag("CritChanceLucky"));
    let enemy = ModDb::new();
    let cfg = CalcConfig::spell().with_mode_effective(true);

    // 法术必中 → hit_chance 1.0，Lucky 在降级之后。
    let crit = resolve_crit(&db, &enemy, &cfg, 1.0, 0.0, true);
    approx(crit.chance, 0.51);
}

/// Lucky 仅 mode_effective 下生效（PoB2 gate）。
#[test]
fn lucky_inactive_without_mode_effective() {
    let mut db = player_with_base_crit(30.0);
    db.add_mod(Modifier::flag("CritChanceLucky"));
    let enemy = ModDb::new();
    let cfg = CalcConfig::spell();

    let crit = resolve_crit(&db, &enemy, &cfg, 1.0, 0.0, false);
    approx(crit.chance, 0.30);
}

// ─────────────────────────── Bifurcate ───────────────────────────

/// BifurcateCrit flag（mode_effective）：50% → 1-(1-0.5)² = 0.75。
#[test]
fn bifurcate_crit_chance_50_to_75() {
    let mut db = player_with_base_crit(50.0);
    db.add_mod(Modifier::flag("BifurcateCrit"));
    let enemy = ModDb::new();
    let cfg = CalcConfig::spell().with_mode_effective(true);

    let crit = resolve_crit(&db, &enemy, &cfg, 1.0, 0.0, true);
    approx(crit.chance, 0.75);
}

/// Bifurcate 额外爆伤：条件概率「至少一次暴击时两次都暴击」额外加权一份 extra
/// （vendor CalcOffence.lua `conditionalBifurcateChance`）。
/// base_crit=50%，extra 基础 = (100)/100 = 1.0；
/// bifurcateMultiChance = 50²/100 = 25；有效暴击 = 75；
/// conditional = 25/75 = 1/3；extra' = 1.0×(1+1/3) → crit_mult = 2.3333。
#[test]
fn bifurcate_adds_extra_crit_multiplier() {
    let mut db = player_with_base_crit(50.0);
    db.add_mod(Modifier::flag("BifurcateCrit"));
    let enemy = ModDb::new();
    let cfg = CalcConfig::spell().with_mode_effective(true);

    let crit = resolve_crit(&db, &enemy, &cfg, 1.0, 0.0, true);
    approx(crit.multiplier, 1.0 + 1.0 + 25.0 / 75.0);
}

// ─────────────────────────── Inevitable ───────────────────────────

/// InevitableCriticalHits flag（mode_effective）：crit 置 100%，且按几何级数折算 less 爆伤。
/// effective crit=50%（spell, hit 1.0）→ lessMore = round((1 - m)*-100) = -27（见 PoB2 4 项截断）。
/// crit_mult = 1 + (100/100)*(1 + (-27)/100) = 1 + 0.73 = 1.73。
#[test]
fn inevitable_forces_100_and_geometric_less_bonus() {
    let mut db = player_with_base_crit(50.0);
    db.add_mod(Modifier::flag("InevitableCriticalHits"));
    let enemy = ModDb::new();
    let cfg = CalcConfig::spell().with_mode_effective(true);

    let crit = resolve_crit(&db, &enemy, &cfg, 1.0, 0.0, true);
    approx(crit.chance, 1.0);
    // ±0.1% 容差（题面要求）。
    assert!(
        (crit.multiplier - 1.73).abs() < 1e-3,
        "inevitable crit_mult expected ~1.73, got {}",
        crit.multiplier
    );
    // crit_effect = (1-1) + 1*1.73 = 1.73。
    assert!((crit.effect - 1.73).abs() < 1e-3);
}

/// Inevitable 在 100% effective crit 下不折损（lessMore=0 → crit_mult=2.0）。
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

// ─────────────────────────── enemy SelfCrit ───────────────────────────

/// 敌方 SelfCritChance（暴击弱点）加在基础暴击上（mode_effective）：
/// 玩家 base 20% + enemy 10% = 30% → chance 0.30。
#[test]
fn enemy_self_crit_chance_raises_base() {
    let db = player_with_base_crit(20.0);
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("SelfCritChance", ModType::Base, 10.0));
    let cfg = CalcConfig::spell().with_mode_effective(true);

    let crit = resolve_crit(&db, &enemy, &cfg, 1.0, 0.0, true);
    approx(crit.chance, 0.30);
}

/// 敌方 SelfCritChance 不在 mode_effective=false 下生效（面板口径）。
#[test]
fn enemy_self_crit_chance_ignored_when_not_effective() {
    let db = player_with_base_crit(20.0);
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("SelfCritChance", ModType::Base, 10.0));
    let cfg = CalcConfig::spell();

    let crit = resolve_crit(&db, &enemy, &cfg, 1.0, 0.0, false);
    approx(crit.chance, 0.20);
}

/// 敌方 SelfCritMultiplier（标记等，BASE 百分点）抬升爆伤：
/// base extra=1.0；+ enemy 50/100 = 1.5 → crit_mult = 2.5。
#[test]
fn enemy_self_crit_multiplier_raises_bonus() {
    let db = player_with_base_crit(100.0);
    let mut enemy = ModDb::new();
    enemy.add_mod(Modifier::number("SelfCritMultiplier", ModType::Base, 50.0));
    let cfg = CalcConfig::spell().with_mode_effective(true);

    let crit = resolve_crit(&db, &enemy, &cfg, 1.0, 0.0, true);
    approx(crit.multiplier, 2.5);
}

/// 端到端（vs enemy）：mode_effective 下敌方 SelfCritChance 提升 total_hit_avg。
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
    // 100% 敌方暴击弱点 → 全暴击，crit_mult=2.0，total_hit_avg = 100*2 = 200。
    approx(out.crit_chance, 1.0);
    approx(out.total_hit_avg, 200.0);
}

// ─────────────────────────── traced 归因 ───────────────────────────

/// traced 路径把 CritChance / CritMultiplier 的 inc/more 贡献接入 TraceGraph：
/// source_ancestors(crit_node) 应能找到 inc 与 more 的来源。
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

/// traced 与 plain resolve_crit 数值一致（保证两路径不漂移）。
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

/// calculate_minimal_traced 整体 DPS 与 plain 一致（端到端不漂移，含 crit 接线后）。
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
