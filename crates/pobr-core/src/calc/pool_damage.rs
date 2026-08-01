//! 扣池状态机（13-G2/G5）——对应 PoB2 `CalcDefence.lua:461-678
//! reducePoolsByDamage`（顺序：allies → aegis → guard → ward → ES(bypass) → MoM →
//! loss-prevention → life → overkill）与 :3540-3601 的 max-hit TotalHitPool 扩展层。
//!
//! 模块切分：
//! - 本文件 = 状态机求值（[`reduce_pools`]）+ 池公式原语（[`pool_protected`] /
//!   [`life_hit_pool_with_loss_prevention`] / [`apply_protected_layer`]）+ max-hit
//!   池层（[`total_hit_pool_base`] / [`extend_total_hit_pool`]）；
//! - 整备（从 ModDb 读 bypass/MoM/guard/aegis 等构造 [`PoolCtx`]/[`PoolState`]）在
//!   `pool_setup.rs`——**本文件不读 ModDb**（整备与求值分离）。
//!
//! 纯函数约定：输入输出皆值类型，**不写 `Env`**、不持共享
//! 可变状态；对 `Env`/`OutputTable` 的写入仍集中在 `perform.rs`（Track F）。

use pobr_data::constants::DamageType;

/// PoB2 防御侧伤害类型遍历顺序（`CalcDefence.lua:27`
/// `dmgTypeList = {"Physical", "Lightning", "Cold", "Fire", "Chaos"}`）。
///
/// 与 pobr [`DamageType`] 枚举序（Physical/Fire/Cold/Lightning/Chaos，per-type 数组
/// 下标）**不同**：状态机按本序正序扣 allies→ward，按本序**逆序**（Chaos 先）扣
/// ES→life（:578 `for i=#dmgTypeList,1,-1`）。共享池（shared aegis/guard/ward/ES/
/// mana/life）被跨类型顺次消耗，遍历序影响结果，必须对齐 vendor。
pub const POB2_DAMAGE_ORDER: [DamageType; 5] = [
    DamageType::Physical,
    DamageType::Lightning,
    DamageType::Cold,
    DamageType::Fire,
    DamageType::Chaos,
];

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
/// 各层 `{remaining, percent}` 比例先扣，**不计入 recoupable**（:529-530 注释
/// 「taken before you does not count as you taking damage」）。
#[derive(Debug, Clone, PartialEq)]
pub struct AllyLayer {
    /// 层 ID（稳定标识，breakdown / resources_lost 用），如 `"frostShield"`。
    pub id: &'static str,
    /// 该层剩余可吸收量。
    pub remaining: f64,
    /// 该层分担比例（%，0-100；CalcDefence.lua:527 `damageRemainder × percent` 分流）。
    pub mitigation_pct: f64,
    /// 仅对单一伤害类型生效的层（:526 `allyValues.damageType` 过滤；`None` = 全类型）。
    pub damage_type: Option<DamageType>,
}

/// 扣池前的全部池快照（对应 PoB2 poolTable；构造一次、EHP 循环中按值传递）。
///
/// per-type 数组下标约定 = [`DamageType`] 枚举序（Physical/Fire/Cold/Lightning/Chaos），
/// 与 [`TypedDamage`] 字段一一对应；遍历序见 [`POB2_DAMAGE_ORDER`]。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PoolState {
    /// 盟友先扣层（本阶段玩家无 frost shield 等来源时为空 Vec，结构保留）。
    pub allies: Vec<AllyLayer>,
    /// 共享 Aegis（全类型，CalcDefence.lua:551-556）。
    pub aegis_shared: f64,
    /// 元素共享 Aegis（仅元素类型可扣，:545-550）。
    pub aegis_shared_elemental: f64,
    /// per-type Aegis（:539-544）。
    pub aegis_by_type: [f64; 5],
    /// 共享 Guard 池与吸收比例（%；CalcDefence.lua:563-568 / 整备 :2823-2826）。
    pub guard_shared: f64,
    pub guard_shared_rate: f64,
    /// per-type Guard 池与吸收比例（%；:557-562 / 整备 :2838-2845）。
    pub guard_by_type: [f64; 5],
    pub guard_rate_by_type: [f64; 5],
    /// 结界（ward；`×(1−WardBypass/100)` 吸收，`WardNotBreak` 时返还，:569-573）。
    pub ward: f64,
    /// 能量护盾池（EHP 口径 = EnergyShieldRecoveryCap）。
    pub energy_shield: f64,
    /// 法力池（EHP 口径 = ManaUnreserved；MoM/EB 消耗）。
    pub mana: f64,
    /// 生命池（EHP 口径 = LifeRecoverable）。
    pub life: f64,
    /// loss-prevention 转入的延迟损失累计（above-half / below-half 分段，
    /// CalcDefence.lua:611-651）。
    pub life_loss_lost_over_time: f64,
    pub life_below_half_loss_lost_over_time: f64,
}

