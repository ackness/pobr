//! Config 选项目录域 schema（`overlay/config_options.json`，M3-T1）。
//!
//! 数据来源：vendor PoB2 `src/Modules/ConfigOptions.lua`（542 静态条目 +
//! questRewards 动态条目），经 `sync-pob-catalog extract-lua --what
//! config-options` 用**调用拦截 + 多探针拟合**归纳 apply 闭包为声明式
//! `effects[]`（蓝图 m3-orchestration §4.2）。无法模板化的真逻辑条目只携带
//! `handler_id`（目标 ≤54，架构 §5）。
//!
//! 表达式 / 谓词 / tag 类型复用 [`super::value_expr`]（受限 DSL 单点，
//! 00-index 裁决 §4-1）；求值统一走 `pobr-core::rules::value_expr`。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::value_expr::{EffectTag, EffectTarget, Predicate, ValueExpr};

/// 当前 overlay 文档 schema 标识（字段演化时递增）。
pub const CONFIG_OPTIONS_SCHEMA: &str = "config_options/v1";

/// `overlay/config_options.json` 消费侧顶层（`_meta` 由 serde 忽略）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigOptionsDef {
    /// 全部条目，按 `var` 升序（byte-stable 排序键）。
    pub options: Vec<ConfigOptionDef>,
}

/// 单个 config 条目（镜像 ConfigOptions schema + 声明式 effects DSL）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigOptionDef {
    /// 稳定变量名（vendor `var`；XML `<Input name=...>` 对应键）。
    pub var: String,
    /// 输入类型。
    pub input_type: ConfigInputType,
    /// 所属 section（General / Skill Options / Combat / …）。
    pub section: String,
    /// UI label（label 为函数的条目缺省）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// 默认值族（defaultState / defaultIndex / defaultPlaceholderState）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<ConfigDefault>,
    /// list 型选项（`{val, label}`）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub list_options: Vec<ListOption>,
    /// 可见性条件（M3 只入库不消费——UI 用）。
    #[serde(default, skip_serializing_if = "ConfigVisibility::is_empty")]
    pub visibility: ConfigVisibility,
    /// implyCond + implyCondList 展开（条目值为真时蕴含置位的条件名）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub imply_conditions: Vec<String>,
    /// 声明式效果（全选项共享；list 型结构随选项变化时空置、改走
    /// [`Self::option_effects`]）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<ConfigEffect>,
    /// list 型逐选项 effects（键 = 选项 `val` 的字符串化）。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub option_effects: BTreeMap<String, Vec<ConfigEffect>>,
    /// 真逻辑条目的 handler 稳定 ID（与 effects 互斥；查
    /// `pobr-core::rules::HandlerRegistry`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handler_id: Option<String>,
    /// 降级为 handler 的原因（抽取器写入，报表用）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handler_reason: Option<String>,
    /// 抽取期多探针对拍通过（蓝图 A2 正确性裁判；false 条目运行时照用、
    /// parity 报告单列）。
    #[serde(default)]
    pub verified: bool,
}

/// 输入类型（vendor `type` 字面量的稳定枚举）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigInputType {
    /// 勾选框（值恒 true 才应用）。
    Check,
    /// 计数（0 视为未设置、不应用）。
    Count,
    /// 计数（0 也应用）。
    CountAllowZero,
    /// 整数。
    Integer,
    /// 浮点。
    Float,
    /// 下拉列表。
    List,
    /// 文本（仅 customMods）。
    Text,
}

/// 默认值族（多形态并存，按输入类型取用）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigDefault {
    /// check 型 `defaultState`（布尔）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_bool: Option<bool>,
    /// count/integer/float 型 `defaultState`（数值）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_number: Option<f64>,
    /// list 型 `defaultIndex`（**1-based**，忠实 vendor）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<u32>,
    /// `defaultPlaceholderState`（数值占位默认）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder_number: Option<f64>,
}

/// list 型单个选项。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListOption {
    /// 选项值的字符串化（`option_effects` 的键；数值选项同时给出
    /// [`Self::number`]）。
    pub value: String,
    /// 数值选项的原始数值（如 resistancePenalty 的 `0/-10/…`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<f64>,
    /// UI label（含 vendor 颜色码，忠实转录）。
    pub label: String,
}

