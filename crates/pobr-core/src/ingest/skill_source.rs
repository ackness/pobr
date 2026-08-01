//! Skill gem modifier source ingest.
//!
//! Parses a gem's (active skill / support gem) English modifier text into
//! attributed `Modifier`s:
//! - Active gems → [`SourceKind::SkillGem`], `SourceId.id` = `gem.<gem_id>`;
//! - Support gems → [`SourceKind::SupportGem`], `SourceId.id` =
//!   `support.<gem_id>`, linked to the active skill source it supports via
//!   [`ModifierSource::with_parent`], so the final output can be traced
//!   source-level back to a specific gem (PoBR's core value-add over PoB).
//!
//! Mirrors the `item` domain's "source → parse modifiers → attributed
//! modifiers + unsupported" pattern.
//!
//! # Feature areas
//!
//! ## mana-multiplier
//!
//! A support gem can carry a `SupportManaMultiplier` (`More` range) applying
//! to the supported skill's mana cost. Mirrors PoB2 `CalcActiveSkill.lua`:
//! ```text
//! if level.manaMultiplier then
//!     skillModList:NewMod("SupportManaMultiplier", "MORE", level.manaMultiplier, ...)
//! end
//! ```
//! The caller supplies it via [`SupportGemSpec::mana_multiplier`] (e.g. read
//! from a gem level table), and [`ingest_support_gem`] injects it as a
//! `ModName::SupportManaMultiplier` More modifier, attributed to that support
//! gem's `SourceId`.
//!
//! ## more-multiplier isolation
//!
//! A support gem's `more`/`less` modifiers are constrained to the matching
//! active skill via [`ModTag::SkillTypes`]
//! (`CalcConfig::skill_types.intersects(support.skill_types)`), ensuring they
//! only affect the supported skill rather than applying globally. When
//! [`SupportGemSpec::supported_skill_types`] is `SkillTypes::NONE`, no
//! SkillTypes tag is attached (unrestricted by default, matching the
//! original behavior).
//!
//! ## skill-type-gating (compatibility gate)
//!
//! Mirrors the **full four-stage semantics** of PoB2's
//! `CalcTools.lua:84-110 canGrantedEffectSupportActiveSkill`: ① the active
//! effect's `cannotBeSupported` → reject; ② support's `supportGemsOnly` when
//! the active skill isn't gem-granted → reject; ③ the `excludeSkillTypes`
//! suffix expression matches → reject; ④ the `requireSkillTypes` suffix
//! expression (empty = accept). Expression evaluation goes through
//! [`crate::rules::skill_type_expr`].
//!
//! **Deferred**: the `fromItem` special case (CalcTools.lua:93, item-granted
//! supports), `isTrigger` for non-player actors (CalcTools.lua:106, player
//! builds never trigger this), and the second `minionTypes` set
//! (CalcTools.lua:98-103, the minion pathway).
//!
//! [`can_support`]/[`judge_support`] let the caller decide before injecting;
//! [`ingest_support_gem`] returns `Err(SupportIngestError::Gating)` when
//! rejected. The group-level addSkillTypes fixed point
//! (CalcActiveSkill.lua:179-210) is implemented by pobr-build
//! orchestrator's `judge_group_supports` (contract C2).
//!
//! ## level/quality scaling
//!
//! [`SourceKind::SkillLevel`] (level modifier attribution) and
//! [`SourceKind::GemQuality`] (quality bonus attribution) are supplied
//! separately via [`LeveledModifier`] / [`QualityModifier`] and injected by
//! [`ingest_gem_leveled`], attributed to their own source nodes distinct from
//! the gem-level source node, so TraceGraph can track them layer by layer.
//!
//! Source: PoB2 `CalcActiveSkill.lua::initSkill`, where `level.*` fields
//! (cost / duration / reservationMultiplier / spiritReservationFlat, etc.)
//! are injected into `skillModList`. PoBR doesn't maintain a level table —
//! the values are supplied by the caller; attribution id =
//! `gem.<id>.level<N>` / `gem.<id>.q<Q>`.
//!
//! **Quality value semantics**: quality stats are sourced from
//! `overlay/gem_quality_stats.json` (`effect_id → [{stat, per_quality_rate}]`),
//! with the stacked value = `trunc(per_quality_rate × quality)` — **truncated
//! toward zero**, matching PoB2 `CalcTools.lua:142`'s
//! `math.modf(stat[2] * skillInstance.quality)`, not floor. The parity main
//! path's (orchestrator) value-reading implementation lives in
//! `pobr-build::BuildData::effect_stats`'s quality section; this module (the
//! CLI/Session minimal path) has the caller pre-compute `quality_mods` under
//! the same semantics and pass it in — both paths share `SourceKind::GemQuality`
//! and the same attribution id convention.

