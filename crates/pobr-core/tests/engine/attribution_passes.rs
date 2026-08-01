//! Dual-pass x attribution model integration tests.
//!
//! Covers PassId partitioning, the combine weight table, and direct/marginal
//! compatibility, organized around invariants I1-I6 and conditions C2
//! (I4 assertions split into three groups) / C3 (I1 hard invariant) / C4
//! (pass_filter semantics).

use pobr_core::attribution::{AttributionMode, AttributionRequest, attribute};
use pobr_core::{CombineMode, CritTag, HandTag, PassId, TraceGraph, TraceOperation};
use pobr_data::prelude::*;

fn src(id: &str) -> SourceId {
    SourceId::new(SourceKind::Item, id)
}

const MH_PASS: PassId = PassId::hand_blended(HandTag::MainHand);
const OH_PASS: PassId = PassId::hand_blended(HandTag::OffHand);

// I4 group 1: linear modes (OR / ADD / AVERAGE / DPS / CRIT-non-doubleHits) —
// "Σ weights×leg == combined value" holds exactly (weights are constants, the combine is a
// linear combination of the legs).

/// Linear modes checked one by one: `combine(legs) == Σ linearized_weights×legs` (hand-computed).
#[test]
fn i4_linear_modes_weighted_sum_equals_combined_value() {
    let legs = [30.0, 40.0];
    // (mode, hand-computed combined value); vendor CalcOffence.lua:2453-2545.
    let cases: &[(CombineMode, f64)] = &[
        (CombineMode::Or, 30.0),                          // MH or OH = MH
        (CombineMode::Add, 70.0),                         // MH + OH
        (CombineMode::Average, 35.0),                     // (MH+OH)/2
        (CombineMode::Dps { double_hits: false }, 35.0),  // (MH+OH)/2
        (CombineMode::Dps { double_hits: true }, 70.0),   // MH + OH
        (CombineMode::Crit { double_hits: false }, 35.0), // (MH+OH)/2
    ];
    for (mode, expected) in cases {
        let combined = mode.combine(&legs).expect("self-computing mode");
        assert_eq!(combined, *expected, "{mode:?} combined value hand-computed");
        let weights = mode.linearized_weights(&legs).expect("self-computing mode");
        let weighted_sum: f64 = weights.iter().zip(legs.iter()).map(|(w, l)| w * l).sum();
        assert!(
            (weighted_sum - combined).abs() < 1e-12,
            "{mode:?} linear mode must conserve: Σw×leg={weighted_sum} != {combined}"
        );
    }
}

/// End-to-end conservation on a real graph for a linear mode (DPS-doubleHits, an ADD shape):
/// the sum of per-source direct contributions equals the top-level combined output (holds
/// exactly when each leg is a pure addition chain).
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

    // A per-hand mod only enters its own leg; a global source is counted once per leg then
    // weighted (under ADD weights (1,1), counting it twice = its true direct contribution to
    // "the sum of both hands", RFC §5.1).
    assert_eq!(
        report.entries[0].value, 30.0,
        "weapon1 only enters the MH leg"
    );
    assert_eq!(
        report.entries[1].value, 20.0,
        "weapon2 only enters the OH leg"
    );
    assert_eq!(
        report.entries[2].value, 20.0,
        "global contributes 10×1.0 to each leg"
    );
    let total: f64 = report.entries.iter().map(|entry| entry.value).sum();
    assert_eq!(
        total, combined_value,
        "linear mode: direct sums across all sources conserve"
    );
}

// I4 group 2: HARMONICMEAN — nonlinear, but the harmonic mean is degree-1 homogeneous, so
// Euler's theorem makes "Σ partial-derivative×leg == value" hold **exactly** (a homogeneity
// coincidence; review condition C2 requires this to be called out separately — don't
// generalize it to other nonlinear modes).

