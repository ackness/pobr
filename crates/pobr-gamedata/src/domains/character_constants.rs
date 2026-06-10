//! `base/character_constants.json` loader（角色等级/属性派生常量，数值迁出
//! `pobr-core::character`，源 PoB2 `data.characterConstants`）。

use pobr_data::catalog::character_constants::CharacterConstantsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// 加载角色基础常量（单对象域：整文件即一个 [`CharacterConstantsDef`]）。
    ///
    /// 走三层目录定位（`base/` 优先，版本根回退，见 [`crate::paths`]）。
    pub fn character_constants(&self) -> Result<CharacterConstantsDef, LoadError> {
        self.load_domain("character_constants.json")
    }
}
