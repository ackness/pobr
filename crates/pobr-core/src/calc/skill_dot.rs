//! 技能 DoT（damage over time）计算模块——M4-T4 W-D1 **骨架**（蓝图
//! `audits/rearchitecture-2026-06-10/blueprints/m4-offence-deep.md` §2-T4）。
//!
//! 本波只落契约类型 + 零行为填充函数；真正的计算（dotCfg flag 剥除 → 逐类型
//! `baseVal × inc × more × DotMultiplier × effMult` → `TotalDotInstance` /
//! `TotalDot` clamp `DotDpsCap` → 末端合并 DPS 族）**依赖 T1 W-A1 的
//! ModFlags Hit/Dot 位**，等主线通知后接线：
//!
//! - dotCfg：`flags = ModFlag.Dot | skillCfg.flags`，再按 dotIs* 五布尔剥除
//!   Area/Projectile/Spell/Attack/Hit 位（vendor `CalcOffence.lua:5831-5860`）；
//!   dotIs* 数据 = [`pobr_data::catalog::DotFlags`]（statSet 级，W-D1 数据侧
//!   已入库；`verified == false` 的技能为保守默认全 false）。
//! - 逐类型 dot 基值 = `base_<type>_damage_to_deal_per_minute / 60`
//!   （granted_effect_stat_sets stat，经 skill_stat_map `XDot` 映射）。
//! - `DotCanStack`：`TotalDot = min(instance × speed × Duration ×
//!   dpsMultiplier × quantityMultiplier, DotDpsCap)`（`:5931`）；速率按
//!   keywordFlags Mine/Trap 换 `MineLayingSpeed/TrapThrowingSpeed`——pobr 无
//!   图腾/陷阱吞吐（12-G11，M4 不做），该分支留 match 臂 + None 退 Speed。
//! - 末端合并族（`:6093-6234`）：`TotalDotDPS = Σ(技能 dot + 异常 DoT 各值)`
//!   clamp `DotDpsCap`（异常侧现值见 `ailment.rs`/`perform.rs`，只读消费）。
//!
//! 字段命名契约（蓝图 §3.3 条目 6，合入 display_catalog 后不再改）：
//! `skill_dot_instance` / `skill_total_dot` / `total_dot_dps` /
//! `with_dot_dps` / `combined_dps`。

use super::output::OutputTable;

/// 技能 DoT 计算结果（OutputTable `// === M4-T4 ===` 区块的来源值）。
///
/// 全零 = 中性（无技能 DoT；接线前 perform 不调用本模块，输出恒中性）。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SkillDotOutput {
    /// 单实例技能 DoT DPS（PoB2 `TotalDotInstance`，clamp `DotDpsCap`）。
    pub skill_dot_instance: f64,
    /// 可叠加实例累计后的技能 DoT DPS（PoB2 `TotalDot`；不可叠加时 = instance）。
    pub skill_total_dot: f64,
    /// 全部 DoT 来源合计 DPS（PoB2 `TotalDotDPS` = 技能 dot + poison/caustic/
    /// ignite/burning/bleed/corrupting/decay，clamp `DotDpsCap`）。
    pub total_dot_dps: f64,
    /// 击中 DPS + DoT（PoB2 `WithDotDPS`）。
    pub with_dot_dps: f64,
    /// 综合 DPS（PoB2 `CombinedDPS`）。
    pub combined_dps: f64,
}

/// 把技能 DoT 结果写入 [`OutputTable`] 的 M4-T4 契约字段（纯字段搬运，零计算）。
///
/// 接线点：W-D1 calc 落地后由 `perform.rs` fill 段调用（函数级新增
/// `fill_skill_dot`，蓝图 §3.2 共享文件规则）；当前无调用方，输出保持
/// `Default` 中性零。
pub fn fill_skill_dot(output: &mut OutputTable, dot: &SkillDotOutput) {
    output.skill_dot_instance = dot.skill_dot_instance;
    output.skill_total_dot = dot.skill_total_dot;
    output.total_dot_dps = dot.total_dot_dps;
    output.with_dot_dps = dot.with_dot_dps;
    output.combined_dps = dot.combined_dps;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 中性不变式：默认 SkillDotOutput 全零，fill 后 OutputTable 契约字段不变
    /// （接线前零行为）。
    #[test]
    fn default_is_neutral_zero() {
        let dot = SkillDotOutput::default();
        let mut out = OutputTable::default();
        let before = out.clone();
        fill_skill_dot(&mut out, &dot);
        assert_eq!(out, before, "默认填充必须零行为");
    }

    /// 契约字段搬运：五个值各自落到同名 OutputTable 字段。
    #[test]
    fn fill_transfers_all_contract_fields() {
        let dot = SkillDotOutput {
            skill_dot_instance: 1.0,
            skill_total_dot: 2.0,
            total_dot_dps: 3.0,
            with_dot_dps: 4.0,
            combined_dps: 5.0,
        };
        let mut out = OutputTable::default();
        fill_skill_dot(&mut out, &dot);
        assert_eq!(out.skill_dot_instance, 1.0);
        assert_eq!(out.skill_total_dot, 2.0);
        assert_eq!(out.total_dot_dps, 3.0);
        assert_eq!(out.with_dot_dps, 4.0);
        assert_eq!(out.combined_dps, 5.0);
    }
}
