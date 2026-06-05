//! 触发（Trigger）域：冷却驱动型触发速率上限 + 能量驱动元宝石模型 + 多技能轮转 + CWC。
//!
//! ## 结构
//!
//! ### §一  冷却驱动（Cooldown-gated）触发速率上限
//! 对应 agent-docs/triggers.md §三：`TriggerRateCap = 1/(ceil(cd × ServerTickRate)/ServerTickRate)`，
//! 双门控 `SkillTriggerRate = min(cap, sourceRate)`，ICDR 除数缩短触发冷却。
//! 出处：PoB2 `CalcTriggers.lua`（`modActionCooldown / rateCapAdjusted / SkillTriggerRate`）、
//!       PoB2 `Data.lua`（`ServerTickTime = 0.033`）。
//!
//! ### §二  能量驱动（Energy / Meta Gem）模型
//! Cast on X（Cast on Critical, Cast on Ailment, etc.）用 Energy 计数器决定何时触发。
//! - `max_energy = Σ(socketed base_cast_time / 0.1) × 10`；total-use-time 修饰词按 ×2 处理。
//! - 产能：`centienergy_per_hit = MonsterPower × baseCentienergy × scale`，Crit/Ignite/Shock=100，
//!   Freeze=1000（centienergy = 1/100 能量），CoC 还乘 (原始伤害 / 异常阈值)。
//! - 等级加成：`energy_generated_+%` 提升产能速率（不改基数/上限）。
//! - 触发频率估算：`≈ source_rate × energy_per_event / max_energy`，上限取 `trigger_rate_cap`。
//!
//! 出处：agent-docs/triggers.md §二；PoB2 `act_int.lua` / `other.lua`；PoE2 Wiki CoC；PoE2DB Energy。
//!
//! ### §三  多技能轮转（Multi-Skill Rotation）
//! 移植 PoB2 `calcMultiSpellRotationImpact`：确定性 1000 次触发机会 + 帧对齐冷却 + 几何分布折算。
//! - 每次触发机会按轮转顺序找第一个「已脱离冷却」的技能触发；都在冷却则该次触发浪费。
//! - `next_trig = ceil_tick(floor_tick(now) + cd)`（冷却从当前帧边界起算）。
//! - 触发几率 < 100% 时用几何分布期望值折算实际触发速率。
//!
//! 出处：agent-docs/triggers.md §五；PoB2 `CalcTriggers.lua::calcMultiSpellRotationImpact`。
//!
//! ### §四  CWC（Cast While Channelling）
//! 引导触发：由 `triggerTime`（引导每隔若干秒触发一次，取整到服务器帧）决定基准节奏，
//! 被触发技能冷却再 clamp。可选 `SpellCastTimeAddedToCooldownIfTriggered`（施法时间加入冷却）。
//! `TriggeredDamage` INC/MORE 作为被触发技能的 Damage 乘区（不在此注入，供集成层引用）。
//! 出处：agent-docs/triggers.md §4.2；PoB2 `CalcTriggers.lua::CWCHandler`。
//!
//! ## 并行安全
//! 本模块**只修改 trigger.rs 与 tests/trigger.rs**，不触碰 perform/output/offence/env/actor/mod_db。
//! 新增 pub 函数通过 `calc/mod.rs` re-export（仅追加，不修改已有 re-export 行）。
//!
//! ## defer
//! 完整能量蒙特卡洛精确对齐（需服务器帧级别逐帧模拟）留 golden fixture；
//! PoB2 对能量驱动元宝石的完整支持本身「needs an entire overhaul」，故当前 pobr 的能量触发
//! 速率估算为**确定性近似**，见 `EnergyTriggerRate` 注释中的偏差说明。

use pobr_data::prelude::SERVER_TICK_SECONDS;

use super::round;

// ---------------------------------------------------------------------------
// §一  服务器帧工具 & 冷却驱动基础
// ---------------------------------------------------------------------------

/// 服务器帧速率（actions/s），`1 / SERVER_TICK_SECONDS ≈ 30.3`。
/// 出处：PoB2 Data.lua `ServerTickRate = 1/0.033`。
pub fn server_tick_rate() -> f64 {
    1.0 / SERVER_TICK_SECONDS
}

/// 把冷却向上取整到服务器帧：`ceil(cd × rate) / rate`。
///
/// 触发只能发生在帧边界，真实冷却被「四舍五入」到下一帧。这是触发速率出现台阶的根因。
/// 出处：agent-docs/triggers.md §3.2；PoB2 CalcTriggers.lua。
pub fn round_cooldown_to_tick(cooldown: f64, tick_rate: f64) -> f64 {
    if cooldown <= 0.0 || tick_rate <= 0.0 {
        return 0.0;
    }
    round((cooldown * tick_rate).ceil() / tick_rate)
}

/// 触发速率上限纯函数：`cap = 1 / (ceil(cd × rate) / rate)`。
///
/// `cd` 为实际动作冷却（已是 `max(triggeredCD, triggerCD/icdr)` 的结果）；`tick_rate` 为
/// 服务器帧速率（默认 `server_tick_rate()`）。返回每秒触发上限。
/// 出处：agent-docs/triggers.md §3.1；PoB2 CalcTriggers.lua
/// `TriggerRateCap = 1/(ceil(modActionCooldown × ServerTickRate)/ServerTickRate)`。
pub fn trigger_rate_cap(cooldown: f64, tick_rate: f64) -> f64 {
    let rounded = round_cooldown_to_tick(cooldown, tick_rate);
    if rounded > 0.0 {
        round(1.0 / rounded)
    } else {
        0.0
    }
}

/// 计算实际动作冷却：`max(triggeredCD, triggerCD / icdr)`。
///
/// - `trigger_cd`：触发宝石本身冷却（`triggeredBy.grantedEffect.levels[lvl].cooldown`）。
/// - `triggered_cd`：被触发技能冷却（`skillData.cooldown`）；无冷却传 0。
/// - `icdr`：冷却恢复速率乘区（`CooldownRecovery`，INC/MORE 折算后的乘数，≥0），作为**除数**缩短触发宝石冷却。
///
/// 出处：agent-docs/triggers.md §3.1；PoB2 CalcTriggers.lua
/// `modActionCooldown = max(triggeredCD, triggerCD / icdrSkill)`。
pub fn action_cooldown(trigger_cd: f64, triggered_cd: f64, icdr: f64) -> f64 {
    let effective_trigger = if icdr > 0.0 {
        trigger_cd / icdr
    } else {
        trigger_cd
    };
    effective_trigger.max(triggered_cd)
}

/// 触发速率上限的结算结果（冷却驱动版）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TriggerRate {
    /// 取较大者后、取整前的实际动作冷却（秒）。
    pub action_cooldown: f64,
    /// 向上取整到服务器帧后的冷却（秒）。
    pub rate_cap_cooldown: f64,
    /// 触发速率上限（次/秒）= 1 / rate_cap_cooldown。
    pub trigger_rate_cap: f64,
    /// 实际触发速率（次/秒）= min(上限, 有效源速率)。
    pub skill_trigger_rate: f64,
    /// 是否被源速率门控（源速率 < 上限）。
    pub limited_by_source: bool,
}

