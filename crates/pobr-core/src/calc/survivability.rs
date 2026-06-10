//! 生存性辅助计算（reservation / regen / capped chance / charges / leech / recoup）。
//!
//! 资料：`agent-docs/active-defences.md`、`agent-docs/block.md`、
//! `agent-docs/recovery-charges-buffs.md`（PoE2 0.5.0）。
//!
//! - **Reservation**：光环 / 守护按 `Σ flat + 池子 * (Σ % / 100)` 预留，结果钳到 [0, pool]。
//! - **Regen**：`base_flat + pool * (Σ %regen / 100)`，再吃 inc/more 恢复速率（含 RecoveryRateMod）。
//! - **Capped chance**：几率类（block）求和后钳到 [0, cap]。
//! - **Charges**：充能层数/上限解析；PoE2 充能无固有属性，仅供 per-charge 词条乘数引用。
//! - **Leech**：0.5.0 重制——单资源单实例（取最高速率），三层上限，默认仅物理。
//! - **Recoup**：承受伤害的一定比例在 8s（或 4s）内返还。

use crate::{CalcConfig, ModDb};
use pobr_data::prelude::*;

use super::round;

// ─────────────────────────────────────────────────────────────────
// 预留 (Reservation)
// ─────────────────────────────────────────────────────────────────

/// 预留结果：预留量 + 剩余可用量。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Reservation {
    pub reserved: f64,
    pub unreserved: f64,
}

/// 计算池子（life / mana）的预留量。
///
/// `flat` 是固定预留之和，`percent` 是百分比预留之和（如 50 表示 50%）。
/// 结果钳到 `[0, pool]`，`unreserved = pool - reserved`。
pub fn reservation(pool: f64, flat: f64, percent: f64) -> Reservation {
    if pool <= 0.0 {
        return Reservation {
            reserved: 0.0,
            unreserved: 0.0,
        };
    }
    let raw = flat + pool * (percent / 100.0);
    let reserved = round(raw.clamp(0.0, pool));
    Reservation {
        reserved,
        unreserved: round(pool - reserved),
    }
}

// ─────────────────────────────────────────────────────────────────
// 再生 (Regeneration)
// ─────────────────────────────────────────────────────────────────

/// 计算每秒恢复（regen）。
///
/// `base_flat` 是固定每秒恢复之和，`percent` 是按池子百分比恢复之和，
/// `inc` / `more` 为恢复速率增益（% 加法 + more 连乘）。
pub fn regen(pool: f64, base_flat: f64, percent: f64, inc: f64, more: f64) -> f64 {
    let base = base_flat + pool * (percent / 100.0);
    round(base * (1.0 + inc / 100.0) * more)
}

/// 计算每秒恢复（regen），含 `RecoveryRateMod` 全局恢复速率乘数。
///
/// PoB2 `CalcDefence.lua`：`regen × RecoveryRateMod`（三类资源各有独立的 `XRecoveryRate`，
/// 其 inc/more 已并入 `regen` 的 inc/more 参数；此处额外乘外部传入的 `recovery_rate_mod`，
/// 对应 `output.LifeRecoveryRateMod` / `ManaRecoveryRateMod` / `EnergyShieldRecoveryRateMod`）。
///
/// 出处：agent-docs/recovery-charges-buffs.md §2.1；
///       PoB2 `src/Modules/CalcDefence.lua` 再生段。
pub fn regen_with_rate(
    pool: f64,
    base_flat: f64,
    percent: f64,
    inc: f64,
    more: f64,
    recovery_rate_mod: f64,
) -> f64 {
    let base_regen = regen(pool, base_flat, percent, inc, more);
    round(base_regen * recovery_rate_mod.max(0.0))
}

