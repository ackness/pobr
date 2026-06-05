//! 召唤物（Minion）域：独立 Actor 的基础属性派生 + player→minion 三通道注入。
//!
//! 核心心智模型（agent-docs/minions.md）：召唤物是一个**独立 Actor**，持有自己的
//! `ModDb`，它的攻防**复用** player 的 offence/defence 管线——只是把 Actor/ModDb 换成
//! 召唤物。玩家身上的普通词条**默认不传递**给召唤物；只有三条通道生效：
//!   1. `MinionModifier`（包裹型 LIST，注入内层 mod 到召唤物 ModDb）；
//!   2. 盟友 buff（`BuffEffectOnSelf` 按召唤物自己缩放）；
//!   3. 属性灌注 flag（`StrengthAddedToMinions` 等把玩家属性以 BASE 注入）。
//!
//! 本模块是 **greenfield 自包含纯函数**：构建一个召唤物 `ModDb` 并填入基础属性 +
//! 内禀属性 + 三通道注入结果，供集成阶段挂到 `Env.minions`。不碰 perform/env/actor。
//!
//! 出处：
//! - agent-docs/minions.md（§1 怪物式 scaling、§1.5 内禀、§2 传递规则、§五 flags）。
//! - PoB2 `src/Modules/CalcPerform.lua`（env.minion 初始化、CritMultiplier/CannotBeEvaded、
//!   MinionModifier/属性灌注注入、L1007/L1063/L1676）。
//! - PoB2 `src/Modules/CalcActiveSkill.lua`（minion 等级判定、lifeTable/damageTable、虚拟武器）。
//! - PoB2 `src/Data/Misc.lua`（minionLevelTable={2,4,…,80}、playerMinionIntrinsicStats）。

use pobr_data::prelude::*;

use crate::{ModDb, Modifier};

use super::round;

/// 召唤物内禀爆伤加成（`playerMinionIntrinsicStats.base_critical_hit_damage_bonus`）。
/// 召唤物最终爆伤基础 = 怪物基础（`MONSTER_BASE_CRIT_DAMAGE_BONUS`=30）+ 该内禀（70）= 100。
/// 出处：agent-docs/minions.md §1.5；PoB2 Misc.lua / CalcPerform.lua L1007。
pub const MINION_INTRINSIC_CRIT_DAMAGE_BONUS: f64 = 70.0;

/// 召唤物虚拟武器默认暴击率（`minionData.critChance` 缺省值）。
/// 出处：agent-docs/minions.md §1.3 / §1.5；PoB2 CalcActiveSkill.lua。
pub const MINION_DEFAULT_WEAPON_CRIT_CHANCE: f64 = 5.0;

/// 召唤物等级表（宝石等级 → 怪物等级）：`minionLevelTable = {2,4,…,80}`。
/// 索引 0 对应宝石等级 1。出处：PoB2 Misc.lua `data.minionLevelTable`。
pub const MINION_LEVEL_TABLE: [u32; 40] = [
    2, 4, 6, 8, 10, 12, 14, 16, 18, 20, 22, 24, 26, 28, 30, 32, 34, 36, 38, 40, 42, 44, 46, 48, 50,
    52, 54, 56, 58, 60, 62, 64, 66, 68, 70, 72, 74, 76, 78, 80,
];

/// 把召唤宝石等级（1..=40）映射到对应的怪物等级（2..=80），超界 clamp 到表边界。
/// 出处：agent-docs/minions.md §1.1；PoB2 `data.minionLevelTable`。
pub fn minion_level_from_gem_level(gem_level: u32) -> u32 {
    let idx = (gem_level.max(1) - 1) as usize;
    let idx = idx.min(MINION_LEVEL_TABLE.len() - 1);
    MINION_LEVEL_TABLE[idx]
}

/// 召唤物的归一化乘数（`Minions.lua` 每个条目的字段）。默认值对应「裸怪物表」。
///
/// 召唤物基础属性 = 怪物等级基准表 × 该乘数。出处：agent-docs/minions.md §1。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MinionData {
    /// 生命归一化系数（如 Zombie 0.7、Raging Spirit 0.25）。
    pub life: f64,
    /// 伤害归一化系数。
    pub damage: f64,
    /// min/max 伤害区间宽度（如 Zombie 0.3）。
    pub damage_spread: f64,
    /// 基础攻击间隔（秒）。
    pub attack_time: f64,
    /// 虚拟武器暴击率（默认 5）。
    pub crit_chance: f64,
    /// 护甲归一化系数（缺省 1）。
    pub armour: f64,
    /// 闪避归一化系数（缺省 1）。
    pub evasion: f64,
    /// 生命转 ES 的占比（如骷髅法师 0.15 → LifeConvertToEnergyShield BASE=15）。
    pub energy_shield: f64,
    pub fire_resist: f64,
    pub cold_resist: f64,
    pub lightning_resist: f64,
    pub chaos_resist: f64,
    /// 基础伤害是否忽略攻速（Zombie/RagingSpirit=true：攻速只影响 DPS、不影响每击伤害）。
    pub base_damage_ignores_attack_speed: bool,
}

