//!  双 pass × 归因模型集成测试。
//!
//! 对照 RFC `audits/rearchitecture-2026-06-10/blueprints/m4-rfc-attribution-passes.md`
//! §2（PassId 分区）/ §3（Combine 权重表）/ §5（direct/marginal 兼容性）与评审报告
//! `m4-rfc-review.md` 条件 C2（I4 分三组断言）/ C3（I1 强不变式）/ C4（pass_filter
//! 口径）。不变量编号 I1–I6 与 RFC §5.5 表一致。

use pobr_core::attribution::{AttributionMode, AttributionRequest, attribute};
use pobr_core::{CombineMode, CritTag, HandTag, PassId, TraceGraph, TraceOperation};
use pobr_data::prelude::*;

fn src(id: &str) -> SourceId {
    SourceId::new(SourceKind::Item, id)
}

const MH_PASS: PassId = PassId::hand_blended(HandTag::MainHand);
const OH_PASS: PassId = PassId::hand_blended(HandTag::OffHand);

// I4 组 1：线性模式（OR / ADD / AVERAGE / DPS / CRIT-非doubleHits）——
// 「Σ weights×leg == 合并值」严格成立（权重为常数，合并是腿的线性组合）。

/// 线性模式逐一断言：`combine(legs) == Σ linearized_weights×legs`（手算值）。
#[test]
fn i4_linear_modes_weighted_sum_equals_combined_value() {
    let legs = [30.0, 40.0];
    // (mode, 手算合并值)；vendor CalcOffence.lua:2453-2545。
    let cases: &[(CombineMode, f64)] = &[
        (CombineMode::Or, 30.0),                          // MH or OH = MH
        (CombineMode::Add, 70.0),                         // MH + OH
        (CombineMode::Average, 35.0),                     // (MH+OH)/2
        (CombineMode::Dps { double_hits: false }, 35.0),  // (MH+OH)/2
        (CombineMode::Dps { double_hits: true }, 70.0),   // MH + OH
        (CombineMode::Crit { double_hits: false }, 35.0), // (MH+OH)/2
    ];
    for (mode, expected) in cases {
        let combined = mode.combine(&legs).expect("自给模式");
        assert_eq!(combined, *expected, "{mode:?} 合并值手算");
        let weights = mode.linearized_weights(&legs).expect("自给模式");
        let weighted_sum: f64 = weights.iter().zip(legs.iter()).map(|(w, l)| w * l).sum();
        assert!(
            (weighted_sum - combined).abs() < 1e-12,
            "{mode:?} 线性模式必须守恒：Σw×leg={weighted_sum} != {combined}"
        );
    }
}

/// 线性模式（DPS-doubleHits=ADD 形）在真实图上的端到端守恒：
/// 各来源 direct 之和 == 顶层合并输出（腿内为纯加法链时严格成立）。
#[test]
fn i4_linear_graph_direct_sums_to_output() {
    let mut trace = TraceGraph::new();

    trace.begin_pass(MH_PASS);
    let mh_a = trace.add_source_node("mh weapon", 30.0, src("weapon1"));
    let mh_g = trace.add_source_node("global node (MH pass copy)", 10.0, src("global"));
    let mh = trace.add_node("MH dps", 40.0, TraceOperation::Add);
    trace.add_edge(mh_a, mh);
    trace.add_edge(mh_g, mh);
    trace.end_pass();

    trace.begin_pass(OH_PASS);
    let oh_a = trace.add_source_node("oh weapon", 20.0, src("weapon2"));
    let oh_g = trace.add_source_node("global node (OH pass copy)", 10.0, src("global"));
    let oh = trace.add_node("OH dps", 30.0, TraceOperation::Add);
    trace.add_edge(oh_a, oh);
    trace.add_edge(oh_g, oh);
    trace.end_pass();

    let mode = CombineMode::Dps { double_hits: true };
    let legs = [40.0, 30.0];
    let combined_value = mode.combine(&legs).unwrap(); // 70
    let weights = mode.linearized_weights(&legs).unwrap(); // [1,1]
    let combined = trace.add_combine_node(
        "TotalDPS",
        combined_value,
        mode,
        &[(mh, weights[0]), (oh, weights[1])],
    );

    let request = AttributionRequest::new(DisplayStatId::from("TotalDPS"))
        .with_mode(AttributionMode::Direct)
        .with_sources([src("weapon1"), src("weapon2"), src("global")]);
    let report = pobr_core::attribution::AttributionReport::direct(
        &request,
        combined_value,
        &trace,
        combined,
    );

    // per-hand 词条只进对应腿；全局来源两腿各计一次再加权（ADD 权重 (1,1) 下计两次
    // = 它对「两手之和」的真实直接贡献，RFC §5.1）。
    assert_eq!(report.entries[0].value, 30.0, "weapon1 只进 MH 腿");
    assert_eq!(report.entries[1].value, 20.0, "weapon2 只进 OH 腿");
    assert_eq!(report.entries[2].value, 20.0, "global 两腿各 10×1.0");
    let total: f64 = report.entries.iter().map(|entry| entry.value).sum();
    assert_eq!(total, combined_value, "线性模式 direct 全来源之和守恒");
}