/// 可见性条件族（vendor `if*` 字段，单值归一为单元素数组）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConfigVisibility {
    /// `ifCond`：玩家条件被计算使用时可见。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub if_cond: Vec<String>,
    /// `ifMinionCond`。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub if_minion_cond: Vec<String>,
    /// `ifEnemyCond`。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub if_enemy_cond: Vec<String>,
    /// `ifFlag`。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub if_flag: Vec<String>,
    /// `ifMult`。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub if_mult: Vec<String>,
    /// `ifEnemyMult`。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub if_enemy_mult: Vec<String>,
    /// `ifSkill` / `ifSkillList`。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub if_skill: Vec<String>,
    /// `ifSkillData`。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub if_skill_data: Vec<String>,
    /// `ifMod`。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub if_mod: Vec<String>,
    /// `ifTagType`。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub if_tag_type: Vec<String>,
}

impl ConfigVisibility {
    /// 是否全部为空（序列化跳过用）。
    pub fn is_empty(&self) -> bool {
        self.if_cond.is_empty()
            && self.if_minion_cond.is_empty()
            && self.if_enemy_cond.is_empty()
            && self.if_flag.is_empty()
            && self.if_mult.is_empty()
            && self.if_enemy_mult.is_empty()
            && self.if_skill.is_empty()
            && self.if_skill_data.is_empty()
            && self.if_mod.is_empty()
            && self.if_tag_type.is_empty()
    }
}

/// 单条声明式效果（对应一次 `modList:NewMod` / `AddMod` 调用）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigEffect {
    /// 写入目标 actor。
    pub target: EffectTarget,
    /// ModName（如 `Condition:Moving` / `Multiplier:StationarySeconds`）。
    pub name: String,
    /// mod 类型 vendor 字面量（`FLAG/BASE/INC/MORE/OVERRIDE/LIST`）。
    pub mod_type: String,
    /// 数值载荷（受限 DSL 表达式）；FLAG / 文本 / LIST 载荷时缺省。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<ValueExpr>,
    /// 布尔载荷（FLAG；缺省视为 true）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_bool: Option<bool>,
    /// 文本载荷（少数 OVERRIDE/LIST 文本值）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_text: Option<String>,
    /// LIST 结构化载荷（SkillData 键值 / 嵌套 mod）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list_value: Option<ListEffectValue>,
    /// 受限谓词：成立才发出本效果（如 conditionStationary 的 `input > 0`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emit_if: Option<Predicate>,
    /// 受限 tag 白名单。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<EffectTag>,
    /// ModFlag 名（vendor `formatFlags` 渲染名，如 `Attack`/`Cast`；
    /// 消费侧映射位枚举，未知名记 diagnostics）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<String>,
    /// KeywordFlag 名（同上）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keyword_flags: Vec<String>,
    /// mod source 字符串（缺省 `Config`；quest 条目为 `Quest:…`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// LIST 型 mod 的结构化载荷（受限闭集，白名单外形态降级 handler）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ListEffectValue {
    /// `{ key = K, value = V }` 键值载荷（SkillData 等）。
    KeyValue {
        /// 载荷键（如 `corpseLife`）。
        key: String,
        /// 载荷值。
        value: ListScalar,
    },
    /// `{ mod = <嵌套 mod> }`（MinionModifier / EnemyModifier 等转发通道）。
    NestedMod {
        /// 嵌套 mod 定义。
        #[serde(rename = "mod")]
        nested: NestedModDef,
    },
}

/// 键值载荷的值形态。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ListScalar {
    /// 布尔字面量。
    Bool {
        /// 字面值。
        value: bool,
    },
    /// 文本字面量。
    Text {
        /// 字面值。
        value: String,
    },
    /// 数值表达式（含 `FromInput` = 恒等 `Input`）。
    Expr {
        /// 受限表达式。
        expr: ValueExpr,
    },
}

