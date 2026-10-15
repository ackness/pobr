//! Queries for the optional Calcs explanations. Each source group is one
//! ModDb query: totals and rows always use the same names and configuration.
use super::CalculationSession;
use crate::ModEvaluation;
use pobr_data::prelude::{ModName, ModType};

pub struct SourceGroup<'a> {
    pub name: String,
    pub more_precision: Option<u32>,
    pub totals: (f64, f64, f64),
    pub rows: Vec<ModEvaluation<'a>>,
}

/// Inputs are stable ids, localized at the display boundary. Formula tokens
/// refer to these ids; contextual inputs explain the upstream calculation.
pub struct DerivedDetail {
    pub formula: String,
    pub inputs: Vec<(String, f64)>,
    pub context: Vec<(String, f64)>,
}

/// Only genuine player-side queries belong here. Enemy-side and non-additive
/// relationships are described by `derived_detail`, never synthetic mod rows.
pub fn source_names(stat: &str) -> Vec<String> {
    let names: &[&str] = match stat {
        "Ward" => &["Ward", "Defences"],
        "LifeReserved" => &[
            "LifeReserved",
            "LifeReservedPercent",
            "LifeReservationEfficiency",
            "ReservationEfficiency",
            "ReservationMultiplier",
        ],
        "ManaReserved" => &[
            "ManaReserved",
            "ManaReservedPercent",
            "ManaReservationEfficiency",
            "ReservationEfficiency",
            "ReservationMultiplier",
        ],
        "SpiritReserved" => &[
            "SkillSpiritReservationBase",
            "ReservationMultiplier",
            "ExtraSpirit",
            "HasNoCost",
        ],
        "BlockChance"
        | "BlockChanceMax"
        | "SpellBlockChance"
        | "SpellBlockChanceMax"
        | "EffectiveBlockChance"
        | "EffectiveSpellBlockChance"
        | "EffectiveProjectileBlockChance"
        | "EffectiveSpellProjectileBlockChance" => &[
            "ShieldBlockChance",
            "BlockChance",
            "BaseBlockChanceMax",
            "BlockChanceMax",
            "ProjectileBlockChance",
            "SpellBlockChance",
            "BaseSpellBlockChanceMax",
            "SpellBlockChanceMax",
            "ProjectileSpellBlockChance",
            "SpellBlockChanceIsBlockChance",
            "SpellBlockChanceMaxIsBlockChanceMax",
            "CannotBlockAttacks",
            "CannotBlockSpells",
            "BlockChanceIsLucky",
            "BlockChanceIsUnlucky",
            "SpellBlockChanceIsLucky",
            "SpellBlockChanceIsUnlucky",
            "ProjectileBlockChanceIsLucky",
            "ProjectileBlockChanceIsUnlucky",
            "SpellProjectileBlockChanceIsLucky",
            "SpellProjectileBlockChanceIsUnlucky",
        ],
        "FireResistMax" => &[
            "MaximumFireResistance",
            "FireResistMax",
            "MaximumAllElementalResistances",
            "ElementalResistMax",
        ],
        "ColdResistMax" => &[
            "MaximumColdResistance",
            "ColdResistMax",
            "MaximumAllElementalResistances",
            "ElementalResistMax",
            "MaximumFireResistance",
            "FireResistMax",
            "FireResistAppliesToColdResist",
            "FireResistMaxAppliesToColdResist",
        ],
        "LightningResistMax" => &[
            "MaximumLightningResistance",
            "LightningResistMax",
            "MaximumAllElementalResistances",
            "ElementalResistMax",
            "MaximumFireResistance",
            "FireResistMax",
            "FireResistAppliesToLightningResist",
            "FireResistMaxAppliesToLightningResist",
        ],
        "BlockEffect" => &["BlockEffect"],
        "AoeRadius" | "AoeAreaMod" => &[
            "SkillRadiusBase",
            "AreaOfEffect",
            "AreaOfEffectPrimary",
            "AreaOfEffectRadius",
        ],
        "ProjectileCount" => &[
            "NoAdditionalProjectiles",
            "ProjectileCount",
            "TwoAdditionalProjectilesChance",
            "SurpassingProjectileChance",
            "AdditionalProjectilesAddChainsInstead",
            "AdditionalProjectilesAddSplitsInstead",
            "AdditionalProjectilesAddBouncesInstead",
        ],
        "ProjectileSpeedMod" => &["ProjectileSpeed"],
        "Cooldown" | "CooldownStoredUses" => &[
            "SkillCooldownBase",
            "CooldownRecovery",
            "CooldownRecoveryFromTemporalis",
            "AdditionalCooldownUses",
        ],
        "TotalTotemPlacementTime" => &[
            "SkillTotemPlacementTimeBase",
            "TotemPlacementTime",
            "TotemPlacementSpeed",
            "ActionSpeed",
        ],
        "TrapThrowingTime" => &[
            "SkillTrapThrowingTimeBase",
            "TrapThrowingTime",
            "TrapThrowingSpeed",
            "SkillTrapThrowingTime",
            "ActionSpeed",
        ],
        "MineLayingTime" => &[
            "SkillMineLayingTimeBase",
            "MineLayingTime",
            "MineLayingSpeed",
            "SkillMineThrowingTime",
            "ActionSpeed",
        ],
        "WarcryCastTime" => &[
            "SkillWarcryCastTimeBase",
            "WarcryCastTime",
            "WarcrySpeed",
            "ActionSpeed",
            "InstantWarcry",
        ],
        "TrapThrowCount" => &["TrapThrowCount"],
        "MineThrowCount" => &["MineThrowCount"],
        "EsRechargeRate" => &["EnergyShieldRechargeRate", "ZealotsOath"],
        "EsRechargeDelay" => &["EnergyShieldRechargeFaster", "EnergyShieldRechargeBase"],
        "ReqStr" | "ReqDex" | "ReqInt" => &[
            "GlobalAttributeRequirements",
            "GlobalGemAttributeRequirements",
            "GlobalItemAttributeRequirements",
            "GlobalWeaponAttributeRequirements",
            "IgnoreAttributeRequirements",
            "GemAttributeRequirementsSatisfiedByHighestAttribute",
            "StrengthSatisfiesMeleeWeaponsAndSkills",
        ],
        _ => &[],
    };
    if !names.is_empty() {
        return names.iter().map(|s| (*s).to_string()).collect();
    }
    for resource in ["Life", "Mana", "EnergyShield", "Ward", "Rage"] {
        if stat == format!("{resource}Regen") || stat == format!("{resource}RegenRecovery") {
            let mut names = [
                "Regen",
                "RegenPercent",
                "RegenRate",
                "RecoveryRate",
                "Recovery",
                "Degen",
                "DegenPercent",
            ]
            .map(|suffix| format!("{resource}{suffix}"))
            .to_vec();
            names.extend([
                format!("No{resource}Regen"),
                format!("CannotGain{resource}"),
                format!("UnaffectedBy{resource}Regen"),
                format!("Overflow{resource}Recovery"),
                "ZealotsOath".into(),
                "CannotRecoverLifeOutsideLeech".into(),
            ]);
            for source in ["Life", "Mana", "EnergyShield", "Ward", "Rage"] {
                names.push(format!("{source}RegenerationRecovers{resource}"));
            }
            return names;
        }
    }
    for resource in ["Mana", "Life", "ES", "Ward", "Rage"] {
        for suffix in ["", "Percent"] {
            if stat == format!("{resource}{suffix}Cost") {
                let mut names = vec![
                    "HasNoCost".into(),
                    "Cost".into(),
                    "SupportManaMultiplier".into(),
                    "HybridManaAndLifeCost_Life".into(),
                    "BaseManaCostAsLifeCost".into(),
                    "ManaCostAsEnergyShieldCost".into(),
                    "WardCostAsPercentOfManaCost".into(),
                    "WardCostAsPercentOfLifeCost".into(),
                ];
                for r in [resource, "Mana"] {
                    for name in [
                        format!("Skill{r}{suffix}CostBase"),
                        format!("{r}{suffix}CostBase"),
                        format!("Base{r}{suffix}CostOverride"),
                        format!("{r}{suffix}Cost"),
                        format!("{r}Cost"),
                        format!("{r}CostPercentOfMaximum{r}"),
                        format!("SkillUnreserved{r}PercentCostBase"),
                        format!("{r}CostNoMult"),
                    ] {
                        if !names.contains(&name) {
                            names.push(name);
                        }
                    }
                }
                return names;
            }
        }
    }
    Vec::new()
}

