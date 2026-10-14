//! What became of every line a build gave the calculation.
//!
//! [`line_usage`] is a ledger of one build's inputs, for finding the lines a
//! result does not reflect. Every line of the build's items, limbs, flasks,
//! jewels, allocated nodes and config, and every other stored player
//! modifier, is listed with what became of it. A stored modifier says whether
//! the calculation counted it (`applied`), read its name and left it out
//! (`inactive`, with the requirement it did not meet), never asked for its
//! name (`unread`), or consumes it outside the modifier database (`outside`);
//! an applied one names the breakdown stats whose lines list it. A line that
//! left no modifier says why: taken before any modifier is stored and read
//! some other way (the item's own defences, gem levels, a jewel's radius) or
//! left out of this calculation; recognised as changing nothing a calculation
//! reports; or not parsed. Beside them: the lines no rule parses with the
//! sources that hold them, the gem stats the stat map could not apply, and the
//! conditions and multipliers modifiers wait for that nothing set.
//!
//! It reads a [`Diagnosis`]: what was counted comes from the read log its
//! calculation kept ([`pobr_core::ReadSnapshot`]). A query that reads a name in
//! another context than the main skill's (one hand, an ailment, a damage
//! type) is judged by that query, so a modifier it counted is applied however
//! the main skill's context sees it; the reason given for one left out is the
//! main skill's context's. The CLI (`calculate-build --usage`) and the web
//! contract (`line_usage_json`) return it as JSON.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use pobr_core::calc::CalculationSession;
use pobr_core::{EvalContext, ModEvaluation, ModValue, Modifier, ReadSnapshot, ReadVariables};
use pobr_data::catalog::PassiveNodeDef;
use pobr_data::item::Item;
use pobr_data::modifier::{ModName, ModType};
use pobr_data::source::SourceKind;
use serde::Serialize;

use crate::breakdown::BREAKDOWN_STATS;
use crate::calc_orchestrator::{engine_ctx, read_outside_the_database};
use crate::{Build, BuildData, Diagnosis, LineFate, ModifierRef};

/// One build's input lines and what the calculation did with each. See the
/// module docs.
#[derive(Debug, Clone, Serialize)]
pub struct LineUsage {
    /// Counts of what is below.
    pub summary: UsageSummary,
    /// The effect that was calculated: the main skill whose context the
    /// reasons below are given in. `null` when no skill resolves.
    pub main_skill: Option<String>,
    /// Every line of the build the calculation received, and every other
    /// stored player modifier grouped by the line or source it came from; by
    /// source kind, source id and text.
    pub lines: Vec<UsageLine>,
    /// The lines no rule parses, in the order the calculation met them. Those
    /// of the build's items, nodes and config are also among [`Self::lines`],
    /// with their exact source; this list also has what the calculation notes
    /// as text, such as a limit of the engine.
    pub unparsed: Vec<UnparsedLine>,
    /// Gem stats the engine could not turn into modifiers: by owner, then stat.
    pub gem_stats: Vec<GemStatUsage>,
    /// Conditions modifiers wait for that the calculation never set: those
    /// nothing sets first, then those a config option the build leaves off sets.
    pub conditions: Vec<ConditionUsage>,
    /// Multipliers modifiers scale by, or are gated on, that nothing provided.
    pub multipliers: Vec<MultiplierUsage>,
    /// Per stored name the calculation read, the queries that read it
    /// ([`ReadSnapshot::sites`]): the kind of query and the names it asked
    /// for together, such as `INC Damage+FireDamage+ElementalDamage`.
    pub reads: BTreeMap<String, Vec<String>>,
}

/// Counts of a [`LineUsage`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct UsageSummary {
    /// Every line, and the lines by [`LineStatus`].
    pub lines: usize,
    pub lines_applied: usize,
    pub lines_partial: usize,
    pub lines_inactive: usize,
    pub lines_unread: usize,
    pub lines_outside: usize,
    pub lines_no_effect: usize,
    pub lines_unmodelled: usize,
    pub lines_unparsed: usize,
    pub lines_no_modifier: usize,
    /// Every stored modifier, and the modifiers by [`ModifierStatus`].
    pub modifiers: usize,
    pub modifiers_applied: usize,
    pub modifiers_inactive: usize,
    pub modifiers_unread: usize,
    pub modifiers_outside: usize,
    /// Distinct unparsed lines ([`LineUsage::unparsed`]).
    pub unparsed: usize,
    pub unmapped_gem_stats: usize,
    pub unknown_gem_stats: usize,
    pub unset_conditions: usize,
    pub unprovided_multipliers: usize,
}

/// What became of a line. With modifiers stored, from theirs: `applied` when
/// every one is applied or used outside the database, `partial` when some
/// are, `inactive` when none is and some name was read, `unread` when no name
/// was. Without: one of the others, or `inactive` with the reason it was left
/// out of this calculation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LineStatus {
    Applied,
    Partial,
    Inactive,
    Unread,
    /// Taken before any modifier was stored, and read by what `reason` names:
    /// the item's own defences or damage, gem levels, a jewel's radius.
    Outside,
    /// Recognised, and registered as changing nothing a calculation reports
    /// (such as experience gain).
    NoEffect,
    /// A rule recognises it and stores nothing, and it is not registered as
    /// changing nothing: what the line does is not modelled. Most such rules
    /// came from PoB2's list of lines it recognises.
    Unmodelled,
    /// No rule parses it whole; it is not applied in part.
    Unparsed,
    /// Parsed, yet no modifier of it was stored: what dropped it is not
    /// recorded, which this ledger should learn.
    NoModifier,
}

