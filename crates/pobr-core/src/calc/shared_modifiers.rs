//! Modifiers to something else that the game says also apply to the player.
//!
//! "Increases and Reductions to Companion Damage also apply to you" (the stat
//! `companion_damage_increases_and_reductions_also_affects_you`): every
//! increase or reduction to Companion damage (`companion_damage_+%`, written
//! "Companions deal N% increased Damage") counts as the same increase or
//! reduction to the player's damage. A modifier to all Minion damage is a
//! different stat with its own transfer below.

use pobr_data::prelude::*;

use crate::{ModTag, Modifier};

use super::Env;

/// Copies each Companion damage increase or reduction to the player, for a
/// character with the transfer. The copy keeps the original's source, so the
/// player's gain is attributed to the line that grants it.
pub fn apply_companion_damage_to_player(env: &mut Env) {
    let db = &env.player.mod_db;
    if !db.flag(&env.cfg, "CompanionDamageAppliesToPlayer") {
        return;
    }
    let Some(companion) = SkillTypes::from_pob2_name("CreatesCompanion") else {
        return;
    };
    // A Companion modifier is scoped to Companion skills. Its other
    // conditions are the player's, so evaluate them as a Companion skill would.
    let mut companion_cfg = env.cfg.clone();
    companion_cfg.skill_types |= companion;

    let mut copies: Vec<Modifier> = Vec::new();
    for outer in db.named("MinionModifier") {
        let for_companions = outer
            .tags
            .iter()
            .any(|tag| matches!(tag, ModTag::SkillTypes(types) if types.intersects(companion)));
        if !for_companions || outer.mod_type != ModType::List || !outer.matches(&companion_cfg) {
            continue;
        }
        for inner in outer.value.as_nested_mods().into_iter().flatten() {
            if inner.name.as_str() != "Damage" || inner.mod_type != ModType::Inc {
                continue;
            }
            let mut copy = inner.clone();
            if copy.source.is_none() {
                copy.source = outer.source.clone();
            }
            if copy.origin.is_none() {
                copy.origin = outer.origin.clone();
            }
            copies.push(copy);
        }
    }
    env.player.mod_db.add_list(copies);
}

