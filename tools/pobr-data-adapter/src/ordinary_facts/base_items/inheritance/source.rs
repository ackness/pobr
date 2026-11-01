//! Authenticate the frozen literal archive and its separate tag interpretation.
use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use serde_json::{Map, Value, json};

use super::{FORMAT, POLICY};
use crate::native_facts::reader;
use crate::ordinary_facts::BranchCodecArgs;
use crate::ordinary_facts::stat_set_codec::read_pinned;
use crate::raw_source::sha256_hex;

pub(super) struct Archive {
    pub manifest: Value,
    pub document: Value,
    pub sidecars: BTreeMap<String, Vec<u8>>,
}

pub(super) fn open(args: &BranchCodecArgs, parent: &Value) -> Result<Archive, String> {
    let manifest = read_pinned(
        &args.path.join("manifest.json"),
        &args.sha256,
        "item inheritance manifest",
    )?;
    let m = &manifest.document;
    let producer = reader::strict_json(include_bytes!(
        "../../../../tests/fixtures/ordinary-facts/item-inheritance-producer.json"
    ))?;
    if m["format"] != FORMAT
        || m["status"] != "partial-candidate"
        || m["production_ready"] != false
        || m["producer"] != producer
        || m["representation"]
            != json!({"payload":"inheritance.json","format":FORMAT,"tag_policy_id":POLICY})
    {
        return Err("unsupported item inheritance manifest/producer/representation".into());
    }
    let inventory: BTreeSet<_> = fs::read_dir(&args.path)
        .map_err(|e| e.to_string())?
        .map(|entry| {
            let entry = entry.map_err(|e| e.to_string())?;
            if !entry.file_type().map_err(|e| e.to_string())?.is_file() {
                return Err("item inheritance package must contain regular files only".to_owned());
            }
            entry
                .file_name()
                .into_string()
                .map_err(|_| "invalid inheritance filename".into())
        })
        .collect::<Result<_, String>>()?;
    if inventory
        != ["manifest.json", "inheritance.json", "source-lock.json"]
            .into_iter()
            .map(String::from)
            .collect()
        || object(&m["files"], "manifest files")?
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>()
            != ["inheritance.json", "source-lock.json"]
    {
        return Err("unexpected item inheritance inventory".into());
    }
    let mut sidecars = BTreeMap::from([(
        "item-inheritance-manifest.json".into(),
        manifest.bytes.clone(),
    )]);
    let mut documents = BTreeMap::new();
    for name in ["inheritance.json", "source-lock.json"] {
        let fp = &m["files"][name];
        let pinned = read_pinned(&args.path.join(name), string(&fp["sha256"])?, name)?;
        if fp != &json!({"bytes":pinned.bytes.len(),"sha256":sha256_hex(&pinned.bytes)}) {
            return Err(format!(
                "inheritance payload size/fingerprint mismatch: {name}"
            ));
        }
        documents.insert(name, pinned.document);
        sidecars.insert(format!("item-{name}"), pinned.bytes);
    }
    let document = documents
        .remove("inheritance.json")
        .ok_or("missing inheritance payload")?;
    let lock = &documents["source-lock.json"];
    let identity = &m["source_identity"];
    if lock["schema_version"] != 1
        || identity["source_lock_sha256"] != m["files"]["source-lock.json"]["sha256"]
        || identity["kind"] != "official-client-resource-candidate"
        || identity["acquisition"]["status"] != "unreviewed-acquisition-draft"
    {
        return Err("invalid item inheritance source identity/lock".into());
    }
    let selected = &lock["sources"][string(&identity["source"])?];
    if identity["selected_source_sha256"] != sha256_hex(&reader::json_bytes(selected)?) {
        return Err("item inheritance selected source mismatch".into());
    }
    for key in ["kind", "game", "game_patch", "realm", "acquisition"] {
        if identity[key] != selected[key] {
            return Err(format!("item inheritance lock identity mismatch: {key}"));
        }
    }
    for key in ["game", "game_patch", "realm"] {
        if identity[key] != parent["source_identity"][key] {
            return Err(format!("item inheritance parent identity mismatch: {key}"));
        }
    }
    verify(&document, m, selected)?;
    Ok(Archive {
        manifest: manifest.document,
        document,
        sidecars,
    })
}

