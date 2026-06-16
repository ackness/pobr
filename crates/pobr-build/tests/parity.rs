//! PoB2 对照 / golden 回归(ninja_parity / golden / pob2_parity / 各 golden / oracle / e2e)。
//!
//! 聚合二进制：原独立测试文件合并为子模块（22→4），减少链接二进制数以加速构建。
#![allow(clippy::all)]

#[path = "parity/coc_trigger_golden.rs"]
mod coc_trigger_golden;
#[path = "parity/crossbow_reload_golden.rs"]
mod crossbow_reload_golden;
#[path = "parity/defence_panels_golden.rs"]
mod defence_panels_golden;
#[path = "parity/e2e_real_build.rs"]
mod e2e_real_build;
#[path = "parity/golden_regression.rs"]
mod golden_regression;
#[path = "parity/ninja_parity.rs"]
mod ninja_parity;
#[path = "parity/pob2_parity.rs"]
mod pob2_parity;
#[path = "parity/skill_dot_golden.rs"]
mod skill_dot_golden;
#[path = "parity/special_oracle_differential.rs"]
mod special_oracle_differential;
#[path = "parity/stored_hand_output.rs"]
mod stored_hand_output;
