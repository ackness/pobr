//! Build Code 编解码 + config fixtures。
//!
//! 聚合二进制：原独立测试文件合并为子模块（22→4），减少链接二进制数以加速构建。
#![allow(clippy::all)]

#[path = "codec/build_code.rs"]
mod build_code;
#[path = "codec/config_fixtures.rs"]
mod config_fixtures;
