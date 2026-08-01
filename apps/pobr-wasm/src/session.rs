//! JSON wrapper for the calculation entry point: `calculate_json` runs a
//! JSON payload (minimal input plus a list of modifier text) through
//! [`CalculationSession`], returning a JSON string of [`MinimalOutput`]'s
//! key fields.
//!
//! Design goal: the host (non-wasm) can compile and test this directly, so
//! this module only uses `serde_json` for the boundary and doesn't pull in
//! wasm-bindgen. Errors are all collapsed into `Result<String, String>`, so
//! they can pass through the wasm boundary as a JS exception string.

use pobr_core::calc::{CalculationSession, MinimalInput, MinimalOutput};
use serde::{Deserialize, Serialize};

/// The input envelope for `calculate_json`.
///
/// Every numeric field has a default (defaulting to 0), and `modifiers`
/// defaults to an empty list, so the caller only needs to supply the fields it cares about.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct CalculateRequest {
    base_life: f64,
    base_mana: f64,
    base_fire_resistance: f64,
    base_cold_resistance: f64,
    base_lightning_resistance: f64,
    base_accuracy: f64,
    enemy_evasion: f64,
    base_hit_min: f64,
    base_hit_max: f64,
    base_action_rate: f64,
    modifiers: Vec<String>,
}

impl From<&CalculateRequest> for MinimalInput {
    fn from(req: &CalculateRequest) -> Self {
        Self {
            base_life: req.base_life,
            base_mana: req.base_mana,
            base_fire_resistance: req.base_fire_resistance,
            base_cold_resistance: req.base_cold_resistance,
            base_lightning_resistance: req.base_lightning_resistance,
            base_accuracy: req.base_accuracy,
            enemy_evasion: req.enemy_evasion,
            base_hit_min: req.base_hit_min,
            base_hit_max: req.base_hit_max,
            base_action_rate: req.base_action_rate,
        }
    }
}

/// The output envelope for `calculate_json`: [`MinimalOutput`]'s key scalar
/// fields plus the list of modifier text that couldn't be parsed (for
/// frontend hints). `breakdown` isn't exposed in this minimal output.
#[derive(Debug, Clone, PartialEq, Serialize)]
struct CalculateResponse {
    life: f64,
    mana: f64,
    fire_resistance: f64,
    cold_resistance: f64,
    lightning_resistance: f64,
    crit_chance: f64,
    crit_multiplier: f64,
    total_hit_avg: f64,
    hit_chance: f64,
    action_rate: f64,
    dps: f64,
    unsupported_modifiers: Vec<String>,
}

impl CalculateResponse {
    fn from_output(output: &MinimalOutput, unsupported: &[String]) -> Self {
        Self {
            life: output.life,
            mana: output.mana,
            fire_resistance: output.fire_resistance,
            cold_resistance: output.cold_resistance,
            lightning_resistance: output.lightning_resistance,
            crit_chance: output.crit_chance,
            crit_multiplier: output.crit_multiplier,
            total_hit_avg: output.total_hit_avg,
            hit_chance: output.hit_chance,
            action_rate: output.action_rate,
            dps: output.dps,
            unsupported_modifiers: unsupported.to_vec(),
        }
    }
}

/// Parses `input_json` into minimal input plus a modifier text list, runs
/// one minimal calculation, and returns a JSON string of [`MinimalOutput`]'s key fields.
///
/// Modifier parsing uses the engine rules from already-initialized data
/// ([`crate::state`]) (the sole parser; the legacy one has been removed).
/// Non-empty `modifiers` with uninitialized data -> an explicit error
/// (fail-fast, never silently treating mod lines as Unsupported); a pure
/// base calculation with no modifiers needs no data. Unrecognized modifiers
/// (`ParseStatus::Unsupported`) never fail this function — they're collected
/// into the output's `unsupported_modifiers` instead.
pub fn calculate_json(input_json: &str) -> Result<String, String> {
    let request: CalculateRequest =
        serde_json::from_str(input_json).map_err(|err| format!("invalid input json: {err}"))?;

    let input = MinimalInput::from(&request);
    let mut session = CalculationSession::new(input);
    if !request.modifiers.is_empty() {
        let data = crate::state::build_data()
            .map_err(|e| format!("cannot parse modifiers without game data: {e}"))?;
        let rules = data
            .parser_rules
            .clone()
            .ok_or("game data has no parser rules (overlay/mod_parser_rules.json)")?;
        session.set_parser_rules(rules);
    }
    session
        .add_modifier_texts(&request.modifiers)
        .map_err(|err| format!("failed to parse modifier: {err}"))?;

    let output = session.perform_minimal();
    let response = CalculateResponse::from_output(&output, session.unsupported_modifier_texts());

    serde_json::to_string(&response).map_err(|err| format!("failed to serialize output: {err}"))
}
