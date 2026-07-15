//! 解析层：modifier 文本 → Mod（mod_parser / special / modcache golden）。
//!
//! 聚合二进制：原独立测试文件合并为子模块，减少链接二进制数（53→8）以加速构建。
//! 各子模块即 `tests/parser/<name>.rs`，测试用例与断言逐一保留。
#![allow(clippy::all)]

#[path = "support/parse.rs"]
mod support;

#[path = "parser/mod_parser.rs"]
mod mod_parser;
#[path = "parser/parser_modcache_golden.rs"]
mod parser_modcache_golden;
#[path = "parser/special_mods_gate.rs"]
mod special_mods_gate;
