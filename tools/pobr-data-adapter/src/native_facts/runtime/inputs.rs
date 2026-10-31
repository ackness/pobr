//! Bounded, externally pinned supplemental inputs. No formatter is executed.

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{Read, Take};
use std::path::{Component, Path};

use serde_json::Value;
use sha2::{Digest, Sha256};

use super::super::trust::{exact_keys, require, strict_json};
use crate::raw_source::sha256_hex;

pub(super) struct Descriptions {
    pub manifest: Value,
}

pub(super) fn pinned_json(path: &Path, expected: &str, cap: u64) -> Result<Value, String> {
    strict_json(&pinned_bytes(path, expected, cap)?)
}

pub(super) fn pinned_bytes(path: &Path, expected: &str, cap: u64) -> Result<Vec<u8>, String> {
    check_sha(expected)?;
    let mut bytes = Vec::new();
    open(path, cap)?
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    require(bytes.len() as u64 <= cap, "supplement exceeds byte limit")?;
    require(sha256_hex(&bytes) == expected, "supplement sha256 mismatch")?;
    Ok(bytes)
}

pub(super) fn descriptions(root: &Path, expected: &str) -> Result<Descriptions, String> {
    let manifest = pinned_json(&root.join("manifest.json"), expected, 1024 * 1024)?;
    require(
        manifest["format"] == "repoe-native-descriptions/v1"
            && manifest["production_ready"] == false
            && manifest["status"] == "partial-candidate",
        "unsupported descriptions candidate",
    )?;
    let identity = &manifest["source_identity"];
    require(
        identity["game"] == "poe2"
            && identity["realm"] == "international"
            && identity["acquisition"]["status"] == "unreviewed-acquisition-draft",
        "unsupported descriptions source identity/status",
    )?;
    exact_keys(
        &manifest["files"],
        &[
            "descriptions.json".into(),
            "diagnostics.json".into(),
            "source-lock.json".into(),
        ],
    )?;
    let mut inventory = BTreeSet::new();
    for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        require(
            entry.file_type().map_err(|e| e.to_string())?.is_file(),
            "descriptions package must be flat regular files",
        )?;
        inventory.insert(
            entry
                .file_name()
                .to_str()
                .ok_or("non-UTF8 descriptions filename")?
                .to_owned(),
        );
    }
    require(
        inventory
            == BTreeSet::from([
                "manifest.json".into(),
                "descriptions.json".into(),
                "diagnostics.json".into(),
                "source-lock.json".into(),
            ]),
        "unexpected descriptions inventory",
    )?;
    let mut total = 0u64;
    for (name, fp) in manifest["files"]
        .as_object()
        .ok_or("invalid descriptions files")?
    {
        exact_keys(fp, &["bytes".into(), "sha256".into()])?;
        let expected_bytes = fp["bytes"].as_u64().ok_or("invalid descriptions size")?;
        total = total
            .checked_add(expected_bytes)
            .ok_or("descriptions size overflow")?;
        require(
            total <= 256 * 1024 * 1024,
            "descriptions package exceeds byte limit",
        )?;
        let expected_sha = fp["sha256"].as_str().ok_or("invalid descriptions hash")?;
        check_sha(expected_sha)?;
        // Stream the large branch archive. It is pinned here but deliberately not
        // parsed as an effective template or numerical interpretation.
        let mut source = open(&root.join(name), expected_bytes)?;
        let mut hash = Sha256::new();
        let mut count = 0u64;
        let mut buffer = [0u8; 65536];
        loop {
            let n = source.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            count += n as u64;
            require(
                count <= expected_bytes,
                "descriptions payload exceeds declared bytes",
            )?;
            hash.update(&buffer[..n]);
        }
        let actual = hash
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        require(
            count == expected_bytes && actual == expected_sha,
            &format!("descriptions payload fingerprint mismatch: {name}"),
        )?;
    }
    let lock_sha = identity["source_lock_sha256"]
        .as_str()
        .ok_or("missing descriptions source lock hash")?;
    require(
        manifest["files"]["source-lock.json"]["sha256"] == lock_sha,
        "descriptions lock identity mismatch",
    )?;
    let lock = pinned_json(&root.join("source-lock.json"), lock_sha, 1024 * 1024)?;
    let key = identity["source"]
        .as_str()
        .ok_or("missing descriptions source key")?;
    let selected = &lock["sources"][key];
    require(
        lock["schema_version"] == 1 && selected.is_object(),
        "invalid descriptions source lock",
    )?;
    require(
        selected["kind"] == "official-client-resource-candidate",
        "unsupported descriptions source kind",
    )?;
    require(
        identity["selected_source_sha256"] == sha256_hex(&super::json_bytes(selected)?),
        "descriptions selected source mismatch",
    )?;
    for name in ["game", "game_patch", "realm", "acquisition"] {
        require(
            identity[name] == selected[name],
            &format!("descriptions lock {name} mismatch"),
        )?;
    }
    let raw = identity["raw_resources"]
        .as_object()
        .ok_or("missing descriptions raw identities")?;
    let resources = selected["resources"]
        .as_object()
        .ok_or("missing descriptions raw lock")?;
    require(
        !raw.is_empty() && raw.keys().eq(resources.keys()),
        "descriptions raw inventory mismatch",
    )?;
    for (name, fp) in raw {
        for field in ["bytes", "sha256"] {
            require(
                fp[field] == resources[name][field],
                "descriptions raw fingerprint/lock mismatch",
            )?;
        }
    }
    Ok(Descriptions { manifest })
}

