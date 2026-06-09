use std::collections::HashMap;

use pobr_data::prelude::*;

use crate::{CalcConfig, ModValue, Modifier, TraceGraph, TraceNodeId, TraceOperation, TracedValue};

/// 单个 modName 的 MORE 连乘积按 PoB2 默认精度 `round(·, 2)` 归一（ModList.lua MoreInternal）。
fn round_more(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModContribution {
    pub name: ModName,
    pub mod_type: ModType,
    pub value: f64,
    pub origin: Option<ModifierSource>,
    pub raw_text: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ModDb {
    mods: HashMap<ModName, Vec<Modifier>>,
}

impl ModDb {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_mod(&mut self, modifier: Modifier) {
        self.mods
            .entry(modifier.name.clone())
            .or_default()
            .push(modifier);
    }

    pub fn add_list(&mut self, modifiers: impl IntoIterator<Item = Modifier>) {
        for modifier in modifiers {
            self.add_mod(modifier);
        }
    }

    /// Returns a new [`ModDb`] containing only the modifiers for which `keep`
    /// returns `true`, preserving insertion order within each [`ModName`] bucket.
    ///
    /// Used by marginal attribution to rebuild a build with a given source
    /// removed, without mutating the original db (read-only snapshot recompute).
    pub fn filtered(&self, mut keep: impl FnMut(&Modifier) -> bool) -> Self {
        let mods = self
            .mods
            .iter()
            .map(|(name, modifiers)| {
                (
                    name.clone(),
                    modifiers
                        .iter()
                        .filter(|modifier| keep(modifier))
                        .cloned()
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        Self { mods }
    }

    pub fn sum(&self, mod_type: ModType, cfg: &CalcConfig, names: &[ModName]) -> f64 {
        names
            .iter()
            .filter_map(|name| self.mods.get(name))
            .flat_map(|mods| mods.iter())
            .filter(|modifier| modifier.mod_type == mod_type && modifier.matches(cfg))
            .filter_map(|modifier| modifier.effective_number(cfg))
            .sum()
    }

    /// 取某组 modifier 中**生效值最大的一份**（曝光 `ExposureMin`/取最强语义）。
    ///
    /// PoB2 `CalcPerform.lua` 对曝光的聚合是逐来源结算后 `magnitude = max(magnitude, value)`
    /// （**取最强单一来源**而非求和）。本方法只考虑 `matches(cfg)` 通过的 modifier，
    /// 空集合返回 `0.0`。
    ///
    /// 出处：agent-docs/debuffs.md §曝光；
    ///       devs/docs/architecture/12-combat-mechanics-architecture.md §4.2（exposure 取最强）。
    pub fn max_of(&self, mod_type: ModType, cfg: &CalcConfig, names: &[ModName]) -> f64 {
        names
            .iter()
            .filter_map(|name| self.mods.get(name))
            .flat_map(|mods| mods.iter())
            .filter(|modifier| modifier.mod_type == mod_type && modifier.matches(cfg))
            .filter_map(|modifier| modifier.effective_number(cfg))
            .fold(0.0_f64, f64::max)
    }

    pub fn contributions(
        &self,
        mod_type: ModType,
        cfg: &CalcConfig,
        names: &[ModName],
    ) -> Vec<ModContribution> {
        names
            .iter()
            .filter_map(|name| self.mods.get(name))
            .flat_map(|mods| mods.iter())
            .filter(|modifier| modifier.mod_type == mod_type && modifier.matches(cfg))
            .filter_map(|modifier| {
                modifier.effective_number(cfg).map(|value| ModContribution {
                    name: modifier.name.clone(),
                    mod_type: modifier.mod_type,
                    value,
                    origin: modifier.origin.clone(),
                    raw_text: modifier.source.clone(),
                })
            })
            .collect()
    }

    pub fn sum_traced(
        &self,
        mod_type: ModType,
        cfg: &CalcConfig,
        names: &[ModName],
        trace: &mut TraceGraph,
        label: impl Into<String>,
    ) -> TracedValue {
        let contributions = self.contributions(mod_type, cfg, names);
        let value = contributions
            .iter()
            .map(|contribution| contribution.value)
            .sum();
        let query_node = trace.add_node(label, value, TraceOperation::QuerySum);

        for contribution in contributions {
            let source = contribution
                .origin
                .as_ref()
                .map(|origin| origin.source_id.clone())
                .unwrap_or_else(|| {
                    SourceId::new(
                        SourceKind::Derived,
                        format!(
                            "{}.{}",
                            contribution.name,
                            contribution.mod_type.as_trace_label()
                        ),
                    )
                });
            let label = contribution.raw_text.clone().unwrap_or_else(|| {
                format!(
                    "{} {} {}",
                    contribution.name,
                    contribution.mod_type.as_trace_label(),
                    contribution.value
                )
            });
            let input_node = trace.add_source_node(label, contribution.value, source);
            trace.add_edge(input_node, query_node);
        }

        TracedValue {
            value,
            node_id: query_node,
        }
    }

    pub fn more(&self, cfg: &CalcConfig, names: &[ModName]) -> f64 {
        self.more_rounded(cfg, names, |_| true)
    }

    /// PoB2 `ModList.lua` `MoreInternal` 语义：**逐 modName** 先连乘该名下所有 MORE mod 得
    /// `modResult`，再 `round(modResult, 2)`（默认精度），最后跨 modName 连乘。逐名取整避免多
    /// more 乘区的浮点末位漂移（影响 golden 逐值对账）。`extra` 施加额外筛选（如槽位）。
    fn more_rounded(
        &self,
        cfg: &CalcConfig,
        names: &[ModName],
        extra: impl Fn(&Modifier) -> bool,
    ) -> f64 {
        names
            .iter()
            .filter_map(|name| self.mods.get(name))
            .map(|mods| {
                let mod_result = mods
                    .iter()
                    .filter(|m| m.mod_type == ModType::More && m.matches(cfg) && extra(m))
                    .filter_map(|m| m.effective_number(cfg))
                    .fold(1.0, |product, value| product * (1.0 + value / 100.0));
                round_more(mod_result)
            })
            .fold(1.0, |product, mod_result| product * mod_result)
    }

    /// Traced [`more`](Self::more)：把 `Π(1 + v/100)` 记录为单个 MoreProduct 节点，
    /// 每个贡献 modifier 各连一个 source 输入节点。
    pub fn more_traced(
        &self,
        cfg: &CalcConfig,
        names: &[ModName],
        trace: &mut TraceGraph,
        label: impl Into<String>,
    ) -> TracedValue {
        let contributions = self.contributions(ModType::More, cfg, names);
        // PoB2 逐 modName：先连乘同名 MORE 得 modResult，round(·,2) 后跨名连乘（与 `more` 一致）。
        let mut per_name: Vec<(ModName, f64)> = Vec::new();
        for contribution in &contributions {
            let factor = 1.0 + contribution.value / 100.0;
            match per_name.iter_mut().find(|(n, _)| *n == contribution.name) {
                Some((_, product)) => *product *= factor,
                None => per_name.push((contribution.name.clone(), factor)),
            }
        }
        let factor = per_name.iter().fold(1.0, |product, (_, mod_result)| {
            product * round_more(*mod_result)
        });
        let factor_node = trace.add_node(label, factor, TraceOperation::MoreProduct);

        for contribution in contributions {
            let source = contribution
                .origin
                .as_ref()
                .map(|origin| origin.source_id.clone())
                .unwrap_or_else(|| {
                    SourceId::new(
                        SourceKind::Derived,
                        format!(
                            "{}.{}",
                            contribution.name,
                            contribution.mod_type.as_trace_label()
                        ),
                    )
                });
            let input_label = contribution
                .raw_text
                .clone()
                .unwrap_or_else(|| format!("{} MORE {}", contribution.name, contribution.value));
            let input_node = trace.add_source_node(input_label, contribution.value, source);
            trace.add_edge(input_node, factor_node);
        }

        TracedValue {
            value: factor,
            node_id: factor_node,
        }
    }

    /// 仅累加**无槽位限定**（无 [`ModTag::SlotName`]）的 modifier（per-slot 防御聚合的全局桶）。
    ///
    /// 与 [`sum`](Self::sum) 区别：[`sum`] 对槽位 tag 透明（会一并算入槽位限定 mod），
    /// 而本方法显式排除带槽位 tag 的 mod，使之只通过 [`sum_for_slot`](Self::sum_for_slot)
    /// 在匹配槽位生效。PoB2 `calcLib.mod` 的 global 部分对应此。
    pub fn sum_global_only(&self, mod_type: ModType, cfg: &CalcConfig, names: &[ModName]) -> f64 {
        names
            .iter()
            .filter_map(|name| self.mods.get(name))
            .flat_map(|mods| mods.iter())
            .filter(|modifier| {
                modifier.mod_type == mod_type
                    && modifier.slot_name().is_none()
                    && modifier.matches(cfg)
            })
            .filter_map(|modifier| modifier.effective_number(cfg))
            .sum()
    }

    /// 仅累加限定到 `slot`（[`ModTag::SlotName`] 匹配）的 modifier（per-slot 防御聚合的槽位桶）。
    pub fn sum_for_slot(
        &self,
        mod_type: ModType,
        cfg: &CalcConfig,
        names: &[ModName],
        slot: &str,
    ) -> f64 {
        names
            .iter()
            .filter_map(|name| self.mods.get(name))
            .flat_map(|mods| mods.iter())
            .filter(|modifier| {
                modifier.mod_type == mod_type
                    && modifier.slot_name() == Some(slot)
                    && modifier.matches(cfg)
            })
            .filter_map(|modifier| modifier.effective_number(cfg))
            .sum()
    }

    /// 仅连乘**无槽位限定**的 `More` modifier（per-slot 防御聚合的全局 more 桶）。
    pub fn more_global_only(&self, cfg: &CalcConfig, names: &[ModName]) -> f64 {
        self.more_rounded(cfg, names, |m| m.slot_name().is_none())
    }

    /// 仅连乘限定到 `slot` 的 `More` modifier（per-slot 防御聚合的槽位 more 桶）。
    pub fn more_for_slot(&self, cfg: &CalcConfig, names: &[ModName], slot: &str) -> f64 {
        self.more_rounded(cfg, names, |m| m.slot_name() == Some(slot))
    }

    /// per-slot 防御聚合所需的槽位 BASE 词条：返回各 `(slot, value)`（带 [`ModTag::SlotName`]
    /// 的 `Base` modifier）。无槽位的 BASE 由调用方另行用 [`sum_global_only`](Self::sum_global_only)
    /// 取（作为「无槽位底」，只享全局乘区）。
    pub fn slot_bases(&self, cfg: &CalcConfig, name: &ModName) -> Vec<(String, f64)> {
        self.mods
            .get(name)
            .into_iter()
            .flat_map(|mods| mods.iter())
            .filter(|modifier| modifier.mod_type == ModType::Base && modifier.matches(cfg))
            .filter_map(|modifier| {
                let slot = modifier.slot_name()?.to_string();
                let value = modifier.effective_number(cfg)?;
                Some((slot, value))
            })
            .collect()
    }

    pub fn flag(&self, cfg: &CalcConfig, name: ModName) -> bool {
        self.mods
            .get(&name)
            .into_iter()
            .flat_map(|mods| mods.iter())
            .any(|modifier| {
                modifier.mod_type == ModType::Flag
                    && modifier.matches(cfg)
                    && modifier.value.as_bool().unwrap_or(false)
            })
    }

    /// 返回**第一条**命中该 flag 的 modifier 的归因 `SourceId`（无 origin 或未命中返回 `None`）。
    /// 供归因路径把旗标行为回溯到来源（如某天赋/宝石赋予 `CritChanceLucky`）。
    pub fn flag_origin(&self, cfg: &CalcConfig, name: ModName) -> Option<SourceId> {
        self.mods
            .get(&name)
            .into_iter()
            .flat_map(|mods| mods.iter())
            .find(|modifier| {
                modifier.mod_type == ModType::Flag
                    && modifier.matches(cfg)
                    && modifier.value.as_bool().unwrap_or(false)
            })
            .and_then(|modifier| {
                modifier
                    .origin
                    .as_ref()
                    .map(|origin| origin.source_id.clone())
            })
    }

    /// Traced [`flag`](Self::flag)：记录一个 QueryFlag 节点（值 1.0/0.0），并把所有
    /// 命中该 flag 的 source 连为输入。
    pub fn flag_traced(
        &self,
        cfg: &CalcConfig,
        name: ModName,
        trace: &mut TraceGraph,
        label: impl Into<String>,
    ) -> bool {
        let active = self.flag(cfg, name.clone());
        let flag_node = trace.add_node(
            label,
            if active { 1.0 } else { 0.0 },
            TraceOperation::QueryFlag,
        );
        let matching = self
            .mods
            .get(&name)
            .into_iter()
            .flat_map(|mods| mods.iter());
        for modifier in matching {
            if modifier.mod_type != ModType::Flag
                || !modifier.matches(cfg)
                || !modifier.value.as_bool().unwrap_or(false)
            {
                continue;
            }
            let source = modifier
                .origin
                .as_ref()
                .map(|origin| origin.source_id.clone())
                .unwrap_or_else(|| {
                    SourceId::new(SourceKind::Derived, format!("{}.FLAG", modifier.name))
                });
            let input_label = modifier
                .source
                .clone()
                .unwrap_or_else(|| format!("{} FLAG", modifier.name));
            let input_node = trace.add_source_node(input_label, 1.0, source);
            trace.add_edge(input_node, flag_node);
        }
        active
    }

    pub fn override_(&self, cfg: &CalcConfig, name: ModName) -> Option<f64> {
        self.mods
            .get(&name)
            .into_iter()
            .flat_map(|mods| mods.iter().rev())
            .filter(|modifier| modifier.mod_type == ModType::Override && modifier.matches(cfg))
            .filter_map(|modifier| modifier.effective_number(cfg))
            .next()
    }

    /// PoB2 `ModStore:GetMultiplier(var, cfg)`（ModStore.lua:276-278）等价原语：
    /// `Override("Multiplier:var")` 优先，否则 `cfg.multipliers[var] + Sum(BASE, "Multiplier:var")`。
    ///
    /// PoBR 的 multiplier 求值（[`crate::Modifier::effective_number`] → `cfg.multiplier`）只读
    /// `cfg.multipliers` HashMap，无法消费 modDB 内 `Multiplier:X` 形态的 BASE/Override mod。
    /// 编排层在注入完所有来源后，对需要的 `var` 调用本方法把结果写回 `cfg.with_multiplier(var, _)`，
    /// 使 per-X 缩放词条引用到这些 modDB 乘数（契约：`Multiplier:X` 由编排层经此原语注入 cfg）。
    ///
    /// 注意：parent 链与 PerStat `tag.base` 偏置暂未纳入；此处覆盖 PoB2 主路径的
    /// Override / multipliers / Sum(BASE) 三项基线。
    pub fn get_multiplier(&self, var: &str, cfg: &CalcConfig) -> f64 {
        let name = ModName::from(format!("Multiplier:{var}"));
        if let Some(overridden) = self.override_(cfg, name.clone()) {
            return overridden;
        }
        cfg.multiplier(var) + self.sum(ModType::Base, cfg, &[name])
    }

    /// Traced [`override_`](Self::override_)：记录一个 QueryOverride 节点（生效值或 0），
    /// 把胜出的 override modifier 连为唯一输入（后写覆盖先写）。
    pub fn override_traced(
        &self,
        cfg: &CalcConfig,
        name: ModName,
        trace: &mut TraceGraph,
        label: impl Into<String>,
    ) -> (Option<f64>, TraceNodeId) {
        let value = self.override_(cfg, name.clone());
        let override_node =
            trace.add_node(label, value.unwrap_or(0.0), TraceOperation::QueryOverride);
        if let Some(winning) = self
            .mods
            .get(&name)
            .into_iter()
            .flat_map(|mods| mods.iter().rev())
            .find(|modifier| modifier.mod_type == ModType::Override && modifier.matches(cfg))
        {
            let source = winning
                .origin
                .as_ref()
                .map(|origin| origin.source_id.clone())
                .unwrap_or_else(|| {
                    SourceId::new(SourceKind::Derived, format!("{}.OVERRIDE", winning.name))
                });
            let input_label = winning
                .source
                .clone()
                .unwrap_or_else(|| format!("{} OVERRIDE {:?}", winning.name, value));
            let input_node = trace.add_source_node(input_label, value.unwrap_or(0.0), source);
            trace.add_edge(input_node, override_node);
        }
        (value, override_node)
    }

    /// 遍历库内全部 modifier（不分 name 桶，顺序为桶内插入序）。供 bench / 诊断用。
    pub fn iter_mods(&self) -> impl Iterator<Item = &Modifier> {
        self.mods.values().flat_map(|mods| mods.iter())
    }

    pub fn list(&self, cfg: &CalcConfig, name: ModName) -> Vec<String> {
        self.mods
            .get(&name)
            .into_iter()
            .flat_map(|mods| mods.iter())
            .filter(|modifier| modifier.mod_type == ModType::List && modifier.matches(cfg))
            .filter_map(|modifier| match &modifier.value {
                ModValue::Text(value) => Some(value.clone()),
                ModValue::Number(_) | ModValue::Bool(_) => None,
            })
            .collect()
    }
}

#[derive(Debug, Clone, Default)]
pub struct ModList {
    db: ModDb,
    parent: Option<Box<ModList>>,
}

impl ModList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_parent(parent: ModList) -> Self {
        Self {
            db: ModDb::new(),
            parent: Some(Box::new(parent)),
        }
    }

    pub fn add_mod(&mut self, modifier: Modifier) {
        self.db.add_mod(modifier);
    }

    pub fn sum(&self, mod_type: ModType, cfg: &CalcConfig, names: &[ModName]) -> f64 {
        let local = self.db.sum(mod_type, cfg, names);
        let parent = self
            .parent
            .as_ref()
            .map_or(0.0, |parent| parent.sum(mod_type, cfg, names));
        local + parent
    }
}
