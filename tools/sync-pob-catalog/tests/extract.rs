//! 抽取逻辑：vendor Lua 抽取 / parser rules / quality / special_mods / scan。
//!
//! 聚合二进制：原独立测试文件合并为子模块（7→2），减少链接二进制数以加速构建。
#![allow(clippy::all)]

#[path = "extract/extract_lua.rs"]
mod extract_lua;
#[path = "extract/extract_parser_rules.rs"]
mod extract_parser_rules;
#[path = "extract/extract_quality.rs"]
mod extract_quality;
#[path = "extract/scan.rs"]
mod scan;
#[path = "extract/special_mods_patterns.rs"]
mod special_mods_patterns;
