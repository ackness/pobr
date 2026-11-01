//! Affix tier inference (display-only, best-effort).
//!
//! Goal: label an **already-rolled** item mod line with "which rank within
//! its pool" (T1 = strongest). Game item text itself carries no tier; even
//! PoB2 only computes it on the fly in its Crafting simulator
//! (`ItemsTab.lua:924`: collect the affix pool sharing a `group` -> filter to
//! what's rollable on this base by spawn weight -> sort -> rank position).
//! This module runs the same algorithm in reverse: starting from a mod
//! line's text, look up which mod it belongs to, then rank it within that pool.
//!
//! Matching pipeline: line text -> numeric skeletonization -> reverse lookup
//! against StatDescriptions templates for the stat_id + captured values ->
//! locate the mod within the affix pool by (matching stat set + captured
//! values fall within its roll range + domain/spawn weight applicability) ->
//! rank ascending by level within the (group, generation_type, stat set) pool.
//!
//! Source-owned optional branches match complete raw integer intervals through
//! their shared display formatter, conditions and first-match precedence.
//! Multiple matching mods remain unlabeled. Legacy templates without metadata
//! keep the original matching behavior, including their unit limitations.
//!
//! The following remain outside the rollable pool or whole-line contract:
//! - Essence/corrupted/exclusive affixes (spawn weight is all zero) — not in
//!   the rollable pool, so unlabeled;
//! - Cross-line hybrids (one mod rendered across multiple lines) — a single
//!   line's stat set won't equal the mod's, so unlabeled.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use pobr_data::catalog::stat_descriptions::{StatDescriptionsDef, StatDescriptor};

mod branches;
use pobr_data::catalog::{ModDef, ModStat, SpawnWeight};

/// GGG `Mods.GenerationType`: 1 = prefix, 2 = suffix (other generation types don't participate in tiering).
const GENERATION_PREFIX: u32 = 1;
const GENERATION_SUFFIX: u32 = 2;

/// The tier verdict for one affix line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TierInfo {
    /// Rank, where 1 = strongest in the pool.
    pub tier: u32,
    /// The total number of tiers rollable on this base within the pool.
    pub total: u32,
    /// Whether it's a prefix (false = suffix).
    pub is_prefix: bool,
    /// The affix name (e.g. `of the Brute`; empty for unnamed internal mods).
    pub affix_name: String,
    /// The matched mod's stable ID (e.g. `Strength5`), for debugging/tracing.
    pub mod_id: String,
}

/// A template slot: either a numeric capture position (mapped to the k-th
/// stat) or a literal constant that must match exactly.
#[derive(Debug, Clone, PartialEq)]
enum Slot {
    Value(usize),
    Literal(f64),
}

/// One reverse-lookupable description template (single and compound entries
/// are normalized to this common shape).
#[derive(Debug, Clone)]
struct TemplateEntry {
    /// Capture index k -> stat_id. Always 1 entry for single; member order for compound.
    stat_ids: Vec<String>,
    /// Numeric slots, in text order.
    slots: Vec<Slot>,
    descriptor: Option<(Arc<StatDescriptor>, usize)>,
    lookup_key: Option<(usize, i64)>,
}

/// An affix entry participating in tier ranking (the fields needed, pulled from [`ModDef`]).
#[derive(Debug, Clone)]
struct TierMod {
    id: String,
    affix_name: String,
    group: String,
    generation_type: u32,
    domain: u32,
    level: u32,
    stats: Vec<ModStat>,
    spawn_weights: Vec<SpawnWeight>,
}

/// The affix-tier reverse-lookup index (built once, queried per line).
#[derive(Debug, Default)]
pub struct TierIndex {
    /// Skeleton text -> candidate templates.
    templates: HashMap<String, Vec<TemplateEntry>>,
    /// Stat-set key (sorted stat_ids joined by `\n`) -> candidate mod indices.
    pools: HashMap<String, Vec<usize>>,
    mods: Vec<TierMod>,
}

