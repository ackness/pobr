//! pobr-gamedata: runtime loader for the stored, adapted JSON
//! (`data/<poe_version>/`).
//!
//! This is the data system's **sole layer that holds file I/O** —
//! `pobr-data` (pure definitions) and `pobr-core` (pure calc) both stay
//! zero-I/O. This crate uses serde to deserialize `data/<version>/`'s
//! minimal JSON into [`pobr_data::catalog`] types, for the layers above to
//! consume as needed.
//!
//! Module breakdown:
//! - [`manifest`]: manifest v1/v2 loading;
//! - [`paths`]: locates a domain's file across the three directory layers
//!   (`base/` first, falling back to the version root for compatibility
//!   with the old layout);
//! - [`overlay`]: the deterministic base→overlay merge engine;
//! - [`ruleset`]: the `RuleSet` aggregation entry-point skeleton (for
//!   pobr-build to inject into pobr-core);
//! - [`domains`]: per-domain loaders for the nine tables (currently empty
//!   shells).

pub mod domains;
pub mod manifest;
pub mod overlay;
pub mod paths;
pub mod ruleset;
pub mod test_pins;

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use pobr_data::catalog::{
    BaseItemDef, CostTypeDef, GrantedEffectDef, ModDef, PassiveNodeDef, PassiveTreeMeta,
    SkillGemDef, SkillLevelDef, SkillStatSetDef, StatDef,
};

pub use overlay::{MergeError, merge};
pub use ruleset::RuleSet;

/// A load error.
#[derive(Debug)]
pub enum LoadError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
    /// An overlay merge failed (e.g. `skill_overrides.json` names a stat
    /// the consumer hasn't wired up).
    Overlay { path: PathBuf, message: String },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "failed to read {}: {source}", path.display()),
            Self::Parse { path, source } => {
                write!(f, "failed to parse {}: {source}", path.display())
            }
            Self::Overlay { path, message } => {
                write!(f, "failed to apply overlay {}: {message}", path.display())
            }
        }
    }
}

impl std::error::Error for LoadError {}

/// A loader pointed at a PoE2 version's data directory
/// (`data/<poe_version>/`).
///
/// Two backends:
/// - **filesystem** ([`GameData::new`]): `root` points at an on-disk
///   version directory, the default;
/// - **in-memory** ([`GameData::from_memory`]): every file is injected up
///   front by the caller as `relative path -> bytes`, with zero file I/O
///   after that — for filesystem-less environments like wasm (the JS side
///   fetches the data files and passes them in).
#[derive(Debug, Clone)]
pub struct GameData {
    root: PathBuf,
    /// The in-memory file table (`Some` = the in-memory backend). Keys are
    /// paths relative to the version directory, always using forward
    /// slashes (e.g. `base/stats.json`, `overlay/uniques.json`).
    files: Option<std::sync::Arc<std::collections::BTreeMap<String, Vec<u8>>>>,
}

impl GameData {
    /// Points at a version directory, e.g. `data/4.5.0.3.4`.
    pub fn new(version_dir: impl Into<PathBuf>) -> Self {
        Self {
            root: version_dir.into(),
            files: None,
        }
    }

    /// The in-memory backend: `files` maps a path relative to the version
    /// directory (forward slashes) to its file contents.
    ///
    /// Every domain load afterward queries this table instead of touching
    /// the filesystem; a file missing from the table has the same
    /// semantics as a file missing from disk ([`LoadError::Io`]'s
    /// NotFound — the consumer's missing-table degradation still applies).
    pub fn from_memory(files: std::collections::BTreeMap<String, Vec<u8>>) -> Self {
        Self {
            root: PathBuf::from("<memory>"),
            files: Some(std::sync::Arc::new(files)),
        }
    }

    /// Reduces a path (possibly carrying the `root` prefix) to an
    /// in-memory table key.
    fn memory_key(&self, path: &Path) -> String {
        let rel = path.strip_prefix(&self.root).unwrap_or(path);
        rel.to_string_lossy().replace('\\', "/")
    }

