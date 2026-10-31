//! Authenticated physical identities, complete stat slots and whole-row context.

use super::policy::{Policy, VARIABLES};
use crate::native_facts::reader::{self, Tables, VerifiedFacts};
use crate::raw_source::sha256_hex;
use pobr_data::native_stat::{NativeStat, NativeStatProvenance, NativeStatReference};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Facts<'a> {
    pub tables: Tables<'a>,
    pub passives: BTreeMap<String, usize>,
    pub characters: BTreeMap<String, usize>,
    pub involved: BTreeMap<usize, Vec<usize>>,
    pub relations: Vec<Value>,
    pub reference_row: usize,
    pub reference_context: Value,
    pub slots: Vec<String>,
}

impl<'a> Facts<'a> {
    pub fn read(input: &'a VerifiedFacts, policy: &Policy) -> Result<Self, String> {
        let tables = Tables::selected(input, &super::TABLES)?;
        for (table, field, kind, array, target) in [
            ("PassiveSkills", "Id", "string", false, None),
            ("PassiveSkills", "Name", "string", false, None),
            ("PassiveSkills", "Icon_DDSFile", "string", false, None),
            ("PassiveSkills", "PassiveSkillGraphId", "u16", false, None),
            ("PassiveSkills", "Stats", "foreignrow", true, Some("Stats")),
            ("Stats", "Id", "string", false, None),
            ("Characters", "Id", "string", false, None),
            ("Characters", "Name", "string", false, None),
            ("Characters", "BaseStrength", "i32", false, None),
            ("Characters", "BaseDexterity", "i32", false, None),
            ("Characters", "BaseIntelligence", "i32", false, None),
            (
                "ClassPassiveSkillOverrides",
                "CharacterToOverrideFor",
                "foreignrow",
                false,
                Some("Characters"),
            ),
            (
                "ClassPassiveSkillOverrides",
                "SkillToOverride",
                "foreignrow",
                false,
                Some("PassiveSkills"),
            ),
            (
                "ClassPassiveSkillOverrides",
                "Override",
                "foreignrow",
                false,
                Some("PassiveSkills"),
            ),
        ] {
            column(input, table, field, kind, array, target)?;
        }
        let fields = input.documents["tables/PassiveSkills.json"]["physical_fields"]
            .as_array()
            .ok_or("missing passive fields")?;
        let mut indexed = BTreeMap::new();
        for f in fields {
            let name = f.as_str().ok_or("invalid field name")?;
            if let Some(slot) = name
                .strip_prefix("Stat")
                .and_then(|s| s.strip_suffix("Value"))
            {
                let n = slot
                    .parse::<usize>()
                    .map_err(|_| "invalid stat value slot column")?;
                if n == 0 || slot != n.to_string() || indexed.insert(n, name.to_owned()).is_some() {
                    return Err("invalid/duplicate stat value slot column".into());
                }
                column(input, "PassiveSkills", name, "i32", false, None)?;
            }
        }
        if indexed.is_empty() || indexed.keys().copied().ne(1..=indexed.len()) {
            return Err("missing/noncontiguous stat value slots".into());
        }
        let slots: Vec<_> = indexed.into_values().collect();
        let passives = identities(&tables, "PassiveSkills", "Id")?;
        let characters = identities(&tables, "Characters", "Name")?;
        for row in 0..tables.len("Characters") {
            let raw = tables.row("Characters", row)?;
            for f in ["BaseStrength", "BaseDexterity", "BaseIntelligence"] {
                integer(&raw[f], f)?;
            }
        }
        let mut involved: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        let mut relations = Vec::new();
        for row in 0..tables.len("ClassPassiveSkillOverrides") {
            let raw = tables.row("ClassPassiveSkillOverrides", row)?;
            let character = reference(&tables, "Characters", &raw["CharacterToOverrideFor"])?;
            let base = reference(&tables, "PassiveSkills", &raw["SkillToOverride"])?;
            let replacement = reference(&tables, "PassiveSkills", &raw["Override"])?;
            involved.entry(base).or_default().push(row);
            involved.entry(replacement).or_default().push(row);
            let mut endpoints = Vec::new();
            for (field, source_row) in [("SkillToOverride", base), ("Override", replacement)] {
                let definition = tables.row("PassiveSkills", source_row)?;
                endpoints.push(json!({"field":field,"source_row":source_row,"id":definition["Id"],"graph_id":graph_id(definition)?}));
            }
            relations.push(json!({"source_row":row,"raw":raw,"character":{"source_row":character,"id":tables.row("Characters",character)?["Id"],"name":tables.row("Characters",character)?["Name"]},"definitions":endpoints}));
        }
        let &reference_row = passives
            .get(&policy.qualification.reference_passive_id)
            .ok_or("missing reviewed passive reference")?;
        let raw = tables.row("PassiveSkills", reference_row)?;
        let reference_context = context(raw)?;
        if sha256_hex(&reader::json_bytes(&reference_context)?)
            != policy.qualification.context_sha256
        {
            return Err("reference context SHA256 mismatch".into());
        }
        let out = Self {
            tables,
            passives,
            characters,
            involved,
            relations,
            reference_row,
            reference_context,
            slots,
        };
        let stats = out.stats(input, reference_row)?;
        if stats.len() != 1 || stats[0].id != "base_fire_damage_resistance_%" {
            return Err("reference must have the complete one fire-resistance stat".into());
        }
        Ok(out)
    }

