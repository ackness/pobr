//! Skill / DPS / mechanics end-to-end tests (damage, dual wield, cost, minions, reservation, quality, gating, corpse explosion).
//!
//! Aggregated test binary: originally separate test files, merged into submodules (22→4) to cut the number of linked test binaries and speed up builds.
#![allow(clippy::all)]

/// One immutable snapshot per test process. Tests that alter fixture data clone it
/// explicitly so parallel cases cannot affect each other. libtest reuses this
/// across cases, including in the full Cargo gate.
fn test_data() -> &'static pobr_build::BuildData {
    static DATA: std::sync::LazyLock<pobr_build::BuildData> = std::sync::LazyLock::new(|| {
        pobr_build::BuildData::load(&pobr_gamedata::GameData::new(
            pobr_gamedata::current_data_dir(),
        ))
        .expect("load current repository game data")
    });
    &DATA
}

#[path = "skills/volatility_events.rs"]
mod volatility_events;

#[path = "skills/cadiro_arrows.rs"]
mod cadiro_arrows;

#[path = "skills/archon.rs"]
mod archon;
#[path = "skills/prevented_recoup.rs"]
mod prevented_recoup;

#[path = "skills/aura_gems.rs"]
mod aura_gems;
#[path = "skills/bear_form.rs"]
mod bear_form;
#[path = "skills/blind.rs"]
mod blind;
#[path = "skills/bonded_modifiers.rs"]
mod bonded_modifiers;
#[path = "skills/buff_stack_limits.rs"]
mod buff_stack_limits;
#[path = "skills/charge_consuming_buffs.rs"]
mod charge_consuming_buffs;
#[path = "skills/charge_durations.rs"]
mod charge_durations;
#[path = "skills/charge_regulation.rs"]
mod charge_regulation;
#[path = "skills/charm_slots.rs"]
mod charm_slots;
#[path = "skills/combined_dps.rs"]
mod combined_dps;
#[path = "skills/conditional_offence.rs"]
mod conditional_offence;
#[path = "skills/config_handlers.rs"]
mod config_handlers;
#[path = "skills/configured_lines.rs"]
mod configured_lines;
#[path = "skills/corpse_explosion.rs"]
mod corpse_explosion;
#[path = "skills/corrupted_gems.rs"]
mod corrupted_gems;
#[path = "skills/cost_conversion.rs"]
mod cost_conversion;
#[path = "skills/cost_multiplier.rs"]
mod cost_multiplier;
#[path = "skills/curse_copies.rs"]
mod curse_copies;
#[path = "skills/dual_wield.rs"]
mod dual_wield;
#[path = "skills/enemy_config.rs"]
mod enemy_config;
#[path = "skills/enemy_debuffs.rs"]
mod enemy_debuffs;
#[path = "skills/energy_triggers.rs"]
mod energy_triggers;
#[path = "skills/equipment_affix_effects.rs"]
mod equipment_affix_effects;
#[path = "skills/equipment_conditions.rs"]
mod equipment_conditions;
#[path = "skills/event_triggered_hits.rs"]
mod event_triggered_hits;
#[path = "skills/flame_wall.rs"]
mod flame_wall;
#[path = "skills/gem_quality.rs"]
mod gem_quality;
#[path = "skills/grand_spectrum.rs"]
mod grand_spectrum;
#[path = "skills/granted_skill_reservation.rs"]
mod granted_skill_reservation;
#[path = "skills/item_bases.rs"]
mod item_bases;
#[path = "skills/limbs.rs"]
mod limbs;
#[path = "skills/mark_effect.rs"]
mod mark_effect;
#[path = "skills/minion_curses.rs"]
mod minion_curses;
#[path = "skills/minions.rs"]
mod minions;
#[path = "skills/no_damage.rs"]
mod no_damage;
#[path = "skills/player_debuffs.rs"]
mod player_debuffs;
#[path = "skills/quarterstaff_conditions.rs"]
mod quarterstaff_conditions;
#[path = "skills/rain.rs"]
mod rain;
#[path = "skills/runic_ward.rs"]
mod runic_ward;
#[path = "skills/shield_block.rs"]
mod shield_block;
#[path = "skills/skill_damage_dps.rs"]
mod skill_damage_dps;
#[path = "skills/skill_metrics.rs"]
mod skill_metrics;
#[path = "skills/spell_hit_rates.rs"]
mod spell_hit_rates;
#[path = "skills/spirit_reservation.rs"]
mod spirit_reservation;
#[path = "skills/stalking_panther.rs"]
mod stalking_panther;
#[path = "skills/support_gating.rs"]
mod support_gating;
#[path = "skills/support_gem_count.rs"]
mod support_gem_count;
#[path = "skills/support_granted_skills.rs"]
mod support_granted_skills;
#[path = "skills/totem_skills.rs"]
mod totem_skills;
#[path = "skills/umbral_well.rs"]
mod umbral_well;
#[path = "skills/weapon_accuracy_damage.rs"]
mod weapon_accuracy_damage;
#[path = "skills/weapon_sets.rs"]
mod weapon_sets;
#[path = "skills/wind_ground.rs"]
mod wind_ground;
#[path = "skills/withered.rs"]
mod withered;

#[path = "skills/passive_jewels.rs"]
mod passive_jewels;

#[path = "skills/minion_reservation.rs"]
mod minion_reservation;

#[path = "skills/jewel_sources.rs"]
mod jewel_sources;

// What the calculation did with each line of a build.
#[path = "skills/line_usage.rs"]
mod line_usage;
#[path = "skills/uhtred_omen.rs"]
mod uhtred_omen;

#[path = "skills/channel.rs"]
mod channel;
#[path = "skills/full_dps.rs"]
mod full_dps;
#[path = "skills/unique_lines.rs"]
mod unique_lines;

#[path = "skills/attribute_requirements.rs"]
mod attribute_requirements;
#[path = "skills/effect_ordinal.rs"]
mod effect_ordinal;
#[path = "skills/unique_lines_2.rs"]
mod unique_lines_2;

#[path = "skills/unique_lines_3.rs"]
mod unique_lines_3;

#[path = "skills/minion_counts.rs"]
mod minion_counts;

#[path = "skills/cost_rate.rs"]
mod cost_rate;
#[path = "skills/limits_reservation.rs"]
mod limits_reservation;
#[path = "skills/party.rs"]
mod party;
#[path = "skills/sidebar_remaining.rs"]
mod sidebar_remaining;

#[path = "skills/combo.rs"]
mod combo;
#[path = "skills/remnant_pickup.rs"]
mod remnant_pickup;
