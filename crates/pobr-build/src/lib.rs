//! pobr-build: Build state machine / PoB Build Code compatible codec / calculation orchestration.
//!
//! Module overview:
//! - [`error`] — three error layers ([`BuildCodeError`] / [`XmlError`] / [`BuildError`]).
//! - [`build_code`] — PoB Build Code codec (URL-safe Base64 + zlib, padding-tolerant + bomb guard).
//! - [`import_detect`] — quick-import recognition (Build Code / XML / pobb.in link / raw item).
//! - [`import_report`] — what an import dropped, and the reference values a build code carries.
//! - [`build_config`] — [`BuildConfig`] + `to_calc_config` (adapts to REAL [`pobr_core::CalcConfig`]).
//! - [`build`] — [`Build`] in-memory state (uses REAL `pobr_data` types + a simplified [`SocketGroup`]).
//! - [`xml_serde`] — PoB Build XML header parsing (quick-xml).
//! - [`xml_build`] — PoB Build XML → full [`Build`] (passive tree / item slots / skill gem groups).
//! - [`snapshot`] — read-only snapshot of calculation input + deterministic content hash.
//! - [`build_data`] — projects [`pobr_gamedata::GameData`] into the in-memory indexes the orchestrator needs (nodes/gems/class attributes).
//! - [`calc_orchestrator`] — drives a [`Build`] through [`pobr_core::calc::CalculationSession`]
//!   (text-only `calculate` + end-to-end attribution `calculate_with_data`).
//! - [`calc_cache`] — result cache keyed by content hash.
//! - [`breakdown`] — how a finished calculation arrives at a displayed stat (steps and
//!   modifier lines), serializable; shared by the web contract and the CLI.
//! - [`comparison`] — scalar field diff between two [`pobr_core::calc::OutputTable`]s.
//! - [`reference_check`] — a calculation against reference values recorded under PoB2 output names.
//! - [`usage`] — what a calculation did with each of a build's lines: applied, left out and
//!   why, never read, or not parsed; serializable, shared by the web contract and the CLI.
//!
//! Design constraints: deterministic, immutable, zero network I/O (share links are only
//! recognized and their key extracted; fetching is left to the caller).

pub mod breakdown;
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
pub mod import_report;
pub mod jewel_tree;
pub mod loadout;
pub mod reference_check;
pub mod snapshot;
pub mod support;
pub mod usage;
pub mod xml_build;
pub mod xml_merge;
pub mod xml_serde;

pub use build::{Build, CharacterIdentity, SocketGroup, WeaponSet};
pub use build_code::{decode_pob_code, encode_pob_code};
pub use build_config::BuildConfig;
pub use build_data::{BuildData, ClassBaseAttributes, EffectStats, ResolvedSkillLevel};
pub use calc_cache::{CalcCache, DataCalcCache};
pub use calc_orchestrator::{
    CalculationReport, DataOrchestratorOptions, Diagnosis, LineFate, MainSkillSummary, ModifierRef,
    OrchestratorOptions, ReceivedLine, SkillMetrics, SkillMetricsReport, StatMapCompareRecord,
    StatMapMode, TreeVersionReport, UnmappedStat, UnsetCondition, calculate,
    calculate_skill_metrics, calculate_with_data, calculate_with_data_report,
    calculate_with_data_session, diagnose, diagnose_tree_version, resolve_main_skill_selection,
};
pub use comparison::{FieldDiff, OutputComparison, compare_outputs};
pub use error::{BuildCodeError, BuildError, XmlError};
pub use import_detect::{ImportKind, ShareService, detect_import};
pub use import_report::{ImportReport, ImportedBuild, SkippedItem, UnmodelledSlot};
pub use loadout::{
    BuildSets, Loadout, SetRef, SetSelection, active_selection, derive_loadouts, select_sets,
};
pub use reference_check::{
    ReferenceRow, compare_build_to_reference, compare_minion_to_reference, compare_to_reference,
    damage_row, main_skill_minion,
};
pub use snapshot::BuildSnapshot;
pub use xml_build::{
    RawItemsView, default_quest_stat_reward_texts, default_true_condition_keys, import_build,
    import_build_from_code, import_build_with_allocated_nodes, parse_build, parse_build_from_code,
    parse_build_sets, parse_build_with_allocated_nodes, parse_custom_modifier_blocks, parse_notes,
    parse_raw_items_view, quest_reward_lines, radius_jewel_from_text,
};
pub use xml_merge::{SetKind, duplicate_set, merge_active_sets, remove_set, rename_set};
pub use xml_serde::{ParsedBuildHeader, is_pob_xml, parse_build_header};
