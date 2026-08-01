//! Config interpreter.
//!
//! Input = `config_options.json` entries ([`ConfigOptionDef`]) + raw XML input
//! ([`RawConfigInputs`]) + a handler registry; output = [`ConfigOutcome`]
//! (player/enemy/minion Modifier lanes + conditions/multipliers backfill +
//! the customMods text lane + scalar echo). Pure, deterministic, no I/O;
//! wiring it up (switching xml_build to dual-run, injecting a ConfigCatalog)
//! is tracked under A5.
//!
//! Evaluation order:
//! 1. For each entry, resolve "explicit input else default"; check=false/None
//!    is skipped outright; count=0 is skipped (matches vendor BuildModList
//!    semantics);
//! 2. effects are instantiated one by one (numeric values go through the
//!    single `rules::value_expr` evaluator; an effect whose `emit_if`
//!    predicate fails is not emitted);
//! 3. target routes to player/enemy/minion; FLAG/BASE effects whose name is
//!    prefixed `Condition:`/`Multiplier:` also backfill the conditions/
//!    multipliers tables (kept for compatibility with the existing cfg lanes);
//! 4. `imply_conditions` are expanded (only when the entry's value is true;
//!    never overrides an existing explicit value);
//! 5. entries with a `handler_id` are looked up in the registry; unregistered
//!    ones are recorded in the `unhandled` report instead of panicking;
//! 6. text-type entries (customMods) are StripEscapes'd line by line into
//!    `custom_mod_lines`, fed to mod_parser by the build layer (vendor
//!    ConfigOptions.lua:2278-2296).

use std::collections::BTreeMap;

use pobr_data::catalog::config_def::{
    ConfigEffect, ConfigInputType, ConfigOptionDef, ListEffectValue, ListScalar, NestedModDef,
};
use pobr_data::catalog::value_expr::{EffectTag, EffectTarget};
use pobr_data::modifier::{ModFlags, ModType};
use pobr_data::source::{ModifierSource, SourceId, SourceKind};

use crate::modifier::{ActorRef, ModTag, ModValue, Modifier};
use crate::rules::registry::{HandlerCtx, HandlerRegistry};
use crate::rules::value_expr;

/// A raw config input value, as read from an xml_build `<Input name bool|number|string>`.
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigInputValue {
    /// Boolean input.
    Bool(bool),
    /// Numeric input.
    Number(f64),
    /// Text input (list option value / customMods).
    Text(String),
}

impl ConfigInputValue {
    /// Numeric view (`Number` as-is; `Bool` → 1.0/0.0; `Text` → `None`). Used by the
    /// orchestration layer to read count/integer-type config inputs (e.g. DistanceRamp's
    /// enemyDistance Input value).
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Self::Number(value) => Some(*value),
            Self::Bool(value) => Some(if *value { 1.0 } else { 0.0 }),
            Self::Text(_) => None,
        }
    }
}

/// A set of raw config inputs (var → value).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RawConfigInputs {
    /// Explicit inputs (parsed from XML `<Input>`).
    pub values: BTreeMap<String, ConfigInputValue>,
    /// Placeholder inputs (parsed from XML `<Placeholder>`). Vendor only consumes this for a
    /// handful of scalars, as an "Input missing → fall back to Placeholder" path (e.g.
    /// `enemyLevel`, ConfigTab.lua:872-877); the interpreter's main flow doesn't read this
    /// table, it's up to individual consumers to use it as needed.
    pub placeholders: BTreeMap<String, ConfigInputValue>,
}

impl RawConfigInputs {
    /// An empty input set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert one input (builder style, for test convenience).
    pub fn with(mut self, var: impl Into<String>, value: ConfigInputValue) -> Self {
        self.values.insert(var.into(), value);
        self
    }
}

/// Result of interpreting a set of config entries.
#[derive(Debug, Clone, Default)]
pub struct ConfigOutcome {
    /// Modifiers to write into the player modDB.
    pub player_mods: Vec<Modifier>,
    /// Modifiers to write into the enemy modDB.
    pub enemy_mods: Vec<Modifier>,
    /// Modifiers to write into the minion modDB (from expanding MinionModifier LIST nested mods).
    pub minion_mods: Vec<Modifier>,
    /// Backfill table for `Condition:`-prefixed FLAGs (kept for compatibility with the existing
    /// cfg.conditions lane).
    pub conditions: BTreeMap<String, bool>,
    /// Backfill table for `Multiplier:`-prefixed BASEs (kept for compatibility with the existing
    /// cfg.multipliers lane).
    pub multipliers: BTreeMap<String, f64>,
    /// SkillData LIST key/value payloads (read by consumers as needed).
    pub skill_data: Vec<SkillDataEntry>,
    /// Raw customMods text lines (after StripEscapes; fed to mod_parser by the build layer).
    pub custom_mod_lines: Vec<String>,
    /// Resolved scalar echo for every activated entry (scalar consumers such as
    /// resistancePenalty/enemyLevel read their value from here, including default resolution).
    pub scalars: BTreeMap<String, ConfigInputValue>,
    /// Entries whose handler_id has no registered handler (coverage report).
    pub unhandled: Vec<UnhandledEntry>,
    /// Non-fatal warnings (unmapped flags, unwired tags, etc.), meant for human review.
    pub diagnostics: Vec<String>,
}

/// A SkillData key/value payload.
#[derive(Debug, Clone, PartialEq)]
pub struct SkillDataEntry {
    /// Payload key (e.g. `corpseLife`).
    pub key: String,
    /// Payload value.
    pub value: ConfigInputValue,
}

