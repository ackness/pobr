//! `overlay/mod_scalability.json` loader——`{range:x}` 词条取值的可缩放性 +
//! 格式换算表（vendor `Data/ModScalability.lua` 经
//! `extract-lua --what mod-scalability` 抽取，schema 见
//! [`pobr_data::catalog::item_overlay`]，M5c 蓝图 WI-B1/B2）。
//!
//! 体积注（R6）：~4MB 级，懒加载域；消费侧（M5c 主波 WI-B3
//! `pobr-core::apply_range`）经 RuleSet `ItemRules` 注入，本波次零接线。

use pobr_data::catalog::item_overlay::ModScalabilityDef;

use crate::{GameData, LoadError};

impl GameData {
    /// 加载可缩放性表（恒走 `overlay/` 定位）。文件缺失返回 `Ok(None)`
    /// （消费侧降级 = 朴素线性取值 + `approx` 标记，M5c 蓝图「无表降级」）；
    /// 其余错误照常上抛。
    pub fn mod_scalability(&self) -> Result<Option<ModScalabilityDef>, LoadError> {
        match self.load_json_at::<ModScalabilityDef>(self.overlay_path("mod_scalability.json")) {
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
