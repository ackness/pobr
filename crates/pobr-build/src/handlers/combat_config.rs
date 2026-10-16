//! Configured enemy debuffs and the strength of an existing buff.

use pobr_core::rules::{HandlerOutcome, HandlerRegistry};
use pobr_core::{ModTag, Modifier};
use pobr_data::modifier::ModType;

pub(super) fn register(registry: &mut HandlerRegistry) {
    for (id, flag) in [
        ("config:raiseSpectreEnableCurses", "SpectreCursesEnabled"),
        (
            "config:summonCompanionEnableCurses",
            "CompanionCursesEnabled",
        ),
    ] {
        registry
            .register(
                id,
                Box::new(move |_| HandlerOutcome::player_mods(vec![Modifier::flag(flag)])),
            )
            .expect("unique minion curse handler");
    }
    for (id, name, skill) in [
        (
            "config:darkPactSkeletonLife",
            "DarkPactSkeletonLife",
            "Dark Pact",
        ),
        ("config:hoaOverkill", "HeraldOfAshOverkill", "Herald of Ash"),
    ] {
        registry
            .register(
                id,
                Box::new(move |ctx| {
                    HandlerOutcome::player_mods(vec![
                        Modifier::number(name, ModType::Override, ctx.input().max(0.0)).with_tag(
                            ModTag::SkillName {
                                names: vec![skill.into()],
                            },
                        ),
                    ])
                }),
            )
            .expect("unique config handler");
    }
    registry
        .register(
            "config:SecondsSinceInevitableCrit",
            Box::new(|ctx| {
                let seconds = ctx.input().max(0.0);
                let mut mods = Vec::new();
                if seconds < 4.0 {
                    mods.extend([
                        Modifier::flag("Condition:CritRecently"),
                        Modifier::flag("Condition:SkillCritRecently"),
                    ]);
                }
                HandlerOutcome {
                    player_mods: mods
                        .into_iter()
                        .map(|modifier| modifier.with_tag(ModTag::condition("Combat", false)))
                        .collect(),
                    scalars: vec![("SecondsSinceInevitableCrit".into(), seconds)],
                    ..Default::default()
                }
            }),
        )
        .expect("unique config handler");
    registry
        .register(
            "config:conditionEnemyArmourBroken",
            Box::new(|_| HandlerOutcome {
                enemy_mods: vec![Modifier::flag("ArmourBreakConfigured")],
                ..Default::default()
            }),
        )
        .expect("unique config handler");
    registry
        .register(
            "config:multiplierArmourBreak",
            Box::new(|ctx| HandlerOutcome {
                enemy_mods: vec![Modifier::number(
                    "ArmourBreakAmount",
                    ModType::Base,
                    ctx.input().max(0.0),
                )],
                ..Default::default()
            }),
        )
        .expect("unique config handler");
    registry
        .register(
            "config:overrideBuffBlinded",
            Box::new(|ctx| HandlerOutcome {
                enemy_mods: vec![Modifier::number(
                    "BlindEffect",
                    ModType::Override,
                    ctx.input().max(0.0),
                )],
                ..Default::default()
            }),
        )
        .expect("unique config handler");
    registry
        .register(
            "config:overrideBuffElusive",
            Box::new(|ctx| {
                HandlerOutcome::player_mods(vec![Modifier::number(
                    "ElusiveEffect",
                    ModType::Override,
                    ctx.input().max(0.0),
                )])
            }),
        )
        .expect("unique config handler");
}
