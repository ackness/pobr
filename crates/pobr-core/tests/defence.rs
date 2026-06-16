//! 防御侧：护甲 / 闪避 / ES / EHP / keystone / 恢复 / 抗性 / 生存。
//!
//! 聚合二进制：原独立测试文件合并为子模块，减少链接二进制数（53→8）以加速构建。
//! 各子模块即 `tests/defence/<name>.rs`，测试用例与断言逐一保留。
#![allow(clippy::all)]

#[path = "defence/defence_ext.rs"]
mod defence_ext;
#[path = "defence/defence_panels.rs"]
mod defence_panels;
#[path = "defence/ehp.rs"]
mod ehp;
#[path = "defence/ehp_pob2.rs"]
mod ehp_pob2;
#[path = "defence/evade_stun.rs"]
mod evade_stun;
#[path = "defence/keystone_defence.rs"]
mod keystone_defence;
#[path = "defence/recovery.rs"]
mod recovery;
#[path = "defence/resistance_cap.rs"]
mod resistance_cap;
#[path = "defence/stat_boundary.rs"]
mod stat_boundary;
#[path = "defence/survivability.rs"]
mod survivability;
