//! SkillStatMap 数据引擎（M1-T2.2）：把 `overlay/skill_stat_map.json`
//! （[`pobr_data::catalog::stat_map`]，vendor `Data/SkillStatMap.lua` 954 条全局 +
//! per-statSet 覆盖的确定性抽取）翻译为 PoBR [`Modifier`] 注入项，替代
//! `pobr-build::skill_stat_map` 的 751 行后缀启发式（缺口 18-G3 / 15-G2）。
//!
//! 纯函数 + 零 I/O（P9 注入风格）：catalog 由 pobr-gamedata 加载、pobr-build
//! 注入，本层只做查表 + merge 公式 + 名字/tag 翻译。
//!
//! ## merge 公式（vendor `Modules/CalcActiveSkill.lua:112` 逐字对齐）
//!
//! ```text
//! 注入值 = entry.value or stat值 × (entry.mult or 1) × scalar / (entry.div or 1) + (entry.base or 0)
//! ```
//!
//! group 元素（无 name 的嵌套 mod 列表）用 group 级参数替代 entry 级参数
//! （CalcActiveSkill.lua:117）。`scalar`（`checkForScalarMultiplier`，:53-66，
//! 依赖 mod_db 反查 `Multiplier:<var>`）M1 固定 1.0，**含 scalar 需求的条目整条归
//! [`MappedOutcome::Unsupported`]**（统计上报，不错算）。
//!
//! ## 支持边界（第一批，宁可跳过不可错算）
//!
//! - **tag**：无 tag / `Condition` / `Multiplier` / `PerStat`（映射到 PoBR 既有
//!   [`ModTag`] 体系）；其余 tag 类型（GlobalEffect / DistanceRamp / SkillType /
//!   actor 系…）条目整条 Unsupported——与 legacy「保守跳过」口径一致，保证双跑可比。
//! - **ModName 翻译层**：PoB2 名 → PoBR 名的 Rust 常量表（框架语义 L4，P2 判据：
//!   名字随机制不随版本变；架构 owner 裁决见蓝图 §6 Q2）。初版从
//!   `pobr-build::skill_stat_map` 既有映射反推，未知名字归
//!   [`UnsupportedReason::UnknownModName`] 上报，由双跑 diff 驱动补全。
//! - **skill_data**（vendor `skill(key, …)` 构造器）：伤害基值键
//!   （`FireMin`/`PhysicalMax` 等）翻译为 `<Type>DamageMin/Max` BASE modifier
//!   （PoBR 无 skillData 表，伤害基值经 modifier 管线消费，对齐 legacy 口径）；
//!   `duration` 出 [`MappedItem::SkillData`]（消费方接入前调用方可忽略）；其余键
//!   Unsupported 统计。
//! - **flag 构造器**（vendor `flag(name)`，技能行为开关如 `projectile`）：
//!   白名单翻译有 ModDb flag 消费方的名字（M4-H：crit/lucky 族），其余
//!   Unsupported（见 [`is_consumable_flag`]）。

use std::collections::BTreeMap;

use pobr_data::catalog::stat_map::{SkillStatMapDef, StatMapEntry, StatMapMod, StatMapValue};
use pobr_data::modifier::{KeywordFlags, ModFlags, ModType};
use pobr_data::skill::SkillTypes;

use crate::modifier::{ModTag, Modifier};

/// M1 阶段 scalar 固定值（vendor `checkForScalarMultiplier` 的 mod_db 反查暂不
/// 接入；含 scalar 字段的条目整条 Unsupported，本常量仅为公式形态完整保留）。
const SCALAR_FIXED: f64 = 1.0;

/// statmap 查表目录（global + per-statSet 覆盖的聚合视图）。
///
/// 由 `overlay/skill_stat_map.json` 反序列化的 [`SkillStatMapDef`] 构造；
/// 查找语义：per-set 命中优先，miss 落回 global（vendor `Data.lua:835-847`
/// statMap metatable 的 `__index` 链等价）。
#[derive(Debug, Clone)]
pub struct StatMapCatalog {
    def: SkillStatMapDef,
}

impl StatMapCatalog {
    /// 从反序列化的 overlay 文档构造。
    pub fn new(def: SkillStatMapDef) -> Self {
        Self { def }
    }

    /// 查条目：`set_key` 给定时先查 `per_stat_set[effect_id][set_key]`，
    /// miss 落回 `global`。
    ///
    /// `set_key = None` = 调用方未做 statSet 选择 → 取**默认 set "1"** 的覆盖表：
    /// PoB2 缺省 statSetIndex 恒为 1（vendor `SkillsTab.lua:354`
    /// `gemInstance.statSet = { index = tonumber(child.attrib.statSetIndex) or 1 }`，
    /// `CalcActiveSkill.lua:171` `statSet = …statSets[activeEffect.statSet.index]`），
    /// 选中 set 的 statMap miss 时经 metatable 链落回全局表（`Data.lua` statMap
    /// `__index`）。非 "1" 的 set 覆盖只有显式 `set_key` 才会命中（statSetIndex
    /// 选择接线随 T5 多 statSet 模型）。
    fn lookup(&self, effect_id: &str, set_key: Option<&str>, stat: &str) -> Option<&StatMapEntry> {
        let key = set_key.unwrap_or("1");
        if let Some(entry) = self
            .def
            .per_stat_set
            .get(effect_id)
            .and_then(|sets| sets.get(key))
            .and_then(|map| map.get(stat))
        {
            return Some(entry);
        }
        self.def.global.get(stat)
    }

    /// global 段条目数（双跑报告用）。
    pub fn global_len(&self) -> usize {
        self.def.global.len()
    }

    /// global 段 stat id 迭代（双跑 L1 穷举用）。
    pub fn global_stats(&self) -> impl Iterator<Item = &str> {
        self.def.global.keys().map(String::as_str)
    }

    /// 指定 (effect, set, stat) 是否存在 per-statSet 覆盖（双跑 L1 用：仅当存在
    /// 覆盖时才需要按 effect 上下文单独出 diff 行，其余 stat 走 global 行去重）。
    pub fn has_per_set_override(&self, effect_id: &str, set_key: &str, stat: &str) -> bool {
        self.def
            .per_stat_set
            .get(effect_id)
            .and_then(|sets| sets.get(set_key))
            .is_some_and(|map| map.contains_key(stat))
    }

    /// per-statSet 段迭代（双跑报告 / oracle 抽样选样用）：
    /// `(effect_id, set_key, stat, entry)`。
    pub fn per_set_entries(&self) -> impl Iterator<Item = (&str, &str, &str, &StatMapEntry)> {
        self.def.per_stat_set.iter().flat_map(|(effect, sets)| {
            sets.iter().flat_map(move |(set_key, map)| {
                map.iter().map(move |(stat, entry)| {
                    (effect.as_str(), set_key.as_str(), stat.as_str(), entry)
                })
            })
        })
    }

    /// global 段条目迭代（oracle 抽样选样用）。
    pub fn global_entries(&self) -> impl Iterator<Item = (&str, &StatMapEntry)> {
        self.def.global.iter().map(|(k, v)| (k.as_str(), v))
    }
}

impl From<SkillStatMapDef> for StatMapCatalog {
    fn from(def: SkillStatMapDef) -> Self {
        Self::new(def)
    }
}

/// 单个翻译产物：注入 modifier 或 skill data 键值。
#[derive(Debug, Clone, PartialEq)]
pub enum MappedItem {
    /// 一条可直接入 ModDb 的 PoBR modifier（名字已翻译、tag 已映射；Box
    /// 平衡与 SkillData 变体的尺寸差）。
    Modifier(Box<Modifier>),
    /// skill data 键值（vendor `skill(key, …)`；如 `duration`，秒）。
    /// 消费方按需接入；无消费方时调用侧忽略即可（不参与计算，不会错算）。
    SkillData {
        /// vendor skillData 键名（原样）。
        key: String,
        /// merge 公式产出的值。
        value: f64,
    },
}

/// 条目不支持的原因分类（双跑 L1 报告的统计维度）。
#[derive(Debug, Clone, PartialEq)]
pub enum UnsupportedReason {
    /// 抽取失真条目（vendor 函数值/畸形构造，`_unextractable: true`）。
    Unextractable,
    /// 条目含 scalar 需求（M1 scalar 固定 1.0，整条跳过避免错算）。
    ScalarMultiplier,
    /// PoB2 ModName 不在翻译表（含 flag 构造器的行为开关名）。
    UnknownModName(String),
    /// mod 构造器缺 type（vendor 笔误条目，抽取忠实保留）。
    MissingModType,
    /// 聚合类型不在第一批（如非 skill_data 的 `LIST`）。
    UnsupportedModType(String),
    /// tag 类型不在第一批（GlobalEffect / DistanceRamp / actor 系…）。
    UnsupportedTag(String),
    /// ModFlag 组合无法翻译到 PoBR ModName 语义。
    UnsupportedFlags(String),
    /// KeywordFlag 语义无法保守丢弃（仅伤害基值族允许丢弃，对齐 legacy）。
    UnsupportedKeywordFlags(String),
    /// skill_data 键不在第一批白名单。
    UnsupportedSkillDataKey(String),
    /// 条目带 `skillFlag`（由 PoB2 statSet flags 路径消费，非 merge 公式）。
    SkillFlag(String),
    /// 未知元素种类（抽取器约定外的 kind）。
    UnsupportedKind(String),
}

impl UnsupportedReason {
    /// 稳定分类标签（双跑报告聚合键）。
    pub fn category(&self) -> &'static str {
        match self {
            Self::Unextractable => "unextractable",
            Self::ScalarMultiplier => "scalar",
            Self::UnknownModName(_) => "unknown_mod_name",
            Self::MissingModType => "missing_mod_type",
            Self::UnsupportedModType(_) => "mod_type",
            Self::UnsupportedTag(_) => "tag",
            Self::UnsupportedFlags(_) => "flags",
            Self::UnsupportedKeywordFlags(_) => "keyword_flags",
            Self::UnsupportedSkillDataKey(_) => "skill_data_key",
            Self::SkillFlag(_) => "skill_flag",
            Self::UnsupportedKind(_) => "kind",
        }
    }
}

/// `map_stat` 的产出（契约 C3）。
#[derive(Debug, Clone, PartialEq)]
pub enum MappedOutcome {
    /// 条目命中且全部元素可翻译——产出注入项列表。
    Mapped(Vec<MappedItem>),
    /// 条目命中但含第一批之外的语义，**整条**跳过（宁可跳过不可错算）。
    Unsupported(UnsupportedReason),
    /// catalog 无该 stat 条目（global 与 per-set 均 miss）。
    Unknown,
}

/// 把一条技能 stat 经 statmap 数据翻译为 PoBR 注入项。
///
/// - `effect_id` + `set_key`：per-statSet 覆盖定位（`set_key` = vendor `statSets`
///   1-based 下标的十进制字符串）；`set_key = None` 或 miss 落回 global；
/// - `stat_value`：stat 的运行时数值（分等级 stat + quality 叠加后）；
/// - merge 公式与支持边界见模块文档。
pub fn map_stat(
    catalog: &StatMapCatalog,
    effect_id: &str,
    set_key: Option<&str>,
    stat: &str,
    stat_value: f64,
) -> MappedOutcome {
    let Some(entry) = catalog.lookup(effect_id, set_key, stat) else {
        return MappedOutcome::Unknown;
    };
    map_entry(entry, stat_value)
}

/// 条目级翻译（查表后的纯 merge + 翻译；供 `map_stat` 与单测共用）。
fn map_entry(entry: &StatMapEntry, stat_value: f64) -> MappedOutcome {
    if entry.unextractable {
        return MappedOutcome::Unsupported(UnsupportedReason::Unextractable);
    }
    if let Some(flag) = &entry.skill_flag {
        return MappedOutcome::Unsupported(UnsupportedReason::SkillFlag(flag.clone()));
    }
    let entry_params = MergeParams {
        div: entry.div,
        mult: entry.mult,
        base: entry.base,
        value: entry.value,
    };
    let mut items = Vec::new();
    for element in &entry.mods {
        if let Err(reason) = collect_element(element, &entry_params, stat_value, &mut items) {
            // 任一元素不支持 → 整条跳过（半条注入会破坏 PoB2 条目的成组语义）。
            return MappedOutcome::Unsupported(reason);
        }
    }
    MappedOutcome::Mapped(items)
}

// ---- W-J：未选 statSet 的 global-only merge（蓝图 §2 W-J / §6 Q3）----

/// vendor `isGlobalEffect`（`Modules/CalcActiveSkill.lua:68-80`）等价判定：
/// modOrGroup 的**任一** mod 带 `type == "GlobalEffect"` tag 即视为 global。
///
/// vendor 形态：`local modList = modOrGroup.name and { modOrGroup } or modOrGroup`
/// ——有 name 即单 mod（查自身 tags），无 name 即 group（逐个成员查 tags）。
/// 抽取层把两种形态分别记为 `kind == "mod"/"flag"/"skill_data"`（tags 直挂）与
/// `kind == "group"`（成员在 [`StatMapMod::mods`]）；vendor 无嵌套 group，此处
/// 递归是忠实的保守泛化（任一层命中即 global）。
///
/// 判定粒度 = **modOrGroup 整体**（vendor `:103` 对 map 的每个元素判一次）：group
/// 任一成员带 tag 则整个 group 按 global 注入（含不带 tag 的成员），不做成员级拆分。
pub fn is_global_effect(element: &StatMapMod) -> bool {
    if element.kind == "group" {
        return element.mods.iter().any(is_global_effect);
    }
    element
        .tags
        .iter()
        .any(|tag| matches!(tag.get("type"), Some(StatMapValue::Text(t)) if t == "GlobalEffect"))
}

/// 条目级 global 记账探针：该 (effect, set, stat) 的 statmap 条目是否含**任一**
/// global modOrGroup。
///
/// 对应 vendor 选中 set merge 时的 `selectedGlobalStats[stat] = true` 记账
/// （`CalcActiveSkill.lua:104-106`：`if isGlobal and not onlyGlobals`）——调用方
/// 对**选中 set** 的每条 stat 调用本函数收集集合，未选 set 的 global-only merge
/// 对集合内 stat 整条跳过（`:107` `not (onlyGlobals and selectedGlobalStats[stat])`；
/// `selectedGlobalStats` 在 onlyGlobals 阶段不再变更，故 stat 级跳过与 vendor
/// 元素级条件等价）。条目 miss / `_unextractable`（mods 为空）按无 global 处理。
pub fn stat_has_global_mods(
    catalog: &StatMapCatalog,
    effect_id: &str,
    set_key: Option<&str>,
    stat: &str,
) -> bool {
    catalog
        .lookup(effect_id, set_key, stat)
        .is_some_and(|entry| entry.mods.iter().any(is_global_effect))
}

/// 未选 statSet 的 **global-only** 映射（vendor `mergeStatSet(set, onlyGlobals=true)`，
/// `CalcActiveSkill.lua:92-141` 的 `:124-129` 调用 + `:103-107` 注入条件）：
/// 只保留 [`is_global_effect`] 命中的 modOrGroup 参与 merge 公式与翻译，
/// 非 global 元素**静默跳过**（vendor 同口径——未选 set 的局部 mod 本就不该注入，
/// 不属于 Unsupported）。过滤后无 global 元素 → `Mapped(空)`。
///
/// `set_key` 应传**未选 set** 的 vendor 序号（per-set 覆盖按该 set 自身的 statMap
/// 链查，miss 落回 global——vendor `set.statMap` metatable `__index` 同语义）。
/// 选中 set 已按 global 记账的 stat 由调用方跳过（见 [`stat_has_global_mods`]）。
///
/// **第一批边界**：`GlobalEffect` tag 本身仍在 tag 翻译边界之外（buff 域随 M3
/// buff_pass 接入，`m1-statmap-switch-log.md` §5）——当前 global 元素会因
/// [`UnsupportedReason::UnsupportedTag`] 整条上报、注入为零（宁可跳过不可错算）；
/// M3 接通 GlobalEffect tag 后本路径**自动**开始产出注入项，无需再改本函数。
pub fn map_stat_global_only(
    catalog: &StatMapCatalog,
    effect_id: &str,
    set_key: Option<&str>,
    stat: &str,
    stat_value: f64,
) -> MappedOutcome {
    let Some(entry) = catalog.lookup(effect_id, set_key, stat) else {
        return MappedOutcome::Unknown;
    };
    if entry.unextractable {
        // 抽取失真：内容未知（mods 为空），按 Unsupported 上报可见（注入同样为零）。
        return MappedOutcome::Unsupported(UnsupportedReason::Unextractable);
    }
    // 注：entry 带 `skill_flag` 时其 flag 不是 modOrGroup（vendor statSet flags 路径
    // 消费），global-only 视角下无 global 元素 → 自然落入 `Mapped(空)`，与 vendor
    // 未选 set 不收 flags 的行为一致（flags 仅在选中 set 生效）。
    let entry_params = MergeParams {
        div: entry.div,
        mult: entry.mult,
        base: entry.base,
        value: entry.value,
    };
    let mut items = Vec::new();
    for element in entry.mods.iter().filter(|e| is_global_effect(e)) {
        if let Err(reason) = collect_element(element, &entry_params, stat_value, &mut items) {
            // 与 map_entry 同口径：任一保留元素不可翻译 → 整条跳过（成组语义）。
            return MappedOutcome::Unsupported(reason);
        }
    }
    MappedOutcome::Mapped(items)
}

// ---- M3-W4：curse 域（GlobalEffect effectType=Curse）敌侧映射 ----

/// 元素是否为 **curse buff 载荷**（vendor `GlobalEffect` tag 且
/// `effectType == "Curse"`——`CalcActiveSkill.lua:976-1041` 把命中元素从
/// skillModList 搬入 `buff.modList`（buff.type = "Curse"），`CalcPerform.lua:2286-2316`
/// 经 CurseEffect 乘区 `ScaleAddList` 后由 :2969-2984 写 enemyDB）。
/// group 任一成员命中即整组（与 [`is_global_effect`] 同口径的保守泛化）。
fn is_curse_effect(element: &StatMapMod) -> bool {
    if element.kind == "group" {
        return element.mods.iter().any(is_curse_effect);
    }
    element.tags.iter().any(|tag| {
        matches!(tag.get("type"), Some(StatMapValue::Text(t)) if t == "GlobalEffect")
            && matches!(tag.get("effectType"), Some(StatMapValue::Text(t)) if t == "Curse")
    })
}

/// 把一条 curse 技能 stat 经 statmap 数据翻译为**敌侧** PoBR 注入项
/// （BuffSpec.mods 取数通道，buff_pass curse 路径消费）。
///
/// 与 [`map_stat`] 的差异（curse 域语义，蓝图 m3-orchestration.md §6.3）：
/// - 只保留 [`is_curse_effect`] 命中的元素（非 curse 元素 = 技能局部 mod，走主
///   技能注入通道，此处**静默跳过**——vendor 同口径：未带 GlobalEffect 的 mod 留在
///   skillModList）。过滤后无 curse 元素 → `Mapped(空)`（该 stat 非 curse 载荷）。
/// - ModName 走敌侧翻译表 [`translate_curse_mod_name`]（enemy db 聚合名 =
///   vendor enemyDB 名直通；`ElementalResist` 展开三元素——pobr 敌侧抗性聚合
///   只读 `<Type>Resist`，vendor `enemyDB:Sum(.. type.."Resist", "ElementalResist")`
///   两名同收，展开等值）。未知名（pobr 暂无敌侧消费方，如
///   `TemporalChainsActionSpeed` / `FreezeBuildup`）整条归
///   [`UnsupportedReason::UnknownModName`] 上报（宁可跳过不可错算，可见性报表
///   由调用方落 Compare 记录）。
/// - `GlobalEffect` tag 本身剥除（路由元数据，不参与匹配）；带约定外键
///   （`effectCond` / `modCond` / `effectStackVar`…携带额外门控语义）→ 整条
///   Unsupported。其余 tag 经 [`translate_tag`] 直译——curse mod 落 enemy db，
///   `Condition` 的 var 即敌方自身状态（如 Enfeeble 的 `Unique`，与编排层
///   boss 档位置位的 cfg 条件 `Unique` 同名同义，**不加** Enemy 前缀）。
/// - flags / keyword_flags 第一批要求为空（curse 载荷数据全部无 flag；非空 =
///   作用域语义未建模 → Unsupported）。
pub fn map_curse_stat(
    catalog: &StatMapCatalog,
    effect_id: &str,
    set_key: Option<&str>,
    stat: &str,
    stat_value: f64,
) -> MappedOutcome {
    let Some(entry) = catalog.lookup(effect_id, set_key, stat) else {
        return MappedOutcome::Unknown;
    };
    if entry.unextractable {
        return MappedOutcome::Unsupported(UnsupportedReason::Unextractable);
    }
    let entry_params = MergeParams {
        div: entry.div,
        mult: entry.mult,
        base: entry.base,
        value: entry.value,
    };
    let mut items = Vec::new();
    for element in entry.mods.iter().filter(|e| is_curse_effect(e)) {
        if let Err(reason) = collect_curse_element(element, &entry_params, stat_value, &mut items) {
            // 与 map_entry 同口径：任一 curse 元素不可翻译 → 整条跳过（成组语义）。
            return MappedOutcome::Unsupported(reason);
        }
    }
    MappedOutcome::Mapped(items)
}

/// 该 stat 是否携带 **curse buff 载荷**（任一元素 [`is_curse_effect`] 命中）。
///
/// 供编排层镜像 vendor 的 curse 注册前置条件：vendor 只把带 `GlobalEffect` tag 的
/// mod 搬入 `activeSkill.buffList`（`CalcActiveSkill.lua:976-1041`），curse 表项
/// 仅从 buffList 构造（`CalcPerform.lua:2286-2316`）——statMap 数据上**没有任何**
/// `GlobalEffect effectType=Curse` 条目的 curse 技能（如 Repulsion
/// `CurseOfRepulsionPlayer`，per-set statMap 全空）buffList 恒空，不注册 curse、
/// 不占 curse 槽、不计入 `Multiplier:CurseOnEnemy`（:2969 `#curseSlots`）。
/// 反例 Freezing Mark：vendor 数据特意给了 `Dummy INC`（GlobalEffect Curse）占位
/// 载荷使其入槽（`act_int.lua:8645`）。
///
/// 与 [`map_curse_stat`] 的差异：只判**存在性**，不要求可翻译——允收名单外的
/// 载荷（Temporal Chains `TemporalChainsActionSpeed` / `Dummy`）仍算 curse 载荷
/// （vendor 同样入槽计数）。`unextractable` 条目 mods 为空 → 按无载荷处理
/// （抽取器对 curse statMap 的 `mod()` 构造均可抽取，当前数据无此形态）。
pub fn has_curse_payload(
    catalog: &StatMapCatalog,
    effect_id: &str,
    set_key: Option<&str>,
    stat: &str,
) -> bool {
    catalog
        .lookup(effect_id, set_key, stat)
        .is_some_and(|entry| entry.mods.iter().any(is_curse_effect))
}