/// 不随单次击中变化的上下文（flag / 比例从 ModDb 读出后固化；状态机本体不读 ModDb）。
///
/// 整备入口（`pool_setup.rs::build_pool_ctx`）对照：ES bypass per-type
/// CalcDefence.lua:2707-2722、MoM shared/per-type :2728/:2773、loss-prevention :2662-2665。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PoolCtx {
    /// 最大生命（分段生命命中池的 half-life 基准，vendor `output.Life`）。
    pub max_life: f64,
    /// per-type ES bypass（%，0-100；vendor :2715 Override 或 Σ BASE，:2720 clamp。
    /// 注意 PoE2 下混沌**不**默认 bypass——混沌对 ES 双倍扣减（:582）取而代之）。
    pub es_bypass_by_type: [f64; 5],
    /// 共享 MoM 比例（%，0-100；`min(Σ DamageTakenFromManaBeforeLife, 100)`，:2728）。
    pub mom_shared: f64,
    /// per-type MoM 比例（%；`<X>DamageTakenFromManaBeforeLife`，
    /// `min(Σ, 100 − shared)`，:2773）。
    pub mom_by_type: [f64; 5],
    /// Ward bypass（%，0-100 预期；vendor :572 不 clamp，此处保持原值语义）。
    pub ward_bypass: f64,
    /// EternalLife 分支（CalcDefence.lua:587-594，与普通 ES 分支互斥；bypass 部分
    /// 直接「免除」而非穿透到生命）。
    pub eternal_life: bool,
    /// EB（`EnergyShieldProtectsMana` flag）：ES 嵌套保护 Mana（池整备 :2735-2744 /
    /// :2782-2791 的 manaProtected 公式；状态机本体不直接分支，经整备结果生效）。
    pub eb: bool,
    /// chaos 对 ES 不双倍（默认 chaos `esDamageTypeMultiplier = 2`，:582）。
    pub chaos_not_double_es: bool,
    /// WardNotBreak：ward 吸收后不破（:509 返还原值），EHP 中伤害低于 ward 时
    /// 致死击数 = ∞（:3030，Track F）。
    pub ward_not_break: bool,
    /// 生命损失防止（%；`min(Σ LifeLossPrevented, 100)`，:2662）。
    pub prevented_life_loss: f64,
    /// 半血以下生命损失防止（%；Σ `LifeLossBelowHalfPrevented` 原始 BASE 和，:514/:2664）。
    pub life_loss_below_half_prevented: f64,
}

impl PoolCtx {
    /// vendor `output.preventedLifeLossBelowHalf`（:2665）：
    /// `(1 − preventedLifeLoss/100) × Σ LifeLossBelowHalfPrevented`。
    /// reduce_pools 的 :623 分支判据（≠0 走 above/below-half 分段）。
    fn prevented_life_loss_below_half_effective(&self) -> f64 {
        (1.0 - self.prevented_life_loss / 100.0) * self.life_loss_below_half_prevented
    }
}

