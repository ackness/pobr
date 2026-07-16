//! 技能宝石 modifier 来源接入。
//!
//! 把一颗宝石（主动技能 / 辅助宝石）携带的英文词条文本解析为带归因的 `Modifier`：
//! - 主动宝石 → [`SourceKind::SkillGem`]，`SourceId.id` = `gem.<gem_id>`；
//! - 辅助宝石 → [`SourceKind::SupportGem`]，`SourceId.id` = `support.<gem_id>`，
//!   并通过 [`ModifierSource::with_parent`] 关联到它所支援的主动技能 source，
//!   从而让最终输出能 source-level 回溯到具体宝石（PoBR 相对 PoB 的核心增量）。
//!
//! 与 `item` 域的「来源 → 解析词条 → 带归因 modifier + unsupported」范式对称。
//!
//! # 功能域
//!
//! ## mana-multiplier
//!
//! 辅助宝石可携带 `SupportManaMultiplier`（`More` 区间）作用于被支援技能的 mana cost。
//! 对照 PoB2 `CalcActiveSkill.lua`：
//! ```text
//! if level.manaMultiplier then
//!     skillModList:NewMod("SupportManaMultiplier", "MORE", level.manaMultiplier, ...)
//! end
//! ```
//! 调用方通过 [`SupportGemSpec::mana_multiplier`] 传入（如从宝石等级表读取），
//! [`ingest_support_gem`] 将其注入为 `ModName::SupportManaMultiplier` More modifier，
//! 归因到该辅助宝石的 `SourceId`。
//!
//! ## more-multiplier 隔离
//!
//! 辅助宝石携带的 `more`/`less` modifier 通过 [`ModTag::SkillTypes`] 约束到匹配的主动
//! 技能（`CalcConfig::skill_types.intersects(support.skill_types)`），保证只作用于被
//! 支援技能而非全局。[`SupportGemSpec::supported_skill_types`] 为 `SkillTypes::NONE` 时
//! 不附加 SkillTypes tag（默认无限制，与原行为兼容）。
//!
//! ## skill-type-gating（兼容性门控）
//!
//! 对照 PoB2 `CalcTools.lua:84-110 canGrantedEffectSupportActiveSkill` 的**全语义四段**：
//! ① 主动效果 `cannotBeSupported` → 拒；② support `supportGemsOnly` 且主动技能非宝石
//! 授予 → 拒；③ `excludeSkillTypes` 后缀表达式命中 → 拒；④ `requireSkillTypes`
//! 后缀表达式（空 = 接受）。表达式求值走 [`crate::rules::skill_type_expr`]。
//!
//! **defer**（蓝图 T3.4 裁决）：`fromItem` 特例（CalcTools.lua:93，物品授予 support，
//! M5c）、`isTrigger` 且非玩家 actor（CalcTools.lua:106，玩家 build 恒不触发，M5a）、
//! `minionTypes` 第二集合（CalcTools.lua:98-103，M5a minion 链路）。
//!
//! [`can_support`]/[`judge_support`] 供调用方在注入前裁决；[`ingest_support_gem`]
//! 被拒时返回 `Err(SupportIngestError::Gating)`。组级 addSkillTypes 不动点
//! （CalcActiveSkill.lua:179-210）在 pobr-build orchestrator 的
//! `judge_group_supports` 实现（契约 C2）。
//!
//! ## level/quality 缩放
//!
//! [`SourceKind::SkillLevel`]（等级词条归因）与 [`SourceKind::GemQuality`]（品质加成
//! 归因）通过 [`LeveledModifier`] / [`QualityModifier`] 独立传入，由 [`ingest_gem_leveled`]
//! 注入，归因到对应 source 节点，与宝石级别的 source 节点分开，便于 TraceGraph 逐层追踪。
//!
//! 出处：PoB2 `CalcActiveSkill.lua::initSkill`，`level.*` 字段（cost / duration /
//! reservationMultiplier / spiritReservationFlat 等）注入 `skillModList`。
//! PoBR 不维护等级表，数值由调用方提供；归因 id = `gem.<id>.level<N>` / `gem.<id>.q<Q>`。
//!
//! **品质数值口径（M1-T1）**：品质 stat 的数据源是
//! `overlay/gem_quality_stats.json`（`effect_id → [{stat, per_quality_rate}]`），
//! 叠加值 = `trunc(per_quality_rate × quality)`——**截断取整**（toward zero），对齐
//! PoB2 `CalcTools.lua:142` `math.modf(stat[2] * skillInstance.quality)`，非 floor。
//! parity 主路径（orchestrator）的取数实现在 `pobr-build::BuildData::effect_stats`
//! 的 quality 段；本模块（CLI/Session 最小路径）由调用方按同一口径预先算好
//! `quality_mods` 传入，两条路径共用 `SourceKind::GemQuality` 与同一归因 id 约定。

