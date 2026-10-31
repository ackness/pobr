//! Consume the byte-pinned player archive against the already verified v4 parent.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

use pobr_data::catalog::native_player::{
    NATIVE_GENERIC_OT, NATIVE_PLAYER_AO, NATIVE_PLAYER_ID, NativePlayerContextDef,
    NativePlayerFacts, NativePlayerMetadataStat, NativePlayerOrigin, NativePlayerReference,
    NativeStartingRelation,
};
use pobr_data::catalog::native_runtime::{NativeGame, NativeRealm};
use serde_json::{Value, json};

use super::super::trust::{self, VerifiedInput, require, strict_json};
use super::{inputs, json_bytes};
use crate::raw_source::sha256_hex;

pub(super) const PRODUCER_REVISION: &str = "cf190d05b09b7aac17e484107cd8152b5ae00fa1";
const FIXTURE: &[u8] =
    include_bytes!("../../../tests/fixtures/native-facts/player-context-manifest.json");
const PAYLOADS: &[&str] = &[
    "KeywordPopups.json",
    "StartingPassiveSkills.json",
    "diagnostics.json",
    "metadata.json",
    "parent-manifest.json",
    "player_context.json",
    "source-lock.json",
];

pub(super) struct Args {
    pub root: PathBuf,
    pub manifest_sha256: String,
    pub source_lock_sha256: String,
}

pub(super) struct Player {
    pub context: NativePlayerContextDef,
    pub manifest: Value,
}

