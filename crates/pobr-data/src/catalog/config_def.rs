//! Config options catalog domain schema (`overlay/config_options.json`).
//!
//! Data source: vendor PoB2 `src/Modules/ConfigOptions.lua` (542 static
//! entries + dynamic questRewards entries), reduced by
//! `sync-pob-catalog extract-lua --what config-options` from apply closures
//! into declarative `effects[]` using **call interception plus multi-probe
//! fitting**. Entries with real logic that can't be templated only carry a
//! `handler_id` (target ≤54, architecture doc §5).
//!
//! Expression / predicate / tag types are shared with [`super::value_expr`]
//! (the single restricted DSL, per decision §4-1); evaluation always goes
//! through `pobr-core::rules::value_expr`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::value_expr::{EffectTag, EffectTarget, Predicate, ValueExpr};

/// Current overlay document schema identifier (bumped when the field shape evolves).
pub const CONFIG_OPTIONS_SCHEMA: &str = "config_options/v1";

/// Top level of `overlay/config_options.json` (the consumer ignores `_meta`
/// by default via serde).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigOptionsDef {
    /// All entries, ascending by `var` (a byte-stable sort key).
    pub options: Vec<ConfigOptionDef>,
}

/// A single config entry (mirrors the ConfigOptions schema plus a
/// declarative effects DSL).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigOptionDef {
    /// Stable variable name (vendor's `var`; the key that
    /// `<Input name=...>` in the build XML corresponds to).
    pub var: String,
    /// Input type.
    pub input_type: ConfigInputType,
    /// The section it belongs to (General / Skill Options / Combat / …).
    pub section: String,
    /// UI label (absent for entries whose label is a function).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Default-value family (defaultState / defaultIndex /
    /// defaultPlaceholderState).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<ConfigDefault>,
    /// list-type options (`{val, label}`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub list_options: Vec<ListOption>,
    /// Visibility condition (stored but not consumed — for UI use).
    #[serde(default, skip_serializing_if = "ConfigVisibility::is_empty")]
    pub visibility: ConfigVisibility,
    /// Expanded implyCond + implyCondList (condition names implied to be
    /// set when this entry's value is true).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub imply_conditions: Vec<String>,
    /// Declarative effects (shared across all options; left empty and
    /// [`Self::option_effects`] used instead when the structure varies with
    /// a list-type option's value).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<ConfigEffect>,
    /// Per-option effects for list-type entries (key = the stringified
    /// option `val`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub option_effects: BTreeMap<String, Vec<ConfigEffect>>,
    /// Stable handler ID for entries with real logic (mutually exclusive
    /// with effects; looked up in `pobr-core::rules::HandlerRegistry`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handler_id: Option<String>,
    /// Reason this entry was downgraded to a handler (written by the
    /// extractor, for reporting).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handler_reason: Option<String>,
    /// Whether multi-probe reconciliation passed at extraction time (the
    /// correctness verdict; entries marked false are still used as-is at
    /// runtime, but listed separately in the parity report).
    #[serde(default)]
    pub verified: bool,
}

/// Input type (a stable enum of vendor's `type` literal).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigInputType {
    /// A checkbox (applies only when its value is exactly true).
    Check,
    /// A count (0 counts as unset and doesn't apply).
    Count,
    /// A count that also applies when 0.
    CountAllowZero,
    /// An integer.
    Integer,
    /// A float.
    Float,
    /// A dropdown list.
    List,
    /// Text (customMods only).
    Text,
}

/// The default-value family (multiple shapes coexist; which one is used
/// depends on the input type).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigDefault {
    /// The check type's `defaultState` (boolean).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_bool: Option<bool>,
    /// The count/integer/float types' `defaultState` (numeric).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_number: Option<f64>,
    /// The list type's `defaultIndex` (**1-based**, kept faithful to vendor).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<u32>,
    /// `defaultPlaceholderState` (a numeric placeholder default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder_number: Option<f64>,
}

/// A single option of a list-type entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListOption {
    /// The option value, stringified (the key used in `option_effects`;
    /// numeric options also give [`Self::number`]).
    pub value: String,
    /// The raw numeric value for a numeric option (e.g.
    /// resistancePenalty's `0/-10/…`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<f64>,
    /// UI label (includes vendor color codes, transcribed faithfully).
    pub label: String,
}