#[test]
fn i4_harmonic_mean_weighted_sum_homogeneity_coincidence() {
    let legs = [4.0, 6.0];
    let mode = CombineMode::HarmonicMean;
    let combined = mode.combine(&legs).unwrap();
    assert!((combined - 4.8).abs() < 1e-12, "2/(1/4+1/6) = 4.8");

    let weights = mode.linearized_weights(&legs).unwrap();
    // Analytic partial derivative: ∂/∂MH [2·MH·OH/(MH+OH)] = 2·OH²/(MH+OH)².
    assert!((weights[0] - 2.0 * 36.0 / 100.0).abs() < 1e-12);
    assert!((weights[1] - 2.0 * 16.0 / 100.0).abs() < 1e-12);

    // Homogeneity coincidence (Euler, degree 1): weighted sum == value. Only this mode; don't generalize.
    let weighted_sum: f64 = weights.iter().zip(legs.iter()).map(|(w, l)| w * l).sum();
    assert!((weighted_sum - combined).abs() < 1e-12);
}

/// HARMONICMEAN's zero-leg edge case (vendor :2466-2467): either leg being 0 → output 0,
/// weights (0,0), so direct gives 0 consistent with the output; the real sensitivity is
/// covered by marginal instead.
#[test]
fn i4_harmonic_mean_zero_leg_yields_zero_weights() {
    let legs = [0.0, 6.0];
    let mode = CombineMode::HarmonicMean;
    assert_eq!(mode.combine(&legs), Some(0.0));
    assert_eq!(mode.linearized_weights(&legs), Some(vec![0.0, 0.0]));
}

// I4 group 3: CRIT-doubleHits / CHANCE / CHANCE_AILMENT / CritBlend —
// **only assert that weights == analytic partial derivatives**. direct is **not conserving**
// under these modes (doubleHits' cross term gets double-counted by the partial derivatives:
// Σw×leg = MH+OH-2·MH·OH/100 != the combined value MH+OH-MH·OH/100); conservation is
// instead guaranteed by a full marginal pipeline recompute (RFC §5.2, review condition C2).

#[test]
fn i4_crit_double_hits_weights_are_partial_derivatives_not_conserving() {
    let legs = [30.0, 40.0];
    let mode = CombineMode::Crit { double_hits: true };
    // vendor :2461: MH + OH - MH×OH/100 = 70 - 12 = 58.
    let combined = mode.combine(&legs).unwrap();
    assert!((combined - 58.0).abs() < 1e-12);

    // Analytic partial derivatives: ∂/∂MH = 1 - OH/100 = 0.6; ∂/∂OH = 1 - MH/100 = 0.7.
    let weights = mode.linearized_weights(&legs).unwrap();
    assert!((weights[0] - 0.6).abs() < 1e-12);
    assert!((weights[1] - 0.7).abs() < 1e-12);

    // Explicitly record the non-conservation (the gap = cross term MH·OH/100 = 12, double-counted by the partials):
    let weighted_sum: f64 = weights.iter().zip(legs.iter()).map(|(w, l)| w * l).sum();
    assert!((weighted_sum - 46.0).abs() < 1e-12);
    assert!(
        (combined - weighted_sum - 12.0).abs() < 1e-12,
        "doubleHits direct non-conservation is an inherent property, backstopped by marginal"
    );
}

/// CHANCE / CHANCE_AILMENT / CritBlend are coefficient modes: the combined value and weights
/// are frozen by an exogenous coefficient (RFC §3.3), so `combine`/`linearized_weights` refuse
/// (return None) and the weights are supplied by the graph-building side instead. This checks
/// the shape of "frozen coefficient == analytic partial derivative" (using CritBlend and the
/// Chance portion formula as examples).
#[test]
fn i4_coefficient_modes_frozen_weights_match_partials() {
    for mode in [
        CombineMode::Chance,
        CombineMode::ChanceAilment,
        CombineMode::CritBlend,
    ] {
        assert_eq!(
            mode.combine(&[1.0, 2.0]),
            None,
            "{mode:?} is a coefficient mode"
        );
        assert_eq!(mode.linearized_weights(&[1.0, 2.0]), None);
    }

    // CritBlend (vendor :4395): blend = hit×(1-c) + crit×c;
    // dblend/dhit = 1-c, dblend/dcrit = c — the frozen weights [1-c, c] are exactly the partials.
    let c: f64 = 0.3;
    let (hit, crit) = (100.0, 250.0);
    let blend = hit * (1.0 - c) + crit * c;
    assert!((blend - 145.0).abs() < 1e-12);
    // (Explaining c's own source goes through c's node inputs / marginal; weights are frozen constants — RFC §3.3.)

    // CHANCE (vendor :2471-2480): portion = chance×HitChance share;
    // output = MH×mainPortion + OH×offPortion, d/dMH = mainPortion (portion is frozen).
    let (mh_chance, oh_chance): (f64, f64) = (25.0 * 0.9, 15.0 * 0.8);
    let main_portion = mh_chance / (mh_chance + oh_chance);
    let off_portion = oh_chance / (mh_chance + oh_chance);
    assert!((main_portion + off_portion - 1.0).abs() < 1e-12);
    assert!(main_portion > off_portion);
}

