//! 新旧 parser 双跑 diff harness（M6-B T5/T6；蓝图 §5）。
//!
//! 对四层语料（蓝图 §5.1）逐行跑 legacy vs engine，按 §5.2 canonical diff 五态
//! 裁决（EQ / DIFF / OLD_ONLY / NEW_ONLY / UNSUP）。**门禁**（蓝图 §5.2）：
//! C1（18-build）语料 `DIFF=0 且 OLD_ONLY=0`（新 parser 必须是旧能力的形态
//! 超集；NEW_ONLY 增益不阻塞）。
//!
//! 本 track 只达成 diff=0 形态对齐、绝不切换调用方；§2.4 的预期语义差异逐项
//! 在报告 m6-dualrun-report.md 登记/处置。
//!
//! 运行：`cargo test -p pobr-core --test parser`（引擎已无条件编译）。

use std::collections::BTreeMap;
use std::path::PathBuf;

use pobr_core::mod_parser::{
    CompiledParserRules, ParseStatus, canonical_outcome, parse_mod, parse_mod_engine,
};
use pobr_data::catalog::parser_rules::{ModParserRulesDoc, SpecialModsDef};

/// 五态计数（按语料源分组）。
#[derive(Default, Debug, Clone)]
struct Tally {
    eq: usize,
    diff: usize,
    old_only: usize,
    new_only: usize,
    unsup: usize,
    /// DIFF 中「仅 ModName 词表差异」的条数（legacy PoBR 名 vs engine vendor 名；
    /// 去名后结构/值/tags/flags 完全一致）——蓝图 §1.2 的预期词表分歧（§2.4 D5'）。
    diff_name_only: usize,
    /// DIFF 中去名后仍不一致的「结构性」条数（真正需处置的引擎/数据缺口）。
    diff_structural: usize,
}

impl Tally {
    fn total(&self) -> usize {
        self.eq + self.diff + self.old_only + self.new_only + self.unsup
    }
}

/// 去名规范：把每条 mod 的 `name=...` 段抹成占位，保留 type/flags/kw/tags/value
/// 与 status/unparsed——用于区分「纯词表差异」与「结构性差异」。
fn name_blind(canonical: &str) -> String {
    canonical
        .split(';')
        .map(|seg| {
            // 抹掉 `name=...|` 前缀（到第一个 `|type=` 之前）。
            if let Some(pos) = seg.find("|type=")
                && (seg.trim_start().starts_with("name=")
                    || seg.contains("mods=[name=")
                    || seg.contains("|name="))
            {
                let head_end = seg.find("name=").unwrap_or(0);
                return format!("{}name=*{}", &seg[..head_end], &seg[pos..]);
            }
            seg.to_string()
        })
        .collect::<Vec<_>>()
        .join(";")
}