/// What became of a stored modifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModifierStatus {
    /// A query counted it with an effect. Or every reader of its name
    /// inspects the modifiers itself (`NAMED`) or lists them (`EVAL`), and it
    /// would count in the main skill's context.
    Applied,
    /// Its name was read and no query counted it; `reason` says what it lacks.
    Inactive,
    /// No query asked for its name.
    Unread,
    /// Consumed outside the modifier database, by what `reason` names.
    Outside,
}

/// One line and the modifiers parsed from it.
#[derive(Debug, Clone, Serialize)]
pub struct UsageLine {
    /// The attribution source category (`SourceKind`'s name: `ItemAffix`,
    /// `PassiveNode`, `SkillGem` …). `null` for modifiers that carry no
    /// source, such as a jewel's lines, parsed as text.
    pub source_kind: Option<String>,
    /// The source's stable id (`item.helmet.explicit`, `node.12345`, a gem id).
    pub source_id: Option<String>,
    /// The item slot, for an item's line.
    pub slot: Option<String>,
    /// What holds the line, by name: the passive node, the item's base type.
    pub holder: Option<String>,
    /// The line as written; `null` for modifiers not parsed from a line,
    /// such as a gem's stats.
    pub text: Option<String>,
    pub status: LineStatus,
    /// For a line that stored no modifier: what read it, why it was left
    /// out, or what became of its modifiers.
    pub reason: Option<String>,
    /// The breakdown stats any of its modifiers reaches ([`UsageModifier::used_in`]).
    pub used_in: Vec<&'static str>,
    /// By name, then type.
    pub modifiers: Vec<UsageModifier>,
}

/// One stored modifier and what became of it.
#[derive(Debug, Clone, Serialize)]
pub struct UsageModifier {
    pub name: String,
    /// `BASE` / `INC` / `MORE` / `FLAG` / `OVERRIDE` / `LIST`.
    pub mod_type: &'static str,
    /// The value as stored: a number, `true` / `false`, a text, or `[nested N]`.
    pub value: UsageValue,
    /// An applied number's value in the main skill's context, scaling tags
    /// applied (caps shared with other modifiers are not). `null` for a
    /// modifier that is not an applied number or does not match that context.
    pub applied: Option<f64>,
    pub status: ModifierStatus,
    /// `inactive`: the requirement it did not meet; `outside`: what consumes
    /// it. `null` otherwise.
    pub reason: Option<String>,
    /// The breakdown stats ([`BREAKDOWN_STATS`]) whose lines list it as
    /// applied, active and not 0.
    pub used_in: Vec<&'static str>,
}

/// A stored value.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum UsageValue {
    Number(f64),
    Bool(bool),
    Text(String),
}

/// A line no rule parses.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UnparsedLine {
    /// The line, without the `Tree:<node>:` or `Config:` the calculation
    /// records it under.
    pub text: String,
    /// How many times the calculation met it from these sources.
    pub count: usize,
    /// What holds it: the node or the config it is recorded under, or the
    /// items with a line that contains it, failing that a line of the same
    /// words with other numbers (a value rolled from a range). Empty when no
    /// item has one: a limit of the engine noted as text, or a line
    /// rewritten before it was parsed.
    pub sources: Vec<LineSource>,
}

/// Where an unparsed line came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LineSource {
    /// `PassiveNode`, `Config`, `Item` (an equipped item), `Limb` (a
    /// Transcendent limb), `Flask` (a flask or charm), `Jewel`.
    pub kind: &'static str,
    /// The node id; the equipment or limb slot id; the flask or charm slot;
    /// for a jewel its socket's kind and position among the build's jewels,
    /// or the tree socket that grants it. `null` for the config.
    pub id: Option<String>,
    /// What holds it, by name: the passive node, the item's base type.
    pub holder: Option<String>,
}

/// A gem stat the engine could not apply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GemStatUsage {
    pub stat: String,
    /// The gem or support it belongs to.
    pub owner: String,
    /// `unmapped`: the stat map has an entry the engine rejected, for
    /// `reason`. `unknown`: no entry at all; most are display or animation
    /// data, so this is where a missing mechanic of the skill in use shows,
    /// not a list of gaps.
    pub status: &'static str,
    pub reason: Option<String>,
}

/// A condition modifiers wait for that the calculation never set.
#[derive(Debug, Clone, Serialize)]
pub struct ConditionUsage {
    pub condition: String,
    /// The config option that sets it, which the build leaves off. `null`:
    /// nothing in the engine or the config sets it.
    pub config_option: Option<String>,
    /// A stored `Condition:<condition>` flag says it holds, and nothing
    /// copies that flag into the calculation's conditions.
    pub flagged: bool,
    pub modifiers: Vec<UsageRef>,
}

/// A multiplier modifiers scale by, or are gated on, that nothing provided.
#[derive(Debug, Clone, Serialize)]
pub struct MultiplierUsage {
    pub multiplier: String,
    pub modifiers: Vec<UsageRef>,
}

/// A stored modifier as a diagnostic names it ([`ModifierRef`]).
#[derive(Debug, Clone, Serialize)]
pub struct UsageRef {
    pub name: String,
    pub mod_type: &'static str,
    pub value: String,
    /// Source kind and id, and the line: `ItemAffix:item.ring1.explicit | <line>`.
    pub source: String,
}

impl From<&ModifierRef> for UsageRef {
    fn from(modifier: &ModifierRef) -> Self {
        Self {
            name: modifier.name.clone(),
            mod_type: modifier.mod_type.as_trace_label(),
            value: modifier.value.clone(),
            source: modifier.source.clone(),
        }
    }
}

