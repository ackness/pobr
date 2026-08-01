//! ModParser rule data schema — this module holds two complementary schemas:
//!
//! 1. **special_mods templates** (`overlay/special_mods.json` +
//!    `generated/special_derived.json`, hand-curated) — the [`SpecialModsDef`]
//!    family;
//! 2. **the ModParser rule six-table set** (`overlay/mod_parser_rules.json`,
//!    `mod_parser_rules/v1`, vendor-extracted) — the [`ModParserRulesDoc`]
//!    family.
//!
//! This module has zero logic and zero I/O; every new field is
//! `#[serde(default)]` / `Option`.
//!
//! # special_mods templates
//!
//! A batched data carrier for vendor PoB2's `Modules/ModParser.lua:2231-6150`
//! `specialModList` (2085 whole-line-anchored special cases). This table is a
//! **hand-curated domain** (`_meta.generator = "hand-curated"`; `regen_command`
//! records the reconciliation command, not a regeneration command); vendor
//! reconciliation is handled by the `vendor_pattern` column plus
//! `sync-pob-catalog check --special-coverage`.
//!
//! ## Hard limits of the restricted template DSL
//!
//! - Allowed: `$1..$n` numeric placeholders, literals, the five operators
//!   `negate / clamp(min,max) / div / mult / base`, `target(player|enemy|minion)`,
//!   restricted predicates (field references + eq/ne/gt/lt + and/or).
//! - Forbidden: loops, recursion, free-form expressions, cross-entry
//!   references, runtime string concatenation.
//! - Extension gate: adding any new DSL capability requires ≥20 entries to
//!   benefit from it, otherwise the entry goes through `handler_id`.
//! - Monitoring: fewer than 100 handler entries; approaching 10% of the total
//!   special count counts as a failed split.
//! - Metadata: entries not yet verified against the oracle carry
//!   `verified:false` — used as-is at runtime, but listed separately in the
//!   parity report.
//!
//! The only evaluator implementation is `pobr-core::rules::value_expr` (one
//! restricted language shared by config / special / parser — no three
//! dialects). This module only defines the serde shape and doesn't import
//! evaluator code. The restricted predicate grammar (value shapes reserved in
//! [`TemplateTagDef`]'s open fields):
//!
//! ```text
//! predicate := comparison | predicate ("and" | "or") predicate
//! comparison := field_ref ("eq" | "ne" | "gt" | "lt") literal
//! field_ref  := a field name from the evaluation context's whitelist (no
//!               free-form expressions, no function calls)
//! ```
//!
//! # ModParser rule six-table set
//!
//! Data source: vendor PoB2 `Modules/ModParser.lua`'s parser rule tables
//! (formList / modNameList / modFlagList / preFlagList / modTagList plus a
//! few small lookup tables), deterministically extracted by
//! `sync-pob-catalog extract-lua --what parser-rules`, which bootstraps
//! headless luajit and dumps the **final loaded tables** (including derived
//! tables like regen/cost, already expanded). specialModList isn't in this
//! table (it lives in `overlay/special_mods.json`); skillNameList /
//! preSkillNameList live in `generated/special_derived.json`.
//!
//! Key extraction conventions (consumer side = the mod_parser scan engine):
//! - **`pattern` keeps raw Lua pattern syntax as-is**, not translated to
//!   regex at extraction time; [`FormDef::literal`] / `anchored` are
//!   Rust-side derived indexing helper fields;
//! - **bitmask → names**: vendor `ModFlag` / `KeywordFlag` masks are
//!   decomposed into name arrays (the bit enums stay in Rust; `from_names`
//!   restores them at load time); a tag's numeric `skillType` enum is
//!   reverse-looked-up into a name (stored under the `skill_type` key), and
//!   `modFlags`/`keywordFlags` masks are decomposed (stored under the
//!   `mod_flags`/`keyword_flags` keys); other tag fields **keep their key
//!   names verbatim** (vendor camelCase, e.g. `limitTotal` / `varList`);
//! - **closures → templates (inferred by probing)**: closure entries are
//!   inferred into placeholder templates (`$1..$5`, operators `:cap`
//!   (capitalize first letter), `:div(k)`/`:mult(k)`/`:negate`, string
//!   concatenation joined with `+`) via a dual-sentinel probe; successful
//!   inferences are marked [`PreFlagDef::inferred`]; entries where inference
//!   fails fall back to `handler_id` (`<section>:<first 12 chars of the
//!   pattern's stable hash>`), backed by a Rust handler registry (global
//!   <100 gate).
//!
//! Recorded deviation: `resource_types` is not stored — after vendor finishes
//! loading, parseMod only ever consumes its derived expansions (the four
//! regen/degen/cost/base_cost tables); the raw table is unreachable and has
//! no runtime consumer.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub use crate::catalog::stat_map::StatMapValue;

