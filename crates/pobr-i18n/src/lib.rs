//! pobr-i18n: the language-neutral game-text engine.
//!
//! Game text (names, mod lines) comes from the translation repository's locale
//! packages; this crate interprets them for any locale code and never embeds
//! a translation. Calculation keeps using stable ids and English canonical
//! lines; translation happens only at the input and display boundaries.
//!
//! - [`LocalePack`]: one locale's files, passed in as bytes or parsed values
//!   (no file I/O here; the caller loads them, as with `GameData::from_memory`).
//! - [`fallback`]: chains declared by each pack's `_meta.json` (`fallback`)
//!   and extended by the caller; English ends every chain.
//! - [`LocalePack::name`] / [`LocaleSet::name`]: names by stable id or
//!   English text ([`NameKind`]).
//! - [`lines`]: [`LineParser`] (localized line -> English canonical line, for
//!   the English parser) and [`LineRenderer`] (English line -> localized
//!   display). Whether a line is in the locale is decided from the pack's
//!   templates and names (letters only that side uses, or, for a pack that
//!   declares `shares_english_script`, its template/name spelling), not
//!   from a fixed character set.
//!
//! Known limits (follow-ups for the locale package format):
//!
//! - Magic and rare item names are composed by the zh-CN client's
//!   conventions: magic = suffix + prefix + base with no separator, rare =
//!   the two words joined by a space (`LineTranslator`'s magic/rare name
//!   fallbacks). Another locale needs a composition rule declared by its pack.

pub mod fallback;
pub mod lines;
pub mod pack;

pub use fallback::{LocaleSet, chain_codes, resolve_chain};
pub use lines::{LineParser, LineRenderer, LineTranslator};
pub use pack::{LocaleMeta, LocalePack, META_FILE, NameKey, NameKind, PackError, STAT_LINES_FILE};
/// Who wrote a text that is not official (re-exported from `pobr-data`).
pub use pobr_data::catalog::TextOrigin;
