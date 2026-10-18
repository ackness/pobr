//! Optional views of terms already resolved by the calculation stages.
use super::{CalculationSession, detail::DerivedDetail};
use crate::calc::{perform, skill_dot, survivability::ChargeKind};

pub const DERIVED_STATS: &[&str] = &[
    "RemnantManaRecoverablePerPickup",
    "RemnantManaRecoverablePerSecond",
    "CreatedRemnantEffectGrantsPerPickup",
    "CreatedRemnantEffectGrantsPerSecond",
    "CreatedRemnantPotentialEffectGrantsPerUse",
    "VolatilityExplosionQuantity",
    "VolatilityRestoredPerExplosion",
    "VolatilityRestoredPerSecond",
    "DynamismChargeInterval",
    "DynamismChargeEventRate",
    "DynamismAllyChargeEventRate",
    "PhysicalResistTakenHitMulti",
    "FireResistTakenHitMulti",
    "ColdResistTakenHitMulti",
    "LightningResistTakenHitMulti",
    "ChaosResistTakenHitMulti",
    "PhysicalAfterReductionTakenHitMulti",
    "FireAfterReductionTakenHitMulti",
    "ColdAfterReductionTakenHitMulti",
    "LightningAfterReductionTakenHitMulti",
    "ChaosAfterReductionTakenHitMulti",
    "PhysicalBaseTakenHitMult",
    "FireBaseTakenHitMult",
    "ColdBaseTakenHitMult",
    "LightningBaseTakenHitMult",
    "ChaosBaseTakenHitMult",
    "PhysicalDamageReductionMax",
    "FireDamageReductionMax",
    "ColdDamageReductionMax",
    "LightningDamageReductionMax",
    "ChaosDamageReductionMax",
    "PhysicalManaEffectiveLife",
    "FireManaEffectiveLife",
    "ColdManaEffectiveLife",
    "LightningManaEffectiveLife",
    "ChaosManaEffectiveLife",
    "PhysicalMoMHitPool",
    "FireMoMHitPool",
    "ColdMoMHitPool",
    "LightningMoMHitPool",
    "ChaosMoMHitPool",
    "PhysicalTotalHitPool",
    "FireTotalHitPool",
    "ColdTotalHitPool",
    "LightningTotalHitPool",
    "ChaosTotalHitPool",
    "DamageReductionMax",
    "LifeHitPool",
    "FireResistTotal",
    "ColdResistTotal",
    "LightningResistTotal",
    "ChaosResistTotal",
    "FireResistOverTime",
    "ColdResistOverTime",
    "LightningResistOverTime",
    "ChaosResistOverTime",
    "PhysicalTakenDotMult",
    "FireTakenDotMult",
    "ColdTakenDotMult",
    "LightningTakenDotMult",
    "ChaosTakenDotMult",
    "PhysicalTotalPool",
    "FireTotalPool",
    "ColdTotalPool",
    "LightningTotalPool",
    "ChaosTotalPool",
    "PhysicalDotEHP",
    "FireDotEHP",
    "ColdDotEHP",
    "LightningDotEHP",
    "ChaosDotEHP",
    "EHPSurvivalTime",
    "LowLifePercentage",
    "LifeCancellableReservation",
    "ManaCancellableReservation",
    "FullLifePercentage",
    "FullManaPercentage",
    "DurationMod",
    "StrikeTargets",
    "LowestAttribute",
    "MaximumChill",
    "MaximumShock",
    "CurrentChill",
    "CurrentShock",
    "HeavyStunBuildupAvg",
    "PinBuildupAvg",
    "PinBuildupRequired",
    "PinDuration",
    "PhysicalTakenDamage",
    "FireTakenDamage",
    "ColdTakenDamage",
    "LightningTakenDamage",
    "ChaosTakenDamage",
    "PhysicalTakenHit",
    "FireTakenHit",
    "ColdTakenHit",
    "LightningTakenHit",
    "ChaosTakenHit",
    "PhysicalTakenHitMult",
    "FireTakenHitMult",
    "ColdTakenHitMult",
    "LightningTakenHitMult",
    "ChaosTakenHitMult",
    "FireDamageReduction",
    "ColdDamageReduction",
    "LightningDamageReduction",
    "totalEnemyDamage",
    "totalTakenDamage",
    "totalTakenHit",
    "ConfiguredDamageChance",
    "ConfiguredNotHitChance",
    "DamageTakenOnBlock",
    "PhysicalMin",
    "PhysicalMax",
    "PhysicalEffMult",
    "FireMin",
    "FireMax",
    "FireEffMult",
    "ColdMin",
    "ColdMax",
    "ColdEffMult",
    "LightningMin",
    "LightningMax",
    "LightningEffMult",
    "ChaosMin",
    "ChaosMax",
    "ChaosEffMult",
    "TotalMin",
    "TotalMax",
    "CritEffect",
    "DpsMultiplier",
    "MainHandPhysicalMin",
    "MainHandPhysicalMax",
    "MainHandPhysicalEffMult",
    "MainHandFireMin",
    "MainHandFireMax",
    "MainHandFireEffMult",
    "MainHandColdMin",
    "MainHandColdMax",
    "MainHandColdEffMult",
    "MainHandLightningMin",
    "MainHandLightningMax",
    "MainHandLightningEffMult",
    "MainHandChaosMin",
    "MainHandChaosMax",
    "MainHandChaosEffMult",
    "MainHandTotalMin",
    "MainHandTotalMax",
    "MainHandCritEffect",
    "MainHandDpsMultiplier",
    "OffHandPhysicalMin",
    "OffHandPhysicalMax",
    "OffHandPhysicalEffMult",
    "OffHandFireMin",
    "OffHandFireMax",
    "OffHandFireEffMult",
    "OffHandColdMin",
    "OffHandColdMax",
    "OffHandColdEffMult",
    "OffHandLightningMin",
    "OffHandLightningMax",
    "OffHandLightningEffMult",
    "OffHandChaosMin",
    "OffHandChaosMax",
    "OffHandChaosEffMult",
    "OffHandTotalMin",
    "OffHandTotalMax",
    "OffHandCritEffect",
    "OffHandDpsMultiplier",
    "Repeats",
    "TotalNumberOfHits",
    "BeiraIgniteDPS",
    "CombinedIgniteDPS",
    "IgniteChancePerHit",
    "IgniteDuration",
    "IgniteEffMult",
    "IgniteMagnitudeEffect",
    "IgniteRollAverage",
    "IgniteStacksMax",
    "IgniteStackPotentialPercent",
    "IgniteFireMin",
    "IgniteFireMax",
    "IgniteColdMin",
    "IgniteColdMax",
    "IgniteLightningMin",
    "IgniteLightningMax",
    "BleedChance",
    "BleedDuration",
    "BleedEffMult",
    "BleedMagnitudeEffect",
    "BleedRollAverage",
    "BleedStacksMax",
    "BleedStackPotentialPercent",
    "BleedPhysicalMin",
    "BleedPhysicalMax",
    "PoisonChance",
    "PoisonDuration",
    "PoisonEffMult",
    "PoisonMagnitudeEffect",
    "PoisonRollAverage",
    "PoisonStacksMax",
    "PoisonStackPotentialPercent",
    "PoisonPhysicalMin",
    "PoisonPhysicalMax",
    "PoisonChaosMin",
    "PoisonChaosMax",
    "ShockChance",
    "ShockDuration",
    "ChillDuration",
    "FreezeDuration",
    "ChillEffectMod",
    "FreezeBuildupAvg",
    "AverageBurstDamage",
    "AverageBurstHits",
    "MaxLifeLeechRate",
    "MaxManaLeechRate",
    "MaxEnergyShieldLeechRate",
    "MaxLifeLeechRatePercent",
    "CullMultiplier",
    "ReservationDpsMultiplier",
    "EnemyAilmentThreshold",
    "CombinedAvg",
    "CombinedAvgToMonsterLife",
    "HitSpeed",
    "IgniteDamage",
    "BleedDamage",
    "PoisonDamage",
    "WithIgniteDPS",
    "WithBleedDPS",
    "WithPoisonDPS",
    "WithDotDPS",
    "CullingDPS",
    "ReservationDPS",
    "MirageDPS",
    "BurningGroundDPS",
    "MirageBurningGroundDPS",
    "DecayDPS",
    "TotalDot",
    "LifeRecoverable",
    "EnergyShieldRecoveryCap",
    "PowerChargesDuration",
    "FrenzyChargesDuration",
    "EnduranceChargesDuration",
    "ChargePowerCurrent",
    "ChargeFrenzyCurrent",
    "ChargeEnduranceCurrent",
    "PhysicalMaxHit",
    "FireMaxHit",
    "ColdMaxHit",
    "LightningMaxHit",
    "ChaosMaxHit",
    "StunThreshold",
    "EnemyCritEffect",
    "EnemyCritChance",
    "SecondMinimalMaximumHitTaken",
    "GemLevel",
    "MainHandAccuracy",
    "OffHandAccuracy",
    "AverageDamage",
    "AccuracyHitChance",
    "AccuracyHitChanceUncapped",
    "PreEffectiveCritChance",
    "PreEffectiveCritMultiplier",
    "FullDPS",
    "FullDotDPS",
    "PhysicalDamageReduction",
    "CritBifurcates",
    "EffectiveActionRate",
    "StunDuration",
    "DeflectionRating",
    "DeflectChance",
    "DeflectEffect",
    "LifeLeechRate",
    "ManaLeechRate",
    "EsLeechRate",
    "LifeRecoupRate",
    "ManaRecoupRate",
    "EsRecoupRate",
    "BleedActiveStacks",
    "PoisonActiveStacks",
    "BleedStackedDPS",
    "PoisonStackedDPS",
    "AvoidStun",
    "SelfStunChance",
    "EvadeChance",
    "MeleeEvadeChance",
    "ProjectileEvadeChance",
    "SpellEvadeChance",
    "SpellProjectileEvadeChance",
    "ChannelTimeToTrigger",
    "ArchonDuration",
    "ArchonRecoveryPeriod",
    "TotalEHPLowestMaxHit",
    "ArmourBreakPerHit",
    "HitsToFullyBreakArmour",
    "TimeToFullyBreakArmour",
    "DazeUptime",
    "MaimUptime",
    "BlindUptime",
    "MaximumFireInfusions",
    "MaximumColdInfusions",
    "MaximumLightningInfusions",
    "SurroundedRadiusMetres",
    "CurseActivationDelay",
    "RemnantsPerUse",
    "TotemLife",
    "TotemArmour",
    "ActiveTotemLimit",
    "TotemEvasion",
    "TotemEnergyShield",
    "TotemDurationMod",
    "TotemFireResist",
    "TotemColdResist",
    "TotemLightningResist",
    "TotemChaosResist",
    "TotemFireResistTotal",
    "TotemColdResistTotal",
    "TotemLightningResistTotal",
    "TotemChaosResistTotal",
    "MissingTotemFireResist",
    "MissingTotemColdResist",
    "MissingTotemLightningResist",
    "MissingTotemChaosResist",
];

