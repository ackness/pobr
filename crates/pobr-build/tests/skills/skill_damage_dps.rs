//! End-to-end verification of the gem data channel, Phase 3: per-level skill
//! **base damage** -> DPS.
//!
//! Uses real ingested data (`data/4.5.0.3.4/granted_effect_stat_sets.json`) to
//! verify the whole channel: `<Gem skillId>` + level -> stat-set base damage ->
//! `<Type>DamageMin/Max` BASE mod -> damage component -> DPS. Reference skill
//! is Fireball (a pure spell whose base damage comes only from the stat-set,
//! independent of the not-yet-wired weapon damage); its L20 base fire damage
//! is 224-336 (matches PoB's own `Data/Skills/act_int.lua` parse
//! value-for-value), average hit 280.

use pobr_build::{
    Build, BuildData, CharacterIdentity, DataOrchestratorOptions, SocketGroup, calculate_with_data,
};
use pobr_core::calc::MinimalInput;
use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};
use pobr_data::monster::EnemyTier;
use pobr_gamedata::{GameData, repo_data_root};

fn load_build_data() -> BuildData {
    let data = GameData::new(repo_data_root().join(pobr_gamedata::data_version()));
    BuildData::load(&data).expect("load BuildData from repo data")
}

fn fireball_build(gem_level: u32) -> Build {
    Build::new()
        .with_character(CharacterIdentity {
            level: 90,
            class_name: "Sorceress".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(
            SocketGroup::new()
                .with_slot("weapon1")
                .with_gem("Metadata/Items/Gems/Fireball")
                .with_active_skill("FireballPlayer", gem_level),
        )
}

fn panel_opts() -> DataOrchestratorOptions {
    DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        enemy_level: 0,
        enemy_tier: EnemyTier::None,
        mode_effective: false,
        extra_modifier_texts: vec![],
        ..Default::default()
    }
}

/// Fireball L20: base fire damage 224-336 (avg 280), DPS = avg × action rate × hit chance > 0.
#[test]
fn fireball_base_damage_drives_nonzero_dps() {
    let build_data = load_build_data();

    // Precondition: the stat-set domain actually loaded Fireball's per-level damage (the data channel isn't broken).
    let resolved = build_data
        .resolve_skill_level("FireballPlayer", 20)
        .expect("FireballPlayer should resolve");
    assert!(
        resolved
            .base_damage
            .iter()
            .any(|d| d.stat == "spell_minimum_base_fire_damage" && d.value == 224.0),
        "expected L20 spell_minimum_base_fire_damage = 224, got {:?}",
        resolved.base_damage
    );

    let build = fireball_build(20);
    let out = calculate_with_data(&build, &build_data, &panel_opts())
        .expect("calculate_with_data should succeed for Fireball build");

    // No increased/more sources -> non-crit average hit = (224 + 336) / 2 = 280.
    let non_crit: f64 = out.damage_components.iter().map(|c| c.avg()).sum();
    assert!(
        (non_crit - 280.0).abs() < 1.0,
        "Fireball L20 non-crit hit should be ~280, got {non_crit}"
    );
    // total_hit_avg includes Fireball's 7% base crit amplification (PoB2 AverageDamage is crit-weighted) -> slightly above 280.
    assert!(
        out.total_hit_avg > 280.0,
        "Fireball average hit should include base crit, got {}",
        out.total_hit_avg
    );
    // Action rate comes from cast time 1.2s -> 1/1.2 ≈ 0.833; hit chance > 0 -> DPS > 0.
    assert!(
        out.dps > 0.0,
        "Fireball DPS should be > 0 once base damage is injected, got {}",
        out.dps
    );
    assert!(out.action_rate > 0.0, "action_rate should be > 0");
}

/// Support gem injection: a same-group **compatible** "increased damage"
/// support gem's `damage_+%` should boost hit.
///
/// Injection is gated by group-level applicability (PoB2 CalcTools.lua:84-110
/// + CalcActiveSkill.lua:179-210): this test originally used
/// `SupportFerociousRoarPlayer` (requires `[Warcry]` — in PoB2, Ferocious Roar
/// can only support Warcry skills), which was a wrong injection for Fireball
/// (a spell) and is now correctly rejected by gating (see the rejection
/// assertion in tests/support_gating.rs). Switched to
/// `SupportMetaCastFireSpellOnHitPlayer` (requires `[Spell, Triggerable, Fire,
/// AND, AND]`, all satisfied by Fireball) to verify the INC Damage injection
/// channel for a compatible support.
#[test]
fn fireball_with_damage_support_raises_hit() {
    let build_data = load_build_data();

    // This support does have a mappable damage_+% stat (the data channel isn't broken). The support
    // has no quality-table entry (skipped by PoB2's export), so pass quality=0 to get the base segment.
    let sup = build_data.effect_stats("SupportMetaCastFireSpellOnHitPlayer", 20, 0, None);
    let inc = sup
        .base
        .iter()
        .find(|s| s.stat == "damage_+%")
        .expect("support should carry damage_+%");
    assert!(inc.value > 0.0);

    let base = calculate_with_data(&fireball_build(20), &build_data, &panel_opts())
        .expect("no-support calc");

    // Adding a compatible support with damage_+% to the same group -> injects a Damage INC.
    let with_support = Build::new()
        .with_character(CharacterIdentity {
            level: 90,
            class_name: "Sorceress".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(
            SocketGroup::new()
                .with_slot("weapon1")
                .with_gem("Metadata/Items/Gems/Fireball")
                .with_active_skill("FireballPlayer", 20)
                .with_gem_skill("FireballPlayer", 20)
                .with_gem_skill("SupportMetaCastFireSpellOnHitPlayer", 20),
        );
    let boosted =
        calculate_with_data(&with_support, &build_data, &panel_opts()).expect("with-support calc");

    // damage_+% is INC: hit = base × (1 + inc/100). L20 inc=200 -> ×3.
    let expected = base.total_hit_avg * (1.0 + inc.value / 100.0);
    assert!(
        (boosted.total_hit_avg - expected).abs() < 1.0,
        "support damage_+% ({}) should scale hit: base {} → {} (got {})",
        inc.value,
        base.total_hit_avg,
        expected,
        boosted.total_hit_avg
    );
    assert!(boosted.total_hit_avg > base.total_hit_avg * 1.5);
}

/// A bare base weapon (no mods), used to verify weapon-base damage assembly.
fn bare_weapon(base_name: &str) -> Item {
    Item {
        base: ItemBaseId::from(base_name),
        rarity: ItemRarity::Normal,
        quality: 0,
        corrupted: false,
        implicit_texts: vec![],
        modifier_texts: vec![],
        enchant_texts: vec![],
        rolled_defence: RolledDefence::default(),
        parsed_stats: vec![],
    }
}

/// Weapon-base damage assembly (roadmap chain A #1): once an attack skill has
/// a weapon equipped, the weapon's base physical damage feeds into hit.
/// `Crude Claw` base 4-10 physical -> average 7 injected into base_hit.
#[test]
fn attack_skill_uses_weapon_base_damage() {
    let build_data = load_build_data();
    let opts = panel_opts();

    let attack_build = |with_weapon: bool| {
        let mut b = Build::new()
            .with_character(CharacterIdentity {
                level: 50,
                class_name: "Warrior".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(
                SocketGroup::new()
                    .with_slot("weapon1")
                    .with_active_skill("AxeChopPlayer", 1),
            );
        if with_weapon {
            b = b.set_item(EquipmentSlot::Weapon1, bare_weapon("Crude Claw"));
        }
        b
    };

    let no_weapon =
        calculate_with_data(&attack_build(false), &build_data, &opts).expect("no weapon");
    let with_weapon =
        calculate_with_data(&attack_build(true), &build_data, &opts).expect("with weapon");

    // Unarmed uses the unarmed base (Warrior physical 2-8, avg 5) -> non-zero; equipping a weapon
    // (Crude Claw 4-10, avg 7) raises hit further. Verifies the weapon base feeds hit, and that the
    // unarmed fallback is non-zero.
    assert!(
        no_weapon.total_hit_avg > 0.0,
        "unarmed should give non-zero base hit, got {}",
        no_weapon.total_hit_avg
    );
    assert!(
        with_weapon.total_hit_avg > no_weapon.total_hit_avg,
        "weapon base damage should raise hit above unarmed: no-weapon {} → with-weapon {}",
        no_weapon.total_hit_avg,
        with_weapon.total_hit_avg
    );
    assert!(
        with_weapon.dps > 0.0,
        "attack DPS should be > 0 with a weapon"
    );
}

/// Attack/spell gating: a spell skill (Fireball) **does not use** weapon damage — hit is unchanged (still 280) whether or not a weapon is equipped.
#[test]
fn spell_skill_ignores_weapon() {
    let build_data = load_build_data();
    let opts = panel_opts();

    let no_weapon =
        calculate_with_data(&fireball_build(20), &build_data, &opts).expect("spell no weapon");
    let with_weapon =
        fireball_build(20).set_item(EquipmentSlot::Weapon1, bare_weapon("Crude Claw"));
    let out = calculate_with_data(&with_weapon, &build_data, &opts).expect("spell + weapon");

    // A spell doesn't consume weapon base damage or weapon base crit -> hit is identical
    // with or without a weapon equipped. (Non-crit component = 280; total_hit_avg includes
    // Fireball's 7% skill base crit, but that's independent of the weapon.)
    let non_crit: f64 = out.damage_components.iter().map(|c| c.avg()).sum();
    assert!(
        (non_crit - 280.0).abs() < 1.0,
        "spell non-crit hit should ignore weapon base damage, got {non_crit}"
    );
    assert!(
        (out.total_hit_avg - no_weapon.total_hit_avg).abs() < 1.0,
        "spell hit should be weapon-independent: no-weapon {} vs with-weapon {}",
        no_weapon.total_hit_avg,
        out.total_hit_avg
    );
}

/// CostTypes resolution: Fireball L20 mana cost 104, resource = Mana (instant).
#[test]
fn fireball_cost_resolves_to_mana_resource() {
    let build_data = load_build_data();
    let resolved = build_data
        .resolve_skill_level("FireballPlayer", 20)
        .expect("FireballPlayer should resolve");

    assert_eq!(
        resolved.mana_cost,
        Some(104.0),
        "Fireball L20 mana cost should be 104"
    );
    let mana = resolved
        .costs
        .iter()
        .find(|c| c.resource == "Mana")
        .expect("should resolve a Mana cost via CostTypes");
    assert_eq!(mana.amount, 104.0);
    assert!(!mana.per_second, "Mana cost is instant, not per-second");
}

/// Level scaling: L1 base damage (8-12, avg 10) is far below L20 (avg 280), and both are > 0.
#[test]
fn fireball_damage_scales_with_gem_level() {
    let build_data = load_build_data();
    let opts = panel_opts();

    let l1 = calculate_with_data(&fireball_build(1), &build_data, &opts).expect("L1 calc");
    let l20 = calculate_with_data(&fireball_build(20), &build_data, &opts).expect("L20 calc");

    assert!(
        (l1.total_hit_avg - 10.0).abs() < 1.0,
        "Fireball L1 avg hit should be ~10, got {}",
        l1.total_hit_avg
    );
    assert!(
        l20.total_hit_avg > l1.total_hit_avg * 10.0,
        "L20 hit ({}) should vastly exceed L1 ({})",
        l20.total_hit_avg,
        l1.total_hit_avg
    );
}

/// End-to-end port of PoB2 TestSkills "cost efficiency modifiers" (gem
/// assembly harness): Ball Lightning L1 mana cost 9 (data cost_amounts[0]=9,
/// matches PoB2); cost efficiency is injected via customMods. Verifies the
/// full `<Gem> -> SkillManaCostBase -> calc_mana_cost (with Cost Efficiency)
/// -> output.mana_cost` chain.
#[test]
fn ball_lightning_cost_efficiency_e2e() {
    let build_data = load_build_data();
    let ball_lightning = || {
        Build::new()
            .with_character(CharacterIdentity {
                level: 90,
                class_name: "Sorceress".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(
                SocketGroup::new()
                    .with_slot("weapon1")
                    .with_gem("Metadata/Items/Gems/SkillGemBallLightning")
                    .with_active_skill("BallLightningPlayer", 1),
            )
    };
    let cost = |mods: &[&str]| -> f64 {
        let opts = DataOrchestratorOptions {
            extra_modifier_texts: mods.iter().map(|s| s.to_string()).collect(),
            ..panel_opts()
        };
        calculate_with_data(&ball_lightning(), &build_data, &opts)
            .expect("calc")
            .mana_cost
    };
    assert_eq!(cost(&[]), 9.0, "Ball Lightning L1 base mana cost (PoB2 9)");
    assert!(
        (cost(&["50% increased Mana Cost Efficiency"]) - 6.0).abs() < 1e-6,
        "50% eff → {} (PoB2 6)",
        cost(&["50% increased Mana Cost Efficiency"])
    );
    assert!(
        (cost(&["25% increased Cost Efficiency"]) - 7.2).abs() < 1e-3,
        "25% generic eff → {} (PoB2 7.2)",
        cost(&["25% increased Cost Efficiency"])
    );
    let inc_eff = cost(&[
        "50% increased Mana Cost",
        "50% increased Mana Cost Efficiency",
    ]);
    assert!(
        (inc_eff - 8.6667).abs() < 0.1,
        "50% inc + 50% eff → {inc_eff} (PoB2 8.67)"
    );
}

/// End-to-end port of PoB2 TestSkills "Flame Breath attack speed scales DPS
/// and is not capped by its channel cooldown": unarmed Flame Breath (PoB
/// unarmedWeaponData base) +100% attack speed -> DPS should be > baseline
/// ×1.9 (linear scaling, not capped by the channel cooldown). A relative
/// assertion verifying attack-speed -> DPS linearity.
#[test]
fn flame_breath_attack_speed_scales_dps_e2e() {
    let build_data = load_build_data();
    let flame_breath = || {
        Build::new()
            .with_character(CharacterIdentity {
                level: 90,
                class_name: "Sorceress".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(
                SocketGroup::new()
                    .with_slot("weapon1")
                    .with_gem("Metadata/Items/Gem/SkillGemWyvernFlameBreath")
                    .with_active_skill("WyvernFlameBreathPlayer", 20),
            )
    };
    let base = calculate_with_data(&flame_breath(), &build_data, &panel_opts()).expect("base calc");
    assert!(
        base.dps > 0.0,
        "base DPS should be > 0 (unarmed), got {}",
        base.dps
    );

    let fast_opts = DataOrchestratorOptions {
        extra_modifier_texts: vec!["100% increased attack speed".to_string()],
        ..panel_opts()
    };
    let fast = calculate_with_data(&flame_breath(), &build_data, &fast_opts).expect("fast calc");
    assert!(
        fast.dps > base.dps * 1.9,
        "100% attack speed → DPS {} should be > base {} × 1.9",
        fast.dps,
        base.dps
    );
}
