//! 防御面板族：Block / Spirit / Ward / Deflection 的
//! 「基底数据 → 聚合 → OutputTable」纯函数计算。
//!
//! 与 `defence.rs` 分文件（Track D：避免与 Track C/E 抢
//! defence.rs 的函数级分区）。各函数只读 `ModDb`/`CalcConfig`，不写 `Env`；
//! 对 `Env` 的写入集中在 [`fill_defence_panels`]（perform 一行调用）。
//!
//! vendor 参照：`vendor/PathOfBuilding-PoE2/src/Modules/CalcDefence.lua`
//! - Block：:961-1058（BlockChanceMax 体系 + 三分型 + lucky/unlucky 幂）
//! - Ward：:1144-1273（per-slot 聚合 + EnergyShieldToWard）
//! - Deflection：:48-54（`deflectChance` 公式）+ :1516-1522（rating 合成，
//!   0.22.0 vendor 行号；0.5.4b 复核公式未变）
//! - Spirit：:73-126（Life/Mana/Spirit 统一池公式）

use crate::{CalcConfig, ModDb};
use pobr_data::prelude::*;

use super::env::Env;
use super::round;

/// `calcLib.mod` 等价：`(1 + Σinc/100) × Πmore`（vendor CalcTools.lua）。
fn scaling_mod(db: &ModDb, cfg: &CalcConfig, names: &[ModName]) -> f64 {
    (1.0 + db.sum(ModType::Inc, cfg, names) / 100.0) * db.more(cfg, names)
}

// Block（CalcDefence.lua:961-1058）

/// Block 面板计算结果（攻击/投射物/法术/法术投射物四分型 + 上限 + 有效值 + 承伤）。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BlockResult {
    /// 攻击格挡几率（%；cap 后）。
    pub block_chance: f64,
    /// 攻击格挡上限（%）。
    pub block_chance_max: f64,
    /// 投射物攻击格挡几率（%）。
    pub projectile_block_chance: f64,
    /// 法术格挡几率（%）。
    pub spell_block_chance: f64,
    /// 法术格挡上限（%）。
    pub spell_block_chance_max: f64,
    /// 法术投射物格挡几率（%）。
    pub spell_projectile_block_chance: f64,
    /// 有效攻击格挡（%；lucky/unlucky 幂后）。
    pub effective_block_chance: f64,
    /// 有效法术格挡（%）。
    pub effective_spell_block_chance: f64,
    /// 有效投射物攻击格挡（%）。EHP 平均格挡 = 四分型均值（vendor :1067）。
    pub effective_projectile_block_chance: f64,
    /// 有效法术投射物格挡（%）。
    pub effective_spell_projectile_block_chance: f64,
    /// 格挡承伤比例（%；被格挡命中仍承受的伤害份额 = Σ BASE BlockEffect，
    /// vendor `DamageTakenOnBlock`；0 = 完全格挡）。
    pub block_effect_taken_pct: f64,
}

/// lucky/unlucky 幂变换（CalcDefence.lua:1038-1052）：
/// lucky（两次取优）→ `(1 − (1−p)²)`；unlucky（两次取劣）→ `p²`。
fn luck_transform(chance_pct: f64, lucky: bool, unlucky: bool) -> f64 {
    // vendor `luck = luck × (lucky and 2 or 1) / (unlucky and 2 or 1)`——同时成立时抵消。
    if lucky && !unlucky {
        (1.0 - (1.0 - chance_pct / 100.0).powi(2)) * 100.0
    } else if unlucky && !lucky {
        chance_pct / 100.0 * chance_pct
    } else {
        chance_pct
    }
}