/// 从 ModDb 中查询并计算某资源的每秒再生速率。
///
/// `stat` 取值：`"LifeRegen"` / `"ManaRegen"` / `"EnergyShieldRegen"`。
/// inc/more 同时参考 `<stat>Rate`（专属速率）和 `XRecoveryRate`（全局恢复速率），
/// 与 PoB2 `CalcDefence.lua` 「`XRegen` + `XRecoveryRate` 合并」行为一致。
///
/// 出处：agent-docs/recovery-charges-buffs.md §2.1；
///       PoB2 `src/Modules/CalcDefence.lua` 再生段注释。
pub fn calc_regen(db: &ModDb, cfg: &CalcConfig, pool: f64, stat: &str) -> f64 {
    let flat = db.sum(ModType::Base, cfg, &[ModName::from(stat)]);
    let percent = db.sum(
        ModType::Base,
        cfg,
        &[ModName::from(format!("{stat}Percent"))],
    );
    // inc/more：同时吃专属 `<stat>Rate` 和通用 `XRecoveryRate`
    let rate_name = ModName::from(format!("{stat}Rate"));
    let recovery_rate_name = recovery_rate_mod_name(stat);
    let inc = db.sum(ModType::Inc, cfg, std::slice::from_ref(&rate_name))
        + db.sum(ModType::Inc, cfg, std::slice::from_ref(&recovery_rate_name));
    let more_stat = db.more(cfg, &[rate_name]);
    let more_recovery = db.more(cfg, &[recovery_rate_name]);
    let more = more_stat * more_recovery;
    regen(pool, flat, percent, inc, more)
}

/// 根据资源名返回对应的 `XRecoveryRate` ModName。
fn recovery_rate_mod_name(stat: &str) -> ModName {
    let prefix = stat
        .trim_end_matches("Regen")
        .trim_end_matches("RegenPercent");
    ModName::from(format!("{prefix}RecoveryRate"))
}

// ─────────────────────────────────────────────────────────────────
// 几率类 (Capped Chance)
// ─────────────────────────────────────────────────────────────────

/// 几率类聚合：求和后钳到 `[0, cap]`（cap 通常 75% 或 100%）。
pub fn capped_chance(percent_sum: f64, cap: f64) -> f64 {
    round(percent_sum.clamp(0.0, cap))
}

/// 格挡几率（PoE2 硬上限 90%，`data.misc.BlockChanceCap = 90`）。
///
/// **Bug#11 修正（block-chance-cap-wrong）**：PoE2 格挡上限为 90%，非 PoE1 的 75%。
/// 出处：agent-docs/block.md §被动格挡、PoB2 DeepWiki `BlockChanceCap = 90`。
///
/// M0-W3：cap 改由调用方自注入常量包传入
/// （`cfg.constants.game().block_chance_cap`，fallback == 旧 const，值不变）。
pub fn block_chance(percent_sum: f64, cap: f64) -> f64 {
    capped_chance(percent_sum, cap)
}

// ─────────────────────────────────────────────────────────────────
// 充能 (Charges) — PoE2：充能无固有属性，仅供 per-charge 词条引用
// ─────────────────────────────────────────────────────────────────

/// 充能默认最大层数（Power / Frenzy / Endurance；PoB2 `Data/Misc.lua`）。
///
/// 出处：agent-docs/recovery-charges-buffs.md §1.3；
///       PoB2 `src/Data/Misc.lua`: `max_power_charges = max_frenzy_charges = max_endurance_charges = 3`。
pub const DEFAULT_MAX_CHARGES: u32 = 3;

/// 充能默认持续时间（秒；0.5.0 已从 20s 改为 15s）。
///
/// 出处：agent-docs/recovery-charges-buffs.md §1.3；
///       PoB2 `src/Modules/CalcSetup.lua`: `NewMod("ChargeDuration","BASE",15,"Base")`。
pub const DEFAULT_CHARGE_DURATION_SECONDS: f64 = 15.0;

/// 充能种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChargeKind {
    Power,
    Frenzy,
    Endurance,
}

impl ChargeKind {
    /// 对应的 `XChargesMax` ModName（最大充能层数修饰词名称）。
    pub fn max_mod_name(self) -> ModName {
        match self {
            ChargeKind::Power => ModName::from("PowerChargesMax"),
            ChargeKind::Frenzy => ModName::from("FrenzyChargesMax"),
            ChargeKind::Endurance => ModName::from("EnduranceChargesMax"),
        }
    }

    /// 对应的 `XChargesMin` ModName（最小充能层数修饰词名称）。
    pub fn min_mod_name(self) -> ModName {
        match self {
            ChargeKind::Power => ModName::from("PowerChargesMin"),
            ChargeKind::Frenzy => ModName::from("FrenzyChargesMin"),
            ChargeKind::Endurance => ModName::from("EnduranceChargesMin"),
        }
    }

