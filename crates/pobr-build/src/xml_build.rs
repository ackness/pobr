//! Parses PoB Build XML into a complete [`Build`].
//!
//! [`crate::xml_serde::parse_build_header`] only reads the `<Build>` header (level /
//! class / ascendancy / view). This module builds on top of that to **reconstruct
//! calculable sources**: the passive tree's allocated nodes, equipment (by slot), and
//! skill gem groups — letting [`crate::calc_orchestrator::calculate_with_data`]
//! calculate end-to-end straight from a PoB Build Code, with no hand-written XML
//! extraction needed from the caller.
//!
//! Parsing coverage:
//! - **Character identity**: reuses [`parse_build_header`] (level / class / ascendancy / `viewMode`).
//! - **Passive tree**: the node id array from the `<Spec nodes="…">` selected by
//!   `<Tree activeSpec>` (takes the `activeSpec` 1-based index when there are multiple
//!   Specs, defaults to the first).
//! - **Equipment**: `<Item id>` text blocks parsed into [`Item`] via
//!   [`parse_pob_xml_item`], then mapped to an [`EquipmentSlot`] via the
//!   `<Slot name itemId>` entries of the `<ItemSet>` selected by `<Items activeItemSet>`
//!   (PoB slot name → enum; slots outside the enum such as Charm/Flask/Ring 3 are ignored).
//! - **Skill gem groups**: each `<Skill>` under the `<SkillSet>` selected by
//!   `<Skills activeSkillSet>` → one [`SocketGroup`] (enabled state from `Skill.enabled`,
//!   gem ids taken from the enabled `<Gem gemId>`s).
//!
//! Robustness: when a single item's text block fails to parse (a structural error),
//! **that item is skipped** rather than aborting the whole import (matching PoB's
//! error-tolerant semantics); unparseable mod lines themselves are filtered downstream
//! by `calculate_with_data`.
//!
//! Known gaps (noted, not blocking): `masteryEffects` selection, jewels embedded in a
//! JewelSocket, the second weapon set's independent Spec, exact base-type normalization
//! for `<Item>`, etc. are left for later.

use quick_xml::Reader;
use std::collections::HashMap;

use quick_xml::events::{BytesRef, BytesStart, Event};

use pobr_core::CampaignProgress;
use pobr_core::item_text::parse_pob_xml_item;
use pobr_core::rules::config_interpreter::{ConfigInputValue, RawConfigInputs};
use pobr_data::item::{EquipmentSlot, Item};
use pobr_data::monster::EnemyTier;
use pobr_data::passive_tree::{AttributeChoice, NodeId, PassiveTreeSpec};

use crate::build::{Build, CharacterIdentity, RadiusJewel, SocketGroup};
use crate::build_code::decode_pob_code;
use crate::error::{BuildError, XmlError};
use crate::loadout::{BuildSets, SetRef};
use crate::xml_serde::parse_build_header;

/// Slotted equipment + jewels (no fixed slot) + the active ItemSet's `useSecondWeaponSet` flag.
type EquippedAndJewels = (
    Vec<(EquipmentSlot, Item)>,
    Vec<Item>,
    Vec<(String, Item)>,
    bool,
);
/// Equipment slot assignments (slot → item_id) + jewel item_id list + active
/// Flask/Charm `(slot name, item_id)` list + `useSecondWeaponSet` flag.
type SlotAssignments = (
    Vec<(EquipmentSlot, u32)>,
    Vec<u32>,
    Vec<(String, u32)>,
    bool,
);

/// Parses a PoB Build Code directly into a complete [`Build`] (decode → XML → parse).
///
/// Equivalent to `parse_build(&decode_pob_code(code)?)`; the most common one-step entry point for import.
pub fn parse_build_from_code(code: &str) -> Result<Build, BuildError> {
    let xml = decode_pob_code(code.trim())?;
    Ok(parse_build(&xml)?)
}

/// Parses a PoB Build XML into a complete [`Build`] (character + passive tree + equipment + skill gem groups).
pub fn parse_build(xml: &str) -> Result<Build, XmlError> {
    let header = parse_build_header(xml)?;

    // The active ItemSet's `useSecondWeaponSet` determines which set of weapon-set-only
    // points is active (matching PoB2 CalcSetup.lua:791-792's `Condition:WeaponSet<N>`
    // flag semantics); the filtered allocated-node set then gates tree socket jewels
    // (jewel mods only enter the calculation through the modList of an allocated socket
    // node — PoB2 CalcSetup.lua:175-244 only walks `spec.allocNodes`).
    let use_second_weapon_set = parse_active_item_set(xml)?.3;
    let ParsedPassives {
        allocated: allocated_nodes,
        tree_version,
    } = parse_passive_nodes(xml, use_second_weapon_set)?;
    let allocated_set: std::collections::HashSet<u32> =
        allocated_nodes.iter().map(|n| n.0).collect();
    let (items, jewels, flask_charms, _) = parse_items_and_slots(xml, &allocated_set)?;
    let attribute_overrides = parse_attribute_overrides(xml)?;
    let radius_jewels = parse_radius_jewels(xml, &allocated_set)?;
    let socket_groups = parse_socket_groups(xml)?;
    let main_socket_group = parse_main_socket_group(xml);

    let mut build = Build::new()
        .with_character(CharacterIdentity {
            level: header.identity.level,
            class_name: header.identity.class_name,
            ascendancy_name: header.identity.ascendancy_name,
        })
        .with_view_mode(header.view_mode)
        .with_tree(PassiveTreeSpec {
            allocated_nodes,
            attribute_overrides,
            ..Default::default()
        })
        .with_tree_version(tree_version);
    if let Some(g) = main_socket_group {
        build = build.with_main_socket_group(g);
    }

    for (slot, item) in items {
        build = build.set_item(slot, item);
    }
    if !jewels.is_empty() {
        build = build.with_jewels(jewels);
    }
    if !radius_jewels.is_empty() {
        build = build.with_radius_jewels(radius_jewels);
    }
    if !flask_charms.is_empty() {
        build = build.with_utility_slots(flask_charms);
    }
    for group in socket_groups {
        build = build.add_socket_group(group);
    }

    // Combat config: the raw three-typed `<Input>` key-values are captured losslessly
    // into `raw_inputs` (the primary-path data source — the orchestrator consumes it via
    // `config_resolve::resolve_config` → `config_interpreter::interpret` once a
    // ConfigCatalog is available); the legacy parse_config output is also kept, filling
    // the existing fields, serving as (a) a fallback tolerant of a missing catalog, (b)
    // the quest text channel (not switched over until naming is unified per §3-⑤), and
    // (c) an ongoing regression reference for config_dualrun. New coverage is opened up
    // category by category; once the report is reviewed, the legacy path is removed
    // (its own commit, report §3-⑧).
    build.config.raw_inputs = parse_config_inputs(xml);
    let parsed = parse_config(xml);
    build.config.conditions.extend(parsed.conditions);
    build.config.multipliers.extend(parsed.multipliers);
    build
        .config
        .global_modifier_texts
        .extend(parsed.global_texts);
    build.config.campaign_progress = parsed.campaign_progress;
    build.config.enemy_tier = parsed.enemy_tier;

    Ok(build)
}

/// A mapping table for boolean config options that PoB2's `ConfigOptions.lua` sets
/// `defaultState = true` for: `(XML <Input name>, calc-side condition variable name)`.
///
/// PoB2 semantics: when an `<Input>` is **omitted** from a build's XML, its value takes
/// `defaultState` rather than defaulting to false uniformly. Most boolean conditions
/// default to false (matching PoBR's all-false fallback), but the entries below default
/// to **true**, so the default must be filled in at the import layer (finding 01-06).
///
/// These entries' XML `name`s don't all carry a `condition` prefix, so they can't go
/// through the generic `strip_prefix("condition")` path — each is mapped individually to
/// the `Condition:` variable name PoB2's `apply` function actually sets. The CD-bypass
/// entries (`*BypassCD`) have no `Condition:` set by PoB2 either, and PoBR's calc side
/// doesn't consume them yet, so they're stored under their original var name to preserve
/// the semantics.
///
/// Source: vendor `src/Modules/ConfigOptions.lua`'s `defaultState = true` entries
///   (targetBrandedEnemy:277, ConcPathBypassCD:309, inDemonForm:345,
///    FlickerStrikeBypassCD:387, VigilantStrikeBypassCD:700,
///    companionInPresence:1012, conditionChampionIntimidate:1403).
const DEFAULT_TRUE_CONDITIONS: &[(&str, &str)] = &[
    ("targetBrandedEnemy", "TargetingBrandedEnemy"),
    ("inDemonForm", "DemonForm"),
    ("companionInPresence", "CompanionInPresence"),
    ("conditionChampionIntimidate", "ChampionIntimidate"),
    ("ConcPathBypassCD", "ConcPathBypassCD"),
    ("FlickerStrikeBypassCD", "FlickerStrikeBypassCD"),
    ("VigilantStrikeBypassCD", "VigilantStrikeBypassCD"),
];

/// The **condition-type** `<Input>` keys that default to true when omitted from XML. The
/// direct-request path doesn't yet implement default injection for these conditions, so
/// the encode side writes `boolean="false"` for any key not explicitly set, pinning down
/// that semantics and keeping an encode→decode round trip's calculation consistent.
/// Quest Stat rewards aren't in this list — the direct-request path already implements
/// the same defaultState=true semantics via [`default_quest_stat_reward_texts`], so
/// omission is already consistent between both paths and needs no false-pinning.
pub fn default_true_condition_keys() -> impl Iterator<Item = &'static str> {
    DEFAULT_TRUE_CONDITIONS.iter().map(|(k, _)| *k)
}

/// Stat-type quest reward injection for the direct-request path (no XML), matching the
/// same PoB2 defaultState=true semantics as the XML path: `explicit(key)` gets the
/// request's explicit checkbox value for that quest key; `None` (omitted) is treated as
/// claimed, `Some(false)` as explicitly declined. Returns the mod-text lines that should
/// be injected globally. Adding a new reward in a future version only needs
/// [`DEFAULT_QUEST_STAT_REWARDS`] extended; both paths pick it up at once.
pub fn default_quest_stat_reward_texts(
    mut explicit: impl FnMut(&str) -> Option<bool>,
) -> Vec<String> {
    let mut out = Vec::new();
    for (key, stat) in DEFAULT_QUEST_STAT_REWARDS {
        if explicit(key).unwrap_or(true) {
            push_quest_lines(&mut out, stat);
        }
    }
    out
}

