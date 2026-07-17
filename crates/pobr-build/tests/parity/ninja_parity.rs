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

use pobr_build::corpus::{CorpusLine, LineSource};
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
    // golden 钉定其被校验的数据版本（与活动 DATA_VERSION 解耦——后者可前进到更新的
    // 数据而不误红 parity；见 pobr_data::GOLDEN_PARITY_DATA_VERSION）。
    let data = GameData::new(repo_data_root().join(pobr_data::GOLDEN_PARITY_DATA_VERSION));
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
    if std::env::var("POBR_DBG_DEFRES").is_ok() {
        eprintln!(
            "[POBR_BUILD] >>> {} (mode_effective={mode_effective})",
            dir.file_name().unwrap().to_string_lossy()
        );
    }
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
// **Mageblood legacies 重记（Phase 1 #1，+17 @5% core-8 118→135）**：9/18 fixture 全戴
// Mageblood，但 `LegacyOf*` BASE + `MagebloodEquipped` flag 从未展开成护甲/闪避/抗性
// （env_finalize.rs 声明未建）。实现 vendor CalcPerform.lua:66-142 legacies 表 +
// :1502-1528 应用逻辑（stacks × duplicate 放大 globalEffect × floor）+ `legacy of (%w+)`
// handler（动态 mod 名）+ MagesLegacyEffect implicit（已在 special_vendor 批）。titan
// Armour（Basalt INC 219 = floor(1.46×150)）/ ice-shot Evasion（Jade BASE 2000 + Stibnite
// INC 150）等多 build 的护甲/闪避/抗性缺口一次收敛，三 canary（physical_armour_block/
// cold_projectile_evasion_es/evasion_melee）un-ignore。
// **Virtuous Barrier Life 名归一重记（+1 @5% core-8 135→136）**：Gemling 升华 buff 的
// per-Mote Life INC（`gem_barrier_red_grants_maximum_life_+%` → 24% = 2×12 StrengthMote）
// 此前经 stat_map_engine 映射到 vendor 名 `Life`，落进死桶——PoBR 生命池聚合名是
// `MaximumLife`（Armour/Evasion/EnergyShield 因规范名与 vendor 同名无此问题）。归一
// `Life`→`MaximumLife` 后 gemling Life 0.79x→0.96x，连带 TotalEHP/5×MaxHit/LifeUnres 共 8 列翻正。
// **Item ES 重算重记（+2 @5% core-8 136→138）**：per-slot 物品 ES 从「信物品文本
// 展示行」改为 PoB2 口径「基底 DB 重算 (esBase+flat)×(1+localInc/100)×(1+quality/100)」
// （Item.lua:1994-1996；展示行跨数据版本会滞后）——titan ES 41→55、stormweaver ES
// 986→1120，各连带 ESRecoveryCap。见 calc_orchestrator/defence.rs::item_rolled_defence。
// **0.5.4b #4 Communion/LowLife + Voices 重记（+2 @5% core-8 138→140）**：
// huntress-ritualist SpiritUnres −13.00x→1.00x / LifeUnres 12.74x→1.01x——
// Atziri's Communion 的 Spirit→Life 保留转换（LifeReservePercentPerSpirit，
// vendor CalcDefence.lua:248-254）接入后双列翻正；abyssal-lich（同戴 Communion）
// SpiritUnres inf→1.00x 同根。见 buffs.rs spirit_reservation_modifiers 转换分支。
const BASELINE_DEF_CORE_HIT5: usize = 142; // 存量 #7-3/4（charm-guard 摘除 + wolf-pack Life/Armour 收敛 + gemling 池名归一）后 142/144（Communion 140（ItemES 138；Barrier-Life 136；Mageblood 135；迁移基线 118；0.5.0=139）
// **per-socket-filled 修复重记（+1 @5%/@10%）**：gemling-legionnaire 身甲 Morior Invictus
// `+14 to Spirit per Socket filled`（×5 socket）经 `RunesSocketedIn{SlotName}` Multiplier
// 接入 → Spirit 180→250（0.72x→1.00x，翻正）。详见 collect.rs::filter_parseable 闸门 +
// legacy/engine per-socket 后缀 + ingest {SlotName} 替换 + per_slot_socket_multipliers 预灌。
// **charm base buff 修复重记（+3 @5% / +2 @10%）**：huntress-ritualist-bow-shot 的
// Sunburst Ruby Charm（base `Ruby Charm`）固有 buff `+25% to Fire Resistance` 经
// base_items.charm_buff（overlay vendor 抽取）并入 CharmBuff 载荷 → FireResist
// 12→37（0.32x→1.00x），连带 FireMaxHit 0.73x→1.00x、TotalEHP 0.93x→1.00x。
// **Disciple of Varashta ES→armour 修复重记（+2 @5% / +2 @10%）**：升华 Sacred Rituals
// （tree node 56857）『N% of your current Energy Shield is added to your Armour for
// determining your Physical Damage Reduction from Armour』→ EnergyShieldAppliesTo
// PhysicalDamageTaken=60，taken.rs effective_applied_armour 加 60% ES 借入项 → PhysDR
// 0.08x→1.03x（连带 PhysMaxHit 命中）。sorceress-disciple-of-varashta-comet。
// **Gemling Virtuous Barrier per-Mote 升华 buff 修复重记（+7 @5% / +7 @10%）**：升华 notable
// 「Essence of Virtue」（tree node 11641）`Grants Skill: Virtuous Barrier`，该 buff 按属性
// Mote 数给 INC（Armour/Evasion/EnergyShield ×DexterityMoteSkillCount、Life ×StrengthMote…）。
// 修复双路：① stat_map_engine buff 域允收名单加 Armour/Evasion/EnergyShield/Life/LifeRegen
// （barrier stat 已在 data，唯缺放行）；② inject_per_x_multipliers 按 vendor CalcSetup.lua:1766-
// 1781 计 Attribute-Mote（base 3/3/3 + 单属性宝石 +2/多属性各 +1）并 provision 三 multiplier。
// mercenary-gemling-legionnaire-explosive-grenade：Armour 0.70x→1.00x、Evasion 0.81x→1.00x、
// EnergyShield 0.72x→1.00x，连带 PhysDR/各 MaxHit/EHP 下游。
// **essence-drain taken+Total 双修复重记（+7 @5% / +7 @10%，两修复须同序合并）**：
// ① 『Take 30% less Damage』动词前置形（tree node 28153 Phased Form）接入 DamageTaken
//   MORE −30（taken_mult 0.7，oracle AfterReductionTakenHitMulti 同值）；
// ② Discipline 光环 buff 改走 `EnergyShieldTotal` 直加通道（vendor CalcDefence.lua:1331/
//   :1394——**不乘 inc/more**；旧映射错并入 EnergyShield 桶吃全局 570% inc → ES 多算
//   1.13x）。defence.rs 矩阵新增 Total 通道（读取/转换传播/缩残/直加）。
// 叠加后 sorceress-chronomancer-essence-drain 七列翻正（ES/ESRecoveryCap 1.13x→1.03x、
// 四 MaxHit+TotalEHP 0.79x→1.03x）。⚠️ 单独合并 ② 会让 MaxHit 0.79→0.72 更差（Total
// 修复缩 ES 而 taken 缺口仍在）——两 commit 同 PR 防拆。
// **Spirit 预留双机制修复重记（+8 @5% / +7 @10%）**：
// ① 宝石品质预留效率（vendor CalcDefence.lua:251 `/(1+efficiency/100)`，数据 =
//   overlay/gem_quality_stats.json 的 `base_reservation_efficiency_+%` 斜率×q）——
//   跨 build 通用缺口，ember/frost-bomb/flicker/monk-twister/pathfinder/stormweaver/
//   detonate-dead 七 build 的 SpiritUnres 列翻正；
// ② Blasphemy per-curse 预留（:273-284 `blasphemy_base_spirit_reservation_per_socketed_curse`
//   =60 × 同组 AppliesCurse 数，**单份缩放 round 后 ×count**——essence-drain
//   60→55×3=165，SpiritUnres 28.5x→1.00x 精确翻正）。
// 剩余：druid-comet 1.26x/coiling（Spell Totem 化预留 + ReservationEfficiency 词条族，
// M2 Track D）、wolf-pack（companion 管线）。
// **域限定预留效率 + Ancestral Bond totem 预留重记（+2 @5% / +3 @10%）**：
// ① SkillTypes 位掩码 u64→[u64;5]（320 位，Meta=122/SummonsTotem=25 等全域可表达，
//   Debug/canonical 高位全零保旧格式→缓存 byte-stable）；
// ② 域限定效率词条（「Meta Skills have N% increased Reservation Efficiency」→
//   ReservationEfficiency INC + SkillTypes tag，per-gem cfg 消费）+ **删除**
//   calc_spirit_reservation 的聚合侧全局 efficiency 除法（与注入侧构成双重应用，
//   frost-bomb 面板 148 vs 注入 166 的 18 差值根因）；
// ③ Ancestral Bond『Totems reserve 75 Spirit each』→ AncestralBond FLAG +
//   ExtraSpirit 75 + SkillType(SummonsTotem)（run-parsemod 双 mod 口径），
//   SummonsTotem×flag 入选预留循环（CalcDefence.lua:197）。
// druid-comet SpiritUnres 209/209 精确翻正、frost-bomb 136/138（0.99x）翻正、
// disciple 连带（efficiency）。coiling/wolf-pack/gemling 仍 miss（companion 管线
// / Blasphemy ExtraSpirit 交互待审计）。
// **aura magnitude 乘区重记（+5 @5% / +6 @10% / core-8 +1）**：buff_pass aura
// 乘区对齐 vendor CalcPerform.lua:2204-2205——① per-skill cfg（BuffSpec 携带
// 来源效果类型位，域词条「Banner Skills have N% increased Aura Magnitudes」
// = AuraEffect INC + SkillTypes(Banner=89) 只命中对应 aura）；② `Magnitude`
// 独立乘区桶（「Aura Skills have N% increased Magnitudes」= Magnitude INC +
// SkillTypes(Aura)，与 AuraEffect 桶分别成 (1+Σinc/100) 后相乘——wolf-pack
// Defiance Banner 30×(1+39%)×(1+88%)=78.4 对 vendor 78）。三形态词条双 parser
// 落地（legacy parse_scoped_buff_magnitude + overlay 3 条目）。
// wolf-pack Armour 0.71→0.98✓/EvadeChance·MeleeEvade→0.95✓/PhysDR→0.96✓，
// huntress-spirit-walker ChaosMaxHit 0.88→1.00✓（aura 小点词条正外溢）。
// **Tactician『A Solid Plan』预留 more 桶（+1 @5%/+1 @10%）**：「Persistent Buffs
// have 50% less Reservation」（tree 15044）→ `Reserved MORE −50 + Persistent+Buff
// 双 tag AND`（vendor ModParser.lua:1339 tagList）；spirit_reservation_modifiers
// 消费 `SpiritReserved`/`Reserved` inc/more + efficiency MORE（CalcDefence.lua:
// 240-252 全式）。wolf-pack SpiritUnres −210→21 精确闭合（oracle 逐技能对账：
// banner 自身 10% efficiency 原有 quality 路径已覆盖；Wolf Pack companion 只带
// Persistent 无 Buff，AND 语义下不吃 ×0.5——两侧一致）。engine 侧 template
// skill_type_bit 补 Persistent/Buff 位映射（pre_flag 丢 tag 根因）。
// **BeenHitRecently 条件后缀（+1 @5%/+1 @10%，core-8 +1）**：「… if you
// have/haven't been Hit Recently」（vendor ModParser.lua:1955/:1961）→
// Condition `BeenHitRecently`（负形 neg），cfg 真值走 config
// `conditionBeenHitRecently` 通用透传（既有）。wolf-pack 树点『Backup Plan』
// (53853，0_5 树) 的 40% Evasion 条件词条激活——Evasion 14618→16824.47 对
// vendor 16824 精确闭合（oracle defenceModList 对账：INC 165→205），EHP
// 0.54→0.59 正外溢。
// **过时导入修正（PR#40 合并后回记 416→415）**：#40 是 B3 前分支——其 legacy.rs
// 词条 + 基线 416 在 pre-B3 世界线实测；B3 闸门切 engine 后该词条已经由
// mod_parser_rules 数据通道生效（B3 commit 的 core-8 138→139 即此格），wolf-pack
// Evasion 在 #40 合并前即 1.00x ✓。合并 #40 行为零变化（18 build 逐格 diff 为空），
// 416 是双重计数；当前实测 415（@10% 432 与 core-8 139 恰与现实吻合，保留）。
// **Mageblood legacies 重记（Phase 1 #1，+50 @5% 343→393 / +56 @10% 361→417）**：见
// BASELINE_DEF_CORE_HIT5 上的说明；护甲/闪避/抗性列跨多 build 收敛。
// **Virtuous Barrier Life 名归一重记（+8 @5% 393→401 / +8 @10% 417→425）**：见
// BASELINE_DEF_CORE_HIT5 上的说明；gemling 8 列（Life/TotalEHP/5×MaxHit/LifeUnres）翻正。
// **Item ES 重算重记（+4 @5% 401→405 / +3 @10% 425→428）**：titan+stormweaver 各
// ES+ESRecoveryCap 翻正；见 BASELINE_DEF_CORE_HIT5 上说明。
// **Refraction buff EvasionGainAsDeflection 重记（+2 @5% 405→407 / +1 @10%
// 428→429）**：support Refraction I/II 的 Refractive Plating buff 载荷
// （`support_tempered_valour_deflection_rating_%_of_evasion_rating` BASE 20）
// 经 player buff 允收名单接入（stat_map_engine），wolf-pack DeflectChance
// 0.80x→1.00x（@5%+@10%）、pathfinder 0.93x→1.00x（@5%）。
// **Refraction buff ArmourAppliesTo<El>DamageTaken 重记（+3 @5% 407→410）**：
// 同 buff 的 `support_tempered_valour_%_armour_to_apply_to_elemental_damage`
// 载荷（三条 BASE 30）经同一 player buff 允收名单接入，消费方
// `calc::taken::armour_applies_pct`（tree 84 + buff 30 = 114%，oracle 钉值
// FireEffectiveAppliedArmour 21181.2）。wolf-pack Fire/Cold/LightMaxHit
// 0.94x→0.96x（@5% 翻正）、TotalEHP 0.81x→0.88x（余量 = Armour 0.98x 本体
// 差 + ChaosMaxHit 0.87x + Life 1.11x，均与本通道无关）。@10% 无变化。
// **0.5.4b #4 Communion/LowLife + Voices 重记（+3 @5% 410→413 / +5 @10% 429→434）**：
// core-8 的 SpiritUnres/LifeUnres 翻正（见 BASELINE_DEF_CORE_HIT5 上说明）+
// abyssal-lich EnergyShield 0.93x→0.98x（Voices sinister 珠宝的 ES 词条找回）。
const BASELINE_DEF_HIT5: usize = 425; // 存量 #7-3/4 后 425/450（ritualist 6 格 + gemling 9 格 + wolf-pack Life/Armour 精确翻正，wolf-pack MaxHit 族随 charm-guard 掩蔽摘除移出——余量 = companion ally-mitigation 层；Communion+Voices 413（ArmourAppliesTo 410；Refraction 407；ItemES 405；Barrier-Life 401；Mageblood 393；迁移基线 343；0.5.0=415）
const BASELINE_DEF_HIT10: usize = 434; // Communion+Voices 后 434/450（Refraction 429；ItemES 428；Barrier-Life 425；Mageblood 417；迁移基线 361；0.5.0=432）
// **附加授予效果展开重记（+3 @10%）**：gem 的 additionalGrantedEffectId1..N
// （overlay/gem_effects.json 外键，如三 banner 的 buff 侧效果——主位是预留侧
// ReservationPlayer、buff 侧 <X>BannerPlayer（Aura）在附加位）在 buff_skill_specs
// 展开为独立效果参与 aura/curse/debuff 分类。wolf-pack：Defiance Banner 的
// Armour/Evasion MORE 30（Condition:BannerPlanted，config 无 tag FLAG 桥已放行）
// 激活——Armour 0.55→0.71、Evasion 0.49→0.63、三 MaxHit 0.89→0.91（@10% 翻正）、
// EHP 0.28→0.37。到 1.00x 还差 banner valour 缩放（AuraEffect MORE per-resource
// ×Multiplier:BannerValour，vendor :1186/:2783——见记忆 companion 管线路线）。
// Dread Banner 的 `..._additional_maximum_all_elemental_resistances_%_to_apply`
// 静态映射按 vendor 口径删除（PoB2 丢弃该 stat，M4-I 实查）。
// **进攻 parity 修复簇累计重记（Onslaught + CI→FullLife + MultiplierThreshold 三修复合并）**：
// - Onslaught 幻影（移除 item.rs `parse_granted_buff_flag`，PoB2 ModParser 对
//   `Grants Onslaught during effect` 返回 unsupported）：detonate-dead/coiling/flicker
//   Speed +20% 偏高 → 归 1.00x。
// - CI→FullLife 桥（vendor CalcDefence.lua:123-126：CI build 恒满生命）：flicker（CI）
//   「while on Full Life」增伤生效 → AvgDamage 0.90x→0.99x、五分量齐命中。
// - MultiplierThreshold:enemyDistance（`... against enemies within/further than N metres`
//   接通，含 collect.rs 预过滤闸门窄放行）：monk-twister node 5802「Stand and Deliver」
//   等 within-metres 节点生效 → monk-twister CritMult/Avg/TotalDPS 转命中，huntress-twister/
//   titan-shield-wall 连带。
// 三修复 build 互不重叠、增益叠加；合并后全量实测重记（master 62/70 → 70/73）。
//
// **radius-jewel × weapon-set 交互修复重记（gemling CritChance）**：非激活武器组专属点
// 在 PoB2 仍留在 allocNodes（CalcSetup.lua:209-228），范围珠宝授予（`... in Radius also
// grant`，源=jewel）照样落到这些 notable 上。PoBR 原先把非激活组节点整个剔除 → gemling
// crit jewel 的 `7% increased Crit Hit Chance for Attacks` 只命中 1/6 个 in-radius
// notable（其余 5 个在 WeaponSet2）→ CritChance 8.55 vs 10.30（0.83x）。修复=
// `parse_passive_nodes` 单独回传被剔除的非激活组节点，`radius_jewel_expansions` 在几何里
// 并回完整已分配集（节点自身 mod 仍 masking，行为不变）。gemling CritChance→1.00x、
// AvgDamage/TotalDPS 0.96x→1.03x。其余 build 零回归（off 70/73 → 71/74）。
// **Mageblood Diamond crit 名归一重记（+2 @5% 39→41 / +3 @10% 47→50）**：Mageblood
// LegacyOfDiamond 注入 vendor 名 `CritChance` INC，但 calc::crit 读 `CriticalStrikeChance`
// （PoBR 规范名）——裸注入不过 parser 的 translate_vendor_name，落死桶（同 Virtuous
// Barrier Life→MaximumLife 类）。表内改用 `CriticalStrikeChance` 后 blood-mage CritChance
// 0.79x→0.96x、ember CritChance+CritMultiplier→1.00x（InevitableCrit 的 crit-mult 惩罚随
// pre-eff crit 修复一并解决），三 Diamond build 的 crit/DPS 抬升。见 calc/mageblood.rs。
// **Mageblood Silver Speed 名归一重记（+4 @5% 41→45 / +4 @10% 50→54）**：同 crit 死桶
// 类——LegacyOfSilver 注入 vendor 裸 `Speed` INC，但 PoBR 速度桶名是 `SkillSpeed`
// （SPEED_BUCKET，攻/施法通吃）。改后 ember/monk-twister/smith/titan 的 Speed 列翻正、
// DPS 抬升（ember 0.68x→0.77x、monk-twister→0.88x）。见 calc/mageblood.rs LegacyOfSilver。
// **bloodmage 池转换后 Mana multiplier 刷新重记（off +1 @5% 45→46 / @10% 54→55；dot +2
// @5% 9→11 / @10% 11→13）**：perform 防御资源转换（Eldritch Battery ES→Mana）后重算了
// mana_pool 并回填 cfg.stats["Mana"]，但漏刷 cfg.multipliers["Mana"]（per-100-max-Mana
// 类词条如 Arcane Intensity 读它）→ 池转换 build 的 mana 缩放用了转换前的旧值。补刷后
// blood-mage SpellDamage INC 39→105、TotalDPS 0.74x→0.87x，其 DoT 基底随之抬升。见 perform.rs。
// **0.5.4b #4 Communion/LowLife + Voices 重记（off +6 @5% 46→52 / +4 @10% 55→59）**：
// 两个 0.5.4b 新机制在真实 build 上叠加成 per-build DPS 簇（gap map「~0.60x」项）：
// 1. Atziri's Communion Spirit→Life 保留转换（vendor CalcDefence.lua:248-254）→
//    重保留 build 自动 Low Life（:335-350 unreserved ≤ 35%）→「while on Low Life」
//    族增伤解锁（ritualist：tree +60 与 Direstrike buff +70 attack INC，oracle
//    damageModList 钉值 = 缺口 130 INC 整）。
// 2. Voices「Allocates 2 Sinister Jewel sockets」→ sinister socket 珠宝入计
//    （ritualist crit chance/mult 13/37 INC 找回，双列精确闭合）。
// huntress-ritualist TotalDPS 0.68x→0.99x（AvgDamage/CritChance/CritMultiplier 同翻）、
// witch-abyssal-lich TotalDPS 0.62x→0.91x（Speed 0.91x→1.00x、CritChance 0.98x、
// CritMultiplier 0.75x→0.97x，其中 Speed/CritChance @5% 翻正、DPS 未回带）。
// **0.5.4b #4 grenade 短语解禁重记（off +4 @5% 52→56 / +3 @10% 59→62）**：vendor
// 0.22.0 ModParser gem 名注册循环新增 `not grantedEffect.fromItem` 排除
// （ModParser.lua:6423）——`MeleeGrenadeLauncherPlayer`（name "Grenade"，fromItem）
// 不再抢注 skillNameList，`grenade` / `for grenade skills` 短语恢复为 live
// `SkillType.Grenade` tag（run-parsemod 双证）。PR#53 时代（0.21 语义）的抽取侧
// 死条目/惰性改写撤销 + mod_parser_rules/parsed_mods 再生成。deadeye 3×15 CDR
// 树词条（oracle extraModList 钉源 Tree:21077/354/48429）生效：deadeye Speed
// 0.65x→1.00x（翻正）、TotalDPS 0.53x→0.83x；gemling Speed/AvgDamage/TotalDPS/
// CombinedDPS 全部 1.00-1.04x 翻正。
// **0.5.4b #5 Blazing Critical 全局火焰 buff 重记（off +2 @5% 56→58 / +2 @10%
// 62→64）**：0.22.0 给 `support_blazing_crits_gain_%_fire_damage_with_attacks_
// on_critical_hit` 补 GlobalEffect/Buff tag（sup_int.lua:959）——15%
// `DamageGainAsFire`（Attack + Condition:CritRecently）从死词条变成全局玩家
// buff。两处接线：stat_map_engine 玩家 buff 允收名单 + support_buff_specs
// 裁决对象补附加授予效果（Charged Staff 的隐藏 Attack 附加效果
// ChargedStaffShockwavePlayer 才是 Blazing Critical 的兼容宿主）。
// monk-twister AverageDamage/TotalDPS 0.60x→0.96x 双列翻正；flicker
// TotalDPS 0.76x→0.84x、spirit-walker 0.78x→0.89x 收敛未入列（残余 =
// 攻击 AvgDamage 族存量缺口的平方传导）。
// **0.5.4b #6 攻击 AvgDamage 族攻坚重记（off +13 @5% 58→71 / +9 @10% 64→73）**：
// 五个通用消费点修复叠加，整族（smith/titan/flicker/monk-twister/spirit-walker/
// pathfinder/ritualist/gemling）全部进入 5% 带：
// 1. Bifurcate 爆伤条件概率（vendor :3823-3846 `conditionalBifurcateChance =
//    (PreBifurcate²/100)/CritChance`，PoBR 误移植为无条件 pre²/10000——新旧 vendor
//    同式，属存量误差）+ 武器无 flag 爆伤词条转按手条件（Item.lua:1954-1961，
//    **0.22.0 新增** CritMultiplier 进转换清单）：spirit-walker/pathfinder CritMult
//    0.94x→1.00x/0.99x，titan CritMult 1.20x→1.00x（oracle 钉值 6.02→5.00 golden 精确）。
// 2. enemyDistance placeholder 喂 skillDist（CalcActiveSkill.lua:671，**0.22.0 新增**
//    configPlaceholder 兜底）：Close Combat 30% MORE 按 ramp(20)=0.6 生效 →
//    flicker 0.838x→0.99x、smith +Close Combat 段。
// 3. PerStat statList 支持（ModParser.lua:1631 `per 75 armour and evasion on
//    equipped shield` → `|` 复合 Multiplier var 求和）：smith/titan 盾防御缩放
//    tree notable（Tree:27687，oracle 钉值 88 INC）生效。
// 4. hybrid mana→life cost + per-LifeCost 词条（Atalui's Bloodletting：
//    `base_skill_cost_life_instead_of_mana_%` 100 + PerStat{stat=LifeCost,div=20,
//    limit=40,limitTotal}，vendor :2067/:2090-2104）：smith +30% DamageGainAsPhysical
//    （oracle LifeCost 309 → floor(309/20)=15 → 30）。
// 5. 暴击短路路径分腿减伤 blend（vendor :4395；敌方护甲 DR 依赖单次击中量，
//    crit 腿用暴击后击中算 DR）：spirit-walker 0.94x→0.98x、blood-mage/abyssal-lich
//    连带收敛（0.87x→0.88x / 0.91x→0.93x，未入列）。
// 逐 build：smith 0.575x→0.996x、titan 0.874x→0.978x、flicker 0.838x→1.003x、
// spirit-walker 0.892x→0.980x、monk-twister 0.958x→0.980x、pathfinder 0.904x→
// 0.963x、ritualist 0.989x→1.000x。剩余脱靶：deadeye 0.832x（pre-0.5.4b per-hit
// 欠条）、blood-mage 0.880x / abyssal-lich 0.926x（Mageblood 词条族缺口，Phase 1
// 独立项——oracle 钉值 blood-mage 缺 `INC CritChance 107 'Mageblood'`）、
// frost-bomb 0.661x（golden 两版未动，存量冷却 DPS 缺口）。
// （存量 #7-1）frost-bomb TotalDPS 0.66x→1.00x 翻正（off @5/@10 各 +1）：
// Archmage buff `DamageGainAsLightning`（BASE 4/100 Mana → 80% gain-as，
// act_int.lua:229-231）+ curse 链两缺口（EW 取数等级未吃 +8 spell skill
// levels：-58→-66；技能局部 CurseEffect 段缺失：Heightened Curse +25 +
// EW 品质 +10 → 敌抗 9→-7，oracle enemyMitigation 逐源钉值）。
// **0.5.4b #8 Zarokh's Gift anoint socket 重记（off +3 @5% 73→76 / +1 @10% 75→76）**：
// #6 留下的「blood-mage 缺 `INC CritChance 107 'Mageblood'`」钉值实为下游症状：
// Mageblood 效果表与 `MagesLegacyEffect` 解析均已在位（逐 mod 对拍与 oracle 一致），
// 真根因 = 两 build 的 amulet anoint `{enchant}Allocates Zarokh's Gift` 未把具名
// jewel socket 节点 11184 视为已分配（vendor PassiveSpec.lua:1106-1114 sockets 名
// 匹配 fallback），socket 内珠宝（blood-mage: Pandemonium Ornament——CritChance
// INC 24 + CritMult INC 25/28；abyssal: 同位 ES/防御珠宝）整体被丢弃。修复见
// xml_build.rs NAMED_SOCKETS_0_5。blood-mage TotalDPS 0.880x→1.00x（CritChance
// 88.5→92.1、CritMult 5.34→5.87 逐值精确）、abyssal-lich TotalDPS 0.926x→1.00x
// （CritChance/CritMult 1.00x；ES 12124→12437 vs golden 12434、MaxHit 五族 +
// TotalEHP 0.97-0.98x→1.00x——def 列先前已在 5% 带内，def 计数不变）。
// 18 build 逐格 diff 仅此两 build 变动。
const BASELINE_OFF_HIT5: usize = 76; // #8 Zarokh's Gift 后 76/80（#6+#7 合并 73；#6 单独 71；#7 单独 60；迁移基线 39）
const BASELINE_OFF_HIT10: usize = 76; // #8 后 76/80（#6+#7 合并 75；#6 单独 73；#7 单独 66；迁移基线 47；0.5.0=74）

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
// 上调 17→20 / 21→24（damaging-ailment magnitude 的 `with Critical Hits` 条件词条
// 接入：parser 新增暴击后缀剥离 + `ailment_scoped_cfg` 置 CriticalStrike=true，对齐
// vendor dotCfg `skillCond["CriticalStrike"]=true`，CalcOffence.lua:5006）。huntress
// poison 0.58x→1.04x、bleed 0.64x→1.11x，TotalDotDPS/CombinedDPS 双列入命中。
// **进攻修复簇 dot 列累计重记**：flicker（CI→FullLife）CombinedDPS 0.90x→0.99x、
// monk-twister / titan-shield-wall（MultiplierThreshold）CombinedDPS 随 hit 转命中。
// 三修复合并后 dot 列全量实测重记（master 22/26 → 26/28）。
// **诚实下调 26→25 / 28→27（gain-as fallback 修复,2026-07-06）**：deadeye ignite
// TotalDotDPS 0.97x✓ 是双错抵消——旧 gain-as base 回退使 fire 分量高估 ~1.30x,
// 恰好补偿 PoBR ignite 链自身 ~0.69x 低估（vendor IgniteChance 2.5%/StackPotential/
// RollAverage 公式链未对齐,登记下一切片）。修复 fire 基础后该格如实显形为 0.69x✗。
// 同 commit：进攻/防御全指标持平（off 71/80·def 415/450·core-8 139/144,且系
// deadeye 两条 INC 冻结行解冻后仍持平）,全 18 build 仅 deadeye 自身 dot 一格变动。
// **回记 25→26 / 27→28（ailment stacks × dpsMultiplier,同日）**：vendor :3878 在
// 异常段前把 `DPS` 乘区折进 skillData.dpsMultiplier,:5046 ailmentStacks 因此吃到
// （Payload 二次起爆 ×1.5）；PoBR 与 TotalDPS 同源取 dps_end_factors 折入
// ctx.speed（:3880 hitRate 同语义）。deadeye ignite 0.69x→1.04x✓ 找回；gemling
// dot 0.77x→1.16x（miss→miss 不计数,其底层 ~1.27x 高估属 gemling 多因子独立
// 缺口）；其余 16 build 逐格不变。
// **上调 26→27 / 28→29（grenade 短语 skillNameList 抢先修复,2026-07-06）**：
// vendor 运行时把裸 `grenade` 短语剥成惰性 SkillName{"Grenade"}（恒不匹配 = 零
// 效果,ModCache/oracle 探针双证）,PoBR 旧 flag_phrase 产 SkillTypes(Grenade) 反而
// 生效 → gemling/deadeye 全类型 inc +60 over-apply。payload 改写 + gemling 冻结行
// 解冻（dps 端因子 1.65 两侧一致）后：gemling AvgDmg/TotalDPS/dot 三列 1.00x 精确
// 闭合（dot 0.77x→1.00x 入列）,deadeye 残余 ~2% 同根消散（AvgDmg/dot 均 1.00x）。
// 18 build 逐格 diff 仅 gemling dot 一格翻正。
// **诚实下调 27→26 / 29→28（blood-mage curse 欠条解冻,2026-07-07）**：
// `magnitudes of curses you inflict are zero`（Coiling Whisper）解冻——curse 机制
// 全链已验证与 vendor 一致（5 诅咒占槽/ignore limit=99/per-Curse gain-as ×5 经
// Multiplier{CurseOnEnemy} 生效）。冻结态的 blood-mage dot 1.01x✓ 是双错抵消：
// 诅咒按满效果错误生效（物品明言 zero）恰好补偿 per-hit/dot 存量低估。解冻 +
// 同组辅助授予等级修复（Chaos Mastery +1,L30→31 vendor 精确）后 TotalDPS
// 0.73x→0.84x、dot 0.54x→0.70x 收敛但未回带；剩余 ~16% per-hit 低估是
// blood-mage 自身根因（deadeye/gemling 同型剧本）,修复后基线回记。
// 18 build 逐格 diff 仅 blood-mage 三格变动。冻结榜只剩 legacy `split`。
// **bloodmage mana-mult 重记（dot +2 @5% 9→11 / @10% 11→13）**：见 BASELINE_OFF_HIT5 上
// 说明——mana 缩放抬 blood-mage 的 spell DoT 基底，其 TotalDotDPS/CombinedDPS 翻正。
// **0.5.4b #4 Communion/LowLife + Voices 重记（dot +2 @5% 11→13 / +5 @10% 13→18）**：
// ritualist TotalDotDPS 0.60x→1.00x / CombinedDPS 0.63x→0.99x（LowLife 增伤 +
// sinister 珠宝的 damaging-ailment-magnitude 词条同时抬 bleed/poison 基底）；
// abyssal-lich dot 列随 hit 侧收敛入 @10% 带。
// **0.5.4b #4 grenade 短语解禁重记（dot +1 @5% 13→14 / +1 @10% 18→19）**：
// gemling CombinedDPS 0.65x→1.04x 翻正（见 BASELINE_OFF_HIT5 上说明）。
// **0.5.4b #5 Blazing Critical 重记（dot +2 @5% 14→16 / +2 @10% 19→21）**：
// 点燃火源随全局 15% DamageGainAsFire 平方级放大（chance ∝ fire/threshold ×
// magnitude ∝ fire）：monk-twister TotalDotDPS 0.44x→0.98x + CombinedDPS
// 0.60x→0.96x 翻正；flicker dot 0.05x→0.72x、spirit-walker 0.23x→0.87x 收敛
// 未入列（残余同为 hit 侧存量缺口的平方传导，非 dot 侧机制）。其余 dot 脱靶
// 逐格判定：deadeye 0.69x=hit 0.83x²、blood-mage 0.79x≈hit 0.87x²、titan/
// abyssal/pathfinder 0.91-0.94x 随 hit 收敛；smith 0.20x 中 dot 特有残差 =
// Infernal Cry uptime-scaled DamageGainAsFire 12% 未建模（新旧 vendor 同值，
// 存量缺口非 0.5.4b 项）；frost-bomb 0.87x / essence-drain WithDotDPS 1.36x
// golden 两版未动（存量）；gemling dot 1.10x 高估 = fire/crit 小幅 hit 侧
// 高估的下游传导（oracle 逐分量：fire 1.03x × stacks 1.05x × crit 混叠）。
// **0.5.4b #6 AvgDamage 族 dot 列跟涨重记（dot +9 @5% 16→25 / +6 @10% 21→27）**：
// 点燃 ∝ 火源²，hit 侧整族闭合后 dot 列自动跟正（flicker 0.72x→0.87✓ 附近、
// spirit-walker 0.87x→0.98x✓、titan/pathfinder/monk-twister CombinedDPS 随 hit
// 翻正）。剩余脱靶：smith TotalDotDPS 0.40x（= 火源比 0.63² —— Infernal Cry
// uptime-scaled DamageGainAsFire 12.04% 未建模，新旧 vendor 同值的存量 warcry
// uptime 机制，见 BASELINE_OFF_HIT5 注）；deadeye 0.69x = hit 0.83x²；
// blood-mage 0.79x（Mageblood）；gemling 1.10x 高估（存量）。
// （存量 #7-1）frost-bomb 修复的 dot 列迁移：CombinedDPS 0.66x→1.00x 入列
// （@5/@10 各 +1）、TotalDotDPS 0.87x→1.07x 入 @10%；druid-oracle-comet
// TotalDotDPS 1.04x→1.06x 出 @5%（EW 变强的下游——vendor 侧 druid 的 EW
// 根本未入 enemy resistMods（curse 槽位/优先级差异，oracle 钉值），PoBR
// 施加了它，存量高估被放大 2%，另行追）。@5 净 0、@10 净 +2。
// **0.5.4b #8 Zarokh's Gift anoint socket 重记（dot +4 @5% 27→31 / +2 @10% 31→33）**：
// blood-mage TotalDotDPS 0.79x→1.00x + CombinedDPS 0.88x→1.00x、abyssal-lich
// TotalDotDPS 0.94x→1.00x + CombinedDPS 0.93x→1.00x——hit 侧 crit 闭合后
// 点燃/DoT 基底跟正（见 BASELINE_OFF_HIT5 上 #8 说明）。

