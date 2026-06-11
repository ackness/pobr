//! pobr-build：Build 状态机 / PoB Build Code 兼容编解码 / 计算编排。
//!
//! 目标设计见 `devs/docs/architecture/02-crate-design.md` §7 与
//! `05-compatibility-and-i18n.md`。
//!
//! 模块概览：
//! - [`error`] — 三层错误（[`BuildCodeError`] / [`XmlError`] / [`BuildError`]）。
//! - [`build_code`] — PoB Build Code 编解码（URL-safe Base64 + zlib，padding 容错 + 防 bomb）。
//! - [`import_detect`] — 快捷导入识别（Build Code / XML / pobb.in 链接 / raw item）。
//! - [`build_config`] — [`BuildConfig`] + `to_calc_config`（适配 REAL [`pobr_core::CalcConfig`]）。
//! - [`build`] — [`Build`] 内存状态（采用 REAL `pobr_data` 类型 + 简化 [`SocketGroup`]）。
//! - [`xml_serde`] — PoB Build XML 头部解析（quick-xml）。
//! - [`xml_build`] — PoB Build XML → 完整 [`Build`]（天赋树 / 装备槽位 / 技能宝石组）。
//! - [`snapshot`] — 计算输入只读快照 + 确定性内容哈希。
//! - [`build_data`] — 把 [`pobr_gamedata::GameData`] 投影为 orchestrator 所需内存索引（节点/宝石/职业属性）。
//! - [`calc_orchestrator`] — 把 [`Build`] 驱动进 [`pobr_core::calc::CalculationSession`]
//!   （text-only `calculate` + 端到端归因 `calculate_with_data`）。
//! - [`calc_cache`] — 以内容哈希为键的结果缓存。
//! - [`comparison`] — 两份 [`pobr_core::calc::OutputTable`] 的标量字段 diff。
//!
//! 设计约束：确定性、不可变、零网络 I/O（分享链接只识别 + 提取 key，抓取由上层做）。

pub mod buff_stat_map;
pub mod build;
pub mod build_code;
pub mod build_config;
pub mod build_data;
pub mod calc_cache;
pub mod calc_orchestrator;
pub mod comparison;
pub mod error;
pub mod import_detect;
pub mod legacy_stat_filter;
pub mod skill_stat_map;
pub mod snapshot;
pub mod xml_build;
pub mod xml_serde;

pub use build::{Build, CharacterIdentity, SocketGroup};
pub use build_code::{decode_pob_code, encode_pob_code};
pub use build_config::BuildConfig;
pub use build_data::{BuildData, ClassBaseAttributes, EffectStats, ResolvedSkillLevel};
pub use calc_cache::CalcCache;
pub use calc_orchestrator::{
    DataOrchestratorOptions, OrchestratorOptions, StatMapCompareRecord, StatMapMode, calculate,
    calculate_with_data, take_stat_map_compare_records,
};
pub use comparison::{FieldDiff, OutputComparison, compare_outputs};
pub use error::{BuildCodeError, BuildError, XmlError};
pub use import_detect::{ImportKind, ShareService, detect_import};
pub use snapshot::BuildSnapshot;
pub use xml_build::{parse_build, parse_build_from_code};
pub use xml_serde::{ParsedBuildHeader, is_pob_xml, parse_build_header};