/// The visibility-condition family (vendor's `if*` fields; a single value
/// is normalized into a one-element array).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConfigVisibility {
    /// `ifCond`: visible when the player condition is used by the calc.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub if_cond: Vec<String>,
    /// `ifMinionCond`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub if_minion_cond: Vec<String>,
    /// `ifEnemyCond`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub if_enemy_cond: Vec<String>,
    /// `ifFlag`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub if_flag: Vec<String>,
    /// `ifMult`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub if_mult: Vec<String>,
    /// `ifEnemyMult`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub if_enemy_mult: Vec<String>,
    /// `ifSkill` / `ifSkillList`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub if_skill: Vec<String>,
    /// `ifSkillData`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub if_skill_data: Vec<String>,
    /// `ifMod`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub if_mod: Vec<String>,
    /// `ifTagType`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub if_tag_type: Vec<String>,
}

impl ConfigVisibility {
    /// Whether every field is empty (used by the serialization skip predicate).
    pub fn is_empty(&self) -> bool {
        self.if_cond.is_empty()
            && self.if_minion_cond.is_empty()
            && self.if_enemy_cond.is_empty()
            && self.if_flag.is_empty()
            && self.if_mult.is_empty()
            && self.if_enemy_mult.is_empty()
            && self.if_skill.is_empty()
            && self.if_skill_data.is_empty()
            && self.if_mod.is_empty()
            && self.if_tag_type.is_empty()
    }
}

/// A single declarative effect (corresponds to one `modList:NewMod` /
/// `AddMod` call).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigEffect {
    /// The actor written to.
    pub target: EffectTarget,
    /// ModName (e.g. `Condition:Moving` / `Multiplier:StationarySeconds`).
    pub name: String,
    /// Mod type, vendor's raw literal (`FLAG/BASE/INC/MORE/OVERRIDE/LIST`).
    pub mod_type: String,
    /// The numeric payload (a restricted DSL expression); absent for
    /// FLAG / text / LIST payloads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<ValueExpr>,
    /// The boolean payload (FLAG; treated as true when absent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_bool: Option<bool>,
    /// The text payload (a few OVERRIDE/LIST text values).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_text: Option<String>,
    /// The LIST structured payload (SkillData key/value or a nested mod).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list_value: Option<ListEffectValue>,
    /// Restricted predicate: only emit this effect when it holds (e.g.
    /// conditionStationary's `input > 0`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emit_if: Option<Predicate>,
    /// Restricted tag whitelist.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<EffectTag>,
    /// ModFlag names (vendor's `formatFlags` display names, e.g.
    /// `Attack`/`Cast`; the consumer maps them to the bit enum, unknown
    /// names get logged as diagnostics).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<String>,
    /// KeywordFlag names (same as above).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keyword_flags: Vec<String>,
    /// The mod source string (defaults to `Config`; quest entries use
    /// `Quest:…`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// The structured payload for a LIST-type mod (a restricted closed set;
/// shapes outside the whitelist fall back to a handler).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ListEffectValue {
    /// A `{ key = K, value = V }` key/value payload (SkillData, etc.).
    KeyValue {
        /// The payload key (e.g. `corpseLife`).
        key: String,
        /// The payload value.
        value: ListScalar,
    },
    /// `{ mod = <nested mod> }` (the MinionModifier / EnemyModifier
    /// forwarding channel).
    NestedMod {
        /// The nested mod definition.
        #[serde(rename = "mod")]
        nested: NestedModDef,
    },
}

/// The value shape for a key/value payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ListScalar {
    /// A boolean literal.
    Bool {
        /// The literal value.
        value: bool,
    },
    /// A text literal.
    Text {
        /// The literal value.
        value: String,
    },
    /// A numeric expression (includes `FromInput` = the identity `Input`).
    Expr {
        /// The restricted expression.
        expr: ValueExpr,
    },
}