/// 端到端：从触发器/被触发技能冷却 + ICDR + 有效源速率求实际触发速率。
///
/// `SkillTriggerRate = min(TriggerRateCap, EffectiveSourceRate)`——伤害再高，若源攻速低或
/// 冷却长，触发也慢（双重门控）。出处：agent-docs/triggers.md §3.3；PoB2 CalcTriggers.lua。
pub fn resolve_trigger_rate(
    trigger_cd: f64,
    triggered_cd: f64,
    icdr: f64,
    effective_source_rate: f64,
) -> TriggerRate {
    let tick_rate = server_tick_rate();
    let cd = action_cooldown(trigger_cd, triggered_cd, icdr);
    let rate_cap_cooldown = round_cooldown_to_tick(cd, tick_rate);
    let cap = if rate_cap_cooldown > 0.0 {
        1.0 / rate_cap_cooldown
    } else {
        0.0
    };

    let source = effective_source_rate.max(0.0);
    let (skill_rate, limited_by_source) = if source > 0.0 && source < cap {
        (source, true)
    } else {
        (cap, false)
    };

    TriggerRate {
        action_cooldown: round(cd),
        rate_cap_cooldown: round(rate_cap_cooldown),
        trigger_rate_cap: round(cap),
        skill_trigger_rate: round(skill_rate),
        limited_by_source,
    }
}

// ---------------------------------------------------------------------------
// §二  能量（Energy）驱动元宝石模型
// ---------------------------------------------------------------------------

/// 触发条件类型——决定 centienergy 基数与产能计算方式。
///
/// 出处：agent-docs/triggers.md §2.2；PoB2 `act_int.lua` centienergy 常量表。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerCondition {
    /// Cast on Critical：centienergy = MonsterPower × 100 × (hit_damage / ailment_threshold)。
    /// 0.5.0 起还看暴击原始伤害对异常阈值的比例。
    CriticalStrike,
    /// Cast on Ignite：centienergy = MonsterPower × 100（按 ignite magnitude/阈值比调整）。
    Ignite,
    /// Cast on Shock：centienergy = MonsterPower × 100。
    Shock,
    /// Cast on Freeze：centienergy = MonsterPower × 1000（冰冻基数是 Crit/Ignite/Shock 的 10 倍）。
    Freeze,
    /// Cast on Melee Kill / Cast on Minion Death / Hit 等——每次事件产固定能量（centienergy=100 默认）。
    Other,
}

impl TriggerCondition {
    /// 该触发条件的基础 centienergy（1/100 能量）/每 MonsterPower/每次事件。
    ///
    /// 出处：agent-docs/triggers.md §2.2 表格；PoB2 act_int.lua
    /// `cast_on_crit_gain_X_centienergy_per_monster_power_on_crit = 100`，
    /// `cast_on_freeze_gain_X_centienergy_per_monster_power_on_freeze = 1000`。
    pub fn base_centienergy(self) -> f64 {
        match self {
            TriggerCondition::CriticalStrike
            | TriggerCondition::Ignite
            | TriggerCondition::Shock
            | TriggerCondition::Other => 100.0,
            TriggerCondition::Freeze => 1000.0,
        }
    }
}

/// 能量最大值计算参数（插槽中各法术的基础施法时间 + total-use-time 修饰词）。
///
/// 出处：agent-docs/triggers.md §2.1；PoB2 other.lua
/// `generic_ongoing_trigger_1_maximum_energy_per_Xms_total_cast_time = 10`、
/// `generic_ongoing_trigger_maximum_energy_is_total_of_socketed_skills`。
#[derive(Debug, Clone, PartialEq)]
pub struct SocketedSpellInfo {
    /// 该插槽法术的基础施法时间（秒）。
    pub base_cast_time: f64,
    /// total-use-time 修饰词百分比（%）；计算最大能量时按 ×2 处理。
    /// 出处：agent-docs/triggers.md §2.1「modifiers to Total use time are treated as though
    /// they were double the value」。
    pub use_time_increase_pct: f64,
}

impl SocketedSpellInfo {
    pub fn new(base_cast_time: f64) -> Self {
        Self {
            base_cast_time,
            use_time_increase_pct: 0.0,
        }
    }

    pub fn with_use_time_increase(mut self, pct: f64) -> Self {
        self.use_time_increase_pct = pct;
        self
    }

    /// 用于计算最大能量的「有效总使用时间」：
    /// `base_cast_time × (1 + use_time_increase_pct/100 × 2)`。
    ///
    /// total-use-time 修饰词在能量计算中按 2 倍处理（相当于把施法更慢的惩罚放大）。
    /// 出处：agent-docs/triggers.md §2.1；PoE2 Wiki CoC；PoE2DB Energy。
    pub fn effective_cast_time_for_energy(&self) -> f64 {
        self.base_cast_time * (1.0 + self.use_time_increase_pct / 100.0 * 2.0)
    }
}

/// 计算能量驱动元宝石的最大能量：`Σ(effective_cast_time / 0.1) × 10`。
///
/// 等价于 `Σ effective_cast_time × 100`（每 0.1s 基础施法时间 = 10 能量）。
/// 最大能量越高 → 越难触发（攒到上限才触发）。
///
/// 出处：agent-docs/triggers.md §2.1；PoB2 other.lua
/// `generic_ongoing_trigger_1_maximum_energy_per_Xms_total_cast_time = 10`，
/// 即「Has 10 maximum Energy per 0.1 seconds of base cast time of Socketed Spells」。
pub fn calc_max_energy(socketed_spells: &[SocketedSpellInfo]) -> f64 {
    if socketed_spells.is_empty() {
        return 0.0;
    }
    let total: f64 = socketed_spells
        .iter()
        .map(|s| (s.effective_cast_time_for_energy() / 0.1) * 10.0)
        .sum();
    round(total)
}

