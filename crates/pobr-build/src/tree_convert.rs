//! Explicit historical-tree conversion. Preview returns a new XML document; it
//! never changes the input or refunds disconnected points without review.
use std::collections::{BTreeMap, BTreeSet, HashMap};

use pobr_data::catalog::{PassiveNodeDef, PassiveNodeKind, PassiveTreeMeta};
use pobr_tree::allocation::{Allocation, AllocationTree, Grant};
use quick_xml::{
    Reader, Writer,
    events::{BytesStart, Event},
};
use serde::Serialize;

use crate::{BuildData, SetSelection, parse_build, select_sets};

#[derive(Debug, Clone, Serialize)]
pub struct ConversionNode {
    pub id: u32,
    pub name: String,
    pub stats: Vec<String>,
    pub reason: Option<String>,
    pub jewel_item_id: Option<u32>,
}
#[derive(Debug, Clone, Serialize)]
pub struct SpecConversion {
    pub index: usize,
    pub title: String,
    pub from_version: String,
    pub to_version: String,
    pub kept: Vec<u32>,
    pub dropped: Vec<ConversionNode>,
    pub disconnected: Vec<ConversionNode>,
    pub disconnected_by_set: [BTreeSet<u32>; 3],
    pub allocation: Allocation,
}
#[derive(Debug, Clone, Serialize)]
pub struct TreeConversion {
    pub xml: String,
    pub specs: Vec<SpecConversion>,
}

/// A numeric id alone is not sufficient when a league reuses it. Older packs
/// have only numeric slugs: names and kinds identify those nodes. Balance
/// changes to the stats or movement of the node do not change its identity.
fn same_node(old: &PassiveNodeDef, new: &PassiveNodeDef) -> bool {
    if old.kind != new.kind {
        return false;
    }
    if old.id.parse::<u32>().is_err() && new.id.parse::<u32>().is_err() {
        return old.id == new.id;
    }
    old.name
        .as_deref()
        .map(pobr_core::mod_parser::engine::strip_display_markup)
        == new
            .name
            .as_deref()
            .map(pobr_core::mod_parser::engine::strip_display_markup)
        && old.ascendancy_id.is_some() == new.ascendancy_id.is_some()
}
fn attrs(e: &BytesStart<'_>) -> Result<BTreeMap<String, String>, String> {
    e.attributes()
        .map(|attribute| {
            let a = attribute.map_err(|e| e.to_string())?;
            Ok((
                a.key.as_ref().to_string(),
                quick_xml::escape::unescape(&a.value)
                    .map_err(|e| e.to_string())?
                    .into_owned(),
            ))
        })
        .collect()
}

fn ids(value: Option<&String>) -> BTreeSet<u32> {
    value
        .into_iter()
        .flat_map(|s| s.split(','))
        .filter_map(|s| s.trim().parse().ok())
        .collect()
}
fn csv(ids: &BTreeSet<u32>) -> String {
    ids.iter().map(u32::to_string).collect::<Vec<_>>().join(",")
}
fn describe(
    id: u32,
    nodes: &HashMap<u32, PassiveNodeDef>,
    reason: Option<&str>,
    sockets: &BTreeMap<u32, u32>,
) -> ConversionNode {
    let node = nodes.get(&id);
    ConversionNode {
        id,
        name: node
            .and_then(|n| n.name.clone())
            .unwrap_or_else(|| format!("#{id}")),
        stats: node.map(|n| n.stats.clone()).unwrap_or_default(),
        reason: reason.map(str::to_string),
        jewel_item_id: sockets.get(&id).copied(),
    }
}

