//! pobr-item：raw item text 解析 + 自定义物品编辑（**占位骨架，尚未实现**）。
//!
//! 目标设计见 `devs/docs/architecture/02-crate-design.md` §6。
//!
//! 复用 `pobr-core::mod_parser` 解析英文 modifier 文本，避免在 item 层重复规则。
//! 当前仅占位，待实现：`RawItemParseResult`（按 section 切分 implicit/explicit/enchant/
//! crafted/...）、`CustomItemDraft` → `pobr_data::Item`、不支持行收集（不阻塞保存）。
//!
//! 注：`pobr-core` 已有 `item_text.rs` 雏形（PoB 兼容物品文本解析），后续迁移 / 拆分时
//! 需明确与本 crate 的职责边界。
