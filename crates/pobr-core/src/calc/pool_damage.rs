//! 扣池状态机类型契约（M2-W0.3）——对应 PoB2 `CalcDefence.lua:461-678
//! reducePoolsByDamage`（顺序：allies → aegis → guard → ward → ES(bypass) → MoM →
//! loss-prevention → life → overkill）。
//!
//! 本文件在 W0 只锁**类型与签名**（Track A 与 Track F 之间的唯一接口面）：
//! - [`reduce_pools`]：状态机本体（Track A 实现；W0 为 `todo!` 占位，无任何消费者）；
//! - [`pool_protected`] / [`life_hit_pool_with_loss_prevention`]：两个小公式 W0 即实现
//!   并以单测锁数值（CalcDefence.lua:2746 / :456-459）。
//!
//! 纯函数约定（蓝图 §0 约束 2 / P17）：输入输出皆值类型，**不写 `Env`**、不读 ModDb
//! （flag/比例由 Track A 的 pool_setup 在整备阶段固化进 [`PoolCtx`]）、不持共享可变状态；
//! 对 `Env`/`OutputTable` 的写入仍集中在 `perform.rs`（Track F）。

use pobr_data::constants::DamageType;

/// 五类型伤害向量（taken 乘区之后、入池之前；PoB2 damageTable 等价）。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TypedDamage {
    pub physical: f64,
    pub fire: f64,
    pub cold: f64,
    pub lightning: f64,
    pub chaos: f64,
}

impl TypedDamage {
    /// 按 [`DamageType`] 取分量。
    pub fn get(&self, dtype: DamageType) -> f64 {
        match dtype {
            DamageType::Physical => self.physical,
            DamageType::Fire => self.fire,
            DamageType::Cold => self.cold,
            DamageType::Lightning => self.lightning,
            DamageType::Chaos => self.chaos,
        }
    }

    /// 五分量之和（总进伤）。
    pub fn total(&self) -> f64 {
        self.physical + self.fire + self.cold + self.lightning + self.chaos
    }
}

/// 盟友先扣层（frost shield / spectre / totem / vaal rejuvenation totem / radiance
/// sentinel / soul link……，PoB2 poolTable 的 allies 段，CalcDefence.lua:466-489）。
///
/// 各层 `{remaining, percent}` 比例先扣，**不计入 recoupable**（:529-536 在 allies
/// 之后才记 damageTakenThatCanBeRecouped）。
#[derive(Debug, Clone, PartialEq)]
pub struct AllyLayer {
    /// 层 ID（稳定标识，breakdown / resources_lost 用），如 `"frostShield"`。
    pub id: &'static str,
    /// 该层剩余可吸收量。
    pub remaining: f64,
    /// 该层分担比例（%，0-100；PoB2 `takenFlat` 前按 percent 分流）。
    pub mitigation_pct: f64,
}

/// 扣池前的全部池快照（对应 PoB2 poolTable；构造一次、EHP 循环中按值传递）。
///
/// per-type 数组下标约定 = [`DamageType`] 枚举序（Physical/Fire/Cold/Lightning/Chaos），
/// 与 [`TypedDamage`] 字段一一对应。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PoolState {
    /// 盟友先扣层（本阶段玩家无 frost shield 等来源时为空 Vec，结构保留）。
    pub allies: Vec<AllyLayer>,
    /// 共享 Aegis（全类型，CalcDefence.lua:490-498）。
    pub aegis_shared: f64,
    /// 元素共享 Aegis（仅元素类型可扣，:544-549）。
    pub aegis_shared_elemental: f64,
    /// per-type Aegis。
    pub aegis_by_type: [f64; 5],
    /// 共享 Guard 池与吸收比例（%；CalcDefence.lua:500-505、:561-567）。
    pub guard_shared: f64,
    pub guard_shared_rate: f64,
    /// per-type Guard 池与吸收比例（%）。
    pub guard_by_type: [f64; 5],
    pub guard_rate_by_type: [f64; 5],
    /// 结界（ward；`×(1−WardBypass/100)` 吸收，`WardNotBreak` 时返还，:568-577）。
    pub ward: f64,
    /// 能量护盾池（EHP 口径 = EnergyShieldRecoveryCap）。
    pub energy_shield: f64,
    /// 法力池（EHP 口径 = ManaUnreserved；MoM/EB 消耗）。
    pub mana: f64,
    /// 生命池（EHP 口径 = LifeRecoverable）。
    pub life: f64,
    /// loss-prevention 转入的延迟损失累计（above-half / below-half 分段，
    /// CalcDefence.lua:611-655）。
    pub life_loss_lost_over_time: f64,
    pub life_below_half_loss_lost_over_time: f64,
}

