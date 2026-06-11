//! `mod_parser_rules/v1` schema 往返单测（M6 前置 / 蓝图 §11.3 契约 1）。
//!
//! fixture `tests/fixtures/mini_parser_rules.json` 是 A→B 契约的 mini 规则集：
//! 每段 ≥3 条**逐字节取自真实抽取产物**的条目（含闭包推断模板与 handler 兜底
//! 两形态），供 M6-B scan 引擎在真实抽取落盘前先行开发。

use pobr_data::catalog::parser_rules::ModParserRulesDoc;
use pobr_data::catalog::stat_map::StatMapValue;

const MINI_FIXTURE: &str = include_str!("fixtures/mini_parser_rules.json");

fn load_mini() -> ModParserRulesDoc {
    serde_json::from_str(MINI_FIXTURE).expect("mini fixture 应可反序列化（_meta 被忽略）")
}

/// 反序列化 → 再序列化 → 再反序列化，结构等价（serde 形状自洽；
/// `_meta` 头按消费侧约定被忽略）。
#[test]
fn roundtrips_mini_fixture() {
    let doc = load_mini();
    let json = serde_json::to_string_pretty(&doc).unwrap();
    let back: ModParserRulesDoc = serde_json::from_str(&json).unwrap();
    assert_eq!(back, doc);
}

/// 每段条目数满足契约（≥3；unsupported 两段各 1）。
#[test]
fn mini_fixture_section_sizes() {
    let doc = load_mini();
    assert!(doc.forms.len() >= 3);
    assert!(doc.name_map.len() >= 3);
    assert!(doc.flag_phrases.len() >= 3);
    assert!(doc.pre_flags.len() >= 3);
    assert!(doc.tag_phrases.len() >= 3);
    assert!(doc.suffix_types.len() >= 3);
    assert!(doc.damage_types.len() >= 3);
    assert!(doc.pen_types.len() >= 3);
    assert!(doc.regen_types.len() >= 3);
    assert!(doc.degen_types.len() >= 3);
    assert!(doc.cost_types_map.len() >= 3);
    assert!(doc.base_cost_types.len() >= 3);
    assert!(doc.flag_types.len() >= 3);
    assert_eq!(doc.unsupported, vec!["mirrored"]);
    assert_eq!(doc.unsupported_pobr_extra, vec!["split"]);
}

/// 关键形态钉值：tag 模板（flatten 字段）/ 闭包推断占位符 / handler 兜底 /
/// flagTypes 双形态。
#[test]
fn mini_fixture_shape_pins() {
    let doc = load_mini();

    // TagTemplate flatten：type 提取 + 其余字段入 BTreeMap
    let mana_cost = doc
        .name_map
        .iter()
        .find(|e| e.phrase == "mana cost of attacks")
        .unwrap();
    let tag = &mana_cost.effects.tags[0];
    assert_eq!(tag.tag_type, "SkillType");
    assert_eq!(
        tag.fields.get("skill_type"),
        Some(&StatMapValue::Text("Attack".into()))
    );

    // 闭包推断模板：占位符落 StatMapValue::Text
    let per_rage = doc
        .tag_phrases
        .iter()
        .find(|e| e.pattern == "per (%d+) rage")
        .unwrap();
    assert!(per_rage.inferred);
    assert_eq!(
        per_rage.effects.tags[0].fields.get("div"),
        Some(&StatMapValue::Text("$1".into()))
    );

    // 字符串拼接模板（:cap 算子，蓝图 §1.5 例）
    let effect = doc
        .tag_phrases
        .iter()
        .find(|e| e.pattern == "per (%d+)%% (%a+) effect on enemy")
        .unwrap();
    assert_eq!(
        effect.effects.tags[0].fields.get("var"),
        Some(&StatMapValue::Text("$2:cap+Effect".into()))
    );

    // handler 兜底形态：handler_id 存在且 effects 为空
    let rampage = doc
        .tag_phrases
        .iter()
        .find(|e| e.pattern == "per (%d+) rampage kills")
        .unwrap();
    assert!(
        rampage
            .handler_id
            .as_deref()
            .is_some_and(|id| id.starts_with("tag_phrase:"))
    );
    assert!(!rampage.inferred);
    assert!(rampage.effects.tags.is_empty());

    // pre_flags 包装指令 + modSuffix
    let take = doc
        .pre_flags
        .iter()
        .find(|e| e.pattern == "^take ")
        .unwrap();
    assert_eq!(take.effects.mod_suffix.as_deref(), Some("Taken"));
    let minions = doc
        .pre_flags
        .iter()
        .find(|e| e.pattern == "^minions [cthd][ae][ukva][sel]e? ")
        .unwrap();
    assert!(minions.effects.add_to_minion);

    // flagTypes 双形态：condition 字符串 vs 内嵌 mod（hexproof）
    let phasing = doc
        .flag_types
        .iter()
        .find(|e| e.phrase == "phasing")
        .unwrap();
    assert_eq!(phasing.condition.as_deref(), Some("Condition:Phasing"));
    assert!(phasing.mod_def.is_none());
    let hexproof = doc
        .flag_types
        .iter()
        .find(|e| e.phrase == "hexproof")
        .unwrap();
    assert!(hexproof.condition.is_none());
    let mod_def = hexproof.mod_def.as_ref().unwrap();
    assert_eq!(
        (
            mod_def.name.as_str(),
            mod_def.mod_type.as_str(),
            mod_def.value
        ),
        ("CurseEffectOnSelf", "MORE", -100.0)
    );
}

/// 缺省字段（serde default / R7 纪律）：最小 JSON 也能反序列化。
#[test]
fn defaults_tolerate_minimal_entries() {
    let json = r#"{
        "forms": [{ "pattern": "^x", "form": "BASE" }],
        "name_map": [{ "phrase": "x", "names": ["X"] }],
        "flag_phrases": [{ "phrase": "x" }],
        "pre_flags": [{ "pattern": "^x" }],
        "tag_phrases": [{ "pattern": "x" }]
    }"#;
    let doc: ModParserRulesDoc = serde_json::from_str(json).unwrap();
    assert_eq!(doc.forms[0].literal, None);
    assert!(!doc.forms[0].anchored);
    assert!(doc.name_map[0].effects.tags.is_empty());
    assert!(!doc.pre_flags[0].inferred);
    assert!(doc.tag_phrases[0].handler_id.is_none());
    assert!(doc.suffix_types.is_empty());
    assert!(doc.unsupported.is_empty());
}