/// Record of an entry whose handler is not registered.
#[derive(Debug, Clone, PartialEq)]
pub struct UnhandledEntry {
    /// The entry's var.
    pub var: String,
    /// The handler_id declared in data.
    pub handler_id: String,
}

/// Interpret every config entry (pure function).
pub fn interpret(
    options: &[ConfigOptionDef],
    inputs: &RawConfigInputs,
    registry: &HandlerRegistry,
) -> ConfigOutcome {
    let mut outcome = ConfigOutcome::default();
    for def in options {
        interpret_entry(def, inputs, registry, &mut outcome);
    }
    outcome
}

/// Resolve a single entry's "explicit input else default" activation value.
///
/// `None` = the entry is not activated (skip it); `Some((echo value, numeric input))` = it's
/// activated, where the numeric input feeds effect instantiation (check is always 1.0, list
/// uses the option's number or 0).
fn resolve_value(
    def: &ConfigOptionDef,
    inputs: &RawConfigInputs,
) -> Option<(ConfigInputValue, f64)> {
    let explicit = inputs.values.get(&def.var);
    match def.input_type {
        ConfigInputType::Check => {
            let enabled = match explicit {
                Some(ConfigInputValue::Bool(b)) => *b,
                Some(ConfigInputValue::Number(n)) => *n != 0.0,
                Some(ConfigInputValue::Text(_)) => false,
                None => def
                    .default
                    .as_ref()
                    .and_then(|d| d.state_bool)
                    .unwrap_or(false),
            };
            enabled.then_some((ConfigInputValue::Bool(true), 1.0))
        }
        ConfigInputType::Count
        | ConfigInputType::CountAllowZero
        | ConfigInputType::Integer
        | ConfigInputType::Float => {
            let number = match explicit {
                Some(value) => value.as_number()?,
                None => def
                    .default
                    .as_ref()
                    .and_then(|d| d.state_number.or(d.placeholder_number))?,
            };
            // vendor BuildModList: for count, 0 counts as unset; other types apply at 0 too.
            if def.input_type == ConfigInputType::Count && number == 0.0 {
                return None;
            }
            Some((ConfigInputValue::Number(number), number))
        }
        ConfigInputType::List => {
            let selected = match explicit {
                Some(ConfigInputValue::Text(text)) => Some(text.clone()),
                Some(ConfigInputValue::Number(n)) => Some(format_list_number(*n)),
                Some(ConfigInputValue::Bool(_)) => None,
                None => def
                    .default
                    .as_ref()
                    .and_then(|d| d.index)
                    .and_then(|index| def.list_options.get(index.saturating_sub(1) as usize))
                    .map(|option| option.value.clone()),
            }?;
            let numeric = def
                .list_options
                .iter()
                .find(|option| option.value == selected)
                .and_then(|option| option.number)
                .unwrap_or(0.0);
            Some((ConfigInputValue::Text(selected), numeric))
        }
        ConfigInputType::Text => match explicit {
            Some(ConfigInputValue::Text(text)) if !text.is_empty() => {
                Some((ConfigInputValue::Text(text.clone()), 0.0))
            }
            _ => None,
        },
    }
}

/// Stringify a list option's numeric value (matches the extractor's `tostring(val)`: integers
/// have no decimal point).
fn format_list_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

fn interpret_entry(
    def: &ConfigOptionDef,
    inputs: &RawConfigInputs,
    registry: &HandlerRegistry,
    outcome: &mut ConfigOutcome,
) {
    let Some((display_value, input_number)) = resolve_value(def, inputs) else {
        return;
    };

    // Scalar echo (all activated entries; scalar consumers like resistancePenalty read from here).
    outcome
        .scalars
        .insert(def.var.clone(), display_value.clone());

    // customMods text lane (the sole text-type entry).
    if def.input_type == ConfigInputType::Text {
        if let ConfigInputValue::Text(text) = &display_value {
            outcome.custom_mod_lines.extend(
                text.lines()
                    .map(strip_escapes)
                    .map(|line| line.trim().to_string())
                    .filter(|line| !line.is_empty()),
            );
        }
        return;
    }

    // handler_id entries: look up the registry, record unregistered ones in the report. The
    // four output lanes each land in their own bucket (see registry::HandlerOutcome docs; the
    // config call site's ctx only carries the numeric placeholder argument).
    if let Some(handler_id) = &def.handler_id {
        match registry.get(handler_id) {
            Some(handler) => {
                let inputs = [input_number];
                let result = handler(&HandlerCtx::with_inputs(&inputs));
                outcome.player_mods.extend(
                    result
                        .player_mods
                        .into_iter()
                        .map(|m| attach_origin(m, def, EffectTarget::Player)),
                );
                outcome.enemy_mods.extend(
                    result
                        .enemy_mods
                        .into_iter()
                        .map(|m| attach_origin(m, def, EffectTarget::Enemy)),
                );
                for (var, enabled) in result.conditions {
                    outcome.conditions.insert(var, enabled);
                }
                for (var, value) in result.scalars {
                    *outcome.multipliers.entry(var).or_insert(0.0) += value;
                }
            }
            None => outcome.unhandled.push(UnhandledEntry {
                var: def.var.clone(),
                handler_id: handler_id.clone(),
            }),
        }
        return;
    }

    // effects: for list type, prefer per-option effects, otherwise fall back to shared effects.
    let effects: &[ConfigEffect] = if def.input_type == ConfigInputType::List {
        if let ConfigInputValue::Text(selected) = &display_value {
            def.option_effects
                .get(selected)
                .map(Vec::as_slice)
                .unwrap_or(&def.effects)
        } else {
            &def.effects
        }
    } else {
        &def.effects
    };

    for effect in effects {
        apply_effect(def, effect, input_number, outcome);
    }

    // implyCond expansion: activating the entry implies these; never overrides an existing value.
    for cond in &def.imply_conditions {
        outcome.conditions.entry(cond.clone()).or_insert(true);
    }
}

