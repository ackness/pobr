//! handler 注册聚合点（M3 T0 骨架，蓝图 m3-orchestration.md §2.4 契约 3 / §4.6 A6）。
//!
//! 数据表（`overlay/config_options.json` / `overlay/buff_definitions.json` …）里
//! 无法用受限模板 DSL 表达的条目只携带稳定字符串 `handler_id`；运行时由本聚合点
//! 构造的 [`HandlerRegistry`] 裁决执行。注册集合在启动期固定、零 I/O。
//!
//! 聚合约定（蓝图 §2.2）：T1/T2 各自在**自己模块**里暴露
//! `pub fn register_xxx_handlers(&mut HandlerRegistry)`，本文件 `build_registry`
//! 内逐行 append 调用（append-only，把共享文件冲突压到最小）。
//!
//! handler_id 命名约定：`config:<name>`（config 域，预算 ≤54）、`buff:<name>`
//! （buff 域，预算 ≤8）；总数 <100（架构文档 20 §5 DSL 硬边界监控，
//! 逼近上限即判数据切分失败、回看裁决 P4/P6）。

use pobr_core::rules::HandlerRegistry;

/// config 域 handler 预算上限（蓝图 §1 D2：542 条目 × 10% ≈ 54）。
pub const CONFIG_HANDLER_BUDGET: usize = 54;
/// buff 域 handler 预算上限（蓝图 §5.2 B2：buff handler 预算 ≤8）。
pub const BUFF_HANDLER_BUDGET: usize = 8;
/// 全部 handler 总数硬上限（架构文档 20 §5：<100）。
pub const TOTAL_HANDLER_CAP: usize = 100;

/// 构造全量 handler 注册表（M3 T0 骨架：暂无任何注册项，T1/T2 落地后逐行 append）。
///
/// 后续 append 顺序约定（占位注释即插入点，每行一个 register 调用）：
/// - T1：config handlers（`config:enemy_is_boss` / `config:preset_boss_skills` /
///   `config:resistance_penalty` / `config:custom_mods` …，蓝图 §4.4 A4）。
/// - T2：`pobr_core::rules::buff_expander::register_handlers(&mut registry)`
///   （`buff:fortify` / `buff:onslaught_flask`，蓝图 §5.2 B2）。
pub fn build_registry() -> HandlerRegistry {
    // T0 骨架：空注册表。重复注册由 `HandlerRegistry::register` 自身报错兜底。
    // ── T1 append 点：config handlers ──
    // ── T2 append 点：buff handlers ──
    HandlerRegistry::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 按 handler_id 前缀统计某域的注册数量（A6 监控断言用）。
    fn count_with_prefix(registry: &HandlerRegistry, prefix: &str) -> usize {
        registry.ids().filter(|id| id.starts_with(prefix)).count()
    }

    /// A6 监控断言（蓝图 §4.6）：config 域 handler ≤54、buff 域 ≤8、总数 <100。
    /// 任何 track 给注册表 append 后此测试自动复核预算；逼近上限即为架构告警信号。
    #[test]
    fn handler_counts_within_budget() {
        let registry = build_registry();
        let config_count = count_with_prefix(&registry, "config:");
        let buff_count = count_with_prefix(&registry, "buff:");

        assert!(
            config_count <= CONFIG_HANDLER_BUDGET,
            "config 域 handler 数 {config_count} 超预算 {CONFIG_HANDLER_BUDGET}（DSL 切分失败信号，回看裁决 P4/P6）"
        );
        assert!(
            buff_count <= BUFF_HANDLER_BUDGET,
            "buff 域 handler 数 {buff_count} 超预算 {BUFF_HANDLER_BUDGET}"
        );
        assert!(
            registry.len() < TOTAL_HANDLER_CAP,
            "handler 总数 {} 达到硬上限 {TOTAL_HANDLER_CAP}",
            registry.len()
        );
    }

    /// T0 骨架不变式：尚无任何注册项（T1/T2 接入后此断言由其 PR 同步更新）。
    #[test]
    fn registry_is_empty_skeleton_in_t0() {
        assert!(build_registry().is_empty());
    }
}
