//! `overlay/mirage_configs.json` loader——5 类 mirage（幻影）配置
//! （vendor `Modules/CalcMirages.lua` 五分支的人工转写，由
//! `sync-pob-catalog gen-mirage-configs` 内嵌生成；schema 见
//! [`pobr_data::catalog::triggers`]）。
//!
//! 消费侧：orchestrator 识别触发条件后调
//! mirage 子环境重算框架；真特殊分支按 `handler_id` 查 rules registry。

use pobr_data::catalog::triggers::MirageConfigsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// 加载 mirage 配置表（恒走 `overlay/` 定位）。文件缺失返回 `Ok(None)`
    /// （缺表容忍）；其余错误照常上抛。
    pub fn mirage_configs(&self) -> Result<Option<MirageConfigsDef>, LoadError> {
        match self.load_json_at::<MirageConfigsDef>(self.overlay_path("mirage_configs.json")) {
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