impl CalculationSession {
    pub fn detail_sources(&self, stat: &str) -> Vec<SourceGroup<'_>> {
        let db = &self.env.player.mod_db;
        let mut cfg = self.env.cfg.clone();
        if stat.starts_with("ManaRegen") && db.flag(&cfg, "Condition:NoInherentManaRegen") {
            cfg.conditions.insert("NoInherentManaRegen".into(), true);
        }
        if stat.starts_with("Req") {
            cfg.flags = pobr_data::prelude::ModFlags::NONE;
            cfg.keyword_flags = pobr_data::prelude::KeywordFlags::NONE;
        }
        source_names(stat)
            .into_iter()
            .map(|name| {
                let names = [ModName::from(name.as_str())];
                SourceGroup {
                    totals: (
                        db.sum(ModType::Base, &cfg, &names),
                        db.sum(ModType::Inc, &cfg, &names),
                        db.more(&cfg, &names),
                    ),
                    more_precision: db.more_precision_for(&name),
                    rows: db.evaluate_names(&cfg, &names),
                    name,
                }
            })
            .collect()
    }

    pub fn derived_detail(&self, stat: &str) -> Option<DerivedDetail> {
        let out = self.output();
        let mut context = Vec::new();
        let (formula, inputs): (String, Vec<(String, f64)>) = match stat {
            "Duration" | "DurationSecondary" | "AuraDuration" | "ReserveDuration" => {
                let t = super::super::skill_mechanics::duration_terms(
                    &self.env.player.mod_db,
                    &self.env.enemy.mod_db,
                    &self.env.cfg,
                    stat,
                );
                ("round(ceil(max(0, BaseDuration) * DurationModifier * DebuffExpiration / ServerTick) * ServerTick, 9)".into(), vec![("BaseDuration".into(),t.base), ("DurationModifier".into(),t.modifier), ("DebuffExpiration".into(),t.expiration), ("ServerTick".into(),t.tick)])
            }
            "ReqStr" | "ReqDex" | "ReqInt" => {
                let terms = super::super::skill_mechanics::attribute_requirement_terms(
                    &self.env.player.mod_db,
                    &self.env.cfg,
                );
                let i = match stat {
                    "ReqStr" => 0,
                    "ReqDex" => 1,
                    _ => 2,
                };
                let names = [
                    "SupportRequirement",
                    "ItemRequirement",
                    "WeaponRequirement",
                    "MeleeWeaponRequirement",
                    "GemRequirement",
                    "MeleeGemRequirement",
                ];
                (
                    format!("max(0, {})", names.join(", ")),
                    names
                        .into_iter()
                        .zip(terms[i])
                        .map(|(id, value)| (id.into(), value))
                        .collect(),
                )
            }
            "FireResistOverCap"
            | "ColdResistOverCap"
            | "LightningResistOverCap"
            | "ChaosResistOverCap" => {
                let input = super::super::perform::player_input(&self.env);
                let (element, base) = match stat {
                    "FireResistOverCap" => ("Fire", input.base_fire_resistance),
                    "ColdResistOverCap" => ("Cold", input.base_cold_resistance),
                    "LightningResistOverCap" => ("Lightning", input.base_lightning_resistance),
                    _ => ("Chaos", 0.0),
                };
                let terms = super::super::offence::resolve_resistance(
                    &self.env.player.mod_db,
                    &self.env.cfg,
                    base,
                    element,
                    element != "Chaos",
                );
                (
                    "round(max(0, UncappedResistance - MaximumResistance), 9)".into(),
                    vec![
                        ("UncappedResistance".into(), terms.total),
                        ("MaximumResistance".into(), terms.uncapped_max),
                    ],
                )
            }
            "LifeLeechGainRate"
            | "ManaLeechGainRate"
            | "EnergyShieldLeechGainRate"
            | "LifeLeechGainPerHit"
            | "ManaLeechGainPerHit"
            | "EnergyShieldLeechGainPerHit" => {
                let resource = if stat.starts_with("Life") {
                    0
                } else if stat.starts_with("Mana") {
                    1
                } else {
                    2
                };
                let rate = usize::from(stat.ends_with("Rate"));
                let values = self.env.leech_gain_terms[rate][resource];
                (
                    "SustainedLeech + InstantLeech + GainOnHit".into(),
                    ["SustainedLeech", "InstantLeech", "GainOnHit"]
                        .into_iter()
                        .zip(values)
                        .map(|(id, value)| (id.into(), value))
                        .collect(),
                )
            }
            "EsRechargePerSecond" => (
                "EnergyShield * EsRechargeRate".into(),
                vec![
                    ("EnergyShield".into(), out.energy_shield),
                    ("EsRechargeRate".into(), out.es_recharge_rate),
                ],
            ),
            "LifeUnreservedPercent" | "ManaUnreservedPercent" | "SpiritUnreservedPercent" => {
                let (resource, pool, unreserved) = match stat {
                    "LifeUnreservedPercent" => ("Life", out.life, out.life_unreserved),
                    "ManaUnreservedPercent" => ("Mana", out.mana, out.mana_unreserved),
                    _ => ("Spirit", out.spirit, out.spirit_unreserved),
                };
                (
                    if pool > 0.0 {
                        format!("{resource}Unreserved / {resource} * 100")
                    } else {
                        format!("0 * {resource}")
                    },
                    vec![
                        (resource.into(), pool),
                        (format!("{resource}Unreserved"), unreserved),
                    ],
                )
            }
            "LifeUnreserved" => (
                "max(0, Life - LifeReserved)".into(),
                vec![
                    ("Life".into(), out.life),
                    ("LifeReserved".into(), out.life_reserved),
                ],
            ),
            "ManaUnreserved" => (
                "max(0, Mana - ManaReserved)".into(),
                vec![
                    ("Mana".into(), out.mana),
                    ("ManaReserved".into(), out.mana_reserved),
                ],
            ),
            "SpiritUnreserved" => (
                "Spirit - SpiritReserved".into(),
                vec![
                    ("Spirit".into(), out.spirit),
                    ("SpiritReserved".into(), out.spirit_reserved),
                ],
            ),
            "TotalNetRegen" => (
                "NetLifeRegen + NetManaRegen + NetEnergyShieldRegen + NetWardRegen".into(),
                vec![
                    ("NetLifeRegen".into(), out.net_life_regen),
                    ("NetManaRegen".into(), out.net_mana_regen),
                    ("NetEnergyShieldRegen".into(), out.net_energy_shield_regen),
                    ("NetWardRegen".into(), out.net_ward_regen),
                ],
            ),
            "NetLifeRegen"
            | "NetManaRegen"
            | "NetEnergyShieldRegen"
            | "NetWardRegen"
            | "TotalBuildDegen" => {
                let terms = super::super::recovery::net_recovery_terms(&self.env);
                if stat == "TotalBuildDegen" {
                    (
                        "ResourceDrain + SelfDamage + Spending".into(),
                        vec![
                            ("ResourceDrain".into(), terms.direct_drain),
                            ("SelfDamage".into(), terms.typed_loss),
                            ("Spending".into(), terms.costs.iter().sum()),
                        ],
                    )
                } else {
                    let (i, resource, recovered) = match stat {
                        "NetLifeRegen" => (0, "Life", out.life_regen_recovery),
                        "NetManaRegen" => (1, "Mana", out.mana_regen_recovery),
                        "NetEnergyShieldRegen" => {
                            (2, "EnergyShield", out.energy_shield_regen_recovery)
                        }
                        _ => (3, "Ward", out.ward_regen_recovery),
                    };
                    (
                        format!("{resource}RegenRecovery - Spending - SelfDamage"),
                        vec![
                            (format!("{resource}RegenRecovery"), recovered),
                            ("Spending".into(), terms.costs[i]),
                            (
                                "SelfDamage".into(),
                                if i < 3 {
                                    terms.loss[i] - terms.costs[i]
                                } else {
                                    0.0
                                },
                            ),
                        ],
                    )
                }
            }
            "AreaOfEffectRadiusMetres" => (
                "AoeRadius / 10".into(),
                vec![("AoeRadius".into(), out.aoe_radius)],
            ),
            "TotalEHP" => {
                let terms = self.ehp_terms()?;
                context.extend([
                    ("LifeRecoverable".into(), out.life_recoverable),
                    ("ManaUnreserved".into(), out.mana_unreserved),
                    (
                        "EnergyShieldRecoveryCap".into(),
                        out.energy_shield_recovery_cap,
                    ),
                    ("Ward".into(), out.ward),
                ]);
                (
                    if terms.enemy_damage_in <= 0.0 {
                        "0 * EnemyDamage"
                    } else {
                        "round(NumberOfMitigatedHits * EnemyDamage / (1 - NotHitChance / 100), 9)"
                    }
                    .into(),
                    vec![
                        ("NumberOfMitigatedHits".into(), terms.mitigated_hits),
                        ("EnemyDamage".into(), terms.enemy_damage_in),
                        ("NotHitChance".into(), terms.not_hit_chance),
                    ],
                )
            }
            _ => {
                let mut cost = None;
                for (i, resource) in ["Mana", "Life", "ES", "Rage", "Ward"]
                    .into_iter()
                    .enumerate()
                {
                    for percent in [false, true] {
                        let suffix = if percent { "Percent" } else { "" };
                        if stat == format!("{resource}{suffix}PerSecondCost") {
                            let costs = super::super::skill_mechanics::resource_costs(
                                &self.env.player.mod_db,
                                &self.env.cfg,
                            );
                            cost = Some((
                                format!(
                                    "round({resource}{suffix}Cost * CostUseRate + ContinuousCost, 9)"
                                ),
                                vec![
                                    (
                                        format!("{resource}{suffix}Cost"),
                                        if percent {
                                            costs.percent[i]
                                        } else {
                                            costs.upfront[i]
                                        },
                                    ),
                                    (
                                        "CostUseRate".into(),
                                        super::super::perform::resource_cost_use_rate(&self.env),
                                    ),
                                    (
                                        "ContinuousCost".into(),
                                        if percent {
                                            costs.percent_per_second[i]
                                        } else {
                                            costs.per_second[i]
                                        },
                                    ),
                                ],
                            ));
                        }
                    }
                }
                cost?
            }
        };
        Some(DerivedDetail {
            formula,
            inputs,
            context,
        })
    }
}
