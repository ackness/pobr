//! buff 九类分发（env_finalize 阶段 4，M3 T3-C2/C3，蓝图 m3-orchestration.md §6.2-§6.3）。
//!
//! 对照 PoB2 `Modules/CalcPerform.lua`（vendor commit `2df5a74`，实读核对）：
//! - **aura 自身路径** :2085-2120（`buff.type == "Aura"`，自身乘区 :2102-2105）；
//! - **debuff 路径** :2219-2285（`buff.type == "Debuff"`，DebuffEffect 乘区 :2274-2278）；
//! - **curse 路径** :2286-2337（curse 表项构造 + CurseEffect 乘区）；
//! - **curse priority** :454-485（`determineCursePriority`，数据表 =
//!   `overlay/curse_priority.json`，schema [`CursePriorityDef`]）；
//! - **curse limit + 分槽** :2829-2920（`output.EnemyCurseLimit` :2830、槽位填充
//!   :2853-2876、`ignoreCurseLimit` 槽外追加 :2882-2896、敌侧应用 :2969-2984）；
//! - **ScaleAddList 取整** `Classes/ModStore.lua:45-79`（`ScaleAddMod`，见
//!   [`scale_value`]）+ `Modules/Data.lua:413-530`（`defaultHighPrecision` /
//!   `highPrecisionMods`）；
//! - **mergeBuff 同名取强** `Modules/CalcPerform.lua:41-63`。
//!
//! 门控（D5）：整段吃 `cfg.mode_buffs`（pobr-build 编排入口对 MAIN 口径恒置 true，
//! 对应 vendor 非 CALCS 模式 buffMode 恒 `"EFFECTIVE"`，CalcSetup.lua:583-597；
//! 默认仍 false——未显式置位的既有调用方逐值不变）；curse/debuff 段内再按 vendor 吃
//! `cfg.mode_effective`。
//!
//! C5 切换（M3-T3，双跑报告 `m3-c5-dualrun-report.md`：18-build display 全列逐值
//! 持平）后本路径是 aura 的唯一通道——编排层 `aura_buff_modifiers` 静态直注与
//! feature `buff-pass-aura` 守门均已删除；回退通道 = revert 切换/删码 commit。
//!
//! ## M3 口径简化清单（与 PoB2 差异显式记录，蓝图 §6.2 要求）
//!
//! - (a) `skill_db` = `player.mod_db` 全局聚合（pobr 无 per-skill modlist 分层）。
//!   差异面 =「只对某 aura 生效的 AuraEffect」词条（少见，命中时按 build 修）。
//! - (b) ally 取强分支（`allyBuffs`，:2105）恒空——party 未落地，分支不实现。
//! - (c) `auraCannotAffectSelf`（:2102）：granted_effect 数据列未落，恒按 false。
//! - (d) `ExtraAuraEffect` 附加词条列表（:2089-2101）未迁——pobr 解析层暂无该
//!   ModName 产出，命中时随 C5 diff 报告补。
//! - (e) `highPrecisionMods`（Data.lua:415-530）按命名规则镜像（全表为
//!   `<资源>{Regen,Degen}{,Percent}` / `*Damage*Leech` / `CritChance` 三族 +
//!   两个 MORE 条目），见 [`high_precision`]；`mod.unscalable`（ModStore.lua:46-52）
//!   pobr 词条模型无此位，不实现。
//! - (f) Debuff 的 `stackVar`/`stackLimit`（:2221-2230）：`BuffSpec` 契约（T0 冻结）
//!   无 stack 字段，M3 按 `stackCount = 1`（vendor `skillData.stackCount or 1` 缺省）。
//! - (g) curse priority 的来源权重：pobr 未建模「装备隐式诅咒 / aura 施加诅咒」
//!   （`socketGroup.source` / Blasphemy 类），编排层恒 [`CurseSourceWeight::None`]；
//!   [`determine_curse_priority`] 保留参数，表驱动测试按 vendor 数值样例覆盖。
//! - (h) `SelfCast<名>` 条件（:2288）、`socketedCursesHexLimit`（:2294/:2899-2914）、
//!   `CurseBuff` 的 buffModList 二次缩放（:2318-2330）不在 M3 范围——`CurseBuff`
//!   与其余未消费 kind 走「原值直注」兼容路径。
//! - (i) buff 名表：curse 名经编排层从 `active_skill` 蛇形名派生（`snipers_mark` →
//!   `Snipers Mark`），与 vendor 撇号名（`Sniper's Mark`）不一致时 `curse_base`
//!   查不到 → 基值 0（vendor `data.cursePriority[curseName] or 0` 同口径回退）。
//! - (j) curse 效果词条（M3-W4，原「mods 恒空」简化**已实现**）：编排层
//!   `buff_skill_specs` 经 statmap curse 域（`stat_map_engine::map_curse_stat`，
//!   vendor 各 curse statSet 的 `GlobalEffect effectType=Curse` 条目）把 statset
//!   stat 映射为敌侧 modifier 填入 `spec.mods`，本路径施 CurseEffect 乘区
//!   （:2295-2305）+ `Condition:Effective` 后随入槽 curse 写 enemy db
//!   （:2969-2984）。**残余口径**：敌侧 ModName 允收名单 = pobr 现有消费方
//!   （`<Type>Resist` / `Damage` / `SelfCritMultiplier`；`ElementalResist` 展开
//!   火/冰/电三条等值），无消费方的名（`TemporalChainsActionSpeed` /
//!   `BuffExpireFaster` / `FreezeBuildup` / `ElectrocuteBuildup` / `IgnoreArmour`
//!   / `Dummy`）整条 Unsupported 落 Compare 可见性报表不注入；`GlobalEffect` 带
//!   `effectCond`/`modCond` 等额外门控键的条目同样跳过上报。
//!
//! 归因：aura/curse/debuff 缩放产物保留原 `origin`（trace 不丢弃）；无 origin 的
//! 词条回退 `(SourceKind::Buff, "aura.<skill_id>" / "curse.<skill_id>" /
//! "buff.<skill_id>")`；缩放倍率记入 `raw_text`。

use std::collections::BTreeMap;

