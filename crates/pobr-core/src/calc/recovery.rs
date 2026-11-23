//! Sustained recovery and the build's own resource loss. Enemy DoT is never
//! an input here. Resource drains ("Lose … per second") bypass mitigation;
//! damage the build deals to itself uses resistance and damage-taken modifiers.

use super::{Env, LeechResource, calc_regen};
use crate::{CalcConfig, HandTag, ModDb};
use pobr_data::prelude::*;

struct ResourceMods {
    is_life: bool,
    cannot_gain: &'static str,
    no_regen: &'static str,
    unaffected_by_regen: &'static str,
    recovery: &'static str,
    recovery_rate: &'static str,
    overflow_recovery: &'static str,
    degen: &'static str,
    degen_percent: &'static str,
}

macro_rules! resource_mods {
    ($r:literal, $is_life:expr) => {
        ResourceMods {
            is_life: $is_life,
            cannot_gain: concat!("CannotGain", $r),
            no_regen: concat!("No", $r, "Regen"),
            unaffected_by_regen: concat!("UnaffectedBy", $r, "Regen"),
            recovery: concat!($r, "Recovery"),
            recovery_rate: concat!($r, "RecoveryRate"),
            overflow_recovery: concat!("Overflow", $r, "Recovery"),
            degen: concat!($r, "Degen"),
            degen_percent: concat!($r, "DegenPercent"),
        }
    };
}

const RESOURCE_MODS: [ResourceMods; 5] = [
    resource_mods!("Life", true),
    resource_mods!("Mana", false),
    resource_mods!("EnergyShield", false),
    resource_mods!("Ward", false),
    resource_mods!("Rage", false),
];

macro_rules! regen_recovers_row {
    ($src:literal) => {
        [
            concat!($src, "RegenerationRecoversLife"),
            concat!($src, "RegenerationRecoversMana"),
            concat!($src, "RegenerationRecoversEnergyShield"),
            concat!($src, "RegenerationRecoversWard"),
            concat!($src, "RegenerationRecoversRage"),
        ]
    };
}

const REGEN_RECOVERS: [[&str; 5]; 5] = [
    regen_recovers_row!("Life"),
    regen_recovers_row!("Mana"),
    regen_recovers_row!("EnergyShield"),
    regen_recovers_row!("Ward"),
    regen_recovers_row!("Rage"),
];

struct LeechMods {
    is_life: bool,
    cannot_gain: &'static str,
    on_hit: &'static str,
    self_on_hit: &'static str,
    cannot_leech: &'static str,
    cannot_leech_from_self: &'static str,
    instant_leech: &'static str,
    recovery_rate: &'static str,
    leech_rate: &'static str,
}

macro_rules! leech_mods {
    ($r:literal, $is_life:expr) => {
        LeechMods {
            is_life: $is_life,
            cannot_gain: concat!("CannotGain", $r),
            on_hit: concat!($r, "OnHit"),
            self_on_hit: concat!("Self", $r, "OnHit"),
            cannot_leech: concat!("CannotLeech", $r),
            cannot_leech_from_self: concat!("CannotLeech", $r, "FromSelf"),
            instant_leech: concat!("Instant", $r, "Leech"),
            recovery_rate: concat!($r, "RecoveryRate"),
            leech_rate: concat!($r, "LeechRate"),
        }
    };
}

fn leech_mods_for(resource: LeechResource) -> &'static LeechMods {
    const LIFE: LeechMods = leech_mods!("Life", true);
    const MANA: LeechMods = leech_mods!("Mana", false);
    const ES: LeechMods = leech_mods!("EnergyShield", false);
    match resource {
        LeechResource::Life => &LIFE,
        LeechResource::Mana => &MANA,
        LeechResource::EnergyShield => &ES,
    }
}

const TYPES: [DamageType; 5] = [
    DamageType::Physical,
    DamageType::Fire,
    DamageType::Cold,
    DamageType::Lightning,
    DamageType::Chaos,
];

struct TypeDegenMods {
    degen: &'static str,
    taken_from_mana_before_life: &'static str,
    es_bypass: &'static str,
    elem_taken_as: &'static str,
}

macro_rules! type_degen_mods {
    ($t:literal) => {
        TypeDegenMods {
            degen: concat!($t, "Degen"),
            taken_from_mana_before_life: concat!($t, "DamageTakenFromManaBeforeLife"),
            es_bypass: concat!($t, "EnergyShieldBypass"),
            elem_taken_as: concat!("ElementalDamageTakenAs", $t),
        }
    };
}