    /// 乘数名称（供 `CalcConfig.multipliers` 查询，与 `Modifier::effective_number` Multiplier tag 同构）。
    ///
    /// 出处：agent-docs/recovery-charges-buffs.md §1.1；
    ///       PoB2 `CalcSetup.lua`: `modDB.multipliers["PowerCharge" | "FrenzyCharge" | "EnduranceCharge"]`。
    pub fn multiplier_key(self) -> &'static str {
        match self {
            ChargeKind::Power => "PowerCharge",
            ChargeKind::Frenzy => "FrenzyCharge",
            ChargeKind::Endurance => "EnduranceCharge",
        }
    }
}

/// 充能层数解析结果。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChargeState {
    /// 当前充能层数（已按上限/下限钳位）。
    pub current: u32,
    /// 最大充能层数（含 `+N to Maximum X Charges` 词条）。
    pub maximum: u32,
    /// 最小充能层数（常驻层数）。
    pub minimum: u32,
}

/// 从 ModDb 解析某种充能的最大层数（默认 3 + `+N to Maximum X Charges` BASE 词条）。
///
/// 出处：agent-docs/recovery-charges-buffs.md §1.3；
///       PoB2 `CalcSetup.lua`: max base = 3，`XChargesMax` BASE 词条加成。
pub fn charge_maximum(db: &ModDb, cfg: &CalcConfig, kind: ChargeKind) -> u32 {
    let extra = db.sum(ModType::Base, cfg, &[kind.max_mod_name()]);
    let max_val = DEFAULT_MAX_CHARGES as f64 + extra;
    max_val.max(0.0) as u32
}

/// 从 ModDb 解析某种充能的最小层数（默认 0；`+N to Minimum X Charges` 可设 constant floor）。
///
/// 出处：agent-docs/recovery-charges-buffs.md §1.5；
///       PoB2 `CalcSetup.lua`: `MinimumXChargesIsMaximumXCharges` 旗标可把最小拉满到上限。
pub fn charge_minimum(db: &ModDb, cfg: &CalcConfig, kind: ChargeKind, maximum: u32) -> u32 {
    // MinimumXChargesIsMaximumXCharges：最小充能 = 最大充能（常驻满层）。
    let full_flag_name = match kind {
        ChargeKind::Power => "MinimumPowerChargesIsMaximumPowerCharges",
        ChargeKind::Frenzy => "MinimumFrenzyChargesIsMaximumFrenzyCharges",
        ChargeKind::Endurance => "MinimumEnduranceChargesIsMaximumEnduranceCharges",
    };
    if db.flag(cfg, ModName::from(full_flag_name)) {
        return maximum;
    }

    let min_val = db.sum(ModType::Base, cfg, &[kind.min_mod_name()]);
    (min_val.max(0.0) as u32).min(maximum)
}

/// 解析充能状态：从 `CalcConfig.multipliers` 读取当前层数，结合 ModDb 的最大/最小值钳位。
///
/// PoB2 通过 `modDB.multipliers["PowerCharge"]` 暴露当前层数供 per-charge 词条引用（Multiplier tag）；
/// pobr 使用 `CalcConfig.multipliers` 保持同构。
///
/// # 参数
/// - `db` — 玩家 ModDb（查询最大/最小充能层数词条）。
/// - `cfg` — 当前计算配置（`cfg.multiplier("PowerCharge")` 等存放当前层数）。
/// - `kind` — 充能种类（Power / Frenzy / Endurance）。
///
/// 出处：agent-docs/recovery-charges-buffs.md §1.1 & §1.5；
///       PoB2 `src/Modules/CalcSetup.lua` 充能段。
pub fn resolve_charge_state(db: &ModDb, cfg: &CalcConfig, kind: ChargeKind) -> ChargeState {
    let maximum = charge_maximum(db, cfg, kind);
    let minimum = charge_minimum(db, cfg, kind, maximum);
    let raw_current = cfg.multiplier(kind.multiplier_key());
    let current = (raw_current.max(0.0) as u32).clamp(minimum, maximum);
    ChargeState {
        current,
        maximum,
        minimum,
    }
}

/// 所有三种充能的聚合状态。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AllChargeStates {
    pub power: ChargeState,
    pub frenzy: ChargeState,
    pub endurance: ChargeState,
}

