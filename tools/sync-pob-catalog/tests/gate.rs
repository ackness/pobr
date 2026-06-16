//! catalog diff 与 parity 门禁。
//!
//! 聚合二进制：原独立测试文件合并为子模块（7→2），减少链接二进制数以加速构建。
#![allow(clippy::all)]

#[path = "gate/catalog_diff.rs"]
mod catalog_diff;
#[path = "gate/parity_gate.rs"]
mod parity_gate;
