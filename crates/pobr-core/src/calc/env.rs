use pobr_data::prelude::*;

use crate::CalcConfig;

use super::{Actor, ActorBaseStats};

#[derive(Debug, Clone)]
pub struct Env {
    pub player: Actor,
    pub enemy: Actor,
    pub cfg: CalcConfig,
}

impl Env {
    pub fn new(player: Actor) -> Self {
        Self {
            player,
            enemy: Actor::new(1, ActorBaseStats::default()),
            cfg: CalcConfig::attack().with_damage_type(DamageType::Physical),
        }
    }

    pub fn with_config(mut self, cfg: CalcConfig) -> Self {
        self.cfg = cfg;
        self
    }
}
