//! 注入计算引擎的运行时常量包（M0-W3 注入管道，架构文档 20 §1 P8/P9、§2.2）。
//!
//! # 管道形状（阶段 2 各域 agent 的接口契约）
//!
//! [`RuntimeConstants`] 聚合「calc 消费的入库数据域」（pobr-data catalog 类型），
//! 沿以下路径注入，pobr-core 保持零 I/O：
//!
//! ```text
//! data/<版本>/base/*.json
//!   →（pobr-gamedata `GameData::load_ruleset()`，唯一 I/O 点）RuleSet { Option<各域 Def> }
//!   →（pobr-build `BuildData::load` 合并：Some=数据，None=Default fallback）RuntimeConstants
//!   →（pobr-build `calculate_with_data` → `CalculationSession::set_constants`）
//!   → pobr-core `CalcConfig.constants`（随 cfg 线程化到全部 calc 函数）
//! ```
//!
//! # 不变式
//!
//! - **`Default` = fallback**：各域字段的 `Default` 必须与对应 JSON 逐值相等
//!   （W2 已锁 JSON == 旧 Rust 硬编码；`Default` 引用旧 Rust 常量而非复制字面量），
//!   因此「无 GameData 走 Default」与「有 GameData 走注入」输出逐值一致——
//!   搬迁不变式（parity 零变化）的结构保证。
//! - **扩展方式（阶段 2）**：每接入一个新数据域，在本结构体**追加一个字段**
//!   （类型 = `catalog` 对应 Def，`Default` 实现落 pobr-data、引用既有 Rust 准源），
//!   并在 `pobr-gamedata::RuleSet` / `pobr-build::BuildData::load` 各补一行合并；
//!   消费侧经 `cfg.constants.<域>` 读取。表型大域（百级表等）如经测量 clone 成本
//!   过高，再考虑改挂 `Env`（setup 期一次性消费），勿提前优化。

use serde::{Deserialize, Serialize};

use super::enemy_presets::EnemyPresetsTable;
use super::game_constants::{
    CharacterConstantsDef, GameConstantsDef, GameMechanicsConstantsDef, MonsterConstantsDef,
};
use super::monster_scaling::MonsterScalingTable;
use super::unarmed_data::UnarmedDataTable;
use super::weapon_types::WeaponTypeTable;

/// 注入 calc 的运行时常量包。`Default` = 无 GameData 时的 fallback（与 JSON 逐值相等）。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RuntimeConstants {
    /// 全局游戏常数三段表（`base/game_constants.json`）。
    pub game_constants: GameConstantsDef,
    /// 角色等级/属性派生常量（`base/character_constants.json`，消费方
    /// `pobr-core::character` 的 `CharacterBase` 派生公式）。注意与
    /// [`Self::character`]（game_constants 的 character 段，玩家基线魔数）区分：
    /// 本域承载的是 life/mana/accuracy 的 per-level / per-attribute 派生系数。
    pub character_constants: super::character_constants::CharacterConstantsDef,
    /// 怪物百级缩放表（`base/monster_scaling.json`；`setup_env` 敌人装配消费）。
    pub monster_scaling: MonsterScalingTable,
    /// 敌人档位预设（`base/enemy_presets.json`；`setup_env` 档位加成/穿透消费）。
    pub enemy_presets: EnemyPresetsTable,
    /// per-class 空手基底表（`base/unarmed_data.json`，源 PoB2 `data.unarmedWeaponData`）。
    /// 无主手武器时攻击技能的 weaponData 来源（pobr-build `unarmed_contribution` 消费）。
    pub unarmed_data: UnarmedDataTable,
    /// 武器类型表（`base/weapon_types.json`，源 PoB2 `data.weaponTypeInfo`）。
    /// 武器持握/近战条件与武器类伤害关键词判定的查表来源（pobr-build 消费；
    /// 键空间 = PoB base `type` 名，GGG `item_class` → 表键的映射留消费侧）。
    pub weapon_types: WeaponTypeTable,
}

