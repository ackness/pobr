//! `overlay/spectres.json` loader——魂灵（spectre）条目（vendor
//! `Data/Spectres.lua` 经 `extract-lua --what spectres` 抽取，与 minions 同
//! schema，key 为完整 metadata 路径；/A5）。
//!
//! 体积注：~700KB 级，懒加载域——只在消费方显式调用时读盘，不进默认
//! 热路径；`BuildData` 接入约定 minions 优先、miss 落 spectres。

use pobr_data::catalog::actors::MinionsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// 加载魂灵条目表（恒走 `overlay/` 定位）。文件缺失返回 `Ok(None)`
    /// （缺表容忍）；其余错误照常上抛。
    pub fn spectres(&self) -> Result<Option<MinionsDef>, LoadError> {
        match self.load_json_at::<MinionsDef>(self.overlay_path("spectres.json")) {
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
