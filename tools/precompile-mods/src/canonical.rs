//! Canonical serialization form for `Modifier`.
//!
//! `mods[]` in `parsed_mods.json` must be byte-stable, and the runtime
//! (D-T8) must be able to reconstruct `Vec<Modifier>` losslessly from it for
//! direct hot-path injection. `pobr-core`'s `Modifier`/`ModTag` currently
//! **don't derive Serialize** (canonical serialization is slated for
//! B-track's `mod_parser/canonical.rs`, not yet delivered). This tool uses
//! its own **self-contained** canonical form instead:
//!
//! - `name` / `type` / `value`: directly serializable (stable forms of
//!   StatId/ModType/ModValue);
//! - `flags` / `kw`: u64 bitmasks (`ModFlags::bits()` / `KeywordFlags::bits()`),
//!   bit-for-bit identical to vendor `Global.lua`, stable across versions;
//! - `tags`: the `Debug` representation of `ModTag` (deterministic, byte-stable).
//!
//! `origin` (SourceId) is **dropped**: precompile has no build context, so
//! attribution is injected by the runtime instead. Once B-track's
//! `canonical.rs` lands, this module should forward to that shared
//! implementation (no two parallel serializations) — `parsed_mods.json`'s
//! shape will change accordingly, distinguished by a schema version bump.

use serde::Serialize;

use pobr_core::Modifier;
use pobr_data::modifier::ModType;

/// Canonical form of one modifier. Fixed field order; f64 uses serde_json's
/// shortest round-trip representation.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CanonMod {
    pub name: String,
    #[serde(rename = "type")]
    pub mod_type: ModTypeRepr,
    pub value: CanonValue,
    /// `ModFlags::bits()` (u64). Omitted when zero (most mods have no flags).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub flags: u64,
    /// `KeywordFlags::bits()` (u64). Omitted when zero.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub kw: u64,
    /// Deterministic `Debug` string of `ModTag`. Omitted when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

fn is_zero(v: &u64) -> bool {
    *v == 0
}

/// Stable string form of `ModType` (matches the vendor's form tags).
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

/// Stable form of `ModValue`.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(untagged)]
pub enum CanonValue {
    Number(f64),
    Bool(bool),
    Text(String),
    NestedMods(Vec<CanonMod>),
}

impl CanonMod {
    /// Extract the canonical form from a runtime `Modifier` (drops `source`/`origin`).
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
        // ModTag order is deterministic in the parse output; keep it as-is
        // (it reflects vendor semantic order). No normalization needed for
        // Debug whitespace since format! output is already stable.
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