/// Block 面板计算（CalcDefence.lua:961-1058 逐段对照）。
///
/// 输入约定：盾基底格挡已由 build 层注入 `ShieldBlockChance` BASE（vendor
/// :975-979 读 Weapon 2/3 `armourData.BlockChance`；PoBR 由 calc_orchestrator
/// 按 Weapon2 槽基底 catalog 值注入）。盾物品上的局部 block 词条留在全局桶
/// 数学等价（`(base+ΣBASE)×mod` 对加法/乘法项的归属不敏感，仅差 vendor 件级
/// `floor`，≤1% 级），不做 drop-local。
///
/// 公式（vendor 行号）：
/// - 上限（:961-965）：`BlockChanceMax = Override ‖ (ΣBASE BaseBlockChanceMax +
///   ΣBASE BlockChanceMax)`，min `BlockChanceCap`(90)；角色固有 `BaseBlockChanceMax`
///   50 走 `cfg.constants.game().base_block_chance_max`（vendor CalcSetup.lua:28
///   注入，Misc.lua:147）。
/// - 攻击（:989-991）：`(ShieldBlockChance + ΣBASE BlockChance) × calcLib.mod(BlockChance)`
///   后 min 上限。
/// - 投射物（:994）：`block + ΣBASE ProjectileBlockChance × mod(BlockChance)` 再 cap。
/// - 法术上限（:995-998）：`SpellBlockChanceMaxIsBlockChanceMax` flag → 同攻击上限；
///   否则 `Override ‖ (ΣBASE BaseSpellBlockChanceMax + ΣBASE SpellBlockChanceMax)` cap。
/// - 法术（:1003-1014）：`SpellBlockChanceIsBlockChance` flag → 等同攻击分型；
///   否则 `ΣBASE SpellBlockChance × mod(SpellBlockChance)` cap；法术投射物
///   `max(spell + ΣBASE ProjectileSpellBlockChance × mod, projectile, 0)`。
/// - `CannotBlockAttacks`/`CannotBlockSpells`（:1026-1033）→ 对应分型清零。
/// - Effective（:1034-1052）：lucky/unlucky flag 幂变换（enemy `reduceEnemyBlock`
///   归 0 略去——敌方 ModDb 无此来源）。
/// - 承伤（:1054-1058）：`DamageTakenOnBlock = ΣBASE BlockEffect`
///   （vendor `BlockEffect = 100 − Σ` 为防住份额，本结构直接给承伤份额）。
pub fn calc_block(db: &ModDb, cfg: &CalcConfig) -> BlockResult {
    let cap = cfg.constants.game().block_chance_cap;
    let inherent_max = cfg.constants.game().base_block_chance_max;

    // :961-965 攻击格挡上限。
    let block_max = db
        .override_(cfg, ModName::from("BlockChanceMax"))
        .unwrap_or_else(|| {
            inherent_max
                + db.sum(ModType::Base, cfg, &[ModName::from("BaseBlockChanceMax")])
                + db.sum(ModType::Base, cfg, &[ModName::from("BlockChanceMax")])
        })
        .min(cap);

    // :975-980 盾基底（build 层注入；双持双盾 = 两槽求和，与 vendor 相加一致）。
    let shield_base = db.sum(ModType::Base, cfg, &[ModName::from("ShieldBlockChance")]);

    // :989-991 攻击格挡。
    let block_names = [ModName::from("BlockChance")];
    let total_block = (shield_base + db.sum(ModType::Base, cfg, &block_names))
        * scaling_mod(db, cfg, &block_names);
    let mut block = total_block.min(block_max);

    // :994 投射物攻击格挡（增量 BASE 也吃 BlockChance 乘区）。
    let mut projectile = (block
        + db.sum(
            ModType::Base,
            cfg,
            &[ModName::from("ProjectileBlockChance")],
        ) * scaling_mod(db, cfg, &block_names))
    .min(block_max);

    // :995-998 法术格挡上限。
    let spell_max = if db.flag(cfg, ModName::from("SpellBlockChanceMaxIsBlockChanceMax")) {
        block_max
    } else {
        db.override_(cfg, ModName::from("BlockChanceMax"))
            .unwrap_or_else(|| {
                inherent_max
                    + db.sum(
                        ModType::Base,
                        cfg,
                        &[ModName::from("BaseSpellBlockChanceMax")],
                    )
                    + db.sum(ModType::Base, cfg, &[ModName::from("SpellBlockChanceMax")])
            })
            .min(cap)
    };

    // :1003-1014 法术 / 法术投射物格挡。
    let spell_names = [ModName::from("SpellBlockChance")];
    let (mut spell, mut spell_projectile) = if db
        .flag(cfg, ModName::from("SpellBlockChanceIsBlockChance"))
    {
        (block, projectile)
    } else {
        let spell = (db.sum(ModType::Base, cfg, &spell_names) * scaling_mod(db, cfg, &spell_names))
            .min(spell_max);
        let spell_proj = (spell
            + db.sum(
                ModType::Base,
                cfg,
                &[ModName::from("ProjectileSpellBlockChance")],
            ) * scaling_mod(db, cfg, &spell_names))
        .min(spell_max)
        .max(projectile)
        .max(0.0);
        (spell, spell_proj)
    };

    // :1026-1033 禁格挡 flag。
    if db.flag(cfg, ModName::from("CannotBlockAttacks")) {
        block = 0.0;
        projectile = 0.0;
    }
    if db.flag(cfg, ModName::from("CannotBlockSpells")) {
        spell = 0.0;
        spell_projectile = 0.0;
    }

    // :1034-1052 有效值（lucky/unlucky 幂）。
    let effective = |v: f64, kind: &str| -> f64 {
        let lucky = db.flag(cfg, ModName::from(format!("{kind}IsLucky").as_str()));
        let unlucky = db.flag(cfg, ModName::from(format!("{kind}IsUnlucky").as_str()));
        round(luck_transform(v, lucky, unlucky))
    };

    BlockResult {
        block_chance: round(block),
        block_chance_max: round(block_max),
        projectile_block_chance: round(projectile),
        spell_block_chance: round(spell),
        spell_block_chance_max: round(spell_max),
        spell_projectile_block_chance: round(spell_projectile),
        effective_block_chance: effective(block, "BlockChance"),
        effective_spell_block_chance: effective(spell, "SpellBlockChance"),
        effective_projectile_block_chance: effective(projectile, "ProjectileBlockChance"),
        effective_spell_projectile_block_chance: effective(
            spell_projectile,
            "SpellProjectileBlockChance",
        ),
        // :1054-1058 承伤份额（clamp 到 [0,100]，超额防住不为负）。
        block_effect_taken_pct: db
            .sum(ModType::Base, cfg, &[ModName::from("BlockEffect")])
            .clamp(0.0, 100.0),
    }
}