// I4 组 2：HARMONICMEAN——非线性，但调和平均是 1 次齐次函数，欧拉定理使
// 「Σ偏导×leg == 值」**恰好成立**（齐次巧合，评审 C2 要求单独注明；
// 不要据此推广到其它非线性模式）。

#[test]
fn i4_harmonic_mean_weighted_sum_homogeneity_coincidence() {
    let legs = [4.0, 6.0];
    let mode = CombineMode::HarmonicMean;
    let combined = mode.combine(&legs).unwrap();
    assert!((combined - 4.8).abs() < 1e-12, "2/(1/4+1/6) = 4.8");

    let weights = mode.linearized_weights(&legs).unwrap();
    // 解析偏导：∂/∂MH [2·MH·OH/(MH+OH)] = 2·OH²/(MH+OH)²。
    assert!((weights[0] - 2.0 * 36.0 / 100.0).abs() < 1e-12);
    assert!((weights[1] - 2.0 * 16.0 / 100.0).abs() < 1e-12);

    // 齐次巧合（Euler，1 次齐次）：加权和 == 值。仅此模式如此，勿推广。
    let weighted_sum: f64 = weights.iter().zip(legs.iter()).map(|(w, l)| w * l).sum();
    assert!((weighted_sum - combined).abs() < 1e-12);
}

/// HARMONICMEAN 零腿边角（vendor :2466-2467）：任一腿 0 → 输出 0、权重 (0,0)，
/// direct 得 0 与输出一致；真实敏感度由 marginal 兜底。
#[test]
fn i4_harmonic_mean_zero_leg_yields_zero_weights() {
    let legs = [0.0, 6.0];
    let mode = CombineMode::HarmonicMean;
    assert_eq!(mode.combine(&legs), Some(0.0));
    assert_eq!(mode.linearized_weights(&legs), Some(vec![0.0, 0.0]));
}

// I4 组 3：CRIT-doubleHits / CHANCE / CHANCE_AILMENT / CritBlend——
// **仅断言权重 == 解析偏导**。direct 在这些模式下**不守恒**
// （doubleHits 的交叉项被偏导重复扣减：Σw×leg = MH+OH−2·MH·OH/100 ≠ 合并值
// MH+OH−MH·OH/100），守恒语义由 marginal 整管线重算兜底（RFC §5.2、评审 C2）。

#[test]
fn i4_crit_double_hits_weights_are_partial_derivatives_not_conserving() {
    let legs = [30.0, 40.0];
    let mode = CombineMode::Crit { double_hits: true };
    // vendor :2461：MH + OH − MH×OH/100 = 70 − 12 = 58。
    let combined = mode.combine(&legs).unwrap();
    assert!((combined - 58.0).abs() < 1e-12);

    // 解析偏导：∂/∂MH = 1 − OH/100 = 0.6；∂/∂OH = 1 − MH/100 = 0.7。
    let weights = mode.linearized_weights(&legs).unwrap();
    assert!((weights[0] - 0.6).abs() < 1e-12);
    assert!((weights[1] - 0.7).abs() < 1e-12);

    // 显式记录不守恒（差值 = 交叉项 MH·OH/100 = 12，被偏导计了两次）：
    let weighted_sum: f64 = weights.iter().zip(legs.iter()).map(|(w, l)| w * l).sum();
    assert!((weighted_sum - 46.0).abs() < 1e-12);
    assert!(
        (combined - weighted_sum - 12.0).abs() < 1e-12,
        "doubleHits direct 不守恒是固有性质，由 marginal 兜底"
    );
}

