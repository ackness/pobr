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

/// 首批规模：B-4 回滚后 66 条 + C-2 安全批次 5 条 = 71 条；M6-conv2（D-T8 第二波
/// 2a）+7 条 = 78 条（special 通道接入引擎后的 C1 收敛缺口：`allocates_passive` /
/// `defend_with_pct_of_armour` / `has_to_defence_per_player_level` /
/// `take_no_extra_damage_from_critical_hits` / `targets_can_be_affected_by_poisons` /
/// `empowered_attacks_deal_increased_damage` / `gain_pct_damage_as_extra_all_elements`）；
/// id 唯一且升序。
///
/// **M5b B-4 消费激活后回滚**：原 107 条含 41 条降级 shadow（allocates_* 大小写
/// 失配 / 不可映射 tag 语义残缺 / target:enemy 误产玩家侧），逐条 oracle/generic
/// 对照归因后回滚（parity 零回归审查，见 B-4 commit）。
#[test]
fn first_batch_shape() {
    let def = load();
    assert_eq!(def.entries.len(), 78);
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
        // M6-conv2：special 通道接入引擎后，开放捕获条目走 handler_id
        // （`allocates_passive` → `special:granted_passive`，文本名经 raw_captures
        // 透传）。handler 条目须注册（`all_handler_ids_registered` 闸门守）+ 占比
        // <10%（`handler_ratio_under_ten_percent`）；本处只校验 handler_id 命名规范。
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

/// 自动转写条目带 vendor 对账锚点：S1/S2 全部携带 `vendor_pattern` 与
/// 行号 source_note（A-3 覆盖率对账 / 漂移告警的输入）。
#[test]
fn auto_batch_has_vendor_anchors() {
    let def = load();
    for e in def.entries.iter().filter(|e| e.batch != "S0") {
        assert!(e.vendor_pattern.is_some(), "{}: 缺 vendor_pattern", e.id);
        let note = e.source_note.as_deref().unwrap_or("");
        assert!(note.contains("ModParser.lua:"), "{}: 缺行号锚点", e.id);
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
