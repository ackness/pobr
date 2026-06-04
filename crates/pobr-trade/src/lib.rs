//! pobr-trade：trade query / price 的抽象与离线 mock。
//!
//! 定义官方 trade API 交互所需的纯数据类型（[`TradeQuery`] / [`StatFilter`] /
//! [`SearchResult`] / [`TradeItem`]）、错误类型（[`TradeError`]）以及后端抽象
//! [`TradeBackend`]。本阶段只提供同步接口与内存 [`MockBackend`]，**测试不联网**；
//! 真实的 async / reqwest HTTP 实现延后到后续阶段。

pub mod backend;
pub mod errors;
pub mod mock;
pub mod query;
pub mod types;

pub use backend::TradeBackend;
pub use errors::TradeError;
pub use mock::MockBackend;
pub use query::{StatFilter, TradeQuery};
pub use types::{SearchResult, TradeItem};