const TYPE_DEGEN_MODS: [TypeDegenMods; 5] = [
    type_degen_mods!("Physical"),
    type_degen_mods!("Fire"),
    type_degen_mods!("Cold"),
    type_degen_mods!("Lightning"),
    type_degen_mods!("Chaos"),
];

macro_rules! taken_as_row {
    ($src:literal) => {
        [
            concat!($src, "DamageTakenAsPhysical"),
            concat!($src, "DamageTakenAsFire"),
            concat!($src, "DamageTakenAsCold"),
            concat!($src, "DamageTakenAsLightning"),
            concat!($src, "DamageTakenAsChaos"),
        ]
    };
}

const TAKEN_AS: [[&str; 5]; 5] = [
    taken_as_row!("Physical"),
    taken_as_row!("Fire"),
    taken_as_row!("Cold"),
    taken_as_row!("Lightning"),
    taken_as_row!("Chaos"),
];

fn base(db: &ModDb, cfg: &CalcConfig, name: &str) -> f64 {
    db.sum(ModType::Base, cfg, &[name])
}

fn factor(db: &ModDb, cfg: &CalcConfig, name: &str) -> f64 {
    let names = [name];
    ((1.0 + db.sum(ModType::Inc, cfg, &names) / 100.0) * db.more(cfg, &names)).max(0.0)
}

/// Called after the final main-skill frequency (channel/trigger/reload) is known.
pub(crate) fn fill_recovery(env: &mut Env, hit: &[(DamageType, f64)], crit: &[(DamageType, f64)]) {
    fill_regeneration(env);
    fill_leech_gain(env, hit, crit);
    fill_net_recovery(env);
}

fn fill_regeneration(env: &mut Env) {
    let db = &env.player.mod_db;
    let cfg = &env.cfg;
    let out = &mut env.player.output;
    // base_rage_regeneration_rate_per_minute_% refers to maximum Rage,
    // not the player's currently selected number of Rage stacks.
    let maximum_rage = (cfg.constants.game().base_maximum_rage + base(db, cfg, "MaximumRage"))
        * factor(db, cfg, "MaximumRage");
    let pools = [
        out.life,
        out.mana,
        out.energy_shield,
        out.ward,
        maximum_rage,
    ];
    let regen = [
        out.life_regen,
        out.mana_regen,
        out.energy_shield_regen,
        out.ward_regen,
        calc_regen(db, cfg, maximum_rage, "RageRegen"),
    ];
    let mut recovery = [0.0; 5];
    for (i, mods) in RESOURCE_MODS.iter().enumerate() {
        let no_recovery = db.flag(cfg, mods.cannot_gain)
            || (mods.is_life && db.flag(cfg, "CannotRecoverLifeOutsideLeech"));
        let regeneration =
            if no_recovery || db.flag(cfg, mods.no_regen) || db.flag(cfg, mods.unaffected_by_regen)
            {
                0.0
            } else {
                (regen[i] * 10.0 + 0.5).floor() / 10.0
            };
        let direct = if no_recovery {
            0.0
        } else {
            base(db, cfg, mods.recovery) * factor(db, cfg, mods.recovery_rate)
                + base(db, cfg, mods.overflow_recovery)
        };
        let drain = (base(db, cfg, mods.degen)
            + pools[i] * base(db, cfg, mods.degen_percent) / 100.0)
            .max(0.0)
            * factor(db, cfg, mods.degen);
        recovery[i] = regeneration + direct - drain;
    }
    // "Mana Recovery from Regeneration is also applied to Runic Ward"
    // copies regeneration before display rounding; it is recovery at the
    // destination and does not receive its regeneration-rate modifiers.
    for (source, source_mods) in RESOURCE_MODS.iter().enumerate() {
        if db.flag(cfg, source_mods.cannot_gain) || db.flag(cfg, source_mods.no_regen) {
            continue;
        }
        for (destination, destination_mods) in RESOURCE_MODS.iter().enumerate() {
            if source != destination
                && !db.flag(cfg, destination_mods.cannot_gain)
                && db.flag(cfg, REGEN_RECOVERS[source][destination])
            {
                recovery[destination] +=
                    regen[source] * factor(db, cfg, destination_mods.recovery_rate);
            }
        }
    }
    // Zealot's Oath: excess Life regeneration is applied to Energy Shield.
    if db.flag(cfg, "ZealotsOath") && recovery[0] > 0.0 {
        let transfer = (recovery[0]
            - base(db, cfg, "LifeRecovery") * factor(db, cfg, "LifeRecoveryRate"))
        .max(0.0);
        if !db.flag(cfg, "CannotGainEnergyShield") {
            recovery[2] += transfer;
        }
        recovery[0] -= transfer;
    }
    [
        out.life_regen_recovery,
        out.mana_regen_recovery,
        out.energy_shield_regen_recovery,
        out.ward_regen_recovery,
        out.rage_regen_recovery,
    ] = recovery;
}