// I6: on a nonlinear (doubleHits) sample, marginal != direct, and both match hand computation.

#[test]
fn i6_marginal_differs_from_direct_on_double_hits_cross_term() {
    // Scenario: MH = 20 (base) + 10 (global), OH = 30 (base) + 10 (global);
    // combined = MH + OH - MH·OH/100.
    // final = 30 + 40 - 12 = 58.
    // direct(global) = w_mh×10 + w_oh×10 = (1-0.40)×10 + (1-0.30)×10 = 13.
    // marginal(global): drop global → MH=20, OH=30 → 20+30-6 = 44; delta = 58-44 = 14.
    // 13 != 14 (the gap = second-order cross term effect 10×10/100 = 1), matching hand computation.
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

    // The recompute closure reruns "the whole pipeline" (here: recomputing both legs and the
    // combine); weights are not frozen.
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
    assert!(
        (entry.value - 13.0).abs() < 1e-12,
        "direct hand-computed = 13"
    );
    assert!(
        (entry.marginal_delta.unwrap() - 14.0).abs() < 1e-12,
        "marginal hand-computed = 14"
    );
    assert!(
        (entry.marginal_delta.unwrap() - entry.value).abs() > 1e-9,
        "on a nonlinear sample, marginal must differ from direct"
    );
}

// §5.4 pass_filter: per-pass direct queries + C4 semantics (marginal set to None).

#[test]
fn pass_filter_restricts_direct_to_matching_pass_inputs() {
    let mut trace = TraceGraph::new();
    // The same SourceId's Input lands once in each of the two passes (RFC §2.4 clause 3).
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
    assert_eq!(
        all.entries[0].value, 20.0,
        "no filter: both passes accumulate"
    );

    let mh_only = pobr_core::attribution::AttributionReport::direct(
        &base.clone().with_pass_filter(MH_PASS),
        20.0,
        &trace,
        combined,
    );
    assert_eq!(
        mh_only.entries[0].value, 12.0,
        "MH filter counts only the MH Input"
    );

    let oh_only = pobr_core::attribution::AttributionReport::direct(
        &base.clone().with_pass_filter(OH_PASS),
        20.0,
        &trace,
        combined,
    );
    assert_eq!(
        oh_only.entries[0].value, 8.0,
        "OH filter counts only the OH Input"
    );
}

/// C4: when pass_filter is not None, marginal / interaction are set to None (refusing to mix
/// semantics), and the recompute closure is never called.
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
        panic!("C4: a pass_filter request must not trigger recompute (a global-scope action)")
    });
    assert_eq!(report.entries[0].marginal_delta, None);
    assert_eq!(report.entries[0].marginal_percent, None);
    assert!(report.interaction.is_none());
}

// I1 / C3: pass-partition structural invariant + begin_pass scope-stack behaviour.

