use crate::ModDb;

use super::{BreakdownTable, OutputTable};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActorBaseStats {
    pub life: f64,
    pub mana: f64,
    pub armour: f64,
    pub evasion: f64,
    pub energy_shield: f64,
    pub accuracy: f64,
    pub fire_resistance: f64,
    pub cold_resistance: f64,
    pub lightning_resistance: f64,
    pub hit_min: f64,
    pub hit_max: f64,
    pub action_rate: f64,
}

impl Default for ActorBaseStats {
    fn default() -> Self {
        Self {
            life: 0.0,
            mana: 0.0,
            armour: 0.0,
            evasion: 0.0,
            energy_shield: 0.0,
            accuracy: 0.0,
            fire_resistance: 0.0,
            cold_resistance: 0.0,
            lightning_resistance: 0.0,
            hit_min: 0.0,
            hit_max: 0.0,
            action_rate: 0.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Actor {
    pub mod_db: ModDb,
    pub level: u8,
    pub base: ActorBaseStats,
    pub output: OutputTable,
    pub breakdown: BreakdownTable,
    /// Whether the summoning skill has `SkillType.Companion`.
    pub is_companion: bool,
    /// The summoning skill's `MinionsAreUndamagable` flag. These minions
    /// cannot contribute to the life pool provided by Loyalty.
    pub is_undamageable: bool,
    /// A minion's catalog id and the skill that summoned it; `None` for the
    /// player and the enemy.
    pub minion: Option<MinionIdentity>,
}

/// Which minion an actor is.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MinionIdentity {
    /// Enabled-group ordinal and gem index (both 0-based) of this summoner.
    pub summoner: Option<(usize, usize)>,
    /// The minion's catalog id (`MinionDef::id`).
    pub minion_id: String,
    /// The id of the skill that summoned it; empty when no skill did.
    pub summoned_by: String,
    /// The skill the minion uses.
    pub skill: super::MinionSkill,
}

impl Actor {
    pub fn new(level: u8, base: ActorBaseStats) -> Self {
        Self {
            mod_db: ModDb::new(),
            level,
            base,
            output: OutputTable::default(),
            breakdown: BreakdownTable::default(),
            is_companion: false,
            is_undamageable: false,
            minion: None,
        }
    }
}
