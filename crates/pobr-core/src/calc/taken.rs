//! taken-as 管线 + effectiveAppliedArmour（13-G1 / 13-G7）。
//!
//! vendor PoB2 对照（`Modules/CalcDefence.lua`，行号 2026-06-11 核实）：
//! - `:2171-2190` 防御侧 `actor.damageShiftTable` 装配（`<Src>DamageTakenAs<Dst>` /
//!   `<Src>DamageFromHitsTakenAs<Dst>` + elemental 变体的 BASE 求和；源类型保留
//!   `max(100−total, 0)`）→ [`damage_shift_table`]；进攻侧同构求和见 `:356-365`
//!   `applyDmgTakenConversion`。
//! - `:2336-2362` per-type `EffectiveAppliedArmour` 合成（`ArmourAppliesTo<X>DamageTaken`
//!   百分比 × `(1 + ArmourDefense)` + Evasion/ES 借入项）→ [`effective_applied_armour`]；
//!   `:1862-1863` 物理隐式 `ArmourAppliesToPhysicalDamageTaken` BASE 100（vendor 写入
//!   modDB，本实现在函数内等价折算、**不写 ModDb**）。
//! - `:422-455` `takenHitFromDamage(rawDamage, damageType, actor)` 等价入口 →
//!   [`taken_hit_from_damage`]（per 转换类型：effArmour 护甲减伤 + flat DR − overwhelm
//!   （clamp per-type `DamageReductionMax`）× 抗性承受乘数 + takenFlat，再 ×
//!   `AfterReductionTakenHitMulti`）。
//!
//! 设计：
//! - [`MitigationCtx`] 是「不随单次击中变化」的减伤快照——整备（[`build_mitigation_ctx`]
//!   读 ModDb）与求值（[`taken_hit_from_damage`] 纯算术）分离，与 `pool_damage` 的
//!   PoolCtx/求值切分同构；消费者为 Track F 的新 max-hit / EHP 管线。
//! - **过渡语义**：F 接线前，旧 `ehp.rs` 的 `armour_applies_to_element: [bool;3]` 路径
//!   保持原行为；本模块的百分比模型只被新测试与 F 消费（B-2 起 perform 由本 ctx 派生
//!   该 `[bool;3]`，词条单一来源化）。
//!
//! per-type 数组下标约定 = `DamageType as usize`（Physical/Fire/Cold/Lightning/Chaos，
//! 与 `pool_damage::PoolState` 同约定）。

use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb};

use super::defence::{armour_reduction, taken_mult_for_type_default};
use super::round;

/// 伤害类型按枚举序（per-type 数组下标序）排列。
///
/// 注意与 PoB2 防御遍历序（`pool_damage::POB2_DAMAGE_ORDER`）不同；本模块的
/// per-type 计算相互独立、无共享池消耗，遍历序不影响结果，故用枚举序。
const DAMAGE_TYPE_BY_INDEX: [DamageType; 5] = [
    DamageType::Physical,
    DamageType::Fire,
    DamageType::Cold,
    DamageType::Lightning,
    DamageType::Chaos,
];

/// DamageType → 词条名前缀（PoB2 ModName 约定）。
fn dt_prefix(dt: DamageType) -> &'static str {
    match dt {
        DamageType::Physical => "Physical",
        DamageType::Fire => "Fire",
        DamageType::Cold => "Cold",
        DamageType::Lightning => "Lightning",
        DamageType::Chaos => "Chaos",
    }
}

/// vendor `round(val)`（`Modules/Common.lua`：`m_floor(val + 0.5)`，四舍五入到整数）。
/// [`taken_hit_from_damage`] 的最终承受伤害按 vendor 同点取整（CalcDefence.lua:442）。
fn vendor_round(value: f64) -> f64 {
    (value + 0.5).floor()
}

// damage shift table（13-G1）

