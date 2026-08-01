//! Calculation request DTOs plus shared request handling (decode/create a
//! build, apply overrides, orchestration options, session execution).
//!
//! The DTO fields and shared functions here are reused across the
//! `calculate` / `analysis` / `encode` submodules, hence marked
//! `pub(crate)` (before the split, this all lived in one module, where
//! private fields would have sufficed).

use std::collections::BTreeMap;

use pobr_build::build::GemSkillRef;
use pobr_build::{
    Build, BuildData, CharacterIdentity, DataOrchestratorOptions, SocketGroup,
    calculate_with_data_session, decode_pob_code, parse_build, radius_jewel_from_text,
};
use pobr_core::calc::{CalculationSession, MinimalInput};
use pobr_core::item_text::parse_pob_xml_item;
use pobr_core::rules::config_interpreter::ConfigInputValue;
use pobr_data::monster::EnemyTier;
use pobr_data::passive_tree::{AttributeChoice, NodeId};
use serde::{Deserialize, Serialize};

use super::{ApiError, localize_input_text, slot_from_id};
use crate::state;

/// A degraded record for a single source item's **text** failing to parse:
/// that item is skipped and the calculation continues, with this going into
/// the response's `item_errors`; the frontend uses it to flag that slot in
/// red without aborting the whole calculation. Structural errors (an
/// unknown slot name, etc. — client bugs) still go through the hard error
/// [`ApiError::bad_request`].
#[derive(Debug, Clone, Serialize)]
pub(crate) struct SlotIssue {
    /// The equipment slot id / `Flask N`·`Charm N` slot name / `Jewel@<socket_node>`.
    pub(crate) slot: String,
    pub(crate) message: String,
}

pub(crate) fn parse_attribute_choice(s: &str) -> Result<AttributeChoice, String> {
    match s {
        "str" => Ok(AttributeChoice::Strength),
        "dex" => Ok(AttributeChoice::Dexterity),
        "int" => Ok(AttributeChoice::Intelligence),
        other => Err(format!("unknown attribute choice: {other}")),
    }
}

/// A character-identity override (the necessary surface for starting a
/// build from scratch / changing level and class after import).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CharacterOverride {
    pub(crate) level: Option<u32>,
    pub(crate) class_name: Option<String>,
    pub(crate) ascendancy_name: Option<String>,
}

/// A gem entry for a manual skill group (either active or support; the
/// first one in the group is the active skill, matching XML import
/// semantics). `gem_id` isn't sent up — it's reverse-looked-up from the
/// `gem_effects` table by `skill_id` (support classification depends on the gem id).
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GemInput {
    pub(crate) skill_id: String,
    pub(crate) level: u32,
    pub(crate) quality: u32,
}

impl Default for GemInput {
    fn default() -> Self {
        Self {
            skill_id: String::new(),
            level: 20,
            quality: 0,
        }
    }
}

/// A manual skill group (wholesale replaces the build's socket_groups).
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SocketGroupInput {
    pub(crate) slot: Option<String>,
    pub(crate) enabled: bool,
    /// The source marker for an equipment-granted skill group (passed
    /// through from decode; `None` for a manual group).
    pub(crate) source: Option<String>,
    pub(crate) gems: Vec<GemInput>,
}

impl Default for SocketGroupInput {
    fn default() -> Self {
        Self {
            slot: None,
            enabled: true,
            source: None,
            gems: Vec::new(),
        }
    }
}

/// Manual equipment (a raw PoB text block, using the same parser as the
/// import path; wholesale replaces the equipment slot).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SlotItemInput {
    pub(crate) slot: String,
    pub(crate) text: String,
}

/// A manual tree-socket jewel (wholesale replaces them; only takes effect
/// if the socket node is allocated, matching XML import gating).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct JewelInput {
    pub(crate) socket_node: u32,
    pub(crate) text: String,
}

