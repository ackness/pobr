//! `overlay/curse_priority.json` loader——curse 优先级数据表（vendor
//! `Modules/Data.lua:274` 的 `data.cursePriority` 纯数据表经
//! `extract-lua --what curse-priority` 抽取，schema 见
//! [`pobr_data::catalog::curse_priority`]，M3 S1-C）。
//!
//! 消费侧（M3-T3 `calc/buff_pass.rs` 的 curse priority/limit，对照
//! `determineCursePriority` CalcPerform.lua:454-485）经接线注入，
//! 本 track 零接线。

use pobr_data::catalog::curse_priority::CursePriorityDef;

use crate::{GameData, LoadError};

impl GameData {
    /// 加载 curse 优先级表（恒走 `overlay/` 定位；`_meta` 由 serde 忽略）。
    /// 文件缺失返回 `Ok(None)`（R7 缺表容忍，消费方按旧路径回退）；
    /// 其余错误照常上抛。
    pub fn curse_priority(&self) -> Result<Option<CursePriorityDef>, LoadError> {
        match self.load_json_at::<CursePriorityDef>(self.overlay_path("curse_priority.json")) {
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
