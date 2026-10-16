//! Incoming debuff magnitudes supplied by player configuration.
//!
//! `shocked_effect_on_self_+%` and `chill_effectiveness_on_self_+%`
//! modify the magnitude on the player, not outgoing ailments or duration.
//! No incoming hit or ailment uptime is inferred from the configured state.

use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb, Modifier};

use super::Env;

fn effect(db: &ModDb, cfg: &CalcConfig, name: &str) -> f64 {
    let names = [ModName::from(name)];
    ((1.0 + db.sum(ModType::Inc, cfg, &names) / 100.0) * db.more(cfg, &names)).max(0.0)
}

/// Runs once after item, flask and buff modifiers have reached the player.
pub(super) fn apply(env: &mut Env) {
    if !env.cfg.mode_combat {
        return;
    }
    let db = &mut env.player.mod_db;
    let cfg = &env.cfg;

    // Config already supplies Shock's damage-taken magnitude. Scale that
    // contribution only, retaining its source, conditions and attribution.
    let shocks: Vec<_> = db
        .named("DamageTaken")
        .filter(|modifier| {
            modifier.source.as_deref() == Some("Shock")
                && modifier.mod_type == ModType::Inc
                && modifier.matches(cfg)
        })
        .cloned()
        .collect();
    if !shocks.is_empty() {
        let scale = if db.flag(cfg, "ShockImmune") {
            0.0
        } else {
            effect(db, cfg, "SelfShockEffect")
        };
        for mut modifier in shocks {
            if let Some(value) = modifier.value.as_number() {
                modifier.value = crate::ModValue::Number(value * scale);
                db.replace_mod(modifier);
            }
        }
    }

    // The supplied ChillVal is a magnitude, not an enemy hit from which to
    // derive one. Without an explicit magnitude, leave the slow unresolved.
    if (cfg.condition("Chilled") || db.flag(cfg, "Condition:Chilled"))
        && !db.flag(cfg, "ChillImmune")
        && let Some(magnitude) = db.override_(cfg, "ChillVal")
    {
        let magnitude = magnitude.max(0.0) * effect(db, cfg, "SelfChillEffect");
        let sign = if db.flag(cfg, "SelfChillEffectIsReversed") {
            1.0
        } else {
            -1.0
        };
        db.add_mod(
            Modifier::number("ActionSpeed", ModType::Inc, sign * magnitude)
                .with_source("Chill")
                .with_origin(
                    ModifierSource::new(SourceId::new(SourceKind::Buff, "debuff.Chill.player"))
                        .with_raw_text(format!("Chill: {magnitude}% action speed")),
                ),
        );
    }
}
