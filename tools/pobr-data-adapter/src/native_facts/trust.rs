//! Verify the external receipt and retain exactly the bytes subsequently parsed.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Component, Path};

use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};

use super::{Args, Profile};
use crate::raw_source::sha256_hex;

pub(super) const PRODUCER_REVISION: &str = "b6def2152dfc48d8ec49c7a237b2e973c3830e97";
const DECODER_PIN_SHA: &str = "64703bc3c5eca107108244e02baf0dc2cd46669e62a6b5ba273f868aa9db0095";
pub(super) const DECODER_PIN: &[u8] =
    include_bytes!("../../tests/fixtures/native-facts/pypoe-sources.json");
const PYPOE: &str = "66d1d7442c5718260e1a8efd24989d22987e91af";
const SCHEMA_REVISION: &str = "6034b071846ef61b39046ebdb59e6d5717179509";
const SCHEMA_SHA: &str = "50e098543345cf8323aeb294e2cfef1b6b44842faccf560ada935cebe22727df";

pub(super) const TABLES: [&str; 8] = [
    "Characters",
    "ClassPassiveSkillOverrides",
    "GrantedEffectQualityStats",
    "GrantedEffectStatSets",
    "GrantedEffects",
    "PassiveSkillTrees",
    "PassiveSkills",
    "Stats",
];

pub(super) struct VerifiedInput {
    pub profile: Profile,
    pub manifest: Value,
    pub manifest_bytes: Vec<u8>,
    pub lock_bytes: Vec<u8>,
    pub bytes: BTreeMap<String, Vec<u8>>,
    pub documents: BTreeMap<String, Value>,
}

pub(super) fn verify(args: &Args) -> Result<VerifiedInput, String> {
    check_sha(&args.expected_manifest_sha256)?;
    let manifest_bytes = read_regular(&args.input.join("manifest.json"))?;
    // External byte authentication precedes even JSON parsing.
    require(
        sha256_hex(&manifest_bytes) == args.expected_manifest_sha256,
        "external manifest sha256 mismatch",
    )?;
    let manifest = strict_json(&manifest_bytes)?;
    verify_manifest(&manifest, args.profile)?;
    let lock_bytes = read_regular(&args.source_lock)?;
    let identity = &manifest["source_identity"];
    require(
        identity["source_lock_sha256"] == sha256_hex(&lock_bytes),
        "source-lock bytes mismatch",
    )?;
    let lock = strict_json(&lock_bytes)?;
    require(
        lock["schema_version"] == 1,
        "unsupported source-lock schema",
    )?;
    let source = &lock["sources"][string(&identity["source"])?];
    if args.profile == Profile::CoreSkillsA0 {
        require(
            source["profile"] == "core-skills-a0",
            "source-lock profile mismatch",
        )?;
        verify_acquisition(&source["acquisition"])?;
        require(
            source["acquisition"] == identity["acquisition"],
            "source-lock acquisition mismatch",
        )?;
    }
    require(
        identity["selected_source_sha256"] == sha256_hex(&super::json_bytes(source)?),
        "selected source-lock entry mismatch",
    )?;
    for key in ["kind", "game", "game_patch", "realm"] {
        require(
            source[key] == identity[key],
            &format!("source-lock {key} mismatch"),
        )?;
    }
    let decoder = &source["decoder"];
    for (key, expected) in [
        ("revision", PYPOE),
        ("dat_schema_revision", SCHEMA_REVISION),
        ("schema_sha256", SCHEMA_SHA),
        ("repository", "https://github.com/repoe-fork/pypoe"),
        (
            "dat_schema_repository",
            "https://github.com/poe-tool-dev/dat-schema",
        ),
    ] {
        require(
            decoder[key] == expected,
            &format!("source-lock decoder {key} mismatch"),
        )?;
    }
    let raw_paths = raw_paths(args.profile);
    exact_keys(&source["resources"], &raw_paths)?;
    exact_keys(&identity["raw_resources"], &raw_paths)?;
    for path in &raw_paths {
        let raw = &identity["raw_resources"][path];
        exact_keys(raw, &["bytes".into(), "sha256".into()])?;
        check_fingerprint(raw)?;
        for key in ["bytes", "sha256"] {
            require(
                source["resources"][path][key] == raw[key],
                &format!("raw resource {path} {key} mismatch"),
            )?;
        }
    }
    let paths = payload_paths(args.profile);
    exact_keys(&manifest["files"], &paths)?;
    let expected: BTreeSet<_> = paths
        .iter()
        .cloned()
        .chain(["manifest.json".into(), "tables".into()])
        .collect();
    verify_inventory(&args.input, &args.input, &expected)?;
    let mut bytes = BTreeMap::new();
    // No payload is consumed until every fingerprint has passed.
    for path in &paths {
        safe_relative(path)?;
        let fingerprint = &manifest["files"][path];
        exact_keys(fingerprint, &["bytes".into(), "sha256".into()])?;
        check_fingerprint(fingerprint)?;
        let data = read_regular(&args.input.join(path))?;
        require(
            fingerprint["bytes"] == data.len() && fingerprint["sha256"] == sha256_hex(&data),
            &format!("payload fingerprint mismatch: {path}"),
        )?;
        bytes.insert(path.clone(), data);
    }
    if args.profile == Profile::CoreSkillsA0 {
        require(
            bytes["source-lock.json"] == lock_bytes,
            "embedded source-lock bytes mismatch",
        )?;
    }
    let documents = bytes
        .iter()
        .map(|(path, data)| {
            strict_json(data)
                .map(|value| (path.clone(), value))
                .map_err(|e| format!("{path}: {e}"))
        })
        .collect::<Result<_, _>>()?;
    Ok(VerifiedInput {
        profile: args.profile,
        manifest,
        manifest_bytes,
        lock_bytes,
        bytes,
        documents,
    })
}