/// A nested mod definition (the inner mod of the LIST forwarding channel).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NestedModDef {
    /// Inner ModName.
    pub name: String,
    /// Inner mod type, vendor's raw literal.
    pub mod_type: String,
    /// The numeric payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<ValueExpr>,
    /// The boolean payload (FLAG; default true).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_bool: Option<bool>,
    /// The text payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_text: Option<String>,
    /// ModFlag names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<String>,
    /// KeywordFlag names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keyword_flags: Vec<String>,
    /// Restricted tags.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<EffectTag>,
    /// Inner mod source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::value_expr::Predicate;

    /// A typical count entry (the conditionStationary shape) round-trips
    /// through serde.
    #[test]
    fn count_entry_round_trip() {
        let def = ConfigOptionDef {
            var: "conditionStationary".to_string(),
            input_type: ConfigInputType::Count,
            section: "General".to_string(),
            label: Some("Time spent stationary".to_string()),
            default: None,
            list_options: Vec::new(),
            visibility: ConfigVisibility {
                if_cond: vec!["Stationary".to_string()],
                ..Default::default()
            },
            imply_conditions: Vec::new(),
            effects: vec![
                ConfigEffect {
                    target: EffectTarget::Player,
                    name: "Multiplier:StationarySeconds".to_string(),
                    mod_type: "BASE".to_string(),
                    value: Some(ValueExpr::Clamp {
                        min: Some(0.0),
                        max: None,
                        inner: Box::new(ValueExpr::input()),
                    }),
                    value_bool: None,
                    value_text: None,
                    list_value: None,
                    emit_if: None,
                    tags: Vec::new(),
                    flags: Vec::new(),
                    keyword_flags: Vec::new(),
                    source: None,
                },
                ConfigEffect {
                    target: EffectTarget::Player,
                    name: "Condition:Stationary".to_string(),
                    mod_type: "FLAG".to_string(),
                    value: None,
                    value_bool: Some(true),
                    value_text: None,
                    list_value: None,
                    emit_if: Some(Predicate::input_gt(0.0)),
                    tags: Vec::new(),
                    flags: Vec::new(),
                    keyword_flags: Vec::new(),
                    source: None,
                },
            ],
            option_effects: BTreeMap::new(),
            handler_id: None,
            handler_reason: None,
            verified: true,
        };
        let json = serde_json::to_string_pretty(&def).unwrap();
        let back: ConfigOptionDef = serde_json::from_str(&json).unwrap();
        assert_eq!(back, def);
    }

    /// Minimal handler-entry shape: unknown fields (e.g. `_meta`) are
    /// ignored and missing fields backfill their defaults.
    #[test]
    fn handler_entry_minimal_and_unknown_fields_ignored() {
        let json = r#"{
            "var": "customMods",
            "input_type": "text",
            "section": "Custom mods",
            "handler_id": "config:custom_mods",
            "future_field": 1
        }"#;
        let def: ConfigOptionDef = serde_json::from_str(json).unwrap();
        assert_eq!(def.handler_id.as_deref(), Some("config:custom_mods"));
        assert!(def.effects.is_empty());
        assert!(!def.verified);
        assert!(def.visibility.is_empty());
    }

    /// Round-trip for a list-type entry's per-option effects plus a nested
    /// mod payload.
    #[test]
    fn list_entry_with_nested_mod_round_trip() {
        let mut option_effects = BTreeMap::new();
        option_effects.insert(
            "AVERAGE".to_string(),
            vec![ConfigEffect {
                target: EffectTarget::Minion,
                name: "MinionModifier".to_string(),
                mod_type: "LIST".to_string(),
                value: None,
                value_bool: None,
                value_text: None,
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
                emit_if: None,
                tags: Vec::new(),
                flags: Vec::new(),
                keyword_flags: Vec::new(),
                source: None,
            }],
        );
        let def = ConfigOptionDef {
            var: "x".to_string(),
            input_type: ConfigInputType::List,
            section: "Combat".to_string(),
            label: None,
            default: Some(ConfigDefault {
                state_bool: None,
                state_number: None,
                index: Some(2),
                placeholder_number: None,
            }),
            list_options: vec![ListOption {
                value: "AVERAGE".to_string(),
                number: None,
                label: "Average".to_string(),
            }],
            visibility: ConfigVisibility::default(),
            imply_conditions: vec!["UsedSkillRecently".to_string()],
            effects: Vec::new(),
            option_effects,
            handler_id: None,
            handler_reason: None,
            verified: false,
        };
        let json = serde_json::to_string(&def).unwrap();
        let back: ConfigOptionDef = serde_json::from_str(&json).unwrap();
        assert_eq!(back, def);
    }
}
