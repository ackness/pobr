//! 宝石数据通道 Phase 3 端到端验证：技能分等级**基础伤害** → DPS。
//!
//! 用真实入库数据（`data/4.5.0.3.4/granted_effect_stat_sets.json`）验证
//! 「`<Gem skillId>` + 等级 → stat-set 基础伤害 → `<Type>DamageMin/Max` BASE 词条
//! → 伤害分量 → DPS」整条通道。基准技能 Fireball（纯法术、基础伤害仅来自 stat-set，
//! 不依赖未接的武器伤害），其 L20 基础火焰伤害 224–336（与 PoB 自身
//! `Data/Skills/act_int.lua` 解析后逐字一致），平均击中 280。

use pobr_build::{
    Build, BuildData, CharacterIdentity, DataOrchestratorOptions, SocketGroup, calculate_with_data,
};
use pobr_core::calc::MinimalInput;
use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};
use pobr_data::monster::EnemyTier;
use pobr_gamedata::{GameData, repo_data_root};

fn load_build_data() -> BuildData {
    let data = GameData::new(repo_data_root().join("4.5.0.3.4"));
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
    }
}

/// Fireball L20：基础火焰伤害 224–336（avg 280），DPS = avg × 行动速率 × 命中率 > 0。
#[test]
fn fireball_base_damage_drives_nonzero_dps() {
    let build_data = load_build_data();

    // 前置：stat-set 域确实载入了 Fireball 的分等级伤害（数据通道未断）。
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

    // 无任何 increased/more 来源 → 非暴击平均击中 = (224 + 336) / 2 = 280。
    let non_crit: f64 = out.damage_components.iter().map(|c| c.avg()).sum();
    assert!(
        (non_crit - 280.0).abs() < 1.0,
        "Fireball L20 non-crit hit should be ~280, got {non_crit}"
    );
    // total_hit_avg 含 Fireball 7% 基础暴击放大（PoB2 AverageDamage 为暴击加权）→ 略高于 280。
    assert!(
        out.total_hit_avg > 280.0,
        "Fireball average hit should include base crit, got {}",
        out.total_hit_avg
    );
    // 行动速率来自 cast time 1.2s → 1/1.2 ≈ 0.833；命中率 > 0 → DPS > 0。
    assert!(
        out.dps > 0.0,
        "Fireball DPS should be > 0 once base damage is injected, got {}",
        out.dps
    );
    assert!(out.action_rate > 0.0, "action_rate should be > 0");
}

/// P0-2 support 宝石注入：同组**兼容**的「增加伤害」support 宝石的 `damage_+%` 应抬升击中。
///
/// M1-T3.6 起注入前经组级适用性裁决（PoB2 CalcTools.lua:84-110 +
/// CalcActiveSkill.lua:179-210）：本测试原用 `SupportFerociousRoarPlayer`（require
/// `[Warcry]`——PoB2 中 Ferocious Roar 只能支援战吼），对 Fireball（法术）属误注入，
/// 裁决后正确拒收（拒收断言见 tests/support_gating.rs）。改用
/// `SupportMetaCastFireSpellOnHitPlayer`（require `[Spell, Triggerable, Fire, AND,
/// AND]`，Fireball 全部具备）验证兼容 support 的 INC Damage 注入通道。
#[test]
fn fireball_with_damage_support_raises_hit() {
    let build_data = load_build_data();

    // 该 support 确有可映射的 damage_+% stat（数据通道未断）。support 无品质表
    // 条目（PoB2 导出即跳过），quality 传 0，取 base 段。
    let sup = build_data.effect_stats("SupportMetaCastFireSpellOnHitPlayer", 20, 0);
    let inc = sup
        .base
        .iter()
        .find(|s| s.stat == "damage_+%")
        .expect("support should carry damage_+%");
    assert!(inc.value > 0.0);

    let base = calculate_with_data(&fireball_build(20), &build_data, &panel_opts())
        .expect("no-support calc");

    // 同组加入带 damage_+% 的兼容 support → 注入 Damage INC。
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

    // damage_+% 是 INC：击中 = base × (1 + inc/100)。L20 inc=200 → ×3。
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

/// 一把裸基底武器（无词条），用于验证武器基底伤害装配。
fn bare_weapon(base_name: &str) -> Item {
    Item {
        base: ItemBaseId::from(base_name),
        rarity: ItemRarity::Normal,
        quality: 0,
        implicit_texts: vec![],
        modifier_texts: vec![],
        enchant_texts: vec![],
        rolled_defence: RolledDefence::default(),
        parsed_stats: vec![],
    }
}

/// 武器基底伤害装配（roadmap 链 A #1）：攻击技能装备武器后，武器基底物理伤害进入击中。
/// `Crude Claw` 基底 4–10 物理 → 平均 7 注入 base_hit。
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

    // 裸手用 unarmed 基底（Warrior 物理 2–8，avg 5）→ 非零；装备武器（Crude Claw 4–10，
    // avg 7）后击中更高。验证武器基底进入击中、且空手回退非零。
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

/// 攻击/法术门控：法术技能（Fireball）**不使用**武器伤害——装备武器击中不变（仍 280）。
#[test]
fn spell_skill_ignores_weapon() {
    let build_data = load_build_data();
    let opts = panel_opts();

    let no_weapon =
        calculate_with_data(&fireball_build(20), &build_data, &opts).expect("spell no weapon");
    let with_weapon =
        fireball_build(20).set_item(EquipmentSlot::Weapon1, bare_weapon("Crude Claw"));
    let out = calculate_with_data(&with_weapon, &build_data, &opts).expect("spell + weapon");

    // 法术不吃武器基底伤害，也不吃武器基底暴击 → 装备武器后击中与无武器完全一致。
    // （非暴击分量 = 280；total_hit_avg 含 Fireball 7% 技能基础暴击，但与武器无关。）
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

/// CostTypes 解析（P2-6/P2-5）：Fireball L20 法力消耗 104，资源名 = Mana（瞬时）。
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

/// 等级缩放：L1 基础伤害（8–12，avg 10）远低于 L20（avg 280），且均 > 0。
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

/// PoB2 TestSkills「cost efficiency modifiers」端到端移植（gem 装配 harness）：Ball Lightning
/// L1 法力消耗 9（data cost_amounts[0]=9，与 PoB2 一致）；消耗效率经 customMods 注入。
/// 验证 `<Gem> → SkillManaCostBase → calc_mana_cost（含 Cost Efficiency）→ output.mana_cost` 整链。
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

/// PoB2 TestSkills「Flame Breath attack speed scales DPS and is not capped by its channel
/// cooldown」端到端移植：空手 Flame Breath（PoB unarmedWeaponData 基底）+100% 攻速 → DPS
/// 应 > 基线 ×1.9（线性缩放，不被通道冷却封顶）。相对断言，验证攻速→DPS 线性。
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
