//! Opt-in RePoE facts to PoBR domain candidate conversion, without runtime publication.

mod convert;
mod model;
mod output;
mod trust;

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests;

#[cfg(all(test, not(any(target_os = "linux", target_os = "macos"))))]
#[test]
fn native_output_is_explicitly_unsupported() {
    let args = Args {
        input: "unused-input".into(),
        expected_manifest_sha256: "0".repeat(64),
        source_lock: "unused-lock".into(),
        out: "unused-output".into(),
    };
    assert_eq!(
        output::Output::prepare(&args).err().as_deref(),
        Some("native-facts output requires Linux or macOS directory handles")
    );
}

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::json;

use crate::raw_source::sha256_hex;

pub(crate) struct Args {
    input: PathBuf,
    expected_manifest_sha256: String,
    source_lock: PathBuf,
    out: PathBuf,
}

pub(crate) fn parse_args(args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut values = BTreeMap::new();
    let mut args = args;
    while let Some(flag) = args.next() {
        if ![
            "--native-facts",
            "--expected-manifest-sha256",
            "--source-lock",
            "--out",
        ]
        .contains(&flag.as_str())
        {
            return Err(format!("unsupported native-facts argument: {flag}"));
        }
        let value = args.next().ok_or_else(|| format!("{flag} needs a value"))?;
        if value.starts_with("--") || values.insert(flag.clone(), value).is_some() {
            return Err(format!("missing value or duplicate argument: {flag}"));
        }
    }
    let mut take = |flag: &str| values.remove(flag).ok_or_else(|| format!("missing {flag}"));
    Ok(Args {
        input: take("--native-facts")?.into(),
        expected_manifest_sha256: take("--expected-manifest-sha256")?,
        source_lock: take("--source-lock")?.into(),
        out: take("--out")?.into(),
    })
}

pub(crate) fn run(args: Args) -> Result<String, String> {
    trust::require_absent(&args.out)?;
    let mut output = output::Output::prepare(&args)?;
    let mut input = trust::verify(&args)?;
    let candidate = convert::normalize(&input)?;
    // Serialize everything before reserving output: input/normalization failures leave no directory.
    let files = BTreeMap::from([
        ("candidate.json", json_bytes(&candidate)?),
        ("source-manifest.json", input.manifest_bytes),
        ("source-lock.json", input.lock_bytes),
        (
            "source-diagnostics.json",
            input
                .bytes
                .remove("diagnostics.json")
                .ok_or("missing verified diagnostics")?,
        ),
    ]);
    let inventory: BTreeMap<_, _> = files
        .iter()
        .map(|(name, bytes)| {
            (
                *name,
                json!({"bytes": bytes.len(), "sha256": sha256_hex(bytes)}),
            )
        })
        .collect();
    let receipt = json!({
        "format": "pobr-native-candidate-receipt/v1",
        "status": "partial-candidate",
        "production_ready": false,
        "adapter": {"name": "pobr-data-adapter/native-facts", "version": env!("CARGO_PKG_VERSION"),
            "implementation_sha256": implementation_sha256()},
        "input": {"format": "repoe-native-facts/v1",
            "manifest_sha256": args.expected_manifest_sha256,
            "producer_revision": trust::PRODUCER_REVISION,
            "source_identity": input.manifest["source_identity"]},
        "summary": candidate.summary,
        "files": inventory,
        "scope": [
            "Partial INTERNATIONAL patch 0.5.5.5 domain candidate; never a runtime manifest.",
            "Raw physical metadata remains in the canonical RePoE package pinned by input.manifest_sha256; candidate input references use its payload paths and JSON pointers.",
            "No geometry/radius interpretation, labels, text, stat mapping or quality application policy.",
            "All source diagnostics retained verbatim; no PoB fallback."
        ]
    });
    let receipt_bytes = json_bytes(&receipt)?;
    let receipt_sha = sha256_hex(&receipt_bytes);
    output.reserve()?;
    for (name, bytes) in &files {
        output.write_new(name, bytes)?;
    }
    // The completion receipt is the last file. Interrupted output without it is incomplete.
    output.write_new("candidate-receipt.json", &receipt_bytes)?;
    output.verify_namespace()?;
    Ok(format!(
        "partial native candidate: {}; receipt sha256 {receipt_sha}",
        args.out.display()
    ))
}

fn implementation_sha256() -> String {
    let mut bytes = Vec::new();
    for (name, source) in [
        ("mod.rs", include_bytes!("mod.rs").as_slice()),
        ("trust.rs", include_bytes!("trust.rs").as_slice()),
        ("model.rs", include_bytes!("model.rs").as_slice()),
        ("convert.rs", include_bytes!("convert.rs").as_slice()),
        ("output.rs", include_bytes!("output.rs").as_slice()),
        (
            "raw_source.rs",
            include_bytes!("../raw_source.rs").as_slice(),
        ),
        ("Cargo.toml", include_bytes!("../../Cargo.toml").as_slice()),
        (
            "Cargo.lock",
            include_bytes!("../../../../Cargo.lock").as_slice(),
        ),
        ("pypoe-sources.json", trust::DECODER_PIN),
    ] {
        bytes.extend_from_slice(name.as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(source);
        bytes.push(0);
    }
    sha256_hex(&bytes)
}

fn json_bytes(value: &impl serde::Serialize) -> Result<Vec<u8>, String> {
    let mut bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    Ok(bytes)
}