use std::collections::HashSet;

use pobr_data::prelude::*;

use crate::mod_parser::{ParseError, ParseStatus};
use crate::rules::skill_type_expr;
use crate::{ModTag, Modifier};

// Public error type

/// Support gem compatibility gate failure (rejection reasons from PoB2's four-stage decision, in decision order).
#[derive(Debug, Clone, PartialEq)]
pub enum SkillGatingError {
    /// The active effect's `cannotBeSupported` — can't be supported by any support gem (decision stage 1).
    CannotBeSupported,
    /// support's `supportGemsOnly` when the active skill isn't gem-granted (decision stage 2).
    SupportGemsOnly,
    /// The exclude suffix expression matched (decision stage 3).
    Excluded { exclude: Vec<String> },
    /// The require suffix expression didn't match (decision stage 4: an empty require always accepts).
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

// GemModSource (raw modifier text carrier, kept for backward compatibility)

/// The minimal input for ingesting a gem: a stable gem id + a set of modifier texts + whether it's a support gem.
///
/// This is the skill domain's minimal carrier (doesn't depend on
/// [`Gem`](pobr_data::gem::Gem)'s level/quality/granted_effect and other
/// fields not yet part of this slice), holding only what's needed for the
/// "source-level attribution" loop.
#[derive(Debug, Clone)]
pub struct GemModSource {
    /// The stable gem id (used internally for calculation; display text goes through i18n).
    pub gem_id: String,
    /// Whether this is a support gem (true → support / false → active).
    pub is_support: bool,
    /// This gem's raw modifier text (one line at a time).
    pub modifier_texts: Vec<String>,
    /// The active skill gem id (active gem id) this support gem supports.
    ///
    /// Only meaningful for support gems; used to link modifiers to the
    /// supported active skill's source. Always `None` for active gems. Left
    /// `None` when the information isn't available (never fabricated).
    pub supported_gem_id: Option<String>,
}

impl GemModSource {
    /// Constructs the modifier source for an active skill gem.
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

    /// Constructs the modifier source for a support gem (no supported skill linked by default).
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

    /// Declares the active skill gem id this support gem supports (for modifier parent-source linking).
    pub fn supporting(mut self, active_gem_id: impl Into<String>) -> Self {
        self.supported_gem_id = Some(active_gem_id.into());
        self
    }

    /// This gem's own [`SourceId`]: active → `gem.<id>` / support → `support.<id>`.
    fn source_id(&self) -> SourceId {
        if self.is_support {
            SourceId::new(SourceKind::SupportGem, format!("support.{}", self.gem_id))
        } else {
            SourceId::new(SourceKind::SkillGem, format!("gem.{}", self.gem_id))
        }
    }

    /// The supported active skill's [`SourceId`] (only when this is a support gem and `supported_gem_id` is available).
    fn parent_source_id(&self) -> Option<SourceId> {
        self.supported_gem_id
            .as_ref()
            .map(|id| SourceId::new(SourceKind::SkillGem, format!("gem.{id}")))
    }
}

// SupportGemSpec — the full support-gem spec (covers 4 TODO extensions)

/// The full ingest spec for a **support gem**, covering 4 extension points.
///
/// Minimal usage: set only `gem_id` + `modifier_texts`; every other field
/// defaults to "no extra constraint applied".
#[derive(Debug, Clone)]
pub struct SupportGemSpec {
    /// The stable gem id (used internally for calculation).
    pub gem_id: String,
    /// This support gem's raw modifier text (parsed line by line into attributed modifiers).
    pub modifier_texts: Vec<String>,
    /// The supported active skill's gem id (for modifier parent-source linking).
    /// `None` → no parent source is linked.
    pub supported_gem_id: Option<String>,