impl TierIndex {
    /// Whether there's no usable data (an old data pack missing group/spawn_weights, or missing StatDescriptions).
    pub fn is_empty(&self) -> bool {
        self.templates.is_empty() || self.mods.is_empty()
    }

    /// Builds the reverse-lookup index from the affix pool plus the StatDescriptions overlay.
    ///
    /// Only collects affixes that are prefix/suffix, carry a group, and have
    /// non-empty spawn_weights (i.e. the rollable pool); templates only use
    /// the root `stat_descriptions` scope (the equipment mod domain).
    pub fn build(mods: &[ModDef], descriptions: &StatDescriptionsDef) -> Self {
        let mut index = TierIndex::default();

        for m in mods {
            let Some(group) = m.group.as_ref() else {
                continue;
            };
            let generation_type = m.generation_type.unwrap_or(0);
            if generation_type != GENERATION_PREFIX && generation_type != GENERATION_SUFFIX {
                continue;
            }
            if m.spawn_weights.is_empty() || m.stats.is_empty() {
                continue;
            }
            let key = stat_key(m.stats.iter().map(|s| s.stat_id.as_str()));
            let idx = index.mods.len();
            index.mods.push(TierMod {
                id: m.id.clone(),
                affix_name: m.name.clone().unwrap_or_default(),
                group: group.clone(),
                generation_type,
                domain: m.domain,
                level: m.level,
                stats: m.stats.clone(),
                spawn_weights: m.spawn_weights.clone(),
            });
            index.pools.entry(key).or_default().push(idx);
        }

        let Some(scope) = descriptions.scopes.get("stat_descriptions") else {
            return index;
        };
        let owned: BTreeSet<_> = scope
            .descriptors
            .values()
            .flat_map(|d| &d.member_stats)
            .collect();
        for descriptor in scope.descriptors.values() {
            if !branches::valid(descriptor) {
                continue;
            }
            let descriptor = Arc::new(descriptor.clone());
            let pool = index.pools.get(&stat_key(
                descriptor.member_stats.iter().map(String::as_str),
            ));
            for (branch, b) in descriptor.branches.iter().enumerate() {
                let lookup = b
                    .lookups
                    .iter()
                    .enumerate()
                    .find_map(|(i, l)| l.as_ref().map(|name| (i, name)));
                let templates = if let Some((member, handler)) = lookup {
                    let Some(pool) = pool else {
                        continue;
                    };
                    let Some(mapping) = scope.lookup_values.get(handler) else {
                        continue;
                    };
                    let mut keys = BTreeSet::new();
                    for &mod_index in pool {
                        if let Some(stat) = index.mods[mod_index]
                            .stats
                            .iter()
                            .find(|s| s.stat_id == descriptor.member_stats[member])
                        {
                            if stat.min > stat.max {
                                continue;
                            }
                            for (&key, _) in mapping.range(stat.min..=stat.max) {
                                let c = &b.conditions[member];
                                if c.min.is_none_or(|m| m <= key)
                                    && c.max.is_none_or(|m| m >= key)
                                    && c.not != Some(key)
                                {
                                    keys.insert(key);
                                }
                            }
                        }
                    }
                    keys.into_iter()
                        .filter_map(|key| {
                            branches::literal_template(&b.template, member, &mapping[&key])
                                .map(|text| (text, Some((member, key))))
                        })
                        .collect::<Vec<_>>()
                } else {
                    vec![(b.template.clone(), None)]
                };
                for (template, lookup_key) in templates {
                    if let Some((skeleton, slots)) = compound_template_slots(&template) {
                        if slots.iter().any(
                            |s| matches!(s, Slot::Value(k) if *k >= descriptor.member_stats.len()),
                        ) {
                            continue;
                        }
                        index
                            .templates
                            .entry(skeleton)
                            .or_default()
                            .push(TemplateEntry {
                                stat_ids: descriptor.member_stats.clone(),
                                slots,
                                descriptor: Some((Arc::clone(&descriptor), branch)),
                                lookup_key,
                            });
                    }
                }
            }
        }
        for (stat_id, lines) in &scope.single {
            if owned.contains(stat_id) {
                continue;
            }
            for line in lines {
                if let Some((skeleton, slots)) = single_template_slots(line) {
                    index
                        .templates
                        .entry(skeleton)
                        .or_default()
                        .push(TemplateEntry {
                            stat_ids: vec![stat_id.clone()],
                            slots,
                            descriptor: None,
                            lookup_key: None,
                        });
                }
            }
        }
        for compound in scope.compound.values() {
            if compound.member_stats.iter().any(|s| owned.contains(s)) {
                continue;
            }
            if let Some((skeleton, slots)) = compound_template_slots(&compound.template) {
                index
                    .templates
                    .entry(skeleton)
                    .or_default()
                    .push(TemplateEntry {
                        stat_ids: compound.member_stats.clone(),
                        slots,
                        descriptor: None,
                        lookup_key: None,
                    });
            }
        }
        index
    }

