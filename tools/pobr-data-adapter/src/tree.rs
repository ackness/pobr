//! Passive tree domain adapter: GGG's official tree export `data.json` -> PoBR's minimal tree JSON.
//!
//! Source: `github.com/grindinggear/poe2-skilltree-export`. Coordinates are
//! copied directly in tree units; class/ascendancy `overridePairs` resolve
//! `skillOverrides` into native variants without changing base allocation IDs.
//! Assets/atlases are omitted. Output is sorted by skill ID, connections and
//! variant selector; required references are validated before either file is written.
//!
//! The raw `data.json` is large (several MB) and lives at a gitignored path — it isn't checked in.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use pobr_data::catalog::{
    PassiveAscendancy, PassiveClass, PassiveNodeDef, PassiveNodeKind, PassiveNodeVariant,
    PassiveTreeMeta, PassiveUnlockConstraint,
};
use serde::Deserialize;

use crate::write_pretty;

pub struct TreeArgs {
    pub data_json: PathBuf,
    pub out: PathBuf,
    pub patch: String,
}

// Raw data.json structure (only the fields we need)

#[derive(Deserialize)]
struct RawTree {
    #[serde(default)]
    tree: Option<String>,
    classes: Vec<RawClass>,
    nodes: BTreeMap<String, RawNode>,
    #[serde(rename = "skillOverrides", default)]
    skill_overrides: BTreeMap<u32, RawSkillOverride>,
}

#[derive(Deserialize)]
struct RawClass {
    name: String,
    #[serde(default)]
    base_str: i32,
    #[serde(default)]
    base_dex: i32,
    #[serde(default)]
    base_int: i32,
    #[serde(default)]
    ascendancies: Vec<RawAscendancy>,
    #[serde(rename = "overridePairs", default, deserialize_with = "override_pairs")]
    override_pairs: BTreeMap<String, u32>,
}

#[derive(Deserialize)]
struct RawAscendancy {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(rename = "overridePairs", default, deserialize_with = "override_pairs")]
    override_pairs: BTreeMap<String, u32>,
}

// GGG emits an object for populated pairs, but [] for empty pairs. Accept only
// the empty array, rather than treating malformed nonempty arrays as no overrides.
fn override_pairs<'de, D>(deserializer: D) -> Result<BTreeMap<String, u32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Pairs {
        Map(BTreeMap<String, u32>),
        Empty([(); 0]),
    }
    Ok(match Pairs::deserialize(deserializer)? {
        Pairs::Map(pairs) => pairs,
        Pairs::Empty([]) => BTreeMap::new(),
    })
}

#[derive(Deserialize)]
struct RawSkillOverride {
    id: String,
    skill: u32,
    name: String,
    // Empty stats explicitly remove the base stats; absent stats are malformed.
    stats: Vec<String>,
}

#[derive(Deserialize)]
struct RawNode {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    skill: Option<u32>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    stats: Vec<String>,
    #[serde(default)]
    group: Option<u32>,
    #[serde(default)]
    orbit: Option<u32>,
    #[serde(rename = "orbitIndex", default)]
    orbit_index: Option<u32>,
    #[serde(default)]
    x: Option<f64>,
    #[serde(default)]
    y: Option<f64>,
    #[serde(default)]
    out: Vec<String>,
    #[serde(rename = "ascendancyId", default)]
    ascendancy_id: Option<String>,
    #[serde(rename = "unlockConstraint", default)]
    unlock_constraint: Option<PassiveUnlockConstraint>,
    #[serde(rename = "isNotable", default)]
    is_notable: bool,
    #[serde(rename = "isKeystone", default)]
    is_keystone: bool,
    #[serde(rename = "isMastery", default)]
    is_mastery: bool,
    #[serde(rename = "isJewelSocket", default)]
    is_jewel_socket: bool,
    #[serde(rename = "isAscendancyStart", default)]
    is_ascendancy_start: bool,
}