/// 构建防御侧 taken-as 转换矩阵 `shift[src][dst]`（fraction，0-1）。
///
/// vendor：CalcDefence.lua:2171-2190（hit 口径 shiftTable；DoT 口径的
/// `damageOverTimeShiftTable` 不含 FromHits 词条，留 Track F/DoT 接线时扩展）：
/// - `dst ≠ src`：`Σ BASE(<Src>DamageTakenAs<Dst>, <Src>DamageFromHitsTakenAs<Dst>
///   [, ElementalDamageTakenAs<Dst>, ElementalDamageFromHitsTakenAs<Dst> 若 src 为元素]) / 100`；
/// - `dst = src`（源保留）：`max(1 − Σ目标/100, 0)`——总转出 >100% 时仅源截断到 0，
///   各目标份额**不归一化**（vendor 同语义：承受总量可超过 raw）。
pub fn damage_shift_table(db: &ModDb, cfg: &CalcConfig) -> [[f64; 5]; 5] {
    let mut shift = [[0.0; 5]; 5];
    for src in DAMAGE_TYPE_BY_INDEX {
        let s = src as usize;
        let src_name = dt_prefix(src);
        let mut total_pct = 0.0;
        for dst in DAMAGE_TYPE_BY_INDEX {
            if dst == src {
                continue;
            }
            let dst_name = dt_prefix(dst);
            let mut names = vec![
                ModName::from(format!("{src_name}DamageTakenAs{dst_name}")),
                ModName::from(format!("{src_name}DamageFromHitsTakenAs{dst_name}")),
            ];
            if src.is_elemental() {
                // 元素源额外吃 Elemental 族（vendor :2181/:2183 isElemental 分支）。
                names.push(ModName::from(format!("ElementalDamageTakenAs{dst_name}")));
                names.push(ModName::from(format!(
                    "ElementalDamageFromHitsTakenAs{dst_name}"
                )));
            }
            let pct = db.sum(ModType::Base, cfg, &names);
            shift[s][dst as usize] = pct / 100.0;
            total_pct += pct;
        }
        // 源保留 max(1−total, 0)（vendor :2189 `m_max(100 - destTotal, 0)`）。
        shift[s][s] = (1.0 - total_pct / 100.0).max(0.0);
    }
    shift
}

// effectiveAppliedArmour（13-G7 百分比模型）

/// 某伤害类型的「护甲适用百分比」（`ArmourAppliesTo<X>DamageTaken` 口径，%）。
///
/// vendor：CalcDefence.lua:2353-2358——
/// - `ArmourDoesNotApplyTo<X>DamageTaken` flag 时本类型份额为 0（instead 变体专属，
///   ModParser.lua:2523）；
/// - 物理隐式 BASE 100（vendor :1862-1863 `NewMod("ArmourAppliesToPhysicalDamageTaken",
///   "BASE", 100)` 注入 modDB；本实现在非 flag 分支内 `+100` 等价折算，不写 ModDb）；
/// - 元素类型额外叠加 `ArmourAppliesToElementalDamageTaken`（独立受
///   `ArmourDoesNotApplyToElementalDamageTaken` flag 门控）。
pub fn armour_applies_pct(db: &ModDb, cfg: &CalcConfig, dtype: DamageType) -> f64 {
    let name = dt_prefix(dtype);
    let mut pct = if db.flag(
        cfg,
        ModName::from(format!("ArmourDoesNotApplyTo{name}DamageTaken")),
    ) {
        0.0
    } else {
        let base = db.sum(
            ModType::Base,
            cfg,
            &[ModName::from(format!("ArmourAppliesTo{name}DamageTaken"))],
        );
        // 物理隐式 BASE 100（:1862-1863）：在函数内实现、不写 ModDb。
        if dtype == DamageType::Physical {
            base + 100.0
        } else {
            base
        }
    };
    if dtype.is_elemental()
        && !db.flag(
            cfg,
            ModName::from("ArmourDoesNotApplyToElementalDamageTaken"),
        )
    {
        pct += db.sum(
            ModType::Base,
            cfg,
            &[ModName::from("ArmourAppliesToElementalDamageTaken")],
        );
    }
    pct
}

/// 某伤害类型的有效适用护甲（`EffectiveAppliedArmour`）。
///
/// vendor：CalcDefence.lua:2336-2362——
/// `Armour × pct/100 × (1 + ArmourDefense)`（ArmourDefense = `Max("ArmourDefense")/100`，
/// :1392）`+ max(Evasion × pctEvasion/100, 0) + max(ES × pctES/100, 0)`；
/// Evasion/ES 份额各受 `<Def>DoesNotApplyTo<X>DamageTaken` flag 与
/// `<Def>AppliesTo<X>DamageTaken` BASE 控制。
pub fn effective_applied_armour(
    db: &ModDb,
    cfg: &CalcConfig,
    armour: f64,
    evasion: f64,
    energy_shield: f64,
    dtype: DamageType,
) -> f64 {
    let name = dt_prefix(dtype);
    let armour_pct = armour_applies_pct(db, cfg, dtype);
    // ArmourDefense：vendor :1392 `(modDB:Max(nil, "ArmourDefense") or 0) / 100`。
    let armour_defense = db
        .max_of(ModType::Base, cfg, &[ModName::from("ArmourDefense")])
        .max(0.0)
        / 100.0;
    let other_pct = |def: &str| -> f64 {
        if db.flag(
            cfg,
            ModName::from(format!("{def}DoesNotApplyTo{name}DamageTaken")),
        ) {
            0.0
        } else {
            db.sum(
                ModType::Base,
                cfg,
                &[ModName::from(format!("{def}AppliesTo{name}DamageTaken"))],
            )
        }
    };
    let from_armour = (armour * armour_pct / 100.0) * (1.0 + armour_defense);
    // Evasion/ES 借入项各自 max(…, 0)（vendor :2356/:2358）。
    let from_evasion = (evasion * other_pct("Evasion") / 100.0).max(0.0);
    let from_es = (energy_shield * other_pct("EnergyShield") / 100.0).max(0.0);
    round(from_armour + from_evasion + from_es)
}

