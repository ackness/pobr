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
//!   templates and names, not from a character set.

pub mod fallback;
pub mod lines;
pub mod pack;

pub use fallback::{LocaleSet, resolve_chain};
pub use lines::{LineParser, LineRenderer, LineTranslator};
pub use pack::{LocaleMeta, LocalePack, META_FILE, NameKey, NameKind, PackError, STAT_LINES_FILE};