use std::collections::HashSet;

use pobr_data::prelude::*;

use crate::mod_parser::{ParseError, ParseStatus};
use crate::rules::skill_type_expr;
use crate::{ModTag, Modifier};

// ────────────────────────────────────────────────────
// Public error type
// ────────────────────────────────────────────────────

/// 辅助宝石兼容性门控失败（PoB2 四段裁决的拒绝原因，按裁决顺序）。
#[derive(Debug, Clone, PartialEq)]
pub enum SkillGatingError {
    /// 主动效果 `cannotBeSupported`——不可被任何 support 支援（裁决第一段）。
    CannotBeSupported,
    /// support `supportGemsOnly` 且主动技能非宝石授予（裁决第二段）。
    SupportGemsOnly,
    /// exclude 后缀表达式命中（裁决第三段）。
    Excluded { exclude: Vec<String> },
    /// require 后缀表达式未命中（裁决第四段；空 require 恒接受）。
    IncompatibleTypes {
        required: Vec<String>,
        active: Vec<String>,
    },
}

impl std::fmt::Display for SkillGatingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CannotBeSupported => write!(f, "active skill cannot be supported"),
            Self::SupportGemsOnly => {
                write!(f, "support gems only: active skill is not granted by a gem")
            }
            Self::Excluded { exclude } => {
                write!(f, "active skill matches exclude expression {exclude:?}")
            }
            Self::IncompatibleTypes { required, active } => {
                write!(
                    f,
                    "support gem requires skill types {required:?} but active skill has {active:?}",
                )
            }
        }
    }
}

impl std::error::Error for SkillGatingError {}

// ────────────────────────────────────────────────────
// GemModSource（原始词条文本载体，向后兼容）
// ────────────────────────────────────────────────────

/// 一颗宝石接入计算的最小输入：稳定宝石 id + 一组 modifier 文本 + 是否辅助宝石。
///
/// 这是技能域的最小载体（不依赖 [`Gem`](pobr_data::gem::Gem) 的等级/品质/granted_effect
/// 等暂不参与本切片的字段），仅承载「source-level 归因」闭环所需的信息。
#[derive(Debug, Clone)]
pub struct GemModSource {
    /// 稳定宝石 id（计算内部使用，显示文本走 i18n）。
    pub gem_id: String,
    /// 是否辅助宝石（true → support / false → active）。
    pub is_support: bool,
    /// 该宝石携带的 modifier 词条原文（逐行）。
    pub modifier_texts: Vec<String>,
    /// 辅助宝石所支援的主动技能宝石 id（active gem id）。
    ///
    /// 仅 support gem 有意义；用于把 modifier 关联到被支援主动技能的 source。
    /// 主动宝石恒为 `None`。信息不可得时留 `None`（不强行编造）。
    pub supported_gem_id: Option<String>,
}