/// CHANCE / CHANCE_AILMENT / CritBlend 是系数模式：合并值与权重由外生系数冻结
/// （RFC §3.3），`combine`/`linearized_weights` 拒绝（None），权重由构图侧给出。
/// 此处验证「冻结系数 == 解析偏导」的形状（以 CritBlend 为例 + Chance portion 公式）。
#[test]
fn i4_coefficient_modes_frozen_weights_match_partials() {
    for mode in [
        CombineMode::Chance,
        CombineMode::ChanceAilment,
        CombineMode::CritBlend,
    ] {
        assert_eq!(mode.combine(&[1.0, 2.0]), None, "{mode:?} 是系数模式");
        assert_eq!(mode.linearized_weights(&[1.0, 2.0]), None);
    }

    // CritBlend（vendor :4395）：blend = hit×(1−c) + crit×c；
    // ∂blend/∂hit = 1−c、∂blend/∂crit = c —— 冻结权重 [1−c, c] 即偏导。
    let c: f64 = 0.3;
    let (hit, crit) = (100.0, 250.0);
    let blend = hit * (1.0 - c) + crit * c;
    assert!((blend - 145.0).abs() < 1e-12);
    // （c 自身的来源解释走 c 节点的入边 / marginal，weights 冻结为常数——RFC §3.3。）

    // CHANCE（vendor :2471-2480）：portion = chance×HitChance 占比；
    // 输出 = MH×mainPortion + OH×offPortion，∂/∂MH = mainPortion（portion 冻结）。
    let (mh_chance, oh_chance): (f64, f64) = (25.0 * 0.9, 15.0 * 0.8);
    let main_portion = mh_chance / (mh_chance + oh_chance);
    let off_portion = oh_chance / (mh_chance + oh_chance);
    assert!((main_portion + off_portion - 1.0).abs() < 1e-12);
    assert!(main_portion > off_portion);
}

// I6：marginal 在非线性（doubleHits）样例上 ≠ direct 且符合手算。

#[test]
fn i6_marginal_differs_from_direct_on_double_hits_cross_term() {
    // 场景：MH = 20(基底) + 10(global)，OH = 30(基底) + 10(global)；
    // combined = MH + OH − MH·OH/100。
    // final = 30 + 40 − 12 = 58。
    // direct(global) = w_mh×10 + w_oh×10 = (1−0.40)×10 + (1−0.30)×10 = 13。
    // marginal(global)：去掉 global → MH=20, OH=30 → 20+30−6 = 44；delta = 58−44 = 14。
    // 13 ≠ 14（差 = 交叉项二阶效应 10×10/100 = 1），符合手算。
    let combine = |mh: f64, oh: f64| mh + oh - mh * oh / 100.0;
    let final_value = combine(30.0, 40.0);

    let mut trace = TraceGraph::new();
    trace.begin_pass(MH_PASS);
    let mh_base = trace.add_source_node("mh base", 20.0, src("weapon1"));
    let mh_glob = trace.add_source_node("global (MH copy)", 10.0, src("global"));
    let mh = trace.add_node("MH stat", 30.0, TraceOperation::Add);
    trace.add_edge(mh_base, mh);
    trace.add_edge(mh_glob, mh);
    trace.end_pass();
    trace.begin_pass(OH_PASS);
    let oh_base = trace.add_source_node("oh base", 30.0, src("weapon2"));
    let oh_glob = trace.add_source_node("global (OH copy)", 10.0, src("global"));
    let oh = trace.add_node("OH stat", 40.0, TraceOperation::Add);
    trace.add_edge(oh_base, oh);
    trace.add_edge(oh_glob, oh);
    trace.end_pass();

    let mode = CombineMode::Crit { double_hits: true };
    let weights = mode.linearized_weights(&[30.0, 40.0]).unwrap();
    let combined = trace.add_combine_node(
        "CritChance",
        final_value,
        mode,
        &[(mh, weights[0]), (oh, weights[1])],
    );

    let global = src("global");
    let request = AttributionRequest::new(DisplayStatId::from("CritChance"))
        .with_mode(AttributionMode::DirectAndMarginal)
        .with_sources([global.clone()]);

    // recompute 闭包重跑「整管线」（这里 = 重算两腿与合并），权重不冻结。
    let report = attribute(
        &request,
        final_value,
        Some((&trace, combined)),
        |excluded| {
            let g = if excluded.contains(&global) {
                0.0
            } else {
                10.0
            };
            combine(20.0 + g, 30.0 + g)
        },
    );

    let entry = &report.entries[0];
    assert!((entry.value - 13.0).abs() < 1e-12, "direct 手算 = 13");
    assert!(
        (entry.marginal_delta.unwrap() - 14.0).abs() < 1e-12,
        "marginal 手算 = 14"
    );
    assert!(
        (entry.marginal_delta.unwrap() - entry.value).abs() > 1e-9,
        "非线性样例上 marginal 必须 ≠ direct"
    );
}