impl Default for MinionData {
    fn default() -> Self {
        Self {
            life: 1.0,
            damage: 1.0,
            damage_spread: 0.0,
            attack_time: 1.0,
            crit_chance: MINION_DEFAULT_WEAPON_CRIT_CHANCE,
            armour: 1.0,
            evasion: 1.0,
            energy_shield: 0.0,
            fire_resist: 0.0,
            cold_resist: 0.0,
            lightning_resist: 0.0,
            chaos_resist: 0.0,
            base_damage_ignores_attack_speed: false,
        }
    }
}

/// 召唤物的虚拟武器（攻击型召唤物伤害入口）。
///
/// 召唤物攻击不是直接伤害词条，而是合成一把虚拟武器再喂给攻击伤害管线。
/// 出处：agent-docs/minions.md §1.3；PoB2 CalcActiveSkill.lua `weaponData1`。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MinionWeaponData {
    pub physical_min: f64,
    pub physical_max: f64,
    pub crit_chance: f64,
    pub attack_rate: f64,
}

/// 召唤物的基础属性（从怪物表 × 归一化乘数派生，写入 ModDb 之前的快照）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MinionBaseStats {
    pub level: u32,
    pub life: f64,
    pub armour: f64,
    pub evasion: f64,
    pub energy_shield: f64,
    pub fire_resist: f64,
    pub cold_resist: f64,
    pub lightning_resist: f64,
    pub chaos_resist: f64,
    /// 爆伤基础（怪物 30 + 内禀 70 = 100）。
    pub crit_multiplier_base: f64,
    /// 默认必中（0.3.0+ 召唤物 `CannotBeEvaded`）。
    pub always_hit: bool,
    pub weapon: MinionWeaponData,
}

/// 召唤物输入：召唤宝石等级 + 归一化数据 + 玩家受控通道。
///
/// 集成阶段由 `Env`/技能数据填充；本模块对其做纯函数派生。
#[derive(Debug, Clone)]
pub struct MinionInput {
    /// 召唤宝石等级（1..=40），经 `minion_level_from_gem_level` 映射到怪物等级。
    pub gem_level: u32,
    /// 归一化乘数。
    pub data: MinionData,
    /// 通道 1：`MinionModifier` 内层 mod（玩家技能上「Minions deal/have …」展开后的内层 mod）。
    /// 每项可带可选 `type` 限定（`Some` 时仅对该类召唤物生效）。
    pub minion_modifiers: Vec<MinionModifierEntry>,
    /// 通道 2：盟友 buff 提供给召唤物的 mod（已按召唤物 `BuffEffectOnSelf` 缩放的结果）。
    pub ally_buff_mods: Vec<Modifier>,
    /// 通道 3：属性灌注（旗标驱动的 BASE 注入），见 [`AttributeInfusion`]。
    pub attribute_infusion: AttributeInfusion,
    /// 本召唤物类型标识（用于匹配 `MinionModifierEntry.type` 限定）。
    pub minion_type: Option<String>,
}

/// `MinionModifier` 包裹项：内层 mod + 可选类型限定。
///
/// 出处：agent-docs/minions.md §2.2；PoB2 CalcPerform.lua L1676
/// `if not value.type or env.minion.type == value.type then AddMod(value.mod)`。
#[derive(Debug, Clone, PartialEq)]
pub struct MinionModifierEntry {
    /// 内层 mod，携带完整 flag/condition/tag，在召唤物自己的 `matches(cfg)` 下生效。
    pub inner: Modifier,
    /// 可选类型限定：`Some` 时仅当召唤物类型相符才注入。
    pub minion_type: Option<String>,
}

