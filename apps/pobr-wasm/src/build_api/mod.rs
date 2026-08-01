//! The web frontend's JSON contract: build decoding / full calculation / breakdown / attribution.
//!
//! Contract principle (TODO.md P0): the frontend only consumes the JSON
//! shapes defined here — it never imports Rust types and never re-implements
//! any calculation. Every value is computed by this module calling into
//! existing crate capabilities. JSON shapes are pinned by
//! `tests/contract_golden.rs`, with `web/src/api/types.ts` hand-written to
//! mirror them isomorphically; a shape change must touch both places plus the golden fixture.
//!
//! Every entry point is `&str -> Result<String, String>` (JSON in, JSON
//! out). **The Err side is also JSON**: [`ApiError`] encodes
//! `{code, message, slot?}` (a wasm `JsError` can only carry a string), and
//! the web side's `wasmBackend` parses it back into a typed error, dispatching by code.
//!
//! Split into submodules by domain: [`decode`] (build code / .build file
//! decoding), [`request`] (calculation request DTOs plus shared assembly),
//! [`calculate`] (full calculation / full DPS), [`analysis`] (node power /
//! variant optimization / attribution), [`encode`] (edit state -> a share
//! code), [`catalog`] (gem/rune catalogs, per-line coloring, translation).
//! Small shared utilities stay in this file.

mod analysis;
mod calculate;
mod catalog;
mod decode;
mod encode;
mod request;

pub use analysis::{AttributionRequest, attribution_json, node_power_json, optimize_variants_json};
pub use calculate::{calculate_build_json, full_dps_json};
pub use catalog::{
    classify_item_lines_json, gem_catalog_json, reforge_runes_json, rune_catalog_json,
    translate_lines_to_zh_cn_json,
};
pub use decode::{
    decode_build_file_json, decode_build_json, decode_build_loadout_json, manage_loadout_json,
};
pub use encode::encode_build_json;
pub use request::{
    CalculateBuildRequest, CharacterOverride, GemInput, JewelInput, SlotItemInput, SocketGroupInput,
};

use pobr_data::item::EquipmentSlot;
use serde::Serialize;

use crate::state;

/// The outward-facing error contract: `{code, message, slot?}`, serialized
/// to a JSON string on an entry point's Err side.
///
/// `code` values (kept in sync with web/src/api/types.ts):
/// - `not_initialized` — game data hasn't been initialized; run the init flow first
/// - `bad_request` — the request JSON is malformed / a field value is invalid (a client bug)
/// - `decode_error` — PoB code / .build file decoding failed (bad user input)
/// - `internal` — every other calculation/serialization error (the catch-all, `From<String>` lands here automatically)
///
/// Note: a single equipment/jewel/flask's **text** failing to parse doesn't
/// go through Err — it degrades to `item_errors` in the response (that item
/// is skipped and the calculation continues), see [`request::apply_request_overrides`].
#[derive(Debug, Serialize)]
pub(crate) struct ApiError {
    code: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    slot: Option<String>,
}

impl ApiError {
    pub(crate) fn bad_request(message: impl Into<String>) -> Self {
        Self {
            code: "bad_request",
            message: message.into(),
            slot: None,
        }
    }

    pub(crate) fn not_initialized(message: String) -> Self {
        Self {
            code: "not_initialized",
            message,
            slot: None,
        }
    }

    pub(crate) fn decode_error(message: impl Into<String>) -> Self {
        Self {
            code: "decode_error",
            message: message.into(),
            slot: None,
        }
    }

    pub(crate) fn with_slot(mut self, slot: impl Into<String>) -> Self {
        self.slot = Some(slot.into());
        self
    }

    /// Serializes at the Err boundary (falls back to the bare message if
    /// self-serialization fails; the frontend has a non-JSON fallback for that).
    pub(crate) fn into_json(self) -> String {
        serde_json::to_string(&self).unwrap_or(self.message)
    }
}

/// Every string error not explicitly classified lands in `internal` — this
/// keeps existing internal code's `?` usage unchanged.
impl From<String> for ApiError {
    fn from(message: String) -> Self {
        Self {
            code: "internal",
            message,
            slot: None,
        }
    }
}

/// Stable equipment-slot id -> [`EquipmentSlot`] (the inverse of `EquipmentSlot::id()`).
fn slot_from_id(id: &str) -> Result<EquipmentSlot, String> {
    const ALL: [EquipmentSlot; 11] = [
        EquipmentSlot::Weapon1,
        EquipmentSlot::Weapon2,
        EquipmentSlot::Helmet,
        EquipmentSlot::BodyArmour,
        EquipmentSlot::Gloves,
        EquipmentSlot::Boots,
        EquipmentSlot::Amulet,
        EquipmentSlot::Ring1,
        EquipmentSlot::Ring2,
        EquipmentSlot::Ring3,
        EquipmentSlot::Belt,
    ];
    ALL.into_iter()
        .find(|s| s.id() == id)
        .ok_or_else(|| format!("unknown equipment slot: {id}"))
}

/// Chinese input preprocessing (Phase 7.1): a line containing CJK attempts
/// "template reverse lookup -> canonical English" translation; if
/// unrecognized or there's no zh-CN data, it's kept as-is (the parser marks
/// it unsupported, visible to the frontend).
fn localize_input_text(text: &str) -> String {
    if !crate::zh::has_cjk(text) {
        return text.to_string();
    }
    let Some(translator) = state::zh_translator() else {
        return text.to_string();
    };
    text.lines()
        .map(|line| {
            if crate::zh::has_cjk(line) {
                translator
                    .translate_line(line)
                    .unwrap_or_else(|| line.to_string())
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