/// （存量 #7-1）curse 技能的**技能局部效果乘区**取数：stat 若映射为
/// **无 GlobalEffect tag** 的 `CurseEffect` INC/MORE（留在 skillModList 的
/// 技能局部 mod——vendor curse 乘区 CalcPerform.lua:2423/:2427 读
/// `skillModList:Sum/More(skillCfg, "CurseEffect")`），返回其
/// `(inc 增量, more 因子)`。典型来源 = curse 宝石自身品质 `curse_effect_+%`
/// （EW 0.5/q）、组内 Heightened Curse support（constantStats +25）、
/// Atziri's Allure lineage（`support_atziri_curse_effect_+%_final` MORE -20）。
///
/// 保守口径：仅 kind=="mod"、无 tag、无 flag 的裸 `CurseEffect` 计入
/// （Mark 门控变体带 SkillType tag，不计——mark 域另行建模时再放开）；
/// 其余 stat / 形态返回 `(0.0, 1.0)`（零贡献，不误算）。
pub fn curse_local_effect(
    catalog: &StatMapCatalog,
    effect_id: &str,
    set_key: Option<&str>,
    stat: &str,
    stat_value: f64,
) -> (f64, f64) {
    let (mut inc, mut more) = (0.0, 1.0);
    let Some(entry) = catalog.lookup(effect_id, set_key, stat) else {
        return (inc, more);
    };
    if entry.unextractable {
        return (inc, more);
    }
    let params = MergeParams {
        div: entry.div,
        mult: entry.mult,
        base: entry.base,
        value: entry.value,
    };
    for element in &entry.mods {
        if element.kind != "mod"
            || element.name.as_deref() != Some("CurseEffect")
            || !element.tags.is_empty()
            || !element.flags.is_empty()
            || !element.keyword_flags.is_empty()
            || element.scalar.is_some()
        {
            continue;
        }
        let merged = params.merge(stat_value);
        match element.mod_type.as_deref() {
            Some("INC") => inc += merged,
            Some("MORE") => more *= 1.0 + merged / 100.0,
            _ => {}
        }
    }
    (inc, more)
}

/// 元素是否为**曝光施加能力**载荷（M4-m，宿主探测用、非取数）：vendor
/// `flag("InflictExposure", …)`（SkillStatMap.lua:1692-1715 各 on_shock /
/// on_cold_crit / on_ignite / on_hit 形）或 `<El>ExposureChance BASE`
/// （:1689-1690 / :1704-1707）。对应 CalcPerform.lua:3196-3200
/// `getSkillExposureEffect` 的 Config 曝光源宿主判据 `HasMod("BASE", cfg,
/// el.."ExposureChance") or HasMod("FLAG", "InflictExposure")`。PoBR 近似
/// 忽略 flag 上的门控 tag（on-Ignited 等条件——vendor `HasMod` 带 cfg 同样
/// 宽松存在性判定，条件不参与）。
fn is_exposure_inflict(element: &StatMapMod) -> bool {
    if element.kind == "group" {
        return element.mods.iter().any(is_exposure_inflict);
    }
    element
        .name
        .as_deref()
        .is_some_and(|n| n == "InflictExposure" || n.ends_with("ExposureChance"))
}

/// 一条 stat 是否含曝光施加能力载荷（[`is_exposure_inflict`]；与
/// [`has_curse_payload`] 同口径的**存在性**判定，不要求允收可翻译）。供编排层
/// 曝光宿主探测：宿主曝光能力来自 support（Fire Exposure
/// `inflict_exposure_for_x_ms_on_ignite`）时，组内曝光效果 support
/// （Potent Exposure）的 `<El>ExposureEffect` 才全局注入。
pub fn has_exposure_inflict_payload(
    catalog: &StatMapCatalog,
    effect_id: &str,
    set_key: Option<&str>,
    stat: &str,
) -> bool {
    catalog
        .lookup(effect_id, set_key, stat)
        .is_some_and(|entry| entry.mods.iter().any(is_exposure_inflict))
}

/// 敌侧 ModName 翻译表（curse 域，PoB2 enemyDB 名 → PoBR enemy db 聚合名）。
///
/// 允收名单 = 当前 pobr 敌侧消费方逐一对照（宁可跳过不可错算）：
/// - `<Type>Resist` BASE：`offence::enemy_damage_multiplier` 抗性减伤段
///   （Despair `ChaosResist`）；`ElementalResist`（Elemental Weakness）展开
///   火/冰/电三条（消费侧只读 `<Type>Resist`，与 vendor 双名同收等值）。
/// - `Damage` INC/MORE：敌方出伤乘区（Enfeeble），`ehp::assemble_enemy_damage`
///   （CalcDefence.lua:2133 enemyDamageMult）消费。
/// - `SelfCritMultiplier` BASE：敌方受暴击加成（Sniper's Mark），
///   `crit.rs` 敌侧段消费（CalcOffence.lua:3814-3825）。
/// - `BuffExpireFaster` MORE：敌方身上效果到期速率（Temporal Chains
///   `base_temporal_chains_other_buff_time_passed_+%_to_apply` → MORE 负值 =
///   expire slower），`ailment::debuff_duration_mult` 消费
///   （CalcOffence.lua:1833-1835 `debuffDurationMult = 1 / max(
///   BuffExpirationSlowCap, calcLib.mod(enemyDB, cfg, "BuffExpireFaster"))`，
///   :5040 折入异常持续）。
///
/// 其余（`TemporalChainsActionSpeed` / `FreezeBuildup` /
/// `ElectrocuteBuildup` / `IgnoreArmour` / `Dummy`…）pobr 暂无敌侧消费方 →
/// `UnknownModName` 上报，消费方落地后补名单。
fn translate_curse_mod_name(name: &str) -> Result<Vec<&'static str>, UnsupportedReason> {
    match name {
        "FireResist" => Ok(vec!["FireResist"]),
        "ColdResist" => Ok(vec!["ColdResist"]),
        "LightningResist" => Ok(vec!["LightningResist"]),
        "ChaosResist" => Ok(vec!["ChaosResist"]),
        "ElementalResist" => Ok(vec!["FireResist", "ColdResist", "LightningResist"]),
        "Damage" => Ok(vec!["Damage"]),
        "SelfCritMultiplier" => Ok(vec!["SelfCritMultiplier"]),
        "BuffExpireFaster" => Ok(vec!["BuffExpireFaster"]),
        other => Err(UnsupportedReason::UnknownModName(other.to_string())),
    }
}

/// curse 元素翻译（group 递归 + mod 构造器；flag/skill_data 带 curse tag 在
/// 数据中不存在，出现即 Unsupported——非 mod 载荷的 curse 语义未建模）。
fn collect_curse_element(
    element: &StatMapMod,
    params: &MergeParams,
    stat_value: f64,
    items: &mut Vec<MappedItem>,
) -> Result<(), UnsupportedReason> {
    if element.scalar.is_some() {
        return Err(UnsupportedReason::ScalarMultiplier);
    }
    match element.kind.as_str() {
        "group" => {
            let group_params = MergeParams {
                div: element.div,
                mult: element.mult,
                base: element.base,
                value: match &element.value {
                    Some(StatMapValue::Number(v)) => Some(*v),
                    Some(_) => {
                        return Err(UnsupportedReason::UnsupportedKind(
                            "group 非数值 value".to_string(),
                        ));
                    }
                    None => None,
                },
            };
            for nested in element.mods.iter().filter(|e| is_curse_effect(e)) {
                collect_curse_element(nested, &group_params, stat_value, items)?;
            }
            Ok(())
        }
        "mod" => collect_curse_mod(element, params.merge(stat_value), items),
        other => Err(UnsupportedReason::UnsupportedKind(format!(
            "curse 非 mod 载荷：{other}"
        ))),
    }
}

/// curse `mod()` 构造器翻译：敌侧名单名 + GlobalEffect 剥除 + 其余 tag 直译。
fn collect_curse_mod(
    element: &StatMapMod,
    merged_value: f64,
    items: &mut Vec<MappedItem>,
) -> Result<(), UnsupportedReason> {
    let Some(name) = element.name.as_deref() else {
        return Err(UnsupportedReason::UnknownModName("<missing name>".into()));
    };
    let Some(mod_type) = element.mod_type.as_deref() else {
        return Err(UnsupportedReason::MissingModType);
    };
    let mod_type = match mod_type {
        "BASE" => ModType::Base,
        "INC" => ModType::Inc,
        "MORE" => ModType::More,
        "FLAG" => ModType::Flag,
        "OVERRIDE" => ModType::Override,
        other => return Err(UnsupportedReason::UnsupportedModType(other.to_string())),
    };
    // 第一批：curse 载荷无 flag / keyword_flag（敌侧 cfg 不派生作用域位，附上
    // 即静默欠算）；非空整条上报。
    if !element.flags.is_empty() {
        return Err(UnsupportedReason::UnsupportedFlags(element.flags.join("|")));
    }
    if !element.keyword_flags.is_empty() {
        return Err(UnsupportedReason::UnsupportedKeywordFlags(
            element.keyword_flags.join("|"),
        ));
    }
    // tag：GlobalEffect 剥除（约定外键 = 额外门控语义，整条上报）；其余直译。
    let mut tags = Vec::new();
    for tag in &element.tags {
        let is_global =
            matches!(tag.get("type"), Some(StatMapValue::Text(t)) if t == "GlobalEffect");
        if is_global {
            if !tag
                .keys()
                .all(|k| matches!(k.as_str(), "type" | "effectType"))
            {
                return Err(UnsupportedReason::UnsupportedTag(format!(
                    "GlobalEffect 含约定外键：{:?}",
                    tag.keys().collect::<Vec<_>>()
                )));
            }
            continue;
        }
        tags.push(translate_tag(tag)?);
    }
    for translated in translate_curse_mod_name(name)? {
        let mut modifier = if mod_type == ModType::Flag {
            Modifier::flag(translated)
        } else {
            Modifier::number(translated, mod_type, merged_value)
        };
        for tag in &tags {
            modifier = modifier.with_tag(tag.clone());
        }
        items.push(MappedItem::Modifier(Box::new(modifier)));
    }
    Ok(())
}

// ---- M4-G：player buff 域（GlobalEffect effectType=Buff/Aura）玩家侧映射 ----

/// 元素是否为**玩家侧 buff 载荷**（vendor `GlobalEffect` tag 且
/// `effectType ∈ {Buff, Aura}`——`CalcActiveSkill.lua:976-1041` 把命中元素搬入
/// `buff.modList`，`CalcPerform.lua:1949-1962`（Buff）/ :2086-2120（Aura）经
/// BuffEffect/AuraEffect 乘区 `ScaleAddList` 后写玩家 modDB）。
/// group 任一成员命中即整组（与 [`is_curse_effect`] 同口径的保守泛化）。
fn is_player_buff_effect(element: &StatMapMod) -> bool {
    if element.kind == "group" {
        return element.mods.iter().any(is_player_buff_effect);
    }
    element.tags.iter().any(|tag| {
        matches!(tag.get("type"), Some(StatMapValue::Text(t)) if t == "GlobalEffect")
            && matches!(tag.get("effectType"), Some(StatMapValue::Text(t)) if t == "Buff" || t == "Aura")
    })
}

/// 把一条 buff 授予技能（或其 support）的 stat 经 statmap 数据翻译为**玩家侧**
/// PoBR 注入项（BuffSpec.mods 取数通道，buff_pass Buff/Aura 路径消费）。
///
/// 与 [`map_curse_stat`] 同构（curse 域先例，蓝图 m3-orchestration.md §6.3）：
/// - 只保留 [`is_player_buff_effect`] 命中的元素（非 buff 元素 = 技能局部 mod，
///   走主技能注入通道，此处**静默跳过**）。过滤后无 buff 元素 → `Mapped(空)`。
/// - ModName 走玩家侧允收名单 [`translate_player_buff_mod_name`]——第一批仅
///   `Accuracy`（Precision I/II support `sup_dex.lua:4181-4250` / War Banner
///   `base_skill_buff_banner_accuracy_+%_to_apply`，进 offence 精准聚合
///   CalcOffence.lua:2555-2572）。与既有 `map_aura_buff_stat` 静态映射的
///   防御名单（ES/抗性族）**不重叠**，避免 aura 路径双注入。
/// - `GlobalEffect` tag 剥除；除 curse 域约定键外额外允许 `effectName`
///   （buff 显示名，vendor 仅用于 AffectedBy 条件命名，无门控语义）。
/// - flags / keyword_flags 第一批要求为空（允收名单内载荷数据全部无 flag）。
///
/// **逐元素独立处置**（M4-n，与 map_curse_stat 的「整条跳过」口径不同）：
/// vendor merge 循环对条目的每个 modOrGroup **独立**翻译入 modList
/// （CalcActiveSkill.lua:96-117 逐元素 `mergeStat`，元素间无成组耦合语义），
/// 故单一元素不可翻译只跳过该元素、不连坐兄弟元素——典型场景 = Pinnacle of
/// Power（other.lua:12503）`elemental_power_elemental_damage_+%_final_per_
/// power_charge` 条目：首元素 `Damage MORE` 带 scalar Multiplier
/// （ScalarMultiplier 边界外）不应丢弃同条目六枚 `<El>Can<Ailment>` flag 载荷。
/// 可见性：全部命中元素失败（零注入）时仍按首个失败原因 Unsupported 上报；
/// 部分成功 → `Mapped(成功子集)`（失败元素维持零注入，宁可跳过不可错算）。
pub fn map_player_buff_stat(
    catalog: &StatMapCatalog,
    effect_id: &str,
    set_key: Option<&str>,
    stat: &str,
    stat_value: f64,
) -> MappedOutcome {
    let Some(entry) = catalog.lookup(effect_id, set_key, stat) else {
        return MappedOutcome::Unknown;
    };
    if entry.unextractable {
        return MappedOutcome::Unsupported(UnsupportedReason::Unextractable);
    }
    let entry_params = MergeParams {
        div: entry.div,
        mult: entry.mult,
        base: entry.base,
        value: entry.value,
    };
    let mut items = Vec::new();
    let mut first_failure: Option<UnsupportedReason> = None;
    for element in entry.mods.iter().filter(|e| is_player_buff_effect(e)) {
        // 逐元素独立处置（见函数文档）：失败元素零注入、不连坐兄弟元素。
        // 元素级 scratch vec 防半元素注入（group 内部分成员已 push 后失败）。
        let mut element_items = Vec::new();
        match collect_player_buff_element(element, &entry_params, stat_value, &mut element_items) {
            Ok(()) => items.append(&mut element_items),
            Err(reason) => {
                first_failure.get_or_insert(reason);
            }
        }
    }
    if items.is_empty()
        && let Some(reason) = first_failure
    {
        // 全部命中元素失败 → 维持 Unsupported 上报（双跑/Compare 可见性不退化）。
        return MappedOutcome::Unsupported(reason);
    }
    MappedOutcome::Mapped(items)
}

/// 玩家侧 ModName 允收名单（buff 域）。逐族对照消费方后准入：
/// - `Accuracy` INC：`offence.rs` 精准段（CalcOffence.lua:2555-2572
///   `skillModList:Sum("INC", cfg, "Accuracy")`）——第一批（M4-G）。
/// - `ManaRegen` INC（M4-m，Clarity I/II，vendor sup_int.txt:305-315）：
///   消费方 = `calc::survivability::calc_regen`（vendor CalcDefence.lua:1642
///   `Sum("INC", nil, resource.."Regen", resource.."RecoveryRate")`）。
/// - `LifeRegenPercent` BASE（M4-m，Vitality I/II，vendor sup_str.txt:1791-1802
///   per-minute div 60）：消费方同上（CalcDefence.lua:1658 `pool ×
///   Sum("BASE", resource.."RegenPercent")/100`）。
///
/// **不准入**（M4-m 已盘点 18-build 语料、登记）：
/// - `base_skill_buff_*_to_apply` 防御族（Purity/Impurity/Discipline 的
///   FireResistance/ChaosResistance/EnergyShield…）——已由
///   `map_aura_buff_stat` 静态名单注入（aura 通道），此处准入即双注入。
/// - Mysticism `Damage INC + ModFlag.Spell + Condition:FullEnergyShield`
///   （sup_int.txt:1250-1251）——伤害向量段归 M4 伤害线，且 flags 非空在
///   本域约定内整条上报。
/// - 自护体异常时长族（Coolheaded/Warmblooded/StrongHearted
///   `*_duration_on_self_+%_final`）、flask 域（Herbalism/Alchemist's Boon）、
///   rage/incision 非 mod 载荷（kind=flag/scalar）——无消费方，维持
///   `UnknownModName`/`UnsupportedKind` 上报（宁可跳过不可错算）。
fn translate_player_buff_mod_name(name: &str) -> Result<Vec<&'static str>, UnsupportedReason> {
    match name {
        "Accuracy" => Ok(vec!["Accuracy"]),
        "ManaRegen" => Ok(vec!["ManaRegen"]),
        "LifeRegen" => Ok(vec!["LifeRegen"]),
        "LifeRegenPercent" => Ok(vec!["LifeRegenPercent"]),
        // 防御 buff 族（Gemling 升华 Virtuous Barrier 的 per-Mote INC：
        // `gem_barrier_green_grants_*` → Armour/Evasion/EnergyShield INC ×Mote、
        // `gem_barrier_red_grants_maximum_life_+%` → Life INC ×Mote）。消费方 =
        // `calc::defence`（Armour/Evasion/EnergyShield 聚合）+ life 池。Multiplier
        // tag（StrengthMoteSkillCount/DexterityMoteSkillCount）在编排层 provision。
        "Armour" => Ok(vec!["Armour"]),
        "Evasion" => Ok(vec!["Evasion"]),
        "EnergyShield" => Ok(vec!["EnergyShield"]),
        // PoBR 生命池聚合名是 `MaximumLife`（parser name_map 把「maximum Life」归一到此，
        // scaled_pool 也查此名）——vendor 名 `Life` 必须归一到 `MaximumLife`，否则 barrier
        // 的 per-Mote Life INC 落进死桶 `Life`、生命池读不到（Armour/Evasion/EnergyShield
        // 因规范名与 vendor 同名而无此问题）。gemling Virtuous Barrier 24% Life INC 根因。
        "Life" => Ok(vec!["MaximumLife"]),
        // （M4-n）伤害向量族（Sigil of Power `circle_of_power_spell_damage_+%
        // _final_per_stage` → Damage MORE Spell；Elemental Conflux
        // `skill_elemental_conflux_active_element_damage_+%_final` →
        // <El>Damage MORE）：消费方 = damage 分桶聚合（`calc::damage` 的
        // `Damage`/`<El>Damage` INC/MORE 查询，vendor CalcOffence 同名）。
        "Damage" => Ok(vec!["Damage"]),
        "FireDamage" => Ok(vec!["FireDamage"]),
        "ColdDamage" => Ok(vec!["ColdDamage"]),
        "LightningDamage" => Ok(vec!["LightningDamage"]),
        // Refraction I/II support（`sup_str.lua:5984/6023` Refractive Plating buff，
        // `support_tempered_valour_deflection_rating_%_of_evasion_rating` → BASE 20）：
        // 消费方 = `calc::defence_panels::calc_deflection`（CalcDefence.lua:1516
        // `Evasion × ΣBASE EvasionGainAsDeflection / 100`）。
        "EvasionGainAsDeflection" => Ok(vec!["EvasionGainAsDeflection"]),
        // 同 buff 的 `support_tempered_valour_%_armour_to_apply_to_elemental_damage`
        // → ArmourAppliesTo<El>DamageTaken BASE 30（Refraction II）。消费方 =
        // `calc::taken::armour_applies_pct`（vendor CalcDefence.lua:2361-2368
        // `percentOfArmourApplies` → `effectiveAppliedArmour`，进 per-type
        // DamageReduction / MaximumHit / EHP）。tag 形态与 deflection 载荷同
        // （GlobalEffect + MultiplierThreshold RefractionMinimumValour 静态折 0）。
        "ArmourAppliesToFireDamageTaken" => Ok(vec!["ArmourAppliesToFireDamageTaken"]),
        "ArmourAppliesToColdDamageTaken" => Ok(vec!["ArmourAppliesToColdDamageTaken"]),
        "ArmourAppliesToLightningDamageTaken" => Ok(vec!["ArmourAppliesToLightningDamageTaken"]),
        // Sigil of Power `circle_of_power_max_stages` → 玩家 `Multiplier:
        // SigilOfPowerMaxStages` BASE（vendor 消费点 = GetMultiplier 动态上限
        // ModStore.lua:369；PoBR 编排层把 buff 载荷中 `Multiplier:` BASE 桥进
        // cfg.multipliers，见 calc_orchestrator buff specs 注入点）。
        "Multiplier:SigilOfPowerMaxStages" => Ok(vec!["Multiplier:SigilOfPowerMaxStages"]),
        // （0.5.4b #5）Blazing Critical support（sup_int.lua:959）：0.22.0 给
        // `support_blazing_crits_gain_%_fire_damage_with_attacks_on_critical_hit`
        // 补上 GlobalEffect/Buff tag——15% `DamageGainAsFire` BASE（ModFlag.Attack
        // + Condition:CritRecently）从「只挂在被支援技能上的死词条」变成全局玩家
        // buff（"imbue all of your Attacks"）。消费方 = `calc::damage` gain-as
        // 矩阵（buildGainTable，`DamageGainAs<To>` BASE 查询）；点燃火源随之
        // 平方级放大（chance ∝ fire/threshold，magnitude ∝ fire）。
        "DamageGainAsFire" => Ok(vec!["DamageGainAsFire"]),
        // （存量 #7-1）Archmage（act_int.lua:229-231）：`archmage_all_damage_%_to_
        // gain_as_lightning_to_grant_to_non_channelling_spells_per_100_max_mana`
        // → `DamageGainAsLightning` BASE 4（GlobalEffect/Buff + SkillType
        // Channel neg + SkillType Spell + PerStat Mana div 100）。消费方与
        // DamageGainAsFire 同 = `calc::damage` gain-as 矩阵；Mana 分母由编排层
        // `inject_per_x_multipliers` 预灌（cfg.multipliers["Mana"] = 全管线池值）。
        // monk-invoker-frost-bomb TotalDPS 0.66x 根因（缺 80% 闪电 gain-as）。
        "DamageGainAsLightning" => Ok(vec!["DamageGainAsLightning"]),
        other => Err(UnsupportedReason::UnknownModName(other.to_string())),
    }
}

