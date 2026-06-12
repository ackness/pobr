//! 敌人 modDB 初始化（对齐 PoB2 `CalcSetup.lua` 的 `enemyDB` 注入段）。
//!
//! 把怪物等级缩放表 + [`EnemyTier`] 档位加成写入 `Env.enemy.mod_db`，归因到
//! [`SourceKind::EnemyConfig`]。所有注入都是 BASE/MORE modifier，由进攻计算
//! （`offence.rs`）在 `mode_effective` 口径下读取。
//!
//! **数据来源（M0-W3）**：百级表与档位预设改读注入的
//! [`RuntimeConstants`]（`cfg.constants.monster_scaling` / `.enemy_presets`，
//! 来自 `base/monster_scaling.json` + `base/enemy_presets.json`）；无 GameData 时
//! 走 `Default` fallback（与 JSON 逐值相等，引用旧 `pobr_data::monster` 准源）——
//! 两条路径输出逐值一致（搬迁不变式）。
//!
//! 设计要点（doc12 §4.2 / §5、accuracy-and-enemy.md §四,§五,§六）：
//! - **怪物缩放**：`accuracy/evasion/armour` 来自 [`EnemyTierDefaults`]（已含档位倍率）。
//! - **抗性**：`{Fire/Cold/Lightning}Resist BASE`、`ChaosResist BASE`。
//! - **Uber 受伤惩罚**：`DamageTaken MORE -70`。
//! - **Boss 通用 debuff 抗性**：`CurseEffectOnSelf/ExposureEffectOnSelf/SlowEffectOnSelf
//!   MORE -50` 等，削弱我方诅咒/曝光/减速对 Boss 的有效度。
//! - **条件态**：Boss → `Condition:Unique`/`RareOrUnique`；Pinnacle/Uber → `Condition:PinnacleBoss`。
//! - **穿透**：`tier.pen()` 仅注入 enemy modDB 的 `Enemy<Element>Pen BASE`（防御侧
//!   EHP/受击消费，vendor CalcDefence.lua:2363）；**不**进玩家进攻穿透（M4-H S1）。
//! - **玩家施加的 debuff（曝光/诅咒/破甲/凋萎）通道**：本步只提供归约 hook
//!   [`reduce_enemy_exposure`]（曝光取最强 → 写入 `*Resist BASE` 减项），具体 debuff
//!   注入由下游 wave 在调用 [`setup_enemy`] 后追加再调 [`reduce_enemy_exposure`]。

use pobr_data::prelude::*;

use crate::{ModDb, ModTag, Modifier};

use super::{Actor, Env};

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