// MitigationCtx：per-actor 减伤快照（整备与求值分离）

/// 不随单次击中变化的减伤上下文（per-type 数组下标 = `DamageType as usize`）。
///
/// vendor 对应 `actor.output` 的 per-type 中间量（CalcDefence.lua:2336-2437 写出、
/// :422-455 `takenHitFromDamage` 消费）。整备入口 [`build_mitigation_ctx`]；
/// 求值入口 [`taken_hit_from_damage`]（纯算术、不读 ModDb）。
#[derive(Debug, Clone, PartialEq)]
pub struct MitigationCtx {
    /// taken-as 转换矩阵 `shift[src][dst]`（fraction，含源保留；[`damage_shift_table`]）。
    pub shift: [[f64; 5]; 5],
    /// per-type 护甲适用百分比（[`armour_applies_pct`]；物理含隐式 100）。
    pub armour_applies_pct: [f64; 5],
    /// per-type 有效适用护甲（`<X>EffectiveAppliedArmour`，:2434）。
    pub effective_applied_armour: [f64; 5],
    /// per-type 固定减伤（`Base<X>DamageReduction`，%；已 `max(0)` 并按 per-type 上限
    /// clamp，:2336-2340）。
    pub flat_dr_pct: [f64; 5],
    /// per-type 减伤上限（`<X>DamageReductionMax`，%；与全局上限取 min，:2333）。
    pub dr_max_pct: [f64; 5],
    /// per-type 敌人压制（`<X>EnemyOverwhelm`，%；:2045/:2134——pobr 以玩家侧
    /// `Enemy<X>Overwhelm` BASE 建模）。
    pub overwhelm_pct: [f64; 5],
    /// per-type 抗性承受乘数（`<X>ResistTakenHitMulti = 1 − resist/100`，:2363/:2435；
    /// 物理无抗性恒 1。敌人穿透项留config_interpreter）。
    pub resist_taken_multi: [f64; 5],
    /// per-type 受击固定承受加量（`<X>takenFlat`，:2365-2373，Average 口径）。
    pub taken_flat: [f64; 5],
    /// per-type 减伤后乘数（`<X>AfterReductionTakenHitMulti` = taken inc/more（Average
    /// 口径）× deflect 乘数，:2436-2437；PoE2 无法术抑制）。
    pub after_reduction_multi: [f64; 5],
}

impl Default for MitigationCtx {
    /// 中性快照：shift 恒等、无护甲/减伤、上限 90（vendor Data.lua:178
    /// DamageReductionCap，与 `game_constants` fallback 同值）、各乘数 1。
    fn default() -> Self {
        let mut shift = [[0.0; 5]; 5];
        for (i, row) in shift.iter_mut().enumerate() {
            row[i] = 1.0;
        }
        Self {
            shift,
            armour_applies_pct: [100.0, 0.0, 0.0, 0.0, 0.0],
            effective_applied_armour: [0.0; 5],
            flat_dr_pct: [0.0; 5],
            dr_max_pct: [90.0; 5],
            overwhelm_pct: [0.0; 5],
            resist_taken_multi: [1.0; 5],
            taken_flat: [0.0; 5],
            after_reduction_multi: [1.0; 5],
        }
    }
}

