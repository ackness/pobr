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
//! handler_id 命名约定：`config:<name>`（config 域，预算 ≤54；`<name>` 沿用
//! overlay 产物里的 vendor var 原拼写）、`buff:<name>`（buff 域，预算 ≤8）；
//! 总数 <100（架构文档 20 §5 DSL 硬边界监控，逼近上限即判数据切分失败、
//! 回看裁决 P4/P6）。

use pobr_core::CampaignProgress;
use pobr_core::rules::config_interpreter::{ConfigInputValue, ConfigOutcome};
use pobr_core::rules::{HandlerOutcome, HandlerRegistry};
use pobr_data::monster::EnemyTier;

/// config 域 handler 预算上限（蓝图 §1 D2：542 条目 × 10% ≈ 54）。
pub const CONFIG_HANDLER_BUDGET: usize = 54;
/// buff 域 handler 预算上限（蓝图 §5.2 B2：buff handler 预算 ≤8）。
pub const BUFF_HANDLER_BUDGET: usize = 8;
/// 全部 handler 总数硬上限（架构文档 20 §5：<100）。
pub const TOTAL_HANDLER_CAP: usize = 100;

/// 已注册但当前为占位 stub 的 handler（消费方应把命中条目以告警口径上报，
/// 不静默视为已覆盖）。
pub const STUB_HANDLER_IDS: &[&str] = &["config:presetBossSkills"];

/// 构造全量 handler 注册表（T1 起逐批 append；T2 buff handlers 待接入）。
///
/// 后续 append 顺序约定（占位注释即插入点，每行一个 register 调用）：
/// - T1：config handlers（第一批已入，见 [`register_config_handlers`]）。
/// - T2：`pobr_core::rules::buff_expander::register_handlers(&mut registry)`
///   （`buff:fortify` / `buff:onslaught_flask`，蓝图 §5.2 B2）。
pub fn build_registry() -> HandlerRegistry {
    let mut registry = HandlerRegistry::new();
    // ── T1 append 点：config handlers ──
    register_config_handlers(&mut registry);
    // ── T2 append 点：buff handlers ──
    pobr_core::rules::buff_expander::register_handlers(&mut registry)
        .expect("启动期 buff handler 注册不冲突");
    registry
}

/// 第一批 config handlers（M3-T1 A5，蓝图 §4.4 末段）。
///
/// 约定：handler 的真实消费若走**标量通道**（list/数值选项由 build 层从
/// [`ConfigOutcome::scalars`] 读出、接既有逻辑），则注册零 Modifier 产出的
/// handler——目的只是把条目从 `unhandled` 报表移除并锁定覆盖责任归属：
/// - `config:enemyIsBoss`：既有 EnemyTier 接线的包装（标量消费走
///   [`enemy_tier_from_config`]；敌档加成本体在 `enemy_presets` 域 +
///   orchestrator，handler 自身不产 Modifier）。
/// - `config:presetBossSkills`：M3 stub 告警（boss 技能预设表 `boss_skills.json`
///   属 M5+），见 [`STUB_HANDLER_IDS`]。
///
/// `resistancePenalty` 在 overlay 数据中是纯 list 条目（不带 handler_id），
/// 其「包装既有逻辑」落在 [`campaign_progress_from_config`]（CampaignProgress
/// 既有七档表），不占 handler 预算。
fn register_config_handlers(registry: &mut HandlerRegistry) {
    registry
        .register(
            "config:enemyIsBoss",
            Box::new(|_| HandlerOutcome::default()),
        )
        .expect("启动期注册不重复");
    registry
        .register(
            "config:presetBossSkills",
            Box::new(|_| HandlerOutcome::default()),
        )
        .expect("启动期注册不重复");
}

/// 包装既有 EnemyTier 接线（蓝图「config:enemy_is_boss 包装既有逻辑」）：从
/// 解释产物标量取 `enemyIsBoss`（list 型，选项值 `None/Boss/Pinnacle/Uber`）。
///
/// 返回 `None` = 条目未激活或表外字符串，消费方回退编排选项档位
/// （与旧 parse_config 路径口径一致；catalog defaultIndex=3 缺省解析为
/// Pinnacle，恰为 PoB2/编排默认档）。
pub fn enemy_tier_from_config(outcome: &ConfigOutcome) -> Option<EnemyTier> {
    match outcome.scalars.get("enemyIsBoss")? {
        ConfigInputValue::Text(text) => EnemyTier::from_pob_str(text),
        _ => None,
    }
}

