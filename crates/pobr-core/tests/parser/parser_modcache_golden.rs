//! ModCache golden differential（M6 蓝图 §5.3 / T6 / §11.3 契约 5）。
//!
//! 把数据驱动引擎（`parse_mod_engine` + 真实规则）的输出，对照从 vendor
//! PoB2 `Data/ModCache.lua` 离线落盘的 golden（`tests/fixtures/modcache_golden.json`，
//! 由 `tools/pob2-oracle/oracle.lua --mode modcache-dump` 生成）逐词条对拍，产出
//! **五态报告**。这是 Track B 引擎重写的正确性基座：B 切换后复用同一 golden +
//! 同一报告 schema，diff 修复以本报告为共同输入（契约 5）。
//!
//! # 五态裁决（§5.2 口径，相对 vendor golden 落位）
//!
//! | 态 | 定义 | §5.2 对应 |
//! |----|------|-----------|
//! | `eq`     | golden parsed 且 PoBR parsed 且 canonical 相等 | EQ |
//! | `diff`   | 两侧都 parsed 但 canonical 不等                | DIFF（切换阻塞——B 终裁修复） |
//! | `miss`   | golden parsed 但 PoBR unsupported（PoBR 缺口） | NEW_ONLY 的镜像（vendor 能、PoBR 不能） |
//! | `extra`  | golden unsupported 但 PoBR parsed（PoBR 多解） | OLD_ONLY 的镜像（PoBR 能、vendor 缓存空） |
//! | `unsup`  | 两侧都 unsupported                              | UNSUP（计入覆盖率缺口，不阻塞） |
//!
//! 命中率 = `eq / (golden parsed 总数)`——B 引擎重写的目标是把 `miss`/`diff` 清零，
//! 使命中率 → 100%。
//!
//! # canonical 比较口径（**本批次的关键取舍**）
//!
//! Modifier canonical = `(name, type, value, flags[], keywordFlags[])`。**tag 结构
//! 不参与 EQ/DIFF 裁决**——vendor tag 表与 pobr `ModTag` 的字段形态不同
//! （vendor `{type="Multiplier",var=...,div=...}` vs pobr 形态化 enum），逐字段映射是
//! Track B 重写的工作而非本对拍基座的工作。tag 数量差异记进报告明细（`tag_delta`）
//! 供 B 参考，但不触发 DIFF。B 引擎落地后可在同一 fixture 上收紧到 tag 全等。
//!
//! 这一取舍使本基座**稳定**（不因 tag 表示差异产生海量假 DIFF），同时仍精确捕捉
//! name/type/value/flag 级的真实偏差——这正是引擎正确性的核心信号面。
//!
//! # 产物
//!
//! 报告写 `target/parser-modcache-diff-report.json`（schema 见模块内 `DiffReport`
//! 文档，契约 5），CI 与 B 的 diff 修复循环共同消费。本测试**不**对命中率设硬阈值
//! （引擎对全 ModCache 语料的覆盖本就部分——硬门禁是 parity 套件）；
//! 仅断言 fixture 可加载、报告可生成、且
//! 五态计数自洽（守住基座可运行性）。
//!
//! 实现注：pobr-core dev-deps 只有 `serde_json`（无 `serde` derive），故 golden
//! 读取与报告构建全走 `serde_json::Value` 手工字段——与仓库「lib 本体零 serde_json」
//! 约定一致（仅测试侧用）。

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use crate::support::parse_mod;
use pobr_core::mod_parser::ParseStatus;
use pobr_core::{ModValue, Modifier};
use pobr_data::modifier::ModType;
use serde_json::{Map, Value, json};

const GOLDEN_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/modcache_golden.json"
);

// ---------------------------------------------------------------------------
// golden fixture（oracle.lua --mode modcache-dump 产物）——serde_json::Value 视图
// ---------------------------------------------------------------------------

struct GoldenDoc {
    meta_total: usize,
    meta_parsed: usize,
    meta_unsupported: usize,
    entries: Vec<GoldenEntry>,
}

struct GoldenEntry {
    text: String,
    /// vendor 侧 canonical（已规范化排序）。空 = unsupported。
    canon: Vec<CanonMod>,
    parsed: bool,
}

fn as_usize(v: &Value, key: &str) -> usize {
    v.get(key)
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("golden _meta.{key} missing/not-uint")) as usize
}

