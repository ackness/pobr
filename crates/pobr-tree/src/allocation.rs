//! Connected shared and weapon-set allocations. A shared path cannot depend on
//! a node that disappears on weapon swap; each exclusive path uses its own set.
use pobr_data::catalog::{PassiveNodeDef, PassiveNodeKind};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Allocation {
    pub shared: BTreeSet<u32>,
    pub exclusive: [BTreeSet<u32>; 2],
}
impl Allocation {
    pub fn all(&self) -> BTreeSet<u32> {
        self.shared
            .iter()
            .chain(self.exclusive.iter().flatten())
            .copied()
            .collect()
    }
    pub fn active(&self, set: usize) -> BTreeSet<u32> {
        self.shared.union(&self.exclusive[set]).copied().collect()
    }
}
#[derive(Clone, Debug)]
pub struct Grant {
    pub source: u32,
    pub nodes: Vec<u32>,
    pub roots: Vec<u32>,
}
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditAction {
    Allocate,
    Refund,
    Promote,
}
#[derive(Clone, Debug, Serialize)]
pub struct EditResult {
    pub allocation: Allocation,
    pub added: Vec<u32>,
    pub removed: Vec<u32>,
    pub promoted: Vec<u32>,
    pub path: Vec<u32>,
    pub points_used: u32,
    pub points_max: u32,
    pub weapon_points_used: [u32; 2],
    pub weapon_points_max: u32,
}

pub struct AllocationTree<'a> {
    pub nodes: BTreeMap<u32, &'a PassiveNodeDef>,
    pub roots: BTreeSet<u32>,
    pub class_starts: BTreeSet<u32>,
    pub ascendancy: Option<String>,
    pub grants: Vec<Grant>,
    pub points_max: u32,
    pub weapon_points_base: u32,
    adjacency: BTreeMap<u32, BTreeSet<u32>>,
}
impl<'a> AllocationTree<'a> {
    pub fn new(nodes: impl Iterator<Item = &'a PassiveNodeDef>) -> Self {
        let nodes: BTreeMap<_, _> = nodes.map(|n| (n.skill, n)).collect();
        let mut adjacency: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
        for n in nodes.values() {
            for other in &n.connections {
                if nodes
                    .get(other)
                    .is_some_and(|o| o.ascendancy_id == n.ascendancy_id)
                {
                    adjacency.entry(n.skill).or_default().insert(*other);
                    adjacency.entry(*other).or_default().insert(n.skill);
                }
            }
        }
        Self {
            nodes,
            adjacency,
            roots: BTreeSet::new(),
            class_starts: BTreeSet::new(),
            ascendancy: None,
            grants: vec![],
            points_max: 0,
            weapon_points_base: 0,
        }
    }
    fn eligible(&self, id: u32, allocated: &BTreeSet<u32>) -> bool {
        let Some(n) = self.nodes.get(&id) else {
            return false;
        };
        if n.ascendancy_id.is_some() && n.ascendancy_id != self.ascendancy {
            return false;
        }
        n.unlock_constraint.as_ref().is_none_or(|c| {
            c.ascendancy
                .as_ref()
                .is_none_or(|a| self.ascendancy.as_ref() == Some(a))
                && c.nodes.iter().all(|id| allocated.contains(id))
        })
    }
    fn shared_only(&self, id: u32) -> bool {
        self.nodes.get(&id).is_some_and(|n| {
            n.ascendancy_id.is_some()
                || self.class_starts.contains(&id)
                || matches!(
                    n.kind,
                    PassiveNodeKind::Keystone
                        | PassiveNodeKind::JewelSocket
                        | PassiveNodeKind::AscendancyStart
                )
        })
    }
    fn access(&self, allocated: &BTreeSet<u32>) -> (BTreeSet<u32>, BTreeSet<u32>, BTreeSet<u32>) {
        let mut roots = self.roots.clone();
        let mut connected = BTreeSet::new();
        loop {
            let before = roots.len();
            let mut queue: VecDeque<_> = roots.iter().copied().collect();
            let mut seen = roots.clone();
            while let Some(id) = queue.pop_front() {
                if allocated.contains(&id) {
                    connected.insert(id);
                }
                if self
                    .nodes
                    .get(&id)
                    .is_some_and(|n| n.kind == PassiveNodeKind::Mastery)
                {
                    continue;
                }
                for next in self.adjacency.get(&id).into_iter().flatten() {
                    if allocated.contains(next)
                        && self.eligible(*next, allocated)
                        && (!self.class_starts.contains(next) || roots.contains(next))
                        && seen.insert(*next)
                    {
                        queue.push_back(*next);
                    }
                }
            }
            for grant in &self.grants {
                if connected.contains(&grant.source) {
                    roots.extend(&grant.roots);
                }
            }
            if roots.len() == before {
                break;
            }
        }
        let free = self
            .grants
            .iter()
            .filter(|g| connected.contains(&g.source))
            .flat_map(|g| g.nodes.iter())
            .filter(|id| self.eligible(**id, allocated))
            .copied()
            .collect();
        (connected, free, roots)
    }
    fn reachable(&self, allocated: &BTreeSet<u32>) -> BTreeSet<u32> {
        let (mut connected, free, _) = self.access(allocated);
        connected.extend(free.intersection(allocated));
        connected
    }
    /// Disconnected points in the shared path and each exclusive path. Inspection
    /// never refunds them: conversion previews must show the player every loss.
    pub fn disconnected(&self, a: &Allocation) -> [BTreeSet<u32>; 3] {
        [
            a.shared
                .difference(&self.reachable(&a.shared))
                .copied()
                .collect(),
            a.exclusive[0]
                .difference(&self.reachable(&a.active(0)))
                .copied()
                .collect(),
            a.exclusive[1]
                .difference(&self.reachable(&a.active(1)))
                .copied()
                .collect(),
        ]
    }

