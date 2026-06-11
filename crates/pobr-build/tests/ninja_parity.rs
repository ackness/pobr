//! 通用 PoB2 parity harness：遍历 `examples/demo-bd-test/builds/*/`，用每个 build 的
//! `meta.json::player_stats`（PoB2/Lua 导出的黄金数值）作为参照，对比 PoBR 计算输出。
//!
//! 设计目标：
//! - **零硬编码、零按技能特化**——同一套比较逻辑作用于全部职业/升华/技能。
//! - **基线度量**：默认不硬失败，逐 build 打印「PoBR vs PoB2」对照 + 聚合命中率，
//!   作为对齐进度的活体仪表盘（`cargo test -p pobr-build --test ninja_parity -- --nocapture`）。
//! - **回归门禁**：`parity_no_regression` 断言聚合命中率不低于已记录基线（防止改动倒退）。
//!
//! 防御/属性按 PoB2 PlayerStat 面板口径比较；DPS 类（与技能管线完整度强相关）单列报告，
//! 不计入防御命中率，避免未完成的 offence 管线掩盖防御侧 parity 信号。

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build_from_code};
use pobr_core::calc::{MinimalInput, OutputTable};
use pobr_data::monster::EnemyTier;
use pobr_gamedata::{GameData, repo_data_root};
use std::path::{Path, PathBuf};

fn builds_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/demo-bd-test/builds")
        .canonicalize()
        .expect("builds dir exists")
}

fn discover_builds() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(builds_dir())
        .expect("read builds dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir() && p.join("code.txt").exists() && p.join("meta.json").exists())
        .collect();
    dirs.sort();
    dirs
}

fn load_data() -> BuildData {
    let data = GameData::new(repo_data_root().join("4.5.0.3.4"));
    BuildData::load(&data).expect("load BuildData")
}

/// 读取 meta.json::player_stats（PoB2 黄金值）。
fn golden_stats(dir: &Path) -> serde_json::Map<String, serde_json::Value> {
    let text = std::fs::read_to_string(dir.join("meta.json")).expect("read meta.json");
    // PoB2 导出含 `Infinity`/`NaN` 字面量（非法 JSON）——替换为可解析占位后再 parse。
    let sanitized = text
        .replace("-Infinity", "-1e308")
        .replace("Infinity", "1e308")
        .replace("NaN", "0");
    let json: serde_json::Value = serde_json::from_str(&sanitized).expect("parse meta.json");
    json.get("player_stats")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default()
}

fn golden(stats: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<f64> {
    stats.get(key).and_then(|v| v.as_f64())
}

fn run_build(dir: &Path, data: &BuildData) -> Option<OutputTable> {
    let code = std::fs::read_to_string(dir.join("code.txt")).ok()?;
    let build = parse_build_from_code(code.trim()).ok()?;
    let opts = DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        enemy_level: 0,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: false,
        extra_modifier_texts: vec![],
        ..Default::default()
    };
    calculate_with_data(&build, data, &opts).ok()
}

/// 比较列：(显示名, PoB2 key, PoBR 取值)。
struct Row {
    label: &'static str,
    golden: Option<f64>,
    pobr: f64,
}

fn ratio(pobr: f64, golden: f64) -> f64 {
    if golden == 0.0 {
        if pobr == 0.0 { 1.0 } else { f64::INFINITY }
    } else {
        pobr / golden
    }
}

/// 防御/属性面板列（与技能管线无关，反映 base/树/装备聚合 parity）。
fn defensive_rows(out: &OutputTable, g: &serde_json::Map<String, serde_json::Value>) -> Vec<Row> {
    vec![
        Row {
            label: "Life",
            golden: golden(g, "Life"),
            pobr: out.life,
        },
        Row {
            label: "Mana",
            golden: golden(g, "Mana"),
            pobr: out.mana,
        },
        Row {
            label: "EnergyShield",
            golden: golden(g, "EnergyShield"),
            pobr: out.energy_shield,
        },
        Row {
            label: "Armour",
            golden: golden(g, "Armour"),
            pobr: out.armour,
        },
        Row {
            label: "Evasion",
            golden: golden(g, "Evasion"),
            pobr: out.evasion,
        },
        Row {
            label: "FireResist",
            golden: golden(g, "FireResist"),
            pobr: out.fire_resistance,
        },
        Row {
            label: "ColdResist",
            golden: golden(g, "ColdResist"),
            pobr: out.cold_resistance,
        },
        Row {
            label: "LightningResist",
            golden: golden(g, "LightningResist"),
            pobr: out.lightning_resistance,
        },
    ]
}

