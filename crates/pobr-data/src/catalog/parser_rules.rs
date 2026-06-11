//! ModParser 解析规则 overlay 域 schema（`overlay/mod_parser_rules.json`，
//! schema `mod_parser_rules/v1`，M6 蓝图 §1）。
//!
//! 数据来源：vendor PoB2 `Modules/ModParser.lua` 的解析规则表（formList /
//! modNameList / modFlagList / preFlagList / modTagList + §1.7 小查找表），由
//! `sync-pob-catalog extract-lua --what parser-rules` 经 headless 引导 luajit
//! 执行后 dump **加载后的最终表**（含 regen/cost 等派生表展开）确定性抽取生成。
//! specialModList 不在本表（归 M5b `overlay/special_mods.json`）；
//! skillNameList / preSkillNameList 归 `generated/special_derived.json`（M6-T7）。
//!
//! 关键抽取约定（蓝图 §1 裁决，消费侧 = M6-B scan 引擎）：
//! - **pattern 原样保留 Lua pattern 语法**，不在抽取期翻译成 regex；
//!   [`FormDef::literal`] / `anchored` 是 Rust 侧派生的索引辅助字段；
//! - **位掩码 → 名字**：vendor `ModFlag` / `KeywordFlag` 掩码分解为名字数组
//!   （P1：位枚举留 Rust，载入期 `from_names` 还原）；tag 内 `skillType` 数值
//!   枚举反查为名字（落 `skill_type` 键）、`modFlags`/`keywordFlags` 掩码分解
//!   （落 `mod_flags`/`keyword_flags` 键）；其余 tag 字段**键名原样转录**
//!   （vendor camelCase，如 `limitTotal` / `varList`）；
//! - **闭包 → 模板（探针推断）**：闭包条目用双哨兵探针推断为占位符模板
//!   （`$1..$5`，算子 `:cap`（首字母大写）、`:div(k)`/`:mult(k)`/`:negate`，
//!   字符串拼接用 `+` 连接段），成功者标 [`PreFlagDef::inferred`]；推断失败
//!   条目落 `handler_id`（`<段名>:<pattern 稳定 hash 前 12 位>`），由 Rust
//!   handler 注册表兜底（全局 <100 闸门）。占位符求值器唯一实现 =
//!   `pobr-core::rules::value_expr`（架构裁决 §4-1），本模块只定义 serde 形状。
//!
//! 与蓝图 §1.8 的已记录偏差：`resource_types` 不入库——vendor 加载完成后
//! parseMod 仅消费其派生展开（regen/degen/cost/base_cost 四表），原始表
//! 不可达也无运行时消费方（见 m6-extraction-report.md）。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub use crate::catalog::stat_map::StatMapValue;

/// 当前 overlay 文档 schema 标识（字段演化时递增）。
pub const MOD_PARSER_RULES_SCHEMA: &str = "mod_parser_rules/v1";

/// 一个 tag 模板：vendor tag 表的忠实转录（`type` + 其余字段）。
///
/// 字段值可含占位符模板字符串（闭包探针推断产物，如 `"$1"` /
/// `"$2:cap+Effect"`）；纯表条目则全为字面值。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TagTemplate {
    /// tag 类型（vendor `type` 字段：`Condition` / `Multiplier` / `PerStat` /
    /// `SkillType` / `ModFlagOr` / `ActorCondition` …）。
    #[serde(rename = "type")]
    pub tag_type: String,
    /// 其余字段（键字典序）：`var` / `varList` / `div` / `limit` / `limitTotal`
    /// / `actor` / `neg` / `threshold` / `skill_type`（已反查名字）/
    /// `mod_flags` / `keyword_flags`（已分解名字数组）等。
    #[serde(flatten)]
    pub fields: BTreeMap<String, StatMapValue>,
}

