//! special 词条 handler 实现。
//!
//! 受限模板 DSL（[`crate::rules::special_mod`]）无法表达的真逻辑 special 条目，
//! 在 `overlay/special_mods.json` 里只记一个稳定 `handler_id`（命名 `special:<name>`）；
//! 运行时由本目录注册的 [`HandlerRegistry`] handler 裁决执行。
//!
//! **DSL 硬边界监控**（20-target-architecture §5）：handler 总数 < 100、占 special
//! 总条目 <10%（闸门测试 `special_mods_gate.rs` 守）。逼近上限即判数据切分失败、
//! 能模板化的条目一律走 DSL，handler 只接「条件分支 / 跨域 LIST
//! 载荷 / PoB2 闭包构造器」三类真逻辑。
//!
//! 注册聚合点 [`register_special_handlers`]：append-only，每个 handler 模块暴露
//! 自己的 `register_*` 函数，本文件逐行 append 调用（共享文件冲突最小化）。
//!
//! [`HandlerRegistry`]: crate::rules::HandlerRegistry

use crate::rules::{DuplicateHandlerError, HandlerRegistry};

mod explode;
mod granted_passive;
mod mageblood;

/// 注册全部 special handler（启动期一次，零 I/O）。
///
/// handler_id 命名 `special:<name>`（`<name>` 沿用 vendor ModParser.lua 构造器名
/// 的 snake_case）。注册冲突（重复 id）→ `Err`（fail-fast）。
pub fn register_special_handlers(
    registry: &mut HandlerRegistry,
) -> Result<(), DuplicateHandlerError> {
    explode::register(registry)?;
    granted_passive::register(registry)?;
    mageblood::register(registry)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_without_conflict() {
        let mut registry = HandlerRegistry::new();
        register_special_handlers(&mut registry).expect("special handler 注册不冲突");
    }
}
