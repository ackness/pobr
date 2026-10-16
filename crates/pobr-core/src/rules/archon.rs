//! Shared Archon properties, from the game's Archon keyword and passive text.
//! https://poe2db.tw/us/Archon_Buff: 10 seconds active, then 20 seconds recovery.
//! The configured state is a snapshot, not an estimate of event-driven uptime.

use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb, ModValue, Modifier};

pub const CONDITION: &str = "AffectedByArchon";

pub struct Timing {
    pub duration: f64,
    pub recovery_period: f64,
    pub duration_factor: f64,
    pub recovery_rate: f64,
    pub no_recovery: bool,
}

pub fn timing(db: &ModDb, cfg: &CalcConfig) -> Timing {
    let duration_names = [StatId::new("ArchonDuration")];
    let duration_factor = ((1.0 + db.sum(ModType::Inc, cfg, &duration_names) / 100.0)
        * db.more(cfg, &duration_names))
    .max(0.0);
    let recovery_rate =
        1.0 + db.sum(ModType::Inc, cfg, &[StatId::new("ArchonRecoveryRate")]) / 100.0;
    let no_recovery = db.flag(cfg, "ArchonNoRecovery");
    Timing {
        duration: 10.0 * duration_factor,
        recovery_period: if no_recovery {
            0.0
        } else {
            20.0 / recovery_rate.max(0.0)
        },
        duration_factor,
        recovery_rate: recovery_rate.max(0.0),
        no_recovery,
    }
}

/// "Archon Buffs also grant" becomes part of the buff; a bonus merely
/// conditional on having an Archon (such as recharge start) is not scaled.
pub fn expand(db: &ModDb, cfg: &CalcConfig, combat: bool) -> Vec<Modifier> {
    if !combat
        || db.flag(cfg, "ArchonBonusesExpanded")
        || !(cfg.condition(CONDITION) || db.flag(cfg, "Condition:AffectedByArchon"))
    {
        return Vec::new();
    }
    let names = [StatId::new("ArchonEffect"), StatId::new("BuffEffectOnSelf")];
    let effect =
        ((1.0 + db.sum(ModType::Inc, cfg, &names) / 100.0) * db.more(cfg, &names)).max(0.0);
    let mut mods = vec![Modifier::flag("ArchonBonusesExpanded")];
    for (input, outputs, mod_type) in [
        ("ArchonMovementSpeed", &["MovementSpeed"][..], ModType::Inc),
        (
            "ArchonElementalResistance",
            &["FireResist", "ColdResist", "LightningResist"][..],
            ModType::Base,
        ),
    ] {
        // Preserve the originating rule and source on each expanded line so
        // attribution and exporter-absent filtering keep their provenance.
        for modifier in db.named(input) {
            if !modifier.matches(cfg) {
                continue;
            }
            let Some(value) = modifier.effective_number(cfg) else {
                continue;
            };
            for output in outputs {
                let mut expanded = modifier.clone();
                expanded.name = StatId::new(*output);
                expanded.mod_type = mod_type;
                expanded.value = ModValue::Number(value * effect);
                expanded.tags.clear();
                mods.push(expanded);
            }
        }
    }
    mods
}
