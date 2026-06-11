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
//! - **flag 构造器**（vendor `flag(name)`，技能行为开关如 `projectile`）：PoBR
//!   当前无消费方，第一批全部 Unsupported。

use std::collections::BTreeMap;

use pobr_data::catalog::stat_map::{SkillStatMapDef, StatMapEntry, StatMapMod, StatMapValue};
use pobr_data::modifier::ModType;

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
    fn lookup(&self, effect_id: &str, set_key: Option<&str>, stat: &str) -> Option<&StatMapEntry> {
        if let Some(key) = set_key
            && let Some(entry) = self
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
            // vendor flag(name) = 技能行为开关（projectile / unarmedMelee…）；PoBR
            // 第一批无消费方 → 未知名上报（与 mod 的未知名同分类，diff 驱动补全）。
            Err(UnsupportedReason::UnknownModName(format!(
                "flag:{}",
                element.name.as_deref().unwrap_or("?")
            )))
        }
        "skill_data" => collect_skill_data(element, params.merge(stat_value), items),
        other => Err(UnsupportedReason::UnsupportedKind(other.to_string())),
    }
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
        other => return Err(UnsupportedReason::UnsupportedModType(other.to_string())),
    };
    let translated = translate_mod_name(name, &element.flags, &element.keyword_flags)?;
    let mut modifier = if mod_type == ModType::Flag {
        // FLAG mod 的 merge 值仅 Lua 真值语义（任意 number 含 0 均 truthy）→ Bool(true)。
        Modifier::flag(translated)
    } else {
        Modifier::number(translated, mod_type, merged_value)
    };
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
    // 第一批 skill_data 白名单：duration（vendor `skill("duration", …)`，entry 级
    // div=1000 已在 merge 公式换算 ms → s）。其余键统计上报。
    if key == "duration" {
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

/// 五伤害类型（PoB2 命名 → PoBR PascalCase 相同）。
const DAMAGE_TYPES: [&str; 5] = ["Physical", "Fire", "Cold", "Lightning", "Chaos"];

/// `<Type>Min` / `<Type>Max` → `<Type>DamageMin/Max`（mod 与 skill_data 共用：
/// vendor 两条通道都用这组键名承载伤害基值）。
fn damage_bound_mod_name(name: &str) -> Option<String> {
    for ty in DAMAGE_TYPES {
        if let Some(bound) = name.strip_prefix(ty)
            && (bound == "Min" || bound == "Max")
        {
            return Some(format!("{ty}Damage{bound}"));
        }
    }
    None
}

/// ModName 翻译层（PoB2 名 + ModFlag 组合 → PoBR 名）。
///
/// 框架语义 L4（蓝图 §6 Q2 裁决：名字随机制不随版本变，留 Rust 常量表）。
/// 初版覆盖 = legacy `pobr-build::skill_stat_map` 既有映射族的反推；未知名归
/// [`UnsupportedReason::UnknownModName`]，由双跑 diff 驱动补全。
///
/// flag 语义分派（PoB2 用 ModFlag 限定作用域，PoBR 现阶段用独立 ModName 表达）：
/// - `Speed` + Attack → `AttackSpeed`；+ Cast → `CastSpeed`；裸 → `SkillSpeed`；
/// - `Damage` + Attack → `AttackDamage`；+ Spell → `Damage`（PoBR 不读 SpellDamage，
///   对齐 legacy 注）；+ Area → `AreaDamage`；裸 → `Damage`；
/// - 其余名字仅接受空 flag；伤害基值族（`<Type>Min/Max`）额外允许丢弃
///   Attack/Spell flag 与 KeywordFlag（legacy 同口径：单主技能全局注入）。
fn translate_mod_name(
    name: &str,
    flags: &[String],
    keyword_flags: &[String],
) -> Result<String, UnsupportedReason> {
    let flags_label = || flags.join("|");
    // 伤害基值族：允许丢弃 Attack/Spell flag 与 KeywordFlag（作用域限定在单主
    // 技能口径下无差异；legacy 同样 flag-blind 注入）。
    if let Some(bound_name) = damage_bound_mod_name(name) {
        let droppable = |f: &String| f == "Attack" || f == "Spell";
        if !flags.iter().all(droppable) {
            return Err(UnsupportedReason::UnsupportedFlags(flags_label()));
        }
        if !keyword_flags.iter().all(|f| f == "Attack" || f == "Spell") {
            return Err(UnsupportedReason::UnsupportedKeywordFlags(
                keyword_flags.join("|"),
            ));
        }
        return Ok(bound_name);
    }
    // flag 语义分派族。
    match name {
        "Speed" => {
            return match flags {
                [] => Ok("SkillSpeed".to_string()),
                [f] if f == "Attack" => Ok("AttackSpeed".to_string()),
                [f] if f == "Cast" => Ok("CastSpeed".to_string()),
                _ => Err(UnsupportedReason::UnsupportedFlags(flags_label())),
            };
        }
        "Damage" => {
            return match flags {
                [] => Ok("Damage".to_string()),
                [f] if f == "Attack" => Ok("AttackDamage".to_string()),
                [f] if f == "Spell" => Ok("Damage".to_string()),
                [f] if f == "Area" => Ok("AreaDamage".to_string()),
                _ => Err(UnsupportedReason::UnsupportedFlags(flags_label())),
            };
        }
        _ => {}
    }
    // 其余名字第一批仅接受无 flag（带 flag 的作用域语义待 ModFlags 管线接入）。
    if !flags.is_empty() {
        return Err(UnsupportedReason::UnsupportedFlags(flags_label()));
    }
    if !keyword_flags.is_empty() {
        return Err(UnsupportedReason::UnsupportedKeywordFlags(
            keyword_flags.join("|"),
        ));
    }
    // 直译常量表（PoB2 名 → PoBR 名）。
    let direct = match name {
        "CritChance" => "CriticalStrikeChance",
        "CritMultiplier" => "CriticalStrikeMultiplier",
        // 同名直通族。
        "PhysicalDamage"
        | "FireDamage"
        | "ColdDamage"
        | "LightningDamage"
        | "ChaosDamage"
        | "ElementalDamage"
        | "AreaDamage"
        | "FirePenetration"
        | "ColdPenetration"
        | "LightningPenetration"
        | "ChaosPenetration"
        | "ElementalPenetration"
        | "TotalCastTime"
        | "TotalAttackTime" => name,
        other => {
            // 转换 / gain-as 族（`Skill<From>DamageConvertTo<To>` /
            // `[Skill]<From>DamageGainAs<To>`）：PoB2 与 PoBR 命名一致，按形态直通。
            if is_conversion_mod_name(other) {
                return Ok(other.to_string());
            }
            return Err(UnsupportedReason::UnknownModName(other.to_string()));
        }
    };
    Ok(direct.to_string())
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

/// tag 翻译（第一批：Condition / Multiplier / PerStat → PoBR [`ModTag`]）。
///
/// 其余 tag 类型整条 Unsupported；已支持类型出现**约定外的键**同样 Unsupported
/// （宁可跳过——多余键往往携带额外语义，静默丢键 = 错算）。
fn translate_tag(tag: &BTreeMap<String, StatMapValue>) -> Result<ModTag, UnsupportedReason> {
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
            Ok(ModTag::Condition { var, negated })
        }
        "Multiplier" => {
            if !keys_subset_of(&["type", "var", "div", "limit"]) {
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
            Ok(ModTag::Multiplier {
                var,
                div: number("div").unwrap_or(1.0),
                limit: number("limit"),
            })
        }
        "PerStat" => {
            if !keys_subset_of(&["type", "stat", "div"]) {
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
            Ok(ModTag::Multiplier {
                var,
                div: number("div").unwrap_or(1.0),
                limit: None,
            })
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
        assert_eq!(
            mods[0].tags,
            vec![ModTag::Condition {
                var: "Leeching".into(),
                negated: true
            }]
        );
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
            vec![ModTag::Multiplier {
                var: "PowerCharge".into(),
                div: 1.0,
                limit: Some(5.0)
            }]
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
            vec![ModTag::Multiplier {
                var: "Intelligence".into(),
                div: 10.0,
                limit: None
            }]
        );
    }

    /// 第一批之外的 tag（GlobalEffect / DistanceRamp）→ 整条 Unsupported。
    #[test]
    fn unsupported_tag_types_reject_entry() {
        for tag in [
            r#"{ "type": "GlobalEffect", "effectType": "Buff" }"#,
            r#"{ "type": "DistanceRamp", "ramp": [[10,1],[35,0]] }"#,
        ] {
            let entry = entry_json(&format!(
                r#"{{ "mods": [ {{ "kind": "mod", "name": "Damage", "mod_type": "MORE",
                     "tags": [ {tag} ] }} ] }}"#
            ));
            assert!(
                matches!(
                    map_entry(&entry, 1.0),
                    MappedOutcome::Unsupported(UnsupportedReason::UnsupportedTag(_))
                ),
                "tag 应整条拒绝：{tag}"
            );
        }
        // 已支持类型 + 约定外键（limitTotal 携带额外语义）→ 同样拒绝。
        let entry = entry_json(
            r#"{ "mods": [ { "kind": "mod", "name": "Damage", "mod_type": "INC",
                 "tags": [ { "type": "Multiplier", "var": "X", "limitTotal": true } ] } ] }"#,
        );
        assert!(matches!(
            map_entry(&entry, 1.0),
            MappedOutcome::Unsupported(UnsupportedReason::UnsupportedTag(_))
        ));
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
