//! Physical prerequisite identities in the existing all-prerequisites DTO.

use super::{ascendancy, source::Facts};
use crate::native_facts::reader::VerifiedFacts;
use serde_json::{Map, Value, json};

pub(super) fn validate(input: &VerifiedFacts) -> Result<(), String> {
    for (field, kind, array, target) in [
        ("UnlockedBy", "row", true, "PassiveSkills"),
        ("VisibleForAscendancy", "foreignrow", false, "Ascendancy"),
    ] {
        super::source::column(input, "PassiveSkills", field, kind, array, Some(target))?;
    }
    Ok(())
}

pub(super) fn project(
    facts: &Facts<'_>,
    ascendancy: &ascendancy::Source,
    row: usize,
    out: &mut Map<String, Value>,
) -> Result<Value, String> {
    let raw = facts.tables.row("PassiveSkills", row)?;
    let refs = raw["UnlockedBy"]
        .as_array()
        .ok_or("UnlockedBy must be an array")?;
    let mut nodes = Vec::new();
    let mut records = Vec::new();
    for (position, reference) in refs.iter().enumerate() {
        let target = index(reference)?;
        let definition = facts.tables.row("PassiveSkills", target)?;
        let skill = super::source::graph_id(definition)?;
        nodes.push(skill);
        records.push(
            json!({"source_pointer":format!("/rows/{row}/values/UnlockedBy/{position}"),
            "target_row":target,"id":definition["Id"],"graph_id":skill}),
        );
    }
    let visibility = raw
        .get("VisibleForAscendancy")
        .ok_or("missing VisibleForAscendancy")?;
    let visible = if visibility.is_null() {
        None
    } else {
        let target = index(visibility)?;
        Some(
            ascendancy
                .entries
                .get(&target)
                .ok_or("unlock targets unavailable ascendancy")?,
        )
    };
    // Preserve extensions in the prepared object; only the two proven fields are owned.
    let mut constraint = match out.get("unlock_constraint") {
        None | Some(Value::Null) => Map::new(),
        Some(value) => value
            .as_object()
            .ok_or("invalid prepared unlock constraint")?
            .clone(),
    };
    constraint.remove("nodes");
    constraint.remove("ascendancy");
    if !nodes.is_empty() || visible.is_some() {
        constraint.insert("nodes".into(), json!(nodes));
    }
    if let Some(entry) = visible {
        constraint.insert("ascendancy".into(), json!(entry.id));
    }
    if constraint.is_empty() {
        out.remove("unlock_constraint");
    } else {
        out.insert("unlock_constraint".into(), Value::Object(constraint));
        out.remove("native_stats");
    }
    Ok(
        json!({"owner":"target-physical-unlock-identities","source_row":row,"nodes":records,
        "visible_for_ascendancy":visible.map(|entry|json!({"source_pointer":format!("/rows/{row}/values/VisibleForAscendancy"),"target_row":entry.row,"id":entry.id})),
        "constraint":out.get("unlock_constraint"),"requirement":"not interpreted; retained in whole-context guard"}),
    )
}

pub(super) fn validate_variant(raw: &Value) -> Result<(), String> {
    let nodes = raw["UnlockedBy"]
        .as_array()
        .ok_or("variant UnlockedBy must be an array")?;
    let visibility = raw
        .get("VisibleForAscendancy")
        .ok_or("missing variant VisibleForAscendancy")?;
    // PassiveNodeVariant has no unlock fields. Do not waive conditional context
    // merely because the containing graph node can represent these identities.
    if !nodes.is_empty() || !visibility.is_null() {
        return Err(
            "variant unlock context cannot be represented by the ordinary variant DTO".into(),
        );
    }
    Ok(())
}

fn index(value: &Value) -> Result<usize, String> {
    value
        .as_u64()
        .and_then(|v| usize::try_from(v).ok())
        .ok_or("invalid unlock physical reference".into())
}
