//! Gem quality stat adapter (owned by Track-1).
//!
//! Originally planned output: `base/gem_quality_stats.json`
//! (`effect_id -> [{stat, per_quality_rate}]`, with rate =
//! `StatValues[i]/1000`; support gem effects are skipped — matching PoB2's
//! export conditions at `Export/Scripts/skills.lua:304-313`).
//!
//! **T1.1 decision (2026-06-11)**: the bundle containing the
//! `GrantedEffectQualityStats` table isn't in the local cache, and the CDN
//! has taken it down for the pinned patch 4.5.0.3.4 (can't be re-downloaded
//! — see `_tablesUnavailableForPinnedPatch` in `pipeline/config.json`). Per
//! the owner's call to "let the producing tool define the layer" (see the
//! audits), this domain is instead produced by the **extract-lua channel**:
//! `sync-pob-catalog extract-lua --what gem-quality` (reading the
//! `qualityStats` field from vendor `Data/Skills/*.lua`, where rate is
//! already `/1000` and support gems are already skipped per export
//! conditions) -> `data/<version>/overlay/gem_quality_stats.json` (schema =
//! the `GemQualityStatDef` section of `pobr_data::catalog::skills`).
//!
//! This module currently **produces no files**; once a version upgrade
//! re-downloads the `.dat` tables, an adapter channel should be implemented
//! here (including the Alt columns `AltStats`/`AltStatValuesPermille`/
//! `AltApplyToStatSets`, stored verbatim but not consumed, marked TODO —
//! PoB2's own export also only reads the main columns). At that point the
//! migration commit must be byte-equivalent to the overlay artifact (the data-migration invariant).
