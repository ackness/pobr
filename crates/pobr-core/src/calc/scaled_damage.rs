//! M4-T3 乘区与 DPS 末端：Double/Triple Damage 乘区（W-C1）+ allMult 占位结构 +
//! DPS 末端两因子（W-C4）。
//!
//! **模块先行、接线最后**（蓝图 m4-offence-deep.md §2-T3 / §3.2）：本文件只提供独立计算
//! 单元与冻结签名，`offence.rs` / `crit_pass` 的消费由 T2 接线（契约 2，§3.3）。接线点与
//! 口径差异见 `audits/rearchitecture-2026-06-10/blueprints/m4-t3-wiring-notes.md`。

use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb};

/// Double/Triple Damage 乘区结算结果（PoB2 `CalcOffence.lua:3840-3861`）。
///
/// - [`double_chance`](Self::double_chance) / [`triple_chance`](Self::triple_chance)：
///   **百分比 0..=100**（与 vendor `output.DoubleDamageChance` / `TripleDamageChance` 同位）。
///   `double_chance` 已做 Triple 抵扣（`DD = max(DD − TD×DD/100, 0)`，`:3858`）。
/// - [`effect`](Self::effect)：`ScaledDamageEffect = 1 × (1 + DD/100 + 2×TD/100)`（`:3861`）。
///   vendor 初始化 `ScaledDamageEffect = 1` 后**只有** DD/TD 乘它（`:3840`，蓝图亲验）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScaledDamage {
    /// `1 + DoubleDamageEffect + TripleDamageEffect`，乘入 allMult。
    pub effect: f64,
    /// 最终 Double Damage 几率（百分比，已抵扣 Triple）。
    pub double_chance: f64,
    /// 最终 Triple Damage 几率（百分比）。
    pub triple_chance: f64,
}

/// Double/Triple Damage 乘区（蓝图 §3.3 契约 2 冻结签名；T2 在 crit_pass 内调用）。
///
/// `crit_chance` 为**分数 0..=1**（即 [`crate::calc::resolve_crit`] 的 `chance`；等价
/// vendor `output.CritChance / 100`——`:3844/:3849` 的 `OnCrit × CritChance / 100` 折算）。
///
/// 逐行对照 PoB2 `CalcOffence.lua:3842-3861`：
///
/// - `TripleDamageChanceOnCrit = min(Sum(BASE), 100)`；`TripleDamageChance =
///   min(Sum(BASE) + enemy.SelfTripleDamageChance(仅 effective) + OnCrit×crit, 100)`。
/// - Double 同构（`:3848-3849`）。
/// - Intimidate（`:3850-3854`）：`Condition:WarcryMaxHit` 时 DD=100，否则
///   `+IntimidatingUpTimeRatio`——warcry 机制 M5+ 未实现，该输入恒缺省，整段跳过
///   （TODO(M5+ warcry)：接入 `IntimidatingUpTimeRatio` 输入后补此分支）。
/// - Triple 抵扣 Double（`:3855-3859`）：`DD = max(DD − TD×DD/100, 0)`。
/// - `ScaledDamageEffect = 1 × (1 + DD/100 + 2×TD/100)`（`:3860-3861`）。
///
/// 注：vendor `:3845`（Triple 行）原文 `Sum(...) or 0 + (...)` 受 Lua 算符优先级影响
/// （`+` 高于 `or`）实际只取 `Sum(...)`，把敌方/OnCrit 两项丢掉——与相邻 Double 行
/// （`:3849`，无 `or`）结构不一致，判为 vendor 笔误；本实现按蓝图 §2 W-C1 给出的
/// **意图语义**（与 Double 同构）。
///
/// TODO(W-A3 globalLimit)：`chance to deal Double Damage` 的 DOUBLED form 词条带
/// globalLimit（蓝图 §2 W-C1 末行）——依赖 T1 W-A3 的 limit 原语，就绪后在 Sum 侧生效，
/// 本函数无需改动。
pub fn scaled_damage_effect(
    db: &ModDb,
    enemy_db: &ModDb,
    cfg: &CalcConfig,
    crit_chance: f64,
) -> ScaledDamage {
    let triple_on_crit = db
        .sum(
            ModType::Base,
            cfg,
            &[ModName::from("TripleDamageChanceOnCrit")],
        )
        .min(100.0);
    let enemy_self_triple = if cfg.mode_effective {
        enemy_db.sum(
            ModType::Base,
            cfg,
            &[ModName::from("SelfTripleDamageChance")],
        )
    } else {
        0.0
    };
    let triple_chance = (db.sum(ModType::Base, cfg, &[ModName::from("TripleDamageChance")])
        + enemy_self_triple
        + triple_on_crit * crit_chance)
        .min(100.0);

    let double_on_crit = db
        .sum(
            ModType::Base,
            cfg,
            &[ModName::from("DoubleDamageChanceOnCrit")],
        )
        .min(100.0);
    let enemy_self_double = if cfg.mode_effective {
        enemy_db.sum(
            ModType::Base,
            cfg,
            &[ModName::from("SelfDoubleDamageChance")],
        )
    } else {
        0.0
    };
    let mut double_chance = (db.sum(ModType::Base, cfg, &[ModName::from("DoubleDamageChance")])
        + enemy_self_double
        + double_on_crit * crit_chance)
        .min(100.0);

    // Triple 抵扣 Double：两者同时掷中时按 Triple 结算，扣掉重叠概率（:3855-3859）。
    if triple_chance > 0.0 {
        double_chance = (double_chance - triple_chance * double_chance / 100.0).max(0.0);
    }

    ScaledDamage {
        effect: 1.0 + double_chance / 100.0 + 2.0 * triple_chance / 100.0,
        double_chance,
        triple_chance,
    }
}