// Ward 池（CalcDefence.lua:1144-1273）

/// Ward 池聚合（CalcDefence.lua:1158-1186 per-slot + :1275-1296 全局 BASE）。
///
/// `Ward = ΣBASE Ward × calcLib.mod(Ward, Defences)`；`EnergyShieldToWard`
/// keystone（C-1 快照传入）时 inc 名集追加 `EnergyShield`（ES 的 inc 借给
/// Ward，:1162-1163 `Sum("INC", slotCfg, "Ward", "Defences", "EnergyShield")`）。
///
/// 件级底值（rolled `Ward:` 行 / catalog 基底 ward）由 build 层注入 `Ward`
/// BASE（SlotName tag），与全局 `+N to Ward` 同桶——「逐件乘全局后求和」与
/// 「求和后乘全局」等价（同 defence_base_modifiers 的等价性论证；slot-scoped
/// inc 词条与 `DoubleBodyArmourDefence` 的件级翻倍缺口同 armour 路径，后续补）。
/// vendor 取整：per-slot 求和后无显式 round，沿用 round 到 1e-9。
pub fn calc_ward(db: &ModDb, cfg: &CalcConfig, es_to_ward: bool) -> f64 {
    let base = db.sum(ModType::Base, cfg, &[ModName::from("Ward")]);
    if base <= 0.0 {
        return 0.0;
    }
    let inc_names: &[ModName] = if es_to_ward {
        &[
            ModName::from("Ward"),
            ModName::from("Defences"),
            ModName::from("EnergyShield"),
        ]
    } else {
        &[ModName::from("Ward"), ModName::from("Defences")]
    };
    let more_names = [ModName::from("Ward"), ModName::from("Defences")];
    let inc = db.sum(ModType::Inc, cfg, inc_names);
    let more = db.more(cfg, &more_names);
    round(base * (1.0 + inc / 100.0) * more)
}

// Deflection（CalcDefence.lua:48-54 / :1516-1522）