// §5.4 pass_filter：per-pass direct 查询 + C4 口径（marginal 置 None）。

#[test]
fn pass_filter_restricts_direct_to_matching_pass_inputs() {
    let mut trace = TraceGraph::new();
    // 同一 SourceId 的 Input 在两个 pass 内各落一次（RFC §2.4 条款 3）。
    trace.begin_pass(MH_PASS);
    let mh_in = trace.add_source_node("ring (MH pass)", 12.0, src("ring"));
    let mh = trace.add_node("MH dps", 12.0, TraceOperation::Multiply);
    trace.add_edge(mh_in, mh);
    trace.end_pass();
    trace.begin_pass(OH_PASS);
    let oh_in = trace.add_source_node("ring (OH pass)", 8.0, src("ring"));
    let oh = trace.add_node("OH dps", 8.0, TraceOperation::Multiply);
    trace.add_edge(oh_in, oh);
    trace.end_pass();
    let combined =
        trace.add_combine_node("TotalDPS", 20.0, CombineMode::Add, &[(mh, 1.0), (oh, 1.0)]);

    let base = AttributionRequest::new(DisplayStatId::from("TotalDPS"))
        .with_mode(AttributionMode::Direct)
        .with_sources([src("ring")]);

    let all = pobr_core::attribution::AttributionReport::direct(&base, 20.0, &trace, combined);
    assert_eq!(all.entries[0].value, 20.0, "无 filter：两 pass 都累计");

    let mh_only = pobr_core::attribution::AttributionReport::direct(
        &base.clone().with_pass_filter(MH_PASS),
        20.0,
        &trace,
        combined,
    );
    assert_eq!(mh_only.entries[0].value, 12.0, "MH filter 只计 MH Input");

    let oh_only = pobr_core::attribution::AttributionReport::direct(
        &base.clone().with_pass_filter(OH_PASS),
        20.0,
        &trace,
        combined,
    );
    assert_eq!(oh_only.entries[0].value, 8.0, "OH filter 只计 OH Input");
}

/// C4：pass_filter 非 None 时 marginal / interaction 置 None（拒绝混口径），
/// recompute 闭包不被调用。
#[test]
fn c4_pass_filter_disables_marginal_and_interaction() {
    let mut trace = TraceGraph::new();
    trace.begin_pass(MH_PASS);
    let input = trace.add_source_node("ring (MH)", 12.0, src("ring"));
    let mh = trace.add_node("MH dps", 12.0, TraceOperation::Multiply);
    trace.add_edge(input, mh);
    trace.end_pass();
    let combined = trace.add_combine_node("TotalDPS", 12.0, CombineMode::Or, &[(mh, 1.0)]);

    let request = AttributionRequest::new(DisplayStatId::from("TotalDPS"))
        .with_mode(AttributionMode::MarginalWithInteraction)
        .with_sources([src("ring")])
        .with_pass_filter(MH_PASS);

    let report = attribute(&request, 12.0, Some((&trace, combined)), |_| {
        panic!("C4：pass_filter 请求不得触发 recompute（全局口径动作）")
    });
    assert_eq!(report.entries[0].marginal_delta, None);
    assert_eq!(report.entries[0].marginal_percent, None);
    assert!(report.interaction.is_none());
}

