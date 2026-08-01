//! 数据前置的加载测试：`overlay/special_mods.json`
//! （schema 见 [`pobr_data::catalog::parser_rules`]）。
//!
//! 只验数据形状与策展纪律（id 唯一 / verified:false / batch 标记 /
//! mods 与 handler 互斥形态）；regex 编译校验在 sync-pob-catalog 测试侧
//! （该 crate 有 regex 依赖），解释器接入归B-2/B-3。

use pobr_data::catalog::parser_rules::{
    SpecialModsDef, TemplateNameDef, TemplateValueDef, ValueOpDef,
};
use pobr_gamedata::{GameData, repo_data_root};

fn version() -> String {
    pobr_gamedata::data_version()
}

fn load() -> SpecialModsDef {
    GameData::new(repo_data_root().join(version()))
        .special_mods()
        .unwrap()
        .expect("special_mods.json 在库")
}

/// 首批形状**不变量**(非快照):非空且非平凡填充 + id 唯一 + 含 S0 段 + 批次标签非空 +
/// 首批 verified:false + handler_id 命名规范。**不钉精确条目数 / 排序 / 批次枚举**——
/// special_mods 随长尾补全增长(71→78→86→…)且新策展波次会引入新批次标签,钉死这些
/// 快照量会让数据增长误红,而它们都不是计算逻辑性质。
#[test]
fn first_batch_shape() {
    let def = load();
    assert!(
        def.entries.len() >= 50,
        "special_mods 条目数 {} 偏低(疑加载截断)",
        def.entries.len()
    );
    // 不变量是 id **唯一**(重复 id ⇒ 查表二义,真 bug)；不要求 vec 已排序——
    // 排序是策展美观,手迁的"整行"条目按批次追加(非字母序),钉死排序会误红。
    let mut seen = std::collections::HashSet::new();
    for e in &def.entries {
        assert!(seen.insert(e.id.as_str()), "{}: 重复 id", e.id);
    }
    assert!(
        def.entries.iter().any(|e| e.batch == "S0"),
        "应含 S0 keystone 段"
    );
    for e in &def.entries {
        // batch 不变量是**非空 provenance 标签**(标明哪一波策展加入)；不钉固定枚举
        // {S0,S1,S2}——后续策展波次(如 fork-a 整行迁移)会引入新标签,钉死枚举会误红。
        assert!(
            !e.batch.trim().is_empty(),
            "{}: batch provenance 标签不得为空",
            e.id
        );
        // verified 不再断言恒 false：策展纪律（docs/contributing-mods.md §7）允许
        // oracle 对拍确认后置 true（4.5.4.3 移植的 hand-curated 条目如
        // take_pct_less_damage 等即为已对拍 verified:true）。
        if let Some(id) = &e.handler_id {
            assert!(
                id.starts_with("special:"),
                "{}: handler_id 命名应为 `special:<name>`（实 {id}）",
                e.id
            );
        }
    }
}

/// S0 keystone 段抽查（准源 = pobr parse_keystone_special 现状）：
/// OVERRIDE 捕获直引（`Your Critical Damage Bonus is N%`）。
#[test]
fn s0_crit_override_entry() {
    let def = load();
    let e = def
        .entries
        .iter()
        .find(|e| e.id == "your_critical_damage_bonus_override")
        .expect("S0 条目在库");
    assert_eq!(
        e.vendor_pattern.as_deref(),
        Some("your critical damage bonus is (%d+)%%")
    );
    assert_eq!(e.mods.len(), 1);
    let m = &e.mods[0];
    assert_eq!(
        m.name,
        TemplateNameDef::Literal("CriticalStrikeMultiplier".to_string())
    );
    assert_eq!(m.mod_type, "OVERRIDE");
    assert_eq!(m.value, TemplateValueDef::Capture("$1".to_string()));
}

/// S0 纯识别条目（mods 与 handler 都缺 = 已知不支持，pobr 现状 Unsupported）。
#[test]
fn s0_recognition_only_entries() {
    let def = load();
    for id in [
        "immune_to_chaos_damage",
        "immune_to_chaos_damage_and_bleeding",
    ] {
        let e = def.entries.iter().find(|e| e.id == id).unwrap();
        assert!(
            e.mods.is_empty() && e.handler_id.is_none(),
            "{id}: 纯识别形态"
        );
        assert!(e.source_note.is_some(), "{id}: 必须注明 vendor 实际语义");
    }
}

/// 算子链条目：`N% reduced Movement Speed Penalty ...` → negate
/// （vendor ModParser.lua:6017，INC 取负——五算子白名单内）。
#[test]
fn value_expr_negate_entry() {
    let def = load();
    let e = def
        .entries
        .iter()
        .find(|e| e.id == "reduced_movement_speed_penalty_from_using_skills_while_moving")
        .expect("negate 条目在库");
    let TemplateValueDef::Expr(expr) = &e.mods[0].value else {
        panic!("应为带算子链表达式");
    };
    assert_eq!(expr.capture, "$1");
    assert_eq!(expr.ops, vec![ValueOpDef::Negate {}]);
}