/// Chober Chaber, Trenchtimbre and Crown of Eyes share increases and
/// reductions only (`minion_damage_increases_and_reductions_also_affects_you`,
/// `additive_modifiers_to_minion_attack_speed_also_affect_you`, and
/// `additive_spell_damage_modifiers_apply_to_attack_damage`). More/less,
/// flat damage, and modifiers to a particular kind of minion do not transfer.
pub fn apply_shared_increases(env: &mut Env) {
    let db = &env.player.mod_db;
    let damage = db.flag(&env.cfg, "MinionDamageAppliesToPlayer");
    let speed = db.flag(&env.cfg, "MinionAttackSpeedAppliesToPlayer");
    let mut copies = Vec::new();
    if damage || speed {
        for outer in db.named("MinionModifier") {
            if outer.mod_type != ModType::List
                || outer.tags.iter().any(|tag| {
                    matches!(
                        tag,
                        ModTag::SkillTypes(_) | ModTag::SkillTypesNeg(_) | ModTag::SkillName { .. }
                    )
                })
            {
                continue;
            }
            for inner in outer.value.as_nested_mods().into_iter().flatten() {
                if inner.mod_type != ModType::Inc
                    || !match inner.name.as_str() {
                        "Damage" => damage,
                        "AttackSpeed" => speed,
                        _ => false,
                    }
                {
                    continue;
                }
                let mut copy = inner.clone();
                copy.tags.extend(outer.tags.iter().cloned());
                copy.flags |= outer.flags;
                if copy.source.is_none() {
                    copy.source = outer.source.clone();
                }
                if copy.origin.is_none() {
                    copy.origin = outer.origin.clone();
                }
                copies.push(copy);
            }
        }
    }
    if db.flag(&env.cfg, "SpellDamageAppliesToAttacks") {
        for modifier in db.named("SpellDamage") {
            if modifier.mod_type != ModType::Inc {
                continue;
            }
            let mut copy = modifier.clone();
            copy.name = ModName::from("Damage");
            copy.flags = copy.flags.without(ModFlags::SPELL) | ModFlags::ATTACK;
            copies.push(copy);
        }
    }
    env.player.mod_db.add_list(copies);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ModValue;
    use crate::calc::{Actor, ActorBaseStats};

    fn companion_types() -> SkillTypes {
        SkillTypes::from_pob2_name("CreatesCompanion").unwrap()
    }

    /// "Companions deal `value`% increased `name`".
    fn companions_deal(name: &str, value: f64) -> Modifier {
        Modifier::new(
            "MinionModifier",
            ModType::List,
            ModValue::NestedMods(vec![Modifier::number(name, ModType::Inc, value)]),
        )
        .with_tag(ModTag::SkillTypes(companion_types()))
        .with_source("Companions deal increased Damage")
    }

    fn player_increased_damage(mods: Vec<Modifier>) -> f64 {
        let mut player = Actor::new(1, ActorBaseStats::default());
        player.mod_db.add_list(mods);
        let mut env = Env::new(player);
        apply_companion_damage_to_player(&mut env);
        env.player
            .mod_db
            .sum(ModType::Inc, &env.cfg, &[ModName::from("Damage")])
    }

    fn transfer() -> Modifier {
        Modifier::flag("CompanionDamageAppliesToPlayer")
    }

    #[test]
    fn general_minion_transfer_keeps_conditions_and_excludes_companion_only_damage() {
        let mut player = Actor::new(1, ActorBaseStats::default());
        player.mod_db.add_list([
            Modifier::flag("MinionDamageAppliesToPlayer"),
            Modifier::new(
                "MinionModifier",
                ModType::List,
                ModValue::NestedMods(vec![Modifier::number("Damage", ModType::Inc, 30.0)]),
            )
            .with_tag(ModTag::condition("FullLife", false)),
            companions_deal("Damage", 40.0),
        ]);
        let mut env = Env::new(player);
        apply_shared_increases(&mut env);
        let names = [ModName::from("Damage")];
        assert_eq!(env.player.mod_db.sum(ModType::Inc, &env.cfg, &names), 0.0);
        env.cfg.conditions.insert("FullLife".into(), true);
        assert_eq!(env.player.mod_db.sum(ModType::Inc, &env.cfg, &names), 30.0);
    }

    #[test]
    fn companion_damage_increases_and_reductions_reach_the_player() {
        let total = player_increased_damage(vec![
            transfer(),
            companions_deal("Damage", 12.0),
            companions_deal("Damage", 20.0),
            companions_deal("Damage", -5.0),
        ]);
        assert_eq!(total, 27.0);
    }

    #[test]
    fn without_the_transfer_they_stay_with_the_companion() {
        assert_eq!(
            player_increased_damage(vec![companions_deal("Damage", 12.0)]),
            0.0
        );
    }

    #[test]
    fn only_companion_damage_is_shared() {
        // All-minion damage is another stat with its own transfer.
        let minions_deal = Modifier::new(
            "MinionModifier",
            ModType::List,
            ModValue::NestedMods(vec![Modifier::number("Damage", ModType::Inc, 30.0)]),
        );
        let total = player_increased_damage(vec![
            transfer(),
            minions_deal,
            companions_deal("MaximumLife", 40.0),
            companions_deal("Damage", 12.0),
        ]);
        assert_eq!(total, 12.0);
    }

    #[test]
    fn the_copy_keeps_the_line_it_came_from() {
        let mut player = Actor::new(1, ActorBaseStats::default());
        player
            .mod_db
            .add_list([transfer(), companions_deal("Damage", 12.0)]);
        let mut env = Env::new(player);
        apply_companion_damage_to_player(&mut env);
        let copy = env.player.mod_db.named("Damage").next().expect("a copy");
        assert_eq!(
            copy.source.as_deref(),
            Some("Companions deal increased Damage")
        );
    }
}