/// 每次触发事件（命中/暴击/击杀等）产生的能量（非 centienergy）。
///
/// 公式（CoC）：`energy = MonsterPower × (hit_damage / ailment_threshold) × scale`
/// 其中 `scale = energy_generated_pct_bonus / 100`（宝石等级加成，≥ 1.0）。
///
/// 其他条件（Ignite/Shock/Freeze/Other）：
/// - `energy = MonsterPower × base_centienergy / 100 × scale`。
/// - CoC 额外乘「原始伤害 / 异常阈值」（伤害越高 → 产能越多）。
///
/// 出处：agent-docs/triggers.md §2.2；PoE2 Wiki CoC 0.5.0 公式。
///
/// # 参数
/// - `condition`：触发条件类型（决定 centienergy 基数）。
/// - `monster_power`：敌人力量（通常 0.5–3，稀有怪乘以稀有度系数 1/2/5/独特=20）。
/// - `hit_damage`：本次命中的原始伤害（减免前）；仅 CoC 有意义，其他条件传 0。
/// - `ailment_threshold`：怪物异常阈值；仅 CoC 有意义，其他条件传 1.0 避免除零。
/// - `energy_generated_scale`：宝石等级提供的「energy_generated_+%」/100 + 1（即乘数，如 1.57）。
pub fn calc_energy_per_event(
    condition: TriggerCondition,
    monster_power: f64,
    hit_damage: f64,
    ailment_threshold: f64,
    energy_generated_scale: f64,
) -> f64 {
    let base_centienergy = condition.base_centienergy();
    let monster_power = monster_power.max(0.0);
    let scale = energy_generated_scale.max(1.0);

    let centienergy = match condition {
        TriggerCondition::CriticalStrike => {
            // CoC 0.5.0：产能还看原始伤害 / 怪物异常阈值。
            // 要可靠触发，暴击伤害需约为怪物异常阈值的 10 倍。
            let threshold = ailment_threshold.max(1.0);
            let damage_ratio = (hit_damage / threshold).max(0.0);
            monster_power * base_centienergy * damage_ratio * scale
        }
        _ => {
            // Ignite/Shock/Freeze/Other：按 MonsterPower × base_centienergy 线性产能。
            monster_power * base_centienergy * scale
        }
    };
    // centienergy / 100 = 能量。
    round(centienergy / 100.0)
}

/// 能量驱动触发速率结算结果。
///
/// **注意（与 PoB2 的偏差）**：当前实现是**确定性近似**（非蒙特卡洛逐帧模拟）。
/// - 假设每次「触发事件」（每次命中/暴击）产能恒定（用均值替代分布）。
/// - `effective_trigger_rate` = `source_rate × energy_per_event / max_energy`，
///   上限取 `trigger_rate_cap`（冷却门控）。
/// - PoB2 的精确版本是「服务器帧 × 逐帧模拟」——差异在高伤方差场景（暴击伤害离散度大）
///   或 MonsterPower 非均匀时会出现偏差。
/// - defer 完整蒙特卡洛精确对齐（保留 golden fixture 测试框架）。
///
/// 出处：agent-docs/triggers.md §二、§对 pobr 实现的启示 #2；
///       PoE2 Wiki CoC；PoE2DB Energy；PoB2 DeepWiki（「needs an entire overhaul」警告）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnergyTriggerRate {
    /// 元宝石能量上限。
    pub max_energy: f64,
    /// 每次触发事件产生的能量（均值，用于速率估算）。
    pub energy_per_event: f64,
    /// 每秒产生的总能量（`energy_per_event × source_rate`）。
    pub energy_per_second: f64,
    /// 能量驱动的原始触发频率估算（次/秒）：`energy_per_second / max_energy`。
    pub raw_trigger_rate: f64,
    /// 受冷却上限截断后的有效触发速率（次/秒）。
    pub effective_trigger_rate: f64,
    /// 冷却驱动的触发速率上限（次/秒）；用于截断能量速率。
    pub cooldown_rate_cap: f64,
    /// 是否被冷却上限截断（能量产出比冷却更快）。
    pub limited_by_cooldown: bool,
}

/// 能量驱动元宝石的端到端触发速率估算（确定性近似）。
///
/// # 参数
/// - `socketed_spells`：插槽中各法术的施法时间信息（决定 max_energy）。
/// - `condition`：触发条件（Crit/Ignite/Shock/Freeze/Other）。
/// - `monster_power`：敌人力量（单次事件）；一波怪建议传总 power（15–20）。
/// - `hit_damage`：命中原始伤害（CoC 使用；其他条件传 0）。
/// - `ailment_threshold`：怪物异常阈值（CoC 使用；其他条件传 1.0）。
/// - `energy_generated_scale`：宝石等级加成乘数（1.0 + energy_generated_+%/100）。
/// - `source_rate`：源技能（命中/攻击/引导）的每秒事件数。
/// - `trigger_cd`：触发宝石本身冷却（秒；用于冷却门控上限）。
/// - `triggered_cd`：被触发技能冷却（秒；无冷却传 0）。
/// - `icdr`：冷却恢复速率乘数（≥ 1.0）。
///
/// 出处：agent-docs/triggers.md §二；PoE2DB Energy；PoE2 Wiki CoC 0.5.0。
#[allow(clippy::too_many_arguments)]
pub fn calc_energy_trigger_rate(
    socketed_spells: &[SocketedSpellInfo],
    condition: TriggerCondition,
    monster_power: f64,
    hit_damage: f64,
    ailment_threshold: f64,
    energy_generated_scale: f64,
    source_rate: f64,
    trigger_cd: f64,
    triggered_cd: f64,
    icdr: f64,
) -> EnergyTriggerRate {
    let max_energy = calc_max_energy(socketed_spells);
    let energy_per_event = calc_energy_per_event(
        condition,
        monster_power,
        hit_damage,
        ailment_threshold,
        energy_generated_scale,
    );

    let source = source_rate.max(0.0);
    let energy_per_second = round(energy_per_event * source);
    let raw_rate = if max_energy > 0.0 {
        round(energy_per_second / max_energy)
    } else {
        0.0
    };

    // 冷却门控上限（不能超过冷却决定的上限）。
    let cd = action_cooldown(trigger_cd, triggered_cd, icdr);
    let tick_rate = server_tick_rate();
    let rate_cap_cd = round_cooldown_to_tick(cd, tick_rate);
    let cd_cap = if rate_cap_cd > 0.0 {
        round(1.0 / rate_cap_cd)
    } else {
        // 无冷却 → 只受能量速率门控，无上限截断。
        f64::INFINITY
    };

    let effective_rate = raw_rate.min(cd_cap);
    let limited_by_cooldown = cd_cap.is_finite() && raw_rate > cd_cap;

    EnergyTriggerRate {
        max_energy,
        energy_per_event,
        energy_per_second,
        raw_trigger_rate: raw_rate,
        effective_trigger_rate: round(effective_rate),
        cooldown_rate_cap: if cd_cap.is_finite() {
            round(cd_cap)
        } else {
            0.0
        },
        limited_by_cooldown,
    }
}

// ---------------------------------------------------------------------------
// §三  多技能轮转（Multi-Skill Rotation）
// ---------------------------------------------------------------------------

/// 轮转中单个技能的参数。
///
/// 每次触发机会按轮转顺序找第一个「已脱离冷却」的技能触发；都在冷却则该次触发浪费。
/// 出处：agent-docs/triggers.md §五；PoB2 CalcTriggers.lua `calcMultiSpellRotationImpact`。
#[derive(Debug, Clone, PartialEq)]
pub struct RotationSkill {
    /// 技能有效冷却（秒；已含 ICDR 除法与 max(triggeredCD, triggerCD/icdr)）。
    /// 若调用方已完成 `action_cooldown()` 计算，直接传入结果即可。
    pub effective_cd: f64,
    /// 每次触发机会的触发几率（0.0–1.0；1.0 = 必然触发）。
    /// 出处：agent-docs/triggers.md §五「几率折算——几何分布期望」。
    pub trigger_chance: f64,
    /// 额外冷却追加（SpellCastTimeAddedToCooldownIfTriggered，秒；无则传 0）。
    /// 出处：agent-docs/triggers.md §4.3；PoB2 CalcTriggers.lua `addsCastTime`。
    pub added_cooldown: f64,
}

