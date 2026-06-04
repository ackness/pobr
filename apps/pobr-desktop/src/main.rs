//! pobr-desktop：桌面 GUI 入口（最小可编译骨架）。
//!
//! GUI 框架（计划采用 [egui](https://github.com/emilk/egui) / `eframe`）尚未引入：
//! headless CI 环境无法验证 GUI，且重 GUI 依赖会拖慢编译，故延后。
//!
//! 当前 `main` 仅作为未来 GUI 的占位/烟雾测试：构造一个内置示例 Build、跑一次
//! `pobr-build` 编排计算、用 `pobr-i18n` 翻译标题，并把结果摘要打印到 stdout。可测的
//! 构造/摘要逻辑见 [`pobr_desktop`] 库部分。

use pobr_desktop::{build_summary, example_build};

fn main() {
    let build = example_build();
    match build_summary(&build, "en-US") {
        Ok(summary) => print!("{summary}"),
        Err(err) => eprintln!("calculation failed: {err}"),
    }
}