/// player buff 元素翻译（group 递归 + mod 构造器；与 curse 域同构）。
fn collect_player_buff_element(
    element: &StatMapMod,
    params: &MergeParams,
    stat_value: f64,
    items: &mut Vec<MappedItem>,
) -> Result<(), UnsupportedReason> {
    if element.scalar.is_some() {
        return Err(UnsupportedReason::ScalarMultiplier);
    }
    match element.kind.as_str() {
        "group" => {
            let group_params = MergeParams {
                div: element.div,
                mult: element.mult,
                base: element.base,
                value: match &element.value {
                    Some(StatMapValue::Number(v)) => Some(*v),
                    Some(_) => {
                        return Err(UnsupportedReason::UnsupportedKind(
                            "group 非数值 value".to_string(),
                        ));
                    }
                    None => None,
                },
            };
            for nested in element.mods.iter().filter(|e| is_player_buff_effect(e)) {
                collect_player_buff_element(nested, &group_params, stat_value, items)?;
            }
            Ok(())
        }
        "mod" => collect_player_buff_mod(element, params.merge(stat_value), items),
        "flag" => collect_player_buff_flag(element, items),
        other => Err(UnsupportedReason::UnsupportedKind(format!(
            "player buff 非 mod 载荷：{other}"
        ))),
    }
}

/// player buff `flag()` 构造器翻译（M4-n）：buff 域 flag 允收 = 跨类型施加
/// `<Type>Can<Ailment>` 族（[`is_cross_type_ailment_flag`]）。
///
/// 消费方：`calc::ailment::{cross_type_source_hit_at_roll, stored_source_at_roll}`
/// 的 `{type_prefix}Can{ailment}` 旗标门控（vendor CalcOffence.lua:4791-4825
/// `canDoAilment` + :5453-5456 `type.."Can"..ailment`）。典型来源 = Pinnacle of
/// Power（武器 Adonia's Ego 授予，other.lua:12503）六枚 `<El>Can<Ailment>`
/// FLAG（全带 GlobalEffect/Buff tag，vendor 经 buff 循环写全局）。
///
/// 名单外 flag 名维持未知名上报（与主通道 [`is_consumable_flag`] 同口径：
/// 错注会污染 ModDb flag 查询）；tag 处理与 [`collect_player_buff_mod`] 同款
/// （GlobalEffect 剥除 + 约定键校验 + 其余直译）。
fn collect_player_buff_flag(
    element: &StatMapMod,
    items: &mut Vec<MappedItem>,
) -> Result<(), UnsupportedReason> {
    let name = element.name.as_deref().unwrap_or("?");
    if !is_cross_type_ailment_flag(name) {
        return Err(UnsupportedReason::UnknownModName(format!("flag:{name}")));
    }
    if !element.flags.is_empty() {
        return Err(UnsupportedReason::UnsupportedFlags(element.flags.join("|")));
    }
    if !element.keyword_flags.is_empty() {
        return Err(UnsupportedReason::UnsupportedKeywordFlags(
            element.keyword_flags.join("|"),
        ));
    }
    let mut modifier = Modifier::flag(name);
    for tag in &element.tags {
        let is_global =
            matches!(tag.get("type"), Some(StatMapValue::Text(t)) if t == "GlobalEffect");
        if is_global {
            if !tag
                .keys()
                .all(|k| matches!(k.as_str(), "type" | "effectType" | "effectName"))
            {
                return Err(UnsupportedReason::UnsupportedTag(format!(
                    "GlobalEffect 含约定外键：{:?}",
                    tag.keys().collect::<Vec<_>>()
                )));
            }
            continue;
        }
        modifier = modifier.with_tag(translate_tag(tag)?);
    }
    items.push(MappedItem::Modifier(Box::new(modifier)));
    Ok(())
}

/// `<Type>Can<Ailment>` 跨类型施加旗标族判定：type ∈ 五伤害类型前缀
/// （`calc::ailment::type_prefix` 同表），ailment ∈ ModDb 消费方识别的七异常名
/// （`calc::ailment::ailment_mod_name` 同表）。消费点拼名格式 =
/// `format!("{prefix}Can{ailment}")`，本判定与之逐字对齐。
fn is_cross_type_ailment_flag(name: &str) -> bool {
    let Some(rest) = ["Physical", "Fire", "Cold", "Lightning", "Chaos"]
        .iter()
        .find_map(|p| name.strip_prefix(p))
    else {
        return false;
    };
    let Some(ailment) = rest.strip_prefix("Can") else {
        return false;
    };
    matches!(
        ailment,
        "Bleed" | "Ignite" | "Poison" | "Shock" | "Chill" | "Freeze" | "Electrocute"
    )
}

/// player buff `mod()` 构造器翻译：玩家侧名单名 + GlobalEffect 剥除
/// （额外允许 `effectName` 键）+ 其余 tag 直译。
fn collect_player_buff_mod(
    element: &StatMapMod,
    merged_value: f64,
    items: &mut Vec<MappedItem>,
) -> Result<(), UnsupportedReason> {
    let Some(name) = element.name.as_deref() else {
        return Err(UnsupportedReason::UnknownModName("<missing name>".into()));
    };
    let Some(mod_type) = element.mod_type.as_deref() else {
        return Err(UnsupportedReason::MissingModType);
    };
    let mod_type = match mod_type {
        "BASE" => ModType::Base,
        "INC" => ModType::Inc,
        "MORE" => ModType::More,
        "FLAG" => ModType::Flag,
        "OVERRIDE" => ModType::Override,
        other => return Err(UnsupportedReason::UnsupportedModType(other.to_string())),
    };
    // flags 走 ModFlag 子集直译（M4-n 放开：Sigil of Power 的 Damage MORE 带
    // `Spell` flag——vendor flags=ModFlag.Spell，匹配语义两边一致；子集外
    // token 仍整条上报）。`Hit` ModFlag 路由到 keyword HIT（见 translate_mod_flags）。
    let (flags, kw_from_flags) = translate_mod_flags(&element.flags)?;
    if !element.keyword_flags.is_empty() {
        return Err(UnsupportedReason::UnsupportedKeywordFlags(
            element.keyword_flags.join("|"),
        ));
    }
    // tag：GlobalEffect 剥除（额外门控键整条上报；`effectName` = buff 显示名，
    // 无门控语义、允许；`unscalable` = buff effect 乘区豁免标记——PoBR
    // buff_pass 缩放豁免维度未建模，乘区为 1 时无差异，允许通过并登记）；
    // 其余直译。
    let mut tags = Vec::new();
    for tag in &element.tags {
        let is_global =
            matches!(tag.get("type"), Some(StatMapValue::Text(t)) if t == "GlobalEffect");
        if is_global {
            if !tag.keys().all(|k| {
                matches!(
                    k.as_str(),
                    "type" | "effectType" | "effectName" | "unscalable"
                )
            }) {
                return Err(UnsupportedReason::UnsupportedTag(format!(
                    "GlobalEffect 含约定外键：{:?}",
                    tag.keys().collect::<Vec<_>>()
                )));
            }
            continue;
        }
        tags.push(translate_tag(tag)?);
    }
    for translated in translate_player_buff_mod_name(name)? {
        let mut modifier = if mod_type == ModType::Flag {
            Modifier::flag(translated)
        } else {
            Modifier::number(translated, mod_type, merged_value)
        };
        modifier.flags = flags;
        modifier.keyword_flags = modifier.keyword_flags | kw_from_flags;
        for tag in &tags {
            modifier = modifier.with_tag(tag.clone());
        }
        items.push(MappedItem::Modifier(Box::new(modifier)));
    }
    Ok(())
}

// ---- M4-L：debuff 域（GlobalEffect effectType=Debuff）敌侧映射 ----

/// 元素是否为**敌侧 debuff 载荷**（vendor `GlobalEffect` tag 且
/// `effectType == "Debuff"`——`CalcActiveSkill.lua:976-1041` 把命中元素搬入
/// `buff.modList`（buff.type = "Debuff"），`CalcPerform.lua:2219-2285` 经
/// DebuffEffect 乘区 `ScaleAddList` 后 mergeBuff 进 `debuffs` 表写 enemyDB）。
/// group 任一成员命中即整组（与 [`is_curse_effect`] 同口径的保守泛化）。
fn is_debuff_effect(element: &StatMapMod) -> bool {
    if element.kind == "group" {
        return element.mods.iter().any(is_debuff_effect);
    }
    element.tags.iter().any(|tag| {
        matches!(tag.get("type"), Some(StatMapValue::Text(t)) if t == "GlobalEffect")
            && matches!(tag.get("effectType"), Some(StatMapValue::Text(t)) if t == "Debuff")
    })
}

/// 把一条 debuff 技能 stat 经 statmap 数据翻译为**敌侧** PoBR 注入项
/// （BuffSpec.mods 取数通道，buff_pass Debuff 路径消费）。
///
/// 与 [`map_curse_stat`] 同构（curse 域先例）：
/// - 只保留 [`is_debuff_effect`] 命中的元素（非 debuff 元素 = 技能局部 mod，
///   走主技能注入通道，此处**静默跳过**）。过滤后无 debuff 元素 → `Mapped(空)`。
/// - ModName 走敌侧允收名单 [`translate_debuff_mod_name`]——第一批 = 元素曝光族
///   （Frost Bomb `active_skill_all_elemental_exposure_magnitude` →
///   `<El>Exposure BASE`，vendor SkillStatMap.lua:1721-1725；消费方 =
///   `calc::reduce_enemy_exposure` 曝光归约，CalcPerform.lua:3214-3247
///   "Apply exposures" 把 enemyDB `<El>Exposure` 最强一份折成
///   `<El>Resist BASE -magnitude`）。未知名整条 `UnknownModName` 上报
///   （宁可跳过不可错算）。
/// - `GlobalEffect` tag 剥除（curse 域同款约定键校验）；其余 tag 直译。
/// - flags / keyword_flags 第一批要求为空。
pub fn map_debuff_stat(
    catalog: &StatMapCatalog,
    effect_id: &str,
    set_key: Option<&str>,
    stat: &str,
    stat_value: f64,
) -> MappedOutcome {
    let Some(entry) = catalog.lookup(effect_id, set_key, stat) else {
        return MappedOutcome::Unknown;
    };
    if entry.unextractable {
        return MappedOutcome::Unsupported(UnsupportedReason::Unextractable);
    }
    let entry_params = MergeParams {
        div: entry.div,
        mult: entry.mult,
        base: entry.base,
        value: entry.value,
    };
    let mut items = Vec::new();
    for element in entry.mods.iter().filter(|e| is_debuff_effect(e)) {
        if let Err(reason) = collect_debuff_element(element, &entry_params, stat_value, &mut items)
        {
            // 与 map_curse_stat 同口径：任一 debuff 元素不可翻译 → 整条跳过（成组语义）。
            return MappedOutcome::Unsupported(reason);
        }
    }
    MappedOutcome::Mapped(items)
}

/// 敌侧 ModName 允收名单（debuff 域）。第一批 = 元素曝光（消费方
/// `calc::reduce_enemy_exposure` 读 enemy db `<El>Exposure` BASE）。
///
/// 其余 debuff 载荷名（`ColdDamageTaken`/`MovementSpeed`…）pobr 暂无敌侧
/// 消费方逐一对照 → `UnknownModName` 上报，消费方落地后补名单。
fn translate_debuff_mod_name(name: &str) -> Result<Vec<&'static str>, UnsupportedReason> {
    match name {
        "FireExposure" => Ok(vec!["FireExposure"]),
        "ColdExposure" => Ok(vec!["ColdExposure"]),
        "LightningExposure" => Ok(vec!["LightningExposure"]),
        other => Err(UnsupportedReason::UnknownModName(other.to_string())),
    }
}

/// debuff 元素翻译（group 递归 + mod 构造器；与 curse 域同构）。
fn collect_debuff_element(
    element: &StatMapMod,
    params: &MergeParams,
    stat_value: f64,
    items: &mut Vec<MappedItem>,
) -> Result<(), UnsupportedReason> {
    if element.scalar.is_some() {
        return Err(UnsupportedReason::ScalarMultiplier);
    }
    match element.kind.as_str() {
        "group" => {
            let group_params = MergeParams {
                div: element.div,
                mult: element.mult,
                base: element.base,
                value: match &element.value {
                    Some(StatMapValue::Number(v)) => Some(*v),
                    Some(_) => {
                        return Err(UnsupportedReason::UnsupportedKind(
                            "group 非数值 value".to_string(),
                        ));
                    }
                    None => None,
                },
            };
            for nested in element.mods.iter().filter(|e| is_debuff_effect(e)) {
                collect_debuff_element(nested, &group_params, stat_value, items)?;
            }
            Ok(())
        }
        "mod" => collect_debuff_mod(element, params.merge(stat_value), items),
        other => Err(UnsupportedReason::UnsupportedKind(format!(
            "debuff 非 mod 载荷：{other}"
        ))),
    }
}

/// debuff `mod()` 构造器翻译：敌侧名单名 + GlobalEffect 剥除 + 其余 tag 直译。
fn collect_debuff_mod(
    element: &StatMapMod,
    merged_value: f64,
    items: &mut Vec<MappedItem>,
) -> Result<(), UnsupportedReason> {
    let Some(name) = element.name.as_deref() else {
        return Err(UnsupportedReason::UnknownModName("<missing name>".into()));
    };
    let Some(mod_type) = element.mod_type.as_deref() else {
        return Err(UnsupportedReason::MissingModType);
    };
    let mod_type = match mod_type {
        "BASE" => ModType::Base,
        "INC" => ModType::Inc,
        "MORE" => ModType::More,
        "FLAG" => ModType::Flag,
        "OVERRIDE" => ModType::Override,
        other => return Err(UnsupportedReason::UnsupportedModType(other.to_string())),
    };
    // 第一批：debuff 允收载荷无 flag / keyword_flag；非空整条上报。
    if !element.flags.is_empty() {
        return Err(UnsupportedReason::UnsupportedFlags(element.flags.join("|")));
    }
    if !element.keyword_flags.is_empty() {
        return Err(UnsupportedReason::UnsupportedKeywordFlags(
            element.keyword_flags.join("|"),
        ));
    }
    // tag：GlobalEffect 剥除（约定外键 = 额外门控语义，整条上报）；其余直译。
    let mut tags = Vec::new();
    for tag in &element.tags {
        let is_global =
            matches!(tag.get("type"), Some(StatMapValue::Text(t)) if t == "GlobalEffect");
        if is_global {
            if !tag
                .keys()
                .all(|k| matches!(k.as_str(), "type" | "effectType"))
            {
                return Err(UnsupportedReason::UnsupportedTag(format!(
                    "GlobalEffect 含约定外键：{:?}",
                    tag.keys().collect::<Vec<_>>()
                )));
            }
            continue;
        }
        tags.push(translate_tag(tag)?);
    }
    for translated in translate_debuff_mod_name(name)? {
        let mut modifier = if mod_type == ModType::Flag {
            Modifier::flag(translated)
        } else {
            Modifier::number(translated, mod_type, merged_value)
        };
        for tag in &tags {
            modifier = modifier.with_tag(tag.clone());
        }
        items.push(MappedItem::Modifier(Box::new(modifier)));
    }
    Ok(())
}

/// entry / group 级 merge 参数（vendor `map.div/mult/base/value`）。
struct MergeParams {
    div: Option<f64>,
    mult: Option<f64>,
    base: Option<f64>,
    value: Option<f64>,
}

impl MergeParams {
    /// merge 公式本体（CalcActiveSkill.lua:112 逐字对齐；scalar M1 固定 1.0，
    /// 含 scalar 的条目在进入本公式前已整条 Unsupported）。
    fn merge(&self, stat_value: f64) -> f64 {
        match self.value {
            Some(v) => v,
            None => {
                stat_value * self.mult.unwrap_or(1.0) * SCALAR_FIXED / self.div.unwrap_or(1.0)
                    + self.base.unwrap_or(0.0)
            }
        }
    }
}

/// 翻译单个元素（mod / flag / skill_data / group）。group 递归展开，嵌套 mod 用
/// group 级参数（CalcActiveSkill.lua:117）。
fn collect_element(
    element: &StatMapMod,
    params: &MergeParams,
    stat_value: f64,
    items: &mut Vec<MappedItem>,
) -> Result<(), UnsupportedReason> {
    if element.scalar.is_some() {
        return Err(UnsupportedReason::ScalarMultiplier);
    }
    match element.kind.as_str() {
        "group" => {
            let group_params = MergeParams {
                div: element.div,
                mult: element.mult,
                base: element.base,
                value: None, // group 级 value 抽取层落在 StatMapMod.value（StatMapValue）——见下
            };
            // group 级字面 value（vendor `modOrGroup.value`）：数值才参与公式覆盖。
            let group_params = match &element.value {
                Some(StatMapValue::Number(v)) => MergeParams {
                    value: Some(*v),
                    ..group_params
                },
                Some(_) => {
                    return Err(UnsupportedReason::UnsupportedKind(
                        "group 非数值 value".to_string(),
                    ));
                }
                None => group_params,
            };
            for nested in &element.mods {
                collect_element(nested, &group_params, stat_value, items)?;
            }
            Ok(())
        }
        "mod" => collect_mod(element, params.merge(stat_value), items),
        "flag" => {
            // vendor flag(name) 多为技能行为开关（projectile / unarmedMelee…），
            // PoBR 无消费方 → 未知名上报；**白名单**翻译有 ModDb flag 消费方的
            // 名字（M4-H：crit/lucky 族，消费点 = calc::crit::resolve_crit 与
            // calc::damage::lucky_hit_chance），tag 照常翻译（如 Garukhan's
            // Resolve `attacks_roll_crits_twice` → flag("BifurcateCrit",
            // SkillType.Attack)，SkillStatMap.lua:1011-1013）。
            let name = element.name.as_deref().unwrap_or("?");
            if !is_consumable_flag(name) {
                return Err(UnsupportedReason::UnknownModName(format!("flag:{name}")));
            }
            let mut modifier = Modifier::flag(name);
            for tag in &element.tags {
                modifier = modifier.with_tag(translate_tag(tag)?);
            }
            items.push(MappedItem::Modifier(Box::new(modifier)));
            Ok(())
        }
        "skill_data" => collect_skill_data(element, params.merge(stat_value), items),
        other => Err(UnsupportedReason::UnsupportedKind(other.to_string())),
    }
}

/// statmap `flag()` 构造器名字白名单：PoBR ModDb 有 `flag()` 消费方的旗标
/// （crit/lucky 族——`calc::crit::resolve_crit` 步骤 4/5/6/爆伤 +
/// `calc::damage::lucky_hit_chance`；mod 转换族——
/// `calc::perform::apply_projectile_speed_to_damage`，CalcOffence.lua:840-845）。
/// 白名单外的 flag 名维持未知名上报（多为技能行为开关，错注会污染 ModDb
/// flag 查询）。
fn is_consumable_flag(name: &str) -> bool {
    matches!(
        name,
        "BifurcateCrit"
            | "CritChanceLucky"
            | "InevitableCriticalHits"
            | "NoCritMultiplier"
            | "LuckyHits"
            | "CritLucky"
            | "ElementalLuckHits"
            | "ProjectileSpeedAppliesToProjectileDamage"
            // M4-K：异常叠层开关（Escalating Poison `number_of_additional_poison_
            // stacks` 与 `PoisonStacks BASE` 成对注入，sup_dex.lua:2188-2191）。
            // 消费方 = `calc::perform::resolve_stack_config` 的 maxStacks flag 门
            // （vendor CalcOffence.lua:5022-5025）。
            | "PoisonCanStack"
            | "BleedCanStack"
            | "IgniteCanStack"
    )
}

/// 翻译 `mod()` 构造器：名字（含 flag 语义分派）→ PoBR ModName，tag → [`ModTag`]。
fn collect_mod(
    element: &StatMapMod,
    merged_value: f64,
    items: &mut Vec<MappedItem>,
) -> Result<(), UnsupportedReason> {
    let Some(name) = element.name.as_deref() else {
        return Err(UnsupportedReason::UnknownModName("<missing name>".into()));
    };
    let Some(mod_type) = element.mod_type.as_deref() else {
        // vendor 笔误条目（如 sup_str CorruptingCry 漏 type），抽取忠实保留 → 跳过。
        return Err(UnsupportedReason::MissingModType);
    };
    let mod_type = match mod_type {
        "BASE" => ModType::Base,
        "INC" => ModType::Inc,
        "MORE" => ModType::More,
        "FLAG" => ModType::Flag,
        "OVERRIDE" => ModType::Override,
        // M4-m：vendor "CHANCE" 桶的消费形态 = `Sum("CHANCE", cfg, name)` 后在
        // 消费点 clamp（CalcOffence.lua:4145 HitsInvertEleResChance）——求和语义
        // 与 BASE 一致，translate_mod_name 名单仍逐名把关（当前数据全集仅
        // `treat_enemy_resistances_as_negated_…` 一条，无 BASE/CHANCE 同名混桶）。
        "CHANCE" => ModType::Base,
        other => return Err(UnsupportedReason::UnsupportedModType(other.to_string())),
    };
    let translated = translate_mod_name(name, &element.flags, &element.keyword_flags)?;
    let mut modifier = if mod_type == ModType::Flag {
        // FLAG mod 的 merge 值仅 Lua 真值语义（任意 number 含 0 均 truthy）→ Bool(true)。
        Modifier::flag(translated.name)
    } else {
        Modifier::number(translated.name, mod_type, merged_value)
    };
    modifier = modifier
        .with_flags(translated.flags)
        .with_keyword_flags(translated.keyword_flags);
    for tag in &element.tags {
        modifier = modifier.with_tag(translate_tag(tag)?);
    }
    items.push(MappedItem::Modifier(Box::new(modifier)));
    Ok(())
}

