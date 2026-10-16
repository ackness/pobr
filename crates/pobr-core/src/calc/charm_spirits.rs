//! Rite of Passage's player possession buffs, from the game Mods table:
//! SpiritOfThe{Bear,Boar,Cat,Monkey,Owl,Ox,Snake,Stag,Wolf}PossessedPlayer
//! and SpiritPossessed{Dex,Int,Str}Player (base/mods.json).
//!
//! Possession grants a separate buff: charm effect/quality do not multiply
//! its fixed stats. Shared attribute-family bonuses apply once. The charm's
//! active state and slot limit are resolved before its grant reaches here.
//! Periodic spirit attacks, maim and on-hit armour break are not simulated.
use pobr_data::prelude::*;

use crate::{ModValue, Modifier};

use super::Env;

/// Numeric modifiers granted by a possession buff: (engine name, type, value).
pub type SpiritStats = &'static [(&'static str, ModType, f64)];

/// (Spirit name, index in `SPIRIT_FAMILY_STATS`, numeric modifiers, flags).
pub type SpiritDefinition = (&'static str, usize, SpiritStats, &'static [&'static str]);

/// Fixed player buffs, exposed for comparison with the active game's Mods table.
pub const SPIRIT_DEFINITIONS: &[SpiritDefinition] = &[
    (
        "Bear",
        0,
        &[
            ("MaximumLife", ModType::Inc, 20.0),
            ("EnemyHeavyStunBuildup", ModType::Inc, 60.0),
            ("StunThreshold", ModType::Inc, 60.0),
        ],
        &[],
    ),
    (
        "Boar",
        0,
        &[
            ("DamageGainAsFire", ModType::Base, 20.0),
            ("BleedChance", ModType::Base, 100.0),
        ],
        &[],
    ),
    (
        "Ox",
        0,
        &[
            ("SlowEffectOnSelf", ModType::Inc, -50.0),
            ("AilmentThreshold", ModType::Inc, 60.0),
            ("Armour", ModType::Inc, 60.0),
        ],
        &[],
    ),
    (
        "Cat",
        1,
        &[
            ("Evasion", ModType::Inc, 60.0),
            ("CriticalStrikeChance", ModType::Inc, 100.0),
        ],
        &[],
    ),
    (
        "Stag",
        1,
        &[
            ("DamageGainAsLightning", ModType::Base, 20.0),
            ("FireResistance", ModType::Base, 30.0),
            ("ColdResistance", ModType::Base, 30.0),
            ("LightningResistance", ModType::Base, 30.0),
        ],
        &[],
    ),
    ("Wolf", 1, &[], &[]),
    (
        "Owl",
        2,
        &[
            ("DamageGainAsCold", ModType::Base, 20.0),
            ("EnergyShield", ModType::Inc, 60.0),
        ],
        &[],
    ),
    (
        "Primate",
        2,
        &[("EnemyFreezeBuildup", ModType::Inc, 60.0)],
        &[
            "PhysicalCanChill",
            "FireCanChill",
            "LightningCanChill",
            "ChaosCanChill",
        ],
    ),
    (
        "Serpent",
        2,
        &[("PoisonChance", ModType::Base, 100.0)],
        &["FireCanPoison", "ColdCanPoison", "LightningCanPoison"],
    ),
];

/// Shared Strength, Dexterity and Intelligence possession bonuses, respectively.
pub const SPIRIT_FAMILY_STATS: [SpiritStats; 3] = [
    &[("DamageTaken", ModType::Inc, -20.0)],
    &[
        ("SkillSpeed", ModType::Inc, 30.0),
        ("MovementSpeed", ModType::Inc, 15.0),
    ],
    &[("Damage", ModType::Inc, 80.0)],
];

pub(super) fn apply(env: &mut Env) {
    if !env.cfg.mode_combat
        || env
            .player
            .mod_db
            .iter_mods()
            .any(|modifier| modifier.source.as_deref() == Some("CharmSpirit"))
    {
        return;
    }
    let mut output = Vec::new();
    let mut families = [false; 3];
    for &(spirit, family, stats, flags) in SPIRIT_DEFINITIONS {
        let name = format!("SpiritPossession:{spirit}");
        let Some(grant) = env
            .player
            .mod_db
            .named(&name)
            .find(|modifier| modifier.matches(&env.cfg))
        else {
            continue;
        };
        let numeric = |name: &str, kind, value| {
            let mut modifier = grant.clone();
            modifier.name = ModName::from(name);
            modifier.mod_type = kind;
            modifier.value = ModValue::Number(value);
            modifier.source = Some("CharmSpirit".into());
            modifier
        };
        let flag = |name: &str| {
            let mut modifier: Modifier = grant.clone();
            modifier.name = ModName::from(name);
            modifier.source = Some("CharmSpirit".into());
            modifier
        };
        output.extend(
            stats
                .iter()
                .map(|(name, kind, value)| numeric(name, *kind, *value)),
        );
        output.extend(flags.iter().map(|name| flag(name)));
        if !families[family] {
            families[family] = true;
            let shared = SPIRIT_FAMILY_STATS[family];
            output.extend(
                shared
                    .iter()
                    .map(|(name, kind, value)| numeric(name, *kind, *value)),
            );
        }
    }
    env.player.mod_db.add_list(output);
}