impl RotationSkill {
    pub fn new(effective_cd: f64) -> Self {
        Self {
            effective_cd,
            trigger_chance: 1.0,
            added_cooldown: 0.0,
        }
    }

    pub fn with_trigger_chance(mut self, chance: f64) -> Self {
        self.trigger_chance = chance.clamp(0.0, 1.0);
        self
    }

    pub fn with_added_cooldown(mut self, added_cd: f64) -> Self {
        self.added_cooldown = added_cd.max(0.0);
        self
    }

    /// 有效总冷却（含追加）。
    pub fn total_cd(&self) -> f64 {
        (self.effective_cd + self.added_cooldown).max(0.0)
    }
}

/// 多技能轮转模拟结果：每个技能的稳态触发速率。
#[derive(Debug, Clone, PartialEq)]
pub struct RotationResult {
    /// 每个技能在轮转中的稳态触发速率（次/秒）。顺序与输入 `skills` 对应。
    pub rates: Vec<f64>,
    /// 触发机会中「所有技能都在冷却，本次触发浪费」的比例（稳态估算，0–1）。
    pub wasted_fraction: f64,
}

/// 多技能轮转确定性模拟：移植 PoB2 `calcMultiSpellRotationImpact`。
///
/// 算法：
/// 1. 模拟 `SIM_ROUNDS`（=1000）次触发机会，间隔 `1 / source_rate` 秒。
/// 2. 每次机会按轮转顺序找第一个「已过冷却时间」的技能触发。
/// 3. 冷却从当前帧起算、对齐到服务器帧：
///    `next_trig = ceil_tick(floor_tick(now) + cd)`（`ceil_tick/floor_tick = ±round to frame`）。
/// 4. 触发几率 < 100% 时，将「平均需要 1/chance 次机会才能触发」折算进稳态触发速率：
///    `rate = triggers_in_sim / (SIM_TIME + expected_extra_wait)`。
///
/// 与 PoB2 蒙特卡洛的差异：本实现是**确定性**模拟——trigger_chance < 1 时用期望值替代
/// 随机采样（无随机数），结果与 PoB2 在 chance=1.0 时严格对齐，chance < 1 时是期望近似。
///
/// 出处：agent-docs/triggers.md §五；PoB2 CalcTriggers.lua L460–520 (calcMultiSpellRotationImpact)。
///
/// # 参数
/// - `skills`：轮转中各技能（有序列表，轮转按此顺序）。
/// - `source_rate`：源技能每秒触发机会数（如攻速 / 吟唱频率）。
///
/// # 返回
/// [`RotationResult`]，每个技能的稳态触发速率（次/秒）与浪费率。
pub fn calc_multi_spell_rotation(skills: &[RotationSkill], source_rate: f64) -> RotationResult {
    if skills.is_empty() || source_rate <= 0.0 {
        return RotationResult {
            rates: Vec::new(),
            wasted_fraction: 0.0,
        };
    }

    let tick_seconds = SERVER_TICK_SECONDS;

    let trigger_increment = 1.0 / source_rate; // 每次触发机会间隔（秒）。
    const SIM_ROUNDS: usize = 1000;
    let sim_time = trigger_increment * SIM_ROUNDS as f64;

    let n = skills.len();
    let mut trigger_counts = vec![0u64; n];
    // next_available[i]：技能 i 下一次可触发的时间（秒），初始为 0（全部立即可用）。
    let mut next_available = vec![0.0f64; n];
    let mut wasted_count = 0u64;

    for round_idx in 0..SIM_ROUNDS {
        let now = trigger_increment * round_idx as f64;

        // 找当前时间可用的第一个技能（按轮转顺序）。
        let triggered_idx = next_available
            .iter()
            .enumerate()
            .take(n)
            .find(|(_, avail)| now >= **avail)
            .map(|(i, _)| i);

        match triggered_idx {
            None => {
                wasted_count += 1;
            }
            Some(i) => {
                let skill = &skills[i];
                trigger_counts[i] += 1;

                // 下一次可触发时间：帧对齐。
                // PoB2：`next_trig = ceil_b(floor_b(now, ServerTickTime) + cd, ServerTickTime)`
                let floor_now = (now / tick_seconds).floor() * tick_seconds;
                let cd = skill.total_cd().max(tick_seconds); // 最小冷却 = 1 帧。
                let raw_next = floor_now + cd;
                let ceil_next = (raw_next / tick_seconds).ceil() * tick_seconds;
                next_available[i] = ceil_next;
            }
        }
    }

    // 把「触发几率 < 1」折算进速率（几何分布期望）。
    // PoB2 几率折算：rate = count / (sim_time + (1/chance - 1) × triggerIncrement × count)
    // 简化：若 chance=1，直接 rate = count/sim_time。
    let rates = skills
        .iter()
        .enumerate()
        .map(|(i, skill)| {
            let count = trigger_counts[i] as f64;
            if count == 0.0 {
                return 0.0;
            }
            let chance = skill.trigger_chance.max(1e-9);
            // 期望每次触发需要 1/chance 次机会；每次机会间隔 trigger_increment 秒。
            // 额外等待时间 = (1/chance - 1) × trigger_increment × count（所有额外机会之和）。
            let extra_wait = (1.0 / chance - 1.0) * trigger_increment * count;
            let effective_time = sim_time + extra_wait;
            round(count / effective_time)
        })
        .collect();

    let wasted_fraction = wasted_count as f64 / SIM_ROUNDS as f64;

    RotationResult {
        rates,
        wasted_fraction: round(wasted_fraction),
    }
}

// ---------------------------------------------------------------------------
// §四  CWC（Cast While Channelling）
// ---------------------------------------------------------------------------

/// CWC（Cast While Channelling）触发速率结算结果。
///
/// CWC 由引导间隔 `triggerTime`（向上取整到服务器帧）决定基准节奏，被触发技能冷却再 clamp。
/// 出处：agent-docs/triggers.md §4.2；PoB2 CalcTriggers.lua `CWCHandler`。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CwcTriggerRate {
    /// 取整到服务器帧后的引导触发间隔（秒）= `ceil(triggerTime × ServerTickRate)/ServerTickRate`。
    pub adjusted_trigger_interval: f64,
    /// 引导触发基准频率（次/秒）= `1 / adjusted_trigger_interval`。
    pub channelling_trigger_rate: f64,
    /// 被触发技能的有效冷却（含 SpellCastTimeAddedToCooldownIfTriggered，经 ICDR 除法，秒）。
    pub effective_triggered_cd: f64,
    /// 最终触发速率上限（次/秒）= `min(channelling_trigger_rate, 1/effective_triggered_cd)`。
    pub trigger_rate_cap: f64,
    /// 是否被被触发技能冷却限制（triggered_cd > 引导间隔）。
    pub limited_by_triggered_cd: bool,
}