//  special mod-line template schema (overlay/special_mods.json)

/// Numeric operator (a whitelist of five operators, externally tagged serde
/// form: `{"negate": {}}` / `{"div": 100}`). Semantics (matches the single
/// value_expr implementation):
///
/// - `negate`: `v → -v`;
/// - `clamp{min,max}`: `v → min(max(v, min), max)`;
/// - `div(n)`: `v → v / n`;
/// - `mult(n)`: `v → v × n`;
/// - `base(n)`: `v → v + n` (added first, before the rest of the operator chain).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueOpDef {
    /// Negation.
    Negate {},
    /// Range clamp.
    Clamp {
        /// Lower bound.
        min: f64,
        /// Upper bound.
        max: f64,
    },
    /// Divide by a constant.
    Div(f64),
    /// Multiply by a constant.
    Mult(f64),
    /// Add a base constant.
    Base(f64),
}

/// A value expression with an operator chain (`{"ref": "$1", "ops": [{"negate": {}}, {"div": 100}]}`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValueExprDef {
    /// Capture reference (`$1..$n`, in the order the capture groups appear
    /// in the pattern).
    #[serde(rename = "ref")]
    pub capture: String,
    /// Operator chain (applied in order).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ops: Vec<ValueOpDef>,
}

/// A template's value shape: a numeric literal | a `"$n"` capture reference |
/// an expression with an operator chain | a nested mod payload | a scalar
/// table. `Flag(bool)` covers the `value=true` literal for FLAG-type mods.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TemplateValueDef {
    /// FLAG boolean literal.
    Flag(bool),
    /// Numeric literal.
    Number(f64),
    /// A direct capture reference (`"$1"`).
    Capture(String),
    /// An expression (capture plus operator chain).
    Expr(ValueExprDef),
    /// A nested mod payload (vendor `mod("EnemyModifier", "LIST", { mod = mod(...) })`
    /// shape; JSON shape `{"mods": [<ModTemplateDef>...]}`). Instantiated as
    /// `ModValue::NestedMods`, forwarded to the target db by the
    /// orchestration layer (env_finalize's `forward_enemy_modifiers`, etc.)
    /// via `ModDb::list_nested`. **Must be listed before `List`**: untagged
    /// tries variants in declaration order, and `{"mods": ...}`'s array
    /// value would fail to deserialize as `List` (a scalar table), but the
    /// reverse order would let `List` wrongly swallow a scalar table.
    Nested {
        /// The inner mod templates (same schema as the top-level
        /// [`ModTemplateDef`], can recurse).
        mods: Vec<ModTemplateDef>,
    },
    /// A LIST-type mod's structured value (e.g. the keystone name for
    /// `Keystone LIST`). Strings inside the value must be literals or
    /// products of a closed enum set — no runtime concatenation (a hard DSL
    /// limit).
    List(BTreeMap<String, TemplateScalarDef>),
}

/// A scalar inside a template: a literal, or a `"$n"` capture /
/// `{"enum": n}` closed-set reference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TemplateScalarDef {
    /// Boolean literal.
    Bool(bool),
    /// Numeric literal.
    Number(f64),
    /// String literal or a `"$n"` capture reference.
    Text(String),
    /// A closed-set enum reference (`{"enum": 3}` = look up the full literal
    /// in this entry's `enums["3"]` table using the 3rd capture's word;
    /// every possible output is an explicit literal in the table, not a
    /// string concatenation).
    Enum {
        /// Capture group index (1-based).
        #[serde(rename = "enum")]
        capture_index: u32,
    },
    /// A list of string literals (the `skillNameList` shape of vendor's
    /// `SkillName` tag). Its JSON array shape is mutually exclusive with the
    /// other scalar variants, so untagged deserialization is unambiguous.
    TextList(Vec<String>),
}

/// A template tag (a serde-shape projection of pobr's `ModTag`; fields
/// besides `type` are transcribed openly, with values that can be a literal
/// / `"$n"` / an enums reference).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateTagDef {
    /// Tag type (e.g. `Condition` / `Multiplier` / `SkillType`).
    #[serde(rename = "type")]
    pub tag_type: String,
    /// The remaining fields (e.g. `var` / `stat` / `threshold`), open per
    /// tag type.
    #[serde(flatten)]
    pub fields: BTreeMap<String, TemplateScalarDef>,
}