impl GemModSource {
    /// 构造一颗主动技能宝石的 modifier 来源。
    pub fn active(
        gem_id: impl Into<String>,
        texts: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            gem_id: gem_id.into(),
            is_support: false,
            modifier_texts: texts.into_iter().map(Into::into).collect(),
            supported_gem_id: None,
        }
    }

    /// 构造一颗辅助宝石的 modifier 来源（默认未关联被支援技能）。
    pub fn support(
        gem_id: impl Into<String>,
        texts: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            gem_id: gem_id.into(),
            is_support: true,
            modifier_texts: texts.into_iter().map(Into::into).collect(),
            supported_gem_id: None,
        }
    }

    /// 声明该辅助宝石所支援的主动技能宝石 id（用于 modifier 的父 source 关联）。
    pub fn supporting(mut self, active_gem_id: impl Into<String>) -> Self {
        self.supported_gem_id = Some(active_gem_id.into());
        self
    }

    /// 该宝石自身的 [`SourceId`]：active → `gem.<id>` / support → `support.<id>`。
    fn source_id(&self) -> SourceId {
        if self.is_support {
            SourceId::new(SourceKind::SupportGem, format!("support.{}", self.gem_id))
        } else {
            SourceId::new(SourceKind::SkillGem, format!("gem.{}", self.gem_id))
        }
    }

    /// 被支援主动技能的 [`SourceId`]（仅辅助宝石且 `supported_gem_id` 可得时）。
    fn parent_source_id(&self) -> Option<SourceId> {
        self.supported_gem_id
            .as_ref()
            .map(|id| SourceId::new(SourceKind::SkillGem, format!("gem.{id}")))
    }
}

// ────────────────────────────────────────────────────
// SupportGemSpec — 辅助宝石完整规格（含 4 个 TODO 扩展）
// ────────────────────────────────────────────────────

/// 一颗**辅助宝石**的完整接入规格，覆盖 4 个扩展点。
///
/// 最简用法：只设置 `gem_id` + `modifier_texts`，其余字段默认值即为"不施加额外约束"。
#[derive(Debug, Clone)]
pub struct SupportGemSpec {
    /// 稳定宝石 id（计算内部使用）。
    pub gem_id: String,
    /// 该辅助宝石携带的 modifier 词条原文（逐行解析为带归因 modifier）。
    pub modifier_texts: Vec<String>,
    /// 被支援主动技能的宝石 id（用于 modifier parent source 关联）。
    /// `None` → 不关联父 source。
    pub supported_gem_id: Option<String>,

    // ── TODO(mana-multiplier) ──────────────────────
    /// 辅助宝石的法力倍乘（PoB2 `SupportManaMultiplier` More 区间）。
    ///
    /// 取值语义与 PoB2 `level.manaMultiplier` 一致：以 **百分比 more** 表示，
    /// 例如 `+40` 表示"被支援技能的 mana cost 额外 more 40%"（即 ×1.4）。
    /// `None` → 不注入 `SupportManaMultiplier` modifier。
    pub mana_multiplier: Option<f64>,

    // ── TODO(more-multiplier 隔离) ─────────────────
    /// 辅助宝石 more/less 倍率作用的目标 skill types（用于 `ModTag::SkillTypes` 隔离）。
    ///
    /// 若非空，解析出的所有 `More` modifier 将附加 `ModTag::SkillTypes(supported_skill_types)`，
    /// 确保只对 `CalcConfig::skill_types.intersects(this)` 的技能生效。
    /// `SkillTypes::NONE` → 不附加 tag（全局生效，与原行为兼容）。
    pub supported_skill_types: SkillTypes,

    // ── skill-type-gating（PoB2 四段裁决的 support 侧输入）─────────
    /// require 后缀表达式 token 流（兼容性门控第四段）。
    ///
    /// 对照 PoB2 `CalcTools.lua:84-110 canGrantedEffectSupportActiveSkill`：
    /// `requireSkillTypes` 非空时须经 `doesTypeExpressionMatch` 匹配主动技能
    /// 类型集合。空 → 无门控（默认，始终允许支援）。
    pub require_skill_types: Vec<String>,
    /// exclude 后缀表达式 token 流（兼容性门控第三段；命中即拒）。空 → 不排除。
    pub exclude_skill_types: Vec<String>,
    /// 仅能支援宝石授予的技能（兼容性门控第二段，PoB2 `supportGemsOnly`）。
    pub support_gems_only: bool,