fn check_sha(value: &str) -> Result<(), String> {
    require(
        value.len() == 64
            && value
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
        "invalid supplemental sha256",
    )
}

fn open(path: &Path, cap: u64) -> Result<Take<File>, String> {
    let absolute = std::path::absolute(path).map_err(|e| e.to_string())?;
    let mut current = std::path::PathBuf::new();
    for component in absolute.components() {
        require(
            matches!(component, Component::RootDir | Component::Normal(_)),
            "supplement traversal is forbidden",
        )?;
        current.push(component.as_os_str());
        require(
            !fs::symlink_metadata(&current)
                .map_err(|e| e.to_string())?
                .is_symlink(),
            "supplement symlinks are forbidden",
        )?;
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    let file = File::from(
        rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )
        .map_err(|e| format!("{}: {e}", path.display()))?,
    );
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    require(
        metadata.is_file() && metadata.len() <= cap,
        "supplement must be a bounded regular file",
    )?;
    Ok(file.take(cap.checked_add(1).ok_or("supplement size overflow")?))
}

pub(super) fn rows<'a>(
    input: &'a super::super::trust::VerifiedInput,
    table: &str,
) -> Result<&'a [Value], String> {
    input
        .documents
        .get(&format!("tables/{table}.json"))
        .and_then(|doc| doc["rows"].as_array())
        .map(Vec::as_slice)
        .ok_or_else(|| format!("missing table {table}"))
}

pub(super) fn row<'a>(
    input: &'a super::super::trust::VerifiedInput,
    table: &str,
    index: usize,
) -> Result<&'a Value, String> {
    rows(input, table)?
        .get(index)
        .map(|row| &row["values"])
        .ok_or_else(|| format!("missing row {table}/{index}"))
}

pub(super) fn find<'a>(
    input: &'a super::super::trust::VerifiedInput,
    table: &str,
    id: &str,
) -> Result<(usize, &'a Value), String> {
    rows(input, table)?
        .iter()
        .enumerate()
        .find(|(_, row)| row["values"]["Id"] == id)
        .map(|(i, row)| (i, &row["values"]))
        .ok_or_else(|| format!("missing {table} id {id}"))
}

pub(super) fn index(value: &Value) -> Result<usize, String> {
    value
        .as_u64()
        .and_then(|v| usize::try_from(v).ok())
        .ok_or_else(|| format!("expected physical row index, got {value}"))
}

pub(super) fn text(value: &Value) -> Result<String, String> {
    value
        .as_str()
        .map(String::from)
        .ok_or_else(|| format!("expected string, got {value}"))
}

pub(super) fn ids(
    input: &super::super::trust::VerifiedInput,
    table: &str,
    values: &Value,
) -> Result<Vec<String>, String> {
    values
        .as_array()
        .ok_or("expected FK array")?
        .iter()
        .map(|v| text(&row(input, table, index(v)?)?["Id"]))
        .collect()
}