/// The result of parsing `<Config>`: conditions / multipliers / global mod text + top-level scalar config options.
///
/// **Temporary export during the dual-run period**: the primary path has switched to
/// `parse_config_inputs` + `config_interpreter` (via the orchestrator's
/// `config_resolve`, commit ①); the legacy path's output is kept as (a) a fallback
/// tolerant of a missing catalog, (b) the quest text channel (report §3-⑤), and (c) an
/// ongoing regression reference for `config_dualrun`. New coverage is opened up category
/// by category; once the report is reviewed, this struct is removed along with the
/// legacy path (its own commit, report §3-⑧).
#[doc(hidden)]
#[derive(Debug, Default)]
pub struct ParsedConfig {
    /// Boolean condition overrides (variable name with the `condition` prefix stripped → value).
    pub conditions: HashMap<String, bool>,
    /// Numeric multiplier overrides (variable name with the `multiplier` prefix stripped → value).
    pub multipliers: HashMap<String, f64>,
    /// Globally-injected mod text (quest rewards etc.).
    pub global_texts: Vec<String>,
    /// The campaign progress that `resistancePenalty` (a list, stored as a number in
    /// XML) maps to. `None` when the XML omits it or the value isn't in PoB2's
    /// seven-tier table (the consumer falls back to PoB2's default Endgame `-60`).
    pub campaign_progress: Option<CampaignProgress>,
    /// The enemy tier that `enemyIsBoss` (a list, stored as a string in XML) maps to.
    /// `None` when the XML omits it or the string isn't in the four-tier table (the
    /// consumer falls back to the orchestrator option's tier, which defaults to PoB2's Pinnacle).
    pub enemy_tier: Option<EnemyTier>,
}

/// Extracts every `<Config>` `<Input name bool|number|string>` into typed raw
/// key-values (the new pipeline: this function does **no semantic interpretation** at
/// all — interpretation goes uniformly through
/// `pobr_core::rules::config_interpreter::interpret` + `ConfigCatalog`).
///
/// Same scan scope as the legacy [`parse_config`]: walks every `Input` element in the
/// whole XML (PoB2 in practice only saves Input under `<Config>`). On a duplicate name,
/// the later write wins (matching the legacy path's HashMap insert semantics). The
/// three-type check order is boolean → number → string; an `<Input>` with none of these
/// payload attributes is skipped.
///
/// `<Placeholder>` elements (placeholder values PoB2's ConfigTab saves, the
/// `setInputAndPlaceholder` sibling in SkillsTab.lua) land in a separate `placeholders`
/// table — vendor only consumes it for a handful of scalars as an "Input missing →
/// Placeholder fallback" (e.g. `enemyLevel`, ConfigTab.lua:872-877); the interpreter's
/// main flow doesn't read this table.
pub fn parse_config_inputs(xml: &str) -> RawConfigInputs {
    let mut inputs = RawConfigInputs::new();
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e))
                if matches!(element_name(&e).as_str(), "Input" | "Placeholder") =>
            {
                let Some(name) = attr_value(&e, b"name") else {
                    continue;
                };
                let value = if let Some(b) = attr_value(&e, b"boolean") {
                    ConfigInputValue::Bool(b == "true")
                } else if let Some(n) =
                    attr_value(&e, b"number").and_then(|v| v.parse::<f64>().ok())
                {
                    ConfigInputValue::Number(n)
                } else if let Some(s) = attr_value(&e, b"string") {
                    ConfigInputValue::Text(s)
                } else {
                    continue;
                };
                if element_name(&e) == "Input" {
                    inputs.values.insert(name, value);
                } else {
                    inputs.placeholders.insert(name, value);
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                // Error mid-scan: stop scanning (permissive semantics that keep what's
                // already been collected), but emit a diagnostic rather than staying
                // silent — otherwise `<Input>`s further down being silently dropped
                // would cause miscalculation with no signal at all.
                eprintln!("[POBR_WARN] parse_config_inputs: XML scan halted on error: {e}");
                break;
            }
            _ => {}
        }
    }
    inputs
}

/// **Temporary export during the dual-run period**: the legacy `<Config>` parse path
/// (a reference for current production behavior).
///
/// Only for integration tests to compare against the new `parse_config_inputs` +
/// config_interpreter path (asserting "legacy ⊆ new and the intersection is
/// value-for-value equal"); no new business consumer may use it.
#[doc(hidden)]
pub fn parse_config_legacy(xml: &str) -> ParsedConfig {
    parse_config(xml)
}

/// Extracts `<Config>`'s `<Input name boolean|number|string>` entries into a
/// [`ParsedConfig`]. Names have the `condition`/`multiplier` prefix stripped to become
/// the variable name (e.g. `conditionEnemyChilled` → `EnemyChilled`), matching the
/// calc side's `ModTag::Condition`/`Multiplier` variable convention.
///
/// **Omission = default value** (PoB2's `defaultState`): an `<Input>` present in the XML
/// takes its own value; among absent boolean conditions, the entries listed in
/// [`DEFAULT_TRUE_CONDITIONS`] get `true` filled in (everything else still falls back to
/// false on the calc side).
///
/// **Temporarily kept during the dual-run period**: this function is a reference for
/// current production behavior, its logic frozen; see [`parse_config_inputs`] for the
/// new pipeline. Removed once the dual-run report is reviewed (its own commit).
fn parse_config(xml: &str) -> ParsedConfig {
    let mut parsed = ParsedConfig::default();
    // Records the `<Input name>`s that **did appear** in the XML, used to determine which defaultState entries were omitted.
    let mut seen_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if element_name(&e) == "Input" => {
                let Some(name) = attr_value(&e, b"name") else {
                    continue;
                };
                seen_names.insert(name.clone());
                if let Some(var) = name.strip_prefix("condition")
                    && let Some(b) = attr_value(&e, b"boolean")
                {
                    parsed.conditions.insert(var.to_string(), b == "true");
                } else if let Some(charge_cond) = use_charge_condition(&name)
                    && let Some(b) = attr_value(&e, b"boolean")
                {
                    // PoB2 ConfigOptions: the `useXCharges` checkbox → `Condition:UseXCharges` FLAG.
                    // Full-stack charge default (current = max) only takes effect when this condition is true (see charge_multipliers_panel_default).
                    parsed
                        .conditions
                        .insert(charge_cond.to_string(), b == "true");
                } else if let Some((_, cond_var)) =
                    DEFAULT_TRUE_CONDITIONS.iter().find(|(n, _)| *n == name)
                    && let Some(b) = attr_value(&e, b"boolean")
                {
                    // A defaultState=true entry that **appears explicitly** takes its own value (no default fill-in).
                    parsed
                        .conditions
                        .insert((*cond_var).to_string(), b == "true");
                } else if let Some(var) = name.strip_prefix("multiplier")
                    && let Some(n) = attr_value(&e, b"number").and_then(|v| v.parse::<f64>().ok())
                {
                    parsed.multipliers.insert(var.to_string(), n);
                } else if name == "enemyIsBoss" {
                    // PoB2 ConfigOptions `enemyIsBoss` (a list, stored as a string in
                    // XML): the four tiers None/Boss/Pinnacle/Uber → EnemyTier. A string
                    // outside the table stays None, and the consumer falls back to the
                    // orchestrator option's tier (PoB2 defaultIndex=3 = Pinnacle).
                    parsed.enemy_tier =
                        attr_value(&e, b"string").and_then(|v| EnemyTier::from_pob_str(&v));
                } else if name == "resistancePenalty" {
                    // PoB2 ConfigOptions `resistancePenalty` (a list, stored as a number
                    // in XML): the seven tiers 0/-10/…/-60 → the existing CampaignProgress
                    // table. Stays None when the value isn't in the tier table
                    // (theoretically PoB2 never saves such a value), and the consumer
                    // falls back to the default Endgame.
                    parsed.campaign_progress = attr_value(&e, b"number")
                        .and_then(|v| v.parse::<f64>().ok())
                        .and_then(CampaignProgress::from_resistance_penalty);
                } else if name.starts_with("quest") {
                    // PoB2 quest rewards (`questRewards`) are injected as **global**
                    // permanent modifiers:
                    // - Options type (list): `string="<selected option>"` (can be
                    //   multi-line, injected line by line);
                    // - Stat type (checkbox, defaultState=true): `boolean="true"` or the
                    //   XML **omitting** it both mean claimed (backfilled from the
                    //   default table [`DEFAULT_QUEST_STAT_REWARDS`]),
                    //   `boolean="false"` means explicitly declined.
                    if let Some(s) = attr_value(&e, b"string") {
                        push_quest_lines(&mut parsed.global_texts, &s);
                    } else if attr_bool(&e, b"boolean")
                        && let Some((_, stat)) = DEFAULT_QUEST_STAT_REWARDS
                            .iter()
                            .find(|(key, _)| *key == name)
                    {
                        push_quest_lines(&mut parsed.global_texts, stat);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                // Error mid-scan: stop scanning (permissive semantics that keep what's
                // already been collected), but emit a diagnostic rather than staying
                // silent — otherwise condition/multiplier/quest entries further down
                // being silently dropped would cause miscalculation with no signal at all.
                eprintln!("[POBR_WARN] parse_config: XML scan halted on error: {e}");
                break;
            }
            _ => {}
        }
    }

    // PoB2 `defaultState = true`: fill in true for entries the XML omits (doesn't override entries that appeared explicitly).
    for (xml_name, cond_var) in DEFAULT_TRUE_CONDITIONS {
        if !seen_names.contains(*xml_name) {
            parsed
                .conditions
                .entry((*cond_var).to_string())
                .or_insert(true);
        }
    }

    // Stat-type quest reward defaultState=true: a key the XML omits is treated as
    // claimed, so the default reward is backfilled (matching PoB2 ConfigOptions's
    // `addQuestModsRewardsConfigOptions` checkbox-default-checked semantics).
    for (key, stat) in DEFAULT_QUEST_STAT_REWARDS {
        if !seen_names.contains(*key) {
            push_quest_lines(&mut parsed.global_texts, stat);
        }
    }

    parsed
}

