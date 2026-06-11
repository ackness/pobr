use pobr_core::calc::OutputTable;
use pobr_core::{display_catalog, extract_display_values};
use pobr_data::prelude::*;

#[test]
fn catalog_defines_core_display_stats() {
    let catalog = display_catalog();
    let ids: Vec<&str> = catalog.iter().map(|def| def.id.as_str()).collect();

    assert!(ids.contains(&"TotalDPS"));
    assert!(ids.contains(&"TotalEHP"));
    assert!(ids.contains(&"BleedDPS"));
    assert!(ids.contains(&"BlockChance"));
}

/// 条目状态与 pob_key 完整性：M2-W0.2 起目录含 Computed（已接线）+ Planned（M2 防御
/// 扩展，待 track 接线）两类；两类都必须带 pob_key（golden 对照键）。
#[test]
fn catalog_entries_have_pob_keys_and_known_status() {
    let catalog = display_catalog();
    for def in &catalog {
        assert!(
            matches!(
                def.parity_status,
                ParityStatus::Computed | ParityStatus::Planned
            ),
            "{} 状态异常",
            def.id.as_str()
        );
        assert!(def.pob_key.is_some(), "{} missing pob_key", def.id.as_str());
    }
}

/// 条目数锁定（M2-W0.2 / F-3 更新）：M2 防御扩展批 24 个字段已于 F-3 全部翻
/// Computed → Planned = 0、Computed = 105；新增条目须显式更新本断言。
#[test]
fn catalog_entry_counts_locked() {
    let catalog = display_catalog();
    let planned = catalog
        .iter()
        .filter(|d| d.parity_status == ParityStatus::Planned)
        .count();
    assert_eq!(
        planned, 0,
        "Planned 条目数应为 0（M2 防御扩展批已全部接线）"
    );
    assert_eq!(
        catalog.len() - planned,
        105,
        "Computed 条目数变化须显式审查"
    );
}

/// M2 防御扩展字段批：F-3 后全部为 Computed，golden key 与 meta.json 对齐。
#[test]
fn catalog_defines_m2_defence_extension_stats_computed() {
    let catalog = display_catalog();
    let computed_ids: Vec<&str> = catalog
        .iter()
        .filter(|d| d.parity_status == ParityStatus::Computed)
        .map(|d| d.id.as_str())
        .collect();
    for id in [
        "Spirit",
        "SpiritUnreserved",
        "BlockChanceMax",
        "SpellBlockChanceMax",
        "EffectiveBlockChance",
        "EffectiveSpellBlockChance",
        "BlockEffect",
        "DeflectionRating",
        "DeflectChance",
        "EvadeChance",
        "MeleeEvadeChance",
        "ProjectileEvadeChance",
        "SpellEvadeChance",
        "SpellProjectileEvadeChance",
        "StunThreshold",
        "SelfStunChance",
        "StunDuration",
        "Ward",
        "LifeRecoverable",
        "EnergyShieldRecoveryCap",
        "PhysicalDamageReduction",
        "NumberOfDamagingHits",
        "NumberOfMitigatedHits",
        "TotalEHPLowestMaxHit",
    ] {
        assert!(computed_ids.contains(&id), "missing computed entry {id}");
    }
    // PoB2 golden key 对齐抽查（NumberOfMitigatedHits → NumberOfMitigatedDamagingHits）。
    let entry = |id: &str| {
        catalog
            .iter()
            .find(|d| d.id.as_str() == id)
            .unwrap_or_else(|| panic!("{id} not found"))
    };
    assert_eq!(
        entry("NumberOfMitigatedHits").pob_key.as_deref(),
        Some("NumberOfMitigatedDamagingHits")
    );
    assert_eq!(entry("Spirit").pob_key.as_deref(), Some("Spirit"));
    // 承伤/受眩晕类：越低越好。
    assert_eq!(entry("BlockEffect").higher_is_better, Some(false));
    assert_eq!(entry("SelfStunChance").higher_is_better, Some(false));
    assert_eq!(entry("StunDuration").higher_is_better, Some(false));
}

/// M2 防御扩展条目 F-3 翻 Computed 后进 extract（对外可见，默认 0 中性值）。
#[test]
fn m2_defence_extension_entries_included_in_extract() {
    let values = extract_display_values(&OutputTable::default());
    for id in ["Spirit", "EffectiveBlockChance", "NumberOfDamagingHits"] {
        assert!(
            values.iter().any(|v| v.id == DisplayStatId::from(id)),
            "Computed 条目 {id} 应出现在 extract 输出"
        );
    }
}