fn load_golden() -> GoldenDoc {
    let raw = fs::read_to_string(GOLDEN_PATH)
        .unwrap_or_else(|e| panic!("read golden fixture {GOLDEN_PATH}: {e}"));
    let doc: Value = serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse golden: {e}"));

    let meta = doc.get("_meta").expect("golden _meta");
    let meta_total = as_usize(meta, "total");
    let meta_parsed = as_usize(meta, "parsed");
    let meta_unsupported = as_usize(meta, "unsupported");

    let arr = doc
        .get("entries")
        .and_then(Value::as_array)
        .expect("golden entries[]");
    let mut entries = Vec::with_capacity(arr.len());
    for e in arr {
        let text = e
            .get("text")
            .and_then(Value::as_str)
            .expect("entry.text")
            .to_string();
        let status = e.get("status").and_then(Value::as_str).unwrap_or("");
        let mods = e.get("mods").and_then(Value::as_array);
        let canon: Vec<CanonMod> = mods
            .map(|ms| ms.iter().map(golden_mod_to_canon).collect())
            .unwrap_or_default();
        let parsed = status == "parsed" && !canon.is_empty();
        entries.push(GoldenEntry {
            text,
            canon: sorted_canon(canon),
            parsed,
        });
    }
    GoldenDoc {
        meta_total,
        meta_parsed,
        meta_unsupported,
        entries,
    }
}

// ---------------------------------------------------------------------------
// canonical mod（两侧统一形态，参与 EQ/DIFF 裁决）
// ---------------------------------------------------------------------------

/// `(name, type, value, flags[], keywordFlags[])` 规范形态。`tag_count` 旁挂，
/// 不参与相等判定（见模块文档「canonical 比较口径」）。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CanonMod {
    name: String,
    mod_type: String,
    /// f64 用最短往返字符串表示，避免 NaN 不可比 + 浮点抖动。
    value: String,
    flags: Vec<String>,
    keyword_flags: Vec<String>,
    /// 旁挂信号，不进 core_eq。
    tag_count: usize,
}

impl CanonMod {
    /// EQ 判定用的「核心键」（排除 tag_count）。
    fn core_eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.mod_type == other.mod_type
            && self.value == other.value
            && self.flags == other.flags
            && self.keyword_flags == other.keyword_flags
    }

    fn to_json(&self) -> Value {
        json!({
            "name": self.name,
            "type": self.mod_type,
            "value": self.value,
            "flags": self.flags,
            "keywordFlags": self.keyword_flags,
            "tag_count": self.tag_count,
        })
    }
}

fn mod_type_label(t: ModType) -> &'static str {
    t.as_trace_label()
}

/// f64 → 最短往返字符串（整数走整数形态对齐 vendor 编码）。
fn num_to_canon(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        serde_json::Number::from_f64(v)
            .map(|n| n.to_string())
            .unwrap_or_else(|| format!("{v}"))
    }
}

fn value_to_canon(v: &ModValue) -> String {
    match v {
        ModValue::Number(n) => num_to_canon(*n),
        ModValue::Bool(b) => b.to_string(),
        ModValue::Text(s) => format!("{s:?}"),
        ModValue::NestedMods(mods) => format!("nested:{}", mods.len()),
    }
}

/// PoBR ModFlags → vendor Global.lua ModFlag 名（位值逐位等于，见
/// pobr-data/src/modifier.rs 文档）。只列单 bit；复合 mask 不单独命名
/// （vendor flagNames 也按单 bit 覆盖展开）。
const MOD_FLAG_NAMES: &[(u64, &str)] = &[
    (0x1, "Attack"),
    (0x2, "Spell"),
    (0x4, "Hit"),
    (0x8, "Dot"),
    (0x10, "Cast"),
    (0x20, "Thorns"),
    (0x100, "Melee"),
    (0x200, "Area"),
    (0x400, "Projectile"),
    (0x800, "Ailment"),
    (0x1000, "MeleeHit"),
    (0x2000, "Weapon"),
    (0x10000, "Axe"),
    (0x20000, "Bow"),
    (0x40000, "Claw"),
    (0x80000, "Dagger"),
    (0x100000, "Mace"),
    (0x200000, "Staff"),
    (0x400000, "Sword"),
    (0x800000, "Wand"),
    (0x1000000, "Unarmed"),
    (0x2000000, "Fishing"),
    (0x4000000, "Crossbow"),
    (0x8000000, "Flail"),
    (0x10000000, "Spear"),
    (0x20000000, "Warstaff"),
    (0x40000000, "Talisman"),
    (0x1_0000_0000, "WeaponMelee"),
    (0x2_0000_0000, "WeaponRanged"),
    (0x4_0000_0000, "Weapon1H"),
    (0x8_0000_0000, "Weapon2H"),
];

