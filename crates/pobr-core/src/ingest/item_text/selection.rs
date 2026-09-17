//! Calculation-only item selections. Original text remains owned by the edit view.
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn annotation<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let marker = format!("{{{key}:");
    line.split_once(&marker)?
        .1
        .split_once('}')
        .map(|(value, _)| value)
}
fn ids(line: &str, key: &str) -> Option<BTreeSet<u32>> {
    annotation(line, key).map(|value| {
        value
            .split(',')
            .filter_map(|id| id.trim().parse().ok())
            .collect()
    })
}
#[derive(Default)]
pub(super) struct ItemSelection {
    variants: BTreeSet<u32>,
    version: u32,
    base: u32,
    groups: BTreeMap<u32, u32>,
    grouped: bool,
}
impl ItemSelection {
    pub(super) fn new(raw: &str) -> Self {
        let lines: Vec<_> = raw.lines().map(str::trim).collect();
        let header = |key: &str| {
            lines
                .iter()
                .rev()
                .find_map(|line| line.strip_prefix(&format!("{key}:")))
                .map(str::trim)
        };
        let count = |key: &str| {
            lines
                .iter()
                .filter(|line| line.starts_with(&format!("{key}:")))
                .count() as u32
        };
        let selected = |key: &str, total: u32, default: u32| {
            header(key)
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(default)
                .max(1)
                .min(total.max(1))
        };
        let variant_count = count("Variant");
        let version_count = count("Version");
        let mut out = Self {
            version: selected("Selected Version", version_count, version_count),
            base: header("Selected Base Variant")
                .and_then(|v| v.parse().ok())
                .unwrap_or(1)
                .max(1),
            ..Default::default()
        };
        let main = selected("Selected Variant", variant_count, variant_count);
        out.variants.insert(main);
        for suffix in ["", " Two", " Three", " Four", " Five"] {
            if header(&format!("Has Alt Variant{suffix}")).is_some_and(|v| v != "false")
                && version_count == 0
            {
                let alt = selected(
                    &format!("Selected Alt Variant{suffix}"),
                    variant_count,
                    variant_count,
                );
                out.variants.insert(alt);
            }
        }
        let mut options: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
        for line in &lines {
            if let Some(groups) = ids(line, "group") {
                out.grouped = true;
                if ids(line, "version").is_some_and(|v| !v.contains(&out.version)) {
                    continue;
                }
                for group in groups {
                    options.entry(group).or_default().extend(
                        ids(line, "variant")
                            .unwrap_or_default()
                            .into_iter()
                            .filter(|id| *id > 0 && *id <= variant_count),
                    );
                }
            }
            if let Some((group, variant)) = line
                .strip_prefix("Selected Variant Group:")
                .and_then(|v| v.trim().split_once('='))
                && let (Ok(group), Ok(variant)) = (group.trim().parse(), variant.trim().parse())
            {
                out.groups.insert(group, variant);
            }
        }
        let mut used = BTreeSet::new();
        let mut missing = Vec::new();
        for (&group, choices) in &options {
            if let Some(&selected) = out
                .groups
                .get(&group)
                .filter(|id| choices.contains(*id) && !used.contains(*id))
            {
                used.insert(selected);
            } else {
                missing.push(group);
            }
        }
        for group in missing {
            out.groups.remove(&group);
            if let Some(&first) = options[&group].iter().find(|id| !used.contains(*id)) {
                out.groups.insert(group, first);
                used.insert(first);
            }
        }
        out.groups.retain(|group, _| options.contains_key(group));
        out
    }
    pub(super) fn accepts(&self, line: &str) -> bool {
        if ids(line, "version").is_some_and(|v| !v.contains(&self.version))
            || ids(line, "base").is_some_and(|v| !v.contains(&self.base))
        {
            return false;
        }
        if let Some(groups) = ids(line, "group") {
            return ids(line, "variant").is_some_and(|variants| {
                groups
                    .iter()
                    .any(|group| self.groups.get(group).is_some_and(|v| variants.contains(v)))
            });
        }
        ids(line, "variant")
            .is_none_or(|variants| !self.grouped && !variants.is_disjoint(&self.variants))
    }
}