/// A calculation request: at least one of `pob_code` and `character` must
/// be given — with a code, it's decoded as the baseline and overrides are
/// applied on top; without one, `character` starts an empty build from
/// scratch (PoB2's "new build" semantics).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CalculateBuildRequest {
    pub(crate) pob_code: String,
    /// The character-identity override (level / class / ascendancy; each field optional).
    pub(crate) character: Option<CharacterOverride>,
    /// The allocated-node-set override (interactive point allocation:
    /// wholesale replaces the build's allocated_nodes).
    pub(crate) allocated_nodes: Option<Vec<u32>>,
    /// The attribute-choice small-node override (node skill id ->
    /// `"str"|"dex"|"int"`; wholesale replaces them).
    pub(crate) attribute_choices: Option<BTreeMap<u32, String>>,
    /// The skill-group override (manual editing: wholesale replaces them;
    /// `None` = keep the code's decoded result).
    pub(crate) socket_groups: Option<Vec<SocketGroupInput>>,
    /// The equipment override (manual editing: wholesale replaces every equipment slot).
    pub(crate) items: Option<Vec<SlotItemInput>>,
    /// The active-flask/charm override (wholesale replaces
    /// `utility_slots`; slot names are `Flask 1/2`, `Charm 1..3`).
    pub(crate) flasks: Option<Vec<SlotItemInput>>,
    /// The tree-socket jewel override (including radius jewels: `in Radius
    /// also grant` lines rewrite passive mod lines via geometric expansion).
    pub(crate) jewels: Option<Vec<JewelInput>>,
    /// The main-socket-group override (0-based, used for switching the main skill on the Skills page).
    pub(crate) main_socket_group: Option<usize>,
    /// The effective-DPS basis (defaults to true, matching PoB2's main panel basis).
    pub(crate) mode_effective: Option<bool>,
    /// The enemy-tier override (`"none" | "boss" | "pinnacle" | "uber"`).
    pub(crate) enemy_tier: Option<String>,
    /// Extra global modifier text (for debugging / hypothetical analysis).
    pub(crate) extra_modifiers: Vec<String>,
    /// The `<Config>` input override (Config page toggles; bool/number/string).
    pub(crate) config_inputs: BTreeMap<String, serde_json::Value>,
    /// Notes (only written into `<Notes>` by `encode_build_json`; ignored by the calculation path).
    pub(crate) notes: Option<String>,
    /// The original build code at import time (used only by
    /// `encode_build_json`): the output is based on it, replacing only the
    /// currently active set, with every other loadout preserved as-is.
    /// Defaults to generating a single set from scratch.
    #[serde(default)]
    pub(crate) base_code: Option<String>,
}

fn parse_enemy_tier(s: &str) -> Result<EnemyTier, String> {
    match s {
        "none" => Ok(EnemyTier::None),
        "boss" => Ok(EnemyTier::Boss),
        "pinnacle" => Ok(EnemyTier::Pinnacle),
        "uber" => Ok(EnemyTier::Uber),
        other => Err(format!("unknown enemy_tier: {other}")),
    }
}

fn json_to_config_value(v: &serde_json::Value) -> Result<ConfigInputValue, String> {
    match v {
        serde_json::Value::Bool(b) => Ok(ConfigInputValue::Bool(*b)),
        serde_json::Value::Number(n) => Ok(ConfigInputValue::Number(n.as_f64().unwrap_or(0.0))),
        serde_json::Value::String(s) => Ok(ConfigInputValue::Text(s.clone())),
        other => Err(format!("unsupported config value: {other}")),
    }
}

/// A manual skill group -> [`SocketGroup`]: the active skill is the first
/// non-support gem in the group (determined by a data-table lookup, more
/// robust than XML's "the first one is active"); the gem id is
/// reverse-looked-up from the `gem_effects` table by effect id (support classification depends on it).
fn socket_group_from_input(input: &SocketGroupInput, data: &BuildData) -> SocketGroup {
    let mut group = SocketGroup {
        slot: input.slot.clone(),
        enabled: input.enabled,
        source: input.source.clone(),
        ..SocketGroup::default()
    };
    for gem in &input.gems {
        if gem.skill_id.is_empty() {
            continue;
        }
        let gem_id = data
            .gem_effects
            .get(&gem.skill_id)
            .map(|e| e.gem_id.clone());
        let is_support = gem_id
            .as_deref()
            .and_then(|id| data.is_support_gem(id))
            .unwrap_or(false);
        if group.active_skill_id.is_none() && !is_support {
            group.active_skill_id = Some(gem.skill_id.clone());
            group.active_gem_level = Some(gem.level);
            group.active_gem_quality = Some(gem.quality);
        }
        group.gem_skills.push(GemSkillRef {
            skill_id: gem.skill_id.clone(),
            gem_level: gem.level,
            quality: gem.quality,
            stat_set_index: None,
            name_spec: None,
        });
        if let Some(gem_id) = gem_id {
            group.gem_ids.push(gem_id);
        }
    }
    group
}