const KEYWORD_FLAG_NAMES: &[(u64, &str)] = &[
    (0x0000_0001, "Aura"),
    (0x0000_0002, "Curse"),
    (0x0000_0004, "Warcry"),
    (0x0000_0008, "Movement"),
    (0x0000_0010, "Physical"),
    (0x0000_0020, "Fire"),
    (0x0000_0040, "Cold"),
    (0x0000_0080, "Lightning"),
    (0x0000_0100, "Chaos"),
    (0x0000_0200, "Vaal"),
    (0x0000_0400, "Bow"),
    (0x0000_0800, "Arrow"),
    (0x0000_1000, "Trap"),
    (0x0000_2000, "Mine"),
    (0x0000_4000, "Totem"),
    (0x0000_8000, "Minion"),
    (0x0001_0000, "Attack"),
    (0x0002_0000, "Spell"),
    (0x0004_0000, "Hit"),
    (0x0008_0000, "Ailment"),
    (0x0010_0000, "Brand"),
    (0x0020_0000, "Poison"),
    (0x0040_0000, "Bleed"),
    (0x0080_0000, "Ignite"),
    (0x0100_0000, "PhysicalDot"),
    (0x0200_0000, "LightningDot"),
    (0x0400_0000, "ColdDot"),
    (0x0800_0000, "FireDot"),
    (0x1000_0000, "ChaosDot"),
];

fn flag_names(bits: u64, table: &[(u64, &str)]) -> Vec<String> {
    let mut names: Vec<String> = table
        .iter()
        .filter(|(bit, _)| bits & bit == *bit && *bit != 0)
        .map(|(_, name)| (*name).to_string())
        .collect();
    names.sort();
    names
}

fn pobr_mod_to_canon(m: &Modifier) -> CanonMod {
    CanonMod {
        name: m.name.as_str().to_string(),
        mod_type: mod_type_label(m.mod_type).to_string(),
        value: value_to_canon(&m.value),
        flags: flag_names(m.flags.bits(), MOD_FLAG_NAMES),
        keyword_flags: flag_names(m.keyword_flags.bits(), KEYWORD_FLAG_NAMES),
        tag_count: m.tags.len(),
    }
}

fn golden_value_to_canon(v: Option<&Value>) -> String {
    match v {
        Some(Value::Number(n)) => {
            if let Some(i) = n.as_i64() {
                format!("{i}")
            } else if let Some(f) = n.as_f64() {
                num_to_canon(f)
            } else {
                n.to_string()
            }
        }
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::String(s)) => format!("{s:?}"),
        Some(Value::Array(a)) => format!("nested:{}", a.len()),
        Some(Value::Object(_)) => "nested:1".to_string(),
        Some(Value::Null) | None => "null".to_string(),
    }
}

fn str_array(v: Option<&Value>) -> Vec<String> {
    let mut out: Vec<String> = v
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

fn golden_mod_to_canon(m: &Value) -> CanonMod {
    let obj = m.as_object().cloned().unwrap_or_else(Map::new);
    let tag_count = obj
        .get("tags")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    CanonMod {
        name: obj
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        mod_type: obj
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        value: golden_value_to_canon(obj.get("value")),
        flags: str_array(obj.get("flags")),
        keyword_flags: str_array(obj.get("keywordFlags")),
        tag_count,
    }
}

fn sorted_canon(mut v: Vec<CanonMod>) -> Vec<CanonMod> {
    v.sort();
    v
}

fn canon_lists_eq(a: &[CanonMod], b: &[CanonMod]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.core_eq(y))
}

// ---------------------------------------------------------------------------
// 五态报告（§11.3 契约 5：B 的 diff 修复与 F 验收的共同输入）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Eq,
    Diff,
    Miss,
    Extra,
    Unsup,
}

