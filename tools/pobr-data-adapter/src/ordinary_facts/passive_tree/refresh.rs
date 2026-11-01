//! Target graph/definition ownership with explicitly bounded compatibility fields.

use super::super::stat_set_codec::read_pinned;
use super::{
    Converted, ascendancy, nonstat, policy, prepared, render::Renderer, source::Facts, unlock,
};
use crate::native_facts::reader::{self, VerifiedFacts};
use crate::raw_source::sha256_hex;
use pobr_data::catalog::{PassiveNodeDef, PassiveNodeKind, PassiveTreeMeta};
use pobr_data::native_stat::{NativePassiveContext, NativePassiveStats};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

pub(in crate::ordinary_facts) struct Args {
    pub reference: PathBuf,
    pub descriptions: PathBuf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Policy {
    pub schema: String,
    pub base_policy: policy::Policy,
    pub reference_sha256: String,
    pub descriptions_sha256: String,
    pub descriptions_manifest_sha256: String,
    pub compatibility_bindings: BTreeMap<String, Binding>,
    pub excluded: BTreeMap<String, Exclusion>,
    pub class_bindings: Vec<ClassBinding>,
    pub statement: String,
    #[serde(default)]
    pub ascendancy: Option<ascendancy::Policy>,
    #[serde(default)]
    pub non_stat_manifest_sha256: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Binding {
    pub context_sha256: String,
    pub stats_sha256: String,
    #[serde(default)]
    pub render_context_sha256: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Exclusion {
    pub definition_sha256: String,
    pub reason: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ClassBinding {
    pub base_id: String,
    pub class: String,
}

impl Policy {
    pub(super) fn owns_unlocks(&self) -> bool {
        matches!(
            self.schema.as_str(),
            "pobr-ordinary-target-tree-policy/v3" | "pobr-ordinary-target-tree-policy/v4"
        )
    }
    fn owns_nonstat_text(&self) -> bool {
        self.schema == "pobr-ordinary-target-tree-policy/v4"
    }
}

pub(super) struct Supplement<'a> {
    pub ascendancy: &'a ascendancy::Source,
    pub input: &'a reader::selected::Input,
}

struct Effects<'a> {
    ascendancy: Option<&'a ascendancy::Source>,
    nonstat: Option<nonstat::Source<'a>>,
}

pub(super) fn convert(
    input: &VerifiedFacts,
    args: &super::super::CodecArgs,
    extra: &Args,
    supplement: Option<&reader::selected::Input>,
) -> Result<Converted, String> {
    let pinned = read_pinned(&args.policy, &args.policy_sha256, "target tree policy")?;
    let policy: Policy = serde_json::from_value(pinned.document).map_err(|e| e.to_string())?;
    if !matches!(
        policy.schema.as_str(),
        "pobr-ordinary-target-tree-policy/v1"
            | "pobr-ordinary-target-tree-policy/v2"
            | "pobr-ordinary-target-tree-policy/v3"
            | "pobr-ordinary-target-tree-policy/v4"
    ) || policy.statement.trim().is_empty()
    {
        return Err("unsupported target tree policy".into());
    }
    if (policy.schema != "pobr-ordinary-target-tree-policy/v1") != policy.ascendancy.is_some()
        || policy.ascendancy.is_some() != supplement.is_some()
    {
        return Err("target tree policy/supplement scope mismatch".into());
    }
    if policy.non_stat_manifest_sha256.is_some() != policy.owns_nonstat_text()
        || policy
            .compatibility_bindings
            .values()
            .any(|b| b.render_context_sha256.is_some() != policy.owns_nonstat_text())
    {
        return Err("target tree render-context policy scope mismatch".into());
    }
    policy.base_policy.validate(input)?;
    if args.source_sha256 != policy.base_policy.source.sha256 {
        return Err("prepared tree policy pin mismatch".into());
    }
    let legacy = read_pinned(
        &args.source,
        &args.source_sha256,
        "prepared field compatibility",
    )?;
    let reference = read_pinned(
        &extra.reference,
        &policy.reference_sha256,
        "coordinate/identity compatibility",
    )?;
    let descriptions = read_pinned(
        &extra.descriptions,
        &policy.descriptions_sha256,
        "target CSD descriptions",
    )?;
    let desc_manifest = read_pinned(
        &extra.descriptions.with_file_name("manifest.json"),
        &policy.descriptions_manifest_sha256,
        "CSD manifest",
    )?;
    if desc_manifest.document["format"] != "repoe-native-descriptions/v1"
        || desc_manifest.document["files"]["descriptions.json"]["sha256"]
            != policy.descriptions_sha256
        || desc_manifest.document["files"]["descriptions.json"]["bytes"] != descriptions.bytes.len()
        || desc_manifest.document["source_identity"]["game_patch"]
            != input.manifest["source_identity"]["game_patch"]
    {
        return Err("CSD manifest/payload/patch mismatch".into());
    }
    let facts = Facts::read(input, &policy.base_policy)?;
    let renderer = Renderer::new(&descriptions.document)?;
    let ascendancy = policy
        .ascendancy
        .as_ref()
        .zip(supplement)
        .map(|(p, s)| {
            ascendancy::Source::read(
                input,
                &facts,
                s,
                p,
                &policy.base_policy.legacy_metadata.value,
            )
        })
        .transpose()?;
    let (tree, metadata, receipt) = project(
        input,
        &facts,
        &policy,
        &legacy.document,
        &reference.document,
        &renderer,
        ascendancy
            .as_ref()
            .zip(supplement)
            .map(|(ascendancy, input)| Supplement { ascendancy, input }),
    )?;
    Ok(Converted {
        base: BTreeMap::from([
            (
                "passive_tree.json".into(),
                super::super::pretty_bytes(&tree)?,
            ),
            (
                "passive_tree_meta.json".into(),
                super::super::pretty_bytes(&metadata)?,
            ),
        ]),
        receipt: json!({"projection":receipt,"legacy_source":{"sha256":args.source_sha256,"declaration":policy.base_policy.source},
            "codec_policy":{"sha256":args.policy_sha256},"reference":{"sha256":policy.reference_sha256,"patch":"unknown","role":"fixed coordinate offsets and canonical identity corroboration, not numeric target source"},
            "descriptions":{"sha256":policy.descriptions_sha256,"manifest_sha256":policy.descriptions_manifest_sha256,"source_identity":desc_manifest.document["source_identity"]},
            "target_fields":{"facts_manifest_sha256":sha256_hex(&input.manifest_bytes),"source_identity":input.manifest["source_identity"]},
            "compatibility_statement":policy.statement,"status":"source draft; candidate only; CURRENT unchanged"}),
        source_bytes: legacy.bytes,
        policy_bytes: pinned.bytes,
        protected_parents: vec![
            legacy.parent,
            pinned.parent,
            reference.parent,
            descriptions.parent,
            desc_manifest.parent,
        ],
    })
}

pub(super) fn context(raw: &Value) -> Result<Value, String> {
    let mut context = raw.as_object().ok_or("invalid passive row")?.clone();
    context.retain(|key, _| {
        ![
            "Id",
            "Name",
            "Icon_DDSFile",
            "PassiveSkillGraphId",
            "Stats",
            "IsNotable",
            "IsKeystone",
            "IsJewelSocket",
            "IsAscendancyStartingNode",
        ]
        .contains(&key.as_str())
            && !(key.starts_with("Stat") && key.ends_with("Value"))
    });
    Ok(Value::Object(context))
}
fn policy_context(policy: &Policy, raw: &Value) -> Result<Value, String> {
    let mut value = context(raw)?;
    if policy.ascendancy.is_some() {
        value
            .as_object_mut()
            .ok_or("invalid context")?
            .remove("Ascendancy");
    }
    if policy.owns_unlocks() {
        let object = value.as_object_mut().ok_or("invalid context")?;
        object.remove("UnlockedBy");
        object.remove("VisibleForAscendancy");
    }
    Ok(value)
}
fn render_context(policy: &Policy, raw: &Value) -> Result<Value, String> {
    let mut context = policy_context(policy, raw)?;
    if policy.owns_nonstat_text() {
        let object = context.as_object_mut().ok_or("invalid context")?;
        for field in nonstat::OWNED_FIELDS {
            object.remove(field);
        }
    }
    Ok(context)
}
fn hash(value: &impl serde::Serialize) -> Result<String, String> {
    Ok(sha256_hex(&reader::json_bytes(value)?))
}
fn raw_stats(
    input: &VerifiedFacts,
    facts: &Facts<'_>,
    row: usize,
) -> Result<Vec<(String, i64)>, String> {
    Ok(facts
        .stats(input, row)?
        .into_iter()
        .map(|s| (s.id, s.value))
        .collect())
}
fn exact_compatibility(
    policy: &Policy,
    id: &str,
    raw: &Value,
    stats: &[(String, i64)],
) -> Result<(), String> {
    let binding = policy
        .compatibility_bindings
        .get(id)
        .ok_or_else(|| format!("{id}: no whole-text compatibility binding"))?;
    if binding.context_sha256 != hash(&policy_context(policy, raw)?)?
        || binding.stats_sha256 != hash(&stats)?
    {
        return Err(format!(
            "{id}: target semantics changed; stale whole-text compatibility rejected"
        ));
    }
    Ok(())
}
fn text(
    input: &VerifiedFacts,
    facts: &Facts<'_>,
    row: usize,
    policy: &Policy,
    renderer: &Renderer<'_>,
    legacy: Option<&[String]>,
    nonstat: Option<&nonstat::Source<'_>>,
) -> Result<(Vec<String>, Value), String> {
    let raw = facts.tables.row("PassiveSkills", row)?;
    let id = raw["Id"].as_str().ok_or("missing passive Id")?;
    let stats = raw_stats(input, facts, row)?;
    // Non-stat grants cannot be inferred from the fact that all stat IDs render.
    let extra_effect = !raw["GrantedSkill"].is_null()
        || raw["SkillPointsGranted"].as_i64().is_some_and(|n| n != 0)
        || raw["WeaponPointsGranted"].as_i64().is_some_and(|n| n != 0)
        || raw["PassiveSkillBuffs"]
            .as_array()
            .is_some_and(|v| !v.is_empty())
        || (stats.is_empty() && legacy.is_some_and(|v| !v.is_empty()));
    let mut non_stat_evidence = Vec::new();
    let result = if let Some(source) = nonstat {
        source.render(raw).and_then(|(extra_lines, evidence)| {
            if stats.is_empty() && extra_lines.is_empty() && legacy.is_some_and(|v| !v.is_empty()) {
                return Err("source has no effect text for nonempty prepared node".into());
            }
            let (mut lines, descriptions) = renderer.render(&stats)?;
            lines.extend(extra_lines);
            non_stat_evidence = evidence;
            Ok((lines, descriptions))
        })
    } else if extra_effect {
        Err("non-stat effect requires complete compatibility text".into())
    } else {
        renderer.render(&stats)
    };
    match result {
        Ok((lines, evidence)) => Ok((
            lines,
            json!({"owner":if non_stat_evidence.is_empty(){"target-csd"}else{"target-csd-and-client-strings"},
                "complete_stats":stats,"descriptions":evidence,"non_stat":non_stat_evidence}),
        )),
        Err(reason) => {
            exact_compatibility(policy, id, raw, &stats)?;
            let lines =
                legacy.ok_or_else(|| format!("{id}: {reason}; no complete compatibility text"))?;
            Ok((
                lines.to_vec(),
                json!({"owner":"prepared-whole-text","reason":reason,"complete_stats":stats,
                "stats_sha256":hash(&stats)?,"target_applicability":"unknown; fixed transition text, no partial target packet"}),
            ))
        }
    }
}

pub(super) fn project(
    input: &VerifiedFacts,
    facts: &Facts<'_>,
    policy: &Policy,
    legacy: &Value,
    reference: &Value,
    renderer: &Renderer<'_>,
    supplement: Option<Supplement<'_>>,
) -> Result<(Value, Value, Value), String> {
    let effects = Effects {
        ascendancy: supplement.as_ref().map(|s| s.ascendancy),
        nonstat: if policy.owns_nonstat_text() {
            let extra = supplement
                .as_ref()
                .ok_or("non-stat text requires supplement")?
                .input;
            if policy.non_stat_manifest_sha256.as_deref()
                != Some(sha256_hex(&extra.manifest_bytes).as_str())
            {
                return Err("non-stat text supplement differs from policy pin".into());
            }
            Some(nonstat::Source::read(input, extra)?)
        } else {
            None
        },
    };
    let ascendancy = effects.ascendancy;
    if policy.owns_unlocks() {
        unlock::validate(input)?;
        if ascendancy.is_none() {
            return Err("source unlocks require target ascendancy identities".into());
        }
    }
    for field in [
        "IsNotable",
        "IsKeystone",
        "IsJewelSocket",
        "IsAscendancyStartingNode",
        "IsJustIcon",
    ] {
        super::source::column(input, "PassiveSkills", field, "bool", false, None)?;
    }
    let old_nodes: Vec<PassiveNodeDef> =
        serde_json::from_value(legacy.clone()).map_err(|e| e.to_string())?;
    let meta: PassiveTreeMeta =
        serde_json::from_value(policy.base_policy.legacy_metadata.value.clone())
            .map_err(|e| e.to_string())?;
    prepared::validate(&old_nodes, &meta)?;
    if reference["tree"] != "Default" {
        return Err("reference is not Default tree".into());
    }
    let old: BTreeMap<_, _> = old_nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.skill, (i, n)))
        .collect();
    let (graph, instances) = reader::tree(input)?;
    let mut excluded = BTreeSet::new();
    let mut excluded_records = Vec::new();
    for instance in &instances {
        let raw = facts.tables.row("PassiveSkills", instance.definition.row)?;
        if let Some(exclusion) = policy.excluded.get(&instance.definition.id) {
            if exclusion.definition_sha256 != hash(raw)? || exclusion.reason.trim().is_empty() {
                return Err(format!(
                    "{}: non-allocatable compatibility binding changed",
                    instance.definition.id
                ));
            }
            excluded.insert(instance.graph_id);
            excluded_records.push(json!({"graph_id":instance.graph_id,"id":instance.definition.id,
                "source_row":instance.definition.row,"reason":exclusion.reason,"definition_sha256":exclusion.definition_sha256}));
        }
    }
    let mut output = Vec::new();
    let mut records = Vec::new();
    let mut native_count = 0;
    let mut rendered_count = 0;
    let mut multi_count = 0;
    for instance in &instances {
        if excluded.contains(&instance.graph_id) {
            continue;
        }
        let skill = u32::try_from(instance.graph_id).map_err(|e| e.to_string())?;
        let row = instance.definition.row;
        let raw = facts.tables.row("PassiveSkills", row)?;
        let id = instance.definition.id.as_str();
        let previous = old.get(&skill).copied();
        let expected_context = policy
            .compatibility_bindings
            .get(id)
            .map(|b| {
                b.render_context_sha256
                    .as_ref()
                    .unwrap_or(&b.context_sha256)
                    .clone()
            })
            .unwrap_or(hash(&render_context(
                policy,
                facts.tables.row("PassiveSkills", facts.reference_row)?,
            )?)?);
        if expected_context != hash(&render_context(policy, raw)?)? {
            return Err(format!(
                "{id}: unsupported source context change or new conditional identity"
            ));
        }
        if let Some((old_index, old)) = previous {
            let alias =
                policy.base_policy.legacy_id_bindings.iter().find(|b| {
                    b.legacy_id == old.id && b.target_passive_id == id && b.skill == skill
                });
            if old.id != id && alias.is_none() {
                return Err(format!("{id}: prepared identity collision at {skill}"));
            }
            if let Some(alias) = alias
                && (alias.source_entry_sha256 != hash(&legacy[old_index])?
                    || old.name.as_deref() != Some(alias.expected_name.as_str()))
            {
                return Err(format!("{id}: stale prepared identity alias"));
            }
        }
        let name = raw["Name"].as_str().ok_or("missing target Name")?;
        if name.is_empty() || name.starts_with("[DNT") {
            return Err(format!("{id}: unclassified layout/unreleased definition"));
        }
        let (lines, text_record) = text(
            input,
            facts,
            row,
            policy,
            renderer,
            previous.map(|(_, n)| n.stats.as_slice()),
            effects.nonstat.as_ref(),
        )?;
        if matches!(
            text_record["owner"].as_str(),
            Some("target-csd" | "target-csd-and-client-strings")
        ) {
            rendered_count += 1;
            if raw["Stats"].as_array().is_some_and(|v| v.len() > 1) {
                multi_count += 1;
            }
        }
        let kind = classify(raw)?;
        let mut entry = previous
            .map(|(i, _)| legacy[i].clone())
            .unwrap_or_else(|| json!({}));
        let object = entry.as_object_mut().ok_or("invalid prepared node")?;
        object.insert("skill".into(), json!(skill));
        object.insert("id".into(), json!(id));
        object.insert("name".into(), json!(name));
        object.insert(
            "kind".into(),
            serde_json::to_value(kind).map_err(|e| e.to_string())?,
        );
        object.insert("stats".into(), json!(lines));
        object.insert("group".into(), json!(instance.group + 1));
        object.insert("orbit".into(), json!(instance.radius));
        object.insert("orbit_index".into(), json!(instance.position));
        object.remove("native_stats");
        let connections: Vec<_> = instance
            .connections
            .iter()
            .filter(|id| !excluded.contains(id))
            .copied()
            .collect();
        object.insert("connections".into(), json!(connections));
        let geometry = geometry(
            reference,
            &graph,
            instance,
            previous.map(|(_, n)| n),
            object,
        )?;
        let variant_records = variants(
            input,
            facts,
            policy,
            renderer,
            previous.map(|(_, n)| n),
            object,
            &effects,
        )?;
        // Keep the existing bounded mapper as an optimization only. Multi-stat
        // nodes use the same ordinary parser as every other rendered tree node.
        let stats = facts.stats(input, row)?;
        if text_record["owner"] == "target-csd"
            && kind == PassiveNodeKind::Normal
            && facts.differences(raw)?.is_empty()
            && !facts.involved.contains_key(&row)
            && previous.is_none_or(|(_, n)| {
                n.variants.is_empty()
                    && n.ascendancy_id.is_none()
                    && n.unlock_constraint.is_none()
                    && !n.apply_to_armour
            })
            && stats.len() == 1
            && lines.len() == 1
            && pobr_core::rules::passive_stats::map_stat(&stats[0]).is_ok()
        {
            let packet = NativePassiveStats {
                data_version: input.manifest["source_identity"]["game_patch"]
                    .as_str()
                    .ok_or("missing patch")?
                    .into(),
                tree_version: Some(policy.base_policy.catalog.tree_compatibility_tag.clone()),
                node_id: skill,
                node_slug: id.into(),
                modifier_texts: lines.clone(),
                context: NativePassiveContext::UnconditionalPlayer,
                stats,
            };
            object.insert(
                "native_stats".into(),
                serde_json::to_value(packet).map_err(|e| e.to_string())?,
            );
            native_count += 1;
        }
        if let Some(source) = ascendancy {
            source.node(raw, object)?;
        }
        let unlock_record = if policy.owns_unlocks() {
            Some(unlock::project(
                facts,
                ascendancy.ok_or("source unlocks require target ascendancy identities")?,
                row,
                object,
            )?)
        } else {
            None
        };
        records.push(json!({"skill":skill,"id":id,"target_row":row,"previous_row":previous.map(|(i,_)|i),
            "name":name,"text":text_record,"geometry":geometry,"variants":variant_records,
            "unlock":unlock_record,
            "source_connections":instance.connections,"excluded_connections":instance.connections.iter().filter(|id|excluded.contains(id)).collect::<Vec<_>>(),
            "compatibility_fields":if policy.owns_unlocks(){vec!["apply_to_armour","unknown prepared extension fields"]}else if ascendancy.is_some(){vec!["unlock_constraint","apply_to_armour","unknown prepared extension fields"]}else{vec!["ascendancy_id","unlock_constraint","apply_to_armour","unknown prepared extension fields"]},
            "context_sha256":hash(&policy_context(policy, raw)?)?,"output_entry_sha256":hash(&entry)?}));
        output.push(entry);
    }
    output.sort_by_key(|n| n["skill"].as_u64());
    let typed: Vec<PassiveNodeDef> =
        serde_json::from_value(json!(output)).map_err(|e| e.to_string())?;
    let (mut metadata, classes) =
        prepared::metadata(&policy.base_policy.legacy_metadata.value, &meta, facts)?;
    if let Some(source) = ascendancy {
        source.metadata(&mut metadata)?;
    }
    let output_meta =
        serde_json::from_value(metadata.clone()).map_err(|e| format!("projected metadata: {e}"))?;
    prepared::validate(&typed, &output_meta)?;
    let skills: BTreeSet<_> = typed.iter().map(|n| n.skill).collect();
    if typed
        .iter()
        .any(|n| n.connections.iter().any(|id| !skills.contains(id)))
    {
        return Err("emitted edge targets non-emitted node".into());
    }
    let removed: Vec<_> = old_nodes
        .iter()
        .filter(|n| !skills.contains(&n.skill))
        .map(|n| n.skill)
        .collect();
    Ok((
        json!(output),
        metadata,
        json!({"entries":typed.len(),"graph_instances":instances.len(),"target_text_nodes":rendered_count,
        "target_multistat_nodes":multi_count,"native_packets":native_count,"source_records":records,"excluded":excluded_records,
        "removed_prepared_skills":removed,"class_records":classes,"class_override_inventory":facts.relations,
        "ascendancy":ascendancy.map(|a|&a.receipt),"non_stat_text":effects.nonstat.as_ref().map(|s|&s.receipt),"ownership":{"ascendancy":if ascendancy.is_some(){"target physical membership, names, character FK, node FK and replacement relations; geometry remains compatibility"}else{"compatibility"},"unlock":if policy.owns_unlocks(){"target UnlockedBy ordered node identities and VisibleForAscendancy stable identity; existing all-prerequisites DTO semantics; Requirement remains opaque context"}else{"prepared compatibility"},"target":["membership minus source-bound eligibility exclusions","ordered emitted edges","id","name","kind","group","orbit","orbit_index","group centres","complete rendered stats","class override identities and rendered stats","base attributes"],
            "compatibility":["whole unrenderable text with exact semantic binding","prepared planar placement/coordinate absence under exact reference group-orbit-slot binding; explicit reference offsets for new nodes","unlock predicates in v1/v2 only; Requirement is never interpreted as a predicate","armour","non-class variants when no supplement","unknown prepared fields"]}}),
    ))
}