/// Instantiate a single effect and write it into outcome.
fn apply_effect(
    def: &ConfigOptionDef,
    effect: &ConfigEffect,
    input: f64,
    outcome: &mut ConfigOutcome,
) {
    if let Some(pred) = &effect.emit_if
        && !value_expr::eval_predicate(pred, input)
    {
        return;
    }

    // Structured LIST payload: SkillData key/value → skill_data; nested mod → its own bucket.
    if let Some(list_value) = &effect.list_value {
        match list_value {
            ListEffectValue::KeyValue { key, value } => {
                let resolved = match value {
                    ListScalar::Bool { value } => ConfigInputValue::Bool(*value),
                    ListScalar::Text { value } => ConfigInputValue::Text(value.clone()),
                    ListScalar::Expr { expr } => {
                        ConfigInputValue::Number(value_expr::eval(expr, input))
                    }
                };
                outcome.skill_data.push(SkillDataEntry {
                    key: key.clone(),
                    value: resolved,
                });
            }
            ListEffectValue::NestedMod { nested } => {
                apply_nested_mod(def, effect, nested, input, outcome);
            }
        }
        return;
    }

    let Some(mod_type) = parse_mod_type(&effect.mod_type) else {
        outcome.diagnostics.push(format!(
            "config.{}: 未知 mod_type `{}`（效果 {} 跳过）",
            def.var, effect.mod_type, effect.name
        ));
        return;
    };

    let value = if mod_type == ModType::Flag {
        ModValue::Bool(effect.value_bool.unwrap_or(true))
    } else if let Some(text) = &effect.value_text {
        ModValue::Text(text.clone())
    } else if let Some(expr) = &effect.value {
        ModValue::Number(value_expr::eval(expr, input))
    } else {
        outcome.diagnostics.push(format!(
            "config.{}: 效果 {} 缺数值载荷，跳过",
            def.var, effect.name
        ));
        return;
    };

    let Some(modifier) = build_modifier(def, effect, mod_type, value.clone(), outcome) else {
        return;
    };

    // Condition:/Multiplier: prefix backfill (only for bare effects with no tag/flag
    // qualifiers — a qualified, conditional effect can't unconditionally land in the global table).
    if effect.tags.is_empty() && effect.flags.is_empty() {
        if mod_type == ModType::Flag
            && let Some(cond) = effect.name.strip_prefix("Condition:")
            && let ModValue::Bool(enabled) = value
        {
            outcome.conditions.insert(cond.to_string(), enabled);
        }
        if mod_type == ModType::Base
            && let Some(mult) = effect.name.strip_prefix("Multiplier:")
            && let ModValue::Number(number) = value
        {
            *outcome.multipliers.entry(mult.to_string()).or_insert(0.0) += number;
        }
    }

    match effect.target {
        EffectTarget::Player => outcome.player_mods.push(modifier),
        EffectTarget::Enemy => outcome.enemy_mods.push(modifier),
        EffectTarget::Minion => outcome.minion_mods.push(modifier),
    }
}

/// Expand a nested mod (MinionModifier/EnemyModifier LIST) into its target bucket.
fn apply_nested_mod(
    def: &ConfigOptionDef,
    effect: &ConfigEffect,
    nested: &NestedModDef,
    input: f64,
    outcome: &mut ConfigOutcome,
) {
    let Some(mod_type) = parse_mod_type(&nested.mod_type) else {
        outcome.diagnostics.push(format!(
            "config.{}: 嵌套 mod 未知 mod_type `{}`，跳过",
            def.var, nested.mod_type
        ));
        return;
    };
    let value = if mod_type == ModType::Flag {
        ModValue::Bool(nested.value_bool.unwrap_or(true))
    } else if let Some(text) = &nested.value_text {
        ModValue::Text(text.clone())
    } else if let Some(expr) = &nested.value {
        ModValue::Number(value_expr::eval(expr, input))
    } else {
        outcome.diagnostics.push(format!(
            "config.{}: 嵌套 mod {} 缺数值载荷，跳过",
            def.var, nested.name
        ));
        return;
    };

    // The nested channel routes by outer LIST name: MinionModifier → minion, EnemyModifier →
    // enemy. Resolve the target bucket first — actor tag translation (what the `enemy` literal
    // points to) depends on the bucket the mod lives in.
    let bucket = match effect.name.as_str() {
        "MinionModifier" => EffectTarget::Minion,
        "EnemyModifier" => EffectTarget::Enemy,
        "PlayerModifier" => EffectTarget::Player,
        other => {
            outcome.diagnostics.push(format!(
                "config.{}: 未知嵌套转发通道 `{other}`，跳过",
                def.var
            ));
            return;
        }
    };

    let mut modifier = Modifier::new(nested.name.as_str(), mod_type, value);
    modifier = modifier.with_source(
        nested
            .source
            .clone()
            .unwrap_or_else(|| "Config".to_string()),
    );
    let Some(modifier) = apply_flags_and_tags(
        modifier,
        &nested.flags,
        &nested.tags,
        &def.var,
        bucket,
        outcome,
    ) else {
        return;
    };
    let modifier = attach_origin(modifier, def, effect.target);

    match bucket {
        EffectTarget::Minion => outcome.minion_mods.push(modifier),
        EffectTarget::Enemy => outcome.enemy_mods.push(modifier),
        EffectTarget::Player => outcome.player_mods.push(modifier),
    }
}