/// 嵌套 mod 定义（LIST 转发通道的内层 mod）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NestedModDef {
    /// 内层 ModName。
    pub name: String,
    /// 内层 mod 类型 vendor 字面量。
    pub mod_type: String,
    /// 数值载荷。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<ValueExpr>,
    /// 布尔载荷（FLAG；缺省 true）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_bool: Option<bool>,
    /// 文本载荷。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_text: Option<String>,
    /// ModFlag 名。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<String>,
    /// KeywordFlag 名。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keyword_flags: Vec<String>,
    /// 受限 tag。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<EffectTag>,
    /// 内层 mod source。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::value_expr::Predicate;

    /// 典型 count 条目（conditionStationary 形态）serde 往返。
    #[test]
    fn count_entry_round_trip() {
        let def = ConfigOptionDef {
            var: "conditionStationary".to_string(),
            input_type: ConfigInputType::Count,
            section: "General".to_string(),
            label: Some("Time spent stationary".to_string()),
            default: None,
            list_options: Vec::new(),
            visibility: ConfigVisibility {
                if_cond: vec!["Stationary".to_string()],
                ..Default::default()
            },
            imply_conditions: Vec::new(),
            effects: vec![
                ConfigEffect {
                    target: EffectTarget::Player,
                    name: "Multiplier:StationarySeconds".to_string(),
                    mod_type: "BASE".to_string(),
                    value: Some(ValueExpr::Clamp {
                        min: Some(0.0),
                        max: None,
                        inner: Box::new(ValueExpr::input()),
                    }),
                    value_bool: None,
                    value_text: None,
                    list_value: None,
                    emit_if: None,
                    tags: Vec::new(),
                    flags: Vec::new(),
                    keyword_flags: Vec::new(),
                    source: None,
                },
                ConfigEffect {
                    target: EffectTarget::Player,
                    name: "Condition:Stationary".to_string(),
                    mod_type: "FLAG".to_string(),
                    value: None,
                    value_bool: Some(true),
                    value_text: None,
                    list_value: None,
                    emit_if: Some(Predicate::input_gt(0.0)),
                    tags: Vec::new(),
                    flags: Vec::new(),
                    keyword_flags: Vec::new(),
                    source: None,
                },
            ],
            option_effects: BTreeMap::new(),
            handler_id: None,
            handler_reason: None,
            verified: true,
        };
        let json = serde_json::to_string_pretty(&def).unwrap();
        let back: ConfigOptionDef = serde_json::from_str(&json).unwrap();
        assert_eq!(back, def);
    }

    /// handler 条目最小形态：未知字段（如 `_meta`）被忽略、缺省字段回填。
    #[test]
    fn handler_entry_minimal_and_unknown_fields_ignored() {
        let json = r#"{
            "var": "customMods",
            "input_type": "text",
            "section": "Custom mods",
            "handler_id": "config:custom_mods",
            "future_field": 1
        }"#;
        let def: ConfigOptionDef = serde_json::from_str(json).unwrap();
        assert_eq!(def.handler_id.as_deref(), Some("config:custom_mods"));
        assert!(def.effects.is_empty());
        assert!(!def.verified);
        assert!(def.visibility.is_empty());
    }

    /// list 型逐选项 effects + 嵌套 mod 载荷往返。
    #[test]
    fn list_entry_with_nested_mod_round_trip() {
        let mut option_effects = BTreeMap::new();
        option_effects.insert(
            "AVERAGE".to_string(),
            vec![ConfigEffect {
                target: EffectTarget::Minion,
                name: "MinionModifier".to_string(),
                mod_type: "LIST".to_string(),
                value: None,
                value_bool: None,
                value_text: None,
                list_value: Some(ListEffectValue::NestedMod {
                    nested: NestedModDef {
                        name: "Condition:FullLife".to_string(),
                        mod_type: "FLAG".to_string(),
                        value: None,
                        value_bool: Some(true),
                        value_text: None,
                        flags: Vec::new(),
                        keyword_flags: Vec::new(),
                        tags: Vec::new(),
                        source: Some("Config".to_string()),
                    },
                }),
                emit_if: None,
                tags: Vec::new(),
                flags: Vec::new(),
                keyword_flags: Vec::new(),
                source: None,
            }],
        );
        let def = ConfigOptionDef {
            var: "x".to_string(),
            input_type: ConfigInputType::List,
            section: "Combat".to_string(),
            label: None,
            default: Some(ConfigDefault {
                state_bool: None,
                state_number: None,
                index: Some(2),
                placeholder_number: None,
            }),
            list_options: vec![ListOption {
                value: "AVERAGE".to_string(),
                number: None,
                label: "Average".to_string(),
            }],
            visibility: ConfigVisibility::default(),
            imply_conditions: vec!["UsedSkillRecently".to_string()],
            effects: Vec::new(),
            option_effects,
            handler_id: None,
            handler_reason: None,
            verified: false,
        };
        let json = serde_json::to_string(&def).unwrap();
        let back: ConfigOptionDef = serde_json::from_str(&json).unwrap();
        assert_eq!(back, def);
    }
}
