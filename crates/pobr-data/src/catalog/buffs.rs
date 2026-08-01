//! Built-in buff definition domain schema (`overlay/buff_definitions.json`).
//!
//! Data source: a **hand-curated distillation** of vendor PoB2
//! `src/Modules/CalcPerform.lua`'s `doActorMisc` (:503-765, a 260-line
//! procedural if-chain) — this section can't be extracted by serializing it
//! through luajit, so it's stored via the overlay-channel exception
//! approved by decision §4.2-4: each entry carries a `vendor_ref` (line
//! numbers + a line-range hash) for drift alerts (reconciled by
//! `sync-pob-catalog check-buff-refs`); correctness is established by
//! oracle reconciliation / per-buff numeric unit tests.
//!
//! The effect formula (the framework logic stays in Rust, see
//! `pobr-core::rules::buff_expander`):
//!
//! ```text
//! scale  = (1 + Σ INC(inc_stats)/100) × Π MORE(more_stats)
//! effect = clamp(rounding(base × scale), min, max)
//! each mod's value = a value template (Literal / coeff×effect / rounding(coeff×scale))
//! ```

use serde::{Deserialize, Serialize};

use super::value_expr::EffectTag;

/// Current overlay document schema identifier.
pub const BUFF_DEFINITIONS_SCHEMA: &str = "buff_definitions/v1";

/// Top level of `overlay/buff_definitions.json` (the consumer ignores
/// `_meta` by default via serde).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuffDefinitionsDef {
    /// All buff definitions, ascending by `id`.
    pub buffs: Vec<BuffDef>,
}

/// A single built-in buff definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuffDef {
    /// Stable ID (e.g. `Onslaught`).
    pub id: String,
    /// Trigger flag name (the mod_db FLAG query key; usually matches `id`).
    pub trigger_flag: String,
    /// Mode gate (the `env.mode_combat` gate that wraps the whole
    /// doActorMisc section).
    pub mode_gate: BuffModeGate,
    /// The effect-magnitude formula; absent for buffs that are pure
    /// literals (HerEmbrace, etc.).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect: Option<BuffEffectFormula>,
    /// The mod templates the expansion produces.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mods: Vec<BuffModTemplate>,
    /// Condition names set alongside this buff (vendor's
    /// `condList[...] = true`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions_set: Vec<String>,
    /// Stable handler ID for entries with real logic (mutually exclusive
    /// with effect/mods; the buff domain's budget is ≤8).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handler_id: Option<String>,
    /// Whether this has been verified via oracle reconciliation.
    #[serde(default)]
    pub verified: bool,
    /// Vendor line-range reference (traceability for the hand-curation,
    /// plus drift alerts).
    pub vendor_ref: VendorRef,
    /// Notes (known discrepancies / uncovered-branch explanations).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// A buff's mode gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuffModeGate {
    /// Only expanded under `mode_combat` (doActorMisc's `:510` whole-section gate).
    Combat,
}

/// Parameters for the effect-magnitude formula.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuffEffectFormula {
    /// Base magnitude (Onslaught=10 / Convergence=30 / Freeze=70…).
    pub base: f64,
    /// Stat names that scale via INC (e.g.
    /// `["OnslaughtEffect","BuffEffectOnSelf"]`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inc_stats: Vec<String>,
    /// Stat names that scale via a MORE product (calcLib.mod semantics).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub more_stats: Vec<String>,
    /// How the effect value is rounded (PoB2 mostly uses `m_floor`).
    #[serde(default)]
    pub rounding: Rounding,
    /// Effect lower bound (Freeze's `m_max(…, 0)`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    /// Effect upper bound.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
}

/// A rounding mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rounding {
    /// No rounding.
    #[default]
    None,
    /// Round down (`m_floor`).
    Floor,
}

/// A single mod template.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuffModTemplate {
    /// ModName.
    pub name: String,
    /// Mod type, vendor's raw literal (`BASE/INC/MORE/FLAG`).
    pub mod_type: String,
    /// Value template.
    pub value: BuffModValue,
    /// ModFlag names (vendor's display names, e.g. `Attack`/`Cast`/`Sword`;
    /// the consumer maps them to the bit enum — an unknown name is logged
    /// as diagnostics and that mod is skipped, conservatively avoiding
    /// emitting a wrong value).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<String>,
    /// Restricted tags (reuses the config DSL's whitelist).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<EffectTag>,
}