/// The default table for Stat-type (checkbox) quest rewards from PoB2's
/// `QuestRewards.lua`: `(XML <Input name> key, reward mod text)`. Only covers
/// `useConfig=true` single-item `Stat` rewards (Options-type rewards go through the
/// string path, default Nothing; entries like `+2 Weapon Set Passive Skill Points` have
/// useConfig=false and don't participate in the calculation).
const DEFAULT_QUEST_STAT_REWARDS: &[(&str, &str)] = &[
    ("questAct 1ClearfellBeira", "+10% to Cold Resistance"),
    ("questAct 1FreythornKing In The Mists", "+30 to Spirit"),
    ("questAct 1Ogham ManorCandlemass", "+20 to maximum Life"),
    (
        "questAct 2Spires of DesharSisters of Garukhan Shrine",
        "+10% to Lightning Resistance",
    ),
    ("questAct 3Azak BogIgnagduk", "+30 to Spirit"),
    (
        "questAct 3Jiquani's MachinariumBlackjaw",
        "+10% to Fire Resistance",
    ),
    (
        "questAct 4Eye of HinekoraSilent Hall",
        "5% increased Maximum Mana",
    ),
    (
        "questInterlude 2Khari CrossingMolten Shrine",
        "5% increased maximum Life",
    ),
    ("questInterlude 3Kriar VillageLythara", "+40 to Spirit"),
];

/// Collects quest reward text line by line (matching PoB2's `applyModsFromString`
/// splitting by line; multi-line options like Tribal Medicine chain several entries with `\n\t`).
fn push_quest_lines(out: &mut Vec<String>, text: &str) {
    for line in text.lines() {
        let line = line.trim();
        if !line.is_empty() {
            out.push(line.to_string());
        }
    }
}

/// PoB2's charge-usage checkboxes (`use{Power,Frenzy,Endurance}Charges`) → the calc-side
/// condition variable name. On a match, sets the corresponding `UseXCharges` condition
/// in build config (gates the full-stack charge default).
fn use_charge_condition(name: &str) -> Option<&'static str> {
    match name {
        "usePowerCharges" => Some("UsePowerCharges"),
        "useFrenzyCharges" => Some("UseFrenzyCharges"),
        "useEnduranceCharges" => Some("UseEnduranceCharges"),
        _ => None,
    }
}

/// Extracts `<Build mainSocketGroup="N">` (1-based main skill group index). Returns `None` when missing.
fn parse_main_socket_group(xml: &str) -> Option<usize> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if element_name(&e) == "Build" => {
                return attr_value(&e, b"mainSocketGroup").and_then(|v| v.parse::<usize>().ok());
            }
            Ok(Event::Eof) | Err(_) => return None,
            _ => {}
        }
    }
}

// Passive tree

/// The node set of a single `<Spec>`: the full `nodes` list + the two weapon-set-only
/// point lists (`<WeaponSet1 nodes>` / `<WeaponSet2 nodes>`, which PoB2's
/// PassiveSpec.lua:104-144 parses into `node.allocMode = 1|2`; nodes not listed there
/// have `allocMode = 0` and are always active).
#[derive(Default)]
struct SpecNodes {
    nodes: Vec<NodeId>,
    weapon_set: [Vec<NodeId>; 2],
    /// `<Spec treeVersion>` (e.g. `"0_5"`) — the PoB passive tree version annotation, used for gap B reconciliation.
    tree_version: Option<String>,
}

/// Extracts the allocated node ids from the `<Spec nodes>` selected by
/// `<Tree activeSpec>`, filtering out points exclusive to the **non-active weapon set**.
///
/// `activeSpec` is a 1-based index; out of range / missing falls back to the first
/// `<Spec>`. Returns empty when there's no `<Spec>`.
///
/// Weapon-set semantics (PoB2 CalcSetup.lua:209-233 / :791-792): **every** mod on a
/// weapon-set-only point (`allocMode = 1|2`) node — including its own mods and radius
/// jewel grants — gets a `Condition: WeaponSet<N>` tag appended (the node's own
/// allocMode takes priority, CalcSetup.lua:222-223; the jewel-source gating at :224-227
/// only applies to allocMode=0 nodes), and that condition flag is only true for the
/// currently active weapon set (`useSecondWeaponSet` ? 2 : 1) — the net effect is that
/// every mod on a non-active set's exclusive points **is entirely inactive**. PoBR
/// implements this equivalently at the parse layer: exclusive points of the non-active
/// set are stripped from the allocated nodes before anything else (mod collection /
/// radius jewel grant counting / per-X multipliers all follow suit automatically; see
/// `collect::radius_jewel_expansions` for oracle-verified proof).
///
/// The result of [`parse_passive_nodes`].
struct ParsedPassives {
    /// Active allocated nodes (their own mods participate in the calculation).
    allocated: Vec<NodeId>,
    tree_version: Option<String>,
}

fn parse_passive_nodes(xml: &str, use_second_weapon_set: bool) -> Result<ParsedPassives, XmlError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut active_spec: usize = 1;
    let mut specs: Vec<SpecNodes> = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = element_name(&e);
                if name == "Tree" {
                    if let Some(v) = attr_value(&e, b"activeSpec")
                        && let Ok(n) = v.parse::<usize>()
                        && n >= 1
                    {
                        active_spec = n;
                    }
                } else if name == "Spec" {
                    let nodes = attr_value(&e, b"nodes")
                        .map(|v| parse_node_csv(&v))
                        .unwrap_or_default();
                    let tree_version = attr_value(&e, b"treeVersion");
                    specs.push(SpecNodes {
                        nodes,
                        tree_version,
                        ..Default::default()
                    });
                } else if let Some(set_idx) = match name.as_str() {
                    "WeaponSet1" => Some(0),
                    "WeaponSet2" => Some(1),
                    _ => None,
                } {
                    // `<WeaponSetN>` is a child of `<Spec>`, so it belongs to the most recent Spec.
                    if let (Some(spec), Some(v)) = (specs.last_mut(), attr_value(&e, b"nodes")) {
                        spec.weapon_set[set_idx] = parse_node_csv(&v);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(XmlError::Parse(e.to_string())),
            _ => {}
        }
    }

    if specs.is_empty() {
        return Ok(ParsedPassives {
            allocated: Vec::new(),
            tree_version: None,
        });
    }
    let idx = active_spec.saturating_sub(1).min(specs.len() - 1);
    let spec = specs.swap_remove(idx);
    let tree_version = spec.tree_version;

    // Strip the non-active weapon set's exclusive points (preserving original order, deterministic).
    let inactive: std::collections::HashSet<NodeId> = spec.weapon_set
        [if use_second_weapon_set { 0 } else { 1 }]
    .iter()
    .copied()
    .collect();
    let allocated = spec
        .nodes
        .into_iter()
        .filter(|n| !inactive.contains(n))
        .collect();
    Ok(ParsedPassives {
        allocated,
        tree_version,
    })
}

/// Parses `nodes="65091,58814,…"` CSV into [`NodeId`]s, skipping non-numeric fragments.
fn parse_node_csv(value: &str) -> Vec<NodeId> {
    value
        .split(',')
        .filter_map(|s| s.trim().parse::<u32>().ok())
        .map(NodeId)
        .collect()
}

/// Parses the active Spec's `<Overrides><AttributeOverride strNodes/dexNodes/intNodes>`
/// into a mapping of attribute-choice nodes to their selected attribute (matching PoB2's
/// `PassiveSpec.lua::SwitchAttributeNode` semantics).
///
/// Selects the Spec by `<Tree activeSpec>`, consistent with [`parse_passive_nodes`];
/// returns an empty map for a build with no Overrides (every attribute-choice node
/// contributes no attribute).
fn parse_attribute_overrides(xml: &str) -> Result<HashMap<NodeId, AttributeChoice>, XmlError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut active_spec: usize = 1;
    let mut specs: Vec<HashMap<NodeId, AttributeChoice>> = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = element_name(&e);
                if name == "Tree" {
                    if let Some(v) = attr_value(&e, b"activeSpec")
                        && let Ok(n) = v.parse::<usize>()
                        && n >= 1
                    {
                        active_spec = n;
                    }
                } else if name == "Spec" {
                    specs.push(HashMap::new());
                } else if name == "AttributeOverride"
                    && let Some(current) = specs.last_mut()
                {
                    for (attr, choice) in [
                        (b"strNodes".as_slice(), AttributeChoice::Strength),
                        (b"dexNodes".as_slice(), AttributeChoice::Dexterity),
                        (b"intNodes".as_slice(), AttributeChoice::Intelligence),
                    ] {
                        if let Some(v) = attr_value(&e, attr) {
                            for node in parse_node_csv(&v) {
                                current.insert(node, choice);
                            }
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(XmlError::Parse(e.to_string())),
            _ => {}
        }
    }

    if specs.is_empty() {
        return Ok(HashMap::new());
    }
    let idx = active_spec.saturating_sub(1).min(specs.len() - 1);
    Ok(specs.swap_remove(idx))
}

// Equipment + slot mapping

