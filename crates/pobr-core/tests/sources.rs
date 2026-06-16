//! 来源接入：item / passive / gem / flask / minion 词条 ingest。
//!
//! 聚合二进制：原独立测试文件合并为子模块，减少链接二进制数（53→8）以加速构建。
//! 各子模块即 `tests/sources/<name>.rs`，测试用例与断言逐一保留。
#![allow(clippy::all)]

#[path = "sources/env_finalize_buffs.rs"]
mod env_finalize_buffs;
#[path = "sources/env_finalize_flasks.rs"]
mod env_finalize_flasks;
#[path = "sources/item_source.rs"]
mod item_source;
#[path = "sources/item_text.rs"]
mod item_text;
#[path = "sources/minion.rs"]
mod minion;
#[path = "sources/passive_source.rs"]
mod passive_source;
#[path = "sources/skill_source.rs"]
mod skill_source;