fn verify_manifest(m: &Value, profile: Profile) -> Result<(), String> {
    let mut manifest_keys = vec![
        "decoder",
        "files",
        "format",
        "producer",
        "production_ready",
        "row_key",
        "schema",
        "scope",
        "source_identity",
        "status",
        "summary",
    ]
    .into_iter()
    .map(String::from)
    .collect::<Vec<_>>();
    if profile == Profile::CoreSkillsA0 {
        manifest_keys.push("profile".into());
        require(m["profile"] == "core-skills-a0", "invalid manifest profile")?;
    }
    exact_keys(m, &manifest_keys)?;
    for (key, value) in [
        ("format", profile.format()),
        ("status", "partial-candidate"),
    ] {
        require(m[key] == value, &format!("invalid manifest {key}"))?;
    }
    require(
        m["production_ready"] == false,
        "native facts must be partial, never production_ready",
    )?;
    let identity = &m["source_identity"];
    let mut identity_keys = vec![
        "game",
        "game_patch",
        "kind",
        "raw_resources",
        "realm",
        "selected_source_sha256",
        "source",
        "source_lock_sha256",
    ]
    .into_iter()
    .map(String::from)
    .collect::<Vec<_>>();
    if profile == Profile::CoreSkillsA0 {
        identity_keys.push("acquisition".into());
        verify_acquisition(&identity["acquisition"])?;
        require(
            !string(&identity["source"])?.is_empty(),
            "empty selected source",
        )?;
    }
    exact_keys(identity, &identity_keys)?;
    for (key, value) in [
        ("game", "poe2"),
        ("game_patch", "0.5.5.5"),
        ("realm", "international"),
        ("kind", "official-client-resource-candidate"),
    ] {
        require(
            identity[key] == value,
            &format!("invalid source identity {key}"),
        )?;
    }
    if profile == Profile::BaseV1 {
        require(
            identity["source"] == "official-client-0.5.5.5",
            "invalid source identity source",
        )?;
    }
    for key in ["source_lock_sha256", "selected_source_sha256"] {
        check_sha(string(&identity[key])?)?;
    }
    // These file identities bind this consumer to the requested producer revision, which v1
    // records as source fingerprints rather than a Git revision in the package itself.
    let mut expected_producer = serde_json::json!({
        "RePoE/native_dat.py": "824f561af6e4b83243b459d726d6bad6821663061ac840d39c2854145707ff86",
        "RePoE/native_facts.py": "f7bf979a7a8e4bf04143cb8d04d3728a876c1bc5dd2498c43cd2d0ec78526548",
        "RePoE/native_tools.py": "46e4a0dd7250c44f9da714f55d90ef967223e0f50b0631d130a112c6144a37c7",
        "RePoE/schema_candidate.py": "1e2d130d8e721c6398bc3cbcd63b0acb27e4487b5cddef9bf60624a8526d1b04",
        "pyproject.toml": "db02b9819adfc21f00365595bf9d53ffb15a644cb759d539b123781a572a7912",
        "schema/SHA256SUMS": "d3e5ecda7d21c8a42f6b66009a42a7e0324dba9131d63e079be6c1f596f3a308",
        "schema/pypoe-sources.json": DECODER_PIN_SHA,
        "schema/source.json": "7afa08a39f3da0b2e368d716593c2adef03308f100ec49edac73e8c7416441d3",
        "uv.lock": "6ac815b9e4263081b51e823a3f297f35a1c55c7fa57956d3bdd5d4a506c1c66b"
    });
    // Fixed SHA-256 values independently checked against aa2fb163 Git blobs, not
    // derived from caller-supplied manifest producer claims.
    if profile == Profile::CoreSkillsA0 {
        expected_producer["RePoE/native_facts.py"] =
            "af8148ccf8c68fef5b0c867a892285ff95b82e50c4933d1acc4f5c499632e4a1".into();
        expected_producer["RePoE/native_skills.py"] =
            "e65aff1265c3c1becd7890acb3c0182232651dd351dab5d982b0d119f74d453d".into();
    }
    require(
        m["producer"] == expected_producer,
        "producer identity mismatch",
    )?;
    exact_keys(
        &m["schema"],
        &[
            "compatibility_views",
            "native_specification_sha256",
            "revision",
            "sha256",
        ]
        .map(String::from),
    )?;
    require(
        m["schema"]["revision"] == SCHEMA_REVISION
            && m["schema"]["sha256"] == SCHEMA_SHA
            && m["schema"]["compatibility_views"] == 0
            && m["schema"]["native_specification_sha256"]
                == "c9417f03e2205ff649721c3dbf9620ee0c703bf905b3cf53469ce3eda86f1e25",
        "schema identity mismatch",
    )?;
    exact_keys(
        &m["decoder"],
        &[
            "pypoe_revision",
            "python",
            "sources",
            "trusted_source_pin_sha256",
        ]
        .map(String::from),
    )?;
    require(
        m["decoder"]["pypoe_revision"] == PYPOE && !string(&m["decoder"]["python"])?.is_empty(),
        "decoder identity mismatch",
    )?;
    let sources = m["decoder"]["sources"]
        .as_object()
        .ok_or("missing decoder sources")?;
    require(
        m["decoder"]["trusted_source_pin_sha256"] == DECODER_PIN_SHA
            && sha256_hex(DECODER_PIN) == DECODER_PIN_SHA,
        "decoder trusted source pin mismatch",
    )?;
    let pin = strict_json(DECODER_PIN)?;
    let pinned_sources = pin["sources"]
        .as_object()
        .ok_or("missing trusted decoder sources")?;
    require(
        pin["revision"] == PYPOE
            && pinned_sources.len() == 117
            && sources.keys().eq(pinned_sources.keys()),
        "decoder source inventory mismatch",
    )?;
    for (path, hash) in sources {
        safe_relative(path)?;
        check_sha(string(hash)?)?;
        require(
            pinned_sources[path]
                .as_array()
                .is_some_and(|allowed| allowed.contains(hash)),
            &format!("decoder source fingerprint mismatch: {path}"),
        )?;
    }
    require(
        m["scope"].is_array() && m["row_key"].is_string(),
        "missing source scope/row evidence",
    )
}