/// A mod name in one of two shapes: a literal or a closed-set enums reference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TemplateNameDef {
    /// ModName literal.
    Literal(String),
    /// A closed-set enums reference (same as [`TemplateScalarDef::Enum`]).
    Enum {
        /// Capture group index (1-based).
        #[serde(rename = "enum")]
        capture_index: u32,
    },
}

/// A template that produces one mod (`ModTemplate`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModTemplateDef {
    /// ModName (a literal or an enums reference).
    pub name: TemplateNameDef,
    /// Mod type (`BASE|INC|MORE|FLAG|OVERRIDE|LIST`, pobr's ModType serde name).
    #[serde(rename = "type")]
    pub mod_type: String,
    /// The value (one of three shapes, see [`TemplateValueDef`]).
    pub value: TemplateValueDef,
    /// ModFlags bit names (using whichever bit names are current at the
    /// time; this batch only uses existing bit names).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<String>,
    /// KeywordFlags bit names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keyword_flags: Vec<String>,
    /// Tag list.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<TemplateTagDef>,
    /// The target: `player` (default) | `enemy` (wrapped as an
    /// EnemyModifier LIST, forwarded) | `minion` (wrapped as a
    /// MinionModifier LIST).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

/// A single special mod-line template (`SpecialTemplateDef`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpecialTemplateDef {
    /// Stable id (snake_case; referenced by diffs / reports / oracle
    /// reconciliation. A rename counts as delete+add).
    pub id: String,
    /// Match pattern: Rust regex syntax (a subset of the regex crate — no
    /// look-around, no backreferences). The engine uniformly lowercases the
    /// input and anchors the whole line (wrapped in `^...$` at compile time,
    /// per vendor `:6155-6158`). Capture groups are numbered in appearance
    /// order as `$1..$n`; numeric captures uniformly use
    /// `(\d+(?:\.\d+)?)`; word-class captures must be an explicit closed set
    /// (e.g. `(fire|cold|lightning|chaos|physical)`) — open captures like
    /// `(.+)` are forbidden (such entries go through `handler_id` instead).
    pub pattern: String,
    /// Vendor reconciliation metadata: the original Lua pattern literal
    /// (used by `check --special-coverage` to diff for existence against
    /// the vendor key). `None` means a pobr-only special case (no matching
    /// key in vendor; see `source_note` for its origin).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor_pattern: Option<String>,
    /// Template paths (mutually exclusive with `handler_id`; both absent
    /// means a "known unsupported" entry that's recognized but produces no
    /// mod — goes into the unsupported report but no longer counts as a
    /// parse failure).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mods: Vec<ModTemplateDef>,
    /// Handler path: the stable id of an entry with real logic (looked up
    /// at runtime in `pobr-core::rules::registry`; unregistered → matched
    /// but produces empty mods plus a report flag).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handler_id: Option<String>,
    /// Handler arguments (captures forwarded in order, as `"$n"`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub handler_args: Vec<String>,
    /// Restricted closed-set enums mapping: key = capture group index (as a
    /// string), value = a closed `captured word → full literal` table.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub enums: BTreeMap<String, BTreeMap<String, String>>,
    /// Oracle-reconciliation-passed marker (set true by the Track D
    /// process; edited by hand in JSON as an independent commit).
    #[serde(default)]
    pub verified: bool,
    /// Batch marker (`S0|S1|S2`, for long-tail continuation).
    pub batch: String,
    /// Source note (unique item name / keystone name / pobr-authoritative
    /// explanation).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_note: Option<String>,
}

/// Top level of `overlay/special_mods.json` / `generated/special_derived.json`
/// (the consumer ignores `_meta`; the two tables' `entries` are concatenated,
/// with id collisions treated as errors — this is a wiring-level semantic).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SpecialModsDef {
    /// Template entries, ascending by `id`.
    pub entries: Vec<SpecialTemplateDef>,
}

//  ModParser rule six-table schema (overlay/mod_parser_rules.json)

/// Current overlay document schema identifier (bumped when the field shape evolves).
pub const MOD_PARSER_RULES_SCHEMA: &str = "mod_parser_rules/v1";

/// A single tag template: a faithful transcription of a vendor tag table
/// (`type` plus the remaining fields).
///
/// Field values may contain placeholder template strings (products of
/// closure-probe inference, e.g. `"$1"` / `"$2:cap+Effect"`); plain-table
/// entries are all literal values.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TagTemplate {
    /// Tag type (vendor's `type` field: `Condition` / `Multiplier` /
    /// `PerStat` / `SkillType` / `ModFlagOr` / `ActorCondition` …).
    #[serde(rename = "type")]
    pub tag_type: String,
    /// The remaining fields (dictionary order): `var` / `varList` / `div` /
    /// `limit` / `limitTotal` / `actor` / `neg` / `threshold` /
    /// `skill_type` (already reverse-looked-up to a name) / `mod_flags` /
    /// `keyword_flags` (already decomposed into name arrays), etc.
    #[serde(flatten)]
    pub fields: BTreeMap<String, StatMapValue>,
}

