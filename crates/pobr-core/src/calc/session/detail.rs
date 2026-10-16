//! Queries for the optional Calcs explanations. Each source group is one
//! ModDb query: totals and rows always use the same names and configuration.
use super::CalculationSession;
use crate::ModEvaluation;
use pobr_data::prelude::{ModName, ModType};

pub use super::detail_views::DERIVED_STATS;

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

/// Each group represents a genuine query. Explicit enemy groups use the enemy
/// database; derived relationships never appear as synthetic modifier rows.
pub fn source_names(stat: &str) -> Vec<String> {
    let names = super::super::player_debuffs::view_names(stat);
    if !names.is_empty() {
        let mut names: Vec<_> = names.iter().map(|name| (*name).to_string()).collect();
        if super::super::player_debuffs::is_duration_view(stat) {
            names.push("SelfDebuffExpirationRate".into());
        }
        return names;
    }
    if let Some((_, names)) = super::super::utility::GLOBAL_STATS
        .iter()
        .find(|(id, _)| *id == stat)
    {
        return names.iter().map(|name| (*name).to_string()).collect();
    }
    let names: &[&str] = match stat {
        "Rage" => &[
            "MaximumRage",
            "MinimumRage",
            "RageRegen",
            "Condition:CanGainRage",
            "Multiplier:RageStack",
        ],
        "FiringRate" => &[
            "AttackSpeed",
            "CastSpeed",
            "SkillSpeed",
            "ActionSpeed",
            "SlowEffectOnSelf",
            "UnaffectedBySlows",
            "TotalAttackTime",
            "TotalCastTime",
            "CooldownRecovery",
            "CrossbowReloadTimeBase",
        ],
        "ChillEffect" => &["AilmentMagnitude", "EnemyChillMagnitude", "CannotChill"],
        "TriggerRateCap" => &[
            "TriggerCooldownBase",
            "TriggeredSkillCooldown",
            "CooldownRecovery",
            "TriggerRateCapOverride",
            "SkillIsTriggered",
            "CWCTriggerTime",
            "CWCAddsCastTime",
        ],
        "SkillTriggerRate" => &[
            "TriggerCooldownBase",
            "TriggeredSkillCooldown",
            "CooldownRecovery",
            "TriggerSourceRate",
            "TriggerSourceHitChance",
            "TriggerSourceCritChance",
            "TriggerOnCrit",
            "TriggerSourceGlobal",
            "TriggerRateCapOverride",
            "SkillIsTriggered",
            "CWCTriggerTime",
            "CWCAddsCastTime",
            "EnergyTriggerMaximum",
            "EnergyPerPowerOnCrit",
            "EnergyTriggerCost",
            "EnergyPerBlock",
            "EnergyPerMinionPower",
            "EnergyPerCastMillisecond",
            "EnergyPerIgnitePower",
            "EnergyPerShockPower",
            "EnergyPerFreezePower",
            "EnergyInvocationCooldown",
            "Multiplier:EnergyIncomingHitRate",
            "Multiplier:EnergyMinionDeathsPerSecond",
            "Multiplier:EnergyMinionPower",
            "Multiplier:EnergyInvocationUsesPerSecond",
            "EnergyGenerated",
            "Multiplier:EnemyPower",
            "EnergyRefundHalfChance",
            "EnergySource",
        ],
        "ReloadTime" => &[
            "CrossbowReloadTimeBase",
            "ReloadSpeed",
            "AttackSpeed",
            "CastSpeed",
            "SkillSpeed",
            "CrossbowHasNoAmmo",
        ],
        "ChannelTime" => &[
            "ChannelMinimumTime",
            "FlameblastMinimumChannelTime",
            "FlameblastMaxStages",
            "SkillChannelStages",
            "SkillChannelStageTimeOverride",
            "ChannelStartLock",
            "ChannelFixedAttackTime",
            "ChannelReleaseTime",
            "ChannelReleaseFraction",
            "ChannelMaxStages",
            "ChannelCycle",
            "AttackSpeed",
            "CastSpeed",
            "SkillSpeed",
            "ActionSpeed",
            "SlowEffectOnSelf",
            "UnaffectedBySlows",
        ],
        "ShockEffect" => &[
            "EnemyShockMagnitude",
            "AilmentMagnitude",
            "EnemySelfShockMagnitude",
            "EnemyAilmentMagnitude",
            "CannotShock",
        ],
        "AilmentThreshold" => &["AilmentThreshold"],
        "LifeOnKill" => &[
            "LifeOnKill",
            "CannotGainLife",
            "CannotRecoverLifeOutsideLeech",
        ],
        "ManaOnKill" => &["ManaOnKill", "CannotGainMana"],
        "EnergyShieldOnKill" => &["EnergyShieldOnKill", "CannotGainEnergyShield"],
        "DazeChance" => &["DazeChance"],
        "FullyBrokenArmourEffect" => &["FullyBrokenArmourEffect"],
        "FreezeBuildupPct" => &[
            "FreezeBuildup",
            "EnemyFreezeBuildup",
            "EnemyImmobilisationBuildup",
            "ImmobilisationBuildup",
            "CannotFreeze",
        ],
        "ElectrocuteBuildupPct" => &[
            "ElectrocuteBuildup",
            "EnemyElectrocuteBuildup",
            "EnemyImmobilisationBuildup",
            "ImmobilisationBuildup",
            "CannotElectrocute",
        ],
        "Str" => &["Strength"],
        "Dex" => &["Dexterity"],
        "Int" => &["Intelligence"],
        "EffectiveMovementSpeedMod" => &[
            "MovementSpeed",
            "ArmourMovementPenalty",
            "ActionSpeed",
            "SlowEffectOnSelf",
            "UnaffectedBySlows",
            "MovementSpeedCannotBeBelowBase",
            "Condition:IgnoreMovementPenalties",
        ],
        "MovementSpeedWhileUsingSkill" => &[
            "MovementSpeed",
            "ActionSpeed",
            "SlowEffectOnSelf",
            "UnaffectedBySlows",
            "ArmourMovementPenalty",
            "MovementSpeedCannotBeBelowBase",
            "Condition:IgnoreMovementPenalties",
            "SkillMovementSpeed",
            "MovementSpeedPenalty",
            "UsableWhileMoving",
        ],
        "PresenceRadiusMetres" | "PresenceRadius" => &["PresenceRadius", "PresenceArea"],
        "CharmLimit" => &["CharmLimit"],
        "EnemyCurseLimit" => &[
            "EnemyCurseLimit",
            "CurseLimitIsMaximumPowerCharges",
            "PowerChargesMax",
        ],
        "EnemyMarkLimit" => &["EnemyMarkLimit"],
        "ActionSpeedMod" => &["ActionSpeed", "SlowEffectOnSelf", "UnaffectedBySlows"],
        "MovementSpeedMod" => &[
            "MovementSpeed",
            "ArmourMovementPenalty",
            "MovementSpeedCannotBeBelowBase",
            "Condition:IgnoreMovementPenalties",
        ],
        "LightRadiusMod" => &["LightRadius"],
        "LifeRecoveryRateMod" => &["LifeRecoveryRate"],
        "ManaRecoveryRateMod" => &["ManaRecoveryRate"],
        "EnergyShieldRecoveryRateMod" => &["EnergyShieldRecoveryRate"],
        "GlobalGemAttributeRequirements" => &[
            "GlobalAttributeRequirements",
            "GlobalGemAttributeRequirements",
        ],
        "GlobalItemAttributeRequirements" => &[
            "GlobalAttributeRequirements",
            "GlobalItemAttributeRequirements",
        ],
        "GlobalWeaponAttributeRequirements" => &[
            "GlobalAttributeRequirements",
            "GlobalItemAttributeRequirements",
            "GlobalWeaponAttributeRequirements",
        ],
        "AuraEffectMod" => &[
            "AuraEffect",
            "SelectedAuraEffect",
            "SkillAuraEffectOnSelf",
            "SelectedSkillAuraEffectOnSelf",
            "Magnitude",
            "SelectedMagnitude",
            "AuraCannotAffectSelf",
        ],
        "CurseEffectMod" => &["CurseEffect", "SelectedCurseEffect"],
        "ActiveMinionLimit" => &["SelectedMinionLimit"],
        "ExplodeChance" => &["EnemyExplodeChance"],
        "QuantityMultiplier" => &["QuantityMultiplier"],
        "PierceCount" => &[
            "PierceCount",
            "PierceChance",
            "CannotPierce",
            "PierceAllTargets",
            "EnemyAlwaysPierceSelf",
        ],
        "ForkCount" => &["ForkOnce", "ForkTwice", "ForkCountMax", "CannotFork"],
        "ChainMax" => &[
            "ChainCountMax",
            "BeamChainCountMax",
            "ChainChance",
            "CannotChain",
            "NoAdditionalChains",
            "AdditionalProjectilesAddChainsInstead",
            "ProjectileCount",
            "TwoAdditionalProjectilesChance",
            "SurpassingProjectileChance",
        ],
        "SplitCount" => &[
            "SplitCount",
            "EnemySelfSplitCount",
            "CannotSplit",
            "AdditionalProjectilesAddSplitsInstead",
            "AdditionalChainsAddSplitsInstead",
            "ChainCountMax",
            "ProjectileCount",
            "TwoAdditionalProjectilesChance",
            "SurpassingProjectileChance",
        ],
        "BounceCount" => &[
            "BounceCount",
            "AdditionalProjectilesAddBouncesInstead",
            "ProjectileCount",
            "TwoAdditionalProjectilesChance",
            "SurpassingProjectileChance",
        ],
        "StoredUses" => &[
            "SkillStoredUsesBase",
            "AdditionalCooldownUses",
            "SkillCooldownBase",
        ],
        "AvoidAllDamageFromHits" => &["AvoidAllDamageFromHitsChance"],
        "AvoidProjectileDamage" => &["AvoidProjectileDamageChance"],
        "AvoidIgnite" => &[
            "AvoidIgnite",
            "IgniteImmune",
            "AvoidElementalAilments",
            "ElementalAilmentImmune",
            "ShockAvoidAppliesToElementalAilments",
            "AvoidShock",
            "ShockImmune",
        ],
        "AvoidChill" => &[
            "AvoidChill",
            "ChillImmune",
            "AvoidElementalAilments",
            "ElementalAilmentImmune",
            "ShockAvoidAppliesToElementalAilments",
            "AvoidShock",
            "ShockImmune",
        ],
        "AvoidFreeze" => &[
            "AvoidFreeze",
            "FreezeImmune",
            "AvoidElementalAilments",
            "ElementalAilmentImmune",
            "ShockAvoidAppliesToElementalAilments",
            "AvoidShock",
            "ShockImmune",
        ],
        "AvoidShock" => &["AvoidShock", "ShockImmune", "ElementalAilmentImmune"],
        "AvoidPoison" => &["AvoidPoison", "PoisonImmune"],
        "AvoidBleeding" => &["AvoidBleeding", "BleedImmune"],
        "CritExtraDamageReduction" => &["ReduceCritExtraDamage"],
        "PowerChargesDuration" => &["ChargeDuration", "PowerChargesDuration"],
        "FrenzyChargesDuration" => &["ChargeDuration", "FrenzyChargesDuration"],
        "EnduranceChargesDuration" => &["ChargeDuration", "EnduranceChargesDuration"],
        "ChargePowerMaximum" => &["PowerChargesMax"],
        "ChargeFrenzyMaximum" => &["FrenzyChargesMax"],
        "ChargeEnduranceMaximum" => &["EnduranceChargesMax"],
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
        "AoeRadius" | "AoeAreaMod" | "AreaOfEffectRadiusMetres" => &[
            "SkillAreaRadiusBase",
            "SkillAreaRadiusExtra",
            "RepeatCount",
            "RepeatAreaOfEffect",
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
            "SlowEffectOnSelf",
            "UnaffectedBySlows",
        ],
        "TrapThrowingTime" => &[
            "SkillTrapThrowingTimeBase",
            "TrapThrowingTime",
            "TrapThrowingSpeed",
            "SkillTrapThrowingTime",
            "ActionSpeed",
            "SlowEffectOnSelf",
            "UnaffectedBySlows",
        ],
        "MineLayingTime" => &[
            "SkillMineLayingTimeBase",
            "MineLayingTime",
            "MineLayingSpeed",
            "SkillMineThrowingTime",
            "ActionSpeed",
            "SlowEffectOnSelf",
            "UnaffectedBySlows",
        ],
        "WarcryCastTime" => &[
            "SkillWarcryCastTimeBase",
            "WarcryCastTime",
            "WarcrySpeed",
            "ActionSpeed",
            "SlowEffectOnSelf",
            "UnaffectedBySlows",
            "InstantWarcry",
        ],
        "TrapThrowCount" => &["TrapThrowCount"],
        "MineThrowCount" => &["MineThrowCount"],
        "EsRechargeRate" => &[
            "EnergyShieldRecharge",
            "ZealotsOath",
            "NoEnergyShieldRecharge",
        ],
        "EsRechargeDelay" => &[
            "EnergyShieldRechargeFaster",
            "EnergyShieldRechargeBase",
            "NoEnergyShieldRecharge",
        ],
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
    if let Some(element) = stat.strip_prefix("TakenMulti")
        && ["Physical", "Fire", "Cold", "Lightning", "Chaos"].contains(&element)
    {
        let mut names = vec![
            "DamageTaken".into(),
            format!("{element}DamageTaken"),
            "DamageTakenWhenHit".into(),
            format!("{element}DamageTakenWhenHit"),
        ];
        if ["Fire", "Cold", "Lightning"].contains(&element) {
            names.extend([
                "ElementalDamageTaken".into(),
                "ElementalDamageTakenWhenHit".into(),
            ]);
        }
        return names;
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
        let view_names = super::super::player_debuffs::view_names(stat);
        if !view_names.is_empty() {
            let cfg = &self.env.cfg;
            let names: Vec<_> = view_names.iter().map(|name| ModName::from(*name)).collect();
            let mut groups = vec![SourceGroup {
                name: stat.into(),
                more_precision: None,
                totals: (
                    db.sum(ModType::Base, cfg, &names),
                    db.sum(ModType::Inc, cfg, &names),
                    db.more(cfg, &names),
                ),
                rows: db.evaluate_names(cfg, &names),
            }];
            if super::super::player_debuffs::is_duration_view(stat) {
                let names = [ModName::from("SelfDebuffExpirationRate")];
                groups.push(SourceGroup {
                    name: "SelfDebuffExpirationRate".into(),
                    more_precision: None,
                    totals: (
                        db.sum(ModType::Base, cfg, &names),
                        db.sum(ModType::Inc, cfg, &names),
                        db.more(cfg, &names),
                    ),
                    rows: db.evaluate_names(cfg, &names),
                });
            }
            return groups;
        }
        let mut cfg = if super::super::utility::GLOBAL_STATS
            .iter()
            .any(|(id, _)| *id == stat)
        {
            super::super::utility::query_config(&self.env.cfg)
        } else {
            self.env.cfg.clone()
        };
        if stat == "DazeChance" {
            cfg.flags |= pobr_data::prelude::ModFlags::HIT;
            cfg.keyword_flags = cfg.keyword_flags | pobr_data::prelude::KeywordFlags::HIT;
        }
        if stat.starts_with("ManaRegen") && db.flag(&cfg, "Condition:NoInherentManaRegen") {
            cfg.conditions.insert("NoInherentManaRegen".into(), true);
        }
        if stat.starts_with("Req")
            || stat.starts_with("Global") && stat.ends_with("AttributeRequirements")
        {
            cfg.flags = pobr_data::prelude::ModFlags::NONE;
            cfg.keyword_flags = pobr_data::prelude::KeywordFlags::NONE;
        }
        source_names(stat)
            .into_iter()
            .filter(|name| {
                !matches!(name.as_str(), "AttackSpeed" | "CastSpeed" | "SkillSpeed")
                    || super::super::skill_use_time::speed_names_for(&cfg)
                        .iter()
                        .any(|n| n.as_str() == name)
            })
            .map(|name| {
                let mut query_cfg = cfg.clone();
                if stat == "SkillTriggerRate"
                    && name == "EnergyGenerated"
                    && db.sum(
                        ModType::Base,
                        &cfg,
                        &[ModName::from("EnergyPerPowerOnCrit")],
                    ) > 0.0
                {
                    query_cfg.conditions.insert("CritRecently".into(), true);
                }
                if stat == "Rage" && name == "Multiplier:RageStack" {
                    query_cfg.conditions.insert("Combat".into(), true);
                    query_cfg.conditions.insert("CanGainRage".into(), true);
                }
                if stat == "FiringRate"
                    && let Some((hand, _)) = super::super::perform::main_hand_scope(&self.env)
                {
                    query_cfg = hand;
                }
                if matches!(
                    stat,
                    "EffectiveMovementSpeedMod"
                        | "PresenceRadiusMetres"
                        | "PresenceRadius"
                        | "MovementSpeedMod"
                        | "ActionSpeedMod"
                        | "LightRadiusMod"
                ) || stat == "MovementSpeedWhileUsingSkill"
                    && !matches!(
                        name.as_str(),
                        "SkillMovementSpeed" | "MovementSpeedPenalty" | "UsableWhileMoving"
                    )
                {
                    query_cfg.flags = pobr_data::prelude::ModFlags::NONE;
                    query_cfg.keyword_flags = pobr_data::prelude::KeywordFlags::NONE;
                    query_cfg.skill_types = pobr_data::prelude::SkillTypes::NONE;
                    query_cfg.skill_name = None;
                }
                let (db, query_name) = match name.as_str() {
                    "EnemySelfSplitCount" => (&self.env.enemy.mod_db, "SelfSplitCount"),
                    "EnemyAlwaysPierceSelf" => (&self.env.enemy.mod_db, "AlwaysPierceSelf"),
                    "EnemySelfShockMagnitude" => (&self.env.enemy.mod_db, "SelfShockMagnitude"),
                    "EnemyAilmentMagnitude" => (&self.env.enemy.mod_db, "AilmentMagnitude"),
                    _ => (db, name.as_str()),
                };
                let cfg = &query_cfg;
                let names = [ModName::from(query_name)];
                SourceGroup {
                    totals: (
                        db.sum(ModType::Base, cfg, &names),
                        db.sum(ModType::Inc, cfg, &names),
                        db.more(cfg, &names),
                    ),
                    more_precision: db.more_precision_for(query_name),
                    rows: db.evaluate_names(cfg, &names),
                    name,
                }
            })
            .collect()
    }

    pub fn derived_detail(&self, stat: &str) -> Option<DerivedDetail> {
        if let Some(terms) = self.env.utility_details.get(stat) {
            return Some(super::super::utility::derived(terms));
        }
        if let Some(terms) = &self.output().energy_trigger {
            if terms.at_own_rate && matches!(stat, "SkillTriggerRate" | "TriggerRateCap") {
                return Some(DerivedDetail {
                    // No trigger rate was resolved; DPS uses the payload's own rate.
                    formula: "0 * EnergyAtOwnRate".into(),
                    inputs: vec![(
                        "EnergyAtOwnRate".into(),
                        self.output().effective_action_rate,
                    )],
                    context: terms
                        .missing_source
                        .map(|reason| vec![(reason.into(), 1.0)])
                        .unwrap_or_default(),
                });
            }
            if stat == "SkillTriggerRate" {
                let formula = if terms.events_per_trigger > 0.0 {
                    "round(min(EnergyEventRate / EventsPerTrigger, TriggerRateCap), 9)"
                } else {
                    "0 * EnergyEventRate"
                };
                return Some(DerivedDetail {
                    formula: formula.into(),
                    inputs: vec![
                        ("EnergyEventRate".into(), terms.events_per_second),
                        ("EventsPerTrigger".into(), terms.events_per_trigger),
                        ("TriggerRateCap".into(), terms.trigger_rate_cap),
                    ],
                    context: vec![
                        ("MaximumEnergy".into(), terms.maximum_energy),
                        ("EnergyPerTrigger".into(), terms.energy_per_trigger),
                        ("EnergyPerEvent".into(), terms.energy_per_event),
                        ("EnergyRefundHalfChance".into(), terms.refund_half_chance),
                    ],
                });
            }
            if stat == "TriggerRateCap" {
                return Some(DerivedDetail {
                    formula: "EnergyTriggerRateCap".into(),
                    inputs: vec![("EnergyTriggerRateCap".into(), terms.trigger_rate_cap)],
                    context: vec![],
                });
            }
        }
        if DERIVED_STATS.contains(&stat) {
            return self.derived_view(stat);
        }
        let out = self.output();
        let mut context = Vec::new();
        let (formula, inputs): (String, Vec<(String, f64)>) = match stat {
            "SkillTriggerRate" => (
                "ResolvedTriggerRate".into(),
                vec![("ResolvedTriggerRate".into(), out.skill_trigger_rate)],
            ),
            "TriggerRateCap" => (
                "ResolvedTriggerRateCap".into(),
                vec![("ResolvedTriggerRateCap".into(), out.trigger_rate_cap)],
            ),
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
                        "ResourceDrain + UseLoss + SelfDamage + Spending".into(),
                        vec![
                            ("ResourceDrain".into(), terms.direct_drain),
                            ("UseLoss".into(), terms.use_loss.iter().sum()),
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
                        format!("{resource}RegenRecovery - Spending - SelfDamage - UseLoss"),
                        vec![
                            (format!("{resource}RegenRecovery"), recovered),
                            ("Spending".into(), terms.costs[i]),
                            (
                                "UseLoss".into(),
                                terms.use_loss.get(i).copied().unwrap_or(0.0),
                            ),
                            (
                                "SelfDamage".into(),
                                if i < 3 {
                                    terms.loss[i] - terms.costs[i] - terms.use_loss[i]
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
