//! `overlay/high_precision_mods.json` loader——取整精度例外表
//! （`defaultHighPrecision` + `mod name → mod type → 精度位数`），
//! schema 见 [`pobr_data::catalog::high_precision_mods`]。
//!
//! 源 vendor PoB2 `src/Modules/Data.lua:413-530`；消费点（ScaleAddMod /
//! MORE 精度例外分支）pobr 当前未实现，本 loader 先随数据落地，零接线。

use pobr_data::catalog::high_precision_mods::HighPrecisionModsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// 加载取整精度例外表（单对象域，恒走 `overlay/` 定位，缺文件即报错）。
    pub fn high_precision_mods(&self) -> Result<HighPrecisionModsDef, LoadError> {
        self.load_json_at(self.overlay_path("high_precision_mods.json"))
    }
}
