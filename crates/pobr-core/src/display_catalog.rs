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
        computed("PhysicalMin", Cat::Offence, Vt::Number, "PhysicalMin"),
        computed("PhysicalMax", Cat::Offence, Vt::Number, "PhysicalMax"),
        computed(
            "PhysicalEffMult",
            Cat::Offence,
            Vt::Number,
            "PhysicalEffMult",
        ),
        computed("FireMin", Cat::Offence, Vt::Number, "FireMin"),
        computed("FireMax", Cat::Offence, Vt::Number, "FireMax"),
        computed("FireEffMult", Cat::Offence, Vt::Number, "FireEffMult"),
        computed("ColdMin", Cat::Offence, Vt::Number, "ColdMin"),
        computed("ColdMax", Cat::Offence, Vt::Number, "ColdMax"),
        computed("ColdEffMult", Cat::Offence, Vt::Number, "ColdEffMult"),
        computed("LightningMin", Cat::Offence, Vt::Number, "LightningMin"),
        computed("LightningMax", Cat::Offence, Vt::Number, "LightningMax"),
        computed(
            "LightningEffMult",
            Cat::Offence,
            Vt::Number,
            "LightningEffMult",
        ),
        computed("ChaosMin", Cat::Offence, Vt::Number, "ChaosMin"),
        computed("ChaosMax", Cat::Offence, Vt::Number, "ChaosMax"),
        computed("ChaosEffMult", Cat::Offence, Vt::Number, "ChaosEffMult"),
        computed("TotalMin", Cat::Offence, Vt::Number, "TotalMin"),
        computed("TotalMax", Cat::Offence, Vt::Number, "TotalMax"),
        computed("CritEffect", Cat::Offence, Vt::Number, "CritEffect"),
        computed("DpsMultiplier", Cat::Offence, Vt::Number, "DpsMultiplier"),
        computed(
            "MainHandPhysicalMin",
            Cat::Offence,
            Vt::Number,
            "MainHand.PhysicalMin",
        ),
        computed(
            "MainHandPhysicalMax",
            Cat::Offence,
            Vt::Number,
            "MainHand.PhysicalMax",
        ),
        computed(
            "MainHandPhysicalEffMult",
            Cat::Offence,
            Vt::Number,
            "MainHand.PhysicalEffMult",
        ),
        computed(
            "MainHandFireMin",
            Cat::Offence,
            Vt::Number,
            "MainHand.FireMin",
        ),
        computed(
            "MainHandFireMax",
            Cat::Offence,
            Vt::Number,
            "MainHand.FireMax",
        ),
        computed(
            "MainHandFireEffMult",
            Cat::Offence,
            Vt::Number,
            "MainHand.FireEffMult",
        ),
        computed(
            "MainHandColdMin",
            Cat::Offence,
            Vt::Number,
            "MainHand.ColdMin",
        ),
        computed(
            "MainHandColdMax",
            Cat::Offence,
            Vt::Number,
            "MainHand.ColdMax",
        ),
        computed(
            "MainHandColdEffMult",
            Cat::Offence,
            Vt::Number,
            "MainHand.ColdEffMult",
        ),
        computed(
            "MainHandLightningMin",
            Cat::Offence,
            Vt::Number,
            "MainHand.LightningMin",
        ),
        computed(
            "MainHandLightningMax",
            Cat::Offence,
            Vt::Number,
            "MainHand.LightningMax",
        ),
        computed(
            "MainHandLightningEffMult",
            Cat::Offence,
            Vt::Number,
            "MainHand.LightningEffMult",
        ),
        computed(
            "MainHandChaosMin",
            Cat::Offence,
            Vt::Number,
            "MainHand.ChaosMin",
        ),
        computed(
            "MainHandChaosMax",
            Cat::Offence,
            Vt::Number,
            "MainHand.ChaosMax",
        ),
        computed(
            "MainHandChaosEffMult",
            Cat::Offence,
            Vt::Number,
            "MainHand.ChaosEffMult",
        ),
        computed(
            "MainHandTotalMin",
            Cat::Offence,
            Vt::Number,
            "MainHand.TotalMin",
        ),
        computed(
            "MainHandTotalMax",
            Cat::Offence,
            Vt::Number,
            "MainHand.TotalMax",
        ),
        computed(
            "MainHandCritEffect",
            Cat::Offence,
            Vt::Number,
            "MainHand.CritEffect",
        ),
        computed(
            "MainHandDpsMultiplier",
            Cat::Offence,
            Vt::Number,
            "MainHand.DpsMultiplier",
        ),
        computed(
            "OffHandPhysicalMin",
            Cat::Offence,
            Vt::Number,
            "OffHand.PhysicalMin",
        ),
        computed(
            "OffHandPhysicalMax",
            Cat::Offence,
            Vt::Number,
            "OffHand.PhysicalMax",
        ),
        computed(
            "OffHandPhysicalEffMult",
            Cat::Offence,
            Vt::Number,
            "OffHand.PhysicalEffMult",
        ),
        computed(
            "OffHandFireMin",
            Cat::Offence,
            Vt::Number,
            "OffHand.FireMin",
        ),
        computed(
            "OffHandFireMax",
            Cat::Offence,
            Vt::Number,
            "OffHand.FireMax",
        ),
        computed(
            "OffHandFireEffMult",
            Cat::Offence,
            Vt::Number,
            "OffHand.FireEffMult",
        ),
        computed(
            "OffHandColdMin",
            Cat::Offence,
            Vt::Number,
            "OffHand.ColdMin",
        ),
        computed(
            "OffHandColdMax",
            Cat::Offence,
            Vt::Number,
            "OffHand.ColdMax",
        ),
        computed(
            "OffHandColdEffMult",
            Cat::Offence,
            Vt::Number,
            "OffHand.ColdEffMult",
        ),
        computed(
            "OffHandLightningMin",
            Cat::Offence,
            Vt::Number,
            "OffHand.LightningMin",
        ),
        computed(
            "OffHandLightningMax",
            Cat::Offence,
            Vt::Number,
            "OffHand.LightningMax",
        ),
        computed(
            "OffHandLightningEffMult",
            Cat::Offence,
            Vt::Number,
            "OffHand.LightningEffMult",
        ),
        computed(
            "OffHandChaosMin",
            Cat::Offence,
            Vt::Number,
            "OffHand.ChaosMin",
        ),
        computed(
            "OffHandChaosMax",
            Cat::Offence,
            Vt::Number,
            "OffHand.ChaosMax",
        ),
        computed(
            "OffHandChaosEffMult",
            Cat::Offence,
            Vt::Number,
            "OffHand.ChaosEffMult",
        ),
        computed(
            "OffHandTotalMin",
            Cat::Offence,
            Vt::Number,
            "OffHand.TotalMin",
        ),
        computed(
            "OffHandTotalMax",
            Cat::Offence,
            Vt::Number,
            "OffHand.TotalMax",
        ),
        computed(
            "OffHandCritEffect",
            Cat::Offence,
            Vt::Number,
            "OffHand.CritEffect",
        ),
        computed(
            "OffHandDpsMultiplier",
            Cat::Offence,
            Vt::Number,
            "OffHand.DpsMultiplier",
        ),
        computed("Repeats", Cat::Offence, Vt::Number, "Repeats"),
        computed(
            "TotalNumberOfHits",
            Cat::Defence,
            Vt::Number,
            "TotalNumberOfHits",
        ),
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
            "SelfChillEffect",
            Cat::Defence,
            Vt::Percent,
            "SelfChillEffect",
        )
        .with_higher_is_better(Some(false)),
        computed(
            "SelfFreezeEffect",
            Cat::Defence,
            Vt::Percent,
            "SelfFreezeEffect",
        )
        .with_higher_is_better(Some(false)),
        computed(
            "SelfPoisonEffect",
            Cat::Defence,
            Vt::Percent,
            "SelfPoisonEffect",
        )
        .with_higher_is_better(Some(false)),
        computed(
            "SelfBlindDuration",
            Cat::Defence,
            Vt::Percent,
            "SelfBlindDuration",
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
        computed(
            "IgniteChancePerHit",
            Cat::Ailment,
            Vt::Percent,
            "IgniteChancePerHit",
        ),
        computed(
            "IgniteDuration",
            Cat::Ailment,
            Vt::TimeSeconds,
            "IgniteDuration",
        ),
        computed("IgniteEffMult", Cat::Ailment, Vt::Number, "IgniteEffMult"),
        computed(
            "IgniteMagnitudeEffect",
            Cat::Ailment,
            Vt::Number,
            "IgniteMagnitudeEffect",
        ),
        computed(
            "IgniteRollAverage",
            Cat::Ailment,
            Vt::Percent,
            "IgniteRollAverage",
        ),
        computed(
            "IgniteStacksMax",
            Cat::Ailment,
            Vt::Number,
            "IgniteStacksMax",
        ),
        computed(
            "IgniteStackPotentialPercent",
            Cat::Ailment,
            Vt::Percent,
            "IgniteStackPotentialPercent",
        ),
        computed("IgniteFireMin", Cat::Ailment, Vt::Number, "IgniteFireMin"),
        computed("IgniteFireMax", Cat::Ailment, Vt::Number, "IgniteFireMax"),
        computed("IgniteColdMin", Cat::Ailment, Vt::Number, "IgniteColdMin"),
        computed("IgniteColdMax", Cat::Ailment, Vt::Number, "IgniteColdMax"),
        computed(
            "IgniteLightningMin",
            Cat::Ailment,
            Vt::Number,
            "IgniteLightningMin",
        ),
        computed(
            "IgniteLightningMax",
            Cat::Ailment,
            Vt::Number,
            "IgniteLightningMax",
        ),
        computed("BleedChance", Cat::Ailment, Vt::Percent, "BleedChance"),
        computed(
            "BleedDuration",
            Cat::Ailment,
            Vt::TimeSeconds,
            "BleedDuration",
        ),
        computed("BleedEffMult", Cat::Ailment, Vt::Number, "BleedEffMult"),
        computed(
            "BleedMagnitudeEffect",
            Cat::Ailment,
            Vt::Number,
            "BleedMagnitudeEffect",
        ),
        computed(
            "BleedRollAverage",
            Cat::Ailment,
            Vt::Percent,
            "BleedRollAverage",
        ),
        computed("BleedStacksMax", Cat::Ailment, Vt::Number, "BleedStacksMax"),
        computed(
            "BleedStackPotentialPercent",
            Cat::Ailment,
            Vt::Percent,
            "BleedStackPotentialPercent",
        ),
        computed(
            "BleedPhysicalMin",
            Cat::Ailment,
            Vt::Number,
            "BleedPhysicalMin",
        ),
        computed(
            "BleedPhysicalMax",
            Cat::Ailment,
            Vt::Number,
            "BleedPhysicalMax",
        ),
        computed("PoisonChance", Cat::Ailment, Vt::Percent, "PoisonChance"),
        computed(
            "PoisonDuration",
            Cat::Ailment,
            Vt::TimeSeconds,
            "PoisonDuration",
        ),
        computed("PoisonEffMult", Cat::Ailment, Vt::Number, "PoisonEffMult"),
        computed(
            "PoisonMagnitudeEffect",
            Cat::Ailment,
            Vt::Number,
            "PoisonMagnitudeEffect",
        ),
        computed(
            "PoisonRollAverage",
            Cat::Ailment,
            Vt::Percent,
            "PoisonRollAverage",
        ),
        computed(
            "PoisonStacksMax",
            Cat::Ailment,
            Vt::Number,
            "PoisonStacksMax",
        ),
        computed(
            "PoisonStackPotentialPercent",
            Cat::Ailment,
            Vt::Percent,
            "PoisonStackPotentialPercent",
        ),
        computed(
            "PoisonPhysicalMin",
            Cat::Ailment,
            Vt::Number,
            "PoisonPhysicalMin",
        ),
        computed(
            "PoisonPhysicalMax",
            Cat::Ailment,
            Vt::Number,
            "PoisonPhysicalMax",
        ),
        computed("PoisonChaosMin", Cat::Ailment, Vt::Number, "PoisonChaosMin"),
        computed("PoisonChaosMax", Cat::Ailment, Vt::Number, "PoisonChaosMax"),
        computed("ShockChance", Cat::Ailment, Vt::Percent, "ShockChance"),
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
        // Limits, speed and recovery multipliers (neutral 1), and ailment
        // maximums; values come from the stages that resolve them.
        computed("PresenceRadius", Cat::Defence, Vt::Number, "PresenceRadius"),
        computed("CharmLimit", Cat::Utility, Vt::Number, "CharmLimit"),
        computed(
            "EnemyCurseLimit",
            Cat::SkillMechanics,
            Vt::Number,
            "EnemyCurseLimit",
        ),
        computed(
            "EnemyMarkLimit",
            Cat::SkillMechanics,
            Vt::Number,
            "EnemyMarkLimit",
        ),
        computed("ActionSpeedMod", Cat::Defence, Vt::Number, "ActionSpeedMod"),
        computed(
            "MovementSpeedMod",
            Cat::Defence,
            Vt::Number,
            "MovementSpeedMod",
        ),
        computed("LightRadiusMod", Cat::Misc, Vt::Number, "LightRadiusMod"),
        computed("GemLevel", Cat::Misc, Vt::Number, "GemLevel"),
        computed(
            "EffectiveLootRarityMod",
            Cat::Misc,
            Vt::Number,
            "EffectiveLootRarityMod",
        ),
        computed(
            "LifeRecoveryRateMod",
            Cat::Recovery,
            Vt::Number,
            "LifeRecoveryRateMod",
        ),
        computed(
            "ManaRecoveryRateMod",
            Cat::Recovery,
            Vt::Number,
            "ManaRecoveryRateMod",
        ),
        computed(
            "EnergyShieldRecoveryRateMod",
            Cat::Recovery,
            Vt::Number,
            "EnergyShieldRecoveryRateMod",
        ),
        computed(
            "LowLifePercentage",
            Cat::Resource,
            Vt::Percent,
            "LowLifePercentage",
        ),
        computed(
            "LifeCancellableReservation",
            Cat::Resource,
            Vt::Percent,
            "LifeCancellableReservation",
        ),
        computed(
            "ManaCancellableReservation",
            Cat::Resource,
            Vt::Percent,
            "ManaCancellableReservation",
        ),
        computed(
            "FullLifePercentage",
            Cat::Resource,
            Vt::Percent,
            "FullLifePercentage",
        ),
        computed(
            "DurationMod",
            Cat::SkillMechanics,
            Vt::Number,
            "DurationMod",
        ),
        computed(
            "StrikeTargets",
            Cat::SkillMechanics,
            Vt::Number,
            "StrikeTargets",
        ),
        computed(
            "LowestAttribute",
            Cat::Resource,
            Vt::Number,
            "LowestAttribute",
        ),
        computed(
            "GlobalGemAttributeRequirements",
            Cat::Requirement,
            Vt::Number,
            "GlobalGemAttributeRequirements",
        ),
        computed(
            "GlobalItemAttributeRequirements",
            Cat::Requirement,
            Vt::Number,
            "GlobalItemAttributeRequirements",
        ),
        computed(
            "GlobalWeaponAttributeRequirements",
            Cat::Requirement,
            Vt::Number,
            "GlobalWeaponAttributeRequirements",
        ),
        computed("MaximumChill", Cat::Ailment, Vt::Percent, "MaximumChill"),
        computed("MaximumShock", Cat::Ailment, Vt::Percent, "MaximumShock"),
        computed("CurrentChill", Cat::Ailment, Vt::Percent, "CurrentChill"),
        computed("CurrentShock", Cat::Ailment, Vt::Percent, "CurrentShock"),
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
        computed(
            "AverageBurstHits",
            Cat::Offence,
            Vt::Number,
            "AverageBurstHits",
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
            "AccuracyHitChance",
            Cat::Offence,
            Vt::Percent,
            "AccuracyHitChance",
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
        computed("TotemLife", Cat::SkillMechanics, Vt::Number, "TotemLife"),
        computed("TotemArmour", Cat::SkillMechanics, Vt::Number, "TotemArmour"),
        computed("TotemEvasion", Cat::SkillMechanics, Vt::Number, "TotemEvasion"),
        computed(
            "TotemEnergyShield",
            Cat::SkillMechanics,
            Vt::Number,
            "TotemEnergyShield",
        ),
        computed(
            "TotemDurationMod",
            Cat::SkillMechanics,
            Vt::Number,
            "TotemDurationMod",
        ),
        computed(
            "TotemFireResist",
            Cat::SkillMechanics,
            Vt::Percent,
            "TotemFireResist",
        ),
        computed(
            "TotemColdResist",
            Cat::SkillMechanics,
            Vt::Percent,
            "TotemColdResist",
        ),
        computed(
            "TotemLightningResist",
            Cat::SkillMechanics,
            Vt::Percent,
            "TotemLightningResist",
        ),
        computed(
            "TotemChaosResist",
            Cat::SkillMechanics,
            Vt::Percent,
            "TotemChaosResist",
        ),
        computed(
            "TotemFireResistTotal",
            Cat::SkillMechanics,
            Vt::Percent,
            "TotemFireResistTotal",
        ),
        computed(
            "TotemColdResistTotal",
            Cat::SkillMechanics,
            Vt::Percent,
            "TotemColdResistTotal",
        ),
        computed(
            "TotemLightningResistTotal",
            Cat::SkillMechanics,
            Vt::Percent,
            "TotemLightningResistTotal",
        ),
        computed(
            "TotemChaosResistTotal",
            Cat::SkillMechanics,
            Vt::Percent,
            "TotemChaosResistTotal",
        ),
        computed(
            "MissingTotemFireResist",
            Cat::SkillMechanics,
            Vt::Percent,
            "MissingTotemFireResist",
        )
        .with_higher_is_better(Some(false)),
        computed(
            "MissingTotemColdResist",
            Cat::SkillMechanics,
            Vt::Percent,
            "MissingTotemColdResist",
        )
        .with_higher_is_better(Some(false)),
        computed(
            "MissingTotemLightningResist",
            Cat::SkillMechanics,
            Vt::Percent,
            "MissingTotemLightningResist",
        )
        .with_higher_is_better(Some(false)),
        computed(
            "MissingTotemChaosResist",
            Cat::SkillMechanics,
            Vt::Percent,
            "MissingTotemChaosResist",
        )
        .with_higher_is_better(Some(false)),
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
        computed(
            "EnemyCritChance",
            Cat::Mitigation,
            Vt::Percent,
            "EnemyCritChance",
        )
        .with_higher_is_better(Some(false)),
        computed(
            "SecondMinimalMaximumHitTaken",
            Cat::Mitigation,
            Vt::Number,
            "SecondMinimalMaximumHitTaken",
        ),
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
            "MaxLifeLeechRate",
            Cat::Recovery,
            Vt::Number,
            "MaxLifeLeechRate",
        ),
        computed(
            "MaxManaLeechRate",
            Cat::Recovery,
            Vt::Number,
            "MaxManaLeechRate",
        ),
        computed(
            "MaxEnergyShieldLeechRate",
            Cat::Recovery,
            Vt::Number,
            "MaxEnergyShieldLeechRate",
        ),
        computed(
            "MaxLifeLeechRatePercent",
            Cat::Recovery,
            Vt::Percent,
            "MaxLifeLeechRatePercent",
        ),
        computed(
            "LifeRecoupRate",
            Cat::Recovery,
            Vt::Number,
            "LifeRecoupRate",
        ),
        computed(
            "ManaRecoupRate",
            Cat::Recovery,
            Vt::Number,
            "ManaRecoupRate",
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
        computed(
            "HeavyStunBuildupAvg",
            Cat::Ailment,
            Vt::Percent,
            "HeavyStunBuildupAvg",
        ),
        computed("PinBuildupAvg", Cat::Ailment, Vt::Percent, "PinBuildupAvg"),
        computed("DazeChance", Cat::Ailment, Vt::Percent, "DazeChance"),
        computed(
            "FullyBrokenArmourEffect",
            Cat::Ailment,
            Vt::Percent,
            "FullyBrokenArmourEffect",
        ),
        // Ailment extensions (Wave 2 / Lane B)
        computed("ChillEffect", Cat::Ailment, Vt::Percent, "ChillEffect"),
        computed("ChillEffectMod", Cat::Ailment, Vt::Number, "ChillEffectMod"),
        computed(
            "ChillDuration",
            Cat::Ailment,
            Vt::TimeSeconds,
            "ChillDuration",
        ),
        computed(
            "ShockDuration",
            Cat::Ailment,
            Vt::TimeSeconds,
            "ShockDuration",
        ),
        computed(
            "FreezeBuildupAvg",
            Cat::Ailment,
            Vt::Percent,
            "FreezeBuildupAvg",
        ),
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
        computed(
            "DeflectEffect",
            Cat::Avoidance,
            Vt::Percent,
            "DeflectEffect",
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
        // Further retained EHP terms: enemy assembly, the mitigation snapshot and hit pools.
        computed(
            "PhysicalEnemyDamageMult",
            Cat::Mitigation,
            Vt::Number,
            "PhysicalEnemyDamageMult",
        ),
        computed(
            "FireEnemyDamageMult",
            Cat::Mitigation,
            Vt::Number,
            "FireEnemyDamageMult",
        ),
        computed(
            "ColdEnemyDamageMult",
            Cat::Mitigation,
            Vt::Number,
            "ColdEnemyDamageMult",
        ),
        computed(
            "LightningEnemyDamageMult",
            Cat::Mitigation,
            Vt::Number,
            "LightningEnemyDamageMult",
        ),
        computed(
            "ChaosEnemyDamageMult",
            Cat::Mitigation,
            Vt::Number,
            "ChaosEnemyDamageMult",
        ),
        computed(
            "PhysicalEnemyPen",
            Cat::Mitigation,
            Vt::Percent,
            "PhysicalEnemyPen",
        ),
        computed("FireEnemyPen", Cat::Mitigation, Vt::Percent, "FireEnemyPen"),
        computed("ColdEnemyPen", Cat::Mitigation, Vt::Percent, "ColdEnemyPen"),
        computed(
            "LightningEnemyPen",
            Cat::Mitigation,
            Vt::Percent,
            "LightningEnemyPen",
        ),
        computed(
            "ChaosEnemyPen",
            Cat::Mitigation,
            Vt::Percent,
            "ChaosEnemyPen",
        ),
        computed(
            "PhysicalResistTakenHitMulti",
            Cat::Mitigation,
            Vt::Number,
            "PhysicalResistTakenHitMulti",
        ),
        computed(
            "FireResistTakenHitMulti",
            Cat::Mitigation,
            Vt::Number,
            "FireResistTakenHitMulti",
        ),
        computed(
            "ColdResistTakenHitMulti",
            Cat::Mitigation,
            Vt::Number,
            "ColdResistTakenHitMulti",
        ),
        computed(
            "LightningResistTakenHitMulti",
            Cat::Mitigation,
            Vt::Number,
            "LightningResistTakenHitMulti",
        ),
        computed(
            "ChaosResistTakenHitMulti",
            Cat::Mitigation,
            Vt::Number,
            "ChaosResistTakenHitMulti",
        ),
        computed(
            "PhysicalAttackTakenHitMult",
            Cat::Mitigation,
            Vt::Number,
            "PhysicalAttackTakenHitMult",
        ),
        computed(
            "FireAttackTakenHitMult",
            Cat::Mitigation,
            Vt::Number,
            "FireAttackTakenHitMult",
        ),
        computed(
            "ColdAttackTakenHitMult",
            Cat::Mitigation,
            Vt::Number,
            "ColdAttackTakenHitMult",
        ),
        computed(
            "LightningAttackTakenHitMult",
            Cat::Mitigation,
            Vt::Number,
            "LightningAttackTakenHitMult",
        ),
        computed(
            "ChaosAttackTakenHitMult",
            Cat::Mitigation,
            Vt::Number,
            "ChaosAttackTakenHitMult",
        ),
        computed(
            "PhysicalSpellTakenHitMult",
            Cat::Mitigation,
            Vt::Number,
            "PhysicalSpellTakenHitMult",
        ),
        computed(
            "FireSpellTakenHitMult",
            Cat::Mitigation,
            Vt::Number,
            "FireSpellTakenHitMult",
        ),
        computed(
            "ColdSpellTakenHitMult",
            Cat::Mitigation,
            Vt::Number,
            "ColdSpellTakenHitMult",
        ),
        computed(
            "LightningSpellTakenHitMult",
            Cat::Mitigation,
            Vt::Number,
            "LightningSpellTakenHitMult",
        ),
        computed(
            "ChaosSpellTakenHitMult",
            Cat::Mitigation,
            Vt::Number,
            "ChaosSpellTakenHitMult",
        ),
        computed(
            "PhysicalAfterReductionTakenHitMulti",
            Cat::Mitigation,
            Vt::Number,
            "PhysicalAfterReductionTakenHitMulti",
        ),
        computed(
            "FireAfterReductionTakenHitMulti",
            Cat::Mitigation,
            Vt::Number,
            "FireAfterReductionTakenHitMulti",
        ),
        computed(
            "ColdAfterReductionTakenHitMulti",
            Cat::Mitigation,
            Vt::Number,
            "ColdAfterReductionTakenHitMulti",
        ),
        computed(
            "LightningAfterReductionTakenHitMulti",
            Cat::Mitigation,
            Vt::Number,
            "LightningAfterReductionTakenHitMulti",
        ),
        computed(
            "ChaosAfterReductionTakenHitMulti",
            Cat::Mitigation,
            Vt::Number,
            "ChaosAfterReductionTakenHitMulti",
        ),
        computed(
            "PhysicalBaseTakenHitMult",
            Cat::Mitigation,
            Vt::Number,
            "PhysicalBaseTakenHitMult",
        ),
        computed(
            "FireBaseTakenHitMult",
            Cat::Mitigation,
            Vt::Number,
            "FireBaseTakenHitMult",
        ),
        computed(
            "ColdBaseTakenHitMult",
            Cat::Mitigation,
            Vt::Number,
            "ColdBaseTakenHitMult",
        ),
        computed(
            "LightningBaseTakenHitMult",
            Cat::Mitigation,
            Vt::Number,
            "LightningBaseTakenHitMult",
        ),
        computed(
            "ChaosBaseTakenHitMult",
            Cat::Mitigation,
            Vt::Number,
            "ChaosBaseTakenHitMult",
        ),
        computed(
            "PhysicalDamageReductionMax",
            Cat::Mitigation,
            Vt::Percent,
            "PhysicalDamageReductionMax",
        ),
        computed(
            "FireDamageReductionMax",
            Cat::Mitigation,
            Vt::Percent,
            "FireDamageReductionMax",
        ),
        computed(
            "ColdDamageReductionMax",
            Cat::Mitigation,
            Vt::Percent,
            "ColdDamageReductionMax",
        ),
        computed(
            "LightningDamageReductionMax",
            Cat::Mitigation,
            Vt::Percent,
            "LightningDamageReductionMax",
        ),
        computed(
            "ChaosDamageReductionMax",
            Cat::Mitigation,
            Vt::Percent,
            "ChaosDamageReductionMax",
        ),
        computed(
            "PhysicalManaEffectiveLife",
            Cat::Resource,
            Vt::Number,
            "PhysicalManaEffectiveLife",
        ),
        computed(
            "FireManaEffectiveLife",
            Cat::Resource,
            Vt::Number,
            "FireManaEffectiveLife",
        ),
        computed(
            "ColdManaEffectiveLife",
            Cat::Resource,
            Vt::Number,
            "ColdManaEffectiveLife",
        ),
        computed(
            "LightningManaEffectiveLife",
            Cat::Resource,
            Vt::Number,
            "LightningManaEffectiveLife",
        ),
        computed(
            "ChaosManaEffectiveLife",
            Cat::Resource,
            Vt::Number,
            "ChaosManaEffectiveLife",
        ),
        computed(
            "PhysicalMoMHitPool",
            Cat::Resource,
            Vt::Number,
            "PhysicalMoMHitPool",
        ),
        computed(
            "FireMoMHitPool",
            Cat::Resource,
            Vt::Number,
            "FireMoMHitPool",
        ),
        computed(
            "ColdMoMHitPool",
            Cat::Resource,
            Vt::Number,
            "ColdMoMHitPool",
        ),
        computed(
            "LightningMoMHitPool",
            Cat::Resource,
            Vt::Number,
            "LightningMoMHitPool",
        ),
        computed(
            "ChaosMoMHitPool",
            Cat::Resource,
            Vt::Number,
            "ChaosMoMHitPool",
        ),
        computed(
            "PhysicalTotalHitPool",
            Cat::Resource,
            Vt::Number,
            "PhysicalTotalHitPool",
        ),
        computed(
            "FireTotalHitPool",
            Cat::Resource,
            Vt::Number,
            "FireTotalHitPool",
        ),
        computed(
            "ColdTotalHitPool",
            Cat::Resource,
            Vt::Number,
            "ColdTotalHitPool",
        ),
        computed(
            "LightningTotalHitPool",
            Cat::Resource,
            Vt::Number,
            "LightningTotalHitPool",
        ),
        computed(
            "ChaosTotalHitPool",
            Cat::Resource,
            Vt::Number,
            "ChaosTotalHitPool",
        ),
        computed(
            "DamageReductionMax",
            Cat::Mitigation,
            Vt::Percent,
            "DamageReductionMax",
        ),
        computed("LifeHitPool", Cat::Resource, Vt::Number, "LifeHitPool"),
        computed(
            "FireResistTotal",
            Cat::Resistance,
            Vt::Percent,
            "FireResistTotal",
        ),
        computed(
            "ColdResistTotal",
            Cat::Resistance,
            Vt::Percent,
            "ColdResistTotal",
        ),
        computed(
            "LightningResistTotal",
            Cat::Resistance,
            Vt::Percent,
            "LightningResistTotal",
        ),
        computed(
            "ChaosResistTotal",
            Cat::Resistance,
            Vt::Percent,
            "ChaosResistTotal",
        ),
        computed(
            "FireResistOverTime",
            Cat::Resistance,
            Vt::Percent,
            "FireResistOverTime",
        ),
        computed(
            "ColdResistOverTime",
            Cat::Resistance,
            Vt::Percent,
            "ColdResistOverTime",
        ),
        computed(
            "LightningResistOverTime",
            Cat::Resistance,
            Vt::Percent,
            "LightningResistOverTime",
        ),
        computed(
            "ChaosResistOverTime",
            Cat::Resistance,
            Vt::Percent,
            "ChaosResistOverTime",
        ),
        computed(
            "PhysicalTakenDotMult",
            Cat::Mitigation,
            Vt::Number,
            "PhysicalTakenDotMult",
        ),
        computed(
            "FireTakenDotMult",
            Cat::Mitigation,
            Vt::Number,
            "FireTakenDotMult",
        ),
        computed(
            "ColdTakenDotMult",
            Cat::Mitigation,
            Vt::Number,
            "ColdTakenDotMult",
        ),
        computed(
            "LightningTakenDotMult",
            Cat::Mitigation,
            Vt::Number,
            "LightningTakenDotMult",
        ),
        computed(
            "ChaosTakenDotMult",
            Cat::Mitigation,
            Vt::Number,
            "ChaosTakenDotMult",
        ),
        computed(
            "PhysicalTotalPool",
            Cat::Resource,
            Vt::Number,
            "PhysicalTotalPool",
        ),
        computed("FireTotalPool", Cat::Resource, Vt::Number, "FireTotalPool"),
        computed("ColdTotalPool", Cat::Resource, Vt::Number, "ColdTotalPool"),
        computed(
            "LightningTotalPool",
            Cat::Resource,
            Vt::Number,
            "LightningTotalPool",
        ),
        computed(
            "ChaosTotalPool",
            Cat::Resource,
            Vt::Number,
            "ChaosTotalPool",
        ),
        computed(
            "PhysicalDotEHP",
            Cat::Mitigation,
            Vt::Number,
            "PhysicalDotEHP",
        ),
        computed("FireDotEHP", Cat::Mitigation, Vt::Number, "FireDotEHP"),
        computed("ColdDotEHP", Cat::Mitigation, Vt::Number, "ColdDotEHP"),
        computed(
            "LightningDotEHP",
            Cat::Mitigation,
            Vt::Number,
            "LightningDotEHP",
        ),
        computed("ChaosDotEHP", Cat::Mitigation, Vt::Number, "ChaosDotEHP"),
        computed(
            "EHPSurvivalTime",
            Cat::Mitigation,
            Vt::TimeSeconds,
            "EHPSurvivalTime",
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
        computed(
            "PreEffectiveCritMultiplier",
            Cat::Offence,
            Vt::Number,
            "PreEffectiveCritMultiplier",
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
        computed("CullMultiplier", Cat::Offence, Vt::Number, "CullMultiplier"),
        computed(
            "EnemyAilmentThreshold",
            Cat::Offence,
            Vt::Number,
            "EnemyAilmentThreshold",
        ),
        computed(
            "ReservationDpsMultiplier",
            Cat::Offence,
            Vt::Number,
            "ReservationDpsMultiplier",
        ),
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
                        .with_higher_is_better(
                            if matches!(
                                *field,
                                "ChargesUsed" | "ManaRemovedOnUse" | "LifeRemovedOnUse"
                            ) {
                                Some(false)
                            } else {
                                None
                            },
                        ),
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
    if let Some(value) = crate::calc::hit_display::display_value(output, id) {
        return value;
    }
    if let Some(value) = crate::calc::ailment_display::display_value(output, id) {
        return value;
    }
    if let Some(value) = crate::calc::limit_display::display_value(output, id) {
        return value;
    }
    if let Some((i, view)) = crate::calc::totem::resist_stat(id) {
        return match view {
            "final" => output.totem_resist[i],
            "total" => output.totem_resist_total[i],
            _ => output.missing_totem_resist[i],
        };
    }
    match id {
        "TotemLife" => output.totem_life,
        "TotemArmour" => output.totem_armour,
        "TotemEvasion" => output.totem_evasion,
        "TotemEnergyShield" => output.totem_energy_shield,
        "TotemDurationMod" => output.totem_duration_mod,
        "DecayDPS" => output.decay_dps,
        "MirageBurningGroundDPS" => output.mirage_burning_ground_dps,

        "ReservationDPS" => output.reservation_dps,
        "NetWardRegen" => output.net_ward_regen,
        "MirageDPS" => output.mirage_dps,
        "BurningGroundDPS" => output.burning_ground_dps,

        "EffectiveMovementSpeedMod" => output.effective_movement_speed_mod,
        "EffectiveLootRarityMod" => output.effective_loot_rarity_mod,
        "GemLevel" => output.gem_level,
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
        "AverageBurstHits" => output.average_burst_hits,
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
        "AccuracyHitChance" => output.accuracy_hit_chance,
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
        "PreEffectiveCritMultiplier" => output.pre_effective_crit_multiplier,
        "StoredUses" => f64::from(output.cooldown_stored_uses),
        "TotalDot" => output.skill_total_dot,
        "WithDotDPS" => output.with_dot_dps,
        "FiringRate" => output.firing_rate,
        "ReloadTime" => output.reload_time,
        "CullingDPS" => output.culling_dps,
        "CullMultiplier" => output.cull_multiplier,
        "EnemyAilmentThreshold" => output.enemy_ailment_threshold,
        "ReservationDpsMultiplier" => output.reservation_dps_multiplier,
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
        "EnemyCritChance" => output.enemy_crit_chance,
        "SecondMinimalMaximumHitTaken" => output.second_minimal_maximum_hit_taken,

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
        "MaxLifeLeechRate" => output.max_life_leech_rate,
        "MaxManaLeechRate" => output.max_mana_leech_rate,
        "MaxEnergyShieldLeechRate" => output.max_energy_shield_leech_rate,
        "MaxLifeLeechRatePercent" => output.max_life_leech_rate_percent,
        "LifeRecoupRate" => output.life_recoup_rate,
        "ManaRecoupRate" => output.mana_recoup_rate,
        "EsRecoupRate" => output.es_recoup_rate,

        // Ailment extensions
        "ChillEffect" => output.chill_effect,
        "ChillEffectMod" => output.chill_effect_mod,
        "ChillDuration" => output.chill_duration,
        "ShockDuration" => output.shock_duration,
        "FreezeBuildupAvg" => output.freeze_buildup_avg,
        "AilmentThreshold" => output.ailment_threshold,
        "LifeOnKill" => output.life_on_kill,
        "ManaOnKill" => output.mana_on_kill,
        "EnergyShieldOnKill" => output.energy_shield_on_kill,
        "DazeChance" => output.daze_chance,
        "HeavyStunBuildupAvg" => output.heavy_stun_buildup_avg,
        "PinBuildupAvg" => output.pin_buildup_avg,
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
        "DeflectEffect" => output.deflect_effect,
        "EvadeChance" => output.evade_chance,
        "MeleeEvadeChance" => output.melee_evade_chance,
        "ProjectileEvadeChance" => output.projectile_evade_chance,
        "SpellEvadeChance" => output.spell_evade_chance,
        "SpellProjectileEvadeChance" => output.spell_projectile_evade_chance,
        "ExposureEffectOnSelf" => output.exposure_effect_on_self,
        "WitherEffectOnSelf" => output.wither_effect_on_self,
        "SelfBleedEffect" => output.self_bleed_effect,
        "SelfIgniteEffect" => output.self_ignite_effect,
        "SelfChillEffect" => output.self_chill_effect,
        "SelfFreezeEffect" => output.self_freeze_effect,
        "SelfPoisonEffect" => output.self_poison_effect,
        "SelfBlindDuration" => output.self_blind_duration,
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
        "PhysicalEnemyDamageMult" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.enemy_damage_multipliers[0]),
        "FireEnemyDamageMult" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.enemy_damage_multipliers[1]),
        "ColdEnemyDamageMult" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.enemy_damage_multipliers[2]),
        "LightningEnemyDamageMult" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.enemy_damage_multipliers[3]),
        "ChaosEnemyDamageMult" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.enemy_damage_multipliers[4]),
        "PhysicalEnemyPen" => output.ehp_terms.as_ref().map_or(0.0, |t| t.enemy_pen[0]),
        "FireEnemyPen" => output.ehp_terms.as_ref().map_or(0.0, |t| t.enemy_pen[1]),
        "ColdEnemyPen" => output.ehp_terms.as_ref().map_or(0.0, |t| t.enemy_pen[2]),
        "LightningEnemyPen" => output.ehp_terms.as_ref().map_or(0.0, |t| t.enemy_pen[3]),
        "ChaosEnemyPen" => output.ehp_terms.as_ref().map_or(0.0, |t| t.enemy_pen[4]),
        "PhysicalResistTakenHitMulti" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.resist_taken_multi[0]),
        "FireResistTakenHitMulti" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.resist_taken_multi[1]),
        "ColdResistTakenHitMulti" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.resist_taken_multi[2]),
        "LightningResistTakenHitMulti" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.resist_taken_multi[3]),
        "ChaosResistTakenHitMulti" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.resist_taken_multi[4]),
        "PhysicalAttackTakenHitMult" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.mitigation.attack_taken_multi[0]),
        "FireAttackTakenHitMult" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.mitigation.attack_taken_multi[1]),
        "ColdAttackTakenHitMult" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.mitigation.attack_taken_multi[2]),
        "LightningAttackTakenHitMult" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.mitigation.attack_taken_multi[3]),
        "ChaosAttackTakenHitMult" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.mitigation.attack_taken_multi[4]),
        "PhysicalSpellTakenHitMult" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.mitigation.spell_taken_multi[0]),
        "FireSpellTakenHitMult" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.mitigation.spell_taken_multi[1]),
        "ColdSpellTakenHitMult" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.mitigation.spell_taken_multi[2]),
        "LightningSpellTakenHitMult" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.mitigation.spell_taken_multi[3]),
        "ChaosSpellTakenHitMult" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.mitigation.spell_taken_multi[4]),
        "PhysicalAfterReductionTakenHitMulti" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.after_reduction_multi[0]),
        "FireAfterReductionTakenHitMulti" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.after_reduction_multi[1]),
        "ColdAfterReductionTakenHitMulti" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.after_reduction_multi[2]),
        "LightningAfterReductionTakenHitMulti" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.after_reduction_multi[3]),
        "ChaosAfterReductionTakenHitMulti" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.after_reduction_multi[4]),
        "PhysicalBaseTakenHitMult" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.base_taken_hit_multiplier(0)),
        "FireBaseTakenHitMult" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.base_taken_hit_multiplier(1)),
        "ColdBaseTakenHitMult" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.base_taken_hit_multiplier(2)),
        "LightningBaseTakenHitMult" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.base_taken_hit_multiplier(3)),
        "ChaosBaseTakenHitMult" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.base_taken_hit_multiplier(4)),
        "PhysicalDamageReductionMax" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.mitigation.dr_max_pct[0]),
        "FireDamageReductionMax" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.mitigation.dr_max_pct[1]),
        "ColdDamageReductionMax" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.mitigation.dr_max_pct[2]),
        "LightningDamageReductionMax" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.mitigation.dr_max_pct[3]),
        "ChaosDamageReductionMax" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.mitigation.dr_max_pct[4]),
        "PhysicalManaEffectiveLife" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.mom.effective_life_by_type[0]),
        "FireManaEffectiveLife" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.mom.effective_life_by_type[1]),
        "ColdManaEffectiveLife" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.mom.effective_life_by_type[2]),
        "LightningManaEffectiveLife" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.mom.effective_life_by_type[3]),
        "ChaosManaEffectiveLife" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.mom.effective_life_by_type[4]),
        "PhysicalMoMHitPool" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.mom.hit_pool_by_type[0]),
        "FireMoMHitPool" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.mom.hit_pool_by_type[1]),
        "ColdMoMHitPool" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.mom.hit_pool_by_type[2]),
        "LightningMoMHitPool" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.mom.hit_pool_by_type[3]),
        "ChaosMoMHitPool" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.mom.hit_pool_by_type[4]),
        "PhysicalTotalHitPool" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.total_hit_pool[0]),
        "FireTotalHitPool" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.total_hit_pool[1]),
        "ColdTotalHitPool" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.total_hit_pool[2]),
        "LightningTotalHitPool" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.total_hit_pool[3]),
        "ChaosTotalHitPool" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.total_hit_pool[4]),
        "DamageReductionMax" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.mitigation.dr_max_global),
        "LifeHitPool" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.mom.life_hit_pool),
        "FireResistTotal" => output.fire_resistance_total,
        "ColdResistTotal" => output.cold_resistance_total,
        "LightningResistTotal" => output.lightning_resistance_total,
        "ChaosResistTotal" => output.chaos_resistance_total,
        "FireResistOverTime" => output.resistance_over_time[0],
        "ColdResistOverTime" => output.resistance_over_time[1],
        "LightningResistOverTime" => output.resistance_over_time[2],
        "ChaosResistOverTime" => output.resistance_over_time[3],
        "PhysicalTakenDotMult" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.dot_taken[0].multiplier()),
        "FireTakenDotMult" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.dot_taken[1].multiplier()),
        "ColdTakenDotMult" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.dot_taken[2].multiplier()),
        "LightningTakenDotMult" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.dot_taken[3].multiplier()),
        "ChaosTakenDotMult" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.dot_taken[4].multiplier()),
        "PhysicalTotalPool" => output.ehp_terms.as_ref().map_or(0.0, |t| t.total_pool[0]),
        "FireTotalPool" => output.ehp_terms.as_ref().map_or(0.0, |t| t.total_pool[1]),
        "ColdTotalPool" => output.ehp_terms.as_ref().map_or(0.0, |t| t.total_pool[2]),
        "LightningTotalPool" => output.ehp_terms.as_ref().map_or(0.0, |t| t.total_pool[3]),
        "ChaosTotalPool" => output.ehp_terms.as_ref().map_or(0.0, |t| t.total_pool[4]),
        "PhysicalDotEHP" => output.ehp_terms.as_ref().map_or(0.0, |t| t.dot_ehp(0)),
        "FireDotEHP" => output.ehp_terms.as_ref().map_or(0.0, |t| t.dot_ehp(1)),
        "ColdDotEHP" => output.ehp_terms.as_ref().map_or(0.0, |t| t.dot_ehp(2)),
        "LightningDotEHP" => output.ehp_terms.as_ref().map_or(0.0, |t| t.dot_ehp(3)),
        "ChaosDotEHP" => output.ehp_terms.as_ref().map_or(0.0, |t| t.dot_ehp(4)),
        "EHPSurvivalTime" => output
            .ehp_terms
            .as_ref()
            .map_or(0.0, |t| t.survival_time(output.total_number_of_hits)),

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