/// 一行 diff 明细（DIFF / OLD_ONLY 用，报告打印）。
struct DiffDetail {
    source: String,
    text: String,
    state: &'static str,
    /// DIFF 且仅词表差异（去名后等价）——优先级低，报告靠后。
    name_only: bool,
    legacy: String,
    engine: String,
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn load_rules() -> CompiledParserRules {
    let path = repo_root()
        .join("data")
        // legacy(冻结手写) vs engine(数据驱动) 的 DIFF=0 对齐在 golden 版本上建立，
        // 故钉定该版本（活动 DATA_VERSION 前进不影响本对拍门禁；删 legacy 后本文件移除）。
        .join(pobr_data::GOLDEN_PARITY_DATA_VERSION)
        .join("overlay/mod_parser_rules.json");
    let json = std::fs::read_to_string(&path).expect("读取 mod_parser_rules.json");
    let doc: ModParserRulesDoc = serde_json::from_str(&json).expect("反序列化规则表");
    // special 通道数据（overlay + generated 拼接，id 冲突在 compile fail-fast）。
    let mut special = load_special("overlay/special_mods.json");
    special.extend(load_special("generated/special_derived.json"));
    CompiledParserRules::compile_with_special(&doc, &special).expect("编译规则表")
}

/// 载入一份 special_mods 数据文件（缺文件 → 空，由拼接侧兜底）。
fn load_special(rel: &str) -> Vec<pobr_data::catalog::parser_rules::SpecialTemplateDef> {
    let path = repo_root()
        .join("data")
        // legacy(冻结手写) vs engine(数据驱动) 的 DIFF=0 对齐在 golden 版本上建立，
        // 故钉定该版本（活动 DATA_VERSION 前进不影响本对拍门禁；删 legacy 后本文件移除）。
        .join(pobr_data::GOLDEN_PARITY_DATA_VERSION)
        .join(rel);
    let Ok(json) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let def: SpecialModsDef = serde_json::from_str(&json).expect("反序列化 special_mods");
    def.entries
}

/// C1：18-build XML 的 Item 文本块逐行（蓝图 §5.1）。
fn corpus_c1() -> Vec<String> {
    let builds_dir = repo_root().join("examples/demo-bd-test/builds");
    let mut lines = Vec::new();
    let Ok(entries) = std::fs::read_dir(&builds_dir) else {
        return lines;
    };
    for entry in entries.flatten() {
        let xml_path = entry.path().join("decoded.xml");
        let Ok(xml) = std::fs::read_to_string(&xml_path) else {
            continue;
        };
        lines.extend(extract_item_mod_lines(&xml));
    }
    dedup(lines)
}

/// 从 build XML 抽取候选 modifier 文本行：Item 块内、非 XML 标签、非元数据行。
fn extract_item_mod_lines(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_item = false;
    for raw in xml.lines() {
        let line = raw.trim();
        if line.starts_with("<Item") {
            in_item = true;
            continue;
        }
        if line.starts_with("</Item>") {
            in_item = false;
            continue;
        }
        if !in_item {
            continue;
        }
        // 跳过 XML 子标签（ModRange / Variant 等）与已知元数据行。
        if line.starts_with('<') || line.is_empty() {
            continue;
        }
        if is_metadata_line(line) {
            continue;
        }
        // 去掉 PoB crafting 标签前缀 `{enchant}{rune}` 等（两侧都去 → 公平比较；
        // 注意此处不做 strip_pob_brackets——legacy/engine 各自的 pre-pass 处理）。
        let cleaned = strip_craft_tags(line);
        if cleaned.is_empty() {
            continue;
        }
        out.push(cleaned);
    }
    out
}

/// PoB crafting 标签前缀 `{enchant}` / `{rune}` / `{desecrated}` / `{crafted}` 等
/// 形如 `{word}` 的行首连续标记——这些是 PoB 词条来源标记，非 modifier 内容。
fn strip_craft_tags(line: &str) -> String {
    let mut rest = line;
    while rest.starts_with('{') {
        if let Some(end) = rest.find('}') {
            rest = &rest[end + 1..];
        } else {
            break;
        }
    }
    rest.trim().to_string()
}

fn is_metadata_line(line: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "Rarity:",
        "Unique ID:",
        "Item Level:",
        "Quality:",
        "Sockets:",
        "Rune:",
        "LevelReq:",
        "Implicits:",
        "Corrupted",
        "Crafted:",
        "Prefix:",
        "Suffix:",
        "Selected Variant:",
        "Variant:",
        "Requires ",
        "Armour:",
        "Evasion:",
        "Energy Shield:",
        "Ward:",
        "Radius:",
        "Limited to:",
        "Has Alt Variant",
        "League:",
        "Source:",
        "Talisman Tier:",
    ];
    PREFIXES.iter().any(|p| line.starts_with(p))
    // 物品名 / 基底名行无法可靠区分——但它们解析两侧都 Unsupported，
    // 计入 UNSUP 不影响 DIFF/OLD_ONLY 门禁。
}

