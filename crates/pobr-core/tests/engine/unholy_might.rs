use pobr_core::calc::{Actor, ActorBaseStats, Env, perform};
use pobr_core::{CalcConfig, ModTag, Modifier};
use pobr_data::catalog::buffs::BuffDefinitionsDef;
use pobr_data::prelude::*;

const CONDUIT: &str =
    "While you are not on Low Mana, you and Allies in your Presence have Unholy Might";
const BLACKENED: &str =
    "4% increased Magnitude of Unholy Might Buffs you grant per 100 maximum Mana";

fn add_line(env: &mut Env, text: &str, id: &str, active: bool) {
    let parsed = crate::support::parse_mod(text).unwrap();
    assert!(parsed.is_complete(), "{text}: {parsed:?}");
    env.player
        .mod_db
        .add_list(parsed.mods.into_iter().map(|mut m| {
            let origin = ModifierSource::new(SourceId::new(SourceKind::PassiveNode, id))
                .with_raw_text(text)
                .with_stat(m.name.clone())
                .with_mod_type(m.mod_type);
            m = m.with_origin(origin);
            if !active {
                m.tags.push(ModTag::condition("InactiveGrant", false));
            }
            m
        }));
}

fn environment(mana: f64, blackened: bool, active: bool) -> Env {
    let base = ActorBaseStats {
        life: 1000.0,
        mana,
        hit_min: 100.0,
        hit_max: 100.0,
        action_rate: 1.0,
        ..Default::default()
    };
    let mut env = Env::new(Actor::new(90, base)).with_config(
        CalcConfig::spell()
            .with_mode_combat(true)
            .with_mode_buffs(true),
    );
    let defs: BuffDefinitionsDef = serde_json::from_str(
        &std::fs::read_to_string(
            crate::support::test_data_dir().join("../overlay-common/buff_definitions.json"),
        )
        .unwrap(),
    )
    .unwrap();
    env.buff_definitions = defs.buffs;
    add_line(&mut env, CONDUIT, "26085", active);
    if blackened {
        add_line(&mut env, BLACKENED, "2877", true);
    }
    // Deliberately unrelated to the caster's pool: recipient Mana must not
    // determine the magnitude of a buff the caster grants.
    env.minions.push(Actor::new(
        36,
        ActorBaseStats {
            mana: 9999.0,
            ..base
        },
    ));
    env
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
}

#[test]
fn caster_mana_scales_self_and_ally_named_buff_once_with_source_attribution() {
    for (mana, blackened, magnitude) in [
        (1037.04, false, 100.0),
        (99.9, true, 100.0),
        (100.0, true, 104.0),
        (499.0, true, 116.0),
        (1037.04, true, 140.0),
        (2299.0, true, 188.0),
        (4199.0, true, 264.0),
    ] {
        let mut env = environment(mana, blackened, true);
        // An explicit buff and the ally grant establish one named buff.
        env.minions[0]
            .mod_db
            .add_mod(Modifier::flag("Condition:UnholyMight"));
        perform(&mut env).unwrap();
        close(env.cfg.multiplier("UnholyMightMagnitude"), magnitude);
        close(env.player.output.dps, 100.0 + magnitude * 0.3);
        close(env.minions[0].output.dps, 100.0 + magnitude * 0.3);
        let gains: Vec<_> = env.minions[0].mod_db.named("DamageGainAsChaos").collect();
        assert_eq!(gains.len(), 1);
        let source = gains[0].origin.as_ref().unwrap();
        assert_eq!(
            source.source_id,
            SourceId::new(SourceKind::Buff, "buff.UnholyMight")
        );
        assert_eq!(
            source.parent_source_id,
            Some(SourceId::new(SourceKind::PassiveNode, "26085"))
        );
        assert_eq!(source.raw_text.as_deref(), Some(CONDUIT));
        if blackened {
            let source = env.minions[0]
                .mod_db
                .named("Multiplier:UnholyMightMagnitude")
                .find(|m| m.origin.as_ref().is_some_and(|o| o.source_id.id == "2877"))
                .unwrap();
            close(source.value.as_number().unwrap(), magnitude - 100.0);
            assert!(
                source.tags.is_empty(),
                "caster Mana was resolved before crossing actors"
            );
            assert_eq!(
                source.origin.as_ref().unwrap().raw_text.as_deref(),
                Some(BLACKENED)
            );
            assert_eq!(
                source.origin.as_ref().unwrap().stat_id,
                Some(source.name.clone())
            );
            assert_eq!(
                source.origin.as_ref().unwrap().mod_type,
                Some(ModType::Base)
            );
            assert!(
                env.player
                    .mod_db
                    .named("UnholyMightGrantedMagnitude")
                    .any(|m| m.origin.as_ref().is_some_and(|o| o.source_id.id == "2877"))
            );
        }
    }
}