    // TODO(mana-multiplier)
    /// The support gem's mana multiplier (PoB2's `SupportManaMultiplier` More range).
    ///
    /// Semantics match PoB2's `level.manaMultiplier`: expressed as a
    /// **percentage more**, e.g. `+40` means "the supported skill's mana cost
    /// gets an extra 40% more" (i.e. ×1.4). `None` → no
    /// `SupportManaMultiplier` modifier is injected.
    pub mana_multiplier: Option<f64>,

    // TODO(more-multiplier isolation)
    /// The skill types the support gem's more/less multiplier applies to (for `ModTag::SkillTypes` isolation).
    ///
    /// If non-empty, every parsed `More` modifier gets a
    /// `ModTag::SkillTypes(supported_skill_types)` tag attached, ensuring it
    /// only applies to skills where
    /// `CalcConfig::skill_types.intersects(this)`. `SkillTypes::NONE` → no
    /// tag attached (applies globally, matching the original behavior).
    pub supported_skill_types: SkillTypes,

    // skill-type-gating
    /// The require suffix expression token stream (compatibility gate stage 4).
    ///
    /// Mirrors PoB2's `CalcTools.lua:84-110 canGrantedEffectSupportActiveSkill`:
    /// when `requireSkillTypes` is non-empty, it must match the active
    /// skill's type set via `doesTypeExpressionMatch`. Empty → no gate
    /// (default, always allows supporting).
    pub require_skill_types: Vec<String>,
    /// The exclude suffix expression token stream (compatibility gate stage 3; a match rejects). Empty → nothing excluded.
    pub exclude_skill_types: Vec<String>,
    /// Only able to support gem-granted skills (compatibility gate stage 2, PoB2's `supportGemsOnly`).
    pub support_gems_only: bool,

    // TODO(level/quality attribution)
    /// The current gem level (for `SourceKind::SkillLevel` attribution).
    /// `None` → no level-specific attribution, folded into the gem-level source (original behavior).
    pub level: Option<u8>,
    /// The current gem quality (0–23, for `SourceKind::GemQuality` attribution).
    /// `None` → no quality attribution is injected.
    pub quality: Option<u8>,
    /// Extra Base modifiers from level (attributed to `SourceKind::SkillLevel`).
    ///
    /// Each entry is `(mod_name, base_value)`, e.g. `("ManaCost", 10.0)`.
    /// Supplied by the caller after reading a gem level table; PoBR doesn't
    /// hold level table data itself.
    pub level_mods: Vec<(String, ModType, f64)>,
    /// Extra Base modifiers from quality (attributed to `SourceKind::GemQuality`).
    ///
    /// Each entry is `(mod_name, base_value)`; when quality is a percentage,
    /// `base_value` is typically `quality * rate`.
    pub quality_mods: Vec<(String, ModType, f64)>,
}

impl SupportGemSpec {
    /// The minimal constructor: only `gem_id` + modifier text is required; every other field takes its default (no gating, no level, no quality).
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

    /// Sets the supported active skill's gem id (for modifier parent-source linking).
    pub fn supporting(mut self, active_gem_id: impl Into<String>) -> Self {
        self.supported_gem_id = Some(active_gem_id.into());
        self
    }

    /// Sets the mana multiplier (PoB2's `SupportManaMultiplier` More range, a percentage value).
    pub fn with_mana_multiplier(mut self, mult: f64) -> Self {
        self.mana_multiplier = Some(mult);
        self
    }

    /// Sets the skill types the more multiplier applies to (more-multiplier isolation).
    pub fn with_supported_skill_types(mut self, types: SkillTypes) -> Self {
        self.supported_skill_types = types;
        self
    }

    /// Sets the require suffix expression (skill-type-gating stage 4).
    pub fn with_require_skill_types(
        mut self,
        tokens: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.require_skill_types = tokens.into_iter().map(Into::into).collect();
        self
    }

    /// Sets the exclude suffix expression (skill-type-gating stage 3).
    pub fn with_exclude_skill_types(
        mut self,
        tokens: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.exclude_skill_types = tokens.into_iter().map(Into::into).collect();
        self
    }

    /// Sets supportGemsOnly (skill-type-gating stage 2).
    pub fn with_support_gems_only(mut self, only: bool) -> Self {
        self.support_gems_only = only;
        self
    }

