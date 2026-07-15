//! PoB2 golden 对照与展示字段目录。
//!
//! 聚合二进制：原独立测试文件合并为子模块，减少链接二进制数（53→8）以加速构建。
//! 各子模块即 `tests/golden/<name>.rs`，测试用例与断言逐一保留。
#![allow(clippy::all)]

#[path = "support/parse.rs"]
mod support;

#[path = "golden/display_catalog.rs"]
mod display_catalog;
#[path = "golden/pob2_golden.rs"]
mod pob2_golden;
