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
//! # 本切片范围（First slice）
//!
//! 仅实现「宝石词条 → 带归因 modifier → 进入最小计算 → 可回溯到宝石」的闭环。
//! 以下技能域复杂点**有意暂不实现**，留作后续切片（结构化 TODO）：
//!
//! - **TODO(mana-multiplier)**: 辅助宝石的 mana multiplier（PoB `ManaMultiplier`，
//!   作用于被支援技能的保留/消耗）尚未接入。需要被支援技能的 cost 上下文，且数值
//!   依赖 PoB Lua `SupportSkill` 数据，本切片不写死。
//! - **TODO(more-multiplier)**: 辅助宝石提供的 `more`/`less` 倍率会作用于被支援
//!   技能而非全局；当前直接注入 player `ModDb`，未做被支援技能隔离。其具体数值需按
//!   `agent-docs/` → PoB Lua → 官方核对，本切片不写死。
//! - **TODO(skill-type-gating)**: 宝石的 [`SkillTypes`](pobr_data::skill::SkillTypes)
//!   兼容（辅助宝石仅作用于匹配 skill type 的主动技能）尚未接入。需要把宝石的
//!   skill type 元数据注入 [`Modifier`] 的 `ModTag::SkillTypes` / `CalcConfig`。
//! - **TODO(level/quality)**: [`SourceKind::SkillLevel`] / [`SourceKind::GemQuality`]
//!   带来的 per-level / per-quality 词条尚未细分，本切片把宝石词条统一归到宝石级
//!   source 节点。

use pobr_data::prelude::*;

use crate::Modifier;
use crate::mod_parser::{ParseError, ParseStatus, parse_mod};

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

/// 一颗宝石接入计算的产物：解析出的 modifier + 无法解析的原始文本。
///
/// 与 `item` 域的 `ItemIngest` 对称。
#[derive(Debug, Clone, Default)]
pub struct GemIngest {
    pub modifiers: Vec<Modifier>,
    pub unsupported: Vec<String>,
}

/// 把一颗宝石的词条文本解析为带宝石归因的 modifier。
///
/// 解析失败（结构性错误）向上抛 [`ParseError`]；无法识别的词条不报错，收集进
/// [`GemIngest::unsupported`]，与 `CalculationSession` 的语义一致。
pub fn ingest_gem(gem: &GemModSource) -> Result<GemIngest, ParseError> {
    let source_id = gem.source_id();
    let parent_source_id = gem.parent_source_id();

    let mut ingest = GemIngest::default();
    for text in &gem.modifier_texts {
        let outcome = parse_mod(text)?;
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
