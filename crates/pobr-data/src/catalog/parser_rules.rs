//! ModParser 规则数据 schema 域——同一模块承载两套互补 schema（两者本就规划在
//! 同一文件，见 `audits/rearchitecture-2026-06-10/blueprints/00-index.md`）：
//!
//! 1. **M5b special_mods 模板 schema**（`overlay/special_mods.json`，人工策展）
//!    —— [`SpecialModsDef`] 一族；
//! 2. **M6 ModParser 解析规则六表 schema**（`overlay/mod_parser_rules.json`，
//!    `mod_parser_rules/v1`，vendor 抽取）—— [`ModParserRulesDoc`] 一族。
//!
//! 两段原始文档如下，合并自 pre-M5b 与 pre-M6 两条工作线。
//!
//! # 第一部分（M5b）：
//!
//! special 词条模板域 schema（`overlay/special_mods.json` +
//! `generated/special_derived.json`，M5b 蓝图 §2.1 契约 / B-1）。
//!
//! 对应 vendor PoB2 `Modules/ModParser.lua:2231-6150` `specialModList`
//! （2085 条整行锚定特例）的分批数据化载体。本表是**人工策展域**（`_meta.generator
//! = "hand-curated"`，`regen_command` 记录对账命令而非再生命令）；vendor 对账由
//! `vendor_pattern` 列 + `sync-pob-catalog check --special-coverage`（M5b A-3）承担。
//!
//! # 受限模板 DSL 硬边界（20-target-architecture §5 原文，review checklist 必读）
//!
//! - 允许：`$1..$n` 数值占位、字面量、`negate / clamp(min,max) / div / mult / base`
//!   五种算子、`target(player|enemy|minion)`、受限谓词（字段引用 + eq/ne/gt/lt +
//!   and/or）。
//! - 禁止：循环、递归、自由表达式、跨条目引用、字符串拼接求值。
//! - 扩展闸门：新增任何 DSL 能力需 ≥20 个条目受益，否则该条目走 `handler_id`。
//! - 监控：handler 条目数 <100；逼近 special 总量 10% 即判切分失败、回看 P4。
//! - 元数据：未经 oracle 验证的条目带 `verified:false`，运行时照用但 parity
//!   报告单列。
//!
//! **求值器单点（00-index 裁决 §4-1）**：`$n` / 五算子 / 受限谓词的唯一求值实现
//! = `pobr-core::rules::value_expr`（M3-T1 起建；config / special / parser 三处
//! 同一套受限语言，禁三套方言）。撰写本 schema 时该模块尚未合并，故此处**只定义
//! serde 形状并以文档注释固定语言定义，不 import 求值代码**；M5b-B2 接入解释器时
//! 以届时 master 的 value_expr 为准对齐。受限谓词的语言定义（值形态预留在
//! [`TemplateTagDef`] 的开放字段中，本批次条目未使用）：
//!
//! ```text
//! predicate := comparison | predicate ("and" | "or") predicate
//! comparison := field_ref ("eq" | "ne" | "gt" | "lt") literal
//! field_ref  := 求值上下文白名单内的字段名（无自由表达式、无函数调用）
//! ```
//!
//! 本模块零逻辑、零 I/O；全部新字段 `#[serde(default)]` / `Option`（R7 纪律）。
//!
//! # 第二部分（M6）：
//!
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

// ===== M5b：special 词条模板 schema（overlay/special_mods.json）=====

/// 数值算子（五算子白名单，外部标签 serde 形：`{"negate": {}}` /
/// `{"div": 100}`）。语义（与 value_expr 单点实现对齐）：
///
/// - `negate`：`v → -v`；
/// - `clamp{min,max}`：`v → min(max(v, min), max)`；
/// - `div(n)`：`v → v / n`；
/// - `mult(n)`：`v → v × n`；
/// - `base(n)`：`v → v + n`（先加基准再继续算子链）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueOpDef {
    /// 取负。
    Negate {},
    /// 区间钳制。
    Clamp {
        /// 下界。
        min: f64,
        /// 上界。
        max: f64,
    },
    /// 除以常数。
    Div(f64),
    /// 乘以常数。
    Mult(f64),
    /// 加基准常数。
    Base(f64),
}