// **存量 #9 warcry uptime 机器重记（dot +1 @10% 31→32）**：Infernal Cry 的
// uptime 缩放 `DamageGainAsFire`（CalcOffence.lua:3229-3256，pobr-core
// `calc::warcry`）落地，smith TotalDotDPS 0.40x→1.06x 入 @10% 带（uptime/
// castTime/cooldown 对 oracle 逐位：19.4116%/0.544218/6.27；gain 62×uptime
// =12.035 与 vendor "Uptime Scaled Infernal Cry" 条 bit-exact）。残余 +6% =
// smith 命中侧原有 +2% 高估（此前被缺失的 gain 抵消遮蔽，AverageDamage
// 0.996→1.02）经点燃 ∝ 火源² 放大——命中侧存量项，非 warcry 机制。titan 随动：
// TotalDotDPS 0.98→1.01、TotalDPS 0.98→1.05（同为被遮蔽的原有高估暴露，
// 仍在 @5% 带内）。其余 16 build 无 warcry，逐值不变。
const BASELINE_DOT_HIT5: usize = 31; // #8+#9 合并实测 31/37（#8 单独 31；#9 单独 27；迁移基线 9；0.5.0=26）
const BASELINE_DOT_HIT10: usize = 34; // #8+#9 合并实测 34/37（#8 单独 33；#9 单独 32；迁移基线 11；0.5.0=28）

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
/// **SkillType 全量化重记（数据驱动 A1，panel +8 @5% / +6 @10%）**：cfg 侧
/// `skill_type_bits` 从手工白名单（Attack/Spell/… 十余位）换全量枚举置位
/// （`SkillTypes::from_pob2_name`，vendor Global.lua 290 名生成表），tag 侧
/// （template.rs / special_mod.rs）同 commit 全量化——一批 `ModTag::SkillTypes`
/// 域词条（Area/Projectile/Grenade 等）在 panel 口径开始正确匹配。effective
/// 主口径与防御/进攻/dot 主基线逐值持平（纯 panel 侧收敛）。
const PANEL_OFF_HIT5: usize = 42; // #6 与存量 #7 RemoveStats 各 +1 叠加实测 42（#4 后 40；Communion+Voices 38）；迁移基线 27；0.5.0=44
const PANEL_OFF_HIT10: usize = 43; // 同上叠加实测 43（#4 后 41；Communion+Voices 39）；迁移基线 30；0.5.0=46

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

