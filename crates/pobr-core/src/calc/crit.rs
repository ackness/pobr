//! 有效暴击管线（resolve_crit）——严格对齐 PoB2 `CalcOffence.lua` 暴击段。
//!
//! 单一实现同时服务面板（`calculate_minimal_vs_enemy`）与归因（`total_dps_traced`）
//! 两个调用点，消除逻辑漂移（gap: crit-resolve-refactor）。归因路径复用 `sum_traced` /
//! `more_factor_traced` 把 BASE/INC/MORE/敌方贡献写入 [`TraceGraph`]
//! （gap: crit-traced-inc-more-untraced）。
//!
//! 计算顺序（不可乱，逐字对照 `CalcOffence.lua` L3681–3838）：
//! 1. `crit_chance = (baseCrit + ΣBase + enemy.SelfCritChance) × (1 + (Σinc + enemy.IncSelfCritChance)/100) × Πmore`
//! 2. clamp 到 `[0, CritChanceCap]`（默认 100%，可被 `Override`/`Sum BASE` 覆盖）→ PreEffectiveCritChance
//! 3. `mode_effective` 下乘命中率降级（闪避二次检定）
//! 4. `Flag(CritChanceLucky)` → `1-(1-c)²`
//! 5. `Flag(BifurcateCrit)` → `1-(1-c)²`（记 PreBifurcate 供额外爆伤）
//! 6. `Flag(InevitableCriticalHits)` → 置 100% + 几何级数 less 爆伤
//! 7. 爆伤：`NoCritMultiplier` → 1.0；否则
//!    `extra = (BASE_BONUS + ΣBase)/100 × (1+Σinc/100) × Πmore`，
//!    + 分岔"两次都暴击"额外一份、+ 敌方 SelfCritMultiplier、+ 必然 less，最终 `1 + max(0, extra)`
//! 8. `crit_effect = (1-c) + c × crit_mult`
//!
//! 出处：agent-docs/critical-hits.md §PoB2 计算实现；
//!       devs/docs/architecture/12-combat-mechanics-architecture.md §4.3；
//!       PathOfBuilding-PoE2 `src/Modules/CalcOffence.lua`。

use pobr_data::constants::PLAYER_BASE_CRIT_DAMAGE_BONUS;
use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb, TraceGraph, TraceNodeId, TraceOperation};

use super::offence::more_factor_traced;
use super::round;

/// 暴击几率上限默认值（百分点）。PoB2 `CritChanceCap` 默认 100，可被覆盖。
const DEFAULT_CRIT_CHANCE_CAP: f64 = 100.0;

/// 有效暴击管线的结算结果。`chance` 为最终（降级/幸运/分岔/必然之后）暴击几率（**fraction**，0..1）；
/// `multiplier` 为爆伤乘区（如 2.0 = 暴击造成双倍）；`effect` 为平均暴击效果
/// `(1-c) + c×mult`，直接乘到平均击中上。`pre_effective_chance` 为命中降级前、cap 之后的
/// 几率（fraction），供 breakdown 显示溢出。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CritOutcome {
    pub chance: f64,
    pub multiplier: f64,
    pub effect: f64,
    pub pre_effective_chance: f64,
}

/// 暴击管线的纯数值结算（fraction 入口/出口）。
///
/// `hit_chance` 为命中率（fraction 0..1，仅 `mode_effective` 下用于降级）。
/// `base_crit` 为武器/宝石底材基础暴击率（fraction，如空手 0.05）。
///
/// 玩家与敌方双 modDB：敌方 `SelfCritChance`/`SelfCritMultiplier`（暴击弱点/标记）仅
/// `mode_effective` 下生效（与 PoB2 一致）。
pub fn resolve_crit(
    player: &ModDb,
    enemy: &ModDb,
    cfg: &CalcConfig,
    hit_chance: f64,
    base_crit: f64,
    mode_effective: bool,
) -> CritOutcome {
    resolve_crit_impl(
        player,
        enemy,
        cfg,
        hit_chance,
        base_crit,
        mode_effective,
        None,
    )
}

/// 与 [`resolve_crit`] 数值一致，但把 BASE/INC/MORE/敌方贡献写入 `trace`，
/// 返回 `(outcome, crit_node)`：`crit_node` 是承载 `crit_effect` 的 [`TraceOperation::Chance`]
/// 节点，调用方可继续 `add_edge` 到下游 DPS 节点。
pub fn resolve_crit_traced(
    player: &ModDb,
    enemy: &ModDb,
    cfg: &CalcConfig,
    hit_chance: f64,
    base_crit: f64,
    mode_effective: bool,
    trace: &mut TraceGraph,
) -> (CritOutcome, TraceNodeId) {
    let mut sink = TraceSink {
        trace,
        node_id: None,
    };
    let outcome = resolve_crit_impl(
        player,
        enemy,
        cfg,
        hit_chance,
        base_crit,
        mode_effective,
        Some(&mut sink),
    );
    let crit_node = sink
        .node_id
        .expect("resolve_crit_impl always sets the crit_effect node when tracing");
    (outcome, crit_node)
}