/// 各短语/pattern 表条目共用的效果字段全集（蓝图 §1.4：pre_flags 与
/// tag_phrases 共用；name_map / flag_phrases 是其子集，serde flatten 嵌入）。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RuleEffectsDef {
    /// ModFlag 名字数组（vendor `flags` 掩码分解）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<String>,
    /// KeywordFlag 名字数组（vendor `keywordFlags` 掩码分解）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keyword_flags: Vec<String>,
    /// tag 模板（vendor 单 `tag` 与 `tagList` 归一为数组，原顺序 = `[tag] ++ tagList`）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<TagTemplate>,
    /// 玩家侧 tag（vendor `playerTag` / `playerTagList` 归一）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub player_tags: Vec<TagTemplate>,
    /// 包装指令：mod 转给召唤物（vendor `addToMinion`）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub add_to_minion: bool,
    /// 转给召唤物时附带的 tag（vendor `addToMinionTag` 单值归一为数组）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub add_to_minion_tags: Vec<TagTemplate>,
    /// 包装指令：并入光环效果（vendor `addToAura`）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub add_to_aura: bool,
    /// 包装指令：仅并入战旗（vendor `onlyAddToBanners`）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub only_add_to_banners: bool,
    /// 包装指令：生成新光环（vendor `newAura`）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub new_aura: bool,
    /// 新光环仅作用盟友（vendor `newAuraOnlyAllies`）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub new_aura_only_allies: bool,
    /// 包装指令：mod 注入技能局部（vendor `addToSkill`，单 tag）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub add_to_skill: Option<TagTemplate>,
    /// 包装指令：mod 施加给敌人（vendor `applyToEnemy`）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub apply_to_enemy: bool,
    /// 敌方 actor 视角（vendor `actorEnemy`）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub actor_enemy: bool,
    /// ModName 后缀（vendor `modSuffix`，如 `^take ` → `"Taken"`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mod_suffix: Option<String>,
}

/// formList 条目：Lua pattern → form id（蓝图 §1.1）。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct FormDef {
    /// Lua pattern 原样（含 `^` 锚 / `(%d+)` 捕获 / `%%` 转义）。
    pub pattern: String,
    /// form id（`INC` / `RED` / `MORE` / `BASE` / `PEN` / `DMG` … 28 种）。
    pub form: String,
    /// 派生：pattern 中最长的连续字面量片段（aho-corasick 预过滤用；
    /// 全类元素 pattern 为 `None` → 引擎 always-check 桶）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub literal: Option<String>,
    /// 派生：pattern 以 `^` 锚定（引擎只在剩余文本头部尝试）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub anchored: bool,
}

/// modNameList 条目：短语（plain 子串匹配）→ ModName 集 + 可选效果（蓝图 §1.2）。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct NameMapDef {
    /// 匹配短语（小写，plain 子串匹配、无 pattern 语法）。
    pub phrase: String,
    /// vendor ModName 列表（单名也包一层数组；直接落 pobr `StatId`）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub names: Vec<String>,
    /// 可选效果（vendor 带 tag / flags / addToMinion 的表条目）。
    #[serde(flatten)]
    pub effects: RuleEffectsDef,
}

/// modFlagList 条目：短语（plain）→ ModFlag/KeywordFlag + 可选 tag（蓝图 §1.3）。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct FlagPhraseDef {
    /// 匹配短语（小写，plain 子串匹配）。
    pub phrase: String,
    /// 效果（flags / keyword_flags / tags / addToMinion…）。
    #[serde(flatten)]
    pub effects: RuleEffectsDef,
}

/// preFlagList 条目：行首 pattern → flags/tag/包装指令（蓝图 §1.4）。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PreFlagDef {
    /// Lua pattern 原样（vendor 全部 `^` 锚定）。
    pub pattern: String,
    /// 派生：最长字面量片段（见 [`FormDef::literal`]）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub literal: Option<String>,
    /// 派生：`^` 锚定。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub anchored: bool,
    /// 效果字段全集。
    #[serde(flatten)]
    pub effects: RuleEffectsDef,
    /// 闭包条目经探针推断为模板（oracle differential 覆盖后可升级 verified，
    /// M6-C 范围）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub inferred: bool,
    /// 探针推断失败的闭包条目：Rust handler 注册表 id
    /// （`pre_flag:<hash12>`）；与 `effects` 互斥。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handler_id: Option<String>,
}

/// modTagList 条目：per-X / 条件短语 pattern → tag 模板（蓝图 §1.5）。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TagPhraseDef {
    /// Lua pattern 原样。
    pub pattern: String,
    /// 派生：最长字面量片段。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub literal: Option<String>,
    /// 派生：`^` 锚定。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub anchored: bool,
    /// 效果字段全集（与 pre_flags 共用；多数条目仅 `tags`）。
    #[serde(flatten)]
    pub effects: RuleEffectsDef,
    /// 见 [`PreFlagDef::inferred`]。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub inferred: bool,
    /// 见 [`PreFlagDef::handler_id`]（`tag_phrase:<hash12>`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handler_id: Option<String>,
}