#[test]
fn extract_display_values_reads_output_table_fields() {
    let output = OutputTable {
        dps: 12345.0,
        total_ehp: 9000.0,
        bleed_dps: 250.0,
        block_chance: 35.0,
        ..OutputTable::default()
    };

    let values = extract_display_values(&output);
    let lookup = |id: &str| {
        values
            .iter()
            .find(|value| value.id == DisplayStatId::from(id))
            .map(|value| value.value)
    };

    assert_eq!(lookup("TotalDPS"), Some(12345.0));
    assert_eq!(lookup("TotalEHP"), Some(9000.0));
    assert_eq!(lookup("BleedDPS"), Some(250.0));
    assert_eq!(lookup("BlockChance"), Some(35.0));
}

#[test]
fn extract_covers_every_computed_catalog_entry() {
    let computed = display_catalog()
        .iter()
        .filter(|def| def.parity_status == ParityStatus::Computed)
        .count();
    let values = extract_display_values(&OutputTable::default());
    assert_eq!(values.len(), computed);
}

// ---------------------------------------------------------------------------
// Wave 2 field coverage tests
// ---------------------------------------------------------------------------

/// Verify all Wave 2 defense extension fields land in the catalog.
#[test]
fn catalog_defines_wave2_defence_extension_stats() {
    let catalog = display_catalog();
    let ids: Vec<&str> = catalog.iter().map(|def| def.id.as_str()).collect();

    // ES recharge
    assert!(ids.contains(&"EsRechargeRate"), "missing EsRechargeRate");
    assert!(ids.contains(&"EsRechargeDelay"), "missing EsRechargeDelay");
    assert!(
        ids.contains(&"EsRechargePerSecond"),
        "missing EsRechargePerSecond"
    );

    // Avoidance
    assert!(
        ids.contains(&"AvoidAllDamageFromHits"),
        "missing AvoidAllDamageFromHits"
    );
    assert!(
        ids.contains(&"AvoidProjectileDamage"),
        "missing AvoidProjectileDamage"
    );
    assert!(ids.contains(&"AvoidStun"), "missing AvoidStun");
    assert!(ids.contains(&"AvoidIgnite"), "missing AvoidIgnite");
    assert!(ids.contains(&"AvoidShock"), "missing AvoidShock");
    assert!(ids.contains(&"AvoidChill"), "missing AvoidChill");
    assert!(ids.contains(&"AvoidFreeze"), "missing AvoidFreeze");
    assert!(ids.contains(&"AvoidPoison"), "missing AvoidPoison");
    assert!(ids.contains(&"AvoidBleeding"), "missing AvoidBleeding");

    // Taken multipliers
    assert!(
        ids.contains(&"TakenMultiPhysical"),
        "missing TakenMultiPhysical"
    );
    assert!(ids.contains(&"TakenMultiFire"), "missing TakenMultiFire");
    assert!(ids.contains(&"TakenMultiCold"), "missing TakenMultiCold");
    assert!(
        ids.contains(&"TakenMultiLightning"),
        "missing TakenMultiLightning"
    );
    assert!(ids.contains(&"TakenMultiChaos"), "missing TakenMultiChaos");
    assert!(
        ids.contains(&"CritExtraDamageReduction"),
        "missing CritExtraDamageReduction"
    );
    assert!(ids.contains(&"EnemyCritEffect"), "missing EnemyCritEffect");
}

/// Verify all Wave 2 charge / leech / recoup fields land in the catalog.
#[test]
fn catalog_defines_wave2_charges_leech_recoup_stats() {
    let catalog = display_catalog();
    let ids: Vec<&str> = catalog.iter().map(|def| def.id.as_str()).collect();

    // Charges
    assert!(
        ids.contains(&"ChargePowerCurrent"),
        "missing ChargePowerCurrent"
    );
    assert!(
        ids.contains(&"ChargePowerMaximum"),
        "missing ChargePowerMaximum"
    );
    assert!(
        ids.contains(&"ChargeFrenzyCurrent"),
        "missing ChargeFrenzyCurrent"
    );
    assert!(
        ids.contains(&"ChargeFrenzyMaximum"),
        "missing ChargeFrenzyMaximum"
    );
    assert!(
        ids.contains(&"ChargeEnduranceCurrent"),
        "missing ChargeEnduranceCurrent"
    );
    assert!(
        ids.contains(&"ChargeEnduranceMaximum"),
        "missing ChargeEnduranceMaximum"
    );

    // Leech
    assert!(ids.contains(&"LifeLeechRate"), "missing LifeLeechRate");
    assert!(ids.contains(&"ManaLeechRate"), "missing ManaLeechRate");
    assert!(ids.contains(&"EsLeechRate"), "missing EsLeechRate");

    // Recoup
    assert!(ids.contains(&"LifeRecoupRate"), "missing LifeRecoupRate");
    assert!(ids.contains(&"EsRecoupRate"), "missing EsRecoupRate");
}