/// allMult 其余五因子的占位结构（PoB2 `CalcOffence.lua:4023-4025`）。
///
/// `allMult = ScaledDamageEffect × FistOfWarDamageEffect × AncestralCallDamageEffect ×
/// AncestralEmpowermentDamageEffect × AncestralEmpowermentCombinedDamageEffect ×
/// OffensiveWarcryEffect(WarcryMaxHit 时换 Max 变体)`。
///
/// M4 范围只实现 ScaledDamageEffect；warcry / ancestral 是 M5+ 机制，本结构默认全 1.0
/// 占位、留接口防返工（蓝图 §2 W-C1）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AllMultExtras {
    pub fist_of_war: f64,
    pub ancestral_call: f64,
    pub ancestral_empowerment: f64,
    pub ancestral_empowerment_combined: f64,
    /// `OffensiveWarcryEffect` 或（`Condition:WarcryMaxHit` 时）`MaxOffensiveWarcryEffect`，
    /// 由未来的 warcry 实现选定后传入。
    pub offensive_warcry: f64,
}

impl Default for AllMultExtras {
    fn default() -> Self {
        Self {
            fist_of_war: 1.0,
            ancestral_call: 1.0,
            ancestral_empowerment: 1.0,
            ancestral_empowerment_combined: 1.0,
            offensive_warcry: 1.0,
        }
    }
}

impl AllMultExtras {
    /// 五因子连乘积。
    pub fn product(&self) -> f64 {
        self.fist_of_war
            * self.ancestral_call
            * self.ancestral_empowerment
            * self.ancestral_empowerment_combined
            * self.offensive_warcry
    }
}

/// allMult 全量（`ScaledDamageEffect × 五因子`，PoB2 `CalcOffence.lua:4023-4025`）。
/// extras 默认全 1.0 时退化为 `scaled.effect`。
pub fn all_mult(scaled: &ScaledDamage, extras: &AllMultExtras) -> f64 {
    scaled.effect * extras.product()
}

/// DPS 末端两因子（W-C4，PoB2 `CalcOffence.lua:3128-3130` / `:3863` / `:4407`）。
///
/// `TotalDPS = AverageDamage × (HitSpeed or Speed) × dps_multiplier × quantity_multiplier`。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DpsEndFactors {
    /// `skillData.dpsMultiplier × calcLib.mod(skillModList, cfg, "DPS")`（`:3863`）。
    pub dps_multiplier: f64,
    /// `max(Sum(BASE, cfg, "QuantityMultiplier"), 1)`（`:3128`，floor 1.0）。
    pub quantity_multiplier: f64,
}

/// 结算 DPS 末端两因子。
///
/// - `skill_dps_multiplier`：技能数据携带的 `dpsMultiplier`（catalog
///   `SkillStatSetDef/SkillLevelDef.dps_multiplier`，T4 数据侧透传；缺省视为 1.0——
///   vendor `skillData.dpsMultiplier or 1`）。
/// - `"DPS"` ModName 的 inc/more 在此消费（`calcLib.mod` =
///   `(1 + Sum(INC)/100) × More`，CalcTools.lua:16-18）。
/// - `QuantityMultiplier` 经 ModDb BASE 聚合、floor 1.0（vendor 仅在 >1 时写
///   `output.QuantityMultiplier`，数值语义等价 floor）。
pub fn dps_end_factors(
    db: &ModDb,
    cfg: &CalcConfig,
    skill_dps_multiplier: Option<f64>,
) -> DpsEndFactors {
    let dps_names = [ModName::from("DPS")];
    let dps_multiplier = skill_dps_multiplier.unwrap_or(1.0)
        * (1.0 + db.sum(ModType::Inc, cfg, &dps_names) / 100.0)
        * db.more(cfg, &dps_names);
    let quantity_multiplier = db
        .sum(ModType::Base, cfg, &[ModName::from("QuantityMultiplier")])
        .max(1.0);
    DpsEndFactors {
        dps_multiplier,
        quantity_multiplier,
    }
}