use pobr_data::catalog::curse_priority::CursePriorityDef;
use pobr_data::prelude::*;
use pobr_data::source::SourceKind;

use crate::{ModTag, ModValue, Modifier};

use super::Env;
use super::session::BuffKind;
use super::survivability::{ChargeKind, charge_maximum};

/// 玩家固有诅咒上限基线（vendor `CalcSetup.lua:648`
/// `NewMod("EnemyCurseLimit","BASE",1,"Base")`；数据镜像 =
/// `base/base_player_mods.json::enemy_curse_limit`，逐值对照测试见
/// `pobr-gamedata/tests/load_base_player_mods.rs`）。
pub const DEFAULT_ENEMY_CURSE_LIMIT: f64 = 1.0;

/// 玩家固有标记上限基线（vendor `CalcSetup.lua:649`；数据镜像 =
/// `base_player_mods.json::enemy_mark_limit`）。
pub const DEFAULT_ENEMY_MARK_LIMIT: f64 = 1.0;

/// socket 序入 priority 的上限（vendor `CalcPerform.lua:465`
/// `m_min(k, 8)`——避免与 `CurseFromEquipment` 权重段碰撞）。
const SOCKET_INDEX_CAP: u32 = 8;

/// 非高精度数值缩放的中间舍入小数位（vendor `ModStore.lua:71`
/// `m_modf(round(subMod.value * scale, 2))` 的 `2`）。
const SCALE_ROUND_DECIMALS: i32 = 2;

/// 高精度缺省小数位（vendor `Data.lua:413` `data.defaultHighPrecision = 1`）。
const DEFAULT_HIGH_PRECISION: i32 = 1;

/// aura 自身乘区聚合的 ModName 集（vendor `CalcPerform.lua:2103-2104`，INC/MORE 同名集）。
const AURA_SELF_EFFECT_NAMES: [&str; 6] = [
    "AuraEffect",
    "BuffEffect",
    "BuffEffectOnSelf",
    "AuraEffectOnSelf",
    "AuraBuffEffect",
    "SkillAuraEffectOnSelf",
];

/// curse priority 的来源权重维度（vendor `determineCursePriority` :473-478：aura 源 →
/// `CurseFromAura`、装备源 → `CurseFromEquipment`）。M3 编排层未建模两类来源，恒
/// [`Self::None`]（简化 (g)）；保留维度供表驱动测试按 vendor 数值样例覆盖。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurseSourceWeight {
    /// 普通宝石施放（vendor `source == "" 且非 aura`，权重 0）。
    None,
    /// aura 施加（Blasphemy 类，恒最高段权重）。
    Aura,
    /// 装备隐式诅咒（Ring 2/3 槽位权重折回 Ring 1，:480-483）。
    Equipment,
}

/// curse 面板输出（`buff_pass` 产出，经 [`Env::curse_pass_output`] 由 `perform` 末端
/// 回填 [`super::OutputTable`] 的 `enemy_curse_limit` / `curse_slots`；蓝图 §6.3
/// 「display_catalog 不扩，仅 OutputTable 字段」）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CursePassOutput {
    /// `output.EnemyCurseLimit`（vendor :2830）。
    pub enemy_curse_limit: f64,
    /// curse 槽占用列表（顺序 = vendor `curseSlots` 合并序：hex 槽 → mark 槽 →
    /// `ignoreCurseLimit` 槽外追加，:2878-2896）。
    pub curse_slots: Vec<String>,
}

/// 一个已通过 gate 的 curse 候选（vendor :2290-2298 `curse` 表项的 pobr 等价）。
#[derive(Debug, Clone)]
struct CurseEntry {
    name: String,
    priority: i64,
    is_mark: bool,
    ignore_curse_limit: bool,
    mods: Vec<Modifier>,
}

/// priority 计算（vendor `determineCursePriority`，CalcPerform.lua:454-485）：
/// `base + min(socket_index, 8) × SocketPriorityBase + slot_weight + source_weight`。
///
/// - `slot` 查表前去 `" (Swap)"` / `" Swap"` 后缀（vendor :471 `slot:gsub(" (Swap)","")`，
///   Lua 模式中括号为捕获、实际匹配字面 `" Swap"`；两种写法都剥以兼容）；查不到 = 0。
/// - `socket_index` 1-based（vendor `ipairs` 序，缺省 1）；钳到 `[1, 8]`。
/// - 装备源且槽位权重为 Ring 2/Ring 3 时折回 Ring 1（:480-483，装备隐式诅咒不分环）。
/// - `curse_base` 查不到 = 0（vendor `data.cursePriority[curseName] or 0`）。
pub fn determine_curse_priority(
    data: &CursePriorityDef,
    curse_name: &str,
    slot: Option<&str>,
    socket_index: u32,
    source: CurseSourceWeight,
) -> i64 {
    let base = data.curse_base.get(curse_name).copied().unwrap_or(0);
    let socket = i64::from(socket_index.clamp(1, SOCKET_INDEX_CAP)) * data.socket_priority_base;
    let slot_key = slot
        .map(|s| {
            s.trim_end_matches(" (Swap)")
                .trim_end_matches(" Swap")
                .to_string()
        })
        .unwrap_or_default();
    let mut slot_weight = data.slot_weights.get(&slot_key).copied().unwrap_or(0);
    if source == CurseSourceWeight::Equipment {
        let ring = |name: &str| data.slot_weights.get(name).copied();
        if Some(slot_weight) == ring("Ring 2") || Some(slot_weight) == ring("Ring 3") {
            slot_weight = ring("Ring 1").unwrap_or(slot_weight);
        }
    }
    let source_weight = match source {
        CurseSourceWeight::None => 0,
        CurseSourceWeight::Aura => data.curse_from_aura,
        CurseSourceWeight::Equipment => data.curse_from_equipment,
    };
    base + socket + slot_weight + source_weight
}