fn verify(doc: &Value, manifest: &Value, selected: &Value) -> Result<(), String> {
    let policy = &doc["interpretation_policy"];
    if doc["format"] != FORMAT
        || policy["id"] != POLICY
        || policy["authority"] != "community-parser-interpretation"
        || policy["official_merge_semantics_certified"] != false
        || policy["repository"] != "https://github.com/repoe-fork/pypoe"
        || policy["revision"] != "66d1d7442c5718260e1a8efd24989d22987e91af"
    {
        return Err("unsupported item tag interpretation".into());
    }
    let files = object(&doc["files"], "inheritance files")?;
    let raw = object(
        &manifest["source_identity"]["raw_resources"],
        "raw resources",
    )?;
    if files.is_empty() || !files.keys().eq(raw.keys()) || selected["resources"] != json!(raw) {
        return Err("item inheritance raw inventory/lock mismatch".into());
    }
    let mut fields = 0;
    let mut tags = 0;
    let mut bytes = 0;
    for (path, file) in files {
        resource(path)?;
        let actual = verify_file(file)?;
        if raw[path] != json!({"bytes":actual.len(),"sha256":sha256_hex(&actual)}) {
            return Err(format!("item metadata raw reconstruction mismatch: {path}"));
        }
        bytes += actual.len();
        let all = array(&file["fields"])?;
        fields += all.len();
        tags += all.iter().filter(|f| is_tag(f)).count();
    }
    let roots = object(&doc["roots"], "inheritance roots")?;
    let selected_roots: Vec<_> = array(&selected["roots"])?
        .iter()
        .map(string)
        .collect::<Result<_, _>>()?;
    let unique_roots: BTreeSet<_> = selected_roots.iter().copied().collect();
    if roots.is_empty()
        || unique_roots.len() != selected_roots.len()
        || unique_roots != roots.keys().map(String::as_str).collect()
    {
        return Err("item inheritance roots differ from source lock".into());
    }
    let mut reachable = BTreeSet::new();
    let mut rejected = 0;
    for (root, row) in roots {
        let mut chain = Vec::new();
        let mut seen = BTreeSet::new();
        let mut path = root.as_str();
        let mut occurrences = Vec::new();
        let mut union = Vec::new();
        let mut members = BTreeSet::new();
        let mut supported = true;
        loop {
            if !seen.insert(path) || chain.len() >= 13 {
                return Err("item inheritance cycle/depth exceeded".into());
            }
            reachable.insert(path);
            chain.push(path);
            let file = files.get(path).ok_or("missing item inheritance parent")?;
            if array(&file["blocks"])?
                .iter()
                .filter(|b| b["scope"] == json!(["Base"]))
                .count()
                > 1
            {
                supported = false;
            }
            for (ordinal, field) in array(&file["fields"])?
                .iter()
                .enumerate()
                .filter(|(_, f)| is_tag(f))
            {
                let mut occurrence = field.clone();
                occurrence["resource"] = json!(path);
                occurrence["field_ordinal"] = json!(ordinal);
                occurrences.push(occurrence);
                if let Some(tag) = field["quoted_literal"]
                    .as_str()
                    .filter(|tag| !tag.is_empty())
                {
                    if members.insert(tag) {
                        union.push(tag);
                    }
                } else {
                    supported = false;
                }
            }
            match file["extends"]["target"].as_str() {
                Some(parent) => path = parent,
                None if file["extends"]["literal"] == "nothing"
                    && file["extends"]["target"].is_null() =>
                {
                    break;
                }
                _ => return Err("invalid item inheritance terminal".into()),
            }
        }
        if row["chain_child_to_parent"] != json!(chain)
            || row["base_tag_occurrences"] != json!(occurrences)
        {
            return Err("item inheritance chain/tag occurrence mismatch".into());
        }
        let interpretation = &row["tag_interpretation"];
        if interpretation["policy_id"] != POLICY
            || interpretation["authority"] != "community-parser-interpretation"
            || interpretation["status"] != if supported { "supported" } else { "rejected" }
            || interpretation["tags"] != if supported { json!(union) } else { Value::Null }
            || array(&interpretation["diagnostics"])?.is_empty() != supported
        {
            return Err("item inherited tag interpretation disagrees with literal fields".into());
        }
        rejected += usize::from(!supported);
    }
    if reachable != files.keys().map(String::as_str).collect() {
        return Err("extra item metadata outside root closure".into());
    }
    if manifest["summary"]
        != json!({"files":files.len(),"roots":roots.len(),"fields":fields,
        "literal_base_tags":tags,"raw_bytes":bytes,"rejected_tag_roots":rejected})
    {
        return Err("item inheritance summary mismatch".into());
    }
    Ok(())
}

