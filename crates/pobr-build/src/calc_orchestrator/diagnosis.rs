//! What a build said that its result does not reflect.
//!
//! A calculation can lose an input at four places, and each used to be silent
//! in its own way: a line no rule parses, a gem stat the stat map cannot turn
//! into a modifier, a modifier that is stored under a name nothing reads, and
//! a modifier that scales by a variable, or waits for a condition, that
//! nothing sets. [`diagnose`] runs the ordinary calculation and reports all
//! four, so a wrong number starts from a list of what was dropped instead of
//! from a comparison with another tool.
//!
//! The list is a set of leads, not of defects: a modifier can be unread
//! because this skill has no use for it (spell damage on an attack). What a
//! lead cannot be is invisible.

use pobr_core::calc::CalculationSession;
use pobr_core::{Modifier, ReadSnapshot, ReadVariables};

use super::sources::ReceivedLine;
use pobr_data::catalog::value_expr::EffectTarget;
use pobr_data::modifier::ModType;
use pobr_gamedata::ruleset::ConfigCatalog;

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

/// A condition stored player modifiers wait for that the calculation never
/// set, so they never applied.
#[derive(Debug, Clone, PartialEq)]
pub struct UnsetCondition {
    /// The condition (one of the enemy's is `Enemy<X>`).
    pub condition: String,
    /// The config option that sets it, when one does: the build leaves it
    /// off. `None`: nothing in the engine or the config sets it.
    pub config_option: Option<String>,
    /// A stored `Condition:<condition>` flag says it holds, and nothing
    /// copies that flag into the calculation's conditions.
    pub flagged: bool,
    /// The stored player modifiers that apply only while it holds.
    pub modifiers: Vec<ModifierRef>,
}

/// A completed calculation and its dropped inputs. See the module docs.
pub struct Diagnosis {
    pub session: CalculationSession,
    /// The effect that was calculated and its level; `None` when the build
    /// has no skill that resolves. A wrong skill or level drops no input, so
    /// nothing below would show it.
    pub main_skill: Option<MainSkillSummary>,
    /// Every active effect of the enabled groups and the level and quality
    /// it runs at: a gem at another level than the exporting tool's changes
    /// what its buff or aura gives without dropping an input.
    pub skills: Vec<MainSkillSummary>,
    /// Modifier lines no rule parses: items, jewels, allocated passive nodes
    /// (`Tree:<node>: …`) and config text.
    pub unparsed_lines: Vec<String>,
    /// Gem stats the stat map has an entry for and the engine rejected.
    pub unmapped_stats: Vec<UnmappedStat>,
    /// `(owner, stat)`: the gem stats with no stat-map entry at all. Most are
    /// display or animation data, so this is where to look for a missing
    /// mechanic of the skill in use, not a list of gaps.
    pub unknown_stats: Vec<(String, String)>,
    /// Modifiers stored under a name the calculation never read.
    pub unread_modifiers: Vec<ModifierRef>,
    /// The stored names the calculation did read. A name unread here and read
    /// by some other build is one this build's skill had no use for; a name
    /// no build reads has no reader.
    pub read_names: Vec<String>,
    /// `(multiplier, modifier)`: the modifier scales by, or is gated on, a
    /// multiplier nothing set, so it contributed nothing.
    pub unprovided_multipliers: Vec<(String, ModifierRef)>,
    /// The conditions stored player modifiers wait for that neither the
    /// build, its config nor any pass of the engine set, and those a config
    /// option sets that the build leaves off. Each once, those nothing sets
    /// first, by name.
    pub unset_conditions: Vec<UnsetCondition>,
    /// The queries the calculation asked of the stored player modifiers and
    /// the modifiers they counted, taken as it ended (before anything reads
    /// the session for a breakdown). [`crate::usage::line_usage`] reads it.
    pub reads: ReadSnapshot,
    /// The conditions and multipliers the calculation asked for, taken at
    /// the same moment as [`Self::reads`].
    pub variables: ReadVariables,
    /// The build's lines as the calculation received them, in order: each
    /// with the source its modifiers are stored under and what became of it
    /// (handed to the parser, not parsed, or taken and read some other way).
    pub received_lines: Vec<ReceivedLine>,
}