/// The ledger of `diagnosis`, the calculation of `build` against `data`.
/// See the module docs.
pub fn line_usage(diagnosis: &Diagnosis, build: &Build, data: &BuildData) -> LineUsage {
    let session = &diagnosis.session;
    let nodes = data.passive_nodes_for(build.tree_version.as_deref());
    let main = EvalContext::new(session.config());
    let variables = Variables {
        referenced: session.referenced_variables(),
        asked: &diagnosis.variables,
    };
    let minions = MinionReads {
        created: session.minion_count(),
        read: session.minion_read_names(),
    };
    let uses = breakdown_uses(session);

    // The stored modifiers by the line they came from. Whole-db iteration
    // has no order across names; the map and the sort below give one.
    type LineKey = (Option<SourceKind>, Option<String>, Option<String>);
    let mut groups: BTreeMap<LineKey, Vec<&Modifier>> = BTreeMap::new();
    for modifier in session.mod_db().iter_mods() {
        let key = (
            modifier
                .origin
                .as_ref()
                .map(|origin| origin.source_id.kind.clone()),
            modifier
                .origin
                .as_ref()
                .map(|origin| origin.source_id.id.clone()),
            line_text(modifier),
        );
        groups.entry(key).or_default().push(modifier);
    }

    let mut lines = Vec::with_capacity(groups.len());
    for ((kind, source_id, text), mut modifiers) in groups {
        modifiers.sort_by(|a, b| {
            (a.name.as_str(), a.mod_type.as_trace_label())
                .cmp(&(b.name.as_str(), b.mod_type.as_trace_label()))
        });
        let origin = modifiers
            .iter()
            .copied()
            .find_map(|modifier| modifier.origin.as_ref());
        let modifiers: Vec<UsageModifier> = modifiers
            .into_iter()
            .map(|modifier| {
                let (status, reason) =
                    judge(modifier, &diagnosis.reads, &main, &variables, &minions);
                UsageModifier {
                    name: modifier.name.as_str().to_string(),
                    mod_type: modifier.mod_type.as_trace_label(),
                    value: stored_value(&modifier.value),
                    applied: (status == ModifierStatus::Applied
                        && matches!(modifier.value, ModValue::Number(_))
                        && modifier.matches(main.cfg))
                    .then(|| modifier.effective_number(main))
                    .flatten(),
                    status,
                    reason,
                    used_in: uses
                        .get(&std::ptr::from_ref(modifier))
                        .cloned()
                        .unwrap_or_default(),
                }
            })
            .collect();
        let slot = origin.and_then(|origin| origin.slot.clone());
        let holder = source_id
            .as_deref()
            .and_then(|id| holder_name(id, slot.as_deref(), build, nodes));
        // A config option is the user's setting, not a line of the build: a
        // condition or multiplier it sets that nothing in this build asks for
        // has no effect here rather than a missing reader.
        let config_option = kind == Some(SourceKind::Config)
            && source_id
                .as_deref()
                .is_some_and(|id| id.starts_with("config."));
        let (status, reason) = if !config_option {
            (line_status(&modifiers), None)
        } else if modifiers.iter().all(is_unasked_variable) {
            (
                LineStatus::NoEffect,
                Some("a config option nothing in this build asks for".to_string()),
            )
        } else {
            let asked = modifiers
                .iter()
                .filter(|modifier| !is_unasked_variable(modifier));
            (line_status(asked), None)
        };
        lines.push(UsageLine {
            source_kind: kind.map(|kind| format!("{kind:?}")),
            source_id,
            slot,
            holder,
            text,
            status,
            reason,
            used_in: stats_reached(&modifiers),
            modifiers,
        });
    }
    lines.extend(lines_without_modifiers(diagnosis, build, data, &lines));
    lines.sort_by(|a, b| {
        (&a.source_kind, &a.source_id, &a.text).cmp(&(&b.source_kind, &b.source_id, &b.text))
    });

    let unparsed = unparsed_lines(&diagnosis.unparsed_lines, build, nodes);
    let mut gem_stats: Vec<GemStatUsage> = diagnosis
        .unmapped_stats
        .iter()
        .map(|stat| GemStatUsage {
            stat: stat.stat.clone(),
            owner: stat.owner.clone(),
            status: "unmapped",
            reason: Some(stat.reason.clone()),
        })
        .collect();
    gem_stats.extend(
        diagnosis
            .unknown_stats
            .iter()
            .map(|(owner, stat)| GemStatUsage {
                stat: stat.clone(),
                owner: owner.clone(),
                status: "unknown",
                reason: None,
            }),
    );
    gem_stats.sort_by(|a, b| (&a.owner, &a.stat).cmp(&(&b.owner, &b.stat)));
    let conditions: Vec<ConditionUsage> = diagnosis
        .unset_conditions
        .iter()
        .map(|unset| ConditionUsage {
            condition: unset.condition.clone(),
            config_option: unset.config_option.clone(),
            flagged: unset.flagged,
            modifiers: unset.modifiers.iter().map(UsageRef::from).collect(),
        })
        .collect();
    // Ordered by variable, so a variable's modifiers arrive together.
    let mut multipliers: Vec<MultiplierUsage> = Vec::new();
    for (variable, modifier) in &diagnosis.unprovided_multipliers {
        match multipliers.last_mut() {
            Some(last) if last.multiplier == *variable => last.modifiers.push(modifier.into()),
            _ => multipliers.push(MultiplierUsage {
                multiplier: variable.clone(),
                modifiers: vec![modifier.into()],
            }),
        }
    }

    let count_lines =
        |status: LineStatus| lines.iter().filter(|line| line.status == status).count();
    let all_modifiers = || lines.iter().flat_map(|line| &line.modifiers);
    let count_modifiers = |status: ModifierStatus| {
        all_modifiers()
            .filter(|modifier| modifier.status == status)
            .count()
    };
    let summary = UsageSummary {
        lines: lines.len(),
        lines_applied: count_lines(LineStatus::Applied),
        lines_partial: count_lines(LineStatus::Partial),
        lines_inactive: count_lines(LineStatus::Inactive),
        lines_unread: count_lines(LineStatus::Unread),
        lines_outside: count_lines(LineStatus::Outside),
        lines_no_effect: count_lines(LineStatus::NoEffect),
        lines_unmodelled: count_lines(LineStatus::Unmodelled),
        lines_unparsed: count_lines(LineStatus::Unparsed),
        lines_no_modifier: count_lines(LineStatus::NoModifier),
        modifiers: all_modifiers().count(),
        modifiers_applied: count_modifiers(ModifierStatus::Applied),
        modifiers_inactive: count_modifiers(ModifierStatus::Inactive),
        modifiers_unread: count_modifiers(ModifierStatus::Unread),
        modifiers_outside: count_modifiers(ModifierStatus::Outside),
        unparsed: unparsed.len(),
        unmapped_gem_stats: diagnosis.unmapped_stats.len(),
        unknown_gem_stats: diagnosis.unknown_stats.len(),
        unset_conditions: conditions.len(),
        unprovided_multipliers: multipliers.len(),
    };

    LineUsage {
        summary,
        main_skill: diagnosis
            .main_skill
            .as_ref()
            .map(|skill| skill.skill_id.clone()),
        lines,
        unparsed,
        gem_stats,
        conditions,
        multipliers,
        reads: diagnosis
            .reads
            .sites
            .iter()
            .map(|(name, sites)| (name.as_str().to_string(), sites.clone()))
            .collect(),
    }
}

