//! Client definitions/topology plus field-scoped official coordinates.

use std::collections::BTreeSet;

use pobr_data::catalog::{PassiveClass, PassiveNodeDef, PassiveNodeKind, PassiveTreeMeta};
use pobr_data::native_stat::{
    NativePassiveContext, NativePassiveStats, NativeStat, NativeStatProvenance, NativeStatReference,
};
use serde_json::{Value, json};

use super::super::{
    model::Candidate,
    trust::{VerifiedInput, require},
};
use super::inputs::{index, row, text};

pub(super) struct Projection {
    pub nodes: Vec<PassiveNodeDef>,
    pub meta: PassiveTreeMeta,
    pub match_evidence: Value,
}

pub(super) fn project(
    input: &VerifiedInput,
    candidate: &Candidate,
    official: &Value,
    source_sha: &str,
    patch: &str,
) -> Result<Projection, String> {
    require(
        official["tree"] == "Default",
        "official supplement is not the Default tree",
    )?;
    let official_nodes = official["nodes"]
        .as_object()
        .ok_or("missing official nodes")?;
    let expected_ids = candidate
        .instances
        .iter()
        .map(|node| node.graph_id)
        .collect::<BTreeSet<_>>();
    let mut supplied_ids = BTreeSet::new();
    require(
        official_nodes.contains_key("root"),
        "official root connector is required",
    )?;
    for (key, node) in official_nodes {
        if key == "root" {
            require(
                node.get("skill").is_none() && node.get("id").is_none(),
                "official root must be a non-allocatable connector",
            )?;
            let roots = node["out"]
                .as_array()
                .ok_or("official root connections missing")?
                .iter()
                .map(|v| text(v)?.parse::<u64>().map_err(|e| e.to_string()))
                .collect::<Result<Vec<_>, _>>()?;
            require(
                roots
                    == candidate
                        .graph
                        .roots
                        .iter()
                        .map(|root| root.graph_id)
                        .collect::<Vec<_>>(),
                "official root links differ from native roots",
            )?;
            continue;
        }
        let id = node["skill"]
            .as_u64()
            .ok_or("official node lacks numeric skill")?;
        require(
            key == &id.to_string() && supplied_ids.insert(id),
            "official node id alias/duplicate",
        )?;
    }
    require(
        expected_ids == supplied_ids,
        "official supplement graph IDs differ from native graph",
    )?;
    let official_groups = official["groups"]
        .as_object()
        .ok_or("missing official groups")?;
    require(
        official_groups.len() == candidate.graph.groups.len(),
        "official group count mismatch",
    )?;
    let mut max_group_rounding_delta = 0.0f64;
    for (index, group) in candidate.graph.groups.iter().enumerate() {
        let raw = official_groups
            .get(&(index + 1).to_string())
            .ok_or("official group key mismatch")?;
        for (field, source) in [("x", group.x_raw), ("y", group.y_raw)] {
            let rounded = number(&raw[field])?;
            let delta = (rounded - source).abs();
            require(
                rounded == export_rounding(source)?,
                "official group centre does not match six-significant-digit client coordinates",
            )?;
            max_group_rounding_delta = max_group_rounding_delta.max(delta);
        }
        let members = raw["nodes"]
            .as_array()
            .ok_or("official group lacks members")?
            .iter()
            .map(|v| text(v)?.parse::<u64>().map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        require(
            members == group.node_order,
            "official group member order mismatch",
        )?;
    }
    // Check all class relations, including definitions absent from the Default
    // graph. Do not turn those absent bases into phantom graph nodes.
    let mut declared_pairs = BTreeSet::new();
    for class in official["classes"]
        .as_array()
        .ok_or("official classes missing")?
    {
        let name = text(&class["name"])?;
        match &class["overridePairs"] {
            Value::Null if class.get("overridePairs").is_none() => {}
            Value::Object(pairs) => {
                for (base, replacement) in pairs {
                    let base = base.parse::<u64>().map_err(|e| e.to_string())?;
                    let replacement = replacement.as_u64().ok_or("invalid official replacement")?;
                    declared_pairs.insert((name.clone(), base, replacement));
                }
            }
            Value::Array(empty) if empty.is_empty() => {}
            _ => return Err("invalid official class pairs".into()),
        }
    }
    let mut expected_pairs = BTreeSet::new();
    for relation in &candidate.class_relations {
        let character_row = pointer_row(&relation.raw_character.pointer)?;
        let character = row(input, "Characters", character_row)?;
        expected_pairs.insert((
            text(&character["Name"])?,
            relation.base_graph_id,
            relation.replacement_graph_id,
        ));
    }
    require(
        expected_pairs == declared_pairs,
        "official class relations differ from native archive",
    )?;
    let mut nodes = Vec::new();
    for instance in &candidate.instances {
        let supplement = &official_nodes[&instance.graph_id.to_string()];
        let definition = &candidate.passives[&instance.definition_id];
        let definition_row = pointer_row(&definition.raw.pointer)?;
        let raw = row(input, "PassiveSkills", definition_row)?;
        if let Some(id) = supplement.get("id").filter(|id| !id.is_null()) {
            require(
                id == &instance.definition_id,
                "official node definition mismatch",
            )?;
        }
        require(
            supplement["group"] == instance.group + 1
                && supplement["orbit"] == instance.radius_raw
                && supplement["orbitIndex"] == instance.position_raw,
            "official node orbit/group mismatch",
        )?;
        let outgoing = supplement["out"]
            .as_array()
            .ok_or("official node has no out array")?
            .iter()
            .map(|v| text(v)?.parse::<u64>().map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        require(
            outgoing == instance.connections,
            "official directed edges differ from native graph",
        )?;
        let native_stats = if instance.definition_id == "maximum_resistances1" {
            Some(packet(
                input,
                candidate,
                instance.graph_id,
                definition_row,
                source_sha,
                patch,
            )?)
        } else {
            None
        };
        nodes.push(PassiveNodeDef {
            skill: u32::try_from(instance.graph_id).map_err(|e| e.to_string())?,
            id: instance.definition_id.clone(),
            name: Some(text(&raw["Name"])?),
            kind: kind(raw)?,
            stats: vec![],
            native_stats,
            group: Some(u32::try_from(instance.group + 1).map_err(|e| e.to_string())?),
            orbit: Some(u32::try_from(instance.radius_raw).map_err(|e| e.to_string())?),
            orbit_index: Some(u32::try_from(instance.position_raw).map_err(|e| e.to_string())?),
            x: Some(number(&supplement["x"])?),
            y: Some(number(&supplement["y"])?),
            connections: instance
                .connections
                .iter()
                .map(|v| u32::try_from(*v).map_err(|e| e.to_string()))
                .collect::<Result<_, _>>()?,
            ascendancy_id: None,
            unlock_constraint: None,
            variants: vec![],
            apply_to_armour: false,
        });
    }
    nodes.sort_by_key(|node| node.skill);
    require(
        nodes
            .iter()
            .filter(|node| node.native_stats.is_some())
            .count()
            == 1,
        "missing selected ordinary passive",
    )?;
    let (_, character) =
        super::inputs::find(input, "Characters", "Metadata/Characters/Int/IntFourb")?;
    let attribute = |key| {
        character[key]
            .as_i64()
            .and_then(|v| i32::try_from(v).ok())
            .ok_or("invalid character attribute".to_owned())
    };
    let meta = PassiveTreeMeta {
        tree: candidate.graph.tree_id.clone(),
        classes: vec![PassiveClass {
            name: text(&character["Name"])?,
            base_str: attribute("BaseStrength")?,
            base_dex: attribute("BaseDexterity")?,
            base_int: attribute("BaseIntelligence")?,
            ascendancies: vec![],
        }],
    };
    Ok(Projection {
        nodes,
        meta,
        match_evidence: json!({
            "format":"pobr-native-tree-coordinate-match/v1","native_manifest_sha256":source_sha,
            "graph_ids":expected_ids.len(),"directed_edges":"exact ordered equality", "groups":official_groups.len(),
            "group_centres":"exact client centres rounded to six significant decimal digits; observed export formatting, not radius semantics", "max_group_rounding_delta":max_group_rounding_delta,
            "class_pairs":expected_pairs.len(),"official_patch":null,"borrowed_fields":["x","y"],
            "official_nodes_without_slug":official_nodes.values().filter(|node|node.get("skill").is_some() && node["id"].is_null()).count(),
            "limits":"Coordinates match this graph; radius semantics, exact official export patch and gameplay use are not certified."
        }),
    })
}

fn packet(
    input: &VerifiedInput,
    candidate: &Candidate,
    graph_id: u64,
    definition_row: usize,
    source_sha: &str,
    patch: &str,
) -> Result<NativePassiveStats, String> {
    let passive = &candidate.passives["maximum_resistances1"];
    let raw = row(input, "PassiveSkills", definition_row)?;
    require(
        passive.stats.len() == 1 && passive.stats[0].stat_id == "base_fire_damage_resistance_%",
        "selected passive is outside the owned one-stat rule",
    )?;
    for key in [
        "Ascendancy",
        "VisibleForAscendancy",
        "Requirement",
        "GrantedSkill",
        "MasteryGroup",
        "VariantType",
    ] {
        require(
            raw[key].is_null(),
            &format!("selected ordinary passive has {key}"),
        )?;
    }
    for key in [
        "Characters",
        "UnlockedBy",
        "PassiveSkillBuffs",
        "Keys0",
        "Keys1",
    ] {
        require(
            raw[key] == json!([]),
            &format!("selected ordinary passive has {key}"),
        )?;
    }
    for key in [
        "IsKeystone",
        "IsNotable",
        "IsJewelSocket",
        "IsAscendancyStartingNode",
        "IsMultipleChoice",
        "IsMultipleChoiceOption",
        "IsAttribute",
        "IsProxyPassive",
        "IsAnointmentOnly",
        "IsExpansion",
        "IsFree",
        "IsJustIcon",
        "IsRootOfAtlasTree",
        "Flag0",
        "Flag1",
        "Flag2",
        "Flag3",
        "Flag4",
        "Flag5",
        "Flag6",
        "Flag7",
    ] {
        require(
            raw[key] == false,
            &format!("selected ordinary passive has {key}"),
        )?;
    }
    for key in ["SkillPointsGranted", "WeaponPointsGranted"] {
        require(
            raw[key] == 0,
            &format!("selected ordinary passive has {key}"),
        )?;
    }
    let stat = &passive.stats[0];
    let stat_row = pointer_row(&stat.raw_stat.pointer)?;
    let reference = |resource: &str, pointer: String| -> Result<NativeStatReference, String> {
        Ok(NativeStatReference {
            resource: resource.into(),
            sha256: text(&input.manifest["files"][resource]["sha256"])?,
            pointer,
        })
    };
    Ok(NativePassiveStats {
        data_version: patch.into(),
        tree_version: Some("0_5".into()),
        node_id: u32::try_from(graph_id).map_err(|e| e.to_string())?,
        node_slug: "maximum_resistances1".into(),
        modifier_texts: vec![],
        context: NativePassiveContext::UnconditionalPlayer,
        stats: vec![NativeStat {
            id: stat.stat_id.clone(),
            value: stat.value,
            provenance: NativeStatProvenance {
                game_patch: patch.into(),
                source_manifest_sha256: source_sha.into(),
                passive_id: "maximum_resistances1".into(),
                passive_row: u32::try_from(definition_row).map_err(|e| e.to_string())?,
                stat_row: u32::try_from(stat_row).map_err(|e| e.to_string())?,
                references: vec![
                    reference(
                        "tables/PassiveSkills.json",
                        format!("/rows/{definition_row}/values/{}", stat.value_field),
                    )?,
                    reference("tables/Stats.json", format!("/rows/{stat_row}/values/Id"))?,
                ],
            },
        }],
    })
}

fn kind(raw: &Value) -> Result<PassiveNodeKind, String> {
    for (field, kind) in [
        ("IsKeystone", PassiveNodeKind::Keystone),
        ("IsNotable", PassiveNodeKind::Notable),
        ("IsJewelSocket", PassiveNodeKind::JewelSocket),
        ("IsAscendancyStartingNode", PassiveNodeKind::AscendancyStart),
    ] {
        if raw[field].as_bool().ok_or("missing passive kind flag")? {
            return Ok(kind);
        }
    }
    Ok(PassiveNodeKind::Normal)
}

fn pointer_row(pointer: &str) -> Result<usize, String> {
    let parts = pointer.split('/').collect::<Vec<_>>();
    require(
        parts.len() == 4 && parts[0].is_empty() && parts[1] == "rows" && parts[3] == "values",
        "invalid physical row reference",
    )?;
    index(&json!(parts[2].parse::<u64>().map_err(|e| e.to_string())?))
}

fn number(value: &Value) -> Result<f64, String> {
    value
        .as_f64()
        .filter(|v| v.is_finite())
        .ok_or("missing finite coordinate".into())
}

pub(super) fn export_rounding(value: f64) -> Result<f64, String> {
    if value == 0.0 {
        return Ok(0.0);
    }
    let places = 5 - value.abs().log10().floor() as i32;
    require(
        (0..=12).contains(&places),
        "unsupported group coordinate magnitude",
    )?;
    format!("{value:.precision$}", precision = places as usize)
        .parse::<f64>()
        .map_err(|e| e.to_string())
}
