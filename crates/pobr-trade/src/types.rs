//! Trade 层返回的数据类型（纯数据，可 serde）。

use serde::{Deserialize, Serialize};

/// 一件可交易物品的最小价格描述。
///
/// 价格统一折算为 chaos（混沌石），便于跨货币比较；具体折算逻辑由真实
/// 后端实现，本数据结构仅承载结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TradeItem {
    /// 官方 trade listing 的稳定标识。
    pub id: String,
    /// 物品显示名（base / unique 名称）。
    pub name: String,
    /// 折算为 chaos 的价格。
    pub price_chaos: f64,
}

/// 一次搜索返回的 listing id 列表。
///
/// 官方 API 中 search 与 fetch 分两步：search 返回 id 列表，fetch 再按 id
/// 拉取详情。此结构对应第一步的结果。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchResult {
    /// 命中的 listing id，保持后端返回顺序。
    pub ids: Vec<String>,
}
