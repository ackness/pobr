//! 归因层——「每个输出回溯到词条」。
//!
//! 词条分层叙事的最后一环（见 crate 根 `lib.rs` 的总览），也是 **PoBR 相对 PoB
//! 的核心增量**：PoB 只算出面板数字，PoBR 还能告诉你「这个 DPS 由哪些词条、各
//! 贡献多少」。
//! - [`trace`]：计算过程的 DAG [`TraceGraph`](trace::TraceGraph)——聚合查询时由
//!   `*_traced` 变体逐贡献建图，把每个输出连回 [`SourceId`](pobr_data::source::SourceId)。
//! - [`attribution`]：在 trace 之上给出 direct / marginal / interaction 三种口径的
//!   [`AttributionReport`](attribution::AttributionReport)（marginal 通过
//!   [`ModDb::filtered`](crate::ModDb::filtered) 去掉某来源后重算得到）。

pub mod attribution;
pub mod trace;
