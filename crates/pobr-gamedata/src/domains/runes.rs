//! `overlay/runes.json` loader——符文 / 魂核镶嵌词条表（vendor
//! `Data/ModRunes.lua` 经 `extract-lua --what runes` 抽取，schema 见
//! [`pobr_data::catalog::item_overlay`]）。
//!
//! 消费侧按需单独加载，不进 ItemRules。

use pobr_data::catalog::item_overlay::RunesDef;

use crate::{GameData, LoadError};

impl GameData {
    /// 加载符文表（恒走 `overlay/` 定位）。文件缺失返回 `Ok(None)`
    /// （缺表容忍）；其余错误照常上抛。
    pub fn runes(&self) -> Result<Option<RunesDef>, LoadError> {
        match self.load_json_at::<RunesDef>(self.overlay_path("runes.json")) {
            Ok(def) => Ok(Some(def)),
            Err(LoadError::Io { ref source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }
}
