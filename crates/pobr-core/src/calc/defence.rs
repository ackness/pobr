use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb};

use super::{Actor, round};

#[derive(Debug, Clone, PartialEq)]
pub struct DefenceOutput {
    pub armour: f64,
    pub evasion: f64,
    pub energy_shield: f64,
    pub chance_to_be_hit: f64,
}

pub fn calc_defence(actor: &mut Actor, cfg: &CalcConfig, enemy_accuracy: f64) -> DefenceOutput {
    let armour = scaled_defence_stat(&actor.mod_db, cfg, actor.base.armour, "Armour");
    let evasion = scaled_defence_stat(&actor.mod_db, cfg, actor.base.evasion, "Evasion");
    let energy_shield =
        scaled_defence_stat(&actor.mod_db, cfg, actor.base.energy_shield, "EnergyShield");
    // 防御侧：怪物命中玩家，用 monster_hit_chance（agent-docs/accuracy-and-enemy.md §二）
    let chance_to_be_hit = monster_hit_chance(evasion, enemy_accuracy);

    actor.output.armour = armour;
    actor.output.evasion = evasion;
    actor.output.energy_shield = energy_shield;
    actor.output.chance_to_be_hit = chance_to_be_hit;

    actor.breakdown.push("armour", armour);
    actor.breakdown.push("evasion", evasion);
    actor.breakdown.push("energy_shield", energy_shield);
    actor.breakdown.push("chance_to_be_hit", chance_to_be_hit);

    DefenceOutput {
        armour,
        evasion,
        energy_shield,
        chance_to_be_hit,
    }
}

/// 玩家攻击命中怪物的几率（进攻侧，`calcs.hitChance`）。
///
/// PoE2 公式（CalcDefence.lua `calcs.hitChance`，agent-docs/accuracy-and-enemy.md §二）：
/// `rawChance = accuracy * 1.25 / (accuracy + evasion * 0.3)`，clamp 到 `[0.05, 1.0]`。
///
/// 边界情况：
/// - accuracy=0, evasion=0（未设定/裸面板）→ 1.0（满命中）
/// - accuracy <= 0, evasion > 0 → 0.05（下限）
/// - accuracy > 0, evasion <= 0 → 1.0（满命中）
///
/// **注意**：法术必中，调用方在 `cfg.is_spell()` 为真时直接用 1.0，不调用此函数
/// （Bug#4 spell-must-hit，agent-docs/accuracy-and-enemy.md §三）。
pub fn hit_chance(evasion: f64, accuracy: f64) -> f64 {
    if accuracy <= 0.0 && evasion <= 0.0 {
        // 两者均为 0 → 无闪避目标 → 满命中
        return 1.0;
    }

    if accuracy <= 0.0 {
        // 精准值为 0（或负），有闪避 → 命中率下限 5%
        return 0.05;
    }

    if evasion <= 0.0 {
        // 怪物无闪避 → 满命中
        return 1.0;
    }

    // PoE2 进攻侧命中公式（agent-docs/accuracy-and-enemy.md §二）：
    //   rawChance (fraction) = accuracy * 1.25 / (accuracy + evasion * 0.3)
    let raw = accuracy * 1.25 / (accuracy + evasion * 0.3);
    let chance = raw.clamp(0.05, 1.0);
    if chance > 0.9999 { 1.0 } else { round(chance) }
}

/// 怪物攻击命中玩家的几率（防御侧，`calcs.monsterHitChance`）。
///
/// PoE2 防御侧公式（CalcDefence.lua，agent-docs/accuracy-and-enemy.md §二.1 注）：
/// `raw = 1 - 0.95 * evasion / (evasion + 4 * accuracy)`，clamp 到 `[0.05, 1.0]`。
/// 与进攻侧公式**不对称**，不可混用。
pub fn monster_hit_chance(player_evasion: f64, enemy_accuracy: f64) -> f64 {
    if player_evasion <= 0.0 {
        return 1.0;
    }
    if enemy_accuracy <= 0.0 {
        // 敌人精准为 0 → 给防守方最大闪避，返回下限 5%
        return 0.05;
    }
    let raw = 1.0 - 0.95 * player_evasion / (player_evasion + 4.0 * enemy_accuracy);
    let chance = raw.clamp(0.05, 1.0);
    if chance > 0.9999 { 1.0 } else { round(chance) }
}

pub fn armour_reduction(armour: f64, raw_hit: f64) -> f64 {
    if armour <= 0.0 || raw_hit <= 0.0 {
        return 0.0;
    }

    round(armour / (armour + 10.0 * raw_hit))
}

fn scaled_defence_stat(db: &ModDb, cfg: &CalcConfig, base: f64, name: &str) -> f64 {
    let names = [ModName::from(name)];
    let base_value = base + db.sum(ModType::Base, cfg, &names);
    let inc = db.sum(ModType::Inc, cfg, &names);
    let more = db.more(cfg, &names);
    round(base_value * (1.0 + inc / 100.0) * more)
}
