//! Source-owned ordinary scope catalogs with complete branch and unit metadata.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use pobr_data::catalog::stat_descriptions::StatDescriptionsDef;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::passive_tree::render::Renderer;
use super::stat_set_codec::read_pinned;
use crate::native_facts::reader::{self, VerifiedFacts};
use crate::raw_source::sha256_hex;

mod lookups;

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests;

pub(super) const TABLES: [&str; 1] = ["Stats"];
const GLOBAL: &str = "Data/StatDescriptions/stat_descriptions.csd";
const SCOPE: &str = "stat_descriptions";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    schema: String,
    version: String,
    source_sha256: String,
    descriptions_sha256: String,
    descriptions_manifest_sha256: String,
    compatibility_sha256: String,
    statement: String,
    #[serde(default)]
    lookup_manifest_sha256: Option<String>,
    #[serde(default)]
    diagnostic_ownership: Option<DiagnosticOwnership>,
    #[serde(default)]
    descriptor_equivalence: Option<DescriptorEquivalence>,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
enum DiagnosticOwnership {
    #[serde(rename = "explicit-no-description/v1")]
    ExplicitNoDescription,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
enum DescriptorEquivalence {
    #[serde(rename = "ordinary-plain-display/v1")]
    PlainDisplay,
}

pub(super) struct Converted {
    pub bytes: Vec<u8>,
    pub sidecars: BTreeMap<String, Vec<u8>>,
    pub protected_parents: Vec<PathBuf>,
    pub receipt: Value,
}

pub(super) fn convert(
    input: &VerifiedFacts,
    args: &super::CodecArgs,
    descriptions: &Path,
    supplement: Option<&reader::selected::Input>,
) -> Result<Converted, String> {
    let policy_input = read_pinned(&args.policy, &args.policy_sha256, "description policy")?;
    let policy: Policy =
        serde_json::from_value(policy_input.document).map_err(|e| e.to_string())?;
    if policy.schema != "pobr-ordinary-stat-descriptions-policy/v1"
        || policy.version != input.manifest["source_identity"]["game_patch"]
        || policy.source_sha256 != args.source_sha256
        || policy.statement.trim().is_empty()
    {
        return Err("description policy identity mismatch".into());
    }
    super::stat_set_codec::policy::hex(&policy.compatibility_sha256, 64, "compatibility")?;
    let legacy = read_pinned(&args.source, &args.source_sha256, "legacy descriptions")?;
    let csd = read_pinned(descriptions, &policy.descriptions_sha256, "descriptions")?;
    let manifest = read_pinned(
        &descriptions.with_file_name("manifest.json"),
        &policy.descriptions_manifest_sha256,
        "description manifest",
    )?;
    if csd.document["format"] != "repoe-native-description-branches/v1"
        || manifest.document["format"] != "repoe-native-descriptions/v1"
        || manifest.document["files"]["descriptions.json"]["sha256"] != policy.descriptions_sha256
        || manifest.document["files"]["descriptions.json"]["bytes"] != csd.bytes.len()
        || manifest.document["source_identity"]["game_patch"] != policy.version
    {
        return Err("description source identity mismatch".into());
    }
    let lookup_source = match (supplement, &policy.lookup_manifest_sha256) {
        (Some(extra), Some(pin)) if sha256_hex(&extra.manifest_bytes) == *pin => {
            Some(lookups::read(extra)?)
        }
        (None, None) => None,
        _ => return Err("description lookup manifest/policy mismatch".into()),
    };
    let empty = BTreeMap::new();
    let values = lookup_source.as_ref().map(|l| &l.values).unwrap_or(&empty);
    let mut projection = project_with_lookups(
        &legacy.document,
        &csd.document,
        values,
        policy.diagnostic_ownership,
        policy.descriptor_equivalence,
    )?;
    let compatibility_sha256 = sha256_hex(&reader::json_bytes(&projection.compatibility)?);
    if compatibility_sha256 != policy.compatibility_sha256 {
        return Err(format!(
            "description compatibility binding changed: expected {}, observed {compatibility_sha256}",
            policy.compatibility_sha256
        ));
    }
    projection.document["_meta"] = json!({
        "generator":"pobr-data-adapter ordinary stat-descriptions",
        "facts_manifest_sha256":sha256_hex(&input.manifest_bytes),
        "source_identity":input.manifest["source_identity"],
        "lookup_manifest_sha256":policy.lookup_manifest_sha256,
        "descriptions_sha256":policy.descriptions_sha256,
        "descriptions_manifest_sha256":policy.descriptions_manifest_sha256,
        "policy_sha256":args.policy_sha256,
        "legacy_source_sha256":args.source_sha256,"legacy_metadata":legacy.document["_meta"],
        "status":"domain candidate; source acquisition draft; not adopted"
    });
    let mut receipt = json!({
        "owned":"Own global/passive/aura CSD descriptors, ordered ordinary branches, representative single/compound templates and matching unrendered membership",
        "target":projection.target,"compatibility":projection.compatibility,
        "lookups":lookup_source.as_ref().map(|l|&l.receipt),
        "compatibility_sha256":compatibility_sha256,"statement":policy.statement,
        "legacy_source":{"sha256":args.source_sha256,"metadata":legacy.document["_meta"]},
        "descriptions":{"sha256":policy.descriptions_sha256,
            "manifest_sha256":policy.descriptions_manifest_sha256,"source_identity":manifest.document["source_identity"]},
        "preserved":"Unknown fields and unsupported/absent complete descriptors remain pinned compatibility. No generated modifier rules, translation or calculation law is changed.",
        "limits":"Representative numeric 1 remains a capture marker. Optional raw-unit branch/format metadata owns reverse tier matching; ambiguous/unknown records remain complete diagnostics, never effectless approval."
    });
    if let Some(ownership) = policy.diagnostic_ownership {
        receipt["diagnostic_ownership"] = json!(ownership);
        receipt["diagnostic_limits"] = json!(
            "Own unique explicit no_description entries as unrendered diagnostics only; no rendered text is removed and no stat is accepted as effectless or omitted from complete effect packets."
        );
        projection.document["_meta"]["diagnostic_ownership"] = json!(ownership);
    }
    if let Some(equivalence) = policy.descriptor_equivalence {
        receipt["descriptor_equivalence"] = json!(equivalence);
        projection.document["_meta"]["descriptor_equivalence"] = json!(equivalence);
    }
    let mut sidecars = BTreeMap::from([
        ("description-policy.json".into(), policy_input.bytes),
        ("legacy-stat-descriptions.json".into(), legacy.bytes),
        ("description-manifest.json".into(), manifest.bytes),
    ]);
    if let Some(extra) = supplement {
        sidecars.insert("lookup-manifest.json".into(), extra.manifest_bytes.clone());
        sidecars.insert(
            "lookup-tables.json".into(),
            reader::json_bytes(&extra.documents)?,
        );
    }
    Ok(Converted {
        bytes: super::pretty_bytes(&projection.document)?,
        sidecars,
        protected_parents: vec![
            policy_input.parent,
            legacy.parent,
            csd.parent,
            manifest.parent,
        ],
        receipt,
    })
}

struct Projection {
    document: Value,
    target: BTreeMap<String, Value>,
    compatibility: BTreeMap<String, Value>,
}

fn entry_key(scope: &str, id: &str) -> String {
    if scope == SCOPE {
        id.into()
    } else {
        format!("{scope}::{id}")
    }
}

#[cfg(test)]
fn project(legacy: &Value, csd: &Value) -> Result<Projection, String> {
    project_with_lookups(legacy, csd, &BTreeMap::new(), None, None)
}
fn project_with_lookups(
    legacy: &Value,
    csd: &Value,
    lookup_values: &super::passive_tree::render::LookupValues,
    diagnostic_ownership: Option<DiagnosticOwnership>,
    descriptor_equivalence: Option<DescriptorEquivalence>,
) -> Result<Projection, String> {
    let typed: StatDescriptionsDef =
        serde_json::from_value(legacy.clone()).map_err(|e| e.to_string())?;
    if !typed.scopes.contains_key(SCOPE) {
        return Err("missing legacy global scope".into());
    }
    let mut document = legacy.clone();
    let mut target = BTreeMap::new();
    let mut compatibility = BTreeMap::new();
    let mut scopes: BTreeSet<_> = typed.scopes.keys().cloned().collect();
    scopes.insert(SCOPE.into());
    for name in scopes {
        let path = if name == SCOPE {
            GLOBAL.to_owned()
        } else {
            format!("Data/StatDescriptions/{name}.csd")
        };
        let legacy_scope = typed.scopes.get(&name).cloned().unwrap_or_default();
        let mut old_ids: BTreeSet<_> = legacy_scope
            .single
            .keys()
            .chain(legacy_scope.unrendered.iter())
            .chain(legacy_scope.compound.values().flat_map(|c| &c.member_stats))
            .cloned()
            .collect();
        let Some(records) = csd["files"][&path]["records"].as_array() else {
            for id in old_ids {
                compatibility.insert(entry_key(&name,&id),json!({"scope":name,"reason":"absent target CSD scope; legacy compatibility only","exact_path":path}));
            }
            continue;
        };
        if document["scopes"][&name].is_null() {
            document["scopes"][&name] = json!({"single":{},"compound":{},"unrendered":[]});
        }
        let renderer = Renderer::scope(csd, &path)?.with_lookup_values(lookup_values);
        let renderer = match descriptor_equivalence {
            Some(DescriptorEquivalence::PlainDisplay) => {
                renderer.with_plain_display_equivalence()?
            }
            None => renderer,
        };
        let mut owners: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
        for record in records
            .iter()
            .filter(|r| r["kind"] == "description" || r["kind"] == "no_description")
        {
            let ids = record["ids"].as_array().ok_or("missing descriptor ids")?;
            if ids.is_empty() {
                return Err("empty descriptor ids".into());
            }
            for id in ids {
                let id = id
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or("invalid descriptor id")?;
                owners.entry(id.into()).or_default().push(record);
            }
        }
        let mut completed = BTreeSet::new();
        let mut migrated = BTreeSet::new();
        let mut branch_only = BTreeSet::new();
        let mut descriptors = serde_json::Map::new();
        for (id, source) in &owners {
            old_ids.remove(id);
            if completed.contains(id) {
                continue;
            }
            // This set is a display diagnostic, not an effectless-stat allowlist.
            // An explicit directive may own it without authorizing Renderer::render
            // to omit that member from a complete passive/socketable effect packet.
            if diagnostic_ownership.is_some()
                && matches!(source.as_slice(), [record]
                    if record["kind"] == "no_description" && record["ids"] == json!([id]))
                && !legacy_scope.single.contains_key(id)
                && !legacy_scope
                    .compound
                    .values()
                    .any(|c| c.member_stats.contains(id))
                && !legacy_scope
                    .descriptors
                    .values()
                    .any(|d| d.member_stats.contains(id))
            {
                completed.insert(id.clone());
                migrated.insert(id.clone());
                branch_only.insert(id.clone());
                target.insert(
                    entry_key(&name, id),
                    json!({
                        "scope":name,"ownership":"unrendered-diagnostic",
                        "source_records_sha256":sha256_hex(&reader::json_bytes(source)?),
                        "effectless":false
                    }),
                );
                continue;
            }
            match renderer.catalog(id) {
                Ok((descriptor, evidence)) => {
                    let first = descriptor
                        .member_stats
                        .first()
                        .ok_or("empty descriptor ids")?;
                    let owned: BTreeSet<_> = descriptor.member_stats.iter().cloned().collect();
                    let original_compound = legacy_scope
                        .compound
                        .iter()
                        .find(|(_, c)| {
                            c.member_stats.iter().cloned().collect::<BTreeSet<_>>() == owned
                        })
                        .map(|(key, _)| legacy["scopes"][&name]["compound"][key].clone());
                    // Erase stale forms only for members now represented by this complete descriptor.
                    if let Some(single) = document["scopes"][&name]["single"].as_object_mut() {
                        single.retain(|id, _| !owned.contains(id));
                    }
                    if let Some(compounds) = document["scopes"][&name]["compound"].as_object_mut() {
                        compounds.retain(|key, c| {
                            key == first
                                || c["member_stats"].as_array().is_none_or(|a| {
                                    !a.iter()
                                        .any(|id| id.as_str().is_some_and(|id| owned.contains(id)))
                                })
                        });
                    }
                    if descriptor.member_stats.len() == 1 {
                        // The old single channel uses numeric 1 as a capture, not
                        // the actual scaled display of raw=1. Metadata owns units.
                        if let Some(branch) = descriptor.branches.iter().find(|b| {
                            b.conditions.iter().all(|c| {
                                c.min.is_none_or(|m| m <= 1)
                                    && c.max.is_none_or(|m| m >= 1)
                                    && c.not != Some(1)
                            })
                        }) {
                            let text =
                                super::passive_tree::render::representative(&branch.template, 1)?;
                            document["scopes"][&name]["single"][first] = json!([text]);
                        } else {
                            branch_only.insert(first.clone());
                        }
                    } else {
                        if !document["scopes"][&name]["compound"][first].is_object() {
                            document["scopes"][&name]["compound"][first] =
                                original_compound.unwrap_or_else(|| json!({}));
                        }
                        document["scopes"][&name]["compound"][first]["member_stats"] =
                            json!(descriptor.member_stats);
                        document["scopes"][&name]["compound"][first]["template"] =
                            json!(descriptor.branches[0].template);
                    }
                    descriptors.insert(
                        first.clone(),
                        serde_json::to_value(&descriptor).map_err(|e| e.to_string())?,
                    );
                    for member in &descriptor.member_stats {
                        completed.insert(member.clone());
                        migrated.insert(member.clone());
                        target.insert(
                            entry_key(&name, member),
                            json!({"scope":name,"descriptor":first,"evidence":evidence}),
                        );
                    }
                }
                Err(reason) => {
                    compatibility.insert(entry_key(&name,id),json!({"scope":name,"reason":reason,
                        "source_records_sha256":sha256_hex(&reader::json_bytes(source)?),
                        "legacy_single":legacy_scope.single.get(id),
                        "legacy_compounds":legacy_scope.compound.iter().filter(|(_,c)|c.member_stats.contains(id)).collect::<BTreeMap<_,_>>(),
                        "legacy_unrendered":legacy_scope.unrendered.contains(id)}));
                }
            }
        }
        for id in old_ids {
            compatibility.insert(
                entry_key(&name, &id),
                json!({"scope":name,"reason":"absent from target CSD; legacy compatibility only"}),
            );
        }
        if diagnostic_ownership.is_some()
            && !branch_only.is_empty()
            && document["scopes"][&name]["unrendered"].is_null()
        {
            document["scopes"][&name]["unrendered"] = json!([]);
        }
        if let Some(unrendered) = document["scopes"][&name]["unrendered"].as_array_mut() {
            unrendered.retain(|id| id.as_str().is_none_or(|id| !migrated.contains(id)));
            unrendered.extend(branch_only.into_iter().map(Value::String));
            unrendered.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
        }
        let used: BTreeSet<_> = descriptors
            .values()
            .flat_map(|d| d["branches"].as_array().into_iter().flatten())
            .flat_map(|b| b["lookups"].as_array().into_iter().flatten())
            .filter_map(Value::as_str)
            .collect();
        if !used.is_empty() {
            document["scopes"][&name]["lookup_values"] = serde_json::to_value(
                lookup_values
                    .iter()
                    .filter(|(name, _)| used.contains(name.as_str()))
                    .collect::<BTreeMap<_, _>>(),
            )
            .map_err(|e| e.to_string())?;
        }
        document["scopes"][&name]["descriptors"] = Value::Object(descriptors);
    }
    Ok(Projection {
        document,
        target,
        compatibility,
    })
}
