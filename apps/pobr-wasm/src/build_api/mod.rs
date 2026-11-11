//! The web frontend's JSON contract: build decoding / full calculation / breakdown / attribution.
//!
//! Contract principle: the frontend only consumes the JSON
//! shapes defined here — it never imports Rust types and never re-implements
//! any calculation. Every value is computed by this module calling into
//! existing crate capabilities. JSON shapes are pinned by
//! `tests/contract_golden.rs`. Shared request DTOs generate
//! `web/src/api/generated/request.ts` through the host-only `typescript` tests;
//! remaining shapes are mirrored in `web/src/api/types.ts`. Shape changes must
//! update their types and contract assertions. The web WASM contract test also
//! exercises the actual frontend request conversion against the compiled engine.
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
//! code), [`catalog`] (gem/rune catalogs, per-line coloring, translation),
//! [`usage`] (what the calculation does with each line of a build).
//! Small shared utilities stay in this file.

mod analysis;
mod augments;
mod calculate;
mod catalog;
mod config_context;
mod crafting;
mod decode;
mod encode;
mod request;
mod support;
#[cfg(test)]
mod typescript;
mod uniques;
mod usage;
pub use support::support_groups_compatible_json;
pub use uniques::{unique_catalog_json, unique_item_draft_json};
pub use usage::line_usage_json;

pub use analysis::{AttributionRequest, attribution_json, node_power_json, optimize_variants_json};
pub use augments::{item_augment_info_json, reforge_runes_json, rune_catalog_json};
pub(crate) use calculate::clear_snapshots;
pub use calculate::{breakdown_descriptors_json, calculate_build_json, skill_metrics_json};
pub use catalog::{
    available_locales_json, classify_item_lines_json, display_catalog_json, game_name,
    gem_catalog_json, minion_catalog_json, parse_localized_lines_json,
    translate_lines_detailed_json, translate_lines_json,
};
pub use crafting::{item_crafting_catalog_json, normalize_item_draft_json};
pub use decode::{
    decode_build_file_json, decode_build_json, decode_build_loadout_json, encode_build_file_json,
    import_trade_items_json, manage_loadout_json,
};
pub use encode::encode_build_json;
pub use request::{
    CalculateBuildRequest, CharacterOverride, GemInput, JewelInput, SlotItemInput,
    SocketGroupInput, UtilitySlotInput,
};

use pobr_data::item::{EquipmentSlot, LimbSlot};
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
    EquipmentSlot::ALL
        .into_iter()
        .find(|s| s.id() == id)
        .ok_or_else(|| format!("unknown equipment slot: {id}"))
}

/// A Transcendent limb slot by its stable id (`arm1` … `leg2`).
fn limb_slot_from_id(id: &str) -> Result<LimbSlot, String> {
    LimbSlot::from_id(id).ok_or_else(|| format!("unknown limb slot: {id}"))
}

/// Localized input preprocessing: a line written in one of the data pack's
/// locales (decided by that locale's templates and names) attempts
/// "template reverse lookup -> canonical English" translation; if
/// unrecognized, or no locale has templates, it's kept as-is (the parser
/// marks it unsupported, visible to the frontend).
///
/// Text that no declared locale could have written (no letter English lacks,
/// and no locale sharing English's script) is returned without loading any
/// locale; text in which no line changes is returned byte for byte.
fn localize_input_text(text: &str) -> String {
    let locales = crate::locales::input_locales();
    if locales.codes.is_empty()
        || !pobr_i18n::lines::may_contain_localized_text(text, locales.shares_english_script)
    {
        return text.to_string();
    }
    let parsers = crate::locales::input_parsers();
    let span = |line: &str| parsers.iter().map(|p| p.span_from(line)).max().unwrap_or(1);
    pobr_i18n::lines::parse_text_with(text, span, |line| {
        parsers
            .iter()
            .filter(|parser| parser.is_localized(line))
            .find_map(|parser| parser.translate_line(line))
    })
    .into_owned()
}

mod tree_edit;
pub use tree_edit::preview_tree_edit_json;

mod tree_convert;
pub use tree_convert::preview_tree_conversion_json;

/// Resolves a member's outgoing party blocks from their build code.
pub fn import_party_member_json(code: &str) -> Result<String, String> {
    let data = state::build_data().map_err(|e| ApiError::not_initialized(e).into_json())?;
    let member = pobr_build::party::import_member(code, &data)
        .map_err(|e| ApiError::decode_error(e.to_string()).into_json())?;
    serde_json::to_string(&member).map_err(|e| e.to_string())
}

#[cfg(test)]
mod localize_tests {
    use super::localize_input_text;

    #[test]
    fn input_text_is_parsed_only_where_a_locale_wrote_it() {
        let dir = pobr_gamedata::current_data_dir();
        crate::init_data_from_dir(&dir.to_string_lossy()).unwrap();
        // English text, curly quotes and full-width punctuation included, loads no
        // locale and keeps its bytes.
        let english = "Rarity: Unique\r\nSapphire Ring\r\n+50 to maximum Life\r\n“Bow before me”，she said：why？\r\n";
        assert_eq!(localize_input_text(english), english);
        assert_eq!(crate::locales::cached(), (vec![], vec![]));
        // A localized line that no template recognizes changes nothing either.
        let unknown = "Rarity: Rare\r\n不认识的词条\r\n";
        assert_eq!(localize_input_text(unknown), unknown);
        assert_eq!(
            localize_input_text("Rarity: Rare\r\n蓝玉戒指\r\n+50 生命上限\r\n“Bow”"),
            "Rarity: Rare\nSapphire Ring\n+50 to maximum Life\n“Bow”"
        );
    }

    #[test]
    fn input_text_reads_a_description_written_on_two_lines_together() {
        let dir = pobr_gamedata::current_data_dir();
        crate::init_data_from_dir(&dir.to_string_lossy()).unwrap();
        // Mind Over Matter's two lines: alone, the second reads as another line
        // ("50% less Mana Regeneration Rate").
        assert_eq!(
            localize_input_text(
                "Rarity: Unique\r\n受到的所有伤害在扣除生命前优先从魔力扣除\r\n魔力回复速度总降 50%\r\n+50 生命上限"
            ),
            "Rarity: Unique\nAll Damage is taken from Mana before Life\n50% less Mana Recovery Rate\n+50 to maximum Life"
        );
        // Two localized lines read as one English line.
        assert_eq!(
            localize_input_text("Rarity: Rare\n力量的固有加成改为\n每 1 点力量 +5 命中值"),
            "Rarity: Rare\nInherent bonus of Strength grants +5 to Accuracy Rating per Strength instead"
        );
        // Lines whose repeated value differs are not one description.
        assert_eq!(
            localize_input_text("附加 5 - 10 冰霜伤害\n玩家及其召唤生物附加 3 - 4 冰霜伤害"),
            "Adds 5 to 10 Cold Damage\n玩家及其召唤生物附加 3 - 4 冰霜伤害"
        );
    }
}