/// 带算子链的取值表达式（`{"ref": "$1", "ops": [{"negate": {}}, {"div": 100}]}`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValueExprDef {
    /// 捕获引用（`$1..$n`，按 pattern 捕获组出现序）。
    #[serde(rename = "ref")]
    pub capture: String,
    /// 算子链（按序应用）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ops: Vec<ValueOpDef>,
}

/// 模板值形态：数字字面量 | `"$n"` 捕获直引 | 带算子链表达式 | 嵌套 mod 载荷
/// | 标量表。`Flag(bool)` 供 FLAG 型 mod 的 value=true 字面量。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TemplateValueDef {
    /// FLAG 布尔字面量。
    Flag(bool),
    /// 数字字面量。
    Number(f64),
    /// 捕获直引（`"$1"`）。
    Capture(String),
    /// 表达式（捕获 + 算子链）。
    Expr(ValueExprDef),
    /// 嵌套 mod 载荷（vendor `mod("EnemyModifier", "LIST", { mod = mod(...) })`
    /// 形态；JSON 形 `{"mods": [<ModTemplateDef>...]}`）。实例化为
    /// `ModValue::NestedMods`，由编排层（env_finalize `forward_enemy_modifiers`
    /// 等）经 `ModDb::list_nested` 透传转发到目标 db。**必须列在 `List` 之前**：
    /// untagged 按声明序尝试，`{"mods": ...}` 的数组值会让 `List`（标量表）
    /// 反序列化失败，但反向顺序会把标量表误吞。
    Nested {
        /// 内层 mod 模板（与顶层 [`ModTemplateDef`] 同 schema，可递归）。
        mods: Vec<ModTemplateDef>,
    },
    /// LIST 型 mod 的结构化值（如 `Keystone LIST` 的关键石名）。值内字符串
    /// 必须是字面量或 enums 闭集产物，禁运行时拼接（DSL 硬边界）。
    List(BTreeMap<String, TemplateScalarDef>),
}

/// 模板内标量：字面量或 `"$n"` 捕获 / `{"enum": n}` 闭集引用。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TemplateScalarDef {
    /// 布尔字面量。
    Bool(bool),
    /// 数字字面量。
    Number(f64),
    /// 字符串字面量或 `"$n"` 捕获引用。
    Text(String),
    /// enums 闭集映射引用（`{"enum": 3}` = 用第 3 个捕获词在条目 `enums["3"]`
    /// 表中查完整字面量；每个可能输出都是表内显式字面量，非字符串拼接）。
    Enum {
        /// 捕获组序号（1-based）。
        #[serde(rename = "enum")]
        capture_index: u32,
    },
    /// 字符串字面量列表（vendor `SkillName` tag 的 `skillNameList` 形态）。
    /// JSON 数组形状与其余标量变体互斥，untagged 无歧义。
    TextList(Vec<String>),
}

/// 模板 tag（pobr `ModTag` 的 serde 形态投影；`type` 之外的字段开放转录，
/// 值可为字面量 / `"$n"` / enums 引用）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateTagDef {
    /// tag 类型（如 `Condition` / `Multiplier` / `SkillType`）。
    #[serde(rename = "type")]
    pub tag_type: String,
    /// 其余字段（如 `var` / `stat` / `threshold`），按 tag 类型开放。
    #[serde(flatten)]
    pub fields: BTreeMap<String, TemplateScalarDef>,
}