/// 包装既有 CampaignProgress 接线（蓝图「config:resistance_penalty 包装既有
/// 逻辑」）：从解释产物标量取 `resistancePenalty`（list 型，数值选项
/// `0/-10/…/-60` 七档），查 CampaignProgress 既有档位表。
///
/// 返回 `None` = 条目未激活或值不在档位表内，消费方回退默认 Endgame（-60）；
/// catalog defaultIndex=7 缺省即解析为 `-60`，与回退值一致。
pub fn campaign_progress_from_config(outcome: &ConfigOutcome) -> Option<CampaignProgress> {
    match outcome.scalars.get("resistancePenalty")? {
        ConfigInputValue::Text(text) => text
            .parse::<f64>()
            .ok()
            .and_then(CampaignProgress::from_resistance_penalty),
        ConfigInputValue::Number(number) => CampaignProgress::from_resistance_penalty(*number),
        ConfigInputValue::Bool(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

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
        println!(
            "[A6] handler 计数：config = {config_count}/{CONFIG_HANDLER_BUDGET}，\
             buff = {buff_count}/{BUFF_HANDLER_BUDGET}，总数 = {}/{TOTAL_HANDLER_CAP}（stub = {}）",
            registry.len(),
            STUB_HANDLER_IDS.len()
        );

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

    /// 第一批 config handlers 已注册（含 stub），id 沿用 overlay 数据原拼写。
    #[test]
    fn first_batch_config_handlers_registered() {
        let registry = build_registry();
        assert!(registry.get("config:enemyIsBoss").is_some());
        assert!(registry.get("config:presetBossSkills").is_some());
        for stub in STUB_HANDLER_IDS {
            assert!(registry.get(stub).is_some(), "stub `{stub}` 应已注册");
        }
        // 第一批 handler 均为零产出（真实消费走标量通道/后续阶段）。
        let handler = registry.get("config:enemyIsBoss").unwrap();
        let out = handler(&pobr_core::rules::HandlerCtx::with_inputs(&[0.0]));
        assert!(out.player_mods.is_empty());
        assert!(out.enemy_mods.is_empty());
        assert!(out.conditions.is_empty());
        assert!(out.scalars.is_empty());
    }

    fn outcome_with_scalar(var: &str, value: ConfigInputValue) -> ConfigOutcome {
        let mut scalars = BTreeMap::new();
        scalars.insert(var.to_string(), value);
        ConfigOutcome {
            scalars,
            ..ConfigOutcome::default()
        }
    }

    /// enemyIsBoss 标量包装：四档映射、表外/缺失回 None（与旧路径口径一致）。
    #[test]
    fn enemy_tier_wrapper_maps_scalar() {
        let outcome = outcome_with_scalar("enemyIsBoss", ConfigInputValue::Text("Uber".into()));
        assert_eq!(enemy_tier_from_config(&outcome), Some(EnemyTier::Uber));

        let outcome = outcome_with_scalar("enemyIsBoss", ConfigInputValue::Text("奇怪档".into()));
        assert_eq!(enemy_tier_from_config(&outcome), None);

        assert_eq!(enemy_tier_from_config(&ConfigOutcome::default()), None);
    }

    /// resistancePenalty 标量包装：list 文本值与数值两形态均映射既有七档表。
    #[test]
    fn campaign_progress_wrapper_maps_scalar() {
        let outcome =
            outcome_with_scalar("resistancePenalty", ConfigInputValue::Text("-30".into()));
        assert_eq!(
            campaign_progress_from_config(&outcome),
            CampaignProgress::from_resistance_penalty(-30.0)
        );
        assert!(campaign_progress_from_config(&outcome).is_some());

        let outcome = outcome_with_scalar("resistancePenalty", ConfigInputValue::Number(-15.0));
        assert_eq!(
            campaign_progress_from_config(&outcome),
            None,
            "表外值回 None"
        );

        assert_eq!(
            campaign_progress_from_config(&ConfigOutcome::default()),
            None
        );
    }
}