fn classify(n: &RawNode) -> PassiveNodeKind {
    // Priority: keystone > notable > mastery > jewel > ascendancy_start > normal.
    if n.is_keystone {
        PassiveNodeKind::Keystone
    } else if n.is_notable {
        PassiveNodeKind::Notable
    } else if n.is_mastery {
        PassiveNodeKind::Mastery
    } else if n.is_jewel_socket {
        PassiveNodeKind::JewelSocket
    } else if n.is_ascendancy_start {
        PassiveNodeKind::AscendancyStart
    } else {
        PassiveNodeKind::Normal
    }
}

/// Resolves an outgoing edge's string node key into a target `skill` id (via the raw nodes table's key -> skill mapping).
fn resolve_connections(
    key: &str,
    out: &[String],
    skill_by_key: &BTreeMap<&str, u32>,
    raw_nodes: &BTreeMap<String, RawNode>,
) -> Result<Vec<u32>, String> {
    let mut conns = Vec::new();
    for target in out {
        if let Some(skill) = skill_by_key.get(target.as_str()) {
            conns.push(*skill);
        } else if !raw_nodes.contains_key(target) {
            return Err(format!(
                "node {key}: out references unknown node key {target}"
            ));
        }
        // Known, nameless/statless nodes without an id are layout placeholders.
        // Some have a numeric skill; only the emitted-node index may form edges.
    }
    conns.sort_unstable();
    conns.dedup();
    Ok(conns)
}

fn adapt(raw: RawTree) -> Result<(Vec<PassiveNodeDef>, PassiveTreeMeta), String> {
    let mut skill_by_key = BTreeMap::new();
    let mut nodes = BTreeMap::new();
    for (key, n) in &raw.nodes {
        let (skill, id) = match (n.skill, n.id.as_deref()) {
            (_, None) if n.name.as_deref().is_none_or(str::is_empty) && n.stats.is_empty() => {
                continue;
            }
            (Some(skill), Some(id)) if !id.is_empty() => (skill, id),
            _ => return Err(format!("node {key}: requires both skill and a nonempty id")),
        };
        match (n.x, n.y) {
            (None, None) => {} // Older exports can still use the separate coordinate backfill.
            (Some(x), Some(y)) if x.is_finite() && y.is_finite() => {}
            _ => {
                return Err(format!(
                    "node {key}: x and y must be a finite coordinate pair"
                ));
            }
        }
        if nodes.contains_key(&skill) {
            return Err(format!("node {key}: duplicate skill {skill}"));
        }
        skill_by_key.insert(key.as_str(), skill);
        nodes.insert(
            skill,
            PassiveNodeDef {
                skill,
                id: id.to_owned(),
                name: n.name.clone(),
                kind: classify(n),
                stats: n.stats.clone(),
                group: n.group,
                orbit: n.orbit,
                orbit_index: n.orbit_index,
                x: n.x,
                y: n.y,
                connections: Vec::new(),
                ascendancy_id: n.ascendancy_id.clone(),
                unlock_constraint: n.unlock_constraint.clone(),
                variants: Vec::new(),
                apply_to_armour: false,
            },
        );
    }

    let skills: BTreeSet<_> = nodes.keys().copied().collect();
    let ascendancies: BTreeSet<_> = raw
        .classes
        .iter()
        .flat_map(|c| &c.ascendancies)
        .filter(|a| a.name.as_ref().is_some_and(|name| !name.is_empty()))
        .filter_map(|a| a.id.as_deref())
        .filter(|id| !id.is_empty())
        .collect();
    for (key, skill) in &skill_by_key {
        let node = nodes
            .get_mut(skill)
            .expect("indexed skills have an emitted node");
        node.connections =
            resolve_connections(key, &raw.nodes[*key].out, &skill_by_key, &raw.nodes)?;
        if let Some(constraint) = &mut node.unlock_constraint {
            for required in &constraint.nodes {
                if !skills.contains(required) {
                    return Err(format!(
                        "node {key}: unlockConstraint references unknown skill {required}"
                    ));
                }
            }
            if let Some(ascendancy) = &constraint.ascendancy
                && !ascendancies.contains(ascendancy.as_str())
            {
                return Err(format!(
                    "node {key}: unlockConstraint references unavailable ascendancy {ascendancy}"
                ));
            }
            constraint.nodes.sort_unstable();
            constraint.nodes.dedup();
        }
    }
    for class in &raw.classes {
        add_variants(
            &class.name,
            &class.override_pairs,
            &raw.skill_overrides,
            &skill_by_key,
            &mut nodes,
        )?;
        for ascendancy in &class.ascendancies {
            if ascendancy.override_pairs.is_empty() {
                continue;
            }
            let (Some(id), Some(name)) = (&ascendancy.id, &ascendancy.name) else {
                return Err(format!(
                    "class {}: placeholder ascendancy has overridePairs",
                    class.name
                ));
            };
            if id.is_empty() || name.is_empty() {
                return Err(format!(
                    "class {}: placeholder ascendancy has overridePairs",
                    class.name
                ));
            }
            add_variants(
                name,
                &ascendancy.override_pairs,
                &raw.skill_overrides,
                &skill_by_key,
                &mut nodes,
            )?;
        }
    }
    for node in nodes.values_mut() {
        node.variants.sort_by(|a, b| a.class.cmp(&b.class));
        if let Some(pair) = node.variants.windows(2).find(|v| v[0].class == v[1].class) {
            return Err(format!(
                "node {}: duplicate variant selector {}",
                node.skill, pair[0].class
            ));
        }
    }

    let mut classes: Vec<PassiveClass> = raw
        .classes
        .into_iter()
        .map(|c| PassiveClass {
            name: c.name,
            base_str: c.base_str,
            base_dex: c.base_dex,
            base_int: c.base_int,
            ascendancies: c
                .ascendancies
                .into_iter()
                // Filter out nameless / idless placeholder ascendancy slots (GGG's export includes null placeholders).
                .filter_map(|a| match (a.id, a.name) {
                    (Some(id), Some(name)) if !id.is_empty() && !name.is_empty() => {
                        Some(PassiveAscendancy { id, name })
                    }
                    _ => None,
                })
                .collect(),
        })
        .collect();
    classes.sort_by(|a, b| a.name.cmp(&b.name));

    let meta = PassiveTreeMeta {
        tree: raw.tree.unwrap_or_else(|| "Default".into()),
        classes,
    };
    Ok((nodes.into_values().collect(), meta))
}

