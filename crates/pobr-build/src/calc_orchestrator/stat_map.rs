//! stat_map — StatMap/curse/debuff/exposure/player_buff 映射 + STAT_MAP_CTX 双跑收集器。
//!
//! **双跑上下文**：[`mapped_stat_modifiers`] 是自由函数、三个取数点
//! （skill_base / quality / support）不持有编排选项——按 §3.2 共享规则（只改
//! `mapped_stat_modifiers` + `OrchestratorOptions` 字段、主流程接线 ≤3 行），
//! 模式与 catalog 经线程局部上下文 [`STAT_MAP_CTX`] 传递：`calculate_with_data`
//! 开头安装、guard 离开作用域复位。单次计算单线程、安装/复位确定性，不构成
//! 共享可变状态。

use super::*;

use pobr_core::Modifier;
use pobr_core::rules::stat_map_engine::{self, MappedItem, MappedOutcome, StatMapCatalog};
use pobr_data::source::{ModifierSource, SourceId, SourceKind};

use crate::build::{Build, SocketGroup};
use crate::build_data::BuildData;

use std::cell::RefCell;

thread_local! {
    pub(crate) static STAT_MAP_CTX: RefCell<StatMapCtx> = RefCell::new(StatMapCtx::default());
}

#[derive(Default)]
pub(crate) struct StatMapCtx {
    mode: StatMapMode,
    pub(crate) catalog: Option<std::sync::Arc<StatMapCatalog>>,
    /// Compare 模式的映射级 outcome 观测记录（跨 guard 存活，由
    /// [`take_stat_map_compare_records`] 取出）。
    compare_records: Vec<StatMapCompareRecord>,
}

/// Compare 模式产出的单条映射级 outcome 观测记录（按 stat 一条）。
#[derive(Debug, Clone)]
pub struct StatMapCompareRecord {
    /// stat 稳定 id。
    pub stat: String,
    /// 取数点标签（skill / gem.<id>.qN / support id）。
    pub label: String,
    /// 分类：`mapped` / `unsupported` / `unknown`（数据通道 outcome 观测；
    /// Legacy 删除前为双跑五分类 diff）。
    pub classification: &'static str,
    /// 细节（注入项列表 / Unsupported 分类）。
    pub detail: String,
}

/// 安装本次计算的 statmap 上下文，返回离开作用域自动复位的 guard。
pub(crate) fn install_stat_map_context(
    mode: StatMapMode,
    catalog: Option<std::sync::Arc<StatMapCatalog>>,
) -> StatMapCtxGuard {
    STAT_MAP_CTX.with(|ctx| {
        let mut ctx = ctx.borrow_mut();
        ctx.mode = mode;
        ctx.catalog = catalog;
    });
    StatMapCtxGuard
}

pub(crate) struct StatMapCtxGuard;

impl Drop for StatMapCtxGuard {
    fn drop(&mut self) {
        STAT_MAP_CTX.with(|ctx| {
            let mut ctx = ctx.borrow_mut();
            ctx.mode = StatMapMode::default();
            ctx.catalog = None;
            // compare_records 保留——调用方在 calculate 返回后 take。
        });
    }
}

/// 取出（并清空）当前线程累计的 Compare 模式 outcome 观测记录。
pub fn take_stat_map_compare_records() -> Vec<StatMapCompareRecord> {
    STAT_MAP_CTX.with(|ctx| std::mem::take(&mut ctx.borrow_mut().compare_records))
}