/// [`build_mitigation_ctx`] 的非 ModDb 输入（防御面板终值 + 抗性终值）。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MitigationInputs {
    /// 面板护甲（全局 inc/more 之后的终值，vendor `output.Armour`）。
    pub armour: f64,
    /// 面板闪避（`output.Evasion`，Evasion 借入项基数）。
    pub evasion: f64,
    /// 面板 ES（`output.EnergyShield`，ES 借入项基数）。
    pub energy_shield: f64,
    /// per-type 抗性终值（%，cap 后；Physical 槽恒 0——物理减伤走护甲/flat DR）。
    pub resist_pct: [f64; 5],
    /// 偏斜几率（%；Track D 接线前为 0。vendor :2433：仅 `DeflectChance == 100` 时
    /// deflect 乘数生效）。
    pub deflect_chance_pct: f64,
    /// 偏斜减伤幅度（%；`DeflectEffect`）。
    pub deflect_effect_pct: f64,
}

/// 从 ModDb 整备 [`MitigationCtx`]（vendor CalcDefence.lua:2326-2437 防御减伤段的
/// per-type 中间量装配）。
pub fn build_mitigation_ctx(
    db: &ModDb,
    cfg: &CalcConfig,
    inputs: &MitigationInputs,
) -> MitigationCtx {
    // 全局减伤上限：`Max("DamageReductionMax") or DamageReductionCap(=90)`（:1862）。
    let global_dr_max = {
        let v = db.max_of(ModType::Base, cfg, &[ModName::from("DamageReductionMax")]);
        if v > 0.0 {
            v
        } else {
            cfg.constants
                .character()
                .maximum_physical_damage_reduction_pct
        }
    };
    // deflect 乘数：`DeflectChance == 100 and (1 − DeflectEffect/100) or 1`（:2433）。
    let deflect_multi = if inputs.deflect_chance_pct >= 100.0 {
        1.0 - inputs.deflect_effect_pct / 100.0
    } else {
        1.0
    };

    let mut ctx = MitigationCtx {
        shift: damage_shift_table(db, cfg),
        ..MitigationCtx::default()
    };
    for dt in DAMAGE_TYPE_BY_INDEX {
        let i = dt as usize;
        let name = dt_prefix(dt);
        // per-type 减伤上限与全局取 min（:2333）。
        ctx.dr_max_pct[i] = {
            let v = db.max_of(
                ModType::Base,
                cfg,
                &[ModName::from(format!("{name}DamageReductionMax"))],
            );
            if v > 0.0 {
                v.min(global_dr_max)
            } else {
                global_dr_max
            }
        };
        // 固定减伤：`max(0, Σ BASE(<X>DamageReduction[, ElementalDamageReduction]))`
        // 再 clamp per-type 上限（:2336-2340）。
        ctx.flat_dr_pct[i] = {
            let mut names = vec![ModName::from(format!("{name}DamageReduction"))];
            if dt.is_elemental() {
                names.push(ModName::from("ElementalDamageReduction"));
            }
            db.sum(ModType::Base, cfg, &names)
                .clamp(0.0, ctx.dr_max_pct[i])
        };
        // 敌人压制（pobr 词条 `Enemy<X>Overwhelm` BASE；目前仅物理有来源）。
        ctx.overwhelm_pct[i] = db.sum(
            ModType::Base,
            cfg,
            &[ModName::from(format!("Enemy{name}Overwhelm"))],
        );
        // 抗性承受乘数（:2363；敌穿透项留config）。
        ctx.resist_taken_multi[i] = 1.0 - inputs.resist_pct[i] / 100.0;
        // takenFlat（Average 口径，:2365-2372）：基础 hit 族 + Attack/Spell 各半、
        // 投射物变体各 1/4。
        ctx.taken_flat[i] = db.sum(
            ModType::Base,
            cfg,
            &[
                ModName::from("DamageTaken"),
                ModName::from(format!("{name}DamageTaken")),
                ModName::from("DamageTakenWhenHit"),
                ModName::from(format!("{name}DamageTakenWhenHit")),
            ],
        ) + db.sum(
            ModType::Base,
            cfg,
            &[
                ModName::from("DamageTakenFromAttacks"),
                ModName::from(format!("{name}DamageTakenFromAttacks")),
            ],
        ) / 2.0
            + db.sum(
                ModType::Base,
                cfg,
                &[ModName::from(format!(
                    "{name}DamageTakenFromProjectileAttacks"
                ))],
            ) / 4.0
            + db.sum(
                ModType::Base,
                cfg,
                &[
                    ModName::from("DamageTakenFromSpells"),
                    ModName::from(format!("{name}DamageTakenFromSpells")),
                ],
            ) / 2.0
            + db.sum(
                ModType::Base,
                cfg,
                &[
                    ModName::from("DamageTakenFromSpellProjectiles"),
                    ModName::from(format!("{name}DamageTakenFromSpellProjectiles")),
                ],
            ) / 4.0;
        // 减伤后乘数（:2429/:2436-2437）：taken inc/more（Average）× deflect；PoE2 无抑制。
        ctx.after_reduction_multi[i] = taken_mult_for_type_default(db, cfg, dt) * deflect_multi;
        // 护甲适用百分比与有效适用护甲（13-G7 百分比模型）。
        ctx.armour_applies_pct[i] = armour_applies_pct(db, cfg, dt);
        ctx.effective_applied_armour[i] = effective_applied_armour(
            db,
            cfg,
            inputs.armour,
            inputs.evasion,
            inputs.energy_shield,
            dt,
        );
    }
    ctx
}