impl AllChargeStates {
    /// 三种充能合计层数（`TotalCharges` 乘数所用）。
    ///
    /// 出处：agent-docs/recovery-charges-buffs.md §充能乘数；
    ///       PoB2 `CalcSetup.lua`: `modDB.multipliers["TotalCharges"]`。
    pub fn total(&self) -> u32 {
        self.power.current + self.frenzy.current + self.endurance.current
    }
}

/// 解析三种充能的完整状态。
pub fn resolve_all_charges(db: &ModDb, cfg: &CalcConfig) -> AllChargeStates {
    AllChargeStates {
        power: resolve_charge_state(db, cfg, ChargeKind::Power),
        frenzy: resolve_charge_state(db, cfg, ChargeKind::Frenzy),
        endurance: resolve_charge_state(db, cfg, ChargeKind::Endurance),
    }
}

/// 按 PoB2 充能口径填充充能层数 multiplier，返回派生 cfg。
///
/// PoB2（CalcPerform.lua L831-832 / L899）：仅当 `Condition:UseXCharges` 为真时
/// `output.XCharges = output.XChargesMax`，随后 `modDB.multipliers["XCharge"] = output.XCharges`，
/// 使 `per X charge` 词条按满层展开。**未启用该充能的 build，PoB2 面板 current=0**（如
/// stormweaver `PowerCharges value="0"`），故此处也保持 0，避免错误施加 `per charge` 罚减/增益。
/// pobr 用 `CalcConfig.multipliers` 与 `Condition:UseXCharges` 同构。
///
/// **覆盖语义**：若 cfg 已显式设正值（build 导入的 `XCharges` 数量覆盖 / 非满层），沿用原值；
/// 若 `MinimumXChargesIsMaximumXCharges`（常驻满层），`charge_minimum` 会返回 maximum，
/// 即便未勾选使用条件也按常驻层填充。
pub fn charge_multipliers_panel_default(db: &ModDb, cfg: &CalcConfig) -> CalcConfig {
    let mut out = cfg.clone();
    for kind in [ChargeKind::Power, ChargeKind::Frenzy, ChargeKind::Endurance] {
        let key = kind.multiplier_key();
        // 已显式设为正值则尊重 build 覆盖。
        if out.multiplier(key) > 0.0 {
            continue;
        }
        let maximum = charge_maximum(db, cfg, kind);
        let minimum = charge_minimum(db, cfg, kind, maximum);
        // 使用条件（PoB2 `Condition:UseXCharges`）：勾选则满层，否则取常驻最小（通常 0）。
        let use_cond = match kind {
            ChargeKind::Power => "UsePowerCharges",
            ChargeKind::Frenzy => "UseFrenzyCharges",
            ChargeKind::Endurance => "UseEnduranceCharges",
        };
        let current = if cfg.condition(use_cond) {
            maximum.max(minimum)
        } else {
            minimum
        };
        if current > 0 {
            out = out.with_multiplier(key, current as f64);
        }
    }
    out
}

// ─────────────────────────────────────────────────────────────────
// 偷取 (Leech) — 0.5.0 重制
// ─────────────────────────────────────────────────────────────────

/// 偷取速率上限（生命/法力：池子的 20%；ES：池子的 10%）。
///
/// 出处：agent-docs/recovery-charges-buffs.md §2.2；
///       PoB2 `src/Modules/CalcSetup.lua`: `MaxLifeLeechRate=20, MaxManaLeechRate=20, MaxEnergyShieldLeechRate=10`。
pub const LEECH_MAX_LIFE_RATE_PCT: f64 = 20.0;
pub const LEECH_MAX_MANA_RATE_PCT: f64 = 20.0;
pub const LEECH_MAX_ES_RATE_PCT: f64 = 10.0;

/// 偷取单实例上限（各资源池的 10%）。
///
/// 出处：agent-docs/recovery-charges-buffs.md §2.2；
///       PoB2 `src/Modules/CalcSetup.lua`: `MaxLifeLeechInstance=10`。
pub const LEECH_MAX_INSTANCE_PCT: f64 = 10.0;