/// Names the orchestrator consumes from item text or build structure rather
/// than by querying the modifier database, so "never read" says nothing.
/// Each with the consumer, as a ledger of line use names it.
pub(crate) const READ_OUTSIDE_THE_DATABASE: &[(&str, &str)] = &[
    (
        "GemProperty",
        "gem level and quality bonuses (skill_env::gem_property_bonuses)",
    ),
    (
        "ExtraSkill",
        "item-granted skills become socket groups (granted_skills)",
    ),
    (
        "GrantedPassive",
        "anointed notables become allocated nodes (append_granted_passives)",
    ),
    ("JewelData", "radius, conquest and socket data (jewel_tree)"),
    (
        "MinionModifier",
        "copied to a minion when one is spawned (extract_minion_modifier_entries)",
    ),
    (
        "SocketedIdolsUseBondedModifiers",
        "puts the Bonded lines of an item's Idols in effect (item::bonded)",
    ),
    (
        "TotemsSummoned",
        "the configured number of summoned totems (prepare::stage_build_cfg)",
    ),
];

/// What consumes `name` outside the modifier database, when something does.
pub(crate) fn read_outside_the_database(name: &str) -> Option<&'static str> {
    READ_OUTSIDE_THE_DATABASE
        .iter()
        .find(|(outside, _)| *outside == name)
        .map(|(_, consumer)| *consumer)
}

/// Conditions the orchestration sets from what a build has before the
/// calculation. One it set for this build is in the calculation's condition
/// map; one listed here it would have set for another build (another weapon,
/// a spell instead of an attack), so its absence is not a lead.
const SET_BY_THE_ORCHESTRATION: &[&str] = &[
    // `conditions::weapon_type_conditions` and the shield in the off hand.
    "UsingQuarterstaff",
    "UsingStaff",
    "UsingMace",
    "UsingCrossbow",
    "UsingBow",
    "UsingSpear",
    "UsingDagger",
    "UsingOneHandedMelee",
    "UsingTwoHandedMelee",
    "DualWielding",
    "UsingShield",
    // `conditions::combat_conditions`: what using the main skill means.
    "HitRecently",
    "HitSpellRecently",
    "AttackedRecently",
    "CastSpellRecently",
    "UsedMovementSkillRecently",
    "UsedMinionSkillRecently",
    "UsedVaalSkillRecently",
    "Channelling",
    "HaveTotem",
    "SummonedTotemRecently",
    // `prepare::stage_build_cfg`: a boss-tier enemy, a Companion skill, a
    // Normal body armour.
    "Unique",
    "RareOrUnique",
    "CompanionInPresence",
    "NormalBodyArmourEquipped",
    // `inject::inject_support_gem_counts`: the colour most supports have.
    "MostNumerousRedSocketedSupports",
    "MostNumerousGreenSocketedSupports",
    "MostNumerousBlueSocketedSupports",
    // `inject::inject_enemy`: the config input of the same name.
    "DisableEHPGainOnBlock",
];

/// The config option that sets `condition`, if one does: its own
/// `condition<condition>` input first, else an option that implies it or
/// whose effect writes `Condition:<X>`. Written on the enemy, that is
/// `Enemy<X>` in the calculation's conditions (and `<X>` as well when the
/// effect has no tags or flags).
fn config_option_setting(catalog: &ConfigCatalog, condition: &str) -> Option<String> {
    let own = format!("condition{condition}");
    if catalog.by_var.contains_key(&own) {
        return Some(own);
    }
    catalog
        .options
        .iter()
        .find(|option| {
            option
                .imply_conditions
                .iter()
                .any(|implied| implied == condition)
                || option
                    .effects
                    .iter()
                    .chain(option.option_effects.values().flatten())
                    .any(|effect| {
                        effect
                            .name
                            .strip_prefix("Condition:")
                            .is_some_and(|written| match effect.target {
                                EffectTarget::Enemy => {
                                    condition.strip_prefix("Enemy") == Some(written)
                                        || (effect.tags.is_empty()
                                            && effect.flags.is_empty()
                                            && written == condition)
                                }
                                EffectTarget::Player | EffectTarget::Minion => written == condition,
                            })
                    })
        })
        .map(|option| option.var.clone())
}