/// 把一组已解析 stat 映射为带 `source_kind` 归因的 modifier——statmap 通道分发
/// 点：Data 走 [`stat_map_engine::map_stat`] 数据引擎；Compare = Data 计算 + 逐
/// stat 记录映射 outcome 观测（**输出与 Data 一致**，纯观测不改结果，记录经
/// [`take_stat_map_compare_records`] 取出）。无法映射的 stat（Unsupported /
/// Unknown）静默跳过；零值跳过。
///
/// `effect_id`：stat 所属 granted effect，per-statSet 覆盖定位。
/// `set_key`：**选中** statSet 的 vendor 1-based 导出序号十进制
/// 字符串（[`BuildData::selected_set_key`]）；`None` = 引擎自动取默认 set "1"
/// 覆盖（PoB2 缺省 statSetIndex=1，vendor `SkillsTab.lua:354`；18 个 ninja build
/// 的 statSetIndex 全为 nil = 与 None 等价）。未选 set 的 global-only merge 走
/// [`unselected_set_global_modifiers`]，不经本分发点。
pub(crate) fn mapped_stat_modifiers(
    stats: &[pobr_data::catalog::SkillDamageStat],
    source_kind: SourceKind,
    label_prefix: &str,
    effect_id: &str,
    set_key: Option<&str>,
) -> Vec<Modifier> {
    let (mode, catalog) =
        STAT_MAP_CTX.with(|ctx| (ctx.borrow().mode, ctx.borrow().catalog.clone()));
    match mode {
        StatMapMode::Data => data_mapped_stat_modifiers(
            stats,
            source_kind,
            label_prefix,
            effect_id,
            set_key,
            catalog.as_deref(),
        ),
        StatMapMode::Compare => {
            record_stat_map_observation(
                stats,
                label_prefix,
                effect_id,
                set_key,
                catalog.as_deref(),
            );
            data_mapped_stat_modifiers(
                stats,
                source_kind,
                label_prefix,
                effect_id,
                set_key,
                catalog.as_deref(),
            )
        }
    }
}

/// Data 通道：statmap 数据引擎。effect 上下文 + 选中 set 覆盖键见
/// [`mapped_stat_modifiers`] 文档；`SkillData` 项暂无消费方，忽略（不参与
/// 计算，不会错算）；Unsupported / Unknown 静默跳过（分类观测走 Compare 模式）。
pub(crate) fn data_mapped_stat_modifiers(
    stats: &[pobr_data::catalog::SkillDamageStat],
    source_kind: SourceKind,
    label_prefix: &str,
    effect_id: &str,
    set_key: Option<&str>,
    catalog: Option<&StatMapCatalog>,
) -> Vec<Modifier> {
    let Some(catalog) = catalog else {
        return Vec::new(); // 未注入 catalog：数据通道全 miss（蓝图 Data 模式必带 catalog）。
    };
    let mut mods = Vec::new();
    for ds in stats {
        if ds.value == 0.0 {
            continue; // 跳零值 stat（无信息量，与历史口径一致）。
        }
        let MappedOutcome::Mapped(items) =
            stat_map_engine::map_stat(catalog, effect_id, set_key, &ds.stat, ds.value)
        else {
            continue;
        };
        for item in items {
            let MappedItem::Modifier(modifier) = item else {
                continue; // SkillData：第一批无消费方。
            };
            let origin = ModifierSource::new(SourceId::new(
                source_kind.clone(),
                format!("{label_prefix}.{}", ds.stat),
            ))
            .with_raw_text(format!("{label_prefix} {} ({})", ds.stat, ds.value));
            mods.push(modifier.with_origin(origin));
        }
    }
    mods
}

/// statmap catalog 取数（线程局部上下文优先——`calculate_with_data` 安装的编排
/// 选项注入；上下文外（直接调用 [`buff_skill_specs`] 等的测试/工具路径）回退
/// `data.stat_map_catalog`——编排主流程两处指向同一 Arc）。
pub(crate) fn resolve_stat_map_catalog(data: &BuildData) -> Option<std::sync::Arc<StatMapCatalog>> {
    STAT_MAP_CTX
        .with(|ctx| ctx.borrow().catalog.clone())
        .or_else(|| data.stat_map_catalog.clone())
}

