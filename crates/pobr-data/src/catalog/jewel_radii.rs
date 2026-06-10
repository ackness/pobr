//! 范围珠宝环形档域 schema（`base/jewel_radii.json`）。
//!
//! 数据来源：
//! - 档位表：vendor PoB2 `src/Modules/Data.lua:595-611` `data.jewelRadii["0_1"]`
//!   （4 个具名档 Small/Medium/Large/Very Large + 8 个 `inner > 0` 的 Variable 环形档）；
//! - 距离乘数：vendor PoB2 `src/Data/Misc.lua:36`
//!   `gameConstants["PassiveTreeJewelDistanceMultiplier"] = 1.2`（转录自 `GameConstants.dat`）。
//!
//! pobr 现有 Rust 准源：`crates/pobr-tree/src/radius_jewel.rs` 的
//! `PASSIVE_TREE_JEWEL_DISTANCE_MULTIPLIER` 与 `JEWEL_RADIUS_{SMALL,MEDIUM,LARGE,VERY_LARGE}`
//! （4 个具名档的 outer 与乘数已逐值一致）；`inner`/`colour` 与 8 个 Variable 档为
//! vendor-only 补全（pobr-tree first pass 用 `JewelRadius::Custom` 兜 Variable，不带 inner 语义）。
//!
//! 距离判定语义（对照 PoB2 `Modules/Data.lua:584-586` `setJewelRadiiGlobally`）：
//! 节点落入环形档 ⇔ `inner² × m² <= dx² + dy² <= outer² × m²`，其中
//! `m = distance_multiplier`；即 inner/outer 均为未乘缩放因子的基础半径（tree units）。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// `base/jewel_radii.json` 顶层：距离乘数 + 按树版本的环形档位表。
///
/// 树版本键形如 `"0_1"`（major_minor）；运行时按 PoB2 `setJewelRadiiGlobally`
/// 语义选取 `<=` 目标树版本中最新的一组（当前数据只有 `0_1` 一组）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JewelRadiiDef {
    /// 天赋树珠宝距离缩放因子（`PassiveTreeJewelDistanceMultiplier`，当前 1.2）。
    ///
    /// 来源：`Data/Misc.lua:36`（GameConstants.dat）。pobr 准源：
    /// `pobr-tree::PASSIVE_TREE_JEWEL_DISTANCE_MULTIPLIER`（逐值一致）。
    pub distance_multiplier: f64,
    /// `树版本 -> 环形档位数组`（保持 vendor `Data.lua` 内的书写顺序：
    /// 4 个具名档在前、8 个 Variable 档在后）。
    pub tree_versions: BTreeMap<String, Vec<JewelRadiusBandDef>>,
}

impl Default for JewelRadiiDef {
    /// `Default` = 注入缺失时的 fallback，与 `base/jewel_radii.json` **逐值相等**
    /// （搬迁不变式：注入与回退两条路径输出一致）。
    ///
    /// 本域的 pobr Rust 准源（`pobr-tree::PASSIVE_TREE_JEWEL_DISTANCE_MULTIPLIER` /
    /// `JEWEL_RADIUS_*`）位于**上游依赖方向不可引用**的 crate（pobr-tree 依赖
    /// pobr-data），故此处以字面量转录 vendor PoB2 `Modules/Data.lua:595-611` +
    /// `Data/Misc.lua:36`；逐值一致性由两道测试锁定：
    /// - `pobr-tree` `tests/radius_jewel.rs`：Default 具名档 × 乘数 == 旧常量；
    /// - `pobr-gamedata` `tests/load_jewel_radii.rs`：Default == JSON 全等。
    fn default() -> Self {
        // 档位行构造便捷函数（仅 Default 用）。
        fn band(label: &str, inner: u32, outer: u32, colour: &str) -> JewelRadiusBandDef {
            JewelRadiusBandDef {
                label: label.to_string(),
                inner,
                outer,
                colour: colour.to_string(),
            }
        }
        // vendor `Modules/Data.lua:595-611` `data.jewelRadii["0_1"]`：
        // 4 个具名档（inner=0 实心圆）在前、8 个 Variable 环形档在后（保持书写顺序）。
        let bands = vec![
            band("Small", 0, 1000, "^xBB6600"),
            band("Medium", 0, 1150, "^x66FFCC"),
            band("Large", 0, 1300, "^x2222CC"),
            band("Very Large", 0, 1500, "^xC100FF"),
            band("Variable", 650, 950, "^xD35400"),
            band("Variable", 800, 1100, "^x66FFCC"),
            band("Variable", 950, 1250, "^x2222CC"),
            band("Variable", 1100, 1400, "^xC100FF"),
            band("Variable", 1250, 1550, "^x0B9300"),
            band("Variable", 1400, 1700, "^xFFCC00"),
            band("Variable", 1650, 1950, "^xFF6600"),
            band("Variable", 1800, 2100, "^x0099FF"),
        ];
        Self {
            // `Data/Misc.lua:36` PassiveTreeJewelDistanceMultiplier（GameConstants.dat）。
            distance_multiplier: 1.2,
            tree_versions: BTreeMap::from([("0_1".to_string(), bands)]),
        }
    }
}

/// 一个环形档位（对应 PoB2 `data.jewelRadii` 的一行）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JewelRadiusBandDef {
    /// 档位标签：`Small` / `Medium` / `Large` / `Very Large` / `Variable`。
    pub label: String,
    /// 环形内半径基础值（tree units，未乘缩放因子）；具名档为 0（实心圆）。
    /// vendor-only 字段（pobr-tree first pass 无 inner 语义），源 `Modules/Data.lua:602-609`。
    pub inner: u32,
    /// 环形外半径基础值（tree units，未乘缩放因子）。
    /// 具名档与 pobr-tree `JEWEL_RADIUS_*`（= outer × 1.2）逐值一致。
    pub outer: u32,
    /// UI 高亮颜色码（PoB2 `col` 字段，`^xRRGGBB` 格式）。vendor-only 展示字段。
    pub colour: String,
}

#[cfg(test)]
mod tests {
    use super::JewelRadiiDef;

    /// Default fallback 结构抽查（完整逐值对照：pobr-tree 旧常量见
    /// `pobr-tree/tests/radius_jewel.rs`，JSON 全等见
    /// `pobr-gamedata/tests/load_jewel_radii.rs`）。
    #[test]
    fn default_has_vendor_zero_one_table() {
        let def = JewelRadiiDef::default();
        assert_eq!(def.distance_multiplier, 1.2);
        let bands = &def.tree_versions["0_1"];
        assert_eq!(bands.len(), 12, "4 具名档 + 8 Variable 档");
        assert_eq!(bands[0].label, "Small");
        assert_eq!(bands[0].outer, 1000);
        assert_eq!(bands[3].label, "Very Large");
        assert_eq!(bands[3].outer, 1500);
        // Variable 档环宽恒为 300（vendor 表结构性质）。
        assert!(
            bands
                .iter()
                .filter(|b| b.label == "Variable")
                .all(|b| b.outer - b.inner == 300)
        );
    }
}