pub(super) fn payload_paths(profile: Profile) -> Vec<String> {
    tables(profile)
        .into_iter()
        .map(|table| format!("tables/{table}.json"))
        .chain(
            [
                "diagnostics.json",
                "graph.json",
                "passives.json",
                "skills.json",
            ]
            .map(String::from),
        )
        .chain(if profile == Profile::CoreSkillsA0 {
            vec!["skill_core.json".into(), "source-lock.json".into()]
        } else {
            vec![]
        })
        .collect()
}

pub(super) fn tables(profile: Profile) -> Vec<&'static str> {
    TABLES
        .into_iter()
        .chain(if profile == Profile::CoreSkillsA0 {
            super::core_skills::TABLES.to_vec()
        } else {
            vec![]
        })
        .collect()
}

fn raw_paths(profile: Profile) -> Vec<String> {
    tables(profile)
        .into_iter()
        .map(|table| format!("Data/Balance/{table}.datc64"))
        .chain(["Metadata/PassiveSkillGraph.psg".into()])
        .collect()
}

fn verify_acquisition(value: &Value) -> Result<(), String> {
    exact_keys(value, &["status".into(), "evidence".into()])?;
    require(
        matches!(
            value["status"].as_str(),
            Some("unreviewed-acquisition-draft" | "reviewed-source-input")
        ),
        "invalid acquisition status",
    )?;
    let evidence = value["evidence"]
        .as_array()
        .ok_or("missing acquisition evidence")?;
    require(!evidence.is_empty(), "empty acquisition evidence")?;
    for item in evidence {
        exact_keys(item, &["kind".into(), "sha256".into()])?;
        require(
            !string(&item["kind"])?.is_empty(),
            "empty acquisition evidence kind",
        )?;
        check_sha(string(&item["sha256"])?)?;
    }
    Ok(())
}

