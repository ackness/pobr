//! 通用 PoB2 parity harness：遍历 `examples/demo-bd-test/builds/*/`，用每个 build 的
//! `meta.json::player_stats`（PoB2/Lua 导出的黄金数值）作为参照，对比 PoBR 计算输出。
//!
//! 设计目标：
//! - **零硬编码、零按技能特化**——同一套比较逻辑作用于全部职业/升华/技能。
//! - **基线度量**：默认不硬失败，逐 build 打印「PoBR vs PoB2」对照 + 聚合命中率，
//!   作为对齐进度的活体仪表盘（`cargo test -p pobr-build --test ninja_parity -- --nocapture`）。
//! - **回归门禁**：`parity_no_regression` 断言聚合命中率不低于已记录基线（防止改动倒退）。
//!
//! 防御/属性按 PoB2 PlayerStat 口径比较；DPS 类（与技能管线完整度强相关）单列报告，
//! 不计入防御命中率，避免未完成的 offence 管线掩盖防御侧 parity 信号。
//!
//! **默认口径 = `mode_effective=true`（M3-W5 切换）**：PoB2 主面板（即 golden 导出）
//! 在非 CALCS 模式下恒为 EFFECTIVE（vendor `CalcSetup.lua:583-588`），与 golden 对齐。
//! 面板口径（`mode_effective=false`）保留 [`panel_mode_no_regression`] 守卫，防口径
//! 回归无感知。切换依据与逐 build 归因：
//! `audits/rearchitecture-2026-06-10/blueprints/m3-effective-switch-report.md`。

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

/// 以指定口径计算一个 build（`mode_effective`：false=面板口径，true=PoB2 主面板
/// EFFECTIVE 口径，vendor `CalcSetup.lua:583-588`——非 CALCS 模式恒 EFFECTIVE）。
fn run_build_mode(dir: &Path, data: &BuildData, mode_effective: bool) -> Option<OutputTable> {
    let code = std::fs::read_to_string(dir.join("code.txt")).ok()?;
    let build = parse_build_from_code(code.trim()).ok()?;
    let opts = DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        enemy_level: 0,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective,
        extra_modifier_texts: vec![],
        ..Default::default()
    };
    calculate_with_data(&build, data, &opts).ok()
}

/// 默认口径：effective（与 PoB2 golden 同口径，M3-W5 切换）。
fn run_build(dir: &Path, data: &BuildData) -> Option<OutputTable> {
    run_build_mode(dir, data, true)
}

/// 比较列：(显示名, PoB2 key, PoBR 取值)。
struct Row {
    label: &'static str,
    golden: Option<f64>,
    pobr: f64,
}

/// golden 经 sanitize 后 `Infinity` → `1e308`；≥ 此阈值按 ∞ 等价处理（比值口径）。
const GOLDEN_INF: f64 = 1e307;

/// ∞ 等价判定（pobr 的 `f64::INFINITY` 与 golden 的 sanitize 占位都算）。
fn is_inf_like(v: f64) -> bool {
    !v.is_finite() || v >= GOLDEN_INF
}

fn ratio(pobr: f64, golden: f64) -> f64 {
    if is_inf_like(golden) {
        // 双方皆 ∞ → 命中（1.0）；golden ∞ 而 pobr 有限 → 0（脱靶）。
        if is_inf_like(pobr) { 1.0 } else { 0.0 }
    } else if golden == 0.0 {
        if pobr == 0.0 { 1.0 } else { f64::INFINITY }
    } else {
        pobr / golden
    }
}