    /// Reverse-looks-up a (already annotation-stripped) mod line: on a
    /// match, returns its tier on the given base.
    ///
    /// `base_tags` = the base's `BaseItemDef::tags`; `mod_domain` = the
    /// base's `BaseItemDef::mod_domain` (the affix's domain must match).
    pub fn lookup(&self, line: &str, base_tags: &[String], mod_domain: u32) -> Option<TierInfo> {
        let (skeleton, exact_captures) = tokenize(line);
        let captures = exact_captures
            .iter()
            .map(|s| s.parse::<f64>().ok())
            .collect::<Option<Vec<_>>>()?;
        let tags: BTreeSet<&str> = base_tags.iter().map(String::as_str).collect();
        let mut best: Option<TierInfo> = None;
        let mut matched = std::collections::BTreeMap::new();
        let mut metadata_match = false;
        let mut lookup_matches = BTreeSet::new();

        for entry in self.templates.get(&skeleton)? {
            let Some(values) = bind_captures(entry, &captures) else {
                continue;
            };
            let key = stat_key(entry.stat_ids.iter().map(String::as_str));
            let Some(pool) = self.pools.get(&key) else {
                continue;
            };
            // Candidates sharing this stat set that are rollable on this
            // base (spans groups / mixes prefix and suffix; split into
            // pools below after matching).
            let spawnable: Vec<usize> = pool
                .iter()
                .copied()
                .filter(|&i| {
                    let m = &self.mods[i];
                    m.domain == mod_domain && spawn_weight_positive(&m.spawn_weights, &tags)
                })
                .collect();
            for &i in &spawnable {
                let m = &self.mods[i];
                if let Some((descriptor, branch)) = &entry.descriptor {
                    if !branches::matches(&m.stats, entry, descriptor, *branch, &exact_captures) {
                        continue;
                    }
                } else if !values_in_range(&m.stats, &entry.stat_ids, &values) {
                    continue;
                }
                // Only mods sharing (group, prefix/suffix) are tiers of each
                // other; rank ascending by level = weakest to strongest.
                let mut ranked: Vec<&TierMod> = spawnable
                    .iter()
                    .map(|&j| &self.mods[j])
                    .filter(|c| c.group == m.group && c.generation_type == m.generation_type)
                    .collect();
                ranked.sort_by_key(|c| (c.level, stat_magnitude(&c.stats)));
                let pos = ranked.iter().position(|c| c.id == m.id)?;
                let info = TierInfo {
                    tier: (ranked.len() - pos) as u32,
                    total: ranked.len() as u32,
                    is_prefix: m.generation_type == GENERATION_PREFIX,
                    affix_name: m.affix_name.clone(),
                    mod_id: m.id.clone(),
                };
                metadata_match |= entry.descriptor.is_some();
                if let Some((member, key)) = entry.lookup_key {
                    lookup_matches.insert((entry.stat_ids.clone(), member, key));
                }
                matched.insert(info.mod_id.clone(), info.clone());
                // Overlapping ranges can produce multiple matches; keep the
                // strongest tier (ponytail: boundary-value ambiguity is rare, picking the best is fine).
                if best.as_ref().is_none_or(|b| info.tier < b.tier) {
                    best = Some(info);
                }
            }
        }
        if metadata_match {
            if matched.len() == 1 && lookup_matches.len() <= 1 {
                matched.into_values().next()
            } else {
                None
            }
        } else {
            best
        }
    }
}