/// 计算 CWC（Cast While Channelling）触发速率。
///
/// CWC 流程：
/// 1. `adjInterval = ceil(triggerTime × ServerTickRate) / ServerTickRate`（帧对齐）。
/// 2. `channelingRate = 1 / adjInterval`（引导基准频率）。
/// 3. `effTriggeredCD = max(triggered_cd, adds_cast_time) / icdr`。
///    - `adds_cast_time`：`SpellCastTimeAddedToCooldownIfTriggered` = 被触发法术基础施法时间/施法速度。
///    - 无此追加时传 0。
/// 4. `TriggerRateCap = min(channelingRate, 1/ceil(effTriggeredCD × rate)/rate)`。
///
/// 出处：agent-docs/triggers.md §4.2；PoB2 `CalcTriggers.lua::CWCHandler`：
/// ```lua
/// adjTriggerInterval = ceil(triggerTime × ServerTickRate) / ServerTickRate
/// triggerRateOfTrigger = 1 / adjTriggerInterval
/// triggeredTotalCD = (cooldownOverride or max(triggeredCD, addsCastTime)) / icdr
/// TriggerRateCap = min(1/effCDTriggeredSkill, triggerRateOfTrigger)
/// ```
///
/// **`TriggeredDamage` 注入**：
/// `TriggeredDamage INC/MORE` 修饰词需由集成层注入被触发技能的 `Damage` 乘区（本函数不操作 ModDb）。
/// 集成层可在 perform 时读取 `TriggeredDamageInc` / `TriggeredDamageMore` mod，追加到技能 damage 管线。
///
/// # 参数
/// - `trigger_time`：引导基准触发间隔（秒；来自 `skillData.triggerTime` 或 PoB2 宝石数据）。
/// - `triggered_cd`：被触发技能的基础冷却（秒；无冷却传 0）。
/// - `adds_cast_time`：`SpellCastTimeAddedToCooldownIfTriggered` 追加的冷却（秒；无则传 0）。
/// - `icdr`：冷却恢复速率乘数（≥ 1.0；1.0 = 无 ICDR 加成）。
pub fn calc_cwc_trigger_rate(
    trigger_time: f64,
    triggered_cd: f64,
    adds_cast_time: f64,
    icdr: f64,
) -> CwcTriggerRate {
    let tick_rate = server_tick_rate();

    // 引导间隔取整到服务器帧。
    let adj_interval = round_cooldown_to_tick(trigger_time.max(0.0), tick_rate);
    let channelling_rate = if adj_interval > 0.0 {
        round(1.0 / adj_interval)
    } else {
        0.0
    };

    // 被触发技能有效冷却：max(triggered_cd, adds_cast_time) / icdr。
    let icdr_eff = if icdr > 0.0 { icdr } else { 1.0 };
    let raw_triggered_cd = triggered_cd.max(adds_cast_time).max(0.0);
    let eff_triggered_cd = raw_triggered_cd / icdr_eff;

    // 被触发技能冷却门控的速率上限。
    let cd_rate_cap = if eff_triggered_cd > 0.0 {
        round(1.0 / round_cooldown_to_tick(eff_triggered_cd, tick_rate))
    } else {
        // 无冷却：纯引导频率驱动。
        channelling_rate
    };

    let final_cap = channelling_rate.min(cd_rate_cap);
    let limited_by_triggered_cd = eff_triggered_cd > 0.0 && cd_rate_cap < channelling_rate;

    CwcTriggerRate {
        adjusted_trigger_interval: adj_interval,
        channelling_trigger_rate: channelling_rate,
        effective_triggered_cd: round(eff_triggered_cd),
        trigger_rate_cap: round(final_cap),
        limited_by_triggered_cd,
    }
}

/// 计算 `SpellCastTimeAddedToCooldownIfTriggered` 的追加冷却量（秒）。
///
/// 部分触发把**被触发法术的施法时间加进冷却**，使施法慢的法术触发更慢：
/// `adds_cast_time = base_cast_time / cast_speed_multiplier`。
///
/// 出处：agent-docs/triggers.md §4.3；PoB2 `CalcTriggers.lua::processAddedCastTime`。
///
/// # 参数
/// - `base_cast_time`：被触发法术的基础施法时间（秒）。
/// - `cast_speed_multiplier`：施法速度乘区总值（> 0）；= 1.0 + cast_speed_pct/100。
pub fn spell_cast_time_added_to_cooldown(base_cast_time: f64, cast_speed_multiplier: f64) -> f64 {
    if base_cast_time <= 0.0 || cast_speed_multiplier <= 0.0 {
        return 0.0;
    }
    round(base_cast_time / cast_speed_multiplier)
}

// ---------------------------------------------------------------------------
// §五  TraceGraph 归因扩展（触发速率拆解到来源）
// ---------------------------------------------------------------------------

use crate::{TraceGraph, TraceNodeId, TraceOperation};
use pobr_data::prelude::{SourceId, SourceKind};

/// 冷却驱动触发速率的归因版本：把 trigger_cd、triggered_cd、icdr、source_rate 各加到 TraceGraph。
///
/// 返回 `(TriggerRate, skill_trigger_rate_node)`。调用方可继续把下游（DPS 等）连上此节点。
/// 出处：agent-docs/triggers.md §对 pobr 实现的启示 #5（触发速率归因到 SourceId）。
pub fn resolve_trigger_rate_traced(
    trigger_cd: f64,
    triggered_cd: f64,
    icdr: f64,
    effective_source_rate: f64,
    trace: &mut TraceGraph,
) -> (TriggerRate, TraceNodeId) {
    let result = resolve_trigger_rate(trigger_cd, triggered_cd, icdr, effective_source_rate);

    let trigger_cd_node = trace.add_source_node(
        "trigger cooldown (gem)",
        trigger_cd,
        SourceId::new(SourceKind::SkillGem, "trigger.cooldown"),
    );
    let triggered_cd_node = trace.add_source_node(
        "triggered skill cooldown",
        triggered_cd,
        SourceId::new(SourceKind::SkillGem, "triggered.cooldown"),
    );
    let icdr_node = trace.add_source_node(
        "ICDR (cooldown recovery rate)",
        icdr,
        SourceId::new(SourceKind::CharacterBase, "icdr"),
    );
    let source_rate_node = trace.add_source_node(
        "effective source rate (attacks/s)",
        effective_source_rate,
        SourceId::new(SourceKind::CharacterBase, "source.rate"),
    );

    let action_cd_node = trace.add_node(
        "action cooldown = max(triggeredCD, triggerCD/icdr)",
        result.action_cooldown,
        TraceOperation::SelectMax,
    );
    trace.add_edge(trigger_cd_node, action_cd_node);
    trace.add_edge(triggered_cd_node, action_cd_node);
    trace.add_edge(icdr_node, action_cd_node);

    let cap_node = trace.add_node(
        "trigger rate cap (frame-aligned)",
        result.trigger_rate_cap,
        TraceOperation::Cap,
    );
    trace.add_edge(action_cd_node, cap_node);

    let rate_node = trace.add_node(
        "skill trigger rate = min(cap, sourceRate)",
        result.skill_trigger_rate,
        TraceOperation::SelectMax,
    );
    trace.add_edge(cap_node, rate_node);
    trace.add_edge(source_rate_node, rate_node);

    (result, rate_node)
}

