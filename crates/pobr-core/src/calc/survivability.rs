//! 生存性辅助计算（reservation / regen / capped chance / suppression）。
//!
//! 资料：`agent-docs/active-defences.md`、`agent-docs/block.md`、
//! `agent-docs/recovery-charges-buffs.md`（PoE2 0.5.0）。
//!
//! - **Reservation**：光环 / 守护按 `Σ flat + 池子 * (Σ % / 100)` 预留，结果钳到 [0, pool]。
//! - **Regen**：`base_flat + pool * (Σ %regen / 100)`，再吃 inc/more 恢复速率。
//! - **Capped chance**：几率类（block / suppression）求和后钳到 [0, cap]。

use pobr_data::constants::BLOCK_CHANCE_CAP;

use super::round;

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

/// 计算每秒恢复（regen）。
///
/// `base_flat` 是固定每秒恢复之和，`percent` 是按池子百分比恢复之和，
/// `inc` / `more` 为恢复速率增益（% 加法 + more 连乘）。
pub fn regen(pool: f64, base_flat: f64, percent: f64, inc: f64, more: f64) -> f64 {
    let base = base_flat + pool * (percent / 100.0);
    round(base * (1.0 + inc / 100.0) * more)
}

/// 几率类聚合：求和后钳到 `[0, cap]`（cap 通常 75% 或 100%）。
pub fn capped_chance(percent_sum: f64, cap: f64) -> f64 {
    round(percent_sum.clamp(0.0, cap))
}

/// 格挡几率（PoE2 硬上限 90%，`data.misc.BlockChanceCap = 90`）。
///
/// **Bug#11 修正（block-chance-cap-wrong）**：PoE2 格挡上限为 90%，非 PoE1 的 75%。
/// 出处：agent-docs/block.md §被动格挡、PoB2 DeepWiki `BlockChanceCap = 90`。
pub fn block_chance(percent_sum: f64) -> f64 {
    capped_chance(percent_sum, BLOCK_CHANCE_CAP)
}

/// 法术压制（spell suppression）几率（PoE2 已移除此机制，此函数保留为 inert 兼容桩）。
///
/// PoE2 中法术压制已从常规防御移除（agent-docs/block.md §法术压制；active-defences.md §六）。
/// 保留此函数仅避免调用方编译失败（值始终为 0 或有效但无意义）；完整移除留 Wave2。
/// 上限保持 100% 与旧行为兼容，但正常 PoE2 build 无词条来源，结果始终 0。
pub fn suppression_chance(percent_sum: f64) -> f64 {
    // PoE2 法术压制已移除：常规 build 无词条来源，始终 0；
    // 保留函数签名避免 ripple，Wave2 再完整清理。
    capped_chance(percent_sum, 100.0)
}
