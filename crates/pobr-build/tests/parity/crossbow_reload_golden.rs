//! 弩 reload golden fixture（M4-T4 W-D2）：语料 18 build 无非 grenade 弩手
//! （mercenary/ranger explosive-grenade 是持弩者但 `Grenade` 类型不消耗弹药，
//! vendor `CalcOffence.lua:1118` 同口径门控豁免——其速率/DPS 不变由
//! `golden_regression.rs` deadeye 销钉与 `parity_no_regression` 共同钉住），
//! 故按蓝图 §2 W-D2 用真实数据**构造**弩手 build 入 golden：
//!
//! - 武器 = Makeshift Crossbow（`reload_time_ms = 800`，overlay
//!   `base_item_overrides.json` 经 gamedata merge——双路线兜底通道活体验证）；
//! - 技能 = Armour Piercing Rounds（射击效果 `ArmourPiercingBoltsPlayer`
//!   `CrossbowSkill` + ammo 效果 `ArmourPiercingBoltsAmmoPlayer` 弹匣
//!   `base_number_of_crossbow_bolts` L1 = 12）。
//!
//! 已登记缺口：真实导入路径下 ammo-gem 的 XML `skillId` = ammo 效果 id，
//! `pick_group_main_skill` 第 3 步会把 ammo 自身（含 Attack 类型）当主技能
//! ——此时 reload 门控保守拒绝（`CrossbowAmmoSkill`），零行为；主技能解析
//! 对 ammo 宝石的穿透留待弩 build 语料入集后对齐。

use pobr_build::{
    Build, BuildData, CharacterIdentity, DataOrchestratorOptions, SocketGroup, calculate_with_data,
};
use pobr_core::calc::MinimalInput;
use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};
use pobr_data::monster::EnemyTier;
use pobr_gamedata::{GameData, repo_data_root};

fn load_build_data() -> BuildData {
    // golden 钉定被校验的数据版本（与活动 DATA_VERSION 解耦）；见 pobr_data::GOLDEN_PARITY_DATA_VERSION。
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
                // 射击效果在前（主技能解析取首个伤害技能）；ammo 效果同组提供弹匣。
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

/// 端到端：弩 reload 折算进有效速率与 DPS（bolt=12、reload=0.8s）。
///
/// 预期速率 = `B / (B/f + R)`，`f` 取折算前射速（`skill_use_time.tooltip_rate`，
/// 本 fixture 无 action speed / 未触帧 cap，tooltip = cap 前速率）。
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
    // 面板速率（PoB2 output.Speed 改写口径）与 DPS 同因子折算，
    // AverageDamage = dps / action_rate 恒等式保持。
    assert!(
        (out.action_rate - expected).abs() < 1e-2,
        "面板速率须为 reload 折算后值：{}",
        out.action_rate
    );
    assert!(out.dps > 0.0, "弩 build DPS 应非零：{}", out.dps);
}

/// 弩武器但 grenade 类技能（语料 deadeye/gemling explosive-grenade 同形态）：
/// reload 门控豁免——同武器、同等级的 grenade 主技能输出不被 reload 折算。
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