/// 给 enemy modDB 注入一条带 `EnemyConfig` 归因、且仅在有效 DPS 口径生效的数值 modifier。
///
/// 附 `Condition:Effective` 标签（由 [`CalcConfig::condition`](crate::CalcConfig::condition)
/// 从 `mode_effective` 派生）：面板口径（`mode_effective == false`）下这些敌侧 debuff-抗
/// （curse/exposure/slow effect-on-self）不参与聚合，避免污染裸 DPS。
fn push_enemy_effective_number(
    db: &mut ModDb,
    name: &str,
    mod_type: ModType,
    value: f64,
    id: &str,
) {
    db.add_mod(
        Modifier::number(ModName::from(name), mod_type, value)
            .with_source(format!("enemy {id}"))
            .with_origin(enemy_source(id))
            .with_tag(ModTag::condition("Effective", false)),
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

/// 从注入的运行时常量包计算档位默认值（`cfg.constants` 的 `monster_scaling` +
/// `enemy_presets`），替代旧 `EnemyTierDefaults::compute` 对 `pobr_data::monster`
/// 硬编码表的直查。
///
/// 公式与旧路径**逐运算同序**（level 先档位下界后 MaxEnemyLevel 上界、倍率先除
/// 100 再乘表值），且 `Default` fallback 与 JSON 逐值相等——两条路径输出逐 bit
/// 一致（搬迁不变式，零 parity 变化）。
///
/// 注入数据缺档（损坏的 enemy_presets）时回退旧 Rust 准源 `compute`（与 Default
/// 同值），保持零 I/O 可计算。
fn tier_defaults_from_constants(
    constants: &RuntimeConstants,
    config_level: u32,
    tier: EnemyTier,
) -> EnemyTierDefaults {
    let presets = &constants.enemy_presets;
    let Some(preset) = presets.tier_for(tier) else {
        return EnemyTierDefaults::compute(config_level, tier);
    };
    let scaling = &constants.monster_scaling;
    // 保证等级满足档位最低要求，并 clamp 到 MaxEnemyLevel（与旧 compute 同序）。
    let level = config_level
        .max(preset.min_level)
        .min(presets.max_enemy_level);
    let armour_mult = preset.armour_mult_pct.value() / 100.0;
    let evasion_mult = preset.evasion_mult_pct.value() / 100.0;
    EnemyTierDefaults {
        level,
        accuracy: scaling.accuracy_at(level),
        evasion: scaling.evasion_at(level) as f64 * evasion_mult,
        armour: scaling.armour_at(level) as f64 * armour_mult,
        life: scaling.life_at(level),
        elemental_resist: preset.elemental_resist_bonus,
        chaos_resist: preset.chaos_resist_bonus,
        pen: preset.pen,
        damage_taken_more: preset.damage_taken_more(),
        base_damage_for_ehp: scaling.damage_at(level)
            * presets.ehp_base_damage_mult
            * preset.dps_mult.value(),
    }
}

/// 按 `(enemy_level, tier)` 初始化 `Env.enemy`：写 `enemy.base`（标量兼容入口）+
/// `enemy.mod_db`（完整 modifier）。
///
/// `config_level`：用户配置的怪物等级（`0` 表示跟随角色等级，调用方先解析为具体值）。
/// 当 `config_level == 0` 时回退为 `min(MaxEnemyLevel, player.level)`（上限读注入的
/// `cfg.constants.enemy_presets.max_enemy_level`，Default fallback = 85）。
///
/// 数据来源：百级表/档位预设读 `env.cfg.constants`（M0-W3 注入管道）——调用方须在
/// `set_constants` **之后**调用本函数（`calculate_with_data` 已遵守此序；无注入时
/// Default fallback 与 JSON 逐值相等，输出不变）。
pub fn setup_enemy(env: &mut Env, config_level: u32, tier: EnemyTier) {
    let constants = &env.cfg.constants;
    let resolved_level = if config_level == 0 {
        (env.player.level as u32).min(constants.enemy_presets.max_enemy_level)
    } else {
        config_level
    };
    let defaults = tier_defaults_from_constants(constants, resolved_level, tier);

    // --- 就地更新 env.enemy（对齐 PoB2 CalcSetup.lua:682-691：enemyDB 是持久增量 db，
    // 不整体替换 actor）。base 标量按档位缩放写入；档位 mod **追加**进现有 mod_db，从而
    // 保留 setup_enemy 之前已注入的 enemy mod（曝光/物理减伤/用户自定义 enemy 词条等）。
    env.enemy.level = defaults.level.max(1) as u8;
    env.enemy.base.accuracy = defaults.accuracy as f64;
    env.enemy.base.evasion = defaults.evasion;
    env.enemy.base.armour = defaults.armour;
    env.enemy.base.fire_resistance = defaults.elemental_resist;
    env.enemy.base.cold_resistance = defaults.elemental_resist;
    env.enemy.base.lightning_resistance = defaults.elemental_resist;
    inject_enemy_mods(&mut env.enemy.mod_db, &defaults, tier);
    inject_ehp_damage_placeholder(&mut env.enemy.mod_db, constants, defaults.level, tier);

    // 注意：Boss 自带元素穿透（Pinnacle 3 / Uber 8，vendor `pinnacleBossPen = 15/5` /
    // `uberBossPen = 40/5`，Modules/Data.lua:231/:233）**只作用在防御侧**——
    // `enemy{Fire,Cold,Lightning}Pen` config var 没有 apply 函数（ConfigOptions.lua:2269-2273，
    // 不生成任何 mod），仅被 CalcDefence.lua:2363 读取折算玩家受击抗性
    // （`resMult = 1 − max(resist − enemyPen, 0)/100`）。对应 PoBR 通道 =
    // `inject_ehp_damage_placeholder` 注入的 `Enemy{Fire,Cold,Lightning}Pen`
    // （`ehp::fill_ehp_pob2` 消费）。历史版本曾把它同时注入玩家 modDB 的
    // `ElementalPenetration BASE`（提高我方进攻穿透）——vendor 进攻侧
    // CalcOffence.lua:4143 的 pen 只读玩家 skillModList，无任何 boss 来源；
    // 该注入是反向假补偿，已删除（M4-H S1）。
}

/// 把 [`EnemyTierDefaults`] + 档位加成写入 enemy modDB（不触碰 base 标量）。
///
/// 注意：Boss 共通 mod 组（Curse/Exposure/Slow `-50`、`PoiseThreshold 500`、条件态）
/// 此处仍按 pobr 现状硬编码注入——`enemy_presets.json` 的 `tiers[].enemy_mods` 里
/// 额外含 vendor-only 条目（Knockback/MinimumMovementSpeed/附加 Poise/player_mods），
/// 本 wave 为搬迁不变式（parity 零变化）**不**改为整组数据驱动；行为对齐
/// （含 Effective 门控口径差异）见 `enemy_presets.rs` 模块 doc 的 TODO(parity)，
/// 属后续独立 commit。
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
    // 这三项 effect-on-self 削弱我方诅咒/曝光/减速对 Boss 的有效度，仅在有效 DPS 口径
    // （`mode_effective`）参与消费——故带 `Condition:Effective` 门控。
    if tier.is_boss() {
        push_enemy_effective_number(
            db,
            "CurseEffectOnSelf",
            ModType::More,
            -50.0,
            "boss_curse_effect",
        );
        push_enemy_effective_number(
            db,
            "ExposureEffectOnSelf",
            ModType::More,
            -50.0,
            "boss_exposure_effect",
        );
        push_enemy_effective_number(
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

/// EHP 进伤 placeholder 注入（M2 F-1）：把 vendor ConfigOptions.lua:1982-1996 的
/// 敌人单击伤害默认占位（`enemy<X>Damage` config placeholder）落成 enemy modDB 的
/// `Enemy<X>Damage` BASE——`default = round(monsterDamageTable[lv] ×
/// ehp_base_damage_mult × DPSMult)`，chaos 再 `round(/chaos_damage_div)`
/// （数值装配在 `ehp::enemy_damage_placeholder`）。
///
/// 行为中性：注入的 ModName 当前仅被 EHP 新管线（`ehp::assemble_enemy_damage`）
/// 消费，全部产出挂新字段——既有输出 parity 逐值不变。M3 config_interpreter 接管
/// `enemy<X>Damage` configInput 后，本注入退化为无 config 时的 placeholder 路径。
fn inject_ehp_damage_placeholder(
    db: &mut ModDb,
    constants: &RuntimeConstants,
    level: u32,
    tier: EnemyTier,
) {
    let damage = super::ehp::enemy_damage_placeholder(constants, level, tier);
    for (name, value) in [
        ("EnemyPhysicalDamage", damage.physical),
        ("EnemyFireDamage", damage.fire),
        ("EnemyColdDamage", damage.cold),
        ("EnemyLightningDamage", damage.lightning),
        ("EnemyChaosDamage", damage.chaos),
    ] {
        if value > 0.0 {
            push_enemy_number(db, name, ModType::Base, value, "ehp_damage_placeholder");
        }
    }
    // 敌方元素穿透 placeholder（vendor ConfigOptions.lua:2072-2074 / :2113-2115：
    // Pinnacle/Uber 预设把 `enemy{Lightning,Cold,Fire}Pen` 的 config placeholder 置为
    // `pinnacleBossPen = 15/5 = 3` / `uberBossPen = 40/5 = 8`，Modules/Data.lua:231/:233；
    // 防御侧消费在 CalcDefence.lua:2328（EnemyCannotPen 门控）/:2363
    // `resMult = 1 − max(resist − enemyPen, 0)/100`）。数据走 enemy_presets.json
    // `tiers[].pen`；chaos/physical 无 pen（vendor 预设仅设三元素）。
    // 仅 EHP 新管线（`ehp::fill_ehp_pob2`）消费本组 ModName。
    let presets = &constants.enemy_presets;
    let pen = presets.tier_for(tier).map_or(0.0, |preset| preset.pen);
    if pen != 0.0 {
        for name in ["EnemyFirePen", "EnemyColdPen", "EnemyLightningPen"] {
            push_enemy_number(db, name, ModType::Base, pen, "boss_ele_pen_placeholder");
        }
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
    // PoB2 `CalcPerform.lua:3225` `exposureEffectOnSelf = enemyDB:More(nil, "ExposureEffectOnSelf")`：
    // 曝光 magnitude 先乘该 effect-on-self（Boss `MORE -50` → 0.5）再折入抗性。该 mod 带
    // `Condition:Effective` 门控，面板口径（`mode_effective == false`）不匹配 → 因子 1.0，
    // 与历史输出一致。
    let exposure_effect = db.more(cfg, &[ModName::from("ExposureEffectOnSelf")]);
    for (exposure_name, resist_name) in [
        ("FireExposure", "FireResist"),
        ("ColdExposure", "ColdResist"),
        ("LightningExposure", "LightningResist"),
    ] {
        let raw = db.max_of(ModType::Base, cfg, &[ModName::from(exposure_name)]);
        // 先乘 effect-on-self 再向下取整（PoB2 `m_floor((value + extraExposure) * ... * effectOnSelf)`）。
        let magnitude = (raw * exposure_effect).floor();
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
/// 注意：本函数构造 enemy 用 [`setup_enemy`]，仅在 boss 自带穿透时向玩家 modDB 注入
/// `ElementalPenetration BASE`（见 [`setup_enemy`]），其余玩家来源（装备/天赋/宝石）不在此注入。
pub fn env_with_enemy(player: Actor, config_level: u32, tier: EnemyTier) -> Env {
    let mut env = Env::new(player);
    setup_enemy(&mut env, config_level, tier);
    env
}
