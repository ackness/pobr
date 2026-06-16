//! 异常状态：magnitude 与应用（点燃 / 冰缓 / 感电 / 中毒 / 流血）。
//!
//! 聚合二进制：原独立测试文件合并为子模块，减少链接二进制数（53→8）以加速构建。
//! 各子模块即 `tests/ailments/<name>.rs`，测试用例与断言逐一保留。
#![allow(clippy::all)]

#[path = "ailments/ailment.rs"]
mod ailment;
#[path = "ailments/ailment_apply.rs"]
mod ailment_apply;