/// A stat-set key: sorted, deduplicated, joined by `\n` (order-independent).
fn stat_key<'a>(ids: impl Iterator<Item = &'a str>) -> String {
    let set: BTreeSet<&str> = ids.collect();
    set.into_iter().collect::<Vec<_>>().join("\n")
}

/// The magnitude used as a ranking tiebreak: the absolute value of the first stat's upper roll bound.
fn stat_magnitude(stats: &[ModStat]) -> u64 {
    stats
        .first()
        .map_or(0, |s| s.max.unsigned_abs().max(s.min.unsigned_abs()))
}

/// Skeletonizes a line of text: replaces each numeric run
/// (`[+-]?\d+(\.\d+)?`) with `#`, capturing the values in order.
fn skeletonize(text: &str) -> (String, Vec<f64>) {
    let (skeleton, tokens) = tokenize(text);
    (
        skeleton,
        tokens.into_iter().filter_map(|s| s.parse().ok()).collect(),
    )
}

fn tokenize(text: &str) -> (String, Vec<&str>) {
    let mut skeleton = String::with_capacity(text.len());
    let mut captures = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        let signed = (c == b'+' || c == b'-') && bytes.get(i + 1).is_some_and(u8::is_ascii_digit);
        if c.is_ascii_digit() || signed {
            let start = i;
            if signed {
                i += 1;
            }
            while bytes.get(i).is_some_and(u8::is_ascii_digit) {
                i += 1;
            }
            if bytes.get(i) == Some(&b'.') && bytes.get(i + 1).is_some_and(u8::is_ascii_digit) {
                i += 1;
                while bytes.get(i).is_some_and(u8::is_ascii_digit) {
                    i += 1;
                }
            }
            if text[start..i].parse::<f64>().is_ok() {
                captures.push(&text[start..i]);
                skeleton.push('#');
                continue;
            }
            // Parse failure: keep the original text (theoretically unreachable).
            skeleton.push_str(&text[start..i]);
            continue;
        }
        // Byte-by-byte advance: multi-byte UTF-8 not starting with a digit is copied wholesale.
        let ch_len = utf8_len(c);
        skeleton.push_str(&text[i..i + ch_len]);
        i += ch_len;
    }
    (skeleton, captures)
}

fn utf8_len(first_byte: u8) -> usize {
    match first_byte {
        b if b >= 0xF0 => 4,
        b if b >= 0xE0 => 3,
        b if b >= 0xC0 => 2,
        _ => 1,
    }
}

/// A single template (rendered at V=1, where the placeholder value is
/// always literal `1`) -> skeleton plus slots.
///
/// A numeric position whose value is 1 is treated as a capture slot (always
/// mapped to stat 0); every other numeric value is a template constant (e.g.
/// the 3 in `per 3 Red Support Gems`) that must match literally. Templates
/// with no numeric positions at all (boolean mods) are also collected.
fn single_template_slots(template: &str) -> Option<(String, Vec<Slot>)> {
    let (skeleton, values) = skeletonize(template);
    let mut slots = Vec::with_capacity(values.len());
    let mut has_value_slot = false;
    for v in values {
        if (v - 1.0).abs() < f64::EPSILON {
            slots.push(Slot::Value(0));
            has_value_slot = true;
        } else {
            slots.push(Slot::Literal(v));
        }
    }
    // Has numeric positions but they're all constants -> no capture slot to anchor on, so give up on this template.
    if !slots.is_empty() && !has_value_slot {
        return None;
    }
    Some((skeleton, slots))
}