/// 非 S0 条目带 vendor 溯源**不变量**(非快照)：携带 `vendor_pattern`(对账输入) +
/// 非空 `source_note`(provenance) + 产物形态(mod 或 handler)。
/// **不钉 `source_note` 文本格式**——自动转写条目锚到 `ModParser.lua:<行号>`,
/// 但手迁的"整行"条目(Herald / armour-applies-chaos 等)语义跨多行、无单一行号,
/// 钉死 `ModParser.lua:` 字面会把合法的人工策展条目误红;provenance 的不变量是
/// "有可追溯说明",而非"必含某行号格式"。
#[test]
fn non_s0_has_vendor_provenance() {
    let def = load();
    for e in def.entries.iter().filter(|e| e.batch != "S0") {
        assert!(e.vendor_pattern.is_some(), "{}: 缺 vendor_pattern", e.id);
        assert!(
            e.source_note
                .as_deref()
                .is_some_and(|n| !n.trim().is_empty()),
            "{}: 缺 source_note provenance",
            e.id
        );
        // 模板条目必须产 mod；handler 条目（开放捕获走 handler_id）产物在 Rust 侧
        // （HandlerOutcome），数据 mods 空，豁免本检查。
        assert!(
            !e.mods.is_empty() || e.handler_id.is_some(),
            "{}: 自动转写条目必须产 mod 或挂 handler_id",
            e.id
        );
    }
}

/// 禁开放捕获（DSL 硬边界）：**模板**条目 pattern 不得含 `(.+)` 等开放捕获——
/// 词类捕获在转写时已内联为显式闭集。开放捕获条目按架构 §5 / DSL 边界**走
/// `handler_id`**（如 `allocates (.+)` → `special:granted_passive`，文本名经
/// `HandlerCtx::raw_captures` 透传），故 handler 条目豁免本检查。
#[test]
fn no_open_captures_in_patterns() {
    let def = load();
    for e in &def.entries {
        if e.handler_id.is_some() {
            continue; // 开放捕获条目走 handler（DSL 边界明示放行）。
        }
        assert!(
            !e.pattern.contains("(.+)") && !e.pattern.contains("(.*)"),
            "{}: 开放捕获越界（应走 handler_id）",
            e.id
        );
    }
}

// 版本无关策展层（data/overlay-common/）两层合并行为（P1-3）
//
// 在临时目录里手搭两层，精确验证合并语义（repo 真数据两层 id 互斥，无 override 样本）：
//   <tmp>/overlay-common/special_mods.json  ← 版本无关基底
//   <tmp>/<ver>/overlay/special_mods.json   ← 版本特有覆盖

/// 建唯一临时 data 根，返回 (根, 版本目录)。
fn temp_data_root(tag: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "pobr-gamedata-special-merge-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let ver_dir = root.join("4.5.4.3");
    std::fs::create_dir_all(ver_dir.join("overlay")).unwrap();
    (root, ver_dir)
}

/// 写一个只含给定 id 条目的 special_mods.json（最小合法形状）。
fn write_special(path: &std::path::Path, entries: &[(&str, &str)]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let body: Vec<String> = entries
        .iter()
        .map(|(id, pattern)| format!(r#"{{"id":"{id}","pattern":"{pattern}","batch":"T"}}"#))
        .collect();
    std::fs::write(path, format!(r#"{{"entries":[{}]}}"#, body.join(","))).unwrap();
}

fn ids(def: &SpecialModsDef) -> Vec<String> {
    def.entries.iter().map(|e| e.id.clone()).collect()
}

/// common 层被继承，且 version-only 条目追加在后（common 顺序在前）。
#[test]
fn overlay_common_is_inherited() {
    let (root, ver_dir) = temp_data_root("inherit");
    write_special(
        &root.join("overlay-common/special_mods.json"),
        &[("common_a", "aaa"), ("common_b", "bbb")],
    );
    write_special(
        &ver_dir.join("overlay/special_mods.json"),
        &[("ver_c", "ccc")],
    );
    let def = GameData::new(&ver_dir).special_mods().unwrap().unwrap();
    assert_eq!(ids(&def), ["common_a", "common_b", "ver_c"]);
}

/// version 层同 id 整条覆盖 common 层（不产生重复 id）。
#[test]
fn version_layer_overrides_common_by_id() {
    let (root, ver_dir) = temp_data_root("override");
    write_special(
        &root.join("overlay-common/special_mods.json"),
        &[("shared", "old_pattern"), ("common_only", "keep")],
    );
    write_special(
        &ver_dir.join("overlay/special_mods.json"),
        &[("shared", "new_pattern")],
    );
    let def = GameData::new(&ver_dir).special_mods().unwrap().unwrap();
    // 覆盖后仍只有一条 shared，取 version 的 pattern；common_only 保留。
    assert_eq!(ids(&def), ["shared", "common_only"]);
    let shared = def.entries.iter().find(|e| e.id == "shared").unwrap();
    assert_eq!(shared.pattern, "new_pattern");
}

/// 仅 common 存在（版本目录无 overlay/special_mods.json）→ 直接继承 common。
#[test]
fn common_only_when_version_absent() {
    let (root, ver_dir) = temp_data_root("common-only");
    write_special(
        &root.join("overlay-common/special_mods.json"),
        &[("common_x", "xxx")],
    );
    // 版本层文件缺失。
    let def = GameData::new(&ver_dir).special_mods().unwrap().unwrap();
    assert_eq!(ids(&def), ["common_x"]);
}

/// 两层皆缺 → None（保留 RuleSet 域缺表软降级语义）。
#[test]
fn both_layers_absent_yields_none() {
    let (_root, ver_dir) = temp_data_root("none");
    assert!(GameData::new(&ver_dir).special_mods().unwrap().is_none());
}