/// 收集一个 build 的全量词条语料行（装备三段 + 珠宝 + **已分配树节点**），供
/// unsupported 分类。**绕开** `filter_parseable` / 树侧闸门——直接对原始行分类，
/// 使缺口语料可见（A-1 方法 §1；树行为 A2 并入——B3 后闸门/ingest 同一 engine，
/// 树侧缺口同样是生产缺口）。
fn collect_corpus_lines(dir: &Path, data: &BuildData) -> Vec<CorpusLine> {
    let Ok(code) = std::fs::read_to_string(dir.join("code.txt")) else {
        return Vec::new();
    };
    let Ok(build) = parse_build_from_code(code.trim()) else {
        return Vec::new();
    };
    let build_id = dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("?")
        .to_string();
    let mut lines = Vec::new();
    let push_item = |item: &pobr_data::item::Item, src: LineSource, lines: &mut Vec<CorpusLine>| {
        for text in item
            .implicit_texts
            .iter()
            .chain(item.modifier_texts.iter())
            .chain(item.enchant_texts.iter())
        {
            let t = text.trim();
            if !t.is_empty() {
                lines.push(CorpusLine {
                    text: t.to_string(),
                    source: src,
                    build_id: build_id.clone(),
                });
            }
        }
    };
    let mut item_slots: Vec<_> = build.items.values().collect();
    item_slots.sort_by_key(|i| i.modifier_texts.len());
    for item in item_slots {
        push_item(item, LineSource::Item, &mut lines);
    }
    for jewel in &build.jewels {
        push_item(jewel, LineSource::Jewel, &mut lines);
    }
    // 已分配树节点 stats（含 `\n` 折行摊平——与 pobr_tree::split_lines 同口径）。
    // 注意：本语料按**独立行**分类，树侧生产还有折行合并兜底（combine_wrapped），
    // 故此处的 Partial/Unsupported 是生产缺口的**上界**。
    for node_id in &build.tree.allocated_nodes {
        let Some(def) = data.passive_nodes.get(&node_id.0) else {
            continue;
        };
        for stat in &def.stats {
            for line in stat.split('\n') {
                let t = line.trim();
                if !t.is_empty() {
                    lines.push(CorpusLine {
                        text: t.to_string(),
                        source: LineSource::Passive,
                        build_id: build_id.clone(),
                    });
                }
            }
        }
    }
    lines
}

