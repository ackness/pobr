//! The display stat catalog.
//!
//! Maps calculation results ([`OutputTable`](crate::calc::OutputTable)) to the
//! stable [`DisplayStatDefinition`] / [`DisplayStatValue`] (pobr-data
//! contract) consumed by upstream UI / parity checks. Calculations use only
//! stable IDs internally; display text goes through i18n.
//!
//! - [`display_catalog`]: statically declares every display field (id /
//!   category / value type / higher-is-better / PoB key). Computed fields are
//!   marked `Computed`, not-yet-landed ones `Planned`.
//! - [`extract_display_values`]: extracts the current value of every
//!   `Computed` field from an `OutputTable`.

use pobr_data::prelude::*;

use crate::calc::OutputTable;

/// All display field definitions (stable declarations, no values).
pub fn display_catalog() -> Vec<DisplayStatDefinition> {
    use DisplayStatCategory as Cat;
    use StatValueType as Vt;

    let computed = |id: &str, cat: Cat, vt: Vt, pob: &str| {
        DisplayStatDefinition::computed(id, cat, vt).with_pob_key(pob)
    };
    let mut catalog = vec![
        computed(
            "ExposureEffectOnSelf",
            Cat::Defence,
            Vt::Percent,
            "ExposureEffectOnSelf",
        )
        .with_higher_is_better(Some(false)),
        computed(
            "WitherEffectOnSelf",
            Cat::Defence,
            Vt::Percent,
            "WitherEffectOnSelf",
        )
        .with_higher_is_better(Some(false)),
        computed(
            "SelfBleedEffect",
            Cat::Defence,
            Vt::Percent,
            "SelfBleedEffect",
        )
        .with_higher_is_better(Some(false)),
        computed(
            "SelfIgniteEffect",
            Cat::Defence,
            Vt::Percent,
            "SelfIgniteEffect",
        )
        .with_higher_is_better(Some(false)),
        computed(
            "DebuffExpirationModifier",
            Cat::Defence,
            Vt::Percent,
            "DebuffExpirationModifier",
        )
        .with_higher_is_better(Some(false)),
        computed(
            "CurseEffectOnSelf",
            Cat::Defence,
            Vt::Percent,
            "CurseEffectOnSelf",
        )
        .with_higher_is_better(Some(false)),
        computed(
            "SelfBleedDuration",
            Cat::Defence,
            Vt::Percent,
            "SelfBleedDuration",
        )
        .with_higher_is_better(Some(false)),
        computed(
            "SelfPoisonDuration",
            Cat::Defence,
            Vt::Percent,
            "SelfPoisonDuration",
        )
        .with_higher_is_better(Some(false)),
        computed(
            "SelfIgniteDuration",
            Cat::Defence,
            Vt::Percent,
            "SelfIgniteDuration",
        )
        .with_higher_is_better(Some(false)),
        computed(
            "SelfChillDuration",
            Cat::Defence,
            Vt::Percent,
            "SelfChillDuration",
        )
        .with_higher_is_better(Some(false)),
        computed(
            "SelfFreezeDuration",
            Cat::Defence,
            Vt::Percent,
            "SelfFreezeDuration",
        )
        .with_higher_is_better(Some(false)),
        computed(
            "SelfShockDuration",
            Cat::Defence,
            Vt::Percent,
            "SelfShockDuration",
        )
        .with_higher_is_better(Some(false)),
        computed(
            "CorruptedBloodImmunity",
            Cat::Defence,
            Vt::Bool,
            "CorruptedBloodImmunity",
        ),
        computed("MaimImmunity", Cat::Defence, Vt::Bool, "MaimImmunity"),
        computed("HinderImmunity", Cat::Defence, Vt::Bool, "HinderImmunity"),
        computed("DecayDPS", Cat::DotDamage, Vt::Number, "DecayDPS"),
        computed(
            "MirageBurningGroundDPS",
            Cat::DotDamage,
            Vt::Number,
            "MirageBurningGroundDPS",
        ),
        computed("ReservationDPS", Cat::Offence, Vt::Number, "ReservationDPS"),
        computed("NetWardRegen", Cat::Recovery, Vt::Number, "NetWardRegen"),
        computed("MirageDPS", Cat::Offence, Vt::Number, "MirageDPS"),
        computed(
            "BurningGroundDPS",
            Cat::DotDamage,
            Vt::Number,
            "BurningGroundDPS",
        ),
        computed(
            "EffectiveMovementSpeedMod",
            Cat::Defence,
            Vt::Number,
            "EffectiveMovementSpeedMod",
        ),
        computed(
            "MovementSpeedWhileUsingSkill",
            Cat::SkillMechanics,
            Vt::Number,
            "MovementSpeedWhileUsingSkill",
        ),
        computed(
            "PresenceRadiusMetres",
            Cat::Defence,
            Vt::Number,
            "PresenceRadiusMetres",
        ),
        computed(
            "AuraEffectMod",
            Cat::SkillMechanics,
            Vt::Number,
            "AuraEffectMod",
        ),
        computed(
            "CurseEffectMod",
            Cat::SkillMechanics,
            Vt::Number,
            "CurseEffectMod",
        ),
        computed(
            "ActiveMinionLimit",
            Cat::SkillMechanics,
            Vt::Number,
            "ActiveMinionLimit",
        ),
        computed("ExplodeChance", Cat::Offence, Vt::Percent, "ExplodeChance"),
        computed(
            "CombinedAvgToMonsterLife",
            Cat::Offence,
            Vt::Percent,
            "CombinedAvgToMonsterLife",
        ),
        computed("CritBifurcates", Cat::Offence, Vt::Number, "CritBifurcates"),
        // Damage and accuracy views
        computed("AverageDamage", Cat::Offence, Vt::Number, "AverageDamage"),
        computed(
            "AverageBurstDamage",
            Cat::Offence,
            Vt::Number,
            "AverageBurstDamage",
        ),
        computed("CombinedAvg", Cat::Offence, Vt::Number, "CombinedAvg"),
        computed("HitSpeed", Cat::SkillMechanics, Vt::Number, "HitSpeed"),
        computed("IgniteDamage", Cat::DotDamage, Vt::Number, "IgniteDamage"),
        computed("BleedDamage", Cat::DotDamage, Vt::Number, "BleedDamage"),
        computed("PoisonDamage", Cat::DotDamage, Vt::Number, "PoisonDamage"),
        computed("WithIgniteDPS", Cat::Offence, Vt::Number, "WithIgniteDPS"),
        computed("WithBleedDPS", Cat::Offence, Vt::Number, "WithBleedDPS"),
        computed("WithPoisonDPS", Cat::Offence, Vt::Number, "WithPoisonDPS"),
        computed(
            "MainHandAccuracy",
            Cat::Offence,
            Vt::Number,
            "MainHand.Accuracy",
        ),
        computed(
            "OffHandAccuracy",
            Cat::Offence,
            Vt::Number,
            "OffHand.Accuracy",
        ),
        computed(
            "AccuracyHitChanceUncapped",
            Cat::Offence,
            Vt::Percent,
            "AccuracyHitChanceUncapped",
        ),
        computed(
            "QuantityMultiplier",
            Cat::SkillMechanics,
            Vt::Number,
            "QuantityMultiplier",
        ),
        computed("Duration", Cat::SkillMechanics, Vt::TimeSeconds, "Duration"),
        computed(
            "DurationSecondary",
            Cat::SkillMechanics,
            Vt::TimeSeconds,
            "DurationSecondary",
        ),
        computed(
            "AuraDuration",
            Cat::SkillMechanics,
            Vt::TimeSeconds,
            "AuraDuration",
        ),
        computed(
            "ReserveDuration",
            Cat::SkillMechanics,
            Vt::TimeSeconds,
            "ReserveDuration",
        ),
        computed(
            "AreaOfEffectRadiusMetres",
            Cat::SkillMechanics,
            Vt::Number,
            "AreaOfEffectRadiusMetres",
        ),
        computed(
            "PierceCount",
            Cat::SkillMechanics,
            Vt::Number,
            "PierceCountString",
        ),
        computed(
            "ForkCount",
            Cat::SkillMechanics,
            Vt::Number,
            "ForkCountString",
        ),
        computed(
            "ChainMax",
            Cat::SkillMechanics,
            Vt::Number,
            "ChainMaxString",
        ),
        computed(
            "SplitCount",
            Cat::SkillMechanics,
            Vt::Number,
            "SplitCountString",
        ),
        computed(
            "BounceCount",
            Cat::SkillMechanics,
            Vt::Number,
            "BounceCount",
        ),
        computed("ReqStr", Cat::Requirement, Vt::Number, "ReqStr"),
        computed("ReqDex", Cat::Requirement, Vt::Number, "ReqDex"),
        computed("ReqInt", Cat::Requirement, Vt::Number, "ReqInt"),
        computed(
            "ManaPercentCost",
            Cat::SkillMechanics,
            Vt::Percent,
            "ManaPercentCost",
        ),
        computed(
            "ManaPercentPerSecondCost",
            Cat::SkillMechanics,
            Vt::Percent,
            "ManaPercentPerSecondCost",
        ),
        computed(
            "ManaPerSecondCost",
            Cat::SkillMechanics,
            Vt::Number,
            "ManaPerSecondCost",
        ),
        computed(
            "LifePercentCost",
            Cat::SkillMechanics,
            Vt::Percent,
            "LifePercentCost",
        ),
        computed(
            "LifePercentPerSecondCost",
            Cat::SkillMechanics,
            Vt::Percent,
            "LifePercentPerSecondCost",
        ),
        computed(
            "LifePerSecondCost",
            Cat::SkillMechanics,
            Vt::Number,
            "LifePerSecondCost",
        ),
        computed("ESCost", Cat::SkillMechanics, Vt::Number, "ESCost"),
        computed(
            "ESPercentCost",
            Cat::SkillMechanics,
            Vt::Percent,
            "ESPercentCost",
        ),
        computed(
            "ESPercentPerSecondCost",
            Cat::SkillMechanics,
            Vt::Percent,
            "ESPercentPerSecondCost",
        ),
        computed(
            "ESPerSecondCost",
            Cat::SkillMechanics,
            Vt::Number,
            "ESPerSecondCost",
        ),
        computed("WardCost", Cat::SkillMechanics, Vt::Number, "WardCost"),
        computed(
            "WardPercentCost",
            Cat::SkillMechanics,
            Vt::Percent,
            "WardPercentCost",
        ),
        computed(
            "WardPercentPerSecondCost",
            Cat::SkillMechanics,
            Vt::Percent,
            "WardPercentPerSecondCost",
        ),
        computed(
            "WardPerSecondCost",
            Cat::SkillMechanics,
            Vt::Number,
            "WardPerSecondCost",
        ),
        computed("RageCost", Cat::SkillMechanics, Vt::Number, "RageCost"),
        computed(
            "RagePerSecondCost",
            Cat::SkillMechanics,
            Vt::Number,
            "RagePerSecondCost",
        ),
        computed(
            "TotalTotemPlacementTime",
            Cat::SkillMechanics,
            Vt::TimeSeconds,
            "TotemPlacementTime",
        ),
        computed(
            "TrapThrowingTime",
            Cat::SkillMechanics,
            Vt::TimeSeconds,
            "TrapThrowingTime",
        ),
        computed(
            "TrapThrowCount",
            Cat::SkillMechanics,
            Vt::Number,
            "TrapThrowCount",
        ),
        computed(
            "MineLayingTime",
            Cat::SkillMechanics,
            Vt::TimeSeconds,
            "MineLayingTime",
        ),
        computed(
            "MineThrowCount",
            Cat::SkillMechanics,
            Vt::Number,
            "MineThrowCount",
        ),
        computed(
            "WarcryCastTime",
            Cat::SkillMechanics,
            Vt::TimeSeconds,
            "WarcryCastTime",
        ),
        computed(
            "ChannelTime",
            Cat::SkillMechanics,
            Vt::TimeSeconds,
            "ChannelTime",
        ),
        computed(
            "ChannelTimeToTrigger",
            Cat::SkillMechanics,
            Vt::TimeSeconds,
            "ChannelTimeToTrigger",
        ),
        computed(
            "ArchonDuration",
            Cat::SkillMechanics,
            Vt::TimeSeconds,
            "ArchonDuration",
        ),
        computed(
            "ArchonRecoveryPeriod",
            Cat::SkillMechanics,
            Vt::TimeSeconds,
            "ArchonRecoveryPeriod",
        ),
        // Offence
        computed("TotalDPS", Cat::Offence, Vt::Number, "TotalDPS"),
        computed("TotalHitAvg", Cat::HitDamage, Vt::Number, "AverageHit"),
        computed("FullDPS", Cat::Offence, Vt::Number, "FullDPS"),
        computed("FullDotDPS", Cat::DotDamage, Vt::Number, "FullDotDPS"),
        computed("CombinedDPS", Cat::Offence, Vt::Number, "CombinedDPS"),
        computed("Accuracy", Cat::Offence, Vt::Number, "Accuracy"),
        computed("HitChance", Cat::Offence, Vt::Percent, "HitChance"),
        computed("ActionRate", Cat::SkillMechanics, Vt::Number, "Speed"),
        computed(
            "EffectiveActionRate",
            Cat::SkillMechanics,
            Vt::Number,
            "Speed",
        ),
        computed("CritChance", Cat::Offence, Vt::Percent, "CritChance"),
        computed("CritMultiplier", Cat::Offence, Vt::Number, "CritMultiplier"),
        // DoT / Ailment
        computed("TotalDotDPS", Cat::DotDamage, Vt::Number, "TotalDotDPS"),
        computed("BleedDPS", Cat::DotDamage, Vt::Number, "BleedDPS"),
        computed("IgniteDPS", Cat::DotDamage, Vt::Number, "IgniteDPS"),
        computed("PoisonDPS", Cat::DotDamage, Vt::Number, "PoisonDPS"),
        computed("ShockEffect", Cat::Ailment, Vt::Number, "ShockEffectMod"),
        // Resource
        computed("Life", Cat::Resource, Vt::Number, "Life"),
        computed("Mana", Cat::Resource, Vt::Number, "Mana"),
        computed("EnergyShield", Cat::Resource, Vt::Number, "EnergyShield"),
        computed("LifeReserved", Cat::Resource, Vt::Number, "LifeReserved")
            .with_higher_is_better(Some(false)),
        computed(
            "LifeUnreserved",
            Cat::Resource,
            Vt::Number,
            "LifeUnreserved",
        ),
        computed("ManaReserved", Cat::Resource, Vt::Number, "ManaReserved")
            .with_higher_is_better(Some(false)),
        computed(
            "ManaUnreserved",
            Cat::Resource,
            Vt::Number,
            "ManaUnreserved",
        ),
        // Recovery
        computed(
            "LifeRegenRecovery",
            Cat::Recovery,
            Vt::Number,
            "LifeRegenRecovery",
        ),
        computed(
            "ManaRegenRecovery",
            Cat::Recovery,
            Vt::Number,
            "ManaRegenRecovery",
        ),
        computed(
            "EnergyShieldRegenRecovery",
            Cat::Recovery,
            Vt::Number,
            "EnergyShieldRegenRecovery",
        ),
        computed(
            "WardRegenRecovery",
            Cat::Recovery,
            Vt::Number,
            "WardRegenRecovery",
        ),
        computed(
            "RageRegenRecovery",
            Cat::Recovery,
            Vt::Number,
            "RageRegenRecovery",
        ),
        computed("NetLifeRegen", Cat::Recovery, Vt::Number, "NetLifeRegen"),
        computed("NetManaRegen", Cat::Recovery, Vt::Number, "NetManaRegen"),
        computed(
            "NetEnergyShieldRegen",
            Cat::Recovery,
            Vt::Number,
            "NetEnergyShieldRegen",
        ),
        computed("TotalNetRegen", Cat::Recovery, Vt::Number, "TotalNetRegen"),
        computed(
            "TotalBuildDegen",
            Cat::Recovery,
            Vt::Number,
            "TotalBuildDegen",
        ),
        computed(
            "LifeLeechGainRate",
            Cat::Recovery,
            Vt::Number,
            "LifeLeechGainRate",
        ),
        computed(
            "ManaLeechGainRate",
            Cat::Recovery,
            Vt::Number,
            "ManaLeechGainRate",
        ),
        computed(
            "EnergyShieldLeechGainRate",
            Cat::Recovery,
            Vt::Number,
            "EnergyShieldLeechGainRate",
        ),
        computed(
            "LifeLeechGainPerHit",
            Cat::Recovery,
            Vt::Number,
            "LifeLeechGainPerHit",
        ),
        computed(
            "ManaLeechGainPerHit",
            Cat::Recovery,
            Vt::Number,
            "ManaLeechGainPerHit",
        ),
        computed(
            "EnergyShieldLeechGainPerHit",
            Cat::Recovery,
            Vt::Number,
            "EnergyShieldLeechGainPerHit",
        ),
        computed("LifeRegen", Cat::Recovery, Vt::Number, "LifeRegen"),
        computed("ManaRegen", Cat::Recovery, Vt::Number, "ManaRegen"),
        computed(
            "EnergyShieldRegen",
            Cat::Recovery,
            Vt::Number,
            "EnergyShieldRegen",
        ),
        // Defence / Mitigation
        computed("Armour", Cat::Defence, Vt::Number, "Armour"),
        computed("Evasion", Cat::Defence, Vt::Number, "Evasion"),
        computed("FireResist", Cat::Resistance, Vt::Percent, "FireResist"),
        computed("ColdResist", Cat::Resistance, Vt::Percent, "ColdResist"),
        computed(
            "LightningResist",
            Cat::Resistance,
            Vt::Percent,
            "LightningResist",
        ),
        computed("ChaosResist", Cat::Resistance, Vt::Percent, "ChaosResist"),
        computed(
            "FireResistMax",
            Cat::Resistance,
            Vt::Percent,
            "FireResistMax",
        ),
        computed(
            "ColdResistMax",
            Cat::Resistance,
            Vt::Percent,
            "ColdResistMax",
        ),
        computed(
            "LightningResistMax",
            Cat::Resistance,
            Vt::Percent,
            "LightningResistMax",
        ),
        computed(
            "ChaosResistMax",
            Cat::Resistance,
            Vt::Percent,
            "ChaosResistMax",
        ),
        computed("BlockChance", Cat::Avoidance, Vt::Percent, "BlockChance"),
        computed(
            "SpellBlockChance",
            Cat::Avoidance,
            Vt::Percent,
            "SpellBlockChance",
        ),
        // EHP / max hit
        computed("TotalEHP", Cat::Mitigation, Vt::Number, "TotalEHP"),
        computed(
            "PhysicalMaxHit",
            Cat::Mitigation,
            Vt::Number,
            "PhysicalMaximumHitTaken",
        ),
        computed(
            "FireMaxHit",
            Cat::Mitigation,
            Vt::Number,
            "FireMaximumHitTaken",
        ),
        computed(
            "ColdMaxHit",
            Cat::Mitigation,
            Vt::Number,
            "ColdMaximumHitTaken",
        ),
        computed(
            "LightningMaxHit",
            Cat::Mitigation,
            Vt::Number,
            "LightningMaximumHitTaken",
        ),
        computed(
            "ChaosMaxHit",
            Cat::Mitigation,
            Vt::Number,
            "ChaosMaximumHitTaken",
        ),
        // ES Recharge (Wave 2)
        computed(
            "EsRechargeRate",
            Cat::Recovery,
            Vt::Percent,
            "EnergyShieldRechargeRate",
        ),
        computed(
            "EsRechargeDelay",
            Cat::Recovery,
            Vt::TimeSeconds,
            "EnergyShieldRechargeDelay",
        )
        .with_higher_is_better(Some(false)),
        computed(
            "EsRechargePerSecond",
            Cat::Recovery,
            Vt::Number,
            "EnergyShieldRechargePerSecond",
        ),
        // Avoidance (Wave 2)
        computed(
            "AvoidAllDamageFromHits",
            Cat::Avoidance,
            Vt::Percent,
            "AvoidAllDamageFromHits",
        ),
        computed(
            "AvoidProjectileDamage",
            Cat::Avoidance,
            Vt::Percent,
            "AvoidProjectileDamage",
        ),
        computed("AvoidStun", Cat::Avoidance, Vt::Percent, "AvoidStun"),
        computed("AvoidIgnite", Cat::Avoidance, Vt::Percent, "AvoidIgnite"),
        computed("AvoidShock", Cat::Avoidance, Vt::Percent, "AvoidShock"),
        computed("AvoidChill", Cat::Avoidance, Vt::Percent, "AvoidChill"),
        computed("AvoidFreeze", Cat::Avoidance, Vt::Percent, "AvoidFreeze"),
        computed("AvoidPoison", Cat::Avoidance, Vt::Percent, "AvoidPoison"),
        computed(
            "AvoidBleeding",
            Cat::Avoidance,
            Vt::Percent,
            "AvoidBleeding",
        ),
        // Taken multipliers (Wave 2)
        computed(
            "TakenMultiPhysical",
            Cat::Mitigation,
            Vt::Number,
            "PhysicalDamageTakenMultiplier",
        )
        .with_higher_is_better(Some(false)),
        computed(
            "TakenMultiFire",
            Cat::Mitigation,
            Vt::Number,
            "FireDamageTakenMultiplier",
        )
        .with_higher_is_better(Some(false)),
        computed(
            "TakenMultiCold",
            Cat::Mitigation,
            Vt::Number,
            "ColdDamageTakenMultiplier",
        )
        .with_higher_is_better(Some(false)),
        computed(
            "TakenMultiLightning",
            Cat::Mitigation,
            Vt::Number,
            "LightningDamageTakenMultiplier",
        )
        .with_higher_is_better(Some(false)),
        computed(
            "TakenMultiChaos",
            Cat::Mitigation,
            Vt::Number,
            "ChaosDamageTakenMultiplier",
        )
        .with_higher_is_better(Some(false)),
        computed(
            "CritExtraDamageReduction",
            Cat::Mitigation,
            Vt::Percent,
            "CritExtraDamageReduction",
        ),
        computed(
            "EnemyCritEffect",
            Cat::Mitigation,
            Vt::Number,
            "EnemyCritEffect",
        )
        .with_higher_is_better(Some(false)),
        // Charges (Wave 2 / Lane A)
        computed(
            "ChargePowerCurrent",
            Cat::Utility,
            Vt::Number,
            "PowerCharges",
        ),
        computed(
            "ChargePowerMaximum",
            Cat::Utility,
            Vt::Number,
            "PowerChargesMax",
        ),
        computed(
            "ChargeFrenzyCurrent",
            Cat::Utility,
            Vt::Number,
            "FrenzyCharges",
        ),
        computed(
            "ChargeFrenzyMaximum",
            Cat::Utility,
            Vt::Number,
            "FrenzyChargesMax",
        ),
        computed(
            "ChargeEnduranceCurrent",
            Cat::Utility,
            Vt::Number,
            "EnduranceCharges",
        ),
        computed(
            "ChargeEnduranceMaximum",
            Cat::Utility,
            Vt::Number,
            "EnduranceChargesMax",
        ),
        computed(
            "PowerChargesDuration",
            Cat::Utility,
            Vt::TimeSeconds,
            "PowerChargesDuration",
        ),
        computed(
            "FrenzyChargesDuration",
            Cat::Utility,
            Vt::TimeSeconds,
            "FrenzyChargesDuration",
        ),
        computed(
            "EnduranceChargesDuration",
            Cat::Utility,
            Vt::TimeSeconds,
            "EnduranceChargesDuration",
        ),
        // Leech / Recoup (Wave 2 / Lane A)
        computed("LifeLeechRate", Cat::Recovery, Vt::Number, "LifeLeechRate"),
        computed("ManaLeechRate", Cat::Recovery, Vt::Number, "ManaLeechRate"),
        computed(
            "EsLeechRate",
            Cat::Recovery,
            Vt::Number,
            "EnergyShieldLeechRate",
        ),
        computed(
            "LifeRecoupRate",
            Cat::Recovery,
            Vt::Number,
            "LifeRecoupRate",
        ),
        computed(
            "EsRecoupRate",
            Cat::Recovery,
            Vt::Number,
            "EnergyShieldRecoupRate",
        ),
        computed(
            "AilmentThreshold",
            Cat::Defence,
            Vt::Number,
            "AilmentThreshold",
        ),
        computed("LifeOnKill", Cat::Recovery, Vt::Number, "LifeOnKill"),
        computed("ManaOnKill", Cat::Recovery, Vt::Number, "ManaOnKill"),
        computed(
            "EnergyShieldOnKill",
            Cat::Recovery,
            Vt::Number,
            "EnergyShieldOnKill",
        ),
        computed("DazeChance", Cat::Ailment, Vt::Percent, "DazeChance"),
        computed(
            "FullyBrokenArmourEffect",
            Cat::Ailment,
            Vt::Percent,
            "FullyBrokenArmourEffect",
        ),
        // Ailment extensions (Wave 2 / Lane B)
        computed("ChillEffect", Cat::Ailment, Vt::Percent, "ChillEffect"),
        computed(
            "FreezeBuildupPct",
            Cat::Ailment,
            Vt::Percent,
            "FreezeBuildupPerHit",
        ),
        computed(
            "ElectrocuteBuildupPct",
            Cat::Ailment,
            Vt::Percent,
            "ElectrocuteBuildupPerHit",
        ),
        computed(
            "BleedStackedDPS",
            Cat::DotDamage,
            Vt::Number,
            "BleedStackedDPS",
        ),
        computed(
            "BleedActiveStacks",
            Cat::DotDamage,
            Vt::Number,
            "BleedActiveStacks",
        ),
        computed(
            "PoisonStackedDPS",
            Cat::DotDamage,
            Vt::Number,
            "PoisonStackedDPS",
        ),
        computed(
            "PoisonActiveStacks",
            Cat::DotDamage,
            Vt::Number,
            "PoisonActiveStacks",
        ),
        // Skill mechanics (Wave 2 / Lane C)
        computed("AoeRadius", Cat::SkillMechanics, Vt::Number, "AreaOfEffect"),
        computed(
            "AoeAreaMod",
            Cat::SkillMechanics,
            Vt::Number,
            "AreaOfEffectMod",
        ),
        computed(
            "ProjectileCount",
            Cat::SkillMechanics,
            Vt::Number,
            "ProjectileCount",
        ),
        computed("Cooldown", Cat::SkillMechanics, Vt::TimeSeconds, "Cooldown")
            .with_higher_is_better(Some(false)),
        computed(
            "CooldownStoredUses",
            Cat::SkillMechanics,
            Vt::Number,
            "CooldownStoredUses",
        ),
        computed("ManaCost", Cat::Cost, Vt::Number, "ManaCost").with_higher_is_better(Some(false)),
        computed("LifeCost", Cat::Cost, Vt::Number, "LifeCost").with_higher_is_better(Some(false)),
        computed("SpiritReserved", Cat::Cost, Vt::Number, "SpiritReserved")
            .with_higher_is_better(Some(false)),
        // Trigger (Wave 2 / Lane 4)
        computed(
            "TriggerRateCap",
            Cat::SkillMechanics,
            Vt::Number,
            "TriggerRateCap",
        ),
        computed(
            "SkillTriggerRate",
            Cat::SkillMechanics,
            Vt::Number,
            "SkillTriggerRate",
        ),
        // --- Defence extensions (entered the catalog in W0.2 → flipped to
        //     Computed in F-3; golden compares against the matching key in
        //     meta.json::player_stats) ---
        // The Spirit pool (Track D wiring).
        computed("Spirit", Cat::Resource, Vt::Number, "Spirit"),
        computed(
            "SpiritUnreserved",
            Cat::Resource,
            Vt::Number,
            "SpiritUnreserved",
        ),
        // The Block family (Track D; CalcDefence.lua:961-1058).
        computed(
            "BlockChanceMax",
            Cat::Avoidance,
            Vt::Percent,
            "BlockChanceMax",
        ),
        computed(
            "SpellBlockChanceMax",
            Cat::Avoidance,
            Vt::Percent,
            "SpellBlockChanceMax",
        ),
        computed(
            "EffectiveBlockChance",
            Cat::Avoidance,
            Vt::Percent,
            "EffectiveBlockChance",
        ),
        computed(
            "EffectiveSpellBlockChance",
            Cat::Avoidance,
            Vt::Percent,
            "EffectiveSpellBlockChance",
        ),
        // The percentage of incoming hit damage prevented by a block.
        computed("BlockEffect", Cat::Mitigation, Vt::Percent, "BlockEffect"),
        // Deflection (Track D; CalcDefence.lua:48-54, :1487-1506).
        computed(
            "DeflectionRating",
            Cat::Avoidance,
            Vt::Number,
            "DeflectionRating",
        ),
        computed(
            "DeflectChance",
            Cat::Avoidance,
            Vt::Percent,
            "DeflectChance",
        ),
        // The four Evade subtypes + the combined figure (Track E; CalcDefence.lua:1396-1466).
        computed("EvadeChance", Cat::Avoidance, Vt::Percent, "EvadeChance"),
        computed(
            "MeleeEvadeChance",
            Cat::Avoidance,
            Vt::Percent,
            "MeleeEvadeChance",
        ),
        computed(
            "ProjectileEvadeChance",
            Cat::Avoidance,
            Vt::Percent,
            "ProjectileEvadeChance",
        ),
        computed(
            "SpellEvadeChance",
            Cat::Avoidance,
            Vt::Percent,
            "SpellEvadeChance",
        ),
        computed(
            "SpellProjectileEvadeChance",
            Cat::Avoidance,
            Vt::Percent,
            "SpellProjectileEvadeChance",
        ),
        // The Stun system (Track E; CalcDefence.lua:2525-2643).
        computed("StunThreshold", Cat::Defence, Vt::Number, "StunThreshold"),
        computed(
            "SelfStunChance",
            Cat::Defence,
            Vt::Percent,
            "SelfStunChance",
        )
        .with_higher_is_better(Some(false)),
        computed(
            "StunDuration",
            Cat::Defence,
            Vt::TimeSeconds,
            "StunDuration",
        )
        .with_higher_is_better(Some(false)),
        // The Ward pool (Track D; CalcDefence.lua:1144-1273).
        computed("Ward", Cat::Resource, Vt::Number, "Ward"),
        computed("WardRegen", Cat::Recovery, Vt::Number, "WardRegen"),
        // Pool-value semantics (Track F; the pool snapshot values consumed by the EHP loop).
        computed(
            "LifeRecoverable",
            Cat::Resource,
            Vt::Number,
            "LifeRecoverable",
        ),
        computed(
            "EnergyShieldRecoveryCap",
            Cat::Resource,
            Vt::Number,
            "EnergyShieldRecoveryCap",
        ),
        computed(
            "PhysicalDamageReduction",
            Cat::Mitigation,
            Vt::Percent,
            "PhysicalDamageReduction",
        ),
        // The new EHP semantics (Track F; CalcDefence.lua:2979-3153, :3246-3247, :3322).
        computed(
            "NumberOfDamagingHits",
            Cat::Mitigation,
            Vt::Number,
            "NumberOfDamagingHits",
        ),
        computed(
            "NumberOfMitigatedHits",
            Cat::Mitigation,
            Vt::Number,
            "NumberOfMitigatedDamagingHits",
        ),
        // Retained EHP hit terms; totals and ratios are views of the same snapshot.
        computed(
            "PhysicalEnemyDamage",
            Cat::Mitigation,
            Vt::Number,
            "PhysicalEnemyDamage",
        ),
        computed(
            "FireEnemyDamage",
            Cat::Mitigation,
            Vt::Number,
            "FireEnemyDamage",
        ),
        computed(
            "ColdEnemyDamage",
            Cat::Mitigation,
            Vt::Number,
            "ColdEnemyDamage",
        ),
        computed(
            "LightningEnemyDamage",
            Cat::Mitigation,
            Vt::Number,
            "LightningEnemyDamage",
        ),
        computed(
            "ChaosEnemyDamage",
            Cat::Mitigation,
            Vt::Number,
            "ChaosEnemyDamage",
        ),
        computed(
            "PhysicalTakenDamage",
            Cat::Mitigation,
            Vt::Number,
            "PhysicalTakenDamage",
        ),
        computed(
            "FireTakenDamage",
            Cat::Mitigation,
            Vt::Number,
            "FireTakenDamage",
        ),
        computed(
            "ColdTakenDamage",
            Cat::Mitigation,
            Vt::Number,
            "ColdTakenDamage",
        ),
        computed(
            "LightningTakenDamage",
            Cat::Mitigation,
            Vt::Number,
            "LightningTakenDamage",
        ),
        computed(
            "ChaosTakenDamage",
            Cat::Mitigation,
            Vt::Number,
            "ChaosTakenDamage",
        ),
        computed(
            "PhysicalTakenHit",
            Cat::Mitigation,
            Vt::Number,
            "PhysicalTakenHit",
        ),
        computed("FireTakenHit", Cat::Mitigation, Vt::Number, "FireTakenHit"),
        computed("ColdTakenHit", Cat::Mitigation, Vt::Number, "ColdTakenHit"),
        computed(
            "LightningTakenHit",
            Cat::Mitigation,
            Vt::Number,
            "LightningTakenHit",
        ),
        computed(
            "ChaosTakenHit",
            Cat::Mitigation,
            Vt::Number,
            "ChaosTakenHit",
        ),
        computed(
            "PhysicalTakenHitMult",
            Cat::Mitigation,
            Vt::Number,
            "PhysicalTakenHitMult",
        ),
        computed(
            "FireTakenHitMult",
            Cat::Mitigation,
            Vt::Number,
            "FireTakenHitMult",
        ),
        computed(
            "ColdTakenHitMult",
            Cat::Mitigation,
            Vt::Number,
            "ColdTakenHitMult",
        ),
        computed(
            "LightningTakenHitMult",
            Cat::Mitigation,
            Vt::Number,
            "LightningTakenHitMult",
        ),
        computed(
            "ChaosTakenHitMult",
            Cat::Mitigation,
            Vt::Number,
            "ChaosTakenHitMult",
        ),
        computed(
            "PhysicalEnergyShieldBypass",
            Cat::Mitigation,
            Vt::Percent,
            "PhysicalEnergyShieldBypass",
        ),
        computed(
            "FireEnergyShieldBypass",
            Cat::Mitigation,
            Vt::Percent,
            "FireEnergyShieldBypass",
        ),
        computed(
            "ColdEnergyShieldBypass",
            Cat::Mitigation,
            Vt::Percent,
            "ColdEnergyShieldBypass",
        ),
        computed(
            "LightningEnergyShieldBypass",
            Cat::Mitigation,
            Vt::Percent,
            "LightningEnergyShieldBypass",
        ),
        computed(
            "ChaosEnergyShieldBypass",
            Cat::Mitigation,
            Vt::Percent,
            "ChaosEnergyShieldBypass",
        ),
        computed(
            "FireDamageReduction",
            Cat::Mitigation,
            Vt::Percent,
            "FireDamageReduction",
        ),
        computed(
            "ColdDamageReduction",
            Cat::Mitigation,
            Vt::Percent,
            "ColdDamageReduction",
        ),
        computed(
            "LightningDamageReduction",
            Cat::Mitigation,
            Vt::Percent,
            "LightningDamageReduction",
        ),
        computed(
            "totalEnemyDamage",
            Cat::Mitigation,
            Vt::Number,
            "totalEnemyDamage",
        ),
        computed(
            "totalTakenDamage",
            Cat::Mitigation,
            Vt::Number,
            "totalTakenDamage",
        ),
        computed(
            "totalTakenHit",
            Cat::Mitigation,
            Vt::Number,
            "totalTakenHit",
        ),
        computed(
            "ConfiguredDamageChance",
            Cat::Mitigation,
            Vt::Percent,
            "ConfiguredDamageChance",
        ),
        computed(
            "ConfiguredNotHitChance",
            Cat::Mitigation,
            Vt::Percent,
            "ConfiguredNotHitChance",
        ),
        computed(
            "DamageTakenOnBlock",
            Cat::Mitigation,
            Vt::Percent,
            "DamageTakenOnBlock",
        ),
        // The old lowest-max-hit semantics are kept as a supplementary metric
        // (still comparable after F switches total_ehp's semantics).
        computed(
            "TotalEHPLowestMaxHit",
            Cat::Mitigation,
            Vt::Number,
            "TotalEHPLowestMaxHit",
        ),
        // Additional sidebar views of the calculation's existing results.
        computed("Str", Cat::Requirement, Vt::Number, "Str"),
        computed("Dex", Cat::Requirement, Vt::Number, "Dex"),
        computed("Int", Cat::Requirement, Vt::Number, "Int"),
        computed("Rage", Cat::Resource, Vt::Number, "Rage"),
        computed(
            "FireResistOverCap",
            Cat::Resistance,
            Vt::Percent,
            "FireResistOverCap",
        ),
        computed(
            "ColdResistOverCap",
            Cat::Resistance,
            Vt::Percent,
            "ColdResistOverCap",
        ),
        computed(
            "LightningResistOverCap",
            Cat::Resistance,
            Vt::Percent,
            "LightningResistOverCap",
        ),
        computed(
            "ChaosResistOverCap",
            Cat::Resistance,
            Vt::Percent,
            "ChaosResistOverCap",
        ),
        computed(
            "LifeUnreservedPercent",
            Cat::Resource,
            Vt::Percent,
            "LifeUnreservedPercent",
        ),
        computed(
            "ManaUnreservedPercent",
            Cat::Resource,
            Vt::Percent,
            "ManaUnreservedPercent",
        ),
        computed(
            "SpiritUnreservedPercent",
            Cat::Resource,
            Vt::Percent,
            "SpiritUnreservedPercent",
        ),
        computed(
            "PreEffectiveCritChance",
            Cat::Offence,
            Vt::Percent,
            "PreEffectiveCritChance",
        ),
        computed("StoredUses", Cat::SkillMechanics, Vt::Number, "StoredUses"),
        computed("TotalDot", Cat::DotDamage, Vt::Number, "TotalDot"),
        computed("WithDotDPS", Cat::Offence, Vt::Number, "WithDotDPS"),
        computed("FiringRate", Cat::SkillMechanics, Vt::Number, "FiringRate"),
        computed(
            "ReloadTime",
            Cat::SkillMechanics,
            Vt::TimeSeconds,
            "ReloadTime",
        )
        .with_higher_is_better(Some(false)),
        computed("CullingDPS", Cat::Offence, Vt::Number, "CullingDPS"),
        computed(
            "ProjectileSpeedMod",
            Cat::SkillMechanics,
            Vt::Number,
            "ProjectileSpeedMod",
        ),
    ];
    for (id, _) in crate::calc::utility::GLOBAL_STATS {
        catalog.push(computed(
            id,
            Cat::Recovery,
            if *id == "FlaskEffect" {
                Vt::Percent
            } else {
                Vt::Number
            },
            id,
        ));
    }
    for (kind, count) in [("Flask", 2), ("Charm", 3)] {
        for slot in 1..=count {
            for field in crate::calc::utility::ITEM_STATS
                .iter()
                .chain(if kind == "Charm" {
                    crate::calc::utility::CHARM_STATS
                } else {
                    crate::calc::utility::FLASK_STATS
                })
            {
                if kind == "Charm"
                    && field.contains("Recovery")
                    && !crate::calc::utility::CHARM_STATS.contains(field)
                {
                    continue;
                }
                let id = format!("{kind}{slot}{field}");
                let vt = if *field == "Duration" {
                    Vt::TimeSeconds
                } else {
                    Vt::Number
                };
                catalog.push(
                    DisplayStatDefinition::computed(id.as_str(), Cat::Recovery, vt)
                        .with_pob_key(id.as_str())
                        .with_higher_is_better(if *field == "ChargesUsed" {
                            Some(false)
                        } else {
                            None
                        }),
                );
            }
        }
    }
    catalog
}

