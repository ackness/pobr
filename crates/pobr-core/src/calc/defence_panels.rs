//! 防御面板族（M2 Track D）：Block / Spirit / Ward / Deflection 的
//! 「基底数据 → 聚合 → OutputTable」纯函数计算。
//!
//! 与 `defence.rs` 分文件（蓝图 m2-defence §2 Track D：避免与 Track C/E 抢
//! defence.rs 的函数级分区）。各函数只读 `ModDb`/`CalcConfig`，不写 `Env`；
//! 对 `Env` 的写入集中在 [`fill_defence_panels`]（perform 一行调用）。
//!
//! vendor 参照：`vendor/PathOfBuilding-PoE2/src/Modules/CalcDefence.lua`
//! - Block：:961-1058（BlockChanceMax 体系 + 三分型 + lucky/unlucky 幂）
//! - Ward：:1144-1273（per-slot 聚合 + EnergyShieldToWard）
//! - Deflection：:48-54（`deflectChance` 公式）+ :1487-1506（rating 合成）
//! - Spirit：:73-126（Life/Mana/Spirit 统一池公式）

use crate::{CalcConfig, ModDb};
use pobr_data::prelude::*;

use super::env::Env;
use super::round;

/// `calcLib.mod` 等价：`(1 + Σinc/100) × Πmore`（vendor CalcTools.lua）。
fn scaling_mod(db: &ModDb, cfg: &CalcConfig, names: &[ModName]) -> f64 {
    (1.0 + db.sum(ModType::Inc, cfg, names) / 100.0) * db.more(cfg, names)
}

// ─────────────────────────────────────────────────────────────────
// Block（CalcDefence.lua:961-1058）
// ─────────────────────────────────────────────────────────────────

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
        // :1054-1058 承伤份额（clamp 到 [0,100]，超额防住不为负）。
        block_effect_taken_pct: db
            .sum(ModType::Base, cfg, &[ModName::from("BlockEffect")])
            .clamp(0.0, 100.0),
    }
}

// ─────────────────────────────────────────────────────────────────
// fill 编排（perform 一行调用，蓝图 §3.2 预登记）
// ─────────────────────────────────────────────────────────────────

/// Track D fill 编排：Block / Spirit / Ward / Deflection 面板族写
/// [`super::OutputTable`]。需在 `fill_skill_mechanics` 之后调用
/// （spirit_unreserved 读取技能侧已写入的 `spirit_reserved`）。
///
/// keystone 开关经快照传入（C-1 契约，蓝图 §3.3，不散读 flag）。
pub fn fill_defence_panels(env: &mut Env, _keystones: &crate::rules::DefenceKeystones) {
    let db = &env.player.mod_db;
    let cfg = &env.cfg;

    // --- Block（CalcDefence.lua:961-1058）---
    // 覆写 fill_mechanics 早期的旧 Σ BASE clamp 口径（缺盾基底与 inc 乘区），
    // 对齐 vendor `(shield_base + ΣBASE) × mod` 全公式。
    let block = calc_block(db, cfg);
    env.player.output.block_chance = block.block_chance;
    env.player.output.spell_block_chance = block.spell_block_chance;
    env.player.output.block_chance_max = block.block_chance_max;
    env.player.output.spell_block_chance_max = block.spell_block_chance_max;
    env.player.output.effective_block_chance = block.effective_block_chance;
    env.player.output.effective_spell_block_chance = block.effective_spell_block_chance;
    env.player.output.block_effect = block.block_effect_taken_pct;
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
