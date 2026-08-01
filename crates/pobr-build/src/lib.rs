//! pobr-build: Build state machine / PoB Build Code compatible codec / calculation orchestration.
//!
//! For the target design see `devs/docs/architecture/02-crate-design.md` §7 and
//! `05-compatibility-and-i18n.md`.
//!
//! Module overview:
//! - [`error`] — three error layers ([`BuildCodeError`] / [`XmlError`] / [`BuildError`]).
//! - [`build_code`] — PoB Build Code codec (URL-safe Base64 + zlib, padding-tolerant + bomb guard).
//! - [`import_detect`] — quick-import recognition (Build Code / XML / pobb.in link / raw item).
//! - [`build_config`] — [`BuildConfig`] + `to_calc_config` (adapts to REAL [`pobr_core::CalcConfig`]).
//! - [`build`] — [`Build`] in-memory state (uses REAL `pobr_data` types + a simplified [`SocketGroup`]).
//! - [`xml_serde`] — PoB Build XML header parsing (quick-xml).
//! - [`xml_build`] — PoB Build XML → full [`Build`] (passive tree / item slots / skill gem groups).
//! - [`snapshot`] — read-only snapshot of calculation input + deterministic content hash.
//! - [`build_data`] — projects [`pobr_gamedata::GameData`] into the in-memory indexes the orchestrator needs (nodes/gems/class attributes).
//! - [`calc_orchestrator`] — drives a [`Build`] through [`pobr_core::calc::CalculationSession`]
//!   (text-only `calculate` + end-to-end attribution `calculate_with_data`).
//! - [`calc_cache`] — result cache keyed by content hash.
//! - [`comparison`] — scalar field diff between two [`pobr_core::calc::OutputTable`]s.
//!
//! Design constraints: deterministic, immutable, zero network I/O (share links are only
//! recognized and their key extracted; fetching is left to the caller).

pub mod buff_stat_map;
pub mod build;
pub mod build_code;
pub mod build_config;
pub mod build_data;
pub mod calc_cache;
pub mod calc_orchestrator;
pub mod comparison;
pub(crate) mod config_resolve;
pub mod corpus;
pub mod error;
pub mod handlers;
pub mod import_detect;
pub mod loadout;
pub mod snapshot;
pub mod xml_build;
pub mod xml_merge;
pub mod xml_serde;

pub use build::{Build, CharacterIdentity, SocketGroup};
pub use build_code::{decode_pob_code, encode_pob_code};
pub use build_config::BuildConfig;
pub use build_data::{BuildData, ClassBaseAttributes, EffectStats, ResolvedSkillLevel};
pub use calc_cache::CalcCache;
pub use calc_orchestrator::{
    DataOrchestratorOptions, FullDpsReport, OrchestratorOptions, SkillDps, StatMapCompareRecord,
    StatMapMode, TreeVersionReport, calculate, calculate_full_dps, calculate_with_data,
    calculate_with_data_session, diagnose_tree_version, resolve_main_skill_selection,
    take_stat_map_compare_records,
};
pub use comparison::{FieldDiff, OutputComparison, compare_outputs};
pub use error::{BuildCodeError, BuildError, XmlError};
pub use import_detect::{ImportKind, ShareService, detect_import};
pub use loadout::{
    BuildSets, Loadout, SetRef, SetSelection, active_selection, derive_loadouts, select_sets,
};
pub use snapshot::BuildSnapshot;
pub use xml_build::{
    RawItemsView, default_quest_stat_reward_texts, default_true_condition_keys, parse_build,
    parse_build_from_code, parse_build_sets, parse_notes, parse_raw_items_view,
    radius_jewel_from_text,
};
pub use xml_merge::{SetKind, duplicate_set, merge_active_sets, remove_set, rename_set};
pub use xml_serde::{ParsedBuildHeader, is_pob_xml, parse_build_header};
