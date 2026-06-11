//! `overlay/catalysts.json` loader——催化剂品质标签匹配表（vendor
//! `Classes/Item.lua:14-29` 三个 local 表字面量经
//! `extract-lua --what catalysts` 抽取，schema 见
//! [`pobr_data::catalog::item_overlay`]，M5c 蓝图 WI-B1/B2）。
//!
//! 消费侧（M5c 主波 WI-B3 `getCatalystScalar` 等价：catalystTags ∩ mod tags
//! 匹配给 `(100+quality)/100`）经 RuleSet `ItemRules` 注入，本波次零接线。

use pobr_data::catalog::item_overlay::CatalystsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// 加载催化剂表（恒走 `overlay/` 定位）。文件缺失返回 `Ok(None)`
    /// （R7 缺表容忍）；其余错误照常上抛。
    pub fn catalysts(&self) -> Result<Option<CatalystsDef>, LoadError> {
        match self.load_json_at::<CatalystsDef>(self.overlay_path("catalysts.json")) {
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
