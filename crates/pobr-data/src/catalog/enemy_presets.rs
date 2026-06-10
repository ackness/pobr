//! 敌人档位预设 schema（`base/enemy_presets.json`）。
//!
//! 对应 PoB2 `src/Modules/ConfigOptions.lua` 的 `enemyIsBoss` 四档配置
//! （vendor commit `2df5a74`，段落 L1963-2121）：每档一组 enemy/player modifier
//! 注入 + per-type damage/pen/resist 默认列；倍率常量来自 `src/Modules/Data.lua`
//! `data.misc`/`data.bossStats`。
//!
//! pobr 准源（迁出前，搬迁不变式——JSON 与下列 Rust 值逐值相等）：
//!
//! | JSON 字段 | pobr 准源 | vendor 来源 |
//! |---|---|---|
//! | `max_enemy_level` | `monster.rs::MAX_ENEMY_LEVEL` (85) | Data.lua `data.misc.MaxEnemyLevel` |
//! | `ehp_base_damage_mult` | `monster.rs::EnemyTierDefaults::compute` 内联 `1.5` | ConfigOptions.lua L1982/L2023/L2065/L2106 `monsterDamageTable[lv] * 1.5 * DPSMult` |
//! | `default_enemy_crit_damage_bonus` | `monster.rs::MONSTER_BASE_CRIT_DAMAGE_BONUS` (30) | ConfigOptions.lua L1967（`data.monsterConstants["base_critical_hit_damage_bonus"]`） |
//! | `tiers[].min_level` | `EnemyTier::min_level()`（Pinnacle/Uber = `PINNACLE_MIN_LEVEL` 82） | ConfigOptions.lua `defaultLevel = 82` + `m_max(...)` |
//! | `tiers[].elemental_resist_bonus` | `EnemyTier::elemental_resist_bonus()` (0/30/50/50) | ConfigOptions.lua 各档 `defaultEleResist` |
//! | `tiers[].chaos_resist_bonus` | `EnemyTier::chaos_resist_bonus()` (恒 0) | ConfigOptions.lua 各档 `enemyChaosResist` 占位 0 |
//! | `tiers[].armour_mult_pct` | `EnemyTier::armour_mult_pct()`（含 `PINNACLE_ARMOUR_MEAN`/`UBER_ARMOUR_MEAN`，PoE1 Bosses.lua 均值占位） | Data.lua `data.bossStats.*ArmourMean` |
//! | `tiers[].evasion_mult_pct` | `EnemyTier::evasion_mult_pct()`（同上均值占位） | Data.lua `data.bossStats.*EvasionMean` |
//! | `tiers[].pen` | `EnemyTier::pen()` (0/0/3/8) | Data.lua `pinnacleBossPen = 15/5`、`uberBossPen = 40/5` |
//! | `tiers[].dps_mult` | `EnemyTier::dps_mult()` (1/4.40, 4/4.40, 8/4.40, 10/4.25) | Data.lua `normalEnemyDPSMult` 等四常量 |
//! | `tiers[].enemy_mods` 中 pobr 已注入条目 | `setup_env.rs::inject_enemy_mods`（Curse/Exposure/Slow -50、PoiseThreshold 500、Uber DamageTaken -70） | ConfigOptions.lua L2000-2006 / L2042-2048 / L2082-2089 |
//! | `tiers[].conditions` | `setup_env.rs`（Unique/RareOrUnique；Pinnacle/Uber 加 PinnacleBoss） | ConfigOptions.lua L1998-1999 / L2039-2041 / L2079-2081 |
//!
//! vendor-only 字段（pobr 此前未实现，自 vendor 抽取，行号见各字段 doc）：
//! - `default_enemy_speed`（700，L1965）、`default_enemy_crit_chance`（5，L1966）；
//! - `tiers[].chaos_damage_div`（None/Boss/Pinnacle = 2.5（L1987/L2028/L2070），Uber = 4（L2111））
//!   ——per-type damage 默认列中混沌伤害对 `defaultDamage` 的除数；
//! - `enemy_mods` 中 `KnockbackDistanceOnSelf MORE -75`、`MinimumMovementSpeed BASE 20`、
//!   `PoiseThreshold MORE 213 (Map Boss)` / `838 (Xesht)`；
//! - `player_mods`（`WarcryPower BASE 20`、`Multiplier:EnemyPower BASE 20`，L2007-2008 等）。
//!
//! 已知 pobr ↔ vendor 行为出入（**本表只记录、不改值**，行为对齐是后续独立 commit）：
//! - TODO(parity): vendor 给 `Condition:Unique/RareOrUnique/PinnacleBoss` 与
//!   `PoiseThreshold MORE 500` 均挂 `Condition:Effective` 门控；pobr `setup_env.rs`
//!   当前对这两类**不带** Effective 门控（仅 Curse/Exposure/Slow 三项带）。
//!   `effective_only` 字段按 pobr 现状落值（PoiseThreshold 500 = false），
//!   vendor-only 条目按 vendor 落值。
//! - TODO(parity): vendor 的 per-type damage 默认 `round(damageTable[lv] * 1.5 * DPSMult)`
//!   有取整；pobr `EnemyTierDefaults::base_damage_for_ehp` 不取整。
//! - TODO(parity): vendor 把档位穿透注入 per-element `enemy{Fire,Cold,Lightning}Pen`；
//!   pobr 合并注入 player modDB 单一 `ElementalPenetration BASE`（语义等价，结构不同）。