/// 不随单次击中变化的上下文（flag / 比例从 ModDb 读出后固化；状态机本体不读 ModDb）。
///
/// 整备入口（Track A 的 `pool_setup.rs::build_pool_ctx`）对照：ES bypass per-type
/// CalcDefence.lua:2707-2723、MoM shared/per-type :2726-2820、loss-prevention :2816。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PoolCtx {
    /// 最大生命（分段生命命中池的 half-life 基准）。
    pub max_life: f64,
    /// per-type ES bypass（%，0-100；chaos 默认 100 除非 ChaosNotBypass/CI 等改写）。
    pub es_bypass_by_type: [f64; 5],
    /// 共享 MoM 比例（%，0-100；`min(Σ DamageTakenFromManaBeforeLife, 100)`）。
    pub mom_shared: f64,
    /// per-type MoM 比例（%；`<X>DamageTakenFromManaBeforeLife`，与 shared 合计 cap 100）。
    pub mom_by_type: [f64; 5],
    /// Ward bypass（%，0-100）。
    pub ward_bypass: f64,
    /// EternalLife 分支（CalcDefence.lua:588-594，与 EB 互斥）。
    pub eternal_life: bool,
    /// EB（`EnergyShieldProtectsMana` flag）：ES 嵌套保护 Mana（:597-603 MoMEBPool 公式）。
    pub eb: bool,
    /// chaos 对 ES 不双倍（默认 chaos `esDamageTypeMultiplier = 2`，:580-586）。
    pub chaos_not_double_es: bool,
    /// WardNotBreak：ward 吸收后不破（返还），EHP 中伤害低于 ward 时致死击数 = ∞。
    pub ward_not_break: bool,
    /// 生命损失防止（%；LifeLossPrevented，above-half 段）。
    pub prevented_life_loss: f64,
    /// 半血以下生命损失防止（%；LifeLossBelowHalfPrevented）。
    pub life_loss_below_half_prevented: f64,
}

/// 一次扣池的完整结果（PoB2 reducePoolsByDamage 返回的 poolsRemaining 等价 + 扣量明细）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PoolsAfter {
    /// 扣减后的池快照。
    pub pools: PoolState,
    /// per-type 可 recoup 伤害（allies 先扣段**不计入**，CalcDefence.lua:529-536）。
    pub recoupable_by_type: [f64; 5],
    /// 溢出击杀量（生命扣穿后剩余伤害，EHP 小数击数折算用，:660-668）。
    pub overkill: f64,
    /// 命中池余量（hit pool 口径，max-hit 判定用）。
    pub hit_pool_remaining: f64,
    /// 每类型每层扣量明细（breakdown 用）：(伤害类型, 层 ID, 扣量)。
    /// 层 ID 取值：`"ally:<id>"`/`"aegis"`/`"guard"`/`"ward"`/`"energy_shield"`/
    /// `"mana"`/`"life"`（Track A 实现时锁定）。
    pub resources_lost: Vec<(DamageType, &'static str, f64)>,
}

