//! 敌人 modDB 初始化（对齐 PoB2 `CalcSetup.lua` 的 `enemyDB` 注入段）。
//!
//! 把怪物等级缩放表（`pobr_data::monster`）+ [`EnemyTier`] 档位加成写入
//! `Env.enemy.mod_db`，归因到 [`SourceKind::EnemyConfig`]。所有注入都是 BASE/MORE
//! modifier，由进攻计算（`offence.rs`）在 `mode_effective` 口径下读取。
//!
//! 设计要点（doc12 §4.2 / §5、accuracy-and-enemy.md §四,§五,§六）：
//! - **怪物缩放**：`accuracy/evasion/armour` 来自 [`EnemyTierDefaults`]（已含档位倍率）。
//! - **抗性**：`{Fire/Cold/Lightning}Resist BASE`、`ChaosResist BASE`。
//! - **Uber 受伤惩罚**：`DamageTaken MORE -70`。
//! - **Boss 通用 debuff 抗性**：`CurseEffectOnSelf/ExposureEffectOnSelf/SlowEffectOnSelf
//!   MORE -50` 等，削弱我方诅咒/曝光/减速对 Boss 的有效度。
//! - **条件态**：Boss → `Condition:Unique`/`RareOrUnique`；Pinnacle/Uber → `Condition:PinnacleBoss`。
//! - **穿透**：`tier.pen()` 注入 **player** modDB 的 `<Element>Penetration BASE`（boss 自带穿透
//!   是作用在玩家伤害上的减抗，归因仍记 `EnemyConfig`）。
//! - **玩家施加的 debuff（曝光/诅咒/破甲/凋萎）通道**：本步只提供归约 hook
//!   [`reduce_enemy_exposure`]（曝光取最强 → 写入 `*Resist BASE` 减项），具体 debuff
//!   注入由下游 wave 在调用 [`setup_enemy`] 后追加再调 [`reduce_enemy_exposure`]。

use pobr_data::prelude::*;

use crate::{ModDb, Modifier};

use super::{Actor, ActorBaseStats, Env};

/// `EnemyConfig` 归因来源（统一 id 前缀，便于 TraceGraph 区分敌人天生属性 vs 我方 debuff）。
fn enemy_source(id: &str) -> ModifierSource {
    ModifierSource::new(SourceId::new(SourceKind::EnemyConfig, id))
}

/// 给 enemy modDB 注入一条带 `EnemyConfig` 归因的数值 modifier。
fn push_enemy_number(db: &mut ModDb, name: &str, mod_type: ModType, value: f64, id: &str) {
    db.add_mod(
        Modifier::number(ModName::from(name), mod_type, value)
            .with_source(format!("enemy {id}"))
            .with_origin(enemy_source(id)),
    );
}

/// 给 enemy modDB 注入一个布尔条件态（`Condition:<name>`）。
fn push_enemy_condition(db: &mut ModDb, condition: &str, id: &str) {
    db.add_mod(
        Modifier::number(
            ModName::from(format!("Condition:{condition}")),
            ModType::Flag,
            1.0,
        )
        .with_source(format!("enemy {id}"))
        .with_origin(enemy_source(id)),
    );
}

/// 按 `(enemy_level, tier)` 初始化 `Env.enemy`：写 `enemy.base`（标量兼容入口）+
/// `enemy.mod_db`（完整 modifier）。
///
/// `config_level`：用户配置的怪物等级（`0` 表示跟随角色等级，调用方先解析为具体值）。
/// 当 `config_level == 0` 时回退为 `min(MAX_ENEMY_LEVEL, player.level)`。
pub fn setup_enemy(env: &mut Env, config_level: u32, tier: EnemyTier) {
    let resolved_level = if config_level == 0 {
        (env.player.level as u32).min(MAX_ENEMY_LEVEL)
    } else {
        config_level
    };
    let defaults = EnemyTierDefaults::compute(resolved_level, tier);

    // --- 标量基础值（保持 perform/offence 现有标量入口的兼容） ---
    let base = ActorBaseStats {
        accuracy: defaults.accuracy as f64,
        evasion: defaults.evasion,
        armour: defaults.armour,
        fire_resistance: defaults.elemental_resist,
        cold_resistance: defaults.elemental_resist,
        lightning_resistance: defaults.elemental_resist,
        ..ActorBaseStats::default()
    };

    let mut enemy = Actor::new(defaults.level.max(1) as u8, base);
    inject_enemy_mods(&mut enemy.mod_db, &defaults, tier);
    env.enemy = enemy;
}