fn classify(raw: &Value) -> Result<PassiveNodeKind, String> {
    for (field, kind) in [
        ("IsKeystone", PassiveNodeKind::Keystone),
        ("IsNotable", PassiveNodeKind::Notable),
        ("IsJustIcon", PassiveNodeKind::Mastery),
        ("IsJewelSocket", PassiveNodeKind::JewelSocket),
        ("IsAscendancyStartingNode", PassiveNodeKind::AscendancyStart),
    ] {
        if raw[field]
            .as_bool()
            .ok_or_else(|| format!("{field}: bool required"))?
        {
            return Ok(kind);
        }
    }
    // MasteryGroup is shared by ordinary notables and icon-only records; it is
    // not a node category. IsJustIcon supplies the non-allocatable mastery icon.
    Ok(PassiveNodeKind::Normal)
}

fn geometry(
    reference: &Value,
    graph: &reader::Graph,
    instance: &reader::InstanceInput,
    previous: Option<&PassiveNodeDef>,
    out: &mut serde_json::Map<String, Value>,
) -> Result<Value, String> {
    let group = &graph.groups[instance.group];
    if !group.x_raw.is_finite() || !group.y_raw.is_finite() {
        return Err("non-finite group centre".into());
    }
    let node = &reference["nodes"][instance.graph_id.to_string()];
    if node.is_null() {
        return Err(format!(
            "node {}: missing physical orbit-offset compatibility; refusing incomplete radius geometry",
            instance.graph_id
        ));
    }
    if node["skill"] != instance.graph_id
        || node["orbit"] != instance.radius
        || node["orbitIndex"] != instance.position
    {
        return Err(format!(
            "node {}: changed orbit/slot needs explicit geometry compatibility",
            instance.graph_id
        ));
    }
    let group_key = node["group"]
        .as_u64()
        .ok_or("missing reference group")?
        .to_string();
    if let Some(old) = previous
        && (old.group.map(u64::from) != node["group"].as_u64()
            || old.orbit.map(u64::from) != node["orbit"].as_u64()
            || old.orbit_index.map(u64::from) != node["orbitIndex"].as_u64())
    {
        return Err("prepared placement lacks exact reference group/orbit/slot binding".into());
    }
    let original_group = &reference["groups"][&group_key];
    for (key, centre) in [("x", group.x_raw), ("y", group.y_raw)] {
        // Prepared physical placement and even absence carry runtime semantics
        // (notably jewel radius and anoint-only nodes). They are not certified by
        // a raw graph group. Translate that placement, do not replace its radius
        // offset or manufacture coordinates for previously coordinate-less nodes.
        let original = if let Some(old) = previous {
            let Some(value) = (if key == "x" { old.x } else { old.y }) else {
                continue;
            };
            value
        } else {
            node[key].as_f64().ok_or("invalid reference coordinate")?
        };
        if !original.is_finite() {
            return Err("non-finite compatibility coordinate".into());
        }
        let original_centre = original_group[key]
            .as_f64()
            .filter(|n| n.is_finite())
            .ok_or("invalid reference group centre")?;
        // Preserve published six-significant-digit group coordinates, and translate
        // only the verified group centre. This does not interpret raw radius units.
        let rounded = round_group(centre)?;
        if previous.is_some() && rounded == original_centre {
            continue;
        }
        let value = original + (rounded - original_centre);
        if !value.is_finite() {
            return Err("non-finite projected coordinate".into());
        }
        out.insert(key.into(), json!(value));
    }
    Ok(
        json!({"owner":if previous.is_some(){"target group translation + prepared physical placement/coordinate absence"}else{"target group translation + explicit reference physical offset"},"reference_group":group_key,
        "source_group":instance.group,"radius":instance.radius,"position":instance.position}),
    )
}
fn round_group(value: f64) -> Result<f64, String> {
    if value == 0.0 {
        return Ok(value);
    }
    let places = 5 - value.abs().log10().floor() as i32;
    if places >= 0 {
        format!("{value:.precision$}", precision = places as usize)
            .parse()
            .map_err(|e| format!("group rounding: {e}"))
    } else {
        let scale = 10f64.powi(-places);
        Ok((value / scale).round() * scale)
    }
}