/// 高精度小数位查表（vendor `Data.lua:415-530` `highPrecisionMods` 的规则镜像，
/// 简化 (e)：全表可归纳为命名族，逐族对照）：
/// - BASE `CritChance` / `SelfCritChance` → 2（:416-421）；
/// - BASE `*RegenPercent` / `*DegenPercent` → 2（:422-451 的 Percent 条目）；
/// - BASE `*Regen` / `*Degen`（非 Percent）→ 1（:431-460，含 `RageRegen`）；
/// - BASE `*Damage*{Life,Mana,EnergyShield}Leech` → 2（:460-523）；
/// - MORE `SupportManaMultiplier` / `ReservationMultiplier` → 4（:524-529）。
fn high_precision(name: &str, mod_type: ModType) -> Option<i32> {
    match mod_type {
        ModType::More => {
            matches!(name, "SupportManaMultiplier" | "ReservationMultiplier").then_some(4)
        }
        ModType::Base => {
            if matches!(name, "CritChance" | "SelfCritChance") {
                return Some(2);
            }
            if name.ends_with("RegenPercent") || name.ends_with("DegenPercent") {
                return Some(2);
            }
            if name.contains("Damage")
                && (name.ends_with("LifeLeech")
                    || name.ends_with("ManaLeech")
                    || name.ends_with("EnergyShieldLeech"))
            {
                return Some(2);
            }
            if name.ends_with("Regen") || name.ends_with("Degen") {
                return Some(1);
            }
            None
        }
        _ => None,
    }
}

/// PoB2 `round(val, dec)`（Common.lua:648-654：`m_floor(val × 10^dec + 0.5) / 10^dec`）。
fn pob_round(value: f64, dec: i32) -> f64 {
    let mult = 10f64.powi(dec);
    (value * mult + 0.5).floor() / mult
}

/// ScaleAddMod 的数值缩放语义（vendor `ModStore.lua:45-79`，逐字对齐）：
/// - `scale == 1` → 原值直返（:54）；
/// - 高精度条目（[`high_precision`]）或非整数原值（→ `defaultHighPrecision = 1`，
///   Data.lua:413）→ `m_floor(value × scale × 10^p) / 10^p`（:67-69）；
/// - 其余 → `m_modf(round(value × scale, 2))` 取整数部（向零截断，:71）。
pub fn scale_value(name: &str, mod_type: ModType, value: f64, scale: f64) -> f64 {
    if scale == 1.0 {
        return value;
    }
    let precision = high_precision(name, mod_type)
        .or_else(|| (value.floor() != value).then_some(DEFAULT_HIGH_PRECISION));
    match precision {
        Some(p) => {
            let mult = 10f64.powi(p);
            (value * scale * mult).floor() / mult
        }
        None => pob_round(value * scale, SCALE_ROUND_DECIMALS).trunc(),
    }
}

/// 对一条 buff 词条施加效果乘区（ScaleAddMod 等价）并整理归因：保留原 `origin`
/// （trace 不丢弃），无 origin 时回退 `(SourceKind::Buff, fallback_source_id)`；
/// 缩放倍率记入 `raw_text`。非数值载荷（Flag/Text/NestedMods）原样透传
/// （vendor 对 table 载荷只缩放 `value.mod`，pobr buff 词条均为数值/Flag）。
fn scale_buff_mod(modifier: &Modifier, mult: f64, fallback_source_id: &str) -> Modifier {
    let mut out = modifier.clone();
    if let ModValue::Number(v) = out.value {
        out.value = ModValue::Number(scale_value(out.name.as_str(), out.mod_type, v, mult));
    }
    let scale_note = format!("buff effect ×{mult:.4}");
    match out.origin.as_mut() {
        Some(origin) => {
            if mult != 1.0 {
                origin.raw_text = Some(match origin.raw_text.take() {
                    Some(text) => format!("{text} ({scale_note})"),
                    None => scale_note,
                });
            }
        }
        None => {
            out.origin = Some(
                ModifierSource::new(SourceId::new(
                    SourceKind::Buff,
                    fallback_source_id.to_string(),
                ))
                .with_raw_text(scale_note),
            );
        }
    }
    out
}

/// mergeBuff 同名取强合并（vendor `CalcPerform.lua:41-63`）：同 buff 名桶内，
/// 「mod 参数相同」（name/type/flags/keyword_flags/tags）且皆为数值时取大者；
/// LIST 载荷恒追加（:48 `mod.type ~= "LIST"`）。
fn merge_buff(dest: &mut Vec<Modifier>, src: Vec<Modifier>) {
    for incoming in src {
        if incoming.mod_type != ModType::List
            && let Some(existing) = dest.iter_mut().find(|m| {
                m.name == incoming.name
                    && m.mod_type == incoming.mod_type
                    && m.flags == incoming.flags
                    && m.keyword_flags == incoming.keyword_flags
                    && m.tags == incoming.tags
            })
        {
            if let (Some(old), Some(new)) = (existing.value.as_number(), incoming.value.as_number())
                && new > old
            {
                *existing = incoming;
            }
            continue;
        }
        dest.push(incoming);
    }
}

/// 确保词条带 `Condition:Effective` tag（敌侧 debuff 口径对齐，蓝图 §6.3；已带则不重复）。
fn ensure_effective_tag(modifier: &mut Modifier) {
    let already = modifier.tags.iter().any(|tag| {
        matches!(tag, ModTag::Condition { var, negated: false, actor: None } if var == "Effective")
    });
    if !already {
        modifier.tags.push(ModTag::condition("Effective", false));
    }
}

/// buff 名 → `AffectedBy<去空格名>` 条件名（vendor :2110 `buff.name:gsub(" ","")`）。
fn affected_by_condition(name: &str) -> String {
    format!("AffectedBy{}", name.replace(' ', ""))
}

