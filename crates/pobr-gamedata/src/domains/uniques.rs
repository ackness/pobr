//! `overlay/uniques.json` loader——传奇物品 raw 文本块 + 预解析索引
//! （双层；vendor `Data/Uniques/*.lua` 经 `extract-lua --what uniques`
//! 抽取，schema 见 [`pobr_data::catalog::item_overlay`]）。
//!
//! 消费侧按需单独加载，
//! 不进 ItemRules；词条模板行解析由 pobr-item 运行时做。

use pobr_data::catalog::item_overlay::UniquesDef;

use crate::{GameData, LoadError};

impl GameData {
    /// 加载传奇表（恒走 `overlay/` 定位）。文件缺失返回 `Ok(None)`
    /// （缺表容忍）；其余错误照常上抛。
    pub fn uniques(&self) -> Result<Option<UniquesDef>, LoadError> {
        match self.load_json_at::<UniquesDef>(self.overlay_path("uniques.json")) {
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
