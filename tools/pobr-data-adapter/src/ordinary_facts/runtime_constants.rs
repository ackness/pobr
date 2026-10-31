//! Three ordinary numeric domains with target-owned leaves and explicit legacy models.

mod policy;
#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests;

use super::stat_set_codec::policy::hex;
use super::stat_set_codec::{Pinned, read_pinned};
use crate::native_facts::reader::{self, Tables, VerifiedFacts};
use crate::raw_source::sha256_hex;
use pobr_data::catalog::character_constants::CharacterConstantsDef;
use pobr_data::catalog::game_constants::GameConstantsDef;
use pobr_data::catalog::jewel_radii::JewelRadiiDef;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

pub(super) const TABLES: [&str; 1] = ["GameConstants"];
const TABLE: &str = TABLES[0];
const PAYLOAD: &str = "tables/GameConstants.json";

pub(super) struct Converted {
    pub base: BTreeMap<String, Vec<u8>>,
    pub legacy_files: BTreeMap<String, Vec<u8>>,
    pub manifest_bytes: Vec<u8>,
    pub policy_bytes: Vec<u8>,
    pub protected_parents: Vec<PathBuf>,
    pub receipt: Value,
}

pub(super) fn convert(input: &VerifiedFacts, args: &super::CodecArgs) -> Result<Converted, String> {
    let pinned_policy = read_pinned(
        &args.policy,
        &args.policy_sha256,
        "runtime-constants policy",
    )?;
    let p: policy::Policy = serde_json::from_value(pinned_policy.document)
        .map_err(|e| format!("runtime-constants policy: {e}"))?;
    p.validate(input)?;
    if p.source_snapshot.sha256 != args.source_sha256 {
        return Err("snapshot pin differs from policy source identity".into());
    }
    let manifest = read_pinned(
        &args.source,
        &args.source_sha256,
        "legacy constants manifest",
    )?;
    let sources = read_sources(&manifest, &p)?;
    let tables = Tables::selected(input, &TABLES)?;
    for (field, kind) in [("Id", "string"), ("Value", "i32"), ("Divisor", "i32")] {
        super::convert::column(
            input,
            TABLE,
            &[field],
            &json!({"name":field,"type":kind,"array":false,"interval":false,"references":null}),
        )?;
    }
    let ids: BTreeMap<_, _> = (0..tables.len(TABLE))
        .map(|row| {
            Ok((
                tables.row(TABLE, row)?["Id"]
                    .as_str()
                    .ok_or("invalid constant Id")?
                    .to_owned(),
                row,
            ))
        })
        .collect::<Result<_, String>>()?;
    let mut documents = BTreeMap::new();
    let mut shapes = BTreeMap::new();
    for (name, source) in &sources {
        shapes.insert(name.clone(), numeric_fields(name, &source.document)?);
        documents.insert(name.clone(), source.document.clone());
    }
    let mut occurrences: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    let mut records = Vec::new();
    for (position, binding) in p.bindings.iter().enumerate() {
        if !shapes[&binding.file].contains(&binding.target_pointer) {
            return Err(format!(
                "unknown/non-numeric DTO field: {}{}",
                binding.file, binding.target_pointer
            ));
        }
        let &row = ids
            .get(&binding.source_id)
            .ok_or_else(|| format!("missing GameConstants Id {:?}", binding.source_id))?;
        let raw = tables.row(TABLE, row)?;
        let raw_value = integer(&raw["Value"], "Value")?;
        let divisor = integer(&raw["Divisor"], "Divisor")?;
        let (value, steps) = normalize(raw_value, divisor, &binding.additional_divisors)?;
        let document = documents
            .get_mut(&binding.file)
            .ok_or("missing selected document")?;
        let (parent, leaf) = leaf_parent(document, &binding.target_pointer)?;
        let old = parent.get(&leaf).cloned();
        match &old {
            Some(v) if v.as_f64().is_some_and(f64::is_finite) => {}
            None if binding.allow_missing_leaf => {}
            _ => {
                return Err(format!(
                    "{}{}: required numeric leaf missing/invalid",
                    binding.file, binding.target_pointer
                ));
            }
        }
        parent.insert(leaf, json!(value));
        occurrences.entry(row).or_default().push(position);
        records.push(json!({"binding_position":position,"binding":binding,"source_row":row,"source_id":raw["Id"],
            "raw_value":raw_value,"raw_divisor":divisor,"source_payload":PAYLOAD,"source_payload_sha256":sha256_hex(&input.bytes[PAYLOAD]),
            "source_pointers":{"id":format!("/rows/{row}/values/Id"),"value":format!("/rows/{row}/values/Value"),"divisor":format!("/rows/{row}/values/Divisor")},
            "normalization_steps":steps,"output":value,"legacy":{"present":old.is_some(),"value":old,"equals_normalized_f64":old.as_ref().and_then(Value::as_f64).map(|v|v==value)},
            "scope":"target-owned value; old equality is diagnostic only, never an input or admission condition"}));
    }
    let mut base = BTreeMap::new();
    let mut file_records = Vec::new();
    let mut legacy_files = BTreeMap::new();
    let mut protected_parents = vec![manifest.parent.clone(), pinned_policy.parent];
    for selected in &p.source_files {
        let source = &sources[&selected.file];
        let document = &documents[&selected.file];
        numeric_fields(&selected.file, document)?; // Output DTO validation, not default materialization.
        let bytes = super::pretty_bytes(document)?;
        let owned: Vec<_> = p
            .bindings
            .iter()
            .filter(|b| b.file == selected.file)
            .map(|b| b.target_pointer.as_str())
            .collect();
        let mut retained_source = source.document.clone();
        let mut retained_output = document.clone();
        for pointer in &owned {
            let (parent, leaf) = leaf_parent(&mut retained_source, pointer)?;
            parent.remove(&leaf);
            let (parent, leaf) = leaf_parent(&mut retained_output, pointer)?;
            parent.remove(&leaf);
        }
        if retained_source != retained_output {
            return Err("non-owned constants fields changed".into());
        }
        file_records.push(json!({"declaration":selected,"legacy_archive":format!("legacy-constants/{}",selected.file),
            "source":{"bytes":source.bytes.len(),"sha256":sha256_hex(&source.bytes),"file_read_and_hash_verified":true},
            "output":{"file":format!("base/{}",selected.file),"bytes":bytes.len(),"sha256":sha256_hex(&bytes)},
            "owned_pointers":owned,"retained_minus_owned_value_sha256":sha256_hex(&reader::json_bytes(&retained_source)?),
            "retention_rule":"full raw JSON Value minus exactly the owned object leaves; empty parents, unknowns, arrays, order and absent fields preserved",
            "scope":"all remaining numeric/model/radius fields are verified legacy compatibility inputs, not target facts"}));
        base.insert(selected.file.clone(), bytes);
        legacy_files.insert(selected.file.clone(), source.bytes.clone());
        protected_parents.push(source.parent.clone());
    }
    let mut unused = Vec::new();
    let mut shared = Vec::new();
    for row in 0..tables.len(TABLE) {
        let raw = tables.row(TABLE, row)?;
        if let Some(positions) = occurrences.get(&row) {
            shared.push(json!({"source_row":row,"id":raw["Id"],"binding_positions":positions}));
        } else {
            unused.push(json!({"source_row":row,"id":raw["Id"],"source_pointer":format!("/rows/{row}/values"),"raw":raw,
                "row_sha256":sha256_hex(&reader::json_bytes(raw)?),"evaluated_as_model":false}));
        }
    }
    let columns: Vec<_> = input.documents[PAYLOAD]["columns"]
        .as_array()
        .ok_or("missing columns")?
        .iter()
        .filter(|c| {
            c["physical_fields"].as_array().is_some_and(|fs| {
                fs.iter()
                    .any(|f| !matches!(f.as_str(), Some("Id" | "Value" | "Divisor")))
            })
        })
        .collect();
    let receipt = json!({"bindings":records,"source_occurrences":shared,"files":file_records,
        "legacy_snapshot":{"declaration":p.source_snapshot,"archive":"legacy-manifest.json","bytes":manifest.bytes.len(),"sha256":sha256_hex(&manifest.bytes),
            "file_read_and_hash_verified":true,"unselected_scope":"manifest metadata only; no unselected file opened, hash-verified or certified for the target"},
        "codec_policy":{"archive":"codec-policy.json","bytes":pinned_policy.bytes.len(),"sha256":sha256_hex(&pinned_policy.bytes)},
        "target_fields":{"source_identity":input.manifest["source_identity"],"facts_manifest_sha256":sha256_hex(&input.manifest_bytes),"payload":PAYLOAD,"payload_sha256":sha256_hex(&input.bytes[PAYLOAD])},
        "unconsumed_source":{"table":TABLE,"columns":columns,"rows":unused,"source_row_range":{"start":0,"end_exclusive":tables.len(TABLE)},"scope":"unapplied facts, not globally effectless; unused values/divisors are not evaluated"},
        "historical_evidence_declarations":p.evidence,"compatibility_statement":p.statement,
        "pending":"retained model constants/radii remain fixed compatibility, not independent target laws; full runtime package, ordinary BD/Web/accuracy, source approval and adoption remain separate"});
    Ok(Converted {
        base,
        legacy_files,
        manifest_bytes: manifest.bytes,
        policy_bytes: pinned_policy.bytes,
        protected_parents,
        receipt,
    })
}