/// 防御/属性面板**核心列**（W1 全程的旧 8 列基线口径；扩列稀释防护的子集指标，
/// 蓝图 §4.2-1 / 00-index §4 owner 双指标裁决）。
fn defensive_core_rows(
    out: &OutputTable,
    g: &serde_json::Map<String, serde_json::Value>,
) -> Vec<Row> {
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

/// 防御扩展列（M2 F-3 扩列 8→25：EHP/max-hit 新口径 + Block/Spirit/Evade/
/// Deflect/池口径面板，蓝图 §2 Track F「defensive_rows 扩列」清单）。
fn defensive_extended_rows(
    out: &OutputTable,
    g: &serde_json::Map<String, serde_json::Value>,
) -> Vec<Row> {
    vec![
        Row {
            label: "TotalEHP",
            golden: golden(g, "TotalEHP"),
            pobr: out.total_ehp,
        },
        Row {
            label: "PhysMaxHit",
            golden: golden(g, "PhysicalMaximumHitTaken"),
            pobr: out.physical_max_hit,
        },
        Row {
            label: "FireMaxHit",
            golden: golden(g, "FireMaximumHitTaken"),
            pobr: out.fire_max_hit,
        },
        Row {
            label: "ColdMaxHit",
            golden: golden(g, "ColdMaximumHitTaken"),
            pobr: out.cold_max_hit,
        },
        Row {
            label: "LightMaxHit",
            golden: golden(g, "LightningMaximumHitTaken"),
            pobr: out.lightning_max_hit,
        },
        Row {
            label: "ChaosMaxHit",
            golden: golden(g, "ChaosMaximumHitTaken"),
            pobr: out.chaos_max_hit,
        },
        Row {
            label: "EffBlock",
            golden: golden(g, "EffectiveBlockChance"),
            pobr: out.effective_block_chance,
        },
        Row {
            label: "EffSpellBlock",
            golden: golden(g, "EffectiveSpellBlockChance"),
            pobr: out.effective_spell_block_chance,
        },
        Row {
            label: "Spirit",
            golden: golden(g, "Spirit"),
            pobr: out.spirit,
        },
        Row {
            label: "SpiritUnres",
            golden: golden(g, "SpiritUnreserved"),
            pobr: out.spirit_unreserved,
        },
        Row {
            label: "EvadeChance",
            golden: golden(g, "EvadeChance"),
            pobr: out.evade_chance,
        },
        Row {
            label: "MeleeEvade",
            golden: golden(g, "MeleeEvadeChance"),
            pobr: out.melee_evade_chance,
        },
        Row {
            label: "LifeUnres",
            golden: golden(g, "LifeUnreserved"),
            pobr: out.life_unreserved,
        },
        Row {
            label: "ManaUnres",
            golden: golden(g, "ManaUnreserved"),
            pobr: out.mana_unreserved,
        },
        Row {
            label: "ESRecoveryCap",
            golden: golden(g, "EnergyShieldRecoveryCap"),
            pobr: out.energy_shield_recovery_cap,
        },
        Row {
            label: "PhysDR",
            golden: golden(g, "PhysicalDamageReduction"),
            pobr: out.physical_damage_reduction,
        },
        Row {
            label: "DeflectChance",
            golden: golden(g, "DeflectChance"),
            pobr: out.deflect_chance,
        },
    ]
}

/// 全量防御列 = 核心 8 列 + 扩展 17 列（≈ 蓝图「8→~24 列」）。
fn defensive_rows(out: &OutputTable, g: &serde_json::Map<String, serde_json::Value>) -> Vec<Row> {
    let mut rows = defensive_core_rows(out, g);
    rows.extend(defensive_extended_rows(out, g));
    rows
}

/// 进攻列（技能管线完整度强相关，单列报告）。
fn offensive_rows(out: &OutputTable, g: &serde_json::Map<String, serde_json::Value>) -> Vec<Row> {
    // M4-m（k2 登记）：vendor 恒等式实为 `TotalDPS = AverageDamage × Speed ×
    // dpsMultiplier × quantityMultiplier`（CalcOffence.lua:4407）——golden 的
    // `AverageDamage` 不含端因子，而 PoBR `dps`（与 golden `TotalDPS` 同口径）含。
    // 旧读数 `dps / action_rate` 对 grenade 等 build 带结构性偏置（deadeye ×1.5、
    // gemling ×1.65、twister ×1.02 实测）。用 golden 自身恒等式反解端因子，把
    // PoBR 读数折回 AverageDamage 同口径（harness 取数口径修正，零 calc 行为）。
    let golden_end_factor = match (
        golden(g, "TotalDPS"),
        golden(g, "AverageDamage"),
        golden(g, "Speed"),
    ) {
        (Some(td), Some(ad), Some(sp)) if td.is_finite() && (ad * sp).abs() > f64::EPSILON => {
            let f = td / (ad * sp);
            if f.is_finite() && f > 0.0 { f } else { 1.0 }
        }
        _ => 1.0,
    };
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
            // PoB2 恒等式 `TotalDPS = AverageDamage × Speed × 端因子`（golden 的平均
            // 伤害已含命中率/暴击/敌方减伤，不含端因子）。PoBR 侧用同一恒等式取
            // `dps / action_rate / 端因子`；旧值 `total_hit_avg`（玩家侧未减伤、不含
            // 命中率）在 effective 口径下与 golden 结构性错配（切换报告 §3-R4）。
            pobr: if out.action_rate > 0.0 {
                out.dps / out.action_rate / golden_end_factor
            } else {
                0.0
            },
        },
        Row {
            label: "TotalDPS",
            golden: golden(g, "TotalDPS"),
            pobr: out.dps,
        },
    ]
}