pub fn preview_conversion(
    xml: &str,
    data: &BuildData,
    meta: &PassiveTreeMeta,
    current: &str,
    indices: &[usize],
) -> Result<TreeConversion, String> {
    parse_build(xml).map_err(|e| e.to_string())?;
    if indices.is_empty() {
        return Err("Select at least one passive spec".into());
    }
    let ranges = spec_ranges(xml)?;
    let mut output = xml.to_string();
    let mut reports = Vec::new();
    let mut replacements = Vec::new();
    for index in indices.iter().copied().collect::<BTreeSet<_>>() {
        if index == 0 {
            return Err("Spec indices are 1-based".into());
        }
        let range = ranges
            .get(index - 1)
            .cloned()
            .ok_or_else(|| format!("Passive spec {index} does not exist"))?;
        let input = &xml[range.clone()];
        let mut reader = Reader::from_str(input);
        let mut header = BTreeMap::new();
        let mut sets = [BTreeSet::new(), BTreeSet::new()];
        let mut sockets = BTreeMap::new();
        loop {
            match reader.read_event().map_err(|e| e.to_string())? {
                Event::Start(e) | Event::Empty(e) => {
                    let a = attrs(&e)?;
                    match e.name().as_ref() {
                        "Spec" => header = a,
                        "WeaponSet1" => sets[0] = ids(a.get("nodes")),
                        "WeaponSet2" => sets[1] = ids(a.get("nodes")),
                        "Socket" => {
                            if let (Some(node), Some(item)) = (
                                a.get("nodeId").and_then(|s| s.parse::<u32>().ok()),
                                a.get("itemId").and_then(|s| s.parse::<u32>().ok()),
                            ) {
                                sockets.insert(node, item);
                            }
                        }
                        _ => {}
                    }
                }
                Event::Eof => break,
                _ => {}
            }
        }
        let version = header
            .get("treeVersion")
            .map(String::as_str)
            .unwrap_or(current);
        let is_current = version == current || version == "Default";
        let old = if is_current {
            &data.passive_nodes
        } else {
            data.versioned_passive_nodes.get(version).ok_or_else(|| format!("Historical tree {version} is unavailable; conversion cannot identify its nodes safely"))?
        };
        let all: BTreeSet<_> = ids(header.get("nodes"))
            .into_iter()
            .chain(sets.iter().flatten().copied())
            .collect();
        let keep = |id: &u32| {
            is_current
                || old
                    .get(id)
                    .zip(data.passive_nodes.get(id))
                    .is_some_and(|(old, new)| same_node(old, new))
        };
        let kept: BTreeSet<_> = all.iter().copied().filter(keep).collect();
        let mut dropped: Vec<_> = all
            .difference(&kept)
            .map(|id| {
                describe(
                    *id,
                    old,
                    Some(if data.passive_nodes.contains_key(id) {
                        "changed_identity"
                    } else {
                        "missing"
                    }),
                    &sockets,
                )
            })
            .collect();
        // An unallocated socket may still hold a saved jewel. Keep surviving
        // sockets; a removed socket's jewel remains in the global item pool.
        for id in sockets.keys().filter(|id| !keep(id) && !all.contains(id)) {
            dropped.push(describe(
                *id,
                old,
                Some("socket_missing_or_changed"),
                &sockets,
            ));
        }
        sets.iter_mut()
            .for_each(|set| set.retain(|id| kept.contains(id)));
        let exclusive: BTreeSet<_> = sets.iter().flatten().copied().collect();
        let allocation = Allocation {
            shared: kept.difference(&exclusive).copied().collect(),
            exclusive: sets,
        };
        let rewritten = if is_current {
            input.to_string()
        } else {
            rewrite_spec(input, current, &kept, &keep)?
        };
        let mut single = xml.to_string();
        single.replace_range(range.clone(), &rewritten);
        let selected = select_sets(
            &single,
            &SetSelection {
                tree: Some(index),
                ..Default::default()
            },
        );
        let build = parse_build(&selected).map_err(|e| e.to_string())?;
        let effects = crate::calc_orchestrator::passive_jewel_state(&build, data);
        let mut tree = AllocationTree::new(data.passive_nodes.values());
        tree.class_starts = effects.class_starts.values().copied().collect();
        let root = effects
            .class_starts
            .get(&build.character.class_name.to_lowercase())
            .ok_or_else(|| "Current class start is unavailable".to_string())?;
        tree.roots.insert(*root);
        tree.ascendancy = meta
            .classes
            .iter()
            .filter(|c| c.name == build.character.class_name)
            .flat_map(|c| &c.ascendancies)
            .find(|a| a.name == build.character.ascendancy_name)
            .map(|a| a.id.clone());
        tree.roots.extend(
            data.passive_nodes
                .values()
                .filter(|n| {
                    n.kind == PassiveNodeKind::AscendancyStart
                        && n.ascendancy_id.is_some()
                        && n.ascendancy_id == tree.ascendancy
                })
                .map(|n| n.skill),
        );
        tree.grants = effects
            .allocation_grants
            .iter()
            .map(|g| Grant {
                source: g.source,
                nodes: g.nodes.clone(),
                roots: g.roots.clone(),
            })
            .collect();
        let disconnected_by_set = tree.disconnected(&allocation);
        let disconnected: BTreeSet<_> = disconnected_by_set.iter().flatten().copied().collect();
        reports.push(SpecConversion {
            index,
            title: header
                .get("title")
                .cloned()
                .unwrap_or_else(|| format!("Spec {index}")),
            from_version: version.into(),
            to_version: current.into(),
            kept: kept.iter().copied().collect(),
            dropped,
            disconnected: disconnected
                .iter()
                .map(|id| describe(*id, &data.passive_nodes, Some("disconnected"), &sockets))
                .collect(),
            disconnected_by_set,
            allocation,
        });
        replacements.push((range, rewritten));
    }
    // Replace from the end so earlier byte ranges retain their meaning.
    replacements.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    for (range, replacement) in replacements {
        output.replace_range(range, &replacement);
    }
    Ok(TreeConversion {
        xml: output,
        specs: reports,
    })
}