/// The line `modifier` was parsed from, as written.
fn line_text(modifier: &Modifier) -> Option<String> {
    modifier
        .origin
        .as_ref()
        .and_then(|origin| origin.raw_text.clone())
        .or_else(|| modifier.source.clone())
}

fn stored_value(value: &ModValue) -> UsageValue {
    match value {
        ModValue::Number(number) => UsageValue::Number(*number),
        ModValue::Bool(flag) => UsageValue::Bool(*flag),
        ModValue::Text(text) => UsageValue::Text(text.clone()),
        ModValue::NestedMods(nested) => UsageValue::Text(format!("[nested {}]", nested.len())),
    }
}

/// The condition and multiplier variables of a calculation. A
/// `Condition:<X>` or `Multiplier:<X>` modifier is consumed through the
/// config's own maps, not by a query of the database: it is used when a
/// stored modifier waits for or scales by `<X>`, or a calculation stage asks
/// for `<X>` by name.
struct Variables<'a> {
    /// What the tags of stored modifiers refer to.
    referenced: BTreeSet<&'a str>,
    /// What the calculation asked of the config.
    asked: &'a ReadVariables,
}

impl Variables<'_> {
    /// What became of a `Condition:` or `Multiplier:` modifier named `name`;
    /// `None` for any other name.
    fn judge(&self, name: &str) -> Option<(ModifierStatus, Option<String>)> {
        let (variable, asked, maps, waits) = match name.strip_prefix("Condition:") {
            Some(variable) => (variable, &self.asked.conditions, "conditions", "wait for"),
            None => (
                name.strip_prefix("Multiplier:")?,
                &self.asked.multipliers,
                "multipliers",
                "scale by",
            ),
        };
        Some(if self.referenced.contains(variable) {
            (
                ModifierStatus::Outside,
                Some(format!(
                    "the calculation's {maps}, for the modifiers that {waits} {variable}"
                )),
            )
        } else if asked.contains(variable) {
            (
                ModifierStatus::Outside,
                Some(format!(
                    "the calculation's {maps}: a calculation stage asks for {variable}"
                )),
            )
        } else {
            (
                ModifierStatus::Unread,
                Some(format!(
                    "no modifier {waits} {variable} and no calculation stage asks for it"
                )),
            )
        })
    }
}

/// The minions a calculation created and the modifier names their own
/// calculations read. A `MinionModifier` line is given to the minions: it is
/// used when their calculations read what it gives them.
struct MinionReads {
    created: usize,
    read: BTreeSet<ModName>,
}

impl MinionReads {
    fn judge(&self, modifier: &Modifier) -> (ModifierStatus, Option<String>) {
        if self.created == 0 {
            return (
                ModifierStatus::Unread,
                Some("the build creates no minion to give it to".to_string()),
            );
        }
        let given: Vec<&str> = modifier
            .value
            .as_nested_mods()
            .into_iter()
            .flatten()
            .map(|inner| inner.name.as_str())
            .collect();
        let unread: Vec<&str> = given
            .iter()
            .copied()
            .filter(|name| !self.read.contains(&ModName::from(*name)))
            .collect();
        if unread.is_empty() {
            return (
                ModifierStatus::Applied,
                Some("given to the minions, whose calculation reads it".to_string()),
            );
        }
        let reason = format!(
            "given to the minions; no minion's calculation reads {}",
            unread.join(", ")
        );
        if unread.len() < given.len() {
            (ModifierStatus::Applied, Some(reason))
        } else {
            (ModifierStatus::Unread, Some(reason))
        }
    }
}