    pub fn validate(&self, a: &Allocation) -> Result<(), String> {
        if !a.shared.is_disjoint(&a.exclusive[0])
            || !a.shared.is_disjoint(&a.exclusive[1])
            || !a.exclusive[0].is_disjoint(&a.exclusive[1])
        {
            return Err("overlapping_allocations".into());
        }
        if a.all().iter().any(|id| !self.nodes.contains_key(id)) {
            return Err("unknown_node".into());
        }
        if a.exclusive.iter().flatten().any(|id| self.shared_only(*id)) {
            return Err("shared_only_node".into());
        }
        if !a.shared.is_subset(&self.reachable(&a.shared)) {
            return Err("disconnected_shared_path".into());
        }
        for set in 0..2 {
            if !a.exclusive[set].is_subset(&self.reachable(&a.active(set))) {
                return Err("disconnected_weapon_path".into());
            }
        }
        Ok(())
    }
    fn route(&self, a: &Allocation, target: u32, mode: u8) -> Option<Vec<u32>> {
        let allocated = if mode == 0 {
            a.shared.clone()
        } else {
            a.active(usize::from(mode - 1))
        };
        let (connected, free, roots) = self.access(&allocated);
        if free.contains(&target) && self.eligible(target, &allocated) {
            return Some(vec![target]);
        }
        let mut sources = connected;
        sources.extend(roots.iter());
        let mut previous: BTreeMap<u32, u32> = BTreeMap::new();
        let mut visited = sources.clone();
        let mut queue: VecDeque<_> = sources.into_iter().collect();
        // Shared allocation may promote existing exclusive travel points.
        let blocked = if mode == 0 {
            BTreeSet::new()
        } else {
            a.exclusive[usize::from(2 - mode)].clone()
        };
        while let Some(id) = queue.pop_front() {
            if id == target {
                let mut path = vec![id];
                let mut at = id;
                while let Some(parent) = previous.get(&at) {
                    path.push(*parent);
                    at = *parent;
                }
                path.reverse();
                return Some(path);
            }
            if self
                .nodes
                .get(&id)
                .is_some_and(|n| n.kind == PassiveNodeKind::Mastery)
            {
                continue;
            }
            for next in self.adjacency.get(&id).into_iter().flatten() {
                if blocked.contains(next)
                    || !self.eligible(*next, &allocated)
                    || (self.class_starts.contains(next) && !roots.contains(next))
                {
                    continue;
                }
                if visited.insert(*next) {
                    previous.insert(*next, id);
                    queue.push_back(*next);
                }
            }
        }
        None
    }
    fn spend(&self, a: &Allocation) -> (u32, [u32; 2], u32, u32) {
        let mut regular = 0;
        let mut ascendancy = 0;
        let mut weapon_max = self.weapon_points_base;
        for id in &a.shared {
            let n = self.nodes[id];
            if self.roots.contains(id)
                || self.class_starts.contains(id)
                || n.kind == PassiveNodeKind::AscendancyStart
            {
                continue;
            }
            if n.ascendancy_id.is_some() {
                ascendancy += 1;
            } else {
                regular += 1;
            }
            // The game's Weapon Master wording converts existing points; it does
            // not grant additional ordinary passives.
            for line in &n.stats {
                let line = line.replace(
                    "[WeaponSetPassiveSkillPoints|Weapon Set Skill Points]",
                    "Weapon Set Skill Points",
                );
                if let Some(value) = line
                    .strip_suffix(" Passive Skill Points become Weapon Set Skill Points")
                    .and_then(|s| s.parse::<u32>().ok())
                {
                    weapon_max += value;
                }
            }
        }
        let sets = a.exclusive.each_ref().map(|s| s.len() as u32);
        (regular + sets[0].max(sets[1]), sets, weapon_max, ascendancy)
    }
    pub fn edit(
        &self,
        before: &Allocation,
        target: u32,
        mode: u8,
        action: EditAction,
    ) -> Result<EditResult, String> {
        if mode > 2 {
            return Err("invalid_mode".into());
        }
        if let Err(reason) = self.validate(before)
            && !matches!(
                reason.as_str(),
                "disconnected_shared_path" | "disconnected_weapon_path"
            )
        {
            return Err(reason);
        }
        // A historical conversion may retain disconnected points for review.
        // Allow an explicit repair/refund, but validate the complete result below.
        if !self.nodes.contains_key(&target) {
            return Err("unknown_node".into());
        }
        if self.roots.contains(&target) || self.class_starts.contains(&target) {
            return Err("protected_root".into());
        }
        // Masteries require an effect choice, which this path-only editor does not supply.
        if !matches!(action, EditAction::Refund)
            && !before.all().contains(&target)
            && self.nodes[&target].kind == PassiveNodeKind::Mastery
        {
            return Err("unsupported_node".into());
        }
        let mut next = before.clone();
        let mut path = vec![];
        match action {
            EditAction::Refund => {
                if !before.all().contains(&target) {
                    return Err("not_allocated".into());
                }
                if !before.shared.contains(&target)
                    && mode > 0
                    && !before.exclusive[usize::from(mode - 1)].contains(&target)
                {
                    return Err("wrong_weapon_set".into());
                }
                next.shared.remove(&target);
                next.exclusive.iter_mut().for_each(|s| {
                    s.remove(&target);
                });
                loop {
                    let reachable = self.reachable(&next.shared);
                    if reachable == next.shared {
                        break;
                    }
                    next.shared = reachable;
                }
                for set in 0..2 {
                    loop {
                        let reachable = self.reachable(&next.active(set));
                        let old = next.exclusive[set].len();
                        next.exclusive[set].retain(|id| reachable.contains(id));
                        if next.exclusive[set].len() == old {
                            break;
                        }
                    }
                }
            }
            EditAction::Allocate | EditAction::Promote => {
                if matches!(action, EditAction::Promote) && !before.all().contains(&target) {
                    return Err("not_allocated".into());
                }
                let effective = if matches!(action, EditAction::Promote) || self.shared_only(target)
                {
                    0
                } else {
                    mode
                };
                path = self
                    .route(before, target, effective)
                    .ok_or("no_legal_path")?;
                if effective == 0 {
                    for id in &path {
                        next.shared.insert(*id);
                        next.exclusive.iter_mut().for_each(|s| {
                            s.remove(id);
                        });
                    }
                } else {
                    if let Some(shared_target) = path.iter().rev().find(|id| {
                        self.shared_only(**id)
                            && !before.shared.contains(id)
                            && !self.roots.contains(id)
                    }) {
                        let shared_path = self
                            .route(before, *shared_target, 0)
                            .ok_or("no_shared_path")?;
                        for id in &shared_path {
                            next.shared.insert(*id);
                            next.exclusive.iter_mut().for_each(|s| {
                                s.remove(id);
                            });
                        }
                        path.extend(shared_path);
                        path.sort_unstable();
                        path.dedup();
                    }
                    for id in &path {
                        if !next.shared.contains(id) && !self.roots.contains(id) {
                            next.exclusive[usize::from(effective - 1)].insert(*id);
                        }
                    }
                }
                // Roots are implicit; retain only roots that the import explicitly carried.
                next.shared
                    .retain(|id| !self.roots.contains(id) || before.shared.contains(id));
            }
        }
        self.validate(&next)?;
        let (used, sets, max, asc) = self.spend(&next);
        let (old_used, old_sets, old_max, old_asc) = self.spend(before);
        // An over-budget import can be reduced, but an edit may not worsen it.
        if used > self.points_max && used > old_used {
            return Err("passive_point_budget".into());
        }
        if asc > 8 && asc > old_asc {
            return Err("ascendancy_point_budget".into());
        }
        if (0..2).any(|i| sets[i] > max && (sets[i] > old_sets[i] || max < old_max)) {
            return Err("weapon_point_budget".into());
        }
        let all = next.all();
        let old_all = before.all();
        Ok(EditResult {
            added: all.difference(&old_all).copied().collect(),
            removed: old_all.difference(&all).copied().collect(),
            promoted: next
                .shared
                .difference(&before.shared)
                .filter(|id| old_all.contains(id))
                .copied()
                .collect(),
            allocation: next,
            path,
            points_used: used,
            points_max: self.points_max,
            weapon_points_used: sets,
            weapon_points_max: max,
        })
    }
}