    /// Reads a data file's bytes (dispatched by backend).
    pub(crate) fn read_bytes(&self, path: &Path) -> Result<Vec<u8>, std::io::Error> {
        match &self.files {
            Some(map) => map.get(&self.memory_key(path)).cloned().ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, "file not in memory data")
            }),
            None => fs::read(path),
        }
    }

    /// Checks whether a data file exists (dispatched by backend; used by
    /// `domain_path` / the patch layer's probing).
    pub(crate) fn file_exists(&self, path: &Path) -> bool {
        match &self.files {
            Some(map) => map.contains_key(&self.memory_key(path)),
            None => path.is_file(),
        }
    }

    /// The version directory root (where `manifest.json` / `i18n/` live).
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    /// Loads JSON from an absolute/already-resolved path, and layers the
    /// **user patch layer** on top of it (if present).
    ///
    /// A user patch: put custom JSON under
    /// `data/<version>/patch/<path relative to the version root, mirroring its structure>`
    /// (e.g. `patch/base/mods.json`, `patch/overlay/uniques.json`); at load
    /// time it's layered on top of the official data per [`merge`]'s rules
    /// (object keys override / arrays merge by `id` / scalars override).
    /// This is a **user-facing extension layer** on top of base→overlay —
    /// adding a JSON file is enough to add/change a mod, exclusivity, or
    /// config, without touching code or official data; a missing patch
    /// directory means pure official data (backward compatible).
    pub(crate) fn load_json_at<T: for<'de> serde::Deserialize<'de>>(
        &self,
        path: PathBuf,
    ) -> Result<T, LoadError> {
        let bytes = self.read_bytes(&path).map_err(|source| LoadError::Io {
            path: path.clone(),
            source,
        })?;
        // User patch: patch/<path relative to the version root>; mirrored by
        // relative structure to avoid filename collisions (both base/ and
        // i18n/ have a base_items.json).
        if let Ok(rel) = path.strip_prefix(&self.root) {
            let patch_path = self.root.join("patch").join(rel);
            if self.file_exists(&patch_path) {
                let base_val: serde_json::Value =
                    serde_json::from_slice(&bytes).map_err(|source| LoadError::Parse {
                        path: path.clone(),
                        source,
                    })?;
                let patch_bytes = self
                    .read_bytes(&patch_path)
                    .map_err(|source| LoadError::Io {
                        path: patch_path.clone(),
                        source,
                    })?;
                let patch_val: serde_json::Value =
                    serde_json::from_slice(&patch_bytes).map_err(|source| LoadError::Parse {
                        path: patch_path.clone(),
                        source,
                    })?;
                let merged = merge(base_val, patch_val).map_err(|e| LoadError::Overlay {
                    path: patch_path,
                    message: e.to_string(),
                })?;
                return serde_json::from_value(merged)
                    .map_err(|source| LoadError::Parse { path, source });
            }
        }
        serde_json::from_slice(&bytes).map_err(|source| LoadError::Parse { path, source })
    }

    /// Loads a data domain's JSON (`base/` first, falling back to the
    /// version root, see [`paths`]).
    fn load_domain<T: for<'de> serde::Deserialize<'de>>(&self, rel: &str) -> Result<T, LoadError> {
        self.load_json_at(self.domain_path(rel))
    }

    /// Loads base item definitions (English canonical names), and merges
    /// `overlay/base_item_overrides.json`'s base overrides (a shield's
    /// `block_chance` / a sceptre's `spirit` — the `.dat` table's bundle
    /// was pruned by the CDN, so this falls back to vendor
    /// `Data/Bases`-extracted data) onto the plain base data (absent
    /// overlay = plain base, see [`domains::base_item_overrides`]).
    pub fn base_items(&self) -> Result<Vec<BaseItemDef>, LoadError> {
        let mut bases: Vec<BaseItemDef> = self.load_domain("base_items.json")?;
        if let Some(overrides) = self.base_item_overrides()? {
            domains::base_item_overrides::apply_base_item_overrides(&mut bases, &overrides);
        }
        Ok(bases)
    }

    /// Loads the item base-name sidecar for a language (`id -> localized name`).
    pub fn base_item_names(
        &self,
        lang: &str,
    ) -> Result<std::collections::BTreeMap<String, String>, LoadError> {
        self.load_json_at(self.root.join(format!("i18n/{lang}/base_items.json")))
    }

    /// Loads a word-for-word translation sidecar for a language
    /// (`i18n/<lang>/<file>`, English name → localized name). Returns an
    /// empty map when the file is missing (an old data pack) — the
    /// consumer degrades to "no word translation".
    fn name_sidecar(
        &self,
        lang: &str,
        file: &str,
    ) -> Result<std::collections::BTreeMap<String, String>, LoadError> {
        let path = self.root.join(format!("i18n/{lang}/{file}"));
        match self.load_json_at(path) {
            Ok(map) => Ok(map),
            Err(LoadError::Io { ref source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                Ok(Default::default())
            }
            Err(e) => Err(e),
        }
    }

    /// Word-for-word translation table (transcribed from GGG's Words
    /// table: unique item names and other proper nouns).
    pub fn word_names(
        &self,
        lang: &str,
    ) -> Result<std::collections::BTreeMap<String, String>, LoadError> {
        self.name_sidecar(lang, "words.json")
    }

    /// Passive-node-name translation table (transcribed from GGG's
    /// PassiveSkills table's Name column).
    pub fn passive_node_names(
        &self,
        lang: &str,
    ) -> Result<std::collections::BTreeMap<String, String>, LoadError> {
        self.name_sidecar(lang, "passive_names.json")
    }

    /// Affix-name translation table (transcribed from GGG's Mods table's
    /// Name column; used to compose translated magic-item names from
    /// prefix + suffix).
    pub fn affix_names(
        &self,
        lang: &str,
    ) -> Result<std::collections::BTreeMap<String, String>, LoadError> {
        self.name_sidecar(lang, "mods.json")
    }

    /// Word list for composing RARE random names (prefix word/suffix word
    /// → a short localized noun; used for two-word name translation).
    pub fn rare_name_words(
        &self,
        lang: &str,
    ) -> Result<std::collections::BTreeMap<String, String>, LoadError> {
        self.name_sidecar(lang, "rare_words.json")
    }

    /// Loads the stat registry (id / is_local / semantic / category).
    pub fn stats(&self) -> Result<Vec<StatDef>, LoadError> {
        self.load_domain("stats.json")
    }

    /// Loads mod pool definitions (Stat foreign keys already resolved to
    /// stable stat ids, roll ranges already merged in).
    pub fn mods(&self) -> Result<Vec<ModDef>, LoadError> {
        self.load_domain("mods.json")
    }

    /// Loads the StatDescriptions overlay (stat_id → canonical English
    /// template lines, `overlay/stat_descriptions.json`). Returns `Ok(None)`
    /// when the overlay is missing (an old data pack) — consumers (mod
    /// tier inference, etc.) degrade to "no template index".
    pub fn stat_descriptions(
        &self,
    ) -> Result<Option<pobr_data::catalog::stat_descriptions::StatDescriptionsDef>, LoadError> {
        let path = self.overlay_path("stat_descriptions.json");
        if !self.file_exists(&path) {
            return Ok(None);
        }
        self.load_json_at(path).map(Some)
    }

    /// Loads the mod-name sidecar for a language (`id -> localized name`).
    pub fn mod_names(
        &self,
        lang: &str,
    ) -> Result<std::collections::BTreeMap<String, String>, LoadError> {
        self.load_json_at(self.root.join(format!("i18n/{lang}/mods.json")))
    }

    /// Loads skill gem definitions (identity taken from the base id), and
    /// merges `overlay/gem_effects.json`'s gem → granted-effect edges
    /// (`granted_effect_id` / `additional_granted_effect_ids`, extracted
    /// from vendor `Data/Gems.lua` — the `.dat`'s `GemEffects` table isn't
    /// downloadable) onto the plain base data by `gem_id` (absent overlay
    /// = plain base, with the edge fields left empty).
    pub fn skill_gems(&self) -> Result<Vec<SkillGemDef>, LoadError> {
        let mut gems: Vec<SkillGemDef> = self.load_domain("skill_gems.json")?;
        if let Some(effects) = self.gem_effects()? {
            let by_gem: std::collections::BTreeMap<&str, &pobr_data::catalog::GemEffectDef> =
                effects
                    .gems
                    .iter()
                    .map(|g| (g.gem_id.as_str(), g))
                    .collect();
            for gem in &mut gems {
                if let Some(link) = by_gem.get(gem.id.as_str()) {
                    gem.granted_effect_id = Some(link.granted_effect_id.clone());
                    gem.additional_granted_effect_ids = link.additional_granted_effect_ids.clone();
                }
                // The gem isn't in the overlay (e.g. a monster-only or
                // deprecated gem) → the edge fields stay empty, no error.
            }
        }
        Ok(gems)
    }

    /// Loads granted-effect definitions (including the resolved
    /// active-skill link plus the StatSet/CostTypes indices).
    ///
    /// At load time, merges `overlay/granted_effect_minions.json` (the
    /// gem → minion foreign-key sidecar) onto the base data: matched by
    /// `effect_id`, folding in `minion_list` / `add_minion_list` /
    /// `minion_uses` / `minion_has_item_set` (the base
    /// `granted_effects.json` doesn't have these fields; a missing overlay
    /// file means they're all empty, backward compatible).
    pub fn granted_effects(&self) -> Result<Vec<GrantedEffectDef>, LoadError> {
        let mut effects: Vec<GrantedEffectDef> = self.load_domain("granted_effects.json")?;
        if let Some(minions) = self.granted_effect_minions()? {
            let mut by_id: std::collections::HashMap<&str, &_> = std::collections::HashMap::new();
            for entry in &minions.entries {
                by_id.insert(entry.effect_id.as_str(), entry);
            }
            for effect in &mut effects {
                if let Some(entry) = by_id.get(effect.id.as_str()) {
                    effect.minion_list = entry.minion_list.clone();
                    effect.add_minion_list = entry.add_minion_list.clone();
                    effect.minion_uses = entry.minion_uses.clone();
                    effect.minion_has_item_set = entry.minion_has_item_set;
                }
            }
        }
        Ok(effects)
    }

    /// Loads granted effects' per-level parameters
    /// (`granted_effect_id -> ascending level array`, cost/cooldown/attack
    /// time), and merges `overlay/skill_overrides.json`'s per-level
    /// override values (crit_chance / attack_speed_multiplier /
    /// base_multiplier — extracted from vendor PoB2, columns missing from
    /// the `.dat` export) onto the plain base data (absent overlay = plain
    /// base, see [`domains::skill_overrides`]).
    pub fn granted_effect_levels(
        &self,
    ) -> Result<std::collections::BTreeMap<String, Vec<SkillLevelDef>>, LoadError> {
        let mut levels = self.load_domain("granted_effect_levels.json")?;
        if let Some(overrides) = self.skill_overrides()? {
            domains::skill_overrides::apply_level_overrides(&mut levels, &overrides).map_err(
                |message| LoadError::Overlay {
                    path: self.overlay_path("skill_overrides.json"),
                    message,
                },
            )?;
        }
        Ok(levels)
    }

    /// Loads granted effects' **multi-statSet per-level stat sets** (an
    /// array sorted by effect id, each item = the primary set plus
    /// additional sets). Returns an empty Vec when absent (an old data
    /// pack without this domain), backward compatible. Two overlays are
    /// merged onto the plain base data here:
    /// - `skill_overrides.json`'s statSet-level override values
    ///   (skill_attack_speed_more, a constant baseMod built into PoB2, not
    ///   in the GGG `.dat`);
    /// - `stat_set_labels.json`'s form label / vendor export index (the
    ///   `.dat` `Label` column's FK target table isn't downloadable, so
    ///   this is extracted from vendor);
    /// - `skill_overrides.json`'s dotIs* booleans (merged after labels,
    ///   since locating the set depends on the vendor index).
    pub fn skill_stat_sets(&self) -> Result<Vec<SkillStatSetDef>, LoadError> {
        let mut sets =
            match self.load_domain::<Vec<SkillStatSetDef>>("granted_effect_stat_sets.json") {
                Ok(v) => v,
                Err(LoadError::Io { .. }) => Vec::new(),
                Err(e) => return Err(e),
            };
        let overrides = self.skill_overrides()?;
        if let Some(overrides) = &overrides {
            domains::skill_overrides::apply_stat_set_overrides(&mut sets, overrides).map_err(
                |message| LoadError::Overlay {
                    path: self.overlay_path("skill_overrides.json"),
                    message,
                },
            )?;
        }
        if let Some(labels) = self.stat_set_labels()? {
            // (skill, set_id) → (vendor export index, label).
            let by_key: std::collections::BTreeMap<(&str, &str), (u32, &str)> = labels
                .labels
                .iter()
                .map(|l| {
                    (
                        (l.skill.as_str(), l.set_id.as_str()),
                        (l.set_index, l.label.as_str()),
                    )
                })
                .collect();
            for def in &mut sets {
                for set in &mut def.sets {
                    if let Some(&(idx, label)) =
                        by_key.get(&(def.effect_id.as_str(), set.set_id.as_str()))
                    {
                        set.vendor_set_index = Some(idx);
                        set.label = Some(label.to_string());
                    }
                    // Vendor didn't export this set (curated out by the
                    // template) → label/index stay None.
                }
            }
        }
        // The dotIs* booleans must be merged after the labels merge — set
        // lookup depends on the `vendor_set_index` just backfilled above
        // (an overlay entry's `stat_set` is vendor's statSets index).
        if let Some(overrides) = &overrides {
            domains::skill_overrides::apply_dot_flag_overrides(&mut sets, overrides).map_err(
                |message| LoadError::Overlay {
                    path: self.overlay_path("skill_overrides.json"),
                    message,
                },
            )?;
            // Implicit stats: same set-lookup semantics as dotIs* (depends
            // on the vendor index backfilled by the labels merge), so also
            // merged after labels.
            domains::skill_overrides::apply_implicit_stat_overrides(&mut sets, overrides).map_err(
                |message| LoadError::Overlay {
                    path: self.overlay_path("skill_overrides.json"),
                    message,
                },
            )?;
        }
        Ok(sets)
    }

    /// Loads the skill cost-resource-type table (ascending by index, the
    /// FK target of [`GrantedEffectDef::cost_types`]). Returns an empty Vec
    /// when absent (an old data pack without this domain), backward compatible.
    pub fn cost_types(&self) -> Result<Vec<CostTypeDef>, LoadError> {
        match self.load_domain::<Vec<CostTypeDef>>("cost_types.json") {
            Ok(v) => Ok(v),
            Err(LoadError::Io { .. }) => Ok(Vec::new()),
            Err(e) => Err(e),
        }
    }

    /// Loads the active-skill display-name sidecar for a language
    /// (`active_skill_id -> localized name`).
    pub fn skill_names(
        &self,
        lang: &str,
    ) -> Result<std::collections::BTreeMap<String, String>, LoadError> {
        self.load_json_at(self.root.join(format!("i18n/{lang}/skills.json")))
    }

    /// Loads the **mod-line input translation templates** for a language
    /// (`i18n/<lang>/stat_lines.json`, Phase 7.1: template pairs mapping a
    /// localized mod line to its English canonical form). Returns
    /// `Ok(None)` when the file is missing (that language isn't stored) —
    /// consumers degrade to "no input translation for this language";
    /// other I/O / parse errors still propagate as usual.
    pub fn stat_line_templates(
        &self,
        lang: &str,
    ) -> Result<Option<Vec<pobr_data::catalog::StatLineTemplate>>, LoadError> {
        let path = self.root.join(format!("i18n/{lang}/stat_lines.json"));
        match self.load_json_at::<Vec<pobr_data::catalog::StatLineTemplate>>(path) {
            Ok(v) => Ok(Some(v)),
            Err(LoadError::Io { ref source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    /// Loads passive tree nodes (from the adapted GGG official tree
    /// export, sorted by `skill` id).
    pub fn passive_nodes(&self) -> Result<Vec<PassiveNodeDef>, LoadError> {
        self.load_domain("passive_tree.json")
    }

    /// Loads passive tree metadata (class / ascendancy summaries).
    pub fn passive_tree_meta(&self) -> Result<PassiveTreeMeta, LoadError> {
        self.load_domain("passive_tree_meta.json")
    }

    /// Loads a historical season's tree-version node table
    /// (`base/passive_trees/<v>.json`, extracted from vendor
    /// `TreeData/<v>/tree.lua` via `pobr-data-adapter --tree-full` — a
    /// minimal mod field set: skill/name/kind/stats/ascendancy, no
    /// topology/coordinates). Returns `Ok(None)` when the file is missing
    /// (that version wasn't extracted / an old data pack) — the consumer
    /// falls back to the current default tree; other I/O / parse errors
    /// still propagate, not silenced.
    pub fn passive_nodes_versioned(
        &self,
        tree_version: &str,
    ) -> Result<Option<Vec<PassiveNodeDef>>, LoadError> {
        // The version string comes from the build XML's `<Spec treeVersion>`;
        // only a conservative character set (alphanumeric/underscore) is
        // allowed, to prevent path-concatenation injection.
        if !tree_version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return Ok(None);
        }
        let path = self
            .root()
            .join("base/passive_trees")
            .join(format!("{tree_version}.json"));
        match self.load_json_at::<Vec<PassiveNodeDef>>(path) {
            Ok(nodes) => Ok(Some(nodes)),
            Err(LoadError::Io { ref source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    /// Enumerates the historical tree versions currently stored
    /// (`base/passive_trees/*.json` filenames). A missing directory means
    /// an empty list.
    pub fn available_tree_versions(&self) -> Vec<String> {
        if let Some(map) = &self.files {
            // In-memory backend: enumerate `base/passive_trees/*.json` keys
            // (a BTreeMap is already ordered).
            return map
                .keys()
                .filter_map(|k| k.strip_prefix("base/passive_trees/"))
                .filter(|rest| !rest.contains('/'))
                .filter_map(|name| name.strip_suffix(".json").map(str::to_string))
                .collect();
        }
        let dir = self.root().join("base/passive_trees");
        let Ok(rd) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut versions: Vec<String> = rd
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let name = e.file_name().into_string().ok()?;
                name.strip_suffix(".json").map(str::to_string)
            })
            .collect();
        versions.sort();
        versions
    }
}

/// The root of the repo's built-in data directory (`<workspace>/data`).
/// Used for tests and the default load path.
pub fn repo_data_root() -> PathBuf {
    // crates/pobr-gamedata/ → two levels up is the workspace root.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data")
        .canonicalize()
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
}

/// Re-export of the compile-time default data version
/// ([`pobr_data::DATA_VERSION`]).
pub use pobr_data::DATA_VERSION;

/// Runtime data version (fully discovered at the I/O layer):
/// 1. the `POBR_DATA_VERSION` environment variable;
/// 2. the `data/CURRENT` marker file (first line, trimmed, written by the
///    update script);
/// 3. falls back to the [`pobr_data::DATA_VERSION`] compile-time constant.
///
/// This is what makes "switch versions after updating data with zero code
/// changes" work: the update script just writes `data/CURRENT`, and every
/// path that reads this function follows automatically.
pub fn data_version() -> String {
    if let Ok(v) = std::env::var("POBR_DATA_VERSION")
        && !v.trim().is_empty()
    {
        return v.trim().to_string();
    }
    if let Ok(content) = fs::read_to_string(repo_data_root().join("CURRENT"))
        && let Some(line) = content.lines().next()
        && !line.trim().is_empty()
    {
        return line.trim().to_string();
    }
    DATA_VERSION.to_string()
}

/// The currently active version's data directory
/// (`<workspace>/data/<data_version()>`).
///
/// The single runtime entry point for "which version to load" — replaces
/// the scattered `crate::current_data_dir()` calls. The version is
/// discovered by [`data_version`] (env → `data/CURRENT` → the constant).
pub fn current_data_dir() -> PathBuf {
    repo_data_root().join(data_version())
}