// I1 / C3：pass 分区结构不变式 + begin_pass 作用域栈行为。

/// begin_pass/end_pass 作用域栈：盖戳、嵌套覆盖、栈空回 None（既有调用点零改动保持
/// `pass: None`——RFC §2.6）。
#[test]
fn pass_scope_stack_stamps_and_nests() {
    let mut trace = TraceGraph::new();
    let before = trace.add_node("outside", 1.0, TraceOperation::Add);
    assert_eq!(trace.node(before).unwrap().pass, None);

    trace.begin_pass(MH_PASS);
    let hand_level = trace.add_node("hand level", 1.0, TraceOperation::Add);
    assert_eq!(trace.node(hand_level).unwrap().pass, Some(MH_PASS));

    // 嵌套：hand 作用域内进入 crit 维度，栈顶生效。
    let crit_pass = PassId::new(HandTag::MainHand, CritTag::Crit);
    trace.begin_pass(crit_pass);
    let crit_level = trace.add_source_node("crit input", 2.0, src("amulet"));
    assert_eq!(trace.node(crit_level).unwrap().pass, Some(crit_pass));
    trace.end_pass();

    let back = trace.add_node("back at hand level", 1.0, TraceOperation::Add);
    assert_eq!(trace.node(back).unwrap().pass, Some(MH_PASS));
    trace.end_pass();

    let after = trace.add_node("outside again", 1.0, TraceOperation::Add);
    assert_eq!(trace.node(after).unwrap().pass, None);
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "end_pass")]
fn end_pass_on_empty_stack_panics_in_debug() {
    let mut trace = TraceGraph::new();
    trace.end_pass();
}

/// I1（C3 强化版）：合并节点各入腿的带 pass 戳祖先集合两两不相交；
/// 违例图被 `combine_partition_violations` 检出。
#[test]
fn i1_combine_partition_violations_detects_shared_stamped_node() {
    // 合法图：两腿各自独立带戳子图，跨腿共享只有 pass==None 的结构常量。
    let mut trace = TraceGraph::new();
    let shared_const = trace.add_node("structural constant", 1.0, TraceOperation::Input);
    trace.begin_pass(MH_PASS);
    let mh = trace.add_node("MH stat", 10.0, TraceOperation::Add);
    trace.end_pass();
    trace.begin_pass(OH_PASS);
    let oh = trace.add_node("OH stat", 20.0, TraceOperation::Add);
    trace.end_pass();
    trace.add_edge(shared_const, mh);
    trace.add_edge(shared_const, oh);
    let ok_combine = trace.add_combine_node("ok", 30.0, CombineMode::Add, &[(mh, 1.0), (oh, 1.0)]);
    assert!(
        trace.combine_partition_violations(ok_combine).is_empty(),
        "pass==None 共享祖先合法（按腿各计一次再加权）"
    );

    // 违例图：带戳 Input 被两腿共享（构图 bug 形态）。
    let mut bad = TraceGraph::new();
    bad.begin_pass(MH_PASS);
    let stamped_shared = bad.add_source_node("stamped shared", 5.0, src("ring"));
    let mh2 = bad.add_node("MH stat", 10.0, TraceOperation::Add);
    bad.end_pass();
    bad.begin_pass(OH_PASS);
    let oh2 = bad.add_node("OH stat", 20.0, TraceOperation::Add);
    bad.end_pass();
    bad.add_edge(stamped_shared, mh2);
    bad.add_edge(stamped_shared, oh2);
    // 注意不能走 add_combine_node + direct（debug 断言会 panic）；直接查诊断 API。
    let bad_combine = bad.add_node(
        "bad",
        30.0,
        TraceOperation::Combine {
            mode: CombineMode::Add,
            weights: vec![1.0, 1.0],
        },
    );
    bad.add_edge(mh2, bad_combine);
    bad.add_edge(oh2, bad_combine);
    assert_eq!(
        bad.combine_partition_violations(bad_combine),
        vec![stamped_shared],
        "带戳节点跨腿共享必须被检出（I1 违例）"
    );
}

