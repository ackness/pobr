//! `base/base_player_mods.json` loader（玩家固有基线 mod，源 PoB2
//! `CalcSetup.lua` `initModDB` + initEnv 基线段 608-678）。
//!
//! 走 `load_domain`（`base/` 优先、版本根回退，见 [`crate::paths`]）。表内容只收录
//! pobr 现有 Rust 准源已有数值的条目（搬迁不变式），逐条对照测试见
//! `tests/load_base_player_mods.rs`。

use pobr_data::catalog::base_player_mods::BasePlayerModDef;

use crate::{GameData, LoadError};

impl GameData {
    /// 加载玩家固有基线 mod 表（保持文件内顺序：vendor 注入顺序分组）。
    pub fn base_player_mods(&self) -> Result<Vec<BasePlayerModDef>, LoadError> {
        self.load_domain("base_player_mods.json")
    }
}
