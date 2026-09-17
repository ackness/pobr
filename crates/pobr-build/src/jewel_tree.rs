//! Shared passive-jewel state for calculation, tree display and allocation planning.

use crate::{Build, BuildData};
use pobr_data::catalog::{PassiveNodeDef, PassiveNodeKind};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Default, Serialize)]
pub struct PassiveJewelState {
    pub class_starts: BTreeMap<String, u32>,
    pub allocation_grants: Vec<AllocationGrant>,
    pub nodes: BTreeMap<u32, JewelNodeOverride>,
    pub conquered: BTreeSet<u32>,
    pub unresolved: BTreeSet<u32>,
    pub rings: Vec<JewelRing>,
    pub warnings: Vec<String>,
    #[serde(skip)]
    pub handled_modifiers: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AllocationGrant {
    pub source: u32,
    /// These points may be allocated individually, but are not path roots.
    pub nodes: Vec<u32>,
    /// Implicit class starts allow connected paths without spending a start point.
    pub roots: Vec<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct JewelNodeOverride {
    pub name: String,
    pub stats: Vec<String>,
    pub replace: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct JewelRing {
    pub source: u32,
    pub center: u32,
    pub inner: f64,
    pub outer: f64,
}

fn bands<'a>(
    data: &'a BuildData,
    version: Option<&str>,
) -> Option<&'a [pobr_data::catalog::jewel_radii::JewelRadiusBandDef]> {
    fn parse(value: &str) -> Option<Vec<u32>> {
        value
            .split('_')
            .map(str::parse)
            .collect::<Result<_, _>>()
            .ok()
    }
    let target = version.and_then(parse);
    data.jewel_radii
        .tree_versions
        .iter()
        .filter_map(|(key, bands)| parse(key).map(|v| (v, bands)))
        .filter(|(v, _)| target.as_ref().is_none_or(|target| v <= target))
        .max_by(|(a, _), (b, _)| a.cmp(b))
        .map(|(_, v)| v.as_slice())
}

fn in_ring(node: &PassiveNodeDef, center: &PassiveNodeDef, inner: f64, outer: f64) -> bool {
    let (Some(x), Some(y), Some(cx), Some(cy)) = (node.x, node.y, center.x, center.y) else {
        return false;
    };
    let distance = (x - cx).powi(2) + (y - cy).powi(2);
    distance.is_finite() && distance >= inner * inner && distance <= outer * outer
}

fn conqueror(line: &str) -> Option<&str> {
    let (seed, name) = line
        .strip_prefix("remembrancing ")
        .and_then(|s| s.split_once(" songworthy deeds by the line of "))
        .or_else(|| {
            line.strip_prefix("glorifying the defilement of ")
                .and_then(|s| s.split_once(" souls in tribute to "))
        })?;
    seed.parse::<u32>().ok()?;
    Some(name)
}

/// The build must already contain only active socket jewels. Missing geometry or
/// data never borrows a different tree's identities or invents a seed result.
pub fn passive_jewel_state(build: &Build, data: &BuildData) -> PassiveJewelState {
    let mut result = PassiveJewelState::default();
    let nodes = data.passive_nodes_for(build.tree_version.as_deref());
    let catalog = &data.passive_jewels;
    // Historical packs without this domain cannot distinguish class starts
    // from ordinary points safely. Existing radius stat grants use their own path.
    if catalog.class_starts.is_empty() {
        return result;
    }
    for (class, id) in &catalog.class_starts {
        if let Some(node) = nodes
            .values()
            .filter(|n| &n.id == id)
            .min_by_key(|n| n.skill)
        {
            result.class_starts.insert(class.clone(), node.skill);
        }
    }
    let starts: BTreeSet<_> = result.class_starts.values().copied().collect();
    let eligible = |node: &&PassiveNodeDef| {
        !starts.contains(&node.skill)
            && node.ascendancy_id.is_none()
            && matches!(
                node.kind,
                PassiveNodeKind::Normal | PassiveNodeKind::Notable | PassiveNodeKind::Keystone
            )
    };
    let bands = bands(data, build.tree_version.as_deref()).unwrap_or_default();
    let mut jewels: Vec<_> = build.radius_jewels.iter().collect();
    jewels.sort_by_key(|jewel| jewel.socket_node);
    for jewel in jewels {
        let lines: Vec<_> = jewel
            .tree_texts
            .iter()
            .map(|s| s.to_ascii_lowercase())
            .collect();
        let mut grant = AllocationGrant {
            source: jewel.socket_node,
            nodes: vec![],
            roots: vec![],
        };
        for (raw, line) in jewel.tree_texts.iter().zip(&lines) {
            if let Some(class) = line
                .strip_prefix("can allocate passive skills from the ")
                .or_else(|| line.strip_prefix("can allocate passives from the "))
                .and_then(|s| s.strip_suffix("'s starting point"))
                && let Some(id) = result.class_starts.get(class)
            {
                grant.roots.push(*id);
                result.handled_modifiers.push(raw.clone());
            }
        }
        let ring_index = lines
            .iter()
            .filter_map(|line| catalog.ring_sizes.get(line))
            .next_back();
        let band = ring_index
            .and_then(|index| index.checked_sub(1))
            .and_then(|index| bands.get(index))
            .or_else(|| {
                ring_index
                    .is_none()
                    .then(|| {
                        let mut matches = bands.iter().filter(|band| {
                            jewel
                                .radius_label
                                .as_ref()
                                .is_some_and(|label| band.label.eq_ignore_ascii_case(label))
                        });
                        let first = matches.next();
                        matches.next().is_none().then_some(first).flatten()
                    })
                    .flatten()
            });
        let free = lines.iter().any(|line| {
            line == "passives in radius can be allocated without being connected to your tree"
        });
        let from_keystones: Vec<_> = lines
            .iter()
            .filter_map(|line| {
                line.strip_prefix("passives in radius of ").and_then(|s| {
                    s.strip_suffix(" can be allocated without being connected to your tree")
                })
            })
            .collect();
        let centers: Vec<_> = if !from_keystones.is_empty() {
            nodes
                .values()
                .filter(|node| {
                    node.kind == PassiveNodeKind::Keystone
                        && node.name.as_ref().is_some_and(|n| {
                            from_keystones
                                .iter()
                                .any(|name| n.eq_ignore_ascii_case(name))
                        })
                })
                .collect()
        } else {
            nodes.get(&jewel.socket_node).into_iter().collect()
        };
        let conquest = lines
            .iter()
            .find_map(|line| conqueror(line).and_then(|name| catalog.conquerors.get(name)));
        if let Some(band) = band {
            let inner = f64::from(band.inner) * data.jewel_radii.distance_multiplier;
            let outer = f64::from(band.outer) * data.jewel_radii.distance_multiplier;
            if inner.is_finite() && outer.is_finite() && inner >= 0.0 && outer >= inner {
                for center in centers
                    .into_iter()
                    .filter(|n| n.x.is_some() && n.y.is_some())
                {
                    if free || !from_keystones.is_empty() || conquest.is_some() {
                        result.rings.push(JewelRing {
                            source: jewel.socket_node,
                            center: center.skill,
                            inner,
                            outer,
                        });
                    }
                    let mut affected: Vec<_> = nodes
                        .values()
                        .filter(eligible)
                        .filter(|n| n.skill != center.skill && in_ring(n, center, inner, outer))
                        .collect();
                    affected.sort_by_key(|n| n.skill);
                    if free || !from_keystones.is_empty() {
                        grant.nodes.extend(affected.iter().map(|n| n.skill));
                        for (raw, line) in jewel.tree_texts.iter().zip(&lines) {
                            if catalog.ring_sizes.contains_key(line)
                                || line == "passives in radius can be allocated without being connected to your tree"
                                || from_keystones.iter().any(|name| line == &format!("passives in radius of {name} can be allocated without being connected to your tree")) {
                                result.handled_modifiers.push(raw.clone());
                            }
                        }
                    }
                    if let Some(conquest) = conquest {
                        for node in affected {
                            result.conquered.insert(node.skill);
                            result.nodes.remove(&node.skill);
                            result.unresolved.remove(&node.skill);
                            let family = catalog.families.get(&conquest.family);
                            // Tree text can contain [Attributes|Attribute] links.
                            let attribute = node.stats.iter().any(|text| {
                                text.to_ascii_lowercase()
                                    .split_once(" to any ")
                                    .is_some_and(|(_, tail)| tail.contains("attribute"))
                            });
                            let replacement = match node.kind {
                                PassiveNodeKind::Keystone => catalog.nodes.get(&conquest.keystone),
                                PassiveNodeKind::Normal if !attribute => family
                                    .and_then(|f| f.small_replacement.as_ref())
                                    .and_then(|id| catalog.nodes.get(id)),
                                _ => None,
                            };
                            if let Some(replacement) = replacement {
                                result.nodes.insert(
                                    node.skill,
                                    JewelNodeOverride {
                                        name: replacement.name.clone(),
                                        stats: replacement.stats.clone(),
                                        replace: true,
                                    },
                                );
                            } else if node.kind == PassiveNodeKind::Normal
                                && attribute
                                && let Some(family) =
                                    family.filter(|f| !f.attribute_additions.is_empty())
                            {
                                result.nodes.insert(
                                    node.skill,
                                    JewelNodeOverride {
                                        name: node.name.clone().unwrap_or_default(),
                                        stats: family.attribute_additions.clone(),
                                        replace: false,
                                    },
                                );
                            } else {
                                result.unresolved.insert(node.skill);
                            }
                        }
                        result.warnings.push(format!(
                            "Jewel@{}: seed-dependent passive transformations are unavailable",
                            jewel.socket_node
                        ));
                    }
                }
            }
        }
        grant.nodes.sort_unstable();
        grant.nodes.dedup();
        grant.roots.sort_unstable();
        grant.roots.dedup();
        if !grant.nodes.is_empty() || !grant.roots.is_empty() {
            result.allocation_grants.push(grant);
        }
    }
    result.rings.sort_by_key(|ring| (ring.source, ring.center));
    result.handled_modifiers.sort();
    result.handled_modifiers.dedup();
    result
}
