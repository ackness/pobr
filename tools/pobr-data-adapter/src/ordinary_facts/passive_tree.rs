//! Complete prepared ordinary tree with narrowly qualified target native facts.

mod policy;
mod prepared;
mod source;
#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
pub(super) mod tests;

use super::stat_set_codec::read_pinned;
use crate::native_facts::reader::{self, VerifiedFacts};
use crate::raw_source::sha256_hex;
use pobr_core::rules::passive_stats;
use pobr_data::catalog::{PassiveNodeDef, PassiveNodeKind, PassiveTreeMeta};
use pobr_data::native_stat::{NativePassiveContext, NativePassiveStats};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

pub(super) const TABLES: [&str; 4] = [
    "PassiveSkills",
    "Stats",
    "Characters",
    "ClassPassiveSkillOverrides",
];

pub(super) struct Converted {
    pub base: BTreeMap<String, Vec<u8>>,
    pub source_bytes: Vec<u8>,
    pub policy_bytes: Vec<u8>,
    pub protected_parents: [PathBuf; 2],
    pub receipt: Value,
}

pub(super) fn convert(input: &VerifiedFacts, args: &super::CodecArgs) -> Result<Converted, String> {
    let pinned_policy = read_pinned(&args.policy, &args.policy_sha256, "passive-tree policy")?;
    let policy: policy::Policy = serde_json::from_value(pinned_policy.document)
        .map_err(|e| format!("passive-tree policy: {e}"))?;
    policy.validate(input)?;
    if policy.source.sha256 != args.source_sha256 {
        return Err("prepared tree pin differs from policy source identity".into());
    }
    let legacy = read_pinned(&args.source, &args.source_sha256, "prepared tree")?;
    let nodes: Vec<PassiveNodeDef> = serde_json::from_value(legacy.document.clone())
        .map_err(|e| format!("prepared tree DTO: {e}"))?;
    let meta: PassiveTreeMeta = serde_json::from_value(policy.legacy_metadata.value.clone())
        .map_err(|e| format!("prepared metadata DTO: {e}"))?;
    prepared::validate(&nodes, &meta)?;
    let facts = source::Facts::read(input, &policy)?;
    let connections = prepared::connections(&nodes, &legacy.document, &policy, &facts)?;
    let (metadata, class_records) =
        prepared::metadata(&policy.legacy_metadata.value, &meta, &facts)?;
    let aliases: BTreeMap<_, _> = policy
        .legacy_id_bindings
        .iter()
        .enumerate()
        .map(|(i, b)| (b.legacy_id.as_str(), (i, b)))
        .collect();
    let mut used_aliases = BTreeSet::new();
    let mut records = Vec::new();
    let mut native_count = 0;
    let mut document = legacy.document.clone();
    let output_nodes = document
        .as_array_mut()
        .ok_or("prepared tree must be an array")?;
    for (source_row, (node, output)) in nodes.iter().zip(output_nodes).enumerate() {
        let raw_entry = &legacy.document[source_row];
        let entry_hash = sha256_hex(&reader::json_bytes(raw_entry)?);
        let alias = aliases.get(node.id.as_str());
        let target_id = if let Some(&(index, binding)) = alias {
            if node.skill != binding.skill
                || node.name.as_deref() != Some(binding.expected_name.as_str())
                || entry_hash != binding.source_entry_sha256
                || facts.passives.contains_key(&node.id)
            {
                return Err("prepared identity binding source mismatch or stale alias".into());
            }
            used_aliases.insert(index);
            binding.target_passive_id.as_str()
        } else {
            node.id.as_str()
        };
        let &row = facts
            .passives
            .get(target_id)
            .ok_or_else(|| format!("missing target passive Id {target_id}"))?;
        let raw = facts.tables.row("PassiveSkills", row)?;
        let graph = source::graph_id(raw)?;
        if graph != node.skill {
            return Err(format!(
                "prepared node {}: target graph ID mismatch",
                node.id
            ));
        }
        if let Some(&(_, binding)) = alias
            && raw["Name"].as_str() != Some(binding.expected_name.as_str())
        {
            return Err("prepared identity binding target name mismatch".into());
        }
        let stats = facts.stats(input, row)?;
        let differences = facts.differences(raw)?;
        let relations = facts.involved.get(&row).cloned().unwrap_or_default();
        let mut reasons = Vec::new();
        if alias.is_some() {
            reasons.push("explicit legacy identity alias".to_owned());
        }
        if stats.len() != 1 {
            reasons.push("complete source is not exactly one stat".into());
        }
        if !differences.is_empty() {
            reasons.push("whole source context differs from reviewed reference".into());
        }
        if !relations.is_empty() {
            reasons.push("source class override participant".into());
        }
        if node.kind != PassiveNodeKind::Normal
            || !node.variants.is_empty()
            || node.ascendancy_id.is_some()
            || node.unlock_constraint.is_some()
            || node.apply_to_armour
        {
            reasons.push("prepared node has transformed/conditional context".into());
        }
        if !matches!(node.stats.as_slice(),[line] if !line.is_empty() && line.trim()==line && !line.contains(['\n','\r']))
        {
            reasons.push("prepared display is not one nonempty trimmed line".into());
        }
        if let [stat] = stats.as_slice()
            && let Err(error) = passive_stats::map_stat(stat)
        {
            reasons.push(format!("shared mapper rejected complete stat: {error:?}"));
        }
        let packet = if reasons.is_empty() {
            native_count += 1;
            Some(NativePassiveStats {
                data_version: input.manifest["source_identity"]["game_patch"]
                    .as_str()
                    .ok_or("missing patch")?
                    .into(),
                tree_version: Some(policy.catalog.tree_compatibility_tag.clone()),
                node_id: node.skill,
                node_slug: node.id.clone(),
                modifier_texts: node.stats.clone(),
                context: NativePassiveContext::UnconditionalPlayer,
                stats: stats.clone(),
            })
        } else {
            None
        };
        let object = output
            .as_object_mut()
            .ok_or("prepared entry must be an object")?;
        object.remove("native_stats");
        if let Some(packet) = &packet {
            object.insert(
                "native_stats".into(),
                serde_json::to_value(packet).map_err(|e| e.to_string())?,
            );
        }
        let retained:Vec<_>=raw_entry.as_object().ok_or("invalid prepared entry")?.iter().filter(|(field,_)|field.as_str()!="native_stats")
            .map(|(field,value)|Ok(json!({"field":field,"source_pointer":format!("/{source_row}/{}",prepared::token(field)),
                "value_sha256":sha256_hex(&reader::json_bytes(value)?)}))).collect::<Result<_,String>>()?;
        records.push(json!({"source_row":source_row,"source_pointer":format!("/{source_row}"),"skill":node.skill,"id":node.id,
            "source_entry_sha256":entry_hash,"identity_alias":alias.map(|(_,b)|*b),
            "target":{"source_row":row,"id":target_id,"graph_id":graph,"name":raw["Name"],"source_pointer":format!("/rows/{row}/values"),"row_sha256":sha256_hex(&reader::json_bytes(raw)?)},
            "ordered_raw_stats":stats,"context":{"matches":differences.is_empty(),"differing_fields":differences,"class_override_rows":relations},
            "native":{"present":packet.is_some(),"reasons":reasons,"packet_sha256":packet.as_ref().map(reader::json_bytes).transpose()?.map(|b|sha256_hex(&b)),
                "legacy_packet_present":raw_entry.get("native_stats").is_some(),"display_scope":"unchanged prepared binding/display text; not a certified target formatter"},
            "retained_fields":retained,"output_entry_sha256":sha256_hex(&reader::json_bytes(output)?)}));
    }
    if used_aliases.len() != aliases.len() {
        return Err("unused/stale prepared identity binding".into());
    }
    let _: Vec<PassiveNodeDef> =
        serde_json::from_value(document.clone()).map_err(|e| format!("output tree DTO: {e}"))?;
    let legacy_identity = json!({"file":"legacy-passive-tree.json","bytes":legacy.bytes.len(),"sha256":sha256_hex(&legacy.bytes),"declaration":policy.source});
    let policy_identity = json!({"file":"codec-policy.json","bytes":pinned_policy.bytes.len(),"sha256":sha256_hex(&pinned_policy.bytes)});
    let metadata_authoring = json!({"declaration":policy.legacy_metadata.source_declaration,"file_read_by_producer":false,"file_hash_verified_by_producer":false,
        "policy_owned_value":policy.legacy_metadata.value,"value_sha256":sha256_hex(&reader::json_bytes(&policy.legacy_metadata.value)?)});
    let target = json!({"facts_manifest_sha256":sha256_hex(&input.manifest_bytes),"source_identity":input.manifest["source_identity"],
        "owned_fields":{"tree":["native_stats"],"class":["base_str","base_dex","base_int"]}});
    let reference_raw = facts.tables.row("PassiveSkills", facts.reference_row)?;
    let receipt = json!({"entries":records.len(),"native_packets":native_count,"source_records":records,"legacy_source":legacy_identity,
        "codec_policy":policy_identity,"target_fields":target,"metadata_authoring":metadata_authoring,"class_records":class_records,
        "output_metadata":metadata,"output_metadata_sha256":sha256_hex(&reader::json_bytes(&metadata)?),
        "qualification_reference":{"declaration":policy.qualification,"source_row":facts.reference_row,"raw":reference_raw,
            "row_sha256":sha256_hex(&reader::json_bytes(reference_raw)?),"context":facts.reference_context},
        "class_override_inventory":facts.relations,"connection_exceptions":connections,
        "historical_evidence_declarations":policy.evidence,"compatibility_statement":policy.statement,
        "unconsumed_source":facts.unmapped(input)?,
        "pending":"complete prepared membership/text/topology/variants/unlock/ascendancies/geometry/armour flags remain fixed compatibility, not target text/legality/whole-patch certification; source approval, complete target package, BD/Web and adoption remain separate"});
    Ok(Converted {
        base: BTreeMap::from([
            ("passive_tree.json".into(), super::pretty_bytes(&document)?),
            (
                "passive_tree_meta.json".into(),
                super::pretty_bytes(&metadata)?,
            ),
        ]),
        source_bytes: legacy.bytes,
        policy_bytes: pinned_policy.bytes,
        protected_parents: [legacy.parent, pinned_policy.parent],
        receipt,
    })
}