/// 属性灌注：旗标驱动的玩家属性 → 召唤物 BASE 注入。
///
/// 召唤物默认 0 基础属性、不继承玩家属性；只有这些旗标把玩家属性灌入，灌入后召唤物
/// 按相同属性派生规则转生命/护甲/暴击。出处：agent-docs/minions.md §2.6；
/// PoB2 CalcPerform.lua L1063（StrengthAddedToMinions / HalfStrengthAddedToMinions / DexterityAddedToMinions）。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AttributeInfusion {
    /// 玩家当前 Strength（已派生值）。
    pub player_strength: f64,
    /// 玩家当前 Dexterity（已派生值）。
    pub player_dexterity: f64,
    /// `StrengthAddedToMinions`：注入全额 Str。
    pub strength_added: bool,
    /// `HalfStrengthAddedToMinions`：注入半额 Str。
    pub half_strength_added: bool,
    /// `DexterityAddedToMinions`：注入全额 Dex。
    pub dexterity_added: bool,
}

/// 召唤物上下文：派生后的基础属性 + 已注入三通道的 ModDb。
///
/// 集成阶段把它转成一个 `Actor` 接到 `Env.minions`，再走标准 offence/defence。
#[derive(Debug, Clone)]
pub struct MinionContext {
    pub base: MinionBaseStats,
    pub mod_db: ModDb,
}

/// 怪物等级基准 + 归一化乘数 → 召唤物基础属性 + 内禀属性 + 虚拟武器。
///
/// 复用 `pobr-data::monster` 的怪物表（`MonsterScalingRow`）：召唤物等级经
/// `minion_level_from_gem_level` 映射后查表，再乘以归一化乘数。
/// 出处：agent-docs/minions.md §1；PoB2 CalcActiveSkill.lua / CalcPerform.lua。
pub fn derive_minion_base_stats(gem_level: u32, data: &MinionData) -> MinionBaseStats {
    let level = minion_level_from_gem_level(gem_level);
    let row = MonsterScalingRow::at_level(level);

    let life = round(row.life as f64 * data.life);
    let armour = round(row.armour as f64 * data.armour);
    let evasion = round(row.evasion as f64 * data.evasion);
    // 生命转 ES：energy_shield 占比 × 100 = LifeConvertToEnergyShield BASE，这里直接折算 ES 基础。
    let energy_shield = round(life * data.energy_shield);

    // 虚拟武器伤害基础：damage 表 × 归一化乘数，base_damage_ignores_attack_speed=false 时再乘攻击间隔。
    let mut damage = row.damage * data.damage;
    if !data.base_damage_ignores_attack_speed {
        damage *= data.attack_time;
    }
    let weapon = MinionWeaponData {
        physical_min: round(damage * (1.0 - data.damage_spread)),
        physical_max: round(damage * (1.0 + data.damage_spread)),
        crit_chance: data.crit_chance,
        attack_rate: if data.attack_time > 0.0 {
            round(1.0 / data.attack_time)
        } else {
            0.0
        },
    };

    MinionBaseStats {
        level,
        life,
        armour,
        evasion,
        energy_shield,
        fire_resist: data.fire_resist,
        cold_resist: data.cold_resist,
        lightning_resist: data.lightning_resist,
        chaos_resist: data.chaos_resist,
        // 爆伤基础 = 怪物 30 + 内禀 70 = 100（不要照搬玩家 +100/+130）。
        crit_multiplier_base: MONSTER_BASE_CRIT_DAMAGE_BONUS + MINION_INTRINSIC_CRIT_DAMAGE_BONUS,
        always_hit: true,
        weapon,
    }
}

/// 把派生的内禀属性写入召唤物 ModDb（爆伤基础、必中、抗性、生命/护甲/闪避/ES）。
fn write_intrinsics(db: &mut ModDb, base: &MinionBaseStats) {
    let game = |id: &str| SourceId::new(SourceKind::GameConstant, id.to_string());

    db.add_mod(
        Modifier::number("CritMultiplier", ModType::Base, base.crit_multiplier_base)
            .with_origin(ModifierSource::new(game("minion.intrinsic.crit"))),
    );
    if base.always_hit {
        db.add_mod(
            Modifier::flag("CannotBeEvaded")
                .with_origin(ModifierSource::new(game("minion.intrinsic.always_hit"))),
        );
    }
    db.add_mod(
        Modifier::number("Life", ModType::Base, base.life)
            .with_origin(ModifierSource::new(game("minion.base.life"))),
    );
    db.add_mod(
        Modifier::number("Armour", ModType::Base, base.armour)
            .with_origin(ModifierSource::new(game("minion.base.armour"))),
    );
    db.add_mod(
        Modifier::number("Evasion", ModType::Base, base.evasion)
            .with_origin(ModifierSource::new(game("minion.base.evasion"))),
    );
    if base.energy_shield > 0.0 {
        db.add_mod(
            Modifier::number("EnergyShield", ModType::Base, base.energy_shield)
                .with_origin(ModifierSource::new(game("minion.base.energy_shield"))),
        );
    }
    for (name, value, id) in [
        ("FireResist", base.fire_resist, "minion.base.fire_resist"),
        ("ColdResist", base.cold_resist, "minion.base.cold_resist"),
        (
            "LightningResist",
            base.lightning_resist,
            "minion.base.lightning_resist",
        ),
        ("ChaosResist", base.chaos_resist, "minion.base.chaos_resist"),
    ] {
        if value != 0.0 {
            db.add_mod(
                Modifier::number(name, ModType::Base, value)
                    .with_origin(ModifierSource::new(game(id))),
            );
        }
    }
}