fn variants(
    input: &VerifiedFacts,
    facts: &Facts<'_>,
    policy: &Policy,
    renderer: &Renderer<'_>,
    previous: Option<&PassiveNodeDef>,
    out: &mut serde_json::Map<String, Value>,
    effects: &Effects<'_>,
) -> Result<Vec<Value>, String> {
    let ascendancy = effects.ascendancy;
    let id = out["id"]
        .as_str()
        .ok_or("missing output node identity")?
        .to_owned();
    let mut variants = out
        .get("variants")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let previous_classes: BTreeSet<_> = policy
        .class_bindings
        .iter()
        .filter(|b| b.base_id == id)
        .map(|b| b.class.as_str())
        .collect();
    let old_by_class: BTreeMap<_, _> = variants
        .iter()
        .cloned()
        .map(|v| {
            let class = v["class"]
                .as_str()
                .ok_or("invalid variant class")?
                .to_owned();
            Ok((class, v))
        })
        .collect::<Result<_, String>>()?;
    // Preserve unknown variant fields. Only declared target class relations replace
    // these entries; remaining ascendancy variants stay complete compatibility.
    variants.retain(|v| {
        let selector = v["class"].as_str().unwrap_or("");
        !previous_classes.contains(selector)
            && !ascendancy.is_some_and(|a| a.previous_selectors.contains(selector))
    });
    let mut records = Vec::new();
    for v in &variants {
        if let Some(skill) = v["variant_skill"].as_u64() {
            let rows: Vec<_> = facts
                .passives
                .values()
                .copied()
                .filter(|row| {
                    facts
                        .tables
                        .row("PassiveSkills", *row)
                        .is_ok_and(|r| r["PassiveSkillGraphId"] == skill)
                })
                .collect();
            let [row] = rows.as_slice() else {
                return Err(format!(
                    "variant {skill}: ambiguous/missing physical identity"
                ));
            };
            let raw = facts.tables.row("PassiveSkills", *row)?;
            if policy.owns_unlocks() {
                unlock::validate_variant(raw)?;
            }
            let variant_id = raw["Id"].as_str().ok_or("invalid variant Id")?;
            exact_compatibility(policy, variant_id, raw, &raw_stats(input, facts, *row)?)?;
            records.push(json!({"class":v["class"],"target":variant_id,"source_row":row,
                "owner":"prepared-whole-variant","reason":"non-class replacement law remains compatibility; changed source is rejected"}));
        } else {
            return Err(format!(
                "{id}: variant without a stable alias requires explicit binding"
            ));
        }
    }
    let mut relations = Vec::new();
    for relation in &facts.relations {
        if relation["definitions"][0]["id"] == id {
            let class = relation["character"]["name"]
                .as_str()
                .ok_or("invalid class relation")?;
            let row = relation["definitions"][1]["source_row"]
                .as_u64()
                .ok_or("invalid replacement row")? as usize;
            relations.push((
                class,
                row,
                Some(class),
                json!({"table":"ClassPassiveSkillOverrides","source_row":relation["source_row"]}),
            ));
        }
    }
    if let Some(source) = ascendancy {
        for link in source.links.get(&id).into_iter().flatten() {
            relations.push((
                link.selector.as_str(),
                link.target,
                link.previous_selector.as_deref(),
                json!({"table":"AscendancyPassiveSkillOverrides","source_row":link.row}),
            ));
        }
    }
    let mut classes = BTreeSet::new();
    for (class, row, old_selector, relationship) in relations {
        if !classes.insert(class) {
            return Err(format!("{id}: repeated replacement selector {class}"));
        }
        let raw = facts.tables.row("PassiveSkills", row)?;
        if policy.owns_unlocks() {
            unlock::validate_variant(raw)?;
        }
        let target = raw["Id"].as_str().ok_or("invalid replacement id")?;
        let binding = policy
            .compatibility_bindings
            .get(target)
            .ok_or_else(|| format!("{target}: missing variant context binding"))?;
        if binding
            .render_context_sha256
            .as_ref()
            .unwrap_or(&binding.context_sha256)
            != &hash(&render_context(policy, raw)?)?
        {
            return Err(format!("{target}: changed variant context"));
        }
        let old_variant = previous.and_then(|n| {
            n.variants
                .iter()
                .find(|v| Some(v.class.as_str()) == old_selector)
        });
        let target_skill = super::source::graph_id(raw)?;
        // A class slot can change its replacement. Its old text belongs only to
        // the original target, even if the new target has a valid semantic guard.
        let compatible_text = old_variant
            .filter(|v| v.variant_skill == Some(target_skill))
            .map(|v| v.stats.as_slice());
        let (stats, evidence) = text(
            input,
            facts,
            row,
            policy,
            renderer,
            compatible_text,
            effects.nonstat.as_ref(),
        )?;
        let mut replacement = old_by_class
            .get(old_selector.unwrap_or(class))
            .cloned()
            .unwrap_or_else(|| json!({}));
        replacement["class"] = json!(class);
        replacement["variant_skill"] = json!(target_skill);
        replacement["name"] = raw["Name"].clone();
        replacement["stats"] = json!(stats);
        variants.retain(|v| v["class"] != class);
        variants.push(replacement);
        records.push(json!({"class":class,"target":target,"source_row":row,"text":evidence,"relationship":relationship}));
    }
    if variants.is_empty() {
        out.remove("variants");
    } else {
        out.insert("variants".into(), json!(variants));
    }
    Ok(records)
}
