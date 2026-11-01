//! RePoE global singleton descriptions to the existing representative-value catalog.
//!
//! TierIndex treats 1 as a capture, so scaled or compound descriptions cannot be
//! substituted by rendering arbitrary packets. Keep those legacy fields explicit.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use pobr_data::catalog::stat_descriptions::StatDescriptionsDef;
use serde::Deserialize;
use serde_json::{Value, json};

use super::passive_tree::render::Renderer;
use super::stat_set_codec::read_pinned;
use crate::native_facts::reader::{self, VerifiedFacts};
use crate::raw_source::sha256_hex;

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
    let mut projection = project(&legacy.document, &csd.document)?;
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
        "descriptions_sha256":policy.descriptions_sha256,
        "descriptions_manifest_sha256":policy.descriptions_manifest_sha256,
        "policy_sha256":args.policy_sha256,
        "legacy_source_sha256":args.source_sha256,"legacy_metadata":legacy.document["_meta"],
        "status":"domain candidate; source acquisition draft; not adopted"
    });
    let receipt = json!({
        "owned":"scopes.stat_descriptions.single: unscaled singleton V=1 text; matching unrendered membership",
        "target":projection.target,"compatibility":projection.compatibility,
        "compatibility_sha256":compatibility_sha256,"statement":policy.statement,
        "legacy_source":{"sha256":args.source_sha256,"metadata":legacy.document["_meta"]},
        "descriptions":{"sha256":policy.descriptions_sha256,
            "manifest_sha256":policy.descriptions_manifest_sha256,"source_identity":manifest.document["source_identity"]},
        "preserved":"All compound descriptions, other scopes, unknown fields and legacy-only IDs remain fixed compatibility. No generated modifier rules, translation, math or runtime package is changed.",
        "limits":"V=1 representative catalog, not a general value renderer. Scaled/negative-only/conditional/compound descriptors remain compatibility, not target applicability approval. No no_description record is classified effectless."
    });
    Ok(Converted {
        bytes: super::pretty_bytes(&projection.document)?,
        sidecars: BTreeMap::from([
            ("description-policy.json".into(), policy_input.bytes),
            ("legacy-stat-descriptions.json".into(), legacy.bytes),
            ("description-manifest.json".into(), manifest.bytes),
        ]),
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

fn project(legacy: &Value, csd: &Value) -> Result<Projection, String> {
    let typed: StatDescriptionsDef =
        serde_json::from_value(legacy.clone()).map_err(|e| e.to_string())?;
    let scope = typed
        .scopes
        .get(SCOPE)
        .ok_or("missing legacy global scope")?;
    if !legacy["scopes"][SCOPE]["single"].is_object() {
        return Err("legacy global single must be an object".into());
    }
    let compound_members: BTreeSet<_> = scope
        .compound
        .values()
        .flat_map(|c| &c.member_stats)
        .collect();
    let renderer = Renderer::global(csd)?;
    let records = csd["files"][GLOBAL]["records"]
        .as_array()
        .ok_or("missing global records")?;
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
                .filter(|id| !id.is_empty())
                .ok_or("invalid descriptor id")?;
            owners.entry(id.into()).or_default().push(record);
        }
    }
    let mut document = legacy.clone();
    let mut target = BTreeMap::new();
    let mut compatibility = BTreeMap::new();
    for (id, source) in &owners {
        let rendered = if compound_members.contains(id) {
            Err("legacy compound membership is preserved as a whole".into())
        } else {
            singleton(&renderer, id)
        };
        match rendered {
            Ok((lines, evidence)) => {
                document["scopes"][SCOPE]["single"][id] = json!(lines);
                target.insert(id.clone(), json!({"lines":lines,"evidence":evidence}));
            }
            Err(reason) => {
                compatibility.insert(
                    id.clone(),
                    json!({"reason":reason,
                    "source_records_sha256":sha256_hex(&reader::json_bytes(source)?),
                    "legacy_single":scope.single.get(id),
                    "legacy_compound_member":compound_members.contains(id),
                    "legacy_unrendered":scope.unrendered.contains(id)}),
                );
            }
        }
    }
    for id in scope
        .single
        .keys()
        .chain(scope.unrendered.iter())
        .chain(compound_members)
    {
        if !owners.contains_key(id) {
            compatibility.insert(
                id.clone(),
                json!({"reason":"absent from target global CSD; legacy compatibility only"}),
            );
        }
    }
    if let Some(unrendered) = document["scopes"][SCOPE]["unrendered"].as_array_mut() {
        unrendered.retain(|id| id.as_str().is_none_or(|id| !target.contains_key(id)));
    }
    Ok(Projection {
        document,
        target,
        compatibility,
    })
}

fn singleton(renderer: &Renderer<'_>, id: &str) -> Result<(Vec<String>, Vec<Value>), String> {
    let (lines, evidence) = renderer.render(&[(id.into(), 1)])?;
    if evidence.len() != 1 || evidence[0]["ids"] != json!([id]) {
        return Err("not a singleton descriptor".into());
    }
    if evidence[0]["handlers"]
        .as_array()
        .is_none_or(|h| !h.is_empty())
    {
        return Err(
            "numeric/display handler requires separate representative-template policy".into(),
        );
    }
    // This catalog's array contains alternative templates, not display lines.
    // Preserve the selected branch as one template, including embedded newlines.
    let text = pobr_core::mod_parser::engine::strip_display_markup(&lines.join("\n"));
    Ok((vec![text], evidence))
}
