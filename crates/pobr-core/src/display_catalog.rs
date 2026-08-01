//! The display stat catalog.
//!
//! Maps calculation results ([`OutputTable`](crate::calc::OutputTable)) to the
//! stable [`DisplayStatDefinition`] / [`DisplayStatValue`] (pobr-data
//! contract) consumed by upstream UI / parity checks. Calculations use only
//! stable IDs internally; display text goes through i18n (not yet
//! implemented).
//!
//! - [`display_catalog`]: statically declares every display field (id /
//!   category / value type / higher-is-better / PoB key). Computed fields are
//!   marked `Computed`, not-yet-landed ones `Planned` (the defence extension
//!   batch was fully flipped to `Computed` in F-3).
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
    // Defence extensions (entered the catalog as Planned in W0.2): once the
    // C/D/E/F tracks finish wiring up, F-3 (the sole exception to the
    // output.rs/display_catalog freeze) flips them all to Computed at once —
    // all 24 extension fields are produced by the perform fill stage.

    vec![
        // Offence
        computed("TotalDPS", Cat::Offence, Vt::Number, "TotalDPS"),
        computed("TotalHitAvg", Cat::HitDamage, Vt::Number, "AverageHit"),
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
        computed("BleedDPS", Cat::DotDamage, Vt::Number, "BleedDPS"),
        computed("IgniteDPS", Cat::DotDamage, Vt::Number, "IgniteDPS"),
        computed("PoisonDPS", Cat::DotDamage, Vt::Number, "PoisonDPS"),
        computed("ShockEffect", Cat::Ailment, Vt::Percent, "ShockEffectMod"),
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
            Vt::Percent,
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
        // The share of damage still taken on a blocked hit: lower is better.
        computed("BlockEffect", Cat::Mitigation, Vt::Percent, "BlockEffect")
            .with_higher_is_better(Some(false)),
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
        // The old lowest-max-hit semantics are kept as a supplementary metric
        // (still comparable after F switches total_ehp's semantics).
        computed(
            "TotalEHPLowestMaxHit",
            Cat::Mitigation,
            Vt::Number,
            "TotalEHPLowestMaxHit",
        ),
    ]
}

/// Extracts the current value of every `Computed` display field from an
/// `OutputTable`, in the same order as [`display_catalog`].
pub fn extract_display_values(output: &OutputTable) -> Vec<DisplayStatValue> {
    display_catalog()
        .into_iter()
        .filter(|def| def.parity_status == ParityStatus::Computed)
        .map(|def| {
            let value = output_value_for(output, def.id.as_str());
            DisplayStatValue {
                id: def.id,
                value,
                category: def.category,
            }
        })
        .collect()
}

/// Maps a display field id to its `OutputTable` field value. Unknown ids return 0.
fn output_value_for(output: &OutputTable, id: &str) -> f64 {
    match id {
        "TotalDPS" => output.dps,
        "TotalHitAvg" => output.total_hit_avg,
        // hit_chance / crit_chance are fractions (0..1) on the calc side, but
        // the display contract's Vt::Percent expects 0..100 (matching every
        // other Percent field), so we convert here at extraction time.
        "HitChance" => output.hit_chance * 100.0,
        "ActionRate" => output.action_rate,
        "EffectiveActionRate" => output.effective_action_rate,
        "CritChance" => output.crit_chance * 100.0,
        "CritMultiplier" => output.crit_multiplier,
        "BleedDPS" => output.bleed_dps,
        "IgniteDPS" => output.ignite_dps,
        "PoisonDPS" => output.poison_dps,
        "ShockEffect" => output.shock_effect,
        "Life" => output.life,
        "Mana" => output.mana,
        "EnergyShield" => output.energy_shield,
        "LifeReserved" => output.life_reserved,
        "LifeUnreserved" => output.life_unreserved,
        "ManaReserved" => output.mana_reserved,
        "ManaUnreserved" => output.mana_unreserved,
        "LifeRegen" => output.life_regen,
        "ManaRegen" => output.mana_regen,
        "EnergyShieldRegen" => output.energy_shield_regen,
        "Armour" => output.armour,
        "Evasion" => output.evasion,
        "FireResist" => output.fire_resistance,
        "ColdResist" => output.cold_resistance,
        "LightningResist" => output.lightning_resistance,
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
        "ChargePowerCurrent" => output.charge_power_current as f64,
        "ChargePowerMaximum" => output.charge_power_maximum as f64,
        "ChargeFrenzyCurrent" => output.charge_frenzy_current as f64,
        "ChargeFrenzyMaximum" => output.charge_frenzy_maximum as f64,
        "ChargeEnduranceCurrent" => output.charge_endurance_current as f64,
        "ChargeEnduranceMaximum" => output.charge_endurance_maximum as f64,

        // Leech / Recoup
        "LifeLeechRate" => output.life_leech_rate,
        "ManaLeechRate" => output.mana_leech_rate,
        "EsLeechRate" => output.es_leech_rate,
        "LifeRecoupRate" => output.life_recoup_rate,
        "EsRecoupRate" => output.es_recoup_rate,

        // Ailment extensions
        "ChillEffect" => output.chill_effect,
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
        "BlockEffect" => output.block_effect,
        "DeflectionRating" => output.deflection_rating,
        "DeflectChance" => output.deflect_chance,
        "EvadeChance" => output.evade_chance,
        "MeleeEvadeChance" => output.melee_evade_chance,
        "ProjectileEvadeChance" => output.projectile_evade_chance,
        "SpellEvadeChance" => output.spell_evade_chance,
        "SpellProjectileEvadeChance" => output.spell_projectile_evade_chance,
        "StunThreshold" => output.stun_threshold,
        "SelfStunChance" => output.self_stun_chance,
        "StunDuration" => output.stun_duration,
        "Ward" => output.ward,
        "LifeRecoverable" => output.life_recoverable,
        "EnergyShieldRecoveryCap" => output.energy_shield_recovery_cap,
        "PhysicalDamageReduction" => output.physical_damage_reduction,
        "NumberOfDamagingHits" => output.number_of_damaging_hits,
        "NumberOfMitigatedHits" => output.number_of_mitigated_hits,
        "TotalEHPLowestMaxHit" => output.total_ehp_lowest_max_hit,

        _ => 0.0,
    }
}
