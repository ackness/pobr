//! Domain-file location logic across the three data-directory layers.
//!
//! `data/<version>/` uses a three-layer physical layout: `base/` (fully
//! auto-regenerated from `.dat`), `overlay/` (vendor-extracted),
//! `generated/` (a deterministic cache). Domain-file location rule:
//! **`base/` first, falling back to the version root** — compatible with
//! the old layout not yet migrated (an old data pack puts domain JSON
//! directly at the version root). `manifest.json` and `i18n/` always live
//! at the version root and don't go through this location logic.

use std::path::PathBuf;

use crate::GameData;

impl GameData {
    /// Locates a data-domain file: uses `<root>/base/<rel>` if it exists,
    /// otherwise falls back to `<root>/<rel>`.
    ///
    /// Note the fallback path is **not checked for existence** — when the
    /// file is truly missing, the loader reports it via a path-carrying
    /// [`crate::LoadError::Io`] (the error message points at the fallback location).
    pub(crate) fn domain_path(&self, rel: &str) -> PathBuf {
        let layered = self.root().join("base").join(rel);
        if self.file_exists(&layered) {
            layered
        } else {
            self.root().join(rel)
        }
    }

    /// Locates a domain file in the **overlay layer**: always
    /// `<root>/overlay/<rel>`, no fallback to the version root (overlay is
    /// the vendor-extraction/transcription layer — the old flat layout
    /// never had overlay files, so there's no compatibility need).
    ///
    /// Like [`Self::domain_path`], **doesn't check existence** — a missing
    /// file is reported by the loader via a path-carrying
    /// [`crate::LoadError::Io`]; whether to degrade to a built-in fallback
    /// is decided per-domain by the consumer (e.g. pobr-build's `BuildData::load`).
    pub(crate) fn overlay_path(&self, rel: &str) -> PathBuf {
        self.root().join("overlay").join(rel)
    }

    /// Locates a domain file in the **generated layer**: always
    /// `<root>/generated/<rel>` (the deterministic cache layer, tool
    /// output). Same as [`Self::overlay_path`]: doesn't check existence,
    /// doesn't fall back to the version root.
    pub(crate) fn generated_path(&self, rel: &str) -> PathBuf {
        self.root().join("generated").join(rel)
    }

    /// Locates a file in the **version-independent curation layer**:
    /// `data/overlay-common/<rel>` — the sibling `overlay-common/` directory
    /// **next to** the version directory (`<root>/../overlay-common/<rel>`).
    ///
    /// Hand-curated vendor-semantics fixes that don't change with the game
    /// version go here, so new data-version directories inherit them
    /// automatically, without a per-version manual migration (see
    /// `docs/version-bump-architecture.md` P1-3). The loader merges this
    /// **underneath** the version layer's `overlay/<rel>` (the version
    /// layer overrides by entry key, everything else is appended).
    ///
    /// Returns `None` only when the version root has no parent directory
    /// (e.g. root is the filesystem root) — a normal disk/in-memory backend
    /// is always `Some`: the in-memory backend's root is `<memory>`, whose
    /// parent is empty → the key reduces to `overlay-common/<rel>`.
    pub(crate) fn overlay_common_path(&self, rel: &str) -> Option<PathBuf> {
        self.root()
            .parent()
            .map(|parent| parent.join("overlay-common").join(rel))
    }

    /// Loads a **single-object curated overlay domain**, with the version
    /// layer taking priority and the version-independent layer
    /// `overlay-common/<rel>` as the fallback: if the version's
    /// `overlay/<rel>` exists, the whole file is used (an escape hatch for
    /// version-specific fixes); otherwise reads from common (free
    /// inheritance for new version directories, see
    /// `docs/version-bump-architecture.md` P1-3). If both layers are
    /// missing → the version layer's [`LoadError::Io`] (NotFound, the path
    /// points at `overlay/`); whether to degrade further is the consumer's
    /// call. List-shaped domains (overridden/appended entry by entry by
    /// id) use [`merge_by_key`] instead of this method.
    pub(crate) fn load_overlay_or_common<T>(&self, rel: &str) -> Result<T, crate::LoadError>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        match self.load_json_at::<T>(self.overlay_path(rel)) {
            Err(crate::LoadError::Io { ref source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                match self.overlay_common_path(rel) {
                    Some(common) => self.load_json_at(common),
                    // root has no parent directory (the FS root) — no
                    // common layer to fall back to; reproduce the version
                    // layer's NotFound for the consumer.
                    None => self.load_json_at(self.overlay_path(rel)),
                }
            }
            other => other,
        }
    }
}

/// Merges two curated overlay layers indexed by a stable key: each entry of
/// `version` overrides the same-key entry of `common` by `key` (a whole-entry
/// replacement, keeping common's original position); a `version` entry
/// whose key isn't in `common` is appended at the end, in appearance order.
/// The same input always produces the same output (deterministic).
/// Hand-curated baseline entries go in common, version-specific overrides go
/// in the version layer (see `docs/version-bump-architecture.md` P1-3).
pub(crate) fn merge_by_key<T>(common: Vec<T>, version: Vec<T>, key: impl Fn(&T) -> &str) -> Vec<T> {
    let mut entries = common;
    for v in version {
        match entries.iter_mut().find(|e| key(e) == key(&v)) {
            Some(slot) => *slot = v,
            None => entries.push(v),
        }
    }
    entries
}

#[cfg(test)]
mod tests {
    use crate::GameData;

    /// Creates a temp directory with a unique name (not force-cleaned up
    /// after the test, lands under the system temp dir).
    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("pobr-gamedata-paths-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// base/ is preferred when a domain file exists under it.
    #[test]
    fn prefers_base_subdirectory() {
        let dir = temp_dir("prefer-base");
        std::fs::create_dir_all(dir.join("base")).unwrap();
        std::fs::write(dir.join("base/stats.json"), "[]").unwrap();
        std::fs::write(dir.join("stats.json"), "[]").unwrap();
        let gd = GameData::new(&dir);
        assert_eq!(gd.domain_path("stats.json"), dir.join("base/stats.json"));
    }

    /// Falls back to the version root when base/ is missing (old layout compatibility).
    #[test]
    fn falls_back_to_version_root() {
        let dir = temp_dir("fallback-root");
        std::fs::write(dir.join("stats.json"), "[]").unwrap();
        let gd = GameData::new(&dir);
        assert_eq!(gd.domain_path("stats.json"), dir.join("stats.json"));
    }
}