// 嵌套 Combine（2×2 同构）：内层 CritBlend per 手、外层 hand-combine，
// direct 沿两层权重正确摊销。

#[test]
fn nested_crit_blend_inside_hand_combine_amortizes_through_both_layers() {
    let mut trace = TraceGraph::new();
    let c = 0.2; // MH 暴击率（冻结系数）

    // MH 手：NonCrit 腿 100（来自 weapon1）、Crit 腿 300（同来源，crit 腿被词条放大）。
    trace.begin_pass(PassId::new(HandTag::MainHand, CritTag::NonCrit));
    let mh_hit = trace.add_source_node("mh hit avg", 100.0, src("weapon1"));
    trace.end_pass();
    trace.begin_pass(PassId::new(HandTag::MainHand, CritTag::Crit));
    let mh_crit = trace.add_source_node("mh crit avg", 300.0, src("weapon1"));
    trace.end_pass();
    // CritBlend 节点属该手子图（pass = MH·Blended，RFC §2.3）。
    trace.begin_pass(MH_PASS);
    let mh_blend_value = 100.0 * (1.0 - c) + 300.0 * c; // 140
    let mh_blend = trace.add_combine_node(
        "MH AverageHit",
        mh_blend_value,
        CombineMode::CritBlend,
        &[(mh_hit, 1.0 - c), (mh_crit, c)], // NonCrit 先、Crit 后（§3.1 约定）
    );
    trace.end_pass();

    // OH 手：无暴击双 pass（Blended 单值 60）。
    trace.begin_pass(OH_PASS);
    let oh_avg = trace.add_source_node("oh hit avg", 60.0, src("weapon2"));
    trace.end_pass();

    // 外层 DPS 合并（pass = None）。
    let legs = [mh_blend_value, 60.0];
    let mode = CombineMode::Dps { double_hits: false };
    let combined_value = mode.combine(&legs).unwrap(); // (140+60)/2 = 100
    let weights = mode.linearized_weights(&legs).unwrap(); // [0.5, 0.5]
    let combined = trace.add_combine_node(
        "AverageDamage",
        combined_value,
        mode,
        &[(mh_blend, weights[0]), (oh_avg, weights[1])],
    );
    assert_eq!(trace.node(combined).unwrap().pass, None);

    let request = AttributionRequest::new(DisplayStatId::from("AverageDamage"))
        .with_mode(AttributionMode::Direct)
        .with_sources([src("weapon1"), src("weapon2")]);
    let report = pobr_core::attribution::AttributionReport::direct(
        &request,
        combined_value,
        &trace,
        combined,
    );

    // weapon1 经两层：0.5 × (0.8×100 + 0.2×300) = 0.5 × 140 = 70。
    assert!((report.entries[0].value - 70.0).abs() < 1e-12);
    // weapon2：0.5 × 60 = 30。
    assert!((report.entries[1].value - 30.0).abs() < 1e-12);
    // 全线性层 → 守恒。
    assert!((report.entries[0].value + report.entries[1].value - combined_value).abs() < 1e-12);

    // node_for_pass 形态留（offence 侧）；此处验证 pass 戳可按子图过滤。
    let mh_scoped = pobr_core::attribution::AttributionReport::direct(
        &request.clone().with_pass_filter(MH_PASS),
        combined_value,
        &trace,
        combined,
    );
    // MH·Blended filter 只命中带该戳的 Input——本图 MH 的 Input 在 Crit/NonCrit 子 pass，
    // Blended 层无 Input，故为 0（per-pass 查询按精确 PassId 分桶）。
    assert_eq!(mh_scoped.entries[0].value, 0.0);
    let mh_crit_scoped = pobr_core::attribution::AttributionReport::direct(
        &request
            .clone()
            .with_pass_filter(PassId::new(HandTag::MainHand, CritTag::Crit)),
        combined_value,
        &trace,
        combined,
    );
    // crit 腿 Input 300，经 CritBlend 权重 c=0.2、外层 0.5 → 30。
    assert!((mh_crit_scoped.entries[0].value - 30.0).abs() < 1e-12);
}