/// The full set of effect fields shared by the various phrase/pattern
/// tables (shared between pre_flags and tag_phrases; name_map /
/// flag_phrases are a subset, flattened in via serde).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RuleEffectsDef {
    /// ModFlag names (decomposed from vendor's `flags` mask).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<String>,
    /// KeywordFlag names (decomposed from vendor's `keywordFlags` mask).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keyword_flags: Vec<String>,
    /// Tag templates (vendor's single `tag` and `tagList` are normalized
    /// into one array, in the original order `[tag] ++ tagList`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<TagTemplate>,
    /// Player-side tags (normalized from vendor's `playerTag` /
    /// `playerTagList`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub player_tags: Vec<TagTemplate>,
    /// Wrapping directive: forward the mod to the minion (vendor `addToMinion`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub add_to_minion: bool,
    /// Tags carried along when forwarding to the minion (vendor's single
    /// `addToMinionTag` normalized into an array).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub add_to_minion_tags: Vec<TagTemplate>,
    /// Wrapping directive: fold into the aura effect (vendor `addToAura`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub add_to_aura: bool,
    /// Wrapping directive: fold only into banners (vendor `onlyAddToBanners`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub only_add_to_banners: bool,
    /// Wrapping directive: create a new aura (vendor `newAura`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub new_aura: bool,
    /// The new aura affects allies only (vendor `newAuraOnlyAllies`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub new_aura_only_allies: bool,
    /// Wrapping directive: inject the mod into the skill's local scope
    /// (vendor `addToSkill`, a single tag).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub add_to_skill: Option<TagTemplate>,
    /// Wrapping directive: apply the mod to the enemy (vendor `applyToEnemy`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub apply_to_enemy: bool,
    /// Enemy-actor perspective (vendor `actorEnemy`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub actor_enemy: bool,
    /// ModName suffix (vendor `modSuffix`, e.g. `^take ` → `"Taken"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mod_suffix: Option<String>,
}

/// A formList entry: Lua pattern → form id.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct FormDef {
    /// Raw Lua pattern (including `^` anchors / `(%d+)` captures / `%%` escapes).
    pub pattern: String,
    /// Form id (`INC` / `RED` / `MORE` / `BASE` / `PEN` / `DMG` … 28 kinds).
    pub form: String,
    /// Derived: the longest contiguous literal substring in the pattern
    /// (used for aho-corasick pre-filtering; `None` for fully-wildcard
    /// patterns → the engine's always-check bucket).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub literal: Option<String>,
    /// Derived: whether the pattern is anchored with `^` (the engine only
    /// tries it at the start of the remaining text).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub anchored: bool,
}

/// A modNameList entry: a phrase (plain substring match) → a set of
/// ModNames plus optional effects.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct NameMapDef {
    /// Match phrase (lowercase, plain substring match, no pattern syntax).
    pub phrase: String,
    /// Vendor ModName list (a single name is still wrapped in an array;
    /// stored directly as pobr `StatId`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub names: Vec<String>,
    /// Optional effects (vendor table entries carrying tag / flags / addToMinion).
    #[serde(flatten)]
    pub effects: RuleEffectsDef,
}

/// A modFlagList entry: a phrase (plain) → ModFlag/KeywordFlag plus an
/// optional tag.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct FlagPhraseDef {
    /// Match phrase (lowercase, plain substring match).
    pub phrase: String,
    /// Effects (flags / keyword_flags / tags / addToMinion…).
    #[serde(flatten)]
    pub effects: RuleEffectsDef,
}

/// A preFlagList entry: a leading pattern → flags/tag/wrapping directives.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PreFlagDef {
    /// Raw Lua pattern (vendor anchors all of these with `^`).
    pub pattern: String,
    /// Derived: the longest literal substring (see [`FormDef::literal`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub literal: Option<String>,
    /// Derived: `^`-anchored.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub anchored: bool,
    /// The full set of effect fields.
    #[serde(flatten)]
    pub effects: RuleEffectsDef,
    /// Whether a closure entry was inferred into a template by probing (can
    /// be upgraded to verified once covered by oracle differential testing).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub inferred: bool,
    /// A closure entry where probe inference failed: the Rust handler
    /// registry id (`pre_flag:<hash12>`); mutually exclusive with `effects`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handler_id: Option<String>,
}