/// DoT 合并族列（M4-G 扩列：技能 DoT + 异常 DoT 的末端合并面板，PoB2
/// `TotalDotDPS`/`WithDotDPS`/`CombinedDPS`，CalcOffence.lua:6093-6234）。
/// 与技能管线完整度强相关，独立于既有进攻 5 列单独计数（新列单独基线常量，
/// 不稀释/不挪动 BASELINE_OFF_*）。golden 键已在 meta.json（WithDotDPS 仅
/// 纯 DoT build 导出，如 essence-drain）。
fn dot_rows(out: &OutputTable, g: &serde_json::Map<String, serde_json::Value>) -> Vec<Row> {
    vec![
        Row {
            label: "TotalDotDPS",
            golden: golden(g, "TotalDotDPS"),
            pobr: out.total_dot_dps,
        },
        Row {
            label: "WithDotDPS",
            golden: golden(g, "WithDotDPS"),
            pobr: out.with_dot_dps,
        },
        Row {
            label: "CombinedDPS",
            golden: golden(g, "CombinedDPS"),
            pobr: out.combined_dps,
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
    let fmt = |v: f64| -> String {
        if is_inf_like(v) {
            "inf".into()
        } else {
            format!("{v:.2}")
        }
    };
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
                    "  {mark} {:<16}{:>14}{:>14}{:>9.2}x",
                    r.label,
                    fmt(r.pobr),
                    fmt(gv),
                    rt
                );
            }
            None => eprintln!("    {:<16}{:>14.2}{:>14}{:>10}", r.label, r.pobr, "—", "—"),
        }
    }
    tally_rows(rows)
}

/// 遍历全部 build 计算防御/进攻/DoT 命中聚合。`verbose` 控制是否逐 build 打印
/// 对照表，`mode_effective` 控制计算口径（默认门禁走 effective，面板守卫走 false）。
/// 返回 `(防御核心 8 列 Tally, 防御全量 25 列 Tally, 进攻 Tally, DoT 三列 Tally,
/// 解析/计算失败的 build 名)`。
fn compute_tallies_mode(
    verbose: bool,
    mode_effective: bool,
) -> (Tally, Tally, Tally, Tally, Vec<String>) {
    let data = load_data();
    let builds = discover_builds();
    assert!(!builds.is_empty(), "no builds discovered");

    let mut def_core = Tally::default();
    let mut def = Tally::default();
    let mut off = Tally::default();
    let mut dot = Tally::default();
    let mut failed_parse = Vec::new();

    for dir in &builds {
        let name = dir.file_name().unwrap().to_string_lossy();
        let g = golden_stats(dir);
        let Some(out) = run_build_mode(dir, &data, mode_effective) else {
            failed_parse.push(name.to_string());
            if verbose {
                eprintln!("\n##### {name} :: PARSE/CALC FAILED #####");
            }
            continue;
        };
        let (def_rows, off_rows, dot_rows) = (
            defensive_rows(&out, &g),
            offensive_rows(&out, &g),
            dot_rows(&out, &g),
        );
        def_core.add(tally_rows(&defensive_core_rows(&out, &g)));
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
            eprintln!("  -- dot --");
            dot.add(print_rows(&dot_rows));
        } else {
            def.add(tally_rows(&def_rows));
            off.add(tally_rows(&off_rows));
            dot.add(tally_rows(&dot_rows));
        }
    }
    (def_core, def, off, dot, failed_parse)
}

/// 默认口径（effective）聚合，主门禁/报告入口。
fn compute_tallies(verbose: bool) -> (Tally, Tally, Tally, Tally, Vec<String>) {
    compute_tallies_mode(verbose, true)
}