/// 用于偷取计算的单击有效伤害上限（超过部分被截断）。
///
/// 出处：agent-docs/recovery-charges-buffs.md §2.2；
///       PoB2 `src/Data/Misc.lua`: `EffectiveMaxDamageForLeech = 40000`。
pub const LEECH_EFFECTIVE_MAX_HIT_DAMAGE: f64 = 40000.0;

/// 偷取的偷取资源类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeechResource {
    Life,
    Mana,
    EnergyShield,
}

impl LeechResource {
    /// 该资源对应的「偷取速率上限（占池子的 %）」。
    pub fn max_rate_pct(self) -> f64 {
        match self {
            LeechResource::Life => LEECH_MAX_LIFE_RATE_PCT,
            LeechResource::Mana => LEECH_MAX_MANA_RATE_PCT,
            LeechResource::EnergyShield => LEECH_MAX_ES_RATE_PCT,
        }
    }

    /// 偷取来源的 ModName（`LifeLeech` / `ManaLeech` / `EnergyShieldLeech`，BASE%）。
    pub fn leech_mod_name(self) -> ModName {
        match self {
            LeechResource::Life => ModName::from("LifeLeech"),
            LeechResource::Mana => ModName::from("ManaLeech"),
            LeechResource::EnergyShield => ModName::from("EnergyShieldLeech"),
        }
    }
}

/// 偷取计算输出（0.5.0 单实例口径）。
///
/// 0.5.0 关键变化：每种资源**同时只有一个偷取实例**；pobr 按「取最高速率实例」估算面板值。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LeechResult {
    /// 单实例偷取总量（= `effective_hit × leech_pct%`，受 instance_cap 钳位）。
    pub instance_total: f64,
    /// 偷取速率上限（每秒；= `pool × max_rate_pct%`）。
    pub rate_cap_per_second: f64,
    /// 面板偷取速率（每秒；min(instance_总量 × 2%/s, rate_cap)）。
    ///
    /// PoE 系列偷取默认每秒恢复偷取量的 2%（PoB2 `CalcSetup.lua` 无此常量但行业共识值，
    /// 从偷取总量 / 持续时间反推；0.5.0 单实例下该速率即面板值）。
    pub display_rate_per_second: f64,
}

/// 计算某资源的偷取面板速率（0.5.0 单实例模型）。
///
/// # 参数
/// - `pool` — 对应资源池（生命 / 法力 / ES）最终值。
/// - `leech_pct` — 本次击中伤害×偷取%，即偷取词条百分比之和（如 `LifeLeech` BASE 0.5 表示 0.5%）。
/// - `hit_damage` — 用于偷取的击中伤害（物理/元素；受 `LEECH_EFFECTIVE_MAX_HIT_DAMAGE` 截断）。
/// - `resource` — 偷取资源类型（决定速率上限）。
///
/// # 说明
/// - PoE2 默认偷取「实例速率 = 偷取总量 × 2%/s」（PoE 系列约定）；
/// - 0.5.0 单实例：只取最高速率的一个实例（此函数计算单实例，调用方可取最高值）；
/// - 单实例总量上限：`pool × LEECH_MAX_INSTANCE_PCT%`；
/// - 速率上限：`pool × max_rate_pct%`；
/// - `CannotLeechXxx` 旗标由调用方在传入前短路（返回 `LeechResult::zero(pool, resource)`）。
///
/// 出处：agent-docs/recovery-charges-buffs.md §2.2；
///       PoB2 `src/Modules/CalcDefence.lua` 偷取段 / `CalcSetup.lua` 上限常量。
pub fn calc_leech(
    pool: f64,
    leech_pct: f64,
    hit_damage: f64,
    resource: LeechResource,
) -> LeechResult {
    if pool <= 0.0 || leech_pct <= 0.0 {
        return LeechResult::zero(pool, resource);
    }
    // 单击有效伤害上限（PoB2 Data/Misc.lua EffectiveMaxDamageForLeech = 40000）
    let effective_hit = hit_damage.clamp(0.0, LEECH_EFFECTIVE_MAX_HIT_DAMAGE);
    // 单实例偷取总量：吃单实例上限（pool × 10%）
    let instance_cap = pool * LEECH_MAX_INSTANCE_PCT / 100.0;
    let instance_total = round((effective_hit * leech_pct / 100.0).min(instance_cap));

    // 速率上限：pool × max_rate_pct% /s（CalcSetup.lua MaxLifeLeechRate=20 等）。
    // 面板显示速率 = min(rate_cap, instance_total × (rate_cap / instance_cap))
    // = min(rate_cap, instance_total × max_rate_pct / max_instance_pct)
    // = min(rate_cap, instance_total × 2)  when max_rate = 20%, max_inst = 10%
    // 上式推导：实例持续时间 = instance_total / rate_cap；速率 = instance_total / duration。
    // 简化：若 instance_total == instance_cap → display_rate = rate_cap（最大速率）；
    //        若 instance_total < instance_cap → display_rate = instance_total × (rate_cap / instance_cap)。
    let rate_cap = pool * resource.max_rate_pct() / 100.0;
    let ratio = resource.max_rate_pct() / LEECH_MAX_INSTANCE_PCT; // rate_cap / instance_cap（规格化）
    let display_rate_per_second = round((instance_total * ratio).min(rate_cap));

    LeechResult {
        instance_total,
        rate_cap_per_second: round(rate_cap),
        display_rate_per_second,
    }
}

