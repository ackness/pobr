use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use pobr_data::catalog::{ModDef, StatDef};
use serde_json::{Value, json};

use super::{Args, parse_args, reader, run, sha256_hex};

static NEXT: AtomicU64 = AtomicU64::new(0);

pub(super) struct Fixture {
    pub(super) root: PathBuf,
    pub(super) manifest: Value,
    pub(super) documents: BTreeMap<String, Value>,
}

impl Fixture {
    pub(super) fn new() -> Self {
        // Reuse the established v4 trust fixture. Only selected payloads exist on disk;
        // this proves the ordinary path does not read unrelated skills/actors/graph data.
        let bundle: Value = serde_json::from_slice(include_bytes!(
            "../../tests/fixtures/native-facts/items-actors-bc-synthetic.json"
        ))
        .unwrap();
        let tables: BTreeMap<String, Value> = serde_json::from_slice(include_bytes!(
            "../../tests/fixtures/ordinary-facts/physical-tables.json"
        ))
        .unwrap();
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.cache/ordinary-facts-tests")
            .join(format!(
                "{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(root.join("input/tables")).unwrap();
        // The verifier rejects traversal components, even in otherwise valid paths.
        let root = root.canonicalize().unwrap();
        let mut documents: BTreeMap<_, _> = tables
            .into_iter()
            .map(|(table, doc)| (format!("tables/{table}.json"), doc))
            .collect();
        documents.insert(
            "source-lock.json".into(),
            bundle["documents"]["source-lock.json"].clone(),
        );
        let mut result = Self {
            root,
            manifest: bundle["manifest"].clone(),
            documents,
        };
        result.seal();
        result
    }

    pub(super) fn seal(&mut self) {
        for (path, doc) in &self.documents {
            let bytes = reader::json_bytes(doc).unwrap();
            fs::write(self.root.join("input").join(path), &bytes).unwrap();
            self.manifest["files"][path] =
                json!({"bytes":bytes.len(), "sha256":sha256_hex(&bytes)});
        }
        let lock = &self.documents["source-lock.json"];
        self.manifest["source_identity"]["source_lock_sha256"] =
            sha256_hex(&reader::json_bytes(lock).unwrap()).into();
        fs::write(
            self.root.join("input/manifest.json"),
            reader::json_bytes(&self.manifest).unwrap(),
        )
        .unwrap();
    }

    pub(super) fn args(&self) -> Args {
        Args {
            input: self.root.join("input"),
            expected_manifest_sha256: sha256_hex(
                &fs::read(self.root.join("input/manifest.json")).unwrap(),
            ),
            version: "0.5.5.5".into(),
            domain: super::Domain::StatsMods,
            quality_policy: None,
            codec: None,
            out: self.root.join("out"),
        }
    }

    pub(super) fn values(&mut self, table: &str, row: usize) -> &mut Value {
        &mut self
            .documents
            .get_mut(&format!("tables/{table}.json"))
            .unwrap()["rows"][row]["values"]
    }

    fn rejects(&self, expected: &str) {
        let error = run(self.args()).unwrap_err();
        assert!(
            error.contains(expected),
            "expected {expected:?}, got {error:?}"
        );
        assert!(!self.root.join("out").exists(), "bad source created output");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

#[test]
fn ordinary_dtos_preserve_eight_slots_ranges_families_and_raw_metadata() {
    let f = Fixture::new();
    run(f.args()).unwrap();
    let out = f.root.join("out");
    let stats: Vec<StatDef> = crate::read_json(&out.join("base/stats.json")).unwrap();
    let mods: Vec<ModDef> = crate::read_json(&out.join("base/mods.json")).unwrap();
    assert_eq!(stats.len(), 8);
    assert_eq!(mods.len(), 3);
    assert_eq!(stats[0].id, "a_negative"); // Output sorting is unrelated to physical row 0.
    assert!(stats[0].is_local);
    assert_eq!(stats[0].category, Some(0));
    assert_eq!(stats[0].semantic, None);
    let zero = stats.iter().find(|s| s.id == "z_zero").unwrap();
    assert_eq!((zero.semantic, zero.category), (Some(0), None));
    let m = mods.iter().find(|m| m.id == "EightSlots").unwrap();
    assert_eq!(m.name.as_deref(), Some("Synthetic"));
    assert_eq!(
        (m.mod_type, m.domain, m.generation_type, m.level),
        (Some(1), 0, Some(0), 0)
    );
    assert_eq!(m.group.as_deref(), Some("TierGroup"));
    assert_eq!(m.families.as_ref().unwrap(), &["family-a", "family-z"]);
    assert_eq!(m.tags, ["ring", "default"]);
    assert_eq!(
        m.spawn_weights
            .iter()
            .map(|w| (w.tag.as_str(), w.weight))
            .collect::<Vec<_>>(),
        [("ring", 0), ("default", 7)]
    );
    let slots: Vec<_> = m
        .stats
        .iter()
        .map(|s| (s.stat_id.as_str(), s.min, s.max))
        .collect();
    assert_eq!(
        slots,
        [
            ("z_zero", 0, 0),
            ("a_negative", -12, -3),
            ("m_crossing", -4, 6),
            ("b_fourth", 2, 3),
            ("y_fifth", 5, 8),
            ("c_sixth", 25, 34),
            ("x_seventh", -9, -9),
            ("d_eighth", 0, 24),
        ]
    );
    let unknown = mods.iter().find(|m| m.id == "UnknownFamily").unwrap();
    assert_eq!(unknown.families, None);
    assert_eq!(unknown.name, None);
    assert_eq!(
        (
            unknown.mod_type,
            unknown.group.as_deref(),
            unknown.generation_type
        ),
        (None, None, None)
    );
    assert_eq!(
        unknown.stats.len(),
        1,
        "null Stat8 must not become Stats row zero"
    );
    assert_eq!(
        mods.iter()
            .find(|m| m.id == "EmptyFamily")
            .unwrap()
            .families,
        Some(vec![])
    );
    let receipt: Value = crate::read_json(&out.join("producer-receipt.json")).unwrap();
    assert_eq!(
        receipt["conversion"]["filtered_shells"],
        json!([{"id":"Shell","source_row":3}])
    );
    assert!(
        receipt["conversion"]["unassigned_stat_ranges"]
            .as_array()
            .unwrap()
            .contains(&json!({"id":"UnknownFamily","source_row":1,"slot":8,"min":-7,"max":9}))
    );
    assert_eq!(receipt["complete_runtime"], false);
    assert_eq!(receipt["adoption"], "not-adopted");
    assert_eq!(
        receipt["input"]["source_identity"],
        f.manifest["source_identity"]
    );
    assert_eq!(
        receipt["input"]["manifest_sha256"],
        f.args().expected_manifest_sha256
    );
    assert_eq!(
        receipt["input"]["selected_payloads"]
            .as_object()
            .unwrap()
            .len(),
        6
    );
    assert_eq!(
        receipt["adapter"]["implementation_revision"]
            .as_str()
            .unwrap()
            .len(),
        71
    );
    for (path, fingerprint) in receipt["files"].as_object().unwrap() {
        let bytes = fs::read(out.join(path)).unwrap();
        assert_eq!(fingerprint["sha256"], sha256_hex(&bytes), "{path}");
        assert_eq!(fingerprint["bytes"], bytes.len(), "{path}");
    }
    assert!(out.join("_domain_candidate.json").is_file());
    assert!(!out.join("manifest.json").exists());
    let second = f.root.join("out-again");
    let mut args = f.args();
    args.out = second.clone();
    run(args).unwrap();
    for path in ["base/stats.json", "base/mods.json", "producer-receipt.json"] {
        assert_eq!(
            fs::read(out.join(path)).unwrap(),
            fs::read(second.join(path)).unwrap(),
            "{path}"
        );
    }
}

#[test]
fn expected_manifest_payload_lock_and_target_are_enforced() {
    let f = Fixture::new();
    let mut args = f.args();
    args.expected_manifest_sha256 = "0".repeat(64);
    assert!(
        run(args)
            .unwrap_err()
            .contains("external manifest sha256 mismatch")
    );
    let mut args = f.args();
    args.version = "0.5.5.4.2".into();
    assert!(run(args).unwrap_err().contains("target version"));
    fs::write(f.root.join("input/tables/Stats.json"), b"{}").unwrap();
    f.rejects("payload fingerprint mismatch");
    let f = Fixture::new();
    fs::write(f.root.join("input/source-lock.json"), b"{}").unwrap();
    f.rejects("source-lock bytes mismatch");
}

#[test]
fn rejects_unresolved_references_in_every_converted_relationship() {
    for (field, value, expected) in [
        ("Stat8", json!(99), "Stats/99"),
        ("ModType", json!(99), "ModType/99"),
        ("Families", json!([1, 99]), "ModFamily/99"),
        ("Tags", json!([99]), "Tags/99"),
        ("SpawnWeight_Tags", json!([1, 99]), "Tags/99"),
        ("Stat1", json!(-1), "physical reference"),
        ("Families", json!([null]), "physical reference"),
    ] {
        let mut f = Fixture::new();
        f.values("Mods", 0)[field] = value;
        f.seal();
        f.rejects(expected);
    }
    let mut f = Fixture::new();
    f.values("Mods", 3)["Families"] = json!([99]);
    f.seal();
    f.rejects("ModFamily/99"); // Filtering shells must not hide broken references.
}

#[test]
fn rejects_bad_ranges_null_required_values_overflow_and_weight_truncation() {
    for (field, value, expected) in [
        ("Stat8Min", json!(25), "bad range"),
        ("Stat8Max", Value::Null, "Stat8Max must be i64"),
        ("Stat8Min", json!(0.5), "Stat8Min must be i64"),
        ("Level", json!(-1), "Level"),
        ("Level", json!(4294967296_u64), "Level"),
        ("Domain", Value::Null, "Domain"),
        ("GenerationType", json!(-1), "GenerationType"),
        ("SpawnWeight_Values", json!([0]), "lengths differ"),
        ("SpawnWeight_Values", json!([-1, 7]), "SpawnWeight"),
        ("Tags", Value::Null, "expected array"),
    ] {
        let mut f = Fixture::new();
        f.values("Mods", 0)[field] = value;
        f.seal();
        f.rejects(expected);
    }
    let mut f = Fixture::new();
    f.values("Stats", 0)["IsLocal"] = Value::Null;
    f.seal();
    f.rejects("IsLocal must be boolean");
}

#[test]
fn rejects_duplicate_ids_rows_and_column_contract_drift() {
    for (table, key) in [
        ("Stats", "Id"),
        ("Mods", "Id"),
        ("ModFamily", "Id"),
        ("Tags", "Id"),
        ("ModType", "Name"),
    ] {
        let mut f = Fixture::new();
        let duplicate = f.values(table, 0)[key].clone();
        f.values(table, 1)[key] = duplicate;
        f.seal();
        f.rejects("duplicate");
    }
    let mut f = Fixture::new();
    f.documents.get_mut("tables/Stats.json").unwrap()["rows"][1]["_row"] = json!(0);
    f.seal();
    f.rejects("physical row order mismatch");
    let mut f = Fixture::new();
    let doc = f.documents.get_mut("tables/Mods.json").unwrap();
    let column = doc["columns"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|c| c["schema"]["name"] == "Families")
        .unwrap();
    column["schema"]["references"]["table"] = json!("ModType");
    f.seal();
    f.rejects("source schema references mismatch");
    let mut f = Fixture::new();
    let doc = f.documents.get_mut("tables/Mods.json").unwrap();
    let index = doc["columns"].as_array().unwrap().len();
    doc["columns"]
        .as_array_mut()
        .unwrap()
        .push(json!({"source_column":index,"physical_fields":["Stat9"],"schema":{}}));
    doc["physical_fields"]
        .as_array_mut()
        .unwrap()
        .push(json!("Stat9"));
    for row in doc["rows"].as_array_mut().unwrap() {
        row["values"]["Stat9"] = Value::Null;
    }
    f.seal();
    f.rejects("unhandled Mods stat-slot column: Stat9");
}

#[test]
fn rejects_duplicate_json_keys_even_with_a_matching_pin() {
    let mut f = Fixture::new();
    let path = "tables/Stats.json";
    let bytes = b"{\"rows\":[],\"rows\":[]}";
    fs::write(f.root.join("input").join(path), bytes).unwrap();
    f.manifest["files"][path] = json!({"bytes":bytes.len(),"sha256":sha256_hex(bytes)});
    fs::write(
        f.root.join("input/manifest.json"),
        reader::json_bytes(&f.manifest).unwrap(),
    )
    .unwrap();
    f.rejects("duplicate JSON key");
}

#[test]
fn refuses_existing_output_production_and_input_directories() {
    let f = Fixture::new();
    run(f.args()).unwrap();
    let before = fs::read(f.root.join("out/base/mods.json")).unwrap();
    assert!(
        run(f.args())
            .unwrap_err()
            .contains("refusing existing output")
    );
    assert_eq!(before, fs::read(f.root.join("out/base/mods.json")).unwrap());
    let mut args = f.args();
    args.out = args.input.join("fresh-output");
    assert!(run(args).unwrap_err().contains("canonical input package"));
    let mut args = f.args();
    args.out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../data")
        .canonicalize()
        .unwrap()
        .join(format!("ordinary-facts-forbidden-{}", std::process::id()));
    assert!(run(args).unwrap_err().contains("outside production data"));
}

#[test]
fn explicit_mode_requires_pin_and_keeps_legacy_raw_dispatch() {
    let flags = [
        "--ordinary-facts",
        "facts",
        "--facts-manifest-sha256",
        "pin",
        "--version",
        "0.5.5.5",
        "--out",
        "fresh",
    ];
    assert!(parse_args(flags.into_iter().map(String::from)).is_ok());
    for extra in [
        vec!["--raw", "legacy"],
        vec!["--native-runtime", "legacy"],
        vec!["--out", "again"],
    ] {
        assert!(parse_args(flags.into_iter().chain(extra).map(String::from)).is_err());
    }
    assert!(
        parse_args(
            [
                "--ordinary-facts",
                "facts",
                "--version",
                "0.5.5.5",
                "--out",
                "fresh"
            ]
            .into_iter()
            .map(String::from)
        )
        .is_err()
    );
    let mode = crate::parse_args_from(
        [
            "--raw",
            "legacy",
            "--out",
            "data",
            "--patch",
            "old-version",
            "--strict-columns",
        ]
        .into_iter()
        .map(String::from),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let crate::Mode::BaseItems(args) = mode else {
        panic!("legacy raw mode changed")
    };
    assert_eq!(args.raw, PathBuf::from("legacy"));
    assert_eq!(args.patch, "old-version");
    assert!(args.strict_columns);
    assert!(matches!(
        crate::parse_args_from(flags.into_iter().map(String::from)),
        Ok(crate::Mode::OrdinaryFacts(_))
    ));
}
