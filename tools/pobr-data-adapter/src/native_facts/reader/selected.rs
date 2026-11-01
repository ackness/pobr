//! Pinned supplemental physical tables; never a full native runtime profile.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Value, json};

use super::super::trust;
use super::Tables;
use crate::raw_source::sha256_hex;

pub(crate) struct Input {
    pub manifest: Value,
    pub manifest_bytes: Vec<u8>,
    pub documents: BTreeMap<String, Value>,
}

impl Input {
    pub fn tables(&self, names: &[&'static str]) -> Result<Tables<'_>, String> {
        Tables::selected_documents(&self.documents, names)
    }

    /// Check a consumed physical column without fabricating a full fact envelope.
    pub fn column(&self, table: &str, field: &str, schema: &Value) -> Result<(), String> {
        let columns = self.documents[&format!("tables/{table}.json")]["columns"]
            .as_array()
            .ok_or("missing supplement columns")?;
        let column = columns
            .iter()
            .find(|c| c["physical_fields"] == json!([field]))
            .ok_or("missing supplement column")?;
        for (key, expected) in schema.as_object().ok_or("invalid column contract")? {
            trust::require(
                column["schema"][key] == *expected,
                &format!("supplement column schema mismatch: {table}/{field}/{key}"),
            )?;
        }
        Ok(())
    }
}

pub(crate) fn open(path: &Path, pin: &str, parent: &Value) -> Result<Input, String> {
    let manifest_bytes = trust::read_regular(&path.join("manifest.json"))?;
    trust::require(
        sha256_hex(&manifest_bytes) == pin,
        "supplement manifest pin mismatch",
    )?;
    let manifest = trust::strict_json(&manifest_bytes)?;
    trust::require(
        manifest["format"] == "repoe-native-tables/v1"
            && manifest["status"] == "partial-candidate"
            && manifest["production_ready"] == false,
        "invalid selected-table envelope",
    )?;
    let producer = trust::strict_json(include_bytes!(
        "../../../tests/fixtures/ordinary-facts/selected-tables-producer.json"
    ))?;
    let localized_capable_producer = trust::strict_json(include_bytes!(
        "../../../tests/fixtures/ordinary-facts/selected-tables-producer-21ace02.json"
    ))?;
    trust::require(
        manifest["producer"] == producer || manifest["producer"] == localized_capable_producer,
        "supplement producer identity mismatch",
    )?;
    trust::require(
        manifest["schema"] == parent["schema"],
        "supplement schema mismatch",
    )?;
    trust::verify_decoder(&manifest["decoder"])?;
    let identity = &manifest["source_identity"];
    for key in ["kind", "game", "game_patch", "realm"] {
        trust::require(
            identity[key] == parent["source_identity"][key],
            "supplement source identity mismatch",
        )?;
    }
    trust::exact_keys(
        &manifest["files"],
        &["source-lock.json".into(), "tables.json".into()],
    )?;
    let mut raw = BTreeMap::new();
    for name in ["source-lock.json", "tables.json"] {
        let bytes = trust::read_regular(&path.join(name))?;
        trust::require(
            manifest["files"][name] == json!({"bytes":bytes.len(),"sha256":sha256_hex(&bytes)}),
            "supplement payload fingerprint mismatch",
        )?;
        raw.insert(name, bytes);
    }
    let lock = trust::strict_json(&raw["source-lock.json"])?;
    trust::require(
        lock["schema_version"] == 1
            && identity["source_lock_sha256"] == sha256_hex(&raw["source-lock.json"]),
        "supplement source lock mismatch",
    )?;
    let name = identity["source"]
        .as_str()
        .ok_or("missing supplement source name")?;
    let source = &lock["sources"][name];
    trust::require(
        identity["selected_source_sha256"] == sha256_hex(&super::json_bytes(source)?),
        "supplement selected-source mismatch",
    )?;
    for key in ["kind", "game", "game_patch", "realm", "acquisition"] {
        trust::require(
            source[key] == identity[key],
            "supplement selected source declaration mismatch",
        )?;
    }
    trust::verify_acquisition(&identity["acquisition"])?;
    for (key, value) in [
        ("repository", "https://github.com/repoe-fork/pypoe"),
        ("revision", trust::PYPOE),
        (
            "dat_schema_repository",
            "https://github.com/poe-tool-dev/dat-schema",
        ),
        ("dat_schema_revision", trust::SCHEMA_REVISION),
        ("schema_sha256", trust::SCHEMA_SHA),
    ] {
        trust::require(
            source["decoder"][key] == value,
            "supplement declared decoder mismatch",
        )?;
    }
    let tables = trust::strict_json(&raw["tables.json"])?;
    let tables = tables.as_object().ok_or("invalid supplement tables")?;
    let paths: Vec<_> = tables
        .keys()
        .map(|name| format!("Data/Balance/{name}.datc64"))
        .collect();
    trust::exact_keys(&source["resources"], &paths)?;
    trust::exact_keys(&identity["raw_resources"], &paths)?;
    let mut documents = BTreeMap::new();
    for (name, doc) in tables {
        let path = format!("Data/Balance/{name}.datc64");
        let pin = &identity["raw_resources"][&path];
        trust::require(
            source["resources"][&path]["bytes"] == pin["bytes"]
                && source["resources"][&path]["sha256"] == pin["sha256"]
                && pin["bytes"].as_u64().is_some_and(|n| n > 0)
                && pin["sha256"].as_str().is_some_and(|h| {
                    h.len() == 64
                        && h.bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                }),
            "supplement raw identity mismatch",
        )?;
        trust::require(
            doc["table"] == *name
                && doc["raw_resource"] == path
                && doc["rows"]
                    .as_array()
                    .is_some_and(|rows| manifest["summary"]["tables"][name] == rows.len()),
            "supplement table identity/summary mismatch",
        )?;
        documents.insert(format!("tables/{name}.json"), doc.clone());
    }
    Ok(Input {
        manifest,
        manifest_bytes,
        documents,
    })
}
