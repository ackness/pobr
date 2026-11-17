use super::*;
use std::collections::BTreeMap;

const MAX_SEARCH_STEPS: usize = 16_384;

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct Fragment {
    lines: BTreeSet<usize>,
    stats: BTreeSet<usize>,
    lookup_keys: BTreeSet<(Vec<String>, usize, i64)>,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Candidate {
    mod_index: usize,
    fragment: Fragment,
}

impl TierIndex {
    /// Matches complete groups of annotation-stripped explicit lines on one item.
    /// Results retain input order; incomplete, duplicate or competing groups
    /// remain `None`. Unlike `lookup`, legacy overlaps never choose a best tier.
    pub fn lookup_lines(
        &self,
        lines: &[&str],
        base_tags: &[String],
        mod_domain: u32,
    ) -> Vec<Option<TierInfo>> {
        self.lookup_lines_with_allowed(lines, base_tags, mod_domain, &BTreeSet::new())
    }

    /// Extends the display pool with exact native IDs validated by the caller
    /// for this base's category. IDs bypass only natural spawn weights, never
    /// native domain, generation type, stat coverage or ambiguity checks.
    /// Natural matches rank only against the natural pool; nonnatural matches
    /// rank only against admitted nonnatural IDs. Both pools retain native
    /// group, stat set and prefix/suffix. No source or text aliases are inferred.
    /// Excessive search ambiguity conservatively leaves every line unknown.
    pub fn lookup_lines_with_allowed(
        &self,
        lines: &[&str],
        base_tags: &[String],
        mod_domain: u32,
        allowed_ids: &BTreeSet<String>,
    ) -> Vec<Option<TierInfo>> {
        self.lookup_groups(lines, base_tags, mod_domain, allowed_ids)
            .unwrap_or_else(|| vec![None; lines.len()])
    }

    fn lookup_groups(
        &self,
        lines: &[&str],
        base_tags: &[String],
        mod_domain: u32,
        allowed_ids: &BTreeSet<String>,
    ) -> Option<Vec<Option<TierInfo>>> {
        let tags = base_tags.iter().map(String::as_str).collect();
        let eligible: BTreeSet<usize> = self
            .mods
            .iter()
            .enumerate()
            .filter(|(_, modifier)| {
                modifier.domain == mod_domain
                    && (spawn_weight_positive(&modifier.spawn_weights, &tags)
                        || allowed_ids.contains(&modifier.id))
                    && modifier
                        .stats
                        .iter()
                        .map(|stat| &stat.stat_id)
                        .collect::<BTreeSet<_>>()
                        .len()
                        == modifier.stats.len()
            })
            .map(|(index, _)| index)
            .collect();
        let natural: BTreeSet<usize> = eligible
            .iter()
            .copied()
            .filter(|&index| spawn_weight_positive(&self.mods[index].spawn_weights, &tags))
            .collect();
        let mut by_stat: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
        for &mod_index in &eligible {
            for stat in &self.mods[mod_index].stats {
                by_stat.entry(&stat.stat_id).or_default().push(mod_index);
            }
        }
        let parsed: Vec<_> = lines.iter().map(|line| tokenize(line)).collect();
        let mut budget = MAX_SEARCH_STEPS;
        let mut fragments: BTreeMap<usize, BTreeSet<Fragment>> = BTreeMap::new();
        for (skeleton, entries) in &self.templates {
            for assignment in line_assignments(skeleton, &parsed, &mut budget)? {
                let captures: Vec<_> = assignment
                    .iter()
                    .flat_map(|&index| parsed[index].1.iter().copied())
                    .collect();
                let values = captures
                    .iter()
                    .map(|capture| capture.parse::<f64>().ok())
                    .collect::<Option<Vec<_>>>()?;
                for entry in entries {
                    let Some(values) = bind_captures(entry, &values) else {
                        continue;
                    };
                    let Some(mod_indices) = entry
                        .stat_ids
                        .first()
                        .and_then(|stat| by_stat.get(stat.as_str()))
                    else {
                        continue;
                    };
                    for &mod_index in mod_indices {
                        spend(&mut budget)?;
                        let modifier = &self.mods[mod_index];
                        let Some(positions) = entry
                            .stat_ids
                            .iter()
                            .map(|id| modifier.stats.iter().position(|stat| stat.stat_id == *id))
                            .collect::<Option<BTreeSet<_>>>()
                        else {
                            continue;
                        };
                        if positions.len() != entry.stat_ids.len() {
                            continue;
                        }
                        let stats: Vec<_> = positions
                            .iter()
                            .map(|&index| modifier.stats[index].clone())
                            .collect();
                        let matches = if let Some((descriptor, branch)) = &entry.descriptor {
                            branches::matches(&stats, entry, descriptor, *branch, &captures)
                        } else {
                            values_in_range(&stats, &entry.stat_ids, &values)
                        };
                        if matches {
                            fragments.entry(mod_index).or_default().insert(Fragment {
                                lines: assignment.iter().copied().collect(),
                                stats: positions,
                                lookup_keys: entry
                                    .lookup_key
                                    .map(|(member, key)| (entry.stat_ids.clone(), member, key))
                                    .into_iter()
                                    .collect(),
                            });
                        }
                    }
                }
            }
        }
        let mut candidates = BTreeSet::new();
        for (mod_index, parts) in fragments {
            let mut pending = vec![Fragment::default()];
            let mut visited = BTreeSet::new();
            while let Some(current) = pending.pop() {
                spend(&mut budget)?;
                if !visited.insert(current.clone()) {
                    continue;
                }
                let missing = (0..self.mods[mod_index].stats.len())
                    .find(|index| !current.stats.contains(index));
                let Some(missing) = missing else {
                    candidates.insert(Candidate {
                        mod_index,
                        fragment: current,
                    });
                    continue;
                };
                for part in &parts {
                    if part.stats.contains(&missing)
                        && part.stats.is_disjoint(&current.stats)
                        && part.lines.is_disjoint(&current.lines)
                    {
                        spend(&mut budget)?;
                        let mut next = current.clone();
                        next.lines.extend(&part.lines);
                        next.stats.extend(&part.stats);
                        next.lookup_keys.extend(part.lookup_keys.iter().cloned());
                        pending.push(next);
                    }
                }
            }
        }
        let mut result = vec![None; lines.len()];
        for candidate in &candidates {
            let duplicate = candidate.fragment.lines.iter().any(|&index| {
                lines
                    .iter()
                    .enumerate()
                    .any(|(other, line)| other != index && *line == lines[index])
            });
            let competing = candidates.iter().any(|other| {
                candidate != other
                    && (!candidate.fragment.lines.is_disjoint(&other.fragment.lines)
                        || self.mods[candidate.mod_index].group == self.mods[other.mod_index].group)
            });
            if duplicate || competing {
                continue;
            }
            let info = self.group_tier(candidate.mod_index, &eligible, &natural)?;
            for &index in &candidate.fragment.lines {
                result[index] = Some(info.clone());
            }
        }
        Some(result)
    }

    fn group_tier(
        &self,
        mod_index: usize,
        eligible: &BTreeSet<usize>,
        natural: &BTreeSet<usize>,
    ) -> Option<TierInfo> {
        let modifier = &self.mods[mod_index];
        let key = stat_key(modifier.stats.iter().map(|stat| stat.stat_id.as_str()));
        let mut ranked: Vec<usize> = self
            .pools
            .get(&key)?
            .iter()
            .copied()
            .filter(|index| {
                eligible.contains(index)
                    && natural.contains(index) == natural.contains(&mod_index)
                    && self.mods[*index].group == modifier.group
                    && self.mods[*index].generation_type == modifier.generation_type
            })
            .collect();
        ranked.sort_by_key(|&index| {
            let modifier = &self.mods[index];
            (
                modifier.level,
                stat_magnitude(&modifier.stats),
                &modifier.id,
            )
        });
        let position = ranked.iter().position(|&index| index == mod_index)?;
        Some(TierInfo {
            tier: (ranked.len() - position) as u32,
            total: ranked.len() as u32,
            is_prefix: modifier.generation_type == GENERATION_PREFIX,
            affix_name: modifier.affix_name.clone(),
            mod_id: modifier.id.clone(),
        })
    }
}

fn spend(budget: &mut usize) -> Option<()> {
    *budget = budget.checked_sub(1)?;
    Some(())
}

fn line_assignments(
    skeleton: &str,
    parsed: &[(String, Vec<&str>)],
    budget: &mut usize,
) -> Option<Vec<Vec<usize>>> {
    let mut assignments = vec![Vec::new()];
    for part in skeleton.split('\n') {
        let choices: Vec<_> = parsed
            .iter()
            .enumerate()
            .filter(|(_, line)| line.0 == part)
            .map(|(index, _)| index)
            .collect();
        if choices.is_empty() {
            return Some(Vec::new());
        }
        let mut next = Vec::new();
        for assignment in assignments {
            for &index in &choices {
                if !assignment.contains(&index) {
                    spend(budget)?;
                    let mut assignment = assignment.clone();
                    assignment.push(index);
                    next.push(assignment);
                }
            }
        }
        assignments = next;
    }
    Some(assignments)
}

#[cfg(test)]
mod tests;