/// unsupported 词条率曲线报表（M5b A-2，roadmap「unsupported 词条率下降曲线纳入
/// 报表」）。**report-only，不进门禁断言**——M5b 验收口径是曲线不是百分比。
/// 逐 build + 聚合的 词条总数 / parsed / unsupported / err 计数与百分比，附 Top-20
/// 缺口模板（C-2 选批的事实来源）。
///
/// `cargo test -p pobr-build --test ninja_parity -- corpus_unsupported_report --nocapture`
#[test]
fn corpus_unsupported_report() {
    let builds = discover_builds();
    let data = load_data();
    let mut all_lines: Vec<CorpusLine> = Vec::new();
    for dir in &builds {
        all_lines.extend(collect_corpus_lines(dir, &data));
    }
    // engine 生产口径段（闸门/ingest 同一 parser——本段数字即生产行为）。
    // Partial = 引擎识别一半但整行被闸门丢弃（迁移候选）；dropped-tags = pre_flag
    // 静默丢 tag（作用域放大 over-apply 风险面，此前完全不可见）。
    if let Some(rules) = data.parser_rules.as_deref() {
        use pobr_build::corpus::build_report_engine;
        let er = build_report_engine(&all_lines, rules);
        // vendor 裁决源：ModCache golden（vendor 全语料预解析缓存落盘，M6-C）。
        // gap 模板按样本（strip 方括号后小写）精查——「vendor 同款不支持」= 伪缺口
        // （不用修，slowing-potency 教训：vendor 空 mods + 全残留 = PoB2 不生效）；
        // 「vendor parsed」= 真缺口（迁移目标清单）。
        let vendor_verdict: std::collections::HashMap<String, bool> = {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../pobr-core/tests/fixtures/modcache_golden.json");
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                .and_then(|v| v.get("entries").and_then(|e| e.as_array()).cloned())
                .map(|entries| {
                    entries
                        .iter()
                        .filter_map(|e| {
                            let text = e.get("text")?.as_str()?.trim().to_ascii_lowercase();
                            // 「vendor 生效」= parsed 且无 leftover——ModCache 的
                            // status 不含 leftover 判定，但树加载对 extra 非空整行
                            // 丢弃（B3 语义），故 leftover 非空一律按不生效裁决。
                            let parsed = e.get("status")?.as_str()? == "parsed"
                                && e.get("leftover")
                                    .and_then(|l| l.as_str())
                                    .is_none_or(|l| l.trim().is_empty());
                            Some((text, parsed))
                        })
                        .collect()
                })
                .unwrap_or_default()
        };
        // pobr 主动降级/另行建模的模板——从 REAL 榜剥离，防止反复误列为待办：
        // - deferred：B3 暂缓行（unsupported_pobr_extra，双错抵消欠条）；
        // - modeled-elsewhere：gem-level 族（gem_property_bonuses 独立扫原始行
        //   已建模，GemProperty LIST 在 ModDb 无消费者；C5 统一）。
        let deferred: std::collections::HashSet<String> = {
            let path = repo_data_root()
                .join(pobr_data::GOLDEN_PARITY_DATA_VERSION)
                .join("overlay/mod_parser_rules.json");
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                .and_then(|v| {
                    v.get("unsupported_pobr_extra")
                        .and_then(|e| e.as_array())
                        .cloned()
                })
                .map(|arr| {
                    arr.iter()
                        .filter_map(|s| Some(s.as_str()?.to_string()))
                        .collect()
                })
                .unwrap_or_default()
        };
        // 显式裁决过的「不迁」模板（A2 收尾）——与 deferred/modeled-elsewhere 同属
        // 伪待办剥离，防止 REAL 榜反复误列：
        // - no-consumer(pobr)：`grants skill:` 族（vendor ExtraSkill LIST）。PoBR 技能
        //   来自 build XML 技能列表，树/装备授予行没有 ModDb 消费者——迁 LIST 只是
        //   噪音（gem-level 族同理），待 granted-skill 消费端立项时一并接线。
        // - wont-do(pobr)：树分配规则行（non-keystone medium radius——不是 stat mod，
        //   属 pobr-tree 分配语义）与 per-skill distance ramp（demo 套件 enemyDistance
        //   全 placeholder → DistanceRamp 恒休眠，DSL 亦无 4 捕获 ramp 形态；见
        //   ModTag::DistanceRamp 文档）。
        const NO_CONSUMER_PREFIXES: &[&str] = &["grants skill:"];
        const WONT_DO_PREFIXES: &[&str] = &[
            "non-keystone passive skills in medium radius",
            "projectiles deal #% more hit damage to targets in the first",
        ];
        let verdict_of = |t: &pobr_build::corpus::EngineTemplateStat| -> &'static str {
            if t.template.starts_with("+# to level of all") {
                return "modeled-elsewhere";
            }
            if NO_CONSUMER_PREFIXES
                .iter()
                .any(|p| t.template.starts_with(p))
            {
                return "no-consumer(pobr)";
            }
            if WONT_DO_PREFIXES.iter().any(|p| t.template.starts_with(p)) {
                return "wont-do(pobr)";
            }
            let mut any_hit = false;
            for s in &t.samples {
                let key = pobr_build::corpus::strip_brackets(s)
                    .trim()
                    .to_ascii_lowercase();
                if deferred.contains(&key) {
                    return "deferred(pobr)";
                }
                match vendor_verdict.get(&key) {
                    Some(true) => return "vendor=PARSED",
                    Some(false) => any_hit = true,
                    None => {}
                }
            }
            if any_hit { "vendor=unsup" } else { "vendor=?" }
        };
        eprintln!("\n============ ENGINE PRODUCTION REPORT (A2) ============");
        eprintln!(
            "[engine] lines: {}  parsed: {} ({:.1}%)  partial: {}  unsupported: {}  gap_rate: {:.1}%",
            er.total_lines,
            er.parsed,
            if er.total_lines == 0 {
                0.0
            } else {
                er.parsed as f64 / er.total_lines as f64 * 100.0
            },
            er.partial,
            er.unsupported,
            er.gap_rate() * 100.0,
        );
        eprintln!(
            "[engine] dropped-tag lines: {}  total dropped tags: {}",
            er.lines_with_dropped_tags, er.total_dropped_tags,
        );
        let (mut real, mut pseudo, mut unknown, mut skipped) = (0usize, 0usize, 0usize, 0usize);
        for t in &er.gap_templates {
            match verdict_of(t) {
                "vendor=PARSED" => real += 1,
                "vendor=unsup" => pseudo += 1,
                "deferred(pobr)" | "modeled-elsewhere" | "no-consumer(pobr)" | "wont-do(pobr)" => {
                    skipped += 1
                }
                _ => unknown += 1,
            }
        }
        eprintln!(
            "[vendor-verdict] gap templates: {real} REAL (vendor parses, we drop)  {pseudo} pseudo (vendor also unsupported)  {skipped} deferred/modeled-elsewhere/no-consumer/wont-do  {unknown} unknown (not in ModCache)",
        );
        eprintln!("--- Top-20 engine gap templates (production drops) ---");
        for (i, t) in er.gap_templates.iter().take(20).enumerate() {
            eprintln!(
                "{:>2}. [{:?}] hit={} cnt={} {} | {}",
                i + 1,
                t.class,
                t.builds_hit,
                t.total_count,
                verdict_of(t),
                t.template,
            );
        }
        eprintln!("--- REAL gaps (vendor parses, we drop — migration list, full) ---");
        let mut shown = 0usize;
        for t in &er.gap_templates {
            if verdict_of(t) == "vendor=PARSED" {
                shown += 1;
                eprintln!(
                    "{:>2}. [{:?}] hit={} cnt={} | {} || sample: {}",
                    shown,
                    t.class,
                    t.builds_hit,
                    t.total_count,
                    t.template,
                    t.samples.first().map(String::as_str).unwrap_or(""),
                );
            }
        }
        eprintln!("--- Top-10 dropped-tag templates (scope-widening risk) ---");
        for (i, t) in er.dropped_tag_templates.iter().take(10).enumerate() {
            eprintln!(
                "{:>2}. dropped={} hit={} cnt={} | {}",
                i + 1,
                t.dropped_tags,
                t.builds_hit,
                t.total_count,
                t.template,
            );
        }
        eprintln!(
            "total distinct engine gap templates: {}  dropped-tag templates: {}",
            er.gap_templates.len(),
            er.dropped_tag_templates.len(),
        );
        assert!(er.total_lines > 0, "engine 语料为空");
    }

    // 弱断言：语料非空（防 fixture/收集链断裂静默归零）。
    assert!(
        !all_lines.is_empty(),
        "corpus 收集为空——检查 build 装备词条收集链"
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