fn read_sources(manifest: &Pinned, p: &policy::Policy) -> Result<BTreeMap<String, Pinned>, String> {
    let m = &manifest.document;
    if m["schema_version"].as_u64() != Some(3)
        || m["poe_version"].as_str() != Some(p.source_snapshot.snapshot_version.as_str())
    {
        return Err("legacy constants manifest schema/version mismatch".into());
    }
    let files = m["files"]
        .as_object()
        .ok_or("manifest files must be an object of SHA strings")?;
    for hash in files.values() {
        hex(
            hash.as_str().ok_or("manifest file SHA must be a string")?,
            64,
            "manifest file SHA256",
        )?;
    }
    for selected in &p.source_files {
        if files.get(&selected.relative_path).and_then(Value::as_str)
            != Some(selected.sha256.as_str())
        {
            return Err("selected constants pin differs from manifest files SHA string".into());
        }
    }
    p.source_files
        .iter()
        .map(|selected| {
            let path = manifest
                .parent
                .join(&selected.relative_path)
                .canonicalize()
                .map_err(|e| e.to_string())?;
            if !path.starts_with(&manifest.parent) {
                return Err("selected constants path escapes supplied snapshot directory".into());
            }
            Ok((
                selected.file.clone(),
                read_pinned(&path, &selected.sha256, "selected legacy constants")?,
            ))
        })
        .collect()
}

