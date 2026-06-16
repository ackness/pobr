//! 聚合层：ModDb 存储 / 查询 / 归因 trace / 敌方 ModDb。
//!
//! 聚合二进制：原独立测试文件合并为子模块，减少链接二进制数（53→8）以加速构建。
//! 各子模块即 `tests/aggregation/<name>.rs`，测试用例与断言逐一保留。
#![allow(clippy::all)]

#[path = "aggregation/enemy_mod_db.rs"]
mod enemy_mod_db;
#[path = "aggregation/mod_db.rs"]
mod mod_db;
#[path = "aggregation/mod_db_traced.rs"]
mod mod_db_traced;
#[path = "aggregation/trace.rs"]
mod trace;
