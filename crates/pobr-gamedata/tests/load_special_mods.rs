//! pre-M5b 数据前置的加载测试：`overlay/special_mods.json`
//! （schema 见 [`pobr_data::catalog::parser_rules`]，M5b 蓝图 §2.1 契约）。
//!
//! 本波次只验数据形状与策展纪律（id 唯一 / verified:false / batch 标记 /
//! mods 与 handler 互斥形态）；regex 编译校验在 sync-pob-catalog 测试侧
//! （该 crate 有 regex 依赖），解释器接入归 M5b 主波 B-2/B-3。

use pobr_data::catalog::parser_rules::{
    SpecialModsDef, TemplateNameDef, TemplateValueDef, ValueOpDef,
};
use pobr_gamedata::{GameData, repo_data_root};

const VERSION: &str = "4.5.0.3.4";

fn load() -> SpecialModsDef {
    GameData::new(repo_data_root().join(VERSION))
        .special_mods()
        .unwrap()
        .expect("special_mods.json 在库")
}

/// 首批规模：S0 keystone-effect 段 8 条 + 自动转写 S1/S2 共 58 条 = 66 条；
/// id 唯一且升序。
///
/// **M5b B-4 消费激活后回滚**：原 107 条含 41 条降级 shadow（allocates_* 大小写
/// 失配 / 不可映射 tag 语义残缺 / target:enemy 误产玩家侧），逐条 oracle/generic
/// 对照归因后回滚（parity 零回归审查，见 B-4 commit）。剩余 66 条行为中性。
#[test]
fn first_batch_shape() {
    let def = load();
    assert_eq!(def.entries.len(), 66);
    assert!(
        def.entries.windows(2).all(|w| w[0].id < w[1].id),
        "id 严格升序（唯一）"
    );
    let s0 = def.entries.iter().filter(|e| e.batch == "S0").count();
    assert_eq!(s0, 8, "S0 keystone 段条目数");
    for e in &def.entries {
        assert!(
            matches!(e.batch.as_str(), "S0" | "S1" | "S2"),
            "{}: 非法批次 {}",
            e.id,
            e.batch
        );
        assert!(
            !e.verified,
            "{}: 首批必须 verified:false（oracle 对拍后才置 true）",
            e.id
        );
        assert!(
            e.handler_id.is_none(),
            "{}: 首批不含 handler 条目（handler 注册表接入归 M5b C-3）",
            e.id
        );
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

/// 自动转写条目带 vendor 对账锚点：S1/S2 全部携带 `vendor_pattern` 与
/// 行号 source_note（A-3 覆盖率对账 / 漂移告警的输入）。
#[test]
fn auto_batch_has_vendor_anchors() {
    let def = load();
    for e in def.entries.iter().filter(|e| e.batch != "S0") {
        assert!(e.vendor_pattern.is_some(), "{}: 缺 vendor_pattern", e.id);
        let note = e.source_note.as_deref().unwrap_or("");
        assert!(note.contains("ModParser.lua:"), "{}: 缺行号锚点", e.id);
        assert!(!e.mods.is_empty(), "{}: 自动转写条目必须产 mod", e.id);
    }
}

/// 禁开放捕获（DSL 硬边界）：pattern 不得含 `(.+)` 等开放捕获——
/// 词类捕获在转写时已内联为显式闭集。
#[test]
fn no_open_captures_in_patterns() {
    let def = load();
    for e in &def.entries {
        assert!(
            !e.pattern.contains("(.+)") && !e.pattern.contains("(.*)"),
            "{}: 开放捕获越界（应走 handler_id）",
            e.id
        );
    }
}