/// Deflection 面板结果。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DeflectionResult {
    /// 偏斜等级（rating）。
    pub rating: f64,
    /// 偏斜几率（%；DeflectIsLucky 幂后）。
    pub chance: f64,
    /// 偏斜减伤幅度（%；基础 40 + ΣBASE DeflectEffect，clamp [0,100]；
    /// Track F 折入 mitigation 层）。
    pub effect_pct: f64,
}

/// `calcs.deflectChance` 等价（CalcDefence.lua:48-54）：
/// `chanceToNotDeflect = acc/(acc + deflection×0.12) × 150 − 50`，
/// `chance = clamp(100 − round(notDeflect), 0, DeflectionChanceCap)`；
/// `deflection < 1` → 0。
fn deflect_chance_pct(deflection: f64, accuracy: f64, cap: f64) -> f64 {
    if deflection < 1.0 {
        return 0.0;
    }
    let chance_to_not_deflect = accuracy / (accuracy + deflection * 0.12) * 150.0 - 50.0;
    (100.0 - chance_to_not_deflect.round()).clamp(0.0, cap)
}

/// Deflection 合成（CalcDefence.lua:1516-1522）。
///
/// `DeflectionRating = ΣBASE DeflectionRating + (Evasion × ΣBASE
/// EvasionGainAsDeflection/100 + Armour × ΣBASE ArmourGainAsDeflection/100)
/// × calcLib.mod(DeflectionRating)`——vendor 括号语义：inc/more 乘区**只作用于
/// GainAs 派生部分**（:1490 原文）。`DeflectChance = deflectChance(rating,
/// enemyAccuracy)`；`DeflectIsLucky` → `(1−(1−p)²)`（:1518-1521）；
/// `DeflectEffect = clamp(基础 40 + ΣBASE, 0, 100)`（:1522，常量
/// `cfg.constants.game().deflect_effect`）。
pub fn calc_deflection(
    db: &ModDb,
    cfg: &CalcConfig,
    armour: f64,
    evasion: f64,
    enemy_accuracy: f64,
) -> DeflectionResult {
    let rating_names = [ModName::from("DeflectionRating")];
    let gain_from_evasion = evasion
        * db.sum(
            ModType::Base,
            cfg,
            &[ModName::from("EvasionGainAsDeflection")],
        )
        / 100.0;
    let gain_from_armour = armour
        * db.sum(
            ModType::Base,
            cfg,
            &[ModName::from("ArmourGainAsDeflection")],
        )
        / 100.0;
    let rating = db.sum(ModType::Base, cfg, &rating_names)
        + (gain_from_evasion + gain_from_armour) * scaling_mod(db, cfg, &rating_names);

    let mut chance = deflect_chance_pct(
        rating,
        enemy_accuracy,
        cfg.constants.game().deflection_chance_cap,
    );
    // :1518-1521 DeflectIsLucky 幂。
    if db.flag(cfg, ModName::from("DeflectIsLucky")) {
        chance = luck_transform(chance, true, false);
    }
    let effect_pct = (cfg.constants.game().deflect_effect
        + db.sum(ModType::Base, cfg, &[ModName::from("DeflectEffect")]))
    .clamp(0.0, 100.0);

    DeflectionResult {
        rating: round(rating),
        chance: round(chance),
        effect_pct: round(effect_pct),
    }
}

// Spirit 池（CalcDefence.lua:73-126）

