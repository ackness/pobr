//! gap B：per-build 天赋树版本对账（capture `treeVersion` + 失配诊断）。
//!
//! 版本无关（B 组）：用活动数据。既验真实 build 能捕获 `<Spec treeVersion>` 且其已分配
//! 节点都在已加载树中，又验**合成的越界节点被显性检出**——calc 现状对未知 id 静默跳过
//! （`pobr_tree` node.rs filter_map 丢弃），`diagnose_tree_version` 把该失配症状显性化。

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

/// `<Spec treeVersion>` 不再被丢弃——解析后挂在 `build.tree_version`。
#[test]
fn captures_tree_version_from_real_build() {
    let build = parse_build_from_code(&real_code()).expect("parse build");
    assert_eq!(
        build.tree_version.as_deref(),
        Some("0_5"),
        "应从 <Spec treeVersion> 捕获树版本（gap B：不再丢弃）"
    );
}

/// 真实 build（treeVersion 0_5）的绝大多数已分配节点都解析到已加载树（证明加载的是对的
/// 树族，非整体失配），少数未解析节点被诊断**显性检出**而非静默吞掉。
///
/// 注：活动 golden 版本 4.5.0.3.4 的 passive_tree 实测漏掉本 build 分配的若干节点
/// （如 skill 35387/17044/6554 = Cold Damage 类，存在于 4.5.2.1.3 却缺于 4.5.0.3.4）——
/// 正是 gap B 要显性化的"calc 静默丢节点"。故此处断言"多数解析 + 诊断自洽"，
/// 不钉精确 clean（那会把真实数据缺口当测试失败）。
#[test]
fn real_build_mostly_resolves_with_diagnosed_gaps() {
    let data = load_data();
    let build = parse_build_from_code(&real_code()).expect("parse build");
    let allocated = build.tree.allocated_nodes.len();
    let report = diagnose_tree_version(&build, &data);

    assert_eq!(report.build_tree_version.as_deref(), Some("0_5"));
    // 诊断自洽：未知节点确实是已分配集的子集。
    let alloc_set: std::collections::HashSet<u32> =
        build.tree.allocated_nodes.iter().map(|n| n.0).collect();
    assert!(report.unknown_nodes.iter().all(|id| alloc_set.contains(id)));
    // 加载的是对的树族：至少 90% 已分配节点解析到已加载树（整体失配会远低于此）。
    let known = allocated - report.unknown_nodes.len();
    assert!(
        known * 10 >= allocated * 9,
        "仅 {known}/{allocated} 节点解析——疑加载了错误树族；未知={:?}",
        report.unknown_nodes
    );
}

/// 越界节点（树版本失配的实际症状）被检出而非静默跳过；build 的 treeVersion 透传到报告。
#[test]
fn detects_unknown_allocated_node() {
    let data = load_data();
    let bogus = 4_000_000_001; // 不存在于任何已加载树的节点 id
    let build = Build::new()
        .with_tree(PassiveTreeSpec {
            allocated_nodes: vec![NodeId(bogus)],
            ..Default::default()
        })
        .with_tree_version(Some("9_9".to_string()));
    let report = diagnose_tree_version(&build, &data);
    assert!(!report.is_clean(), "越界节点应被检出而非静默跳过");
    assert_eq!(report.unknown_nodes, vec![bogus]);
    assert_eq!(report.build_tree_version.as_deref(), Some("9_9"));
}

/// 多版本树适配（PoB2 `TreeData/<v>` 对位）：历史赛季树（vendor 抽取，
/// `base/passive_trees/<v>.json`）已入库且按 `<Spec treeVersion>` 正确选择。
/// 验收锚点 = 53853『Backup Plan』：0_3 是 50/50 两条条件词条，当前默认
/// （0_5）是 20/40/40 三条——同一节点 id、不同赛季数值形态。
#[test]
fn versioned_tree_selects_historic_stats() {
    let data = load_data();

    // 0_1..0_4 已抽取入库。
    for v in ["0_1", "0_2", "0_3", "0_4"] {
        assert!(
            data.versioned_passive_nodes.contains_key(v),
            "历史树 {v} 应已入库（base/passive_trees/{v}.json）"
        );
    }

    // 0_3 树：Backup Plan = 50/50 两条（历史形态）。
    let n3 = data
        .passive_nodes_for(Some("0_3"))
        .get(&53853)
        .expect("0_3 树含 53853");
    assert_eq!(
        n3.stats.len(),
        2,
        "0_3 Backup Plan 两条词条: {:?}",
        n3.stats
    );
    assert!(
        n3.stats[0].starts_with("50% increased Evasion Rating"),
        "0_3 形态应为 50%：{:?}",
        n3.stats
    );

    // 当前默认版本（0_5）不在 versioned 表 → 落回默认树（20/40/40 三条）；
    // 未标注（None）同。
    for tv in [Some("0_5"), None] {
        let n5 = data
            .passive_nodes_for(tv)
            .get(&53853)
            .expect("默认树含 53853");
        assert_eq!(
            n5.stats.len(),
            3,
            "默认树 Backup Plan 三条词条（tv={tv:?}）: {:?}",
            n5.stats
        );
    }

    // 未知版本号（乱写/未抽取）安全落默认。
    assert_eq!(
        data.passive_nodes_for(Some("9_9")).len(),
        data.passive_nodes.len(),
        "未知版本落回默认树"
    );
}
