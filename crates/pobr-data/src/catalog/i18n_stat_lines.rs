//! 词条行输入翻译模板（`i18n/<lang>/stat_lines.json`，TODO Phase 7.1）。
//!
//! 来源 = GGG stat descriptions 的本地化模板对（经 `pipeline/gen-zh-cn.mjs`
//! 从国服客户端词典转录）。消费侧（pobr-wasm 契约层）把本地化词条行反查
//! 模板、代回数值，得到英文 canonical 行喂现有 parser——引擎本身保持英文。

use serde::{Deserialize, Serialize};

/// 一对词条行模板：`src` = 本地化模板（含 `{0}` / `{0:+d}` 占位符），
/// `en` = 对应英文 canonical 模板。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatLineTemplate {
    /// 本地化模板（如 `{0:+d} 生命上限`）。
    pub src: String,
    /// 英文 canonical 模板（如 `{0:+d} to maximum Life`）。
    pub en: String,
}