/// 已记录的 parity 基线（命中数）——回归门禁的下限。**仅在确认改动整体提升 parity 时上调**，
/// 永不下调（防止改动悄悄倒退）。对应 commit 当时的 ninja_parity 输出。
///
/// M2 F-3 扩列重记（蓝图 §2 Track F commit 3 / §4.2 / 00-index §4 owner 双指标裁决）：
/// - `DEF_CORE`：旧 8 列子集（扩列稀释防护指标，裁决下限 111——W1 全程冻结基线）；
/// - `DEF`：扩列后全量 25 列（分母 = golden 可比项总数）。
///
/// **已审查例外**（M1-T2.4 statmap 切换，独立 baseline commit 显式登记）：
/// OFF_HIT5 23→22——deadeye-explosive-grenade 的 TotalDPS 由 Legacy「过算抵消
/// 欠算」假命中（1.02x）回归真实 0.77x（Multishot −25% less `sup_dex.lua:3154-3156`
/// 与 LightningPen +30 `SkillStatMap.lua:929-931`，均为修对）。补偿清单与逐 build
/// 依据见 `audits/rearchitecture-2026-06-10/blueprints/m1-statmap-switch-log.md` §3。
///
/// M1+M2 合并重记（merge commit）：在 M1（statmap 切换 + quality + support
/// 裁决）与 M2（扣池 + EHP 口径 + 25 列扩列 + 补刀 1-3）合并后的代码上实测重记。
/// 防御 369→374（83.1%，两分支改进叠加）/ @10% 385→390；核心 130（=M2，90.3%）/
/// @10% 132→133；进攻 27（=M2）/ @10% 32→33。与两分支基线对比见 merge commit message。
///
/// **M3-W5 effective 口径切换重记**（独立 baseline commit，显式审查；逐 build 归因
/// 见 `m3-effective-switch-report.md` §2-§5）：默认口径 panel→effective（与 golden
/// 对齐），防御 425 行逐值不变；进攻 @5% 27→26、@10% 33→35。
/// **已审查例外（−1 @5%）**：smith-of-kitava CritChance 1.00x→0.93x——golden
/// `HitChance`=100（PoB2 玩家精准足额过 cap）而 PoBR 精准聚合低估（≈1015 vs 1438，
/// 装备/天赋精准词条与武器局部精准未入聚合，登记 M4），effective 下暴击二次命中检定
/// （vendor CalcOffence.lua:3700）放大该缺口。面板口径水平由
/// [`panel_mode_no_regression`]（PANEL_OFF_*）继续守住 27/35。
const BASELINE_DEF_CORE_HIT5: usize = 132; // 实测 132/144 = 91.7%（M4-k2 油涂池连带）
const BASELINE_DEF_HIT5: usize = 379; // 实测 379/450 = 84.2%（M4-k 波合并重记）
const BASELINE_DEF_HIT10: usize = 392; // 实测 392/450 = 87.1%
const BASELINE_OFF_HIT5: usize = 52; // 实测 52/80 = 65.0%（M4-m：击中量级线 + 曝光/Buff 收尾 + 杂项五项）
const BASELINE_OFF_HIT10: usize = 62; // 实测 62/80 = 77.5%

/// DoT 三列（TotalDotDPS/WithDotDPS/CombinedDPS）独立基线（M4-G 扩列时实测；
/// 新列单独常量，不动既有 BASELINE_OFF_*）。命中 3 = wolf-pack 双 0 命中
/// （TotalDotDPS/CombinedDPS golden=0）+ essence-drain TotalDotDPS 1.0000；
/// 分母 37 = 18 build × (TotalDotDPS + CombinedDPS) + essence-drain WithDotDPS。
///
/// **M4-J 已审查例外 ×2（两处伪命中被修复揭示，叠加 −1 @5% / −2 @10%）**：
/// 1. druid-oracle-comet TotalDotDPS 1.08x→1.17x——旧 1.08x 是伪命中：
///    isSwitchable 树变体缺失（druid 6898 误用基础版、缺 `Gain 5% of Damage as
///    Extra Damage of a random Element`）恰好部分抵消既有 ignite 高估。变体修复
///    后（解析/展开与 vendor CalcOffence.lua:1175-1200 同口径：三元素均分 n/3）
///    真实偏差 ~1.17x 暴露。**M4-K 后续**：1.17x 高估根因（ailment 堆叠速率源
///    2.54x 过记）已修复 → 0.45x；剩余低估逐因子归属暴击量级/curse duration/
///    副技能 debuff 线（m4-skill-gaps.md §7.1，逐因子乘积闭合）。
/// 2. deadeye grenade CombinedDPS 1.02x——偶然命中：旧「攻速补偿吞吐」近似把
///    Speed 高估 ×1.95，与 GrenadeActivateTwice ×1.5 形成吞吐双重计入，恰好
///    抵消 per-hit 低估（0.52x）。M4-J 按 vendor CalcOffence.lua:2852-2856 切
///    冷却管辖速率（Speed 1.00x ✓ 入 off 列）后双计消失。
///    **M4-K 进展**：grenade 宝石等级 gating 已解除（等级链 oracle 双证
///    deadeye 27=vendor、gemling 24=vendor）+ 油涂 Paragon 品质接通，
///    CombinedDPS 0.52x → 0.82x 收敛但未回带。剩余 per-hit ~0.82x 缺口
///    登记 m4-skill-gaps §7（含「attack/spell area damage」暂缓短语——其
///    启用前提『冷却线修复后』已满足，归 parser 线）。
const BASELINE_DOT_HIT5: usize = 9; // 实测 9/37 = 24.3%（m1 击中量级连带 DoT 收敛）
const BASELINE_DOT_HIT10: usize = 12; // 实测 12/37 = 32.4%