    // ── TODO(level/quality 归因) ──────────────────
    /// 当前宝石等级（用于 `SourceKind::SkillLevel` 归因）。
    /// `None` → 不分等级归因，统一归到宝石级 source（原行为）。
    pub level: Option<u8>,
    /// 当前宝石品质（0–23，用于 `SourceKind::GemQuality` 归因）。
    /// `None` → 不注入品质归因。
    pub quality: Option<u8>,
    /// 因等级带来的额外 Base modifier 列表（`SourceKind::SkillLevel` 归因）。
    ///
    /// 每项 `(mod_name, base_value)` 例如 `("ManaCost", 10.0)`。
    /// 由调用方从宝石等级表读取后传入；PoBR 不持有等级表数据。
    pub level_mods: Vec<(String, ModType, f64)>,
    /// 因品质带来的额外 Base modifier 列表（`SourceKind::GemQuality` 归因）。
    ///
    /// 每项 `(mod_name, base_value)`，通常品质为百分比时 `base_value = quality * rate`。
    pub quality_mods: Vec<(String, ModType, f64)>,
}

impl SupportGemSpec {
    /// 最简构造：仅需 `gem_id` + 词条文本，其余字段取默认值（无门控、无等级、无品质）。
    pub fn new(
        gem_id: impl Into<String>,
        modifier_texts: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            gem_id: gem_id.into(),
            modifier_texts: modifier_texts.into_iter().map(Into::into).collect(),
            supported_gem_id: None,
            mana_multiplier: None,
            supported_skill_types: SkillTypes::NONE,
            require_skill_types: Vec::new(),
            exclude_skill_types: Vec::new(),
            support_gems_only: false,
            level: None,
            quality: None,
            level_mods: Vec::new(),
            quality_mods: Vec::new(),
        }
    }

    /// 设定被支援主动技能的宝石 id（用于 modifier parent source 关联）。
    pub fn supporting(mut self, active_gem_id: impl Into<String>) -> Self {
        self.supported_gem_id = Some(active_gem_id.into());
        self
    }

    /// 设定 mana multiplier（PoB2 `SupportManaMultiplier` More 区间，百分比值）。
    pub fn with_mana_multiplier(mut self, mult: f64) -> Self {
        self.mana_multiplier = Some(mult);
        self
    }

    /// 设定 more 倍率作用域 skill types（more-multiplier 隔离）。
    pub fn with_supported_skill_types(mut self, types: SkillTypes) -> Self {
        self.supported_skill_types = types;
        self
    }

    /// 设定 require 后缀表达式（skill-type-gating 第四段）。
    pub fn with_require_skill_types(
        mut self,
        tokens: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.require_skill_types = tokens.into_iter().map(Into::into).collect();
        self
    }

    /// 设定 exclude 后缀表达式（skill-type-gating 第三段）。
    pub fn with_exclude_skill_types(
        mut self,
        tokens: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.exclude_skill_types = tokens.into_iter().map(Into::into).collect();
        self
    }

    /// 设定 supportGemsOnly（skill-type-gating 第二段）。
    pub fn with_support_gems_only(mut self, only: bool) -> Self {
        self.support_gems_only = only;
        self
    }

    /// 设定宝石等级与等级 modifier（level/quality 缩放）。
    pub fn with_level(
        mut self,
        level: u8,
        mods: impl IntoIterator<Item = (impl Into<String>, ModType, f64)>,
    ) -> Self {
        self.level = Some(level);
        self.level_mods = mods.into_iter().map(|(n, t, v)| (n.into(), t, v)).collect();
        self
    }

    /// 设定宝石品质与品质 modifier（level/quality 缩放）。
    pub fn with_quality(
        mut self,
        quality: u8,
        mods: impl IntoIterator<Item = (impl Into<String>, ModType, f64)>,
    ) -> Self {
        self.quality = Some(quality);
        self.quality_mods = mods.into_iter().map(|(n, t, v)| (n.into(), t, v)).collect();
        self
    }

    /// 该辅助宝石自身的 [`SourceId`]。
    pub fn source_id(&self) -> SourceId {
        SourceId::new(SourceKind::SupportGem, format!("support.{}", self.gem_id))
    }

    /// 被支援主动技能的父 [`SourceId`]（若 `supported_gem_id` 可得）。
    pub fn parent_source_id(&self) -> Option<SourceId> {
        self.supported_gem_id
            .as_ref()
            .map(|id| SourceId::new(SourceKind::SkillGem, format!("gem.{id}")))
    }

    /// 等级 source id（`SourceKind::SkillLevel`，id = `gem.<id>.level<N>`）。
    pub fn level_source_id(&self) -> Option<SourceId> {
        self.level.map(|lvl| {
            SourceId::new(
                SourceKind::SkillLevel,
                format!("support.{}.level{}", self.gem_id, lvl),
            )
        })
    }

    /// 品质 source id（`SourceKind::GemQuality`，id = `gem.<id>.q<Q>`）。
    pub fn quality_source_id(&self) -> Option<SourceId> {
        self.quality.map(|q| {
            SourceId::new(
                SourceKind::GemQuality,
                format!("support.{}.q{}", self.gem_id, q),
            )
        })
    }
}