/// 能量驱动触发速率的归因版本：把 max_energy、energy_per_event、source_rate 加到 TraceGraph。
///
/// 返回 `(EnergyTriggerRate, effective_trigger_rate_node)`。
#[allow(clippy::too_many_arguments)]
pub fn calc_energy_trigger_rate_traced(
    socketed_spells: &[SocketedSpellInfo],
    condition: TriggerCondition,
    monster_power: f64,
    hit_damage: f64,
    ailment_threshold: f64,
    energy_generated_scale: f64,
    source_rate: f64,
    trigger_cd: f64,
    triggered_cd: f64,
    icdr: f64,
    trace: &mut TraceGraph,
) -> (EnergyTriggerRate, TraceNodeId) {
    let result = calc_energy_trigger_rate(
        socketed_spells,
        condition,
        monster_power,
        hit_damage,
        ailment_threshold,
        energy_generated_scale,
        source_rate,
        trigger_cd,
        triggered_cd,
        icdr,
    );

    let max_energy_node = trace.add_source_node(
        "max energy (socketed spell cast times)",
        result.max_energy,
        SourceId::new(SourceKind::SkillGem, "energy.max"),
    );
    let energy_per_event_node = trace.add_source_node(
        "energy per event (MonsterPower × baseCentienergy / 100)",
        result.energy_per_event,
        SourceId::new(SourceKind::CharacterBase, "energy.per_event"),
    );
    let source_rate_node = trace.add_source_node(
        "source rate (events/s)",
        source_rate,
        SourceId::new(SourceKind::CharacterBase, "source.rate"),
    );
    let raw_rate_node = trace.add_node(
        "raw energy trigger rate (energy_per_second / max_energy)",
        result.raw_trigger_rate,
        TraceOperation::Multiply,
    );
    trace.add_edge(max_energy_node, raw_rate_node);
    trace.add_edge(energy_per_event_node, raw_rate_node);
    trace.add_edge(source_rate_node, raw_rate_node);

    let effective_node = trace.add_node(
        "effective trigger rate (min(raw, cd_cap))",
        result.effective_trigger_rate,
        TraceOperation::Cap,
    );
    trace.add_edge(raw_rate_node, effective_node);

    (result, effective_node)
}

/// CWC 触发速率的归因版本：把 trigger_time、triggered_cd、adds_cast_time、icdr 加到 TraceGraph。
///
/// 返回 `(CwcTriggerRate, trigger_rate_cap_node)`。
pub fn calc_cwc_trigger_rate_traced(
    trigger_time: f64,
    triggered_cd: f64,
    adds_cast_time: f64,
    icdr: f64,
    trace: &mut TraceGraph,
) -> (CwcTriggerRate, TraceNodeId) {
    let result = calc_cwc_trigger_rate(trigger_time, triggered_cd, adds_cast_time, icdr);

    let trigger_time_node = trace.add_source_node(
        "CWC triggerTime (channelling interval)",
        trigger_time,
        SourceId::new(SourceKind::SkillGem, "cwc.triggerTime"),
    );
    let triggered_cd_node = trace.add_source_node(
        "triggered skill cooldown",
        triggered_cd,
        SourceId::new(SourceKind::SkillGem, "triggered.cooldown"),
    );
    let adds_cast_time_node = trace.add_source_node(
        "SpellCastTimeAddedToCooldownIfTriggered",
        adds_cast_time,
        SourceId::new(SourceKind::SkillGem, "triggered.addsCastTime"),
    );
    let icdr_node = trace.add_source_node(
        "ICDR",
        icdr,
        SourceId::new(SourceKind::CharacterBase, "icdr"),
    );

    let interval_node = trace.add_node(
        "adjusted trigger interval (frame-aligned)",
        result.adjusted_trigger_interval,
        TraceOperation::Cap,
    );
    trace.add_edge(trigger_time_node, interval_node);

    let eff_cd_node = trace.add_node(
        "effective triggered CD = max(triggered_cd, adds_cast_time) / icdr",
        result.effective_triggered_cd,
        TraceOperation::SelectMax,
    );
    trace.add_edge(triggered_cd_node, eff_cd_node);
    trace.add_edge(adds_cast_time_node, eff_cd_node);
    trace.add_edge(icdr_node, eff_cd_node);

    let rate_cap_node = trace.add_node(
        "CWC trigger rate cap",
        result.trigger_rate_cap,
        TraceOperation::SelectMax,
    );
    trace.add_edge(interval_node, rate_cap_node);
    trace.add_edge(eff_cd_node, rate_cap_node);

    (result, rate_cap_node)
}

