//! Build 层所需的纯配置枚举。
//!
//! `BuildConfig` 本体（含 `to_calc_config()`）归 `pobr-build`，因其依赖 `pobr-core`，
//! 不允许出现在 `pobr-data`。这里只放语言无关、零逻辑的稳定枚举。

use serde::{Deserialize, Serialize};

/// PoB Build 的当前视图状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ViewMode {
    Calcs,
    Items,
    #[default]
    Tree,
    Skills,
    Config,
    Import,
}

/// 盗贼任务奖励选择（PoB Build XML 兼容字段）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum BanditChoice {
    #[default]
    None,
    Kraityn,
    Alira,
    Oak,
}