fn rewrite_spec(
    input: &str,
    version: &str,
    kept: &BTreeSet<u32>,
    keep: &impl Fn(&u32) -> bool,
) -> Result<String, String> {
    let mut reader = Reader::from_str(input);
    let mut writer = Writer::new(Vec::new());
    loop {
        let event = reader.read_event().map_err(|e| e.to_string())?;
        let empty = matches!(&event, Event::Empty(_));
        match event {
            Event::Start(e) | Event::Empty(e) => {
                let mut a = attrs(&e)?;
                let name = e.name();
                match name.as_ref() {
                    "Spec" => {
                        a.insert("nodes".into(), csv(kept));
                        a.insert("treeVersion".into(), version.into());
                    }
                    "WeaponSet1" | "WeaponSet2" => {
                        a.insert(
                            "nodes".into(),
                            csv(&ids(a.get("nodes")).intersection(kept).copied().collect()),
                        );
                    }
                    "AttributeOverride" => {
                        for key in ["strNodes", "dexNodes", "intNodes"] {
                            if a.contains_key(key) {
                                a.insert(
                                    key.into(),
                                    csv(&ids(a.get(key)).intersection(kept).copied().collect()),
                                );
                            }
                        }
                    }
                    "Socket"
                        if a.get("nodeId")
                            .and_then(|s| s.parse().ok())
                            .is_some_and(|id| !keep(&id)) =>
                    {
                        if !empty {
                            reader.read_to_end(name).map_err(|e| e.to_string())?;
                        }
                        continue;
                    }
                    _ => {}
                }
                let mut start = BytesStart::new(name.as_ref());
                for (k, v) in &a {
                    start.push_attribute((k.as_str(), v.as_str()));
                }
                writer
                    .write_event(if empty {
                        Event::Empty(start)
                    } else {
                        Event::Start(start)
                    })
                    .map_err(|e| e.to_string())?;
            }
            Event::Eof => break,
            event => writer.write_event(event).map_err(|e| e.to_string())?,
        }
    }
    String::from_utf8(writer.into_inner()).map_err(|e| e.to_string())
}

/// Byte ranges from the XML reader exclude fake tags inside comments/CDATA.
fn spec_ranges(xml: &str) -> Result<Vec<std::ops::Range<usize>>, String> {
    let mut reader = Reader::from_str(xml);
    let mut depth = 0;
    let mut tree_depth = None;
    let mut spec = None;
    let mut ranges = Vec::new();
    loop {
        let start = reader.buffer_position() as usize;
        match reader.read_event().map_err(|e| e.to_string())? {
            Event::Start(e) => {
                depth += 1;
                if e.name().as_ref() == "Tree" {
                    tree_depth = Some(depth);
                }
                if e.name().as_ref() == "Spec" && tree_depth == Some(depth - 1) {
                    spec = Some((start, depth));
                }
            }
            Event::Empty(e) if e.name().as_ref() == "Spec" && tree_depth == Some(depth) => {
                ranges.push(start..reader.buffer_position() as usize);
            }
            Event::End(e) => {
                if e.name().as_ref() == "Spec" && spec.is_some_and(|(_, level)| level == depth) {
                    ranges.push(spec.take().unwrap().0..reader.buffer_position() as usize);
                }
                if tree_depth == Some(depth) {
                    tree_depth = None;
                }
                depth -= 1;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(ranges)
}