    /// Sets the gem level and its level modifiers (level/quality scaling).
    pub fn with_level(
        mut self,
        level: u8,
        mods: impl IntoIterator<Item = (impl Into<String>, ModType, f64)>,
    ) -> Self {
        self.level = Some(level);
        self.level_mods = mods.into_iter().map(|(n, t, v)| (n.into(), t, v)).collect();
        self
    }

    /// Sets the gem quality and its quality modifiers (level/quality scaling).
    pub fn with_quality(
        mut self,
        quality: u8,
        mods: impl IntoIterator<Item = (impl Into<String>, ModType, f64)>,
    ) -> Self {
        self.quality = Some(quality);
        self.quality_mods = mods.into_iter().map(|(n, t, v)| (n.into(), t, v)).collect();
        self
    }

    /// This support gem's own [`SourceId`].
    pub fn source_id(&self) -> SourceId {
        SourceId::new(SourceKind::SupportGem, format!("support.{}", self.gem_id))
    }

    /// The supported active skill's parent [`SourceId`] (if `supported_gem_id` is available).
    pub fn parent_source_id(&self) -> Option<SourceId> {
        self.supported_gem_id
            .as_ref()
            .map(|id| SourceId::new(SourceKind::SkillGem, format!("gem.{id}")))
    }

    /// The level source id (`SourceKind::SkillLevel`, id = `gem.<id>.level<N>`).
    pub fn level_source_id(&self) -> Option<SourceId> {
        self.level.map(|lvl| {
            SourceId::new(
                SourceKind::SkillLevel,
                format!("support.{}.level{}", self.gem_id, lvl),
            )
        })
    }

    /// The quality source id (`SourceKind::GemQuality`, id = `gem.<id>.q<Q>`).
    pub fn quality_source_id(&self) -> Option<SourceId> {
        self.quality.map(|q| {
            SourceId::new(
                SourceKind::GemQuality,
                format!("support.{}.q{}", self.gem_id, q),
            )
        })
    }
}

// ActiveSkillSpec — active skill spec (used for gating checks)

/// A brief active-skill spec (supplied for skill-type-gating).
#[derive(Debug, Clone)]
pub struct ActiveSkillSpec {
    /// The active skill gem id.
    pub gem_id: String,
    /// The active skill's skill types (determines support-gem compatibility and more-multiplier isolation scope).
    pub skill_types: SkillTypes,
    /// This gem's raw modifier text (one line at a time).
    pub modifier_texts: Vec<String>,
}

impl ActiveSkillSpec {
    /// Constructs an active skill spec.
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

    /// This active skill's [`SourceId`].
    pub fn source_id(&self) -> SourceId {
        SourceId::new(SourceKind::SkillGem, format!("gem.{}", self.gem_id))
    }
}

// GemIngest — the parse result

/// Result of ingesting a gem: parsed modifiers + raw text that couldn't be parsed.
///
/// Mirrors the `item` domain's `ItemIngest`.
#[derive(Debug, Clone, Default)]
pub struct GemIngest {
    pub modifiers: Vec<Modifier>,
    pub unsupported: Vec<String>,
}

// Public helpers — skill-type-gating check (PoB2's full four-stage semantics)

/// The support-effect side's decision input (data sourced from `GrantedEffectDef`'s support rows).
#[derive(Debug, Clone, Copy, Default)]
pub struct SupportJudgeInput<'a> {
    /// Only able to support gem-granted skills (PoB2's `supportGemsOnly`).
    pub support_gems_only: bool,
    /// The exclude suffix expression token stream (a match rejects).
    pub exclude_skill_types: &'a [String],
    /// The require suffix expression token stream (empty = accept everything).
    pub require_skill_types: &'a [String],
}

/// The active-skill side's decision input.
#[derive(Debug, Clone, Copy)]
pub struct ActiveSkillJudgeInput<'a> {
    /// The active effect itself can't be supported (PoB2's `cannotBeSupported`, decision stage 1).
    pub cannot_be_supported: bool,
    /// Whether the active skill comes from a gem (PoB2's `activeEffect.gemData` is present). A
    /// gem-based skill in a socket group is always `true`; an item-granted
    /// skill (fromItem) is `false` when ingested.
    pub from_gem: bool,
    /// The current skill type set (includes any addSkillTypes already merged in during the group-level decision's fixed-point process).
    pub skill_types: &'a HashSet<String>,
}