/// Extracts `<Item id>` text blocks and maps them to slots via the `<ItemSet>` selected
/// by `<Items activeItemSet>`, returning a `(EquipmentSlot, Item)` list (sorted by slot
/// id, deterministic).
///
/// Tree socket jewels are gated by `allocated` (the allocated node set, after weapon-set
/// filtering): in PoB2 a jewel's mods only enter the calculation through the modList of
/// an **allocated** socket node (CalcSetup.lua:175-244 only walks `spec.allocNodes`;
/// PassiveSpec hangs a jewel's modList off its socket node) — a jewel in an unallocated
/// socket is entirely inactive. The ItemSet-side `Jewel*`/`*Socket*` slot-name path is
/// kept as-is (PoE2 build XML routes tree jewels through `<Sockets><Socket>`; ItemSet
/// only holds a `<SocketIdURL>` with no itemId, and doesn't go through this path).
fn parse_items_and_slots(
    xml: &str,
    allocated: &std::collections::HashSet<u32>,
) -> Result<EquippedAndJewels, XmlError> {
    let items = parse_item_blocks(xml)?;
    let (slot_assignments, jewel_ids, flask_charm_ids, use_second_weapon_set) =
        parse_active_item_set(xml)?;

    let mut out: Vec<(EquipmentSlot, Item)> = Vec::new();
    for (slot, item_id) in slot_assignments {
        if let Some(item) = items.get(&item_id) {
            out.push((slot, item.clone()));
        }
    }
    out.sort_by_key(|(slot, _)| slot.id());

    // Tree jewels live in `<Tree><Spec><Sockets><Socket nodeId itemId/>` (not ItemSet),
    // collected separately; only jewels in an allocated socket node are kept.
    let socket_items = parse_socket_node_items(xml)?;
    // Voices (a 0.5.4b unique): "Allocates N Sinister Jewel sockets" — when a jewel in
    // an allocated socket carries this mod, the first N sinister sockets (in vendor's
    // alias order) are treated as allocated too (vendor PassiveSpec.lua:1067-1090's
    // `voices_jewel_slot1..5` → 0_5 tree node ids, pinned from TreeData/0_5/tree.lua's
    // `sinister=true` + `aliasPassiveSocket`).
    // ponytail: node ids are pinned to the 0_5 tree (sinister sockets only exist from
    // 0.5.4+; older tree versions have no source for this mod, so zero behavior change
    // there). The parity gate will call this out when the tree version iterates again;
    // switch to reading the node id list from tree data at that point.
    const SINISTER_SOCKETS_0_5: [u32; 5] = [62152, 26178, 23960, 39087, 3367];
    let sinister_count: usize = socket_items
        .iter()
        .filter(|(node, _)| allocated.contains(node))
        .filter_map(|(_, id)| items.get(id))
        .flat_map(|it| it.implicit_texts.iter().chain(&it.modifier_texts))
        .filter_map(|t| sinister_socket_alloc_count(t))
        .sum();
    let mut sinister_allocated: std::collections::HashSet<u32> = SINISTER_SOCKETS_0_5
        .iter()
        .copied()
        .take(sinister_count)
        .collect();
    // Named jewel sockets' "Allocates <name>" grant (vendor PassiveSpec.lua:1106-1114's
    // ResolveGrantedPassiveNodes name-matching fallback): an amulet anoint like
    // `{enchant}Allocates Zarokh's Gift` allocates the socket node, so the jewel in that
    // socket enters the calculation too.
    // ponytail: the only named socket in the 0_5 tree is Zarokh's Gift (everything else
    // is called Sinister Jewel Socket and goes through the Voices counting channel
    // above); the parity gate will call this out when a future tree version adds more
    // named sockets, switch to reading the name table from tree data at that point.
    const NAMED_SOCKETS_0_5: [(&str, u32); 1] = [("zarokh's gift", 11184)];
    let equipped_texts = out.iter().flat_map(|(_, item)| {
        item.implicit_texts
            .iter()
            .chain(&item.modifier_texts)
            .chain(&item.enchant_texts)
    });
    for text in equipped_texts {
        if let Some(name) = text.trim().strip_prefix("Allocates ")
            && let Some((_, node)) = NAMED_SOCKETS_0_5
                .iter()
                .find(|(n, _)| name.trim().eq_ignore_ascii_case(n))
        {
            sinister_allocated.insert(*node);
        }
    }
    let mut all_jewel_ids = jewel_ids;
    all_jewel_ids.extend(
        socket_items
            .into_iter()
            .filter(|(node, _)| allocated.contains(node) || sinister_allocated.contains(node))
            .map(|(_, item)| item),
    );
    all_jewel_ids.sort_unstable();
    all_jewel_ids.dedup();

    let jewels: Vec<Item> = all_jewel_ids
        .iter()
        .filter_map(|id| items.get(id).cloned())
        .collect();
    let flask_charms: Vec<(String, Item)> = flask_charm_ids
        .iter()
        .filter_map(|(slot, id)| Some((slot.clone(), items.get(id).cloned()?)))
        .collect();
    Ok((out, jewels, flask_charms, use_second_weapon_set))
}

/// Parses the "Allocates N Sinister Jewel socket(s)" mod → N (matching vendor
/// ModParser.lua's `allocates (%d+) sinister jewel sockets?` →
/// GrantedPassive SinisterJewelSockets). Returns None for any other mod text.
fn sinister_socket_alloc_count(text: &str) -> Option<usize> {
    let rest = text.trim().strip_prefix("Allocates ")?;
    let (num, tail) = rest.split_once(' ')?;
    matches!(
        tail.trim().to_ascii_lowercase().as_str(),
        "sinister jewel sockets" | "sinister jewel socket"
    )
    .then(|| num.parse().ok())?
}

/// Parses tree socket `<Socket nodeId="N" itemId="M"/>` → `(socket_node, item_id)` (itemId≠0).
fn parse_socket_node_items(xml: &str) -> Result<Vec<(u32, u32)>, XmlError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut out = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if element_name(&e) == "Socket" => {
                let node = attr_value(&e, b"nodeId").and_then(|v| v.parse::<u32>().ok());
                let item = attr_value(&e, b"itemId").and_then(|v| v.parse::<u32>().ok());
                if let (Some(node), Some(item)) = (node, item)
                    && item != 0
                {
                    out.push((node, item));
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(XmlError::Parse(e.to_string())),
            _ => {}
        }
    }
    Ok(out)
}

/// Parses the **raw text** of every `<Item id="N">…</Item>` into `id -> text block`
/// (preserving line structure).
///
/// Differs from [`parse_item_blocks`]: this function doesn't parse into an [`Item`], it
/// keeps lines like `Radius:` / `... in Radius also grant ...` that item parsing
/// discards, for radius jewel geometric expansion.
fn parse_raw_item_texts(xml: &str) -> Result<std::collections::HashMap<u32, String>, XmlError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut result = std::collections::HashMap::new();
    let mut in_item = false;
    let mut current_id: u32 = 0;
    let mut current_text = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) if element_name(&e) == "Item" => {
                in_item = true;
                current_text.clear();
                current_id = attr_value(&e, b"id")
                    .and_then(|v| v.parse::<u32>().ok())
                    .unwrap_or(0);
            }
            Ok(Event::Text(t)) if in_item => match t.decode() {
                Ok(text) => current_text.push_str(&text),
                // On decode failure, degrade to a lossy decode of the raw bytes rather
                // than dropping the whole block — otherwise a single mod line would be
                // silently truncated (PoB raw item text is parsed line by line).
                Err(_) => current_text.push_str(&String::from_utf8_lossy(&t)),
            },
            Ok(Event::GeneralRef(r)) if in_item => append_general_ref(&mut current_text, &r),
            Ok(Event::End(e)) if element_name_end(&e) == "Item" && in_item => {
                in_item = false;
                if current_id > 0 {
                    result.insert(current_id, current_text.clone());
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(XmlError::Parse(e.to_string())),
            _ => {}
        }
    }
    Ok(result)
}

/// A view of equipment/jewels/flasks as **raw text blocks** (for the web contract
/// layer's display, doesn't participate in the calculation).
///
/// Complements [`parse_build`]'s structured [`Item`] path: this keeps PoB's raw text
/// (including `Rarity:` / `Radius:` / brace-tagged lines) for the frontend to color and
/// render directly, PoB2-style.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawItemsView {
    /// The active ItemSet's equipment: `(slot's stable id, raw text block)`, sorted by slot id.
    pub equipped: Vec<(String, String)>,
    /// Tree socket jewels' raw text (not filtered by allocation state — the display view collects all of them).
    pub jewels: Vec<String>,
    /// Tree socket jewels with their socket node number (the editable view: `(socket node's skill id, raw text)`).
    pub socket_jewels: Vec<(u32, String)>,
    /// Active Flask/Charm: `(slot name, raw text block)`.
    pub flasks: Vec<(String, String)>,
}

/// Parses the free-text `<Notes>` in the build XML (PoB's notes page; returns `None` when the section is absent).
pub fn parse_notes(xml: &str) -> Result<Option<String>, XmlError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut in_notes = false;
    let mut text = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) if element_name(&e) == "Notes" => in_notes = true,
            Ok(Event::Text(t)) if in_notes => match t.decode() {
                Ok(chunk) => text.push_str(&chunk),
                Err(_) => text.push_str(&String::from_utf8_lossy(&t)),
            },
            Ok(Event::GeneralRef(r)) if in_notes => append_general_ref(&mut text, &r),
            Ok(Event::End(e)) if element_name_end(&e) == "Notes" => break,
            Ok(Event::Eof) => break,
            Err(e) => return Err(XmlError::Parse(e.to_string())),
            _ => {}
        }
    }
    let trimmed = text.trim();
    Ok((!trimmed.is_empty()).then(|| trimmed.to_string()))
}

/// Extracts the list of sets of each category in the build XML (`<Spec>` /
/// `<ItemSet>` / `<SkillSet>`), for [`crate::loadout::derive_loadouts`] to derive
/// grouped switching.
///
/// `id` uses **document order** (1-based) rather than the XML's `id` attribute —
/// `activeSpec` / `activeItemSet` / `activeSkillSet` are all selected by ordinal
/// position (see [`parse_passive_nodes`] etc.), and an element's own `id` attribute
/// doesn't necessarily match that. `title` defaults to `"Default"`, matching vendor's
/// `spec.title or "Default"` semantics.
pub fn parse_build_sets(xml: &str) -> Result<BuildSets, XmlError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut sets = BuildSets::default();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let bucket = match element_name(&e).as_str() {
                    "Spec" => &mut sets.trees,
                    "ItemSet" => &mut sets.items,
                    "SkillSet" => &mut sets.skills,
                    _ => continue,
                };
                let title = attr_value(&e, b"title")
                    .filter(|t| !t.trim().is_empty())
                    .unwrap_or_else(|| "Default".to_string());
                bucket.push(SetRef {
                    id: bucket.len() + 1,
                    title,
                });
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(XmlError::Parse(e.to_string())),
            _ => {}
        }
    }
    Ok(sets)
}

/// Parses the raw item text view of the build XML (see [`RawItemsView`]).
pub fn parse_raw_items_view(xml: &str) -> Result<RawItemsView, XmlError> {
    let texts = parse_raw_item_texts(xml)?;
    let (slot_assignments, jewel_ids, flask_charm_ids, _) = parse_active_item_set(xml)?;
    let mut equipped: Vec<(String, String)> = slot_assignments
        .into_iter()
        .filter_map(|(slot, id)| Some((slot.id().to_string(), texts.get(&id)?.clone())))
        .collect();
    equipped.sort();
    let socket_items = parse_socket_node_items(xml)?;
    let mut socket_jewels: Vec<(u32, String)> = socket_items
        .iter()
        .filter_map(|&(node, id)| Some((node, texts.get(&id)?.clone())))
        .collect();
    socket_jewels.sort_by_key(|(node, _)| *node);
    let mut jewel_item_ids: Vec<u32> = jewel_ids;
    jewel_item_ids.extend(socket_items.into_iter().map(|(_, id)| id));
    jewel_item_ids.sort_unstable();
    jewel_item_ids.dedup();
    let jewels = jewel_item_ids
        .iter()
        .filter_map(|id| texts.get(id).cloned())
        .collect();
    let flasks = flask_charm_ids
        .into_iter()
        .filter_map(|(slot, id)| Some((slot, texts.get(&id)?.clone())))
        .collect();
    Ok(RawItemsView {
        equipped,
        jewels,
        socket_jewels,
        flasks,
    })
}