impl RuntimeConstants {
    /// 机制公式魔数段（抗性边界 / 护甲系数 / 服务器帧 / 异常基线 / 各类 cap）。
    pub fn game(&self) -> &GameMechanicsConstantsDef {
        &self.game_constants.game
    }

    /// 玩家固有常数段（默认最大抗性 / 玩家基础爆伤加成等）。
    pub fn character(&self) -> &CharacterConstantsDef {
        &self.game_constants.character
    }

    /// 怪物固有常数段（怪物最大抗性 / 怪物基础爆伤加成等）。
    pub fn monster(&self) -> &MonsterConstantsDef {
        &self.game_constants.monster
    }
}

#[cfg(test)]
mod tests {
    use super::RuntimeConstants;

    /// fallback 不变式抽查：`Default` 与旧 Rust 准源逐值相等
    /// （完整 Default == JSON 对照在 `pobr-gamedata/tests/load_game_constants.rs`）。
    #[test]
    fn default_matches_legacy_rust_constants() {
        let c = RuntimeConstants::default();
        assert_eq!(
            c.character().base_maximum_all_resistances_pct,
            crate::constants::DEFAULT_MAX_RESISTANCE
        );
        assert_eq!(
            c.game().resist_hard_cap,
            crate::constants::HARD_MAX_RESISTANCE
        );
        assert_eq!(c.game().resist_floor, crate::constants::RESIST_FLOOR);
        assert_eq!(c.game().armour_ratio, crate::constants::ARMOUR_RATIO);
        assert_eq!(
            c.game().server_tick_seconds,
            crate::constants::SERVER_TICK_SECONDS
        );
        assert_eq!(
            c.game().block_chance_cap,
            crate::constants::BLOCK_CHANCE_CAP
        );
        assert_eq!(c.game().dot_dps_cap, crate::constants::DOT_DPS_CAP);
        assert_eq!(
            c.game().shock_min_effect,
            crate::constants::SHOCK_MIN_EFFECT
        );
        assert_eq!(
            c.character().base_critical_hit_damage_bonus,
            crate::constants::PLAYER_BASE_CRIT_DAMAGE_BONUS
        );
        let legacy = crate::constants::GameConstants::poe2();
        assert_eq!(c.game().bleed_base_fraction, legacy.bleed_base_fraction);
        assert_eq!(c.game().bleed_base_duration, legacy.bleed_base_duration);
        assert_eq!(c.game().ignite_base_fraction, legacy.ignite_base_fraction);
        assert_eq!(c.game().ignite_base_duration, legacy.ignite_base_duration);
        assert_eq!(c.game().poison_base_fraction, legacy.poison_base_fraction);
        assert_eq!(c.game().poison_base_duration, legacy.poison_base_duration);
        assert_eq!(c.game().shock_default_effect, legacy.shock_default_effect);
    }

    /// fallback 不变式抽查（monster-scaling 域）：百级表/档位预设的 `Default`
    /// 与旧 Rust 准源逐值相等（完整对照在各自 catalog 模块测试 +
    /// `pobr-gamedata/tests/load_monster_scaling.rs` / `load_enemy_presets.rs`）。
    #[test]
    fn default_monster_domains_match_legacy_rust_sources() {
        let c = RuntimeConstants::default();
        assert_eq!(
            c.monster_scaling.accuracy_at(85),
            crate::monster::monster_accuracy(85)
        );
        assert_eq!(
            c.monster_scaling.damage_at(85),
            crate::monster::monster_damage(85)
        );
        assert_eq!(
            c.enemy_presets.max_enemy_level,
            crate::monster::MAX_ENEMY_LEVEL
        );
        let pinnacle = c
            .enemy_presets
            .tier_for(crate::monster::EnemyTier::Pinnacle)
            .expect("Pinnacle 档存在");
        assert_eq!(
            pinnacle.armour_mult_pct.value(),
            crate::monster::EnemyTier::Pinnacle.armour_mult_pct()
        );
    }
}