/// 一次扣池的完整结果（PoB2 reducePoolsByDamage 返回的 poolsRemaining 等价 + 扣量明细）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PoolsAfter {
    /// 扣减后的池快照（ward 字段 = 返还语义：`WardNotBreak` 时回满原值、否则 0，:509/:666）。
    pub pools: PoolState,
    /// per-type 可 recoup 伤害（allies 先扣段**不计入**，CalcDefence.lua:529-530）。
    pub recoupable_by_type: [f64; 5],
    /// 溢出击杀量（生命扣穿后剩余伤害，EHP 小数击数折算用，:619-621/:656）。
    pub overkill: f64,
    /// 命中池余量（:659-660：扣后 life 的分段命中池 + MoM 池余量 + ES 池余量，floor）。
    pub hit_pool_remaining: f64,
    /// 每类型每层扣量明细（breakdown 用）：(伤害类型, 层 ID, 扣量)。同一 (类型, 层)
    /// 可能出现多条（如 loss-prevention 段与普通段各扣一次 life），调用方自行汇总。
    /// 层 ID 取值（vendor resourcesLostToTypeDamage 键的蛇形对应）：
    /// `"ally:<id>"` / `"aegis"` / `"shared_elemental_aegis"` / `"shared_aegis"` /
    /// `"guard"` / `"shared_guard"` / `"ward"` / `"energy_shield"` /
    /// `"eternal_life_prevented"` / `"mana"` / `"life"` / `"life_loss_prevented"` /
    /// `"overkill"`。
    /// 注：vendor 仅在扣量 ≥1 时落 breakdown 行（显示层裁剪）；本实现保留全部 >0
    /// 明细，不丢精度。
    pub resources_lost: Vec<(DamageType, &'static str, f64)>,
}

