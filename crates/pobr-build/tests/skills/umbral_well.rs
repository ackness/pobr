//! Umbral Well, the Abyssal Lich's notable: "Skeletal Minions you would
//! create instead grant you Umbral Souls for each Minion you would have
//! created". Each kind of Skeletal Minion becomes the game's Soul buff of
//! its kind, granting the skill's `skeletal_graft_buff_effect_magnitude` per
//! Soul.

use pobr_build::{
    Build, BuildData, CharacterIdentity, DataOrchestratorOptions, SocketGroup,
    calculate_with_data_session,
};
use pobr_core::calc::{CalculationSession, CompletedCalculationSession};

const UMBRAL_WELL: &str = "Skeletal Minions you would create instead grant you Umbral Souls for each Minion you would have created";

use crate::test_data as data;

/// A Witch casting Fireball beside a Skeletal Reaver and a Skeletal Sniper.
fn witch() -> Build {
    Build::new()
        .with_character(CharacterIdentity {
            level: 90,
            class_name: "Witch".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(
            SocketGroup::new()
                .with_slot("weapon1")
                .with_gem_skill("FireballPlayer", 20),
        )
        .add_socket_group(SocketGroup::new().with_gem_skill("SummonSkeletalReaversPlayer", 20))
        .add_socket_group(SocketGroup::new().with_gem_skill("SummonSkeletalSnipersPlayer", 20))
}

fn run(build: &Build, data: &BuildData, lines: &[&str]) -> CompletedCalculationSession {
    let options = DataOrchestratorOptions {
        inject_character_base: true,
        extra_modifier_texts: lines.iter().map(|line| line.to_string()).collect(),
        ..Default::default()
    };
    calculate_with_data_session(build, data, &options).expect("calculates")
}

/// The values the Soul buff `buff` grants under the modifier name `name`.
fn from_soul(session: &CalculationSession, name: &str, buff: &str) -> Vec<Option<f64>> {
    session
        .mods_named(name)
        .into_iter()
        .filter(|m| {
            m.origin
                .as_ref()
                .is_some_and(|origin| origin.source_id.id == format!("buff.{buff}"))
        })
        .map(|m| m.value.as_number())
        .collect()
}

#[test]
fn skeletal_minions_become_souls_on_the_character() {
    let data = data();
    let summoner = run(&witch(), &data, &[]);
    let lich = run(&witch(), &data, &[UMBRAL_WELL]);

    // The Reaver's Soul: "6% increased Skill Speed".
    let ratio = lich.output().action_rate / summoner.output().action_rate;
    assert!((ratio - 1.06).abs() < 1e-9, "{ratio}");
    assert_eq!(
        from_soul(&lich, "SkillSpeed", "lich_skeletal_graft_reaver"),
        [Some(6.0)]
    );
    // The Sniper's: "15% increased Projectile Speed".
    assert_eq!(
        from_soul(&lich, "ProjectileSpeed", "lich_skeletal_graft_archer"),
        [Some(15.0)]
    );

    // The minions are not created.
    assert_eq!(summoner.output().minions.len(), 2);
    assert!(lich.output().minions.is_empty());
}

/// A summoner without Umbral Well keeps the minions and gets no Soul.
#[test]
fn without_umbral_well_there_are_no_souls() {
    let data = data();
    let summoner = run(&witch(), &data, &[]);
    assert!(from_soul(&summoner, "SkillSpeed", "lich_skeletal_graft_reaver").is_empty());
    assert!(from_soul(&summoner, "ProjectileSpeed", "lich_skeletal_graft_archer").is_empty());
}