impl CalculationSession {
    pub(super) fn derived_view(&self, stat: &str) -> Option<DerivedDetail> {
        if crate::calc::hit_display::is_display_stat(stat) {
            return crate::calc::hit_display::detail(&self.env, stat);
        }
        if crate::calc::ailment_display::is_display_stat(stat) {
            return crate::calc::ailment_display::detail(&self.env, stat);
        }
        if crate::calc::limit_display::has_derived_detail(stat) {
            return crate::calc::limit_display::detail(&self.env, stat);
        }
        if crate::calc::effect_views::has_derived_detail(stat) {
            return crate::calc::effect_views::detail(&self.env, stat);
        }
        if let Some(detail) = crate::calc::totem::detail(&self.env, stat) {
            return Some(detail);
        }
        let env = &self.env;
        let out = &env.player.output;
        let db = &env.player.mod_db;
        let cfg = &env.cfg;
        let mut context = Vec::new();
        let one = |id: &str, value| (id.to_string(), vec![(id.to_string(), value)]);
        let (formula, inputs) = match stat {
            "HeavyStunBuildupAvg" => {
                let t = env.heavy_stun_terms.as_ref()?;
                if !t.enabled || t.threshold <= 0.0 {
                    (
                        "0 * CanHeavyStun".into(),
                        vec![("CanHeavyStun".into(), 0.0)],
                    )
                } else {
                    (
                        "(PhysicalHitAverage * PhysicalStunMultiplier + OtherStunHitAverage) * MeleeStunMultiplier * round(HeavyStunDamageScale / EnemyPoiseThreshold * (1 + StunBuildupIncreased / 100) * StunBuildupMore * 100, 9) * EnemyStunBuildupMultiplier".into(),
                        vec![
                            ("PhysicalHitAverage".into(), t.physical),
                            ("PhysicalStunMultiplier".into(), 1.5),
                            ("OtherStunHitAverage".into(), t.other),
                            ("MeleeStunMultiplier".into(), t.melee_multiplier),
                            ("HeavyStunDamageScale".into(), t.damage_scale),
                            ("EnemyPoiseThreshold".into(), t.threshold),
                            ("StunBuildupIncreased".into(), t.increased),
                            ("StunBuildupMore".into(), t.more),
                            ("EnemyStunBuildupMultiplier".into(), t.enemy_multiplier),
                        ],
                    )
                }
            }
            "PinBuildupAvg" => {
                let t = env.pin_terms.as_ref()?;
                let rows: Vec<_> = crate::calc::ailment::PIN_DAMAGE_TYPES
                    .iter()
                    .zip(t.damage)
                    .filter(|(_, damage)| *damage != 0.0)
                    .map(|(kind, damage)| (format!("{kind:?}PinningHitAverage"), damage))
                    .collect();
                if !t.enabled || t.threshold <= 0.0 || rows.is_empty() {
                    ("0 * CanPin".into(), vec![("CanPin".into(), 0.0)])
                } else {
                    let damage = rows
                        .iter()
                        .map(|(id, _)| id.as_str())
                        .collect::<Vec<_>>()
                        .join(" + ");
                    let mut inputs = rows;
                    inputs.extend([
                        ("PinDamageScale".into(), t.damage_scale),
                        ("EnemyPoiseThreshold".into(), t.threshold),
                        ("PinBuildupIncreased".into(), t.increased),
                        ("PinBuildupMore".into(), t.more),
                        ("EnemyPinBuildupMultiplier".into(), t.enemy_multiplier),
                    ]);
                    (
                        format!(
                            "({damage}) * round(PinDamageScale / EnemyPoiseThreshold * (1 + PinBuildupIncreased / 100) * PinBuildupMore * 100, 9) * EnemyPinBuildupMultiplier"
                        ),
                        inputs,
                    )
                }
            }
            "totalEnemyDamage" | "totalTakenDamage" | "totalTakenHit" => {
                let t = self.ehp_terms()?;
                let (suffix, values) = match stat {
                    "totalEnemyDamage" => ("EnemyDamage", t.enemy_damage),
                    "totalTakenDamage" => ("TakenDamage", t.taken_damage),
                    _ => ("TakenHit", t.taken_hit),
                };
                let rows: Vec<_> = ["Physical", "Fire", "Cold", "Lightning", "Chaos"]
                    .into_iter()
                    .zip(values)
                    .map(|(kind, value)| (format!("{kind}{suffix}"), value))
                    .collect();
                (
                    rows.iter()
                        .map(|(id, _)| id.as_str())
                        .collect::<Vec<_>>()
                        .join(" + "),
                    rows,
                )
            }
            "ConfiguredDamageChance" => {
                let t = self.ehp_terms()?;
                ("100 * BlockEffectMultiplier * DeflectMultiplier * (1 - AverageAvoidChance / 100)".into(), vec![
                    ("BlockEffectMultiplier".into(), t.block_effect_multiplier),
                    ("DeflectMultiplier".into(), t.deflect_multiplier),
                    ("AverageAvoidChance".into(), t.average_avoid_chance),
                ])
            }
            "ConfiguredNotHitChance" => {
                let t = self.ehp_terms()?;
                (
                    "100 * pow(clamp(NotHitBeforeUnlucky / 100, 0, 1), UnluckyWorstOf)".into(),
                    vec![
                        ("NotHitBeforeUnlucky".into(), t.not_hit_before_unlucky),
                        ("UnluckyWorstOf".into(), t.unlucky_worst_of),
                    ],
                )
            }
            "DamageTakenOnBlock" => (
                "100 - BlockEffect".into(),
                vec![("BlockEffect".into(), 100.0 - out.block_effect)],
            ),
            "PhysicalTakenDamage"
            | "FireTakenDamage"
            | "ColdTakenDamage"
            | "LightningTakenDamage"
            | "ChaosTakenDamage"
            | "PhysicalTakenHit"
            | "FireTakenHit"
            | "ColdTakenHit"
            | "LightningTakenHit"
            | "ChaosTakenHit"
            | "PhysicalTakenHitMult"
            | "FireTakenHitMult"
            | "ColdTakenHitMult"
            | "LightningTakenHitMult"
            | "ChaosTakenHitMult" => {
                let t = self.ehp_terms()?;
                let (i, kind) = ["Physical", "Fire", "Cold", "Lightning", "Chaos"]
                    .into_iter()
                    .enumerate()
                    .find(|(_, kind)| stat.starts_with(kind))?;
                match &stat[kind.len()..] {
                    "TakenDamage" => {
                        let mut rows = Vec::new();
                        let mut products = Vec::new();
                        for (src, source) in ["Physical", "Fire", "Cold", "Lightning", "Chaos"].into_iter().enumerate() {
                            rows.push((format!("{source}EnemyDamage"), t.enemy_damage[src]));
                            rows.push((format!("{source}TakenAsFraction"), t.damage_shift[src][i]));
                            products.push(format!("max({source}EnemyDamage, 0) * {source}TakenAsFraction"));
                        }
                        (products.join(" + "), rows)
                    }
                    "TakenHit" => (
                        "max(ConvertedIncomingDamage * ResistTakenMultiplier * ReductionTakenMultiplier + FlatDamageTaken, 0) * AfterReductionMultiplier".into(),
                        vec![
                            ("ConvertedIncomingDamage".into(), t.taken_damage[i]),
                            ("ResistTakenMultiplier".into(), t.resist_taken_multi[i]),
                            ("ReductionTakenMultiplier".into(), t.damage_reduction[i].taken_multiplier()),
                            ("FlatDamageTaken".into(), t.taken_flat[i]),
                            ("AfterReductionMultiplier".into(), t.after_reduction_multi[i]),
                        ],
                    ),
                    "TakenHitMult" => (
                        if t.taken_damage[i] > 0.0 { "MitigatedHit / ConvertedIncomingDamage" }
                        else { "0 * ConvertedIncomingDamage" }.into(),
                        vec![
                            ("MitigatedHit".into(), t.taken_hit[i]),
                            ("ConvertedIncomingDamage".into(), t.taken_damage[i]),
                        ],
                    ),
                    _ => unreachable!(),
                }
            }

            "DamageReductionMax" => {
                let m = &self.ehp_terms()?.mitigation;
                if m.dr_max_global_modifier > 0.0 {
                    one("HighestDamageReductionMax", m.dr_max_global_modifier)
                } else {
                    one("DamageReductionCap", m.dr_max_global)
                }
            }
            "LifeHitPool" => {
                let t = self.ehp_terms()?.life_hit_pool_terms;
                (
                    "max(Life - MaximumLife / 2, 0) / (1 - LifeLossPrevented / 100) + min(Life, MaximumLife / 2) / (1 - LifeLossBelowHalfPrevented / 100) / (1 - LifeLossPrevented / 100)".into(),
                    vec![
                        ("Life".into(), t.life),
                        ("MaximumLife".into(), t.max_life),
                        ("LifeLossPrevented".into(), t.loss_prevented),
                        ("LifeLossBelowHalfPrevented".into(), t.below_half_loss_prevented),
                    ],
                )
            }
            "FireResistTotal" | "ColdResistTotal" | "LightningResistTotal" | "ChaosResistTotal" => {
                let input = perform::player_input(env);
                let (element, base) = match stat {
                    "FireResistTotal" => ("Fire", input.base_fire_resistance),
                    "ColdResistTotal" => ("Cold", input.base_cold_resistance),
                    "LightningResistTotal" => ("Lightning", input.base_lightning_resistance),
                    _ => ("Chaos", 0.0),
                };
                let r = crate::calc::offence::resolve_resistance(
                    db,
                    cfg,
                    base,
                    element,
                    element != "Chaos",
                );
                // Resistance truncates toward zero: floor when positive, ceil when negative.
                let raw = r.override_total.unwrap_or_else(|| {
                    (r.base + r.flat) * ((1.0 + r.inc / 100.0) * r.more).max(0.0)
                });
                let toward_zero = if raw >= 0.0 { "floor" } else { "ceil" };
                match r.override_total {
                    Some(value) => (
                        format!("{toward_zero}(OverrideResistance)"),
                        vec![("OverrideResistance".into(), value)],
                    ),
                    None => (
                        format!(
                            "{toward_zero}((BaseResistance + FlatResistance) * max((1 + IncreasedResistance / 100) * MoreResistance, 0))"
                        ),
                        vec![
                            ("BaseResistance".into(), r.base),
                            ("FlatResistance".into(), r.flat),
                            ("IncreasedResistance".into(), r.inc),
                            ("MoreResistance".into(), r.more),
                        ],
                    ),
                }
            }
            "PhysicalResistTakenHitMulti"
            | "FireResistTakenHitMulti"
            | "ColdResistTakenHitMulti"
            | "LightningResistTakenHitMulti"
            | "ChaosResistTakenHitMulti"
            | "PhysicalAfterReductionTakenHitMulti"
            | "FireAfterReductionTakenHitMulti"
            | "ColdAfterReductionTakenHitMulti"
            | "LightningAfterReductionTakenHitMulti"
            | "ChaosAfterReductionTakenHitMulti"
            | "PhysicalBaseTakenHitMult"
            | "FireBaseTakenHitMult"
            | "ColdBaseTakenHitMult"
            | "LightningBaseTakenHitMult"
            | "ChaosBaseTakenHitMult"
            | "PhysicalDamageReductionMax"
            | "FireDamageReductionMax"
            | "ColdDamageReductionMax"
            | "LightningDamageReductionMax"
            | "ChaosDamageReductionMax"
            | "PhysicalManaEffectiveLife"
            | "FireManaEffectiveLife"
            | "ColdManaEffectiveLife"
            | "LightningManaEffectiveLife"
            | "ChaosManaEffectiveLife"
            | "PhysicalMoMHitPool"
            | "FireMoMHitPool"
            | "ColdMoMHitPool"
            | "LightningMoMHitPool"
            | "ChaosMoMHitPool"
            | "PhysicalTotalHitPool"
            | "FireTotalHitPool"
            | "ColdTotalHitPool"
            | "LightningTotalHitPool"
            | "ChaosTotalHitPool" => {
                let t = self.ehp_terms()?;
                let (i, kind) = ["Physical", "Fire", "Cold", "Lightning", "Chaos"]
                    .into_iter()
                    .enumerate()
                    .find(|(_, kind)| stat.starts_with(kind))?;
                let m = &t.mitigation;
                match &stat[kind.len()..] {
                    "ResistTakenHitMulti" => (
                        if t.resist_before_pen[i] > 0.0 { "1 - max(Resistance - EnemyPenetration, 0) / 100" }
                        else { "1 - Resistance / 100 + 0 * EnemyPenetration" }.into(),
                        vec![
                            ("Resistance".into(), t.resist_before_pen[i]),
                            ("EnemyPenetration".into(), t.enemy_pen[i]),
                        ],
                    ),
                    "AfterReductionTakenHitMulti" => (
                        "HitDamageTakenMultiplier * DeflectMultiplier".into(),
                        vec![
                            ("HitDamageTakenMultiplier".into(), m.hit_taken_multi[i]),
                            ("DeflectMultiplier".into(), m.deflect_multi),
                        ],
                    ),
                    "BaseTakenHitMult" => (
                        "ResistTakenMultiplier * ReductionTakenMultiplier * AfterReductionMultiplier".into(),
                        vec![
                            ("ResistTakenMultiplier".into(), t.resist_taken_multi[i]),
                            ("ReductionTakenMultiplier".into(), t.damage_reduction[i].taken_multiplier()),
                            ("AfterReductionMultiplier".into(), t.after_reduction_multi[i]),
                        ],
                    ),
                    "DamageReductionMax" => {
                        let type_cap = format!("Highest{kind}DamageReductionMax");
                        if m.dr_max_type_modifier[i] > 0.0 {
                            (
                                format!("min({type_cap}, DamageReductionMax)"),
                                vec![
                                    (type_cap, m.dr_max_type_modifier[i]),
                                    ("DamageReductionMax".into(), m.dr_max_global),
                                ],
                            )
                        } else {
                            one("DamageReductionMax", m.dr_max_global)
                        }
                    }
                    "ManaEffectiveLife" => (
                        "LifeRecoverable + ManaThroughMindOverMatter".into(),
                        vec![
                            ("LifeRecoverable".into(), t.life_hit_pool_terms.life),
                            ("ManaThroughMindOverMatter".into(), t.mom.effective_life_by_type[i] - t.life_hit_pool_terms.life),
                        ],
                    ),
                    "MoMHitPool" => (
                        "LifeHitPool + ManaThroughMindOverMatter".into(),
                        vec![
                            ("LifeHitPool".into(), t.mom.life_hit_pool),
                            ("ManaThroughMindOverMatter".into(), t.mom.hit_pool_by_type[i] - t.mom.life_hit_pool),
                        ],
                    ),
                    "TotalHitPool" => (
                        "MoMHitPool + EnergyShieldInPool + WardAegisGuardAllyLayers".into(),
                        vec![
                            ("MoMHitPool".into(), t.mom.hit_pool_by_type[i]),
                            ("EnergyShieldInPool".into(), t.base_hit_pool[i] - t.mom.hit_pool_by_type[i]),
                            ("WardAegisGuardAllyLayers".into(), t.total_hit_pool[i] - t.base_hit_pool[i]),
                        ],
                    ),
                    _ => unreachable!(),
                }
            }
            "FireResistOverTime"
            | "ColdResistOverTime"
            | "LightningResistOverTime"
            | "ChaosResistOverTime" => {
                let input = perform::player_input(env);
                let (element, base) = match stat {
                    "FireResistOverTime" => ("Fire", input.base_fire_resistance),
                    "ColdResistOverTime" => ("Cold", input.base_cold_resistance),
                    "LightningResistOverTime" => ("Lightning", input.base_lightning_resistance),
                    _ => ("Chaos", 0.0),
                };
                let r = crate::calc::offence::resolve_resistance_over_time(
                    db,
                    cfg,
                    base,
                    element,
                    element != "Chaos",
                );
                let raw = r.override_total.unwrap_or_else(|| {
                    (r.base + r.flat) * ((1.0 + r.inc / 100.0) * r.more).max(0.0)
                });
                // Resistance truncates toward zero: floor when positive, ceil when negative.
                let toward_zero = if raw >= 0.0 { "floor" } else { "ceil" };
                let mut inputs = vec![
                    ("MaximumResistance".to_string(), r.max),
                    ("ResistFloor".to_string(), r.floor),
                ];
                let total = match r.override_total {
                    Some(value) => {
                        inputs.push(("OverrideResistance".into(), value));
                        format!("{toward_zero}(OverrideResistance)")
                    }
                    None => {
                        inputs.extend([
                            ("BaseResistance".into(), r.base),
                            ("FlatResistance".into(), r.flat),
                            ("IncreasedResistance".into(), r.inc),
                            ("MoreResistance".into(), r.more),
                        ]);
                        format!(
                            "{toward_zero}((BaseResistance + FlatResistance) * max((1 + IncreasedResistance / 100) * MoreResistance, 0))"
                        )
                    }
                };
                (
                    format!("max(min({total}, MaximumResistance), ResistFloor)"),
                    inputs,
                )
            }
            "PhysicalTakenDotMult"
            | "FireTakenDotMult"
            | "ColdTakenDotMult"
            | "LightningTakenDotMult"
            | "ChaosTakenDotMult"
            | "PhysicalTotalPool"
            | "FireTotalPool"
            | "ColdTotalPool"
            | "LightningTotalPool"
            | "ChaosTotalPool"
            | "PhysicalDotEHP"
            | "FireDotEHP"
            | "ColdDotEHP"
            | "LightningDotEHP"
            | "ChaosDotEHP" => {
                let t = self.ehp_terms()?;
                let (i, kind) = ["Physical", "Fire", "Cold", "Lightning", "Chaos"]
                    .into_iter()
                    .enumerate()
                    .find(|(_, kind)| stat.starts_with(kind))?;
                let dot = t.dot_taken[i];
                match &stat[kind.len()..] {
                    "TakenDotMult" if dot.immune => one("ImmuneToChaosDamage", 0.0),
                    "TakenDotMult" => (
                        "max((1 - Resistance / 100) * (1 - DamageReduction / 100) * OverTimeDamageTaken, 0)".into(),
                        vec![
                            ("Resistance".into(), dot.resistance),
                            ("DamageReduction".into(), dot.reduction),
                            ("OverTimeDamageTaken".into(), dot.taken),
                        ],
                    ),
                    "TotalPool" => (
                        "ManaEffectiveLife + EnergyShieldInPool + RunicWardInPool".into(),
                        vec![
                            ("ManaEffectiveLife".into(), t.mom.effective_life_by_type[i]),
                            ("EnergyShieldInPool".into(), t.pool_before_ward[i] - t.mom.effective_life_by_type[i]),
                            ("RunicWardInPool".into(), t.total_pool[i] - t.pool_before_ward[i]),
                        ],
                    ),
                    "DotEHP" => (
                        "TotalPool / TakenDotMult".into(),
                        vec![
                            ("TotalPool".into(), t.total_pool[i]),
                            ("TakenDotMult".into(), dot.multiplier()),
                        ],
                    ),
                    _ => unreachable!(),
                }
            }
            "EHPSurvivalTime" => {
                let t = self.ehp_terms()?;
                if t.enemy_action_time_ms <= 0.0 {
                    // No configured enemy attack / cast time: no survival time.
                    one("EnemyActionTime", 0.0)
                } else {
                    (
                        "TotalNumberOfHits * EnemyActionTime / 1000 / EnemyActionSpeed".into(),
                        vec![
                            ("TotalNumberOfHits".into(), out.total_number_of_hits),
                            ("EnemyActionTime".into(), t.enemy_action_time_ms),
                            ("EnemyActionSpeed".into(), t.enemy_action_speed),
                        ],
                    )
                }
            }
            "ArchonDuration" | "ArchonRecoveryPeriod" => {
                let timing = crate::rules::archon::timing(db, cfg);
                if stat == "ArchonDuration" {
                    (
                        "BaseDuration * DurationFactor".into(),
                        vec![
                            ("BaseDuration".into(), 10.0),
                            ("DurationFactor".into(), timing.duration_factor),
                        ],
                    )
                } else if timing.no_recovery {
                    (
                        "0 * BaseRecoveryPeriod".into(),
                        vec![("BaseRecoveryPeriod".into(), 20.0)],
                    )
                } else {
                    (
                        "BaseRecoveryPeriod / RecoveryRate".into(),
                        vec![
                            ("BaseRecoveryPeriod".into(), 20.0),
                            ("RecoveryRate".into(), timing.recovery_rate),
                        ],
                    )
                }
            }
            "TotalEHPLowestMaxHit" => {
                let t = env.legacy_ehp_terms.as_ref()?;
                let rows: Vec<_> = [
                    ("LegacyPhysicalMaxHit", t.physical_max_hit),
                    ("LegacyFireMaxHit", t.fire_max_hit),
                    ("LegacyColdMaxHit", t.cold_max_hit),
                    ("LegacyLightningMaxHit", t.lightning_max_hit),
                    ("LegacyChaosMaxHit", t.chaos_max_hit),
                ]
                .into_iter()
                .filter(|(_, value)| value.is_finite())
                .map(|(id, value)| (id.to_string(), value))
                .collect();
                if rows.is_empty() {
                    // The legacy stage falls back to its resource pool when every
                    // damage type is immune, instead of taking an infinite minimum.
                    one("LegacyImmunePool", t.total_ehp)
                } else {
                    (
                        format!(
                            "round(min({}), 9)",
                            rows.iter()
                                .map(|(id, _)| id.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        rows,
                    )
                }
            }
            "ChannelTimeToTrigger" => {
                let (maximum, gain) = crate::calc::energy::channel_trigger_terms(db, cfg);
                (
                    if maximum > 0.0 && gain > 0.0 {
                        "round(MaximumChannelEnergy / ChannelEnergyGain, 9)"
                    } else {
                        "0 * MaximumChannelEnergy"
                    }
                    .into(),
                    vec![
                        ("MaximumChannelEnergy".into(), maximum),
                        ("ChannelEnergyGain".into(), gain),
                    ],
                )
            }
            "EvadeChance"
            | "MeleeEvadeChance"
            | "ProjectileEvadeChance"
            | "SpellEvadeChance"
            | "SpellProjectileEvadeChance" => {
                use pobr_data::prelude::{ModName, ModType};
                let enemy = &env.enemy.mod_db;
                let names = [ModName::from("HitChance")];
                let enemy_multiplier =
                    (1.0 + enemy.sum(ModType::Inc, cfg, &names) / 100.0) * enemy.more(cfg, &names);
                let terms = crate::calc::defence::evade_terms(
                    db,
                    cfg,
                    out.evasion,
                    crate::calc::setup_env::enemy_accuracy(&env.enemy, cfg),
                    enemy_multiplier,
                    enemy.flag(cfg, "CannotBeEvaded"),
                );
                let i = [
                    "EvadeChance",
                    "MeleeEvadeChance",
                    "ProjectileEvadeChance",
                    "SpellEvadeChance",
                    "SpellProjectileEvadeChance",
                ]
                .iter()
                .position(|id| *id == stat)?;
                let t = terms[i];
                if let Some(value) = t.override_value {
                    one("EvadeOverride", value)
                } else {
                    let raw = "(100 - (EnemyChanceToHit - ExtraEvadeChance) * EnemyHitModifier) * EvadeModifier";
                    let bounded = if t.clamp_zero {
                        format!("clamp({raw}, 0, EvadeCap)")
                    } else {
                        format!("min({raw}, EvadeCap)")
                    };
                    let formula = if t.unlucky {
                        format!("round(pow({bounded}, 2) / 100, 9)")
                    } else {
                        format!("round({bounded}, 9)")
                    };
                    context.push(("Evasion".into(), out.evasion));
                    (
                        formula,
                        vec![
                            ("EnemyChanceToHit".into(), t.enemy_chance),
                            ("ExtraEvadeChance".into(), t.extra),
                            ("EnemyHitModifier".into(), t.enemy_multiplier),
                            ("EvadeModifier".into(), t.scaling),
                            ("EvadeCap".into(), t.cap),
                        ],
                    )
                }
            }
            "AvoidStun" | "SelfStunChance" => {
                let t = self.ehp_terms()?;
                let keystones = crate::rules::DefenceKeystones::from_db(db, cfg);
                let input = crate::calc::stun::StunInputs {
                    life: out.life,
                    life_base_flat: env.player.base.life,
                    energy_shield: out.energy_shield,
                    mana: out.mana,
                    total_taken_hit: t.taken_hit.iter().sum(),
                    physical_taken_hit: t.taken_hit[0],
                    avoid_stun: out.avoid_stun,
                    chaos_inoculation: keystones.chaos_inoculation,
                };
                if stat == "AvoidStun" {
                    let (not_avoid, shield_factor) = crate::calc::defence::stun_avoidance_terms(
                        db,
                        cfg,
                        input.energy_shield,
                        input.total_taken_hit,
                        keystones.energy_shield_protects_mana,
                    );
                    context.extend([
                        ("EnergyShield".into(), out.energy_shield),
                        ("DamageTakenPerHit".into(), input.total_taken_hit),
                    ]);
                    ("round(clamp(100 - StunChanceBeforeShield * ShieldStunMultiplier, 0, 100), 9)".into(), vec![
                        ("StunChanceBeforeShield".into(), not_avoid), ("ShieldStunMultiplier".into(), shield_factor),
                    ])
                } else {
                    let (base, gated, damage) =
                        crate::calc::stun::stun_chance_terms(cfg, &input, out.stun_threshold);
                    context.extend([
                        ("StunThreshold".into(), out.stun_threshold),
                        ("StunEffectiveDamage".into(), damage),
                        (
                            "MinimumStunChance".into(),
                            cfg.constants.game().min_stun_chance_needed,
                        ),
                    ]);
                    (
                        if gated > 0.0 {
                            "round(BaseStunChance * max(0, 100 - AvoidStun) / 100, 9)"
                        } else {
                            "0 * BaseStunChance"
                        }
                        .into(),
                        vec![
                            ("BaseStunChance".into(), base),
                            ("AvoidStun".into(), out.avoid_stun),
                        ],
                    )
                }
            }
            "BleedActiveStacks" | "PoisonActiveStacks" | "BleedStackedDPS" | "PoisonStackedDPS" => {
                let prefix = if stat.starts_with("Bleed") {
                    "Bleed"
                } else {
                    "Poison"
                };
                let Some(t) = self.ailment_stage_terms(&format!("{prefix}DPS")) else {
                    return Some(DerivedDetail {
                        formula: "0 * AilmentInstances".into(),
                        inputs: vec![("AilmentInstances".into(), 0.0)],
                        context,
                    });
                };
                if stat.ends_with("ActiveStacks") {
                    let mut rows = Vec::new();
                    for (i, (_, terms)) in t.passes.iter().enumerate() {
                        let prefix = if i == 0 { "Primary" } else { "Secondary" };
                        let (suffix, value) = if terms.stacks > 0.0 {
                            ("EstimatedStacks", terms.stacks)
                        } else {
                            ("MaximumStacks", terms.max_stacks)
                        };
                        rows.push((format!("{prefix}{suffix}"), value));
                    }
                    (
                        format!(
                            "max(0, {})",
                            rows.iter()
                                .map(|(id, _)| id.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        rows,
                    )
                } else if let Some(merge) = t.stacked_merge {
                    ("round(StrongerAilmentDPS * StrongerAilmentShare + WeakerAilmentDPS * (1 - StrongerAilmentShare), 9)".into(), vec![
                        ("StrongerAilmentDPS".into(), merge.stronger), ("WeakerAilmentDPS".into(), merge.weaker),
                        ("StrongerAilmentShare".into(), merge.stronger_share),
                    ])
                } else {
                    let terms = &t.passes.first()?.1;
                    let id = format!("{prefix}DPS");
                    if terms.stacks > 0.0 {
                        one(&id, terms.dps)
                    } else {
                        (
                            format!("min(round({id} * MaximumAilmentStacks, 9), DotDpsCap)"),
                            vec![
                                (id, terms.dps),
                                ("MaximumAilmentStacks".into(), terms.max_stacks),
                                ("DotDpsCap".into(), terms.cap),
                            ],
                        )
                    }
                }
            }
            "LifeLeechRate" | "ManaLeechRate" | "EsLeechRate" => {
                let i = match stat {
                    "LifeLeechRate" => 0,
                    "ManaLeechRate" => 1,
                    _ => 2,
                };
                let terms = perform::leech_panel_terms(env);
                let mut rows = vec![];
                let mut expressions = vec!["0".to_string()];
                for (index, pass) in terms.iter().enumerate() {
                    let prefix = if index == 0 { "Primary" } else { "Secondary" };
                    let t = &pass[i];
                    rows.extend([
                        (format!("{prefix}LeechedAmount"), t.instance_total),
                        (format!("{prefix}LeechRateCap"), t.rate_cap_per_second),
                        (
                            format!("{prefix}LeechInstanceRate"),
                            t.display_rate_per_second,
                        ),
                    ]);
                    expressions.push(format!("{prefix}LeechInstanceRate"));
                }
                if terms.is_empty() {
                    // No leech source: the only instance rate is zero.
                    rows.push(("PrimaryLeechInstanceRate".into(), 0.0));
                    expressions.push("PrimaryLeechInstanceRate".into());
                }
                (format!("max({})", expressions.join(", ")), rows)
            }
            "MaxLifeLeechRate" | "MaxManaLeechRate" | "MaxEnergyShieldLeechRate" => {
                let i = match stat {
                    "MaxLifeLeechRate" => 0,
                    "MaxManaLeechRate" => 1,
                    _ => 2,
                };
                let t = perform::leech_cap_terms_for(env)[i];
                (
                    "Pool * MaxRatePercent / 100".into(),
                    vec![
                        ("Pool".into(), t.pool.max(0.0)),
                        ("MaxRatePercent".into(), t.percent()),
                    ],
                )
            }
            "MaxLifeLeechRatePercent" => {
                let t = perform::leech_cap_terms_for(env)[0];
                (
                    "DefaultPercent * (1 + IncreasedMaximum / 100) * MoreMaximum".into(),
                    vec![
                        ("DefaultPercent".into(), t.default_percent),
                        ("IncreasedMaximum".into(), t.increased),
                        ("MoreMaximum".into(), t.more),
                    ],
                )
            }
            "LifeRecoupRate" => {
                use crate::calc::survivability::life_recoup_terms;
                let terms = self.ehp_terms()?;
                let t = life_recoup_terms(db, cfg, terms.recoupable_total, terms.prevented());
                ("round((RecoupableDamage * RecoupPercent + PhysicalDamagePrevented * PreventedRecoupPercent + ChaosDamagePrevented * ChaosPreventedRecoupPercent) / 100 / RecoupDuration * RecoupRecovery, 9)".into(), vec![
                        ("RecoupableDamage".into(), t.damage), ("RecoupPercent".into(), t.percent),
                        ("PhysicalDamagePrevented".into(), t.physical_prevented),
                        ("PreventedRecoupPercent".into(), t.physical_prevented_percent),
                        ("ChaosDamagePrevented".into(), t.chaos_prevented),
                        ("ChaosPreventedRecoupPercent".into(), t.chaos_prevented_percent),
                        ("RecoupDuration".into(), t.duration), ("RecoupRecovery".into(), t.recovery),
                    ])
            }
            "ManaRecoupRate" => {
                use crate::calc::survivability::mana_recoup_terms;
                let terms = self.ehp_terms()?;
                let t = mana_recoup_terms(db, cfg, terms.recoupable_total, terms.prevented());
                ("round((RecoupableDamage * RecoupPercent + ChaosDamagePrevented * ChaosPreventedRecoupPercent) / 100 / RecoupDuration * RecoupRecovery, 9)".into(), vec![
                        ("RecoupableDamage".into(), t.damage), ("RecoupPercent".into(), t.percent),
                        ("ChaosDamagePrevented".into(), t.chaos_prevented),
                        ("ChaosPreventedRecoupPercent".into(), t.chaos_prevented_percent),
                        ("RecoupDuration".into(), t.duration), ("RecoupRecovery".into(), t.recovery),
                    ])
            }
            "EsRecoupRate" => {
                use crate::calc::survivability::es_recoup_terms;
                let t = es_recoup_terms(db, cfg, self.ehp_terms()?.recoupable_by_type);
                let inputs = vec![
                    ("RecoupablePhysical".into(), t.recoupable_by_type[0]),
                    ("RecoupableFire".into(), t.recoupable_by_type[1]),
                    ("RecoupableCold".into(), t.recoupable_by_type[2]),
                    ("RecoupableLightning".into(), t.recoupable_by_type[3]),
                    ("RecoupableChaos".into(), t.recoupable_by_type[4]),
                    ("RecoupPctPhysical".into(), t.percent_by_type[0]),
                    ("RecoupPctFire".into(), t.percent_by_type[1]),
                    ("RecoupPctCold".into(), t.percent_by_type[2]),
                    ("RecoupPctLightning".into(), t.percent_by_type[3]),
                    ("RecoupPctChaos".into(), t.percent_by_type[4]),
                    ("RecoupDuration".into(), t.duration),
                    ("RecoupRecovery".into(), t.recovery),
                ];
                (if t.rate_per_second <= 0.0 { "0 * RecoupablePhysical" }
                    else { "round((RecoupablePhysical * RecoupPctPhysical + RecoupableFire * RecoupPctFire + RecoupableCold * RecoupPctCold + RecoupableLightning * RecoupPctLightning + RecoupableChaos * RecoupPctChaos) / 100 / RecoupDuration * max(0, RecoupRecovery), 9)" }.into(), inputs)
            }
            "DeflectionRating" | "DeflectChance" => {
                let (base, evasion, armour, scaling) =
                    crate::calc::defence_panels::deflection_rating_terms(
                        db,
                        cfg,
                        out.armour,
                        out.evasion,
                    );
                if stat == "DeflectionRating" {
                    ("round(DeflectionBase + (EvasionAsDeflection + ArmourAsDeflection) * DeflectionScaling, 9)".into(), vec![
                        ("DeflectionBase".into(), base), ("EvasionAsDeflection".into(), evasion),
                        ("ArmourAsDeflection".into(), armour), ("DeflectionScaling".into(), scaling),
                    ])
                } else {
                    let rating = base + (evasion + armour) * scaling;
                    let accuracy = crate::calc::setup_env::enemy_accuracy(&env.enemy, cfg);
                    let chance = crate::calc::defence_panels::deflect_chance_pct(
                        rating,
                        accuracy,
                        cfg.constants.game().deflection_chance_cap,
                    );
                    context.extend([
                        ("DeflectionRating".into(), rating),
                        ("EnemyAccuracy".into(), accuracy),
                    ]);
                    (
                        if db.flag(cfg, "DeflectIsLucky") {
                            "round((1 - pow(1 - DeflectBeforeLuck / 100, 2)) * 100, 9)"
                        } else {
                            "round(DeflectBeforeLuck, 9)"
                        }
                        .into(),
                        vec![("DeflectBeforeLuck".into(), chance)],
                    )
                }
            }
            "DeflectEffect" => {
                let t = crate::calc::defence_panels::deflect_effect_terms(db, cfg);
                (
                    "round(clamp(GameBase + AddedDeflectEffect, 0, 100), 9)".into(),
                    vec![
                        ("GameBase".into(), t.game_base),
                        ("AddedDeflectEffect".into(), t.added),
                    ],
                )
            }
            "StunDuration" => {
                let (base, increase, recovery, tick) =
                    crate::calc::stun::stun_duration_terms(db, cfg, out.avoid_stun);
                (if out.avoid_stun >= 100.0 { "0 * BaseStunDuration" } else { "round(ceil(BaseStunDuration * StunDurationIncrease / StunRecoveryModifier / ServerTick) * ServerTick, 9)" }.into(), vec![
                    ("BaseStunDuration".into(), base), ("StunDurationIncrease".into(), increase),
                    ("StunRecoveryModifier".into(), recovery), ("ServerTick".into(), tick),
                ])
            }
            "FullDPS" | "FullDotDPS" => {
                let rows: Vec<_> = out
                    .full_dps
                    .as_ref()
                    .into_iter()
                    .flat_map(|full| &full.skills)
                    .map(|skill| {
                        (
                            format!(
                                "SkillGroup{}Effect{}DPS",
                                skill.group_index + 1,
                                skill.active_skill_index
                            ),
                            if stat == "FullDPS" {
                                skill.dps
                            } else {
                                skill.dot_dps
                            },
                        )
                    })
                    .collect();
                if rows.is_empty() {
                    one("IncludedSkillsDPS", 0.0)
                } else {
                    (
                        rows.iter()
                            .map(|(id, _)| id.as_str())
                            .collect::<Vec<_>>()
                            .join(" + "),
                        rows,
                    )
                }
            }
            "EffectiveActionRate" => {
                if out
                    .energy_trigger
                    .as_ref()
                    .is_some_and(|terms| !terms.at_own_rate)
                {
                    one("SkillTriggerRate", out.skill_trigger_rate)
                } else if out.action_rate > 0.0 {
                    one("ActionRate", out.action_rate)
                } else {
                    one(
                        "FallbackUseRate",
                        out.skill_use_time
                            .as_ref()
                            .map_or(0.0, |t| t.effective_rate),
                    )
                }
            }
            "PhysicalDamageReduction"
            | "FireDamageReduction"
            | "ColdDamageReduction"
            | "LightningDamageReduction" => {
                let i = [
                    "PhysicalDamageReduction",
                    "FireDamageReduction",
                    "ColdDamageReduction",
                    "LightningDamageReduction",
                ]
                .iter()
                .position(|id| *id == stat)?;
                let t = self.ehp_terms()?.damage_reduction[i];
                ("clamp(min(ReductionCap, ArmourReduction + FlatReduction) - EnemyOverwhelm, 0, ReductionCap)".into(), vec![
                    ("ReductionCap".into(), t.cap), ("ArmourReduction".into(), t.armour),
                    ("FlatReduction".into(), t.flat), ("EnemyOverwhelm".into(), t.overwhelm),
                ])
            }
            "CritBifurcates" => {
                let mut formulas = Vec::new();
                let mut rows = Vec::new();
                for (i, (enabled, chance)) in crate::calc::sidebar::critical_roll_chances(env)
                    .into_iter()
                    .enumerate()
                {
                    let prefix = if i == 0 { "Primary" } else { "Secondary" };
                    formulas.push(format!("{prefix}DoubleCritEnabled * (1 + {prefix}CritRollChance / (2 - {prefix}CritRollChance))"));
                    rows.extend([
                        (format!("{prefix}DoubleCritEnabled"), f64::from(enabled)),
                        (format!("{prefix}CritRollChance"), chance),
                    ]);
                }
                if rows.is_empty() {
                    one("PrimaryDoubleCritEnabled", 0.0)
                } else {
                    (
                        format!("({}) / {}", formulas.join(" + "), formulas.len()),
                        rows,
                    )
                }
            }
            "AverageBurstDamage" => (
                "round(AverageDamage * BurstHits, 9)".into(),
                vec![
                    ("AverageDamage".into(), out.average_damage),
                    (
                        "BurstHits".into(),
                        perform::damage_view_terms(env).burst.hits(),
                    ),
                ],
            ),
            "AverageBurstHits" => match perform::damage_view_terms(env).burst {
                perform::BurstTerms::Repeats(repeats) => {
                    ("Repeats".into(), vec![("Repeats".into(), repeats)])
                }
                perform::BurstTerms::Flickers {
                    per_charge,
                    charges,
                    doubled_percent,
                } => (
                    "1 + FlickersPerCharge * ConsumedCharges * (1 + DoubledChance / 100)".into(),
                    vec![
                        ("FlickersPerCharge".into(), per_charge),
                        ("ConsumedCharges".into(), charges),
                        ("DoubledChance".into(), doubled_percent),
                    ],
                ),
            },
            "EnemyAilmentThreshold" => {
                let t =
                    perform::enemy_ailment_threshold_terms(&env.enemy.mod_db, cfg, env.enemy.level);
                (
                    "ThresholdAtEnemyLevel * (1 + IncreasedThreshold / 100) * MoreThreshold".into(),
                    vec![
                        ("ThresholdAtEnemyLevel".into(), t.table),
                        ("IncreasedThreshold".into(), t.increased),
                        ("MoreThreshold".into(), t.more),
                    ],
                )
            }
            "GemLevel" => {
                // Without a selected skill both inputs are zero.
                let (level, bonus) = env.main_skill_level.unwrap_or((0, 0));
                (
                    "GemBaseLevel + GemBonusLevels".into(),
                    vec![
                        ("GemBaseLevel".into(), f64::from(level)),
                        ("GemBonusLevels".into(), f64::from(bonus)),
                    ],
                )
            }
            "CullMultiplier" => {
                let enemy = &env.enemy.mod_db;
                match skill_dot::cull_threshold_percent(db, enemy, cfg) {
                    Some(percent) => (
                        "100 / (100 - CullThreshold)".into(),
                        vec![("CullThreshold".into(), percent)],
                    ),
                    None => (
                        "1 + 0 * CullThreshold".into(),
                        vec![("CullThreshold".into(), 0.0)],
                    ),
                }
            }
            "ReservationDpsMultiplier" => (
                "100 / (100 - LifeReservedPercent)".into(),
                vec![(
                    "LifeReservedPercent".into(),
                    skill_dot::reserved_life_percent(&env.enemy.mod_db, cfg),
                )],
            ),
            "CombinedAvg" | "WithIgniteDPS" | "WithBleedDPS" | "WithPoisonDPS" => {
                let t = perform::damage_view_terms(env);
                let base = if t.show_average {
                    "AverageDamage"
                } else {
                    "TotalDPS"
                };
                let mut rows = vec![(base.to_string(), t.base)];
                match stat {
                    "CombinedAvg" if t.show_average => rows.extend([
                        ("IgniteDamage".into(), out.ignite_damage),
                        ("BleedDamage".into(), out.bleed_damage),
                        ("PoisonDamage".into(), out.poison_damage),
                    ]),
                    "WithIgniteDPS" | "WithBleedDPS" | "WithPoisonDPS" => {
                        let (prefix, damage, dps, stacked) = match stat {
                            "WithIgniteDPS" => (
                                "Ignite",
                                out.ignite_damage,
                                out.ignite_dps,
                                out.ignite_stacked_dps,
                            ),
                            "WithBleedDPS" => (
                                "Bleed",
                                out.bleed_damage,
                                out.bleed_dps,
                                out.bleed_stacked_dps,
                            ),
                            _ => (
                                "Poison",
                                out.poison_damage,
                                out.poison_dps,
                                out.poison_stacked_dps,
                            ),
                        };
                        if t.show_average {
                            rows.push((format!("{prefix}Damage"), damage));
                        } else {
                            rows.extend([
                                (format!("{prefix}DPS"), dps),
                                (format!("{prefix}StackedDPS"), stacked),
                            ]);
                            return Some(DerivedDetail {
                                formula: format!("{base} + max({prefix}DPS, {prefix}StackedDPS)"),
                                inputs: rows,
                                context,
                            });
                        }
                    }
                    _ => {}
                }
                (
                    rows.iter()
                        .map(|(id, _)| id.as_str())
                        .collect::<Vec<_>>()
                        .join(" + "),
                    rows,
                )
            }
            "CombinedAvgToMonsterLife" => {
                let life = f64::from(
                    cfg.constants
                        .monster_scaling
                        .life_at(u32::from(env.enemy.level)),
                );
                (
                    if life > 0.0 {
                        "CombinedAvg / EnemyLife * 100"
                    } else {
                        "0 * EnemyLife"
                    }
                    .into(),
                    vec![
                        ("CombinedAvg".into(), out.combined_avg),
                        ("EnemyLife".into(), life),
                    ],
                )
            }
            "HitSpeed" => one("EffectiveActionRate", out.effective_action_rate),
            "IgniteDamage" | "BleedDamage" | "PoisonDamage" => {
                let prefix = stat.trim_end_matches("Damage");
                let id = format!("{prefix}DPS");
                let duration = self
                    .ailment_stage_terms(&id)
                    .and_then(|t| t.passes.first())
                    .map_or(0.0, |(_, t)| t.duration);
                (
                    format!("round({id} * AilmentDuration, 9)"),
                    vec![
                        (id.clone(), crate::display_catalog::display_value(out, &id)),
                        ("AilmentDuration".into(), duration),
                    ],
                )
            }
            "CullingDPS" | "ReservationDPS" => {
                let t = perform::skill_dot_terms(env);
                let (id, multiplier) = if stat == "CullingDPS" {
                    ("CullMultiplier", t.cull_multiplier)
                } else {
                    ("ReservationDamageMultiplier", t.reservation_multiplier)
                };
                (
                    format!("round((HitDPSBeforeCull + DotDPSBeforeCull) * ({id} - 1), 9)"),
                    vec![
                        ("HitDPSBeforeCull".into(), t.base_dps),
                        ("DotDPSBeforeCull".into(), t.total_dot_dps),
                        (id.into(), multiplier),
                    ],
                )
            }
            "WithDotDPS" => {
                let t = perform::skill_dot_terms(env);
                (
                    if t.dot_active {
                        "round(HitDPSBeforeCull + SkillDotBeforeRound, 9)"
                    } else {
                        "0 * SkillDotBeforeRound"
                    }
                    .into(),
                    vec![
                        ("HitDPSBeforeCull".into(), t.base_dps),
                        ("SkillDotBeforeRound".into(), t.total_dot),
                    ],
                )
            }
            "TotalDot" => {
                let t = perform::skill_dot_terms(env);
                if let Some([speed, duration, dps, quantity]) = t.stack_factors {
                    ("round(min(SkillDotInstance * DotApplicationRate * DotDuration * DpsMultiplier * QuantityMultiplier, DotDpsCap), 9)".into(), vec![
                        ("SkillDotInstance".into(), t.instance), ("DotApplicationRate".into(), speed),
                        ("DotDuration".into(), duration), ("DpsMultiplier".into(), dps),
                        ("QuantityMultiplier".into(), quantity), ("DotDpsCap".into(), t.cap)])
                } else {
                    (
                        "round(SkillDotInstance, 9)".into(),
                        vec![("SkillDotInstance".into(), t.instance)],
                    )
                }
            }
            "MirageDPS" | "BurningGroundDPS" | "MirageBurningGroundDPS" | "DecayDPS" => {
                let (enabled, id, value) = match stat {
                    "MirageDPS" => (db.flag(cfg, "MirageSkill"), "CombinedDPS", out.combined_dps),
                    "MirageBurningGroundDPS" => (
                        db.flag(cfg, "MirageSkill"),
                        "BurningGroundDPS",
                        out.burning_ground_dps,
                    ),
                    "DecayDPS" => (
                        db.flag(cfg, "ProfaneDecay"),
                        "TotalDot",
                        out.skill_total_dot,
                    ),
                    _ if db.flag(cfg, "IgnitedGround") => (true, "IgniteDPS", out.ignite_dps),
                    _ => (
                        db.flag(cfg, "BurningGround"),
                        "TotalDot",
                        out.skill_total_dot,
                    ),
                };
                (
                    if enabled {
                        id.into()
                    } else {
                        format!("0 * {id}")
                    },
                    vec![(id.into(), value)],
                )
            }
            "LifeRecoverable" => (
                "max(1, LifeUnreserved)".into(),
                vec![("LifeUnreserved".into(), out.life_unreserved)],
            ),
            "EnergyShieldRecoveryCap" => one("EnergyShield", out.energy_shield),
            "PhysicalMaxHit" | "FireMaxHit" | "ColdMaxHit" | "LightningMaxHit" | "ChaosMaxHit" => {
                let t = self.ehp_terms()?;
                let i = [
                    "PhysicalMaxHit",
                    "FireMaxHit",
                    "ColdMaxHit",
                    "LightningMaxHit",
                    "ChaosMaxHit",
                ]
                .iter()
                .position(|id| *id == stat)?;
                for (kind, pool) in ["Physical", "Fire", "Cold", "Lightning", "Chaos"]
                    .into_iter()
                    .zip(t.total_hit_pool)
                {
                    context.push((format!("{kind}HitPool"), pool));
                }
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
                    "round(SolvedMaxHit / EnemyDamageMultiplier, 0)".into(),
                    vec![
                        ("SolvedMaxHit".into(), t.max_hit_solutions[i]),
                        (
                            "EnemyDamageMultiplier".into(),
                            t.enemy_damage_multipliers[i],
                        ),
                    ],
                )
            }
            "StunThreshold" => {
                let input = crate::calc::stun::StunInputs {
                    life: out.life,
                    life_base_flat: env.player.base.life,
                    energy_shield: out.energy_shield,
                    mana: out.mana,
                    chaos_inoculation: crate::rules::DefenceKeystones::from_db(db, cfg)
                        .chaos_inoculation,
                    ..Default::default()
                };
                let t = crate::calc::stun::stun_threshold_terms(db, cfg, &input);
                context.extend([
                    ("Life".into(), out.life),
                    ("EnergyShield".into(), out.energy_shield),
                    ("Mana".into(), out.mana),
                ]);
                ("round((ThresholdBase + ThresholdAdded) * ThresholdIncrease * ThresholdMore, 9)".into(), vec![
                    ("ThresholdBase".into(), t.base), ("ThresholdAdded".into(), t.flat),
                    ("ThresholdIncrease".into(), t.inc), ("ThresholdMore".into(), t.more),
                ])
            }
            "SecondMinimalMaximumHitTaken" => {
                // The second smallest of five values is the smallest of the
                // larger value of every pair.
                let ids = [
                    "PhysicalMaxHit",
                    "FireMaxHit",
                    "ColdMaxHit",
                    "LightningMaxHit",
                    "ChaosMaxHit",
                ];
                let values = [
                    out.physical_max_hit,
                    out.fire_max_hit,
                    out.cold_max_hit,
                    out.lightning_max_hit,
                    out.chaos_max_hit,
                ];
                let mut pairs = Vec::new();
                for (i, a) in ids.iter().enumerate() {
                    for b in &ids[i + 1..] {
                        pairs.push(format!("max({a}, {b})"));
                    }
                }
                (
                    format!("min({})", pairs.join(", ")),
                    ids.iter()
                        .zip(values)
                        .map(|(id, v)| ((*id).to_string(), v))
                        .collect(),
                )
            }
            "EnemyCritChance" => {
                use crate::calc::ehp::EnemyCritMode;
                let t = self.ehp_terms()?.enemy_crit_chance;
                let rolled = "clamp(BaseChance * (1 + IncreasedChance / 100) * (1 - ConfiguredEvadeChance / 100), 0, 100)";
                let formula = match (t.mode, t.unlucky) {
                    (EnemyCritMode::Never, _) => "0 * BaseChance".to_string(),
                    (EnemyCritMode::Always, _) => "100 + 0 * BaseChance".to_string(),
                    (EnemyCritMode::Rolled, false) => format!("round({rolled}, 9)"),
                    (EnemyCritMode::Rolled, true) => format!("round(pow({rolled}, 2) / 100, 9)"),
                };
                (
                    formula,
                    vec![
                        ("BaseChance".into(), t.base),
                        ("IncreasedChance".into(), t.increased),
                        ("ConfiguredEvadeChance".into(), t.evade),
                    ],
                )
            }
            "EnemyCritEffect" => {
                use pobr_data::prelude::{ModName, ModType};
                let sum = |name: &str| {
                    env.enemy
                        .mod_db
                        .sum(ModType::Base, cfg, &[ModName::from(name)])
                };
                ("round(1 + EnemyCritChance / 100 * EnemyCritBonus / 100 * (1 - CritExtraDamageReduction / 100), 9)".into(), vec![
                    ("EnemyCritChance".into(), sum("CritChance")), ("EnemyCritBonus".into(), sum("CritMultiplier")),
                    ("CritExtraDamageReduction".into(), out.crit_extra_damage_reduction),
                ])
            }
            "MainHandAccuracy" | "OffHandAccuracy" => {
                let hand = if stat == "MainHandAccuracy" {
                    out.main_hand.as_ref()
                } else {
                    out.off_hand.as_ref()
                };
                if hand.is_none() {
                    return Some(DerivedDetail {
                        formula: "0 * WeaponAccuracyBase".into(),
                        inputs: vec![("WeaponAccuracyBase".into(), 0.0)],
                        context,
                    });
                };
                let scopes = perform::pass_scopes(env);
                let (_, scope, input) = scopes.iter().find(|(tag, _, _)| {
                    if stat == "MainHandAccuracy" {
                        *tag != crate::HandTag::OffHand
                    } else {
                        *tag == crate::HandTag::OffHand
                    }
                })?;
                let t = crate::calc::offence::accuracy_terms(db, scope, input);
                context.push(("WeaponAccuracyBase".into(), input.base_accuracy));
                if let Some(value) = t.override_value {
                    one("AccuracyOverride", value)
                } else {
                    ("round((WeaponAccuracyBase + AccuracyAdded) * (1 + AccuracyIncrease / 100) * AccuracyMore, 9)".into(),
                        vec![("WeaponAccuracyBase".into(), t.base), ("AccuracyAdded".into(), t.flat),
                            ("AccuracyIncrease".into(), t.inc), ("AccuracyMore".into(), t.more)])
                }
            }
            "AverageDamage"
            | "AccuracyHitChance"
            | "AccuracyHitChanceUncapped"
            | "PreEffectiveCritChance"
            | "PreEffectiveCritMultiplier" => {
                let scopes = perform::pass_scopes(env);
                let mut rows = Vec::new();
                for (i, (_, scope, input)) in scopes.iter().enumerate() {
                    let prefix = if i == 0 { "Primary" } else { "Secondary" };
                    let hit = crate::calc::offence::hit_pass(db, &env.enemy.mod_db, scope, input);
                    let (formula, inputs) = match stat {
                        "AverageDamage" => (
                            format!("round({prefix}MitigatedHit * {prefix}HitChance, 9)"),
                            vec![
                                (
                                    format!("{prefix}MitigatedHit"),
                                    hit.crit_pass.total_hit_avg_mitigated,
                                ),
                                (format!("{prefix}HitChance"), hit.hit.value),
                            ],
                        ),
                        "AccuracyHitChance" => (
                            format!("round({prefix}AccuracyChance * 100, 9)"),
                            vec![(format!("{prefix}AccuracyChance"), hit.hit.accuracy_chance)],
                        ),
                        "AccuracyHitChanceUncapped" => (
                            format!(
                                "round({prefix}AccuracyChance * 100 + {prefix}ExcessChance, 9)"
                            ),
                            vec![
                                (format!("{prefix}AccuracyChance"), hit.hit.accuracy_chance),
                                (format!("{prefix}ExcessChance"), hit.hit.excess),
                            ],
                        ),
                        "PreEffectiveCritMultiplier" => match hit.crit.multiplier_terms {
                            Some(terms) => terms.pre_effective_detail(prefix),
                            None => (
                                format!("0 * {prefix}NeverCrit"),
                                vec![(format!("{prefix}NeverCrit"), 1.0)],
                            ),
                        },
                        _ => (
                            format!("{prefix}CriticalChance * 100"),
                            vec![(
                                format!("{prefix}CriticalChance"),
                                hit.crit.pre_effective_chance,
                            )],
                        ),
                    };
                    rows.push((formula, inputs));
                }
                let summed = rows
                    .iter()
                    .map(|(f, _)| format!("({f})"))
                    .collect::<Vec<_>>()
                    .join(" + ");
                let divisor = if stat == "AverageDamage" && env.double_hits_when_dual_wielding {
                    1
                } else {
                    rows.len()
                };
                let formula = if rows.len() == 1 {
                    rows[0].0.clone()
                } else if matches!(stat, "AccuracyHitChance" | "AccuracyHitChanceUncapped") {
                    format!("({summed}) / {divisor}")
                } else {
                    format!("round(({summed}) / {divisor}, 9)")
                };
                (
                    formula,
                    rows.into_iter().flat_map(|(_, inputs)| inputs).collect(),
                )
            }
            "PowerChargesDuration" | "FrenzyChargesDuration" | "EnduranceChargesDuration" => {
                let kind = match stat {
                    "PowerChargesDuration" => ChargeKind::Power,
                    "FrenzyChargesDuration" => ChargeKind::Frenzy,
                    _ => ChargeKind::Endurance,
                };
                let t = crate::calc::survivability::charge_duration_terms(db, cfg, kind);
                (
                    "max(0, BaseChargeDuration * (1 + ChargeDurationIncreased / 100) * ChargeDurationMore)".into(),
                    vec![
                        ("BaseChargeDuration".into(), t.base),
                        ("ChargeDurationIncreased".into(), t.increased),
                        ("ChargeDurationMore".into(), t.more),
                    ],
                )
            }
            "ChargePowerCurrent" | "ChargeFrenzyCurrent" | "ChargeEnduranceCurrent" => {
                let kind = match stat {
                    "ChargePowerCurrent" => ChargeKind::Power,
                    "ChargeFrenzyCurrent" => ChargeKind::Frenzy,
                    _ => ChargeKind::Endurance,
                };
                let t = crate::calc::survivability::resolve_charge_state(db, cfg, kind);
                (
                    "max(ConfiguredCharges, MinimumCharges)".into(),
                    vec![
                        (
                            "ConfiguredCharges".into(),
                            cfg.multiplier(kind.multiplier_key()),
                        ),
                        ("MinimumCharges".into(), f64::from(t.minimum)),
                    ],
                )
            }
            _ => return None,
        };
        Some(DerivedDetail {
            formula,
            inputs,
            context,
        })
    }
}