/// begin_pass/end_pass scope stack: stamping, nested overrides, empty stack returns None
/// (existing call sites keep `pass: None` with zero changes — RFC §2.6).
#[test]
fn pass_scope_stack_stamps_and_nests() {
    let mut trace = TraceGraph::new();
    let before = trace.add_node("outside", 1.0, TraceOperation::Add);
    assert_eq!(trace.node(before).unwrap().pass, None);

    trace.begin_pass(MH_PASS);
    let hand_level = trace.add_node("hand level", 1.0, TraceOperation::Add);
    assert_eq!(trace.node(hand_level).unwrap().pass, Some(MH_PASS));

    // Nesting: entering the crit dimension inside the hand scope, the stack top applies.
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

/// I1 (a stronger version of C3): a combine node's incoming legs must have pairwise-disjoint
/// pass-stamped ancestor sets; a violating graph is caught by `combine_partition_violations`.
#[test]
fn i1_combine_partition_violations_detects_shared_stamped_node() {
    // Valid graph: each leg has its own independently stamped subgraph; the only thing
    // shared across legs is a structural constant with pass==None.
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
        "pass==None shared ancestor is legal (counted once per leg then weighted)"
    );

    // Violating graph: a stamped Input is shared by both legs (a graph-construction bug shape).
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
    // Can't go through add_combine_node + direct here (the debug assertion would panic);
    // query the diagnostic API directly instead.
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
        "a stamped node shared across legs must be detected (I1 violation)"
    );
}

// Nested Combine (a 2x2 shape): inner CritBlend per hand, outer hand-combine —
// direct correctly amortizes through both layers' weights.

#[test]
fn nested_crit_blend_inside_hand_combine_amortizes_through_both_layers() {
    let mut trace = TraceGraph::new();
    let c = 0.2; // MH crit chance (frozen coefficient)

    // MH hand: NonCrit leg is 100 (from weapon1), Crit leg is 300 (same source, amplified by a mod on the crit leg).
    trace.begin_pass(PassId::new(HandTag::MainHand, CritTag::NonCrit));
    let mh_hit = trace.add_source_node("mh hit avg", 100.0, src("weapon1"));
    trace.end_pass();
    trace.begin_pass(PassId::new(HandTag::MainHand, CritTag::Crit));
    let mh_crit = trace.add_source_node("mh crit avg", 300.0, src("weapon1"));
    trace.end_pass();
    // The CritBlend node belongs to this hand's subgraph (pass = MH·Blended, RFC §2.3).
    trace.begin_pass(MH_PASS);
    let mh_blend_value = 100.0 * (1.0 - c) + 300.0 * c; // 140
    let mh_blend = trace.add_combine_node(
        "MH AverageHit",
        mh_blend_value,
        CombineMode::CritBlend,
        &[(mh_hit, 1.0 - c), (mh_crit, c)], // NonCrit first, then Crit (§3.1 convention)
    );
    trace.end_pass();

    // OH hand: no crit split (single Blended value of 60).
    trace.begin_pass(OH_PASS);
    let oh_avg = trace.add_source_node("oh hit avg", 60.0, src("weapon2"));
    trace.end_pass();

    // Outer DPS combine (pass = None).
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

    // weapon1 through both layers: 0.5 × (0.8×100 + 0.2×300) = 0.5 × 140 = 70.
    assert!((report.entries[0].value - 70.0).abs() < 1e-12);
    // weapon2: 0.5 × 60 = 30.
    assert!((report.entries[1].value - 30.0).abs() < 1e-12);
    // Every layer is linear → conserving.
    assert!((report.entries[0].value + report.entries[1].value - combined_value).abs() < 1e-12);

    // node_for_pass shape reserved for the offence side; here we just verify pass stamps can filter by subgraph.
    let mh_scoped = pobr_core::attribution::AttributionReport::direct(
        &request.clone().with_pass_filter(MH_PASS),
        combined_value,
        &trace,
        combined,
    );
    // The MH·Blended filter only matches Inputs carrying that exact stamp. In this graph
    // MH's Inputs live in the Crit/NonCrit sub-passes; the Blended layer has no Input of its
    // own, so this is 0 (per-pass queries bucket by exact PassId).
    assert_eq!(mh_scoped.entries[0].value, 0.0);
    let mh_crit_scoped = pobr_core::attribution::AttributionReport::direct(
        &request
            .clone()
            .with_pass_filter(PassId::new(HandTag::MainHand, CritTag::Crit)),
        combined_value,
        &trace,
        combined,
    );
    // Crit leg Input 300, through CritBlend weight c=0.2 and the outer 0.5 → 30.
    assert!((mh_crit_scoped.entries[0].value - 30.0).abs() < 1e-12);
}