/// A compound template (`{0}`/`{1}` placeholders plus possible literal constants) -> skeleton plus slots.
fn compound_template_slots(template: &str) -> Option<(String, Vec<Slot>)> {
    // First replace {k} (including the {k:...} format spec) with a
    // placeholder marker, then skeletonize the literal numbers.
    let mut rendered = String::with_capacity(template.len());
    let mut placeholder_order = Vec::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let close = rest[open..].find('}')? + open;
        let inner = &rest[open + 1..close];
        let key = inner.split(':').next().unwrap_or(inner);
        let k: usize = key.parse().ok()?;
        rendered.push_str(&rest[..open]);
        rendered.push('\u{1}'); // placeholder sentinel (never appears in real game text)
        placeholder_order.push(k);
        rest = &rest[close + 1..];
    }
    rendered.push_str(rest);

    let mut slots = Vec::new();
    let mut ph = placeholder_order.into_iter();
    let mut skeleton = String::with_capacity(rendered.len());
    // Both sentinels and literal numbers become `#`; slots are interleaved back in text order.
    for piece in split_keep_sentinel(&rendered) {
        match piece {
            Piece::Sentinel => {
                slots.push(Slot::Value(ph.next()?));
                skeleton.push('#');
            }
            Piece::Text(t) => {
                let (skel, values) = skeletonize(t);
                skeleton.push_str(&skel);
                slots.extend(values.into_iter().map(Slot::Literal));
            }
        }
    }
    Some((skeleton, slots))
}

enum Piece<'a> {
    Sentinel,
    Text(&'a str),
}

fn split_keep_sentinel(s: &str) -> impl Iterator<Item = Piece<'_>> {
    s.split_inclusive('\u{1}').flat_map(|chunk| {
        if let Some(text) = chunk.strip_suffix('\u{1}') {
            vec![Piece::Text(text), Piece::Sentinel]
        } else {
            vec![Piece::Text(chunk)]
        }
    })
}

/// Binds a line's captured values to the template's slots: constant
/// positions must match exactly; a capture position reused multiple times
/// must agree each time. Returns `values[k]` = the k-th stat's line value
/// (a boolean template returns an all-`None` binding).
fn bind_captures(entry: &TemplateEntry, captures: &[f64]) -> Option<Vec<Option<f64>>> {
    if captures.len() != entry.slots.len() {
        return None;
    }
    let mut values: Vec<Option<f64>> = vec![None; entry.stat_ids.len()];
    for (cap, slot) in captures.iter().zip(&entry.slots) {
        match slot {
            Slot::Literal(v) => {
                if (cap - v).abs() > 1e-9 {
                    return None;
                }
            }
            Slot::Value(k) => match values.get(*k) {
                Some(None) => values[*k] = Some(*cap),
                Some(Some(prev)) if (prev - cap).abs() < 1e-9 => {}
                _ => return None,
            },
        }
    }
    Some(values)
}

/// Whether every captured value falls within that mod's roll range for its
/// stat (also checking the negated-roll mirror).
fn values_in_range(stats: &[ModStat], stat_ids: &[String], values: &[Option<f64>]) -> bool {
    if stats.len() != stat_ids.len() {
        return false;
    }
    for stat in stats {
        let Some(k) = stat_ids.iter().position(|id| id == &stat.stat_id) else {
            return false;
        };
        let Some(Some(v)) = values.get(k) else {
            // Boolean template: no captured value, matched purely by stat set and pool.
            continue;
        };
        let (min, max) = (stat.min as f64, stat.max as f64);
        let hit = (*v >= min && *v <= max) || (-*v >= min && -*v <= max);
        if !hit {
            return false;
        }
    }
    true
}