impl State {
    fn as_str(self) -> &'static str {
        match self {
            State::Eq => "eq",
            State::Diff => "diff",
            State::Miss => "miss",
            State::Extra => "extra",
            State::Unsup => "unsup",
        }
    }
}

#[derive(Debug, Default)]
struct StateCounts {
    eq: usize,
    diff: usize,
    miss: usize,
    extra: usize,
    unsup: usize,
}

impl StateCounts {
    fn total(&self) -> usize {
        self.eq + self.diff + self.miss + self.extra + self.unsup
    }
    fn bump(&mut self, s: State) {
        match s {
            State::Eq => self.eq += 1,
            State::Diff => self.diff += 1,
            State::Miss => self.miss += 1,
            State::Extra => self.extra += 1,
            State::Unsup => self.unsup += 1,
        }
    }
    fn to_json(&self) -> Value {
        json!({
            "eq": self.eq, "diff": self.diff, "miss": self.miss,
            "extra": self.extra, "unsup": self.unsup,
        })
    }
}

/// 单条对拍明细（DIFF/MISS/EXTRA 入明细，EQ/UNSUP 仅计数）。
struct DiffDetail {
    text: String,
    state: &'static str,
    pobr: Vec<CanonMod>,
    golden: Vec<CanonMod>,
    /// pobr_tag_total - golden_tag_total（tag 形态差异提示，见模块文档）。
    tag_delta: i64,
}

impl DiffDetail {
    fn to_json(&self) -> Value {
        json!({
            "text": self.text,
            "state": self.state,
            "pobr": self.pobr.iter().map(CanonMod::to_json).collect::<Vec<_>>(),
            "golden": self.golden.iter().map(CanonMod::to_json).collect::<Vec<_>>(),
            "tag_delta": self.tag_delta,
        })
    }
}

/// 五态报告顶层 schema（契约 5），写 `target/parser-modcache-diff-report.json`：
///
/// ```jsonc
/// {
///   "schema": "parser-modcache-diff/v1",
///   "engine": "engine",
///   "golden": { "source", "total", "parsed", "unsupported" },
///   "counts": { "eq", "diff", "miss", "extra", "unsup" },
///   "hit_rate": 0.0..1.0,         // eq / golden.parsed
///   "miss_forms": { "<vendor 首 mod name>": count, ... },  // MISS 缺口分组
///   "details": [ { "text", "state", "pobr":[CanonMod], "golden":[CanonMod], "tag_delta" } ]
/// }
/// ```
/// 其中 `CanonMod = { name, type, value, flags[], keywordFlags[], tag_count }`。
struct DiffReport {
    counts: StateCounts,
    golden_total: usize,
    golden_parsed: usize,
    golden_unsupported: usize,
    hit_rate: f64,
    miss_forms: BTreeMap<String, usize>,
    details: Vec<DiffDetail>,
}

impl DiffReport {
    fn to_json(&self) -> Value {
        json!({
            "schema": "parser-modcache-diff/v1",
            "engine": "engine",
            "golden": {
                "source": "vendor Data/ModCache.lua",
                "total": self.golden_total,
                "parsed": self.golden_parsed,
                "unsupported": self.golden_unsupported,
            },
            "counts": self.counts.to_json(),
            "hit_rate": self.hit_rate,
            "miss_forms": Value::Object(
                self.miss_forms
                    .iter()
                    .map(|(k, v)| (k.clone(), json!(v)))
                    .collect(),
            ),
            "details": self.details.iter().map(DiffDetail::to_json).collect::<Vec<_>>(),
        })
    }
}

// ---------------------------------------------------------------------------
// 对拍主流程
// ---------------------------------------------------------------------------

