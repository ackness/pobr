//! 内建 buff 定义域 schema（`overlay/buff_definitions.json`，M3-T2）。
//!
//! 数据来源：vendor PoB2 `src/Modules/CalcPerform.lua` `doActorMisc`
//! （:503-765，260 行过程式 if-chain）的**人工归纳**——该段无法 luajit
//! 序列化抽取，按 00-index 裁决 §4.2-4 批准的 overlay 通道例外落库：
//! 每条带 `vendor_ref`（行号 + 行段 hash）供 drift 告警（`sync-pob-catalog
//! check-buff-refs` 对账），正确性以 oracle 对拍 / 逐 buff 数值单测为准。
//!
//! 效果公式（框架逻辑留 Rust，见 `pobr-core::rules::buff_expander`）：
//!
//! ```text
//! scale  = (1 + Σ INC(inc_stats)/100) × Π MORE(more_stats)
//! effect = clamp(rounding(base × scale), min, max)
//! 每条 mod 值 = value 模板（Literal / coeff×effect / rounding(coeff×scale)）
//! ```

use serde::{Deserialize, Serialize};

use super::value_expr::EffectTag;

/// 当前 overlay 文档 schema 标识。
pub const BUFF_DEFINITIONS_SCHEMA: &str = "buff_definitions/v1";

/// `overlay/buff_definitions.json` 消费侧顶层（`_meta` 由 serde 忽略）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuffDefinitionsDef {
    /// 全部 buff 定义，按 `id` 升序。
    pub buffs: Vec<BuffDef>,
}

/// 单个内建 buff 定义。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuffDef {
    /// 稳定 ID（如 `Onslaught`）。
    pub id: String,
    /// 触发 flag 名（mod_db 的 FLAG 查询键，多数同 id）。
    pub trigger_flag: String,
    /// 模式门控（doActorMisc 整段 `env.mode_combat` 门控）。
    pub mode_gate: BuffModeGate,
    /// 效果量公式；纯字面量 buff（HerEmbrace 等）缺省。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect: Option<BuffEffectFormula>,
    /// 展开产出的 mod 模板。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mods: Vec<BuffModTemplate>,
    /// 附带置位的条件名（vendor `condList[...] = true`）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions_set: Vec<String>,
    /// 真逻辑条目的 handler 稳定 ID（与 effect/mods 互斥；buff 域预算 ≤8）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handler_id: Option<String>,
    /// 是否经 oracle 对拍验证。
    #[serde(default)]
    pub verified: bool,
    /// vendor 行段引用（人工归纳可追溯性 + drift 告警）。
    pub vendor_ref: VendorRef,
    /// 备注（已知差异 / 未覆盖分支说明）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// buff 的模式门控。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuffModeGate {
    /// 仅 `mode_combat` 下展开（doActorMisc :510 整段门控）。
    Combat,
}

/// 效果量公式参数。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuffEffectFormula {
    /// 基础量（Onslaught=10 / Convergence=30 / Freeze=70…）。
    pub base: f64,
    /// 参与 INC 缩放的 stat 名（如 `["OnslaughtEffect","BuffEffectOnSelf"]`）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inc_stats: Vec<String>,
    /// 参与 MORE 连乘的 stat 名（calcLib.mod 语义）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub more_stats: Vec<String>,
    /// effect 取整方式（PoB2 多为 `m_floor`）。
    #[serde(default)]
    pub rounding: Rounding,
    /// effect 下界（Freeze 的 `m_max(…, 0)`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    /// effect 上界。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
}

/// 取整方式。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rounding {
    /// 不取整。
    #[default]
    None,
    /// 向下取整（`m_floor`）。
    Floor,
}

/// 单条 mod 模板。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuffModTemplate {
    /// ModName。
    pub name: String,
    /// mod 类型 vendor 字面量（`BASE/INC/MORE/FLAG`）。
    pub mod_type: String,
    /// 数值模板。
    pub value: BuffModValue,
    /// ModFlag 名（vendor 渲染名，如 `Attack`/`Cast`/`Sword`；消费侧映射
    /// 位枚举，未知名记 diagnostics 并跳过该 mod——保守不发错值）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<String>,
    /// 受限 tag（复用 config DSL 白名单）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<EffectTag>,
}