fn add_variants(
    selector: &str,
    pairs: &BTreeMap<String, u32>,
    overrides: &BTreeMap<u32, RawSkillOverride>,
    skill_by_key: &BTreeMap<&str, u32>,
    nodes: &mut BTreeMap<u32, PassiveNodeDef>,
) -> Result<(), String> {
    for (base, alias) in pairs {
        if selector.is_empty() {
            return Err("overridePairs require a nonempty class/ascendancy name".into());
        }
        let skill = skill_by_key.get(base.as_str()).ok_or_else(|| {
            format!("{selector}: overridePairs base {base} is missing or filtered")
        })?;
        let replacement = overrides.get(alias).ok_or_else(|| {
            format!("{selector}: overridePairs {base} -> {alias} has no skillOverrides entry")
        })?;
        if replacement.skill != *alias || replacement.id.is_empty() || replacement.name.is_empty() {
            return Err(format!(
                "{selector}: skillOverrides {alias} requires matching skill and nonempty id/name"
            ));
        }
        if nodes.contains_key(alias) {
            return Err(format!(
                "{selector}: skillOverrides alias {alias} collides with an emitted node"
            ));
        }
        nodes
            .get_mut(skill)
            .expect("indexed override base has an emitted node")
            .variants
            .push(PassiveNodeVariant {
                class: selector.to_owned(),
                variant_skill: Some(*alias),
                name: Some(replacement.name.clone()),
                stats: replacement.stats.clone(),
            });
    }
    Ok(())
}

