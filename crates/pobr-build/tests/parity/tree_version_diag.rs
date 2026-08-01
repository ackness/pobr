//! Gap B: per-build passive-tree version reconciliation (captures
//! `treeVersion` + mismatch diagnosis).
//!
//! Version-independent (group B): uses the active data. Verifies both that a
//! real build can capture `<Spec treeVersion>` with all its allocated nodes
//! present in the loaded tree, and that a **synthetic out-of-range node gets
//! surfaced**: calc currently silently skips unknown ids (`pobr_tree`
//! node.rs's filter_map drops them), and `diagnose_tree_version` makes that
//! mismatch symptom visible.

use pobr_build::{Build, BuildData, diagnose_tree_version, parse_build_from_code};
use pobr_data::passive_tree::{NodeId, PassiveTreeSpec};
use pobr_gamedata::{GameData, repo_data_root};
use std::path::Path;

fn load_data() -> BuildData {
    let data = GameData::new(repo_data_root().join(pobr_gamedata::data_version()));
    BuildData::load(&data).expect("load BuildData")
}

fn real_code() -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/demo-bd-test/builds/sorceress-stormweaver-comet/code.txt");
    std::fs::read_to_string(p)
        .expect("read code.txt")
        .trim()
        .to_string()
}

/// `<Spec treeVersion>` is no longer discarded — after parsing it's attached to `build.tree_version`.
#[test]
fn captures_tree_version_from_real_build() {
    let build = parse_build_from_code(&real_code()).expect("parse build");
    assert_eq!(
        build.tree_version.as_deref(),
        Some("0_5"),
        "should capture the tree version from <Spec treeVersion> (gap B: no longer discarded)"
    );
}

/// The vast majority of a real build's (treeVersion 0_5) allocated nodes
/// resolve against the loaded tree (proving the correct tree family was
/// loaded, not a wholesale mismatch); the few unresolved nodes get
/// **surfaced by diagnosis** rather than silently swallowed.
///
/// Note: the active golden version 4.5.0.3.4's passive_tree is measurably
/// missing several nodes this build allocates (e.g. skill 35387/17044/6554,
/// Cold Damage nodes present in 4.5.2.1.3 but missing from 4.5.0.3.4) — this
/// is exactly the "calc silently drops nodes" behaviour gap B is meant to
/// surface. So this test asserts "mostly resolves + diagnosis is
/// self-consistent" rather than pinning exact cleanliness (which would treat
/// a real data gap as a test failure).
#[test]
fn real_build_mostly_resolves_with_diagnosed_gaps() {
    let data = load_data();
    let build = parse_build_from_code(&real_code()).expect("parse build");
    let allocated = build.tree.allocated_nodes.len();
    let report = diagnose_tree_version(&build, &data);

    assert_eq!(report.build_tree_version.as_deref(), Some("0_5"));
    // Diagnosis self-consistency: the unknown nodes are indeed a subset of the allocated set.
    let alloc_set: std::collections::HashSet<u32> =
        build.tree.allocated_nodes.iter().map(|n| n.0).collect();
    assert!(report.unknown_nodes.iter().all(|id| alloc_set.contains(id)));
    // The correct tree family was loaded: at least 90% of allocated nodes resolve against the loaded tree (a wholesale mismatch would be far lower).
    let known = allocated - report.unknown_nodes.len();
    assert!(
        known * 10 >= allocated * 9,
        "only {known}/{allocated} nodes resolved -- possibly the wrong tree family was loaded; unknown={:?}",
        report.unknown_nodes
    );
}

/// An out-of-range node (the real symptom of a tree-version mismatch) gets detected instead of silently skipped; the build's treeVersion passes through to the report.
#[test]
fn detects_unknown_allocated_node() {
    let data = load_data();
    let bogus = 4_000_000_001; // a node id that doesn't exist in any loaded tree
    let build = Build::new()
        .with_tree(PassiveTreeSpec {
            allocated_nodes: vec![NodeId(bogus)],
            ..Default::default()
        })
        .with_tree_version(Some("9_9".to_string()));
    let report = diagnose_tree_version(&build, &data);
    assert!(
        !report.is_clean(),
        "an out-of-range node should be detected, not silently skipped"
    );
    assert_eq!(report.unknown_nodes, vec![bogus]);
    assert_eq!(report.build_tree_version.as_deref(), Some("9_9"));
}

/// Multi-version tree support (aligned with PoB2's `TreeData/<v>`): historic
/// league trees (vendor-extracted, `base/passive_trees/<v>.json`) are
/// ingested and correctly selected by `<Spec treeVersion>`.
/// Acceptance anchor = node 53853 "Backup Plan": 0_3 has two 50/50 conditional
/// mods, the current default (0_5) has three 20/40/40 mods — same node id,
/// different per-league numeric form.
#[test]
fn versioned_tree_selects_historic_stats() {
    let data = load_data();

    // 0_1..0_4 have already been extracted and ingested.
    for v in ["0_1", "0_2", "0_3", "0_4"] {
        assert!(
            data.versioned_passive_nodes.contains_key(v),
            "historic tree {v} should already be in the data pack (base/passive_trees/{v}.json)"
        );
    }

    // 0_3 tree: Backup Plan = two 50/50 mods (the historic form).
    let n3 = data
        .passive_nodes_for(Some("0_3"))
        .get(&53853)
        .expect("0_3 tree contains 53853");
    assert_eq!(
        n3.stats.len(),
        2,
        "0_3 Backup Plan should have two mods: {:?}",
        n3.stats
    );
    assert!(
        n3.stats[0].starts_with("50% increased Evasion Rating"),
        "0_3's form should be 50%: {:?}",
        n3.stats
    );

    // The current default version (0_5) isn't in the versioned table -> falls back to the default tree (three 20/40/40 mods);
    // same for unmarked (None).
    for tv in [Some("0_5"), None] {
        let n5 = data
            .passive_nodes_for(tv)
            .get(&53853)
            .expect("the default tree contains 53853");
        assert_eq!(
            n5.stats.len(),
            3,
            "the default tree's Backup Plan should have three mods (tv={tv:?}): {:?}",
            n5.stats
        );
    }

    // An unknown version string (typo/not extracted) safely falls back to default.
    assert_eq!(
        data.passive_nodes_for(Some("9_9")).len(),
        data.passive_nodes.len(),
        "an unknown version falls back to the default tree"
    );
}