#[derive(Clone, Copy, Default)]
struct Gain {
    per_hit: f64,
    instant: f64,
    peak: f64,
    on_hit: f64,
}

pub(super) fn proxy(cfg: &CalcConfig) -> bool {
    use pobr_data::skill::SkillTypes;
    cfg.skill_types.intersects(
        SkillTypes::TRAPPED
            | SkillTypes::REMOTE_MINED
            | SkillTypes::USED_BY_TOTEM
            | SkillTypes::USED_BY_PROXY,
    )
}

fn gain_for_hit(
    env: &Env,
    cfg: &CalcConfig,
    damage: &[(DamageType, f64)],
    resource: LeechResource,
    pool: f64,
) -> Gain {
    let db = &env.player.mod_db;
    let enemy = &env.enemy.mod_db;
    let mods = leech_mods_for(resource);
    if db.flag(cfg, mods.cannot_gain) || proxy(cfg) {
        return Gain::default();
    }
    let on_hit = if mods.is_life && db.flag(cfg, "CannotRecoverLifeOutsideLeech") {
        0.0
    } else {
        base(db, cfg, mods.on_hit) + base(enemy, cfg, mods.self_on_hit)
    };
    if db.flag(cfg, mods.cannot_leech) || enemy.flag(cfg, mods.cannot_leech_from_self) {
        return Gain {
            on_hit,
            ..Gain::default()
        };
    }
    let dealt: Vec<_> = damage
        .iter()
        .map(|&(ty, amount)| {
            let multiplier = if cfg.mode_effective {
                super::offence::enemy_damage_multiplier(db, enemy, cfg, ty, amount)
            } else {
                1.0
            };
            (ty, (amount * multiplier).max(0.0))
        })
        .collect();
    // Reuse the engine's single-instance leech limits. A sustained rate also
    // needs sufficient hits to replace each expired instance; hit frequency
    // never stacks the recovery rates of overlapping instances.
    let instant_fraction = if db.flag(cfg, mods.instant_leech) {
        1.0
    } else {
        (base(db, cfg, mods.instant_leech) / 100.0).clamp(0.0, 1.0)
    };
    let recovery = factor(db, cfg, mods.recovery_rate);
    let speed = factor(db, cfg, mods.leech_rate);
    let cap = super::survivability::leech_cap_terms(db, cfg, pool, resource);
    let instance = super::survivability::calc_leech_for_hit(db, cfg, pool, &dealt, resource);
    let normal = instance.instance_total * (1.0 - instant_fraction);
    let normal_instance = super::survivability::leech_instance(normal, speed, cap);
    Gain {
        per_hit: normal * recovery,
        instant: instance.instance_total * instant_fraction,
        peak: normal_instance.display_rate_per_second * recovery,
        on_hit,
    }
}