pub fn run(args: TreeArgs) -> Result<String, String> {
    let bytes = std::fs::read(&args.data_json)
        .map_err(|e| format!("failed to read {}: {e}", args.data_json.display()))?;
    let raw_mb = bytes.len() as f64 / (1024.0 * 1024.0);
    let raw: RawTree = serde_json::from_slice(&bytes)
        .map_err(|e| format!("failed to parse {}: {e}", args.data_json.display()))?;
    let total = raw.nodes.len();
    let (nodes, meta) = adapt(raw)?;

    // The three-layer layout: passive tree domain JSON goes in the base/ layer.
    let base_dir = args.out.join(&args.patch).join("base");
    std::fs::create_dir_all(&base_dir)
        .map_err(|e| format!("failed to create output directory: {e}"))?;
    write_pretty(&base_dir.join("passive_tree.json"), &nodes)?;
    write_pretty(&base_dir.join("passive_tree_meta.json"), &meta)?;

    Ok(format!(
        "passive tree adaptation complete: {}/{} node(s) (skipped {} layout placeholder(s)), {} class(es) -> {} (source {:.1}MB)",
        nodes.len(),
        total,
        total - nodes.len(),
        meta.classes.len(),
        base_dir.display(),
        raw_mb,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pobr_data::passive_tree::{NodeId, PassiveTreeSpec};
    use pobr_tree::allocation::{Allocation, AllocationTree, EditAction};
    use pobr_tree::{ClassContext, JewelRadius, PassiveTree, collect_allocated_mods_for_class};
    use serde_json::{Value, json};

    // Synthetic, deliberately small graph using the shapes observed in GGG's
    // poe2-skilltree-export at bd87e6512c92b868542eddfb1ba4ea8b6dc2da36 (0.5.5).
    // The IDs, text and geometry below are test data, not an official tree excerpt.
    fn fixture() -> Value {
        json!({
            "classes": [
                {"name": "Witch", "base_int": 15, "overridePairs": {"20": 210},
                 "ascendancies": [{"id": "Witch3b", "name": "Abyssal Lich",
                                   "overridePairs": {"20": 230}}]},
                {"name": "Druid", "overridePairs": {"20": 220}, "ascendancies": [
                    {"id": "Druid1", "name": "Oracle", "overridePairs": []},
                    {"id": "Druid3", "name": null, "overridePairs": []}
                ]}
            ],
            "nodes": {
                "root": {"out": ["10"]},
                "10": {"id": "socket", "skill": 10, "isJewelSocket": true,
                       "x": -100, "y": 200, "out": ["50", "root", "20", "20", "70"]},
                "20": {"id": "base", "skill": 20, "name": "Base", "isNotable": true,
                       "stats": ["+10 to Strength"], "group": 7, "orbit": 2, "orbitIndex": 3,
                       "x": -97, "y": 204, "out": ["30"]},
                "30": {"id": "outside", "skill": 30, "x": -96.99, "y": 204},
                "40": {"id": "prerequisite", "skill": 40, "ascendancyId": "Druid1",
                       "x": -500, "y": -100},
                "50": {"id": "locked", "skill": 50, "x": -100, "y": 201,
                       "unlockConstraint": {"nodes": [40], "ascendancy": "Druid1"}},
                "60": {"id": "unplaced", "skill": 60},
                "70": {"id": null, "skill": 70, "name": "", "stats": [],
                       "x": -100, "y": 200, "out": ["root"]}
            },
            "skillOverrides": {
                "210": {"id": "class_override", "skill": 210, "name": "Class override",
                        "stats": ["+20 to [Strength]"]},
                "220": {"id": "empty_override", "skill": 220, "name": "Empty override", "stats": []},
                "230": {"id": "ascendancy_override", "skill": 230, "name": "Ascendancy override",
                        "ascendancyId": "Witch3b", "stats": ["+30 to Dexterity"]}
            }
        })
    }

    fn convert(value: Value) -> Result<(Vec<PassiveNodeDef>, PassiveTreeMeta), String> {
        let raw = serde_json::from_value(value).map_err(|e| e.to_string())?;
        adapt(raw)
    }

    #[test]
    fn official_coordinates_drive_native_distance_and_radius_boundaries() {
        let (nodes, _) = convert(fixture()).unwrap();
        // Exercise the serialized schema and the position injection used by callers.
        let nodes: Vec<PassiveNodeDef> =
            serde_json::from_slice(&serde_json::to_vec(&nodes).unwrap()).unwrap();
        assert_eq!((nodes[1].x, nodes[1].y), (Some(-97.0), Some(204.0)));
        assert_eq!(
            (nodes[1].group, nodes[1].orbit, nodes[1].orbit_index),
            (Some(7), Some(2), Some(3))
        );
        let positions = nodes
            .iter()
            .filter_map(|n| n.x.zip(n.y).map(|p| (n.skill, p)))
            .collect();
        let tree = PassiveTree::from_nodes(nodes).with_positions(positions);
        for (radius, expected) in [
            (4.999, vec![50]),
            (5.0, vec![20, 50]),
            (5.01, vec![20, 30, 50]),
        ] {
            let affected: Vec<_> = tree
                .nodes_in_radius(NodeId(10), radius)
                .into_iter()
                .map(|n| n.0)
                .collect();
            assert_eq!(affected, expected, "radius {radius}");
            let effect = tree
                .radius_jewel_effect(NodeId(10), JewelRadius::Custom(radius), vec![])
                .unwrap();
            assert_eq!(effect.affected_nodes, expected, "jewel radius {radius}");
        }
        assert!(
            tree.radius_jewel_effect(NodeId(60), JewelRadius::Custom(5.0), vec![])
                .is_err()
        );
    }

    #[test]
    fn official_class_and_ascendancy_overrides_replace_stats_at_the_base_identity() {
        let (nodes, meta) = convert(fixture()).unwrap();
        assert_eq!(
            nodes.iter().map(|n| n.skill).collect::<Vec<_>>(),
            [10, 20, 30, 40, 50, 60]
        );
        assert_eq!(nodes[0].connections, [20, 50]);
        assert_eq!(
            nodes[1]
                .variants
                .iter()
                .map(|v| (v.class.as_str(), v.variant_skill))
                .collect::<Vec<_>>(),
            [
                ("Abyssal Lich", Some(230)),
                ("Druid", Some(220)),
                ("Witch", Some(210))
            ]
        );
        assert_eq!(nodes[1].variants[2].name.as_deref(), Some("Class override"));
        assert_eq!(
            meta.classes[0].ascendancies.len(),
            1,
            "placeholder slots stay filtered"
        );
        let tree = PassiveTree::from_nodes(nodes);
        let spec = PassiveTreeSpec {
            allocated_nodes: vec![NodeId(20)],
            ..Default::default()
        };
        for (class_name, ascendancy_name, expected) in [
            ("Sorceress", "", vec!["+10 to Strength"]),
            ("Witch", "Abyssal Lich", vec!["+20 to [Strength]"]),
            ("Sorceress", "Abyssal Lich", vec!["+30 to Dexterity"]),
            ("Druid", "", vec![]),
        ] {
            let mods = collect_allocated_mods_for_class(
                &spec,
                &tree.nodes,
                ClassContext {
                    class_name,
                    ascendancy_name,
                },
            );
            assert_eq!(
                mods.iter()
                    .flat_map(|m| m.modifier_texts.iter().map(String::as_str))
                    .collect::<Vec<_>>(),
                expected
            );
            assert!(
                mods.iter().all(|m| m.node_id == NodeId(20)),
                "aliases never replace allocation/source IDs"
            );
        }
    }

    #[test]
    fn official_unlock_requirements_gate_native_allocation() {
        let (nodes, _) = convert(fixture()).unwrap();
        let mut tree = AllocationTree::new(nodes.iter());
        tree.roots = [10, 40].into();
        tree.ascendancy = Some("Druid1".into());
        tree.points_max = 10;
        assert!(
            tree.edit(&Allocation::default(), 50, 0, EditAction::Allocate)
                .is_err()
        );
        let before = Allocation {
            shared: [40].into(),
            ..Default::default()
        };
        assert!(
            tree.edit(&before, 50, 0, EditAction::Allocate)
                .unwrap()
                .allocation
                .shared
                .contains(&50)
        );
        tree.ascendancy = Some("Witch3b".into());
        assert!(tree.edit(&before, 50, 0, EditAction::Allocate).is_err());
    }

    #[test]
    fn legacy_exports_keep_unplaced_nodes_and_existing_fields() {
        let (nodes, meta) = convert(json!({"classes": [{"name": "Legacy"}], "nodes": {
            "root": {"out": ["base-key"]},
            "base-key": {"id": "legacy", "skill": 7, "out": ["target-key", "root"],
                         "stats": ["original text"], "ascendancyId": "Legacy1"},
            "target-key": {"id": "target", "skill": 8}
        }}))
        .unwrap();
        assert_eq!(meta.tree, "Default");
        assert_eq!(nodes.len(), 2);
        assert_eq!(
            nodes[0].connections,
            [8],
            "connections resolve map keys, not parsed numbers"
        );
        assert_eq!(nodes[0].stats, ["original text"]);
        assert_eq!(nodes[0].ascendancy_id.as_deref(), Some("Legacy1"));
        assert_eq!((nodes[0].x, nodes[0].y), (None, None));
        assert!(nodes[0].variants.is_empty());
        assert!(!nodes[0].apply_to_armour);
    }

    #[test]
    fn aliases_with_unchanged_stats_are_preserved_without_extra_nodes() {
        let mut source = fixture();
        source["skillOverrides"]["230"]["stats"] = source["nodes"]["20"]["stats"].clone();
        let (nodes, _) = convert(source).unwrap();
        assert_eq!(nodes.len(), 6);
        assert_eq!(nodes[1].variants[0].variant_skill, Some(230));
        assert_eq!(nodes[1].variants[0].stats, nodes[1].stats);
    }

    #[test]
    fn rejects_malformed_and_unresolved_required_references() {
        for (pointer, value, expected) in [
            ("/nodes/20/id", json!(""), "requires both skill"),
            ("/nodes/20/id", Value::Null, "requires both skill"),
            ("/nodes/20/skill", Value::Null, "requires both skill"),
            ("/nodes/30/skill", json!(20), "duplicate skill 20"),
            ("/nodes/20/out", json!(["999"]), "unknown node key 999"),
            (
                "/classes/0/overridePairs",
                json!({"root": 210}),
                "base root is missing or filtered",
            ),
            (
                "/classes/0/overridePairs",
                json!({"70": 210}),
                "base 70 is missing or filtered",
            ),
            (
                "/classes/0/overridePairs",
                json!({"999": 210}),
                "base 999 is missing or filtered",
            ),
            (
                "/classes/0/overridePairs",
                json!({"20": 999}),
                "has no skillOverrides entry",
            ),
            (
                "/skillOverrides/210/skill",
                json!(211),
                "requires matching skill",
            ),
            ("/skillOverrides/210/id", json!(""), "nonempty id/name"),
            (
                "/classes/1/ascendancies/1/overridePairs",
                json!({"20": 210}),
                "placeholder ascendancy has overridePairs",
            ),
            (
                "/nodes/50/unlockConstraint/nodes",
                json!([999]),
                "unknown skill 999",
            ),
            (
                "/nodes/50/unlockConstraint/nodes",
                json!([70]),
                "unknown skill 70",
            ),
            (
                "/nodes/50/unlockConstraint/ascendancy",
                json!("Druid3"),
                "unavailable ascendancy Druid3",
            ),
            (
                "/classes/0/ascendancies/0/name",
                json!("Witch"),
                "duplicate variant selector Witch",
            ),
        ] {
            let mut source = fixture();
            *source.pointer_mut(pointer).unwrap() = value;
            let error = convert(source).unwrap_err();
            assert!(
                error.contains(expected),
                "{pointer}: {error}; expected {expected}"
            );
        }
        let mut source = fixture();
        source["skillOverrides"]["30"] =
            json!({"id": "collision", "skill": 30, "name": "Collision", "stats": []});
        source["classes"][0]["overridePairs"] = json!({"20": 30});
        assert!(convert(source).unwrap_err().contains("alias 30 collides"));
    }

    #[test]
    fn override_pairs_accept_only_maps_or_empty_arrays_and_require_stats() {
        for pairs in [
            Value::Null,
            json!([1]),
            json!([[20, 210]]),
            json!({"20": "210"}),
        ] {
            let mut source = fixture();
            source["classes"][0]["overridePairs"] = pairs;
            assert!(convert(source).is_err());
        }
        let mut source = fixture();
        source["skillOverrides"]["210"]
            .as_object_mut()
            .unwrap()
            .remove("stats");
        assert!(
            convert(source)
                .unwrap_err()
                .contains("missing field `stats`")
        );
        for empty in [json!({}), json!([])] {
            let mut source = fixture();
            source["classes"][0]["overridePairs"] = empty;
            assert!(
                convert(source).unwrap().0[1]
                    .variants
                    .iter()
                    .all(|v| v.class != "Witch")
            );
        }
    }

    #[test]
    fn rejects_partial_and_nonfinite_coordinates() {
        for coordinate in ["x", "y"] {
            let mut source = fixture();
            source["nodes"]["20"]
                .as_object_mut()
                .unwrap()
                .remove(coordinate);
            assert!(
                convert(source)
                    .unwrap_err()
                    .contains("finite coordinate pair")
            );
        }
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut raw: RawTree = serde_json::from_value(fixture()).unwrap();
            raw.nodes.get_mut("20").unwrap().x = Some(value);
            assert!(adapt(raw).unwrap_err().contains("finite coordinate pair"));
        }
    }

    #[test]
    fn output_order_is_independent_of_class_connections_and_constraint_order() {
        let mut source = fixture();
        source["nodes"]["50"]["unlockConstraint"]["nodes"] = json!([40, 30]);
        let expected = convert(source.clone()).unwrap();
        source["classes"].as_array_mut().unwrap().reverse();
        source["nodes"]["10"]["out"] = json!(["20", "50", "root"]);
        source["nodes"]["50"]["unlockConstraint"]["nodes"] = json!([30, 40, 30]);
        let actual = convert(source).unwrap();
        assert_eq!(
            serde_json::to_vec(&actual).unwrap(),
            serde_json::to_vec(&expected).unwrap()
        );
    }

    #[test]
    fn adapter_writes_native_files_only_after_validation() {
        let root = std::env::temp_dir().join(format!("pobr-tree-adapter-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("source.json");
        let mut input = fixture();
        std::fs::write(&source, serde_json::to_vec(&input).unwrap()).unwrap();
        let args = || TreeArgs {
            data_json: source.clone(),
            out: root.clone(),
            patch: "test".into(),
        };
        run(args()).unwrap();
        let tree_path = root.join("test/base/passive_tree.json");
        let meta_path = root.join("test/base/passive_tree_meta.json");
        let tree_bytes = std::fs::read(&tree_path).unwrap();
        let meta_bytes = std::fs::read(&meta_path).unwrap();
        let output: Vec<PassiveNodeDef> = serde_json::from_slice(&tree_bytes).unwrap();
        assert_eq!(output, convert(input.clone()).unwrap().0);
        run(args()).unwrap();
        assert_eq!(std::fs::read(&tree_path).unwrap(), tree_bytes);
        assert_eq!(std::fs::read(&meta_path).unwrap(), meta_bytes);
        input["nodes"]["20"]["out"] = json!(["missing"]);
        std::fs::write(&source, serde_json::to_vec(&input).unwrap()).unwrap();
        assert!(run(args()).is_err());
        assert_eq!(std::fs::read(&tree_path).unwrap(), tree_bytes);
        assert_eq!(std::fs::read(&meta_path).unwrap(), meta_bytes);
        std::fs::remove_dir_all(root).unwrap();
    }
}
