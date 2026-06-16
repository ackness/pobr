//! M4-T5 W-E1 的加载测试：`overlay/trigger_configs.json`
//! （schema 见 [`pobr_data::catalog::triggers`]；蓝图 §4.1 T5 门禁
//! 「61 项抽取计数断言」的入库侧 + handler 计数监控）。

use pobr_data::catalog::triggers::TriggerConfigsDef;
use pobr_gamedata::{GameData, repo_data_root};

fn version() -> String {
    pobr_gamedata::data_version()
}

fn load() -> TriggerConfigsDef {
    GameData::new(repo_data_root().join(version()))
        .trigger_configs()
        .unwrap()
        .expect("trigger_configs.json 在库")
}

/// 61 项计数断言（drift 防线：vendor configTable 条目数 = 入库条目数）；
/// key 唯一且升序。
#[test]
fn sixty_one_entries_sorted_unique() {
    let def = load();
    assert_eq!(def.configs.len(), 61, "vendor configTable 61 项全量入库");
    assert!(
        def.configs
            .windows(2)
            .all(|w| w[0].key.name < w[1].key.name),
        "key.name 严格升序（唯一）"
    );
}

/// handler 条目纪律：`trigger:` 前缀 + 计数监控（20 号 §5 全阶段 <100 总闸；
/// 当前 15 条，变化须同步台账）。
#[test]
fn handler_discipline() {
    let def = load();
    let handlers: Vec<&str> = def
        .configs
        .iter()
        .filter_map(|c| c.handler_id.as_deref())
        .collect();
    assert_eq!(handlers.len(), 15, "handler 条目数变化须同步监控台账");
    assert!(handlers.len() < 100, "handler 总数超 20 号 §5 监控闸");
    for h in handlers {
        assert!(h.starts_with("trigger:"), "handler id {h} 缺 trigger: 前缀");
    }
}

/// 策展纪律：W-E1 落库全部 verified:false；每条带 vendor 行段锚点 +
/// 合法 kind；受限谓词三字段封顶（schema 即三字段，此处断言非空谓词有约束）。
#[test]
fn curation_discipline() {
    let def = load();
    for c in &def.configs {
        assert!(!c.verified, "{}: W-E1 落库必须 verified:false", c.key.name);
        assert!(
            c.vendor_ref.starts_with("Modules/CalcTriggers.lua:"),
            "{}: 缺 vendor 锚点",
            c.key.name
        );
        assert!(
            matches!(
                c.key.kind.as_str(),
                "skill" | "triggered_by" | "unique_item"
            ),
            "{}: 非法 kind {}",
            c.key.name,
            c.key.kind
        );
        for cond in [&c.source_skill_cond, &c.triggered_skill_cond]
            .into_iter()
            .flatten()
        {
            assert!(!cond.is_empty(), "{}: 空谓词应省略字段", c.key.name);
        }
    }
}

/// CoC 条目抽查（W-E2 暴击折入的数据前提）：trigger_on_crit + 攻击源谓词 +
/// PoE2 join 键 `MetaCastOnCritPlayer`。
#[test]
fn coc_entry_shape() {
    let def = load();
    let coc = def
        .configs
        .iter()
        .find(|c| c.key.name == "cast on critical strike")
        .expect("缺 CoC 条目");
    assert!(coc.trigger_on_crit);
    assert!(
        coc.match_effect_ids
            .iter()
            .any(|id| id == "MetaCastOnCritPlayer")
    );
    let cond = coc.source_skill_cond.as_ref().expect("CoC 源谓词");
    assert!(cond.any_skill_types.iter().any(|t| t == "Attack"));
}

/// 受限谓词抽查（蓝图 §2 W-E1 示例条目）：Law of the Wilds 的 any/all/not 三段。
#[test]
fn law_of_the_wilds_predicate() {
    let def = load();
    let entry = def
        .configs
        .iter()
        .find(|c| c.key.name == "law of the wilds")
        .expect("缺 law of the wilds");
    let cond = entry.source_skill_cond.as_ref().expect("源谓词");
    assert_eq!(cond.any_skill_types, vec!["Melee", "Attack"]);
    assert_eq!(cond.all_mod_flags, vec!["Claw"]);
    assert_eq!(cond.not_skill_types, vec!["SummonsTotem"]);
}

/// 缺表容忍（R7）：不存在的版本目录返回 Ok(None) 而非错误。
#[test]
fn missing_overlay_tolerated() {
    let missing = GameData::new(repo_data_root().join("0.0.0.0-nonexistent"))
        .trigger_configs()
        .unwrap();
    assert!(missing.is_none());
}
