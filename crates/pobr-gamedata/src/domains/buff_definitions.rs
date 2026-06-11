//! `overlay/buff_definitions.json` loader——内建 buff 定义
//! （doActorMisc 人工归纳，00-index 裁决 §4.2-4 批准的 overlay 例外），
//! schema 见 [`pobr_data::catalog::buffs`]。
//!
//! drift 防线在工具侧：`sync-pob-catalog check-buff-refs` 对账 vendor_ref
//! 行段 hash；消费侧 = `pobr-core::rules::buff_expander`（M3 主波 T3 接线）。

use pobr_data::catalog::buffs::BuffDefinitionsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// 加载内建 buff 定义（恒走 `overlay/` 定位；`_meta` 由 serde 忽略）。
    pub fn buff_definitions(&self) -> Result<BuffDefinitionsDef, LoadError> {
        self.load_json_at(self.overlay_path("buff_definitions.json"))
    }
}