use serde::{Deserialize, Serialize};

/// 以「偏移 + 分子/分母」表达的精确 f64 值：`value = base + num / den`。
///
/// 两个动机：
/// 1. **vendor 同构**——PoB2 源码即以分数书写这些常量（Data.lua
///    `stdBossDPSMult = 4/4.40`；bossStats 均值 = `100 + Σmult/数量`，
///    见 `monster.rs` 常量注释的推导）；
/// 2. **bit 级精确**——`1/4.4` 等值的最短十进制表示需 17 位有效数字，
///    serde_json 默认浮点解析（未开 `float_roundtrip` feature）对其有 1-ulp
///    误差；分量（4.0 / 4.4 / 548.0 / 22.0 …）均为短十进制，解析无损，
///    [`Self::value`] 在 Rust 侧重算除法即得与 pobr 准源逐 bit 相等的 f64。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ExactRatio {
    /// 加法偏移（无偏移时为 0）。
    pub base: f64,
    /// 分子。
    pub num: f64,
    /// 分母（不得为 0）。
    pub den: f64,
}

impl ExactRatio {
    /// 求值：`base + num / den`（与 pobr 准源常量的定义表达式同序，bit 级一致）。
    pub fn value(&self) -> f64 {
        self.base + self.num / self.den
    }
}

/// 敌人档位预设表（`enemyIsBoss` 四档 + 全档位公共默认）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnemyPresetsTable {
    /// 普通怪/Boss 的最大敌人等级（PoB2 `data.misc.MaxEnemyLevel`；pobr `MAX_ENEMY_LEVEL`）。
    pub max_enemy_level: u32,
    /// EHP 用基础伤害倍率：`damage = monsterDamageTable[lv] * ehp_base_damage_mult * dps_mult`
    /// （ConfigOptions.lua L1982 等内联 `1.5`；pobr `EnemyTierDefaults::compute` 同值内联）。
    pub ehp_base_damage_mult: f64,
    /// 敌人攻击间隔默认占位（ConfigOptions.lua L1965 `enemySpeed` placeholder = 700，
    /// 单位 ms；vendor-only，pobr 暂无消费）。
    pub default_enemy_speed: f64,
    /// 敌人暴击几率默认占位（%；ConfigOptions.lua L1966 `enemyCritChance` placeholder = 5；
    /// vendor-only，pobr 从 enemy modDB 聚合、无写死默认）。
    pub default_enemy_crit_chance: f64,
    /// 敌人基础爆伤加成默认（%；ConfigOptions.lua L1967 ←
    /// `data.monsterConstants["base_critical_hit_damage_bonus"]`；
    /// pobr 准源 `monster.rs::MONSTER_BASE_CRIT_DAMAGE_BONUS = 30`）。
    pub default_enemy_crit_damage_bonus: f64,
    /// 四档预设，顺序固定 None → Boss → Pinnacle → Uber
    /// （与 vendor list 顺序及 pobr `EnemyTier` 枚举序一致）。
    pub tiers: Vec<EnemyTierPreset>,
}

