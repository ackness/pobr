//! handler 注册表：数据条目里 `handler_id` → Rust 真逻辑的裁决通道。
//!
//! `overlay/special_mods.json` / `overlay/config_options.json` 等数据表中，
//! 无法用受限模板 DSL 表达的条目（约 10%，见架构文档 20 §5 的 DSL 硬边界）
//! 只携带一个稳定字符串 `handler_id`；运行时经本注册表查到对应 Rust 闭包执行，
//! 产出 Modifier 列表。注册表本身零 I/O、注册集合在启动期固定。
//!
//! 监控约束（架构文档 20 §5）：handler 条目数应 <100；逼近 special 总量 10%
//! 即判数据切分失败、回看裁决 P4。

use std::collections::BTreeMap;
use std::fmt;

use crate::modifier::Modifier;

/// handler 闭包：输入数据条目捕获的数值占位参数（`$1..$n` 已求值），
/// 输出该条目展开的 Modifier 列表。
///
/// M0 骨架签名——后续接入 special_mods/config 时按需扩上下文参数
/// （如 `&CalcConfig` / 条目元数据），届时同步更新全部已注册 handler。
pub type Handler = Box<dyn Fn(&[f64]) -> Vec<Modifier> + Send + Sync>;

/// 重复注册同一 `handler_id` 的错误（注册集合必须唯一且启动期确定）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateHandlerError {
    /// 冲突的 handler 稳定 ID。
    pub id: &'static str,
}

impl fmt::Display for DuplicateHandlerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "handler_id 重复注册：`{}`", self.id)
    }
}

impl std::error::Error for DuplicateHandlerError {}

/// `&'static str` handler_id → handler 闭包的注册表。
///
/// 用 `BTreeMap` 保证遍历顺序确定（便于覆盖清单报表与可重复测试）。
#[derive(Default)]
pub struct HandlerRegistry {
    handlers: BTreeMap<&'static str, Handler>,
}

impl fmt::Debug for HandlerRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HandlerRegistry")
            .field("ids", &self.handlers.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl HandlerRegistry {
    /// 空注册表。
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册一个 handler；同 id 重复注册返回错误（不静默覆盖）。
    pub fn register(
        &mut self,
        id: &'static str,
        handler: Handler,
    ) -> Result<(), DuplicateHandlerError> {
        if self.handlers.contains_key(id) {
            return Err(DuplicateHandlerError { id });
        }
        self.handlers.insert(id, handler);
        Ok(())
    }

    /// 按 id 查 handler；未注册返回 `None`（调用方据此把条目记入未覆盖清单）。
    pub fn get(&self, id: &str) -> Option<&Handler> {
        self.handlers.get(id)
    }

    /// 已注册 handler 数量（覆盖率监控用，约束 <100）。
    pub fn len(&self) -> usize {
        self.handlers.len()
    }

    /// 是否为空注册表。
    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }

    /// 已注册的 handler_id（确定性升序）——覆盖清单报表用。
    pub fn ids(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.handlers.keys().copied()
    }
}

#[cfg(test)]
mod tests {
    use pobr_data::modifier::ModType;

    use super::*;

    fn noop_handler() -> Handler {
        Box::new(|_| Vec::new())
    }

    /// 注册后可按 id 取回并调用，产出 Modifier。
    #[test]
    fn register_then_get_and_invoke() {
        let mut registry = HandlerRegistry::new();
        registry
            .register(
                "test:scaled_life",
                Box::new(|nums| {
                    vec![Modifier::number(
                        "Life",
                        ModType::Base,
                        nums.first().copied().unwrap_or(0.0),
                    )]
                }),
            )
            .unwrap();

        let handler = registry.get("test:scaled_life").expect("已注册");
        let mods = handler(&[50.0]);
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].value.as_number(), Some(50.0));
    }

    /// 同 id 重复注册报错，不静默覆盖既有 handler。
    #[test]
    fn duplicate_registration_errors() {
        let mut registry = HandlerRegistry::new();
        registry.register("dup", noop_handler()).unwrap();
        let err = registry.register("dup", noop_handler()).unwrap_err();
        assert_eq!(err, DuplicateHandlerError { id: "dup" });
        assert_eq!(registry.len(), 1);
    }

    /// 未注册 id 返回 None；len/is_empty/ids 反映注册集合（升序确定）。
    #[test]
    fn lookup_miss_and_deterministic_ids() {
        let mut registry = HandlerRegistry::new();
        assert!(registry.is_empty());
        assert!(registry.get("missing").is_none());

        registry.register("b", noop_handler()).unwrap();
        registry.register("a", noop_handler()).unwrap();
        assert_eq!(registry.len(), 2);
        assert!(!registry.is_empty());
        assert_eq!(registry.ids().collect::<Vec<_>>(), vec!["a", "b"]);
    }
}