/// Decides whether a support gem can support an active skill, returning the rejection reason if not.
///
/// Mirrors the four-stage order of PoB2's
/// `CalcTools.lua:84-110 canGrantedEffectSupportActiveSkill`:
/// 1. The active effect's `cannotBeSupported` → reject (CalcTools.lua:86-88);
/// 2. support's `supportGemsOnly` when the active skill isn't gem-granted → reject (:89-91);
/// 3. the `excludeSkillTypes` suffix expression matches → reject (:104-105);
/// 4. the `requireSkillTypes` suffix expression: empty = accept, otherwise must match (:109).
///
/// **Deferred**: the `fromItem` special case (:93), `isTrigger` for
/// non-player actors (:106-108, player builds never trigger this), and the
/// second `minionTypes` set (:98-103).
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

/// A boolean convenience wrapper around [`judge_support`] (the consumption form used by contract C2's orchestrator).
pub fn can_support(support: &SupportJudgeInput<'_>, active: &ActiveSkillJudgeInput<'_>) -> bool {
    judge_support(support, active).is_ok()
}

// ingest_gem_with_ctx — gem modifier ingest (GemModSource)

/// Parses a gem's modifier text into gem-attributed modifiers.
///
/// Parse failures (structural errors) propagate as [`ParseError`];
/// unrecognized modifiers don't error, they're collected into
/// [`GemIngest::unsupported`] instead, matching `CalculationSession`'s
/// semantics.
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

// ingest_active_gem — active skill gem ingest

/// Ingests an active skill gem into the calculation, producing modifiers
/// attributed to `SourceKind::SkillGem`. Modifier parsing goes through `ctx`.
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

// ingest_support_gem — full support-gem ingest (4 TODOs)

/// An error from ingesting a support gem.
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

/// Fully ingests a support gem into the calculation, implementing 4 extension points:
///
/// 1. **mana-multiplier**: if `spec.mana_multiplier` has a value, injects a
///    `ModName("SupportManaMultiplier")` More modifier (attributed to the
///    support gem's source).
/// 2. **more-multiplier isolation**: if `spec.supported_skill_types` is
///    non-empty, attaches `ModTag::SkillTypes` to every `More` modifier so it
///    only applies to matching skills.
/// 3. **skill-type-gating**: checks compatibility with `active_skill_types`
///    via [`judge_support`], returning `Err(SupportIngestError::Gating(...))`
///    when rejected. This minimal path assumes the active skill comes from a
///    gem and can be supported (`cannot_be_supported=false`,
///    `from_gem=true`); the full active-side input + the group-level
///    addSkillTypes fixed point live in the orchestrator's
///    `judge_group_supports` (contract C2).
/// 4. **level/quality attribution**: injects `spec.level_mods` /
///    `spec.quality_mods` as modifiers attributed to `SourceKind::SkillLevel`
///    / `SourceKind::GemQuality`.
pub fn ingest_support_gem_with_ctx(
    spec: &SupportGemSpec,
    active_skill_types: &HashSet<String>,
    ctx: crate::mod_parser::ParseCtx<'_>,
) -> Result<GemIngest, SupportIngestError> {
    // skill-type-gating (PoB2's four-stage decision, CalcTools.lua:84-110)
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

    // TODO(mana-multiplier)
    // Mirrors PoB2 CalcActiveSkill.lua:
    //   skillModList:NewMod("SupportManaMultiplier", "MORE", level.manaMultiplier, ...)
    // The "MORE" range, in percentage units (e.g. 40 = +40% more = ×1.4).
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

    // Modifier text parsing (including more-multiplier isolation)
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

                    // TODO(more-multiplier isolation)
                    // If the support gem specifies target skill types, attach
                    // a SkillTypes tag to the More modifier, ensuring it only
                    // applies to the supported skill (matches only when
                    // CalcConfig's skill_types intersects it).
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

    // TODO(level/quality scaling)
    // level_mods → SourceKind::SkillLevel attribution
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

    // quality_mods → SourceKind::GemQuality attribution
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

// ingest_gem_leveled — active skill gem level/quality attribution variant

/// Ingests an active skill gem's level/quality modifiers.
///
/// `level_mods` is attributed to `SourceKind::SkillLevel` (id =
/// `gem.<id>.level<N>`), `quality_mods` to `SourceKind::GemQuality` (id =
/// `gem.<id>.q<Q>`).
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