/// env_finalize 阶段 4 实现体：`Env::buff_skills` 九类分发。
///
/// M3 消费 Aura / Curse / Debuff 三类；其余 kind 走「原值直注」兼容路径（词条不
/// 缩放、不置条件）。`cfg.mode_buffs == false`（默认）或无 spec 时整段空转
/// （逐值不变）。
pub fn buff_pass(env: &mut Env) {
    if !env.cfg.mode_buffs || env.buff_skills.is_empty() {
        return;
    }
    let specs = env.buff_skills.clone();
    let priority_data = env.curse_priority.clone().unwrap_or_default();

    // —— 收集阶段（只读 env）——
    // 玩家侧 buff（aura 等）按 buff 名分桶（mergeBuff 同名取强）；BTreeMap 保证确定性序。
    let mut player_buffs: BTreeMap<String, Vec<Modifier>> = BTreeMap::new();
    // 敌侧 debuff 按 buff 名分桶（vendor `debuffs` 表）。
    let mut enemy_debuffs: BTreeMap<String, Vec<Modifier>> = BTreeMap::new();
    // 未消费 kind 的原值直注兼容路径。
    let mut passthrough: Vec<Modifier> = Vec::new();
    let mut curses: Vec<CurseEntry> = Vec::new();
    let mut conditions: Vec<String> = Vec::new();

    for spec in &specs {
        match spec.kind {
            BuffKind::Aura => {
                // vendor :2102-2105：自身乘区（简化 (a)：skill_db = player.mod_db 全局聚合；
                // (b) ally 取强恒空；(c) auraCannotAffectSelf 恒 false）。
                let names: Vec<ModName> = AURA_SELF_EFFECT_NAMES
                    .iter()
                    .map(|n| ModName::from(*n))
                    .collect();
                let inc = env.player.mod_db.sum(ModType::Inc, &env.cfg, &names);
                let more = env.player.mod_db.more(&env.cfg, &names) * spec.magnitude;
                let mult = (1.0 + inc / 100.0) * more;
                // vendor :2107-2110：条件置位。
                conditions.push("AffectedByAura".to_string());
                conditions.push(affected_by_condition(&spec.name));
                let source_id = format!("aura.{}", spec.skill_id);
                let scaled = spec
                    .mods
                    .iter()
                    .map(|m| scale_buff_mod(m, mult, &source_id))
                    .collect();
                merge_buff(player_buffs.entry(spec.name.clone()).or_default(), scaled);
            }
            BuffKind::Curse => {
                // vendor :2289 gate：`(mode_effective and (not Hexproof or 豁免)) or mark`。
                let hexproof = env.enemy.mod_db.flag(&env.cfg, ModName::from("Hexproof"));
                let ignores_hexproof = env
                    .player
                    .mod_db
                    .flag(&env.cfg, ModName::from("CursesIgnoreHexproof"))
                    || spec.ignore_curse_limit;
                if !((env.cfg.mode_effective && (!hexproof || ignores_hexproof)) || spec.is_mark) {
                    continue;
                }
                // vendor :2295-2305：CurseEffect 乘区。蓝图 §6.3 原文记
                // `INC(CurseEffect, BuffEffect)`，实读 :2295 为
                // `Sum(INC, CurseEffect) + enemyDB Sum(INC, CurseEffectOnSelf)`
                // （无 BuffEffect），按 vendor 实读对齐；aura 源附加 AuraEffect INC
                // （:2296-2298）不实现（简化 (g)：无 aura 源建模）。
                let curse_effect = [ModName::from("CurseEffect")];
                let curse_effect_on_self = [ModName::from("CurseEffectOnSelf")];
                let inc = env.player.mod_db.sum(ModType::Inc, &env.cfg, &curse_effect)
                    + env
                        .enemy
                        .mod_db
                        .sum(ModType::Inc, &env.cfg, &curse_effect_on_self);
                let mut more = env.player.mod_db.more(&env.cfg, &curse_effect) * spec.magnitude;
                if !spec.is_mark {
                    // vendor :2303-2305：非 mark 再乘敌侧 CurseEffectOnSelf MORE。
                    more *= env.enemy.mod_db.more(&env.cfg, &curse_effect_on_self);
                }
                let mult = (1.0 + inc / 100.0) * more;
                let source_id = format!("curse.{}", spec.skill_id);
                let mods = spec
                    .mods
                    .iter()
                    .map(|m| {
                        let mut scaled = scale_buff_mod(m, mult, &source_id);
                        // 敌侧写入带 Condition:Effective 门控（蓝图 §6.3，对齐现有敌侧口径）。
                        ensure_effective_tag(&mut scaled);
                        scaled
                    })
                    .collect();
                // vendor :2294 `ignoreCurseLimit = (...) and not mark or false`。
                let ignore_curse_limit = (env
                    .player
                    .mod_db
                    .flag(&env.cfg, ModName::from("CursesIgnoreCurseLimit"))
                    || spec.ignore_curse_limit)
                    && !spec.is_mark;
                curses.push(CurseEntry {
                    name: spec.name.clone(),
                    priority: determine_curse_priority(
                        &priority_data,
                        &spec.name,
                        spec.slot.as_deref(),
                        spec.socket_index,
                        CurseSourceWeight::None, // 简化 (g)：aura/装备源未建模。
                    ),
                    is_mark: spec.is_mark,
                    ignore_curse_limit,
                    mods,
                });
            }
            BuffKind::Debuff => {
                // vendor :2219-2285（Debuff 分支）：mode_effective 门控 + DebuffEffect
                // 乘区；stackCount = 1（简化 (f)）。
                if !env.cfg.mode_effective {
                    continue;
                }
                let names = [ModName::from("DebuffEffect")];
                let inc = env.player.mod_db.sum(ModType::Inc, &env.cfg, &names);
                let more = env.player.mod_db.more(&env.cfg, &names) * spec.magnitude;
                let mult = (1.0 + inc / 100.0) * more;
                conditions.push(affected_by_condition(&spec.name));
                let source_id = format!("buff.{}", spec.skill_id);
                let scaled = spec
                    .mods
                    .iter()
                    .map(|m| scale_buff_mod(m, mult, &source_id))
                    .collect();
                merge_buff(enemy_debuffs.entry(spec.name.clone()).or_default(), scaled);
            }
            // 其余 kind（Buff/Guard/Warcry/AuraDebuff/CurseBuff/Link）：M3 框架内
            // 「原值直注」兼容路径（不缩放、不置条件；当前编排层不构造这些 kind，
            // 行为与现状一致）。
            _ => {
                let source_id = format!("buff.{}", spec.skill_id);
                passthrough.extend(spec.mods.iter().map(|m| scale_buff_mod(m, 1.0, &source_id)));
            }
        }
    }

    // —— curse limit + 分槽（vendor :2829-2896）——
    // limit：override(CurseLimitIsMaximumPowerCharges → PowerChargesMax) else
    // 基线 1（CalcSetup.lua:648，数据镜像 base_player_mods.json）+ Σ BASE。
    let curse_limit = if env
        .player
        .mod_db
        .flag(&env.cfg, ModName::from("CurseLimitIsMaximumPowerCharges"))
    {
        f64::from(charge_maximum(
            &env.player.mod_db,
            &env.cfg,
            ChargeKind::Power,
        ))
    } else {
        DEFAULT_ENEMY_CURSE_LIMIT
            + env
                .player
                .mod_db
                .sum(ModType::Base, &env.cfg, &[ModName::from("EnemyCurseLimit")])
    };
    let mark_limit = DEFAULT_ENEMY_MARK_LIMIT
        + env
            .player
            .mod_db
            .sum(ModType::Base, &env.cfg, &[ModName::from("EnemyMarkLimit")]);

    // 分槽填充（vendor :2845-2876）：curse/mark 槽**分开**，同名高 priority 替换、
    // 低者跳过；异名时替换「扫描中最后一个更低 priority 槽」（vendor 循环语义逐字）。
    let mut curse_slots: Vec<Option<CurseEntry>> = vec![None; curse_limit.max(0.0) as usize];
    let mut mark_slots: Vec<Option<CurseEntry>> = vec![None; mark_limit.max(0.0) as usize];
    for curse in curses.iter().filter(|c| !c.ignore_curse_limit) {
        let slots = if curse.is_mark {
            &mut mark_slots
        } else {
            &mut curse_slots
        };
        let mut target: Option<usize> = None;
        for (i, slot) in slots.iter().enumerate() {
            match slot {
                None => {
                    target = Some(i);
                    break;
                }
                Some(existing) if existing.name == curse.name => {
                    target = (existing.priority < curse.priority).then_some(i);
                    break;
                }
                Some(existing) => {
                    if existing.priority < curse.priority {
                        target = Some(i);
                    }
                }
            }
        }
        if let Some(i) = target {
            slots[i] = Some(curse.clone());
        }
    }
    // 合并（vendor :2879 `tableConcat(curseSlots, markSlots)`）。
    let mut occupied: Vec<CurseEntry> = curse_slots
        .into_iter()
        .chain(mark_slots)
        .flatten()
        .collect();
    // `ignoreCurseLimit` 槽外追加（vendor :2882-2896：同名高者替换、否则跳过；异名追加）。
    for curse in curses.iter().filter(|c| c.ignore_curse_limit) {
        match occupied.iter_mut().find(|c| c.name == curse.name) {
            Some(existing) => {
                if existing.priority < curse.priority {
                    *existing = curse.clone();
                }
            }
            None => occupied.push(curse.clone()),
        }
    }

    // —— 写入阶段（只写 player/enemy mod_db + cfg.conditions/multipliers + 输出桥）——
    let buff_on_self = player_buffs.len() as f64;
    for (_, mods) in player_buffs {
        env.player.mod_db.add_list(mods);
    }
    if buff_on_self > 0.0 {
        // vendor :2949-2951：每个生效 buff 计入 multipliers["BuffOnSelf"]。
        *env.cfg
            .multipliers
            .entry("BuffOnSelf".to_string())
            .or_insert(0.0) += buff_on_self;
    }
    env.player.mod_db.add_list(passthrough);
    for (_, mods) in enemy_debuffs {
        env.enemy.mod_db.add_list(mods);
    }
    // curse 槽应用（vendor :2969-2984）：敌侧条件 + modList 注入 + CurseOnEnemy 乘数。
    env.cfg
        .multipliers
        .insert("CurseOnEnemy".to_string(), occupied.len() as f64);
    let mut slot_names = Vec::with_capacity(occupied.len());
    for slot in occupied {
        conditions.push("EnemyCursed".to_string());
        if slot.is_mark {
            conditions.push("EnemyMarked".to_string());
        }
        env.enemy.mod_db.add_list(slot.mods);
        slot_names.push(slot.name);
    }
    for condition in conditions {
        env.cfg.conditions.insert(condition, true);
    }
    env.curse_pass_output = Some(CursePassOutput {
        enemy_curse_limit: curse_limit,
        curse_slots: slot_names,
    });
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::CalcConfig;
    use crate::calc::{Actor, ActorBaseStats, BuffSpec};

    /// vendor `Modules/Data.lua:274` `data.cursePriority` 数值镜像（与
    /// `overlay/curse_priority.json` 逐值一致；pobr-core 零 I/O，测试内联构造）。
    fn vendor_priority_data() -> CursePriorityDef {
        CursePriorityDef {
            curse_base: BTreeMap::from(
                [
                    ("Temporal Chains", 1),
                    ("Enfeeble", 2),
                    ("Vulnerability", 3),
                    ("Elemental Weakness", 4),
                    ("Flammability", 5),
                    ("Frostbite", 6),
                    ("Conductivity", 7),
                    ("Despair", 8),
                    ("Punishment", 9),
                    ("Warlord's Mark", 10),
                    ("Assassin's Mark", 11),
                    ("Sniper's Mark", 12),
                    ("Poacher's Mark", 13),
                ]
                .map(|(k, v)| (k.to_string(), v)),
            ),
            socket_priority_base: 100,
            slot_weights: BTreeMap::from(
                [
                    ("Weapon 1", 1000),
                    ("Amulet", 2000),
                    ("Helmet", 3000),
                    ("Weapon 2", 4000),
                    ("Body Armour", 5000),
                    ("Gloves", 6000),
                    ("Boots", 7000),
                    ("Ring 1", 8000),
                    ("Ring 2", 9000),
                    ("Ring 3", 10000),
                ]
                .map(|(k, v)| (k.to_string(), v)),
            ),
            curse_from_equipment: 11000,
            curse_from_aura: 20000,
        }
    }

    fn buffed_env() -> Env {
        let mut env = Env::new(Actor::new(1, ActorBaseStats::default()));
        env.cfg = CalcConfig::attack()
            .with_mode_buffs(true)
            .with_mode_effective(true);
        env.curse_priority = Some(vendor_priority_data());
        env
    }

    fn curse_spec(
        name: &str,
        slot: &str,
        socket_index: u32,
        taken_inc: f64,
        is_mark: bool,
        ignore_curse_limit: bool,
    ) -> BuffSpec {
        BuffSpec {
            name: name.to_string(),
            kind: BuffKind::Curse,
            skill_id: format!("{}Player", name.replace([' ', '\''], "")),
            mods: vec![Modifier::number("DamageTaken", ModType::Inc, taken_inc)],
            magnitude: 1.0,
            slot: Some(slot.to_string()),
            socket_index,
            is_mark,
            ignore_curse_limit,
        }
    }

    fn aura_spec(name: &str, es: f64) -> BuffSpec {
        BuffSpec {
            name: name.to_string(),
            kind: BuffKind::Aura,
            skill_id: format!("{}Player", name.replace(' ', "")),
            mods: vec![Modifier::number("EnergyShield", ModType::Base, es)],
            magnitude: 1.0,
            slot: Some("Body Armour".to_string()),
            socket_index: 1,
            is_mark: false,
            ignore_curse_limit: false,
        }
    }

    // ===== ScaleAddMod 取整语义（vendor ModStore.lua:45-79，逐字对齐） =====

    /// 整数原值走 `m_modf(round(x, 2))`（向零截断）；高精度族走 floor 截位。
    #[test]
    fn scale_value_matches_vendor_rounding() {
        // scale == 1 → 原值直返（含非整数，:54）。
        assert_eq!(scale_value("EnergyShield", ModType::Base, 1.5, 1.0), 1.5);
        // 整数原值：100 × 1.2 = 120（恰整）；33 × 1.1 = 36.3 → round(…,2) → trunc 36。
        assert_eq!(
            scale_value("EnergyShield", ModType::Base, 100.0, 1.2),
            120.0
        );
        assert_eq!(scale_value("Damage", ModType::Inc, 33.0, 1.1), 36.0);
        // 负值向零截断（m_modf 语义）：-33 × 1.1 = -36.3 → -36。
        assert_eq!(scale_value("ActionSpeed", ModType::Inc, -33.0, 1.1), -36.0);
        // 非整数原值 → defaultHighPrecision = 1（Data.lua:413）：1.5 × 1.3 = 1.95 → 1.9。
        assert_eq!(scale_value("Damage", ModType::Inc, 1.5, 1.3), 1.9);
        // CritChance BASE → 精度 2（Data.lua:416-418）：5 × 1.234 = 6.17。
        assert_eq!(scale_value("CritChance", ModType::Base, 5.0, 1.234), 6.17);
        // LifeRegen BASE → 精度 1：7 × 1.15 = 8.05 → 8.0（floor 截位）。
        assert_eq!(scale_value("LifeRegen", ModType::Base, 7.0, 1.15), 8.0);
        // LifeRegenPercent BASE → 精度 2。
        assert_eq!(
            scale_value("LifeRegenPercent", ModType::Base, 1.0, 1.155),
            1.15
        );
        // Damage*Leech 族 BASE → 精度 2（Data.lua:460-523）。
        assert_eq!(
            scale_value("PhysicalDamageLifeLeech", ModType::Base, 2.0, 1.333),
            2.66
        );
        // MORE SupportManaMultiplier → 精度 4（Data.lua:524-526）。
        assert_eq!(
            scale_value("SupportManaMultiplier", ModType::More, 130.0, 1.11111),
            144.4443
        );
    }

    // ===== curse priority（vendor determineCursePriority :454-485，表驱动） =====

    /// vendor 数值样例：`base + min(socket,8)×100 + slot_weight + source_weight`。
    #[test]
    fn curse_priority_matches_vendor_samples() {
        let data = vendor_priority_data();
        // Temporal Chains（1），Ring 1（8000），socket 2：1 + 200 + 8000。
        assert_eq!(
            determine_curse_priority(
                &data,
                "Temporal Chains",
                Some("Ring 1"),
                2,
                CurseSourceWeight::None
            ),
            8201
        );
        // Despair（8），"Weapon 1 Swap" 去后缀 → Weapon 1（1000），aura 源（20000）。
        assert_eq!(
            determine_curse_priority(
                &data,
                "Despair",
                Some("Weapon 1 Swap"),
                1,
                CurseSourceWeight::Aura
            ),
            8 + 100 + 1000 + 20000
        );
        // 装备隐式源 + Ring 2 → 槽位权重折回 Ring 1（:480-483）。
        assert_eq!(
            determine_curse_priority(
                &data,
                "Enfeeble",
                Some("Ring 2"),
                1,
                CurseSourceWeight::Equipment
            ),
            2 + 100 + 8000 + 11000
        );
        // socket 序钳到 8（:465）；未知槽 / 未知 curse 名 → 0 回退。
        assert_eq!(
            determine_curse_priority(&data, "Vulnerability", None, 12, CurseSourceWeight::None),
            3 + 800
        );
        assert_eq!(
            determine_curse_priority(
                &data,
                "Unknown Hex",
                Some("Flask 1"),
                1,
                CurseSourceWeight::None
            ),
            100
        );
    }

    // ===== 门控不变式（D5 / 本波锚点） =====

    /// `mode_buffs == false`（默认）→ 有 spec 也整段空转：双 db / 条件 / 输出桥逐值不变。
    #[test]
    fn mode_buffs_off_is_value_identical_noop() {
        let mut env = Env::new(Actor::new(1, ActorBaseStats::default()));
        env.cfg = CalcConfig::attack().with_mode_effective(true); // mode_buffs 默认 false
        env.curse_priority = Some(vendor_priority_data());
        env.buff_skills.push(aura_spec("Discipline", 100.0));
        env.buff_skills.push(curse_spec(
            "Temporal Chains",
            "Ring 1",
            1,
            10.0,
            false,
            false,
        ));
        let conditions_before = env.cfg.conditions.clone();

        buff_pass(&mut env);

        assert_eq!(env.player.mod_db.iter_mods().count(), 0);
        assert_eq!(env.enemy.mod_db.iter_mods().count(), 0);
        assert_eq!(env.cfg.conditions, conditions_before);
        assert_eq!(env.curse_pass_output, None);
    }

    // ===== curse limit / 分槽（vendor :2829-2896） =====

    /// 3 hex 入 1 limit → priority 最高者独占槽；败者词条不入敌 db。
    #[test]
    fn curse_limit_keeps_highest_priority() {
        let mut env = buffed_env();
        // priority：Despair(Boots,socket1)=8+100+7000 < Enfeeble(Ring 1)=2+100+8000
        // < Temporal Chains(Ring 1, socket 3)=1+300+8000。
        env.buff_skills
            .push(curse_spec("Despair", "Boots", 1, 5.0, false, false));
        env.buff_skills
            .push(curse_spec("Enfeeble", "Ring 1", 1, 7.0, false, false));
        env.buff_skills.push(curse_spec(
            "Temporal Chains",
            "Ring 1",
            3,
            11.0,
            false,
            false,
        ));

        buff_pass(&mut env);

        let out = env.curse_pass_output.as_ref().expect("buff_pass 已运行");
        assert_eq!(
            out.enemy_curse_limit, 1.0,
            "基线 limit = 1（CalcSetup.lua:648）"
        );
        assert_eq!(out.curse_slots, vec!["Temporal Chains".to_string()]);
        assert_eq!(
            env.enemy
                .mod_db
                .sum(ModType::Inc, &env.cfg, &[ModName::from("DamageTaken")]),
            11.0,
            "仅 priority 最高者的词条入敌 db"
        );
        assert_eq!(env.cfg.multiplier("CurseOnEnemy"), 1.0);
        assert!(env.cfg.condition("EnemyCursed"));
        assert!(!env.cfg.condition("EnemyMarked"));
    }

    /// `+1 EnemyCurseLimit` BASE 词条扩槽：两 hex 并存。
    #[test]
    fn curse_limit_extends_with_base_mods() {
        let mut env = buffed_env();
        env.player
            .mod_db
            .add_mod(Modifier::number("EnemyCurseLimit", ModType::Base, 1.0));
        env.buff_skills
            .push(curse_spec("Despair", "Boots", 1, 5.0, false, false));
        env.buff_skills
            .push(curse_spec("Enfeeble", "Ring 1", 1, 7.0, false, false));

        buff_pass(&mut env);

        let out = env.curse_pass_output.as_ref().unwrap();
        assert_eq!(out.enemy_curse_limit, 2.0);
        assert_eq!(out.curse_slots.len(), 2);
        assert_eq!(
            env.enemy
                .mod_db
                .sum(ModType::Inc, &env.cfg, &[ModName::from("DamageTaken")]),
            12.0
        );
    }

    /// `CurseLimitIsMaximumPowerCharges` override：limit = PowerChargesMax（vendor :2830）。
    #[test]
    fn curse_limit_override_uses_power_charges_max() {
        let mut env = buffed_env();
        env.player
            .mod_db
            .add_mod(Modifier::flag("CurseLimitIsMaximumPowerCharges"));
        env.buff_skills
            .push(curse_spec("Despair", "Boots", 1, 5.0, false, false));

        buff_pass(&mut env);

        assert_eq!(
            env.curse_pass_output.as_ref().unwrap().enemy_curse_limit,
            3.0,
            "默认最大充能 3（survivability::DEFAULT_MAX_CHARGES）"
        );
    }

    /// mark 与 hex 分槽（vendor :2835/:2852-2853）：各占各的 limit，互不挤占。
    #[test]
    fn marks_and_hexes_use_separate_slots() {
        let mut env = buffed_env();
        env.buff_skills.push(curse_spec(
            "Temporal Chains",
            "Ring 1",
            1,
            10.0,
            false,
            false,
        ));
        env.buff_skills
            .push(curse_spec("Sniper's Mark", "Gloves", 1, 6.0, true, false));

        buff_pass(&mut env);

        let out = env.curse_pass_output.as_ref().unwrap();
        // 合并序 = hex 槽 → mark 槽（vendor tableConcat :2879）。
        assert_eq!(
            out.curse_slots,
            vec!["Temporal Chains".to_string(), "Sniper's Mark".to_string()]
        );
        assert_eq!(
            env.enemy
                .mod_db
                .sum(ModType::Inc, &env.cfg, &[ModName::from("DamageTaken")]),
            16.0
        );
        assert!(env.cfg.condition("EnemyMarked"));
        assert_eq!(env.cfg.multiplier("CurseOnEnemy"), 2.0);
    }

    /// `ignore_curse_limit` 槽外追加（vendor :2882-2896）：limit 1 被占满后仍追加。
    #[test]
    fn ignore_curse_limit_appends_beyond_slots() {
        let mut env = buffed_env();
        env.buff_skills
            .push(curse_spec("Enfeeble", "Ring 1", 1, 7.0, false, false));
        env.buff_skills
            .push(curse_spec("Despair", "Boots", 1, 5.0, false, true));

        buff_pass(&mut env);

        let out = env.curse_pass_output.as_ref().unwrap();
        assert_eq!(
            out.curse_slots,
            vec!["Enfeeble".to_string(), "Despair".to_string()]
        );
        assert_eq!(
            env.enemy
                .mod_db
                .sum(ModType::Inc, &env.cfg, &[ModName::from("DamageTaken")]),
            12.0
        );
    }

    // ===== CurseEffect 乘区 + Condition:Effective 口径（vendor :2295-2316） =====

    /// 20% inc CurseEffect → hex 词条 ×1.2；敌侧 CurseEffectOnSelf MORE 只作用于
    /// hex（非 mark，:2303-2305）；缩放产物带 Condition:Effective（面板口径不命中）。
    #[test]
    fn curse_effect_scales_hex_not_mark() {
        let mut env = buffed_env();
        env.player
            .mod_db
            .add_mod(Modifier::number("CurseEffect", ModType::Inc, 20.0));
        env.enemy
            .mod_db
            .add_mod(Modifier::number("CurseEffectOnSelf", ModType::More, -50.0));
        env.buff_skills
            .push(curse_spec("Enfeeble", "Ring 1", 1, 10.0, false, false));
        env.buff_skills
            .push(curse_spec("Sniper's Mark", "Gloves", 1, 10.0, true, false));

        buff_pass(&mut env);

        let contributions =
            env.enemy
                .mod_db
                .contributions(ModType::Inc, &env.cfg, &[ModName::from("DamageTaken")]);
        let mut values: Vec<f64> = contributions.iter().map(|c| c.value).collect();
        values.sort_by(f64::total_cmp);
        // hex：10 × (1.2 × 0.5) = 6；mark：10 × 1.2 = 12。
        assert_eq!(values, vec![6.0, 12.0]);
        // 面板口径（mode_effective=false）不命中（Condition:Effective 门控）。
        let panel_cfg = CalcConfig::attack().with_mode_buffs(true);
        assert_eq!(
            env.enemy
                .mod_db
                .sum(ModType::Inc, &panel_cfg, &[ModName::from("DamageTaken")]),
            0.0
        );
        // 归因：无 origin 的 curse 词条回退 (Buff, "curse.<skill_id>")。
        let origin = contributions[0].origin.as_ref().expect("回退归因已附");
        assert_eq!(origin.source_id.kind, pobr_data::source::SourceKind::Buff);
        assert!(origin.source_id.id.starts_with("curse."));
    }

    /// 敌方 Hexproof：hex 整条跳过（vendor :2289 gate），mark 不受影响。
    #[test]
    fn hexproof_blocks_hex_but_not_mark() {
        let mut env = buffed_env();
        env.enemy.mod_db.add_mod(Modifier::flag("Hexproof"));
        env.buff_skills
            .push(curse_spec("Enfeeble", "Ring 1", 1, 10.0, false, false));
        env.buff_skills
            .push(curse_spec("Sniper's Mark", "Gloves", 1, 6.0, true, false));

        buff_pass(&mut env);

        assert_eq!(
            env.curse_pass_output.as_ref().unwrap().curse_slots,
            vec!["Sniper's Mark".to_string()]
        );
    }

    // ===== BuffSpec 兼容路径 / 双计防护（§6.1） =====

    /// 未消费 kind（如 Buff）走原值直注：词条不缩放、无条件置位（行为与现状一致）。
    #[test]
    fn unconsumed_kinds_pass_through_unscaled() {
        let mut env = buffed_env();
        env.player
            .mod_db
            .add_mod(Modifier::number("BuffEffect", ModType::Inc, 50.0));
        env.buff_skills.push(BuffSpec {
            name: "Onslaught".to_string(),
            kind: BuffKind::Buff,
            skill_id: "OnslaughtPlayer".to_string(),
            mods: vec![Modifier::number("ActionSpeed", ModType::Inc, 20.0)],
            magnitude: 1.0,
            slot: None,
            socket_index: 1,
            is_mark: false,
            ignore_curse_limit: false,
        });

        buff_pass(&mut env);

        assert_eq!(
            env.player
                .mod_db
                .sum(ModType::Inc, &env.cfg, &[ModName::from("ActionSpeed")]),
            20.0,
            "原值直注：BuffEffect 乘区不施加"
        );
        assert!(!env.cfg.condition("AffectedByOnslaught"));
    }

    // ===== aura 乘区（vendor :2102-2110） =====

    /// 20% inc AuraEffect + aura 给 100 ES → 120（蓝图 §6.5 fixture，单元级）。
    #[test]
    fn aura_effect_scales_buff_mods() {
        let mut env = buffed_env();
        env.player
            .mod_db
            .add_mod(Modifier::number("AuraEffect", ModType::Inc, 20.0));
        env.buff_skills.push(aura_spec("Discipline", 100.0));

        buff_pass(&mut env);

        assert_eq!(
            env.player
                .mod_db
                .sum(ModType::Base, &env.cfg, &[ModName::from("EnergyShield")]),
            120.0
        );
        assert!(env.cfg.condition("AffectedByAura"));
        assert!(env.cfg.condition("AffectedByDiscipline"));
        assert_eq!(env.cfg.multiplier("BuffOnSelf"), 1.0);
    }

    /// 同上 fixture 的端到端口径：CalculationSession → perform → 防御输出 ES = 120。
    #[test]
    fn aura_fixture_end_to_end_session() {
        use crate::calc::{CalculationSession, MinimalInput};

        let mut session = CalculationSession::new(MinimalInput {
            base_life: 100.0,
            ..Default::default()
        })
        .with_config(CalcConfig::attack().with_mode_buffs(true));
        session.add_modifiers([Modifier::number("AuraEffect", ModType::Inc, 20.0)]);
        session.add_buff_skill(aura_spec("Discipline", 100.0));

        session.perform_minimal();

        assert_eq!(session.output().energy_shield, 120.0);
    }

    /// MORE 乘区与 magnitude 一并进 mult（vendor :2104 `More(...) × calcLib.mod Magnitude`）。
    #[test]
    fn aura_more_and_magnitude_multiply() {
        let mut env = buffed_env();
        env.player
            .mod_db
            .add_mod(Modifier::number("AuraBuffEffect", ModType::More, 50.0));
        let mut spec = aura_spec("Grace", 100.0);
        spec.magnitude = 1.1;
        env.buff_skills.push(spec);

        buff_pass(&mut env);

        // mult = 1.5 × 1.1 = 1.65 → 100 × 1.65 = 165。
        assert_eq!(
            env.player
                .mod_db
                .sum(ModType::Base, &env.cfg, &[ModName::from("EnergyShield")]),
            165.0
        );
    }

    /// mergeBuff 同名取强（vendor :41-63）：同名 aura 两份 spec 不叠加，取数值大者。
    #[test]
    fn same_name_auras_merge_take_strongest() {
        let mut env = buffed_env();
        env.buff_skills.push(aura_spec("Discipline", 80.0));
        env.buff_skills.push(aura_spec("Discipline", 100.0));

        buff_pass(&mut env);

        assert_eq!(
            env.player
                .mod_db
                .sum(ModType::Base, &env.cfg, &[ModName::from("EnergyShield")]),
            100.0,
            "同名 buff 同参数词条取强不叠加"
        );
    }
}