// ────────────────────────────────────────────────────
// ActiveSkillSpec — 主动技能规格（用于门控检查）
// ────────────────────────────────────────────────────

/// 主动技能的简要规格（skill-type-gating 时传入）。
#[derive(Debug, Clone)]
pub struct ActiveSkillSpec {
    /// 主动技能宝石 id。
    pub gem_id: String,
    /// 主动技能的 skill types（决定辅助宝石兼容性与 more 倍率隔离作用域）。
    pub skill_types: SkillTypes,
    /// 该宝石携带的 modifier 词条原文（逐行）。
    pub modifier_texts: Vec<String>,
}

impl ActiveSkillSpec {
    /// 构造一颗主动技能规格。
    pub fn new(
        gem_id: impl Into<String>,
        skill_types: SkillTypes,
        modifier_texts: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            gem_id: gem_id.into(),
            skill_types,
            modifier_texts: modifier_texts.into_iter().map(Into::into).collect(),
        }
    }

    /// 该主动技能的 [`SourceId`]。
    pub fn source_id(&self) -> SourceId {
        SourceId::new(SourceKind::SkillGem, format!("gem.{}", self.gem_id))
    }
}

// ────────────────────────────────────────────────────
// GemIngest — 解析产物
// ────────────────────────────────────────────────────

/// 一颗宝石接入计算的产物：解析出的 modifier + 无法解析的原始文本。
///
/// 与 `item` 域的 `ItemIngest` 对称。
#[derive(Debug, Clone, Default)]
pub struct GemIngest {
    pub modifiers: Vec<Modifier>,
    pub unsupported: Vec<String>,
}

// ────────────────────────────────────────────────────
// Public helpers — skill-type-gating check（PoB2 全语义四段）
// ────────────────────────────────────────────────────

/// 辅助效果侧的裁决输入（数据来自 `GrantedEffectDef` 的 support 行）。
#[derive(Debug, Clone, Copy, Default)]
pub struct SupportJudgeInput<'a> {
    /// 仅能支援宝石授予的技能（PoB2 `supportGemsOnly`）。
    pub support_gems_only: bool,
    /// exclude 后缀表达式 token 流（命中即拒）。
    pub exclude_skill_types: &'a [String],
    /// require 后缀表达式 token 流（空 = 接受一切）。
    pub require_skill_types: &'a [String],
}

/// 主动技能侧的裁决输入。
#[derive(Debug, Clone, Copy)]
pub struct ActiveSkillJudgeInput<'a> {
    /// 主动效果自身不可被支援（PoB2 `cannotBeSupported`，裁决第一段）。
    pub cannot_be_supported: bool,
    /// 主动技能是否来自宝石（PoB2 `activeEffect.gemData` 存在）。socket group 内
    /// 宝石技能恒为 `true`；物品授予技能（fromItem）M5c 接入时为 `false`。
    pub from_gem: bool,
    /// 当前技能类型集合（组级裁决不动点过程中含已并入的 addSkillTypes）。
    pub skill_types: &'a HashSet<String>,
}