/// Extracts the current value of every `Computed` display field from an
/// `OutputTable`, in the same order as [`display_catalog`].
pub fn extract_display_values(output: &OutputTable) -> Vec<DisplayStatValue> {
    display_catalog()
        .into_iter()
        .filter(|def| def.parity_status == ParityStatus::Computed)
        .filter(|def| {
            output.full_dps.is_some() || !matches!(def.id.as_str(), "FullDPS" | "FullDotDPS")
        })
        .filter(|def| {
            crate::calc::utility::item_stat(def.id.as_str()).is_none()
                || output.utility_stats.contains_key(def.id.as_str())
        })
        .map(|def| {
            let value = display_value(output, def.id.as_str());
            DisplayStatValue {
                id: def.id,
                value,
                category: def.category,
            }
        })
        .collect()
}

/// Reads one display field without allocating the full catalog. Units match
/// [`extract_display_values`], including fractional chance to percent conversion.
/// Unknown ids return 0.
pub fn display_value(output: &OutputTable, id: &str) -> f64 {
    if let Some(value) = output.utility_stats.get(id) {
        return *value;
    }
    match id {
        "DecayDPS" => output.decay_dps,
        "MirageBurningGroundDPS" => output.mirage_burning_ground_dps,

        "ReservationDPS" => output.reservation_dps,
        "NetWardRegen" => output.net_ward_regen,
        "MirageDPS" => output.mirage_dps,
        "BurningGroundDPS" => output.burning_ground_dps,

        "EffectiveMovementSpeedMod" => output.effective_movement_speed_mod,
        "MovementSpeedWhileUsingSkill" => output.movement_speed_while_using_skill,
        "PresenceRadiusMetres" => output.presence_radius_metres,
        "AuraEffectMod" => output.aura_effect_mod,
        "CurseEffectMod" => output.curse_effect_mod,
        "ActiveMinionLimit" => output.active_minion_limit,
        "ExplodeChance" => output.explode_chance,
        "CombinedAvgToMonsterLife" => output.combined_avg_to_monster_life,
        "CritBifurcates" => output.crit_bifurcates,

        "TotalTotemPlacementTime" => output.total_totem_placement_time,
        "TrapThrowingTime" => output.trap_throwing_time,
        "TrapThrowCount" => output.trap_throw_count,
        "MineLayingTime" => output.mine_laying_time,
        "MineThrowCount" => output.mine_throw_count,
        "WarcryCastTime" => output.warcry_cast_time,
        "ChannelTime" => output.channel_time,
        "ChannelTimeToTrigger" => output.channel_time_to_trigger,
        "ArchonDuration" => output.archon_duration,
        "ArchonRecoveryPeriod" => output.archon_recovery_period,
        "ManaPercentCost" => output.mana_percent_cost,
        "ManaPercentPerSecondCost" => output.mana_percent_per_second_cost,
        "ManaPerSecondCost" => output.mana_per_second_cost,
        "LifePercentCost" => output.life_percent_cost,
        "LifePercentPerSecondCost" => output.life_percent_per_second_cost,
        "LifePerSecondCost" => output.life_per_second_cost,
        "ESCost" => output.es_cost,
        "ESPercentCost" => output.es_percent_cost,
        "ESPercentPerSecondCost" => output.es_percent_per_second_cost,
        "ESPerSecondCost" => output.es_per_second_cost,
        "WardCost" => output.ward_cost,
        "WardPercentCost" => output.ward_percent_cost,
        "WardPercentPerSecondCost" => output.ward_percent_per_second_cost,
        "WardPerSecondCost" => output.ward_per_second_cost,
        "RageCost" => output.rage_cost,
        "RagePerSecondCost" => output.rage_per_second_cost,
        "ReqStr" => output.req_str,
        "ReqDex" => output.req_dex,
        "ReqInt" => output.req_int,
        "PierceCount" => output.pierce_count,
        "ForkCount" => output.fork_count,
        "ChainMax" => output.chain_max,
        "SplitCount" => output.split_count,
        "BounceCount" => output.bounce_count,
        "AreaOfEffectRadiusMetres" => output.aoe_radius / 10.0,
        "Duration" => output.duration,
        "DurationSecondary" => output.duration_secondary,
        "AuraDuration" => output.aura_duration,
        "ReserveDuration" => output.reserve_duration,
        "AverageDamage" => output.average_damage,
        "AverageBurstDamage" => output.average_burst_damage,
        "CombinedAvg" => output.combined_avg,
        "HitSpeed" => output.hit_speed,
        "IgniteDamage" => output.ignite_damage,
        "BleedDamage" => output.bleed_damage,
        "PoisonDamage" => output.poison_damage,
        "WithIgniteDPS" => output.with_ignite_dps,
        "WithBleedDPS" => output.with_bleed_dps,
        "WithPoisonDPS" => output.with_poison_dps,
        "MainHandAccuracy" => output.main_hand.as_ref().map_or(0.0, |hand| hand.accuracy),
        "OffHandAccuracy" => output.off_hand.as_ref().map_or(0.0, |hand| hand.accuracy),
        "AccuracyHitChanceUncapped" => output.accuracy_hit_chance_uncapped,
        "QuantityMultiplier" => output.quantity_multiplier,
        "Str" => output.strength,
        "Dex" => output.dexterity,
        "Int" => output.intelligence,
        "Rage" => output.rage,
        "FireResistOverCap" => output.fire_resistance_over_cap,
        "ColdResistOverCap" => output.cold_resistance_over_cap,
        "LightningResistOverCap" => output.lightning_resistance_over_cap,
        "ChaosResistOverCap" => output.chaos_resistance_over_cap,
        "LifeUnreservedPercent" => unreserved_percent(output.life_unreserved, output.life),
        "ManaUnreservedPercent" => unreserved_percent(output.mana_unreserved, output.mana),
        "SpiritUnreservedPercent" => unreserved_percent(output.spirit_unreserved, output.spirit),
        "PreEffectiveCritChance" => output.pre_effective_crit_chance * 100.0,
        "StoredUses" => f64::from(output.cooldown_stored_uses),
        "TotalDot" => output.skill_total_dot,
        "WithDotDPS" => output.with_dot_dps,
        "FiringRate" => output.firing_rate,
        "ReloadTime" => output.reload_time,
        "CullingDPS" => output.culling_dps,
        "ProjectileSpeedMod" => output.projectile_speed_mod,
        "TotalDPS" => output.dps,
        "TotalHitAvg" => output.total_hit_avg,
        // hit_chance / crit_chance are fractions (0..1) on the calc side, but
        // the display contract's Vt::Percent expects 0..100 (matching every
        // other Percent field), so we convert here at extraction time.
        "Accuracy" => output.accuracy,
        "HitChance" => output.hit_chance * 100.0,
        "ActionRate" => output.action_rate,
        "EffectiveActionRate" => output.effective_action_rate,
        "CritChance" => output.crit_chance * 100.0,
        "CritMultiplier" => output.crit_multiplier,
        "FullDPS" => output.full_dps.as_ref().map_or(0.0, |full| full.dps),
        "FullDotDPS" => output.full_dps.as_ref().map_or(0.0, |full| full.dot_dps),
        "CombinedDPS" => output.combined_dps,
        "TotalDotDPS" => output.total_dot_dps,
        "BleedDPS" => output.bleed_dps,
        "IgniteDPS" => output.ignite_dps,
        "PoisonDPS" => output.poison_dps,
        "ShockEffect" => output.shock_effect_mod,
        "Life" => output.life,
        "Mana" => output.mana,
        "EnergyShield" => output.energy_shield,
        "LifeReserved" => output.life_reserved,
        "LifeUnreserved" => output.life_unreserved,
        "ManaReserved" => output.mana_reserved,
        "ManaUnreserved" => output.mana_unreserved,
        "LifeRegenRecovery" => output.life_regen_recovery,
        "ManaRegenRecovery" => output.mana_regen_recovery,
        "EnergyShieldRegenRecovery" => output.energy_shield_regen_recovery,
        "WardRegenRecovery" => output.ward_regen_recovery,
        "RageRegenRecovery" => output.rage_regen_recovery,
        "NetLifeRegen" => output.net_life_regen,
        "NetManaRegen" => output.net_mana_regen,
        "NetEnergyShieldRegen" => output.net_energy_shield_regen,
        "TotalNetRegen" => output.total_net_regen,
        "TotalBuildDegen" => output.total_build_degen,
        "LifeLeechGainRate" => output.life_leech_gain_rate,
        "ManaLeechGainRate" => output.mana_leech_gain_rate,
        "EnergyShieldLeechGainRate" => output.energy_shield_leech_gain_rate,
        "LifeLeechGainPerHit" => output.life_leech_gain_per_hit,
        "ManaLeechGainPerHit" => output.mana_leech_gain_per_hit,
        "EnergyShieldLeechGainPerHit" => output.energy_shield_leech_gain_per_hit,
        "LifeRegen" => output.life_regen,
        "ManaRegen" => output.mana_regen,
        "EnergyShieldRegen" => output.energy_shield_regen,
        "Armour" => output.armour,
        "Evasion" => output.evasion,
        "FireResist" => output.fire_resistance,
        "ColdResist" => output.cold_resistance,
        "LightningResist" => output.lightning_resistance,
        "ChaosResist" => output.chaos_resistance,
        "FireResistMax" => output.max_fire_resistance,
        "ColdResistMax" => output.max_cold_resistance,
        "LightningResistMax" => output.max_lightning_resistance,
        "ChaosResistMax" => output.max_chaos_resistance,
        "BlockChance" => output.block_chance,
        "SpellBlockChance" => output.spell_block_chance,
        "TotalEHP" => output.total_ehp,
        "PhysicalMaxHit" => output.physical_max_hit,
        "FireMaxHit" => output.fire_max_hit,
        "ColdMaxHit" => output.cold_max_hit,
        "LightningMaxHit" => output.lightning_max_hit,
        "ChaosMaxHit" => output.chaos_max_hit,

        // ES Recharge
        "EsRechargeRate" => output.es_recharge_rate,
        "EsRechargeDelay" => output.es_recharge_delay,
        "EsRechargePerSecond" => output.es_recharge_per_second,

        // Avoidance
        "AvoidAllDamageFromHits" => output.avoid_all_damage_from_hits,
        "AvoidProjectileDamage" => output.avoid_projectile_damage,
        "AvoidStun" => output.avoid_stun,
        "AvoidIgnite" => output.avoid_ignite,
        "AvoidShock" => output.avoid_shock,
        "AvoidChill" => output.avoid_chill,
        "AvoidFreeze" => output.avoid_freeze,
        "AvoidPoison" => output.avoid_poison,
        "AvoidBleeding" => output.avoid_bleeding,

        // Taken multipliers
        "TakenMultiPhysical" => output.taken_multi_physical,
        "TakenMultiFire" => output.taken_multi_fire,
        "TakenMultiCold" => output.taken_multi_cold,
        "TakenMultiLightning" => output.taken_multi_lightning,
        "TakenMultiChaos" => output.taken_multi_chaos,
        "CritExtraDamageReduction" => output.crit_extra_damage_reduction,
        "EnemyCritEffect" => output.enemy_crit_effect,

        // Charges
        "ChargePowerCurrent" => output.charge_power_current,
        "ChargePowerMaximum" => output.charge_power_maximum as f64,
        "ChargeFrenzyCurrent" => output.charge_frenzy_current,
        "ChargeFrenzyMaximum" => output.charge_frenzy_maximum as f64,
        "ChargeEnduranceCurrent" => output.charge_endurance_current,
        "ChargeEnduranceMaximum" => output.charge_endurance_maximum as f64,

        "PowerChargesDuration" => output.charge_power_duration,
        "FrenzyChargesDuration" => output.charge_frenzy_duration,
        "EnduranceChargesDuration" => output.charge_endurance_duration,

        // Leech / Recoup
        "LifeLeechRate" => output.life_leech_rate,
        "ManaLeechRate" => output.mana_leech_rate,
        "EsLeechRate" => output.es_leech_rate,
        "LifeRecoupRate" => output.life_recoup_rate,
        "EsRecoupRate" => output.es_recoup_rate,

        // Ailment extensions
        "ChillEffect" => output.chill_effect,
        "AilmentThreshold" => output.ailment_threshold,
        "LifeOnKill" => output.life_on_kill,
        "ManaOnKill" => output.mana_on_kill,
        "EnergyShieldOnKill" => output.energy_shield_on_kill,
        "DazeChance" => output.daze_chance,
        "FullyBrokenArmourEffect" => output.fully_broken_armour_effect,
        "FreezeBuildupPct" => output.freeze_buildup_pct,
        "ElectrocuteBuildupPct" => output.electrocute_buildup_pct,
        "BleedStackedDPS" => output.bleed_stacked_dps,
        "BleedActiveStacks" => output.bleed_active_stacks,
        "PoisonStackedDPS" => output.poison_stacked_dps,
        "PoisonActiveStacks" => output.poison_active_stacks,

        // Skill mechanics
        "AoeRadius" => output.aoe_radius,
        "AoeAreaMod" => output.aoe_area_mod,
        "ProjectileCount" => output.projectile_count,
        "Cooldown" => output.cooldown,
        "CooldownStoredUses" => output.cooldown_stored_uses as f64,
        "ManaCost" => output.mana_cost,
        "LifeCost" => output.life_cost,
        "SpiritReserved" => output.spirit_reserved,

        // Trigger
        "TriggerRateCap" => output.trigger_rate_cap,
        "SkillTriggerRate" => output.skill_trigger_rate,

        // Defence extensions (the W0.2 mapping is already in place; entries enter extract once F-3 flips them to Computed)
        "Spirit" => output.spirit,
        "SpiritUnreserved" => output.spirit_unreserved,
        "BlockChanceMax" => output.block_chance_max,
        "SpellBlockChanceMax" => output.spell_block_chance_max,
        "EffectiveBlockChance" => output.effective_block_chance,
        "EffectiveSpellBlockChance" => output.effective_spell_block_chance,
        "BlockEffect" => 100.0 - output.block_effect,
        "DeflectionRating" => output.deflection_rating,
        "DeflectChance" => output.deflect_chance,
        "EvadeChance" => output.evade_chance,
        "MeleeEvadeChance" => output.melee_evade_chance,
        "ProjectileEvadeChance" => output.projectile_evade_chance,
        "SpellEvadeChance" => output.spell_evade_chance,
        "SpellProjectileEvadeChance" => output.spell_projectile_evade_chance,
        "ExposureEffectOnSelf" => output.exposure_effect_on_self,
        "WitherEffectOnSelf" => output.wither_effect_on_self,
        "SelfBleedEffect" => output.self_bleed_effect,
        "SelfIgniteEffect" => output.self_ignite_effect,
        "DebuffExpirationModifier" => output.debuff_expiration_modifier,
        "CurseEffectOnSelf" => output.curse_effect_on_self,
        "SelfBleedDuration" => output.self_bleed_duration,
        "SelfPoisonDuration" => output.self_poison_duration,
        "SelfIgniteDuration" => output.self_ignite_duration,
        "SelfChillDuration" => output.self_chill_duration,
        "SelfFreezeDuration" => output.self_freeze_duration,
        "SelfShockDuration" => output.self_shock_duration,
        "CorruptedBloodImmunity" => f64::from(output.corrupted_blood_immunity),
        "MaimImmunity" => f64::from(output.maim_immunity),
        "HinderImmunity" => f64::from(output.hinder_immunity),
        "StunThreshold" => output.stun_threshold,
        "SelfStunChance" => output.self_stun_chance,
        "StunDuration" => output.stun_duration,
        "Ward" => output.ward,
        "WardRegen" => output.ward_regen,
        "LifeRecoverable" => output.life_recoverable,
        "EnergyShieldRecoveryCap" => output.energy_shield_recovery_cap,
        "PhysicalDamageReduction" => output.physical_damage_reduction,
        "NumberOfDamagingHits" => output.number_of_damaging_hits,
        "NumberOfMitigatedHits" => output.number_of_mitigated_hits,
        "PhysicalEnemyDamage" => output.ehp_terms.as_ref().map_or(0.0, |t| t.enemy_damage[0]),
        "PhysicalTakenDamage" => output.ehp_terms.as_ref().map_or(0.0, |t| t.taken_damage[0]),
        "PhysicalTakenHit" => output.ehp_terms.as_ref().map_or(0.0, |t| t.taken_hit[0]),
        "PhysicalEnergyShieldBypass" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.energy_shield_bypass[0]),
        "PhysicalTakenHitMult" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.taken_hit_multiplier(0)),
        "FireEnemyDamage" => output.ehp_terms.as_ref().map_or(0.0, |t| t.enemy_damage[1]),
        "FireTakenDamage" => output.ehp_terms.as_ref().map_or(0.0, |t| t.taken_damage[1]),
        "FireTakenHit" => output.ehp_terms.as_ref().map_or(0.0, |t| t.taken_hit[1]),
        "FireEnergyShieldBypass" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.energy_shield_bypass[1]),
        "FireTakenHitMult" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.taken_hit_multiplier(1)),
        "FireDamageReduction" => output.ehp_terms.as_ref().map_or(0.0, |t| {
            100.0 - t.damage_reduction[1].taken_multiplier() * 100.0
        }),
        "ColdEnemyDamage" => output.ehp_terms.as_ref().map_or(0.0, |t| t.enemy_damage[2]),
        "ColdTakenDamage" => output.ehp_terms.as_ref().map_or(0.0, |t| t.taken_damage[2]),
        "ColdTakenHit" => output.ehp_terms.as_ref().map_or(0.0, |t| t.taken_hit[2]),
        "ColdEnergyShieldBypass" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.energy_shield_bypass[2]),
        "ColdTakenHitMult" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.taken_hit_multiplier(2)),
        "ColdDamageReduction" => output.ehp_terms.as_ref().map_or(0.0, |t| {
            100.0 - t.damage_reduction[2].taken_multiplier() * 100.0
        }),
        "LightningEnemyDamage" => output.ehp_terms.as_ref().map_or(0.0, |t| t.enemy_damage[3]),
        "LightningTakenDamage" => output.ehp_terms.as_ref().map_or(0.0, |t| t.taken_damage[3]),
        "LightningTakenHit" => output.ehp_terms.as_ref().map_or(0.0, |t| t.taken_hit[3]),
        "LightningEnergyShieldBypass" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.energy_shield_bypass[3]),
        "LightningTakenHitMult" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.taken_hit_multiplier(3)),
        "LightningDamageReduction" => output.ehp_terms.as_ref().map_or(0.0, |t| {
            100.0 - t.damage_reduction[3].taken_multiplier() * 100.0
        }),
        "ChaosEnemyDamage" => output.ehp_terms.as_ref().map_or(0.0, |t| t.enemy_damage[4]),
        "ChaosTakenDamage" => output.ehp_terms.as_ref().map_or(0.0, |t| t.taken_damage[4]),
        "ChaosTakenHit" => output.ehp_terms.as_ref().map_or(0.0, |t| t.taken_hit[4]),
        "ChaosEnergyShieldBypass" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.energy_shield_bypass[4]),
        "ChaosTakenHitMult" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.taken_hit_multiplier(4)),
        "totalEnemyDamage" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.enemy_damage.iter().sum()),
        "totalTakenDamage" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.taken_damage.iter().sum()),
        "totalTakenHit" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.taken_hit.iter().sum()),
        "ConfiguredDamageChance" => output.ehp_terms.as_ref().map_or(0.0, |t| t.damage_chance),
        "ConfiguredNotHitChance" => output.ehp_terms.as_ref().map_or(0.0, |t| t.not_hit_chance),
        "DamageTakenOnBlock" => output.block_effect,
        "TotalEHPLowestMaxHit" => output.total_ehp_lowest_max_hit,

        _ => 0.0,
    }
}

/// A pool with no capacity has no unreserved share to display.
fn unreserved_percent(unreserved: f64, maximum: f64) -> f64 {
    if maximum > 0.0 {
        unreserved / maximum * 100.0
    } else {
        0.0
    }
}