/// Applies the request's overrides to a decoded/newly-created build
/// (character / tree / skill groups / equipment / main socket group / config).
pub(crate) fn apply_request_overrides(
    build: &mut Build,
    req: &CalculateBuildRequest,
    data: &BuildData,
) -> Result<Vec<SlotIssue>, ApiError> {
    let mut issues = Vec::new();
    if let Some(ch) = &req.character {
        if let Some(level) = ch.level {
            build.character.level = level;
        }
        if let Some(class_name) = &ch.class_name {
            build.character.class_name = class_name.clone();
        }
        if let Some(ascendancy_name) = &ch.ascendancy_name {
            build.character.ascendancy_name = ascendancy_name.clone();
        }
    }
    if let Some(nodes) = &req.allocated_nodes {
        build.tree.allocated_nodes = nodes.iter().map(|&n| NodeId(n)).collect();
    }
    if let Some(choices) = &req.attribute_choices {
        build.tree.attribute_overrides = choices
            .iter()
            .map(|(&node, choice)| Ok((NodeId(node), parse_attribute_choice(choice)?)))
            .collect::<Result<_, String>>()
            .map_err(ApiError::bad_request)?;
    }
    if let Some(groups) = &req.socket_groups {
        build.socket_groups = groups
            .iter()
            .map(|g| socket_group_from_input(g, data))
            .collect();
    }
    if let Some(items) = &req.items {
        build.items.clear();
        for item in items {
            let slot = slot_from_id(&item.slot)
                .map_err(|e| ApiError::bad_request(e).with_slot(item.slot.as_str()))?;
            let text = localize_input_text(&item.text);
            // Degrade on text parse failure: skip this item and keep
            // calculating, recording the slot and reason into issues.
            match parse_pob_xml_item(&text) {
                Ok(parsed) => {
                    build.items.insert(slot, parsed);
                }
                Err(e) => issues.push(SlotIssue {
                    slot: item.slot.clone(),
                    message: format!("{e:?}"),
                }),
            }
        }
    }
    if let Some(flasks) = &req.flasks {
        build.utility_slots.clear();
        for flask in flasks {
            // Same semantics as XML import: only active slots go into the
            // list; slot names are restricted to PoB's Flask/Charm family.
            if !(flask.slot.starts_with("Flask ") || flask.slot.starts_with("Charm ")) {
                return Err(ApiError::bad_request(format!(
                    "unknown flask/charm slot: {}",
                    flask.slot
                ))
                .with_slot(flask.slot.as_str()));
            }
            let text = localize_input_text(&flask.text);
            match parse_pob_xml_item(&text) {
                Ok(parsed) => build.utility_slots.push((flask.slot.clone(), parsed)),
                Err(e) => issues.push(SlotIssue {
                    slot: flask.slot.clone(),
                    message: format!("{e:?}"),
                }),
            }
        }
    }
    if let Some(jewels) = &req.jewels {
        // Gating: only jewels in an allocated socket are accepted (matching
        // XML import's parse_radius_jewels semantics).
        let allocated: std::collections::HashSet<u32> =
            build.tree.allocated_nodes.iter().map(|n| n.0).collect();
        let mut plain = Vec::new();
        let mut radius = Vec::new();
        for jewel in jewels {
            if !allocated.contains(&jewel.socket_node) {
                continue;
            }
            let text = localize_input_text(&jewel.text);
            let parsed = match parse_pob_xml_item(&text) {
                Ok(p) => p,
                Err(e) => {
                    issues.push(SlotIssue {
                        slot: format!("Jewel@{}", jewel.socket_node),
                        message: format!("{e:?}"),
                    });
                    continue;
                }
            };
            plain.push(parsed);
            if let Some(rj) = radius_jewel_from_text(jewel.socket_node, &text) {
                radius.push(rj);
            }
        }
        build.jewels = plain;
        build.radius_jewels = radius;
    }
    if let Some(main) = req.main_socket_group {
        // The contract is 0-based (web index semantics); Build's internal
        // representation matches PoB XML's 1-based indexing.
        build.main_socket_group = Some(main + 1);
    }
    for (key, value) in &req.config_inputs {
        build.config.raw_inputs.values.insert(
            key.clone(),
            json_to_config_value(value).map_err(ApiError::bad_request)?,
        );
    }

    // Quest rewards are wholesale rebuilt on top of the merged config
    // inputs (matching PoB2's defaultState=true semantics): the effective
    // value is the result of the XML `<Input>` capture (raw_inputs)
    // overridden by the request's config_inputs; an omitted Stat-type value
    // means it's been claimed, an explicit false means it's been given up,
    // and an Options-type value (a string) injects the selected mod text.
    // This lets switching quest rewards on the Config page after importing
    // a build still take effect (the lines injected by the XML path at
    // decode time get overridden here). global_modifier_texts only carries
    // quest lines — config_resolve's interpreter channel explicitly
    // excludes quest to avoid double-counting — so wholesale replacement is safe.
    let values = &build.config.raw_inputs.values;
    let mut quest_texts = pobr_build::default_quest_stat_reward_texts(|key| {
        values.get(key).and_then(|v| match v {
            ConfigInputValue::Bool(b) => Some(*b),
            _ => None,
        })
    });
    for (key, value) in values {
        if key.starts_with("quest")
            && let ConfigInputValue::Text(s) = value
        {
            quest_texts.extend(
                s.lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(String::from),
            );
        }
    }
    build.config.global_modifier_texts = quest_texts;
    Ok(issues)
}