/// Extracts radius jewel info from a jewel's raw text (the `... in Radius also grant
/// ...` line + the `Radius:` tier + the Notable-effect-boost line); returns `None` when
/// there's no radius mod. Shared logic between XML import and manually-entered web jewels.
pub fn radius_jewel_from_text(socket_node: u32, text: &str) -> Option<RadiusJewel> {
    let grant_lines: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|l| l.contains("in Radius also grant"))
        .map(strip_brace_tags)
        .collect();
    // `N% increased Effect of Notable Passive Skills in Radius` (vendor
    // ModParser.lua:6847): the last of multiple such lines on the same jewel wins (matching vendor's overwrite-on-write semantics).
    let notable_effect_inc: u32 = text
        .lines()
        .map(str::trim)
        .map(strip_brace_tags)
        .filter_map(|l| {
            l.strip_suffix("% increased Effect of Notable Passive Skills in Radius")
                .and_then(|n| n.trim().parse::<u32>().ok())
        })
        .next_back()
        .unwrap_or(0);
    if grant_lines.is_empty() && notable_effect_inc == 0 {
        return None;
    }
    let radius_label = text
        .lines()
        .map(str::trim)
        .find_map(|l| l.strip_prefix("Radius:").map(|r| r.trim().to_string()))
        .filter(|s| !s.is_empty());
    Some(RadiusJewel {
        socket_node,
        radius_label,
        grant_lines,
        notable_effect_inc,
    })
}

/// Parses radius jewels: collects tree socket jewels (`<Socket nodeId itemId>`) that
/// carry an `... in Radius also grant ...` mod, along with their `Radius:` tier, into
/// [`RadiusJewel`]s (the geometric expansion input).
///
/// Only collects jewels that **actually carry an `also grant` line**; jewels without it
/// produce no entry (their global mods are still injected via the `jewels` path, no duplication).
fn parse_radius_jewels(
    xml: &str,
    allocated: &std::collections::HashSet<u32>,
) -> Result<Vec<RadiusJewel>, XmlError> {
    let socket_items = parse_socket_node_items(xml)?;
    let raw_texts = parse_raw_item_texts(xml)?;
    let mut out = Vec::new();
    for (socket_node, item_id) in socket_items {
        // A jewel in an unallocated socket is entirely inactive (the same gate as parse_items_and_slots).
        if !allocated.contains(&socket_node) {
            continue;
        }
        let Some(text) = raw_texts.get(&item_id) else {
            continue;
        };
        if let Some(jewel) = radius_jewel_from_text(socket_node, text) {
            out.push(jewel);
        }
    }
    // Deterministic: sorted by socket node, then by lines.
    out.sort_by(|a, b| {
        a.socket_node
            .cmp(&b.socket_node)
            .then_with(|| a.grant_lines.cmp(&b.grant_lines))
    });
    Ok(out)
}

/// Strips a PoB mod line's brace-tag prefixes such as `{crafted}` / `{desecrated}`.
fn strip_brace_tags(line: &str) -> String {
    let mut s = line.trim();
    while let Some(rest) = s.strip_prefix('{') {
        if let Some(end) = rest.find('}') {
            s = rest[end + 1..].trim_start();
        } else {
            break;
        }
    }
    s.to_string()
}

/// Parses every `<Item id="N">…</Item>` text block into `id -> Item`.
///
/// Item text is `<Item>`'s text content, interspersed with `<ModRange>` child elements
/// (only the text portion is taken). A block that fails to parse is skipped (error
/// tolerant), without aborting the rest of the parse.
fn parse_item_blocks(xml: &str) -> Result<std::collections::HashMap<u32, Item>, XmlError> {
    let mut reader = Reader::from_str(xml);
    // Preserve original newlines / indentation: item text blocks are parsed line by line.
    reader.config_mut().trim_text(false);

    let mut result = std::collections::HashMap::new();
    let mut in_item = false;
    let mut current_id: u32 = 0;
    let mut current_text = String::new();
    // Count of `<Item>` blocks skipped due to structural parse failure (a single diagnostic emitted at the end of the loop, see below).
    let mut skipped: usize = 0;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) if element_name(&e) == "Item" => {
                in_item = true;
                current_text.clear();
                current_id = attr_value(&e, b"id")
                    .and_then(|v| v.parse::<u32>().ok())
                    .unwrap_or(0);
            }
            Ok(Event::Text(t)) if in_item => match t.decode() {
                Ok(text) => current_text.push_str(&text),
                // On decode failure, degrade to a lossy decode of the raw bytes rather
                // than dropping the whole block — otherwise a mod line would be silently
                // truncated, causing this item to calculate incorrectly.
                Err(_) => current_text.push_str(&String::from_utf8_lossy(&t)),
            },
            Ok(Event::GeneralRef(r)) if in_item => append_general_ref(&mut current_text, &r),
            Ok(Event::End(e)) if element_name_end(&e) == "Item" && in_item => {
                in_item = false;
                if current_id > 0 {
                    match parse_pob_xml_item(&current_text) {
                        Ok(item) => {
                            result.insert(current_id, item);
                        }
                        // A structural parse failure still skips this item (preserving
                        // PoB's error-tolerant semantics, without aborting the whole
                        // import), but the count gets one diagnostic at the end of the
                        // loop — to avoid a malformed custom item being silently
                        // dropped, or participating in the calculation with wrong
                        // attributes, with no signal at all.
                        Err(_) => skipped += 1,
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(XmlError::Parse(e.to_string())),
            _ => {}
        }
    }
    if skipped > 0 {
        eprintln!("[POBR_WARN] parse_item_blocks: skipped {skipped} unparseable <Item> block(s)");
    }
    Ok(result)
}

/// Accumulates a `<ItemSet>`'s slot mapping and weapon-set flag.
struct ItemSetData {
    id: String,
    use_second_weapon_set: bool,
    /// `(slot name, item_id, active)` — `active` only matters for Flask/Charm slots
    /// (PoB's `<Slot active="true">` marks a flask/charm's enabled state).
    slots: Vec<(String, u32, bool)>,
}

/// Reads `id` and `useSecondWeaponSet` from an `<ItemSet>` tag (slots are filled in afterward by `<Slot>`).
fn item_set_data(e: &BytesStart<'_>) -> ItemSetData {
    ItemSetData {
        id: attr_value(e, b"id").unwrap_or_default(),
        use_second_weapon_set: attr_bool(e, b"useSecondWeaponSet"),
        slots: Vec::new(),
    }
}