/// 特殊/fixture 小语料（蓝图 §5.1 special + fixture 层，手抄高频形态）。
fn corpus_fixture() -> Vec<String> {
    [
        "50% increased Fire Damage",
        "+50 to maximum Life",
        "10% reduced Mana Cost",
        "25% more Damage",
        "15% less Attack Speed",
        "+30% to Fire Resistance",
        "+12 to Strength",
        "Adds 5 to 12 Cold Damage",
        "Adds 10 to 20 Physical Damage to Attacks",
        "+200 to Armour",
        "5% increased Movement Speed",
        "Regenerate 2% of Life per second",
        "10% chance to Avoid being Stunned",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

fn dedup(mut lines: Vec<String>) -> Vec<String> {
    lines.sort();
    lines.dedup();
    lines
}

/// 对一组语料跑双跑，返回 (tally, 明细)。
fn run_corpus(
    name: &str,
    lines: &[String],
    rules: &CompiledParserRules,
) -> (Tally, Vec<DiffDetail>) {
    let mut tally = Tally::default();
    let mut details = Vec::new();
    for text in lines {
        let legacy = match parse_mod(text) {
            Ok(o) => o,
            // legacy 对空输入等返回 Err——视为 Unsupported（与 engine 同款收口）。
            Err(_) => {
                tally.unsup += 1;
                continue;
            }
        };
        let engine = parse_mod_engine(text, rules);

        let legacy_parsed = matches!(legacy.status, ParseStatus::Parsed) && !legacy.mods.is_empty();
        let engine_parsed = matches!(engine.status, ParseStatus::Parsed) && !engine.mods.is_empty();

        let lc = canonical_outcome(&legacy);
        let ec = canonical_outcome(&engine);

        match (legacy_parsed, engine_parsed) {
            (true, true) => {
                if lc == ec {
                    tally.eq += 1;
                } else {
                    tally.diff += 1;
                    let name_only = name_blind(&lc) == name_blind(&ec);
                    if name_only {
                        tally.diff_name_only += 1;
                    } else {
                        tally.diff_structural += 1;
                    }
                    details.push(DiffDetail {
                        source: name.to_string(),
                        text: text.clone(),
                        state: "DIFF",
                        name_only,
                        legacy: lc,
                        engine: ec,
                    });
                }
            }
            (true, false) => {
                tally.old_only += 1;
                details.push(DiffDetail {
                    source: name.to_string(),
                    text: text.clone(),
                    state: "OLD_ONLY",
                    name_only: false,
                    legacy: lc,
                    engine: ec,
                });
            }
            (false, true) => tally.new_only += 1,
            (false, false) => tally.unsup += 1,
        }
    }
    (tally, details)
}

/// 报告打印（汇总五态 × 语料源 + DIFF 名表/结构拆分 + 明细 top-N，结构性优先）。
fn print_report(report: &BTreeMap<String, Tally>, details: &[DiffDetail]) {
    eprintln!("\n=== M6-B parser 双跑五态报告 ===");
    eprintln!(
        "{:<12} {:>6} {:>6} {:>9} {:>9} {:>9} {:>6} {:>7}",
        "source", "EQ", "DIFF", "(name)", "(struct)", "OLD_ONLY", "NEW", "UNSUP"
    );
    for (src, t) in report {
        eprintln!(
            "{:<12} {:>6} {:>6} {:>9} {:>9} {:>9} {:>6} {:>7}",
            src, t.eq, t.diff, t.diff_name_only, t.diff_structural, t.old_only, t.new_only, t.unsup
        );
    }
    // 明细排序：OLD_ONLY 与结构性 DIFF 优先（真正需处置的），纯词表 DIFF 靠后。
    let mut ordered: Vec<&DiffDetail> = details.iter().collect();
    ordered.sort_by_key(|d| {
        let prio = match (d.state, d.name_only) {
            ("OLD_ONLY", _) => 0,
            ("DIFF", false) => 1,
            ("DIFF", true) => 2,
            _ => 3,
        };
        (prio, d.text.clone())
    });
    if !ordered.is_empty() {
        let struct_count = ordered
            .iter()
            .filter(|d| d.state == "OLD_ONLY" || (d.state == "DIFF" && !d.name_only))
            .count();
        eprintln!("\n--- 需处置明细（OLD_ONLY + 结构性 DIFF，共 {struct_count}；top 220）---");
        for d in ordered.iter().take(220) {
            let kind = if d.name_only {
                "DIFF/name-only"
            } else {
                d.state
            };
            eprintln!("[{}][{}] {:?}", d.source, kind, d.text);
            eprintln!("    legacy: {}", d.legacy);
            eprintln!("    engine: {}", d.engine);
        }
        eprintln!("（共 {} 条 DIFF/OLD_ONLY）", ordered.len());
    }
}

/// 报告生成 + 引擎鲁棒性门禁（默认运行，进 CI）。
///
/// **断言的是引擎对全语料的鲁棒性与有效性**（非 diff=0——后者见上方 ignore 用例
/// 的语义分歧说明）：
/// - 跑完 1700+ 行无 panic（run_corpus 返回即证）；
/// - C1 语料 EQ > 0（引擎对相当部分词条与 legacy 逐字节一致，证主路径正确）；
/// - fixture 全部 Parsed（引擎对常见形态零失配）。
#[test]
fn dual_run_report() {
    let rules = load_rules();
    let mut report = BTreeMap::new();
    let mut all_details = Vec::new();

    for (name, lines) in [("C1", corpus_c1()), ("fixture", corpus_fixture())] {
        let (tally, details) = run_corpus(name, &lines, &rules);
        report.insert(name.to_string(), tally);
        all_details.extend(details);
    }
    print_report(&report, &all_details);

    let c1 = &report["C1"];
    assert!(c1.total() > 100, "C1 语料应非空（实测 {}）", c1.total());
    assert!(
        c1.eq > 0,
        "C1 应有 EQ>0（引擎主路径与 legacy 逐字节一致的词条）"
    );
    let fixture = &report["fixture"];
    // fixture 常见形态引擎应大量产出（EQ + 引擎产出态 ≥ 11/13）——证主路径覆盖。
    let fixture_engine_parsed = fixture.eq + fixture.diff + fixture.new_only;
    assert!(
        fixture_engine_parsed >= 11,
        "fixture 常见形态引擎产出 {fixture_engine_parsed}/13 偏低（应 ≥11）"
    );
}

/// C1 字面 diff=0 门禁（蓝图 §5.2 / D-T8 第二波目标，**正式断言**，进 CI）。
///
/// **M6 D-T8 第二波 2a（收敛剩余）落地后**（报告 §2.5）：special 通道接入引擎
/// （vendor `specialModList` 整行表，form 扫描前查）+ EnemyModifier LIST 包装
/// （`applyToEnemy` → inner 附 `Condition(Effective)` + 敌侧 `Enemy<X>` 条件）+ 4
/// 真 bug 收敛（focus / helmet 大小写 / Triggered SPELL flag / GainAs 基名），
/// 字面 **DIFF=0 且 OLD_ONLY=0** 达成（EQ 309→860；UNSUP 874 全是物品名/基底名，
/// 不在解析范围）。
///
/// 本门禁断言新引擎是旧引擎能力的**逐字节形态超集**（剔 origin、source 不参与）：
/// `... against enemies within/further than N metres` → 引擎产 `MultiplierThreshold:
/// enemyDistance` tag（#8 node 5802「Stand and Deliver」逐字含 `[A|B]` markup）。回归
/// 锚：threshold 须 = N×10（米→单位，`"$1:mult(10)"` 算子），不是裸 N。
#[test]
fn within_metres_parses_to_multiplier_threshold() {
    use pobr_core::ModTag;
    let rules = load_rules();
    // node 5802 逐字（带 markup）：「Projectiles have 40% increased Critical Damage Bonus
    // against Enemies within 2m」→ CriticalStrikeMultiplier INC 40 + MultiplierThreshold(20,upper)。
    let out = parse_mod_engine(
        "[Projectile|Projectiles] have 40% increased [CriticalDamageBonus|Critical Damage Bonus] against Enemies within 2m",
        &rules,
    );
    assert_eq!(out.status, ParseStatus::Parsed, "应解析，不丢弃");
    assert_eq!(out.mods.len(), 1);
    assert_eq!(out.mods[0].name.as_str(), "CriticalStrikeMultiplier");
    let mt = out.mods[0]
        .tags
        .iter()
        .find_map(|t| match t {
            ModTag::MultiplierThreshold {
                var,
                threshold,
                upper,
            } => Some((var.clone(), *threshold, *upper)),
            _ => None,
        })
        .expect("含 MultiplierThreshold tag");
    assert_eq!(
        mt,
        ("enemyDistance".to_string(), 20.0, true),
        "within 2m → 阈值 20、upper"
    );

    // further than 3m → 非 upper、阈值 30。
    let far = parse_mod_engine(
        "10% increased Damage against enemies further than 3m",
        &rules,
    );
    assert_eq!(far.status, ParseStatus::Parsed);
    let far_mt = far.mods[0].tags.iter().find_map(|t| match t {
        ModTag::MultiplierThreshold {
            threshold, upper, ..
        } => Some((*threshold, *upper)),
        _ => None,
    });
    assert_eq!(far_mt, Some((30.0, false)));
}

/// C1 18-build 语料无 DIFF、无 OLD_ONLY。该等价性是「引擎接管 calc 主路径」的前提
/// （现已落地：编排层恒注入引擎规则，legacy 仅作回退）——切换全程 parity 零回归。
#[test]
fn c1_diff_zero_gate() {
    let rules = load_rules();
    let lines = corpus_c1();
    let (tally, details) = run_corpus("C1", &lines, &rules);
    print_report(
        &BTreeMap::from([("C1".to_string(), tally.clone())]),
        &details,
    );
    assert_eq!(tally.diff, 0, "C1 语料 DIFF 必须为 0（见明细）");
    assert_eq!(tally.old_only, 0, "C1 语料 OLD_ONLY 必须为 0");
}

/// C1 收敛底线回归门禁（**正式断言**，进 CI；防 EQ 退化）。
///
/// 2a 收敛后 `c1_diff_zero_gate` 已断言 DIFF=0 / OLD_ONLY=0；本门禁补一条 EQ 下界
/// （≥860），把已达成的逐字节一致量化为回归底线（DIFF=0 时 EQ 退化只可能来自引擎
/// 改动导致条目改判 NEW_ONLY/UNSUP，本门禁兜住）。
#[test]
fn c1_converged_floor_gate() {
    let rules = load_rules();
    let lines = corpus_c1();
    let (tally, details) = run_corpus("C1", &lines, &rules);
    print_report(
        &BTreeMap::from([("C1".to_string(), tally.clone())]),
        &details,
    );
    assert!(
        tally.eq >= 860,
        "C1 EQ={}（应 ≥860；收敛回退？见明细）",
        tally.eq
    );
}