/// 裁决辅助宝石能否支援主动技能，返回拒绝原因。
///
/// 对照 PoB2 `CalcTools.lua:84-110 canGrantedEffectSupportActiveSkill` 的四段顺序：
/// 1. 主动效果 `cannotBeSupported` → 拒（CalcTools.lua:86-88）；
/// 2. support `supportGemsOnly` 且主动技能非宝石授予 → 拒（:89-91）；
/// 3. `excludeSkillTypes` 后缀表达式命中 → 拒（:104-105）；
/// 4. `requireSkillTypes` 后缀表达式：空 = 接受，否则须匹配（:109）。
///
/// **defer**：`fromItem` 特例（:93，M5c）、`isTrigger` 非玩家 actor（:106-108，
/// 玩家 build 恒不触发，M5a）、`minionTypes` 第二集合（:98-103，M5a）。
pub fn judge_support(
    support: &SupportJudgeInput<'_>,
    active: &ActiveSkillJudgeInput<'_>,
) -> Result<(), SkillGatingError> {
    if active.cannot_be_supported {
        return Err(SkillGatingError::CannotBeSupported);
    }
    if support.support_gems_only && !active.from_gem {
        return Err(SkillGatingError::SupportGemsOnly);
    }
    if !support.exclude_skill_types.is_empty()
        && skill_type_expr::matches(support.exclude_skill_types, active.skill_types)
    {
        return Err(SkillGatingError::Excluded {
            exclude: support.exclude_skill_types.to_vec(),
        });
    }
    if support.require_skill_types.is_empty()
        || skill_type_expr::matches(support.require_skill_types, active.skill_types)
    {
        Ok(())
    } else {
        let mut active_sorted: Vec<String> = active.skill_types.iter().cloned().collect();
        active_sorted.sort();
        Err(SkillGatingError::IncompatibleTypes {
            required: support.require_skill_types.to_vec(),
            active: active_sorted,
        })
    }
}

/// [`judge_support`] 的布尔便捷封装（契约 C2 的 orchestrator 消费形态）。
pub fn can_support(support: &SupportJudgeInput<'_>, active: &ActiveSkillJudgeInput<'_>) -> bool {
    judge_support(support, active).is_ok()
}

// ────────────────────────────────────────────────────
// ingest_gem_with_ctx — 宝石词条接入（GemModSource）
// ────────────────────────────────────────────────────

/// 把一颗宝石的词条文本解析为带宝石归因的 modifier。
///
/// 解析失败（结构性错误）向上抛 [`ParseError`]；无法识别的词条不报错，收集进
/// [`GemIngest::unsupported`]，与 `CalculationSession` 的语义一致。
pub fn ingest_gem_with_ctx(
    gem: &GemModSource,
    ctx: crate::mod_parser::ParseCtx<'_>,
) -> Result<GemIngest, ParseError> {
    let source_id = gem.source_id();
    let parent_source_id = gem.parent_source_id();

    let mut ingest = GemIngest::default();
    for text in &gem.modifier_texts {
        let outcome = ctx.parse(text)?;
        match outcome.status {
            ParseStatus::Parsed => {
                for modifier in outcome.mods {
                    let mut origin =
                        ModifierSource::new(source_id.clone()).with_raw_text(text.clone());
                    if let Some(parent) = &parent_source_id {
                        origin = origin.with_parent(parent.clone());
                    }
                    ingest.modifiers.push(modifier.with_origin(origin));
                }
            }
            ParseStatus::Unsupported => {
                if let Some(unparsed) = outcome.unparsed {
                    ingest.unsupported.push(unparsed);
                }
            }
        }
    }

    Ok(ingest)
}

// ────────────────────────────────────────────────────
// ingest_active_gem — 主动技能宝石接入
// ────────────────────────────────────────────────────

/// 把一颗主动技能宝石接入计算，产出带 `SourceKind::SkillGem` 归因的 modifier。
/// 词条解析走 `ctx`。
pub fn ingest_active_gem_with_ctx(
    spec: &ActiveSkillSpec,
    ctx: crate::mod_parser::ParseCtx<'_>,
) -> Result<GemIngest, ParseError> {
    let source_id = spec.source_id();
    let mut ingest = GemIngest::default();

    for text in &spec.modifier_texts {
        let outcome = ctx.parse(text)?;
        match outcome.status {
            ParseStatus::Parsed => {
                for modifier in outcome.mods {
                    let origin = ModifierSource::new(source_id.clone()).with_raw_text(text.clone());
                    ingest.modifiers.push(modifier.with_origin(origin));
                }
            }
            ParseStatus::Unsupported => {
                if let Some(unparsed) = outcome.unparsed {
                    ingest.unsupported.push(unparsed);
                }
            }
        }
    }

    Ok(ingest)
}

