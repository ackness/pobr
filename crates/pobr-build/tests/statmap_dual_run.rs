//! statmap 双跑对照（M1-T2.3，蓝图 18-G3 / 15-G2 / 风险 R5）：Legacy 后缀启发式
//! vs Data 数据引擎（`overlay/skill_stat_map.json` + `rules/stat_map_engine`）。
//!
//! 两个 `#[ignore]` 测试（手动跑，产报告不设门禁——切换门禁见蓝图 T2.4 四前置条件）：
//!
//! ```bash
//! cargo test -p pobr-build --test statmap_dual_run -- --ignored --nocapture
//! ```
//!
//! - **L1 映射级 diff**：枚举 `granted_effect_stat_sets.json` 全量 distinct stat id
//!   × 两引擎，分类计数（`both_equal / both_diff / legacy_only / data_only /
//!   both_absent`），明细落 `target/statmap-diff/L1.jsonl`；
//! - **L2 端到端 diff**：18 个 ninja build 分别以 Legacy / Data 跑
//!   `calculate_with_data`，逐 OutputTable 标量字段 diff，按 build 分组落
//!   `target/statmap-diff/L2-<build>.md`（roadmap R5 缓解原文）。
//!
//! 本 stage（T2.1–T2.3）只建 diff 基线：Data 通道 stage-1 限定 global 表（per-set
//! 接线随 T5 多 statSet 模型）、tag 第一批、scalar 固定 1.0。穷举意义上的 L1 清零
//! 与默认切换属 Stage 2/3（依赖 T5.3 全量 stat 入库，蓝图 §3.1 串行序）。

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use pobr_build::skill_stat_map::map_skill_stats;
use pobr_build::{
    BuildData, DataOrchestratorOptions, StatMapMode, calculate_with_data, parse_build_from_code,
};
use pobr_core::calc::{MinimalInput, OutputTable};
use pobr_core::rules::stat_map_engine::{self, MappedItem, MappedOutcome, StatMapCatalog};
use pobr_data::monster::EnemyTier;
use pobr_gamedata::{GameData, repo_data_root};

/// 报告输出目录（`target/statmap-diff/`，与蓝图 T2.3 约定一致）。
fn report_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/statmap-diff");
    std::fs::create_dir_all(&dir).expect("create target/statmap-diff");
    dir
}

fn load_game_data() -> GameData {
    GameData::new(repo_data_root().join("4.5.0.3.4"))
}

fn load_stat_map_catalog(data: &GameData) -> StatMapCatalog {
    StatMapCatalog::new(
        data.skill_stat_map()
            .expect("load skill_stat_map.json")
            .expect("overlay/skill_stat_map.json 应已落库（M1-T2.1）"),
    )
}

/// 注入项的可比较形态（名字 / 聚合类型 / 数值）。
type Injection = (String, &'static str, f64);

/// Legacy 通道：与 `calc_orchestrator::legacy_mapped_stat_modifiers` 同一映射函数
/// （`map_skill_stats`）+ 同一 value × scale 口径。
fn legacy_injections(stat: &str, value: f64) -> Vec<Injection> {
    let mut v: Vec<Injection> = map_skill_stats(stat)
        .into_iter()
        .map(|m| {
            (
                m.mod_name.clone(),
                m.mod_type.as_trace_label(),
                value * m.scale,
            )
        })
        .collect();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    v
}

/// Data 通道：stage-1 与 orchestrator 同口径（global 表、忽略 SkillData 项）。
/// 返回（注入集合, 结果标注）。
fn data_injections(
    catalog: &StatMapCatalog,
    stat: &str,
    value: f64,
) -> (Vec<Injection>, &'static str, String) {
    match stat_map_engine::map_stat(catalog, "", None, stat, value) {
        MappedOutcome::Mapped(items) => {
            let mut v: Vec<Injection> = items
                .iter()
                .filter_map(|item| match item {
                    MappedItem::Modifier(m) => Some((
                        m.name.to_string(),
                        m.mod_type.as_trace_label(),
                        m.value.as_number().unwrap_or(0.0),
                    )),
                    MappedItem::SkillData { .. } => None,
                })
                .collect();
            v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            (v, "mapped", String::new())
        }
        MappedOutcome::Unsupported(reason) => {
            (Vec::new(), "unsupported", reason.category().to_string())
        }
        MappedOutcome::Unknown => (Vec::new(), "unknown", String::new()),
    }
}

fn injections_equal(a: &[Injection], b: &[Injection]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| x.0 == y.0 && x.1 == y.1 && (x.2 - y.2).abs() < 1e-9)
}

