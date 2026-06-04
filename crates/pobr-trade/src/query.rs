//! Trade 搜索查询条件（纯数据，可 serde 回环）。

use serde::{Deserialize, Serialize};

/// 单条属性过滤：要求某 stat 落在 `[min, max]` 区间内。
///
/// `min` / `max` 任一为 `None` 表示该侧无约束。`stat` 使用与官方 trade
/// 相同的 stat 标识字符串（如 `"life"`、`"fire_resistance"`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatFilter {
    /// 目标 stat 标识。
    pub stat: String,
    /// 下界（含），`None` 表示无下界。
    pub min: Option<f64>,
    /// 上界（含），`None` 表示无上界。
    pub max: Option<f64>,
}

/// 一次 trade 搜索的完整查询条件。
///
/// 可经 serde 序列化 / 反序列化回环，便于持久化与跨进程传递。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TradeQuery {
    /// 联盟名（如 `"Standard"`）。
    pub league: String,
    /// 价格下界（chaos），`None` 表示无下界。
    pub min_price: Option<f64>,
    /// 价格上界（chaos），`None` 表示无上界。
    pub max_price: Option<f64>,
    /// 属性过滤列表，空列表表示不按属性过滤。
    pub stat_filters: Vec<StatFilter>,
}