/// Parses the `<ItemSet>` selected by `<Items activeItemSet>`, returning
/// `(equipment slot map, jewel item_id list)`. `itemId="0"` (empty slot) and slot names
/// outside the enum are ignored; the weapon set toggles per that ItemSet's own
/// `useSecondWeaponSet`; items in `Jewel*` / `*Socket*` slots go into the jewel list
/// (injected globally, see orchestrator).
fn parse_active_item_set(xml: &str) -> Result<SlotAssignments, XmlError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut active_item_set: Option<String> = None;
    let mut sets: Vec<ItemSetData> = Vec::new();
    let mut current: Option<ItemSetData> = None;

    loop {
        match reader.read_event() {
            // `<Items>` / `<ItemSet>` have child elements: a start tag.
            Ok(Event::Start(e)) => match element_name(&e).as_str() {
                "Items" => active_item_set = attr_value(&e, b"activeItemSet"),
                "ItemSet" => current = Some(item_set_data(&e)),
                _ => {}
            },
            // `<Slot/>` is always self-closing; `<ItemSet/>` (with no slots) can be too.
            Ok(Event::Empty(e)) => match element_name(&e).as_str() {
                "Items" => active_item_set = attr_value(&e, b"activeItemSet"),
                "ItemSet" => sets.push(item_set_data(&e)),
                "Slot" => {
                    if let Some(cur) = current.as_mut()
                        && let (Some(slot_name), Some(item_id)) = (
                            attr_value(&e, b"name"),
                            attr_value(&e, b"itemId").and_then(|v| v.parse::<u32>().ok()),
                        )
                    {
                        cur.slots
                            .push((slot_name, item_id, attr_bool(&e, b"active")));
                    }
                }
                _ => {}
            },
            Ok(Event::End(e)) if element_name_end(&e) == "ItemSet" => {
                if let Some(data) = current.take() {
                    sets.push(data);
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(XmlError::Parse(e.to_string())),
            _ => {}
        }
    }

    let Some(first_set) = sets.first() else {
        return Ok((Vec::new(), Vec::new(), Vec::new(), false));
    };
    let chosen = active_item_set
        .as_deref()
        .and_then(|id| sets.iter().find(|s| s.id == id))
        .unwrap_or(first_set);

    let mut assignments = Vec::new();
    let mut jewel_ids = Vec::new();
    let mut flask_charm_ids = Vec::new();
    for (slot_name, item_id, active) in &chosen.slots {
        if *item_id == 0 {
            continue;
        }
        if let Some(slot) = slot_from_pob_name(slot_name, chosen.use_second_weapon_set) {
            assignments.push((slot, *item_id));
        } else if is_jewel_slot(slot_name) {
            jewel_ids.push(*item_id);
        } else if *active && is_flask_charm_slot(slot_name) {
            // Only **active** (`active="true"`) flasks/charms enter the calculation —
            // mirroring PoB2's flask/charm enable toggle (CalcSetup.lua:1014-1028's
            // `slot.active` gating of env.flasks/charms).
            // The slot name is kept too (for `SourceId(Flask, "flask.<slot>")` attribution + flask/charm classification).
            flask_charm_ids.push((slot_name.clone(), *item_id));
        }
    }
    Ok((
        assignments,
        jewel_ids,
        flask_charm_ids,
        chosen.use_second_weapon_set,
    ))
}

/// PoB flask/charm slot names (`Flask 1`/`Flask 2`/`Charm 1..3`).
fn is_flask_charm_slot(name: &str) -> bool {
    name.starts_with("Flask ") || name.starts_with("Charm ")
}

/// PoB jewel/abyss slot names (`Jewel 12345` / `… Abyssal Socket N` / `… Socket N`) → collected into the jewel list.
fn is_jewel_slot(name: &str) -> bool {
    name.starts_with("Jewel") || name.contains("Socket")
}

/// PoB `<Slot name>` → [`EquipmentSlot`]. Slot names outside the enum (Charm/Flask/armour
/// swap groups etc.) return `None` (these sources don't enter the current equipment
/// calculation). The weapon set toggles per `use_second_weapon_set`.
fn slot_from_pob_name(name: &str, use_second_weapon_set: bool) -> Option<EquipmentSlot> {
    match name {
        "Helmet" => Some(EquipmentSlot::Helmet),
        "Body Armour" => Some(EquipmentSlot::BodyArmour),
        "Gloves" => Some(EquipmentSlot::Gloves),
        "Boots" => Some(EquipmentSlot::Boots),
        "Amulet" => Some(EquipmentSlot::Amulet),
        "Ring 1" => Some(EquipmentSlot::Ring1),
        "Ring 2" => Some(EquipmentSlot::Ring2),
        // The third ring slot (the Ritualist ascendancy's "Unfurled Finger"); whether it
        // participates in the calculation is gated by the orchestrator based on
        // AdditionalRingSlot allocation state (PoB2 CalcSetup.lua:821).
        "Ring 3" => Some(EquipmentSlot::Ring3),
        "Belt" => Some(EquipmentSlot::Belt),
        "Weapon 1" if !use_second_weapon_set => Some(EquipmentSlot::Weapon1),
        "Weapon 2" if !use_second_weapon_set => Some(EquipmentSlot::Weapon2),
        "Weapon 1 Swap" if use_second_weapon_set => Some(EquipmentSlot::Weapon1),
        "Weapon 2 Swap" if use_second_weapon_set => Some(EquipmentSlot::Weapon2),
        _ => None,
    }
}

// Skill gem groups

/// Parses each `<Skill>` under the `<SkillSet>` selected by `<Skills activeSkillSet>`
/// into a [`SocketGroup`] (enabled state from `Skill.enabled`, gem ids taken from the
/// enabled `<Gem gemId>`s).
fn parse_socket_groups(xml: &str) -> Result<Vec<SocketGroup>, XmlError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let active_skill_set = active_skill_set_id(xml)?;

    let mut in_target_set = active_skill_set.is_none(); // Collects the first set encountered when there's no active marker
    let mut first_set_consumed = false;
    let mut groups: Vec<SocketGroup> = Vec::new();
    let mut current: Option<SocketGroup> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = element_name(&e);
                match name.as_str() {
                    "SkillSet" => {
                        let set_id = attr_value(&e, b"id");
                        in_target_set = match active_skill_set.as_deref() {
                            Some(target) => set_id.as_deref() == Some(target),
                            None => !first_set_consumed,
                        };
                    }
                    "Skill" if in_target_set => {
                        let enabled = attr_bool_default_true(&e, b"enabled");
                        let mut group = SocketGroup::new().with_enabled(enabled);
                        if let Some(source) = attr_value(&e, b"source") {
                            group = group.with_source(source);
                        }
                        if let Some(slot) = attr_value(&e, b"slot") {
                            group = group.with_slot(slot);
                        }
                        // PoB `mainActiveSkill` (1-based, indexing this group's
                        // non-support skill list): marks the designated main skill in a
                        // group with multiple active skills (e.g. Cast on Crit +
                        // Comet); the actual "skip support/meta, pick by ordinal"
                        // determination happens in resolve_main_skill (which has
                        // granted_effect data available).
                        if let Some(n) =
                            attr_value(&e, b"mainActiveSkill").and_then(|v| v.parse::<usize>().ok())
                        {
                            group = group.with_main_active_skill(n);
                        }
                        current = Some(group);
                    }
                    "Gem" if in_target_set => {
                        if let Some(cur) = current.as_mut()
                            && attr_bool_default_true(&e, b"enabled")
                        {
                            let gem_id = attr_value(&e, b"gemId").filter(|v| !v.is_empty());
                            let skill_id = attr_value(&e, b"skillId").filter(|v| !v.is_empty());
                            // A lineage support (e.g. Atziri's Communion) lacks
                            // skillId/gemId when serialized, only nameSpec — the display
                            // name is kept, and the orchestrator's `stage_build_view`
                            // resolves it back to an id by looking it up against granted_effects.
                            let name_spec = attr_value(&e, b"nameSpec").filter(|v| !v.is_empty());
                            // Captures skillId + level + quality for every enabled gem
                            // (both active and support). A missing/invalid quality
                            // attribute defaults to 0 (no quality), matching PoB2
                            // SkillsTab.lua's `quality` attribute read (default 0).
                            if skill_id.is_some() || name_spec.is_some() {
                                let level = attr_value(&e, b"level")
                                    .and_then(|v| v.parse::<u32>().ok())
                                    .unwrap_or(1);
                                let quality = attr_value(&e, b"quality")
                                    .and_then(|v| v.parse::<u32>().ok())
                                    .unwrap_or(0);
                                // statSet form selection (T5.4, PoB2 SkillsTab.lua:354
                                // reads / :489 writes): invalid/missing/the literal
                                // "nil" (PoB2's default serialization) → None (default
                                // primary set). `statSetIndexCalcs` (a separate
                                // selection on the calcs page) is not handled, and ignored.
                                let stat_set_index = attr_value(&e, b"statSetIndex")
                                    .and_then(|v| v.parse::<u32>().ok());
                                // The first enabled gem in the group with a skillId is
                                // treated as the active skill (PoB's Gem list has active
                                // first; nameSpec-only references are always lineage
                                // supports and don't factor into active-skill determination).
                                if let Some(skill_id) = &skill_id
                                    && cur.active_skill_id.is_none()
                                {
                                    cur.active_skill_id = Some(skill_id.clone());
                                    cur.active_gem_level = Some(level);
                                    cur.active_gem_quality = Some(quality);
                                }
                                let name_spec_pending = skill_id.is_none();
                                cur.gem_skills.push(crate::build::GemSkillRef {
                                    skill_id: skill_id.unwrap_or_default(),
                                    gem_level: level,
                                    quality,
                                    stat_set_index,
                                    name_spec: if name_spec_pending { name_spec } else { None },
                                });
                            }
                            if let Some(gem_id) = gem_id {
                                cur.gem_ids.push(gem_id);
                            }
                        }
                    }
                    "StatSetIndex" if in_target_set => {
                        // PoB2's newer statSet serialization (confirmed against real
                        // ninja codes; vendor SkillsTab.lua:375 reads / :508 writes): a
                        // per-grantedEffect child element
                        // `<StatSetIndex grantedEffect="X" index="2"/>`, in which case
                        // the Gem's `statSetIndex` attribute is the literal `"nil"`. The
                        // child element arrives before the Gem's End, so it's attributed
                        // to the most recently pushed gem; only backfilled when
                        // grantedEffect matches that gem's skillId and the attribute
                        // channel hasn't already supplied a value (the older attribute
                        // takes priority, backward compatible).
                        if let Some(cur) = current.as_mut()
                            && let Some(effect) = attr_value(&e, b"grantedEffect")
                            && let Some(idx) =
                                attr_value(&e, b"index").and_then(|v| v.parse::<u32>().ok())
                            && let Some(gem) = cur.gem_skills.last_mut()
                            && gem.skill_id == effect
                            && gem.stat_set_index.is_none()
                        {
                            gem.stat_set_index = Some(idx);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(e)) => match element_name_end(&e).as_str() {
                "Skill" if in_target_set => {
                    if let Some(group) = current.take()
                        && !group.gem_ids.is_empty()
                    {
                        groups.push(group);
                    }
                }
                "SkillSet" if in_target_set => {
                    first_set_consumed = true;
                    in_target_set = false;
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(e) => return Err(XmlError::Parse(e.to_string())),
            _ => {}
        }
    }

    Ok(groups)
}

/// Reads the target SkillSet id from `<Skills activeSkillSet>` (returns `None` when missing).
fn active_skill_set_id(xml: &str) -> Result<Option<String>, XmlError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if element_name(&e) == "Skills" => {
                return Ok(attr_value(&e, b"activeSkillSet"));
            }
            Ok(Event::Eof) => return Ok(None),
            Err(e) => return Err(XmlError::Parse(e.to_string())),
            _ => {}
        }
    }
}

// quick-xml helpers

fn element_name(e: &BytesStart<'_>) -> String {
    String::from_utf8_lossy(e.name().as_ref()).into_owned()
}

fn element_name_end(e: &quick_xml::events::BytesEnd<'_>) -> String {
    String::from_utf8_lossy(e.name().as_ref()).into_owned()
}

fn attr_value(e: &BytesStart<'_>, key: &[u8]) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.as_ref() == key)
        .and_then(|a| {
            // Deliberately avoid normalized_value: whitespace normalization collapses
            // literal newlines into spaces, but PoB stores multi-line mod text in
            // attributes (<Input string="a\nb">), where the newline is a line separator.
            let raw = String::from_utf8_lossy(&a.value).into_owned();
            quick_xml::escape::unescape(&raw)
                .ok()
                .map(|v| v.into_owned())
        })
}

/// quick-xml 0.38+ splits `&ref;` out of `Text` events into a separate `GeneralRef`
/// event; this restores the old `unescape` behavior: character references and
/// predefined entities decode into the text, unknown entities are kept verbatim
/// (rather than dropped — item text is parsed line by line, and dropping a character
/// would silently truncate a mod line).
fn append_general_ref(buf: &mut String, r: &BytesRef<'_>) {
    if let Ok(Some(ch)) = r.resolve_char_ref() {
        buf.push(ch);
        return;
    }
    match r.decode().as_deref() {
        Ok("amp") => buf.push('&'),
        Ok("lt") => buf.push('<'),
        Ok("gt") => buf.push('>'),
        Ok("apos") => buf.push('\''),
        Ok("quot") => buf.push('"'),
        Ok(name) => {
            buf.push('&');
            buf.push_str(name);
            buf.push(';');
        }
        Err(_) => {
            buf.push('&');
            buf.push_str(&String::from_utf8_lossy(r));
            buf.push(';');
        }
    }
}

/// Boolean attribute: missing or anything other than `"true"` counts as `false`.
fn attr_bool(e: &BytesStart<'_>, key: &[u8]) -> bool {
    attr_value(e, key).as_deref() == Some("true")
}