/// 从 ModDb 计算某资源的偷取结果。
///
/// `leech_mod` 已内化为 `resource.leech_mod_name()`；
/// 调用方负责传入 `hit_damage`（通常是物理/击中伤害；PoE2 默认仅物理）。
///
/// 出处：agent-docs/recovery-charges-buffs.md §2.2；
///       PoB2 `src/Modules/CalcDefence.lua` 偷取段。
pub fn calc_leech_from_db(
    db: &ModDb,
    cfg: &CalcConfig,
    pool: f64,
    hit_damage: f64,
    resource: LeechResource,
) -> LeechResult {
    // CannotLeechXxx 旗标短路
    let cannot_flag = match resource {
        LeechResource::Life => "CannotLeechLife",
        LeechResource::Mana => "CannotLeechMana",
        LeechResource::EnergyShield => "CannotLeechEnergyShield",
    };
    if db.flag(cfg, ModName::from(cannot_flag)) {
        return LeechResult::zero(pool, resource);
    }
    let leech_pct = db.sum(ModType::Base, cfg, &[resource.leech_mod_name()]);
    calc_leech(pool, leech_pct, hit_damage, resource)
}

impl LeechResult {
    /// 偷取为 0 的空结果（pool = 0 或无偷取词条）。
    pub fn zero(pool: f64, resource: LeechResource) -> Self {
        Self {
            instance_total: 0.0,
            rate_cap_per_second: round(pool * resource.max_rate_pct() / 100.0),
            display_rate_per_second: 0.0,
        }
    }
}

// ─────────────────────────────────────────────────────────────────
// 返还 (Recoup)
// ─────────────────────────────────────────────────────────────────

/// Recoup 默认返还持续时间（秒）。
///
/// 出处：agent-docs/recovery-charges-buffs.md §2.3；
///       PoB2 `CalcPerform.lua`: 默认 8 秒。
pub const RECOUP_DURATION_DEFAULT: f64 = 8.0;

/// Recoup 4 秒旗标对应的持续时间。
///
/// 出处：agent-docs/recovery-charges-buffs.md §2.3；
///       PoB2 `CalcPerform.lua`: `4SecondRecoup` / `4SecondLifeRecoup` 旗标。
pub const RECOUP_DURATION_4S: f64 = 4.0;

/// Recoup 资源类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoupResource {
    Life,
    Mana,
    EnergyShield,
}

impl RecoupResource {
    /// 对应的 `XRecoup` ModName（BASE%，如 `LifeRecoup` 表示对承受伤害返还比例）。
    pub fn recoup_mod_name(self) -> ModName {
        match self {
            RecoupResource::Life => ModName::from("LifeRecoup"),
            RecoupResource::Mana => ModName::from("ManaRecoup"),
            RecoupResource::EnergyShield => ModName::from("EnergyShieldRecoup"),
        }
    }

    /// 全局 4 秒 Recoup 旗标名（`4SecondRecoup` 作用于全部资源）。
    pub fn four_sec_flag_global() -> &'static str {
        "4SecondRecoup"
    }

    /// 单资源 4 秒 Recoup 旗标名（`4SecondLifeRecoup` 等）。
    pub fn four_sec_flag(self) -> &'static str {
        match self {
            RecoupResource::Life => "4SecondLifeRecoup",
            RecoupResource::Mana => "4SecondManaRecoup",
            RecoupResource::EnergyShield => "4SecondEnergyShieldRecoup",
        }
    }
}

