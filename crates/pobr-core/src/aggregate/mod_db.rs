use std::collections::HashMap;

use pobr_data::catalog::high_precision_mods::HighPrecisionModsDef;
use pobr_data::prelude::*;

use crate::{
    CalcConfig, EvalContext, ModTag, ModValue, Modifier, TraceGraph, TraceNodeId, TraceOperation,
    TracedValue,
};

/// Normalizes a single modName's MORE product to PoB2's default precision
/// `round(·, 2)` (ModList.lua MoreInternal).
fn round_more(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

/// PoB2's `round(val, dec)` (vendor `Common.lua:648-654`):
/// `m_floor(val × 10^dec + 0.5) / 10^dec` — always rounds half up (not
/// banker's rounding; differs from Rust's `f64::round` at `.5` for negative
/// numbers, so this function must be used to match vendor).
fn pob_round(value: f64, dec: i32) -> f64 {
    let mult = 10f64.powi(dec);
    (value * mult + 0.5).floor() / mult
}

/// Rounding-precision rules: vendor's `data.highPrecisionMods` +
/// `data.defaultHighPrecision` (`Data.lua:413-530`, stored on disk as
/// `overlay/high_precision_mods.json`, injected via pobr-gamedata's
/// `RuleSet`).
///
/// `Default` (no data injected) is the **no exception table** fallback
/// (defaults to `round(·,2)` truncation; a fractional original value still
/// goes through `default_high_precision = 1` — this constant is value-equal
/// to the on-disk JSON, a migration invariant). Consumers:
/// [`ModDb::scale_add_mod`] and the MORE aggregation precision-exception
/// branch.
#[derive(Debug, Clone, Default)]
pub struct HighPrecisionRules {
    def: Option<HighPrecisionModsDef>,
}

impl HighPrecisionRules {
    /// Constructs from the on-disk table (pobr-gamedata's `RuleSet::high_precision_mods` → here).
    pub fn from_def(def: HighPrecisionModsDef) -> Self {
        Self { def: Some(def) }
    }

    /// `data.highPrecisionMods[name][type]` (vendor `ModStore.lua:69` /
    /// `ModDB.lua:175-180`). The mod type key uses vendor's literal
    /// (`BASE`/`MORE`…, = [`ModType::as_trace_label`]). Returns `None` when
    /// not injected / no match.
    pub fn precision_for(&self, name: &str, mod_type: ModType) -> Option<u32> {
        self.def
            .as_ref()?
            .mods
            .get(name)?
            .get(mod_type.as_trace_label())
            .copied()
    }

    /// `data.defaultHighPrecision` (`Data.lua:413` = 1; same-value fallback when not injected).
    pub fn default_high_precision(&self) -> u32 {
        self.def.as_ref().map_or(1, |d| d.default_high_precision)
    }
}

/// ScaleAddMod's value rounding (a line-by-line port of vendor `ModStore.lua:69-77`):
/// - precision = the exception table's `[name][type]`; when missing and
///   **the original value has a fractional part**, falls back to
///   `defaultHighPrecision`;
/// - with precision `p` → `m_floor(value × scale × 10^p) / 10^p` (floor, not rounding);
/// - without precision → `m_modf(round(value × scale, 2))` takes the integer part (truncated toward zero).
fn scale_mod_value(
    name: &str,
    mod_type: ModType,
    value: f64,
    scale: f64,
    rules: &HighPrecisionRules,
) -> f64 {
    let precision = rules
        .precision_for(name, mod_type)
        .or_else(|| (value.floor() != value).then(|| rules.default_high_precision()));
    match precision {
        Some(p) => {
            let power = 10f64.powi(p as i32);
            (value * scale * power).floor() / power
        }
        None => pob_round(value * scale, 2).trunc(),
    }
}

/// The GlobalLimit accounting table for a single aggregate query (vendor
/// creates a fresh `local globalLimits = { }` on every Sum/More/Tabulate
/// call, ModDB.lua:133/159/269). Lazily allocated: zero overhead when there's
/// no [`ModTag::GlobalLimit`] mod (no table is built on the hot path).
type GlobalLimits = Option<HashMap<String, f64>>;

/// The globalLimit accounting at the tail of EvalMod (a line-by-line port of
/// vendor ModStore.lua:895-905): effective values sharing the same `key` are
/// capped cumulatively — when `used + value > limit`, clips to the remaining
/// balance, then records it. Returns (the clipped value, `Some(original
/// value)` if clipping occurred — the traced path attaches a Clamp node
/// based on this).
///
/// `#[inline]` + the caller-side `tags.is_empty()` fast path: mod_db
/// aggregation is a hot path (gated by mod_db_bench), so mods with no tags
/// must have zero overhead.
#[inline]
fn apply_global_limits(
    modifier: &Modifier,
    mut value: f64,
    limits: &mut GlobalLimits,
) -> (f64, Option<f64>) {
    let mut clamped_from = None;
    for tag in &modifier.tags {
        if let ModTag::GlobalLimit { value: limit, key } = tag {
            let used = limits
                .get_or_insert_with(HashMap::new)
                .entry(key.clone())
                .or_insert(0.0);
            if *used + value > *limit {
                if clamped_from.is_none() {
                    clamped_from = Some(value);
                }
                value = *limit - *used;
            }
            *used += value;
        }
    }
    (value, clamped_from)
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModContribution {
    pub name: ModName,
    pub mod_type: ModType,
    pub value: f64,
    pub origin: Option<ModifierSource>,
    pub raw_text: Option<String>,
    /// The original effective value before [`ModTag::GlobalLimit`] clipping
    /// (`Some` means this entry was clipped by the cumulative cap; the
    /// traced path attaches a Clamp node based on this).
    pub clamped_from: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct ModDb {
    mods: HashMap<ModName, Vec<Modifier>>,
    /// Rounding-precision rules: vendor treats `data.highPrecisionMods` as
    /// global environment data, while pobr keeps it scoped to the db
    /// instance (the calc core stays zero-I/O; the orchestration layer
    /// injects it via [`Self::set_high_precision_rules`]). `Default` = no
    /// exception table → MORE aggregation takes the default `round(·,2)`
    /// branch, unchanged from before this field existed.
    high_precision: HighPrecisionRules,
    /// The set of names with a [`ModTag::GlobalLimit`] mod (a performance
    /// fork: [`Self::sum`] only takes the accounting slow path when a
    /// queried name hits this set — the accounting closure defeats the pure
    /// summation chain's optimization, measured at +50% in benchmarks).
    /// Maintained on write (`add_mod`/`replace_mod`); not reclaimed on
    /// removal (a stale false-positive only costs performance, not
    /// correctness).
    global_limit_names: std::collections::HashSet<ModName>,
}

impl ModDb {
    pub fn new() -> Self {
        Self::default()
    }

    /// Maintains [`Self::global_limit_names`] (one tags scan per write, zero cost at query time).
    fn note_global_limit(&mut self, modifier: &Modifier) {
        if modifier
            .tags
            .iter()
            .any(|tag| matches!(tag, ModTag::GlobalLimit { .. }))
        {
            self.global_limit_names.insert(modifier.name.clone());
        }
    }

    /// Whether the queried name set might include a GlobalLimit mod (the fast/slow path fork).
    #[inline]
    fn names_have_global_limit(&self, names: &[ModName]) -> bool {
        !self.global_limit_names.is_empty()
            && names
                .iter()
                .any(|name| self.global_limit_names.contains(name))
    }

    /// Injects the rounding-precision rules (sourced from pobr-gamedata's
    /// `RuleSet::high_precision_mods`). Consumed by the MORE aggregation
    /// precision-exception branch ([`Self::more`]) and the same rules the
    /// caller passes to [`Self::scale_add_mod`].
    pub fn set_high_precision_rules(&mut self, rules: HighPrecisionRules) {
        self.high_precision = rules;
    }

    pub fn add_mod(&mut self, modifier: Modifier) {
        self.note_global_limit(&modifier);
        self.mods
            .entry(modifier.name.clone())
            .or_default()
            .push(modifier);
    }

    pub fn add_list(&mut self, modifiers: impl IntoIterator<Item = Modifier>) {
        for modifier in modifiers {
            self.add_mod(modifier);
        }
    }

    /// The write-side primitive ReplaceMod (vendor `ModStore.lua:114-118` +
    /// `ModDB.lua:38-66`'s `ReplaceModInternal`): an existing mod matching
    /// `name + type + flags + keywordFlags + source` is **replaced in
    /// place** (preserving bucket order); with no match, appends instead
    /// (= [`add_mod`](Self::add_mod)).
    ///
    /// Returns whether a replacement occurred (`false` means the append
    /// branch was taken). A typical consumer: the crossbow reload's
    /// `Multiplier:BoltsReloadedPastSixSeconds` write-back
    /// (`CalcOffence.lua:2890-2894`, T4). Note: pobr's `ModDb` has no parent
    /// chain (vendor's parent recursion doesn't apply here).
    pub fn replace_mod(&mut self, modifier: Modifier) -> bool {
        self.note_global_limit(&modifier);
        if let Some(bucket) = self.mods.get_mut(&modifier.name)
            && let Some(slot) = bucket.iter_mut().find(|cur| {
                cur.mod_type == modifier.mod_type
                    && cur.flags == modifier.flags
                    && cur.keyword_flags == modifier.keyword_flags
                    && cur.source == modifier.source
            })
        {
            *slot = modifier;
            return true;
        }
        self.add_mod(modifier);
        false
    }

    /// The write-side primitive ConvertMod (vendor `ModStore.lua:120-132` +
    /// `ModDB.lua:75-105`'s `ConvertModInternal`): finds an existing mod in
    /// the `from` bucket matching `type + flags + keywordFlags + source`
    /// (compared against `to`), **removes it from the old name's bucket and
    /// lands `to` in the new name's bucket** (a cross-bucket move); with no
    /// match, just appends `to` directly.
    ///
    /// Returns whether a move occurred. A known divergence from vendor
    /// (tracked here): vendor marks mods with `converted` to prevent the
    /// same mod from being chain-converted twice; pobr's `Modifier` has no
    /// such marker — the case where `from` equals `to.name`, or a conversion
    /// chain loops back, isn't guarded against (no consumer currently
    /// triggers this; will be added if chain conversion shows up when T4/T5 land).
    pub fn convert_mod(&mut self, from: &ModName, to: Modifier) -> bool {
        if let Some(old_bucket) = self.mods.get_mut(from)
            && let Some(index) = old_bucket.iter().position(|cur| {
                cur.mod_type == to.mod_type
                    && cur.flags == to.flags
                    && cur.keyword_flags == to.keyword_flags
                    && cur.source == to.source
            })
        {
            old_bucket.remove(index);
            self.add_mod(to);
            return true;
        }
        self.add_mod(to);
        false
    }

    /// The write-side primitive ScaleAddMod (vendor `ModStore.lua:45-81`):
    /// scales the value by `scale` before storing; rounding goes through
    /// [`HighPrecisionRules`] (see [`scale_mod_value`] for the line-by-line
    /// branches). `scale == 1` stores directly (vendor :54).
    ///
    /// Known divergences from vendor (tracked here):
    /// - `effects.unscalable` (:46-52, non-scalable modifiers store their
    ///   original value) — pobr's `ModTag` has no such bit yet, so
    ///   everything is treated as scalable (no producer in the current parse
    ///   layer);
    /// - the `value.keyOfScaledMod` / `+level` floor special case (:59-66) —
    ///   for scaling non-`mod` keys inside a table value; pobr's `ModValue`
    ///   has no corresponding shape;
    /// - [`ModValue::NestedMods`] (vendor's `value.mod` nested payload)
    ///   scales each inner Number under the same rule (a generalization of
    ///   vendor :57's `subMod = scaledMod.value.mod` for multiple payloads).
    pub fn scale_add_mod(
        &mut self,
        mut modifier: Modifier,
        scale: f64,
        rules: &HighPrecisionRules,
    ) {
        if scale == 1.0 {
            self.add_mod(modifier);
            return;
        }
        match &mut modifier.value {
            ModValue::Number(value) => {
                *value = scale_mod_value(
                    modifier.name.as_str(),
                    modifier.mod_type,
                    *value,
                    scale,
                    rules,
                );
            }
            ModValue::NestedMods(nested) => {
                for inner in nested.iter_mut() {
                    if let ModValue::Number(value) = &mut inner.value {
                        *value = scale_mod_value(
                            inner.name.as_str(),
                            inner.mod_type,
                            *value,
                            scale,
                            rules,
                        );
                    }
                }
            }
            // Bool / text payloads aren't scaled (vendor only rounds the number branch, :68).
            ModValue::Bool(_) | ModValue::Text(_) => {}
        }
        self.add_mod(modifier);
    }

    /// Returns a new [`ModDb`] containing only the modifiers for which `keep`
    /// returns `true`, preserving insertion order within each [`ModName`] bucket.
    ///
    /// Used by marginal attribution to rebuild a build with a given source
    /// removed, without mutating the original db (read-only snapshot recompute).
    pub fn filtered(&self, mut keep: impl FnMut(&Modifier) -> bool) -> Self {
        let mods = self
            .mods
            .iter()
            .map(|(name, modifiers)| {
                (
                    name.clone(),
                    modifiers
                        .iter()
                        .filter(|modifier| keep(modifier))
                        .cloned()
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        Self {
            mods,
            high_precision: self.high_precision.clone(),
            // A copy of the full set (a stale false-positive is fine: a
            // filtered-out name just takes the slow path check once more).
            global_limit_names: self.global_limit_names.clone(),
        }
    }

    /// The parameter is upgraded to `impl Into<EvalContext>`: existing call
    /// sites passing `&cfg` compile unchanged; PerStat consumers pass an
    /// [`EvalContext`] with `stat_lookup`. Consumes [`ModTag::GlobalLimit`]
    /// within the aggregation loop (vendor's SumInternal passes a
    /// `globalLimits` table, ModDB.lua:131-154).
    pub fn sum<'a>(
        &self,
        mod_type: ModType,
        ctx: impl Into<EvalContext<'a>>,
        names: &[ModName],
    ) -> f64 {
        let ctx = ctx.into();
        // Fast path (the vast majority of queries): the name set has no
        // GlobalLimit mod → a pure summation chain (the accounting state
        // would defeat the chain's optimization, measured at +50% in
        // benchmarks, hence the fork instead of an inline check).
        if self.names_have_global_limit(names) {
            return self.sum_with_global_limits(mod_type, ctx, names);
        }
        names
            .iter()
            .filter_map(|name| self.mods.get(name))
            .flat_map(|mods| mods.iter())
            .filter(|modifier| modifier.mod_type == mod_type && modifier.matches(ctx.cfg))
            .filter_map(|modifier| modifier.effective_number_ref(&ctx))
            .sum()
    }

    /// The GlobalLimit accounting slow path for [`sum`](Self::sum) (vendor's
    /// SumInternal passes a `globalLimits` table, ModDB.lua:131-154).
    /// Value-equal to the fast path for mods without a GlobalLimit tag (the
    /// accounting only clips tagged entries).
    fn sum_with_global_limits(
        &self,
        mod_type: ModType,
        ctx: EvalContext<'_>,
        names: &[ModName],
    ) -> f64 {
        let mut total = 0.0;
        let mut limits: GlobalLimits = None;
        for name in names {
            for modifier in self.mods.get(name).into_iter().flatten() {
                if modifier.mod_type != mod_type || !modifier.matches(ctx.cfg) {
                    continue;
                }
                let Some(value) = modifier.effective_number_ref(&ctx) else {
                    continue;
                };
                total += apply_global_limits(modifier, value, &mut limits).0;
            }
        }
        total
    }

    /// Takes the **largest effective value** among a set of modifiers
    /// (exposure's `ExposureMin`/take-the-strongest semantics).
    ///
    /// PoB2's `CalcPerform.lua` aggregates exposure by settling each source
    /// then `magnitude = max(magnitude, value)` (**takes the single
    /// strongest source** rather than summing). This method only considers
    /// modifiers passing `matches(cfg)`, returning `0.0` for an empty set.
    ///
    /// Source: agent-docs/debuffs.md §Exposure;
    ///         devs/docs/architecture/12-combat-mechanics-architecture.md §4.2 (exposure takes the strongest).
    pub fn max_of(&self, mod_type: ModType, cfg: &CalcConfig, names: &[ModName]) -> f64 {
        names
            .iter()
            .filter_map(|name| self.mods.get(name))
            .flat_map(|mods| mods.iter())
            .filter(|modifier| modifier.mod_type == mod_type && modifier.matches(cfg))
            .filter_map(|modifier| modifier.effective_number(cfg))
            .fold(0.0_f64, f64::max)
    }

    /// Matches [`sum`](Self::sum)'s semantics: emits an entry per modifier
    /// after GlobalLimit accounting (`clamped_from` carries the value before
    /// clipping); `Σ value == sum()` always holds.
    pub fn contributions<'a>(
        &self,
        mod_type: ModType,
        ctx: impl Into<EvalContext<'a>>,
        names: &[ModName],
    ) -> Vec<ModContribution> {
        let ctx = ctx.into();
        let mut out = Vec::new();
        let mut limits: GlobalLimits = None;
        for name in names {
            for modifier in self.mods.get(name).into_iter().flatten() {
                if modifier.mod_type != mod_type || !modifier.matches(ctx.cfg) {
                    continue;
                }
                let Some(raw) = modifier.effective_number_ref(&ctx) else {
                    continue;
                };
                let (value, clamped_from) = apply_global_limits(modifier, raw, &mut limits);
                out.push(ModContribution {
                    name: modifier.name.clone(),
                    mod_type: modifier.mod_type,
                    value,
                    origin: modifier.origin.clone(),
                    raw_text: modifier.source.clone(),
                    clamped_from,
                });
            }
        }
        out
    }

    pub fn sum_traced<'a>(
        &self,
        mod_type: ModType,
        ctx: impl Into<EvalContext<'a>>,
        names: &[ModName],
        trace: &mut TraceGraph,
        label: impl Into<String>,
    ) -> TracedValue {
        let contributions = self.contributions(mod_type, ctx.into(), names);
        let value = contributions
            .iter()
            .map(|contribution| contribution.value)
            .sum();
        let query_node = trace.add_node(label, value, TraceOperation::QuerySum);

        for contribution in contributions {
            let source = contribution
                .origin
                .as_ref()
                .map(|origin| origin.source_id.clone())
                .unwrap_or_else(|| {
                    SourceId::new(
                        SourceKind::Derived,
                        format!(
                            "{}.{}",
                            contribution.name,
                            contribution.mod_type.as_trace_label()
                        ),
                    )
                });
            let label = contribution.raw_text.clone().unwrap_or_else(|| {
                format!(
                    "{} {} {}",
                    contribution.name,
                    contribution.mod_type.as_trace_label(),
                    contribution.value
                )
            });
            // The source node carries the value before clipping; a
            // contribution clipped by GlobalLimit enters the graph through a
            // Clamp node (so the cap is explicitly visible in the
            // attribution graph, with the clamp value being what actually
            // counts toward aggregation).
            let raw_value = contribution.clamped_from.unwrap_or(contribution.value);
            let input_node = trace.add_source_node(label, raw_value, source);
            let feed_node = if contribution.clamped_from.is_some() {
                let clamp_node = trace.add_node(
                    format!("{} globalLimit", contribution.name),
                    contribution.value,
                    TraceOperation::Clamp,
                );
                trace.add_edge(input_node, clamp_node);
                clamp_node
            } else {
                input_node
            };
            trace.add_edge(feed_node, query_node);
        }

        TracedValue {
            value,
            node_id: query_node,
        }
    }

    /// The parameter is upgraded to `impl Into<EvalContext>` (same as [`sum`](Self::sum)).
    pub fn more<'a>(&self, ctx: impl Into<EvalContext<'a>>, names: &[ModName]) -> f64 {
        self.more_rounded(ctx.into(), names, |_| true)
    }

    /// PoB2's `MoreInternal` semantics (`ModDB.lua:156-190`, the bucketed
    /// variant): **per modName**, first multiplies together all the MORE
    /// mods under that name to get `modResult`, rounds it to precision, then
    /// multiplies across modNames. Rounding per-name avoids floating-point
    /// drift in the last digit across multiple more multiplier buckets.
    /// `extra` applies additional filtering (e.g. by slot).
    ///
    /// Rounding branches (a line-by-line port of vendor `ModDB.lua:175-186`):
    /// - default (no precision exception hit): `result *= round(modResult, 2)`
    ///   — unchanged from before the exception branch existed ([`round_more`]
    ///   stays as-is, a migration invariant);
    /// - hits a MORE exception in [`HighPrecisionRules`]
    ///   (`SupportManaMultiplier` / `ReservationMultiplier` → 4):
    ///   `result = floor(result × modResult × 10^p) / 10^p` (floor applies to
    ///   the **cumulative product**).
    /// - a vendor quirk kept verbatim: `modPrecision` **persists across names
    ///   for the entire query** (once set, it never resets, only ever
    ///   increases via max, :175-180) — ordinary names after an exception
    ///   name, and even names with no bucket (`modResult = 1`), also take
    ///   the floor branch and re-clip the cumulative product.
    fn more_rounded(
        &self,
        ctx: EvalContext<'_>,
        names: &[ModName],
        extra: impl Fn(&Modifier) -> bool,
    ) -> f64 {
        let cfg = ctx.cfg;
        let mut result = 1.0;
        let mut mod_precision: Option<u32> = None;
        // GlobalLimit accounting (vendor's MoreInternal likewise passes a
        // globalLimits table, ModDB.lua:159-169 — the cap applies to the
        // percentage value, before it's folded into the multiplier bucket).
        let mut limits: GlobalLimits = None;
        for name in names {
            let mut mod_result = 1.0;
            for m in self.mods.get(name).into_iter().flatten() {
                if m.mod_type != ModType::More || !m.matches(cfg) || !extra(m) {
                    continue;
                }
                let Some(value) = m.effective_number_ref(&ctx) else {
                    continue;
                };
                let value = apply_global_limits(m, value, &mut limits).0;
                mod_result *= 1.0 + value / 100.0;
                // vendor ModDB.lua:175-180: modPrecision = max(prev, table value or prev).
                let hit = self
                    .high_precision
                    .precision_for(m.name.as_str(), ModType::More);
                mod_precision = match (mod_precision, hit) {
                    (Some(prev), hit) => Some(prev.max(hit.unwrap_or(prev))),
                    (None, hit) => hit,
                };
            }
            match mod_precision {
                Some(p) => {
                    let power = 10f64.powi(p as i32);
                    result = (result * mod_result * power).floor() / power;
                }
                None => result *= round_more(mod_result),
            }
        }
        result
    }

    /// The traced version of [`more`](Self::more): records `Π(1 + v/100)` as
    /// a single MoreProduct node, with each contributing modifier connected
    /// as its own source input node.
    pub fn more_traced<'a>(
        &self,
        ctx: impl Into<EvalContext<'a>>,
        names: &[ModName],
        trace: &mut TraceGraph,
        label: impl Into<String>,
    ) -> TracedValue {
        let ctx = ctx.into();
        let contributions = self.contributions(ModType::More, ctx, names);
        // Rounding shares the same implementation as [`more`](Self::more)
        // (including the precision-exception branch), so traced / non-traced
        // values are always equal (previously a duplicate implementation,
        // which was prone to drifting apart).
        let factor = self.more(ctx, names);
        let factor_node = trace.add_node(label, factor, TraceOperation::MoreProduct);

        for contribution in contributions {
            let source = contribution
                .origin
                .as_ref()
                .map(|origin| origin.source_id.clone())
                .unwrap_or_else(|| {
                    SourceId::new(
                        SourceKind::Derived,
                        format!(
                            "{}.{}",
                            contribution.name,
                            contribution.mod_type.as_trace_label()
                        ),
                    )
                });
            let input_label = contribution
                .raw_text
                .clone()
                .unwrap_or_else(|| format!("{} MORE {}", contribution.name, contribution.value));
            let input_node = trace.add_source_node(input_label, contribution.value, source);
            trace.add_edge(input_node, factor_node);
        }

        TracedValue {
            value: factor,
            node_id: factor_node,
        }
    }

    /// Sums only modifiers **without a slot restriction** (no
    /// [`ModTag::SlotName`]) — the global bucket for per-slot defence
    /// aggregation.
    ///
    /// Differs from [`sum`](Self::sum): [`sum`] is transparent to the slot
    /// tag (it also counts slot-restricted mods), whereas this method
    /// explicitly excludes mods with a slot tag, so they only apply through
    /// [`sum_for_slot`](Self::sum_for_slot) on a matching slot. Corresponds
    /// to the global part of PoB2's `calcLib.mod`.
    pub fn sum_global_only(&self, mod_type: ModType, cfg: &CalcConfig, names: &[ModName]) -> f64 {
        names
            .iter()
            .filter_map(|name| self.mods.get(name))
            .flat_map(|mods| mods.iter())
            .filter(|modifier| {
                modifier.mod_type == mod_type
                    && modifier.slot_name().is_none()
                    && modifier.matches(cfg)
            })
            .filter_map(|modifier| modifier.effective_number(cfg))
            .sum()
    }

    /// Sums only modifiers restricted to `slot` (matched via [`ModTag::SlotName`]) — the slot bucket for per-slot defence aggregation.
    pub fn sum_for_slot(
        &self,
        mod_type: ModType,
        cfg: &CalcConfig,
        names: &[ModName],
        slot: &str,
    ) -> f64 {
        names
            .iter()
            .filter_map(|name| self.mods.get(name))
            .flat_map(|mods| mods.iter())
            .filter(|modifier| {
                modifier.mod_type == mod_type
                    && modifier.slot_name() == Some(slot)
                    && modifier.matches(cfg)
            })
            .filter_map(|modifier| modifier.effective_number(cfg))
            .sum()
    }

    /// Multiplies only `More` modifiers **without a slot restriction** — the global more bucket for per-slot defence aggregation.
    pub fn more_global_only(&self, cfg: &CalcConfig, names: &[ModName]) -> f64 {
        self.more_rounded(EvalContext::new(cfg), names, |m| m.slot_name().is_none())
    }

    /// Multiplies only `More` modifiers restricted to `slot` — the slot more bucket for per-slot defence aggregation.
    pub fn more_for_slot(&self, cfg: &CalcConfig, names: &[ModName], slot: &str) -> f64 {
        self.more_rounded(EvalContext::new(cfg), names, |m| {
            m.slot_name() == Some(slot)
        })
    }

    /// The per-slot BASE modifiers needed for per-slot defence aggregation:
    /// returns each `(slot, value)` (`Base` modifiers with a
    /// [`ModTag::SlotName`]). Slot-less BASE values are read separately by
    /// the caller via [`sum_global_only`](Self::sum_global_only) (as the
    /// "slot-less base", which only benefits from the global multiplier bucket).
    pub fn slot_bases(&self, cfg: &CalcConfig, name: &ModName) -> Vec<(String, f64)> {
        self.mods
            .get(name)
            .into_iter()
            .flat_map(|mods| mods.iter())
            .filter(|modifier| modifier.mod_type == ModType::Base && modifier.matches(cfg))
            .filter_map(|modifier| {
                let slot = modifier.slot_name()?.to_string();
                let value = modifier.effective_number(cfg)?;
                Some((slot, value))
            })
            .collect()
    }

    pub fn flag(&self, cfg: &CalcConfig, name: ModName) -> bool {
        self.mods
            .get(&name)
            .into_iter()
            .flat_map(|mods| mods.iter())
            .any(|modifier| {
                modifier.mod_type == ModType::Flag
                    && modifier.matches(cfg)
                    && modifier.value.as_bool().unwrap_or(false)
            })
    }

    /// Returns the attributed `SourceId` of the **first** modifier that
    /// activates this flag (`None` if no origin or no match). Lets the
    /// attribution path trace a flag's behavior back to its source (e.g.
    /// which passive/gem grants `CritChanceLucky`).
    pub fn flag_origin(&self, cfg: &CalcConfig, name: ModName) -> Option<SourceId> {
        self.mods
            .get(&name)
            .into_iter()
            .flat_map(|mods| mods.iter())
            .find(|modifier| {
                modifier.mod_type == ModType::Flag
                    && modifier.matches(cfg)
                    && modifier.value.as_bool().unwrap_or(false)
            })
            .and_then(|modifier| {
                modifier
                    .origin
                    .as_ref()
                    .map(|origin| origin.source_id.clone())
            })
    }

    /// The traced version of [`flag`](Self::flag): records a QueryFlag node
    /// (value 1.0/0.0), connecting every source that activates this flag as
    /// an input.
    pub fn flag_traced(
        &self,
        cfg: &CalcConfig,
        name: ModName,
        trace: &mut TraceGraph,
        label: impl Into<String>,
    ) -> bool {
        let active = self.flag(cfg, name.clone());
        let flag_node = trace.add_node(
            label,
            if active { 1.0 } else { 0.0 },
            TraceOperation::QueryFlag,
        );
        let matching = self
            .mods
            .get(&name)
            .into_iter()
            .flat_map(|mods| mods.iter());
        for modifier in matching {
            if modifier.mod_type != ModType::Flag
                || !modifier.matches(cfg)
                || !modifier.value.as_bool().unwrap_or(false)
            {
                continue;
            }
            let source = modifier
                .origin
                .as_ref()
                .map(|origin| origin.source_id.clone())
                .unwrap_or_else(|| {
                    SourceId::new(SourceKind::Derived, format!("{}.FLAG", modifier.name))
                });
            let input_label = modifier
                .source
                .clone()
                .unwrap_or_else(|| format!("{} FLAG", modifier.name));
            let input_node = trace.add_source_node(input_label, 1.0, source);
            trace.add_edge(input_node, flag_node);
        }
        active
    }

    pub fn override_(&self, cfg: &CalcConfig, name: ModName) -> Option<f64> {
        self.mods
            .get(&name)
            .into_iter()
            .flat_map(|mods| mods.iter().rev())
            .filter(|modifier| modifier.mod_type == ModType::Override && modifier.matches(cfg))
            .filter_map(|modifier| modifier.effective_number(cfg))
            .next()
    }

    /// The equivalent primitive to PoB2's `ModStore:GetMultiplier(var, cfg)`
    /// (ModStore.lua:276-278): `Override("Multiplier:var")` takes priority,
    /// otherwise `cfg.multipliers[var] + Sum(BASE, "Multiplier:var")`.
    ///
    /// PoBR's multiplier evaluation
    /// ([`crate::Modifier::effective_number`] → `cfg.multiplier`) only reads
    /// the `cfg.multipliers` HashMap, and can't consume `Multiplier:X`-shaped
    /// BASE/Override mods inside modDB. After injecting all sources, the
    /// orchestration layer calls this method for each needed `var` and
    /// writes the result back via `cfg.with_multiplier(var, _)`, so per-X
    /// scaling modifiers can reference these modDB multipliers (the
    /// contract: `Multiplier:X` is injected into cfg by the orchestration
    /// layer through this primitive).
    ///
    /// Note: the parent chain and PerStat's `tag.base` offset aren't covered
    /// yet; this covers PoB2's main-path baseline of Override /
    /// multipliers / Sum(BASE).
    pub fn get_multiplier(&self, var: &str, cfg: &CalcConfig) -> f64 {
        let name = ModName::from(format!("Multiplier:{var}"));
        if let Some(overridden) = self.override_(cfg, name.clone()) {
            return overridden;
        }
        cfg.multiplier(var) + self.sum(ModType::Base, cfg, &[name])
    }

    /// The traced version of [`override_`](Self::override_): records a
    /// QueryOverride node (the effective value or 0), connecting the winning
    /// override modifier as the sole input (later writes override earlier
    /// ones).
    pub fn override_traced(
        &self,
        cfg: &CalcConfig,
        name: ModName,
        trace: &mut TraceGraph,
        label: impl Into<String>,
    ) -> (Option<f64>, TraceNodeId) {
        let value = self.override_(cfg, name.clone());
        let override_node =
            trace.add_node(label, value.unwrap_or(0.0), TraceOperation::QueryOverride);
        if let Some(winning) = self
            .mods
            .get(&name)
            .into_iter()
            .flat_map(|mods| mods.iter().rev())
            .find(|modifier| modifier.mod_type == ModType::Override && modifier.matches(cfg))
        {
            let source = winning
                .origin
                .as_ref()
                .map(|origin| origin.source_id.clone())
                .unwrap_or_else(|| {
                    SourceId::new(SourceKind::Derived, format!("{}.OVERRIDE", winning.name))
                });
            let input_label = winning
                .source
                .clone()
                .unwrap_or_else(|| format!("{} OVERRIDE {:?}", winning.name, value));
            let input_node = trace.add_source_node(input_label, value.unwrap_or(0.0), source);
            trace.add_edge(input_node, override_node);
        }
        (value, override_node)
    }

    /// Iterates over every modifier in the db (across all name buckets).
    ///
    /// **The order guarantee is bucket-local only**: modifiers under the same
    /// [`ModName`] appear consecutively in insertion order; **the order
    /// across buckets is not guaranteed** — `mods` is a `HashMap`, whose
    /// `RandomState` is reseeded every process, so the relative order across
    /// names can differ from run to run.
    ///
    /// When a caller collects across multiple names, the result may only
    /// feed **order-independent** consumers (`max` / summation / `any` /
    /// collecting into a `HashSet`); when a stable order is needed, filter by
    /// a single name instead (equivalent to touching only one bucket), or use
    /// [`Self::filtered`]. Counter-example: writing cross-bucket results back
    /// into another ModDb in this order would make `override_` (last write
    /// wins) and [`Self::list`]'s output drift across process runs.
    pub fn iter_mods(&self) -> impl Iterator<Item = &Modifier> {
        self.mods.values().flat_map(|mods| mods.iter())
    }

    pub fn list(&self, cfg: &CalcConfig, name: ModName) -> Vec<String> {
        self.mods
            .get(&name)
            .into_iter()
            .flat_map(|mods| mods.iter())
            .filter(|modifier| modifier.mod_type == ModType::List && modifier.matches(cfg))
            .filter_map(|modifier| match &modifier.value {
                ModValue::Text(value) => Some(value.clone()),
                ModValue::Number(_) | ModValue::Bool(_) | ModValue::NestedMods(_) => None,
            })
            .collect()
    }

    /// Passes through nested modifiers from the List channel: collects the
    /// [`ModValue::NestedMods`] payloads of List-type modifiers under `name`
    /// that pass `matches(cfg)` (cloned and expanded in bucket insertion
    /// order).
    ///
    /// Lets the orchestration layer pull out and forward nested modifiers
    /// like `EnemyModifier`, where "the outer mod lands on the player db but
    /// the inner mods forward to a target db" (env_finalize's
    /// `forward_enemy_modifiers`); this method only passes them through
    /// without evaluating — the inner mods' `matches`/`effective_number` are
    /// settled by the target db's aggregation under its own context.
    pub fn list_nested(&self, cfg: &CalcConfig, name: ModName) -> Vec<Modifier> {
        self.mods
            .get(&name)
            .into_iter()
            .flat_map(|mods| mods.iter())
            .filter(|modifier| modifier.mod_type == ModType::List && modifier.matches(cfg))
            .filter_map(|modifier| modifier.value.as_nested_mods())
            .flat_map(|nested| nested.iter().cloned())
            .collect()
    }
}

#[derive(Debug, Clone, Default)]
pub struct ModList {
    db: ModDb,
    parent: Option<Box<ModList>>,
}

impl ModList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_parent(parent: ModList) -> Self {
        Self {
            db: ModDb::new(),
            parent: Some(Box::new(parent)),
        }
    }

    pub fn add_mod(&mut self, modifier: Modifier) {
        self.db.add_mod(modifier);
    }

    pub fn sum(&self, mod_type: ModType, cfg: &CalcConfig, names: &[ModName]) -> f64 {
        let local = self.db.sum(mod_type, cfg, names);
        let parent = self
            .parent
            .as_ref()
            .map_or(0.0, |parent| parent.sum(mod_type, cfg, names));
        local + parent
    }
}