/// Assemble an effect into a Modifier (mapping flags/tags; an unknown one conservatively drops
/// the whole mod).
fn build_modifier(
    def: &ConfigOptionDef,
    effect: &ConfigEffect,
    mod_type: ModType,
    value: ModValue,
    outcome: &mut ConfigOutcome,
) -> Option<Modifier> {
    let mut modifier = Modifier::new(effect.name.as_str(), mod_type, value);
    modifier = modifier.with_source(
        effect
            .source
            .clone()
            .unwrap_or_else(|| "Config".to_string()),
    );
    let modifier = apply_flags_and_tags(
        modifier,
        &effect.flags,
        &effect.tags,
        &def.var,
        effect.target,
        outcome,
    )?;
    Some(attach_origin(modifier, def, effect.target))
}

/// Map flag/tag names; an unknown one is recorded in diagnostics and drops the whole mod.
///
/// actor tag translation (backfilled later): the actor literal on vendor's
/// `ActorCondition`/`Multiplier(actor=…)` goes through [`map_vendor_actor`] (resolved against the
/// bucket the mod lives in) into the `actor` field of [`ModTag::Condition`]/[`ModTag::Multiplier`].
fn apply_flags_and_tags(
    mut modifier: Modifier,
    flags: &[String],
    tags: &[EffectTag],
    var: &str,
    bucket: EffectTarget,
    outcome: &mut ConfigOutcome,
) -> Option<Modifier> {
    let mut mod_flags = ModFlags::NONE;
    for flag in flags {
        match map_mod_flag(flag) {
            Some(bit) => mod_flags |= bit,
            None => {
                outcome.diagnostics.push(format!(
                    "config.{var}: ModFlag `{flag}` 未映射（pobr ModFlags 缺位），mod {} 跳过",
                    modifier.name
                ));
                return None;
            }
        }
    }
    modifier = modifier.with_flags(mod_flags);

    for tag in tags {
        match tag {
            EffectTag::Condition { var: cond, neg } => {
                modifier = modifier.with_tag(ModTag::condition(cond.clone(), *neg));
            }
            EffectTag::Multiplier {
                var: mult,
                div,
                limit,
                actor,
            } => {
                // Constructor + field update: the actor dimension (PoB2 ModStore.lua:347-353
                // `tag.actor`) is translated per-bucket and written into `ModTag::Multiplier::actor`.
                let mut mod_tag = ModTag::multiplier(mult.clone(), *div, *limit);
                if let Some(literal) = actor {
                    let Some(actor_ref) = map_vendor_actor(literal, bucket) else {
                        outcome.diagnostics.push(format!(
                            "config.{var}: Multiplier tag actor `{literal}`（桶 {bucket:?}）无 ActorRef 映射，mod {} 跳过",
                            modifier.name
                        ));
                        return None;
                    };
                    if let ModTag::Multiplier { actor, .. } = &mut mod_tag {
                        *actor = Some(actor_ref);
                    }
                }
                modifier = modifier.with_tag(mod_tag);
            }
            EffectTag::ActorCondition {
                actor,
                var: cond,
                neg,
            } => {
                // Cross-actor condition (PoB2 ModStore.lua:607-624 `ActorCondition`:
                // `getActor(self, tag.actor)` looks up the condition in the target actor's modDB).
                let Some(actor_ref) = map_vendor_actor(actor, bucket) else {
                    outcome.diagnostics.push(format!(
                        "config.{var}: ActorCondition actor `{actor}`（桶 {bucket:?}）无 ActorRef 映射，mod {} 跳过",
                        modifier.name
                    ));
                    return None;
                };
                let mut mod_tag = ModTag::condition(cond.clone(), *neg);
                if let ModTag::Condition { actor, .. } = &mut mod_tag {
                    *actor = Some(actor_ref);
                }
                modifier = modifier.with_tag(mod_tag);
            }
        }
    }
    Some(modifier)
}

/// Vendor actor literal → [`ActorRef`] (resolved against the bucket the mod lives in).
///
/// PoB2's actor chain (CalcSetup.lua:536-545): `env.player.enemy = env.enemy`,
/// `env.enemy.enemy = env.player` — so on an **enemy-bucket** mod, `actor = "enemy"`
/// actually points at the player (e.g. the exposure entries' player-side
/// `CanApply<X>Exposure` flag, ConfigOptions.lua:1864-1872). On player/minion buckets,
/// `"enemy"` points at the enemy actor — pobr's `ActorRef` has no Enemy variant yet (no
/// enemy-side snapshot channel exists), so this conservatively returns `None` (no catalog
/// entry currently takes this shape).
fn map_vendor_actor(literal: &str, bucket: EffectTarget) -> Option<ActorRef> {
    match literal {
        "player" => Some(ActorRef::Player),
        "parent" => Some(ActorRef::Parent),
        "minion" => Some(ActorRef::Minion),
        "enemy" if bucket == EffectTarget::Enemy => Some(ActorRef::Player),
        _ => None,
    }
}