/// trace 输出汇聚点：实现里把所有贡献边连到此处，并在末尾设置 `node_id`
/// 为承载 `crit_effect` 的节点。
struct TraceSink<'a> {
    trace: &'a mut TraceGraph,
    node_id: Option<TraceNodeId>,
}

#[allow(clippy::too_many_arguments)]
fn resolve_crit_impl(
    player: &ModDb,
    enemy: &ModDb,
    cfg: &CalcConfig,
    hit_chance: f64,
    base_crit: f64,
    mode_effective: bool,
    sink: Option<&mut TraceSink<'_>>,
) -> CritOutcome {
    let crit_chance_names = [ModName::from("CriticalStrikeChance")];
    let self_crit_chance = [ModName::from("SelfCritChance")];

    // --- 1) 基础求和（含敌方暴击弱点 SelfCritChance，仅 mode_effective） ---
    let player_base = player.sum(ModType::Base, cfg, &crit_chance_names);
    let enemy_base = if mode_effective {
        enemy.sum(ModType::Base, cfg, &self_crit_chance)
    } else {
        0.0
    };
    let player_inc = player.sum(ModType::Inc, cfg, &crit_chance_names);
    let enemy_inc = if mode_effective {
        enemy.sum(ModType::Inc, cfg, &self_crit_chance)
    } else {
        0.0
    };
    let chance_more = player.more(cfg, &crit_chance_names);

    // base_crit 与 player_base 均为百分点；统一在百分点空间运算（对齐 PoB2），末端转 fraction。
    let base_pct = (base_crit * 100.0) + player_base + enemy_base;
    let inc = player_inc + enemy_inc;
    let mut crit_pct = base_pct * (1.0 + inc / 100.0) * chance_more;

    // --- 2) cap（默认 100，可 Override 或 Sum BASE 覆盖） ---
    let cap = crit_chance_cap(player, cfg);
    crit_pct = crit_pct.min(cap);
    if base_pct > 0.0 {
        crit_pct = crit_pct.max(0.0);
    }
    let pre_effective_pct = crit_pct;

    // --- 3) mode_effective：命中率降级（闪避二次检定） ---
    if mode_effective {
        crit_pct *= hit_chance;
    }

    // --- 4) 幸运暴击 ---
    if mode_effective && player.flag(cfg, ModName::from("CritChanceLucky")) {
        let c = crit_pct / 100.0;
        crit_pct = (1.0 - (1.0 - c).powi(2)) * 100.0;
    }

    // --- 5) 分岔暴击（记 PreBifurcate 供额外爆伤） ---
    let pre_bifurcate_pct = crit_pct;
    let bifurcate = mode_effective && player.flag(cfg, ModName::from("BifurcateCrit"));
    if bifurcate {
        let c = crit_pct / 100.0;
        crit_pct = (1.0 - (1.0 - c).powi(2)) * 100.0;
    }

    // --- 6) 必然暴击：置 100% + 几何级数 less 爆伤 ---
    let mut inevitable_less_more: Option<f64> = None;
    let inevitable = mode_effective
        && player.flag(cfg, ModName::from("InevitableCriticalHits"))
        && crit_pct > 0.0;
    if inevitable {
        inevitable_less_more = Some(inevitable_less_crit_bonus(
            crit_pct,
            pre_bifurcate_pct,
            bifurcate,
        ));
        crit_pct = 100.0;
    }

    let crit_chance = round((crit_pct / 100.0).clamp(0.0, 1.0));
    let pre_effective_chance = round((pre_effective_pct / 100.0).clamp(0.0, 1.0));

    // --- 7) 爆伤 ---
    let crit_multiplier = resolve_crit_multiplier(
        player,
        enemy,
        cfg,
        mode_effective,
        pre_bifurcate_pct,
        bifurcate,
        inevitable,
        inevitable_less_more,
    );

    // --- 8) 平均暴击效果 ---
    let crit_effect = round(1.0 - crit_chance + crit_chance * crit_multiplier);

    if let Some(sink) = sink {
        record_trace(sink, player, enemy, cfg, mode_effective, crit_effect);
    }

    CritOutcome {
        chance: crit_chance,
        multiplier: crit_multiplier,
        effect: crit_effect,
        pre_effective_chance,
    }
}