/// L1 映射级 diff：granted_effect_stat_sets 全量 distinct stat × 两引擎。
///
/// 探针值取 100.0（merge 公式对 value 线性/常量覆盖两形态都可见；div/mult/base
/// 的数值差异在比较里直接体现）。
#[test]
#[ignore = "diff 报告生成（手动跑）：cargo test -p pobr-build --test statmap_dual_run -- --ignored --nocapture"]
fn l1_mapping_level_diff() {
    const PROBE_VALUE: f64 = 100.0;
    let game_data = load_game_data();
    let catalog = load_stat_map_catalog(&game_data);
    let data = BuildData::load(&game_data).expect("load BuildData");

    // distinct stat id：分等级 stats + 常量 stats（当前已入库子集——蓝图 T2.3
    // "先对现有已入库 stat 子集建 diff 基线"；全量入库随 T5.3）。
    let mut stats: BTreeSet<String> = BTreeSet::new();
    for set in data.skill_stat_sets.values() {
        for cs in &set.constant_stats {
            stats.insert(cs.stat.clone());
        }
        for level in &set.levels {
            for ds in &level.stats {
                stats.insert(ds.stat.clone());
            }
        }
    }

    let mut counts: BTreeMap<&'static str, u64> = BTreeMap::new();
    let mut unsupported_counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut out = std::fs::File::create(report_dir().join("L1.jsonl")).expect("create L1.jsonl");
    for stat in &stats {
        let legacy = legacy_injections(stat, PROBE_VALUE);
        let (data_side, data_status, data_note) = data_injections(&catalog, stat, PROBE_VALUE);
        let classification = match (legacy.is_empty(), data_side.is_empty()) {
            (true, true) => "both_absent",
            (false, true) => "legacy_only",
            (true, false) => "data_only",
            (false, false) if injections_equal(&legacy, &data_side) => "both_equal",
            (false, false) => "both_diff",
        };
        *counts.entry(classification).or_default() += 1;
        if data_status == "unsupported" {
            *unsupported_counts.entry(data_note.clone()).or_default() += 1;
        }
        // 明细 JSONL（stat 字典序遍历 → 重跑确定性；serde_json 序列化转义）。
        let line = serde_json::json!({
            "stat": stat,
            "classification": classification,
            "legacy": legacy.iter().map(|(n, t, v)| format!("{n} {t} {v}")).collect::<Vec<_>>(),
            "data": data_side.iter().map(|(n, t, v)| format!("{n} {t} {v}")).collect::<Vec<_>>(),
            "data_status": data_status,
            "data_note": data_note,
        });
        writeln!(out, "{line}").expect("write L1.jsonl");
    }

    println!(
        "== statmap L1 映射级 diff（{} distinct stats）==",
        stats.len()
    );
    for (k, v) in &counts {
        println!("  {k:<12} {v}");
    }
    println!("  -- data 侧 Unsupported 分类 --");
    for (k, v) in &unsupported_counts {
        println!("  unsupported:{k:<24} {v}");
    }
    println!("明细：target/statmap-diff/L1.jsonl");
    // 报告生成可重复性由 stat 集合（BTreeSet）与分类逻辑的确定性保证；
    // 此处只断言「报告非空 + 两引擎至少有交集」防呆。
    assert!(!stats.is_empty(), "stat 集合不应为空");
    assert!(
        counts.get("both_equal").copied().unwrap_or(0) > 0,
        "两引擎应至少有相等映射（翻译表从 legacy 反推）"
    );
}

// ---- L2 端到端 diff ----

fn builds_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/demo-bd-test/builds")
        .canonicalize()
        .expect("builds dir exists")
}

fn opts(mode: StatMapMode, catalog: Option<Arc<StatMapCatalog>>) -> DataOrchestratorOptions {
    DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        enemy_level: 0,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: false,
        extra_modifier_texts: vec![],
        stat_map_mode: mode,
        stat_map_catalog: catalog,
    }
}

/// OutputTable 主要标量字段的命名展开（diff 行集；OutputTable 非 serde 类型，
/// 此处手工枚举与 ninja_parity 报告同族的字段 + 进攻/机制扩展字段）。
fn scalar_fields(out: &OutputTable) -> Vec<(&'static str, f64)> {
    vec![
        ("life", out.life),
        ("mana", out.mana),
        ("armour", out.armour),
        ("evasion", out.evasion),
        ("energy_shield", out.energy_shield),
        ("fire_resistance", out.fire_resistance),
        ("cold_resistance", out.cold_resistance),
        ("lightning_resistance", out.lightning_resistance),
        ("crit_chance", out.crit_chance),
        ("crit_multiplier", out.crit_multiplier),
        ("total_hit_avg", out.total_hit_avg),
        ("hit_chance", out.hit_chance),
        ("action_rate", out.action_rate),
        ("effective_action_rate", out.effective_action_rate),
        ("dps", out.dps),
        ("bleed_dps", out.bleed_dps),
        ("ignite_dps", out.ignite_dps),
        ("poison_dps", out.poison_dps),
        ("aoe_radius", out.aoe_radius),
        ("projectile_count", out.projectile_count),
        ("cooldown", out.cooldown),
        ("mana_cost", out.mana_cost),
        ("life_cost", out.life_cost),
        ("spirit_reserved", out.spirit_reserved),
        ("mana_reserved", out.mana_reserved),
        ("life_reserved", out.life_reserved),
        ("total_ehp", out.total_ehp),
    ]
}

