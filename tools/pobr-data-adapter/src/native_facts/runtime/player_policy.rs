//! Explicit profile interpretation, separate from all source facts.

use std::collections::BTreeSet;

use pobr_data::catalog::native_player::{
    NATIVE_GENERIC_OT, NATIVE_PLAYER_ID, NativeActorBinding, NativeActorPolicy,
    NativeAssignmentSelector, NativeCriticalBonus, NativeEvaluationKind, NativeFactDisposition,
    NativeFactDispositionKind, NativeMetadataScope, NativeModelStatus, NativePlayerContextDef,
    NativePlayerOrigin, NativePlayerReference,
};
use serde_json::Value;

use super::super::trust::{require, strict_json};

pub(super) fn bind(
    c: &NativePlayerContextDef,
    context_sha256: String,
) -> Result<NativeActorBinding, String> {
    let facts = &c.facts;
    let candidates = facts
        .metadata_stats
        .iter()
        .filter(|s| s.key_raw == "base_critical_hit_damage_bonus")
        .collect::<Vec<_>>();
    // Do not choose the first of repeated top-level assignments or a scoped override.
    let critical_assignments = c
        .metadata
        .files
        .iter()
        .flat_map(|(path, file)| file.assignments.iter().map(move |a| (path, a)))
        .filter(|(_, a)| a.key_raw == "base_critical_hit_damage_bonus")
        .collect::<Vec<_>>();
    require(
        candidates.len() == 1 && critical_assignments.len() == 1,
        "duplicate/ambiguous critical assignment",
    )?;
    let source = candidates[0];
    require(
        source.path == NATIVE_GENERIC_OT
            && source.scope.len() == 1
            && source.scope[0].name_raw == "Stats",
        "critical assignment must be plain generic OT Stats",
    )?;
    let bonus = source
        .value_raw
        .parse::<f64>()
        .map_err(|_| "critical bonus is not an explicit numeric token")?;
    require(bonus.is_finite(), "critical bonus must be finite")?;
    let stat = source.stat.clone().ok_or("critical stat ID unresolved")?;
    require(
        stat.id.as_deref() == Some("base_critical_hit_damage_bonus"),
        "critical stat identity mismatch",
    )?;
    let mut meaning = Vec::new();
    for id in ["Critical", "CriticalDamageBonus"] {
        let rows = c
            .keyword_popups
            .rows
            .iter()
            .filter(|r| r.values.get("Id").and_then(Value::as_str) == Some(id))
            .collect::<Vec<_>>();
        require(
            rows.len() == 1
                && rows[0]
                    .values
                    .get("Definition")
                    .and_then(Value::as_str)
                    .is_some_and(|s| !s.is_empty()),
            "critical glossary missing/ambiguous",
        )?;
        let row = rows[0].row;
        meaning.push(NativePlayerReference {
            origin: NativePlayerOrigin::Player,
            table: "KeywordPopups".into(),
            payload: "KeywordPopups.json".into(),
            pointer: format!("/rows/{row}/values"),
            row,
            id: Some(id.into()),
        });
    }
    let policy = strict_json(include_bytes!("player-stat-policy.json"))?;
    let mut dispositions = Vec::new();
    let mut keys = BTreeSet::new();
    for (i, s) in facts.metadata_stats.iter().enumerate() {
        require(
            keys.insert(&s.key_raw),
            "duplicate selected plain Stats key",
        )?;
        let entry = policy
            .get(&s.key_raw)
            .ok_or_else(|| format!("unresolved profile-relevant raw stat: {}", s.key_raw))?;
        let mut disposition: NativeFactDisposition = serde_json::from_value(serde_json::json!({
            "selector":format!("/facts/metadata_stats/{i}"), "disposition":entry["disposition"],
            "relevant_to_conditional_hit":entry["relevant_to_conditional_hit"], "reason":entry["reason"]
        })).map_err(|e| e.to_string())?;
        // Retain every assignment, also when a known native stat ID is unavailable.
        if s.stat.is_none() {
            disposition
                .reason
                .push_str(" Native Stats ID is unresolved and remains retained.");
        }
        dispositions.push(disposition);
    }
    require(
        keys.len() == policy.as_object().ok_or("invalid stat policy")?.len(),
        "profile raw Stats inventory changed",
    )?;
    for (path, f) in &c.metadata.files {
        for (i, a) in f.assignments.iter().enumerate() {
            if a.plain_stats_scope {
                continue;
            }
            let (disposition, relevant, reason) = scoped_fact(&a.scope, &a.key_raw);
            dispositions.push(NativeFactDisposition {
                selector: format!("/metadata/files/{}/assignments/{i}", pointer_escape(path)),
                disposition,
                relevant_to_conditional_hit: relevant,
                reason,
            });
        }
        let assigned = f
            .assignments
            .iter()
            .map(|a| a.line)
            .collect::<BTreeSet<_>>();
        for (i, record) in f.records.iter().enumerate() {
            if assigned.contains(&record.line) {
                continue;
            }
            let literal = record.literal.trim();
            let empty_block = literal.split_once('{').is_some_and(|(name, tail)| {
                !name.trim().is_empty()
                    && name
                        .trim()
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_')
                    && tail.trim() == "}"
            });
            let structural = empty_block
                || literal.is_empty()
                || matches!(literal, "{" | "}")
                || literal.starts_with("//")
                || literal.starts_with("version ")
                || literal.starts_with("extends ")
                || literal
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_');
            let (disposition, relevant, reason) = if structural {
                (NativeFactDispositionKind::OutsideOutputScope, false,
                    "Literal structure/source evidence retained; no effective metadata merge or executable action is inferred.".into())
            } else {
                scoped_fact(&record.scope, literal)
            };
            dispositions.push(NativeFactDisposition {
                selector: format!("/metadata/files/{}/records/{i}", pointer_escape(path)),
                disposition,
                relevant_to_conditional_hit: relevant,
                reason,
            });
        }
        for (i, diagnostic) in f.diagnostics.iter().enumerate() {
            let scope = diagnostic["line"]
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .and_then(|n| n.checked_sub(1))
                .and_then(|n| f.records.get(n))
                .map(|r| r.scope.as_slice())
                .unwrap_or(&[]);
            let (disposition, relevant, reason) = scoped_fact(scope, "opaque source diagnostic");
            dispositions.push(NativeFactDisposition {
                selector: format!("/metadata/files/{}/diagnostics/{i}", pointer_escape(path)),
                disposition,
                relevant_to_conditional_hit: relevant,
                reason,
            });
        }
    }
    for key in facts.character.values.keys() {
        let selected = matches!(key.as_str(), "Id" | "AOFile" | "StartingPassive");
        dispositions.push(NativeFactDisposition {
            selector: format!("/facts/character/values/{}", pointer_escape(key)),
            disposition: if selected { NativeFactDispositionKind::Consumed } else if key.starts_with("Unknown") || key.starts_with("Key") || key.starts_with("Data") {
                NativeFactDispositionKind::Unresolved
            } else { NativeFactDispositionKind::OutsideOutputScope },
            relevant_to_conditional_hit: selected,
            reason: if selected { "Exact stable actor/root/starting-reference selection, never a stat grant.".into() }
                else { format!("Physical character field {key} retained without execution. This profile applies only explicit generic OT critical bonus; row attributes/pools, equipment, animations, descriptions and opaque fields do not become hit modifiers.") },
        });
    }
    for i in 0..facts.all_starting_relations.len() {
        dispositions.push(NativeFactDisposition { selector: format!("/facts/all_starting_relations/{i}"),
            disposition: NativeFactDispositionKind::Inactive, relevant_to_conditional_hit: false,
            reason: "Ordered starting choices retained; no free allocation or autogrant, including the selected relation.".into() });
    }
    Ok(NativeActorBinding {
        schema_version: 1, policy: NativeActorPolicy::SelectedPlayerGenericOtV1,
        evaluation: NativeEvaluationKind::ConditionalSkillHitV1, model_status: NativeModelStatus::ConditionalModel,
        context_sha256, actor_id: NATIVE_PLAYER_ID.into(), actor_level: 1,
        critical_bonus: NativeCriticalBonus { bonus_percent: bonus,
            source: NativeAssignmentSelector { path: source.path.clone(), assignment: source.assignment }, stat, meaning },
        required_conditions: REQUIRED_CONDITIONS.iter().map(|s| (*s).into()).collect(),
        excluded_sources: EXCLUDED_SOURCES.iter().map(|s| (*s).into()).collect(), dispositions,
        assumptions: vec![
            "Applying the separate generic OT critical bonus to this exact selected player is a PoBR applicability assumption, not sourced AO-to-OT inheritance.".into(),
            "Conditional on a successful damaging hit; contact, hit count, enemy mitigation and ailment outcomes are unavailable.".into(),
            "Mana per use is intrinsic cost, not payment timing, affordability or sustain; analytical base-use duration is not effective server cadence.".into(),
            "No actor pool/accuracy/attribute requirement law or full DPS is inferred; acquisition remains unreviewed-acquisition-draft.".into(),
        ],
    })
}

