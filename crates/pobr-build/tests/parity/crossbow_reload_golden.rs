//! Crossbow reload golden fixture: the 18-build corpus has no non-grenade
//! crossbow user (mercenary/ranger explosive-grenade wields a crossbow, but
//! the `Grenade` type doesn't consume ammo — vendor `CalcOffence.lua:1118`
//! exempts it under the same gating; its speed/DPS staying unchanged is
//! already pinned jointly by the deadeye anchor in `golden_regression.rs` and
//! `parity_no_regression`), so this file **constructs** a crossbow-user build
//! with real data and adds it to the golden set:
//!
//! - Weapon = Makeshift Crossbow (`reload_time_ms = 800`, from overlay
//!   `base_item_overrides.json` merged by gamedata — a live check of the
//!   dual-route fallback channel);
//! - Skill = Armour Piercing Rounds (firing effect `ArmourPiercingBoltsPlayer`
//!   `CrossbowSkill` + ammo effect `ArmourPiercingBoltsAmmoPlayer`, magazine
//!   `base_number_of_crossbow_bolts` L1 = 12).
//!
//! Registered gap: under the real import path, an ammo gem's XML `skillId` is
//! the ammo effect's id, and step 3 of `pick_group_main_skill` will pick the
//! ammo effect itself (which has an Attack type) as the main skill — in that
//! case the reload gate conservatively rejects it (`CrossbowAmmoSkill`), a
//! zero-behaviour outcome. Fixing main-skill resolution's handling of the ammo
//! gem is deferred until a real crossbow-build fixture is added to the corpus.

use pobr_build::{
    Build, BuildData, CharacterIdentity, DataOrchestratorOptions, SocketGroup, calculate_with_data,
};
use pobr_core::calc::MinimalInput;
use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};
use pobr_data::monster::EnemyTier;
use pobr_gamedata::{GameData, repo_data_root};

fn load_build_data() -> BuildData {
    // Pins the data version being checked against the golden values (decoupled from the active DATA_VERSION); see pobr_data::GOLDEN_PARITY_DATA_VERSION.
    let data = GameData::new(repo_data_root().join(pobr_data::GOLDEN_PARITY_DATA_VERSION));
    BuildData::load(&data).expect("load BuildData")
}

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

fn crossbow_build() -> Build {
    Build::new()
        .with_character(CharacterIdentity {
            level: 50,
            class_name: "Mercenary".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(
            SocketGroup::new()
                .with_slot("weapon1")
                .with_gem("Metadata/Items/Gem/SkillGemArmourPiercingRounds")
                // Firing effect goes first (main-skill resolution picks the first damage skill); the ammo effect in the same group provides the magazine.
                .with_gem_skill("ArmourPiercingBoltsPlayer", 1)
                .with_gem_skill("ArmourPiercingBoltsAmmoPlayer", 1),
        )
        .set_item(EquipmentSlot::Weapon1, bare_weapon("Makeshift Crossbow"))
}

fn opts() -> DataOrchestratorOptions {
    DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        enemy_level: 0,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: false,
        extra_modifier_texts: vec![],
        ..Default::default()
    }
}

/// End-to-end: crossbow reload folds into effective rate and DPS (bolt=12, reload=0.8s).
///
/// Expected rate = `B / (B/f + R)`, where `f` is the pre-reload firing rate
/// (`skill_use_time.tooltip_rate`; this fixture has no action speed and
/// doesn't hit the frame cap, so tooltip = the pre-cap rate).
#[test]
fn constructed_crossbow_build_applies_reload_cycle() {
    let data = load_build_data();
    let out = calculate_with_data(&crossbow_build(), &data, &opts()).expect("calculate");

    let sut = out.skill_use_time.expect("skill use time present");
    let firing_rate = sut.tooltip_rate;
    assert!(firing_rate > 0.0, "弩攻击须有非零射速：{sut:?}");

    let expected = 12.0 / (12.0 / firing_rate + 0.8);
    assert!(
        (out.effective_action_rate - expected).abs() < 1e-2,
        "reload 循环平均速率：{} vs {expected}（firing_rate={firing_rate}）",
        out.effective_action_rate
    );
    assert!(
        out.effective_action_rate < firing_rate,
        "reload 必须降低有效速率：{} >= {firing_rate}",
        out.effective_action_rate
    );
    // Panel rate (PoB2's output.Speed rewrite convention) folds by the same
    // factor as DPS, keeping the AverageDamage = dps / action_rate identity.
    assert!(
        (out.action_rate - expected).abs() < 1e-2,
        "面板速率须为 reload 折算后值：{}",
        out.action_rate
    );
    assert!(out.dps > 0.0, "弩 build DPS 应非零：{}", out.dps);
}

/// A crossbow weapon with a grenade-type skill (same shape as the
/// deadeye/gemling explosive-grenade corpus builds): the reload gate is
/// exempt — a grenade main skill's output on the same weapon and level isn't
/// folded by reload.
#[test]
fn grenade_on_crossbow_is_exempt_from_reload() {
    let data = load_build_data();
    let build = Build::new()
        .with_character(CharacterIdentity {
            level: 50,
            class_name: "Mercenary".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(
            SocketGroup::new()
                .with_slot("weapon1")
                .with_gem("Metadata/Items/Gem/SkillGemExplosiveGrenade")
                .with_gem_skill("ExplosiveGrenadePlayer", 1),
        )
        .set_item(EquipmentSlot::Weapon1, bare_weapon("Makeshift Crossbow"));
    let out = calculate_with_data(&build, &data, &opts()).expect("calculate");
    if let Some(sut) = out.skill_use_time {
        assert_eq!(
            out.effective_action_rate, sut.effective_rate,
            "grenade 不进 reload 模型：有效速率须与 use-time 解析一致"
        );
    }
}
