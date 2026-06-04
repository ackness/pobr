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
    let chance_to_be_hit = hit_chance(evasion, enemy_accuracy);

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

pub fn hit_chance(evasion: f64, accuracy: f64) -> f64 {
    if evasion <= 0.0 && accuracy <= 0.0 {
        return 1.0;
    }

    if accuracy <= 0.0 {
        return 0.05;
    }

    if evasion <= 0.0 {
        return 1.0;
    }

    let chance = accuracy / (accuracy + (evasion / 4.0).powf(0.8));
    let chance = chance.clamp(0.05, 1.0);
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