/// 单个敌人档位预设（`enemyIsBoss` 的一档）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnemyTierPreset {
    /// 档位稳定 ID（vendor list `val`：`None`/`Boss`/`Pinnacle`/`Uber`，
    /// 与 pobr `EnemyTier` 变体名一致）。
    pub id: String,
    /// vendor list 显示标签（如 `Guardian/Pinnacle Boss`）。
    pub label: String,
    /// 是否为默认档位（vendor `defaultIndex = 3` → Pinnacle；pobr `EnemyTier::default()`）。
    pub is_default: bool,
    /// 默认/最低怪物等级下界（Pinnacle/Uber = 82，其余 1；pobr `EnemyTier::min_level()`）。
    pub min_level: u32,
    /// 元素抗性加成（%，BASE；pobr `EnemyTier::elemental_resist_bonus()`）。
    pub elemental_resist_bonus: f64,
    /// 混沌抗性加成（%；vendor 占位 0，pobr `EnemyTier::chaos_resist_bonus()` 恒 0）。
    pub chaos_resist_bonus: f64,
    /// 护甲倍率（%，100 = 不加成；pobr `EnemyTier::armour_mult_pct()`，
    /// Pinnacle/Uber 为 PoE1 Bosses.lua 均值占位：`100 + 1100/22`、`100 + 175/7`，
    /// 推导见 `monster.rs` 常量注释）。
    pub armour_mult_pct: ExactRatio,
    /// 闪避倍率（%；pobr `EnemyTier::evasion_mult_pct()`；
    /// Pinnacle/Uber 均值 `100 + 548/22`、`100 + 116/7`）。
    pub evasion_mult_pct: ExactRatio,
    /// 元素穿透（%；pobr `EnemyTier::pen()`，注入口径差异见模块 doc TODO）。
    pub pen: f64,
    /// EHP 用 DPS 倍率（pobr `EnemyTier::dps_mult()`；vendor `data.misc.*DPSMult`，
    /// 分数书写 `1/4.40`、`4/4.40`、`8/4.40`、`10/4.25`）。
    pub dps_mult: ExactRatio,
    /// per-type damage 默认列中混沌伤害除数：`chaosDamage = round(defaultDamage / 此值)`
    /// （vendor-only：None/Boss/Pinnacle = 2.5，Uber = 4，L1987/L2028/L2070/L2111；
    /// 物理/火/冰/雷四类直接取 `defaultDamage` 不除）。
    pub chaos_damage_div: f64,
    /// 注入 enemy modDB 的档位 mod 组（含 pobr 已实现与 vendor-only 条目，见模块 doc）。
    pub enemy_mods: Vec<EnemyPresetMod>,
    /// 注入 player modDB 的档位 mod 组（vendor-only：WarcryPower/Multiplier:EnemyPower）。
    pub player_mods: Vec<EnemyPresetMod>,
    /// 注入 enemy modDB 的布尔条件态（`Condition:<名>`；pobr `setup_env.rs` 同名注入）。
    pub conditions: Vec<String>,
}

/// 档位 mod 组中的一条 modifier。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnemyPresetMod {
    /// ModName（如 `CurseEffectOnSelf`）。
    pub name: String,
    /// mod 类型（`BASE` / `MORE`，沿用 vendor 字面）。
    pub mod_type: String,
    /// 数值。
    pub value: f64,
    /// vendor 来源标签（NewMod 第 4 参：`Unique`/`Map Boss`/`Xesht`/`Boss`），
    /// 仅溯源用，不参与计算。
    pub source_label: String,
    /// 是否仅在有效 DPS 口径（`Condition:Effective`）下生效。
    /// pobr 已实现条目按 `setup_env.rs` 现状落值，vendor-only 条目按 vendor 落值
    /// （两侧门控口径差异见模块 doc TODO）。
    pub effective_only: bool,
}