/// 翻译 `skill()` 构造器：伤害基值键 → `<Type>DamageMin/Max` BASE modifier；
/// `duration` → [`MappedItem::SkillData`]；其余键第一批 Unsupported。
fn collect_skill_data(
    element: &StatMapMod,
    merged_value: f64,
    items: &mut Vec<MappedItem>,
) -> Result<(), UnsupportedReason> {
    // skill_data 的键在抽取层落 value 表 `{key, value}`。
    let key = match &element.value {
        Some(StatMapValue::Table(t)) => match t.get("key") {
            Some(StatMapValue::Text(k)) => k.as_str(),
            _ => {
                return Err(UnsupportedReason::UnsupportedKind(
                    "skill_data 缺 key".into(),
                ));
            }
        },
        _ => {
            return Err(UnsupportedReason::UnsupportedKind(
                "skill_data 缺 key".into(),
            ));
        }
    };
    // tag 翻译先行（带不支持 tag 的 skill_data 同样整条跳过）。
    let mut tags = Vec::new();
    for tag in &element.tags {
        tags.push(translate_tag(tag)?);
    }
    // 伤害基值键（vendor 把技能基础伤害写进 skillData；PoBR 无 skillData 表，
    // 经 modifier 管线消费 `<Type>DamageMin/Max` BASE，对齐 legacy
    // `map_base_damage` 口径）。
    if let Some(mod_name) = damage_bound_mod_name(key) {
        let mut modifier = Modifier::number(mod_name, ModType::Base, merged_value);
        for tag in tags {
            modifier = modifier.with_tag(tag);
        }
        items.push(MappedItem::Modifier(Box::new(modifier)));
        return Ok(());
    }
    // M4-T4 W-D1：技能 DoT 基值键（vendor `skill("<Type>Dot", …)`，源 stat
    // `base_<type>_damage_to_deal_per_minute`、entry 级 div=60 已换算每分 → 每秒）
    // → 同名 `<Type>Dot` BASE modifier（PoBR 无 skillData 表，经 modifier 管线
    // 消费，对齐伤害基值族 `<Type>DamageMin/Max` 的既有口径；消费方 =
    // `calc::skill_dot`，按 dotTypeCfg 聚合）。
    if let Some(mod_name) = dot_base_mod_name(key) {
        let mut modifier = Modifier::number(mod_name, ModType::Base, merged_value);
        for tag in tags {
            modifier = modifier.with_tag(tag);
        }
        items.push(MappedItem::Modifier(Box::new(modifier)));
        return Ok(());
    }
    // M4-T4 W-D1：dotIs* 布尔键（vendor `skill("dotIsSpell", true)` 类，源 stat
    // 如 `spell_damage_modifiers_apply_to_skill_dot`）→ `DotIs<X>` FLAG modifier
    // （`calc::skill_dot` 据此保留 dotCfg 对应位，否则剥除——CalcOffence.lua:5839-5856）。
    // 注：当前 `.dat` 入库不含 value-less 布尔 stat，本通道在现有数据下不触发；
    // statSet baseMods 直挂的 dotIs*（TornadoShot）走 catalog `DotFlags` →
    // 编排层注入同名 FLAG（同一消费口径）。
    if let Some(flag_name) = dot_is_flag_mod_name(key) {
        if !tags.is_empty() {
            return Err(UnsupportedReason::UnsupportedTag(
                "skill_data 带 tag".into(),
            ));
        }
        items.push(MappedItem::Modifier(Box::new(Modifier::flag(flag_name))));
        return Ok(());
    }
    // skill_data 白名单（出 [`MappedItem::SkillData`]，编排层按键消费）：
    // - duration（vendor `skill("duration", …)`，entry 级 div=1000 已在 merge
    //   公式换算 ms → s）；
    // - corpseExplosionLifeMultiplier（M4-G，vendor SkillStatMap.lua:309-316：
    //   `corpse_explosion_monster_life_%` div=100 / `_permillage_physical`
    //   div=1000 → 尸体生命倍率；消费方 = 编排层尸体爆炸基伤注入，
    //   CalcOffence.lua:2211-2217）。
    // 其余键统计上报。
    if matches!(key, "duration" | "corpseExplosionLifeMultiplier") {
        if !tags.is_empty() {
            return Err(UnsupportedReason::UnsupportedTag(
                "skill_data 带 tag".into(),
            ));
        }
        items.push(MappedItem::SkillData {
            key: key.to_string(),
            value: merged_value,
        });
        return Ok(());
    }
    Err(UnsupportedReason::UnsupportedSkillDataKey(key.to_string()))
}

/// `<Type>Dot` skill_data 键 → 同名 BASE ModName（M4-T4 W-D1；vendor SkillStatMap
/// `base_<type>_damage_to_deal_per_minute` 条目，五伤害类型全集）。
fn dot_base_mod_name(key: &str) -> Option<String> {
    DAMAGE_TYPES.iter().find_map(|ty| {
        (key == format!("{ty}Dot")).then(|| key.to_string()) // ModName 与键同名
    })
}

/// `dotIs*` skill_data 键 → `DotIs*` FLAG ModName（M4-T4 W-D1；与 catalog
/// `DotFlags`（statSet baseMods 直挂布尔）的编排层注入共用同一组 FLAG 名）。
fn dot_is_flag_mod_name(key: &str) -> Option<&'static str> {
    Some(match key {
        "dotIsArea" => "DotIsArea",
        "dotIsProjectile" => "DotIsProjectile",
        "dotIsSpell" => "DotIsSpell",
        "dotIsAttack" => "DotIsAttack",
        "dotIsHit" => "DotIsHit",
        _ => return None,
    })
}

/// 五伤害类型（PoB2 命名 → PoBR PascalCase 相同）。
const DAMAGE_TYPES: [&str; 5] = ["Physical", "Fire", "Cold", "Lightning", "Chaos"];

/// `<Type>Min` / `<Type>Max` → `<Type>DamageMin/Max`（mod 与 skill_data 共用：
/// vendor 两条通道都用这组键名承载伤害基值）。
pub(crate) fn damage_bound_mod_name(name: &str) -> Option<String> {
    for ty in DAMAGE_TYPES {
        if let Some(bound) = name.strip_prefix(ty)
            && (bound == "Min" || bound == "Max")
        {
            return Some(format!("{ty}Damage{bound}"));
        }
    }
    None
}

/// 名字翻译产物：PoBR ModName + 直译后的 ModFlags / KeywordFlags。
///
/// pub 供双跑 oracle 对拍（vendor `mergeSkillInstanceMods` 注入的 modList 用
/// PoB2 名，对拍前经本翻译层归一，见 `pobr-build/tests/statmap_dual_run.rs`）。
#[derive(Debug, Clone, PartialEq)]
pub struct TranslatedName {
    /// PoBR ModName。
    pub name: String,
    /// 直译后的作用域 flags（PoB2 `band(cfg.flags, mod.flags) == mod.flags` 与
    /// PoBR [`ModFlags::is_subset_of`] 同语义，逐位对齐）。
    pub flags: ModFlags,
    /// 直译后的 keyword flags（PoB2 `MatchKeywordFlags` ANY 语义同
    /// [`KeywordFlags::matches_context`]）。
    pub keyword_flags: KeywordFlags,
}

impl TranslatedName {
    fn bare(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            flags: ModFlags::NONE,
            keyword_flags: KeywordFlags::NONE,
        }
    }
}

/// ModFlag token 直译（PoBR [`ModFlags`] 已移植子集；匹配语义两边一致——
/// PoB2 `ModList.lua` 子集判定 = PoBR `Modifier::matches` 的 `is_subset_of`）。
/// 子集外 token（Thorns/Weapon/武器类…）→ 条目 Unsupported（PoBR cfg
/// 不会置这些位，附上会让 mod 永不匹配 = 静默欠算，宁可整条上报）。
///
/// 返回 `(mod_flags, keyword_flags)`：`Hit` 是 vendor `ModFlag.Hit`，但 PoBR 的
/// hit-scoping **走 KeywordFlag.Hit 通道**（`offence` 击中 cfg `| KeywordFlags::HIT`，
/// `skill_dot`/`ailment` 剥 `KeywordFlags::HIT`）——cfg.flags 从不置 `ModFlags::HIT`。
/// 故把 `Hit` ModFlag 路由到 keyword HIT：匹配语义与 vendor 逐位一致（命中适用、
/// 异常/DoT 不适用），与 [`translate_keyword_flags`] 对 `KeywordFlag.Hit` 的处理同口径。
fn translate_mod_flags(tokens: &[String]) -> Result<(ModFlags, KeywordFlags), UnsupportedReason> {
    let mut flags = ModFlags::NONE;
    let mut keyword_flags = KeywordFlags::NONE;
    for token in tokens {
        match token.as_str() {
            "Attack" => flags |= ModFlags::ATTACK,
            "Spell" => flags |= ModFlags::SPELL,
            "Melee" => flags |= ModFlags::MELEE,
            "Projectile" => flags |= ModFlags::PROJECTILE,
            "Area" => flags |= ModFlags::AREA,
            // M4-T4 W-D1：Dot 位——dotCfg（`calc::skill_dot`）置 DOT 位后该类
            // mod（如 `support_rapid_decay_damage_over_time_+%_final` →
            // Damage MORE Dot）只命中 DoT 聚合（M4-i1 切换 PoB2 全位表后常驻）。
            "Dot" => flags |= ModFlags::DOT,
            // vendor `ModFlag.Hit` → PoBR keyword HIT 通道（见函数文档）。
            "Hit" => keyword_flags = keyword_flags | KeywordFlags::HIT,
            _ => return Err(UnsupportedReason::UnsupportedFlags(tokens.join("|"))),
        }
    }
    Ok((flags, keyword_flags))
}

/// 名字本身已承载作用域、且 PoBR 当前无消费方的惰性名：其 KeywordFlag 仅是
/// 冗余作用域限定（如 vendor `SkillStatMap.lua:556` `mod("WarcrySpeed","INC",nil,
/// 0,KeywordFlag.Warcry)`——WarcrySpeed 名字即 Warcry 作用域），安全丢弃：
/// 该名无任何 PoBR 查询方，丢弃不会改变任何现有计算。
const SCOPE_NAMED_INERT: [&str; 2] = ["WarcrySpeed", "TotemPlacementSpeed"];

/// KeywordFlag token 直译（位值两边均对齐 PoB2 `Global.lua:263-292`）。
///
/// 返回 `(keyword_flags, extra_mod_flags)`：`KeywordFlag.Attack` / `KeywordFlag.Spell`
/// 折算为 [`ModFlags::ATTACK`] / [`ModFlags::SPELL`]——PoB2 对这两个 keyword 的
/// ANY 匹配（`MatchKeywordFlags`，cfg.keywordFlags 由技能类型派生含 Attack/Spell）
/// 与 PoBR cfg.flags 的 ATTACK/SPELL 子集匹配门控等价（cfg.flags 同样由
/// skill_types 派生，`calc_orchestrator.rs:1125-1129`）。例
/// `support_attack_skills_elemental_damage_+%_final`（vendor `sup_str.lua:2825-2827`
/// `mod("ElementalDamage","MORE",nil,0,KeywordFlag.Attack)`）。
fn translate_keyword_flags(
    tokens: &[String],
    name: &str,
) -> Result<(KeywordFlags, ModFlags), UnsupportedReason> {
    let mut flags = KeywordFlags::NONE;
    let mut extra_mod_flags = ModFlags::NONE;
    for token in tokens {
        // 惰性作用域名上的冗余 keyword（见 [`SCOPE_NAMED_INERT`]）→ 丢弃。
        if matches!(token.as_str(), "Warcry" | "Totem") && SCOPE_NAMED_INERT.contains(&name) {
            continue;
        }
        // Attack/Spell keyword → 等价 ModFlags 门控（见函数文档）。
        if token == "Attack" {
            extra_mod_flags |= ModFlags::ATTACK;
            continue;
        }
        if token == "Spell" {
            extra_mod_flags |= ModFlags::SPELL;
            continue;
        }
        let bit = match token.as_str() {
            "Aura" => KeywordFlags::AURA,
            "Curse" => KeywordFlags::CURSE,
            "Hit" => KeywordFlags::HIT,
            "Ailment" => KeywordFlags::AILMENT,
            "Poison" => KeywordFlags::POISON,
            "Bleed" => KeywordFlags::BLEED,
            "Ignite" => KeywordFlags::IGNITE,
            "PhysicalDot" => KeywordFlags::PHYSICAL_DOT,
            "LightningDot" => KeywordFlags::LIGHTNING_DOT,
            "ColdDot" => KeywordFlags::COLD_DOT,
            "FireDot" => KeywordFlags::FIRE_DOT,
            "ChaosDot" => KeywordFlags::CHAOS_DOT,
            _ => {
                return Err(UnsupportedReason::UnsupportedKeywordFlags(tokens.join("|")));
            }
        };
        flags = flags | bit;
    }
    Ok((flags, extra_mod_flags))
}

/// ModName 翻译层（PoB2 名 + ModFlag/KeywordFlag 组合 → PoBR 名 + flags）。
///
/// 框架语义 L4（蓝图 §6 Q2 裁决：名字随机制不随版本变，留 Rust 常量表）。
/// 初版覆盖 = legacy `pobr-build::skill_stat_map` 既有映射族的反推，双跑 diff
/// 驱动补全（M1-T2b）；未知名归 [`UnsupportedReason::UnknownModName`] 上报。
///
/// flag 处理分两层：
/// - **名字分派**（PoBR 用独立 ModName 表达的速度/伤害桶）：`Speed` + Attack →
///   `AttackSpeed`；+ Cast → `CastSpeed`；裸 → `SkillSpeed`。`Damage` 的
///   Attack/Spell/Area 单 flag 同 legacy 口径分派（PoBR `calc::damage` 按 cfg
///   flag 加读 `AttackDamage`/`AreaDamage` 桶，两种表达等价）；
/// - **flags 直译**（其余组合）：经 [`translate_mod_flags`] 附在 Modifier 上，
///   匹配语义与 PoB2 子集判定逐位一致（如 `support_melee_physical_damage_+%_final`
///   per-set 条目 `mod("PhysicalDamage","MORE",nil,ModFlag.Melee)` →
///   `PhysicalDamage` MORE + `MELEE`，cfg 取 skill_types 派生 flags）。
///
/// 伤害基值族（`<Type>Min/Max`）维持丢弃 Attack/Spell flag 与 KeywordFlag
/// （legacy 同口径：单主技能全局注入，作用域限定无差异）。
pub fn translate_mod_name(
    name: &str,
    flags: &[String],
    keyword_flags: &[String],
) -> Result<TranslatedName, UnsupportedReason> {
    // 伤害基值族：允许丢弃 Attack/Spell flag 与 KeywordFlag（作用域限定在单主
    // 技能口径下无差异；legacy 同样 flag-blind 注入）。
    if let Some(bound_name) = damage_bound_mod_name(name) {
        let droppable = |f: &String| f == "Attack" || f == "Spell";
        if !flags.iter().all(droppable) {
            return Err(UnsupportedReason::UnsupportedFlags(flags.join("|")));
        }
        if !keyword_flags.iter().all(|f| f == "Attack" || f == "Spell") {
            return Err(UnsupportedReason::UnsupportedKeywordFlags(
                keyword_flags.join("|"),
            ));
        }
        return Ok(TranslatedName::bare(bound_name));
    }
    // 名字分派族：分派吃掉的 flag 从直译集合移除，剩余 flag 照常直译。
    let (base_name, remaining_flags): (&str, Vec<String>) = match name {
        "Speed" => {
            if let Some(pos) = flags.iter().position(|f| f == "Attack") {
                let mut rest = flags.to_vec();
                rest.remove(pos);
                ("AttackSpeed", rest)
            } else if let Some(pos) = flags.iter().position(|f| f == "Cast") {
                let mut rest = flags.to_vec();
                rest.remove(pos);
                ("CastSpeed", rest)
            } else {
                ("SkillSpeed", flags.to_vec())
            }
        }
        "Damage" => match flags {
            [f] if f == "Attack" => ("AttackDamage", Vec::new()),
            [f] if f == "Spell" => ("Damage", Vec::new()),
            [f] if f == "Area" => ("AreaDamage", Vec::new()),
            _ => ("Damage", flags.to_vec()),
        },
        _ => (name, flags.to_vec()),
    };
    // 名字翻译（分派后的 base_name）。
    let translated: String = match base_name {
        "CritChance" => "CriticalStrikeChance".to_string(),
        "CritMultiplier" => "CriticalStrikeMultiplier".to_string(),
        // 暴击上限（M4-H）：vendor 同名（如 Garukhan's Resolve
        // `maximum_critical_strike_chance_is_%` → CritChanceCap OVERRIDE 50），
        // 消费方 = `calc::crit::crit_chance_cap`。
        "CritChanceCap" => base_name.to_string(),
        // 同名直通族（PoBR calc 消费名或惰性作用域名）。
        "Damage"
        | "AttackDamage"
        | "AreaDamage"
        | "AttackSpeed"
        | "CastSpeed"
        | "SkillSpeed"
        | "PhysicalDamage"
        | "FireDamage"
        | "ColdDamage"
        | "LightningDamage"
        | "ChaosDamage"
        | "ElementalDamage"
        // 区间端 MORE 族（vendor `mod("Max<Type>Damage"/"Min<Type>Damage","MORE",…)`，
        // 如 Heft `support_heft_maximum_physical_damage_+%_final` →
        // `MaxPhysicalDamage` MORE，sup_str.lua:4222-4223）。消费方 =
        // `calc::damage::scale_with_path`（CalcOffence.lua:138-139,153-154 的
        // `Min/Max<Type>Damage` 独立 MORE 乘区，仅缩放区间一端）。
        | "MaxPhysicalDamage"
        | "MaxFireDamage"
        | "MaxColdDamage"
        | "MaxLightningDamage"
        | "MaxChaosDamage"
        | "MinPhysicalDamage"
        | "MinFireDamage"
        | "MinColdDamage"
        | "MinLightningDamage"
        | "MinChaosDamage"
        | "FirePenetration"
        | "ColdPenetration"
        | "LightningPenetration"
        | "ChaosPenetration"
        | "ElementalPenetration"
        | "TotalCastTime"
        | "TotalAttackTime"
        // vendor `SkillStatMap.lua:554-557`（skill_speed_+% 同条目三 mod）与
        // `:2400-2401`（summon_totem_cast_speed_+%）：警吼/图腾速度的独立名。
        // PoBR 当前无消费方（惰性注入，名字即作用域，不会错算），保留 PoB2 原名
        // 以免 legacy 把 TotemPlacementSpeed 误并入 CastSpeed（legacy 误映射，
        // 见 m1-statmap-switch-log.md）。
        | "WarcrySpeed"
        | "TotemPlacementSpeed"
        // M4-J：冷却恢复速率直通（vendor `base_cooldown_speed_+%`/quality 段/
        // `support_cooldown_reduction_cooldown_recovery_+%` → CooldownRecovery）。
        // 消费方 = `calc::skill_mechanics::calc_cooldown` /
        // `offence::apply_cooldown_cap`（冷却管辖速率整链）。
        | "CooldownRecovery"
        // M4-L：曝光效果直通（vendor `exposure_effect_+%` → 三元素
        // `<El>ExposureEffect` INC，SkillStatMap.lua:1731-1735，Potent Exposure
        // support 载荷）。消费方 = `calc::reduce_enemy_exposure`（CalcPerform.lua
        // :3223 曝光效果 INC；vendor 按技能作用域，PoBR 扁平 db 全局求和近似，
        // 见 reduce_enemy_exposure doc 的已登记 TODO(parity)）。
        | "FireExposureEffect"
        | "ColdExposureEffect"
        | "LightningExposureEffect" => base_name.to_string(),
        // M4-m：击中视敌元素抗性为反转（Rakiata's Flow
        // `treat_enemy_resistances_as_negated_on_elemental_damage_hit_%_chance`
        // → CHANCE，SkillStatMap.lua:941-944，entry div=100 → 分数）。消费方 =
        // `offence::enemy_damage_multiplier` 抗性段（CalcOffence.lua:4145-4148
        // `resist = resist - 2 * invertChance * resist`）。
        "HitsInvertEleResChance" => base_name.to_string(),
        // M4-G：grenade 二次起爆几率（vendor SkillStatMap.lua:2795-2797
        // `grenade_skill_%_chance_to_explode_twice` → GrenadeActivateTwice BASE，
        // 仅 SupportPayload 产出该 stat）。消费方 = `calc::scaled_damage::
        // dps_end_factors`（vendor CalcOffence.lua:1124-1127 折 DPS MORE）。
        "GrenadeActivateTwice" => base_name.to_string(),
        // M4-K：伤害异常族直通（消费方 = `calc::ailment` + `calc::perform::
        // fill_ailments`）。施加几率：流血/中毒内禀 `<Ailment>Chance`（vendor
        // `base_chance_to_inflict_bleeding_%`/`base_chance_to_poison_on_hit_%`，
        // SkillStatMap.lua:1267/:1311 族），点燃/感电几率派生 `Enemy<Ailment>Chance`
        // （`base_chance_to_ignite_%` 等）；量级：`AilmentMagnitude`（Deadly
        // Poison/Ignites 的 `*_effect_+%_final`，kw 限定经 keyword 直译）+
        // 感电/冰缓幅度（`EnemyShockMagnitude`/`EnemyChillMagnitude`，
        // `shock_traced`/`chill_traced` 消费）；叠层：`<Ailment>Stacks`
        // （Escalating Poison `number_of_additional_poison_stacks`，
        // `resolve_stack_config` 消费，与 `<Ailment>CanStack` flag 成对）。
        "PoisonChance" | "BleedChance" | "EnemyIgniteChance" | "EnemyShockChance"
        | "AilmentMagnitude" | "EnemyShockMagnitude" | "EnemyChillMagnitude" | "PoisonStacks"
        | "BleedStacks" | "IgniteStacks" => base_name.to_string(),
        // M4-m（k3 登记）：异常堆叠速率 rateMod 名直通（vendor `faster_burn_%`/
        // `faster_poison_%`/`faster_bleed_%`/`damaging_ailments_deal_damage_+%_faster`
        // → `<Ailment>Faster` INC，SkillStatMap.lua:843-848/:1255/:1479-1483）。
        // 消费方 = `calc::ailment::ailment_rate_mod`（CalcOffence.lua:5036 rateMod，
        // calcLib.mod 的 INC+MORE 两腿同名集）。
        "BleedFaster" | "PoisonFaster" | "IgniteFaster" => base_name.to_string(),
        // 存量 #9：warcry uptime 机器直通族（vendor `warcry_empowers_per_X_monster_power[_mp_cap]`
        // → WarcryPowerPer/Cap（SkillStatMap.lua:608-613）、Infernal Cry per-set
        // `infernal_cry_exerted_attack_all_damage_%_to_gain_as_fire_%` →
        // InfernalExtraFireDamageMultiplier（act_str.lua:7729-7731））。消费方 =
        // `calc::warcry`（賦能次数 CalcPerform.lua:2121-2123 + uptime 缩放的
        // DamageGainAsFire 注入 CalcOffence.lua:3251-3254）。
        "WarcryPowerPer" | "WarcryPowerCap" | "InfernalExtraFireDamageMultiplier" => {
            base_name.to_string()
        }
        // M4-K：异常持续时间——vendor 施加方词条名带 Enemy 前缀（作用于敌身上的
        // debuff 时长，CalcOffence.lua:5037 durationMod 取
        // `Enemy<Ailment>Duration`/`EnemyAilmentDuration`/`DamagingAilmentDuration`），
        // PoBR `ailment_duration`/`scale_duration` 的聚合名为 `<Ailment>Duration`/
        // `AilmentDuration`（mod_parser 同名族）→ 翻译归一。
        "EnemyPoisonDuration" => "PoisonDuration".to_string(),
        "EnemyBleedDuration" => "BleedDuration".to_string(),
        "EnemyIgniteDuration" => "IgniteDuration".to_string(),
        "EnemyAilmentDuration" | "DamagingAilmentDuration" => "AilmentDuration".to_string(),
        other => {
            // 转换 / gain-as 族（`Skill<From>DamageConvertTo<To>` /
            // `[Skill]<From>DamageGainAs<To>`）：PoB2 与 PoBR 命名一致，按形态直通。
            if is_conversion_mod_name(other) {
                other.to_string()
            } else {
                return Err(UnsupportedReason::UnknownModName(other.to_string()));
            }
        }
    };
    let (kw, extra_mod_flags) = translate_keyword_flags(keyword_flags, &translated)?;
    let (mod_flags, kw_from_flags) = translate_mod_flags(&remaining_flags)?;
    Ok(TranslatedName {
        flags: mod_flags | extra_mod_flags,
        keyword_flags: kw | kw_from_flags,
        name: translated,
    })
}