fn fill_leech_gain(env: &mut Env, hit: &[(DamageType, f64)], crit: &[(DamageType, f64)]) {
    let out = &env.player.output;
    let pools = [out.life, out.mana, out.energy_shield];
    let resources = [
        LeechResource::Life,
        LeechResource::Mana,
        LeechResource::EnergyShield,
    ];
    let mut details = [[[0.0; 3]; 3]; 2];
    let mut per_hit = [0.0; 3];
    let mut rate = [0.0; 3];
    let mut instances: [Vec<(f64, f64)>; 3] = Default::default();
    let mut accumulate = |cfg: &CalcConfig,
                          normal: &[(DamageType, f64)],
                          critical: &[(DamageType, f64)],
                          crit_chance: f64,
                          hit_chance: f64,
                          share: f64| {
        let mut pass_cfg = cfg.clone();
        pass_cfg.keyword_flags = pass_cfg.keyword_flags | KeywordFlags::HIT;
        for (damage, portion, is_crit) in [
            (normal, 1.0 - crit_chance, false),
            (critical, crit_chance, true),
        ] {
            if portion <= 0.0 {
                continue;
            }
            pass_cfg.conditions.insert("CriticalStrike".into(), is_crit);
            for (i, resource) in resources.iter().enumerate() {
                let gain = gain_for_hit(env, &pass_cfg, damage, *resource, pools[i]);
                let hit_rate = out.effective_action_rate * hit_chance * share;
                for (channel, value) in [gain.per_hit, gain.instant, gain.on_hit]
                    .into_iter()
                    .enumerate()
                {
                    details[0][i][channel] += value * portion * share;
                }
                if gain.peak > 0.0 {
                    let duration = gain.per_hit / gain.peak;
                    instances[i].push((gain.peak, (duration * hit_rate * portion).min(1.0)));
                }
                details[1][i][1] += gain.instant * hit_rate * portion;
                details[1][i][2] += gain.on_hit * hit_rate * portion;
                per_hit[i] += (gain.per_hit + gain.instant + gain.on_hit) * portion * share;
                rate[i] += (gain.instant + gain.on_hit) * hit_rate * portion;
            }
        }
    };
    if env.hand_sources.is_empty() {
        accumulate(&env.cfg, hit, crit, out.crit_chance, out.hit_chance, 1.0);
    } else {
        let share = if env.hand_sources.len() > 1 && !env.double_hits_when_dual_wielding {
            0.5
        } else {
            1.0
        };
        for source in &env.hand_sources {
            let hand = match source.label {
                HandTag::MainHand | HandTag::Single => out.main_hand.as_ref(),
                HandTag::OffHand => out.off_hand.as_ref(),
            };
            let Some(hand) = hand else { continue };
            let (cfg, _) = super::hand_pass::hand_scope(
                &env.player.mod_db,
                source,
                &env.cfg,
                &super::perform::player_input(env),
            );
            accumulate(
                &cfg,
                &hand.stored_hit_avg,
                &hand.stored_crit_avg,
                hand.crit_chance,
                hand.hit_chance,
                share,
            );
        }
    }
    // "Only a single Leech instance per resource (Life, Mana, or Energy
    // Shield) can apply at a time."
    // https://www.pathofexile.com/forum/view-thread/3932540
    // Allocate the available time to the highest rates first; lower rates
    // can only fill time left after those instances expire. This is a
    // sustained, evenly-spaced-hit estimate, not a simulation of crit timing.
    for (i, candidates) in instances.iter_mut().enumerate() {
        candidates.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut remaining: f64 = 1.0;
        for &(peak, uptime) in candidates.iter() {
            let active = uptime.min(remaining);
            details[1][i][0] += peak * active;
            remaining -= active;
        }
        rate[i] += details[1][i][0];
    }
    let out = &mut env.player.output;
    env.leech_gain_terms = details;
    [
        out.life_leech_gain_rate,
        out.mana_leech_gain_rate,
        out.energy_shield_leech_gain_rate,
    ] = rate;
    [
        out.life_leech_gain_per_hit,
        out.mana_leech_gain_per_hit,
        out.energy_shield_leech_gain_per_hit,
    ] = per_hit;
}

/// The player's damage-over-time taken terms of one type, with the resolved
/// resistance against damage over time (`defence::dot_taken_terms`).
pub(crate) fn dot_taken(
    db: &ModDb,
    cfg: &CalcConfig,
    out: &super::OutputTable,
    ty: DamageType,
) -> super::defence::DotTakenTerms {
    let resistance = match ty {
        DamageType::Physical => 0.0,
        DamageType::Fire => out.resistance_over_time[0],
        DamageType::Cold => out.resistance_over_time[1],
        DamageType::Lightning => out.resistance_over_time[2],
        DamageType::Chaos => out.resistance_over_time[3],
    };
    super::defence::dot_taken_terms(db, cfg, ty, resistance)
}

pub(super) struct NetRecoveryTerms {
    pub costs: [f64; 5],
    pub loss: [f64; 3],
    pub direct_drain: f64,
    pub use_loss: [f64; 3],
    pub typed_loss: f64,
}