/// 面板口径（`mode_effective=false`）守卫基线：防止口径回归无感知（effective 与
/// panel 在防御侧逐值相同，故只守进攻）。M3-W5 切换 commit 实测。
///
/// **M4-K 已审查例外（−1 @10%）**：witch-abyssal-lich-detonate-dead panel
/// TotalDPS 1.09x→1.12x——`CritInPast8Sec` 短语族接入（vendor
/// ModParser.lua:1904-1906，oracle 证实 vendor 计入）使 panel 口径既有
/// 9% 过记（panel 无敌方减伤）越过 10% 带沿。effective 主口径同一改动为
/// 纯收敛（TotalDPS 0.81x→0.83x，主基线 41/47 不倒退；dot 列 twister
/// 0.96x 新入列 3→4）。panel 侧待 effective 减伤线把 DD 过记根因收敛后
/// 回升再上记。同 commit panel @5% 35→36（titan 入列），下限保守不上调。
const PANEL_OFF_HIT5: usize = 35; // 实测 36/80 = 45.0%（M4-K，基线保守持平）
const PANEL_OFF_HIT10: usize = 40; // 实测 40/80 = 50.0%（M4-K 例外下修）

/// 回归门禁：聚合命中数不得低于已记录基线（[`BASELINE_*`]）。CI gate，防止改动倒退 parity。
#[test]
fn parity_no_regression() {
    let (def_core, def, off, dot, failed) = compute_tallies(false);
    assert!(failed.is_empty(), "builds failed to parse/calc: {failed:?}");
    // owner 双指标裁决之一：旧 8 列子集 ≥ 111（防「扩列稀释」掩盖回退）。
    assert!(
        def_core.hit5 >= BASELINE_DEF_CORE_HIT5,
        "defensive core-8 @5% regressed: {} < baseline {BASELINE_DEF_CORE_HIT5}",
        def_core.hit5
    );
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
    assert!(
        dot.hit5 >= BASELINE_DOT_HIT5,
        "dot @5% regressed: {} < baseline {BASELINE_DOT_HIT5}",
        dot.hit5
    );
    assert!(
        dot.hit10 >= BASELINE_DOT_HIT10,
        "dot @10% regressed: {} < baseline {BASELINE_DOT_HIT10}",
        dot.hit10
    );
}

/// 面板口径守卫：`mode_effective=false` 的进攻聚合不得低于切换时实测水平
/// （[`PANEL_OFF_HIT5`]/[`PANEL_OFF_HIT10`]）。防御侧与 effective 逐值相同，
/// 由主门禁覆盖。防止口径开关上游接线被改动而无感知回归。
#[test]
fn panel_mode_no_regression() {
    // DoT 三列只在 effective 主门禁守（面板口径无独立 golden 口径，不另设守卫）。
    let (_, _, off, _dot, failed) = compute_tallies_mode(false, false);
    assert!(failed.is_empty(), "builds failed to parse/calc: {failed:?}");
    assert!(
        off.hit5 >= PANEL_OFF_HIT5,
        "panel offensive @5% regressed: {} < baseline {PANEL_OFF_HIT5}",
        off.hit5
    );
    assert!(
        off.hit10 >= PANEL_OFF_HIT10,
        "panel offensive @10% regressed: {} < baseline {PANEL_OFF_HIT10}",
        off.hit10
    );
}

