//! 迁移期新旧双跑 harness(statmap / config)——迁移切换完成后可整组删除。
//!
//! 聚合二进制：原独立测试文件合并为子模块（22→4），减少链接二进制数以加速构建。
#![allow(clippy::all)]

#[path = "dualrun/config_dualrun.rs"]
mod config_dualrun;
#[path = "dualrun/statmap_dual_run.rs"]
mod statmap_dual_run;