pub(super) fn verify(args: &Args, parent: &VerifiedInput) -> Result<Player, String> {
    let manifest_bytes = inputs::pinned_bytes(
        &args.root.join("manifest.json"),
        &args.manifest_sha256,
        1024 * 1024,
    )?;
    let manifest = strict_json(&manifest_bytes)?;
    verify_manifest(&manifest)?;
    let expected = PAYLOADS
        .iter()
        .copied()
        .chain(["manifest.json"])
        .collect::<BTreeSet<_>>();
    let mut observed = BTreeSet::new();
    for entry in fs::read_dir(&args.root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        require(
            entry.file_type().map_err(|e| e.to_string())?.is_file(),
            "player inventory must contain only regular files",
        )?;
        observed.insert(
            entry
                .file_name()
                .into_string()
                .map_err(|_| "non-UTF8 player filename")?,
        );
    }
    require(
        observed.iter().map(String::as_str).collect::<BTreeSet<_>>() == expected,
        "player eight-file inventory mismatch",
    )?;
    trust::exact_keys(
        &manifest["files"],
        &PAYLOADS.iter().map(|s| (*s).into()).collect::<Vec<_>>(),
    )?;
    let mut archive = BTreeMap::from([(
        "manifest.json".into(),
        String::from_utf8(manifest_bytes).map_err(|e| e.to_string())?,
    )]);
    let mut documents = BTreeMap::new();
    let mut total = 0;
    for &name in PAYLOADS {
        let fp = &manifest["files"][name];
        trust::exact_keys(fp, &["bytes".into(), "sha256".into()])?;
        let size = fp["bytes"].as_u64().ok_or("player payload size missing")?;
        total += size;
        require(
            size > 0 && size <= 8 * 1024 * 1024 && total <= 16 * 1024 * 1024,
            "player payload byte limit",
        )?;
        let bytes = inputs::pinned_bytes(
            &args.root.join(name),
            fp["sha256"].as_str().ok_or("player hash missing")?,
            size,
        )?;
        require(bytes.len() as u64 == size, "player payload size mismatch")?;
        documents.insert(name.to_owned(), strict_json(&bytes)?);
        archive.insert(
            name.to_owned(),
            String::from_utf8(bytes).map_err(|e| e.to_string())?,
        );
    }
    let identity = &manifest["source_identity"];
    // The embedded manifest is compared byte-for-byte, not only by matching metadata.
    let parent_sha = sha256_hex(&parent.manifest_bytes);
    require(
        archive["parent-manifest.json"].as_bytes() == parent.manifest_bytes.as_slice()
            && manifest["parent"]["manifest_sha256"] == parent_sha
            && manifest["parent"]["source_identity"] == parent.manifest["source_identity"],
        "player embedded parent mismatch",
    )?;
    for field in ["game", "realm", "game_patch"] {
        require(
            identity[field] == parent.manifest["source_identity"][field],
            &format!("player/parent {field} mismatch"),
        )?;
    }
    let lock_bytes = archive["source-lock.json"].as_bytes();
    require(
        sha256_hex(lock_bytes) == args.source_lock_sha256
            && identity["source_lock_sha256"] == args.source_lock_sha256,
        "external player source lock mismatch",
    )?;
    let lock = &documents["source-lock.json"];
    let selected = &lock["sources"][identity["source"]
        .as_str()
        .ok_or("player source key missing")?];
    require(
        lock["schema_version"] == 1
            && selected["profile"] == "player-context-a2"
            && selected["kind"] == "official-client-resource-candidate"
            && identity["selected_source_sha256"] == sha256_hex(&json_bytes(selected)?),
        "player selected lock mismatch",
    )?;
    for field in ["game", "realm", "game_patch", "acquisition"] {
        require(
            identity[field] == selected[field],
            &format!("player lock {field} mismatch"),
        )?;
    }
    let fixture = strict_json(FIXTURE)?;
    let decoder_lock = &selected["decoder"];
    for (key, value) in [
        ("revision", &fixture["decoder"]["revision"]),
        (
            "dat_schema_revision",
            &fixture["decoder"]["schema_revision"],
        ),
        ("schema_sha256", &fixture["decoder"]["schema_sha256"]),
    ] {
        require(&decoder_lock[key] == value, "player lock decoder mismatch")?;
    }
    require(
        selected["resources"]
            .as_object()
            .is_some_and(|r| r.len() == 6)
            && selected["resources"] == identity["raw_resources"],
        "player raw lock inventory mismatch",
    )?;
    let facts: NativePlayerFacts = serde_json::from_value(documents["player_context.json"].clone())
        .map_err(|e| e.to_string())?;
    require(
        serde_json::to_value(&facts.metadata_roots).map_err(|e| e.to_string())?
            == selected["metadata_roots"]
            && facts.character.source.id.as_deref() == selected["character_id"].as_str(),
        "player selected roots/actor mismatch",
    )?;
    let context = NativePlayerContextDef {
        schema_version: 1,
        game: NativeGame::Poe2,
        realm: NativeRealm::International,
        game_patch: identity["game_patch"]
            .as_str()
            .ok_or("player patch missing")?
            .into(),
        source_manifest_sha256: args.manifest_sha256.clone(),
        source_lock_sha256: args.source_lock_sha256.clone(),
        parent_manifest_sha256: parent_sha,
        parent_source_lock_sha256: sha256_hex(&parent.lock_bytes),
        facts,
        metadata: serde_json::from_value(documents["metadata.json"].clone())
            .map_err(|e| e.to_string())?,
        starting_passive_skills: serde_json::from_value(
            documents["StartingPassiveSkills.json"].clone(),
        )
        .map_err(|e| e.to_string())?,
        keyword_popups: serde_json::from_value(documents["KeywordPopups.json"].clone())
            .map_err(|e| e.to_string())?,
        source_archive: archive,
    };
    validate_projection(&context, parent)?;
    Ok(Player { context, manifest })
}

fn verify_manifest(m: &Value) -> Result<(), String> {
    let fixed = strict_json(FIXTURE)?;
    require(
        m["format"] == "repoe-native-player-context/v1"
            && m["status"] == "partial-candidate"
            && m["production_ready"] == false,
        "unsupported player package",
    )?;
    // All 14 fixed cf190d05 Git-source fingerprints, independent of package claims.
    require(
        m["producer"] == fixed["producer"]
            && m["producer"].as_object().is_some_and(|p| p.len() == 14),
        "player producer identity mismatch",
    )?;
    let identity = &m["source_identity"];
    require(
        identity["game"] == "poe2"
            && identity["realm"] == "international"
            && identity["game_patch"] == "0.5.5.5"
            && identity["acquisition"]["status"] == "unreviewed-acquisition-draft",
        "unsupported player game/realm/patch/status",
    )?;
    // Reuse the existing 117-source verifier, translating only the producer's field names.
    let d = &m["decoder"];
    for field in [
        "revision",
        "schema_revision",
        "schema_sha256",
        "specification_sha256",
        "trusted_source_pin_sha256",
    ] {
        require(
            d[field] == fixed["decoder"][field],
            "player decoder/schema identity mismatch",
        )?;
    }
    trust::verify_decoder(
        &json!({"pypoe_revision":d["revision"], "python":m["python_version"],
        "sources":d["sources"], "trusted_source_pin_sha256":d["trusted_source_pin_sha256"]}),
    )
}

