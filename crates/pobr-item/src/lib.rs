//! pobr-item：raw item text 的**全保真编辑态**解析 + 逆向序列化（P16）。
//!
//! 目标设计见 `devs/docs/architecture/02-crate-design.md` §6 与
//! `audits/rearchitecture-2026-06-10/blueprints/m5c-item-tree.md` Track A。
//!
//! 职责边界：
//! - **calc 视图**（剥标注、variant 门控、range 取值后喂计算引擎）仍由
//!   `pobr-core::item_text` + `pobr-core::item::ingest_item` 承担。
//! - **编辑态视图**（保留 calc 刻意丢弃的 variant 名列表 / 行级标注 / 未建模标注）由
//!   本 crate 的 [`ItemDraft`] 承担，支持 BuildRaw 往返（P16 验收契约）。
//!
//! 复用 `pobr-core::mod_parser` 解析英文 modifier 文本，避免在 item 层重复规则；
//! 规则函数（variant 门控 / range 取值）后续由 pobr-core 提供共享入口，本 crate 只编排。

pub mod annotations;
pub mod build_raw;
pub mod draft;
pub mod tier;

pub use annotations::{ModLineAnnotations, parse_mod_line, round_to};
pub use draft::{
    CatalystState, DisplayLine, DisplayLineKind, DraftError, DraftHeader, ItemDraft, ItemStates,
    LineBucket, ModLineDraft, VariantState, classify_display_lines,
};
pub use tier::{TierIndex, TierInfo};
