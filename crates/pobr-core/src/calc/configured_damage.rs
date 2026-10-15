//! Damage based on a stated target Life or overkill amount. The skill's
//! own game stat supplies the share; the config supplies only the amount.

use pobr_data::prelude::*;

use crate::Modifier;

use super::Env;

pub(super) fn apply(env: &mut Env) {
    for (input, share, divisor, names) in [
        (
            "DarkPactSkeletonLife",
            "DarkPactLifeAsChaos",
            100.0,
            &["ChaosDamageMin", "ChaosDamageMax"][..],
        ),
        (
            "HeraldOfAshOverkill",
            "HeraldOfAshBurningPerMinute",
            6000.0,
            &["FireDot"][..],
        ),
    ] {
        let Some(amount) = env.player.mod_db.override_(&env.cfg, input) else {
            continue;
        };
        let percent = env
            .player
            .mod_db
            .sum(ModType::Base, &env.cfg, &[ModName::from(share)]);
        let damage = amount * percent / divisor;
        if damage <= 0.0 {
            continue;
        }
        let origin = ModifierSource::new(SourceId::new(
            SourceKind::SkillGem,
            format!("skill.{share}"),
        ))
        .with_raw_text(format!("{share}: {percent} of configured {input} {amount}"));
        for name in names {
            env.player.mod_db.add_mod(
                Modifier::number(*name, ModType::Base, damage).with_origin(origin.clone()),
            );
        }
    }
}