// Typed values are used only to discover known numeric object-leaf paths and validate shape.
// No serialized DTO value (including defaulted absent leaves) enters output or normalization.
fn numeric_fields(file: &str, raw: &Value) -> Result<BTreeSet<String>, String> {
    let shape = match file {
        "game_constants.json" => serde_json::to_value(
            serde_json::from_value::<GameConstantsDef>(raw.clone()).map_err(|e| e.to_string())?,
        ),
        "character_constants.json" => serde_json::to_value(
            serde_json::from_value::<CharacterConstantsDef>(raw.clone())
                .map_err(|e| e.to_string())?,
        ),
        "jewel_radii.json" => serde_json::to_value(
            serde_json::from_value::<JewelRadiiDef>(raw.clone()).map_err(|e| e.to_string())?,
        ),
        _ => return Err("unknown constants DTO file".into()),
    }
    .map_err(|e| format!("{file} DTO: {e}"))?;
    fn visit(value: &Value, path: &str, out: &mut BTreeSet<String>) {
        if value.is_number() {
            out.insert(path.to_owned());
        } else if let Some(object) = value.as_object() {
            for (key, v) in object {
                visit(
                    v,
                    &format!("{path}/{}", key.replace('~', "~0").replace('/', "~1")),
                    out,
                );
            }
        }
        // Array elements are retained data, not named numeric DTO object leaves.
    }
    let mut out = BTreeSet::new();
    visit(&shape, "", &mut out);
    Ok(out)
}
fn leaf_parent<'a>(
    document: &'a mut Value,
    pointer: &str,
) -> Result<(&'a mut serde_json::Map<String, Value>, String), String> {
    let (parent, leaf) = pointer
        .rsplit_once('/')
        .ok_or("invalid numeric leaf pointer")?;
    let key = leaf.replace("~1", "/").replace("~0", "~");
    let object = document
        .pointer_mut(parent)
        .and_then(Value::as_object_mut)
        .ok_or("numeric destination requires an existing object parent")?;
    Ok((object, key))
}
fn integer(v: &Value, field: &str) -> Result<i32, String> {
    v.as_i64()
        .and_then(|v| i32::try_from(v).ok())
        .ok_or_else(|| format!("GameConstants.{field}: exact i32 required"))
}
fn normalize(value: i32, divisor: i32, additional: &[f64]) -> Result<(f64, Vec<Value>), String> {
    if divisor == 0 {
        return Err("GameConstants.Divisor must be nonzero for a consumed binding".into());
    }
    let mut out = f64::from(value) / f64::from(divisor);
    let mut steps = vec![
        json!({"operation":"divide","input":value,"divisor":divisor,"output":out,"origin":"raw Value / raw Divisor"}),
    ];
    for (position, &divisor) in additional.iter().enumerate() {
        if !divisor.is_finite() || divisor == 0.0 {
            return Err("additional divisor must be finite and nonzero".into());
        }
        let before = out;
        out /= divisor;
        if !out.is_finite() {
            return Err("nonfinite normalized constant".into());
        }
        steps.push(json!({"operation":"divide","input":before,"divisor":divisor,"output":out,"additional_position":position}));
    }
    if !out.is_finite() {
        return Err("nonfinite normalized constant".into());
    }
    Ok((out, steps))
}
