//! Defence side: armour / evasion / ES / EHP / keystones / recovery / resistance / survivability.
//!
//! Aggregated test binary: the former standalone test files are merged into submodules
//! to cut the number of linked binaries (53→8) and speed up builds. Each submodule is
//! `tests/defence/<name>.rs`, with test cases and assertions preserved as-is.
#![allow(clippy::all)]

#[path = "support/parse.rs"]
mod support;

#[path = "defence/defence_ext.rs"]
mod defence_ext;
#[path = "defence/defence_panels.rs"]
mod defence_panels;
#[path = "defence/ehp.rs"]
mod ehp;
#[path = "defence/ehp_pob2.rs"]
mod ehp_pob2;
#[path = "defence/evade_stun.rs"]
mod evade_stun;
#[path = "defence/keystone_defence.rs"]
mod keystone_defence;
#[path = "defence/recovery.rs"]
mod recovery;
#[path = "defence/resistance_cap.rs"]
mod resistance_cap;
#[path = "defence/stat_boundary.rs"]
mod stat_boundary;
#[path = "defence/survivability.rs"]
mod survivability;