/// Boolean attribute: missing counts as `true` (matching PoB's default-enabled `enabled`
/// semantics); only an explicit `"false"` turns it off.
fn attr_bool_default_true(e: &BytesStart<'_>, key: &[u8]) -> bool {
    attr_value(e, key).as_deref() != Some("false")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0"?>
<PathOfBuilding2>
    <Build level="92" className="Ranger" ascendClassName="Deadeye" viewMode="TREE"/>
    <Tree activeSpec="1">
        <Spec nodes="100,200,300" treeVersion="0_5"/>
    </Tree>
    <Skills activeSkillSet="1">
        <SkillSet id="1">
            <Skill enabled="true" source="Item:2:Dragon Wand" slot="Weapon 1">
                <Gem gemId="Metadata/Items/Gem/Active" skillId="FireballPlayer" level="18" enabled="true"/>
                <Gem gemId="Metadata/Items/Gems/Support" enabled="true"/>
                <Gem gemId="Metadata/Items/Gems/Disabled" enabled="false"/>
            </Skill>
            <Skill enabled="false">
                <Gem gemId="Metadata/Items/Gem/Other" enabled="true"/>
            </Skill>
        </SkillSet>
    </Skills>
    <Items activeItemSet="1" useSecondWeaponSet="false">
        <Item id="1">
Rarity: RARE
Dragon Hold
Topaz Ring
Item Level: 80
Implicits: 1
+30% to Lightning Resistance
+50 to maximum Life
        </Item>
        <Item id="2">
Rarity: RARE
Plague Core
Siege Crossbow
Item Level: 81
Implicits: 0
Adds 47 to 86 Physical Damage
        </Item>
        <ItemSet useSecondWeaponSet="false" title="Default" id="1">
            <Slot name="Ring 1" itemId="1"/>
            <Slot name="Weapon 1" itemId="2"/>
            <Slot name="Ring 2" itemId="0"/>
            <Slot name="Charm 1" itemId="9"/>
        </ItemSet>
    </Items>
</PathOfBuilding2>"#;

    /// quick-xml 0.38+ splits entity references into GeneralRef events; pins down the
    /// text-collection path's restoration behavior for predefined entities / character
    /// references / unknown entities (dropping a character = silently truncating a mod line).
    #[test]
    fn text_collection_resolves_entity_references() {
        let xml = r#"<PathOfBuilding2>
    <Items activeItemSet="1">
        <Item id="1">
Rarity: UNIQUE
Fury &amp; Wrath
Item Level: 80
+1 to &#65; &unknown; marker
        </Item>
    </Items>
    <Notes>DPS &gt; EHP &amp; life</Notes>
</PathOfBuilding2>"#;
        let texts = parse_raw_item_texts(xml).expect("parse items");
        let item = &texts[&1];
        assert!(item.contains("Fury & Wrath"), "amp entity: {item}");
        assert!(
            item.contains("+1 to A &unknown; marker"),
            "char ref + unknown entity kept verbatim: {item}"
        );
        let notes = parse_notes(xml).expect("parse notes").expect("has notes");
        assert_eq!(notes, "DPS > EHP & life");
    }

    /// PoB uses literal newlines in attribute values to separate multi-line mod text
    /// (custom mods / timeless jewel lines); attribute decoding must preserve whitespace
    /// verbatim (standard XML attribute normalization collapses newlines into spaces,
    /// which would run mod lines together).
    #[test]
    fn attr_value_preserves_literal_newlines() {
        let mut reader = Reader::from_str("<X v=\"line one\nline two &amp; more\"/>");
        loop {
            match reader.read_event() {
                Ok(Event::Empty(e)) => {
                    assert_eq!(
                        attr_value(&e, b"v").as_deref(),
                        Some("line one\nline two & more")
                    );
                    return;
                }
                Ok(Event::Eof) => panic!("no element parsed"),
                _ => {}
            }
        }
    }

    #[test]
    fn parses_full_build_identity() {
        let build = parse_build(SAMPLE).expect("parse");
        assert_eq!(build.character.level, 92);
        assert_eq!(build.character.class_name, "Ranger");
        assert_eq!(build.character.ascendancy_name, "Deadeye");
    }

    #[test]
    fn parses_active_spec_nodes() {
        let build = parse_build(SAMPLE).expect("parse");
        let nodes: Vec<u32> = build.tree.allocated_nodes.iter().map(|n| n.0).collect();
        assert_eq!(nodes, vec![100, 200, 300]);
    }

    /// Weapon-set-exclusive point filtering (PoB2 CalcSetup.lua:209-233/:791-792 +
    /// PassiveSpec.lua:104-144): with `useSecondWeaponSet=false`, WeaponSet2-exclusive
    /// points are inactive; WeaponSet1 and shared points are kept.
    #[test]
    fn weapon_set_nodes_filtered_by_active_set() {
        let xml = r#"<?xml version="1.0"?>
<PathOfBuilding2>
    <Build level="92" className="Ranger" ascendClassName="Deadeye" viewMode="TREE"/>
    <Tree activeSpec="1">
        <Spec nodes="100,200,300,400,500" treeVersion="0_5">
            <WeaponSet1 nodes="200"/>
            <WeaponSet2 nodes="400,500"/>
        </Spec>
    </Tree>
    <Items activeItemSet="1">
        <ItemSet useSecondWeaponSet="false" title="Default" id="1"/>
    </Items>
</PathOfBuilding2>"#;
        let build = parse_build(xml).expect("parse");
        let nodes: Vec<u32> = build.tree.allocated_nodes.iter().map(|n| n.0).collect();
        assert_eq!(nodes, vec![100, 200, 300]);
    }

    /// With `useSecondWeaponSet=true`, WeaponSet1-exclusive points are stripped instead.
    #[test]
    fn weapon_set_nodes_filtered_when_second_set_active() {
        let xml = r#"<?xml version="1.0"?>
<PathOfBuilding2>
    <Build level="92" className="Ranger" ascendClassName="Deadeye" viewMode="TREE"/>
    <Tree activeSpec="1">
        <Spec nodes="100,200,300,400,500" treeVersion="0_5">
            <WeaponSet1 nodes="200"/>
            <WeaponSet2 nodes="400,500"/>
        </Spec>
    </Tree>
    <Items activeItemSet="1">
        <ItemSet useSecondWeaponSet="true" title="Default" id="1"/>
    </Items>
</PathOfBuilding2>"#;
        let build = parse_build(xml).expect("parse");
        let nodes: Vec<u32> = build.tree.allocated_nodes.iter().map(|n| n.0).collect();
        assert_eq!(nodes, vec![100, 300, 400, 500]);
    }

    #[test]
    fn parses_attribute_overrides_from_active_spec() {
        let xml = r#"<?xml version="1.0"?>
<PathOfBuilding2>
    <Build level="92" className="Ranger" ascendClassName="Deadeye" viewMode="TREE"/>
    <Tree activeSpec="1">
        <Spec nodes="100,200,300" treeVersion="0_5">
            <Overrides>
                <AttributeOverride dexNodes="100,200" intNodes="300" strNodes=""/>
            </Overrides>
        </Spec>
    </Tree>
</PathOfBuilding2>"#;
        let build = parse_build(xml).expect("parse");
        let ov = &build.tree.attribute_overrides;
        assert_eq!(ov.get(&NodeId(100)), Some(&AttributeChoice::Dexterity));
        assert_eq!(ov.get(&NodeId(200)), Some(&AttributeChoice::Dexterity));
        assert_eq!(ov.get(&NodeId(300)), Some(&AttributeChoice::Intelligence));
        assert_eq!(ov.len(), 2 + 1);
    }

    #[test]
    fn attribute_overrides_default_empty_without_overrides_element() {
        let build = parse_build(SAMPLE).expect("parse");
        assert!(build.tree.attribute_overrides.is_empty());
    }

    #[test]
    fn quest_stat_rewards_default_to_claimed_when_absent() {
        // SAMPLE has no <Input name="quest…"> at all → every Stat-type reward is backfilled per defaultState=true.
        let build = parse_build(SAMPLE).expect("parse");
        let texts = &build.config.global_modifier_texts;
        assert!(texts.iter().any(|t| t == "+10% to Fire Resistance"));
        assert!(texts.iter().any(|t| t == "+20 to maximum Life"));
        assert!(texts.iter().any(|t| t == "5% increased maximum Life"));
    }

    #[test]
    fn quest_stat_reward_skipped_when_explicitly_unchecked() {
        let xml = r#"<?xml version="1.0"?>
<PathOfBuilding2>
    <Build level="92" className="Ranger" ascendClassName="Deadeye" viewMode="TREE"/>
    <Config>
        <Input name="questAct 3Jiquani's MachinariumBlackjaw" boolean="false"/>
        <Input name="questAct 4Halls Of The DeadNgamahu's Test" string="+5% to Fire Resistance"/>
    </Config>
</PathOfBuilding2>"#;
        let build = parse_build(xml).expect("parse");
        let texts = &build.config.global_modifier_texts;
        // An explicitly-declined check-type reward isn't injected; every other default entry is backfilled as usual.
        assert!(!texts.iter().any(|t| t == "+10% to Fire Resistance"));
        // An Options-type reward is injected per the selected string.
        assert!(texts.iter().any(|t| t == "+5% to Fire Resistance"));
        // Default entries not mentioned are still there.
        assert!(texts.iter().any(|t| t == "+10% to Cold Resistance"));
    }

    // Config resistancePenalty → CampaignProgress (19-G5 wiring)

    #[test]
    fn resistance_penalty_number_maps_to_campaign_progress() {
        let xml = r#"<?xml version="1.0"?>
<PathOfBuilding2>
    <Build level="40" className="Witch"/>
    <Config>
        <Input name="resistancePenalty" number="-10"/>
    </Config>
</PathOfBuilding2>"#;
        let build = parse_build(xml).expect("parse");
        assert_eq!(build.config.campaign_progress, Some(CampaignProgress::Act2));
    }

    #[test]
    fn resistance_penalty_omitted_leaves_progress_unset() {
        // SAMPLE has no resistancePenalty → None (calc side falls back to PoB2's default Endgame -60).
        let build = parse_build(SAMPLE).expect("parse");
        assert_eq!(build.config.campaign_progress, None);
    }

    #[test]
    fn resistance_penalty_unknown_value_falls_back_to_unset() {
        // A value outside PoB2's seven-tier table (which theoretically never happens) isn't force-mapped, and stays None.
        let xml = r#"<?xml version="1.0"?>
<PathOfBuilding2>
    <Build level="40" className="Witch"/>
    <Config>
        <Input name="resistancePenalty" number="-15"/>
    </Config>
</PathOfBuilding2>"#;
        let build = parse_build(xml).expect("parse");
        assert_eq!(build.config.campaign_progress, None);
    }

    // Config enemyIsBoss → EnemyTier (19-G3 wiring)

    #[test]
    fn enemy_is_boss_string_maps_to_enemy_tier() {
        for (raw, expected) in [
            ("None", EnemyTier::None),
            ("Boss", EnemyTier::Boss),
            ("Pinnacle", EnemyTier::Pinnacle),
            ("Uber", EnemyTier::Uber),
        ] {
            let xml = format!(
                r#"<?xml version="1.0"?>
<PathOfBuilding2>
    <Build level="90" className="Witch"/>
    <Config>
        <Input name="enemyIsBoss" string="{raw}"/>
    </Config>
</PathOfBuilding2>"#
            );
            let build = parse_build(&xml).expect("parse");
            assert_eq!(build.config.enemy_tier, Some(expected), "string={raw}");
        }
    }

    #[test]
    fn enemy_is_boss_omitted_or_unknown_leaves_tier_unset() {
        // SAMPLE has no enemyIsBoss → None (calc side falls back to the orchestrator option, default Pinnacle).
        let build = parse_build(SAMPLE).expect("parse");
        assert_eq!(build.config.enemy_tier, None);

        // A string outside the table isn't force-mapped (a Placeholder element likewise isn't read).
        let xml = r#"<?xml version="1.0"?>
<PathOfBuilding2>
    <Build level="90" className="Witch"/>
    <Config>
        <Input name="enemyIsBoss" string="SuperUber"/>
        <Placeholder name="enemyLevel" number="82"/>
    </Config>
</PathOfBuilding2>"#;
        let build = parse_build(xml).expect("parse");
        assert_eq!(build.config.enemy_tier, None);
        // `<Placeholder>` lands in raw_inputs.placeholders (not mixed into values — the
        // interpreter's activation semantics only recognize Input); enemyLevel's
        // consumer reads it as an "Input missing → Placeholder fallback" (vendor
        // ConfigTab.lua:872-877).
        assert!(!build.config.raw_inputs.values.contains_key("enemyLevel"));
        assert_eq!(
            build.config.raw_inputs.placeholders.get("enemyLevel"),
            Some(&pobr_core::rules::config_interpreter::ConfigInputValue::Number(82.0))
        );
    }

    #[test]
    fn assigns_items_to_mapped_slots_only() {
        let build = parse_build(SAMPLE).expect("parse");
        // Ring 1 (item 1) and Weapon 1 (item 2) are mapped; Ring 2 (itemId 0) is an empty slot; Charm 1 is outside the enum.
        let slots: Vec<EquipmentSlot> =
            build.equipped_items().into_iter().map(|(s, _)| s).collect();
        assert!(slots.contains(&EquipmentSlot::Ring1));
        assert!(slots.contains(&EquipmentSlot::Weapon1));
        assert!(
            !slots.contains(&EquipmentSlot::Ring2),
            "an empty slot should not be assigned"
        );
        assert_eq!(
            slots.len(),
            2,
            "slots outside the enum, like Charm, are ignored"
        );
    }

    /// Flask/Charm slot round trip — only slots with `active="true"` enter
    /// `utility_slots` (slot name + item).
    #[test]
    fn utility_slots_keep_slot_names_for_active_flask_charm() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<PathOfBuilding2>
    <Build level="90" className="Ranger" ascendClassName="Pathfinder"/>
    <Items activeItemSet="1">
        <Item id="1">
Rarity: MAGIC
Sapphire Charm of Lightning
Implicits: 1
Used when you take Cold damage from a Hit
+15% to Lightning Resistance
        </Item>
        <Item id="2">
Rarity: MAGIC
Undiluted Ultimate Life Flask
Implicits: 0
69% increased Recovery rate
        </Item>
        <ItemSet id="1">
            <Slot name="Charm 1" itemId="1" active="true"/>
            <Slot name="Flask 1" itemId="2" active="true"/>
            <Slot name="Flask 2" itemId="2"/>
            <Slot name="Charm 2" itemId="0" active="true"/>
        </ItemSet>
    </Items>
</PathOfBuilding2>"#;
        let build = parse_build(xml).expect("parse");
        let names: Vec<(String, String)> = build
            .utility_slots
            .iter()
            .map(|(slot, item)| (slot.clone(), item.base.to_string()))
            .collect();
        assert_eq!(
            names,
            vec![
                (
                    "Charm 1".to_string(),
                    "Sapphire Charm of Lightning".to_string()
                ),
                (
                    "Flask 1".to_string(),
                    "Undiluted Ultimate Life Flask".to_string()
                ),
            ],
            "only active, non-empty slots are included; inactive Flask 2 and empty Charm 2 are ignored"
        );
    }

    #[test]
    fn item_text_parsed_into_segments() {
        let build = parse_build(SAMPLE).expect("parse");
        let (_, ring) = build
            .equipped_items()
            .into_iter()
            .find(|(s, _)| *s == EquipmentSlot::Ring1)
            .expect("ring present");
        assert!(
            ring.modifier_texts
                .iter()
                .any(|t| t == "+50 to maximum Life"),
            "the ring's explicit mod should parse: {:?}",
            ring.modifier_texts
        );
        assert_eq!(ring.implicit_texts, vec!["+30% to Lightning Resistance"]);
    }

    #[test]
    fn parses_socket_groups_respecting_enabled() {
        let build = parse_build(SAMPLE).expect("parse");
        // Two Skills: the first is enabled (2 enabled gems, 1 disabled gem skipped), the second is disabled.
        assert_eq!(build.socket_groups.len(), 2);
        let enabled: Vec<&SocketGroup> = build.enabled_socket_groups().collect();
        assert_eq!(enabled.len(), 1, "only the first Skill is enabled");
        assert_eq!(
            enabled[0].gem_ids,
            vec![
                "Metadata/Items/Gem/Active".to_string(),
                "Metadata/Items/Gems/Support".to_string()
            ],
            "a disabled gem should be skipped"
        );
        // The first enabled gem's skillId + level is captured as the active skill (the key for resolving per-level parameters).
        assert_eq!(
            enabled[0].active_skill_id.as_deref(),
            Some("FireballPlayer")
        );
        assert_eq!(enabled[0].active_gem_level, Some(18));
        assert_eq!(
            enabled[0].source.as_deref(),
            Some("Item:2:Dragon Wand"),
            "the grant source must be preserved, for precise de-duplication of item-granted skill groups"
        );
        assert_eq!(enabled[0].slot.as_deref(), Some("Weapon 1"));
    }

    /// T5.4: `<Gem statSetIndex>` parsing — a number → Some(n); PoB2's default
    /// serialized literal `"nil"` / missing → None (default primary set);
    /// `statSetIndexCalcs` is ignored.
    #[test]
    fn parses_gem_stat_set_index() {
        let xml = r#"<?xml version="1.0"?>
<PathOfBuilding2>
    <Build level="1" className="Witch"/>
    <Skills activeSkillSet="1">
        <SkillSet id="1">
            <Skill enabled="true">
                <Gem gemId="g1" skillId="IceNovaPlayer" level="20" statSetIndex="2" statSetIndexCalcs="3" enabled="true"/>
                <Gem gemId="g2" skillId="ArcPlayer" level="20" statSetIndex="nil" statSetIndexCalcs="nil" enabled="true"/>
                <Gem gemId="g3" skillId="SparkPlayer" level="20" enabled="true"/>
            </Skill>
        </SkillSet>
    </Skills>
</PathOfBuilding2>"#;
        let build = parse_build(xml).expect("parse");
        let gems = &build.socket_groups[0].gem_skills;
        assert_eq!(
            gems[0].stat_set_index,
            Some(2),
            "a numeric attribute parses as Some"
        );
        assert_eq!(
            gems[1].stat_set_index, None,
            "the literal nil normalizes to None"
        );
        assert_eq!(gems[2].stat_set_index, None, "a missing attribute is None");
    }

    #[test]
    fn poe1_root_rejected() {
        // PoE2-only: the PoE1 `PathOfBuilding` root is no longer accepted.
        let xml = r#"<PathOfBuilding><Build level="1" className="Witch"/></PathOfBuilding>"#;
        assert!(matches!(parse_build(xml), Err(XmlError::NotPobRoot(_))));
    }

    #[test]
    fn rejects_non_pob_root() {
        assert!(matches!(
            parse_build("<NotPoB><Build/></NotPoB>"),
            Err(XmlError::NotPobRoot(_))
        ));
    }

    // Config defaultState import (finding 01-06)

    #[test]
    fn omitted_default_true_conditions_fill_to_true() {
        // SAMPLE has no <Config> → every defaultState=true entry should be backfilled true (PoB2's "omission = default value").
        let build = parse_build(SAMPLE).expect("parse");
        for (_, cond_var) in DEFAULT_TRUE_CONDITIONS {
            assert_eq!(
                build.config.conditions.get(*cond_var).copied(),
                Some(true),
                "an omitted defaultState=true condition {cond_var} should be backfilled true"
            );
        }
    }

    #[test]
    fn explicit_false_overrides_default_true() {
        // XML explicitly gives inDemonForm=false → shouldn't be overridden by the default true.
        let xml = r#"<?xml version="1.0"?>
<PathOfBuilding2>
    <Build level="1" className="Witch"/>
    <Config>
        <Input name="inDemonForm" boolean="false"/>
        <Input name="conditionChampionIntimidate" boolean="false"/>
    </Config>
</PathOfBuilding2>"#;
        let build = parse_build(xml).expect("parse");
        assert_eq!(
            build.config.conditions.get("DemonForm").copied(),
            Some(false)
        );
        assert_eq!(
            build.config.conditions.get("ChampionIntimidate").copied(),
            Some(false)
        );
        // Every other defaultState=true entry that didn't appear is still backfilled true.
        assert_eq!(
            build.config.conditions.get("CompanionInPresence").copied(),
            Some(true)
        );
    }

    #[test]
    fn explicit_true_default_condition_maps_to_calc_var() {
        // Explicit targetBrandedEnemy=true → the calc-side condition name TargetingBrandedEnemy=true.
        let xml = r#"<?xml version="1.0"?>
<PathOfBuilding2>
    <Build level="1" className="Witch"/>
    <Config>
        <Input name="targetBrandedEnemy" boolean="true"/>
    </Config>
</PathOfBuilding2>"#;
        let build = parse_build(xml).expect("parse");
        assert_eq!(
            build
                .config
                .conditions
                .get("TargetingBrandedEnemy")
                .copied(),
            Some(true)
        );
    }

    #[test]
    fn ordinary_omitted_conditions_remain_unset() {
        // An ordinary condition without defaultState=true still doesn't enter the table when omitted (calc side falls back to false).
        let build = parse_build(SAMPLE).expect("parse");
        assert!(
            !build.config.conditions.contains_key("EnemyChilled"),
            "an ordinary condition should not be backfilled with a default when omitted"
        );
    }
}
