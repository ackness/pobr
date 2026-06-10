//! `base/jewel_radii.json` loader——范围珠宝环形档（距离乘数 + 按树版本的
//! label/inner/outer 档位），schema 见 [`pobr_data::catalog::jewel_radii`]。
//!
//! 源 PoB2 `src/Modules/Data.lua:595-611` + `src/Data/Misc.lua:36`；
//! pobr Rust 准源为 `crates/pobr-tree/src/radius_jewel.rs`（4 个具名档逐值一致）。

use pobr_data::catalog::jewel_radii::JewelRadiiDef;

use crate::{GameData, LoadError};

impl GameData {
    /// 加载范围珠宝环形档表（走 `base/` 优先、版本根回退的域定位）。
    pub fn jewel_radii(&self) -> Result<JewelRadiiDef, LoadError> {
        self.load_domain("jewel_radii.json")
    }
}