pub(super) const REQUIRED_CONDITIONS: &[&str] = &[
    "exact-selected-actor-level1",
    "primary-SparkPlayer-level1-q0",
    "ordinary-stationary-self-cast",
    "Act1-no-quests",
    "successful-damaging-hit",
];
pub(super) const EXCLUDED_SOURCES: &[&str] = &[
    "gear",
    "supports",
    "allocations",
    "ascendancy",
    "infusion",
    "buffs",
    "charges",
    "rampage",
    "quests",
    "PvP",
    "delirium",
    "events",
    "movement",
    "cutscene",
    "external-modifiers",
];

fn pointer_escape(text: &str) -> String {
    text.replace('~', "~0").replace('/', "~1")
}

fn scoped_fact(
    scope: &[NativeMetadataScope],
    key: &str,
) -> (NativeFactDispositionKind, bool, String) {
    use NativeFactDispositionKind::{Inactive, OutsideOutputScope, Unresolved};
    let names = scope
        .iter()
        .map(|s| s.name_raw.as_str())
        .collect::<Vec<_>>();
    let (kind, relevant, reason) = match names.as_slice() {
        ["client", ..] => (
            OutsideOutputScope,
            false,
            "Client rendering, visuals and animation execution are outside conditional hit quantities.",
        ),
        ["AnimationController" | "AttachedAnimatedObject"] => (
            OutsideOutputScope,
            false,
            "AO model/attachment metadata is retained as presentation evidence; no effective stat inheritance is inferred.",
        ),
        ["Melee"] => (
            Inactive,
            false,
            "Melee offset/geometry is inactive for the selected ordinary spell.",
        ),
        ["Positioned"] => (
            OutsideOutputScope,
            true,
            "Positioning, targeting size, team and blocking concern contact/encounter geometry, not damage conditional on a successful hit.",
        ),
        ["Pathfinding" | "Animated" | "Player"] => (
            Inactive,
            false,
            "Stationary ordinary-use policy excludes movement/turning and does not infer effective animation cadence.",
        ),
        ["Actor"] if matches!(key, "basic_action" | "leveled_starting_effects") => (
            Inactive,
            true,
            "Basic actions and EnforcedWalking are retained but excluded; their action/cast time is not imported into stationary ordinary self-cast.",
        ),
        ["Actor"]
            if matches!(
                key,
                "actor_size" | "simulation_height" | "blend_once_animations_by_default"
            ) =>
        {
            (
                OutsideOutputScope,
                false,
                "Actor geometry/animation blending is retained; contact and server cadence are unavailable outputs.",
            )
        }
        _ => (
            Unresolved,
            true,
            "Unrecognized scope may affect a requested hit input; retained unresolved and must block later admission.",
        ),
    };
    (
        kind,
        relevant,
        format!("{} key {key}: {reason}", names.join("/")),
    )
}