fn verify_file(file: &Value) -> Result<Vec<u8>, String> {
    let mut raw = vec![0xff, 0xfe];
    let lines = array(&file["lines"])?;
    for (ordinal, line) in lines.iter().enumerate() {
        let start = raw.len();
        raw.extend(
            string(&line["raw_text"])?
                .encode_utf16()
                .flat_map(u16::to_le_bytes),
        );
        if line["line"] != ordinal + 1 || line["byte_span"] != json!([start, raw.len()]) {
            return Err("item metadata line/span mismatch".into());
        }
    }
    let blocks = array(&file["blocks"])?;
    for (ordinal, block) in blocks.iter().enumerate() {
        if block["ordinal"] != ordinal
            || block["raw_slice_sha256"] != sha256_hex(slice(&raw, &block["byte_span"])?)
        {
            return Err("item metadata block/span mismatch".into());
        }
    }
    for (ordinal, field) in array(&file["fields"])?.iter().enumerate() {
        let line = field["line"]
            .as_u64()
            .and_then(|n| n.checked_sub(1))
            .and_then(|n| usize::try_from(n).ok())
            .and_then(|n| lines.get(n))
            .ok_or("invalid item field line")?;
        let block = field["block_ordinal"]
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .and_then(|n| blocks.get(n))
            .ok_or("invalid item field block")?;
        if field["ordinal"] != ordinal
            || field["raw_text"] != line["raw_text"]
            || field["byte_span"] != line["byte_span"]
            || field["scope"] != block["scope"]
            || !array(&block["field_ordinals"])?.contains(&json!(ordinal))
        {
            return Err("item metadata field/span/scope mismatch".into());
        }
        let (key, rhs) = string(&field["raw_text"])?
            .trim()
            .split_once('=')
            .ok_or("invalid item field assignment")?;
        if key.trim() != string(&field["key"])? || rhs.trim() != string(&field["rhs_literal"])? {
            return Err("item metadata field literal mismatch".into());
        }
        if let Some(quoted) = field.get("quoted_literal")
            && rhs
                .trim()
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                != quoted.as_str()
        {
            return Err("item metadata quoted literal mismatch".into());
        }
    }
    let edge = &file["extends"];
    let line = edge["line"]
        .as_u64()
        .and_then(|n| n.checked_sub(1))
        .and_then(|n| usize::try_from(n).ok())
        .and_then(|n| lines.get(n))
        .ok_or("invalid item extends line")?;
    let literal = string(&edge["literal"])?;
    let expected = if literal == "nothing" {
        Value::Null
    } else {
        let path = format!("{literal}.it");
        resource(&path)?;
        json!(path)
    };
    if edge["target"] != expected
        || edge["raw_text"] != line["raw_text"]
        || edge["byte_span"] != line["byte_span"]
        || string(&edge["raw_text"])?.trim() != format!("extends \"{literal}\"")
    {
        return Err("item metadata extends literal/span mismatch".into());
    }
    Ok(raw)
}

pub(super) fn resource(path: &str) -> Result<(), String> {
    if !path.starts_with("Metadata/Items/")
        || !path.ends_with(".it")
        || path.contains('\\')
        || path
            .split('/')
            .any(|s| s.is_empty() || s == "." || s == "..")
    {
        return Err("invalid item metadata resource".into());
    }
    Ok(())
}
fn is_tag(field: &Value) -> bool {
    field["scope"] == json!(["Base"]) && field["key"] == "tag"
}
fn array(value: &Value) -> Result<&[Value], String> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or("item inheritance array required".into())
}
fn object<'a>(value: &'a Value, name: &str) -> Result<&'a Map<String, Value>, String> {
    value
        .as_object()
        .ok_or_else(|| format!("{name}: object required"))
}
fn string(value: &Value) -> Result<&str, String> {
    value
        .as_str()
        .ok_or("item inheritance string required".into())
}
fn slice<'a>(raw: &'a [u8], span: &Value) -> Result<&'a [u8], String> {
    let span: [usize; 2] = serde_json::from_value(span.clone()).map_err(|e| e.to_string())?;
    raw.get(span[0]..span[1])
        .ok_or("invalid item metadata span".into())
}