/// Spirit 池本值（CalcDefence.lua:87-95，Life/Mana/Spirit 统一公式）。
///
/// `Spirit = Override ‖ max(round((ΣBASE Spirit × (1−conv/100) + ΣBASE ExtraSpirit)
/// × (1+Σinc/100) × Πmore), 1)`，其中
/// `conv = min(ΣBASE SpiritConvertTo{EnergyShield,Armour,Evasion}, 100)`（:92）。
/// 取整为 vendor `round`（最近整数）；下限 1 与 Life/Mana 一致（:95）。
///
/// 来源：基底 Spirit（权杖 `Spirit:` 行 / catalog `spirit`，build 层注入
/// `Spirit` BASE）+ 任务奖励 `+30/+30/+40 to Spirit`（xml_build 全局词条）+
/// 树/装备 `+N to Spirit`。
pub fn calc_spirit_pool(db: &ModDb, cfg: &CalcConfig) -> f64 {
    let names = [ModName::from("Spirit")];
    if let Some(v) = db.override_(cfg, ModName::from("Spirit")) {
        return v;
    }
    let base = db.sum(ModType::Base, cfg, &names);
    let extra = db.sum(ModType::Base, cfg, &[ModName::from("ExtraSpirit")]);
    let conv = db
        .sum(
            ModType::Base,
            cfg,
            &[
                ModName::from("SpiritConvertToEnergyShield"),
                ModName::from("SpiritConvertToArmour"),
                ModName::from("SpiritConvertToEvasion"),
            ],
        )
        .min(100.0);
    ((base * (1.0 - conv / 100.0) + extra) * scaling_mod(db, cfg, &names))
        .round()
        .max(1.0)
}

// fill 编排

/// Track D fill 编排：Block / Spirit / Ward / Deflection 面板族写
/// [`super::OutputTable`]。需在 `fill_skill_mechanics` 之后调用
/// （spirit_unreserved 读取技能侧已写入的 `spirit_reserved`）。
///
/// keystone 开关经快照传入。
pub fn fill_defence_panels(env: &mut Env, keystones: &crate::rules::DefenceKeystones) {
    let db = &env.player.mod_db;
    let cfg = &env.cfg;

    // Block（CalcDefence.lua:961-1058）
    // 覆写 fill_mechanics 早期的旧 Σ BASE clamp 口径（缺盾基底与 inc 乘区），
    // 对齐 vendor `(shield_base + ΣBASE) × mod` 全公式。
    let block = calc_block(db, cfg);
    env.player.output.block_chance = block.block_chance;
    env.player.output.spell_block_chance = block.spell_block_chance;
    env.player.output.block_chance_max = block.block_chance_max;
    env.player.output.spell_block_chance_max = block.spell_block_chance_max;
    env.player.output.effective_block_chance = block.effective_block_chance;
    env.player.output.effective_spell_block_chance = block.effective_spell_block_chance;
    env.player.output.effective_projectile_block_chance = block.effective_projectile_block_chance;
    env.player.output.effective_spell_projectile_block_chance =
        block.effective_spell_projectile_block_chance;
    env.player.output.block_effect = block.block_effect_taken_pct;

    // Spirit 池本值 + 未预留余量（CalcDefence.lua:73-126 / :330-337）
    // 技能侧 spirit_reserved 由 fill_skill_mechanics先行写入，
    // 本段只做池本值与差值。
    // vendor :337 `Unreserved = max − reserved` 无下限——超订（reserved > 池）
    // 为负，与 golden（SpiritUnreserved 可为 −130 等）一致。
    env.player.output.spirit = calc_spirit_pool(db, cfg);
    env.player.output.spirit_unreserved =
        env.player.output.spirit - env.player.output.spirit_reserved;

    // Ward 池（CalcDefence.lua:1144-1296；EnergyShieldToWard 走 C-1 快照）
    env.player.output.ward = calc_ward(db, cfg, keystones.energy_shield_to_ward);

    // Deflection（CalcDefence.lua:48-54 / :1516-1522）
    // 敌人命中读数同 Track E evade 路径（env.enemy.base.accuracy）。
    let deflect = calc_deflection(
        db,
        cfg,
        env.player.output.armour,
        env.player.output.evasion,
        env.enemy.base.accuracy,
    );
    env.player.output.deflection_rating = deflect.rating;
    env.player.output.deflect_chance = deflect.chance;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// lucky 幂：50% → 75%；unlucky 幂：50% → 25%；双 flag 抵消。
    /// （CalcDefence.lua:1038-1052 手算）
    #[test]
    fn luck_transform_powers() {
        assert_eq!(luck_transform(50.0, true, false), 75.0);
        assert_eq!(luck_transform(50.0, false, true), 25.0);
        assert_eq!(luck_transform(50.0, true, true), 50.0);
        assert_eq!(luck_transform(50.0, false, false), 50.0);
    }
}