// ────────────────────────────────────────────────────
// ingest_support_gem — 辅助宝石完整接入（4 TODO）
// ────────────────────────────────────────────────────

/// 辅助宝石接入计算的错误。
#[derive(Debug)]
pub enum SupportIngestError {
    Parse(ParseError),
    Gating(SkillGatingError),
}

impl From<ParseError> for SupportIngestError {
    fn from(e: ParseError) -> Self {
        Self::Parse(e)
    }
}

impl From<SkillGatingError> for SupportIngestError {
    fn from(e: SkillGatingError) -> Self {
        Self::Gating(e)
    }
}

impl std::fmt::Display for SupportIngestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "parse error: {e}"),
            Self::Gating(e) => write!(f, "skill gating: {e}"),
        }
    }
}

impl std::error::Error for SupportIngestError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Parse(e) => Some(e),
            Self::Gating(e) => Some(e),
        }
    }
}

/// 把一颗辅助宝石完整接入计算，实现 4 个扩展点：
///
/// 1. **mana-multiplier**：若 `spec.mana_multiplier` 有值，注入
///    `ModName("SupportManaMultiplier")` More modifier（归因到辅助宝石 source）。
/// 2. **more-multiplier 隔离**：若 `spec.supported_skill_types` 非空，给所有 `More`
///    modifier 附加 `ModTag::SkillTypes`，使其只对匹配的技能生效。
/// 3. **skill-type-gating**：经 [`judge_support`]（PoB2 四段裁决）检查与
///    `active_skill_types` 的兼容性，被拒返回 `Err(SupportIngestError::Gating(...))`。
///    本最小路径假定主动技能来自宝石且可被支援（`cannot_be_supported=false`、
///    `from_gem=true`）；完整主动侧输入 + 组级 addSkillTypes 不动点在 orchestrator
///    的 `judge_group_supports`（契约 C2）。
/// 4. **level/quality 归因**：把 `spec.level_mods` / `spec.quality_mods` 注入为
///    `SourceKind::SkillLevel` / `SourceKind::GemQuality` 归因的 modifier。
pub fn ingest_support_gem_with_ctx(
    spec: &SupportGemSpec,
    active_skill_types: &HashSet<String>,
    ctx: crate::mod_parser::ParseCtx<'_>,
) -> Result<GemIngest, SupportIngestError> {
    // ── skill-type-gating（PoB2 四段裁决，CalcTools.lua:84-110）────────────
    judge_support(
        &SupportJudgeInput {
            support_gems_only: spec.support_gems_only,
            exclude_skill_types: &spec.exclude_skill_types,
            require_skill_types: &spec.require_skill_types,
        },
        &ActiveSkillJudgeInput {
            cannot_be_supported: false,
            from_gem: true,
            skill_types: active_skill_types,
        },
    )?;

    let source_id = spec.source_id();
    let parent_source_id = spec.parent_source_id();
    let mut ingest = GemIngest::default();

    // ── TODO(mana-multiplier) ────────────────────────────────────────────────
    // 对照 PoB2 CalcActiveSkill.lua:
    //   skillModList:NewMod("SupportManaMultiplier", "MORE", level.manaMultiplier, ...)
    // "MORE" 区间，数值单位为百分比（如 40 = +40% more = ×1.4）。
    if let Some(mult) = spec.mana_multiplier {
        let mut origin = ModifierSource::new(source_id.clone())
            .with_raw_text(format!("SupportManaMultiplier +{mult}% more"));
        if let Some(parent) = &parent_source_id {
            origin = origin.with_parent(parent.clone());
        }
        let modifier =
            Modifier::number("SupportManaMultiplier", ModType::More, mult).with_origin(origin);
        ingest.modifiers.push(modifier);
    }

    // ── 词条文本解析（含 more-multiplier 隔离）──────────────────────────────
    for text in &spec.modifier_texts {
        let outcome = ctx.parse(text)?;
        match outcome.status {
            ParseStatus::Parsed => {
                for modifier in outcome.mods {
                    let mut origin =
                        ModifierSource::new(source_id.clone()).with_raw_text(text.clone());
                    if let Some(parent) = &parent_source_id {
                        origin = origin.with_parent(parent.clone());
                    }
                    let modifier = modifier.with_origin(origin);

                    // ── TODO(more-multiplier 隔离) ─────────────────────────
                    // 若辅助宝石指定了目标 skill types，在 More modifier 上附加
                    // SkillTypes tag，保证只作用于被支援技能（当 CalcConfig 中
                    // skill_types 与之有交集时才 matches）。
                    let modifier = if !spec.supported_skill_types.is_empty()
                        && modifier.mod_type == ModType::More
                    {
                        modifier.with_tag(ModTag::SkillTypes(spec.supported_skill_types))
                    } else {
                        modifier
                    };

                    ingest.modifiers.push(modifier);
                }
            }
            ParseStatus::Unsupported => {
                if let Some(unparsed) = outcome.unparsed {
                    ingest.unsupported.push(unparsed);
                }
            }
        }
    }

    // ── TODO(level/quality 缩放) ─────────────────────────────────────────────
    // level_mods → SourceKind::SkillLevel 归因
    if let Some(level_source) = spec.level_source_id() {
        for (name, mod_type, value) in &spec.level_mods {
            let mut origin = ModifierSource::new(level_source.clone())
                .with_raw_text(format!("{name} level-{}", spec.level.unwrap_or(0)));
            if let Some(parent) = &parent_source_id {
                origin = origin.with_parent(parent.clone());
            }
            let modifier = Modifier::number(ModName::from(name.as_str()), *mod_type, *value)
                .with_origin(origin);
            ingest.modifiers.push(modifier);
        }
    }

    // quality_mods → SourceKind::GemQuality 归因
    if let Some(quality_source) = spec.quality_source_id() {
        for (name, mod_type, value) in &spec.quality_mods {
            let mut origin = ModifierSource::new(quality_source.clone())
                .with_raw_text(format!("{name} q{}", spec.quality.unwrap_or(0)));
            if let Some(parent) = &parent_source_id {
                origin = origin.with_parent(parent.clone());
            }
            let modifier = Modifier::number(ModName::from(name.as_str()), *mod_type, *value)
                .with_origin(origin);
            ingest.modifiers.push(modifier);
        }
    }

    Ok(ingest)
}