/// What became of `modifier`, and why when it is not applied.
fn judge(
    modifier: &Modifier,
    reads: &ReadSnapshot,
    main: &EvalContext<'_>,
    variables: &Variables<'_>,
    minions: &MinionReads,
) -> (ModifierStatus, Option<String>) {
    let name = modifier.name.as_str();
    if name == "MinionModifier" {
        return minions.judge(modifier);
    }
    if let Some(consumer) = read_outside_the_database(name) {
        return (ModifierStatus::Outside, Some(consumer.to_string()));
    }
    if reads.counted(modifier) {
        return (ModifierStatus::Applied, None);
    }
    if let Some(judged) = variables.judge(name)
        && (judged.0 == ModifierStatus::Outside || !reads.sites.contains_key(name))
    {
        return judged;
    }
    let Some(sites) = reads.sites.get(name) else {
        return (ModifierStatus::Unread, None);
    };
    if let Some(reason) = modifier.unmet_requirement(main) {
        return (ModifierStatus::Inactive, Some(reason));
    }
    // It would count in the main skill's context. A consumer that inspects
    // the modifiers itself is taken to use one that does.
    if sites.iter().all(|site| query_kind(site).is_none()) {
        return (ModifierStatus::Applied, None);
    }
    (
        ModifierStatus::Inactive,
        Some(uncounted_reason(modifier, sites)),
    )
}

/// The type a query asks for (`BASE`, `INC` …), from its label in
/// [`ReadSnapshot::sites`]; `None` for a consumer that inspects the
/// modifiers itself (`NAMED`) and for a listing (`EVAL`).
fn query_kind(site: &str) -> Option<&str> {
    let mut words = site.split_whitespace();
    match words.next()? {
        "NAMED" | "EVAL" => None,
        "MAX" => words.next(),
        kind => Some(kind),
    }
}

/// Why a modifier that would count in the main skill's context was counted
/// by none of the queries of its name (`sites`).
fn uncounted_reason(modifier: &Modifier, sites: &[String]) -> String {
    match modifier.value {
        ModValue::Number(0.0) => return "its value is 0".to_string(),
        ModValue::Bool(false) => return "it is stored as false".to_string(),
        _ => {}
    }
    let name = modifier.name.as_str();
    let kind = modifier.mod_type.as_trace_label();
    let mut kinds: Vec<&str> = sites.iter().filter_map(|site| query_kind(site)).collect();
    kinds.sort_unstable();
    kinds.dedup();
    if !kinds.contains(&kind) {
        return format!("{name} is read as {} only, not as {kind}", kinds.join(", "));
    }
    if modifier.mod_type == ModType::Override {
        return format!("another OVERRIDE of {name} wins");
    }
    if sites.iter().any(|site| site.starts_with("MAX ")) {
        return format!("only the strongest {name} counts, and another is stronger");
    }
    format!(
        "no query counted it: the {} of {name} ask in another context than the main skill's",
        if sites.len() == 1 { "query" } else { "queries" }
    )
}

/// A `Condition:` or `Multiplier:` modifier nothing asks for.
fn is_unasked_variable(modifier: &UsageModifier) -> bool {
    modifier.status == ModifierStatus::Unread
        && (modifier.name.starts_with("Condition:") || modifier.name.starts_with("Multiplier:"))
}

fn line_status<'a>(modifiers: impl IntoIterator<Item = &'a UsageModifier>) -> LineStatus {
    let (mut all, mut used, mut inactive) = (0, 0, false);
    for modifier in modifiers {
        all += 1;
        match modifier.status {
            ModifierStatus::Applied | ModifierStatus::Outside => used += 1,
            ModifierStatus::Inactive => inactive = true,
            ModifierStatus::Unread => {}
        }
    }
    if used == all {
        LineStatus::Applied
    } else if used > 0 {
        LineStatus::Partial
    } else if inactive {
        LineStatus::Inactive
    } else {
        LineStatus::Unread
    }
}

/// What holds the lines of source `id`, by name: an allocated node's name,
/// or the base type of the item in its slot (equipment, limb, flask or
/// charm, jewel). `slot` is the item slot the source names, when it does.
fn holder_name(
    id: &str,
    slot: Option<&str>,
    build: &Build,
    nodes: &HashMap<u32, PassiveNodeDef>,
) -> Option<String> {
    if let Some(node) = id
        .strip_prefix("node.")
        .and_then(|node| node.parse::<u32>().ok())
    {
        return nodes.get(&node).and_then(|def| def.name.clone());
    }
    if id.starts_with("flask.") {
        return build
            .utility_slots
            .iter()
            .find(|(name, _)| pobr_core::item::flask_charm_source(name).id == id)
            .map(|(_, item)| item.base.to_string());
    }
    if let Some(index) = id
        .strip_prefix("jewel.")
        .and_then(|index| index.parse::<usize>().ok())
    {
        return index
            .checked_sub(1)
            .and_then(|index| build.jewels.get(index))
            .map(|(_, item)| item.base.to_string());
    }
    let slot = slot.or_else(|| item_slot(id))?;
    build
        .items
        .iter()
        .find(|(equipment, _)| equipment.id() == slot)
        .map(|(_, item)| item.base.to_string())
        .or_else(|| {
            build
                .limbs
                .iter()
                .find(|(limb, _)| limb.id() == slot)
                .map(|(_, item)| item.base.to_string())
        })
}

/// The slot of an item source id, `item.<slot>.<section>`.
fn item_slot(id: &str) -> Option<&str> {
    id.strip_prefix("item.")?.split('.').next()
}

