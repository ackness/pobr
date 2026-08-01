//! `overlay/config_options.json` loader——config 选项目录
//! （542+ 条目的声明式 effects DSL + handler_id 例外），schema 见
//! [`pobr_data::catalog::config_def`]。
//!
//! 产物来源 `sync-pob-catalog extract-lua --what config-options`（探针法
//! 归纳 ConfigOptions.lua apply 闭包）；消费侧 =
//! `pobr-core::rules::config_interpreter`。

use pobr_data::catalog::config_def::ConfigOptionsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// 加载 config 选项目录（恒走 `overlay/` 定位；`_meta` 由 serde 忽略）。
    ///
    /// 缺表时调用方按缺表容忍回退旧路径（`xml_build::parse_config` 既有分支）。
    pub fn config_options(&self) -> Result<ConfigOptionsDef, LoadError> {
        self.load_json_at(self.overlay_path("config_options.json"))
    }
}
