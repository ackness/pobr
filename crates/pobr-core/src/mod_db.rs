use std::collections::HashMap;

use pobr_data::prelude::*;

use crate::{CalcConfig, ModValue, Modifier, TraceGraph, TraceNodeId, TraceOperation, TracedValue};

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
        names
            .iter()
            .filter_map(|name| self.mods.get(name))
            .flat_map(|mods| mods.iter())
            .filter(|modifier| modifier.mod_type == ModType::More && modifier.matches(cfg))
            .filter_map(|modifier| modifier.effective_number(cfg))
            .fold(1.0, |product, value| product * (1.0 + value / 100.0))
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
        let factor = contributions.iter().fold(1.0, |product, contribution| {
            product * (1.0 + contribution.value / 100.0)
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