#[test]
fn uses_completed_caster_mana_and_separate_recipient_buff_effect() {
    use pobr_core::calc::session::{BuffKind, BuffSpec};
    let mut env = environment(990.0, true, true);
    env.buff_skills.push(BuffSpec {
        name: "Mana buff".into(),
        kind: BuffKind::Buff,
        skill_id: "mana-buff".into(),
        mods: vec![Modifier::number("MaximumMana", ModType::Base, 60.0)],
        magnitude: 1.0,
        slot: None,
        socket_index: 1,
        is_mark: false,
        ignore_curse_limit: false,
        local_effect_inc: 0.0,
        local_effect_more: 1.0,
        skill_types: pobr_data::skill::SkillTypes::NONE,
    });
    env.player
        .mod_db
        .add_mod(Modifier::number("BuffEffectOnSelf", ModType::Inc, 10.0));
    env.minions[0]
        .mod_db
        .add_mod(Modifier::number("BuffEffectOnSelf", ModType::Inc, 25.0));
    perform(&mut env).unwrap();
    assert!(env.player.output.mana >= 1000.0 && env.player.output.mana < 1100.0);
    close(env.player.output.dps, 100.0 + 0.3 * 1.1 * 140.0);
    close(env.minions[0].output.dps, 100.0 + 0.3 * 1.25 * 140.0);
}

#[test]
fn low_mana_inactive_grant_and_noncombat_do_not_grant_the_buff() {
    for case in ["low_mana", "inactive", "noncombat"] {
        let mut env = environment(1037.04, true, case != "inactive");
        if case == "low_mana" {
            env.cfg.conditions.insert("LowMana".into(), true);
        }
        if case == "noncombat" {
            env.cfg.mode_combat = false;
        }
        perform(&mut env).unwrap();
        close(env.player.output.dps, 100.0);
        close(env.minions[0].output.dps, 100.0);
        assert_eq!(env.minions[0].mod_db.named("DamageGainAsChaos").count(), 0);
    }
}

#[test]
fn no_presence_or_blocked_ally_grants_preserve_self_buff_only() {
    for case in ["no_presence", "hidden", "cannot_share"] {
        let mut env = environment(1037.04, true, true);
        match case {
            "no_presence" => {
                env.cfg
                    .multipliers
                    .insert("MinionPresenceCount".into(), 0.0);
            }
            "hidden" => env.minions[0]
                .mod_db
                .add_mod(Modifier::flag("HiddenMonster")),
            "cannot_share" => env
                .player
                .mod_db
                .add_mod(Modifier::flag("SelfAurasCannotAffectAllies")),
            _ => unreachable!(),
        }
        perform(&mut env).unwrap();
        close(env.player.output.dps, 142.0);
        close(env.minions[0].output.dps, 100.0);
    }
}

#[test]
fn parent_buff_condition_alone_never_grants_unholy_might_to_minions() {
    let mut env = environment(1037.04, true, false);
    env.cfg.conditions.insert("UnholyMight".into(), true);
    perform(&mut env).unwrap();
    close(env.player.output.dps, 142.0);
    close(env.minions[0].output.dps, 100.0);
}