/// Mirrors PoB2 `Item:GetModSpawnWeight` semantics: takes the first weight
/// entry whose tag matches a base tag, in order; `default` always matches as a fallback.
fn spawn_weight_positive(weights: &[SpawnWeight], base_tags: &BTreeSet<&str>) -> bool {
    for w in weights {
        if w.tag == "default" || base_tags.contains(w.tag.as_str()) {
            return w.weight > 0;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use pobr_data::catalog::stat_descriptions::ScopeDescriptions;
    use std::collections::BTreeMap;

    fn mod_def(
        id: &str,
        name: &str,
        generation_type: u32,
        level: u32,
        stats: &[(&str, i64, i64)],
        weights: &[(&str, u32)],
        group: &str,
    ) -> ModDef {
        ModDef {
            id: id.into(),
            name: Some(name.into()),
            mod_type: None,
            domain: 1,
            generation_type: Some(generation_type),
            level,
            stats: stats
                .iter()
                .map(|(sid, min, max)| ModStat {
                    stat_id: (*sid).into(),
                    min: *min,
                    max: *max,
                })
                .collect(),
            tags: Vec::new(),
            group: Some(group.into()),
            families: None,
            spawn_weights: weights
                .iter()
                .map(|(t, w)| SpawnWeight {
                    tag: (*t).into(),
                    weight: *w,
                })
                .collect(),
        }
    }

    fn descriptions() -> StatDescriptionsDef {
        let mut scope = ScopeDescriptions::default();
        scope
            .single
            .insert("additional_strength".into(), vec!["+1 to Strength".into()]);
        scope.compound.insert(
            "local_minimum_added_fire_damage".into(),
            pobr_data::catalog::stat_descriptions::CompoundDescription {
                member_stats: vec![
                    "local_minimum_added_fire_damage".into(),
                    "local_maximum_added_fire_damage".into(),
                ],
                template: "Adds {0} to {1} Fire Damage".into(),
            },
        );
        let mut def = StatDescriptionsDef::default();
        def.scopes.insert("stat_descriptions".into(), scope);
        def
    }

    fn strength_pool() -> Vec<ModDef> {
        vec![
            mod_def(
                "Strength1",
                "of the Brute",
                2,
                1,
                &[("additional_strength", 5, 8)],
                &[("ring", 1), ("belt", 1), ("default", 0)],
                "Strength",
            ),
            mod_def(
                "Strength2",
                "of the Wrestler",
                2,
                15,
                &[("additional_strength", 9, 12)],
                &[("ring", 1), ("belt", 1), ("default", 0)],
                "Strength",
            ),
            mod_def(
                "Strength3",
                "of the Bear",
                2,
                30,
                &[("additional_strength", 13, 17)],
                &[("ring", 1), ("belt", 1), ("default", 0)],
                "Strength",
            ),
            // A higher tier that only rolls on belts: shouldn't enter the ring pool.
            mod_def(
                "Strength4",
                "of the Goliath",
                2,
                44,
                &[("additional_strength", 18, 22)],
                &[("belt", 1), ("default", 0)],
                "Strength",
            ),
        ]
    }

    #[test]
    fn ranks_tier_within_spawnable_group() {
        let index = TierIndex::build(&strength_pool(), &descriptions());
        let ring_tags = vec!["ring".to_string()];
        let info = index.lookup("+10 to Strength", &ring_tags, 1).unwrap();
        assert_eq!(info.mod_id, "Strength2");
        assert_eq!(info.tier, 2); // the ring pool only has tiers 1..3, 10 lands in the middle tier
        assert_eq!(info.total, 3);
        assert!(!info.is_prefix);
        assert_eq!(info.affix_name, "of the Wrestler");

        // The top tier is T1 in the ring pool.
        let top = index.lookup("+16 to Strength", &ring_tags, 1).unwrap();
        assert_eq!(top.tier, 1);

        // The belt pool includes a 4th tier, so the same line is T3/4 on belt.
        let belt = index
            .lookup("+16 to Strength", &["belt".to_string()], 1)
            .unwrap();
        assert_eq!((belt.tier, belt.total), (2, 4));
    }

    #[test]
    fn compound_flat_damage_matches_both_ranges() {
        let mut mods = strength_pool();
        mods.push(mod_def(
            "FireDamage1",
            "Heated",
            1,
            1,
            &[
                ("local_minimum_added_fire_damage", 1, 5),
                ("local_maximum_added_fire_damage", 6, 10),
            ],
            &[("ring", 1), ("default", 0)],
            "FireDamage",
        ));
        mods.push(mod_def(
            "FireDamage2",
            "Smouldering",
            1,
            20,
            &[
                ("local_minimum_added_fire_damage", 6, 12),
                ("local_maximum_added_fire_damage", 13, 22),
            ],
            &[("ring", 1), ("default", 0)],
            "FireDamage",
        ));
        let index = TierIndex::build(&mods, &descriptions());
        let info = index
            .lookup("Adds 8 to 15 Fire Damage", &["ring".to_string()], 1)
            .unwrap();
        assert_eq!(info.mod_id, "FireDamage2");
        assert_eq!((info.tier, info.total), (1, 2));
        assert!(info.is_prefix);
    }

    #[test]
    fn no_match_outside_ranges_or_domain_or_tags() {
        let index = TierIndex::build(&strength_pool(), &descriptions());
        // Value outside every range.
        assert!(
            index
                .lookup("+99 to Strength", &["ring".to_string()], 1)
                .is_none()
        );
        // Domain mismatch (flask=2).
        assert!(
            index
                .lookup("+10 to Strength", &["ring".to_string()], 2)
                .is_none()
        );
        // Base tag not rollable (falls back to the default weight of 0).
        assert!(
            index
                .lookup("+10 to Strength", &["amulet".to_string()], 1)
                .is_none()
        );
        // Completely unrelated text.
        assert!(
            index
                .lookup("Corrupted", &["ring".to_string()], 1)
                .is_none()
        );
    }
    #[test]
    fn source_branches_reverse_scaled_compound_values_and_reject_ambiguous_tiers() {
        use pobr_data::catalog::stat_descriptions::{
            DescriptionBranch, StatCondition, StatDescriptor, StatValueFormat,
        };
        let mut descriptions = StatDescriptionsDef::default();
        let scope = descriptions
            .scopes
            .entry("stat_descriptions".into())
            .or_default();
        scope.descriptors.insert(
            "regen".into(),
            StatDescriptor {
                member_stats: vec!["regen".into()],
                branches: vec![DescriptionBranch {
                    template: "Regenerate {0} Life per second".into(),
                    conditions: vec![StatCondition::default()],
                    lookups: Vec::new(),
                    formats: vec![StatValueFormat {
                        divisor: 60,
                        precision: Some(1),
                        ..Default::default()
                    }],
                }],
            },
        );
        scope
            .single
            .insert("regen".into(), vec!["Regenerate 1 Life per second".into()]);
        scope.descriptors.insert(
            "duration".into(),
            StatDescriptor {
                member_stats: vec!["duration".into(), "chance".into()],
                branches: vec![DescriptionBranch {
                    template: "{1}% chance for {0} seconds".into(),
                    conditions: vec![StatCondition::default(); 2],
                    lookups: Vec::new(),
                    formats: vec![
                        StatValueFormat {
                            divisor: 1000,
                            ..Default::default()
                        },
                        StatValueFormat {
                            offset: 200,
                            ..Default::default()
                        },
                    ],
                }],
            },
        );
        let mut mods = vec![
            mod_def(
                "Regen",
                "",
                1,
                1,
                &[("regen", 150, 150)],
                &[("ring", 1)],
                "Regen",
            ),
            mod_def(
                "Compound",
                "",
                1,
                1,
                &[("chance", 25, 25), ("duration", 1500, 1500)],
                &[("ring", 1)],
                "Compound",
            ),
        ];
        let tags = vec!["ring".into()];
        let index = TierIndex::build(&mods, &descriptions);
        assert_eq!(
            index
                .lookup("Regenerate 2.5 Life per second", &tags, 1)
                .unwrap()
                .mod_id,
            "Regen"
        );
        assert_eq!(
            index
                .lookup("225% chance for 1.5 seconds", &tags, 1)
                .unwrap()
                .mod_id,
            "Compound"
        );
        assert!(
            index
                .lookup("Regenerate 150 Life per second", &tags, 1)
                .is_none()
        );
        // A distinct raw mod maps to the same rounded display. Neither tier is certain.
        mods.push(mod_def(
            "RoundedOther",
            "",
            1,
            2,
            &[("regen", 151, 151)],
            &[("ring", 1)],
            "Regen",
        ));
        assert!(
            TierIndex::build(&mods, &descriptions)
                .lookup("Regenerate 2.5 Life per second", &tags, 1)
                .is_none()
        );
    }
    #[test]
    fn lookup_templates_bind_source_keys_only_in_actual_pools_and_refuse_same_name_ambiguity() {
        use pobr_data::catalog::stat_descriptions::{
            DescriptionBranch, StatCondition, StatDescriptor, StatValueFormat,
        };
        let mut descriptions = StatDescriptionsDef::default();
        let scope = descriptions
            .scopes
            .entry("stat_descriptions".into())
            .or_default();
        scope.lookup_values.insert(
            "passive_hash".into(),
            BTreeMap::from([
                (-32768, "Named passive".into()),
                (-32767, "Other passive".into()),
            ]),
        );
        scope.descriptors.insert(
            "key".into(),
            StatDescriptor {
                member_stats: vec!["key".into()],
                branches: vec![DescriptionBranch {
                    template: "Allocates {0}".into(),
                    conditions: vec![StatCondition::default()],
                    formats: vec![StatValueFormat::default()],
                    lookups: vec![Some("passive_hash".into())],
                }],
            },
        );
        assert!(TierIndex::build(&[], &descriptions).templates.is_empty());
        let mut mods = vec![mod_def(
            "Named",
            "",
            1,
            1,
            &[("key", -32768, -32768)],
            &[("ring", 1)],
            "Named",
        )];
        let tags = vec!["ring".into()];
        assert_eq!(
            TierIndex::build(&mods, &descriptions)
                .lookup("Allocates Named passive", &tags, 1)
                .unwrap()
                .mod_id,
            "Named"
        );
        assert!(
            TierIndex::build(&mods, &descriptions)
                .lookup("Allocates Other passive", &tags, 1)
                .is_none()
        );
        let mut missing = descriptions.clone();
        missing
            .scopes
            .get_mut("stat_descriptions")
            .unwrap()
            .lookup_values
            .clear();
        assert!(TierIndex::build(&mods, &missing).templates.is_empty());
        descriptions
            .scopes
            .get_mut("stat_descriptions")
            .unwrap()
            .lookup_values
            .get_mut("passive_hash")
            .unwrap()
            .insert(-32767, "Named passive".into());
        mods[0].stats[0].max = -32767;
        // One mod permits two raw keys with the same display: key identity is uncertain.
        assert!(
            TierIndex::build(&mods, &descriptions)
                .lookup("Allocates Named passive", &tags, 1)
                .is_none()
        );
        mods.push(mod_def(
            "Other",
            "",
            1,
            2,
            &[("key", -32767, -32767)],
            &[("ring", 1)],
            "Named",
        ));
        assert!(
            TierIndex::build(&mods, &descriptions)
                .lookup("Allocates Named passive", &tags, 1)
                .is_none()
        );
    }
    #[test]
    fn full_signed_raw_limit_does_not_overflow_tier_ranking() {
        use pobr_data::catalog::stat_descriptions::{
            DescriptionBranch, StatCondition, StatDescriptor, StatValueFormat,
        };
        let mut descriptions = StatDescriptionsDef::default();
        descriptions
            .scopes
            .entry("stat_descriptions".into())
            .or_default()
            .descriptors
            .insert(
                "signed".into(),
                StatDescriptor {
                    member_stats: vec!["signed".into()],
                    branches: vec![DescriptionBranch {
                        template: "Signed {0}".into(),
                        conditions: vec![StatCondition::default()],
                        formats: vec![StatValueFormat::default()],
                        lookups: Vec::new(),
                    }],
                },
            );
        let mods = vec![mod_def(
            "Signed",
            "",
            1,
            1,
            &[("signed", i64::MIN, i64::MIN)],
            &[("ring", 1)],
            "Signed",
        )];
        assert_eq!(
            TierIndex::build(&mods, &descriptions)
                .lookup("Signed -9223372036854775808", &["ring".into()], 1)
                .unwrap()
                .mod_id,
            "Signed"
        );
    }
}
