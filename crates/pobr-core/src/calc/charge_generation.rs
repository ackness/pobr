//! Passive generation events, separate from held charges and consumption.
//!
//! BuffTemplates.AscendancyGemlingChargeGenerator supplies 5000 ms. The game
//! keyword says socketed gem requirements bias the charge type, but gives no
//! probability formula. Report only the known total event rate; do not invent
//! per-type probabilities, charge uptime, or on-gain chain effects.
use pobr_data::prelude::*;

use super::{Env, session::detail::DerivedDetail, utility::query_config};

pub const STATS: &[&str] = &[
    "DynamismChargeInterval",
    "DynamismChargeEventRate",
    "DynamismAllyChargeEventRate",
];

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChargeGeneration {
    pub interval: f64,
    pub to_allies: bool,
}

impl ChargeGeneration {
    pub fn value(&self, id: &str) -> Option<f64> {
        Some(match id {
            "DynamismChargeInterval" => self.interval,
            "DynamismChargeEventRate" | "DynamismAllyChargeEventRate" => {
                let ally_view = id == "DynamismAllyChargeEventRate";
                if self.interval > 0.0 && ally_view == self.to_allies {
                    1.0 / self.interval
                } else {
                    0.0
                }
            }
            _ => return None,
        })
    }

    pub fn detail(&self, id: &str) -> Option<DerivedDetail> {
        self.value(id)?;
        let interval = id == "DynamismChargeInterval";
        Some(DerivedDetail {
            formula: if interval {
                "DynamismBaseInterval"
            } else if self.interval > 0.0 {
                "DynamismRecipientEnabled / DynamismBaseInterval"
            } else {
                "0 * DynamismBaseInterval"
            }
            .into(),
            inputs: vec![
                ("DynamismBaseInterval".into(), self.interval),
                (
                    "DynamismRecipientEnabled".into(),
                    f64::from((id == "DynamismAllyChargeEventRate") == self.to_allies),
                ),
            ],
            context: vec![],
        })
    }
}

pub fn calculate(env: &mut Env) {
    let cfg = query_config(&env.cfg);
    // This is one granted buff, even when two sources grant it.
    let interval = env
        .player
        .mod_db
        .max_of(
            ModType::Base,
            &cfg,
            &[ModName::from("ThaumaturgicalDynamismChargeInterval")],
        )
        .max(0.0);
    env.player.output.charge_generation = ChargeGeneration {
        interval,
        to_allies: env.player.mod_db.flag(&cfg, "ChargesGainedByAllies"),
    };
}