/// 主基线报告：逐 build 打印防御 + 进攻对照，并汇总聚合命中率。
#[test]
fn parity_baseline_report() {
    let (def_core, def, off, dot, failed_parse) = compute_tallies(true);
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
        "defensive parity (25 cols): {}/{} = {:.1}% @5%  |  {}/{} = {:.1}% @10%",
        def.hit5,
        def.total,
        pct(def.hit5, def.total),
        def.hit10,
        def.total,
        pct(def.hit10, def.total),
    );
    eprintln!(
        "defensive core-8 subset:    {}/{} = {:.1}% @5%  |  {}/{} = {:.1}% @10%",
        def_core.hit5,
        def_core.total,
        pct(def_core.hit5, def_core.total),
        def_core.hit10,
        def_core.total,
        pct(def_core.hit10, def_core.total),
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
    eprintln!(
        "dot parity (3 cols): {}/{} = {:.1}% @5%  |  {}/{} = {:.1}% @10%",
        dot.hit5,
        dot.total,
        pct(dot.hit5, dot.total),
        dot.hit10,
        dot.total,
        pct(dot.hit10, dot.total),
    );
}

/// M3-W5 口径切换双跑报告：同一 build 以 `mode_effective=false`（面板口径）与
/// `mode_effective=true`（PoB2 主面板 EFFECTIVE 口径，vendor `CalcSetup.lua:583-588`）
/// 各算一遍，逐 stat 三列输出（panel / effective / PoB2 golden）+ 收敛/恶化标记。
///
/// 打印型仪表盘（不设门禁）：
/// `cargo test -p pobr-build --test ninja_parity -- effective_switch_dual_run_report --nocapture`
///
/// 报告归档：`audits/rearchitecture-2026-06-10/blueprints/m3-effective-switch-report.md`。
#[test]
fn effective_switch_dual_run_report() {
    let data = load_data();
    let builds = discover_builds();
    assert!(!builds.is_empty(), "no builds discovered");

    let fmt_v = |v: f64| -> String {
        if is_inf_like(v) {
            "inf".into()
        } else {
            format!("{v:.2}")
        }
    };
    // 命中带宽标记：✓ = @5%，~ = @10%，空 = 脱靶。
    let band = |rt: f64| -> &'static str {
        if (rt - 1.0).abs() < TOL {
            "✓"
        } else if (rt - 1.0).abs() < TOL10 {
            "~"
        } else {
            " "
        }
    };

    let mut panel_tally = (Tally::default(), Tally::default(), Tally::default()); // (core, def, off)
    let mut eff_tally = (Tally::default(), Tally::default(), Tally::default());
    // 迁移统计（@5% 口径）：(收敛 panel✗→eff✓, 恶化 panel✓→eff✗, 双✓, 双✗)。
    let mut moved: Vec<String> = Vec::new();

    for dir in &builds {
        let name = dir.file_name().unwrap().to_string_lossy();
        let g = golden_stats(dir);
        let (Some(panel), Some(eff)) = (
            run_build_mode(dir, &data, false),
            run_build_mode(dir, &data, true),
        ) else {
            eprintln!("\n##### {name} :: PARSE/CALC FAILED #####");
            continue;
        };

        panel_tally
            .0
            .add(tally_rows(&defensive_core_rows(&panel, &g)));
        panel_tally.1.add(tally_rows(&defensive_rows(&panel, &g)));
        panel_tally.2.add(tally_rows(&offensive_rows(&panel, &g)));
        eff_tally.0.add(tally_rows(&defensive_core_rows(&eff, &g)));
        eff_tally.1.add(tally_rows(&defensive_rows(&eff, &g)));
        eff_tally.2.add(tally_rows(&offensive_rows(&eff, &g)));

        eprintln!("\n##### {name} #####");
        eprintln!(
            "  {:<18}{:>14}{:>14}{:>14}{:>9}{:>9}",
            "stat", "panel", "effective", "PoB2", "p-ratio", "e-ratio"
        );
        let p_rows = defensive_rows(&panel, &g)
            .into_iter()
            .chain(offensive_rows(&panel, &g));
        let e_rows = defensive_rows(&eff, &g)
            .into_iter()
            .chain(offensive_rows(&eff, &g));
        for (p, e) in p_rows.zip(e_rows) {
            let Some(gv) = p.golden else {
                continue;
            };
            let (rp, re) = (ratio(p.pobr, gv), ratio(e.pobr, gv));
            let (bp, be) = (band(rp), band(re));
            let trans = match (bp == "✓", be == "✓") {
                (false, true) => " ↑5%",
                (true, false) => " ↓LOST",
                _ if (rp - re).abs() > 1e-9 => " Δ",
                _ => "",
            };
            if bp != be || (rp - re).abs() > 1e-9 {
                moved.push(format!(
                    "{name} :: {:<16} panel {:.3}x → eff {:.3}x{trans}",
                    p.label, rp, re
                ));
            }
            eprintln!(
                "  {bp}{be} {:<16}{:>14}{:>14}{:>14}{:>8.2}x{:>8.2}x{trans}",
                p.label,
                fmt_v(p.pobr),
                fmt_v(e.pobr),
                fmt_v(gv),
                rp,
                re
            );
        }
    }

    let pct = |n: usize, d: usize| 100.0 * n as f64 / d.max(1) as f64;
    eprintln!("\n================ EFFECTIVE-SWITCH DUAL-RUN SUMMARY ================");
    for (label, p, e) in [
        ("def core-8", panel_tally.0, eff_tally.0),
        ("def 25-col", panel_tally.1, eff_tally.1),
        ("offensive ", panel_tally.2, eff_tally.2),
    ] {
        eprintln!(
            "{label}: panel {}/{} = {:.1}% @5% ({:.1}% @10%)  →  effective {}/{} = {:.1}% @5% ({:.1}% @10%)",
            p.hit5,
            p.total,
            pct(p.hit5, p.total),
            pct(p.hit10, p.total),
            e.hit5,
            e.total,
            pct(e.hit5, e.total),
            pct(e.hit10, e.total),
        );
    }
    eprintln!("\n-- 口径间逐值变化（panel ≠ effective 或命中带迁移） --");
    for m in &moved {
        eprintln!("  {m}");
    }
}