// takenHitFromDamage 等价入口

/// 单类型 raw 进伤 → 经 taken-as 转换 + 减伤后的实际承受（总量, per-type 分量）。
///
/// vendor：CalcDefence.lua:422-455 `takenHitFromDamage`——对 `shift[src]` 的每个转换
/// 目标类型：
/// ```text
/// armourDR  = armourReductionF(<conv>EffectiveAppliedArmour, convertedDamage)
/// totalDR   = min(<src>DamageReductionMax, armourDR + <conv>flatDR)
/// drMulti   = 1 − max(min(<src>DamageReductionMax, totalDR − <conv>overwhelm), 0)/100
/// reduced   = round(max(converted × <conv>ResistTakenHitMulti × drMulti + <conv>takenFlat, 0)
///                   × <conv>AfterReductionTakenHitMulti)
/// ```
/// 注意：减伤上限按**源类型**索引（vendor :429/:431 `output[damageType..…]` 闭包捕获
/// 外层 damageType），其余量按转换后类型索引。`VaalArcticArmourMitigation`（:441，
/// PoE1 遗留）无词条来源，略去。
pub fn taken_hit_from_damage(
    raw_damage: f64,
    dtype: DamageType,
    mit: &MitigationCtx,
) -> (f64, [f64; 5]) {
    let src = dtype as usize;
    let dr_max = mit.dr_max_pct[src];
    let mut total = 0.0;
    let mut parts = [0.0; 5];
    for conv in DAMAGE_TYPE_BY_INDEX {
        let c = conv as usize;
        let convert_frac = mit.shift[src][c];
        let taken_flat = mit.taken_flat[c];
        if convert_frac <= 0.0 && taken_flat == 0.0 {
            continue;
        }
        let converted = raw_damage * convert_frac;
        let armour_dr_pct = armour_reduction(mit.effective_applied_armour[c], converted) * 100.0;
        let total_dr_pct = (armour_dr_pct + mit.flat_dr_pct[c]).min(dr_max);
        // vendor :431 `m_max(m_min(drMax, totalDR − overwhelm), 0)`，dr_max 恒 >0 → clamp 等价。
        let dr_multi = 1.0 - (total_dr_pct - mit.overwhelm_pct[c]).clamp(0.0, dr_max) / 100.0;
        let mult = mit.resist_taken_multi[c] * dr_multi;
        let reduced =
            vendor_round((converted * mult + taken_flat).max(0.0) * mit.after_reduction_multi[c]);
        total += reduced;
        parts[c] = reduced;
    }
    (total, parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// per-type 数组下标约定（与 pool_damage 同）：枚举序 Physical=0 … Chaos=4。
    #[test]
    fn damage_type_index_convention() {
        assert_eq!(DamageType::Physical as usize, 0);
        assert_eq!(DamageType::Fire as usize, 1);
        assert_eq!(DamageType::Cold as usize, 2);
        assert_eq!(DamageType::Lightning as usize, 3);
        assert_eq!(DamageType::Chaos as usize, 4);
    }

    /// vendor round = floor(x + 0.5)（Modules/Common.lua）。
    #[test]
    fn vendor_round_half_up() {
        assert_eq!(vendor_round(357.142_857), 357.0);
        assert_eq!(vendor_round(0.5), 1.0);
        assert_eq!(vendor_round(124.999), 125.0);
    }

    /// 中性 ctx：恒等 shift、乘数 1 → 承受 = round(raw)。
    #[test]
    fn neutral_ctx_identity() {
        let ctx = MitigationCtx::default();
        let (sum, parts) = taken_hit_from_damage(1000.0, DamageType::Fire, &ctx);
        assert_eq!(sum, 1000.0);
        assert_eq!(parts[DamageType::Fire as usize], 1000.0);
        assert_eq!(parts[DamageType::Physical as usize], 0.0);
    }
}
