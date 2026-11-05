//! One locale's game-text package, as published by the translation repository
//! under `i18n/<code>/`.
//!
//! The crate never reads files: a caller passes the files' contents
//! ([`LocalePack::from_files`]) or values it has already loaded
//! ([`LocalePack::set_stat_lines`] / [`LocalePack::set_names`] /
//! [`LocalePack::set_meta`]), the way `GameData::from_memory` takes bytes.
//! Nothing here is specific to one language: the code is any locale code and
//! every table is keyed by a stable id or by the English canonical text.

use std::collections::BTreeMap;
use std::fmt;

use pobr_data::catalog::StatLineTemplate;
use serde::{Deserialize, Serialize};

/// The pack's descriptor file. Its known fields are in [`LocaleMeta`]; the
/// rest (provenance, counts) is kept by the publisher and ignored here.
pub const META_FILE: &str = "_meta.json";

/// The mod-line template pairs (`[{src, en}]`).
pub const STAT_LINES_FILE: &str = "stat_lines.json";

/// The fields of `_meta.json` the engine uses.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocaleMeta {
    /// Locales to consult, in order, for text this pack lacks (for example
    /// `["zh-CN"]` for a `zh-TW` pack). English is the implicit end of every
    /// chain: a lookup that no pack answers keeps the English text.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fallback: Vec<String>,
    /// Whether the locale is written in English's (ASCII Latin) script, so
    /// a line of it may contain no letter English lacks. Such a locale's
    /// lines are also recognized by matching its templates and names (see
    /// [`crate::LineTranslator::is_source_text`]); otherwise only a letter
    /// English never uses marks a line as the locale's.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub shares_english_script: bool,
}

/// A name table of a locale pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NameKind {
    /// Item bases: base item id (`Metadata/Items/...`) -> name.
    BaseItem,
    /// Active skills: active skill id -> name.
    Skill,
    /// Affix names (prefix/suffix, from the Mods table): English -> name.
    Affix,
    /// Passive node names: English -> name.
    PassiveNode,
    /// Proper nouns from the Words table (unique items and the like): English -> name.
    Word,
    /// Words composing rare item names: English -> name.
    RareWord,
}

/// What a [`NameKind`] table is keyed by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameKey {
    /// A stable game id.
    Id,
    /// The English canonical text.
    English,
}

impl NameKind {
    /// Every name table, in a fixed order.
    pub const ALL: [NameKind; 6] = [
        NameKind::BaseItem,
        NameKind::Skill,
        NameKind::Affix,
        NameKind::PassiveNode,
        NameKind::Word,
        NameKind::RareWord,
    ];

    /// The table's file name inside the pack.
    pub fn file_name(self) -> &'static str {
        match self {
            NameKind::BaseItem => "base_items.json",
            NameKind::Skill => "skills.json",
            NameKind::Affix => "mods.json",
            NameKind::PassiveNode => "passive_names.json",
            NameKind::Word => "words.json",
            NameKind::RareWord => "rare_words.json",
        }
    }

    /// What the table's keys are.
    pub fn key(self) -> NameKey {
        match self {
            NameKind::BaseItem | NameKind::Skill => NameKey::Id,
            NameKind::Affix | NameKind::PassiveNode | NameKind::Word | NameKind::RareWord => {
                NameKey::English
            }
        }
    }

    /// The table stored in `file`, if it is one.
    pub fn from_file_name(file: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.file_name() == file)
    }

    /// The kind's stable name in APIs (`base_item`, `skill`, `affix`,
    /// `passive_node`, `word`, `rare_word`).
    pub fn id(self) -> &'static str {
        match self {
            NameKind::BaseItem => "base_item",
            NameKind::Skill => "skill",
            NameKind::Affix => "affix",
            NameKind::PassiveNode => "passive_node",
            NameKind::Word => "word",
            NameKind::RareWord => "rare_word",
        }
    }

    /// The kind named `id` (see [`Self::id`]).
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.id() == id)
    }
}

/// Why a locale pack could not be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackError {
    /// The locale code is empty or contains characters other than ASCII
    /// letters, digits, `-` and `_`.
    InvalidCode(String),
    /// A file the engine reads does not have the expected JSON shape.
    InvalidFile {
        /// The file name inside the pack.
        file: String,
        /// The decoder's message.
        message: String,
    },
}

impl fmt::Display for PackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PackError::InvalidCode(code) => write!(f, "invalid locale code {code:?}"),
            PackError::InvalidFile { file, message } => {
                write!(f, "invalid locale file {file}: {message}")
            }
        }
    }
}

impl std::error::Error for PackError {}

/// One locale's game text: mod-line templates, name tables and descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalePack {
    code: String,
    meta: LocaleMeta,
    stat_lines: Vec<StatLineTemplate>,
    names: BTreeMap<NameKind, BTreeMap<String, String>>,
    unrecognized: Vec<String>,
}

fn invalid(file: &str, error: impl fmt::Display) -> PackError {
    PackError::InvalidFile {
        file: file.to_string(),
        message: error.to_string(),
    }
}

impl LocalePack {
    /// An empty pack for `code` (any language code, e.g. `zh-CN`, `ja`, `pt-BR`).
    pub fn new(code: &str) -> Result<Self, PackError> {
        let valid = !code.is_empty()
            && code
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
        if !valid {
            return Err(PackError::InvalidCode(code.to_string()));
        }
        Ok(Self {
            code: code.to_string(),
            meta: LocaleMeta::default(),
            stat_lines: Vec::new(),
            names: BTreeMap::new(),
            unrecognized: Vec::new(),
        })
    }