/// L2 端到端 diff：18 个 ninja build 分别以 Legacy / Data 跑，逐字段 diff
/// 按 build 分组落 markdown（roadmap R5 缓解："双跑 + 按 ninja build 分组 diff"）。
#[test]
#[ignore = "diff 报告生成（手动跑）：cargo test -p pobr-build --test statmap_dual_run -- --ignored --nocapture"]
fn l2_per_build_end_to_end_diff() {
    let game_data = load_game_data();
    let catalog = Arc::new(load_stat_map_catalog(&game_data));
    let data = BuildData::load(&game_data).expect("load BuildData");

    let mut dirs: Vec<PathBuf> = std::fs::read_dir(builds_dir())
        .expect("read builds dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir() && p.join("code.txt").exists())
        .collect();
    dirs.sort();
    assert!(!dirs.is_empty(), "demo builds 应存在");

    let mut summary: Vec<(String, usize)> = Vec::new();
    for dir in &dirs {
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        let Ok(code) = std::fs::read_to_string(dir.join("code.txt")) else {
            continue;
        };
        let Ok(build) = parse_build_from_code(code.trim()) else {
            continue;
        };
        let legacy_out = calculate_with_data(&build, &data, &opts(StatMapMode::Legacy, None)).ok();
        let data_out = calculate_with_data(
            &build,
            &data,
            &opts(StatMapMode::Data, Some(catalog.clone())),
        )
        .ok();
        let (Some(legacy_out), Some(data_out)) = (legacy_out, data_out) else {
            summary.push((format!("{name} (计算失败)"), 0));
            continue;
        };

        let mut report = String::new();
        report.push_str(&format!("# statmap L2 diff — {name}\n\n"));
        report.push_str("Legacy（skill_stat_map.rs 启发式）vs Data（overlay + stat_map_engine，stage-1 global 表）。\n\n");
        report.push_str("| 字段 | Legacy | Data | Δ | Δ% |\n|---|---|---|---|---|\n");
        let mut diff_count = 0usize;
        for ((label, lv), (_, dv)) in scalar_fields(&legacy_out)
            .into_iter()
            .zip(scalar_fields(&data_out))
        {
            if (lv - dv).abs() < 1e-9 {
                continue;
            }
            diff_count += 1;
            let pct = if lv != 0.0 {
                format!("{:+.2}%", (dv - lv) / lv * 100.0)
            } else {
                "n/a".to_string()
            };
            report.push_str(&format!(
                "| {label} | {lv:.4} | {dv:.4} | {:+.4} | {pct} |\n",
                dv - lv
            ));
        }
        if diff_count == 0 {
            report.push_str("| (无差异) | — | — | — | — |\n");
        }
        std::fs::write(report_dir().join(format!("L2-{name}.md")), report)
            .expect("write L2 report");
        summary.push((name, diff_count));
    }

    println!("== statmap L2 端到端 diff（Legacy vs Data，按 build 分组）==");
    for (name, n) in &summary {
        println!("  {name:<44} 差异字段 {n}");
    }
    println!("报告：target/statmap-diff/L2-<build>.md");
}

/// Compare 模式纯观测契约：同一 build 以 Legacy 与 Compare 各跑一次，输出
/// **逐字段一致**（Compare 不改变计算结果）；且 Compare 跑后能取出映射级记录。
/// 非 ignore——这是双跑框架本身的回归门禁（不依赖报告人工 review）。
#[test]
fn compare_mode_is_pure_observation() {
    let game_data = load_game_data();
    let Ok(Some(def)) = game_data.skill_stat_map() else {
        return; // 数据包未就位时跳过
    };
    let catalog = Arc::new(StatMapCatalog::new(def));
    let data = BuildData::load(&game_data).expect("load BuildData");
    // 取一个固定样本 build（quality fixture 同款，含 15×q20 宝石、覆盖三个取数点）。
    let code_path = builds_dir().join("sorceress-stormweaver-comet/code.txt");
    let Ok(code) = std::fs::read_to_string(&code_path) else {
        return;
    };
    let build = parse_build_from_code(code.trim()).expect("parse build");

    let legacy_out =
        calculate_with_data(&build, &data, &opts(StatMapMode::Legacy, None)).expect("legacy run");
    // 清空历史记录，确保取到的是本次 Compare 的。
    let _ = pobr_build::take_stat_map_compare_records();
    let compare_out = calculate_with_data(
        &build,
        &data,
        &opts(StatMapMode::Compare, Some(catalog.clone())),
    )
    .expect("compare run");
    for ((label, lv), (_, cv)) in scalar_fields(&legacy_out)
        .into_iter()
        .zip(scalar_fields(&compare_out))
    {
        assert!(
            (lv - cv).abs() < 1e-12,
            "Compare 模式改变了输出字段 {label}：legacy={lv} compare={cv}"
        );
    }
    let records = pobr_build::take_stat_map_compare_records();
    assert!(
        !records.is_empty(),
        "Compare 模式应记录映射级对照（主技能/品质/support 取数点至少其一）"
    );
    // 记录已取出 → 再取应为空（take 语义）。
    assert!(pobr_build::take_stat_map_compare_records().is_empty());
}
