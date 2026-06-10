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

use crate::monster::{EnemyTier, MAX_ENEMY_LEVEL, MONSTER_BASE_CRIT_DAMAGE_BONUS};

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

impl EnemyPresetsTable {
    /// 按档位稳定 ID 查预设（`None`/`Boss`/`Pinnacle`/`Uber`，与
    /// [`EnemyTier`] 变体名一致）。损坏/缺档数据返回 `None`（消费方自行回退）。
    pub fn tier(&self, id: &str) -> Option<&EnemyTierPreset> {
        self.tiers.iter().find(|p| p.id == id)
    }

    /// 按 [`EnemyTier`] 枚举查预设（`tier(枚举 Debug 名)` 的便捷封装）。
    pub fn tier_for(&self, tier: EnemyTier) -> Option<&EnemyTierPreset> {
        let id = match tier {
            EnemyTier::None => "None",
            EnemyTier::Boss => "Boss",
            EnemyTier::Pinnacle => "Pinnacle",
            EnemyTier::Uber => "Uber",
        };
        self.tier(id)
    }
}

impl EnemyTierPreset {
    /// 从 `enemy_mods` 提取 `DamageTaken MORE` 值（Uber = -70，其余档无此条 → 0）。
    ///
    /// 与旧 Rust 准源 `EnemyTier::damage_taken_more()` 逐值相等
    /// （`load_enemy_presets.rs::uber_damage_taken_matches_rust_source` 已锁）。
    pub fn damage_taken_more(&self) -> f64 {
        self.enemy_mods
            .iter()
            .find(|m| m.name == "DamageTaken" && m.mod_type == "MORE")
            .map(|m| m.value)
            .unwrap_or(0.0)
    }
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

/// `Default` 构造用：拼一条档位 mod。
fn preset_mod(
    name: &str,
    mod_type: &str,
    value: f64,
    source_label: &str,
    effective_only: bool,
) -> EnemyPresetMod {
    EnemyPresetMod {
        name: name.into(),
        mod_type: mod_type.into(),
        value,
        source_label: source_label.into(),
        effective_only,
    }
}

/// Boss/Pinnacle/Uber 三档共通 enemy mod 组（顺序与 `base/enemy_presets.json` 一致）：
/// - Curse/Exposure/Slow `MORE -50`（pobr `setup_env.rs` 已注入，Effective 门控）；
/// - Knockback `MORE -75`、MinimumMovementSpeed `BASE 20`（vendor-only，
///   ConfigOptions.lua L2002/L2004 等）；
/// - `uber_damage_taken`：Uber 专属 `DamageTaken MORE -70`（pobr
///   `EnemyTier::damage_taken_more()`；vendor L2087）；
/// - `PoiseThreshold MORE 500`（pobr 注入，无门控——pobr 现状口径，见模块 doc TODO）
///   + 档位附加 poise 条目（Boss=213 "Map Boss" / Pinnacle/Uber=838 "Xesht"，vendor-only）。
fn boss_enemy_mods(uber_damage_taken: bool, extra_poise: (f64, &str)) -> Vec<EnemyPresetMod> {
    let mut mods = vec![
        preset_mod("CurseEffectOnSelf", "MORE", -50.0, "Unique", true),
        preset_mod("ExposureEffectOnSelf", "MORE", -50.0, "Unique", true),
        preset_mod("KnockbackDistanceOnSelf", "MORE", -75.0, "Unique", true),
        preset_mod("SlowEffectOnSelf", "MORE", -50.0, "Unique", true),
        preset_mod("MinimumMovementSpeed", "BASE", 20.0, "Unique", true),
    ];
    if uber_damage_taken {
        mods.push(preset_mod(
            "DamageTaken",
            "MORE",
            EnemyTier::Uber.damage_taken_more(),
            "Boss",
            false,
        ));
    }
    mods.push(preset_mod("PoiseThreshold", "MORE", 500.0, "Unique", false));
    let (value, label) = extra_poise;
    mods.push(preset_mod("PoiseThreshold", "MORE", value, label, true));
    mods
}

/// Boss/Pinnacle/Uber 三档共通 player mod 组（vendor-only，L2007-2008 等）。
fn boss_player_mods() -> Vec<EnemyPresetMod> {
    vec![
        preset_mod("WarcryPower", "BASE", 20.0, "Boss", false),
        preset_mod("Multiplier:EnemyPower", "BASE", 20.0, "Boss", false),
    ]
}

/// `Default` 构造用：单档预设骨架（pobr 准源标量引用 [`EnemyTier`] 各方法，
/// 零字面量复制；ExactRatio 分量为 vendor 分数书写形，与 JSON 同构）。
#[allow(clippy::too_many_arguments)]
fn tier_preset(
    tier: EnemyTier,
    id: &str,
    label: &str,
    armour_mult_pct: ExactRatio,
    evasion_mult_pct: ExactRatio,
    dps_mult: ExactRatio,
    chaos_damage_div: f64,
    enemy_mods: Vec<EnemyPresetMod>,
    player_mods: Vec<EnemyPresetMod>,
    conditions: &[&str],
) -> EnemyTierPreset {
    EnemyTierPreset {
        id: id.into(),
        label: label.into(),
        is_default: tier == EnemyTier::default(),
        min_level: tier.min_level(),
        elemental_resist_bonus: tier.elemental_resist_bonus(),
        chaos_resist_bonus: tier.chaos_resist_bonus(),
        armour_mult_pct,
        evasion_mult_pct,
        pen: tier.pen(),
        dps_mult,
        chaos_damage_div,
        enemy_mods,
        player_mods,
        conditions: conditions.iter().map(|c| (*c).into()).collect(),
    }
}

/// 无加成倍率（100%；None/Boss 档护甲/闪避）。
const RATIO_100: ExactRatio = ExactRatio {
    base: 100.0,
    num: 0.0,
    den: 1.0,
};

/// fallback（无 GameData 注入时使用）：与 `base/enemy_presets.json` **逐值相等**
/// （搬迁不变式，W2 测试已锁 JSON == Rust 准源）。
///
/// - pobr 准源字段引用 `crate::monster`（`MAX_ENEMY_LEVEL` /
///   `MONSTER_BASE_CRIT_DAMAGE_BONUS` / [`EnemyTier`] 各方法）；
/// - ExactRatio 分量为 vendor 分数书写形（`1/4.40`、`100 + 1100/22` 等，
///   见 [`ExactRatio`] doc——`value()` 与旧 const 逐 bit 相等）；
/// - vendor-only 字段（speed/crit 占位、chaos_damage_div、Knockback/MMS/
///   附加 Poise/player_mods）字面量抄录自 ConfigOptions.lua（行号见各处 doc）。
impl Default for EnemyPresetsTable {
    fn default() -> Self {
        Self {
            max_enemy_level: MAX_ENEMY_LEVEL,
            // pobr 准源：`EnemyTierDefaults::compute` 内联 `damage * 1.5 * dps_mult` 的 1.5。
            ehp_base_damage_mult: 1.5,
            // vendor-only：ConfigOptions.lua L1965 / L1966 占位。
            default_enemy_speed: 700.0,
            default_enemy_crit_chance: 5.0,
            default_enemy_crit_damage_bonus: MONSTER_BASE_CRIT_DAMAGE_BONUS,
            tiers: vec![
                tier_preset(
                    EnemyTier::None,
                    "None",
                    "No",
                    RATIO_100,
                    RATIO_100,
                    // normalEnemyDPSMult = 1/4.40
                    ExactRatio {
                        base: 0.0,
                        num: 1.0,
                        den: 4.4,
                    },
                    2.5,
                    Vec::new(),
                    Vec::new(),
                    &[],
                ),
                tier_preset(
                    EnemyTier::Boss,
                    "Boss",
                    "Standard Boss",
                    RATIO_100,
                    RATIO_100,
                    // stdBossDPSMult = 4/4.40
                    ExactRatio {
                        base: 0.0,
                        num: 4.0,
                        den: 4.4,
                    },
                    2.5,
                    boss_enemy_mods(false, (213.0, "Map Boss")),
                    boss_player_mods(),
                    &["Unique", "RareOrUnique"],
                ),
                tier_preset(
                    EnemyTier::Pinnacle,
                    "Pinnacle",
                    "Guardian/Pinnacle Boss",
                    // PinnacleArmourMean = 100 + 1100/22（推导见 monster.rs 常量注释）
                    ExactRatio {
                        base: 100.0,
                        num: 1100.0,
                        den: 22.0,
                    },
                    // PinnacleEvasionMean = 100 + 548/22
                    ExactRatio {
                        base: 100.0,
                        num: 548.0,
                        den: 22.0,
                    },
                    // pinnacleBossDPSMult = 8/4.40
                    ExactRatio {
                        base: 0.0,
                        num: 8.0,
                        den: 4.4,
                    },
                    2.5,
                    boss_enemy_mods(false, (838.0, "Xesht")),
                    boss_player_mods(),
                    &["Unique", "RareOrUnique", "PinnacleBoss"],
                ),
                tier_preset(
                    EnemyTier::Uber,
                    "Uber",
                    "Uber Pinnacle Boss",
                    // UberArmourMean = 100 + 175/7
                    ExactRatio {
                        base: 100.0,
                        num: 175.0,
                        den: 7.0,
                    },
                    // UberEvasionMean = 100 + 116/7
                    ExactRatio {
                        base: 100.0,
                        num: 116.0,
                        den: 7.0,
                    },
                    // uberBossDPSMult = 10/4.25
                    ExactRatio {
                        base: 0.0,
                        num: 10.0,
                        den: 4.25,
                    },
                    4.0,
                    boss_enemy_mods(true, (838.0, "Xesht")),
                    boss_player_mods(),
                    &["Unique", "RareOrUnique", "PinnacleBoss"],
                ),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// fallback 不变式：Default 各档标量与旧 Rust 准源（`EnemyTier` 各方法）逐值相等，
    /// ExactRatio `value()` 与旧 const 逐 bit 相等
    /// （Default == JSON 的完整对照在 `pobr-gamedata` ruleset 测试）。
    #[test]
    fn default_tier_scalars_match_enemy_tier_methods() {
        let t = EnemyPresetsTable::default();
        assert_eq!(t.max_enemy_level, MAX_ENEMY_LEVEL);
        assert_eq!(
            t.default_enemy_crit_damage_bonus,
            MONSTER_BASE_CRIT_DAMAGE_BONUS
        );
        for tier in [
            EnemyTier::None,
            EnemyTier::Boss,
            EnemyTier::Pinnacle,
            EnemyTier::Uber,
        ] {
            let p = t.tier_for(tier).expect("四档齐全");
            assert_eq!(p.min_level, tier.min_level(), "{tier:?} min_level");
            assert_eq!(
                p.elemental_resist_bonus,
                tier.elemental_resist_bonus(),
                "{tier:?} elemental_resist_bonus"
            );
            assert_eq!(
                p.chaos_resist_bonus,
                tier.chaos_resist_bonus(),
                "{tier:?} chaos_resist_bonus"
            );
            assert_eq!(
                p.armour_mult_pct.value(),
                tier.armour_mult_pct(),
                "{tier:?} armour_mult_pct 逐 bit"
            );
            assert_eq!(
                p.evasion_mult_pct.value(),
                tier.evasion_mult_pct(),
                "{tier:?} evasion_mult_pct 逐 bit"
            );
            assert_eq!(p.pen, tier.pen(), "{tier:?} pen");
            assert_eq!(
                p.dps_mult.value(),
                tier.dps_mult(),
                "{tier:?} dps_mult 逐 bit"
            );
            assert_eq!(
                p.damage_taken_more(),
                tier.damage_taken_more(),
                "{tier:?} damage_taken_more"
            );
            assert_eq!(
                p.is_default,
                tier == EnemyTier::default(),
                "{tier:?} 默认位"
            );
        }
    }

    /// Default 档位顺序与 vendor list / `EnemyTier` 枚举序一致。
    #[test]
    fn default_tiers_in_canonical_order() {
        let t = EnemyPresetsTable::default();
        let ids: Vec<&str> = t.tiers.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["None", "Boss", "Pinnacle", "Uber"]);
    }
}