pub(super) fn net_recovery_terms(env: &Env) -> NetRecoveryTerms {
    let db = &env.player.mod_db;
    let cfg = &env.cfg;
    let out = &env.player.output;
    let pools = [out.life, out.mana, out.energy_shield];
    // Spending already follows casts, channels, reloads, triggers or placements.
    // Percent costs remain separate display channels; convert them to resources.
    let costs = [
        out.life_per_second_cost + out.life * out.life_percent_per_second_cost / 100.0,
        out.mana_per_second_cost + out.mana * out.mana_percent_per_second_cost / 100.0,
        out.es_per_second_cost + out.energy_shield * out.es_percent_per_second_cost / 100.0,
        out.ward_per_second_cost + out.ward * out.ward_percent_per_second_cost / 100.0,
        out.rage_per_second_cost,
    ];
    // Event Horizon is resource loss on use, not a skill cost or Chaos
    // damage: cost efficiency, resistance and immunity do not reduce it.
    // Only the selected player-used Chaos skill contributes; triggered and
    // proxy actions are not player uses. Repeats share the original use.
    let percent = base(db, cfg, "ChaosSkillLifeAndEnergyShieldLossPercent").max(0.0);
    let applies = cfg
        .skill_types
        .contains(pobr_data::skill::SkillTypes::CHAOS)
        && !cfg
            .skill_types
            .contains(pobr_data::skill::SkillTypes::TRIGGERED)
        && !db.flag(cfg, "SkillIsTriggered")
        && out.skill_trigger_rate <= 0.0
        && !proxy(cfg);
    let use_rate = if percent > 0.0 && applies {
        super::perform::resource_cost_use_rate(env)
    } else {
        0.0
    };
    let use_loss = [
        out.life * percent / 100.0 * use_rate,
        0.0,
        out.energy_shield * percent / 100.0 * use_rate,
    ];
    let mut loss = std::array::from_fn(|i| costs[i] + use_loss[i]);
    let direct_drain: f64 = RESOURCE_MODS[..3]
        .iter()
        .enumerate()
        .map(|(i, mods)| {
            (base(db, cfg, mods.degen) + pools[i] * base(db, cfg, mods.degen_percent) / 100.0)
                .max(0.0)
                * factor(db, cfg, mods.degen)
        })
        .sum();
    let mut damage = [0.0; 5];
    for (src, src_mods) in TYPE_DEGEN_MODS.iter().enumerate() {
        let amount = base(db, cfg, src_mods.degen).max(0.0);
        if amount == 0.0 {
            continue;
        }
        let mut remaining = 1.0;
        for (dst, dst_mods) in TYPE_DEGEN_MODS.iter().enumerate() {
            if src == dst {
                continue;
            }
            // Damage from Hits taken as ... does not convert degeneration.
            let mut share = base(db, cfg, TAKEN_AS[src][dst]);
            if TYPES[src].is_elemental() {
                share += base(db, cfg, dst_mods.elem_taken_as);
            }
            let share = (share / 100.0).max(0.0);
            damage[dst] += amount * share;
            remaining -= share;
        }
        damage[src] += amount * remaining.max(0.0);
    }
    let mut typed_loss = 0.0;
    for (i, ty) in TYPES.iter().enumerate() {
        let mods = &TYPE_DEGEN_MODS[i];
        let amount = damage[i] * dot_taken(db, cfg, out, *ty).multiplier();
        typed_loss += amount;
        let mom = ((base(db, cfg, "DamageTakenFromManaBeforeLife")
            + base(db, cfg, mods.taken_from_mana_before_life))
            / 100.0)
            .clamp(0.0, 1.0);
        let bypass = if db.flag(cfg, "UnblockedDamageDoesBypassES") {
            1.0
        } else {
            db.override_(cfg, mods.es_bypass)
                .unwrap_or_else(|| base(db, cfg, mods.es_bypass))
                / 100.0
        }
        .clamp(0.0, 1.0);
        if out.energy_shield_regen_recovery > 0.0 && out.energy_shield > 0.0 {
            if db.flag(cfg, "EnergyShieldProtectsMana") {
                loss[0] += amount * (1.0 - mom);
                loss[1] += amount * mom * bypass;
                loss[2] += amount * mom * (1.0 - bypass);
            } else {
                loss[0] += amount * bypass * (1.0 - mom);
                loss[1] += amount * bypass * mom;
                loss[2] += amount * (1.0 - bypass);
            }
        } else {
            loss[0] += amount * (1.0 - mom);
            loss[1] += amount * mom;
        }
    }
    NetRecoveryTerms {
        costs,
        loss,
        direct_drain,
        use_loss,
        typed_loss,
    }
}

fn fill_net_recovery(env: &mut Env) {
    let NetRecoveryTerms {
        costs,
        loss,
        direct_drain,
        use_loss,
        typed_loss,
    } = net_recovery_terms(env);
    let out = &mut env.player.output;
    out.total_build_degen =
        direct_drain + use_loss.iter().sum::<f64>() + typed_loss + costs.iter().sum::<f64>();
    out.net_life_regen = out.life_regen_recovery - loss[0];
    out.net_mana_regen = out.mana_regen_recovery - loss[1];
    out.net_energy_shield_regen = out.energy_shield_regen_recovery - loss[2];
    // This panel total combines Life, Mana, ES and Ward. Rage recovery remains
    // its own channel; its spending still contributes to TotalBuildDegen.
    out.net_ward_regen = out.ward_regen_recovery - costs[3];
    out.total_net_regen =
        out.net_life_regen + out.net_mana_regen + out.net_energy_shield_regen + out.net_ward_regen;
}
