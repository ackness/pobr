//! Sustained recovery and the build's own resource loss. Enemy DoT is never
//! an input here. Resource drains ("Lose … per second") bypass mitigation;
//! damage the build deals to itself uses resistance and damage-taken modifiers.

use super::{Env, LeechResource, calc_regen};
use crate::{CalcConfig, HandTag, ModDb};
use pobr_data::prelude::*;

const RESOURCES: [&str; 5] = ["Life", "Mana", "EnergyShield", "Ward", "Rage"];
const TYPES: [DamageType; 5] = [
    DamageType::Physical,
    DamageType::Fire,
    DamageType::Cold,
    DamageType::Lightning,
    DamageType::Chaos,
];
const TYPE_NAMES: [&str; 5] = ["Physical", "Fire", "Cold", "Lightning", "Chaos"];

fn base(db: &ModDb, cfg: &CalcConfig, name: &str) -> f64 {
    db.sum(ModType::Base, cfg, &[ModName::from(name)])
}

fn factor(db: &ModDb, cfg: &CalcConfig, name: &str) -> f64 {
    let names = [ModName::from(name)];
    ((1.0 + db.sum(ModType::Inc, cfg, &names) / 100.0) * db.more(cfg, &names)).max(0.0)
}

fn cannot_gain(db: &ModDb, cfg: &CalcConfig, resource: &str) -> bool {
    db.flag(cfg, &format!("CannotGain{resource}"))
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
    let maximum_rage = (pobr_data::constants::BASE_MAXIMUM_RAGE + base(db, cfg, "MaximumRage"))
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
    for (i, resource) in RESOURCES.iter().enumerate() {
        let no_recovery = cannot_gain(db, cfg, resource)
            || (*resource == "Life" && db.flag(cfg, "CannotRecoverLifeOutsideLeech"));
        let regeneration = if no_recovery
            || db.flag(cfg, &format!("No{resource}Regen"))
            || db.flag(cfg, &format!("UnaffectedBy{resource}Regen"))
        {
            0.0
        } else {
            (regen[i] * 10.0 + 0.5).floor() / 10.0
        };
        let direct = if no_recovery {
            0.0
        } else {
            base(db, cfg, &format!("{resource}Recovery"))
                * factor(db, cfg, &format!("{resource}RecoveryRate"))
                + base(db, cfg, &format!("Overflow{resource}Recovery"))
        };
        let drain = (base(db, cfg, &format!("{resource}Degen"))
            + pools[i] * base(db, cfg, &format!("{resource}DegenPercent")) / 100.0)
            .max(0.0)
            * factor(db, cfg, &format!("{resource}Degen"));
        recovery[i] = regeneration + direct - drain;
    }
    // "Mana Recovery from Regeneration is also applied to Runic Ward"
    // copies regeneration before display rounding; it is recovery at the
    // destination and does not receive its regeneration-rate modifiers.
    for (source, source_name) in RESOURCES.iter().enumerate() {
        if cannot_gain(db, cfg, source_name) || db.flag(cfg, &format!("No{source_name}Regen")) {
            continue;
        }
        for (destination, destination_name) in RESOURCES.iter().enumerate() {
            if source != destination
                && !cannot_gain(db, cfg, destination_name)
                && db.flag(
                    cfg,
                    &format!("{source_name}RegenerationRecovers{destination_name}"),
                )
            {
                recovery[destination] +=
                    regen[source] * factor(db, cfg, &format!("{destination_name}RecoveryRate"));
            }
        }
    }
    // Zealot's Oath: excess Life regeneration is applied to Energy Shield.
    if db.flag(cfg, "ZealotsOath") && recovery[0] > 0.0 {
        if !cannot_gain(db, cfg, "EnergyShield") {
            recovery[2] += (recovery[0]
                - base(db, cfg, "LifeRecovery") * factor(db, cfg, "LifeRecoveryRate"))
            .max(0.0);
        }
        recovery[0] = 0.0;
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

fn proxy(cfg: &CalcConfig) -> bool {
    use pobr_data::skill::SkillTypes;
    cfg.skill_types.intersects(
        SkillTypes::TRAPPED
            | SkillTypes::REMOTE_MINED
            | SkillTypes::from_pob2_name("UsedByTotem").unwrap_or_default()
            | SkillTypes::from_pob2_name("UsedByProxy").unwrap_or_default(),
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
    let name = match resource {
        LeechResource::Life => "Life",
        LeechResource::Mana => "Mana",
        LeechResource::EnergyShield => "EnergyShield",
    };
    if cannot_gain(db, cfg, name) || proxy(cfg) {
        return Gain::default();
    }
    let on_hit = if name == "Life" && db.flag(cfg, "CannotRecoverLifeOutsideLeech") {
        0.0
    } else {
        base(db, cfg, &format!("{name}OnHit")) + base(enemy, cfg, &format!("Self{name}OnHit"))
    };
    if db.flag(cfg, &format!("CannotLeech{name}"))
        || enemy.flag(cfg, &format!("CannotLeech{name}FromSelf"))
    {
        return Gain {
            on_hit,
            ..Gain::default()
        };
    }
    // The 40,000 damage ceiling applies to the whole hit. Scale each type
    // proportionally, so added elemental damage cannot fuel physical leech.
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
    let total: f64 = dealt.iter().map(|(_, amount)| amount).sum();
    let scale = if total > 0.0 {
        (cfg.constants.game().effective_max_damage_for_leech / total).min(1.0)
    } else {
        0.0
    };
    let amount: f64 = dealt
        .iter()
        .map(|&(ty, damage)| {
            let ty_name = TYPE_NAMES[ty as usize];
            let mut percent = base(db, cfg, &format!("Damage{name}Leech"));
            if ty == DamageType::Physical {
                percent += base(db, cfg, &format!("{name}Leech"));
            }
            percent += base(db, cfg, &format!("{ty_name}Damage{name}Leech"));
            if ty.is_elemental() {
                percent += base(db, cfg, &format!("ElementalDamage{name}Leech"));
            }
            if db.flag(cfg, &format!("{name}LeechBasedOn{ty_name}Damage"))
                || (ty.is_elemental()
                    && db.flag(cfg, &format!("{name}LeechBasedOnElementalDamage")))
            {
                percent += base(db, cfg, &format!("PhysicalDamage{name}Leech"));
            }
            damage * scale * percent.max(0.0) / 100.0
        })
        .sum();
    // Reuse the engine's single-instance leech limits. A sustained rate also
    // needs sufficient hits to replace each expired instance; hit frequency
    // never stacks the recovery rates of overlapping instances.
    let instant_fraction = if db.flag(cfg, &format!("Instant{name}Leech")) {
        1.0
    } else {
        (base(db, cfg, &format!("Instant{name}Leech")) / 100.0).clamp(0.0, 1.0)
    };
    let recovery = factor(db, cfg, &format!("{name}RecoveryRate"));
    let amount = amount * factor(db, cfg, &format!("{name}LeechAmount"));
    let instance = super::survivability::leech_instance(
        pool,
        amount,
        resource,
        factor(db, cfg, &format!("{name}LeechRate")),
        factor(db, cfg, &format!("Max{name}LeechRate")),
    );
    let normal = instance.instance_total * (1.0 - instant_fraction);
    let normal_instance = super::survivability::leech_instance(
        pool,
        normal,
        resource,
        factor(db, cfg, &format!("{name}LeechRate")),
        factor(db, cfg, &format!("Max{name}LeechRate")),
    );
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
        .contains(pobr_data::skill::SkillTypes::from_pob2_name("Chaos").unwrap_or_default())
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
    let direct_drain: f64 = RESOURCES[..3]
        .iter()
        .enumerate()
        .map(|(i, name)| {
            (base(db, cfg, &format!("{name}Degen"))
                + pools[i] * base(db, cfg, &format!("{name}DegenPercent")) / 100.0)
                .max(0.0)
                * factor(db, cfg, &format!("{name}Degen"))
        })
        .sum();
    let mut damage = [0.0; 5];
    for (src, src_name) in TYPE_NAMES.iter().enumerate() {
        let amount = base(db, cfg, &format!("{src_name}Degen")).max(0.0);
        if amount == 0.0 {
            continue;
        }
        let mut remaining = 1.0;
        for (dst, dst_name) in TYPE_NAMES.iter().enumerate() {
            if src == dst {
                continue;
            }
            // Damage from Hits taken as ... does not convert degeneration.
            let mut share = base(db, cfg, &format!("{src_name}DamageTakenAs{dst_name}"));
            if TYPES[src].is_elemental() {
                share += base(db, cfg, &format!("ElementalDamageTakenAs{dst_name}"));
            }
            let share = (share / 100.0).max(0.0);
            damage[dst] += amount * share;
            remaining -= share;
        }
        damage[src] += amount * remaining.max(0.0);
    }
    let resistance = [
        base(db, cfg, "PhysicalDamageReduction").clamp(
            0.0,
            cfg.constants
                .character()
                .maximum_physical_damage_reduction_pct,
        ),
        out.fire_resistance,
        out.cold_resistance,
        out.lightning_resistance,
        out.chaos_resistance,
    ];
    let mut typed_loss = 0.0;
    for (i, ty) in TYPES.iter().enumerate() {
        let amount = damage[i]
            * (1.0 - resistance[i] / 100.0).max(0.0)
            * super::defence::taken_mult_over_time(db, cfg, *ty);
        typed_loss += amount;
        let mom = ((base(db, cfg, "DamageTakenFromManaBeforeLife")
            + base(
                db,
                cfg,
                &format!("{}DamageTakenFromManaBeforeLife", TYPE_NAMES[i]),
            ))
            / 100.0)
            .clamp(0.0, 1.0);
        let bypass = if db.flag(cfg, "UnblockedDamageDoesBypassES") {
            1.0
        } else {
            db.override_(cfg, &format!("{}EnergyShieldBypass", TYPE_NAMES[i]))
                .unwrap_or_else(|| base(db, cfg, &format!("{}EnergyShieldBypass", TYPE_NAMES[i])))
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