/// 扣池状态机（纯函数）。顺序固定（CalcDefence.lua:461-678）：
/// allies → aegis（per-type → sharedElemental(仅元素) → shared）→ guard（per-type 与
/// shared 各按 AbsorbRate% 吸收）→ ward（×(1−WardBypass/100)）——以上**正序**遍历伤害
/// 类型；ES（chaos 双倍除非 ChaosNotDoubleESDamage；per-type bypass；EternalLife/EB
/// 分支）→ MoM（`MoMPool = min(lifeHitPool/(1−MoM)−lifeHitPool, mana)`）→
/// loss-prevention（above/below half 分段）→ life → overkill——ES 起**逆序**遍历
/// （:578 `for i=#dmgTypeList,1,-1`，dmgTypeList = Physical,Lightning,Cold,Fire,Chaos，
/// 逆序即 Chaos 先）。
///
/// W0.3 仅锁契约：**Track A 实现**（蓝图 §2 Track A），当前无任何消费者。
pub fn reduce_pools(pools: &PoolState, hit: &TypedDamage, ctx: &PoolCtx) -> PoolsAfter {
    let _ = (pools, hit, ctx);
    todo!("M2 Track A：reducePoolsByDamage 状态机实现（CalcDefence.lua:461-678）")
}

/// X-protects-Y 通用原语（CalcDefence.lua:2746 / :2837 / :3546-3550）：
/// `poolProtected = source_pool / rate × (1 − rate)`。
///
/// `rate_fraction` 为保护比例（小数）：
/// - `rate ≥ 1` → 全额保护（PoB2 `m_huge`），返回 `f64::INFINITY`；
/// - `rate ≤ 0` → 无保护层，返回 0；
/// - 其余 → 公式值（source 池按 rate 分担时，能保护的"另一侧"额度）。
///
/// MoM / Guard / Ward bypass / SoulLink / EB 嵌套全部复用本原语。
pub fn pool_protected(source_pool: f64, rate_fraction: f64) -> f64 {
    if rate_fraction >= 1.0 {
        return f64::INFINITY;
    }
    if rate_fraction <= 0.0 {
        return 0.0;
    }
    source_pool / rate_fraction * (1.0 - rate_fraction)
}