/// A mod's value template (two rounding shapes: effect-level rounding vs.
/// per-mod rounding).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BuffModValue {
    /// A literal (doesn't scale with the effect, e.g. HerEmbrace).
    Literal {
        /// The literal value.
        value: f64,
    },
    /// `coeff × effect` (effect is already rounded per the formula —
    /// Onslaught's `2 × effect`).
    PerEffect {
        /// Coefficient.
        coeff: f64,
    },
    /// `rounding(coeff × scale)` (rounded per-mod — Adrenaline's
    /// `m_floor(25 × effectMod)`; scale = the unrounded scaling factor).
    ScaledRounded {
        /// Coefficient.
        coeff: f64,
        /// Rounding mode.
        #[serde(default)]
        rounding: Rounding,
    },
}

/// A vendor line-range reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VendorRef {
    /// Vendor source path, relative (e.g. `Modules/CalcPerform.lua`).
    pub file: String,
    /// Start line (1-based, inclusive).
    pub line_start: u32,
    /// End line (1-based, inclusive).
    pub line_end: u32,
    /// Hash of the line range (`fnv1a64:<16hex>`, for drift-alert
    /// reconciliation; not for cryptographic use).
    pub segment_hash: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn onslaught() -> BuffDef {
        BuffDef {
            id: "Onslaught".to_string(),
            trigger_flag: "Onslaught".to_string(),
            mode_gate: BuffModeGate::Combat,
            effect: Some(BuffEffectFormula {
                base: 10.0,
                inc_stats: vec![
                    "OnslaughtEffect".to_string(),
                    "BuffEffectOnSelf".to_string(),
                ],
                more_stats: Vec::new(),
                rounding: Rounding::Floor,
                min: None,
                max: None,
            }),
            mods: vec![
                BuffModTemplate {
                    name: "Speed".to_string(),
                    mod_type: "INC".to_string(),
                    value: BuffModValue::PerEffect { coeff: 2.0 },
                    flags: vec!["Attack".to_string()],
                    tags: Vec::new(),
                },
                BuffModTemplate {
                    name: "MovementSpeed".to_string(),
                    mod_type: "INC".to_string(),
                    value: BuffModValue::PerEffect { coeff: 1.0 },
                    flags: Vec::new(),
                    tags: Vec::new(),
                },
            ],
            conditions_set: Vec::new(),
            handler_id: None,
            verified: false,
            vendor_ref: VendorRef {
                file: "Modules/CalcPerform.lua".to_string(),
                line_start: 540,
                line_end: 571,
                segment_hash: "fnv1a64:0000000000000000".to_string(),
            },
            notes: None,
        }
    }

    /// BuffDef round-trips through serde.
    #[test]
    fn buff_def_round_trip() {
        let def = onslaught();
        let json = serde_json::to_string_pretty(&def).unwrap();
        let back: BuffDef = serde_json::from_str(&json).unwrap();
        assert_eq!(back, def);
    }

    /// Minimal handler-entry shape plus default backfill.
    #[test]
    fn handler_entry_defaults() {
        let json = r#"{
            "id": "Fortify",
            "trigger_flag": "Fortified",
            "mode_gate": "combat",
            "handler_id": "buff:fortify",
            "vendor_ref": {
                "file": "Modules/CalcPerform.lua",
                "line_start": 523,
                "line_end": 539,
                "segment_hash": "fnv1a64:abcd"
            }
        }"#;
        let def: BuffDef = serde_json::from_str(json).unwrap();
        assert_eq!(def.handler_id.as_deref(), Some("buff:fortify"));
        assert!(def.mods.is_empty());
        assert!(def.effect.is_none());
        assert_eq!(def.effect.and_then(|e| e.max), None);
    }

    /// Rounding defaults to None; BuffModValue's three shapes round-trip.
    #[test]
    fn mod_value_variants_round_trip() {
        let values = vec![
            BuffModValue::Literal { value: 100.0 },
            BuffModValue::PerEffect { coeff: 2.0 },
            BuffModValue::ScaledRounded {
                coeff: 25.0,
                rounding: Rounding::Floor,
            },
        ];
        for value in values {
            let json = serde_json::to_string(&value).unwrap();
            let back: BuffModValue = serde_json::from_str(&json).unwrap();
            assert_eq!(back, value);
        }
        let json = r#"{"kind":"scaled_rounded","coeff":0.3}"#;
        let back: BuffModValue = serde_json::from_str(json).unwrap();
        assert_eq!(
            back,
            BuffModValue::ScaledRounded {
                coeff: 0.3,
                rounding: Rounding::None
            }
        );
    }
}