/// M2 F-2/F-3：EHP 新旧口径 18-build 双跑对照报告（蓝图 m2-defence §2 Track F）。
///
/// F-3 口径切换后：canonical `total_ehp`/`*_max_hit` 即新口径，「old」列取
/// `total_ehp_lowest_max_hit`（旧管线仍双跑产出，revert 通道保留）；per-type
/// max hit 旧值不再单列（新旧在中性输入下数学等价，见 F-2 报告 §3.1）。
/// 打印型仪表盘（不设门禁）：
/// `cargo test -p pobr-build --test ninja_parity -- ehp_dual_run_report --nocapture`
#[test]
fn ehp_dual_run_report() {
    let data = load_data();
    let builds = discover_builds();
    assert!(!builds.is_empty(), "no builds discovered");

    // golden 的 Infinity 经 sanitize 变为 1e308——展示与比值口径按 ∞ 处理。
    let fmt_v = |v: f64| -> String {
        if !v.is_finite() || v >= 1e307 {
            "inf".into()
        } else {
            format!("{v:.0}")
        }
    };
    let fmt_ratio = |pobr: f64, golden: Option<f64>| -> String {
        match golden {
            Some(g) if g >= 1e307 || !g.is_finite() => {
                if !pobr.is_finite() {
                    "✓inf".into()
                } else {
                    "fin/inf".into()
                }
            }
            Some(g) if g != 0.0 => format!("{:.2}x", pobr / g),
            Some(_) => "g=0".into(),
            None => "—".into(),
        }
    };

    eprintln!("\n========== M2 F-2 EHP 双跑对照（旧 lowest-max-hit vs 新 PoB2 口径） ==========");
    for dir in &builds {
        let name = dir.file_name().unwrap().to_string_lossy();
        let g = golden_stats(dir);
        let Some(out) = run_build(dir, &data) else {
            eprintln!("\n##### {name} :: PARSE/CALC FAILED #####");
            continue;
        };
        eprintln!("\n##### {name} #####");
        let g_ehp = golden(&g, "TotalEHP");
        eprintln!(
            "  TotalEHP        old {:>12}  new {:>12}  golden {:>12}  old {}  new {}",
            fmt_v(out.total_ehp_lowest_max_hit),
            fmt_v(out.total_ehp_pob2),
            g_ehp.map_or("—".into(), fmt_v),
            fmt_ratio(out.total_ehp_lowest_max_hit, g_ehp),
            fmt_ratio(out.total_ehp_pob2, g_ehp),
        );
        eprintln!(
            "  hitsToDie {:>8}  mitigatedHits {:>8}  enemyDamageIn {:>8}",
            fmt_v(out.number_of_damaging_hits),
            fmt_v(out.number_of_mitigated_hits),
            fmt_v(out.total_enemy_damage_in),
        );
        for (label, key, now_v) in [
            (
                "PhysMaxHit",
                "PhysicalMaximumHitTaken",
                out.physical_max_hit,
            ),
            ("FireMaxHit", "FireMaximumHitTaken", out.fire_max_hit),
            ("ColdMaxHit", "ColdMaximumHitTaken", out.cold_max_hit),
            (
                "LightMaxHit",
                "LightningMaximumHitTaken",
                out.lightning_max_hit,
            ),
            ("ChaosMaxHit", "ChaosMaximumHitTaken", out.chaos_max_hit),
        ] {
            let gv = golden(&g, key);
            eprintln!(
                "  {label:<14}  now {:>12}  golden {:>12}  {}",
                fmt_v(now_v),
                gv.map_or("—".into(), fmt_v),
                fmt_ratio(now_v, gv),
            );
        }
    }
    eprintln!(
        "\n（F-3 已切换：total_ehp/*_max_hit = PoB2 口径；旧 lowest-max-hit 口径保留在 total_ehp_lowest_max_hit）"
    );
}

