pub mod actor;
pub mod ailment;
pub mod breakdown;
pub mod crit;
pub mod damage;
pub mod defence;
pub mod defence_panels;
pub mod ehp;
pub mod env;
pub mod env_finalize;
pub mod error;
pub mod minion;
pub mod offence;
pub mod output;
pub mod perform;
pub mod pool_damage;
pub mod pool_setup;
pub mod session;
pub mod setup_env;
pub mod skill_mechanics;
pub mod skill_use_time;
pub mod stat_boundary;
pub mod stun;
pub mod survivability;
pub mod taken;
pub mod trigger;

pub use actor::{Actor, ActorBaseStats};
pub use ailment::{
    bleed_instance, corrupted_blood_instance, ignite_instance, poison_instance, shock_effect,
};
pub use breakdown::{BreakdownStep, BreakdownTable};
pub use crit::{CritOutcome, resolve_crit, resolve_crit_traced};
pub use damage::{
    ConversionRules, DAMAGE_TYPES, DamageComponent, convert_damage, gain_as_extra,
    normalize_conversion, sum_avg,
};
pub use defence::{
    ANY_TAKEN_REFLECT_ENABLED, AVOID_AILMENT_CAP, AVOID_HIT_CAP, AvoidanceResult,
    CritExtraReduction, DefenceOutput, EsRecharge, EvadeSuite, HitSource, TakenMultiSuite,
    armour_reduction, calc_avoidance, calc_crit_extra_reduction, calc_defence, calc_es_recharge,
    calc_evade_suite, calc_taken_multi_suite, enemy_crit_effect, es_recharge_per_second,
    fill_evade_stun, hit_chance, monster_hit_chance, taken_mult_for_type,
    taken_mult_for_type_default, taken_mult_for_type_with_source, taken_mult_over_time,
};
pub use defence_panels::{
    BlockResult, DeflectionResult, calc_block, calc_deflection, calc_spirit_pool, calc_ward,
    fill_defence_panels,
};
pub use ehp::{
    EhpLoopParams, EhpOptions, EhpResult, EnemyDamageIn, MaxHitInputs, NotHitSuite,
    ResistanceSuite, assemble_enemy_damage, calc_ehp, calc_ehp_with_opts, enemy_crit_effect_ehp,
    enemy_damage_placeholder, fill_ehp_pob2, max_hit_for_type, max_hit_pob2, not_hit_suite,
    number_of_hits_to_die, physical_max_hit, physical_taken_fraction, taken_hit_per_type,
    total_hit_pools,
};
pub use env::Env;
pub use error::CalcError;
pub use minion::{
    AttributeInfusion, MinionBaseStats, MinionCategory, MinionContext, MinionData, MinionDef,
    MinionInput, MinionLimitId, MinionModifierEntry, MinionWeaponData, build_minion_context,
    build_minion_context_from_def, derive_minion_base_stats, minion_level_from_gem_level,
    minion_modifier_applies, resolve_minion_level, write_summoned_minion_multipliers,
};
pub use offence::{
    MinimalInput, MinimalOutput, TracedMinimalOutput, calculate_minimal, calculate_minimal_traced,
    calculate_minimal_traced_vs_enemy, calculate_minimal_vs_enemy,
};
pub use output::{MinionOutput, OutputTable};
pub use perform::perform;
pub use pool_damage::{
    AllyLayer, POB2_DAMAGE_ORDER, PoolCtx, PoolState, PoolsAfter, TypedDamage,
    apply_protected_layer, extend_total_hit_pool, life_hit_pool_with_loss_prevention,
    pool_protected, reduce_pools, total_hit_pool_base,
};
pub use pool_setup::{
    MomHitPools, PoolBaseStats, build_pool_ctx, build_pool_state, minimum_es_bypass, mom_hit_pools,
};
pub use session::{BuffKind, BuffSpec, CalculationSession};
pub use setup_env::{env_with_enemy, reduce_enemy_exposure, setup_enemy};
pub use skill_use_time::{SkillUseTime, calc_skill_use_time};
pub use stat_boundary::{StatBoundary, stat_boundary};
pub use stun::{StunInputs, StunResult, calc_stun, calc_stun_threshold};
pub use survivability::{
    AllChargeStates, ChargeKind, ChargeState, DEFAULT_CHARGE_DURATION_SECONDS, DEFAULT_MAX_CHARGES,
    LEECH_EFFECTIVE_MAX_HIT_DAMAGE, LEECH_MAX_ES_RATE_PCT, LEECH_MAX_INSTANCE_PCT,
    LEECH_MAX_LIFE_RATE_PCT, LEECH_MAX_MANA_RATE_PCT, LeechResource, LeechResult,
    RECOUP_DURATION_4S, RECOUP_DURATION_DEFAULT, RecoupResource, RecoupResult, Reservation,
    block_chance, calc_leech, calc_leech_from_db, calc_recoup, calc_recoup_from_db, calc_regen,
    capped_chance, charge_maximum, charge_minimum, regen, regen_with_rate, reservation,
    reservation_with_efficiency, resolve_all_charges, resolve_charge_state,
};
pub use taken::{
    MitigationCtx, MitigationInputs, armour_applies_pct, build_mitigation_ctx, damage_shift_table,
    effective_applied_armour, taken_hit_from_damage,
};
pub use trigger::{
    CwcTriggerRate, EnergyTriggerRate, RotationResult, RotationSkill, SocketedSpellInfo,
    TriggerCondition, TriggerRate, action_cooldown, calc_cwc_trigger_rate,
    calc_cwc_trigger_rate_traced, calc_energy_per_event, calc_energy_trigger_rate,
    calc_energy_trigger_rate_traced, calc_max_energy, calc_multi_spell_rotation,
    resolve_trigger_rate, resolve_trigger_rate_traced, round_cooldown_to_tick, server_tick_rate,
    spell_cast_time_added_to_cooldown, trigger_rate_cap,
};

pub(crate) fn round(value: f64) -> f64 {
    (value * 1_000_000_000.0).round() / 1_000_000_000.0
}
