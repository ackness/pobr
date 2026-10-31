//! Exact native build admission and evaluation of one conditional successful skill hit.
//!
//! Loading facts or decoding a request does not grant calculation authority. Every
//! edit/reopen must be admitted again. Ordinary BuildData and ruleset guards stay closed.

use pobr_core::Modifier;
use pobr_core::calc::conditional_hit::{self, ConditionalHitContext, ConditionalSkillHitV1};
use pobr_core::calc::crit::ResolvedCriticalBase;
use pobr_data::catalog::NativeRuntimeContract;
use pobr_data::catalog::native_player::{NativeFactDisposition, NativeModelStatus};
use pobr_data::constants::DamageType;
use pobr_data::modifier::ModType;
use pobr_data::source::{ExporterRule, ModifierSource, SourceId};
use pobr_gamedata::NativeData;
use serde::Serialize;
use thiserror::Error;

use crate::native_document::{
    NativeActorPolicyPin, NativeActorSelection, NativeBuildRequest, NativeDatasetBinding,
    NativeDocumentError, NativeEvaluationKind, NativeGemSelection,
};

mod resolve;
mod scope;

#[cfg(test)]
mod tests;

/// Owned validated inputs; only [`admit_native_skill`] can construct this capability.
/// No deserialization, public constructor, defaults or mutable getters are provided.
#[derive(Debug)]
pub struct AdmittedNativeSkill {
    request: NativeBuildRequest,
    request_sha256: String,
    context: ConditionalHitContext,
    critical_base: ResolvedCriticalBase,
    hit_modifiers: Vec<Modifier>,
    intrinsic_modifiers: Vec<NativeModifierEvidence>,
    base_use_ms: u32,
    costs: Vec<NativeResourceCost>,
    total_projectiles: f64,
    projectile_lifetime_ms: f64,
    provenance: NativeSkillProvenance,
    skill_facts: Vec<NativeSkillFact>,
    assumptions: Vec<String>,
}

impl AdmittedNativeSkill {
    /// Fingerprint of the complete immutable admitted request, including retained UI state.
    pub fn request_sha256(&self) -> &str {
        &self.request_sha256
    }
}

/// Rejection of document structure, exact snapshot binding, scope or selected source facts.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum NativeAdmissionError {
    #[error(transparent)]
    Document(#[from] NativeDocumentError),
    #[error("native dataset differs at {field}")]
    DatasetMismatch { field: &'static str },
    #[error("unsupported native input at {path}: {reason}")]
    Unsupported { path: String, reason: String },
    #[error("invalid native source at {path}: {reason}")]
    Source { path: String, reason: String },
}

fn unsupported(path: impl Into<String>, reason: impl Into<String>) -> NativeAdmissionError {
    NativeAdmissionError::Unsupported {
        path: path.into(),
        reason: reason.into(),
    }
}

fn source_error(path: impl Into<String>, reason: impl Into<String>) -> NativeAdmissionError {
    NativeAdmissionError::Source {
        path: path.into(),
        reason: reason.into(),
    }
}

/// Failure in the common bounded hit evaluator, without substituting partial numerical output.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum NativeCalculationError {
    #[error(transparent)]
    Hit(#[from] conditional_hit::ConditionalHitError),
}

/// Exact JSON pointer into an artifact in the result's bound runtime snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct NativeArtifactReference {
    pub artifact: String,
    pub pointer: String,
}

fn reference(artifact: &str, pointer: impl Into<String>) -> NativeArtifactReference {
    NativeArtifactReference {
        artifact: artifact.into(),
        pointer: pointer.into(),
    }
}

/// Local disposition of every selected skill stat, including facts outside this output scope.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[non_exhaustive]
pub struct NativeSkillFact {
    pub stat: String,
    /// Implicit stat presence is represented as one; explicit values retain their source unit.
    pub value: f64,
    pub source: NativeArtifactReference,
    pub disposition: pobr_data::catalog::native_player::NativeFactDispositionKind,
    pub reason: String,
}

/// Source-normalized instantaneous resource cost, without payment or affordability claims.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[non_exhaustive]
pub struct NativeResourceCost {
    pub resource: String,
    pub amount_per_use: f64,
    pub raw_amount: u32,
    pub divisor: u32,
    pub amount_source: NativeArtifactReference,
    pub type_source: NativeArtifactReference,
    pub resource_source: NativeArtifactReference,
}

/// The original owned mapping and actual reader evidence, without serializing a whole catalog.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct NativeModifierEvidence {
    pub native_stat_id: String,
    pub runtime_source: NativeArtifactReference,
    pub name: String,
    pub mod_type: ModType,
    pub value: f64,
    pub source: Option<String>,
    pub origin: Option<ModifierSource>,
    pub rule_provenance: Option<ExporterRule>,
    /// Exact shared ModFlags bits.
    pub flags: u64,
    /// Exact shared KeywordFlags bits.
    pub keyword_flags: u64,
    /// Debug spellings of the shared core's static tags; empty for this initial profile.
    pub tags: Vec<String>,
    pub queries: Vec<String>,
    pub counted: bool,
    /// Intrinsic values use the mapped numeric payload directly, without a hit query.
    pub intrinsic_reader: Option<String>,
}