/// M2 阶段验收专项 fixture（蓝图 §4.2-3「MoM/CI/taken-as/盾 block 四类」，@5% 对 golden）。
///
/// 四类覆盖方式：
/// - **MoM**：sorceress-stormweaver-comet（`DamageTakenFromManaBeforeLife` 来源；
///   mana 池折入 TotalHitPool）——TotalEHP / PhysicalMaximumHitTaken @5%；
/// - **CI**：monk-invoker-frost-bomb（CI keystone）——TotalEHP @5% +
///   ChaosMaximumHitTaken 双 ∞（混沌免疫）；
/// - **盾 block**：warrior-titan / warrior-smith-of-kitava——EffectiveBlockChance /
///   EffectiveSpellBlockChance @5%（block 概率层；TotalEHP 残差 0.24-0.48x 为
///   已知缺口：护甲聚合上游 + 格挡回复 GainWhenHit（vendor :3168-3177）未实现，
///   见 F-3 commit message 残差清单，不在本断言内）；
/// - **taken-as**：18-build golden 无该词条载体，由 pobr-core 合成 fixture 覆盖
///   （`tests/taken_as.rs` Lightning Coil 型 + `tests/ehp_pob2.rs` 端到端，
///   期望值手算自 CalcDefence.lua:356-455 公式）。
#[test]
fn m2_f3_specialty_fixtures() {
    let data = load_data();
    let dir = builds_dir();
    let run = |name: &str| -> (OutputTable, serde_json::Map<String, serde_json::Value>) {
        let d = dir.join(name);
        let g = golden_stats(&d);
        let out = run_build(&d, &data).unwrap_or_else(|| panic!("{name} 计算失败"));
        (out, g)
    };
    let assert_5pct = |build: &str, stat: &str, pobr: f64, golden_v: Option<f64>| {
        let gv = golden_v.unwrap_or_else(|| panic!("{build} golden 缺 {stat}"));
        let rt = ratio(pobr, gv);
        assert!(
            (rt - 1.0).abs() < TOL,
            "{build} {stat}: pobr {pobr:.1} vs golden {gv:.1} = {rt:.3}x（超 5% 容差）"
        );
    };

    // MoM 类。
    let (out, g) = run("sorceress-stormweaver-comet");
    assert_5pct(
        "sorceress-stormweaver-comet",
        "TotalEHP",
        out.total_ehp,
        golden(&g, "TotalEHP"),
    );
    assert_5pct(
        "sorceress-stormweaver-comet",
        "PhysicalMaximumHitTaken",
        out.physical_max_hit,
        golden(&g, "PhysicalMaximumHitTaken"),
    );

    // CI 类。
    let (out, g) = run("monk-invoker-frost-bomb");
    assert_5pct(
        "monk-invoker-frost-bomb",
        "TotalEHP",
        out.total_ehp,
        golden(&g, "TotalEHP"),
    );
    // CI 混沌免疫：双方皆 ∞（golden 经 sanitize 为 1e308 占位）。
    assert!(
        is_inf_like(out.chaos_max_hit)
            && golden(&g, "ChaosMaximumHitTaken").is_some_and(is_inf_like),
        "CI 混沌免疫应双 ∞"
    );

    // 盾 block 类（两个 warrior build 的 block 概率层）。
    for name in [
        "warrior-titan-shield-wall",
        "warrior-smith-of-kitava-shield-wall",
    ] {
        let (out, g) = run(name);
        assert_5pct(
            name,
            "EffectiveBlockChance",
            out.effective_block_chance,
            golden(&g, "EffectiveBlockChance"),
        );
        assert_5pct(
            name,
            "EffectiveSpellBlockChance",
            out.effective_spell_block_chance,
            golden(&g, "EffectiveSpellBlockChance"),
        );
    }
}
