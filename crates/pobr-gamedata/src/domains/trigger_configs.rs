//! `overlay/trigger_configs.json` loader——vendor `Modules/CalcTriggers.lua`
//! configTable 61 项触发配置（`sync-pob-catalog gen-trigger-configs` 内嵌生成
//! + vendor key 扫描对账；schema 见 [`pobr_data::catalog::triggers`]）。
//!
//! 消费侧：`pobr-build` 的 `BuildData::load` 投影为按 `match_effect_ids` 索引的
//! 识别表，orchestrator 触发段据此识别 gem-link/triggeredBy 关系；
//! 真逻辑条目按 `handler_id` 留待 registry（零接线，计数监控 <100）。

use pobr_data::catalog::triggers::TriggerConfigsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// 加载触发配置表（恒走 `overlay/` 定位）。文件缺失返回 `Ok(None)`
    /// （缺表容忍；旧数据包识别面保持空 = 行为不变）；其余错误照常上抛。
    pub fn trigger_configs(&self) -> Result<Option<TriggerConfigsDef>, LoadError> {
        match self.load_json_at::<TriggerConfigsDef>(self.overlay_path("trigger_configs.json")) {
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
