//! vendor PoB2 ModName → PoBR canonical StatId 别名表 schema
//! （`overlay/vendor_name_aliases.json`，`vendor_name_aliases/v1`）。
//!
//! .3 切换前置**数据资产**（见
//! `audits/rearchitecture-2026-06-10/blueprints/m6-switch-decision.md`）。
//!
//! 背景：legacy 手写 parser 产 **PoBR
//! 自有词表**（`MaximumLife`/`Strength`/`ColdResistance`…），新引擎按
//! 忠实落 **vendor PoB2 词表**（`Life`/`Str`/`ColdResist`…），全部下游消费 PoBR
//! 词表。本表用一张 `vendor_name → pobr_stat_id` 别名把两侧词表桥接起来，自举来源
//! = legacy `parse_name` 短语映射 ∩ engine `name_map`（同一触发短语对齐）。
//!
//! **零消费纪律**：本模块仅定义 serde 形状（往返单测兜底），**不被任何 calc /
//! loader / parser 路径消费**。A/B 两条切换路线（决策文档 §两条路）的接线点见
//! `m6-alias-table.md`，由 owner 定夺后于 D-T8 接入。届时本表用在「运行期翻译」
//! (A) 或「抽取期归一」(B)，schema 不变。
//!
//! 消费侧视角：serde 默认忽略顶层 `_meta` 与 `structural_deferrals`（生成溯源 /
//! 结构性分歧登记，备查不入计算，与既有 overlay schema 同款）。本模块零逻辑、
//! 零 I/O；新字段 `#[serde(default)]`。

use serde::{Deserialize, Serialize};

/// schema 标识（`_meta.schema` 期望值）。
pub const VENDOR_NAME_ALIASES_SCHEMA: &str = "vendor_name_aliases/v1";

/// 别名表顶层文档（`overlay/vendor_name_aliases.json`）。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct VendorNameAliasesDoc {
    /// vendor → PoBR 别名条目（自举 + 人工核对）。
    #[serde(default)]
    pub aliases: Vec<VendorNameAliasDef>,
}

/// 单条 vendor → PoBR 别名。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct VendorNameAliasDef {
    /// 引擎产出的 vendor PoB2 ModName（如 `Life` / `Str` / `ColdResistMax`）。
    pub vendor_name: String,
    /// 归一目标——PoBR canonical StatId（如 `MaximumLife` / `Strength`）。
    pub pobr_stat_id: String,
    /// 是否同名（`vendor_name == pobr_stat_id`）——identity 别名占多数，
    /// 真正改名（identity=false）的是切换时下游 miss 的根因子集。
    #[serde(default)]
    pub identity: bool,
    /// 自举证据：legacy 与 engine 共同识别、产出此对的触发短语（空串 = 人工补录）。
    #[serde(default)]
    pub via_phrase: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// serde 往返：顶层文档结构稳定（零消费资产的唯一保障）。
    #[test]
    fn doc_roundtrips_through_json() {
        let doc = VendorNameAliasesDoc {
            aliases: vec![
                VendorNameAliasDef {
                    vendor_name: "Life".into(),
                    pobr_stat_id: "MaximumLife".into(),
                    identity: false,
                    via_phrase: "maximum life".into(),
                },
                VendorNameAliasDef {
                    vendor_name: "Armour".into(),
                    pobr_stat_id: "Armour".into(),
                    identity: true,
                    via_phrase: "armour".into(),
                },
            ],
        };
        let json = serde_json::to_string(&doc).expect("serialize");
        let back: VendorNameAliasesDoc = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(doc, back);
    }

    /// 缺省字段：仅 `aliases` 的精简 JSON 也能反序列化；顶层 `_meta` /
    /// `structural_deferrals` 被 serde 忽略（不破坏消费侧）。
    #[test]
    fn minimal_json_deserializes_and_ignores_meta() {
        let json = r#"{
            "_meta": {"schema": "vendor_name_aliases/v1"},
            "structural_deferrals": {"note": "see m6-alias-table.md"},
            "aliases": [{"vendor_name": "Str", "pobr_stat_id": "Strength"}]
        }"#;
        let doc: VendorNameAliasesDoc = serde_json::from_str(json).expect("deserialize");
        assert_eq!(doc.aliases.len(), 1);
        assert_eq!(doc.aliases[0].vendor_name, "Str");
        assert_eq!(doc.aliases[0].pobr_stat_id, "Strength");
        assert!(!doc.aliases[0].identity);
        assert!(doc.aliases[0].via_phrase.is_empty());
    }
}