/// mod 数值模板（双取整形态：effect 级取整 vs 逐 mod 取整）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BuffModValue {
    /// 字面量（不吃 effect 缩放，如 HerEmbrace）。
    Literal {
        /// 字面值。
        value: f64,
    },
    /// `coeff × effect`（effect 已按公式取整——Onslaught 的 `2 × effect`）。
    PerEffect {
        /// 系数。
        coeff: f64,
    },
    /// `rounding(coeff × scale)`（逐 mod 取整——Adrenaline 的
    /// `m_floor(25 × effectMod)`；scale = 未取整缩放系数）。
    ScaledRounded {
        /// 系数。
        coeff: f64,
        /// 取整方式。
        #[serde(default)]
        rounding: Rounding,
    },
}

/// vendor 行段引用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VendorRef {
    /// vendor src 相对路径（如 `Modules/CalcPerform.lua`）。
    pub file: String,
    /// 起始行（1-based，含）。
    pub line_start: u32,
    /// 结束行（1-based，含）。
    pub line_end: u32,
    /// 行段 hash（`fnv1a64:<16hex>`，drift 告警对账；非加密用途）。
    pub segment_hash: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn onslaught() -> BuffDef {
        BuffDef {
            id: "Onslaught".to_string(),
            trigger_flag: "Onslaught".to_string(),
            mode_gate: BuffModeGate::Combat,
            effect: Some(BuffEffectFormula {
                base: 10.0,
                inc_stats: vec![
                    "OnslaughtEffect".to_string(),
                    "BuffEffectOnSelf".to_string(),
                ],
                more_stats: Vec::new(),
                rounding: Rounding::Floor,
                min: None,
                max: None,
            }),
            mods: vec![
                BuffModTemplate {
                    name: "Speed".to_string(),
                    mod_type: "INC".to_string(),
                    value: BuffModValue::PerEffect { coeff: 2.0 },
                    flags: vec!["Attack".to_string()],
                    tags: Vec::new(),
                },
                BuffModTemplate {
                    name: "MovementSpeed".to_string(),
                    mod_type: "INC".to_string(),
                    value: BuffModValue::PerEffect { coeff: 1.0 },
                    flags: Vec::new(),
                    tags: Vec::new(),
                },
            ],
            conditions_set: Vec::new(),
            handler_id: None,
            verified: false,
            vendor_ref: VendorRef {
                file: "Modules/CalcPerform.lua".to_string(),
                line_start: 540,
                line_end: 571,
                segment_hash: "fnv1a64:0000000000000000".to_string(),
            },
            notes: None,
        }
    }

    /// BuffDef serde 往返。
    #[test]
    fn buff_def_round_trip() {
        let def = onslaught();
        let json = serde_json::to_string_pretty(&def).unwrap();
        let back: BuffDef = serde_json::from_str(&json).unwrap();
        assert_eq!(back, def);
    }

    /// handler 条目最小形态 + 缺省回填。
    #[test]
    fn handler_entry_defaults() {
        let json = r#"{
            "id": "Fortify",
            "trigger_flag": "Fortified",
            "mode_gate": "combat",
            "handler_id": "buff:fortify",
            "vendor_ref": {
                "file": "Modules/CalcPerform.lua",
                "line_start": 523,
                "line_end": 539,
                "segment_hash": "fnv1a64:abcd"
            }
        }"#;
        let def: BuffDef = serde_json::from_str(json).unwrap();
        assert_eq!(def.handler_id.as_deref(), Some("buff:fortify"));
        assert!(def.mods.is_empty());
        assert!(def.effect.is_none());
        assert_eq!(def.effect.and_then(|e| e.max), None);
    }

    /// Rounding 缺省 = None；BuffModValue 三形态往返。
    #[test]
    fn mod_value_variants_round_trip() {
        let values = vec![
            BuffModValue::Literal { value: 100.0 },
            BuffModValue::PerEffect { coeff: 2.0 },
            BuffModValue::ScaledRounded {
                coeff: 25.0,
                rounding: Rounding::Floor,
            },
        ];
        for value in values {
            let json = serde_json::to_string(&value).unwrap();
            let back: BuffModValue = serde_json::from_str(&json).unwrap();
            assert_eq!(back, value);
        }
        let json = r#"{"kind":"scaled_rounded","coeff":0.3}"#;
        let back: BuffModValue = serde_json::from_str(json).unwrap();
        assert_eq!(
            back,
            BuffModValue::ScaledRounded {
                coeff: 0.3,
                rounding: Rounding::None
            }
        );
    }
}
