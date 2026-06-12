use std::collections::HashMap;

use pobr_data::catalog::high_precision_mods::HighPrecisionModsDef;
use pobr_data::prelude::*;

use crate::{CalcConfig, ModValue, Modifier, TraceGraph, TraceNodeId, TraceOperation, TracedValue};

/// 单个 modName 的 MORE 连乘积按 PoB2 默认精度 `round(·, 2)` 归一（ModList.lua MoreInternal）。
fn round_more(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

/// PoB2 `round(val, dec)`（vendor `Common.lua:648-654`）：
/// `m_floor(val × 10^dec + 0.5) / 10^dec`——半数恒向上（非银行家舍入；
/// 负数与 Rust `f64::round` 在 `.5` 处不同，须用本函数对拍 vendor）。
fn pob_round(value: f64, dec: i32) -> f64 {
    let mult = 10f64.powi(dec);
    (value * mult + 0.5).floor() / mult
}

/// 取整精度规则（M4-T1 W-A2）：vendor `data.highPrecisionMods` +
/// `data.defaultHighPrecision`（`Data.lua:413-530`，入库为
/// `overlay/high_precision_mods.json`，经 pobr-gamedata `RuleSet` 注入）。
///
/// `Default`（未注入数据）= **无例外表** fallback（蓝图 W-A2：默认
/// `round(·,2)` 截整；小数原值仍走 `default_high_precision = 1`——该常量与
/// 入库 JSON 逐值相等，搬迁不变式）。消费方：[`ModDb::scale_add_mod`] 与
/// MORE 聚合精度例外分支。
#[derive(Debug, Clone, Default)]
pub struct HighPrecisionRules {
    def: Option<HighPrecisionModsDef>,
}

impl HighPrecisionRules {
    /// 从入库表构造（pobr-gamedata `RuleSet::high_precision_mods` → 此处）。
    pub fn from_def(def: HighPrecisionModsDef) -> Self {
        Self { def: Some(def) }
    }

    /// `data.highPrecisionMods[name][type]`（vendor `ModStore.lua:69` /
    /// `ModDB.lua:175-180`）。mod type 键用 vendor 字面量（`BASE`/`MORE`…，
    /// = [`ModType::as_trace_label`]）。未注入 / 未命中 → `None`。
    pub fn precision_for(&self, name: &str, mod_type: ModType) -> Option<u32> {
        self.def
            .as_ref()?
            .mods
            .get(name)?
            .get(mod_type.as_trace_label())
            .copied()
    }

    /// `data.defaultHighPrecision`（`Data.lua:413` = 1；未注入时同值 fallback）。
    pub fn default_high_precision(&self) -> u32 {
        self.def.as_ref().map_or(1, |d| d.default_high_precision)
    }
}

/// ScaleAddMod 的数值取整（vendor `ModStore.lua:69-77` 逐字）：
/// - 精度 = 例外表 `[name][type]`，未命中且**原值含小数** → `defaultHighPrecision`；
/// - 有精度 `p` → `m_floor(value × scale × 10^p) / 10^p`（floor，非四舍五入）；
/// - 无精度 → `m_modf(round(value × scale, 2))` 取整数部（向零截断）。
fn scale_mod_value(
    name: &str,
    mod_type: ModType,
    value: f64,
    scale: f64,
    rules: &HighPrecisionRules,
) -> f64 {
    let precision = rules
        .precision_for(name, mod_type)
        .or_else(|| (value.floor() != value).then(|| rules.default_high_precision()));
    match precision {
        Some(p) => {
            let power = 10f64.powi(p as i32);
            (value * scale * power).floor() / power
        }
        None => pob_round(value * scale, 2).trunc(),
    }
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
    /// 取整精度规则（M4-T1 W-A2）：vendor 把 `data.highPrecisionMods` 当全局
    /// 环境数据消费，pobr 收口在 db 实例上（计算核心零 I/O，由编排层经
    /// [`Self::set_high_precision_rules`] 注入）。`Default` = 无例外表 →
    /// MORE 聚合走默认 `round(·,2)` 分支，行为与字段引入前逐字一致。
    high_precision: HighPrecisionRules,
}

impl ModDb {
    pub fn new() -> Self {
        Self::default()
    }

    /// 注入取整精度规则（来源 = pobr-gamedata `RuleSet::high_precision_mods`）。
    /// 消费点：MORE 聚合精度例外分支（[`Self::more`]）与调用方传给
    /// [`Self::scale_add_mod`] 的同一份规则。
    pub fn set_high_precision_rules(&mut self, rules: HighPrecisionRules) {
        self.high_precision = rules;
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

    /// 写侧原语 ReplaceMod（M4-T1 W-A2；vendor `ModStore.lua:114-118` +
    /// `ModDB.lua:38-66` `ReplaceModInternal`）：同 `name + type + flags +
    /// keywordFlags + source` 的既有 mod **原位替换**（保持桶内顺序），无匹配则
    /// append（= [`add_mod`](Self::add_mod)）。
    ///
    /// 返回是否发生替换（`false` = 走了 append 分支）。典型消费方：弩 reload 的
    /// `Multiplier:BoltsReloadedPastSixSeconds` 回写（`CalcOffence.lua:2890-2894`，
    /// T4 W-D2）。注：pobr `ModDb` 无 parent 链（vendor 的 parent 递归不适用）。
    pub fn replace_mod(&mut self, modifier: Modifier) -> bool {
        if let Some(bucket) = self.mods.get_mut(&modifier.name)
            && let Some(slot) = bucket.iter_mut().find(|cur| {
                cur.mod_type == modifier.mod_type
                    && cur.flags == modifier.flags
                    && cur.keyword_flags == modifier.keyword_flags
                    && cur.source == modifier.source
            })
        {
            *slot = modifier;
            return true;
        }
        self.add_mod(modifier);
        false
    }

    /// 写侧原语 ConvertMod（M4-T1 W-A2；vendor `ModStore.lua:120-132` +
    /// `ModDB.lua:75-105` `ConvertModInternal`）：在 `from` 桶内找同
    /// `type + flags + keywordFlags + source`（与 `to` 比对）的既有 mod，
    /// **从旧名桶移除、`to` 落新名桶**（跨桶搬迁）；无匹配则直接 append `to`。
    ///
    /// 返回是否发生搬迁。与 vendor 的已知出入（登记）：vendor 以 `converted`
    /// 标记防同一 mod 被链式转换二次命中；pobr `Modifier` 无该标记——`from` 与
    /// `to.name` 相同或转换链回环的场景未防护（当前无消费方，T4/T5 接入时若
    /// 出现链式转换再补）。
    pub fn convert_mod(&mut self, from: &ModName, to: Modifier) -> bool {
        if let Some(old_bucket) = self.mods.get_mut(from)
            && let Some(index) = old_bucket.iter().position(|cur| {
                cur.mod_type == to.mod_type
                    && cur.flags == to.flags
                    && cur.keyword_flags == to.keyword_flags
                    && cur.source == to.source
            })
        {
            old_bucket.remove(index);
            self.add_mod(to);
            return true;
        }
        self.add_mod(to);
        false
    }

    /// 写侧原语 ScaleAddMod（M4-T1 W-A2；vendor `ModStore.lua:45-81`）：按
    /// `scale` 缩放数值后入库，取整走 [`HighPrecisionRules`]（见
    /// [`scale_mod_value`] 的逐字分支）。`scale == 1` 直接入库（vendor :54）。
    ///
    /// 与 vendor 的已知出入（登记）：
    /// - `effects.unscalable`（:46-52，不可缩放词条原值入库）——pobr `ModTag`
    ///   暂无该位，全部视为可缩放（当前解析层无产出点）；
    /// - `value.keyOfScaledMod` / `+level` floor 特例（:59-66）——针对 table 值
    ///   内非 `mod` 键的缩放，pobr `ModValue` 无对应形态；
    /// - [`ModValue::NestedMods`]（vendor `value.mod` 嵌套载荷）按同规则逐内层
    ///   Number 缩放（vendor :57 `subMod = scaledMod.value.mod` 的多载荷推广）。
    pub fn scale_add_mod(
        &mut self,
        mut modifier: Modifier,
        scale: f64,
        rules: &HighPrecisionRules,
    ) {
        if scale == 1.0 {
            self.add_mod(modifier);
            return;
        }
        match &mut modifier.value {
            ModValue::Number(value) => {
                *value = scale_mod_value(
                    modifier.name.as_str(),
                    modifier.mod_type,
                    *value,
                    scale,
                    rules,
                );
            }
            ModValue::NestedMods(nested) => {
                for inner in nested.iter_mut() {
                    if let ModValue::Number(value) = &mut inner.value {
                        *value = scale_mod_value(
                            inner.name.as_str(),
                            inner.mod_type,
                            *value,
                            scale,
                            rules,
                        );
                    }
                }
            }
            // 布尔 / 文本载荷不缩放（vendor 仅对 number 分支取整，:68）。
            ModValue::Bool(_) | ModValue::Text(_) => {}
        }
        self.add_mod(modifier);
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
        Self {
            mods,
            high_precision: self.high_precision.clone(),
        }
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

    /// PoB2 `MoreInternal` 语义（`ModDB.lua:156-190`，桶式变体）：**逐 modName**
    /// 先连乘该名下所有 MORE mod 得 `modResult`，按精度取整后跨 modName 连乘。
    /// 逐名取整避免多 more 乘区的浮点末位漂移。`extra` 施加额外筛选（如槽位）。
    ///
    /// 取整分支（M4-T1 W-A2 例外修复，10-G6；vendor :175-186 逐字）：
    /// - 默认（无精度例外命中）：`result *= round(modResult, 2)`——与例外分支
    ///   引入前逐字一致（[`round_more`] 不动，搬迁不变式）；
    /// - 命中 [`HighPrecisionRules`] 的 MORE 例外（`SupportManaMultiplier` /
    ///   `ReservationMultiplier` → 4）：`result = floor(result × modResult ×
    ///   10^p) / 10^p`（floor 作用于**累计积**）。
    /// - vendor quirk 逐字保留：`modPrecision` 在**整个查询内跨名持留**
    ///   （一旦置位不复位、只 max 抬升，:175-180）——例外名之后的普通名乃至
    ///   缺桶名（`modResult = 1`）也走 floor 分支重截累计积。
    fn more_rounded(
        &self,
        cfg: &CalcConfig,
        names: &[ModName],
        extra: impl Fn(&Modifier) -> bool,
    ) -> f64 {
        let mut result = 1.0;
        let mut mod_precision: Option<u32> = None;
        for name in names {
            let mut mod_result = 1.0;
            for m in self.mods.get(name).into_iter().flatten() {
                if m.mod_type != ModType::More || !m.matches(cfg) || !extra(m) {
                    continue;
                }
                let Some(value) = m.effective_number(cfg) else {
                    continue;
                };
                mod_result *= 1.0 + value / 100.0;
                // vendor ModDB.lua:175-180：modPrecision = max(prev, 表值 or prev)。
                let hit = self
                    .high_precision
                    .precision_for(m.name.as_str(), ModType::More);
                mod_precision = match (mod_precision, hit) {
                    (Some(prev), hit) => Some(prev.max(hit.unwrap_or(prev))),
                    (None, hit) => hit,
                };
            }
            match mod_precision {
                Some(p) => {
                    let power = 10f64.powi(p as i32);
                    result = (result * mod_result * power).floor() / power;
                }
                None => result *= round_more(mod_result),
            }
        }
        result
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
        // 取整口径与 [`more`](Self::more) 共用同一实现（含 W-A2 精度例外分支），
        // traced / 非 traced 值恒等（此前为重复实现，易漂移）。
        let factor = self.more(cfg, names);
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
                ModValue::Number(_) | ModValue::Bool(_) | ModValue::NestedMods(_) => None,
            })
            .collect()
    }

    /// List 通道的嵌套 modifier 透传：收集 `name` 名下 List 型、`matches(cfg)` 通过的
    /// [`ModValue::NestedMods`] 载荷（按桶内插入序克隆展开）。
    ///
    /// 供编排层把 `EnemyModifier` 等「外层落 player db、内层转发目标 db」的嵌套词条
    /// 取出转发（M3 env_finalize `forward_enemy_modifiers`）；本方法只透传不求值，
    /// 内层 mod 的 `matches`/`effective_number` 由目标 db 聚合时按目标上下文结算。
    pub fn list_nested(&self, cfg: &CalcConfig, name: ModName) -> Vec<Modifier> {
        self.mods
            .get(&name)
            .into_iter()
            .flat_map(|mods| mods.iter())
            .filter(|modifier| modifier.mod_type == ModType::List && modifier.matches(cfg))
            .filter_map(|modifier| modifier.value.as_nested_mods())
            .flat_map(|nested| nested.iter().cloned())
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