/// [`CalculationSession::unset_conditions`] less what the orchestration would
/// have set for another build, each condition once with the modifiers that
/// wait for it and the config option that could set it.
fn group_unset_conditions(session: &CalculationSession, data: &BuildData) -> Vec<UnsetCondition> {
    let mut conditions: Vec<UnsetCondition> = Vec::new();
    for (condition, modifier) in session.unset_conditions() {
        if SET_BY_THE_ORCHESTRATION.contains(&condition) {
            continue;
        }
        // Ordered by condition, so its modifiers arrive together.
        if let Some(last) = conditions
            .last_mut()
            .filter(|last| last.condition == condition)
        {
            last.modifiers.push(ModifierRef::of(modifier));
            continue;
        }
        conditions.push(UnsetCondition {
            condition: condition.to_string(),
            config_option: data
                .config_catalog
                .as_deref()
                .and_then(|catalog| config_option_setting(catalog, condition)),
            flagged: session.mod_db().has_name(&format!("Condition:{condition}")),
            modifiers: vec![ModifierRef::of(modifier)],
        });
    }
    // What nothing sets first. The sort is stable: by name within each part.
    conditions.sort_by_key(|entry| entry.config_option.is_some());
    conditions
}

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
    let reads = session.modifier_reads();
    let variables = session.read_variables();

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
    let mut unknown_stats: Vec<(String, String)> = context
        .compare_records
        .iter()
        .filter(|record| record.classification == "unknown")
        .map(|record| (record.label.clone(), record.stat.clone()))
        .collect();
    unknown_stats.sort();
    unknown_stats.dedup();

    // A condition or multiplier modifier is consumed through the config's
    // maps, not by a query: it is used when some stored modifier depends on
    // it or a calculation stage asks for it.
    let referenced = session.referenced_variables();
    let depended_on = |name: &str| {
        if let Some(variable) = name.strip_prefix("Condition:") {
            referenced.contains(variable) || variables.conditions.contains(variable)
        } else if let Some(variable) = name.strip_prefix("Multiplier:") {
            referenced.contains(variable) || variables.multipliers.contains(variable)
        } else {
            false
        }
    };
    let unread_modifiers = session
        .unread_modifiers()
        .into_iter()
        .filter(|modifier| read_outside_the_database(modifier.name.as_str()).is_none())
        .filter(|modifier| !depended_on(modifier.name.as_str()))
        .map(ModifierRef::of)
        .collect();
    let unprovided_multipliers = session
        .unprovided_multipliers()
        .into_iter()
        .map(|(variable, modifier)| (variable.to_string(), ModifierRef::of(modifier)))
        .collect();
    let unset_conditions = group_unset_conditions(&session, data);

    let view = super::with_extra_gem_bonuses(build, &options);
    Ok(Diagnosis {
        main_skill: super::skill::resolve::main_skill_summary(&view, data),
        skills: super::skill::resolve::skill_summaries(&view, data),
        read_names: session
            .read_modifier_names()
            .iter()
            .map(|name| name.as_str().to_string())
            .collect(),
        unparsed_lines: session.unsupported_modifier_texts().to_vec(),
        unmapped_stats,
        unknown_stats,
        unread_modifiers,
        unprovided_multipliers,
        unset_conditions,
        reads,
        variables,
        received_lines: std::mem::take(&mut context.received_lines),
        session,
    })
}