/// 分段生命命中池（CalcDefence.lua:456-459 `calcLifeHitPoolWithLossPrevention`）：
///
/// ```text
/// halfLife = maxLife × 0.5
/// aboveLow = max(life − halfLife, 0)
/// pool = aboveLow / (1 − lossPrev/100)
///      + min(life, halfLife) / (1 − belowHalfPrev/100) / (1 − lossPrev/100)
/// ```
///
/// `loss_prev_pct` / `below_half_prev_pct` 为百分比（0-100）；任一达 100 时分母为 0，
/// 池自然为 ∞（与 vendor 除零 → m_huge 行为一致，不额外 clamp）。
pub fn life_hit_pool_with_loss_prevention(
    life: f64,
    max_life: f64,
    loss_prev_pct: f64,
    below_half_prev_pct: f64,
) -> f64 {
    let half_life = max_life * 0.5;
    let above_low = (life - half_life).max(0.0);
    above_low / (1.0 - loss_prev_pct / 100.0)
        + life.min(half_life) / (1.0 - below_half_prev_pct / 100.0) / (1.0 - loss_prev_pct / 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// poolProtected 公式锁值（CalcDefence.lua:2746：`source/(rate)×(1−rate)`）。
    /// 手算：1000/0.3×0.7 = 2333.33…；500/0.5×0.5 = 500。
    #[test]
    fn pool_protected_formula_locked() {
        assert!((pool_protected(1000.0, 0.3) - 1000.0 / 0.3 * 0.7).abs() < 1e-9);
        assert_eq!(pool_protected(500.0, 0.5), 500.0);
    }

    /// rate ≥ 1 → ∞ 保护（vendor `sharedMindOverMatter >= 100` → m_huge，:2748-2751）；
    /// rate ≤ 0 → 无保护层（0）。
    #[test]
    fn pool_protected_boundary_rates() {
        assert_eq!(pool_protected(800.0, 1.0), f64::INFINITY);
        assert_eq!(pool_protected(800.0, 1.5), f64::INFINITY);
        assert_eq!(pool_protected(800.0, 0.0), 0.0);
        assert_eq!(pool_protected(800.0, -0.2), 0.0);
    }

    /// 无损失防止时分段池退化为 life 本值（CalcDefence.lua:456-459，prev=0 →
    /// aboveLow + min(life, half) = life）。
    #[test]
    fn life_hit_pool_without_prevention_equals_life() {
        assert_eq!(
            life_hit_pool_with_loss_prevention(1000.0, 1000.0, 0.0, 0.0),
            1000.0
        );
        assert_eq!(
            life_hit_pool_with_loss_prevention(400.0, 1000.0, 0.0, 0.0),
            400.0
        );
    }

    /// 全段 20% 损失防止：满血 1000/1000 → 500/0.8 + 500/0.8 = 1250（手算，
    /// CalcDefence.lua:459 两段同除 (1−lossPrev/100)）。
    #[test]
    fn life_hit_pool_with_full_range_prevention() {
        assert!(
            (life_hit_pool_with_loss_prevention(1000.0, 1000.0, 20.0, 0.0) - 1250.0).abs() < 1e-9
        );
    }

    /// 半血以下 50% 防止：满血 1000/1000 → 500 + 500/0.5 = 1500；
    /// 双段叠加（life=800, max=1000, lossPrev=20, belowHalf=50）→
    /// 300/0.8 + 500/0.5/0.8 = 375 + 1250 = 1625（手算）。
    #[test]
    fn life_hit_pool_below_half_prevention_segments() {
        assert!(
            (life_hit_pool_with_loss_prevention(1000.0, 1000.0, 0.0, 50.0) - 1500.0).abs() < 1e-9
        );
        assert!(
            (life_hit_pool_with_loss_prevention(800.0, 1000.0, 20.0, 50.0) - 1625.0).abs() < 1e-9
        );
    }

    /// 半血以下段只作用于 min(life, halfLife)：当前生命低于半血时 aboveLow=0，
    /// 整池 = life/(1−belowHalf/100)（400/0.5 = 800，手算）。
    #[test]
    fn life_hit_pool_when_life_below_half() {
        assert_eq!(
            life_hit_pool_with_loss_prevention(400.0, 1000.0, 0.0, 50.0),
            800.0
        );
    }

    /// 100% 损失防止 → 池 ∞（vendor 除零 → m_huge 等价，不 clamp）。
    #[test]
    fn life_hit_pool_full_prevention_is_infinite() {
        assert_eq!(
            life_hit_pool_with_loss_prevention(1000.0, 1000.0, 100.0, 0.0),
            f64::INFINITY
        );
    }

    /// TypedDamage 访问器与 PoolState/PoolCtx/PoolsAfter 的 Default 构造（契约可编译 +
    /// 中性默认锁定；reduce_pools 本体由 Track A 实现）。
    #[test]
    fn contract_types_construct_with_neutral_defaults() {
        let hit = TypedDamage {
            physical: 100.0,
            fire: 50.0,
            ..Default::default()
        };
        assert_eq!(hit.get(DamageType::Physical), 100.0);
        assert_eq!(hit.get(DamageType::Chaos), 0.0);
        assert_eq!(hit.total(), 150.0);

        let pools = PoolState::default();
        assert!(pools.allies.is_empty());
        assert_eq!(pools.life, 0.0);

        let ctx = PoolCtx::default();
        assert!(!ctx.eb);
        assert_eq!(ctx.mom_shared, 0.0);

        let after = PoolsAfter::default();
        assert_eq!(after.overkill, 0.0);
        assert!(after.resources_lost.is_empty());
    }
}