fn physical_rows<'a>(doc: &'a Value, table: &str) -> Result<&'a [Value], String> {
    require(doc["table"] == table, "player reference table mismatch")?;
    let rows = doc["rows"].as_array().ok_or("physical rows missing")?;
    let mut seen = BTreeSet::new();
    for (i, row) in rows.iter().enumerate() {
        require(
            row["_row"] == i && row["values"].is_object(),
            "player physical row/order mismatch",
        )?;
        if let Some(id) = row["values"].get("Id") {
            let id = id.as_str().ok_or("physical ID must be string")?;
            require(
                !id.is_empty() && seen.insert(id),
                "duplicate player physical stable ID",
            )?;
        }
    }
    Ok(rows)
}

fn reference(table: &str, row: usize, values: &Value) -> NativePlayerReference {
    let local = matches!(table, "StartingPassiveSkills" | "KeywordPopups");
    NativePlayerReference {
        origin: if local {
            NativePlayerOrigin::Player
        } else {
            NativePlayerOrigin::Parent
        },
        table: table.into(),
        row,
        id: values["Id"].as_str().map(String::from),
        payload: if local {
            format!("{table}.json")
        } else {
            format!("tables/{table}.json")
        },
        pointer: format!("/rows/{row}/values"),
    }
}

pub(super) fn validate_projection(
    c: &NativePlayerContextDef,
    parent: &VerifiedInput,
) -> Result<(), String> {
    let starting = serde_json::to_value(&c.starting_passive_skills).map_err(|e| e.to_string())?;
    let keywords = serde_json::to_value(&c.keyword_popups).map_err(|e| e.to_string())?;
    physical_rows(&keywords, "KeywordPopups")?;
    let starts = physical_rows(&starting, "StartingPassiveSkills")?;
    for (doc, table) in [
        (&starting, "StartingPassiveSkills"),
        (&keywords, "KeywordPopups"),
    ] {
        require(
            doc["raw_resource"] == format!("Data/Balance/{table}.datc64")
                && doc["validation"]["rows"]
                    == doc["rows"]
                        .as_array()
                        .ok_or("missing local physical rows")?
                        .len(),
            "local player physical resource/row count mismatch",
        )?;
    }
    let mut tables = BTreeMap::new();
    for name in [
        "Characters",
        "PassiveSkills",
        "Stats",
        "ClassPassiveSkillOverrides",
    ] {
        let doc = parent
            .documents
            .get(&format!("tables/{name}.json"))
            .ok_or("missing player parent table")?;
        tables.insert(name, physical_rows(doc, name)?);
    }
    let characters = tables["Characters"];
    let matches = characters
        .iter()
        .enumerate()
        .filter(|(_, row)| row["values"]["Id"] == NATIVE_PLAYER_ID)
        .collect::<Vec<_>>();
    require(
        matches.len() == 1 && matches[0].0 == 7,
        "selected actor must be exact Character7 stable ID",
    )?;
    let raw = &matches[0].1["values"];
    require(
        c.facts.character.source == reference("Characters", 7, raw)
            && serde_json::to_value(&c.facts.character.values).map_err(|e| e.to_string())? == *raw
            && raw["AOFile"] == NATIVE_PLAYER_AO,
        "selected character physical projection mismatch",
    )?;
    let mut relations = Vec::new();
    for (i, row) in starts.iter().enumerate() {
        let values = &row["values"];
        let mut options = Vec::new();
        for index in values["PassiveSkills"]
            .as_array()
            .ok_or("missing starting options")?
        {
            let index = inputs::index(index)?;
            let passive = tables["PassiveSkills"]
                .get(index)
                .ok_or("starting passive out of range")?;
            options.push(reference("PassiveSkills", index, &passive["values"]));
        }
        relations.push(NativeStartingRelation {
            source: reference("StartingPassiveSkills", i, values),
            options,
        });
    }
    let selected = inputs::index(&raw["StartingPassive"])?;
    require(
        relations.len() == 6
            && relations.iter().map(|r| r.options.len()).sum::<usize>() == 12
            && selected == 2
            && relations.get(selected) == Some(&c.facts.starting_relation)
            && relations == c.facts.all_starting_relations
            && relations[2]
                .options
                .iter()
                .map(|r| r.row)
                .collect::<Vec<_>>()
                == [2779, 1146],
        "starting relationship projection mismatch; choices are never grants",
    )?;
    let overrides = tables["ClassPassiveSkillOverrides"]
        .iter()
        .enumerate()
        .filter(|(_, row)| row["values"]["CharacterToOverrideFor"] == 7)
        .map(|(i, row)| reference("ClassPassiveSkillOverrides", i, &row["values"]))
        .collect::<Vec<_>>();
    require(
        overrides == c.facts.selected_class_overrides && overrides.is_empty(),
        "Character1 class override cannot bind Character7",
    )?;
    validate_metadata(c)?;
    let stat_rows = tables["Stats"];
    let stat_ids = stat_rows
        .iter()
        .enumerate()
        .map(|(i, row)| {
            row["values"]["Id"]
                .as_str()
                .ok_or("stat ID missing")
                .map(|id| (id, i))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let mut expected = Vec::new();
    for (path, file) in &c.metadata.files {
        for (i, assignment) in file.assignments.iter().enumerate() {
            if assignment.plain_stats_scope {
                expected.push(NativePlayerMetadataStat {
                    path: path.clone(),
                    assignment: i,
                    key_raw: assignment.key_raw.clone(),
                    value_raw: assignment.value_raw.clone(),
                    scope: assignment.scope.clone(),
                    stat: stat_ids
                        .get(assignment.key_raw.as_str())
                        .map(|&row| reference("Stats", row, &stat_rows[row]["values"])),
                });
            }
        }
    }
    require(
        expected == c.facts.metadata_stats && expected.len() == 99,
        "metadata Stats physical projection mismatch",
    )?;
    // Validate every reference recursively, including nullable IDs and origin-specific paths.
    fn walk(
        value: &Value,
        local: &BTreeMap<&str, Value>,
        parent: &VerifiedInput,
    ) -> Result<(), String> {
        if let Some(object) = value.as_object() {
            if object.contains_key("origin") {
                let r: NativePlayerReference =
                    serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
                let doc = match r.origin {
                    NativePlayerOrigin::Player => local.get(r.payload.as_str()),
                    NativePlayerOrigin::Parent => parent.documents.get(&r.payload),
                }
                .ok_or("reference resolved in wrong player/parent origin")?;
                let rows = physical_rows(doc, &r.table)?;
                let row = rows.get(r.row).ok_or("reference row out of range")?;
                require(
                    r == reference(&r.table, r.row, &row["values"]),
                    "reference stable ID/payload/pointer/origin mismatch",
                )?;
            }
            for value in object.values() {
                walk(value, local, parent)?;
            }
        } else if let Some(values) = value.as_array() {
            for value in values {
                walk(value, local, parent)?;
            }
        }
        Ok(())
    }
    let local = BTreeMap::from([
        ("StartingPassiveSkills.json", starting),
        ("KeywordPopups.json", keywords),
    ]);
    walk(
        &serde_json::to_value(&c.facts).map_err(|e| e.to_string())?,
        &local,
        parent,
    )
}

fn utf16(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn validate_metadata(c: &NativePlayerContextDef) -> Result<(), String> {
    let manifest = strict_json(c.source_archive["manifest.json"].as_bytes())?;
    let roots = &c.facts.metadata_roots;
    require(
        roots.character_ao == NATIVE_PLAYER_AO
            && roots.generic_stats_ot == NATIVE_GENERIC_OT
            && c.metadata.format == "repoe-native-metadata-archive/v1",
        "AO/OT roots must stay distinct",
    )?;
    let edges = [
        (NATIVE_PLAYER_AO, Some("Metadata/Characters/Character.ao")),
        (
            "Metadata/Characters/Character.ao",
            Some("Metadata/Parent.ao"),
        ),
        ("Metadata/Parent.ao", None),
        (NATIVE_GENERIC_OT, None),
    ];
    require(
        c.metadata.files.len() == edges.len(),
        "metadata literal closure inventory mismatch",
    )?;
    let mut raw_critical_count = 0;
    for (path, target) in edges {
        let f = c
            .metadata
            .files
            .get(path)
            .ok_or("missing metadata literal closure")?;
        require(
            f.path == path
                && f.extends.len() == 1
                && f.extends[0].target.as_deref() == target
                && f.format == "repoe-native-metadata-lines/v1"
                && f.encoding == "BOM UTF-16LE",
            "metadata literal parent/format mismatch",
        )?;
        let edge = &f.extends[0];
        let parent_line = edge
            .line
            .checked_sub(1)
            .and_then(|i| f.records.get(i))
            .ok_or("literal parent source line missing")?;
        require(
            parent_line.scope.is_empty()
                && parent_line.literal.trim() == format!("extends \"{}\"", edge.literal),
            "literal metadata edge/source line mismatch",
        )?;
        require(
            f.merge_policy == "none; ordered lexical records only",
            "metadata must not declare an effective merge",
        )?;
        // Reconstruct the canonical physical UTF-16 bytes from complete retained lines.
        let mut bytes = vec![0xff, 0xfe];
        for (i, record) in f.records.iter().enumerate() {
            require(
                record.line == i + 1 && record.byte_start == bytes.len(),
                "metadata record span/order mismatch",
            )?;
            bytes.extend(utf16(&record.literal));
            let newline = record
                .byte_end
                .checked_sub(bytes.len())
                .ok_or("metadata line span too short")?;
            match newline {
                0 => {}
                2 => bytes.extend(utf16("\n")),
                4 => bytes.extend(utf16("\r\n")),
                _ => return Err("unsupported metadata line ending span".into()),
            }
        }
        let raw = &manifest["source_identity"]["raw_resources"][path];
        require(
            bytes.len() == f.bytes
                && raw["bytes"] == bytes.len()
                && raw["sha256"] == sha256_hex(&bytes),
            "metadata canonical physical fingerprint mismatch",
        )?;
        let mut lines = BTreeSet::new();
        let mut raw_plain_count = 0;
        for record in &f.records {
            if let Some((key, _)) = record.literal.trim_start().split_once('=') {
                if key.trim() == "base_critical_hit_damage_bonus" {
                    raw_critical_count += 1;
                }
                if record.scope.len() == 1 && record.scope[0].name_raw == "Stats" {
                    raw_plain_count += 1;
                    require(
                        f.assignments
                            .iter()
                            .any(|a| a.line == record.line && a.key_raw == key.trim()),
                        "plain raw Stats assignment missing from metadata archive",
                    )?;
                }
            }
        }
        require(
            raw_plain_count == f.assignments.iter().filter(|a| a.plain_stats_scope).count(),
            "raw plain Stats assignment inventory mismatch",
        )?;
        for a in &f.assignments {
            let record = a
                .line
                .checked_sub(1)
                .and_then(|i| f.records.get(i))
                .ok_or("assignment line missing")?;
            let (key, _) = record
                .literal
                .trim_start()
                .split_once('=')
                .ok_or("assignment absent from raw line")?;
            require(
                lines.insert(a.line)
                    && key.trim() == a.key_raw
                    && record.scope == a.scope
                    && a.plain_stats_scope
                        == (a.scope.len() == 1 && a.scope[0].name_raw == "Stats")
                    && a.value_byte_start >= record.byte_start
                    && a.value_byte_end <= record.byte_end
                    && bytes.get(a.value_byte_start..a.value_byte_end)
                        == Some(utf16(&a.value_raw).as_slice()),
                "metadata assignment/raw span mismatch",
            )?;
        }
    }
    require(
        raw_critical_count == 1,
        "duplicate/ambiguous raw critical assignment",
    )?;
    require(
        c.facts.literal_closure_heights
            == BTreeMap::from([(NATIVE_PLAYER_AO.into(), 2), (NATIVE_GENERIC_OT.into(), 0)]),
        "literal closure heights mismatch",
    )
}
