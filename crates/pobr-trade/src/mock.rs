//! 离线 mock 后端：返回预置数据，可配置错误以覆盖错误路径。测试不联网。

use crate::backend::TradeBackend;
use crate::errors::TradeError;
use crate::query::TradeQuery;
use crate::types::{SearchResult, TradeItem};

/// 实现 [`TradeBackend`] 的内存 mock。
///
/// - [`search`](TradeBackend::search) 返回预置物品的全部 id（保持插入顺序）。
/// - [`fetch`](TradeBackend::fetch) 按请求的 id 顺序返回匹配项，跳过未知 id。
/// - 通过 [`failing`](MockBackend::failing) 注入固定错误，使 search / fetch
///   均返回该错误，用于测试 RateLimit / NotFound / Http 等错误路径。
#[derive(Debug, Clone, Default)]
pub struct MockBackend {
    items: Vec<TradeItem>,
    forced_error: Option<TradeError>,
}

impl MockBackend {
    /// 以预置物品构造 mock 后端。
    pub fn with_items(items: Vec<TradeItem>) -> Self {
        Self {
            items,
            forced_error: None,
        }
    }

    /// 返回一个会让所有调用都失败为 `error` 的副本。
    pub fn failing(mut self, error: TradeError) -> Self {
        self.forced_error = Some(error);
        self
    }

    /// 当前预置物品的只读视图。
    pub fn items(&self) -> &[TradeItem] {
        &self.items
    }
}

impl TradeBackend for MockBackend {
    fn search(&self, _league: &str, _query: &TradeQuery) -> Result<SearchResult, TradeError> {
        if let Some(error) = &self.forced_error {
            return Err(error.clone());
        }
        let ids = self.items.iter().map(|item| item.id.clone()).collect();
        Ok(SearchResult { ids })
    }

    fn fetch(&self, _league: &str, ids: &[String]) -> Result<Vec<TradeItem>, TradeError> {
        if let Some(error) = &self.forced_error {
            return Err(error.clone());
        }
        let items = ids
            .iter()
            .filter_map(|id| self.items.iter().find(|item| &item.id == id).cloned())
            .collect();
        Ok(items)
    }
}