/// The breakdown stats any of `modifiers` reaches, in [`BREAKDOWN_STATS`] order.
fn stats_reached(modifiers: &[UsageModifier]) -> Vec<&'static str> {
    BREAKDOWN_STATS
        .iter()
        .copied()
        .filter(|stat| {
            modifiers
                .iter()
                .any(|modifier| modifier.used_in.contains(stat))
        })
        .collect()
}

/// What a gem level or quality line does in this build: the skills of the
/// enabled groups whose gems are of the kind it names, or `Unread` when no
/// gem is. `None` for a line that is no such bonus.
fn gem_bonus_reach(text: &str, build: &Build, data: &BuildData) -> Option<(LineStatus, String)> {
    use pobr_core::skill_env::{
        EffectLookup, GemPropertyKind, gem_property_applies, parse_gem_property_bonus,
    };
    let bonus = parse_gem_property_bonus(text)?;
    let what = match bonus.kind {
        GemPropertyKind::Level => "level",
        GemPropertyKind::Quality => "quality",
    };
    let mut raised: Vec<&str> = Vec::new();
    for gem in build
        .enabled_socket_groups()
        .flat_map(|group| &group.gem_skills)
    {
        // A support gem has neither levels nor quality of its own here.
        let Some(effect) = data
            .effect(&gem.skill_id)
            .filter(|effect| !effect.is_support)
        else {
            continue;
        };
        if !raised.contains(&gem.skill_id.as_str())
            && gem_property_applies(
                &bonus,
                data,
                &effect.skill_types,
                &gem.skill_id,
                gem.corrupted,
            )
        {
            raised.push(&gem.skill_id);
        }
    }
    Some(if raised.is_empty() {
        (
            LineStatus::Unread,
            format!("raises the {what} of no gem in this build"),
        )
    } else {
        (
            LineStatus::Outside,
            format!("raises the {what} of {}", raised.join(", ")),
        )
    })
}

/// The lines the calculation received ([`Diagnosis::received_lines`]) that
/// are not among `stored`, the lines with modifiers stored, each once: taken
/// and read some other way, left out, not parsed, or parsed and leaving no
/// modifier, which the parser tells apart.
fn lines_without_modifiers(
    diagnosis: &Diagnosis,
    build: &Build,
    data: &BuildData,
    stored: &[UsageLine],
) -> Vec<UsageLine> {
    let nodes = data.passive_nodes_for(build.tree_version.as_deref());
    let with_modifiers: BTreeSet<(String, &str, &str)> = stored
        .iter()
        .filter_map(|line| {
            Some((
                line.source_kind.clone()?,
                line.source_id.as_deref()?,
                line.text.as_deref()?,
            ))
        })
        .collect();
    let parse = engine_ctx(data);
    let mut seen = BTreeSet::new();
    let mut lines = Vec::new();
    for received in &diagnosis.received_lines {
        let kind = format!("{:?}", received.source.kind);
        let id = received.source.id.as_str();
        let text = received.text.as_str();
        if with_modifiers.contains(&(kind.clone(), id, text))
            || !seen.insert((kind.clone(), id, text))
        {
            continue;
        }
        let (status, reason) = match received.fate {
            LineFate::ReadBy(reader) => match gem_bonus_reach(text, build, data) {
                Some((status, reach)) => (status, Some(format!("{reader}: {reach}"))),
                None => (LineStatus::Outside, Some(reader.to_string())),
            },
            LineFate::LeftOut(reason) => (LineStatus::Inactive, Some(reason.to_string())),
            LineFate::Unparsed => (LineStatus::Unparsed, None),
            LineFate::Parsed => match parse.parse(text) {
                Ok(outcome)
                    if outcome.status == pobr_core::mod_parser::ParseStatus::Parsed
                        && outcome.unparsed.is_none() =>
                {
                    if outcome.mods.is_empty() {
                        match outcome.special_meta {
                            Some(meta) if meta.no_effect => (
                                LineStatus::NoEffect,
                                Some(format!(
                                    "registered as changing nothing a calculation reports ({})",
                                    meta.entry_id
                                )),
                            ),
                            Some(meta) => (
                                LineStatus::Unmodelled,
                                Some(format!(
                                    "rule {} recognises it and stores nothing",
                                    meta.entry_id
                                )),
                            ),
                            None => (
                                LineStatus::Unmodelled,
                                Some("a rule recognises it and stores nothing".to_string()),
                            ),
                        }
                    } else {
                        (
                            LineStatus::NoModifier,
                            Some(format!(
                                "parsed into {} modifier(s), none stored under this source",
                                outcome.mods.len()
                            )),
                        )
                    }
                }
                _ => (LineStatus::Unparsed, None),
            },
        };
        let slot = item_slot(id).map(str::to_string);
        lines.push(UsageLine {
            source_kind: Some(kind),
            source_id: Some(id.to_string()),
            holder: holder_name(id, slot.as_deref(), build, nodes),
            slot,
            text: Some(text.to_string()),
            status,
            reason,
            used_in: Vec::new(),
            modifiers: Vec::new(),
        });
    }
    lines
}