/// 转换 / gain-as ModName 形态校验：`[Skill]<From>DamageConvertTo<To>` /
/// `[Skill]<From>DamageGainAs<To>`，`<From>` 可为空（全伤害源）、`<To>` 必须是
/// 伤害类型。与 PoBR `calc::damage` 消费的名字逐字一致。
fn is_conversion_mod_name(name: &str) -> bool {
    let core = name.strip_prefix("Skill").unwrap_or(name);
    for marker in ["DamageConvertTo", "DamageGainAs"] {
        if let Some((from, to)) = core.split_once(marker) {
            let from_ok = from.is_empty() || DAMAGE_TYPES.contains(&from);
            let to_ok = DAMAGE_TYPES.contains(&to);
            return from_ok && to_ok;
        }
    }
    false
}

/// tag 翻译（第一批：Condition / ActorCondition(enemy) / Multiplier / PerStat →
/// PoBR [`ModTag`]）。
///
/// 其余 tag 类型整条 Unsupported；已支持类型出现**约定外的键**同样 Unsupported
/// （宁可跳过——多余键往往携带额外语义，静默丢键 = 错算）。
/// pub 供双跑 oracle 对拍（vendor modList 的 tag 经同一翻译归一后比对）。
pub fn translate_tag(tag: &BTreeMap<String, StatMapValue>) -> Result<ModTag, UnsupportedReason> {
    let tag_type = match tag.get("type") {
        Some(StatMapValue::Text(t)) => t.as_str(),
        _ => return Err(UnsupportedReason::UnsupportedTag("<missing type>".into())),
    };
    let text = |key: &str| match tag.get(key) {
        Some(StatMapValue::Text(v)) => Some(v.clone()),
        _ => None,
    };
    let number = |key: &str| match tag.get(key) {
        Some(StatMapValue::Number(v)) => Some(*v),
        _ => None,
    };
    let keys_subset_of = |allowed: &[&str]| tag.keys().all(|k| allowed.contains(&k.as_str()));
    match tag_type {
        "Condition" => {
            if !keys_subset_of(&["type", "var", "neg"]) {
                return Err(UnsupportedReason::UnsupportedTag(format!(
                    "Condition 含约定外键：{:?}",
                    tag.keys().collect::<Vec<_>>()
                )));
            }
            let Some(var) = text("var") else {
                // varList 等变体第一批不支持。
                return Err(UnsupportedReason::UnsupportedTag("Condition 缺 var".into()));
            };
            let negated = matches!(tag.get("neg"), Some(StatMapValue::Bool(true)));
            Ok(ModTag::condition(var, negated))
        }
        // 敌方状态条件（vendor 如 `SkillStatMap.lua:1119` `{ type = "ActorCondition",
        // actor = "enemy", var = "Burning" }`）：PoBR 既有约定把敌方条件折算为
        // `Enemy<Var>` 条件变量（`mod_parser.rs:950-964`，编排层经 build config
        // 注入 `EnemyBurning` 等），翻译为同名 Condition tag。actor ≠ enemy
        // （player/parent…）第一批不支持。
        "ActorCondition" => {
            if !keys_subset_of(&["type", "actor", "var", "neg"]) {
                return Err(UnsupportedReason::UnsupportedTag(format!(
                    "ActorCondition 含约定外键：{:?}",
                    tag.keys().collect::<Vec<_>>()
                )));
            }
            if text("actor").as_deref() != Some("enemy") {
                return Err(UnsupportedReason::UnsupportedTag(
                    "ActorCondition 非 enemy actor".into(),
                ));
            }
            let Some(var) = text("var") else {
                return Err(UnsupportedReason::UnsupportedTag(
                    "ActorCondition 缺 var".into(),
                ));
            };
            let negated = matches!(tag.get("neg"), Some(StatMapValue::Bool(true)));
            Ok(ModTag::condition(format!("Enemy{var}"), negated))
        }
        "Multiplier" => {
            if !keys_subset_of(&[
                "type",
                "var",
                "div",
                "limit",
                "limitVar",
                "invert",
                "limitTotal",
            ]) {
                return Err(UnsupportedReason::UnsupportedTag(format!(
                    "Multiplier 含约定外键：{:?}",
                    tag.keys().collect::<Vec<_>>()
                )));
            }
            let Some(var) = text("var") else {
                return Err(UnsupportedReason::UnsupportedTag(
                    "Multiplier 缺 var".into(),
                ));
            };
            // limitVar（vendor ModStore.lua:369 动态上限，如 Sigil of Power
            // `SigilOfPowerStage` 受 `SigilOfPowerMaxStages` 封顶）、invert
            // （:378-380 倒数缩放，如 Elemental Conflux 三元素按
            // `ElementalConflux<El>Effect` 取 1/N 均摊）与 limitTotal（:370-371 +
            // 402-404 总量封顶，如「每层中毒 +N% 伤害至多 +M%」）直译为 ModTag 字段。
            Ok(ModTag::Multiplier {
                var,
                div: number("div").unwrap_or(1.0),
                limit: number("limit"),
                actor: None,
                limit_var: text("limitVar"),
                limit_actor: None,
                invert: matches!(tag.get("invert"), Some(StatMapValue::Bool(true))),
                limit_total: matches!(tag.get("limitTotal"), Some(StatMapValue::Bool(true))),
            })
        }
        // 阈值 gate（vendor ModStore.lua:429-459）：`mult = GetMultiplier(var)`，
        // `threshold = tag.threshold or GetMultiplier(thresholdVar)`，落错侧跳过。
        // `thresholdVar` 形态只准入 **已核实全 vendor 树零 setter** 的变量
        // （GetMultiplier 对未设变量恒返 0 → 阈值恒 0，静态折算无损）；有 setter
        // 的（如 Attrition `AttritionCullSeconds`）维持 Unsupported——静态折 0 会
        // 错开 gate。actor/thresholdActor/scalar/equals 变体由 keys_subset 挡下。
        "MultiplierThreshold" => {
            if !keys_subset_of(&["type", "var", "threshold", "thresholdVar", "upper"]) {
                return Err(UnsupportedReason::UnsupportedTag(format!(
                    "MultiplierThreshold 含约定外键：{:?}",
                    tag.keys().collect::<Vec<_>>()
                )));
            }
            let Some(var) = text("var") else {
                return Err(UnsupportedReason::UnsupportedTag(
                    "MultiplierThreshold 缺 var".into(),
                ));
            };
            let threshold = match (number("threshold"), text("thresholdVar")) {
                (Some(t), None) => t,
                // Refraction I/II `RefractionMinimumValour`（sup_str.lua:5978-6024）：
                // 全 vendor 树无任何 setter → 恒 0（ValourStacks ≥ 0 恒过 gate，
                // 与 vendor 默认配置行为一致）。
                (None, Some(v)) if v == "RefractionMinimumValour" => 0.0,
                (None, Some(v)) => {
                    return Err(UnsupportedReason::UnsupportedTag(format!(
                        "MultiplierThreshold thresholdVar 非零 setter 未核实：{v}"
                    )));
                }
                _ => {
                    return Err(UnsupportedReason::UnsupportedTag(
                        "MultiplierThreshold 缺 threshold".into(),
                    ));
                }
            };
            Ok(ModTag::MultiplierThreshold {
                var,
                threshold,
                upper: matches!(tag.get("upper"), Some(StatMapValue::Bool(true))),
            })
        }
        "PerStat" => {
            if !keys_subset_of(&["type", "stat", "div", "limit", "limitTotal"]) {
                return Err(UnsupportedReason::UnsupportedTag(format!(
                    "PerStat 含约定外键：{:?}",
                    tag.keys().collect::<Vec<_>>()
                )));
            }
            let Some(stat) = text("stat") else {
                return Err(UnsupportedReason::UnsupportedTag("PerStat 缺 stat".into()));
            };
            // PoB2 PerStat 读 actor 输出 stat；PoBR 经 cfg.multipliers 注入同名变量
            // （缩写归一化为 PoBR 资源名）。变量未注入时乘 0 → 贡献 0（欠算安全）。
            let var = match stat.as_str() {
                "Str" => "Strength".to_string(),
                "Dex" => "Dexterity".to_string(),
                "Int" => "Intelligence".to_string(),
                other => other.to_string(),
            };
            // limit / limitTotal（vendor ModStore.lua:461-468 + :402-404；如 Atalui's
            // Bloodletting `PerStat{stat=LifeCost,div=20,limit=40,limitTotal}`——
            // per 20 life cost 至多 +40% 总量封顶）。
            let mut mtag = ModTag::multiplier(var, number("div").unwrap_or(1.0), number("limit"));
            if let (ModTag::Multiplier { limit_total, .. }, Some(StatMapValue::Bool(true))) =
                (&mut mtag, tag.get("limitTotal"))
            {
                *limit_total = true;
            }
            Ok(mtag)
        }
        // 技能类型限定（vendor `{ type = "SkillType", skillType = SkillType.X,
        // [neg = true] }`，如 Garukhan `attacks_roll_crits_twice` 的 Attack 限定、
        // Archmage buff 的「non-channelling Spells」= Channel neg + Spell）→
        // [`ModTag::SkillTypes`] / [`ModTag::SkillTypesNeg`]（`Modifier::matches`
        // 按 `cfg.skill_types` intersects / 反选判定）。类型名走单源
        // `SkillTypes::from_pob2_name`（A1 全量 290 枚举表）——编排层
        // `skill_type_bits` 已全量置位（conditions.rs），限 Attack/Spell 的
        // 旧白名单口径已过时。枚举外名维持 Unsupported。
        "SkillType" => {
            if !keys_subset_of(&["type", "skillType", "neg"]) {
                return Err(UnsupportedReason::UnsupportedTag(format!(
                    "SkillType 含约定外键：{:?}",
                    tag.keys().collect::<Vec<_>>()
                )));
            }
            let name = text("skillType");
            let Some(bits) = name.as_deref().and_then(SkillTypes::from_pob2_name) else {
                return Err(UnsupportedReason::UnsupportedTag(format!(
                    "SkillType 未支持类型：{name:?}"
                )));
            };
            if matches!(tag.get("neg"), Some(StatMapValue::Bool(true))) {
                Ok(ModTag::SkillTypesNeg(bits))
            } else {
                Ok(ModTag::SkillTypes(bits))
            }
        }
        // 距离插值（vendor `{ type = "DistanceRamp", ramp = {{d,m},...} }`，如 Close
        // Combat `support_close_combat_attack_damage_+%_final_from_distance`）→
        // [`ModTag::DistanceRamp`]（求值期按 `enemyDistance` 线性插值，ModStore.lua:574-590）。
        "DistanceRamp" => {
            if !keys_subset_of(&["type", "ramp"]) {
                return Err(UnsupportedReason::UnsupportedTag(format!(
                    "DistanceRamp 含约定外键：{:?}",
                    tag.keys().collect::<Vec<_>>()
                )));
            }
            let Some(StatMapValue::List(points)) = tag.get("ramp") else {
                return Err(UnsupportedReason::UnsupportedTag(
                    "DistanceRamp 缺 ramp 点列".into(),
                ));
            };
            let mut ramp = Vec::with_capacity(points.len());
            for point in points {
                // 每点须是 `[距离, 倍率]` 二元数组。
                let StatMapValue::List(pair) = point else {
                    return Err(UnsupportedReason::UnsupportedTag(
                        "DistanceRamp ramp 点非数组".into(),
                    ));
                };
                let (Some(StatMapValue::Number(d)), Some(StatMapValue::Number(m))) =
                    (pair.first(), pair.get(1))
                else {
                    return Err(UnsupportedReason::UnsupportedTag(
                        "DistanceRamp ramp 点非 [距离,倍率] 数对".into(),
                    ));
                };
                ramp.push((*d, *m));
            }
            if ramp.is_empty() {
                return Err(UnsupportedReason::UnsupportedTag(
                    "DistanceRamp ramp 点列为空".into(),
                ));
            }
            Ok(ModTag::DistanceRamp { ramp })
        }
        other => Err(UnsupportedReason::UnsupportedTag(other.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pobr_data::catalog::stat_map::StatMapEntry;

    /// 便捷构造：单 mod 条目。
    fn entry_json(json: &str) -> StatMapEntry {
        serde_json::from_str(json).expect("测试条目 JSON 合法")
    }

    fn catalog_json(json: &str) -> StatMapCatalog {
        StatMapCatalog::new(serde_json::from_str(json).expect("测试 catalog JSON 合法"))
    }

    fn expect_modifiers(outcome: MappedOutcome) -> Vec<Modifier> {
        match outcome {
            MappedOutcome::Mapped(items) => items
                .into_iter()
                .map(|item| match item {
                    MappedItem::Modifier(m) => *m,
                    other => panic!("期望 Modifier，得到 {other:?}"),
                })
                .collect(),
            other => panic!("期望 Mapped，得到 {other:?}"),
        }
    }

    // ---- flag 构造器白名单（M4-H）----

    /// 白名单 flag（BifurcateCrit）+ SkillType tag → FLAG modifier +
    /// `ModTag::SkillTypes(ATTACK)`（Garukhan `attacks_roll_crits_twice` 实形，
    /// SkillStatMap.lua:1011-1013）。
    #[test]
    fn flag_kind_whitelist_translates_with_skill_type_tag() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "flag", "name": "BifurcateCrit", "mod_type": "FLAG",
                 "value": true,
                 "tags": [ { "type": "SkillType", "skillType": "Attack" } ] } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 1.0));
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].name.as_str(), "BifurcateCrit");
        assert_eq!(mods[0].mod_type, ModType::Flag);
        assert!(
            mods[0]
                .tags
                .iter()
                .any(|t| matches!(t, ModTag::SkillTypes(st) if st.contains(SkillTypes::ATTACK)))
        );
    }

    // ---- 伤害异常族白名单（M4-K）----

    /// Escalating Poison 实形（sup_dex.lua:2188-2191 `number_of_additional_
    /// poison_stacks` → `PoisonStacks BASE + PoisonCanStack flag` 成对）：
    /// 两元素都翻译成功（flag 走白名单、mod 走直通族）。
    #[test]
    fn poison_stacks_pair_translates() {
        let entry = entry_json(
            r#"{ "mods": [
                 { "kind": "mod", "name": "PoisonStacks", "mod_type": "BASE" },
                 { "kind": "flag", "name": "PoisonCanStack", "mod_type": "FLAG", "value": true } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 1.0));
        assert_eq!(mods.len(), 2);
        assert_eq!(mods[0].name.as_str(), "PoisonStacks");
        assert_eq!(mods[0].mod_type, ModType::Base);
        assert_eq!(mods[1].name.as_str(), "PoisonCanStack");
        assert_eq!(mods[1].mod_type, ModType::Flag);
    }

    /// 异常持续时间归一：vendor `EnemyPoisonDuration`（施加方对敌 debuff 时长，
    /// CalcOffence.lua:5037）→ PoBR 聚合名 `PoisonDuration`（Escalating Poison
    /// `support_multi_poison_poison_duration_+%_final` MORE -20 实形）。
    #[test]
    fn enemy_poison_duration_renames_to_pobr_bucket() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "EnemyPoisonDuration", "mod_type": "MORE" } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, -20.0));
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].name.as_str(), "PoisonDuration");
        assert_eq!(mods[0].mod_type, ModType::More);
        assert_eq!(mods[0].value.as_number(), Some(-20.0));
    }

    /// 量级词条 keyword 限定直译：Deadly Poison `support_deadly_poison_poison_
    /// effect_+%_final` → `AilmentMagnitude MORE kw=Poison`（sup_dex.lua:1748-1750）。
    /// 消费侧 ailment_scoped_cfg 置位后 ANY-overlap 命中。
    #[test]
    fn ailment_magnitude_with_poison_keyword_translates() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "AilmentMagnitude", "mod_type": "MORE",
                 "keyword_flags": ["Poison"] } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 75.0));
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].name.as_str(), "AilmentMagnitude");
        assert!(mods[0].keyword_flags.intersects(KeywordFlags::POISON));
    }

    /// 施加几率直通：Envenom/Bleed III 的 `<Ailment>Chance BASE`
    /// （SkillStatMap.lua:1267 / sup_str.lua:932）。
    #[test]
    fn ailment_chance_passthrough() {
        for name in ["PoisonChance", "BleedChance", "EnemyIgniteChance"] {
            let entry = entry_json(&format!(
                r#"{{ "mods": [ {{ "kind": "mod", "name": "{name}", "mod_type": "BASE" }} ] }}"#
            ));
            let mods = expect_modifiers(map_entry(&entry, 60.0));
            assert_eq!(mods[0].name.as_str(), name, "{name} 应直通");
        }
    }

    /// M4-m（k3）：异常堆叠速率 rateMod 名直通（vendor `faster_burn_%` 族 →
    /// `<Ailment>Faster` INC，SkillStatMap.lua:843-848；消费方 ailment_rate_mod）。
    #[test]
    fn ailment_faster_passthrough() {
        for name in ["BleedFaster", "PoisonFaster", "IgniteFaster"] {
            let entry = entry_json(&format!(
                r#"{{ "mods": [ {{ "kind": "mod", "name": "{name}", "mod_type": "INC" }} ] }}"#
            ));
            let mods = expect_modifiers(map_entry(&entry, 25.0));
            assert_eq!(mods[0].name.as_str(), name, "{name} 应直通");
            assert_eq!(mods[0].mod_type, ModType::Inc);
        }
    }

    /// M4-m：CHANCE 桶 → Base 求和语义（Rakiata's Flow
    /// `treat_enemy_resistances_as_negated_…` → HitsInvertEleResChance，
    /// SkillStatMap.lua:941-944，entry div=100；消费点 clamp 见
    /// `offence::enemy_damage_multiplier`）。
    #[test]
    fn chance_mod_type_maps_to_base_for_invert_ele_res() {
        let entry = entry_json(
            r#"{ "div": 100.0, "mods": [ { "kind": "mod",
                 "name": "HitsInvertEleResChance", "mod_type": "CHANCE" } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 100.0));
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].name.as_str(), "HitsInvertEleResChance");
        assert_eq!(mods[0].mod_type, ModType::Base);
        assert_eq!(mods[0].value.as_number(), Some(1.0), "div=100 → 分数");
    }

    /// 白名单外 flag 维持未知名上报；SkillType 枚举外类型名整条跳过
    /// （枚举内类型经 `SkillTypes::from_pob2_name` 全量准入——存量 #7-1）。
    #[test]
    fn flag_kind_outside_whitelist_or_unknown_skill_type_unsupported() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "flag", "name": "projectile", "mod_type": "FLAG" } ] }"#,
        );
        assert!(matches!(
            map_entry(&entry, 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::UnknownModName(_))
        ));

        let entry = entry_json(
            r#"{ "mods": [ { "kind": "flag", "name": "BifurcateCrit", "mod_type": "FLAG",
                 "tags": [ { "type": "SkillType", "skillType": "NotARealSkillType" } ] } ] }"#,
        );
        assert!(matches!(
            map_entry(&entry, 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::UnsupportedTag(_))
        ));
    }

    /// （存量 #7-1）SkillType tag：枚举内类型名全量准入（单源
    /// `SkillTypes::from_pob2_name`）；`neg = true` → [`ModTag::SkillTypesNeg`]
    /// （vendor ModStore.lua:829-833 反选，如 Archmage 的 non-channelling 限定）。
    #[test]
    fn skill_type_tag_full_enum_and_neg_translate() {
        use pobr_data::skill::SkillTypes;
        let mut tag = BTreeMap::new();
        tag.insert("type".into(), StatMapValue::Text("SkillType".into()));
        tag.insert("skillType".into(), StatMapValue::Text("Minion".into()));
        assert_eq!(
            translate_tag(&tag).unwrap(),
            ModTag::SkillTypes(SkillTypes::MINION)
        );
        tag.insert("skillType".into(), StatMapValue::Text("Channel".into()));
        tag.insert("neg".into(), StatMapValue::Bool(true));
        assert_eq!(
            translate_tag(&tag).unwrap(),
            ModTag::SkillTypesNeg(SkillTypes::CHANNEL)
        );
    }

    /// CritChanceCap 直通（Garukhan constant stat 50 → OVERRIDE）。
    #[test]
    fn crit_chance_cap_override_passthrough() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "CritChanceCap", "mod_type": "OVERRIDE" } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 50.0));
        assert_eq!(mods[0].name.as_str(), "CritChanceCap");
        assert_eq!(mods[0].mod_type, ModType::Override);
        assert_eq!(mods[0].value.as_number(), Some(50.0));
    }

    // ---- merge 公式四参全覆盖（蓝图 T2 局部门禁）----

    /// 无参数：注入值 = stat 值。
    #[test]
    fn merge_defaults_to_stat_value() {
        let entry =
            entry_json(r#"{ "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "INC" } ] }"#);
        let mods = expect_modifiers(map_entry(&entry, 42.0));
        assert_eq!(mods[0].name.as_str(), "Damage");
        assert_eq!(mods[0].mod_type, ModType::Inc);
        assert_eq!(mods[0].value.as_number(), Some(42.0));
    }

    /// div：total_cast_time_+_ms 形态（1000ms → 1.0s）。
    #[test]
    fn merge_div_scales_down() {
        let entry = entry_json(
            r#"{ "div": 1000.0,
                 "mods": [ { "kind": "mod", "name": "TotalCastTime", "mod_type": "BASE" } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 1000.0));
        assert_eq!(mods[0].name.as_str(), "TotalCastTime");
        assert_eq!(mods[0].value.as_number(), Some(1.0));
    }

    /// mult + base：注入值 = stat × mult + base。
    #[test]
    fn merge_mult_and_base() {
        let entry = entry_json(
            r#"{ "mult": 2.0, "base": 5.0,
                 "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "INC" } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 10.0));
        assert_eq!(mods[0].value.as_number(), Some(25.0));
    }

    /// value：恒值覆盖，忽略 stat 值（global_bleed_on_hit = 100 形态）。
    #[test]
    fn merge_value_overrides_stat() {
        let entry = entry_json(
            r#"{ "value": 100.0,
                 "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "BASE" } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 7.0));
        assert_eq!(mods[0].value.as_number(), Some(100.0));
    }

    /// 四参组合：value 优先级最高（vendor `map.value or …` 短路）。
    #[test]
    fn merge_value_wins_over_other_params() {
        let entry = entry_json(
            r#"{ "value": 3.0, "div": 2.0, "mult": 10.0, "base": 99.0,
                 "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "INC" } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 8.0));
        assert_eq!(mods[0].value.as_number(), Some(3.0));
    }

    /// group：嵌套 mod 用 group 级参数（CalcActiveSkill.lua:117），entry 级参数不串扰。
    #[test]
    fn group_params_apply_to_nested_mods() {
        let entry = entry_json(
            r#"{ "div": 7.0,
                 "mods": [ { "kind": "group", "div": 2.0, "mods": [
                     { "kind": "mod", "name": "FireDamage", "mod_type": "MORE" },
                     { "kind": "mod", "name": "ColdDamage", "mod_type": "MORE" } ] } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 10.0));
        assert_eq!(mods.len(), 2);
        assert_eq!(mods[0].value.as_number(), Some(5.0)); // 10/2，非 10/7
        assert_eq!(mods[1].name.as_str(), "ColdDamage");
    }

    // ---- scalar / 失真 / 未知名 ----

    /// 含 scalar（entry 元素级）→ 整条 Unsupported。
    #[test]
    fn scalar_entry_is_unsupported() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "group", "scalar": "ConsumedPowerChargeEffect", "mods": [
                     { "kind": "mod", "name": "Damage", "mod_type": "MORE" } ] } ] }"#,
        );
        assert_eq!(
            map_entry(&entry, 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::ScalarMultiplier)
        );
    }

    /// 抽取失真条目 → Unsupported(Unextractable)。
    #[test]
    fn unextractable_entry_is_unsupported() {
        let entry = entry_json(r#"{ "_unextractable": true }"#);
        assert_eq!(
            map_entry(&entry, 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::Unextractable)
        );
    }

    /// 未知 ModName → Unsupported(UnknownModName) 上报。
    #[test]
    fn unknown_mod_name_is_reported() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "WeaponRangeMetre", "mod_type": "BASE" } ] }"#,
        );
        assert_eq!(
            map_entry(&entry, 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::UnknownModName(
                "WeaponRangeMetre".into()
            ))
        );
    }

    /// 缺 mod_type（vendor 笔误条目）→ Unsupported(MissingModType)。
    #[test]
    fn missing_mod_type_is_unsupported() {
        let entry = entry_json(r#"{ "mods": [ { "kind": "mod", "name": "Damage" } ] }"#);
        assert_eq!(
            map_entry(&entry, 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::MissingModType)
        );
    }

    /// 任一元素不支持 → 整条跳过（不半条注入）。
    #[test]
    fn one_bad_element_rejects_whole_entry() {
        let entry = entry_json(
            r#"{ "mods": [
                 { "kind": "mod", "name": "Damage", "mod_type": "INC" },
                 { "kind": "mod", "name": "SomethingNovel", "mod_type": "INC" } ] }"#,
        );
        assert!(matches!(
            map_entry(&entry, 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::UnknownModName(_))
        ));
    }

    // ---- 名字翻译 / flag 分派 ----

    /// Speed 按 ModFlag 分派：Attack → AttackSpeed / Cast → CastSpeed / 裸 → SkillSpeed。
    #[test]
    fn speed_dispatches_on_flags() {
        for (flags, expect) in [
            (r#"["Attack"]"#, "AttackSpeed"),
            (r#"["Cast"]"#, "CastSpeed"),
            (r#"[]"#, "SkillSpeed"),
        ] {
            let entry = entry_json(&format!(
                r#"{{ "mods": [ {{ "kind": "mod", "name": "Speed", "mod_type": "INC", "flags": {flags} }} ] }}"#
            ));
            let mods = expect_modifiers(map_entry(&entry, 15.0));
            assert_eq!(mods[0].name.as_str(), expect);
        }
        // 未知 flag 组合 → Unsupported。
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "Speed", "mod_type": "INC", "flags": ["Warcry"] } ] }"#,
        );
        assert!(matches!(
            map_entry(&entry, 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::UnsupportedFlags(_))
        ));
    }

    /// Damage 按 ModFlag 分派；CritChance/CritMultiplier 直译。
    #[test]
    fn damage_and_crit_translation() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "INC", "flags": ["Attack"] } ] }"#,
        );
        assert_eq!(
            expect_modifiers(map_entry(&entry, 1.0))[0].name.as_str(),
            "AttackDamage"
        );
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "CritChance", "mod_type": "MORE" } ] }"#,
        );
        assert_eq!(
            expect_modifiers(map_entry(&entry, 1.0))[0].name.as_str(),
            "CriticalStrikeChance"
        );
    }

    /// 伤害基值族（mod 形态，带 KeywordFlag.Spell）：丢弃 flag 直译 `<Type>DamageMin`。
    #[test]
    fn damage_bound_mod_drops_scoping_flags() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "PhysicalMin", "mod_type": "BASE",
                             "keyword_flags": ["Spell"] } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 12.0));
        assert_eq!(mods[0].name.as_str(), "PhysicalDamageMin");
        assert_eq!(mods[0].value.as_number(), Some(12.0));
        // 不可丢弃的 keyword flag（如 Warcry）→ Unsupported。
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "PhysicalMin", "mod_type": "BASE",
                             "keyword_flags": ["Warcry"] } ] }"#,
        );
        assert!(matches!(
            map_entry(&entry, 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::UnsupportedKeywordFlags(_))
        ));
    }

    /// ModFlag 直译：已移植子集附在 Modifier 上（PoB2 子集匹配语义两边一致）。
    /// vendor `sup_str.lua` Melee Physical Damage statMap：
    /// `mod("PhysicalDamage","MORE",nil,ModFlag.Melee)`。
    #[test]
    fn supported_mod_flags_are_attached() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "PhysicalDamage", "mod_type": "MORE", "flags": ["Melee"] } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 25.0));
        assert_eq!(mods[0].name.as_str(), "PhysicalDamage");
        assert_eq!(mods[0].flags, ModFlags::MELEE);
        // vendor `ModFlag.Hit` → PoBR keyword HIT 通道（hit-scoping 走 KeywordFlag，
        // cfg.flags 从不置 ModFlags::HIT）：mod 产出、ModFlags 为空、keyword 含 HIT。
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "PhysicalDamage", "mod_type": "MORE", "flags": ["Hit"] } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 25.0));
        assert_eq!(mods[0].name.as_str(), "PhysicalDamage");
        assert_eq!(mods[0].flags, ModFlags::NONE);
        assert_eq!(mods[0].keyword_flags, KeywordFlags::HIT);
    }

    /// gain-as + ModFlag.Attack（vendor `SkillStatMap.lua:1116`
    /// `non_skill_base_all_damage_%_to_gain_as_chaos_with_attacks`）：
    /// 名直通 + ATTACK flag 附着（攻击 cfg 下生效，法术不吃——对齐 PoB2 作用域）。
    #[test]
    fn gain_as_with_attack_flag_translates() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "DamageGainAsChaos", "mod_type": "BASE", "flags": ["Attack"] } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 30.0));
        assert_eq!(mods[0].name.as_str(), "DamageGainAsChaos");
        assert_eq!(mods[0].flags, ModFlags::ATTACK);
        assert_eq!(mods[0].value.as_number(), Some(30.0));
    }

    /// Speed 分派吃掉 Attack 后剩余 flag 照常直译（Attack+Melee → AttackSpeed+MELEE）。
    #[test]
    fn speed_dispatch_keeps_remaining_flags() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "Speed", "mod_type": "INC", "flags": ["Attack", "Melee"] } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 10.0));
        assert_eq!(mods[0].name.as_str(), "AttackSpeed");
        assert_eq!(mods[0].flags, ModFlags::MELEE);
    }

    /// `skill_speed_+%` 全条目三 mod（vendor `SkillStatMap.lua:554-557`）：
    /// Speed → SkillSpeed；WarcrySpeed/TotemPlacementSpeed 惰性名直通
    /// （WarcrySpeed 的冗余 KeywordFlag.Warcry 安全丢弃）。
    #[test]
    fn skill_speed_entry_maps_all_three_mods() {
        let entry = entry_json(
            r#"{ "mods": [
                 { "kind": "mod", "name": "Speed", "mod_type": "INC" },
                 { "kind": "mod", "name": "WarcrySpeed", "mod_type": "INC", "keyword_flags": ["Warcry"] },
                 { "kind": "mod", "name": "TotemPlacementSpeed", "mod_type": "INC" } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 20.0));
        assert_eq!(
            mods.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(),
            vec!["SkillSpeed", "WarcrySpeed", "TotemPlacementSpeed"]
        );
        assert!(mods.iter().all(|m| m.value.as_number() == Some(20.0)));
        // 非惰性名上的 Warcry keyword 仍拒绝（无对应 KeywordFlags 位）。
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "INC", "keyword_flags": ["Warcry"] } ] }"#,
        );
        assert!(matches!(
            map_entry(&entry, 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::UnsupportedKeywordFlags(_))
        ));
    }

    /// 已移植 KeywordFlag 位直译附着（如 KeywordFlag.Bleed）。
    #[test]
    fn ported_keyword_flags_are_attached() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "INC", "keyword_flags": ["Bleed"] } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 15.0));
        assert_eq!(mods[0].keyword_flags, KeywordFlags::BLEED);
    }

    /// KeywordFlag.Attack/Spell → 等价 ModFlags 门控（vendor `sup_str.lua:2825-2827`
    /// Elemental Armament：`mod("ElementalDamage","MORE",nil,0,KeywordFlag.Attack)`；
    /// PoB2 ANY keyword 匹配与 PoBR cfg.flags ATTACK 子集匹配同为"仅攻击技能生效"）。
    #[test]
    fn attack_spell_keywords_become_mod_flags() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "ElementalDamage", "mod_type": "MORE", "keyword_flags": ["Attack"] } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 25.0));
        assert_eq!(mods[0].name.as_str(), "ElementalDamage");
        assert_eq!(mods[0].flags, ModFlags::ATTACK);
        assert_eq!(mods[0].keyword_flags, KeywordFlags::NONE);
    }

    /// ActorCondition(enemy) → `Enemy<Var>` Condition（vendor `SkillStatMap.lua:1119`
    /// + PoBR `mod_parser.rs:950-964` 敌方条件命名约定）。
    #[test]
    fn enemy_actor_condition_translates() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "DamageGainAsFire", "mod_type": "BASE",
                 "tags": [ { "type": "ActorCondition", "actor": "enemy", "var": "Burning" } ] } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 40.0));
        assert_eq!(mods[0].tags, vec![ModTag::condition("EnemyBurning", false)]);
        // 非 enemy actor → Unsupported。
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "INC",
                 "tags": [ { "type": "ActorCondition", "actor": "parent", "var": "Stationary" } ] } ] }"#,
        );
        assert!(matches!(
            map_entry(&entry, 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::UnsupportedTag(_))
        ));
    }

    /// set_key=None 自动取默认 set "1" 覆盖（PoB2 缺省 statSetIndex=1，
    /// `SkillsTab.lua:354` / `CalcActiveSkill.lua:166-171`）。
    #[test]
    fn none_set_key_uses_default_set_one() {
        let catalog = catalog_json(
            r#"{
              "global": {
                "damage_+%": { "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "INC" } ] }
              },
              "per_stat_set": {
                "FooPlayer": { "1": {
                  "damage_+%": { "mods": [ { "kind": "mod", "name": "FireDamage", "mod_type": "INC" } ] }
                } },
                "BarPlayer": { "2": {
                  "damage_+%": { "mods": [ { "kind": "mod", "name": "ColdDamage", "mod_type": "INC" } ] }
                } }
              }
            }"#,
        );
        // 默认 set "1" 覆盖命中。
        let outcome = map_stat(&catalog, "FooPlayer", None, "damage_+%", 10.0);
        assert_eq!(expect_modifiers(outcome)[0].name.as_str(), "FireDamage");
        // 非 "1" 的 set 覆盖不被默认选择（需显式 set_key，T5 接线）。
        let outcome = map_stat(&catalog, "BarPlayer", None, "damage_+%", 10.0);
        assert_eq!(expect_modifiers(outcome)[0].name.as_str(), "Damage");
        let outcome = map_stat(&catalog, "BarPlayer", Some("2"), "damage_+%", 10.0);
        assert_eq!(expect_modifiers(outcome)[0].name.as_str(), "ColdDamage");
    }

    /// 转换 / gain-as 名直通；非法类型词不通过。
    #[test]
    fn conversion_names_pass_through() {
        for name in [
            "SkillPhysicalDamageConvertToFire",
            "SkillDamageGainAsChaos",
            "PhysicalDamageGainAsCold",
        ] {
            let entry = entry_json(&format!(
                r#"{{ "mods": [ {{ "kind": "mod", "name": "{name}", "mod_type": "BASE" }} ] }}"#
            ));
            assert_eq!(
                expect_modifiers(map_entry(&entry, 30.0))[0].name.as_str(),
                name
            );
        }
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "SkillFooDamageConvertToBar", "mod_type": "BASE" } ] }"#,
        );
        assert!(matches!(
            map_entry(&entry, 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::UnknownModName(_))
        ));
    }

    // ---- tag 第一批 ----

    /// Condition tag → ModTag::Condition（含 neg）。
    #[test]
    fn condition_tag_translates() {
        let entry = entry_json(
            r#"{ "value": 100.0, "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "MORE",
                 "tags": [ { "type": "Condition", "var": "Leeching", "neg": true } ] } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 1.0));
        assert_eq!(mods[0].tags, vec![ModTag::condition("Leeching", true)]);
    }

    /// Multiplier / PerStat tag → ModTag::Multiplier（PerStat 缩写归一化）。
    #[test]
    fn multiplier_and_per_stat_tags_translate() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "INC",
                 "tags": [ { "type": "Multiplier", "var": "PowerCharge", "limit": 5.0 } ] } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 1.0));
        assert_eq!(
            mods[0].tags,
            vec![ModTag::multiplier("PowerCharge", 1.0, Some(5.0))]
        );
        let entry =
            entry_json(r#"{ "mods": [ { "kind": "skill_data", "value": { "key": "Damage" } } ] }"#);
        // （上一行只为构造合法 JSON 的反例占位——skill_data Damage 键不在白名单）
        assert!(matches!(
            map_entry(&entry, 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::UnsupportedSkillDataKey(_))
        ));
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "INC",
                 "tags": [ { "type": "PerStat", "stat": "Int", "div": 10.0 } ] } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 1.0));
        assert_eq!(
            mods[0].tags,
            vec![ModTag::multiplier("Intelligence", 10.0, None)]
        );
    }

    /// 第一批之外的 tag（GlobalEffect）→ 整条 Unsupported。
    #[test]
    fn unsupported_tag_types_reject_entry() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "MORE",
                 "tags": [ { "type": "GlobalEffect", "effectType": "Buff" } ] } ] }"#,
        );
        assert!(matches!(
            map_entry(&entry, 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::UnsupportedTag(_))
        ));
    }

    /// `DistanceRamp` tag（Close Combat `..._final_from_distance`）→
    /// [`ModTag::DistanceRamp`]，ramp 点列原样翻译（vendor ModStore.lua:574-590）。
    #[test]
    fn distance_ramp_tag_translates() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "MORE",
                 "tags": [ { "type": "DistanceRamp", "ramp": [[10,1],[35,0]] } ] } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 30.0));
        assert_eq!(
            mods[0].tags,
            vec![ModTag::DistanceRamp {
                ramp: vec![(10.0, 1.0), (35.0, 0.0)],
            }]
        );
    }

    /// DistanceRamp 含约定外键 → 整条 Unsupported（多余键往往携带额外语义）。
    #[test]
    fn distance_ramp_rejects_extra_keys() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "MORE",
                 "tags": [ { "type": "DistanceRamp", "ramp": [[10,1]], "var": "x" } ] } ] }"#,
        );
        assert!(matches!(
            map_entry(&entry, 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::UnsupportedTag(_))
        ));
    }

    /// `Multiplier{limitTotal}`（vendor ModStore.lua:370-371 + 402-404 总量封顶）：
    /// `limit` 不截断乘数计数，而在 `value × mult` 后对最终贡献封顶。
    /// 形如「每层中毒 +15% 伤害，至多 +75%」（var=PoisonStacks, limit=75, limitTotal）。
    #[test]
    fn multiplier_limit_total_caps_final_contribution() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "INC",
                 "tags": [ { "type": "Multiplier", "var": "PoisonStacks", "limit": 75.0,
                             "limitTotal": true } ] } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 15.0));
        assert_eq!(
            mods[0].tags,
            vec![ModTag::Multiplier {
                var: "PoisonStacks".into(),
                div: 1.0,
                limit: Some(75.0),
                actor: None,
                limit_var: None,
                limit_actor: None,
                invert: false,
                limit_total: true,
            }]
        );
        // 3 层：15×3 = 45 ≤ 75 → 45（未触顶）。
        let cfg3 = crate::CalcConfig::new().with_multiplier("PoisonStacks", 3.0);
        assert_eq!(mods[0].effective_number(&cfg3), Some(45.0));
        // 8 层：15×8 = 120 → 总量封顶到 75（计数封顶会给 15×min(8,75)=120，错算）。
        let cfg8 = crate::CalcConfig::new().with_multiplier("PoisonStacks", 8.0);
        assert_eq!(mods[0].effective_number(&cfg8), Some(75.0));
    }

    // ---- skill_data / flag 构造器 ----

    /// skill_data 伤害基值键 → `<Type>DamageMin/Max` BASE modifier。
    #[test]
    fn skill_data_damage_bounds_become_modifiers() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "skill_data", "value": { "key": "FireMin" } } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 19.0));
        assert_eq!(mods[0].name.as_str(), "FireDamageMin");
        assert_eq!(mods[0].mod_type, ModType::Base);
        assert_eq!(mods[0].value.as_number(), Some(19.0));
    }

    /// skill_data duration → SkillData 项（entry div=1000 换算 ms → s）。
    #[test]
    fn skill_data_duration_emits_skill_data() {
        let entry = entry_json(
            r#"{ "div": 1000.0,
                 "mods": [ { "kind": "skill_data", "value": { "key": "duration" } } ] }"#,
        );
        match map_entry(&entry, 4000.0) {
            MappedOutcome::Mapped(items) => assert_eq!(
                items,
                vec![MappedItem::SkillData {
                    key: "duration".into(),
                    value: 4.0
                }]
            ),
            other => panic!("期望 Mapped，得到 {other:?}"),
        }
    }

    /// flag 构造器（技能行为开关）第一批无消费方 → Unsupported 上报。
    #[test]
    fn flag_ctor_is_unsupported_in_first_batch() {
        let entry = entry_json(r#"{ "mods": [ { "kind": "flag", "name": "projectile" } ] }"#);
        assert_eq!(
            map_entry(&entry, 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::UnknownModName("flag:projectile".into()))
        );
    }

    /// entry 带 skillFlag（statSet flags 路径消费）→ Unsupported(SkillFlag)。
    #[test]
    fn entry_skill_flag_is_unsupported() {
        let entry = entry_json(r#"{ "skill_flag": "arrow" }"#);
        assert_eq!(
            map_entry(&entry, 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::SkillFlag("arrow".into()))
        );
    }

    /// FLAG 类型 mod（vendor `mod(…, "FLAG", …)`）→ Modifier::flag（Lua 真值语义）。
    #[test]
    fn flag_typed_mod_becomes_bool_modifier() {
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "FLAG" } ] }"#,
        );
        let mods = expect_modifiers(map_entry(&entry, 0.0));
        assert_eq!(mods[0].mod_type, ModType::Flag);
        assert_eq!(mods[0].value.as_bool(), Some(true));
    }

    // ---- catalog 查找语义 ----

    /// per-set 覆盖优先；miss 落回 global；双 miss → Unknown。
    #[test]
    fn per_set_overrides_global_then_falls_back() {
        let catalog = catalog_json(
            r#"{
              "global": {
                "damage_+%": { "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "INC" } ] }
              },
              "per_stat_set": {
                "IceNovaPlayer": { "2": {
                  "damage_+%": { "mods": [ { "kind": "mod", "name": "ColdDamage", "mod_type": "INC" } ] }
                } }
              }
            }"#,
        );
        // per-set 命中。
        let outcome = map_stat(&catalog, "IceNovaPlayer", Some("2"), "damage_+%", 10.0);
        assert_eq!(expect_modifiers(outcome)[0].name.as_str(), "ColdDamage");
        // set miss → global。
        let outcome = map_stat(&catalog, "IceNovaPlayer", Some("1"), "damage_+%", 10.0);
        assert_eq!(expect_modifiers(outcome)[0].name.as_str(), "Damage");
        // 无 set 上下文 → global。
        let outcome = map_stat(&catalog, "Other", None, "damage_+%", 10.0);
        assert_eq!(expect_modifiers(outcome)[0].name.as_str(), "Damage");
        // 双 miss → Unknown。
        assert_eq!(
            map_stat(&catalog, "Other", None, "nonexistent_stat", 1.0),
            MappedOutcome::Unknown
        );
    }

    /// （存量 #7-1）curse 技能局部效果乘区取数：裸 `CurseEffect` INC/MORE 计入，
    /// 带 GlobalEffect / 其他 tag 的元素与无关 stat 归零贡献。
    #[test]
    fn curse_local_effect_collects_bare_curse_effect_only() {
        let catalog = catalog_json(
            r#"{
              "global": {
                "curse_effect_+%": { "mods": [ { "kind": "mod", "name": "CurseEffect", "mod_type": "INC" } ] },
                "support_atziri_curse_effect_+%_final": { "mods": [ { "kind": "mod", "name": "CurseEffect", "mod_type": "MORE" } ] },
                "mark_effect_+%": { "mods": [ { "kind": "mod", "name": "CurseEffect", "mod_type": "INC",
                    "tags": [ { "type": "SkillType", "skillType": "Mark" } ] } ] }
              },
              "per_stat_set": {}
            }"#,
        );
        assert_eq!(
            curse_local_effect(&catalog, "X", None, "curse_effect_+%", 25.0),
            (25.0, 1.0)
        );
        assert_eq!(
            curse_local_effect(
                &catalog,
                "X",
                None,
                "support_atziri_curse_effect_+%_final",
                -20.0
            ),
            (0.0, 0.8)
        );
        // 带 tag（Mark 门控）的变体保守不计。
        assert_eq!(
            curse_local_effect(&catalog, "X", None, "mark_effect_+%", 30.0),
            (0.0, 1.0)
        );
        // 无关 stat / 无条目 → 零贡献。
        assert_eq!(
            curse_local_effect(&catalog, "X", None, "nope", 1.0),
            (0.0, 1.0)
        );
    }

    // ---- W-J：isGlobalEffect / global-only merge ----

    /// isGlobalEffect 等价（CalcActiveSkill.lua:68-80）：单 mod 查自身 tags；
    /// group 任一成员命中即 global；flag/skill_data 形态同样按 tags 判。
    #[test]
    fn is_global_effect_matches_vendor_predicate() {
        let m = |json: &str| -> StatMapMod { serde_json::from_str(json).expect("mod JSON 合法") };
        // 单 mod：带 / 不带 GlobalEffect tag。
        assert!(is_global_effect(&m(
            r#"{ "kind": "mod", "name": "Damage", "mod_type": "INC",
                 "tags": [ { "type": "GlobalEffect", "effectType": "Buff" } ] }"#
        )));
        assert!(!is_global_effect(&m(
            r#"{ "kind": "mod", "name": "Damage", "mod_type": "INC",
                 "tags": [ { "type": "Condition", "var": "Leeching" } ] }"#
        )));
        assert!(!is_global_effect(&m(
            r#"{ "kind": "mod", "name": "Damage", "mod_type": "INC" }"#
        )));
        // group：任一成员带 tag → 整组 global；全员不带 → 非 global。
        assert!(is_global_effect(&m(r#"{ "kind": "group", "mods": [
                 { "kind": "mod", "name": "Damage", "mod_type": "INC" },
                 { "kind": "mod", "name": "CastSpeed", "mod_type": "INC",
                   "tags": [ { "type": "GlobalEffect", "effectType": "Buff" } ] } ] }"#)));
        assert!(!is_global_effect(&m(r#"{ "kind": "group", "mods": [
                 { "kind": "mod", "name": "Damage", "mod_type": "INC" },
                 { "kind": "mod", "name": "ColdDamage", "mod_type": "MORE" } ] }"#)));
        // skill_data 形态（vendor skill() 构造器同样可带 GlobalEffect tag）。
        assert!(is_global_effect(&m(
            r#"{ "kind": "skill_data", "value": { "key": "duration" },
                 "tags": [ { "type": "GlobalEffect", "effectType": "Buff" } ] }"#
        )));
    }

    /// global-only 双 set 用例（对照 CalcActiveSkill.lua:124-140，蓝图 W-J 门禁）：
    /// 选中 set 全量 merge + global 记账；未选 set 仅 global 元素参与、
    /// 非 global 静默跳过（Mapped 空）、记账命中 stat 由调用方整条跳过。
    #[test]
    fn global_only_merge_dual_set_semantics() {
        // 合成双 set 目录（DemonForm 形态：global = GlobalEffect Buff tag 的 INC，
        // 见 other.lua:4384-4386；local = 普通分等级 stat）：
        // set "1"（选中）：alpha（global+local 混合条目）、beta（纯 local）；
        // set "2"（未选）：alpha（per-set 覆盖，global）、beta（纯 local）、
        //                 gamma（global，仅经 global 表可见）。
        let catalog = catalog_json(
            r#"{
              "global": {
                "alpha": { "mods": [
                    { "kind": "mod", "name": "Damage", "mod_type": "INC" },
                    { "kind": "mod", "name": "CastSpeed", "mod_type": "INC",
                      "tags": [ { "type": "GlobalEffect", "effectType": "Buff" } ] } ] },
                "beta": { "mods": [ { "kind": "mod", "name": "ColdDamage", "mod_type": "INC" } ] },
                "gamma": { "mods": [ { "kind": "mod", "name": "AttackSpeed", "mod_type": "INC",
                      "tags": [ { "type": "GlobalEffect", "effectType": "Buff" } ] } ] }
              },
              "per_stat_set": {
                "Foo": { "2": {
                  "alpha": { "mods": [ { "kind": "mod", "name": "SkillSpeed", "mod_type": "INC",
                      "tags": [ { "type": "GlobalEffect", "effectType": "Buff" } ] } ] }
                } }
              }
            }"#,
        );
        // ① 选中 set "1" merge 时的记账（:104-106）：alpha 含 global 元素 → 记账；
        //    beta 纯 local → 不记账。
        assert!(stat_has_global_mods(&catalog, "Foo", Some("1"), "alpha"));
        assert!(!stat_has_global_mods(&catalog, "Foo", Some("1"), "beta"));
        // 未选 set "2" 视角：alpha 走 per-set 覆盖链，同样含 global。
        assert!(stat_has_global_mods(&catalog, "Foo", Some("2"), "alpha"));
        // ② 未选 set 的 global-only：纯 local 条目 → 静默无注入（Mapped 空，
        //    非 Unsupported——vendor :107 直接不收，不属于"不支持"）。
        assert_eq!(
            map_stat_global_only(&catalog, "Foo", Some("2"), "beta", 10.0),
            MappedOutcome::Mapped(Vec::new())
        );
        // ③ global 元素保留参与翻译——第一批 GlobalEffect tag 仍在翻译边界外
        //    （buff 域 M3），整条 Unsupported 上报、注入为零（宁可跳过不可错算）。
        for stat in ["alpha", "gamma"] {
            assert!(
                matches!(
                    map_stat_global_only(&catalog, "Foo", Some("2"), stat, 10.0),
                    MappedOutcome::Unsupported(UnsupportedReason::UnsupportedTag(_))
                ),
                "global 元素应整条上报（M3 前注入为零）：{stat}"
            );
        }
        // ④ 调用方记账跳过语义：alpha 在选中 set 已按 global 记账 → 未选 set 不再
        //    调用 map_stat_global_only（vendor selectedGlobalStats 的 stat 级等价，
        //    见 stat_has_global_mods 文档）。记账探针 + global-only 组合即 :107 全条件。
        // ⑤ 条目双 miss → Unknown。
        assert_eq!(
            map_stat_global_only(&catalog, "Foo", Some("2"), "nonexistent", 1.0),
            MappedOutcome::Unknown
        );
    }

    /// global-only 的元素粒度与 skill_flag 行为：group 整体按 global 保留（含不带
    /// tag 的成员，vendor :103 粒度 = modOrGroup）；skill_flag 条目无 modOrGroup →
    /// global-only 下自然 Mapped 空（flags 仅在选中 set 生效）。
    #[test]
    fn global_only_group_granularity_and_skill_flag() {
        let entry: StatMapEntry = serde_json::from_str(
            r#"{ "mods": [
                 { "kind": "group", "mods": [
                     { "kind": "mod", "name": "Damage", "mod_type": "INC" },
                     { "kind": "mod", "name": "CastSpeed", "mod_type": "INC",
                       "tags": [ { "type": "GlobalEffect", "effectType": "Buff" } ] } ] },
                 { "kind": "mod", "name": "ColdDamage", "mod_type": "MORE" } ] }"#,
        )
        .expect("条目 JSON 合法");
        // group 任一成员 global → 整组保留；兄弟普通 mod 不沾染。
        assert!(is_global_effect(&entry.mods[0]));
        assert!(!is_global_effect(&entry.mods[1]));
        // skill_flag 条目：无 modOrGroup → global-only 下 Mapped 空。
        let catalog =
            catalog_json(r#"{ "global": { "skill_can_fire_arrows": { "skill_flag": "arrow" } } }"#);
        assert_eq!(
            map_stat_global_only(&catalog, "Any", None, "skill_can_fire_arrows", 1.0),
            MappedOutcome::Mapped(Vec::new())
        );
        // 抽取失真条目 → Unsupported 上报（内容未知，可见性优先；注入同为零）。
        let catalog = catalog_json(r#"{ "global": { "broken": { "_unextractable": true } } }"#);
        assert!(matches!(
            map_stat_global_only(&catalog, "Any", None, "broken", 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::Unextractable)
        ));
    }

    // ---- M3-W4：curse 域敌侧映射 ----

    /// Despair 形态（per-set `ChaosResist` BASE + GlobalEffect Curse tag）：
    /// 直通敌侧名、GlobalEffect 剥除、merge 值 = stat 原值（负数减抗）。
    #[test]
    fn curse_chaos_resist_maps_to_enemy_name() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "DespairPlayer": { "1": {
                 "base_skill_buff_chaos_damage_resistance_%_to_apply": {
                   "mods": [ { "kind": "mod", "name": "ChaosResist", "mod_type": "BASE",
                               "tags": [ { "type": "GlobalEffect", "effectType": "Curse" } ] } ] } } } } }"#,
        );
        let MappedOutcome::Mapped(items) = map_curse_stat(
            &catalog,
            "DespairPlayer",
            None,
            "base_skill_buff_chaos_damage_resistance_%_to_apply",
            -35.0,
        ) else {
            panic!("期望 Mapped");
        };
        assert_eq!(items.len(), 1);
        let MappedItem::Modifier(m) = &items[0] else {
            panic!("期望 Modifier");
        };
        assert_eq!(m.name.as_str(), "ChaosResist");
        assert_eq!(m.mod_type, ModType::Base);
        assert_eq!(m.value.as_number(), Some(-35.0));
        assert!(m.tags.is_empty(), "GlobalEffect 路由 tag 已剥除");
    }

    /// Elemental Weakness 形态：`ElementalResist` 展开火/冰/电三条（pobr 敌侧
    /// 聚合只读 `<Type>Resist`，与 vendor 双名同收等值）。
    #[test]
    fn curse_elemental_resist_expands_to_three_types() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "ElementalWeaknessPlayer": { "1": {
                 "base_skill_buff_all_elements_resistance_%_to_apply": {
                   "mods": [ { "kind": "mod", "name": "ElementalResist", "mod_type": "BASE",
                               "tags": [ { "type": "GlobalEffect", "effectType": "Curse" } ] } ] } } } } }"#,
        );
        let MappedOutcome::Mapped(items) = map_curse_stat(
            &catalog,
            "ElementalWeaknessPlayer",
            None,
            "base_skill_buff_all_elements_resistance_%_to_apply",
            -59.0,
        ) else {
            panic!("期望 Mapped");
        };
        let names: Vec<&str> = items
            .iter()
            .map(|i| match i {
                MappedItem::Modifier(m) => m.name.as_str(),
                other => panic!("期望 Modifier，得到 {other:?}"),
            })
            .collect();
        assert_eq!(names, vec!["FireResist", "ColdResist", "LightningResist"]);
    }

    /// Enfeeble 形态：`Damage` MORE + Condition(Unique[, neg]) 直译保留
    /// （curse mod 落 enemy db，var 即敌方自身状态，不加 Enemy 前缀）。
    #[test]
    fn curse_damage_more_keeps_condition_tags() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "EnfeeblePlayer": { "1": {
                 "base_skill_buff_damage_+%_final_to_apply": {
                   "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "MORE",
                               "tags": [ { "type": "GlobalEffect", "effectType": "Curse" },
                                         { "type": "Condition", "var": "Unique", "neg": true } ] } ] } } } } }"#,
        );
        let MappedOutcome::Mapped(items) = map_curse_stat(
            &catalog,
            "EnfeeblePlayer",
            None,
            "base_skill_buff_damage_+%_final_to_apply",
            -21.0,
        ) else {
            panic!("期望 Mapped");
        };
        let MappedItem::Modifier(m) = &items[0] else {
            panic!("期望 Modifier");
        };
        assert_eq!(m.name.as_str(), "Damage");
        assert_eq!(m.mod_type, ModType::More);
        assert_eq!(m.value.as_number(), Some(-21.0));
        assert_eq!(
            m.tags,
            vec![crate::ModTag::condition("Unique", true)],
            "Condition Unique(neg) 直译保留，GlobalEffect 剥除"
        );
    }

    /// 未知敌侧名（pobr 暂无消费方，如 TemporalChainsActionSpeed）→
    /// Unsupported(UnknownModName) 上报（可见性，不静默注入）。
    #[test]
    fn curse_unknown_enemy_name_is_reported() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "TemporalChainsPlayer": { "1": {
                 "base_skill_debuff_action_speed_+%_final_to_inflict": {
                   "mods": [ { "kind": "mod", "name": "TemporalChainsActionSpeed", "mod_type": "INC",
                               "tags": [ { "type": "GlobalEffect", "effectType": "Curse" } ] } ] } } } } }"#,
        );
        assert_eq!(
            map_curse_stat(
                &catalog,
                "TemporalChainsPlayer",
                None,
                "base_skill_debuff_action_speed_+%_final_to_inflict",
                -25.0,
            ),
            MappedOutcome::Unsupported(UnsupportedReason::UnknownModName(
                "TemporalChainsActionSpeed".into()
            ))
        );
    }

    /// Temporal Chains 第二载荷（M4-l 允收）：`BuffExpireFaster MORE` 负值
    /// （expire slower）直通敌侧同名（消费方 = `ailment::debuff_duration_mult`，
    /// CalcOffence.lua:1833-1835 / :5040）。
    #[test]
    fn curse_buff_expire_faster_maps_to_enemy_name() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "TemporalChainsPlayer": { "1": {
                 "base_temporal_chains_other_buff_time_passed_+%_to_apply": {
                   "mods": [ { "kind": "mod", "name": "BuffExpireFaster", "mod_type": "MORE",
                               "tags": [ { "type": "GlobalEffect", "effectType": "Curse" } ] } ] } } } } }"#,
        );
        let MappedOutcome::Mapped(items) = map_curse_stat(
            &catalog,
            "TemporalChainsPlayer",
            None,
            "base_temporal_chains_other_buff_time_passed_+%_to_apply",
            -25.0,
        ) else {
            panic!("期望 Mapped");
        };
        assert_eq!(items.len(), 1);
        let MappedItem::Modifier(m) = &items[0] else {
            panic!("期望 Modifier");
        };
        assert_eq!(m.name.as_str(), "BuffExpireFaster");
        assert_eq!(m.mod_type, ModType::More);
        assert_eq!(m.value.as_number(), Some(-25.0));
        assert!(m.tags.is_empty(), "GlobalEffect 路由 tag 已剥除");
    }

    /// 非 curse 载荷（global 条目无 Curse tag，如技能自带 duration/AoE）→
    /// Mapped(空)：静默跳过（走主技能注入通道，非 Unsupported）。
    #[test]
    fn non_curse_payload_yields_empty_mapped() {
        let catalog = catalog_json(
            r#"{ "global": { "base_skill_effect_duration": {
                   "div": 1000.0,
                   "mods": [ { "kind": "skill_data", "value": { "key": "duration" } } ] } } }"#,
        );
        assert_eq!(
            map_curse_stat(
                &catalog,
                "DespairPlayer",
                None,
                "base_skill_effect_duration",
                6000.0,
            ),
            MappedOutcome::Mapped(Vec::new())
        );
        // catalog miss → Unknown（与 map_stat 同口径）。
        assert_eq!(
            map_curse_stat(&catalog, "DespairPlayer", None, "no_such_stat", 1.0),
            MappedOutcome::Unknown
        );
    }

    /// 曝光施加能力载荷存在性（M4-m）：`flag("InflictExposure", …)`（Fire
    /// Exposure `inflict_exposure_for_x_ms_on_ignite`，SkillStatMap.lua:1701-1703，
    /// 门控 tag 不参与判定）与 `<El>ExposureChance BASE`（:1689-1690）均命中；
    /// 普通载荷 / catalog miss → 不存在。
    #[test]
    fn has_exposure_inflict_payload_matches_flag_and_chance() {
        let catalog = catalog_json(
            r#"{ "global": {
                 "inflict_exposure_for_x_ms_on_ignite": {
                   "mods": [ { "kind": "mod", "name": "ExposureDuration", "mod_type": "BASE",
                               "tags": [ { "type": "ActorCondition", "actor": "enemy", "var": "Ignited" } ] },
                             { "kind": "flag", "name": "InflictExposure", "mod_type": "FLAG", "value": true,
                               "tags": [ { "type": "ActorCondition", "actor": "enemy", "var": "Ignited" } ] } ] },
                 "base_inflict_fire_exposure_on_hit_%_chance": {
                   "mods": [ { "kind": "mod", "name": "FireExposureChance", "mod_type": "BASE" } ] },
                 "plain_damage": {
                   "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "INC" } ] } },
                 "per_stat_set": {} }"#,
        );
        assert!(has_exposure_inflict_payload(
            &catalog,
            "X",
            None,
            "inflict_exposure_for_x_ms_on_ignite"
        ));
        assert!(has_exposure_inflict_payload(
            &catalog,
            "X",
            None,
            "base_inflict_fire_exposure_on_hit_%_chance"
        ));
        assert!(!has_exposure_inflict_payload(
            &catalog,
            "X",
            None,
            "plain_damage"
        ));
        assert!(!has_exposure_inflict_payload(
            &catalog,
            "X",
            None,
            "no_such_stat"
        ));
    }

    /// curse 载荷**存在性**判定（vendor buffList 注册前置，CalcActiveSkill.lua:976-1041）：
    /// 允收名单外的载荷（TemporalChainsActionSpeed / Dummy 占位）仍算存在
    /// （vendor 同样入槽计数）；非 curse 条目 / catalog miss → 不存在。
    #[test]
    fn has_curse_payload_is_presence_not_translatability() {
        let catalog = catalog_json(
            r#"{ "global": { "base_skill_effect_duration": {
                   "div": 1000.0,
                   "mods": [ { "kind": "skill_data", "value": { "key": "duration" } } ] } },
                 "per_stat_set": { "TemporalChainsPlayer": { "1": {
                 "base_skill_debuff_action_speed_+%_final_to_inflict": {
                   "mods": [ { "kind": "mod", "name": "TemporalChainsActionSpeed", "mod_type": "INC",
                               "tags": [ { "type": "GlobalEffect", "effectType": "Curse" } ] } ] } } } } }"#,
        );
        // 允收名单外（map_curse_stat 会 Unsupported）但载荷存在 → true。
        assert!(has_curse_payload(
            &catalog,
            "TemporalChainsPlayer",
            None,
            "base_skill_debuff_action_speed_+%_final_to_inflict",
        ));
        // 非 curse 条目（global duration，Repulsion 全部 stat 的形态）→ false。
        assert!(!has_curse_payload(
            &catalog,
            "CurseOfRepulsionPlayer",
            None,
            "base_skill_effect_duration",
        ));
        // catalog miss → false。
        assert!(!has_curse_payload(
            &catalog,
            "CurseOfRepulsionPlayer",
            None,
            "no_such_stat",
        ));
    }

    /// GlobalEffect 带约定外键（effectCond 等额外门控语义）→ 整条 Unsupported。
    #[test]
    fn curse_global_effect_extra_keys_unsupported() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "X": { "1": {
                 "some_stat": {
                   "mods": [ { "kind": "mod", "name": "ChaosResist", "mod_type": "BASE",
                               "tags": [ { "type": "GlobalEffect", "effectType": "Curse",
                                           "effectCond": "Stationary" } ] } ] } } } } }"#,
        );
        assert!(matches!(
            map_curse_stat(&catalog, "X", Some("1"), "some_stat", -10.0),
            MappedOutcome::Unsupported(UnsupportedReason::UnsupportedTag(_))
        ));
    }

    /// Frost Bomb 形态（SkillStatMap.lua:1721-1725）：
    /// `active_skill_all_elemental_exposure_magnitude` → 三元素
    /// `<El>Exposure BASE`（GlobalEffect Debuff 剥除，敌侧曝光归约消费）。
    #[test]
    fn debuff_exposure_magnitude_maps_three_elements() {
        let catalog = catalog_json(
            r#"{ "global": { "active_skill_all_elemental_exposure_magnitude": {
                 "mods": [
                   { "kind": "mod", "name": "FireExposure", "mod_type": "BASE",
                     "tags": [ { "type": "GlobalEffect", "effectType": "Debuff" } ] },
                   { "kind": "mod", "name": "ColdExposure", "mod_type": "BASE",
                     "tags": [ { "type": "GlobalEffect", "effectType": "Debuff" } ] },
                   { "kind": "mod", "name": "LightningExposure", "mod_type": "BASE",
                     "tags": [ { "type": "GlobalEffect", "effectType": "Debuff" } ] } ] } } }"#,
        );
        let MappedOutcome::Mapped(items) = map_debuff_stat(
            &catalog,
            "FrostBombPlayer",
            None,
            "active_skill_all_elemental_exposure_magnitude",
            20.0,
        ) else {
            panic!("期望 Mapped");
        };
        let mods: Vec<(&str, ModType, Option<f64>)> = items
            .iter()
            .map(|i| match i {
                MappedItem::Modifier(m) => (m.name.as_str(), m.mod_type, m.value.as_number()),
                other => panic!("期望 Modifier，得到 {other:?}"),
            })
            .collect();
        assert_eq!(
            mods,
            vec![
                ("FireExposure", ModType::Base, Some(20.0)),
                ("ColdExposure", ModType::Base, Some(20.0)),
                ("LightningExposure", ModType::Base, Some(20.0)),
            ]
        );
        for item in &items {
            let MappedItem::Modifier(m) = item else {
                unreachable!()
            };
            assert!(m.tags.is_empty(), "GlobalEffect Debuff tag 剥除");
        }
    }

    /// 未知敌侧 debuff 名（允收名单外）→ Unsupported(UnknownModName) 上报；
    /// 非 debuff 载荷（无 Debuff tag）→ Mapped(空) 静默跳过（走主技能通道）。
    #[test]
    fn debuff_unknown_name_reported_and_non_debuff_empty() {
        let catalog = catalog_json(
            r#"{ "global": {
                 "hinder_debuff_movement_speed_+%": {
                   "mods": [ { "kind": "mod", "name": "MovementSpeed", "mod_type": "INC",
                               "tags": [ { "type": "GlobalEffect", "effectType": "Debuff" } ] } ] },
                 "spell_minimum_base_cold_damage": {
                   "mods": [ { "kind": "mod", "name": "ColdMin", "mod_type": "BASE" } ] } } }"#,
        );
        assert_eq!(
            map_debuff_stat(
                &catalog,
                "X",
                None,
                "hinder_debuff_movement_speed_+%",
                -30.0
            ),
            MappedOutcome::Unsupported(UnsupportedReason::UnknownModName("MovementSpeed".into()))
        );
        assert_eq!(
            map_debuff_stat(&catalog, "X", None, "spell_minimum_base_cold_damage", 7.0),
            MappedOutcome::Mapped(Vec::new())
        );
        assert_eq!(
            map_debuff_stat(&catalog, "X", None, "no_such_stat", 1.0),
            MappedOutcome::Unknown
        );
    }

    /// Precision II 形态（sup_dex.lua:4216-4250）：`Accuracy INC` + GlobalEffect
    /// Buff（含 effectName 键，无门控语义）→ 玩家侧 Accuracy INC，tag 剥净。
    #[test]
    fn player_buff_precision_accuracy_inc_maps() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "SupportPrecisionPlayerTwo": { "1": {
                 "support_precision_accuracy_rating_+%": {
                   "mods": [ { "kind": "mod", "name": "Accuracy", "mod_type": "INC",
                               "tags": [ { "type": "GlobalEffect", "effectType": "Buff",
                                           "effectName": "Precision II" } ] } ] } } } } }"#,
        );
        let MappedOutcome::Mapped(items) = map_player_buff_stat(
            &catalog,
            "SupportPrecisionPlayerTwo",
            None,
            "support_precision_accuracy_rating_+%",
            50.0,
        ) else {
            panic!("期望 Mapped");
        };
        assert_eq!(items.len(), 1);
        let MappedItem::Modifier(m) = &items[0] else {
            panic!("期望 Modifier");
        };
        assert_eq!(m.name.as_str(), "Accuracy");
        assert_eq!(m.mod_type, ModType::Inc);
        assert_eq!(m.value.as_number(), Some(50.0));
        assert!(m.tags.is_empty(), "GlobalEffect（含 effectName）剥除");
    }

    /// War Banner 形态（GlobalEffect effectType=Aura + Condition BannerPlanted）：
    /// Aura 类玩家侧 buff 同收，Condition tag 直译保留。
    #[test]
    fn player_buff_banner_accuracy_keeps_condition() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "WarBannerPlayer": { "1": {
                 "base_skill_buff_banner_accuracy_+%_to_apply": {
                   "mods": [ { "kind": "mod", "name": "Accuracy", "mod_type": "INC",
                               "tags": [ { "type": "Condition", "var": "BannerPlanted" },
                                         { "type": "GlobalEffect", "effectType": "Aura" } ] } ] } } } } }"#,
        );
        let MappedOutcome::Mapped(items) = map_player_buff_stat(
            &catalog,
            "WarBannerPlayer",
            None,
            "base_skill_buff_banner_accuracy_+%_to_apply",
            130.0,
        ) else {
            panic!("期望 Mapped");
        };
        let MappedItem::Modifier(m) = &items[0] else {
            panic!("期望 Modifier");
        };
        assert_eq!(m.name.as_str(), "Accuracy");
        assert_eq!(m.value.as_number(), Some(130.0));
        assert_eq!(
            m.tags,
            vec![crate::ModTag::condition("BannerPlanted", false)],
            "Condition 直译保留，GlobalEffect 剥除"
        );
    }

    /// Clarity II 形态（vendor sup_int.txt:305-315：`support_clarity_mana_
    /// regeneration_rate_+%` → `ManaRegen INC` + GlobalEffect Buff effectName
    /// "Clarity II"）→ 玩家侧 ManaRegen INC，tag 剥净。oracle 钉值
    /// （pob2-oracle sorceress-stormweaver-comet）：Clarity II lv1 载荷 50 +
    /// 其余裸名 INC 25 → calcsOutput.ManaRegenInc = 75。
    #[test]
    fn player_buff_clarity_mana_regen_inc_maps() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "SupportClarityPlayerTwo": { "1": {
                 "support_clarity_mana_regeneration_rate_+%": {
                   "mods": [ { "kind": "mod", "name": "ManaRegen", "mod_type": "INC",
                               "tags": [ { "type": "GlobalEffect", "effectType": "Buff",
                                           "effectName": "Clarity II" } ] } ] } } } } }"#,
        );
        let MappedOutcome::Mapped(items) = map_player_buff_stat(
            &catalog,
            "SupportClarityPlayerTwo",
            None,
            "support_clarity_mana_regeneration_rate_+%",
            50.0,
        ) else {
            panic!("期望 Mapped");
        };
        assert_eq!(items.len(), 1);
        let MappedItem::Modifier(m) = &items[0] else {
            panic!("期望 Modifier");
        };
        assert_eq!(m.name.as_str(), "ManaRegen");
        assert_eq!(m.mod_type, ModType::Inc);
        assert_eq!(m.value.as_number(), Some(50.0));
        assert!(m.tags.is_empty(), "GlobalEffect（含 effectName）剥除");
    }

    /// Vitality II 形态（vendor sup_str.txt:1791-1802：`support_vitality_life_
    /// regeneration_rate_per_minute_%` div=60 → `LifeRegenPercent BASE`）→
    /// 每分钟 % 折每秒（120/60 = 2.0）。oracle 钉值（pob2-oracle
    /// warrior-smith-of-kitava-shield-wall）：Vitality II 2.0 + 其余 6.1 →
    /// calcsOutput.LifeRegenPercent = 8.1。
    #[test]
    fn player_buff_vitality_life_regen_percent_div60() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "SupportVitalityPlayerTwo": { "1": {
                 "support_vitality_life_regeneration_rate_per_minute_%": {
                   "div": 60.0,
                   "mods": [ { "kind": "mod", "name": "LifeRegenPercent", "mod_type": "BASE",
                               "tags": [ { "type": "GlobalEffect", "effectType": "Buff",
                                           "effectName": "Vitality II" } ] } ] } } } } }"#,
        );
        let MappedOutcome::Mapped(items) = map_player_buff_stat(
            &catalog,
            "SupportVitalityPlayerTwo",
            None,
            "support_vitality_life_regeneration_rate_per_minute_%",
            120.0,
        ) else {
            panic!("期望 Mapped");
        };
        assert_eq!(items.len(), 1);
        let MappedItem::Modifier(m) = &items[0] else {
            panic!("期望 Modifier");
        };
        assert_eq!(m.name.as_str(), "LifeRegenPercent");
        assert_eq!(m.mod_type, ModType::Base);
        assert_eq!(m.value.as_number(), Some(2.0), "120/min ÷ 60 = 2.0/s");
        assert!(m.tags.is_empty());
    }

    /// 玩家侧允收名单外的名（如 AttackSpeed）→ Unsupported(UnknownModName) 上报；
    /// 非 buff 载荷（无 GlobalEffect Buff/Aura tag）→ Mapped(空) 静默跳过。
    #[test]
    fn player_buff_unknown_name_reported_and_non_buff_empty() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "X": { "1": {
                 "buff_attack_speed": {
                   "mods": [ { "kind": "mod", "name": "AttackSpeed", "mod_type": "INC",
                               "tags": [ { "type": "GlobalEffect", "effectType": "Buff" } ] } ] },
                 "local_damage": {
                   "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "INC" } ] } } } } }"#,
        );
        assert_eq!(
            map_player_buff_stat(&catalog, "X", Some("1"), "buff_attack_speed", 10.0),
            MappedOutcome::Unsupported(UnsupportedReason::UnknownModName("AttackSpeed".into()))
        );
        assert_eq!(
            map_player_buff_stat(&catalog, "X", Some("1"), "local_damage", 10.0),
            MappedOutcome::Mapped(Vec::new())
        );
    }

    /// Pinnacle of Power 形态（M4-n，other.lua:12503 `elemental_power_elemental_
    /// damage_+%_final_per_power_charge`）：首元素 `Damage MORE` 带 scalar
    /// Multiplier（边界外，零注入）**不连坐**同条目六枚 `<El>Can<Ailment>` flag
    /// ——逐元素独立处置后 flag 载荷全部产出、GlobalEffect tag 剥净。
    /// stormweaver-comet IgniteDPS 1911 的跨类型通行证（oracle 钉源
    /// `Skill:PinnacleOfPowerPlayer`，m4-skill-gaps.md §7.4）。
    #[test]
    fn player_buff_pinnacle_flags_survive_scalar_sibling() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "PinnacleOfPowerPlayer": { "1": {
                 "elemental_power_elemental_damage_+%_final_per_power_charge": {
                   "mods": [
                     { "kind": "mod", "name": "Damage", "mod_type": "MORE",
                       "tags": [ { "type": "SkillType", "skillTypeList": ["Cold","Fire","Lightning"] },
                                 { "type": "Multiplier", "var": "RemovablePowerCharge",
                                   "scalar": "ConsumedPowerChargeEffect" },
                                 { "type": "GlobalEffect", "effectType": "Buff" } ] },
                     { "kind": "flag", "name": "ColdCanIgnite", "mod_type": "FLAG", "value": true,
                       "tags": [ { "type": "GlobalEffect", "effectType": "Buff" } ] },
                     { "kind": "flag", "name": "ColdCanShock", "mod_type": "FLAG", "value": true,
                       "tags": [ { "type": "GlobalEffect", "effectType": "Buff" } ] },
                     { "kind": "flag", "name": "FireCanFreeze", "mod_type": "FLAG", "value": true,
                       "tags": [ { "type": "GlobalEffect", "effectType": "Buff" } ] },
                     { "kind": "flag", "name": "FireCanShock", "mod_type": "FLAG", "value": true,
                       "tags": [ { "type": "GlobalEffect", "effectType": "Buff" } ] },
                     { "kind": "flag", "name": "LightningCanFreeze", "mod_type": "FLAG", "value": true,
                       "tags": [ { "type": "GlobalEffect", "effectType": "Buff" } ] },
                     { "kind": "flag", "name": "LightningCanIgnite", "mod_type": "FLAG", "value": true,
                       "tags": [ { "type": "GlobalEffect", "effectType": "Buff" } ] } ] } } } } }"#,
        );
        let MappedOutcome::Mapped(items) = map_player_buff_stat(
            &catalog,
            "PinnacleOfPowerPlayer",
            None,
            "elemental_power_elemental_damage_+%_final_per_power_charge",
            15.0,
        ) else {
            panic!("期望 Mapped（scalar 元素不连坐 flag）");
        };
        let flags: Vec<(&str, ModType)> = items
            .iter()
            .map(|item| {
                let MappedItem::Modifier(m) = item else {
                    panic!("期望 Modifier");
                };
                assert!(m.tags.is_empty(), "GlobalEffect Buff tag 剥除");
                (m.name.as_str(), m.mod_type)
            })
            .collect();
        assert_eq!(
            flags,
            vec![
                ("ColdCanIgnite", ModType::Flag),
                ("ColdCanShock", ModType::Flag),
                ("FireCanFreeze", ModType::Flag),
                ("FireCanShock", ModType::Flag),
                ("LightningCanFreeze", ModType::Flag),
                ("LightningCanIgnite", ModType::Flag),
            ],
            "六枚 <El>Can<Ailment> flag 全部产出；scalar Damage MORE 零注入"
        );
    }

    /// `<Type>Can<Ailment>` 族名单外的 flag（如行为开关 projectile）维持未知名
    /// 上报；全部命中元素失败（零注入）时 Unsupported 可见性不退化。
    #[test]
    fn player_buff_flag_outside_family_reported() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "X": { "1": {
                 "some_behaviour_stat": {
                   "mods": [ { "kind": "flag", "name": "projectile", "mod_type": "FLAG",
                               "value": true,
                               "tags": [ { "type": "GlobalEffect", "effectType": "Buff" } ] } ] } } } } }"#,
        );
        assert_eq!(
            map_player_buff_stat(&catalog, "X", Some("1"), "some_behaviour_stat", 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::UnknownModName("flag:projectile".into()))
        );
    }

    /// Sigil of Power 形态（M4-n；vendor other.lua SigilOfPowerPlayer statMap
    /// `circle_of_power_spell_damage_+%_final_per_stage`）：Damage MORE +
    /// Spell flag + Multiplier{var=SigilOfPowerStage, limitVar=
    /// SigilOfPowerMaxStages}。oracle 钉值（varashta）：eff level 32 →
    /// per-stage 17，stage=1 → modDB `Damage MORE 17 | Skill:SigilOfPowerPlayer`。
    #[test]
    fn player_buff_sigil_per_stage_damage_more_with_limit_var() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "SigilOfPowerPlayer": { "1": {
                 "circle_of_power_spell_damage_+%_final_per_stage": {
                   "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "MORE",
                               "flags": [ "Spell" ],
                               "tags": [ { "type": "GlobalEffect", "effectType": "Buff" },
                                         { "type": "Multiplier", "var": "SigilOfPowerStage",
                                           "limitVar": "SigilOfPowerMaxStages" } ] } ] } } } } }"#,
        );
        let MappedOutcome::Mapped(items) = map_player_buff_stat(
            &catalog,
            "SigilOfPowerPlayer",
            None,
            "circle_of_power_spell_damage_+%_final_per_stage",
            17.0,
        ) else {
            panic!("期望 Mapped");
        };
        let MappedItem::Modifier(m) = &items[0] else {
            panic!("期望 Modifier");
        };
        assert_eq!(m.name.as_str(), "Damage");
        assert_eq!(m.mod_type, ModType::More);
        assert_eq!(m.value.as_number(), Some(17.0));
        assert_eq!(m.flags, ModFlags::SPELL, "Spell flag 直译");
        assert_eq!(
            m.tags,
            vec![crate::ModTag::Multiplier {
                var: "SigilOfPowerStage".into(),
                div: 1.0,
                limit: None,
                actor: None,
                limit_var: Some("SigilOfPowerMaxStages".into()),
                limit_actor: None,
                invert: false,
                limit_total: false,
            }],
            "Multiplier limitVar 直译，GlobalEffect 剥除"
        );
        // 求值：stage=1、maxStages=4 → ×1；stage=9 被 maxStages 封到 4。
        let cfg = crate::CalcConfig::new()
            .with_multiplier("SigilOfPowerStage", 1.0)
            .with_multiplier("SigilOfPowerMaxStages", 4.0);
        assert_eq!(m.effective_number(&cfg), Some(17.0));
        let cfg9 = crate::CalcConfig::new()
            .with_multiplier("SigilOfPowerStage", 9.0)
            .with_multiplier("SigilOfPowerMaxStages", 4.0);
        assert_eq!(m.effective_number(&cfg9), Some(68.0));
    }

    /// Refraction II 形态（vendor sup_str.lua:6023-6025：`support_tempered_
    /// valour_deflection_rating_%_of_evasion_rating` → `EvasionGainAsDeflection
    /// BASE 20` + GlobalEffect Buff "Refractive Plating" + MultiplierThreshold
    /// ValourStacks/thresholdVar=RefractionMinimumValour）→ 玩家侧 BASE，
    /// threshold 静态折 0（该 var 全 vendor 树零 setter），默认 cfg 下生效。
    /// oracle 钉值（wolf-pack）：tree 28 + 本载荷 20 = 48%，rating 11791.52。
    #[test]
    fn player_buff_refraction_evasion_gain_as_deflection_maps() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "SupportRefractionPlayerTwo": { "1": {
                 "support_tempered_valour_deflection_rating_%_of_evasion_rating": {
                   "mods": [ { "kind": "mod", "name": "EvasionGainAsDeflection",
                               "mod_type": "BASE",
                               "tags": [ { "type": "GlobalEffect", "effectType": "Buff",
                                           "effectName": "Refractive Plating" },
                                         { "type": "MultiplierThreshold", "var": "ValourStacks",
                                           "thresholdVar": "RefractionMinimumValour" } ] } ] } } } } }"#,
        );
        let MappedOutcome::Mapped(items) = map_player_buff_stat(
            &catalog,
            "SupportRefractionPlayerTwo",
            None,
            "support_tempered_valour_deflection_rating_%_of_evasion_rating",
            20.0,
        ) else {
            panic!("期望 Mapped");
        };
        let MappedItem::Modifier(m) = &items[0] else {
            panic!("期望 Modifier");
        };
        assert_eq!(m.name.as_str(), "EvasionGainAsDeflection");
        assert_eq!(m.mod_type, ModType::Base);
        assert_eq!(m.value.as_number(), Some(20.0));
        assert_eq!(
            m.tags,
            vec![crate::ModTag::MultiplierThreshold {
                var: "ValourStacks".into(),
                threshold: 0.0,
                upper: false,
            }],
            "thresholdVar 静态折 0，GlobalEffect 剥除"
        );
        // 默认 cfg（ValourStacks 未注入 = 0）：0 ≥ 0 → 生效，与 vendor 默认一致。
        assert!(m.matches(&crate::CalcConfig::new()));
    }

    /// 同 buff 的护甲折算载荷（vendor sup_str.lua:6019-6021：`support_tempered_
    /// valour_%_armour_to_apply_to_elemental_damage` → ArmourAppliesTo{Fire,Cold,
    /// Lightning}DamageTaken BASE 30，tag 形态与 deflection 载荷同）→ 三条玩家侧
    /// BASE，消费方 `calc::taken::armour_applies_pct`。oracle 钉值（wolf-pack）：
    /// tree 84 + 本载荷 30 = 114%，FireEffectiveAppliedArmour 21181.2。
    #[test]
    fn player_buff_refraction_armour_applies_to_elements_maps() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "SupportRefractionPlayerTwo": { "1": {
                 "support_tempered_valour_%_armour_to_apply_to_elemental_damage": {
                   "mods": [ { "kind": "mod", "name": "ArmourAppliesToFireDamageTaken",
                               "mod_type": "BASE",
                               "tags": [ { "type": "GlobalEffect", "effectType": "Buff",
                                           "effectName": "Refractive Plating" },
                                         { "type": "MultiplierThreshold", "var": "ValourStacks",
                                           "thresholdVar": "RefractionMinimumValour" } ] },
                             { "kind": "mod", "name": "ArmourAppliesToColdDamageTaken",
                               "mod_type": "BASE",
                               "tags": [ { "type": "GlobalEffect", "effectType": "Buff",
                                           "effectName": "Refractive Plating" },
                                         { "type": "MultiplierThreshold", "var": "ValourStacks",
                                           "thresholdVar": "RefractionMinimumValour" } ] },
                             { "kind": "mod", "name": "ArmourAppliesToLightningDamageTaken",
                               "mod_type": "BASE",
                               "tags": [ { "type": "GlobalEffect", "effectType": "Buff",
                                           "effectName": "Refractive Plating" },
                                         { "type": "MultiplierThreshold", "var": "ValourStacks",
                                           "thresholdVar": "RefractionMinimumValour" } ] } ] } } } } }"#,
        );
        let MappedOutcome::Mapped(items) = map_player_buff_stat(
            &catalog,
            "SupportRefractionPlayerTwo",
            None,
            "support_tempered_valour_%_armour_to_apply_to_elemental_damage",
            30.0,
        ) else {
            panic!("期望 Mapped");
        };
        let names: Vec<&str> = items
            .iter()
            .map(|item| {
                let MappedItem::Modifier(m) = item else {
                    panic!("期望 Modifier");
                };
                assert_eq!(m.mod_type, ModType::Base);
                assert_eq!(m.value.as_number(), Some(30.0));
                // 默认 cfg（ValourStacks 未注入 = 0）：0 ≥ 0 → 生效。
                assert!(m.matches(&crate::CalcConfig::new()));
                m.name.as_str()
            })
            .collect();
        assert_eq!(
            names,
            vec![
                "ArmourAppliesToFireDamageTaken",
                "ArmourAppliesToColdDamageTaken",
                "ArmourAppliesToLightningDamageTaken",
            ]
        );
    }

    /// MultiplierThreshold thresholdVar 有 setter（Attrition
    /// `AttritionCullSeconds`，act_str.lua:1258 设 Multiplier）→ 静态折 0 会
    /// 错开 gate，维持 Unsupported 整条上报。
    #[test]
    fn player_buff_threshold_var_with_setter_reported() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "AttritionPlayer": { "1": {
                 "attrition_cull_payload": {
                   "mods": [ { "kind": "mod", "name": "EvasionGainAsDeflection",
                               "mod_type": "BASE",
                               "tags": [ { "type": "GlobalEffect", "effectType": "Buff" },
                                         { "type": "MultiplierThreshold", "var": "EnemyPresenceSeconds",
                                           "thresholdVar": "AttritionCullSeconds" } ] } ] } } } } }"#,
        );
        assert!(matches!(
            map_player_buff_stat(
                &catalog,
                "AttritionPlayer",
                None,
                "attrition_cull_payload",
                1.0,
            ),
            MappedOutcome::Unsupported(UnsupportedReason::UnsupportedTag(_))
        ));
    }

    /// Sigil of Power 最大层数载荷（`circle_of_power_max_stages` →
    /// `Multiplier:SigilOfPowerMaxStages` BASE，GlobalEffect 带 unscalable
    /// 标记——允收通过；编排层把该 BASE 桥进 cfg.multipliers 作 limitVar 分母）。
    #[test]
    fn player_buff_sigil_max_stages_multiplier_base() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "SigilOfPowerPlayer": { "1": {
                 "circle_of_power_max_stages": {
                   "mods": [ { "kind": "mod", "name": "Multiplier:SigilOfPowerMaxStages",
                               "mod_type": "BASE",
                               "tags": [ { "type": "GlobalEffect", "effectType": "Buff",
                                           "unscalable": true } ] } ] } } } } }"#,
        );
        let MappedOutcome::Mapped(items) = map_player_buff_stat(
            &catalog,
            "SigilOfPowerPlayer",
            None,
            "circle_of_power_max_stages",
            4.0,
        ) else {
            panic!("期望 Mapped");
        };
        let MappedItem::Modifier(m) = &items[0] else {
            panic!("期望 Modifier");
        };
        assert_eq!(m.name.as_str(), "Multiplier:SigilOfPowerMaxStages");
        assert_eq!(m.mod_type, ModType::Base);
        assert_eq!(m.value.as_number(), Some(4.0));
        assert!(m.tags.is_empty(), "GlobalEffect（含 unscalable 键）剥除");
    }

    /// Elemental Conflux 形态（M4-n；vendor SkillStatMap
    /// `skill_elemental_conflux_active_element_damage_+%_final`）：三元素
    /// MORE，各带 invert Multiplier（`ElementalConflux<El>Effect`，config
    /// Average 档 = 3 → ×1/3 均摊；锁单元素档 = 1/0 → ×1/×0）。oracle 钉值
    /// （varashta）：73 → 三条 `<El>Damage MORE 24.33`。
    #[test]
    fn player_buff_conflux_inverted_element_more() {
        let catalog = catalog_json(
            r#"{ "global": {}, "per_stat_set": { "ElementalConfluxPlayer": { "1": {
                 "skill_elemental_conflux_active_element_damage_+%_final": {
                   "mods": [
                     { "kind": "mod", "name": "LightningDamage", "mod_type": "MORE",
                       "tags": [ { "type": "GlobalEffect", "effectType": "Buff",
                                   "effectName": "Elemental Conflux" },
                                 { "type": "Multiplier",
                                   "var": "ElementalConfluxLightningEffect",
                                   "invert": true } ] },
                     { "kind": "mod", "name": "ColdDamage", "mod_type": "MORE",
                       "tags": [ { "type": "GlobalEffect", "effectType": "Buff",
                                   "effectName": "Elemental Conflux" },
                                 { "type": "Multiplier",
                                   "var": "ElementalConfluxColdEffect",
                                   "invert": true } ] } ] } } } } }"#,
        );
        let MappedOutcome::Mapped(items) = map_player_buff_stat(
            &catalog,
            "ElementalConfluxPlayer",
            None,
            "skill_elemental_conflux_active_element_damage_+%_final",
            73.0,
        ) else {
            panic!("期望 Mapped");
        };
        assert_eq!(items.len(), 2);
        let MappedItem::Modifier(lightning) = &items[0] else {
            panic!("期望 Modifier");
        };
        assert_eq!(lightning.name.as_str(), "LightningDamage");
        assert_eq!(lightning.mod_type, ModType::More);
        assert_eq!(lightning.value.as_number(), Some(73.0));
        // Average 档：multiplier 3 → invert ×1/3 = 24.33（vendor Tabulate 同值）。
        let avg = crate::CalcConfig::new().with_multiplier("ElementalConfluxLightningEffect", 3.0);
        let v = lightning.effective_number(&avg).expect("数值");
        assert!((v - 73.0 / 3.0).abs() < 1e-9, "invert 1/3 均摊，得 {v}");
        // 锁定别的元素：multiplier 0 → invert 保持 0 → 该元素载荷为 0。
        let locked =
            crate::CalcConfig::new().with_multiplier("ElementalConfluxLightningEffect", 0.0);
        assert_eq!(lightning.effective_number(&locked), Some(0.0));
    }

    /// Unsupported 分类标签稳定（双跑报告聚合键）。
    #[test]
    fn unsupported_categories_are_stable() {
        assert_eq!(UnsupportedReason::Unextractable.category(), "unextractable");
        assert_eq!(UnsupportedReason::ScalarMultiplier.category(), "scalar");
        assert_eq!(
            UnsupportedReason::UnknownModName(String::new()).category(),
            "unknown_mod_name"
        );
        assert_eq!(
            UnsupportedReason::UnsupportedTag(String::new()).category(),
            "tag"
        );
    }
}
