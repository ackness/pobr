//! 计算编排：session / env / perform fill / 各 pass / 归因 / 角色基础。
//!
//! 聚合二进制：原独立测试文件合并为子模块，减少链接二进制数（53→8）以加速构建。
//! 各子模块即 `tests/engine/<name>.rs`，测试用例与断言逐一保留。
#![allow(clippy::all)]

#[path = "engine/attribution.rs"]
mod attribution;
#[path = "engine/attribution_passes.rs"]
mod attribution_passes;
#[path = "engine/calc_env.rs"]
mod calc_env;
#[path = "engine/calc_minimal.rs"]
mod calc_minimal;
#[path = "engine/calc_modules.rs"]
mod calc_modules;
#[path = "engine/calc_session.rs"]
mod calc_session;
#[path = "engine/campaign.rs"]
mod campaign;
#[path = "engine/character_base.rs"]
mod character_base;
#[path = "engine/hand_pass.rs"]
mod hand_pass;
#[path = "engine/perform_fill.rs"]
mod perform_fill;