/// 进攻列（技能管线完整度强相关，单列报告）。
fn offensive_rows(out: &OutputTable, g: &serde_json::Map<String, serde_json::Value>) -> Vec<Row> {
    vec![
        Row {
            label: "CritChance",
            golden: golden(g, "CritChance"),
            pobr: out.crit_chance * 100.0,
        },
        Row {
            label: "CritMultiplier",
            golden: golden(g, "CritMultiplier"),
            pobr: out.crit_multiplier,
        },
        Row {
            label: "Speed",
            golden: golden(g, "Speed"),
            pobr: out.action_rate,
        },
        Row {
            label: "AverageDamage",
            golden: golden(g, "AverageDamage"),
            pobr: out.total_hit_avg,
        },
        Row {
            label: "TotalDPS",
            golden: golden(g, "TotalDPS"),
            pobr: out.dps,
        },
    ]
}

const TOL: f64 = 0.05; // 命中 = 相对误差 < 5%

/// 一组比较列的命中统计：5% 命中数、10% 接近数、总比较数。
#[derive(Default, Clone, Copy)]
struct Tally {
    hit5: usize,
    hit10: usize,
    total: usize,
}

impl Tally {
    fn add(&mut self, other: Tally) {
        self.hit5 += other.hit5;
        self.hit10 += other.hit10;
        self.total += other.total;
    }
}

const TOL10: f64 = 0.10; // 接近 = 相对误差 < 10%（进度可见性辅助指标）

/// 仅计数（不打印）：供回归门禁 [`parity_no_regression`] 用。
fn tally_rows(rows: &[Row]) -> Tally {
    let mut t = Tally::default();
    for r in rows {
        if let Some(gv) = r.golden {
            let rt = ratio(r.pobr, gv);
            t.total += 1;
            if (rt - 1.0).abs() < TOL {
                t.hit5 += 1;
            }
            if (rt - 1.0).abs() < TOL10 {
                t.hit10 += 1;
            }
        }
    }
    t
}

/// 打印逐 stat 对照表并返回命中聚合（复用 [`tally_rows`] 的计数口径）。
fn print_rows(rows: &[Row]) -> Tally {
    for r in rows {
        match r.golden {
            Some(gv) => {
                let rt = ratio(r.pobr, gv);
                let mark = if (rt - 1.0).abs() < TOL {
                    "✓"
                } else if (rt - 1.0).abs() < TOL10 {
                    "~"
                } else {
                    " "
                };
                eprintln!(
                    "  {mark} {:<16}{:>14.2}{:>14.2}{:>9.2}x",
                    r.label, r.pobr, gv, rt
                );
            }
            None => eprintln!("    {:<16}{:>14.2}{:>14}{:>10}", r.label, r.pobr, "—", "—"),
        }
    }
    tally_rows(rows)
}

/// 遍历全部 build 计算防御/进攻命中聚合。`verbose` 控制是否逐 build 打印对照表。
/// 返回 `(防御 Tally, 进攻 Tally, 解析/计算失败的 build 名)`。
fn compute_tallies(verbose: bool) -> (Tally, Tally, Vec<String>) {
    let data = load_data();
    let builds = discover_builds();
    assert!(!builds.is_empty(), "no builds discovered");

    let mut def = Tally::default();
    let mut off = Tally::default();
    let mut failed_parse = Vec::new();

    for dir in &builds {
        let name = dir.file_name().unwrap().to_string_lossy();
        let g = golden_stats(dir);
        let Some(out) = run_build(dir, &data) else {
            failed_parse.push(name.to_string());
            if verbose {
                eprintln!("\n##### {name} :: PARSE/CALC FAILED #####");
            }
            continue;
        };
        let (def_rows, off_rows) = (defensive_rows(&out, &g), offensive_rows(&out, &g));
        if verbose {
            eprintln!("\n##### {name} #####");
            eprintln!(
                "  {:<18}{:>14}{:>14}{:>10}",
                "stat", "PoBR", "PoB2", "ratio"
            );
            eprintln!("  -- defensive --");
            def.add(print_rows(&def_rows));
            eprintln!("  -- offensive --");
            off.add(print_rows(&off_rows));
        } else {
            def.add(tally_rows(&def_rows));
            off.add(tally_rows(&off_rows));
        }
    }
    (def, off, failed_parse)
}