    /// Builds a pack from its files' contents, keyed by file name inside the
    /// locale directory (`stat_lines.json`, `base_items.json`, ...).
    ///
    /// Each known file must have its expected shape: `_meta.json` an object,
    /// `stat_lines.json` an array of `{src, en}`, a name table an object of
    /// strings. Files the engine does not read (other display tables) are
    /// accepted and listed by [`Self::unrecognized_files`]; a missing file
    /// is an empty table.
    pub fn from_files<I, K, V>(code: &str, files: I) -> Result<Self, PackError>
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<[u8]>,
    {
        let mut pack = Self::new(code)?;
        for (file, bytes) in files {
            let (file, bytes) = (file.as_ref(), bytes.as_ref());
            if file == META_FILE {
                pack.meta = serde_json::from_slice(bytes).map_err(|e| invalid(file, e))?;
            } else if file == STAT_LINES_FILE {
                pack.stat_lines = serde_json::from_slice(bytes).map_err(|e| invalid(file, e))?;
            } else if let Some(kind) = NameKind::from_file_name(file) {
                let table = serde_json::from_slice(bytes).map_err(|e| invalid(file, e))?;
                pack.names.insert(kind, table);
            } else {
                pack.unrecognized.push(file.to_string());
            }
        }
        pack.unrecognized.sort();
        Ok(pack)
    }

    /// Replaces the descriptor.
    pub fn set_meta(&mut self, meta: LocaleMeta) {
        self.meta = meta;
    }

    /// Replaces the mod-line templates.
    pub fn set_stat_lines(&mut self, templates: Vec<StatLineTemplate>) {
        self.stat_lines = templates;
    }

    /// Replaces one name table.
    pub fn set_names(&mut self, kind: NameKind, table: BTreeMap<String, String>) {
        self.names.insert(kind, table);
    }

    /// The locale code.
    pub fn code(&self) -> &str {
        &self.code
    }

    /// The descriptor.
    pub fn meta(&self) -> &LocaleMeta {
        &self.meta
    }

    /// The locales this pack falls back to, as it declares them.
    pub fn fallback(&self) -> &[String] {
        &self.meta.fallback
    }

    /// The mod-line template pairs (`src` localized, `en` canonical English).
    pub fn stat_lines(&self) -> &[StatLineTemplate] {
        &self.stat_lines
    }

    /// One name table (empty when the pack has none).
    pub fn names(&self, kind: NameKind) -> &BTreeMap<String, String> {
        static EMPTY: BTreeMap<String, String> = BTreeMap::new();
        self.names.get(&kind).unwrap_or(&EMPTY)
    }

    /// The localized name for `key` (an id or English text, per [`NameKind::key`]).
    pub fn name(&self, kind: NameKind, key: &str) -> Option<&str> {
        self.names(kind).get(key).map(String::as_str)
    }

    /// Files passed to [`Self::from_files`] that the engine does not read.
    pub fn unrecognized_files(&self) -> &[String] {
        &self.unrecognized
    }

    /// Whether the pack has no templates and no names.
    pub fn is_empty(&self) -> bool {
        self.stat_lines.is_empty() && self.names.values().all(BTreeMap::is_empty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_known_files_and_lists_the_rest() {
        let pack = LocalePack::from_files(
            "xx-XX",
            [
                (
                    "_meta.json",
                    br#"{"fallback":["yy"],"source":"ignored"}"#.as_slice(),
                ),
                (
                    "stat_lines.json",
                    br#"[{"src":"{0} vie","en":"{0} life"}]"#.as_slice(),
                ),
                ("words.json", br#"{"Doom":"Fato"}"#.as_slice()),
                ("display_texts.json", br#"{"schema":"x"}"#.as_slice()),
            ],
        )
        .unwrap();
        assert_eq!(pack.code(), "xx-XX");
        assert_eq!(pack.fallback(), ["yy"]);
        assert_eq!(pack.stat_lines().len(), 1);
        assert_eq!(pack.name(NameKind::Word, "Doom"), Some("Fato"));
        assert_eq!(pack.name(NameKind::BaseItem, "Doom"), None);
        assert_eq!(pack.unrecognized_files(), ["display_texts.json"]);
        assert!(!pack.is_empty());
    }

    #[test]
    fn rejects_misshapen_files_and_codes() {
        let error =
            LocalePack::from_files("xx", [("words.json", br#"["Doom"]"#.as_slice())]).unwrap_err();
        assert!(matches!(error, PackError::InvalidFile { ref file, .. } if file == "words.json"));
        let error =
            LocalePack::from_files("xx", [("stat_lines.json", br#"[{"src":"a"}]"#.as_slice())])
                .unwrap_err();
        assert!(error.to_string().contains("stat_lines.json"), "{error}");
        assert!(
            LocalePack::from_files(
                "xx",
                [("_meta.json", br#"{"fallback":"zh-CN"}"#.as_slice())]
            )
            .is_err()
        );
        assert_eq!(
            LocalePack::new("../zh").unwrap_err(),
            PackError::InvalidCode("../zh".into())
        );
        assert!(LocalePack::new("").is_err());
    }

    #[test]
    fn name_kinds_round_trip_their_file_names() {
        for kind in NameKind::ALL {
            assert_eq!(NameKind::from_file_name(kind.file_name()), Some(kind));
            assert_eq!(NameKind::from_id(kind.id()), Some(kind));
        }
        assert_eq!(NameKind::from_id("base_items"), None);
        assert_eq!(NameKind::BaseItem.key(), NameKey::Id);
        assert_eq!(NameKind::Word.key(), NameKey::English);
    }
}