fn modifier_evidence(
    modifier: &Modifier,
    fact: &NativeSkillFact,
    queries: Vec<String>,
    counted: bool,
) -> NativeModifierEvidence {
    NativeModifierEvidence {
        native_stat_id: fact.stat.clone(),
        runtime_source: fact.source.clone(),
        name: modifier.name.to_string(),
        mod_type: modifier.mod_type,
        value: modifier
            .value
            .as_number()
            .expect("admission requires a numeric owned mapping"),
        source: modifier.source.clone(),
        origin: modifier.origin.clone(),
        rule_provenance: modifier.exporter_rule.as_deref().cloned(),
        flags: modifier.flags.bits(),
        keyword_flags: modifier.keyword_flags.bits(),
        tags: modifier.tags.iter().map(|tag| format!("{tag:?}")).collect(),
        queries,
        counted,
        intrinsic_reader: None,
    }
}

/// Common core ranges before enemy mitigation; zero types remain explicit core baseline rows.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[non_exhaustive]
pub struct NativeDamageRange {
    pub min: f64,
    pub max: f64,
    pub mean: f64,
}

impl From<conditional_hit::ConditionalHitRange> for NativeDamageRange {
    fn from(value: conditional_hit::ConditionalHitRange) -> Self {
        Self {
            min: value.min,
            max: value.max,
            mean: value.mean,
        }
    }
}

/// Noncritical and critical branches for one damage type, conditional on a damaging hit.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[non_exhaustive]
pub struct NativeHitDamage {
    pub damage_type: DamageType,
    pub non_critical: NativeDamageRange,
    pub critical: NativeDamageRange,
}

/// Small projection of the actual core critical trace, retaining its graph edges.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct NativeCriticalTrace {
    pub nodes: Vec<NativeCriticalTraceNode>,
    pub edges: Vec<(usize, usize)>,
}

/// An actual shared-core trace node; source scalar references are carried in provenance.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct NativeCriticalTraceNode {
    pub id: usize,
    pub label: String,
    pub value: f64,
    pub operation: String,
    pub source: Option<SourceId>,
    pub pass: Option<String>,
}

/// Snapshot/source/policy pins and selected scalar addresses; raw physical archives stay sealed.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct NativeSkillProvenance {
    /// Includes source lock, normalization/rules, reference index, acquisition and player pins.
    pub contract: NativeRuntimeContract,
    pub selection_sources: Vec<NativeArtifactReference>,
    pub critical_chance_source: NativeArtifactReference,
    pub critical_bonus_source: NativeArtifactReference,
    pub base_use_source: NativeArtifactReference,
    pub required_conditions: Vec<String>,
    pub excluded_sources: Vec<String>,
    /// All source-player dispositions remain local to the pinned policy/profile.
    pub actor_dispositions: Vec<NativeFactDisposition>,
}

/// An unavailable output is never represented as a fabricated zero.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct NativeUnavailableOutput {
    pub output: String,
    pub reason: String,
}

/// Only the bounded successful-hit and intrinsic-use quantities certified by admission.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct NativeSkillResult {
    pub evaluation: NativeEvaluationKind,
    pub model_status: NativeModelStatus,
    pub dataset: NativeDatasetBinding,
    pub actor: NativeActorSelection,
    pub skill: NativeGemSelection,
    pub actor_policy: NativeActorPolicyPin,
    pub request_sha256: String,
    /// Retained compatibility/display text; never the actor selection or an attribute source.
    pub compatibility_class_name: String,
    /// Retained pre-adjustment levels; their presence does not certify requirements.
    pub gem_requirement_levels: Vec<u32>,
    pub damage: Vec<NativeHitDamage>,
    pub critical_probability: f64,
    pub critical_multiplier: f64,
    pub critical_base_chance_fraction: f64,
    pub critical_base_damage_bonus_percent: f64,
    pub conditional_weighted_mean: f64,
    pub analytical_base_use_ms: u32,
    pub resource_costs: Vec<NativeResourceCost>,
    pub total_projectiles: f64,
    pub projectile_lifetime_ms: f64,
    pub modifiers: Vec<NativeModifierEvidence>,
    pub critical_trace: NativeCriticalTrace,
    pub skill_facts: Vec<NativeSkillFact>,
    pub provenance: NativeSkillProvenance,
    pub assumptions: Vec<String>,
    pub unavailable: Vec<NativeUnavailableOutput>,
}

