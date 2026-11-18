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

struct ParsedLine<'a> {
    index: usize,
    skeleton: String,
    captures: Vec<&'a str>,
    unscaled: bool,
}

struct Observation<'a> {
    entry: &'a TemplateEntry,
    captures: Vec<&'a str>,
    values: Vec<Option<f64>>,
    lines: BTreeSet<usize>,
}

struct MergedFamily<'a> {
    group: &'a str,
    generation_type: u32,
    stats: Vec<ModStat>,
}

struct MergedExplanation {
    lines: BTreeSet<usize>,
    prefixes: usize,
    suffixes: usize,
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
        self.lookup_lines_with_allowed(lines, base_tags, mod_domain, &BTreeMap::new())
    }

    /// Extends the display pool with exact native IDs validated by the caller
    /// for this base's category. IDs bypass only natural spawn weights, never
    /// native domain, generation type, stat coverage or ambiguity checks.
    /// Natural matches rank only against the natural pool; nonnatural matches
    /// rank separately within each admitted source. Both pools retain native
    /// group, stat set and prefix/suffix. No source or text aliases are inferred.
    /// Excessive search ambiguity conservatively leaves every line unknown.
    pub fn lookup_lines_with_allowed(
        &self,
        lines: &[&str],
        base_tags: &[String],
        mod_domain: u32,
        allowed_ids: &BTreeMap<String, BTreeSet<String>>,
    ) -> Vec<Option<TierInfo>> {
        let lines = lines
            .iter()
            .map(|&raw| TierLine {
                raw,
                unscaled: None,
            })
            .collect::<Vec<_>>();
        self.lookup_line_rolls(&lines, base_tags, mod_domain, allowed_ids, None)
            .into_iter()
            .map(|matched| matched.map(|matched| matched.info))
            .collect()
    }

    /// Test both interpretations in the same whole-item candidate search.
    /// An item level limits natural rolls without changing the published tier scale.
    pub fn lookup_line_rolls(
        &self,
        lines: &[TierLine<'_>],
        base_tags: &[String],
        mod_domain: u32,
        allowed_ids: &BTreeMap<String, BTreeSet<String>>,
        item_level: Option<u32>,
    ) -> Vec<Option<TierMatch>> {
        self.lookup_groups(lines, base_tags, mod_domain, allowed_ids, item_level)
            .unwrap_or_else(|| vec![None; lines.len()])
    }

    fn lookup_groups(
        &self,
        lines: &[TierLine<'_>],
        base_tags: &[String],
        mod_domain: u32,
        allowed_ids: &BTreeMap<String, BTreeSet<String>>,
        item_level: Option<u32>,
    ) -> Option<Vec<Option<TierMatch>>> {
        let tags = base_tags.iter().map(String::as_str).collect();
        let eligible: BTreeSet<usize> = self
            .mods
            .iter()
            .enumerate()
            .filter(|(_, modifier)| {
                modifier.domain == mod_domain
                    && (spawn_weight_positive(&modifier.spawn_weights, &tags)
                        || allowed_ids
                            .get(&modifier.id)
                            .is_some_and(|sources| !sources.is_empty()))
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
        let matchable: BTreeSet<_> = eligible
            .iter()
            .copied()
            .filter(|&index| {
                let modifier = &self.mods[index];
                item_level.is_none_or(|level| modifier.level <= level)
                    || allowed_ids.contains_key(&modifier.id)
            })
            .collect();
        let mut by_stat: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
        for &mod_index in &matchable {
            for stat in &self.mods[mod_index].stats {
                by_stat.entry(&stat.stat_id).or_default().push(mod_index);
            }
        }
        let parsed: Vec<_> = lines
            .iter()
            .enumerate()
            .flat_map(|(index, line)| {
                [(line.raw, false)]
                    .into_iter()
                    .chain(
                        line.unscaled
                            .filter(|&raw| raw != line.raw)
                            .map(|raw| (raw, true)),
                    )
                    .filter(|(text, _)| !text.is_empty())
                    .map(move |(text, unscaled)| {
                        let (skeleton, captures) = tokenize(text);
                        ParsedLine {
                            index,
                            skeleton,
                            captures,
                            unscaled,
                        }
                    })
            })
            .collect();
        let mut budget = MAX_SEARCH_STEPS;
        let mut fragments: BTreeMap<usize, BTreeSet<Fragment>> = BTreeMap::new();
        let mut raw_matches = BTreeSet::new();
        let mut observations = Vec::new();
        for (skeleton, entries) in &self.templates {
            for assignment in line_assignments(skeleton, &parsed, &mut budget)? {
                let captures: Vec<_> = assignment
                    .iter()
                    .flat_map(|line| line.captures.iter().copied())
                    .collect();
                let values = captures
                    .iter()
                    .map(|capture| capture.parse::<f64>().ok())
                    .collect::<Option<Vec<_>>>()?;
                for entry in entries {
                    let Some(values) = bind_captures(entry, &values) else {
                        continue;
                    };
                    let observation = Observation {
                        entry,
                        captures: captures.clone(),
                        values,
                        lines: assignment.iter().map(|line| line.index).collect(),
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
                        if let Some(fragment) = matching_fragment(&modifier.stats, &observation) {
                            raw_matches.extend(assignment.iter().filter_map(|line| {
                                (!line.unscaled && lines[line.index].unscaled.is_some())
                                    .then_some((mod_index, line.index))
                            }));
                            fragments.entry(mod_index).or_default().insert(fragment);
                        }
                    }
                    observations.push(observation);
                }
            }
        }
        let mut candidates = BTreeSet::new();
        for (mod_index, parts) in fragments {
            for fragment in
                complete_fragments(self.mods[mod_index].stats.len(), &parts, &mut budget)?
            {
                candidates.insert(Candidate {
                    mod_index,
                    fragment,
                });
            }
        }
        // Reserve slots for independent affixes before accepting a merged explanation.
        // Only a unique, non-overlapping candidate with no merged alternative can reserve one.
        let certain: Vec<_> = candidates
            .iter()
            .filter(|candidate| {
                !candidate.fragment.lines.iter().any(|&index| {
                    lines
                        .iter()
                        .enumerate()
                        .any(|(other, line)| other != index && line.raw == lines[index].raw)
                }) && !candidates.iter().any(|other| {
                    *candidate != other
                        && (!candidate.fragment.lines.is_disjoint(&other.fragment.lines)
                            || self.mods[candidate.mod_index].group
                                == self.mods[other.mod_index].group)
                })
            })
            .collect();
        let explanations = if certain.is_empty() {
            Vec::new()
        } else {
            self.merged_explanations(&matchable, &observations, &mut budget)?
        };
        let mut possibly_merged = BTreeSet::new();
        for explanation in &explanations {
            possibly_merged.extend(explanation.lines.iter().copied());
        }
        let mut required = [0, 0];
        if mod_domain == 1 {
            for candidate in &certain {
                if candidate.fragment.lines.is_disjoint(&possibly_merged) {
                    let is_suffix =
                        self.mods[candidate.mod_index].generation_type == GENERATION_SUFFIX;
                    required[usize::from(is_suffix)] += 1;
                }
            }
        }
        let mut merged = BTreeSet::new();
        for explanation in explanations {
            if required[0] + explanation.prefixes <= 3 && required[1] + explanation.suffixes <= 3 {
                merged.extend(explanation.lines);
            }
        }
        let mut result = vec![None; lines.len()];
        for candidate in certain {
            if !candidate.fragment.lines.is_disjoint(&merged) {
                continue;
            }
            let Some(info) = self.group_tier(candidate.mod_index, &eligible, &natural, allowed_ids)
            else {
                continue;
            };
            for &index in &candidate.fragment.lines {
                result[index] = Some(TierMatch {
                    info: info.clone(),
                    requires_catalyst: lines[index].unscaled.is_some()
                        && !raw_matches.contains(&(candidate.mod_index, index)),
                });
            }
        }
        Some(result)
    }

    fn merged_explanations(
        &self,
        eligible: &BTreeSet<usize>,
        observations: &[Observation<'_>],
        budget: &mut usize,
    ) -> Option<Vec<MergedExplanation>> {
        let visible: BTreeSet<_> = observations
            .iter()
            .filter(|observation| observation.entry.lookup_key.is_none())
            .flat_map(|observation| {
                observation
                    .entry
                    .slots
                    .iter()
                    .filter_map(|slot| match slot {
                        Slot::Value(member) => observation.entry.stat_ids.get(*member),
                        Slot::Literal(_) => None,
                    })
            })
            .collect();
        let mut families: BTreeMap<_, MergedFamily<'_>> = BTreeMap::new();
        for &index in eligible {
            let modifier = &self.mods[index];
            if modifier
                .stats
                .iter()
                .any(|stat| !visible.contains(&stat.stat_id))
            {
                continue;
            }
            let key = (
                modifier.group.as_str(),
                modifier.generation_type,
                stat_key(modifier.stats.iter().map(|stat| stat.stat_id.as_str())),
            );
            let family = families.entry(key).or_insert_with(|| MergedFamily {
                group: &modifier.group,
                generation_type: modifier.generation_type,
                stats: modifier.stats.clone(),
            });
            for stat in &modifier.stats {
                let range = family
                    .stats
                    .iter_mut()
                    .find(|range| range.stat_id == stat.stat_id)?;
                range.min = range.min.min(stat.min);
                range.max = range.max.max(stat.max);
            }
        }
        let families: Vec<_> = families.into_values().collect();
        let mut pending: Vec<_> = families
            .iter()
            .enumerate()
            .map(|(index, family)| (vec![index], family.stats.clone()))
            .collect();
        let mut visited = BTreeSet::new();
        let mut explanations = Vec::new();
        while let Some((selected, stats)) = pending.pop() {
            spend(budget)?;
            if selected.len() >= 2 {
                let mut parts = BTreeSet::new();
                for observation in observations {
                    spend(budget)?;
                    if let Some(fragment) = matching_fragment(&stats, observation) {
                        parts.insert(fragment);
                    }
                }
                let prefixes = selected
                    .iter()
                    .filter(|&&index| families[index].generation_type == GENERATION_PREFIX)
                    .count();
                for fragment in complete_fragments(stats.len(), &parts, budget)? {
                    explanations.push(MergedExplanation {
                        lines: fragment.lines,
                        prefixes,
                        suffixes: selected.len() - prefixes,
                    });
                }
            }
            for (index, family) in families.iter().enumerate() {
                if selected
                    .iter()
                    .any(|&other| families[other].group == family.group)
                    || selected
                        .iter()
                        .filter(|&&other| families[other].generation_type == family.generation_type)
                        .count()
                        >= 3
                    || !family
                        .stats
                        .iter()
                        .any(|stat| stats.iter().any(|current| current.stat_id == stat.stat_id))
                {
                    continue;
                }
                let mut next = selected.clone();
                next.push(index);
                next.sort_unstable();
                if !visited.insert(next.clone()) {
                    continue;
                }
                spend(budget)?;
                let mut sum = stats.clone();
                for stat in &family.stats {
                    if let Some(current) = sum
                        .iter_mut()
                        .find(|current| current.stat_id == stat.stat_id)
                    {
                        current.min = current.min.checked_add(stat.min)?;
                        current.max = current.max.checked_add(stat.max)?;
                    } else {
                        sum.push(stat.clone());
                    }
                }
                pending.push((next, sum));
            }
        }
        Some(explanations)
    }

    fn group_tier(
        &self,
        mod_index: usize,
        eligible: &BTreeSet<usize>,
        natural: &BTreeSet<usize>,
        allowed_ids: &BTreeMap<String, BTreeSet<String>>,
    ) -> Option<TierInfo> {
        let modifier = &self.mods[mod_index];
        let key = stat_key(modifier.stats.iter().map(|stat| stat.stat_id.as_str()));
        let sources: Vec<_> = if natural.contains(&mod_index) {
            vec![None]
        } else {
            allowed_ids.get(&modifier.id)?.iter().map(Some).collect()
        };
        let mut verdict = None;
        for source in sources {
            let mut ranked: Vec<usize> = self
                .pools
                .get(&key)?
                .iter()
                .copied()
                .filter(|index| {
                    eligible.contains(index)
                        && natural.contains(index) == natural.contains(&mod_index)
                        && source.is_none_or(|source| {
                            allowed_ids
                                .get(&self.mods[*index].id)
                                .is_some_and(|sources| sources.contains(source))
                        })
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
            let info = TierInfo {
                tier: (ranked.len() - position) as u32,
                total: ranked.len() as u32,
                is_prefix: modifier.generation_type == GENERATION_PREFIX,
                affix_name: modifier.affix_name.clone(),
                mod_id: modifier.id.clone(),
            };
            if verdict.as_ref().is_some_and(|previous| *previous != info) {
                return None;
            }
            verdict = Some(info);
        }
        verdict
    }
}

fn matching_fragment(stats: &[ModStat], observation: &Observation<'_>) -> Option<Fragment> {
    let entry = observation.entry;
    let positions = entry
        .stat_ids
        .iter()
        .map(|id| stats.iter().position(|stat| stat.stat_id == *id))
        .collect::<Option<BTreeSet<_>>>()?;
    if positions.len() != entry.stat_ids.len() {
        return None;
    }
    let members: Vec<_> = positions
        .iter()
        .map(|&index| stats[index].clone())
        .collect();
    let matches = if let Some((descriptor, branch)) = &entry.descriptor {
        branches::matches(&members, entry, descriptor, *branch, &observation.captures)
    } else {
        values_in_range(&members, &entry.stat_ids, &observation.values)
    };
    matches.then(|| Fragment {
        lines: observation.lines.clone(),
        stats: positions,
        lookup_keys: entry
            .lookup_key
            .map(|(member, key)| (entry.stat_ids.clone(), member, key))
            .into_iter()
            .collect(),
    })
}

fn complete_fragments(
    count: usize,
    parts: &BTreeSet<Fragment>,
    budget: &mut usize,
) -> Option<BTreeSet<Fragment>> {
    let mut complete = BTreeSet::new();
    let mut pending = vec![Fragment::default()];
    let mut visited = BTreeSet::new();
    while let Some(current) = pending.pop() {
        spend(budget)?;
        if !visited.insert(current.clone()) {
            continue;
        }
        let missing = (0..count).find(|index| !current.stats.contains(index));
        let Some(missing) = missing else {
            complete.insert(current);
            continue;
        };
        for part in parts {
            if part.stats.contains(&missing)
                && part.stats.is_disjoint(&current.stats)
                && part.lines.is_disjoint(&current.lines)
            {
                spend(budget)?;
                let mut next = current.clone();
                next.lines.extend(&part.lines);
                next.stats.extend(&part.stats);
                next.lookup_keys.extend(part.lookup_keys.iter().cloned());
                pending.push(next);
            }
        }
    }
    Some(complete)
}

fn spend(budget: &mut usize) -> Option<()> {
    *budget = budget.checked_sub(1)?;
    Some(())
}

fn line_assignments<'a, 'text>(
    skeleton: &str,
    parsed: &'a [ParsedLine<'text>],
    budget: &mut usize,
) -> Option<Vec<Vec<&'a ParsedLine<'text>>>> {
    let mut assignments: Vec<Vec<&ParsedLine<'text>>> = vec![Vec::new()];
    for part in skeleton.split('\n') {
        let choices: Vec<_> = parsed.iter().filter(|line| line.skeleton == part).collect();
        if choices.is_empty() {
            return Some(Vec::new());
        }
        let mut next = Vec::new();
        for assignment in assignments {
            for &line in &choices {
                if assignment.iter().all(|other| other.index != line.index) {
                    spend(budget)?;
                    let mut assignment = assignment.clone();
                    assignment.push(line);
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