pub(crate) fn orchestrator_options(
    req: &CalculateBuildRequest,
) -> Result<DataOrchestratorOptions, ApiError> {
    Ok(DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        mode_effective: req.mode_effective.unwrap_or(true),
        enemy_tier: req
            .enemy_tier
            .as_deref()
            .map(parse_enemy_tier)
            .transpose()
            .map_err(ApiError::bad_request)?
            .unwrap_or_default(),
        extra_modifier_texts: req
            .extra_modifiers
            .iter()
            .map(|line| localize_input_text(line))
            .collect(),
        ..Default::default()
    })
}

pub(crate) fn parse_build_from_request(req: &CalculateBuildRequest) -> Result<Build, ApiError> {
    if req.pob_code.trim().is_empty() {
        // Starting a build from scratch (PoB2's "new build" semantics): an
        // empty build with no equipment/skill groups, with character
        // identity coming from the character override (class is required, level defaults to 1).
        let ch = req
            .character
            .as_ref()
            .ok_or_else(|| ApiError::bad_request("either pob_code or character is required"))?;
        let class_name = ch.class_name.clone().ok_or_else(|| {
            ApiError::bad_request("character.class_name is required for a scratch build")
        })?;
        // Quest rewards aren't injected here: they're uniformly rebuilt by
        // apply_request_overrides from the merged config inputs (the XML
        // and direct-construction paths share one basis, and rewards can be changed on the Config page after import).
        return Ok(Build::new().with_character(CharacterIdentity {
            level: ch.level.unwrap_or(1),
            class_name,
            ascendancy_name: ch.ascendancy_name.clone().unwrap_or_default(),
        }));
    }
    let xml = decode_pob_code(req.pob_code.trim())
        .map_err(|e| ApiError::decode_error(format!("decode: {e}")))?;
    parse_build(&xml).map_err(|e| ApiError::decode_error(format!("parse build: {e}")))
}

pub(crate) fn run_session_for_build(
    build: &Build,
    req: &CalculateBuildRequest,
) -> Result<CalculationSession, ApiError> {
    let data = state::build_data().map_err(ApiError::not_initialized)?;
    let opts = orchestrator_options(req)?;
    Ok(calculate_with_data_session(build, &data, &opts).map_err(|e| format!("calculate: {e}"))?)
}