/// Vendor ModFlag display name → pobr ModFlags bit (currently a closed set; extend the table
/// when more bits are added).
fn map_mod_flag(name: &str) -> Option<ModFlags> {
    match name {
        "Attack" => Some(ModFlags::ATTACK),
        "Spell" => Some(ModFlags::SPELL),
        "Melee" => Some(ModFlags::MELEE),
        "Projectile" => Some(ModFlags::PROJECTILE),
        "Area" => Some(ModFlags::AREA),
        _ => None,
    }
}

fn parse_mod_type(literal: &str) -> Option<ModType> {
    match literal {
        "BASE" => Some(ModType::Base),
        "INC" => Some(ModType::Inc),
        "MORE" => Some(ModType::More),
        "FLAG" => Some(ModType::Flag),
        "OVERRIDE" => Some(ModType::Override),
        "LIST" => Some(ModType::List),
        _ => None,
    }
}

/// Attribution: SourceKind::Config / EnemyConfig + `config.<var>`.
fn attach_origin(modifier: Modifier, def: &ConfigOptionDef, target: EffectTarget) -> Modifier {
    let kind = match target {
        EffectTarget::Enemy => SourceKind::EnemyConfig,
        EffectTarget::Player | EffectTarget::Minion => SourceKind::Config,
    };
    let source_id = SourceId::new(kind, format!("config.{}", def.var));
    modifier.with_origin(ModifierSource::new(source_id))
}