/// The modifiers each breakdown stat's lines list as applied (active, and
/// not 0), by the address of the modifier in the session's database, which
/// the evaluations borrow. The lines are the ones `breakdown::stat_breakdown`
/// lists: each stat's own, and with two weapon passes each hand's for the
/// stats split by hand; the average hit's per damage type and per source
/// type; the skill's damage over time per type; a damaging ailment's, when
/// the skill applies it.
fn breakdown_uses(session: &CalculationSession) -> HashMap<*const Modifier, Vec<&'static str>> {
    let mut uses: HashMap<*const Modifier, Vec<&'static str>> = HashMap::new();
    let output = session.output();
    let passes = session.combined_hands().len().max(1);
    for &stat in BREAKDOWN_STATS {
        let ailment_dps = match stat {
            "IgniteDPS" => Some(output.ignite_dps),
            "BleedDPS" => Some(output.bleed_dps),
            "PoisonDPS" => Some(output.poison_dps),
            _ => None,
        };
        // An ailment the skill does not apply has no lines in use.
        if ailment_dps == Some(0.0) {
            continue;
        }
        let by_hand = matches!(
            stat,
            "ActionRate"
                | "HitChance"
                | "CritChance"
                | "CritMultiplier"
                | "TotalHitAvg"
                | "TotalDPS"
        );
        let stat_passes = if by_hand { passes } else { 1 };
        for pass in 0..stat_passes {
            note_uses(&mut uses, stat, &session.hand_stat_mods(stat, pass));
        }
        if ailment_dps.is_some()
            && let Some(lines) = session.ailment_lines(stat)
        {
            for rows in [&lines.chance, &lines.duration, &lines.effect, &lines.rate] {
                note_uses(&mut uses, stat, rows);
            }
        }
    }
    for pass in 0..passes {
        let Some(terms) = session.hit_damage_terms(pass) else {
            continue;
        };
        let types: Vec<_> = terms
            .parts
            .iter()
            .filter(|part| part.base_max != 0.0 || part.max != 0.0)
            .map(|part| part.damage_type)
            .collect();
        let lines = session.hit_damage_mods(pass, &types);
        note_uses(&mut uses, "TotalHitAvg", &lines.shared);
        for (_, rows) in &lines.by_type {
            note_uses(&mut uses, "TotalHitAvg", rows);
        }
        for (min_rows, max_rows) in &lines.range_more {
            note_uses(&mut uses, "TotalHitAvg", min_rows);
            note_uses(&mut uses, "TotalHitAvg", max_rows);
        }
        let source_types: Vec<_> = terms
            .sources
            .iter()
            .filter(|source| source.min != 0.0 || source.max != 0.0)
            .map(|source| source.damage_type)
            .collect();
        for lines in session.hit_source_mods(pass, &source_types) {
            for rows in [
                &lines.added_min,
                &lines.added_max,
                &lines.added_effect,
                &lines.convert,
                &lines.gain,
            ] {
                note_uses(&mut uses, "TotalHitAvg", rows);
            }
        }
    }
    if let Some(parts) = session.skill_dot_parts()
        && !parts.is_empty()
    {
        let types: Vec<_> = parts.iter().map(|part| part.damage_type).collect();
        let dot = session.skill_dot_mods(&types);
        for (_, rows) in &dot.by_type {
            note_uses(&mut uses, "TotalDotDPS", rows);
        }
    }
    uses
}

fn note_uses(
    uses: &mut HashMap<*const Modifier, Vec<&'static str>>,
    stat: &'static str,
    rows: &[ModEvaluation<'_>],
) {
    for row in rows
        .iter()
        .filter(|row| row.active && row.value != Some(0.0))
    {
        let stats = uses.entry(std::ptr::from_ref(row.modifier)).or_default();
        if !stats.contains(&stat) {
            stats.push(stat);
        }
    }
}

/// `unparsed` (as the calculation records them) each once with its sources:
/// an allocated node's line (`Tree:<node>: …`), a config line (`Config: …`),
/// or the items, limbs, flasks, charms and jewels whose lines contain it.
fn unparsed_lines(
    unparsed: &[String],
    build: &Build,
    nodes: &HashMap<u32, PassiveNodeDef>,
) -> Vec<UnparsedLine> {
    let holders = line_holders(build);
    let mut out: Vec<UnparsedLine> = Vec::new();
    for entry in unparsed {
        let (text, sources) = if let Some((node, line)) = entry
            .strip_prefix("Tree:")
            .and_then(|rest| rest.split_once(": "))
        {
            let holder = node
                .parse::<u32>()
                .ok()
                .and_then(|id| nodes.get(&id))
                .and_then(|def| def.name.clone());
            let source = LineSource {
                kind: "PassiveNode",
                id: Some(node.to_string()),
                holder,
            };
            (line.to_string(), vec![source])
        } else if let Some(line) = entry.strip_prefix("Config: ") {
            let source = LineSource {
                kind: "Config",
                id: None,
                holder: None,
            };
            (line.to_string(), vec![source])
        } else {
            let wanted = plain(entry);
            let mut sources = if wanted.is_empty() {
                Vec::new()
            } else {
                holding(&holders, |line| line.contains(&wanted))
            };
            // A line whose numbers were resolved from a range, or rewritten,
            // is found by its words.
            if sources.is_empty() && wanted.chars().any(char::is_alphabetic) {
                let wanted = shape(&wanted);
                sources = holding(&holders, |line| shape(line).contains(&wanted));
            }
            (entry.clone(), sources)
        };
        match out
            .iter()
            .position(|line| line.text == text && line.sources == sources)
        {
            Some(index) => out[index].count += 1,
            None => out.push(UnparsedLine {
                text,
                count: 1,
                sources,
            }),
        }
    }
    out
}

/// The sources among `holders` with a line that `holds`.
fn holding(holders: &[(LineSource, Vec<String>)], holds: impl Fn(&str) -> bool) -> Vec<LineSource> {
    holders
        .iter()
        .filter(|(_, lines)| lines.iter().any(|line| holds(line.as_str())))
        .map(|(source, _)| source.clone())
        .collect()
}

/// Every item of `build` that holds modifier lines, with those lines as
/// [`plain`] reads them: equipment by slot id, limbs, flasks and charms, jewels.
fn line_holders(build: &Build) -> Vec<(LineSource, Vec<String>)> {
    let holder = |kind: &'static str, id: String, item: &Item| {
        let lines: Vec<String> = item
            .implicit_texts
            .iter()
            .chain(&item.modifier_texts)
            .chain(&item.enchant_texts)
            .map(|line| plain(line))
            .collect();
        let source = LineSource {
            kind,
            id: Some(id),
            holder: Some(item.base.to_string()),
        };
        (source, lines)
    };
    let mut equipped: Vec<_> = build.items.iter().collect();
    equipped.sort_by_key(|(slot, _)| slot.id());
    let mut holders: Vec<(LineSource, Vec<String>)> = equipped
        .into_iter()
        .map(|(slot, item)| holder("Item", slot.id().to_string(), item))
        .collect();
    holders.extend(
        build
            .limbs
            .iter()
            .map(|(slot, item)| holder("Limb", slot.id().to_string(), item)),
    );
    holders.extend(
        build
            .utility_slots
            .iter()
            .map(|(slot, item)| holder("Flask", slot.clone(), item)),
    );
    holders.extend(
        build
            .jewels
            .iter()
            .enumerate()
            .map(|(index, (socket, item))| {
                holder("Jewel", format!("{socket:?} {}", index + 1), item)
            }),
    );
    holders.extend(
        build
            .granted_socket_jewels
            .iter()
            .map(|(node, item, _)| holder("Jewel", format!("socket {node}"), item)),
    );
    holders
}

