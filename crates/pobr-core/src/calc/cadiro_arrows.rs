//! One explicitly selected Cadiro's Gambit arrow, never a probability average.
//!
//! Game stat `each_arrow_fired_gains_random_perdandus_prefix` (Mods:
//! `UniquePerandusArrows1`); KeywordPopups `UniqueArrowModifier{Chain,Split,
//! PierceReturn,Crit,Rarity,Stun}` supply the effects and fixed values.
//! https://poe2db.tw/us/Cadiros_Gambit links all six game keyword texts.
use pobr_data::prelude::*;

use crate::{CalcConfig, ModValue};

use super::Env;

/// Native multiplier and the config catalog's single selection channel.
/// 0/unset = unspecified, 1..6 = Crescendo, Splinter, Reversing, Diamond,
/// Covetous, Blunt. Invalid input is neutral rather than rounded to an arrow.
pub const SELECTION: &str = "CadiroArrow";

fn selection(cfg: &CalcConfig) -> Option<u8> {
    let n = cfg.multiplier_if_set(SELECTION)?;
    (n.is_finite() && (1.0..=6.0).contains(&n) && n.fract() == 0.0).then_some(n as u8)
}

pub(super) fn apply(env: &mut Env) {
    // Read even when absent/off so config discovery can offer the control.
    let selected = selection(&env.cfg);
    let grant = env.player.mod_db.named("CadiroArrows").find(|m| {
        m.mod_type == ModType::Flag && m.value.as_bool() == Some(true) && m.matches(&env.cfg)
    });
    let (Some(selected), Some(grant)) = (selected, grant) else {
        return;
    };
    let cfg = &env.cfg;
    let excluded =
        |name| SkillTypes::from_pob2_name(name).is_some_and(|kind| cfg.skill_types.contains(kind));
    if !cfg.mode_combat
        || !(ModFlags::ATTACK | ModFlags::BOW).is_subset_of(cfg.flags)
        || !cfg.keyword_flags.intersects(KeywordFlags::ARROW)
        || env.player.minion.is_some()
        || [
            "Minion",
            "NonWeaponAttack",
            "UsedByProxy",
            "OtherThingUsesSkill",
            "UsedByTotem",
            "SummonsTotem",
            "Trapped",
            "RemoteMined",
        ]
        .into_iter()
        .any(excluded)
    {
        return;
    }
    let grant = grant.clone();
    env.player.mod_db.flag(&env.cfg, "CadiroArrows");
    let make = |name: &str, kind, value| {
        let mut m = grant.clone();
        m.name = ModName::from(name);
        m.mod_type = kind;
        m.value = value;
        m.source = Some("CadiroArrow".into());
        m
    };
    let number = |name, kind, value| make(name, kind, ModValue::Number(value));
    let flag = |name| make(name, ModType::Flag, ModValue::Bool(true));
    // Repeated preparation does not duplicate the arrow's fixed effects.
    if env
        .player
        .mod_db
        .iter_mods()
        .any(|m| m.source.as_deref() == Some("CadiroArrow"))
    {
        return;
    }
    let mods = match selected {
        1 => vec![number("ChainCountMax", ModType::Base, 6.0)],
        2 => vec![number("SplitCount", ModType::Base, 6.0)],
        3 => vec![
            flag("PierceAllTargets"),
            number("CadiroArrowReturn", ModType::Base, 1.0),
        ],
        4 => vec![
            flag("AlwaysCrit"),
            number("CriticalStrikeMultiplier", ModType::Inc, 60.0),
        ],
        5 => vec![number("CadiroArrowKillRarity", ModType::Base, 600.0)],
        6 => vec![number("EnemyHeavyStunBuildup", ModType::Inc, 600.0)],
        _ => unreachable!(),
    };
    env.player.mod_db.add_list(mods);
    if selected == 1 {
        // Reuse the existing '# of times Skill has Chained' input. It is a
        // hit snapshot, not a hit count multiplier. Native callers may use
        // Chain directly. No configured count means the initial hit.
        let chains = env.cfg.multiplier_if_set("Chain").unwrap_or_else(|| {
            env.player
                .mod_db
                .sum(ModType::Base, &env.cfg, &[ModName::from("ChainCount")])
        });
        let available = super::skill_mechanics::projectile_behavior_counts(
            &env.player.mod_db,
            &env.enemy.mod_db,
            &env.cfg,
        )
        .chain;
        let chains = if chains.is_finite() {
            chains.floor().clamp(0.0, available)
        } else {
            0.0
        };
        if chains > 0.0 {
            env.player
                .mod_db
                .add_mod(number("Damage", ModType::Inc, 60.0 * chains));
        }
    }
}

/// Capability/kill-condition metadata only. Return geometry, hit timing,
/// loot outcomes and the probability of selecting an arrow are not simulated.
pub(super) fn fill(env: &mut Env) {
    let sum = |name| {
        env.player
            .mod_db
            .sum(ModType::Base, &env.cfg, &[ModName::from(name)])
    };
    env.player.output.cadiro_arrow_return = sum("CadiroArrowReturn");
    env.player.output.cadiro_arrow_kill_rarity = sum("CadiroArrowKillRarity");
}