/// curse 效果词条取数点：把一个 curse 技能 statset 的全部 stat 经
/// [`stat_map_engine::map_curse_stat`]（curse 域数据通道）映射为**敌侧**
/// modifier 列表（BuffSpec.mods 载荷，buff_pass curse 路径消费）。
///
/// - catalog 取数：优先线程局部上下文（`calculate_with_data` 安装的编排选项
///   注入），上下文外（直接调用 [`buff_skill_specs`] 的测试/工具路径）回退
///   `data.stat_map_catalog`——两处在编排主流程指向同一 Arc。
/// - 归因：`(SkillGem, "curse.<skill_id>.<stat>")`（aura 路径同构口径），
///   buff_pass 缩放保留 origin（trace 不丢弃）。
/// - 可见性（不静默）：Compare 模式逐 stat 把 curse 载荷的
///   `mapped` / `unsupported:<类别>` 落 [`StatMapCompareRecord`]（label =
///   `curse.<skill_id>`）；`Mapped(空)`（非 curse 载荷，走主技能通道）与
///   `Unknown`（catalog 无条目）不记——非 curse 语义，记录即噪声。
///   Data 模式与 statmap 主通道同口径静默跳过（分类观测走 Compare）。
pub(crate) fn curse_stat_modifiers(
    data: &BuildData,
    stats: &crate::build_data::EffectStats,
    skill_id: &str,
    set_key: Option<&str>,
) -> Vec<Modifier> {
    let mode = STAT_MAP_CTX.with(|ctx| ctx.borrow().mode);
    let Some(catalog) = resolve_stat_map_catalog(data) else {
        return Vec::new(); // 无 catalog（旧数据包）：curse 词条全 miss（与主通道同口径）。
    };
    let mut mods = Vec::new();
    for ds in stats.all() {
        if ds.value == 0.0 {
            continue; // 跳零值 stat（与主通道同口径）。
        }
        let outcome =
            stat_map_engine::map_curse_stat(&catalog, skill_id, set_key, &ds.stat, ds.value);
        // Compare 模式可见性记录（curse 载荷专属行）。
        if mode == StatMapMode::Compare {
            let record = match &outcome {
                MappedOutcome::Mapped(items) if !items.is_empty() => {
                    let injected: Vec<(String, &'static str, f64)> = items
                        .iter()
                        .filter_map(|item| match item {
                            MappedItem::Modifier(m) => Some((
                                m.name.to_string(),
                                m.mod_type.as_trace_label(),
                                m.value.as_number().unwrap_or(0.0),
                            )),
                            MappedItem::SkillData { .. } => None,
                        })
                        .collect();
                    Some(("mapped", format!("curse={injected:?}")))
                }
                MappedOutcome::Unsupported(reason) => {
                    Some(("unsupported", format!("unsupported:{}", reason.category())))
                }
                _ => None, // Mapped(空)/Unknown = 非 curse 载荷，不记。
            };
            if let Some((classification, detail)) = record {
                STAT_MAP_CTX.with(|ctx| {
                    ctx.borrow_mut().compare_records.push(StatMapCompareRecord {
                        stat: ds.stat.clone(),
                        label: format!("curse.{skill_id}"),
                        classification,
                        detail,
                    });
                });
            }
        }
        let MappedOutcome::Mapped(items) = outcome else {
            continue;
        };
        for item in items {
            let MappedItem::Modifier(modifier) = item else {
                continue;
            };
            let origin = ModifierSource::new(SourceId::new(
                SourceKind::SkillGem,
                format!("curse.{skill_id}.{}", ds.stat),
            ))
            .with_raw_text(format!("curse {skill_id} {} ({})", ds.stat, ds.value));
            mods.push(modifier.with_origin(origin));
        }
    }
    mods
}

/// debuff 效果词条取数点：把一个 debuff 技能 statset 的全部 stat 经
/// [`stat_map_engine::map_debuff_stat`]（debuff 域数据通道，敌侧允收名单 =
/// 元素曝光族第一批）映射为**敌侧** modifier 列表（BuffSpec.mods 载荷，
/// buff_pass Debuff 路径消费）。与 [`curse_stat_modifiers`] 同构：
/// - catalog 取数：线程局部上下文优先，回退 `data.stat_map_catalog`；
/// - 归因：`(SkillGem, "debuff.<skill_id>.<stat>")`，buff_pass 缩放保留 origin；
/// - 可见性：Compare 模式逐 stat 落 [`StatMapCompareRecord`]
///   （label = `debuff.<skill_id>`）；`Mapped(空)` / `Unknown` 不记（非 debuff 载荷）。
pub(crate) fn debuff_stat_modifiers(
    data: &BuildData,
    stats: &crate::build_data::EffectStats,
    skill_id: &str,
    set_key: Option<&str>,
) -> Vec<Modifier> {
    let (mode, ctx_catalog) =
        STAT_MAP_CTX.with(|ctx| (ctx.borrow().mode, ctx.borrow().catalog.clone()));
    let catalog = ctx_catalog.or_else(|| data.stat_map_catalog.clone());
    let Some(catalog) = catalog else {
        return Vec::new(); // 无 catalog（旧数据包）：debuff 词条全 miss（与主通道同口径）。
    };
    let mut mods = Vec::new();
    for ds in stats.all() {
        if ds.value == 0.0 {
            continue; // 跳零值 stat（与主通道同口径）。
        }
        let outcome =
            stat_map_engine::map_debuff_stat(&catalog, skill_id, set_key, &ds.stat, ds.value);
        if mode == StatMapMode::Compare {
            let record = match &outcome {
                MappedOutcome::Mapped(items) if !items.is_empty() => {
                    let injected: Vec<(String, &'static str, f64)> = items
                        .iter()
                        .filter_map(|item| match item {
                            MappedItem::Modifier(m) => Some((
                                m.name.to_string(),
                                m.mod_type.as_trace_label(),
                                m.value.as_number().unwrap_or(0.0),
                            )),
                            MappedItem::SkillData { .. } => None,
                        })
                        .collect();
                    Some(("mapped", format!("debuff={injected:?}")))
                }
                MappedOutcome::Unsupported(reason) => {
                    Some(("unsupported", format!("unsupported:{}", reason.category())))
                }
                _ => None, // Mapped(空)/Unknown = 非 debuff 载荷，不记。
            };
            if let Some((classification, detail)) = record {
                STAT_MAP_CTX.with(|ctx| {
                    ctx.borrow_mut().compare_records.push(StatMapCompareRecord {
                        stat: ds.stat.clone(),
                        label: format!("debuff.{skill_id}"),
                        classification,
                        detail,
                    });
                });
            }
        }
        let MappedOutcome::Mapped(items) = outcome else {
            continue;
        };
        for item in items {
            let MappedItem::Modifier(modifier) = item else {
                continue;
            };
            let origin = ModifierSource::new(SourceId::new(
                SourceKind::SkillGem,
                format!("debuff.{skill_id}.{}", ds.stat),
            ))
            .with_raw_text(format!("debuff {skill_id} {} ({})", ds.stat, ds.value));
            mods.push(modifier.with_origin(origin));
        }
    }
    mods
}

/// 组内是否存在 debuff 曝光载荷（[`exposure_support_modifiers`] 的
/// 宿主探测）：与 [`debuff_stat_modifiers`] 同一取数链但**纯只读**（不落
/// Compare 记录——同一 stat 已由 buff_skill_specs 的 Debuff 分支记录，
/// 探测重复记录即噪声）。
pub(crate) fn has_debuff_payload(
    data: &BuildData,
    stats: &crate::build_data::EffectStats,
    skill_id: &str,
    set_key: Option<&str>,
) -> bool {
    let ctx_catalog = STAT_MAP_CTX.with(|ctx| ctx.borrow().catalog.clone());
    let Some(catalog) = ctx_catalog.or_else(|| data.stat_map_catalog.clone()) else {
        return false;
    };
    stats.all().any(|ds| {
        ds.value != 0.0
            && matches!(
                stat_map_engine::map_debuff_stat(&catalog, skill_id, set_key, &ds.stat, ds.value),
                MappedOutcome::Mapped(items) if !items.is_empty()
            )
    })
}

/// 效果 statset 是否含**曝光施加能力**载荷（`InflictExposure` flag /
/// `<El>ExposureChance` BASE，[`stat_map_engine::has_exposure_inflict_payload`]
/// 存在性判定）——[`exposure_support_modifiers`] 宿主探测的第二判据：宿主曝光
/// 能力来自 support（Fire Exposure `inflict_exposure_for_x_ms_on_ignite` →
/// `flag("InflictExposure", on-Ignited)`，vendor SkillStatMap.lua:1701-1703）
/// 而非自身 debuff 载荷时同样成立（vendor CalcPerform.lua:3196-3200 Config
/// 曝光源判据 `HasMod("FLAG", "InflictExposure")` 查的是合并 support 后的
/// skillModList）。零值 stat 不计（与 [`has_debuff_payload`] 同口径）。
pub(crate) fn has_exposure_inflict_stats(
    data: &BuildData,
    stats: &crate::build_data::EffectStats,
    skill_id: &str,
    set_key: Option<&str>,
) -> bool {
    let ctx_catalog = STAT_MAP_CTX.with(|ctx| ctx.borrow().catalog.clone());
    let Some(catalog) = ctx_catalog.or_else(|| data.stat_map_catalog.clone()) else {
        return false;
    };
    stats.all().any(|ds| {
        ds.value != 0.0
            && stat_map_engine::has_exposure_inflict_payload(&catalog, skill_id, set_key, &ds.stat)
    })
}

/// 非主组的曝光效果 support 注入面（h3 登记 Potent Exposure 同根）。
///
/// vendor：support mod 并入宿主技能 skillModList（CalcActiveSkill.lua:210-214
/// effectList），曝光应用时按**来源技能**取 `<El>ExposureEffect` INC
/// （CalcPerform.lua:3193-3211 getSkillExposureEffect，:3226-3231 对每个曝光源
/// 独立缩放）——非主组的 Potent Exposure（`exposure_effect_+%`，
/// SkillStatMap.lua:1731-1735）同样作用于其宿主（如 chronomancer 的 Frost Bomb
/// 副组）。PoBR 曝光归约（`reduce_enemy_exposure`）读 player db 扁平求和（已
/// 登记近似），等价注入面 = 把**曝光源所在组**的兼容 support 的
/// `<El>ExposureEffect` 词条全局注入 player db：
/// - 仅扫曝光宿主组——两判据任一成立（无曝光源的组其曝光效果词条不全局
///   生效，保持 vendor 作用域语义的最小外延）：
///   1. 主动技能自身产出 debuff 曝光载荷（[`has_debuff_payload`]，Frost Bomb
///      `active_skill_all_elemental_exposure_magnitude` 形）；
///   2. 主动技能或其兼容 support 含曝光施加能力载荷
///      （[`has_exposure_inflict_stats`]：`InflictExposure` flag /
///      `<El>ExposureChance`，Fire Exposure support
///      `inflict_exposure_for_x_ms_on_ignite` 形——vendor Config 曝光源判据
///      CalcPerform.lua:3196-3200 查合并 support 后的 skillModList）。
/// - **主组跳过**（其 support 已由 [`support_modifiers`] 全量注入、含本名族，
///   避免双注入）；
/// - 只保留 `<El>ExposureEffect` 名（其余 support 词条仍是技能局部语义，
///   不得从非主组泄漏到全局）。
///
/// 已登记近似（多源场景，语料均单源）：vendor 对每个曝光源以
/// `global + 该源技能 skill INC` 独立缩放后取 max（:3226-3231）；PoBR 全局
/// 扁平求和——若多个曝光宿主组各带曝光效果 support，PoBR 求和会高估
/// （vendor 各源取各自的）。EE（Elemental Equilibrium）对已命中元素跳过
/// 曝光（:3216-3219）与 `Condition:Has<El>Exposure` 落 flag（:3242-3244）
/// 未实现（语料无 EE + 曝光组合、无该条件消费方）。
pub(crate) fn exposure_support_modifiers(
    build: &Build,
    data: &BuildData,
    main_group: Option<&SocketGroup>,
) -> Vec<Modifier> {
    use std::collections::BTreeSet;
    let mut mods = Vec::new();
    for group in build.enabled_socket_groups() {
        if main_group.is_some_and(|mg| std::ptr::eq(mg, group)) {
            continue;
        }
        // 曝光源宿主：组内主动技能自身产 debuff 曝光载荷，或自身/兼容
        // support 含曝光施加能力载荷 → 其兼容 support 名单。
        let mut support_entries: BTreeSet<(usize, String)> = BTreeSet::new();
        for gem in &group.gem_skills {
            let Some(effect) = data.granted_effects.get(&gem.skill_id) else {
                continue;
            };
            if effect.is_support {
                continue;
            }
            let es = data.effect_stats(
                &gem.skill_id,
                gem.gem_level,
                gem.quality,
                gem.stat_set_index,
            );
            let set_key = data.selected_set_key(&gem.skill_id, gem.stat_set_index);
            let judgement = judge_group_supports(group, data, &gem.skill_id);
            let is_host = has_debuff_payload(data, &es, &gem.skill_id, set_key.as_deref())
                || has_exposure_inflict_stats(data, &es, &gem.skill_id, set_key.as_deref())
                || judgement.compatible.iter().any(|sup| {
                    let host = &group.gem_skills[sup.gem_index];
                    // quality 传 0 与 support_modifiers 同口径。
                    let set_index = sup.stat_set_index(group);
                    let sup_stats = data.effect_stats(&sup.effect_id, host.gem_level, 0, set_index);
                    let sup_key = data.selected_set_key(&sup.effect_id, set_index);
                    has_exposure_inflict_stats(data, &sup_stats, &sup.effect_id, sup_key.as_deref())
                });
            if !is_host {
                continue;
            }
            for sup in judgement.compatible {
                support_entries.insert((sup.gem_index, sup.effect_id));
            }
        }
        for (idx, effect_id) in support_entries {
            let gem = &group.gem_skills[idx];
            let set_index = (gem.skill_id == effect_id)
                .then_some(gem.stat_set_index)
                .flatten();
            // quality 传 0 与 support_modifiers 同口径（support 品质表条目不存在）。
            let stats = data.effect_stats(&effect_id, gem.gem_level, 0, set_index);
            let set_key = data.selected_set_key(&effect_id, set_index);
            mods.extend(
                mapped_stat_modifiers(
                    &stats.base,
                    SourceKind::SupportGem,
                    &effect_id,
                    &effect_id,
                    set_key.as_deref(),
                )
                .into_iter()
                .filter(|m| m.name.as_str().ends_with("ExposureEffect")),
            );
        }
    }
    mods
}

/// 玩家侧 buff 词条取数点：把一个 buff 授予效果（support / aura 技能）
/// statset 的全部 stat 经 [`stat_map_engine::map_player_buff_stat`]（buff 域数据
/// 通道，玩家侧允收名单）映射为**玩家侧** modifier 列表（BuffSpec.mods 载荷，
/// buff_pass Buff/Aura 路径消费）。与 [`curse_stat_modifiers`] 同构：
/// - catalog 取数：线程局部上下文优先，回退 `data.stat_map_catalog`；
/// - 归因：`(SkillGem, "buff.<skill_id>.<stat>")`，buff_pass 缩放保留 origin；
/// - 可见性：Compare 模式逐 stat 落 [`StatMapCompareRecord`]
///   （label = `buff.<skill_id>`）；`Mapped(空)` / `Unknown` 不记（非 buff 载荷）。
pub(crate) fn player_buff_stat_modifiers(
    data: &BuildData,
    stats: &crate::build_data::EffectStats,
    skill_id: &str,
    set_key: Option<&str>,
) -> Vec<Modifier> {
    let (mode, ctx_catalog) =
        STAT_MAP_CTX.with(|ctx| (ctx.borrow().mode, ctx.borrow().catalog.clone()));
    let catalog = ctx_catalog.or_else(|| data.stat_map_catalog.clone());
    let Some(catalog) = catalog else {
        return Vec::new(); // 无 catalog（旧数据包）：buff 词条全 miss（与主通道同口径）。
    };
    // 同名 stat 先加法合并（vendor CalcTools.lua:138-200 buildSkillInstanceStats
    // `stats[stat] += value`：品质段与等级段同名时合一后才建 mod）。不合并会
    // 产出两条同 (name/type/flags/tags) 的 mod，被 buff_pass merge_buff 的
    // 「同名取强」（vendor mergeBuff CalcPerform.lua:41-63）丢弃较小一条——
    // Elemental Conflux q20 的品质段 +10 即此前被静默吞掉。
    let mut merged: Vec<(String, f64)> = Vec::new();
    for ds in stats.all() {
        match merged.iter_mut().find(|(stat, _)| *stat == ds.stat) {
            Some((_, value)) => *value += ds.value,
            None => merged.push((ds.stat.clone(), ds.value)),
        }
    }
    let mut mods = Vec::new();
    for (stat, value) in &merged {
        if *value == 0.0 {
            continue; // 跳零值 stat（与主通道同口径）。
        }
        let outcome =
            stat_map_engine::map_player_buff_stat(&catalog, skill_id, set_key, stat, *value);
        if mode == StatMapMode::Compare {
            let record = match &outcome {
                MappedOutcome::Mapped(items) if !items.is_empty() => {
                    let injected: Vec<(String, &'static str, f64)> = items
                        .iter()
                        .filter_map(|item| match item {
                            MappedItem::Modifier(m) => Some((
                                m.name.to_string(),
                                m.mod_type.as_trace_label(),
                                m.value.as_number().unwrap_or(0.0),
                            )),
                            MappedItem::SkillData { .. } => None,
                        })
                        .collect();
                    Some(("mapped", format!("buff={injected:?}")))
                }
                MappedOutcome::Unsupported(reason) => {
                    Some(("unsupported", format!("unsupported:{}", reason.category())))
                }
                _ => None, // Mapped(空)/Unknown = 非玩家侧 buff 载荷，不记。
            };
            if let Some((classification, detail)) = record {
                STAT_MAP_CTX.with(|ctx| {
                    ctx.borrow_mut().compare_records.push(StatMapCompareRecord {
                        stat: stat.clone(),
                        label: format!("buff.{skill_id}"),
                        classification,
                        detail,
                    });
                });
            }
        }
        let MappedOutcome::Mapped(items) = outcome else {
            continue;
        };
        for item in items {
            let MappedItem::Modifier(modifier) = item else {
                continue;
            };
            let origin = ModifierSource::new(SourceId::new(
                SourceKind::SkillGem,
                format!("buff.{skill_id}.{stat}"),
            ))
            .with_raw_text(format!("buff {skill_id} {stat} ({value})"));
            mods.push(modifier.with_origin(origin));
        }
    }
    mods
}

/// Compare 模式：逐 stat 记录数据通道映射 outcome 观测（分类
/// `mapped` / `unsupported:<类别>` / `unknown`），进线程局部缓冲。Legacy 启发式
/// 已删除（T2.4），本函数保留为长期对照/观测框架——config / parser 双跑
/// 复用同模式（裁决：保留枚举与报告框架）。
pub(crate) fn record_stat_map_observation(
    stats: &[pobr_data::catalog::SkillDamageStat],
    label_prefix: &str,
    effect_id: &str,
    set_key: Option<&str>,
    catalog: Option<&StatMapCatalog>,
) {
    for ds in stats {
        if ds.value == 0.0 {
            continue;
        }
        // 数据通道 outcome（effect 上下文 + 选中 set 覆盖键，与 Data 通道同口径）。
        let outcome = match catalog {
            Some(c) => stat_map_engine::map_stat(c, effect_id, set_key, &ds.stat, ds.value),
            None => MappedOutcome::Unknown,
        };
        let (classification, detail): (&'static str, String) = match &outcome {
            MappedOutcome::Mapped(items) => {
                let mut injected: Vec<(String, &'static str, f64)> = items
                    .iter()
                    .filter_map(|item| match item {
                        MappedItem::Modifier(m) => Some((
                            m.name.to_string(),
                            m.mod_type.as_trace_label(),
                            m.value.as_number().unwrap_or(0.0),
                        )),
                        MappedItem::SkillData { .. } => None, // 无消费方，不计入
                    })
                    .collect();
                injected.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                ("mapped", format!("data={injected:?}"))
            }
            MappedOutcome::Unsupported(reason) => {
                ("unsupported", format!("unsupported:{}", reason.category()))
            }
            MappedOutcome::Unknown => ("unknown", String::new()),
        };
        STAT_MAP_CTX.with(|ctx| {
            ctx.borrow_mut().compare_records.push(StatMapCompareRecord {
                stat: ds.stat.clone(),
                label: label_prefix.to_string(),
                classification,
                detail,
            });
        });
    }
}