/// A modTagList entry: a per-X / conditional-phrase pattern → a tag template.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TagPhraseDef {
    /// Raw Lua pattern.
    pub pattern: String,
    /// Derived: the longest literal substring.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub literal: Option<String>,
    /// Derived: `^`-anchored.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub anchored: bool,
    /// The full set of effect fields (shared with pre_flags; most entries
    /// only use `tags`).
    #[serde(flatten)]
    pub effects: RuleEffectsDef,
    /// See [`PreFlagDef::inferred`].
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub inferred: bool,
    /// See [`PreFlagDef::handler_id`] (`tag_phrase:<hash12>`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handler_id: Option<String>,
}

/// A small lookup-table entry: phrase → a single suffix/type name
/// (suffix_types / damage_types / pen_types).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PhraseValueDef {
    /// Match phrase (plain).
    pub phrase: String,
    /// Target name (e.g. `GainAsFire` / `Physical` / `LightningPenetration`).
    pub value: String,
}

/// A small lookup-table entry: phrase → a set of names.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PhraseNamesDef {
    /// Match phrase (plain; includes the `maximum X` variants vendor adds
    /// at load time).
    pub phrase: String,
    /// Target names (e.g. `["LifeRegen", "ManaRegen"]`; a single name is
    /// still wrapped in an array).
    pub names: Vec<String>,
}

/// An embedded mod inside a flagTypes entry (currently only the hexproof
/// special case).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct FlagTypeModDef {
    /// ModName (e.g. `CurseEffectOnSelf`).
    pub name: String,
    /// Aggregation type, raw text (e.g. `MORE`).
    pub mod_type: String,
    /// Numeric value.
    pub value: f64,
}

/// A flagTypes entry: a FLAG-form phrase → either a `Condition:X` string or
/// an embedded mod.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct FlagTypeDef {
    /// Match phrase (plain; a few use pattern syntax, e.g. the hindered variants).
    pub phrase: String,
    /// String form: a condition/flag name (e.g. `Condition:Phasing` / `NoLifeRegen`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    /// Table form: an embedded mod (the hexproof special case); mutually
    /// exclusive with `condition`.
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "mod")]
    pub mod_def: Option<FlagTypeModDef>,
}

/// Top level of `overlay/mod_parser_rules.json` (from the consumer's
/// perspective: serde ignores `_meta`'s provenance header by default;
/// fields follow in the order below).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ModParserRulesDoc {
    /// formList (91 pattern → one of 28 form ids), ascending by pattern.
    pub forms: Vec<FormDef>,
    /// modNameList (775 phrase → ModName set), ascending by phrase.
    pub name_map: Vec<NameMapDef>,
    /// modFlagList (202 phrase → flag bit names + tag), ascending by phrase.
    pub flag_phrases: Vec<FlagPhraseDef>,
    /// preFlagList (219 leading pattern → wrapping directive), ascending by
    /// pattern.
    pub pre_flags: Vec<PreFlagDef>,
    /// modTagList (682 per-X/conditional pattern → tag template), ascending
    /// by pattern.
    pub tag_phrases: Vec<TagPhraseDef>,
    /// suffixTypes (suffix scan table for the BASE/GAIN/LOSE/GRANTS form family).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suffix_types: Vec<PhraseValueDef>,
    /// dmgTypes (damage-type table for the DMG form family).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub damage_types: Vec<PhraseValueDef>,
    /// penTypes (penetration-target table for the PEN form).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pen_types: Vec<PhraseValueDef>,
    /// regenTypes (REGEN family; the final expanded shape derived at vendor
    /// load time via `appendMod(resourceTypes, "Regen")`, dumped as-is).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub regen_types: Vec<PhraseNamesDef>,
    /// degenTypes (derived expansion for the DEGEN family).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub degen_types: Vec<PhraseNamesDef>,
    /// costTypes (derived expansion for the TOTALCOST form; named with a
    /// `_map` suffix to avoid clashing with the base domain's `cost_types`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cost_types_map: Vec<PhraseNamesDef>,
    /// baseCostTypes (derived expansion for the BASECOST form).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub base_cost_types: Vec<PhraseNamesDef>,
    /// flagTypes (condition table for the FLAG form).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flag_types: Vec<FlagTypeDef>,
    /// unsupportedModList (verbatim from vendor, currently only `mirrored`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unsupported: Vec<String>,
    /// pobr's own additional unsupported entries (kept in a separate
    /// section from vendor's to keep drift diffs clean; `split` comes from
    /// the current hardcoded value in `mod_parser.rs:63` and must be
    /// preserved when migrating tables).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unsupported_pobr_extra: Vec<String>,
}