// ---------------------------------------------------------------------------
// §一  内部单元测试（冷却驱动基础）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_tick_rate_matches_constant() {
        // 1 / 0.033 ≈ 30.30/s。
        assert!((server_tick_rate() - 30.303_030_303).abs() < 1e-6);
    }

    #[test]
    fn cooldown_rounds_up_to_frame() {
        // 0.10s 冷却在 30.3/s 帧率下：ceil(0.10 × 30.303) = ceil(3.03) = 4 帧 → 4/30.303 ≈ 0.132s。
        let rate = server_tick_rate();
        let rounded = round_cooldown_to_tick(0.10, rate);
        assert!((rounded - 4.0 / rate).abs() < 1e-9);
        assert!(rounded > 0.10); // 取整后冷却变长。
    }

    #[test]
    fn cap_is_inverse_of_rounded_cooldown() {
        let rate = server_tick_rate();
        let cd = 0.15;
        let cap = trigger_rate_cap(cd, rate);
        let rounded = round_cooldown_to_tick(cd, rate);
        assert!((cap - 1.0 / rounded).abs() < 1e-6);
    }

    #[test]
    fn icdr_shortens_trigger_cooldown() {
        // trigger_cd=0.3, icdr=1.5 → 0.2；被触发技能无冷却 → action_cd=0.2。
        let cd = action_cooldown(0.3, 0.0, 1.5);
        assert!((cd - 0.2).abs() < 1e-9);
    }

    #[test]
    fn larger_of_two_cooldowns_wins() {
        // triggered_cd=0.5 大于 trigger_cd/icdr=0.3 → action_cd=0.5。
        let cd = action_cooldown(0.3, 0.5, 1.0);
        assert!((cd - 0.5).abs() < 1e-9);
    }

    #[test]
    fn source_rate_gates_trigger_rate() {
        // 上限远高于源速率 2/s → 实际速率被源门控为 2/s。
        let r = resolve_trigger_rate(0.05, 0.0, 1.0, 2.0);
        assert!(r.limited_by_source);
        assert!((r.skill_trigger_rate - 2.0).abs() < 1e-6);
    }

    #[test]
    fn cap_gates_when_source_is_fast() {
        // 源速率 100/s 高于上限 → 实际速率 = 上限。
        let r = resolve_trigger_rate(0.3, 0.0, 1.0, 100.0);
        assert!(!r.limited_by_source);
        assert!((r.skill_trigger_rate - r.trigger_rate_cap).abs() < 1e-9);
    }

    // ---------------------------------------------------------------------------
    // §二  能量模型测试
    // ---------------------------------------------------------------------------

    #[test]
    fn max_energy_single_spell_0_5s() {
        // base_cast_time = 0.5s → effective = 0.5s → (0.5/0.1)×10 = 50。
        let spells = [SocketedSpellInfo::new(0.5)];
        assert!((calc_max_energy(&spells) - 50.0).abs() < 1e-6);
    }

    #[test]
    fn max_energy_two_spells() {
        // 0.3s + 0.6s = 0.9s → 90 能量。
        let spells = [SocketedSpellInfo::new(0.3), SocketedSpellInfo::new(0.6)];
        assert!((calc_max_energy(&spells) - 90.0).abs() < 1e-6);
    }

    #[test]
    fn max_energy_use_time_penalty_doubled() {
        // base=0.5s, use_time_increase=20% → effective = 0.5 × (1 + 0.20 × 2) = 0.5 × 1.4 = 0.7s。
        // max_energy = (0.7/0.1)×10 = 70。
        let spell = SocketedSpellInfo::new(0.5).with_use_time_increase(20.0);
        assert!((spell.effective_cast_time_for_energy() - 0.7).abs() < 1e-9);
        let spells = [spell];
        assert!((calc_max_energy(&spells) - 70.0).abs() < 1e-6);
    }

    #[test]
    fn energy_per_event_freeze_10x_crit_same_ratio() {
        // Freeze base_centienergy=1000 vs Crit base_centienergy=100（ratio=1时）→ 10 倍。
        // CoC ratio=1 时（hit_damage=ailment_threshold）：crit = 1×100×1/100 = 1。
        // Freeze：freeze = 1×1000/100 = 10。
        let crit_ratio1 =
            calc_energy_per_event(TriggerCondition::CriticalStrike, 1.0, 100.0, 100.0, 1.0);
        let freeze = calc_energy_per_event(TriggerCondition::Freeze, 1.0, 0.0, 1.0, 1.0);
        assert!(
            (crit_ratio1 - 1.0).abs() < 1e-6,
            "crit_ratio1={crit_ratio1}"
        );
        assert!((freeze - 10.0).abs() < 1e-6, "freeze={freeze}");
        assert!(
            (freeze / crit_ratio1 - 10.0).abs() < 1e-3,
            "freeze={freeze} crit={crit_ratio1}"
        );
    }

    #[test]
    fn energy_per_event_coc_damage_ratio() {
        // CoC：MonsterPower=1, hit_damage=500, threshold=100 → ratio=5 → energy=1×100×5/100=5。
        let e = calc_energy_per_event(TriggerCondition::CriticalStrike, 1.0, 500.0, 100.0, 1.0);
        assert!((e - 5.0).abs() < 1e-6);
    }

    #[test]
    fn energy_per_event_scale_increases_gain() {
        // energy_generated_scale=1.57（lvl20 +57%）应比 1.0 多 57% 能量。
        let base = calc_energy_per_event(TriggerCondition::Shock, 2.0, 0.0, 1.0, 1.0);
        let scaled = calc_energy_per_event(TriggerCondition::Shock, 2.0, 0.0, 1.0, 1.57);
        assert!((scaled / base - 1.57).abs() < 1e-3);
    }

    #[test]
    fn energy_trigger_rate_increases_with_source_rate() {
        // source_rate 增大时 effective_trigger_rate 单调非递减（受冷却上限截断）。
        let spells = [SocketedSpellInfo::new(0.5)];
        let low = calc_energy_trigger_rate(
            &spells,
            TriggerCondition::Shock,
            5.0,
            0.0,
            1.0,
            1.0,
            2.0,
            0.3,
            0.0,
            1.0,
        );
        let high = calc_energy_trigger_rate(
            &spells,
            TriggerCondition::Shock,
            5.0,
            0.0,
            1.0,
            1.0,
            5.0,
            0.3,
            0.0,
            1.0,
        );
        assert!(high.effective_trigger_rate >= low.effective_trigger_rate);
    }

    #[test]
    fn energy_trigger_rate_limited_by_cooldown() {
        // 很高 source_rate 且能量充足 → 应被冷却上限截断。
        let spells = [SocketedSpellInfo::new(0.1)]; // max_energy=10（小 → 产能很快）
        let r = calc_energy_trigger_rate(
            &spells,
            TriggerCondition::Freeze,
            20.0, // 高 MonsterPower
            0.0,
            1.0,
            1.0,
            100.0, // 源速率极高
            0.5,   // 触发宝石冷却 0.5s → cap ≈ 2/s
            0.0,
            1.0,
        );
        assert!(r.limited_by_cooldown, "should be limited by cooldown cap");
        // effective ≤ cd_cap。
        assert!(r.effective_trigger_rate <= r.cooldown_rate_cap + 1e-6);
    }

    #[test]
    fn energy_trigger_rate_no_spells_yields_zero() {
        let r = calc_energy_trigger_rate(
            &[],
            TriggerCondition::Shock,
            5.0,
            0.0,
            1.0,
            1.0,
            3.0,
            0.3,
            0.0,
            1.0,
        );
        assert_eq!(r.max_energy, 0.0);
        assert_eq!(r.effective_trigger_rate, 0.0);
    }

    // ---------------------------------------------------------------------------
    // §三  多技能轮转测试
    // ---------------------------------------------------------------------------

    #[test]
    fn single_skill_rotation_no_waste() {
        // 单技能轮转：每次触发机会都触发该技能（无浪费），速率 ≈ source_rate（受冷却上限限）。
        let skill = RotationSkill::new(0.15); // 0.15s 冷却。
        let source_rate = 4.0; // 4/s 源速率。
        let result = calc_multi_spell_rotation(&[skill], source_rate);
        assert_eq!(result.rates.len(), 1);
        // 速率上界 = source_rate（0.15s 冷却 << 0.25s 触发间隔，不构成瓶颈）。
        assert!(result.rates[0] > 0.0);
        assert_eq!(result.wasted_fraction, 0.0); // 单技能无浪费。
    }

    #[test]
    fn two_skills_share_trigger_opportunities() {
        // 两个技能，源速率 4/s，每个技能冷却比触发间隔长 → 共享触发机会，各自速率 < 源速率。
        let skill_a = RotationSkill::new(0.5);
        let skill_b = RotationSkill::new(0.5);
        let source_rate = 4.0;
        let result = calc_multi_spell_rotation(&[skill_a, skill_b], source_rate);
        assert_eq!(result.rates.len(), 2);
        let total: f64 = result.rates.iter().sum::<f64>();
        // 总触发速率 ≤ 源速率（可能有浪费）。
        assert!(total <= source_rate + 1e-6);
        // 每个技能都有非零速率。
        assert!(result.rates[0] > 0.0);
        assert!(result.rates[1] > 0.0);
    }

    #[test]
    fn rotation_with_long_cooldowns_causes_waste() {
        // 所有技能冷却极长（10s），触发频率很高（10/s）→ 大量触发机会浪费。
        let skills: Vec<RotationSkill> = (0..3).map(|_| RotationSkill::new(10.0)).collect();
        let source_rate = 10.0;
        let result = calc_multi_spell_rotation(&skills, source_rate);
        // 极长冷却下大多数触发机会浪费。
        assert!(
            result.wasted_fraction > 0.5,
            "wasted={}",
            result.wasted_fraction
        );
    }

    #[test]
    fn rotation_trigger_chance_reduces_rate() {
        // 触发几率 50% 的技能，稳态速率约为 chance=1 时的一半（几何分布期望值近似）。
        let full_chance = RotationSkill::new(0.3).with_trigger_chance(1.0);
        let half_chance = RotationSkill::new(0.3).with_trigger_chance(0.5);
        let source_rate = 3.0;
        let r_full = calc_multi_spell_rotation(&[full_chance], source_rate);
        let r_half = calc_multi_spell_rotation(&[half_chance], source_rate);
        // 50% 几率的速率应显著低于 100% 几率。
        assert!(r_half.rates[0] < r_full.rates[0]);
    }

    #[test]
    fn empty_rotation_returns_empty() {
        let result = calc_multi_spell_rotation(&[], 5.0);
        assert!(result.rates.is_empty());
    }

    #[test]
    fn rotation_zero_source_rate_returns_zeros() {
        let skill = RotationSkill::new(0.3);
        let result = calc_multi_spell_rotation(&[skill], 0.0);
        assert!(result.rates.is_empty() || result.rates.iter().all(|&r| r == 0.0));
    }

    #[test]
    fn added_cooldown_slows_rotation() {
        // 无 added_cooldown 与有 added_cooldown 相比，后者触发速率更低。
        let no_add = RotationSkill::new(0.3).with_added_cooldown(0.0);
        let with_add = RotationSkill::new(0.3).with_added_cooldown(0.5);
        let source_rate = 5.0;
        let r_no = calc_multi_spell_rotation(&[no_add], source_rate);
        let r_with = calc_multi_spell_rotation(&[with_add], source_rate);
        assert!(r_with.rates[0] <= r_no.rates[0] + 1e-9);
    }

    // ---------------------------------------------------------------------------
    // §四  CWC 测试
    // ---------------------------------------------------------------------------

    #[test]
    fn cwc_basic_trigger_rate() {
        // triggerTime=0.3s → ceil(0.3 × 30.303) = 10 帧 → 10/30.303 ≈ 0.33s → rate ≈ 3.03/s。
        let r = calc_cwc_trigger_rate(0.3, 0.0, 0.0, 1.0);
        let tick_rate = server_tick_rate();
        let expected_interval = round_cooldown_to_tick(0.3, tick_rate);
        assert!((r.adjusted_trigger_interval - expected_interval).abs() < 1e-9);
        assert!((r.channelling_trigger_rate - 1.0 / expected_interval).abs() < 1e-6);
        assert!(!r.limited_by_triggered_cd);
    }

    #[test]
    fn cwc_triggered_cd_limits_rate() {
        // triggered_cd=1.0s >> triggerTime=0.1s → 被触发技能冷却成为瓶颈。
        let r = calc_cwc_trigger_rate(0.1, 1.0, 0.0, 1.0);
        assert!(r.limited_by_triggered_cd, "triggered CD should limit rate");
        assert!(r.trigger_rate_cap < r.channelling_trigger_rate);
    }

    #[test]
    fn cwc_icdr_increases_trigger_rate() {
        // ICDR=2.0 把 0.6s triggered_cd 缩短到 0.3s → rate 上限提高。
        let r_no_icdr = calc_cwc_trigger_rate(0.2, 0.6, 0.0, 1.0);
        let r_icdr = calc_cwc_trigger_rate(0.2, 0.6, 0.0, 2.0);
        assert!(r_icdr.trigger_rate_cap >= r_no_icdr.trigger_rate_cap);
    }

    #[test]
    fn cwc_adds_cast_time_increases_effective_cd() {
        // adds_cast_time=0.5s 追加到冷却 → effective_triggered_cd 比 triggered_cd=0.2s 大。
        let r = calc_cwc_trigger_rate(0.2, 0.2, 0.5, 1.0);
        // max(0.2, 0.5) = 0.5s → effective_triggered_cd = 0.5。
        assert!((r.effective_triggered_cd - 0.5).abs() < 1e-6);
        assert!(r.limited_by_triggered_cd);
    }

    #[test]
    fn spell_cast_time_to_cooldown_basic() {
        // base=0.5s, cast_speed=1.5 → 0.5/1.5 ≈ 0.333s。
        let added = spell_cast_time_added_to_cooldown(0.5, 1.5);
        assert!((added - 0.5 / 1.5).abs() < 1e-6);
    }

    #[test]
    fn spell_cast_time_to_cooldown_no_speed_bonus() {
        // 无施法速度加成（乘数=1.0）→ 追加冷却 = 基础施法时间。
        let added = spell_cast_time_added_to_cooldown(0.8, 1.0);
        assert!((added - 0.8).abs() < 1e-9);
    }

    // ---------------------------------------------------------------------------
    // §五  归因测试
    // ---------------------------------------------------------------------------

    #[test]
    fn trace_graph_nodes_created_for_trigger_rate() {
        let mut trace = TraceGraph::new();
        let (result, rate_node) = resolve_trigger_rate_traced(0.3, 0.0, 1.0, 5.0, &mut trace);
        assert!(trace.nodes().len() >= 5); // 至少 5 个节点。
        let node = trace.node(rate_node).unwrap();
        assert!((node.value - result.skill_trigger_rate).abs() < 1e-9);
        // rate_node 应有来自 cap_node 和 source_rate_node 的入边。
        let incoming = trace.incoming(rate_node);
        assert!(incoming.len() >= 2);
    }

    #[test]
    fn trace_graph_energy_trigger_rate() {
        let spells = [SocketedSpellInfo::new(0.5)];
        let mut trace = TraceGraph::new();
        let (result, node) = calc_energy_trigger_rate_traced(
            &spells,
            TriggerCondition::Shock,
            5.0,
            0.0,
            1.0,
            1.0,
            3.0,
            0.3,
            0.0,
            1.0,
            &mut trace,
        );
        let n = trace.node(node).unwrap();
        assert!((n.value - result.effective_trigger_rate).abs() < 1e-9);
    }

    #[test]
    fn trace_graph_cwc_trigger_rate() {
        let mut trace = TraceGraph::new();
        let (result, node) = calc_cwc_trigger_rate_traced(0.3, 0.5, 0.0, 1.0, &mut trace);
        let n = trace.node(node).unwrap();
        assert!((n.value - result.trigger_rate_cap).abs() < 1e-9);
        assert!(trace.nodes().len() >= 4);
    }
}
