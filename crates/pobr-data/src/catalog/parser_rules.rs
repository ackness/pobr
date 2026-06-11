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

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

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

/// 模板值三态：数字字面量 | `"$n"` 捕获直引 | 带算子链表达式。
/// `Flag(bool)` 供 FLAG 型 mod 的 value=true 字面量。
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