/// mod 名三态：字面量或 enums 闭集引用。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TemplateNameDef {
    /// ModName 字面量。
    Literal(String),
    /// enums 闭集引用（同 [`TemplateScalarDef::Enum`]）。
    Enum {
        /// 捕获组序号（1-based）。
        #[serde(rename = "enum")]
        capture_index: u32,
    },
}

/// 一条产出 mod 的模板（M5b 蓝图 §2.1 `ModTemplate`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModTemplateDef {
    /// ModName（字面量或 enums 引用）。
    pub name: TemplateNameDef,
    /// mod 类型（`BASE|INC|MORE|FLAG|OVERRIDE|LIST`，pobr ModType serde 名）。
    #[serde(rename = "type")]
    pub mod_type: String,
    /// 取值（三态，见 [`TemplateValueDef`]）。
    pub value: TemplateValueDef,
    /// ModFlags 位名列表（按届时 M4 扩位后的位名；本批次只用既有位名）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<String>,
    /// KeywordFlags 位名列表。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keyword_flags: Vec<String>,
    /// tag 列表。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<TemplateTagDef>,
    /// 作用目标：`player`（缺省）| `enemy`（→ EnemyModifier LIST 包装，M3 通道）
    /// | `minion`（→ MinionModifier LIST 包装）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

/// 一条 special 词条模板（M5b 蓝图 §2.1 `SpecialTemplateDef`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpecialTemplateDef {
    /// 稳定 id（snake_case；diff / 报表 / oracle 对拍引用。重命名视为删+增）。
    pub id: String,
    /// 匹配模式：Rust regex 语法（regex crate 子集：无 look-around / 反向引用）。
    /// 引擎统一做输入小写规范化 + 整行锚定（编译期包 `^...$`，对照 vendor
    /// :6155-6158）。捕获组按出现序 = `$1..$n`；数值捕获统一
    /// `(\d+(?:\.\d+)?)`；词类捕获必须是显式闭集（如
    /// `(fire|cold|lightning|chaos|physical)`），禁 `(.+)` 开放捕获
    /// （开放捕获条目走 `handler_id`）。
    pub pattern: String,
    /// vendor 对账元数据：原 Lua pattern 字面量（`check --special-coverage`
    /// 按它对 vendor key 做存在性 diff）。`None` = pobr 自有特例
    /// （vendor 无同 key，来源见 `source_note`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor_pattern: Option<String>,
    /// 模板路径（与 `handler_id` 互斥；两者都缺 = 纯识别不产 mod 的
    /// 「已知不支持」条目，进 unsupported 报表但不再算解析失败）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mods: Vec<ModTemplateDef>,
    /// handler 路径：真逻辑条目的稳定 id（运行时查
    /// `pobr-core::rules::registry`；未注册 → 命中但产空 mods + 报表标记）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handler_id: Option<String>,
    /// handler 实参（捕获按序透传，`"$n"` 形）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub handler_args: Vec<String>,
    /// enums 受限闭集映射（DSL 微扩展，00-index 裁决 §4.2-3 已批准）：
    /// 键 = 捕获组序号（字符串形），值 = `捕获词 → 完整字面量` 闭集表。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub enums: BTreeMap<String, BTreeMap<String, String>>,
    /// oracle 对拍通过标记（Track D 流程置 true；人工改 JSON + 独立 commit）。
    #[serde(default)]
    pub verified: bool,
    /// 批次标记（`S0|S1|S2`，M6 长尾续编）。
    pub batch: String,
    /// 来源备注（unique 名 / keystone 名 / pobr 准源说明）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_note: Option<String>,
}

/// `overlay/special_mods.json` / `generated/special_derived.json` 顶层
/// （消费侧忽略 `_meta`；两表 entries 拼接、id 冲突报错——M5b B-4 接线语义）。
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SpecialModsDef {
    /// 模板条目列表，按 `id` 升序。
    pub entries: Vec<SpecialTemplateDef>,
}

// ===== M6：ModParser 解析规则六表 schema（overlay/mod_parser_rules.json）=====

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