/// 小查找表条目：短语 → 单个后缀/类型名（suffix_types / damage_types /
/// pen_types，蓝图 §1.7）。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PhraseValueDef {
    /// 匹配短语（plain）。
    pub phrase: String,
    /// 目标名（如 `GainAsFire` / `Physical` / `LightningPenetration`）。
    pub value: String,
}

/// 小查找表条目：短语 → 名集（resource 派生四表，值可多名，蓝图 §1.7）。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PhraseNamesDef {
    /// 匹配短语（plain，含 vendor 加载期补入的 `maximum X` 变体）。
    pub phrase: String,
    /// 目标名列表（如 `["LifeRegen", "ManaRegen"]`；单名也包一层）。
    pub names: Vec<String>,
}

/// flagTypes 条目内嵌 mod（目前仅 hexproof 特例）。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct FlagTypeModDef {
    /// ModName（如 `CurseEffectOnSelf`）。
    pub name: String,
    /// 聚合类型原文（如 `MORE`）。
    pub mod_type: String,
    /// 数值。
    pub value: f64,
}

/// flagTypes 条目：FLAG form 的短语 → `Condition:X` 字符串或内嵌 mod
/// （蓝图 §1.7，value 双形态）。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct FlagTypeDef {
    /// 匹配短语（plain；个别含 pattern 语法，如 hindered 变体）。
    pub phrase: String,
    /// 字符串形态：条件/flag 名（如 `Condition:Phasing` / `NoLifeRegen`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    /// 表形态：内嵌 mod（hexproof 特例）；与 `condition` 互斥。
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "mod")]
    pub mod_def: Option<FlagTypeModDef>,
}

/// `overlay/mod_parser_rules.json` 顶层（消费侧视角：serde 默认忽略 `_meta`
/// 生成溯源头；段顺序 = 蓝图 §1.8）。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ModParserRulesDoc {
    /// formList（91 条 pattern → 28 种 form id），按 pattern 字典序。
    pub forms: Vec<FormDef>,
    /// modNameList（775 条短语 → ModName 集），按 phrase 字典序。
    pub name_map: Vec<NameMapDef>,
    /// modFlagList（202 条短语 → flag 位名 + tag），按 phrase 字典序。
    pub flag_phrases: Vec<FlagPhraseDef>,
    /// preFlagList（219 条行首 pattern → 包装指令），按 pattern 字典序。
    pub pre_flags: Vec<PreFlagDef>,
    /// modTagList（682 条 per-X/条件 pattern → tag 模板），按 pattern 字典序。
    pub tag_phrases: Vec<TagPhraseDef>,
    /// suffixTypes（BASE/GAIN/LOSE/GRANTS 族 form 的后缀扫描表）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suffix_types: Vec<PhraseValueDef>,
    /// dmgTypes（DMG 族 form 的伤害类型表）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub damage_types: Vec<PhraseValueDef>,
    /// penTypes（PEN form 的穿透目标表）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pen_types: Vec<PhraseValueDef>,
    /// regenTypes（REGEN 族；vendor 加载期 `appendMod(resourceTypes, "Regen")`
    /// 派生的最终展开形态，照 dump）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub regen_types: Vec<PhraseNamesDef>,
    /// degenTypes（DEGEN 族派生展开）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub degen_types: Vec<PhraseNamesDef>,
    /// costTypes（TOTALCOST form 派生展开；命名带 `_map` 后缀避让 base 域
    /// `cost_types`，蓝图 §1.8）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cost_types_map: Vec<PhraseNamesDef>,
    /// baseCostTypes（BASECOST form 派生展开）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub base_cost_types: Vec<PhraseNamesDef>,
    /// flagTypes（FLAG form 的条件表）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flag_types: Vec<FlagTypeDef>,
    /// unsupportedModList（vendor 原样，目前仅 `mirrored`）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unsupported: Vec<String>,
    /// pobr 自加的 unsupported 项（与 vendor 段分列保证 drift diff 纯净；
    /// `split` 来自现 `mod_parser.rs:63` 硬编码，蓝图 §1.6 要求迁表保留）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unsupported_pobr_extra: Vec<String>,
}