/// Admit the fixed native actor/skill profile against the complete exact-bound request.
///
/// This operation has no I/O, defaults, legacy BuildData, ruleset or calculation side effects.
///
/// # Errors
/// Rejects mismatched snapshots/policies, unsupported Build state, ambiguous joins,
/// unknown or invalid selected facts, incomplete costs and unresolved relevant actor inputs.
pub fn admit_native_skill(
    data: &NativeData,
    request: &NativeBuildRequest,
) -> Result<AdmittedNativeSkill, NativeAdmissionError> {
    request.validate_document()?;
    scope::validate(data, request)?;
    let resolved = resolve::resolve(data, request)?;
    Ok(AdmittedNativeSkill {
        request: request.clone(),
        request_sha256: request.input_sha256()?,
        context: resolved.context,
        critical_base: resolved.critical_base,
        hit_modifiers: resolved.hit_modifiers,
        intrinsic_modifiers: resolved.intrinsic_modifiers,
        base_use_ms: resolved.base_use_ms,
        costs: resolved.costs,
        total_projectiles: resolved.total_projectiles,
        projectile_lifetime_ms: resolved.projectile_lifetime_ms,
        provenance: resolved.provenance,
        skill_facts: resolved.skill_facts,
        assumptions: resolved.assumptions,
    })
}

/// Evaluate an owned admitted snapshot through the shared conditional hit core only.
///
/// No pool, requirements, contact, enemy, ailment, cadence or full-build calculation runs.
///
/// # Errors
/// Returns the shared core error on invalid ranges or nonfinite numerical results.
pub fn calculate_native_skill(
    admitted: &AdmittedNativeSkill,
) -> Result<NativeSkillResult, NativeCalculationError> {
    let hit = conditional_hit::calculate_conditional_hit(&ConditionalSkillHitV1 {
        context: admitted.context,
        critical_base: Some(admitted.critical_base),
        modifiers: &admitted.hit_modifiers,
    })?;
    let mut modifiers: Vec<_> = hit
        .modifiers
        .iter()
        .map(|entry| {
            let fact = admitted
                .skill_facts
                .iter()
                .find(|fact| {
                    entry.modifier.origin.as_ref().is_some_and(|origin| {
                        origin.source_id.id == format!("native.skill.{}", fact.stat)
                    })
                })
                .expect("every admitted modifier retains its selected stat source");
            modifier_evidence(&entry.modifier, fact, entry.queries.clone(), entry.counted)
        })
        .collect();
    modifiers.extend(admitted.intrinsic_modifiers.clone());
    let critical_trace = NativeCriticalTrace {
        nodes: hit
            .crit_trace
            .nodes()
            .iter()
            .map(|node| NativeCriticalTraceNode {
                id: node.id.as_usize(),
                label: node.label.clone(),
                value: node.value,
                operation: format!("{:?}", node.operation),
                source: node.source.clone(),
                pass: node.pass.map(|pass| format!("{pass:?}")),
            })
            .collect(),
        edges: hit
            .crit_trace
            .edges()
            .iter()
            .map(|edge| (edge.from.as_usize(), edge.to.as_usize()))
            .collect(),
    };
    let request = &admitted.request;
    Ok(NativeSkillResult {
        evaluation: request.evaluation,
        model_status: NativeModelStatus::ConditionalModel,
        dataset: request.dataset.clone(),
        actor: request.actor.clone(),
        skill: request.skill.clone(),
        actor_policy: request.actor_policy.clone(),
        request_sha256: admitted.request_sha256.clone(),
        compatibility_class_name: request.build.character.class_name.clone(),
        gem_requirement_levels: request.build.socket_groups[0]
            .gem_requirement_levels
            .clone(),
        damage: hit
            .damage
            .into_iter()
            .map(|row| NativeHitDamage {
                damage_type: row.damage_type,
                non_critical: row.non_critical.into(),
                critical: row.critical.into(),
            })
            .collect(),
        critical_probability: hit.crit_probability,
        critical_multiplier: hit.crit_multiplier,
        critical_base_chance_fraction: hit.critical_base.chance_fraction,
        critical_base_damage_bonus_percent: hit.critical_base.damage_bonus_percent,
        conditional_weighted_mean: hit.conditional_weighted_mean,
        analytical_base_use_ms: admitted.base_use_ms,
        resource_costs: admitted.costs.clone(),
        total_projectiles: admitted.total_projectiles,
        projectile_lifetime_ms: admitted.projectile_lifetime_ms,
        modifiers,
        critical_trace,
        skill_facts: admitted.skill_facts.clone(),
        provenance: admitted.provenance.clone(),
        assumptions: admitted.assumptions.clone(),
        unavailable: [
            (
                "pools",
                "actor initialization and resource laws are not admitted",
            ),
            (
                "requirements",
                "native gem attribute requirements are unverified; legacy formulas are not used",
            ),
            (
                "accuracy-and-hit-probability",
                "evaluation conditions on a successful damaging hit",
            ),
            (
                "hit-count",
                "projectile contact, paths and re-hit history are outside this evaluation",
            ),
            (
                "cadence-and-dps",
                "analytical use time does not establish server cadence or repeated use",
            ),
            (
                "cost-per-second-and-sustain",
                "per-use cost does not certify affordability, timing or recovery",
            ),
            (
                "enemy-mitigation",
                "no target is supplied to the conditional hit operation",
            ),
            (
                "ailments",
                "the admitted operation evaluates hit damage only",
            ),
            (
                "defence-recovery-and-ehp",
                "actor pools, defence and recovery dependencies are not admitted",
            ),
        ]
        .into_iter()
        .map(|(output, reason)| NativeUnavailableOutput {
            output: output.into(),
            reason: reason.into(),
        })
        .collect(),
    })
}