/// Verify all Wave 2 ailment extension fields land in the catalog.
#[test]
fn catalog_defines_wave2_ailment_extension_stats() {
    let catalog = display_catalog();
    let ids: Vec<&str> = catalog.iter().map(|def| def.id.as_str()).collect();

    assert!(ids.contains(&"ChillEffect"), "missing ChillEffect");
    assert!(
        ids.contains(&"FreezeBuildupPct"),
        "missing FreezeBuildupPct"
    );
    assert!(
        ids.contains(&"ElectrocuteBuildupPct"),
        "missing ElectrocuteBuildupPct"
    );
    assert!(ids.contains(&"BleedStackedDPS"), "missing BleedStackedDPS");
    assert!(
        ids.contains(&"BleedActiveStacks"),
        "missing BleedActiveStacks"
    );
    assert!(
        ids.contains(&"PoisonStackedDPS"),
        "missing PoisonStackedDPS"
    );
    assert!(
        ids.contains(&"PoisonActiveStacks"),
        "missing PoisonActiveStacks"
    );
}

/// Verify all Wave 2 skill mechanics / trigger / cost fields land in the catalog.
#[test]
fn catalog_defines_wave2_skill_mechanics_stats() {
    let catalog = display_catalog();
    let ids: Vec<&str> = catalog.iter().map(|def| def.id.as_str()).collect();

    assert!(ids.contains(&"AoeRadius"), "missing AoeRadius");
    assert!(ids.contains(&"AoeAreaMod"), "missing AoeAreaMod");
    assert!(ids.contains(&"ProjectileCount"), "missing ProjectileCount");
    assert!(ids.contains(&"Cooldown"), "missing Cooldown");
    assert!(
        ids.contains(&"CooldownStoredUses"),
        "missing CooldownStoredUses"
    );
    assert!(ids.contains(&"ManaCost"), "missing ManaCost");
    assert!(ids.contains(&"LifeCost"), "missing LifeCost");
    assert!(ids.contains(&"SpiritReserved"), "missing SpiritReserved");
    assert!(ids.contains(&"TriggerRateCap"), "missing TriggerRateCap");
    assert!(
        ids.contains(&"SkillTriggerRate"),
        "missing SkillTriggerRate"
    );
}

/// Verify Wave 2 OutputTable field extraction round-trips correctly.
#[test]
fn extract_wave2_defence_values_from_output_table() {
    let output = OutputTable {
        // ES recharge
        es_recharge_rate: 0.125,
        es_recharge_delay: 4.0,
        es_recharge_per_second: 250.0,
        // Avoidance
        avoid_all_damage_from_hits: 10.0,
        avoid_stun: 20.0,
        avoid_ignite: 30.0,
        avoid_shock: 40.0,
        // Taken
        taken_multi_fire: 1.2,
        crit_extra_damage_reduction: 15.0,
        enemy_crit_effect: 1.5,
        ..OutputTable::default()
    };

    let values = extract_display_values(&output);
    let lookup = |id: &str| {
        values
            .iter()
            .find(|v| v.id == DisplayStatId::from(id))
            .map(|v| v.value)
    };

    assert_eq!(lookup("EsRechargeRate"), Some(0.125));
    assert_eq!(lookup("EsRechargeDelay"), Some(4.0));
    assert_eq!(lookup("EsRechargePerSecond"), Some(250.0));
    assert_eq!(lookup("AvoidAllDamageFromHits"), Some(10.0));
    assert_eq!(lookup("AvoidStun"), Some(20.0));
    assert_eq!(lookup("AvoidIgnite"), Some(30.0));
    assert_eq!(lookup("AvoidShock"), Some(40.0));
    assert_eq!(lookup("TakenMultiFire"), Some(1.2));
    assert_eq!(lookup("CritExtraDamageReduction"), Some(15.0));
    assert_eq!(lookup("EnemyCritEffect"), Some(1.5));
}

/// Verify Wave 2 charge / leech / recoup round-trip.
#[test]
fn extract_wave2_charges_leech_recoup_from_output_table() {
    let output = OutputTable {
        charge_power_current: 3,
        charge_power_maximum: 3,
        charge_frenzy_current: 2,
        charge_frenzy_maximum: 4,
        charge_endurance_current: 1,
        charge_endurance_maximum: 3,
        life_leech_rate: 500.0,
        mana_leech_rate: 100.0,
        es_leech_rate: 200.0,
        life_recoup_rate: 50.0,
        es_recoup_rate: 30.0,
        ..OutputTable::default()
    };

    let values = extract_display_values(&output);
    let lookup = |id: &str| {
        values
            .iter()
            .find(|v| v.id == DisplayStatId::from(id))
            .map(|v| v.value)
    };

    assert_eq!(lookup("ChargePowerCurrent"), Some(3.0));
    assert_eq!(lookup("ChargePowerMaximum"), Some(3.0));
    assert_eq!(lookup("ChargeFrenzyCurrent"), Some(2.0));
    assert_eq!(lookup("ChargeFrenzyMaximum"), Some(4.0));
    assert_eq!(lookup("ChargeEnduranceCurrent"), Some(1.0));
    assert_eq!(lookup("ChargeEnduranceMaximum"), Some(3.0));
    assert_eq!(lookup("LifeLeechRate"), Some(500.0));
    assert_eq!(lookup("ManaLeechRate"), Some(100.0));
    assert_eq!(lookup("EsLeechRate"), Some(200.0));
    assert_eq!(lookup("LifeRecoupRate"), Some(50.0));
    assert_eq!(lookup("EsRecoupRate"), Some(30.0));
}

