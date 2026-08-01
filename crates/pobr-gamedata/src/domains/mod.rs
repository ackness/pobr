//! Per-domain loaders: W2's nine constant tables (`base/`) + W4d's two
//! small lookup tables (`overlay/`), corresponding to each domain's schema
//! in `pobr_data::catalog`.
//!
//! Each submodule implements its own loading method on `GameData` (base
//! domains resolve via `base/` first, falling back to the version root;
//! overlay domains always resolve under `overlay/`).

pub mod base_player_mods;
pub mod character_constants;
pub mod enemy_presets;
pub mod game_constants;
pub mod jewel_radii;
pub mod monster_scaling;
pub mod non_damaging_ailments;
pub mod unarmed_data;
pub mod weapon_types;

// Small lookup tables (overlay layer)
pub mod high_precision_mods;
pub mod local_mods;

// Per-skill override values (overlay layer, loader + a dedicated merge)
pub mod skill_overrides;

// Gem quality-stat slopes (overlay layer, a plain lookup table)
pub mod gem_quality_stats;

// SkillStatMap mapping table (overlay layer, consumer = stat_map_engine)
pub mod skill_stat_map;

// Gem → granted-effect edges (overlay layer, merged into skill_gems + the meta expansion index)
pub mod gem_effects;

// statSet label / vendor export index sidecar (overlay layer, merged into stat sets)
pub mod stat_set_labels;

// Base item overrides (overlay layer, loader + a dedicated merge: block/spirit)
pub mod base_item_overrides;

//  Config options catalog + built-in buff definitions (overlay layer, plain lookup tables)
pub mod buff_definitions;
pub mod config_options;

// curse priority data table (overlay layer, a plain lookup table; consumer = buff_pass)
pub mod curse_priority;

// Data prerequisites (overlay layer, a plain loader with zero wiring; one table per file)
pub mod catalysts; // M5c: catalyst quality-tag matching table
pub mod granted_effect_minions; // M5a: gem → minion foreign-key sidecar
pub mod minions; // M5a: minion entries
pub mod mirage_configs; // M5a-D2: mirage configs
pub mod mod_scalability; // M5c: {range:x} scalability table
pub mod runes; // M5c: rune/soul-core mod table
pub mod special_mods; // M5b: special mod-line templates
pub mod spectres; // M5a: spectre entries
pub mod uniques; // M5c: unique items, raw+index two-layer

//  The ModParser rule six-table set (overlay layer, consumer = the mod_parser scan engine)
pub mod parser_rules;

//  61 trigger configs (overlay layer, consumer = the orchestrator's trigger section)
pub mod trigger_configs;