/// 扣池状态机（纯函数，逐行对照 CalcDefence.lua:461-678）。
///
/// 顺序固定：
/// 1. **正序**（[`POB2_DAMAGE_ORDER`]）逐类型：allies（:525-531，比例先扣、不计
///    recoupable）→ recoupable 记账（:531）→ aegis（per-type :539 → sharedElemental
///    仅元素 :545 → shared :551）→ guard（per-type :557 与 shared :563 各按
///    AbsorbRate% 比例吸收）→ ward（:569 `×(1−WardBypass/100)`）；
/// 2. **逆序**（:578，Chaos 先）逐类型：ES（chaos 双倍除非 `ChaosNotDoubleESDamage`
///    :582；per-type bypass；`EternalLife` :587-594 与普通分支 :594-601 互斥）→
///    MoM（:602-609 `MoMPool = min(lifeHitPool/(1−MoM)−lifeHitPool, mana)`）→
///    loss-prevention（:611-651 above/below half 分段，超出生命可承部分先记 overkill）
///    → life（:651-655）→ overkill（:656）。
///
/// 伤害分量 ≤0 的类型跳过（vendor 以 damageTable 键存在性门控，EHP/max-hit 调用方
/// 只传正分量）。
pub fn reduce_pools(pools: &PoolState, hit: &TypedDamage, ctx: &PoolCtx) -> PoolsAfter {
    let mut allies = pools.allies.clone();
    let mut aegis_by_type = pools.aegis_by_type;
    let mut aegis_shared_elemental = pools.aegis_shared_elemental;
    let mut aegis_shared = pools.aegis_shared;
    let mut guard_by_type = pools.guard_by_type;
    let mut guard_shared = pools.guard_shared;
    let mut ward = pools.ward;
    let mut energy_shield = pools.energy_shield;
    let mut mana = pools.mana;
    let mut life = pools.life;
    let mut life_loss_lost_over_time = pools.life_loss_lost_over_time;
    let mut life_below_half_loss_lost_over_time = pools.life_below_half_loss_lost_over_time;

    // :509 ward 返还语义：WardNotBreak 持有原值，否则击后归零。
    let restore_ward = if ctx.ward_not_break { ward } else { 0.0 };

    let mut recoupable_by_type = [0.0_f64; 5];
    let mut overkill = 0.0_f64;
    let mut resources_lost: Vec<(DamageType, &'static str, f64)> = Vec::new();
    // :520-521：MoM/ES 池余量跨类型取 min；m_huge 哨兵 = 该池从未被求值。
    let mut mom_pool_remaining = f64::INFINITY;
    let mut es_pool_remaining = f64::INFINITY;

    // 记一条扣量明细（保留全部 >0 明细；vendor 仅 ≥1 落 breakdown，显示层差异）。
    let lose = |list: &mut Vec<(DamageType, &'static str, f64)>,
                dtype: DamageType,
                layer: &'static str,
                amount: f64| {
        if amount > 0.0 {
            list.push((dtype, layer, amount));
        }
    };

    // 前半段：正序 allies → aegis → guard → ward（:524-575）
    let mut remainder_before_es = [0.0_f64; 5];
    for dtype in POB2_DAMAGE_ORDER {
        let idx = dtype as usize;
        let mut rem = hit.get(dtype);
        if rem <= 0.0 {
            continue;
        }
        // allies（:525-530）：各层按 percent 比例分担，cap 于层余量。
        for ally in allies.iter_mut() {
            if (ally.damage_type.is_none() || ally.damage_type == Some(dtype))
                && ally.remaining > 0.0
            {
                let temp = (rem * ally.mitigation_pct / 100.0).min(ally.remaining);
                ally.remaining -= temp;
                rem -= temp;
                if temp > 0.0 {
                    resources_lost.push((dtype, ally.id, temp));
                }
            }
        }
        // :531 allies 之后才计 recoupable（先扣段不算「你受到的伤害」）。
        recoupable_by_type[idx] += rem;
        // aegis：per-type（:539）→ sharedElemental 仅元素（:545）→ shared（:551）。
        if aegis_by_type[idx] > 0.0 {
            let temp = rem.min(aegis_by_type[idx]);
            aegis_by_type[idx] -= temp;
            rem -= temp;
            lose(&mut resources_lost, dtype, "aegis", temp);
        }
        if DamageType::ELEMENTAL.contains(&dtype) && aegis_shared_elemental > 0.0 {
            let temp = rem.min(aegis_shared_elemental);
            aegis_shared_elemental -= temp;
            rem -= temp;
            lose(&mut resources_lost, dtype, "shared_elemental_aegis", temp);
        }
        if aegis_shared > 0.0 {
            let temp = rem.min(aegis_shared);
            aegis_shared -= temp;
            rem -= temp;
            lose(&mut resources_lost, dtype, "shared_aegis", temp);
        }
        // guard：per-type（:557）与 shared（:563）各按 AbsorbRate% 比例吸收、cap 于池量。
        if guard_by_type[idx] > 0.0 {
            let temp = (rem * pools.guard_rate_by_type[idx] / 100.0).min(guard_by_type[idx]);
            guard_by_type[idx] -= temp;
            rem -= temp;
            lose(&mut resources_lost, dtype, "guard", temp);
        }
        if guard_shared > 0.0 {
            let temp = (rem * pools.guard_shared_rate / 100.0).min(guard_shared);
            guard_shared -= temp;
            rem -= temp;
            lose(&mut resources_lost, dtype, "shared_guard", temp);
        }
        // ward（:569-573）：`×(1−WardBypass/100)` 部分被 ward 吸收。
        if ward > 0.0 {
            let temp = (rem * (1.0 - ctx.ward_bypass / 100.0)).min(ward);
            ward -= temp;
            rem -= temp;
            lose(&mut resources_lost, dtype, "ward", temp);
        }
        remainder_before_es[idx] = rem;
    }

    // 后半段：逆序（Chaos 先）ES → MoM → loss-prevention → life → overkill（:578-657）
    for dtype in POB2_DAMAGE_ORDER.into_iter().rev() {
        let idx = dtype as usize;
        let mut rem = remainder_before_es[idx];
        if rem <= 0.0 {
            continue;
        }
        // :582 chaos 对 ES 双倍（除非 ChaosNotDoubleESDamage）。
        let es_mult = if dtype == DamageType::Chaos && !ctx.chaos_not_double_es {
            2.0
        } else {
            1.0
        };
        let es_bypass = ctx.es_bypass_by_type[idx] / 100.0;
        // :584 分段生命命中池（以当前 life 重算）。
        let life_hit_pool = life_hit_pool_with_loss_prevention(
            life,
            ctx.max_life,
            ctx.prevented_life_loss,
            ctx.life_loss_below_half_prevented,
        );
        // :585-586 MoM 比例与 MoM 池。
        let mom_effect = (ctx.mom_shared + ctx.mom_by_type[idx]).min(100.0) / 100.0;
        let mom_pool = if mom_effect < 1.0 {
            (life_hit_pool / (1.0 - mom_effect) - life_hit_pool).min(mana)
        } else {
            mana
        };
        if energy_shield > 0.0 && ctx.eternal_life {
            // :587-594 EternalLife 分支：bypass 部分整体免除（eternalLifePrevented），
            // 不穿透到生命。
            let temp = rem.min(energy_shield / (1.0 - es_bypass) / es_mult);
            energy_shield -= temp * (1.0 - es_bypass) * es_mult;
            es_pool_remaining = es_pool_remaining.min(energy_shield);
            rem -= temp;
            lose(
                &mut resources_lost,
                dtype,
                "energy_shield",
                temp * (1.0 - es_bypass) * es_mult,
            );
            lose(
                &mut resources_lost,
                dtype,
                "eternal_life_prevented",
                temp * es_bypass,
            );
        } else if energy_shield > 0.0 && es_bypass < 1.0 {
            // :594-601 普通分支：ES 可用额度被 (MoM+life) 池经 bypass 嵌套限制
            // （MoMEBPool），chaos 双倍折算。
            let mom_eb_pool = if es_bypass > 0.0 {
                ((mom_pool + life_hit_pool) / es_bypass * es_mult - (mom_pool + life_hit_pool))
                    .min(energy_shield)
            } else {
                energy_shield
            };
            let temp = (rem * (1.0 - es_bypass)).min(mom_eb_pool / es_mult);
            es_pool_remaining = es_pool_remaining.min(mom_eb_pool - temp * es_mult);
            energy_shield -= temp * es_mult;
            rem -= temp;
            lose(&mut resources_lost, dtype, "energy_shield", temp * es_mult);
        }
        if mom_effect > 0.0 && mana > 0.0 {
            // :602-608 MoM：rem 的 MoM 份额进 mana，cap 于 MoMPool。
            let mom_damage = rem * mom_effect;
            let temp = mom_damage.min(mom_pool);
            mom_pool_remaining = mom_pool_remaining.min(mom_pool - temp);
            mana -= temp;
            rem -= temp;
            lose(&mut resources_lost, dtype, "mana", temp);
        } else {
            // :609 无 MoM 路径：MoM 池余量记 0（参与 :659 命中池余量求和）。
            mom_pool_remaining = 0.0;
        }
        // :611-651 loss-prevention（gate = preventedLifeLossTotal > 0，:2670 推导：
        // prev>0 或 belowHalf 有效值>0）。
        let below_half_eff = ctx.prevented_life_loss_below_half_effective();
        if ctx.prevented_life_loss > 0.0 || below_half_eff > 0.0 {
            let half_life = ctx.max_life * 0.5;
            let life_over_half = (life - half_life).max(0.0);
            let prevent_pct = ctx.prevented_life_loss / 100.0;
            let pool_above_low = life_over_half / (1.0 - prevent_pct);
            let prevent_below_half_pct = ctx.life_loss_below_half_prevented / 100.0;
            // :617-618 生命（含防止折算）还能吃下的伤害；超出部分先记 overkill。
            let damage_that_life_can_still_take = pool_above_low
                + life.min(half_life).max(0.0)
                    / (1.0 - prevent_below_half_pct)
                    / (1.0 - prevent_pct);
            if damage_that_life_can_still_take < rem {
                overkill += rem - damage_that_life_can_still_take;
                rem = damage_that_life_can_still_take;
            }
            if below_half_eff != 0.0 {
                // :623-641 above/below half 分段：先按半血以上池拆分，再对剩余按
                // 「非特定低血防止 → 特定低血防止」两步折算。
                let damage_to_split = rem.min(pool_above_low);
                let lost_life = damage_to_split * (1.0 - prevent_pct);
                let prevented_loss = damage_to_split * prevent_pct;
                rem -= damage_to_split;
                life_loss_lost_over_time += prevented_loss;
                life -= lost_life;
                lose(&mut resources_lost, dtype, "life", lost_life);
                let mut prevented_total_this_type = prevented_loss;
                if life <= half_life {
                    let unspecific = rem * prevent_pct;
                    life_loss_lost_over_time += unspecific;
                    rem -= unspecific;
                    let specific = rem * prevent_below_half_pct;
                    life_below_half_loss_lost_over_time += specific;
                    rem -= specific;
                    prevented_total_this_type += unspecific + specific;
                }
                lose(
                    &mut resources_lost,
                    dtype,
                    "life_loss_prevented",
                    prevented_total_this_type,
                );
            } else {
                // :643-647 仅 above-half 防止：固定比例转入延迟损失。
                let temp = rem * ctx.prevented_life_loss / 100.0;
                life_loss_lost_over_time += temp;
                rem -= temp;
                lose(&mut resources_lost, dtype, "life_loss_prevented", temp);
            }
        }
        // :651-655 life。
        if life > 0.0 {
            let temp = rem.min(life);
            life -= temp;
            rem -= temp;
            lose(&mut resources_lost, dtype, "life", temp);
        }
        // :656-657 overkill。
        overkill += rem;
        lose(&mut resources_lost, dtype, "overkill", rem);
    }

    // :659-660 命中池余量 = 扣后 life 的分段命中池 + MoM/ES 余量（未求值的 m_huge
    // 哨兵不计入），floor。
    let life_hit_pool_after = life_hit_pool_with_loss_prevention(
        life,
        ctx.max_life,
        ctx.prevented_life_loss,
        ctx.life_loss_below_half_prevented,
    );
    let hit_pool_remaining = (life_hit_pool_after
        + if mom_pool_remaining.is_finite() {
            mom_pool_remaining
        } else {
            0.0
        }
        + if es_pool_remaining.is_finite() {
            es_pool_remaining
        } else {
            0.0
        })
    .floor();

    PoolsAfter {
        pools: PoolState {
            allies,
            aegis_shared,
            aegis_shared_elemental,
            aegis_by_type,
            guard_shared,
            guard_shared_rate: pools.guard_shared_rate,
            guard_by_type,
            guard_rate_by_type: pools.guard_rate_by_type,
            ward: restore_ward,
            energy_shield,
            mana,
            life,
            life_loss_lost_over_time,
            life_below_half_loss_lost_over_time,
        },
        recoupable_by_type,
        overkill,
        hit_pool_remaining,
        resources_lost,
    }
}

/// X-protects-Y 通用原语（CalcDefence.lua:2746 / :2827 / :3547 / :3563）：
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

/// 把一个「按比例分担」的保护层折进目标池（PoB2 :2753-2754 / :3549 / :3564 / :3571
/// 共用形）：`pool' = max(pool − protected, 0) + min(pool, protected) / passthrough`。
///
/// `protected` = [`pool_protected`] 的产出；`passthrough_fraction` = 伤害穿过该层到达
/// 目标池的比例（= 1 − 层吸收比例）。`protected = ∞`（全额保护）时 vendor 数值上
/// 即 `min(pool,∞)/passthrough`；passthrough ≤ 0 的全吸收层不适用本公式（调用方按
/// vendor 走平铺相加分支，如 :3560-3561）。
pub fn apply_protected_layer(pool: f64, protected: f64, passthrough_fraction: f64) -> f64 {
    (pool - protected).max(0.0) + pool.min(protected) / passthrough_fraction
}

/// 分段生命命中池（CalcDefence.lua:450-454 `calcLifeHitPoolWithLossPrevention`）：
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

/// max-hit TotalHitPool 基底：把 ES（bypass / chaos 双倍 / EternalLife 分支）折在
/// MoM 命中池之上（CalcDefence.lua:2942-2960）。
///
/// `mom_hit_pool` = `<X>MoMHitPool`（pool_setup `mom_hit_pools` 产出）；
/// `energy_shield` = EnergyShieldRecoveryCap。Track F 在此之上再叠
/// [`extend_total_hit_pool`] 的 ward/aegis/guard/allies 层。
pub fn total_hit_pool_base(
    dtype: DamageType,
    mom_hit_pool: f64,
    energy_shield: f64,
    ctx: &PoolCtx,
) -> f64 {
    let es_bypass = ctx.es_bypass_by_type[dtype as usize] / 100.0;
    // :2947 chaos 对 ES 双倍 → ES 折半计入池。
    let chaos_es_mult = if dtype == DamageType::Chaos && !ctx.chaos_not_double_es {
        2.0
    } else {
        1.0
    };
    if ctx.eternal_life {
        // :2948-2950 EternalLife：bypass 部分被免除 → ES 实际能挡 ES/(1−bypass)。
        mom_hit_pool + energy_shield / (1.0 - es_bypass) / chaos_es_mult
    } else if es_bypass < 1.0 {
        if es_bypass > 0.0 {
            // :2952-2955 bypass 嵌套：poolProtected = EScap/(1−bypass)×bypass/chaosMult。
            let protected = energy_shield / (1.0 - es_bypass) * es_bypass / chaos_es_mult;
            apply_protected_layer(mom_hit_pool, protected, es_bypass)
        } else {
            // :2956-2958 无 bypass：平铺相加（chaos 双倍折半）。
            mom_hit_pool + energy_shield / chaos_es_mult
        }
    } else {
        // bypass ≥ 100%：ES 不参与该类型的池。
        mom_hit_pool
    }
}

/// max-hit TotalHitPool 扩展层（CalcDefence.lua:3540-3596）：在
/// [`total_hit_pool_base`] 之上依次叠 ward（bypass poolProtected，:3544-3553）、
/// aegis（取最强口径平铺相加，:3554-3555）、guard（:3556-3566）、allies
/// （各层 poolProtected，:3567-3595）。
///
/// 注意 vendor :3557/:3559 的 Lua 运算符优先级（`a or 0 + b or 0` 解析为
/// `a or (0+b) or 0`）使 guard 段**只有 shared guard 生效**（sharedGuardAbsorbRate
/// 恒非 nil）；为 parity 逐值对齐，此处复刻该求值语义而非字面公式。
pub fn extend_total_hit_pool(
    base_pool: f64,
    dtype: DamageType,
    pools: &PoolState,
    ctx: &PoolCtx,
) -> f64 {
    let idx = dtype as usize;
    let mut pool = base_pool;
    // ward（:3544-3553）：bypass>0 时按 poolProtected 嵌套，否则平铺相加。
    if ctx.ward_bypass > 0.0 {
        let bypass = ctx.ward_bypass / 100.0;
        // :3547 protected = Ward/(1−bypass)×bypass = pool_protected(Ward, 1−bypass)。
        let protected = pool_protected(pools.ward, 1.0 - bypass);
        pool = apply_protected_layer(pool, protected, bypass);
    } else {
        pool += pools.ward;
    }
    // aegis（:3555）：max(per-type, shared, 元素时 per-type+sharedElemental) 平铺相加
    // （AegisDisplay = perType + sharedElemental，:2879）。
    let aegis_display = if DamageType::ELEMENTAL.contains(&dtype) {
        pools.aegis_by_type[idx] + pools.aegis_shared_elemental
    } else {
        0.0
    };
    pool += pools.aegis_by_type[idx]
        .max(pools.aegis_shared)
        .max(aegis_display);
    // guard（:3556-3566）：vendor 求值语义 = 仅 shared guard（见函数文档）。
    let guard_rate = pools.guard_shared_rate;
    if guard_rate > 0.0 {
        let guard_absorb = pools.guard_shared;
        if guard_rate >= 100.0 {
            pool += guard_absorb;
        } else {
            let protected = pool_protected(guard_absorb, guard_rate / 100.0);
            pool = apply_protected_layer(pool, protected, 1.0 - guard_rate / 100.0);
        }
    }
    // allies（:3567-3595）：各层 poolProtected 依次折入（mitigation 0 时 protected=∞，
    // 公式自然退化为原池）。
    for ally in &pools.allies {
        if ally.remaining > 0.0 && (ally.damage_type.is_none() || ally.damage_type == Some(dtype)) {
            let rate = ally.mitigation_pct / 100.0;
            let protected = pool_protected(ally.remaining, rate);
            pool = apply_protected_layer(pool, protected, 1.0 - rate);
        }
    }
    pool
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

    /// apply_protected_layer 两段语义（CalcDefence.lua:2753-2754 形）：
    /// 池 ≤ protected → pool/passthrough；池 > protected → 超出部分原值 + protected 段放大。
    /// 手算：pool=1000, protected=2400, pass=0.5 → 0 + 1000/0.5 = 2000；
    ///       pool=3000, protected=2400, pass=0.5 → 600 + 2400/0.5 = 5400。
    #[test]
    fn apply_protected_layer_two_segments() {
        assert_eq!(apply_protected_layer(1000.0, 2400.0, 0.5), 2000.0);
        assert_eq!(apply_protected_layer(3000.0, 2400.0, 0.5), 5400.0);
    }

    /// 无损失防止时分段池退化为 life 本值（CalcDefence.lua:450-454，prev=0 →
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
    /// CalcDefence.lua:453 两段同除 (1−lossPrev/100)）。
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
    /// 中性默认锁定）。
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

    /// POB2 遍历序锁定（CalcDefence.lua:27）：正序 Physical→Lightning→Cold→Fire→Chaos，
    /// 数组下标仍按 pobr DamageType 枚举序。
    #[test]
    fn pob2_damage_order_locked() {
        assert_eq!(
            POB2_DAMAGE_ORDER,
            [
                DamageType::Physical,
                DamageType::Lightning,
                DamageType::Cold,
                DamageType::Fire,
                DamageType::Chaos,
            ]
        );
        assert_eq!(DamageType::Physical as usize, 0);
        assert_eq!(DamageType::Chaos as usize, 4);
    }
}
