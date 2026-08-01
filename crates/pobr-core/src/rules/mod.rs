//! The data interpreter layer (rules) — a pure-function layer that
//! translates ingested JSON rule tables into modifiers/calc switches.
//!
//! This is the "new rules/ data interpreter layer": PoB2's hand-curated
//! logic (specialModList / ConfigOptions etc.) is split into "restricted
//! template DSL, with exceptions routed through handler_id" — entries
//! expressible with placeholder templates live as `overlay/*.json` data;
//! the small minority carrying real logic (conditional branching /
//! cross-domain reads) record only a stable `handler_id` in the data, and
//! this layer's [`registry::HandlerRegistry`] decides what to run on the
//! Rust side.
//!
//! This layer stays pure-function and zero-I/O: data is loaded by
//! pobr-gamedata and injected by pobr-build. The handler registry skeleton
//! has landed; so has [`keystone_registry`] (the defensive keystone switch
//! snapshot); config_interpreter / stat_map_engine / buff_expander /
//! special_mod and other interpreters land in later phases.

pub mod buff_expander;
pub mod config_interpreter;
pub mod handlers;
pub mod keystone_registry;
pub mod registry;
pub mod skill_type_expr;
pub mod special_mod;
pub mod stat_map_engine;
pub mod value_expr;

pub use handlers::register_special_handlers;
pub use keystone_registry::DefenceKeystones;
pub use registry::{
    DuplicateHandlerError, Handler, HandlerCtx, HandlerOutcome, HandlerRegistry, MainSkillCtx,
};
pub use special_mod::{
    SpecialCompileError, SpecialMatch, SpecialModRules, flag_name_is_mappable,
    keyword_flag_name_is_mappable, tag_is_mappable,
};