/// Recoup 计算输出。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RecoupResult {
    /// 返还的每秒速率（承受伤害量 × recoup_pct% / duration）。
    pub rate_per_second: f64,
    /// Recoup 持续时间（秒；8 或 4）。
    pub duration: f64,
}

/// 计算 Recoup 每秒返还速率（纯函数版本）。
///
/// Recoup = `damage_taken × recoup_pct%`，在 `duration` 秒内均匀返还；
/// 乘 `recovery_rate_mod`（`XRecoveryRateMod`）。
///
/// # 参数
/// - `damage_taken` — 单次承受的伤害量（已过减免；是到达资源池的部分）。
/// - `recoup_pct` — 返还词条百分比之和（如 `LifeRecoup` BASE 15 表示 15%）。
/// - `duration` — 返还持续时间（8 秒或 4 秒）。
/// - `recovery_rate_mod` — `XRecoveryRateMod`（fraction；1.0 = 无加成）。
///
/// 出处：agent-docs/recovery-charges-buffs.md §2.3；
///       PoB2 `src/Modules/CalcDefence.lua` Recoup 段。
pub fn calc_recoup(
    damage_taken: f64,
    recoup_pct: f64,
    duration: f64,
    recovery_rate_mod: f64,
) -> RecoupResult {
    if damage_taken <= 0.0 || recoup_pct <= 0.0 || duration <= 0.0 {
        return RecoupResult {
            rate_per_second: 0.0,
            duration: duration.max(RECOUP_DURATION_DEFAULT),
        };
    }
    let total = damage_taken * recoup_pct / 100.0;
    let rate_per_second = round(total / duration * recovery_rate_mod.max(0.0));
    RecoupResult {
        rate_per_second,
        duration,
    }
}

/// 从 ModDb 计算 Recoup 结果。
///
/// - 持续时间：先检查 `4SecondRecoup`（全局），再检查资源专属 `4SecondXRecoup` 旗标；
///   其中任一为真 → 4 秒，否则 8 秒。
/// - `recovery_rate_mod`：从 `XRecoveryRate` INC/MORE 计算（与再生共用乘区）。
///
/// 出处：agent-docs/recovery-charges-buffs.md §2.3；
///       PoB2 `src/Modules/CalcDefence.lua` Recoup 段；
///       PoB2 `src/Modules/CalcPerform.lua` `4SecondRecoup` 旗标处理。
pub fn calc_recoup_from_db(
    db: &ModDb,
    cfg: &CalcConfig,
    damage_taken: f64,
    resource: RecoupResource,
) -> RecoupResult {
    let recoup_pct = db.sum(ModType::Base, cfg, &[resource.recoup_mod_name()]);
    if recoup_pct <= 0.0 {
        return RecoupResult {
            rate_per_second: 0.0,
            duration: RECOUP_DURATION_DEFAULT,
        };
    }

    // 持续时间：全局 4s 旗标 或 单资源 4s 旗标
    let four_sec = db.flag(cfg, ModName::from(RecoupResource::four_sec_flag_global()))
        || db.flag(cfg, ModName::from(resource.four_sec_flag()));
    let duration = if four_sec {
        RECOUP_DURATION_4S
    } else {
        RECOUP_DURATION_DEFAULT
    };

    // RecoveryRateMod：此资源的 XRecoveryRate inc/more 合并为乘数
    let recovery_rate_name = match resource {
        RecoupResource::Life => ModName::from("LifeRecoveryRate"),
        RecoupResource::Mana => ModName::from("ManaRecoveryRate"),
        RecoupResource::EnergyShield => ModName::from("EnergyShieldRecoveryRate"),
    };
    let rate_inc = db.sum(ModType::Inc, cfg, std::slice::from_ref(&recovery_rate_name));
    let rate_more = db.more(cfg, &[recovery_rate_name]);
    let recovery_rate_mod = (1.0 + rate_inc / 100.0) * rate_more;

    calc_recoup(damage_taken, recoup_pct, duration, recovery_rate_mod)
}