// ────────────────────────────────────────────────────
// ingest_gem_leveled — 主动技能宝石等级/品质归因版
// ────────────────────────────────────────────────────

/// 主动技能宝石携带的等级/品质 modifier 接入。
///
/// `level_mods` 归因到 `SourceKind::SkillLevel`（id = `gem.<id>.level<N>`），
/// `quality_mods` 归因到 `SourceKind::GemQuality`（id = `gem.<id>.q<Q>`）。
pub fn ingest_gem_leveled(
    gem_id: &str,
    level: u8,
    quality: u8,
    level_mods: &[(String, ModType, f64)],
    quality_mods: &[(String, ModType, f64)],
) -> GemIngest {
    let mut ingest = GemIngest::default();
    let gem_source = SourceId::new(SourceKind::SkillGem, format!("gem.{gem_id}"));

    let level_source = SourceId::new(SourceKind::SkillLevel, format!("gem.{gem_id}.level{level}"));
    for (name, mod_type, value) in level_mods {
        let origin = ModifierSource::new(level_source.clone())
            .with_parent(gem_source.clone())
            .with_raw_text(format!("{name} level-{level}"));
        let modifier =
            Modifier::number(ModName::from(name.as_str()), *mod_type, *value).with_origin(origin);
        ingest.modifiers.push(modifier);
    }

    let quality_source = SourceId::new(SourceKind::GemQuality, format!("gem.{gem_id}.q{quality}"));
    for (name, mod_type, value) in quality_mods {
        let origin = ModifierSource::new(quality_source.clone())
            .with_parent(gem_source.clone())
            .with_raw_text(format!("{name} q{quality}"));
        let modifier =
            Modifier::number(ModName::from(name.as_str()), *mod_type, *value).with_origin(origin);
        ingest.modifiers.push(modifier);
    }

    ingest
}