    pub fn stats(&self, input: &VerifiedFacts, row: usize) -> Result<Vec<NativeStat>, String> {
        let raw = self.tables.row("PassiveSkills", row)?;
        graph_id(raw)?;
        for field in ["Name", "Icon_DDSFile"] {
            raw[field]
                .as_str()
                .ok_or_else(|| format!("PassiveSkills/{row}.{field}: string required"))?;
        }
        for field in &self.slots {
            integer(&raw[field], field)?;
        }
        let refs = raw["Stats"].as_array().ok_or("Stats must be an array")?;
        if refs.len() > self.slots.len() {
            return Err("stat/value slot cardinality exceeds declared columns".into());
        }
        let patch = input.manifest["source_identity"]["game_patch"]
            .as_str()
            .ok_or("missing game patch")?;
        refs.iter()
            .enumerate()
            .map(|(position, value)| {
                let stat_row = reference(&self.tables, "Stats", value)?;
                let id = self.tables.row("Stats", stat_row)?["Id"]
                    .as_str()
                    .ok_or("invalid stat Id")?;
                let value = i64::from(integer(&raw[&self.slots[position]], &self.slots[position])?);
                Ok(NativeStat {
                    id: id.into(),
                    value,
                    provenance: NativeStatProvenance {
                        game_patch: patch.into(),
                        source_manifest_sha256: sha256_hex(&input.manifest_bytes),
                        passive_id: raw["Id"].as_str().ok_or("invalid passive Id")?.into(),
                        passive_row: u32::try_from(row).map_err(|e| e.to_string())?,
                        stat_row: u32::try_from(stat_row).map_err(|e| e.to_string())?,
                        references: vec![
                            address(
                                input,
                                "PassiveSkills",
                                format!("/rows/{row}/values/{}", self.slots[position]),
                            )?,
                            address(input, "Stats", format!("/rows/{stat_row}/values/Id"))?,
                        ],
                    },
                })
            })
            .collect()
    }
    pub fn differences(&self, raw: &Value) -> Result<Vec<String>, String> {
        let reference = self
            .tables
            .row("PassiveSkills", self.reference_row)?
            .as_object()
            .ok_or("invalid reference context")?;
        let raw = raw.as_object().ok_or("invalid passive context")?;
        // Same whole-row rule as native_facts/runtime/tree.rs: identical keys;
        // only these six fields vary. No interpretation of unknown/context keys.
        let keys: BTreeSet<_> = raw.keys().chain(reference.keys()).collect();
        Ok(keys
            .into_iter()
            .filter(|key| {
                !raw.contains_key(*key)
                    || !reference.contains_key(*key)
                    || (!VARIABLES.contains(&key.as_str()) && raw.get(*key) != reference.get(*key))
            })
            .cloned()
            .collect())
    }
    pub fn unmapped(&self, input: &VerifiedFacts) -> Result<Vec<Value>, String> {
        super::TABLES.iter().map(|table| {
            let mut used:Vec<&str>=match *table {
                "PassiveSkills"=>vec!["Id","Name","Icon_DDSFile","PassiveSkillGraphId","Stats"],
                "Stats"=>vec!["Id"],"Characters"=>vec!["Id","Name","BaseStrength","BaseDexterity","BaseIntelligence"],
                _=>vec!["CharacterToOverrideFor","SkillToOverride","Override"],
            };
            if *table=="PassiveSkills" {used.extend(self.slots.iter().map(String::as_str));}
            let path=format!("tables/{table}.json");
            let columns:Vec<_>=input.documents[&path]["columns"].as_array().ok_or("missing columns")?.iter()
                .filter(|c| c["physical_fields"].as_array().is_some_and(|fs|fs.iter().any(|f|!used.iter().any(|u|f.as_str()==Some(*u))))).collect();
            Ok(json!({"table":table,"payload":path,"consumed_identity_or_value_columns":used,
                "qualification_only_columns":if *table=="PassiveSkills"{columns.clone()}else{vec![]},
                "unmapped_columns":if *table=="PassiveSkills"{vec![]}else{columns},
                "source_row_range":{"start":0,"end_exclusive":self.tables.len(table)},
                "scope":"remaining passive fields are compared as opaque complete context only (including ClientStrings Requirement and unavailable Ascendancy references); no predicate/ordinal/text rules inferred"}))
        }).collect()
    }
}
fn column(
    input: &VerifiedFacts,
    table: &str,
    field: &str,
    kind: &str,
    array: bool,
    target: Option<&str>,
) -> Result<(), String> {
    super::super::convert::column(
        input,
        table,
        &[field],
        &json!({"name":field,"type":kind,"array":array,"interval":false,"references":target.map(|t|json!({"table":t}))}),
    )
}
fn identities(
    tables: &Tables<'_>,
    table: &str,
    field: &str,
) -> Result<BTreeMap<String, usize>, String> {
    let mut out = BTreeMap::new();
    for row in 0..tables.len(table) {
        let id = tables.row(table, row)?[field]
            .as_str()
            .ok_or("invalid source identity")?;
        if id.is_empty() || out.insert(id.into(), row).is_some() {
            return Err(format!("empty/duplicate {table}.{field}"));
        }
    }
    Ok(out)
}
pub(super) fn integer(v: &Value, field: &str) -> Result<i32, String> {
    v.as_i64()
        .and_then(|n| i32::try_from(n).ok())
        .ok_or_else(|| format!("{field}: exact i32 required"))
}
pub(super) fn graph_id(raw: &Value) -> Result<u32, String> {
    raw["PassiveSkillGraphId"]
        .as_u64()
        .and_then(|n| u16::try_from(n).ok())
        .map(u32::from)
        .ok_or("PassiveSkillGraphId: exact u16 required".into())
}
fn reference(tables: &Tables<'_>, table: &str, v: &Value) -> Result<usize, String> {
    let row = v
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or("required physical FK must be usize")?;
    tables.row(table, row)?;
    Ok(row)
}
pub(super) fn context(raw: &Value) -> Result<Value, String> {
    let mut out = raw.as_object().ok_or("invalid raw context")?.clone();
    out.retain(|field, _| !VARIABLES.contains(&field.as_str()));
    Ok(Value::Object(out))
}
fn address(
    input: &VerifiedFacts,
    table: &str,
    pointer: String,
) -> Result<NativeStatReference, String> {
    let resource = format!("tables/{table}.json");
    let sha256 = input.manifest["files"][&resource]["sha256"]
        .as_str()
        .ok_or("missing payload hash")?
        .into();
    Ok(NativeStatReference {
        resource,
        sha256,
        pointer,
    })
}