pub(super) fn require(condition: bool, message: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}

pub(super) fn exact_keys(value: &Value, expected: &[String]) -> Result<(), String> {
    let keys: BTreeSet<_> = value
        .as_object()
        .ok_or("expected JSON object")?
        .keys()
        .collect();
    require(
        keys == expected.iter().collect(),
        "unexpected or missing JSON object keys",
    )
}

fn string(value: &Value) -> Result<&str, String> {
    value.as_str().ok_or_else(|| "expected string".into())
}

fn check_sha(value: &str) -> Result<(), String> {
    require(
        value.len() == 64
            && value
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
        "invalid lowercase sha256",
    )
}

fn check_fingerprint(value: &Value) -> Result<(), String> {
    require(
        value["bytes"].as_u64().is_some_and(|size| size > 0),
        "invalid positive byte size",
    )?;
    check_sha(string(&value["sha256"])?)
}

fn safe_relative(path: &str) -> Result<(), String> {
    require(
        !path.is_empty()
            && !path.contains('\\')
            && path
                .split('/')
                .all(|s| !s.is_empty() && s != "." && s != "..")
            && Path::new(path)
                .components()
                .all(|c| matches!(c, Component::Normal(_))),
        "unsafe relative input path",
    )
}

fn no_symlink_components(path: &Path) -> Result<(), String> {
    let absolute = std::path::absolute(path).map_err(|e| e.to_string())?;
    let mut prefix = std::path::PathBuf::new();
    for component in absolute.components() {
        require(
            !matches!(component, Component::ParentDir),
            "parent traversal in input path",
        )?;
        prefix.push(component);
        let meta =
            fs::symlink_metadata(&prefix).map_err(|e| format!("{}: {e}", prefix.display()))?;
        require(
            !meta.file_type().is_symlink(),
            &format!("symlink input component: {}", prefix.display()),
        )?;
    }
    Ok(())
}

fn read_regular(path: &Path) -> Result<Vec<u8>, String> {
    no_symlink_components(path)?;
    require(
        fs::symlink_metadata(path)
            .map_err(|e| e.to_string())?
            .is_file(),
        "input is not a regular file",
    )?;
    fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn verify_inventory(root: &Path, dir: &Path, expected: &BTreeSet<String>) -> Result<(), String> {
    no_symlink_components(dir)?;
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|e| e.to_string())?
            .to_str()
            .ok_or("non-UTF8 input path")?
            .to_owned();
        require(
            expected.contains(&relative),
            &format!("unexpected input path: {relative}"),
        )?;
        let ty = entry.file_type().map_err(|e| e.to_string())?;
        require(!ty.is_symlink(), &format!("symlink input: {relative}"))?;
        if relative == "tables" {
            require(ty.is_dir(), "tables is not a directory")?;
            verify_inventory(root, &path, expected)?;
        } else {
            require(ty.is_file(), "payload is not a regular file")?;
        }
    }
    Ok(())
}

pub(super) fn require_absent(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("inspect output: {e}")),
        Ok(_) => Err(format!("refusing existing output: {}", path.display())),
    }
}

/// Recursively reject duplicate keys before serde can overwrite evidence.
pub(super) fn strict_json(bytes: &[u8]) -> Result<Value, String> {
    let mut decoder = serde_json::Deserializer::from_slice(bytes);
    let value = UniqueJson::deserialize(&mut decoder).map_err(|e| e.to_string())?;
    decoder.end().map_err(|e| e.to_string())?;
    Ok(value.0)
}

struct UniqueJson(Value);
impl<'de> Deserialize<'de> for UniqueJson {
    fn deserialize<D: Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        struct JsonVisitor;
        impl<'de> Visitor<'de> for JsonVisitor {
            type Value = UniqueJson;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("JSON without duplicate object keys")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> {
                Number::from_f64(v)
                    .map(|n| UniqueJson(n.into()))
                    .ok_or_else(|| E::custom("nonfinite JSON number"))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(UniqueJson(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(value) = seq.next_element::<UniqueJson>()? {
                    values.push(value.0);
                }
                Ok(UniqueJson(values.into()))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut values = Map::new();
                while let Some((key, value)) = map.next_entry::<String, UniqueJson>()? {
                    if values.insert(key, value.0).is_some() {
                        return Err(de::Error::custom("duplicate JSON key"));
                    }
                }
                Ok(UniqueJson(values.into()))
            }
        }
        decoder.deserialize_any(JsonVisitor)
    }
}
