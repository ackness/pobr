//! Trade 后端抽象。

use crate::errors::TradeError;
use crate::query::TradeQuery;
use crate::types::{SearchResult, TradeItem};

/// 交易后端的抽象接口：先 [`search`](TradeBackend::search) 拿到 listing id，
/// 再 [`fetch`](TradeBackend::fetch) 按 id 拉取详情。
///
/// # 同步设计说明
///
/// 此 trait 故意采用**同步**签名，便于在离线 mock 下进行确定性测试，
/// 不引入运行时依赖。真实的官方 trade API 客户端走 async / `reqwest`
/// （HTTP + 速率限制 + JSON 解析），将在后续阶段以独立适配器实现，并把
/// 网络层结果映射到本 trait 的同步契约（或提供平行的 async trait）。
pub trait TradeBackend {
    /// 按查询条件搜索，返回命中的 listing id 列表。
    fn search(&self, league: &str, query: &TradeQuery) -> Result<SearchResult, TradeError>;

    /// 按 id 列表拉取物品详情。
    fn fetch(&self, league: &str, ids: &[String]) -> Result<Vec<TradeItem>, TradeError>;
}
