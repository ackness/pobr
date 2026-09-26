//! Bind raw table bytes to the exporter-recorded patch before any output is written.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path};

use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
struct ExportSource {
    schema: String,
    patch: String,
    files: BTreeMap<String, String>,
}

pub(crate) fn verify(raw: &Path, patch: &str) -> Result<(), String> {
    let receipt_path = raw.join("export-source.json");
    let receipt: ExportSource = crate::read_json(&receipt_path)?;
    if receipt.schema != "official-raw-export/v1" || receipt.patch != patch {
        return Err(format!(
            "{}: raw export source does not match requested patch {patch}",
            receipt_path.display()
        ));
    }
    if receipt.files.is_empty() {
        return Err("raw export source contains no table fingerprints".into());
    }
    for (name, expected) in &receipt.files {
        let path = Path::new(name);
        if path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err(format!("invalid raw source path: {name}"));
        }
        let bytes = fs::read(raw.join(path)).map_err(|e| format!("raw source {name}: {e}"))?;
        if format!("{:x}", Sha256::digest(&bytes)) != *expected {
            return Err(format!("raw source fingerprint mismatch: {name}"));
        }
    }
    // An extra stale table must not participate through an optional loader.
    for language in ["English", crate::ZH_TW] {
        let entries =
            fs::read_dir(raw.join(language)).map_err(|e| format!("raw source {language}: {e}"))?;
        for entry in entries {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                let name = format!("{language}/{}", path.file_name().unwrap().to_string_lossy());
                if !receipt.files.contains_key(&name) {
                    return Err(format!(
                        "table is not bound to the raw export source: {name}"
                    ));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_wrong_patch_changed_bytes_and_unrecorded_tables() {
        let root = std::env::temp_dir().join(format!("pobr-raw-source-{}", std::process::id()));
        fs::create_dir_all(root.join("English")).unwrap();
        fs::create_dir_all(root.join(crate::ZH_TW)).unwrap();
        let table = root.join("English/Stats.json");
        fs::write(&table, "[]").unwrap();
        let document = serde_json::json!({
            "schema": "official-raw-export/v1", "patch": "9.1",
            "files": {"English/Stats.json": format!("{:x}", Sha256::digest(b"[]"))}
        });
        fs::write(root.join("export-source.json"), document.to_string()).unwrap();
        verify(&root, "9.1").unwrap();
        assert!(verify(&root, "9.2").unwrap_err().contains("patch"));
        fs::write(&table, "[{}]").unwrap();
        assert!(verify(&root, "9.1").unwrap_err().contains("fingerprint"));
        fs::write(&table, "[]").unwrap();
        fs::write(root.join("English/Unexpected.json"), "[]").unwrap();
        assert!(verify(&root, "9.1").unwrap_err().contains("not bound"));
        fs::remove_dir_all(root).unwrap();
    }
}