/// 已记录的 parity 基线（命中数）——回归门禁的下限。**仅在确认改动整体提升 parity 时上调**，
/// 永不下调（防止改动悄悄倒退）。对应 commit 当时的 ninja_parity 输出。
///
/// **已审查例外**（M1-T2.4 statmap 切换，独立 baseline commit 显式登记）：
/// OFF_HIT5 23→22——deadeye-explosive-grenade 的 TotalDPS 由 Legacy「过算抵消
/// 欠算」假命中（1.02x）回归真实 0.77x（Multishot −25% less `sup_dex.lua:3154-3156`
/// 与 LightningPen +30 `SkillStatMap.lua:929-931`，均为修对）。补偿清单与逐 build
/// 依据见 `audits/rearchitecture-2026-06-10/blueprints/m1-statmap-switch-log.md` §3。
const BASELINE_DEF_HIT5: usize = 114;
const BASELINE_DEF_HIT10: usize = 120;
const BASELINE_OFF_HIT5: usize = 22;
const BASELINE_OFF_HIT10: usize = 32;

/// 回归门禁：聚合命中数不得低于已记录基线（[`BASELINE_*`]）。CI gate，防止改动倒退 parity。
#[test]
fn parity_no_regression() {
    let (def, off, failed) = compute_tallies(false);
    assert!(failed.is_empty(), "builds failed to parse/calc: {failed:?}");
    assert!(
        def.hit5 >= BASELINE_DEF_HIT5,
        "defensive @5% regressed: {} < baseline {BASELINE_DEF_HIT5}",
        def.hit5
    );
    assert!(
        def.hit10 >= BASELINE_DEF_HIT10,
        "defensive @10% regressed: {} < baseline {BASELINE_DEF_HIT10}",
        def.hit10
    );
    assert!(
        off.hit5 >= BASELINE_OFF_HIT5,
        "offensive @5% regressed: {} < baseline {BASELINE_OFF_HIT5}",
        off.hit5
    );
    assert!(
        off.hit10 >= BASELINE_OFF_HIT10,
        "offensive @10% regressed: {} < baseline {BASELINE_OFF_HIT10}",
        off.hit10
    );
}

/// 主基线报告：逐 build 打印防御 + 进攻对照，并汇总聚合命中率。
#[test]
fn parity_baseline_report() {
    let (def, off, failed_parse) = compute_tallies(true);
    let builds = discover_builds();

    eprintln!(
        "\n================ PARITY SUMMARY (tol {:.0}%) ================",
        TOL * 100.0
    );
    eprintln!(
        "builds: {} ({} parse/calc failed)",
        builds.len(),
        failed_parse.len()
    );
    if !failed_parse.is_empty() {
        eprintln!("  failed: {}", failed_parse.join(", "));
    }
    let pct = |n: usize, d: usize| 100.0 * n as f64 / d.max(1) as f64;
    eprintln!(
        "defensive parity: {}/{} = {:.1}% @5%  |  {}/{} = {:.1}% @10%",
        def.hit5,
        def.total,
        pct(def.hit5, def.total),
        def.hit10,
        def.total,
        pct(def.hit10, def.total),
    );
    eprintln!(
        "offensive parity: {}/{} = {:.1}% @5%  |  {}/{} = {:.1}% @10%",
        off.hit5,
        off.total,
        pct(off.hit5, off.total),
        off.hit10,
        off.total,
        pct(off.hit10, off.total),
    );
}
