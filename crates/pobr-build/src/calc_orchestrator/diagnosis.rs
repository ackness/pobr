//! What a build said that its result does not reflect.
//!
//! A calculation can lose an input at four places, and each used to be silent
//! in its own way: a line no rule parses, a gem stat the stat map cannot turn
//! into a modifier, a modifier that is stored under a name nothing reads, and
//! a modifier that scales by a variable nothing sets. [`diagnose`] runs the
//! ordinary calculation and reports all four, so a wrong number starts from a
//! list of what was dropped instead of from a comparison with another tool.
//!
//! The list is a set of leads, not of defects: a modifier can be unread
//! because this skill has no use for it (spell damage on an attack). What a
//! lead cannot be is invisible.

use pobr_core::Modifier;
use pobr_core::calc::CalculationSession;
use pobr_data::modifier::ModType;

use super::context::CalculationContext;
pub use super::skill::resolve::MainSkillSummary;
use super::{DataOrchestratorOptions, StatMapMode, calculate_with_context};
use crate::build::Build;
use crate::build_data::BuildData;
use crate::error::BuildError;

/// A stored modifier, as a diagnostic names it.
#[derive(Debug, Clone, PartialEq)]
pub struct ModifierRef {
    pub name: String,
    pub mod_type: ModType,
    /// The value as stored: a number, `true`, a text, or `[nested]`.
    pub value: String,
    /// Where it came from: source id, and the line it was parsed from.
    pub source: String,
}

impl ModifierRef {
    fn of(modifier: &Modifier) -> Self {
        let value = match &modifier.value {
            pobr_core::ModValue::Number(number) => number.to_string(),
            pobr_core::ModValue::Bool(flag) => flag.to_string(),
            pobr_core::ModValue::Text(text) => text.clone(),
            pobr_core::ModValue::NestedMods(_) => "[nested]".to_string(),
        };
        let source = match &modifier.origin {
            Some(origin) => {
                let id = format!("{:?}:{}", origin.source_id.kind, origin.source_id.id);
                match origin.raw_text.as_deref().or(modifier.source.as_deref()) {
                    Some(text) => format!("{id} | {text}"),
                    None => id,
                }
            }
            None => modifier.source.clone().unwrap_or_default(),
        };
        Self {
            name: modifier.name.as_str().to_string(),
            mod_type: modifier.mod_type,
            value,
            source,
        }
    }
}

/// A gem stat with a stat-map entry the engine could not apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnmappedStat {
    pub stat: String,
    /// The gem or support it belongs to.
    pub owner: String,
    /// Why: the stat map's rejection category.
    pub reason: String,
}

/// A completed calculation and its dropped inputs. See the module docs.
pub struct Diagnosis {
    pub session: CalculationSession,
    /// The effect that was calculated and its level; `None` when the build
    /// has no skill that resolves. A wrong skill or level drops no input, so
    /// nothing below would show it.
    pub main_skill: Option<MainSkillSummary>,
    /// Modifier lines no rule parses: items, jewels, allocated passive nodes
    /// (`Tree:<node>: …`) and config text.
    pub unparsed_lines: Vec<String>,
    /// Gem stats the stat map has an entry for and the engine rejected.
    pub unmapped_stats: Vec<UnmappedStat>,
    /// How many gem stats have no stat-map entry at all. Most are display or
    /// animation data; the count is a size, not a list of gaps.
    pub unknown_stat_count: usize,
    /// Modifiers stored under a name the calculation never read.
    pub unread_modifiers: Vec<ModifierRef>,
    /// The stored names the calculation did read. A name unread here and read
    /// by some other build is one this build's skill had no use for; a name
    /// no build reads has no reader.
    pub read_names: Vec<String>,
    /// `(multiplier, modifier)`: the modifier scales by, or is gated on, a
    /// multiplier nothing set, so it contributed nothing.
    pub unprovided_multipliers: Vec<(String, ModifierRef)>,
}

/// Names the orchestrator consumes from item text or build structure rather
/// than by querying the modifier database, so "never read" says nothing.
const READ_OUTSIDE_THE_DATABASE: &[&str] = &[
    // Gem level and quality bonuses: `skill_env::gem_property_bonuses`.
    "GemProperty",
    // Item-granted skills become socket groups: `granted_skills`.
    "ExtraSkill",
    // Anointed notables become allocated nodes: `append_granted_passives`.
    "GrantedPassive",
    // Radius, conquest and socket data: `jewel_tree`.
    "JewelData",
    // Copied to a minion when one is spawned: `extract_minion_modifier_entries`.
    "MinionModifier",
    // Puts the Bonded lines of an item's Idols in effect: `item::bonded`.
    "SocketedIdolsUseBondedModifiers",
];

/// Calculates `build` like [`calculate_with_data_session`](super::calculate_with_data_session)
/// and reports its dropped inputs. It tracks every modifier read and records
/// every stat mapping, so it costs more than a plain calculation: use it to
/// investigate a build, not on a hot path.
pub fn diagnose(
    build: &Build,
    data: &BuildData,
    options: &DataOrchestratorOptions,
) -> Result<Diagnosis, BuildError> {
    let options = DataOrchestratorOptions {
        stat_map_mode: StatMapMode::Compare,
        ..options.clone()
    };
    let mut context = CalculationContext::new(data, &options);
    context.track_reads = true;
    let session = calculate_with_context(build, data, &options, &mut context)?;

    let mut unmapped_stats: Vec<UnmappedStat> = context
        .compare_records
        .iter()
        .filter(|record| record.classification == "unsupported")
        .map(|record| UnmappedStat {
            stat: record.stat.clone(),
            owner: record.label.clone(),
            reason: record
                .detail
                .strip_prefix("unsupported:")
                .unwrap_or(&record.detail)
                .to_string(),
        })
        .collect();
    unmapped_stats.sort_by(|a, b| (&a.owner, &a.stat).cmp(&(&b.owner, &b.stat)));
    unmapped_stats.dedup();
    let unknown_stat_count = context
        .compare_records
        .iter()
        .filter(|record| record.classification == "unknown")
        .count();

    // A condition or multiplier modifier is consumed through the config's
    // maps, not by a query: it is used when some stored modifier depends on it.
    let referenced = session.referenced_variables();
    let depended_on = |name: &str| {
        name.strip_prefix("Condition:")
            .or_else(|| name.strip_prefix("Multiplier:"))
            .is_some_and(|variable| referenced.contains(variable))
    };
    let unread_modifiers = session
        .unread_modifiers()
        .into_iter()
        .filter(|modifier| !READ_OUTSIDE_THE_DATABASE.contains(&modifier.name.as_str()))
        .filter(|modifier| !depended_on(modifier.name.as_str()))
        .map(ModifierRef::of)
        .collect();
    let unprovided_multipliers = session
        .unprovided_multipliers()
        .into_iter()
        .map(|(variable, modifier)| (variable.to_string(), ModifierRef::of(modifier)))
        .collect();

    Ok(Diagnosis {
        main_skill: super::skill::resolve::main_skill_summary(
            &super::with_extra_gem_bonuses(build, &options),
            data,
        ),
        read_names: session
            .read_modifier_names()
            .iter()
            .map(|name| name.as_str().to_string())
            .collect(),
        unparsed_lines: session.unsupported_modifier_texts().to_vec(),
        unmapped_stats,
        unknown_stat_count,
        unread_modifiers,
        unprovided_multipliers,
        session,
    })
}
