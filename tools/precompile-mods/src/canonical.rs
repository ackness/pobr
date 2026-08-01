//! Modifier 的规范化（canonical）序列化形态。
//!
//! `parsed_mods.json` 的 `mods[]` 必须 byte-stable，且能让运行时（D-T8）无损
//! 还原为 `Vec<Modifier>` 供热路径直接注入。pobr-core 的 `Modifier`/`ModTag`
//! 当前**未派生 Serialize**（把 canonical 序列化划归 B-track 的
//! `mod_parser/canonical.rs`，尚未交付）。本工具用一份**自包含**的 canonical
//! 形态：
//!
//! - `name` / `type` / `value`：直接可序列化（StatId/ModType/ModValue 的稳定形态）；
//! - `flags` / `kw`：位掩码 u64（`ModFlags::bits()` / `KeywordFlags::bits()`），
//!   位值逐位等于 vendor `Global.lua`，跨版本稳定；
//! - `tags`：`ModTag` 的 `Debug` 表示（确定性、byte-stable）。
//!
//! `origin`（SourceId）**剔除**：precompile 阶段无构建上下文，归因由运行时注入。
//! B-track 的 `canonical.rs` 交付后，本模块改为转发到那份共享实现（禁止两套
//! 序列化），届时 `parsed_mods.json` 形态会随之统一——schema 版本号 bump 区分。

use serde::Serialize;

use pobr_core::Modifier;
use pobr_data::modifier::ModType;

/// 单条 modifier 的规范化形态。字段顺序固定，f64 走 serde_json 最短往返表示。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CanonMod {
    pub name: String,
    #[serde(rename = "type")]
    pub mod_type: ModTypeRepr,
    pub value: CanonValue,
    /// `ModFlags::bits()`（u64）。0 时省略（绝大多数 mod 无 flag）。
    #[serde(default, skip_serializing_if = "is_zero")]
    pub flags: u64,
    /// `KeywordFlags::bits()`（u64）。0 时省略。
    #[serde(default, skip_serializing_if = "is_zero")]
    pub kw: u64,
    /// `ModTag` 的确定性 Debug 串。空时省略。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

fn is_zero(v: &u64) -> bool {
    *v == 0
}

/// `ModType` 的稳定字符串形态（与 vendor form 标签一致）。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum ModTypeRepr {
    Base,
    Inc,
    More,
    Flag,
    Override,
    List,
}

impl From<ModType> for ModTypeRepr {
    fn from(t: ModType) -> Self {
        match t {
            ModType::Base => Self::Base,
            ModType::Inc => Self::Inc,
            ModType::More => Self::More,
            ModType::Flag => Self::Flag,
            ModType::Override => Self::Override,
            ModType::List => Self::List,
        }
    }
}

/// `ModValue` 的稳定形态。
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(untagged)]
pub enum CanonValue {
    Number(f64),
    Bool(bool),
    Text(String),
    NestedMods(Vec<CanonMod>),
}

impl CanonMod {
    /// 从运行时 `Modifier` 提取 canonical 形态（剔除 `source`/`origin`）。
    pub fn from_mod(m: &Modifier) -> Self {
        use pobr_core::ModValue;
        let value = match &m.value {
            ModValue::Number(n) => CanonValue::Number(*n),
            ModValue::Bool(b) => CanonValue::Bool(*b),
            ModValue::Text(t) => CanonValue::Text(t.clone()),
            ModValue::NestedMods(mods) => {
                CanonValue::NestedMods(mods.iter().map(CanonMod::from_mod).collect())
            }
        };
        let mut tags: Vec<String> = m.tags.iter().map(|t| format!("{t:?}")).collect();
        // ModTag 顺序在 parse 产出中是确定的；不重排（保留产出顺序即 vendor 语义顺序）。
        // 但去除潜在的 Debug 多空格差异：format! 输出是稳定的，无需归一。
        tags.shrink_to_fit();
        Self {
            name: m.name.as_str().to_string(),
            mod_type: m.mod_type.into(),
            value,
            flags: m.flags.bits(),
            kw: m.keyword_flags.bits(),
            tags,
        }
    }
}
