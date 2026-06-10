//! `base/game_constants.json` loader——character/monster/game 三段全局常数，
//! schema 见 [`pobr_data::catalog::game_constants`]。
//!
//! pobr Rust 准源为 `crates/pobr-data/src/constants.rs`（顶层常量 +
//! `GameConstants::poe2()`，逐值一致）；vendor-only 字段抽取自 PoB2
//! `src/Data/Misc.lua` + `src/Modules/Data.lua`（data.misc，L171-250）。

use pobr_data::catalog::game_constants::GameConstantsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// 加载全局游戏常数表（走 `base/` 优先、版本根回退的域定位）。
    pub fn game_constants(&self) -> Result<GameConstantsDef, LoadError> {
        self.load_domain("game_constants.json")
    }
}