/// 暴击几率上限（百分点）。PoB2: `Override("CritChanceCap") or Sum("BASE","CritChanceCap")`，
/// 默认 100。
fn crit_chance_cap(player: &ModDb, cfg: &CalcConfig) -> f64 {
    if let Some(override_cap) = player.override_(cfg, ModName::from("CritChanceCap")) {
        return override_cap;
    }
    let summed = player.sum(ModType::Base, cfg, &[ModName::from("CritChanceCap")]);
    if summed > 0.0 {
        summed
    } else {
        DEFAULT_CRIT_CHANCE_CAP
    }
}

/// 爆伤乘区（`1 + max(0, extra)`）。
///
/// `extra = (BASE_BONUS + ΣCriticalStrikeMultiplier BASE)/100 × (1+Σinc/100) × Πmore`，
/// 然后（仅 mode_effective）叠加分岔额外一份、敌方 SelfCritMultiplier、必然 less。
///
/// 出处：CalcOffence.lua L3781–3827。
#[allow(clippy::too_many_arguments)]
fn resolve_crit_multiplier(
    player: &ModDb,
    enemy: &ModDb,
    cfg: &CalcConfig,
    mode_effective: bool,
    pre_bifurcate_pct: f64,
    bifurcate: bool,
    inevitable: bool,
    inevitable_less_more: Option<f64>,
) -> f64 {
    // NoCritMultiplier：爆伤无效（CalcOffence.lua L3782）。
    if player.flag(cfg, ModName::from("NoCritMultiplier")) {
        return 1.0;
    }

    let crit_multiplier_names = [ModName::from("CriticalStrikeMultiplier")];
    let base = player.sum(ModType::Base, cfg, &crit_multiplier_names);
    let inc = player.sum(ModType::Inc, cfg, &crit_multiplier_names);
    let mut more = player.more(cfg, &crit_multiplier_names);

    // 必然暴击折算的 less 爆伤作为一条额外 MORE（PoB2 `NewMod("CritMultiplier","MORE",lessCritBonus)`）。
    if let Some(less_more) = inevitable_less_more {
        more *= 1.0 + less_more / 100.0;
    }

    // OVERRIDE 硬覆盖爆伤加成（如 `Your Critical Damage Bonus is 250%`）：胜过 base/inc/more
    // （PoB2 OVERRIDE 语义），直接作为加成百分点。否则走 (100 + ΣBASE)×(1+inc)×more。
    let mut extra =
        if let Some(ov) = player.override_(cfg, ModName::from("CriticalStrikeMultiplier")) {
            ov / 100.0
        } else {
            (PLAYER_BASE_CRIT_DAMAGE_BONUS + base) / 100.0 * (1.0 + inc / 100.0) * more
        };

    // 分岔："两次都暴击"的概率额外加权一份爆伤（CalcOffence.lua L3796–3811），
    // 与必然互斥（必然路径已把 100% 暴击折进 less）。
    if mode_effective && bifurcate && !inevitable {
        let bifurcate_multi_chance = pre_bifurcate_pct.powi(2) / 100.0;
        extra += bifurcate_multi_chance * extra / 100.0;
    }

    // 敌方 SelfCritMultiplier（标记等）：BASE 加成 × (1 + INC/100)（CalcOffence.lua L3814–3825）。
    if mode_effective {
        let self_crit_mult = [ModName::from("SelfCritMultiplier")];
        let enemy_inc = 1.0 + enemy.sum(ModType::Inc, cfg, &self_crit_mult) / 100.0;
        extra += enemy.sum(ModType::Base, cfg, &self_crit_mult) / 100.0;
        extra *= enemy_inc;
    }

    round(1.0 + extra.max(0.0))
}

/// 必然暴击的几何级数 less 爆伤（折成一条 `CritMultiplier MORE` 负值，百分点）。
///
/// 严格按 PoB2 `CalcOffence.lua` L3716–3733 的有限 4 项截断：
/// 第 N 次（N=1..4）才暴击的概率 `c·(1-c)^(N-1)` 享受 `{100,70,40,10}%` 档爆伤，
/// 期望倍率 `critBonusMultiplier`，折算 `lessCritBonus = round((1-critBonusMultiplier)·-100)`。
/// 分岔时终端档可暴两次，乘 `2·PreBifurcate/PostBifurcate`。
///
/// `crit_pct` / `pre_bifurcate_pct` 均为百分点。
fn inevitable_less_crit_bonus(crit_pct: f64, pre_bifurcate_pct: f64, bifurcate: bool) -> f64 {
    let crit_chance = crit_pct / 100.0;
    let non_crit = 1.0 - crit_chance;

    let mut crit_bonus_multiplier = crit_chance
        + 0.7 * non_crit * crit_chance
        + 0.4 * non_crit.powi(2) * crit_chance
        + 0.1 * non_crit.powi(3) * crit_chance;

    if bifurcate {
        // 终端档可暴两次，按该档期望暴击数缩放（PoB2 L3724–3729）。
        crit_bonus_multiplier = crit_bonus_multiplier * 2.0 * pre_bifurcate_pct / crit_pct;
    }

    // PoB2: `round((1 - critBonusMultiplier) * -100.0, 0)`（整数）。
    ((1.0 - crit_bonus_multiplier) * -100.0).round()
}