/// A line as the parser compares it, for finding what holds an unparsed one:
/// a display link `[Hit|Hits]` shows its last part, a run of whitespace is
/// one space, case does not matter.
fn plain(text: &str) -> String {
    let mut shown = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '[' {
            let link: String = chars.by_ref().take_while(|&inner| inner != ']').collect();
            shown.push_str(link.rsplit('|').next().unwrap_or(&link));
        } else {
            shown.push(c);
        }
    }
    shown
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// A [`plain`] line with each number or range of numbers (`15`, `(10-20)`,
/// `1.5`) as `#`: the words of a line, which a line whose numbers were
/// resolved from a range shares with the line it came from.
fn shape(plain: &str) -> String {
    let mut shaped = String::with_capacity(plain.len());
    let mut run = String::new();
    // A trailing space ends the last run; it is removed below.
    for c in plain.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_digit() || matches!(c, '.' | '(' | ')' | '-') {
            run.push(c);
            continue;
        }
        if run.chars().any(|c| c.is_ascii_digit()) {
            shaped.push('#');
        } else {
            shaped.push_str(&run);
        }
        run.clear();
        shaped.push(c);
    }
    shaped.pop();
    shaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_reads_links_spacing_and_case_as_the_parser_does() {
        assert_eq!(
            plain("Adds 3 to 5 [Fire|Fire  Damage] to [Attack|Attacks]"),
            "adds 3 to 5 fire damage to attacks"
        );
        assert_eq!(plain("  +10 to   maximum Life "), "+10 to maximum life");
    }

    #[test]
    fn a_shape_keeps_the_words_of_a_line_and_drops_its_numbers() {
        assert_eq!(shape("+(10-20) to maximum life"), "+# to maximum life");
        assert_eq!(shape("+15 to maximum life"), "+# to maximum life");
        assert_eq!(shape("adds 1.5 to 3 damage."), "adds # to # damage.");
        assert_eq!(shape("non-critical (hits)"), "non-critical (hits)");
    }

    #[test]
    fn a_query_kind_is_the_type_it_asks_for() {
        assert_eq!(query_kind("INC Damage+FireDamage"), Some("INC"));
        assert_eq!(query_kind("MAX BASE FireExposure"), Some("BASE"));
        assert_eq!(query_kind("BASE Armour [slot helmet]"), Some("BASE"));
        assert_eq!(query_kind("NAMED Damage"), None);
        assert_eq!(query_kind("EVAL Life"), None);
    }

    #[test]
    fn a_line_is_as_used_as_its_modifiers() {
        let modifier = |status| UsageModifier {
            name: "Life".to_string(),
            mod_type: "BASE",
            value: UsageValue::Number(10.0),
            applied: None,
            status,
            reason: None,
            used_in: Vec::new(),
        };
        let status = |statuses: &[ModifierStatus]| {
            line_status(&statuses.iter().map(|s| modifier(*s)).collect::<Vec<_>>())
        };
        use ModifierStatus::*;
        assert_eq!(status(&[Applied, Outside]), LineStatus::Applied);
        assert_eq!(status(&[Applied, Inactive]), LineStatus::Partial);
        assert_eq!(status(&[Inactive, Unread]), LineStatus::Inactive);
        assert_eq!(status(&[Unread]), LineStatus::Unread);
    }

    #[test]
    fn a_modifier_read_in_no_query_of_its_type_says_so() {
        let modifier = Modifier::number("Life", ModType::Base, 10.0);
        let sites = vec!["INC Life".to_string(), "MORE Life".to_string()];
        assert_eq!(
            uncounted_reason(&modifier, &sites),
            "Life is read as INC, MORE only, not as BASE"
        );
        let zero = Modifier::number("Life", ModType::Base, 0.0);
        assert_eq!(uncounted_reason(&zero, &sites), "its value is 0");
    }
}
