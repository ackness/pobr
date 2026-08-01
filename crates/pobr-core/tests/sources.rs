//! Source ingest: item / passive / gem / flask / minion modifier ingestion.
//!
//! Aggregated test binary: the former standalone test files are merged into submodules
//! to cut the number of linked binaries (53→8) and speed up builds. Each submodule is
//! `tests/sources/<name>.rs`, with test cases and assertions preserved as-is.
#![allow(clippy::all)]

#[path = "support/parse.rs"]
mod support;

#[path = "sources/env_finalize_buffs.rs"]
mod env_finalize_buffs;
#[path = "sources/env_finalize_flasks.rs"]
mod env_finalize_flasks;
#[path = "sources/item_source.rs"]
mod item_source;
#[path = "sources/item_text.rs"]
mod item_text;
#[path = "sources/minion.rs"]
mod minion;
#[path = "sources/passive_source.rs"]
mod passive_source;
#[path = "sources/skill_source.rs"]
mod skill_source;
