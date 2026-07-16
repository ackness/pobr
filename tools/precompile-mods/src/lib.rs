//! `precompile-mods` 库面：供 `main.rs` 与集成测试共用。
//!
//! 工具职责见 `main.rs` 文件级文档。模块划分：
//! - [`corpus`]：四层语料收集器（蓝图 §5.1）；
//! - [`canonical`]：Modifier 的 byte-stable canonical 形态；
//! - [`parsed`]：逐行预解析 → `parsed_mods.json` + 覆盖率统计；
//! - [`report`]：覆盖率报表 → `parse-coverage.json`；
//! - [`check`]：`--check` overlay JSON 合法性校验（贡献者门禁）。

pub mod canonical;
pub mod check;
pub mod corpus;
pub mod parsed;
pub mod report;