/// Vendor `StripEscapes`: strips `^x RRGGBB` colour codes and `^<digit>` shorthand codes.
fn strip_escapes(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '^' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            Some('x') | Some('X') => {
                chars.next();
                for _ in 0..6 {
                    if chars.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                        chars.next();
                    }
                }
            }
            Some(d) if d.is_ascii_digit() => {
                chars.next();
            }
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use pobr_data::catalog::config_def::{ConfigDefault, ConfigVisibility, ListOption, ListScalar};
    use pobr_data::catalog::value_expr::{Predicate, ValueExpr};

    use super::*;

    fn entry(var: &str, input_type: ConfigInputType) -> ConfigOptionDef {
        ConfigOptionDef {
            var: var.to_string(),
            input_type,
            section: "General".to_string(),
            label: None,
            default: None,
            list_options: Vec::new(),
            visibility: ConfigVisibility::default(),
            imply_conditions: Vec::new(),
            effects: Vec::new(),
            option_effects: BTreeMap::new(),
            handler_id: None,
            handler_reason: None,
            verified: true,
        }
    }

    fn flag_effect(name: &str) -> ConfigEffect {
        ConfigEffect {
            target: EffectTarget::Player,
            name: name.to_string(),
            mod_type: "FLAG".to_string(),
            value: None,
            value_bool: Some(true),
            value_text: None,
            list_value: None,
            emit_if: None,
            tags: Vec::new(),
            flags: Vec::new(),
            keyword_flags: Vec::new(),
            source: None,
        }
    }

    fn base_effect(name: &str, value: ValueExpr) -> ConfigEffect {
        ConfigEffect {
            value: Some(value),
            mod_type: "BASE".to_string(),
            value_bool: None,
            ..flag_effect(name)
        }
    }

    /// check entry: only emits the effect and backfills conditions when explicitly true.
    #[test]
    fn check_entry_emits_flag_and_condition() {
        let mut def = entry("conditionMoving", ConfigInputType::Check);
        def.effects = vec![flag_effect("Condition:Moving")];

        let inputs = RawConfigInputs::new().with("conditionMoving", ConfigInputValue::Bool(true));
        let outcome = interpret(std::slice::from_ref(&def), &inputs, &HandlerRegistry::new());
        assert_eq!(outcome.player_mods.len(), 1);
        assert_eq!(outcome.player_mods[0].name.as_str(), "Condition:Moving");
        assert_eq!(outcome.conditions.get("Moving"), Some(&true));
        // Attribution carries SourceKind::Config + config.<var>.
        let origin = outcome.player_mods[0].origin.as_ref().unwrap();
        assert_eq!(origin.source_id.kind, SourceKind::Config);
        assert_eq!(origin.source_id.id, "config.conditionMoving");

        // Unset → no output.
        let outcome = interpret(&[def], &RawConfigInputs::new(), &HandlerRegistry::new());
        assert!(outcome.player_mods.is_empty());
        assert!(outcome.conditions.is_empty());
    }

    /// check's defaultState=true: activates even without input (DEFAULT_TRUE_CONDITIONS made data-driven).
    #[test]
    fn check_default_state_true_activates() {
        let mut def = entry("conditionHitRecently", ConfigInputType::Check);
        def.default = Some(ConfigDefault {
            state_bool: Some(true),
            state_number: None,
            index: None,
            placeholder_number: None,
        });
        def.effects = vec![flag_effect("Condition:HitRecently")];
        let outcome = interpret(&[def], &RawConfigInputs::new(), &HandlerRegistry::new());
        assert_eq!(outcome.conditions.get("HitRecently"), Some(&true));
    }

    /// conditionStationary end-to-end: number=5 → Multiplier + FLAG; number=0 → the whole entry
    /// is skipped per count semantics (vendor BuildModList: 0 counts as unset for count).
    #[test]
    fn stationary_count_entry() {
        let mut def = entry("conditionStationary", ConfigInputType::Count);
        def.effects = vec![
            ConfigEffect {
                emit_if: None,
                ..base_effect(
                    "Multiplier:StationarySeconds",
                    ValueExpr::Clamp {
                        min: Some(0.0),
                        max: None,
                        inner: Box::new(ValueExpr::input()),
                    },
                )
            },
            ConfigEffect {
                emit_if: Some(Predicate::input_gt(0.0)),
                ..flag_effect("Condition:Stationary")
            },
        ];

        let inputs =
            RawConfigInputs::new().with("conditionStationary", ConfigInputValue::Number(5.0));
        let outcome = interpret(std::slice::from_ref(&def), &inputs, &HandlerRegistry::new());
        assert_eq!(outcome.multipliers.get("StationarySeconds"), Some(&5.0));
        assert_eq!(outcome.conditions.get("Stationary"), Some(&true));
        assert_eq!(outcome.player_mods.len(), 2);

        let inputs =
            RawConfigInputs::new().with("conditionStationary", ConfigInputValue::Number(0.0));
        let outcome = interpret(&[def], &inputs, &HandlerRegistry::new());
        assert!(outcome.player_mods.is_empty(), "count=0 视为未设置");
    }

    /// countAllowZero: applies at 0 too.
    #[test]
    fn count_allow_zero_applies_at_zero() {
        let mut def = entry("multiplierX", ConfigInputType::CountAllowZero);
        def.effects = vec![base_effect("Multiplier:X", ValueExpr::input())];
        let inputs = RawConfigInputs::new().with("multiplierX", ConfigInputValue::Number(0.0));
        let outcome = interpret(&[def], &inputs, &HandlerRegistry::new());
        assert_eq!(outcome.multipliers.get("X"), Some(&0.0));
        assert_eq!(outcome.player_mods.len(), 1);
    }

    /// Numeric default (the shape of multiplierCurrentManaPercentage with defaultState=100).
    #[test]
    fn count_default_number_used_when_missing() {
        let mut def = entry("multiplierCurrentManaPercentage", ConfigInputType::Count);
        def.default = Some(ConfigDefault {
            state_bool: None,
            state_number: Some(100.0),
            index: None,
            placeholder_number: None,
        });
        def.effects = vec![base_effect(
            "Multiplier:CurrentManaPercentage",
            ValueExpr::Clamp {
                min: Some(0.0),
                max: Some(100.0),
                inner: Box::new(ValueExpr::input()),
            },
        )];
        let outcome = interpret(&[def], &RawConfigInputs::new(), &HandlerRegistry::new());
        assert_eq!(
            outcome.multipliers.get("CurrentManaPercentage"),
            Some(&100.0)
        );
    }

    /// list type: option routing via option_effects + defaultIndex fallback + numeric echo.
    #[test]
    fn list_entry_with_option_effects_and_default_index() {
        let mut def = entry("lifeRegenMode", ConfigInputType::List);
        def.list_options = vec![
            ListOption {
                value: "MIN".to_string(),
                number: None,
                label: "Minimum".to_string(),
            },
            ListOption {
                value: "AVERAGE".to_string(),
                number: None,
                label: "Average".to_string(),
            },
        ];
        def.default = Some(ConfigDefault {
            state_bool: None,
            state_number: None,
            index: Some(2),
            placeholder_number: None,
        });
        def.option_effects.insert(
            "AVERAGE".to_string(),
            vec![flag_effect("Condition:LifeRegenBurstAvg")],
        );
        def.option_effects.insert("MIN".to_string(), Vec::new());

        // Explicitly selects AVERAGE.
        let inputs =
            RawConfigInputs::new().with("lifeRegenMode", ConfigInputValue::Text("AVERAGE".into()));
        let outcome = interpret(std::slice::from_ref(&def), &inputs, &HandlerRegistry::new());
        assert_eq!(outcome.conditions.get("LifeRegenBurstAvg"), Some(&true));

        // No input → defaultIndex=2 → AVERAGE.
        let outcome = interpret(
            std::slice::from_ref(&def),
            &RawConfigInputs::new(),
            &HandlerRegistry::new(),
        );
        assert_eq!(outcome.conditions.get("LifeRegenBurstAvg"), Some(&true));

        // Explicitly selects MIN → no effects but still echoed as a scalar.
        let inputs =
            RawConfigInputs::new().with("lifeRegenMode", ConfigInputValue::Text("MIN".into()));
        let outcome = interpret(&[def], &inputs, &HandlerRegistry::new());
        assert!(outcome.conditions.is_empty());
        assert_eq!(
            outcome.scalars.get("lifeRegenMode"),
            Some(&ConfigInputValue::Text("MIN".into()))
        );
    }

    /// enemy target routing + EnemyConfig attribution + Condition:Effective tag mapping.
    #[test]
    fn enemy_effect_routes_to_enemy_bucket() {
        let mut def = entry("conditionEnemyShocked", ConfigInputType::Check);
        def.effects = vec![ConfigEffect {
            target: EffectTarget::Enemy,
            tags: vec![EffectTag::Condition {
                var: "Effective".to_string(),
                neg: false,
            }],
            ..flag_effect("Condition:Shocked")
        }];
        let inputs =
            RawConfigInputs::new().with("conditionEnemyShocked", ConfigInputValue::Bool(true));
        let outcome = interpret(&[def], &inputs, &HandlerRegistry::new());
        assert_eq!(outcome.enemy_mods.len(), 1);
        let modifier = &outcome.enemy_mods[0];
        assert_eq!(
            modifier.origin.as_ref().unwrap().source_id.kind,
            SourceKind::EnemyConfig
        );
        assert_eq!(modifier.tags, vec![ModTag::condition("Effective", false)]);
        // A tagged, conditional FLAG does not backfill the global conditions table.
        assert!(outcome.conditions.is_empty());
    }

    /// implyCond: activation implies the condition is set, but never overrides an existing explicit value.
    #[test]
    fn imply_conditions_do_not_override_explicit() {
        let mut def_a = entry("a", ConfigInputType::Check);
        def_a.effects = vec![flag_effect("Condition:UsedSkillRecently")];
        // The explicit effect first writes false into conditions — set up the contrast case by
        // changing the explicit value to false.
        def_a.effects[0].value_bool = Some(false);
        let mut def_b = entry("b", ConfigInputType::Check);
        def_b.imply_conditions = vec!["UsedSkillRecently".to_string()];

        let inputs = RawConfigInputs::new()
            .with("a", ConfigInputValue::Bool(true))
            .with("b", ConfigInputValue::Bool(true));
        let outcome = interpret(&[def_a, def_b], &inputs, &HandlerRegistry::new());
        assert_eq!(
            outcome.conditions.get("UsedSkillRecently"),
            Some(&false),
            "imply 不覆盖显式 false"
        );
    }

    /// handler entry: unregistered → unhandled report; registered → its output is injected.
    #[test]
    fn handler_entry_registered_and_unregistered() {
        let mut def = entry("enemyIsBoss", ConfigInputType::List);
        def.handler_id = Some("config:enemy_is_boss".to_string());
        def.list_options = vec![ListOption {
            value: "Boss".to_string(),
            number: None,
            label: "Boss".to_string(),
        }];

        let inputs =
            RawConfigInputs::new().with("enemyIsBoss", ConfigInputValue::Text("Boss".into()));
        let outcome = interpret(std::slice::from_ref(&def), &inputs, &HandlerRegistry::new());
        assert_eq!(
            outcome.unhandled,
            vec![UnhandledEntry {
                var: "enemyIsBoss".to_string(),
                handler_id: "config:enemy_is_boss".to_string()
            }]
        );

        let mut registry = HandlerRegistry::new();
        registry
            .register(
                "config:enemy_is_boss",
                Box::new(|_| crate::rules::registry::HandlerOutcome {
                    player_mods: vec![Modifier::flag("Condition:Boss")],
                    enemy_mods: vec![Modifier::flag("Condition:RareOrUnique")],
                    conditions: vec![("Boss".to_string(), true)],
                    scalars: vec![("BossPresence".to_string(), 1.0)],
                }),
            )
            .unwrap();
        let outcome = interpret(&[def], &inputs, &registry);
        assert!(outcome.unhandled.is_empty());
        // The four output lanes each land in their bucket; attribution is attached uniformly by
        // the call site (player=Config / enemy=EnemyConfig).
        assert_eq!(outcome.player_mods.len(), 1);
        assert_eq!(
            outcome.player_mods[0]
                .origin
                .as_ref()
                .unwrap()
                .source_id
                .kind,
            SourceKind::Config
        );
        assert_eq!(outcome.enemy_mods.len(), 1);
        assert_eq!(
            outcome.enemy_mods[0]
                .origin
                .as_ref()
                .unwrap()
                .source_id
                .kind,
            SourceKind::EnemyConfig
        );
        assert_eq!(outcome.conditions.get("Boss"), Some(&true));
        assert_eq!(outcome.multipliers.get("BossPresence"), Some(&1.0));
    }

    /// customMods: each line goes through StripEscapes into the text lane.
    #[test]
    fn custom_mods_text_lines_stripped() {
        let def = entry("customMods", ConfigInputType::Text);
        let inputs = RawConfigInputs::new().with(
            "customMods",
            ConfigInputValue::Text("^x7070FF20% increased Fire Damage\n\n+10 to Spirit^7".into()),
        );
        let outcome = interpret(&[def], &inputs, &HandlerRegistry::new());
        assert_eq!(
            outcome.custom_mod_lines,
            vec![
                "20% increased Fire Damage".to_string(),
                "+10 to Spirit".to_string()
            ]
        );
    }

    /// SkillData LIST key/value payload (the shape of detonateDeadCorpseLife).
    #[test]
    fn skill_data_key_value_payload() {
        let mut def = entry("detonateDeadCorpseLife", ConfigInputType::Count);
        def.effects = vec![ConfigEffect {
            mod_type: "LIST".to_string(),
            value_bool: None,
            list_value: Some(ListEffectValue::KeyValue {
                key: "corpseLife".to_string(),
                value: ListScalar::Expr {
                    expr: ValueExpr::input(),
                },
            }),
            ..flag_effect("SkillData")
        }];
        let inputs =
            RawConfigInputs::new().with("detonateDeadCorpseLife", ConfigInputValue::Number(5000.0));
        let outcome = interpret(&[def], &inputs, &HandlerRegistry::new());
        assert_eq!(outcome.skill_data.len(), 1);
        assert_eq!(outcome.skill_data[0].key, "corpseLife");
        assert_eq!(
            outcome.skill_data[0].value,
            ConfigInputValue::Number(5000.0)
        );
    }

    /// MinionModifier nested mod → minion bucket.
    #[test]
    fn minion_modifier_nested_routes_to_minion_bucket() {
        let mut def = entry("minionsConditionFullLife", ConfigInputType::Check);
        def.effects = vec![ConfigEffect {
            mod_type: "LIST".to_string(),
            value_bool: None,
            list_value: Some(ListEffectValue::NestedMod {
                nested: NestedModDef {
                    name: "Condition:FullLife".to_string(),
                    mod_type: "FLAG".to_string(),
                    value: None,
                    value_bool: Some(true),
                    value_text: None,
                    flags: Vec::new(),
                    keyword_flags: Vec::new(),
                    tags: Vec::new(),
                    source: Some("Config".to_string()),
                },
            }),
            ..flag_effect("MinionModifier")
        }];
        let inputs =
            RawConfigInputs::new().with("minionsConditionFullLife", ConfigInputValue::Bool(true));
        let outcome = interpret(&[def], &inputs, &HandlerRegistry::new());
        assert_eq!(outcome.minion_mods.len(), 1);
        assert_eq!(outcome.minion_mods[0].name.as_str(), "Condition:FullLife");
    }

    /// actor tag translation: on the enemy bucket, `ActorCondition{actor:"enemy"}` points at the
    /// player (PoB2 CalcSetup.lua:542 `env.enemy.enemy = env.player`) →
    /// `ModTag::Condition{actor:Player}`. This is the shape of the exposure entries
    /// (ConfigOptions.lua:1864-1866).
    #[test]
    fn actor_condition_translated_for_enemy_bucket() {
        let mut def = entry("conditionEnemyFireExposure", ConfigInputType::Check);
        def.effects = vec![ConfigEffect {
            target: EffectTarget::Enemy,
            mod_type: "BASE".to_string(),
            value: Some(ValueExpr::literal(20.0)),
            value_bool: None,
            tags: vec![
                EffectTag::Condition {
                    var: "Effective".to_string(),
                    neg: false,
                },
                EffectTag::ActorCondition {
                    actor: "enemy".to_string(),
                    var: "CanApplyFireExposure".to_string(),
                    neg: false,
                },
            ],
            ..flag_effect("FireExposure")
        }];
        let inputs =
            RawConfigInputs::new().with("conditionEnemyFireExposure", ConfigInputValue::Bool(true));
        let outcome = interpret(&[def], &inputs, &HandlerRegistry::new());
        assert!(outcome.diagnostics.is_empty(), "{:?}", outcome.diagnostics);
        assert_eq!(outcome.enemy_mods.len(), 1);
        let modifier = &outcome.enemy_mods[0];
        assert_eq!(modifier.name.as_str(), "FireExposure");
        assert!(modifier.tags.contains(&ModTag::Condition {
            var: "CanApplyFireExposure".to_string(),
            negated: false,
            actor: Some(ActorRef::Player),
        }));
    }

    /// On the player bucket, `actor = "enemy"`: ActorRef has no Enemy variant → conservatively
    /// skip + diagnostics (no catalog entry currently takes this shape; this is defensive coverage).
    #[test]
    fn actor_condition_unmappable_skips_with_diagnostic() {
        let mut def = entry("x", ConfigInputType::Check);
        def.effects = vec![ConfigEffect {
            tags: vec![EffectTag::ActorCondition {
                actor: "enemy".to_string(),
                var: "Y".to_string(),
                neg: false,
            }],
            ..flag_effect("Condition:Z")
        }];
        let inputs = RawConfigInputs::new().with("x", ConfigInputValue::Bool(true));
        let outcome = interpret(&[def], &inputs, &HandlerRegistry::new());
        assert!(outcome.player_mods.is_empty());
        assert_eq!(outcome.diagnostics.len(), 1);
        assert!(outcome.diagnostics[0].contains("无 ActorRef 映射"));
    }

    /// The Multiplier tag's actor dimension (PoB2 ModStore.lua:347-353) is translated into
    /// `ModTag::Multiplier::actor` (constructor + field update).
    #[test]
    fn multiplier_actor_translated() {
        let mut def = entry("m", ConfigInputType::Check);
        def.effects = vec![ConfigEffect {
            tags: vec![EffectTag::Multiplier {
                var: "Virulence".to_string(),
                div: 1.0,
                limit: None,
                actor: Some("parent".to_string()),
            }],
            ..base_effect("X", ValueExpr::literal(1.0))
        }];
        let inputs = RawConfigInputs::new().with("m", ConfigInputValue::Bool(true));
        let outcome = interpret(&[def], &inputs, &HandlerRegistry::new());
        assert_eq!(outcome.player_mods.len(), 1);
        assert!(outcome.player_mods[0].tags.iter().any(|t| matches!(
            t,
            ModTag::Multiplier {
                actor: Some(ActorRef::Parent),
                ..
            }
        )));
    }

    /// Unmapped ModFlag → conservatively skip + diagnostics.
    #[test]
    fn unmapped_flag_skips_mod_with_diagnostic() {
        let mut def = entry("x", ConfigInputType::Check);
        def.effects = vec![ConfigEffect {
            flags: vec!["Cast".to_string()],
            ..flag_effect("Condition:Y")
        }];
        let inputs = RawConfigInputs::new().with("x", ConfigInputValue::Bool(true));
        let outcome = interpret(&[def], &inputs, &HandlerRegistry::new());
        assert!(outcome.player_mods.is_empty());
        assert_eq!(outcome.diagnostics.len(), 1);
        assert!(outcome.diagnostics[0].contains("Cast"));
    }

    /// strip_escapes shape coverage.
    #[test]
    fn strip_escapes_variants() {
        assert_eq!(strip_escapes("^xE05030Life^7 left"), "Life left");
        assert_eq!(strip_escapes("plain"), "plain");
        assert_eq!(strip_escapes("^1red"), "red");
    }
}