/// 把暴击贡献写入 TraceGraph：BASE/INC/MORE（CritChance 与 CritMultiplier）+ 敌方 SelfCrit*，
/// 全部连入一个承载 `crit_effect` 的 [`TraceOperation::Chance`] 节点。设置 `sink.node_id`。
fn record_trace(
    sink: &mut TraceSink<'_>,
    player: &ModDb,
    enemy: &ModDb,
    cfg: &CalcConfig,
    mode_effective: bool,
    crit_effect: f64,
) {
    let trace = &mut *sink.trace;
    let crit_node = trace.add_node("crit average factor", crit_effect, TraceOperation::Chance);

    let crit_chance_names = [ModName::from("CriticalStrikeChance")];
    let crit_multiplier_names = [ModName::from("CriticalStrikeMultiplier")];

    // CritChance BASE / INC / MORE。
    let chance_base = player.sum_traced(
        ModType::Base,
        cfg,
        &crit_chance_names,
        trace,
        "CriticalStrikeChance BASE sum",
    );
    trace.add_edge(chance_base.node_id, crit_node);
    let chance_inc = player.sum_traced(
        ModType::Inc,
        cfg,
        &crit_chance_names,
        trace,
        "CriticalStrikeChance INC sum",
    );
    trace.add_edge(chance_inc.node_id, crit_node);
    let chance_more = more_factor_traced(
        player,
        cfg,
        &crit_chance_names,
        "CritChance MORE factor",
        trace,
    );
    trace.add_edge(chance_more.node_id, crit_node);

    // CritMultiplier BASE / INC / MORE。
    let mult_base = player.sum_traced(
        ModType::Base,
        cfg,
        &crit_multiplier_names,
        trace,
        "CriticalStrikeMultiplier BASE sum",
    );
    trace.add_edge(mult_base.node_id, crit_node);
    let mult_inc = player.sum_traced(
        ModType::Inc,
        cfg,
        &crit_multiplier_names,
        trace,
        "CriticalStrikeMultiplier INC sum",
    );
    trace.add_edge(mult_inc.node_id, crit_node);
    let mult_more = more_factor_traced(
        player,
        cfg,
        &crit_multiplier_names,
        "CritMultiplier MORE factor",
        trace,
    );
    trace.add_edge(mult_more.node_id, crit_node);

    // 敌方 SelfCritChance / SelfCritMultiplier（仅 mode_effective，归因 EnemyConfig）。
    if mode_effective {
        let enemy_chance = enemy.sum_traced(
            ModType::Base,
            cfg,
            &[ModName::from("SelfCritChance")],
            trace,
            "enemy SelfCritChance BASE sum",
        );
        trace.add_edge(enemy_chance.node_id, crit_node);
        let enemy_mult = enemy.sum_traced(
            ModType::Base,
            cfg,
            &[ModName::from("SelfCritMultiplier")],
            trace,
            "enemy SelfCritMultiplier BASE sum",
        );
        trace.add_edge(enemy_mult.node_id, crit_node);
    }

    // crit 旗标贡献（Lucky / Bifurcate / Inevitable / NoCritMultiplier）连入归因，
    // 使 breakdown 能回溯到旗标来源（SourceKind::Config 或 mod 自身 origin）。
    for flag in [
        "CritChanceLucky",
        "BifurcateCrit",
        "InevitableCriticalHits",
        "NoCritMultiplier",
    ] {
        if player.flag(cfg, ModName::from(flag)) {
            let flag_node =
                trace.add_source_node(format!("{flag} flag"), 1.0, flag_source(player, cfg, flag));
            trace.add_edge(flag_node, crit_node);
        }
    }

    sink.node_id = Some(crit_node);
}

/// 取某旗标命中 modifier 的归因 `SourceId`（无 origin 时回退 Derived `<flag>.FLAG`）。
fn flag_source(player: &ModDb, cfg: &CalcConfig, flag: &str) -> SourceId {
    player
        .flag_origin(cfg, ModName::from(flag))
        .unwrap_or_else(|| SourceId::new(SourceKind::Derived, format!("{flag}.FLAG")))
}
