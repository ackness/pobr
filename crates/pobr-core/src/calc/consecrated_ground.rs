//! Explicit Consecrated Ground snapshot. KeywordPopups ConsecratedGround:
//! 5% maximum Life regenerated per second; 50% reduced Curse effect on you.
//! https://poe2db.tw/us/Keywords (also verified against the Traditional Chinese
//! game keyword). Creation, ownership and uptime are separate from standing on it.
use pobr_data::prelude::*;

use crate::{CalcConfig, ModDb, ModTag, Modifier};

const SOURCE: &str = "Consecrated Ground";

pub(super) fn is_active(db: &ModDb, cfg: &CalcConfig) -> bool {
    // Record even absent/off state so config discovery can offer this toggle.
    let requested = cfg.condition("OnConsecratedGround");
    if !cfg.mode_combat {
        return false;
    }
    if cfg.conditions.contains_key("OnConsecratedGround") {
        requested
    } else {
        // Config's flag carries the Combat tag. Native callers can supply the
        // same flag without depending on the build's player-condition bridge.
        db.flag(
            &cfg.clone().with_condition("Combat", true),
            "Condition:OnConsecratedGround",
        )
    }
}

/// Applies once to this actor. Callers give minions their own ground condition.
/// Own-ground effect modifiers are deliberately not read: this state can refer
/// to an ally's ground and carries no creator/effect snapshot.
pub(super) fn apply(db: &mut ModDb, cfg: &mut CalcConfig) {
    let on_ground = is_active(db, cfg);
    cfg.conditions
        .insert("OnConsecratedGround".into(), on_ground);
    if !on_ground
        || db
            .named("LifeRegenPercent")
            .any(|modifier| modifier.source.as_deref() == Some(SOURCE))
    {
        return;
    }
    let origin = ModifierSource::new(SourceId::new(SourceKind::Buff, "buff.ConsecratedGround"))
        .with_raw_text("Consecrated Ground");
    for (name, kind, value) in [
        ("LifeRegenPercent", ModType::Base, 5.0),
        ("CurseEffectOnSelf", ModType::Inc, -50.0),
    ] {
        db.add_mod(
            Modifier::number(name, kind, value)
                .with_tag(ModTag::condition("OnConsecratedGround", false))
                .with_source(SOURCE)
                .with_origin(origin.clone()),
        );
    }
}