/// Verify Wave 2 ailment extension round-trip.
#[test]
fn extract_wave2_ailment_values_from_output_table() {
    let output = OutputTable {
        chill_effect: 30.0,
        freeze_buildup_pct: 15.0,
        electrocute_buildup_pct: 20.0,
        bleed_stacked_dps: 800.0,
        bleed_active_stacks: 5.0,
        poison_stacked_dps: 1200.0,
        poison_active_stacks: 8.0,
        ..OutputTable::default()
    };

    let values = extract_display_values(&output);
    let lookup = |id: &str| {
        values
            .iter()
            .find(|v| v.id == DisplayStatId::from(id))
            .map(|v| v.value)
    };

    assert_eq!(lookup("ChillEffect"), Some(30.0));
    assert_eq!(lookup("FreezeBuildupPct"), Some(15.0));
    assert_eq!(lookup("ElectrocuteBuildupPct"), Some(20.0));
    assert_eq!(lookup("BleedStackedDPS"), Some(800.0));
    assert_eq!(lookup("BleedActiveStacks"), Some(5.0));
    assert_eq!(lookup("PoisonStackedDPS"), Some(1200.0));
    assert_eq!(lookup("PoisonActiveStacks"), Some(8.0));
}

/// Verify Wave 2 skill mechanics / trigger / cost round-trip.
#[test]
fn extract_wave2_skill_mechanics_values_from_output_table() {
    let output = OutputTable {
        aoe_radius: 15.0,
        aoe_area_mod: 1.4,
        projectile_count: 3.0,
        cooldown: 0.5,
        cooldown_stored_uses: 2,
        mana_cost: 45.0,
        life_cost: 10.0,
        spirit_reserved: 30.0,
        trigger_rate_cap: 4.0,
        skill_trigger_rate: 3.5,
        ..OutputTable::default()
    };

    let values = extract_display_values(&output);
    let lookup = |id: &str| {
        values
            .iter()
            .find(|v| v.id == DisplayStatId::from(id))
            .map(|v| v.value)
    };

    assert_eq!(lookup("AoeRadius"), Some(15.0));
    assert_eq!(lookup("AoeAreaMod"), Some(1.4));
    assert_eq!(lookup("ProjectileCount"), Some(3.0));
    assert_eq!(lookup("Cooldown"), Some(0.5));
    assert_eq!(lookup("CooldownStoredUses"), Some(2.0));
    assert_eq!(lookup("ManaCost"), Some(45.0));
    assert_eq!(lookup("LifeCost"), Some(10.0));
    assert_eq!(lookup("SpiritReserved"), Some(30.0));
    assert_eq!(lookup("TriggerRateCap"), Some(4.0));
    assert_eq!(lookup("SkillTriggerRate"), Some(3.5));
}

/// Verify taken-multiplier higher_is_better flags are correctly set to false.
#[test]
fn taken_multi_and_cost_entries_have_higher_is_better_false() {
    let catalog = display_catalog();
    let entry = |id: &str| {
        catalog
            .iter()
            .find(|def| def.id.as_str() == id)
            .unwrap_or_else(|| panic!("{id} not found in catalog"))
    };

    // Taken multipliers: more damage taken → worse.
    assert_eq!(
        entry("TakenMultiPhysical").higher_is_better,
        Some(false),
        "TakenMultiPhysical should have higher_is_better=false"
    );
    assert_eq!(
        entry("TakenMultiFire").higher_is_better,
        Some(false),
        "TakenMultiFire should have higher_is_better=false"
    );
    // Cost fields: lower cost → better.
    assert_eq!(
        entry("ManaCost").higher_is_better,
        Some(false),
        "ManaCost should have higher_is_better=false"
    );
    assert_eq!(
        entry("Cooldown").higher_is_better,
        Some(false),
        "Cooldown should have higher_is_better=false"
    );
    // EsRechargeDelay: shorter delay → better.
    assert_eq!(
        entry("EsRechargeDelay").higher_is_better,
        Some(false),
        "EsRechargeDelay should have higher_is_better=false"
    );
}