/// 把属性灌注（旗标驱动）写入召唤物 ModDb 的 `Str`/`Dex` BASE。
fn write_attribute_infusion(db: &mut ModDb, infusion: &AttributeInfusion) {
    let src = |id: &str| ModifierSource::new(SourceId::new(SourceKind::Config, id.to_string()));
    if infusion.strength_added {
        db.add_mod(
            Modifier::number("Str", ModType::Base, round(infusion.player_strength))
                .with_origin(src("minion.infusion.strength")),
        );
    } else if infusion.half_strength_added {
        db.add_mod(
            Modifier::number("Str", ModType::Base, round(infusion.player_strength * 0.5))
                .with_origin(src("minion.infusion.half_strength")),
        );
    }
    if infusion.dexterity_added {
        db.add_mod(
            Modifier::number("Dex", ModType::Base, round(infusion.player_dexterity))
                .with_origin(src("minion.infusion.dexterity")),
        );
    }
}

/// 判定某条 `MinionModifierEntry` 是否应注入给该类型召唤物。
///
/// 出处：PoB2 CalcPerform.lua L1676 `if not value.type or env.minion.type == value.type`。
pub fn minion_modifier_applies(entry: &MinionModifierEntry, minion_type: Option<&str>) -> bool {
    match &entry.minion_type {
        None => true,
        Some(want) => minion_type == Some(want.as_str()),
    }
}

/// 三通道注入 + 内禀，构建完整的召唤物 `MinionContext`。
///
/// 这是召唤物域的高层入口：纯函数，输入玩家受控通道 + 归一化数据，输出可直接走
/// offence/defence 的召唤物 Actor 数据。
pub fn build_minion_context(input: &MinionInput) -> MinionContext {
    let base = derive_minion_base_stats(input.gem_level, &input.data);
    let mut db = ModDb::new();

    // 内禀 + 怪物式基础属性。
    write_intrinsics(&mut db, &base);

    // 通道 1：MinionModifier（带 type 限定）。
    for entry in &input.minion_modifiers {
        if minion_modifier_applies(entry, input.minion_type.as_deref()) {
            db.add_mod(entry.inner.clone());
        }
    }

    // 通道 2：盟友 buff（已按召唤物 BuffEffectOnSelf 缩放后的 mod）。
    for m in &input.ally_buff_mods {
        db.add_mod(m.clone());
    }

    // 通道 3：属性灌注。
    write_attribute_infusion(&mut db, &input.attribute_infusion);

    MinionContext { base, mod_db: db }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CalcConfig;

    #[test]
    fn gem_level_maps_to_monster_level() {
        // 宝石等级 1 → 怪物等级 2；等级 40 → 80；超界 clamp。
        assert_eq!(minion_level_from_gem_level(1), 2);
        assert_eq!(minion_level_from_gem_level(20), 40);
        assert_eq!(minion_level_from_gem_level(40), 80);
        assert_eq!(minion_level_from_gem_level(0), 2);
        assert_eq!(minion_level_from_gem_level(999), 80);
    }

    #[test]
    fn minion_crit_multiplier_base_is_monster_plus_intrinsic() {
        let base = derive_minion_base_stats(20, &MinionData::default());
        // 30 (monster) + 70 (intrinsic) = 100，不等于玩家默认。
        assert_eq!(base.crit_multiplier_base, 100.0);
    }

    #[test]
    fn minion_always_hits_by_default() {
        let ctx = build_minion_context(&MinionInput {
            gem_level: 20,
            data: MinionData::default(),
            minion_modifiers: vec![],
            ally_buff_mods: vec![],
            attribute_infusion: AttributeInfusion::default(),
            minion_type: None,
        });
        assert!(
            ctx.mod_db
                .flag(&CalcConfig::attack(), ModName::from("CannotBeEvaded"))
        );
    }
}