fn run_differential(doc: &GoldenDoc) -> DiffReport {
    let mut counts = StateCounts::default();
    let mut details = Vec::new();
    let mut miss_forms: BTreeMap<String, usize> = BTreeMap::new();

    for entry in &doc.entries {
        let outcome = parse_mod(&entry.text);
        let (pobr_parsed, pobr_canon) = match outcome {
            Ok(o) if o.status == ParseStatus::Parsed && !o.mods.is_empty() => (
                true,
                sorted_canon(o.mods.iter().map(pobr_mod_to_canon).collect()),
            ),
            _ => (false, Vec::new()),
        };

        let state = match (entry.parsed, pobr_parsed) {
            (true, true) => {
                if canon_lists_eq(&pobr_canon, &entry.canon) {
                    State::Eq
                } else {
                    State::Diff
                }
            }
            (true, false) => State::Miss,
            (false, true) => State::Extra,
            (false, false) => State::Unsup,
        };
        counts.bump(state);

        if state == State::Miss {
            let form = entry
                .canon
                .first()
                .map(|m| m.name.clone())
                .unwrap_or_else(|| "<unknown>".to_string());
            *miss_forms.entry(form).or_insert(0) += 1;
        }

        if matches!(state, State::Diff | State::Miss | State::Extra) {
            let pobr_tags: usize = pobr_canon.iter().map(|m| m.tag_count).sum();
            let golden_tags: usize = entry.canon.iter().map(|m| m.tag_count).sum();
            details.push(DiffDetail {
                text: entry.text.clone(),
                state: state.as_str(),
                pobr: pobr_canon,
                golden: entry.canon.clone(),
                tag_delta: pobr_tags as i64 - golden_tags as i64,
            });
        }
    }

    let golden_parsed_total = doc.entries.iter().filter(|e| e.parsed).count();
    let hit_rate = if golden_parsed_total == 0 {
        0.0
    } else {
        counts.eq as f64 / golden_parsed_total as f64
    };

    DiffReport {
        counts,
        golden_total: doc.meta_total,
        golden_parsed: doc.meta_parsed,
        golden_unsupported: doc.meta_unsupported,
        hit_rate,
        miss_forms,
        details,
    }
}

fn write_report(report: &DiffReport) -> PathBuf {
    let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target");
    let _ = fs::create_dir_all(&out_dir);
    let path = out_dir.join("parser-modcache-diff-report.json");
    let json = serde_json::to_string_pretty(&report.to_json()).expect("serialize report");
    fs::write(&path, json).expect("write report");
    path
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

/// golden fixture 自洽：meta 计数与 entries 状态一致（落盘正确性守门）。
#[test]
fn golden_fixture_is_self_consistent() {
    let doc = load_golden();
    assert_eq!(doc.entries.len(), doc.meta_total, "entries vs meta.total");
    let parsed = doc.entries.iter().filter(|e| e.parsed).count();
    let unsup = doc.entries.len() - parsed;
    assert_eq!(parsed, doc.meta_parsed, "parsed count vs meta.parsed");
    assert_eq!(unsup, doc.meta_unsupported, "unsupported count vs meta");
    assert!(
        doc.meta_total > 1000,
        "golden should be the full ModCache dump"
    );
}

/// 引擎对 golden 的五态对拍 + 报告落盘 + 计数自洽。
///
/// **不设命中率硬阈值**——引擎对全 ModCache 语料覆盖本就部分，硬门禁由
/// parity 套件承担。本测试守住基座可运行性并把命中率打到 stderr
/// （`--nocapture` 可见），供缺口收敛跟踪。
#[test]
fn engine_modcache_differential() {
    let doc = load_golden();
    let report = run_differential(&doc);

    assert_eq!(
        report.counts.total(),
        doc.entries.len(),
        "five-state counts must cover every entry exactly once"
    );

    let path = write_report(&report);

    let c = &report.counts;
    eprintln!("\n=== ModCache golden differential (engine) ===");
    eprintln!(
        "golden: {} entries ({} parsed / {} unsupported)",
        report.golden_total, report.golden_parsed, report.golden_unsupported
    );
    eprintln!(
        "five-state: eq={} diff={} miss={} extra={} unsup={}",
        c.eq, c.diff, c.miss, c.extra, c.unsup
    );
    eprintln!(
        "hit_rate (eq / golden.parsed) = {:.4} ({}/{})",
        report.hit_rate, c.eq, report.golden_parsed
    );
    eprintln!("MISS top forms:");
    let mut top: Vec<(&String, &usize)> = report.miss_forms.iter().collect();
    top.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (name, n) in top.into_iter().take(15) {
        eprintln!("  {n:>4}  {name}");
    }
    eprintln!("report -> {}", path.display());

    assert!(
        c.eq + c.diff + c.miss > 0,
        "differential produced no signal"
    );
}