/// 把 [`EnemyTierDefaults`] + 档位加成写入 enemy modDB（不触碰 base 标量）。
fn inject_enemy_mods(db: &mut ModDb, defaults: &EnemyTierDefaults, tier: EnemyTier) {
    // 怪物缩放：精准 / 闪避 / 护甲（含档位倍率，已在 defaults 中乘好）。
    push_enemy_number(
        db,
        "Accuracy",
        ModType::Base,
        defaults.accuracy as f64,
        "accuracy",
    );
    push_enemy_number(db, "Evasion", ModType::Base, defaults.evasion, "evasion");
    push_enemy_number(db, "Armour", ModType::Base, defaults.armour, "armour");

    // 元素抗性（Boss 档位加成）。
    if defaults.elemental_resist != 0.0 {
        push_enemy_number(
            db,
            "FireResist",
            ModType::Base,
            defaults.elemental_resist,
            "fire_resist",
        );
        push_enemy_number(
            db,
            "ColdResist",
            ModType::Base,
            defaults.elemental_resist,
            "cold_resist",
        );
        push_enemy_number(
            db,
            "LightningResist",
            ModType::Base,
            defaults.elemental_resist,
            "lightning_resist",
        );
    }
    if defaults.chaos_resist != 0.0 {
        push_enemy_number(
            db,
            "ChaosResist",
            ModType::Base,
            defaults.chaos_resist,
            "chaos_resist",
        );
    }

    // Uber：DamageTaken MORE -70（受伤减少）。
    if defaults.damage_taken_more != 0.0 {
        push_enemy_number(
            db,
            "DamageTaken",
            ModType::More,
            defaults.damage_taken_more,
            "uber_damage_taken",
        );
    }

    // Boss 通用 debuff 抗性（Boss/Pinnacle/Uber 共有；accuracy-and-enemy.md §五）。
    if tier.is_boss() {
        push_enemy_number(
            db,
            "CurseEffectOnSelf",
            ModType::More,
            -50.0,
            "boss_curse_effect",
        );
        push_enemy_number(
            db,
            "ExposureEffectOnSelf",
            ModType::More,
            -50.0,
            "boss_exposure_effect",
        );
        push_enemy_number(
            db,
            "SlowEffectOnSelf",
            ModType::More,
            -50.0,
            "boss_slow_effect",
        );
        push_enemy_number(
            db,
            "PoiseThreshold",
            ModType::More,
            500.0,
            "boss_poise_threshold",
        );
        push_enemy_condition(db, "Unique", "boss_unique");
        push_enemy_condition(db, "RareOrUnique", "boss_rare_or_unique");
    }
    if tier.is_pinnacle_or_uber() {
        push_enemy_condition(db, "PinnacleBoss", "pinnacle_boss");
    }
}

/// 曝光取最强（PoB2 `ExposureMin` 逻辑）：把 enemy modDB 内各元素的 `<Element>Exposure BASE`
/// 多来源归约为**最强一份**，并写入对应 `<Element>Resist BASE -magnitude`。
///
/// 调用时机：下游 wave 把玩家施加的曝光 debuff（`FireExposure BASE 20` 等）注入
/// enemy modDB **之后**调用本函数完成归约。曝光约定为正数 magnitude（如 `20`），
/// 写入 `*Resist BASE` 时取负。归因记到产生最强曝光那条 modifier 的 `SourceId`
/// （若可得），否则记 `EnemyConfig`。
///
/// 出处：agent-docs/debuffs.md §曝光（`magnitude = max(magnitude, value)`）；
///       devs/docs/architecture/12-combat-mechanics-architecture.md §4.2。
pub fn reduce_enemy_exposure(db: &mut ModDb, cfg: &crate::CalcConfig) {
    for (exposure_name, resist_name) in [
        ("FireExposure", "FireResist"),
        ("ColdExposure", "ColdResist"),
        ("LightningExposure", "LightningResist"),
    ] {
        let magnitude = db.max_of(ModType::Base, cfg, &[ModName::from(exposure_name)]);
        if magnitude > 0.0 {
            db.add_mod(
                Modifier::number(ModName::from(resist_name), ModType::Base, -magnitude)
                    .with_source(format!("exposure {exposure_name}"))
                    .with_origin(
                        enemy_source("exposure")
                            .with_parent(SourceId::new(SourceKind::EnemyConfig, exposure_name)),
                    ),
            );
        }
    }
}

/// 便捷构造：从 player [`Actor`] 起一个完整 `Env`（player + enemy 缩放 + cfg）。
///
/// 注意：本函数构造 enemy 用 [`setup_enemy`]，对玩家 modDB 不做任何注入。
pub fn env_with_enemy(player: Actor, config_level: u32, tier: EnemyTier) -> Env {
    let mut env = Env::new(player);
    setup_enemy(&mut env, config_level, tier);
    env
}
