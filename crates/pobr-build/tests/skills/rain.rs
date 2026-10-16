//! Rain of Arrows hits as its arrows land (`pobr_core::calc::rain`): one
//! arrow every 0.08 seconds, which projectile speed hastens, and an enemy
//! can be hit once every 0.12 seconds.

use pobr_build::{
    Build, BuildData, CharacterIdentity, DataOrchestratorOptions, SocketGroup,
    calculate_with_data_session,
};
use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};
use pobr_gamedata::{GameData, repo_data_root};

fn data() -> BuildData {
    let data = GameData::new(repo_data_root().join(pobr_gamedata::data_version()));
    BuildData::load(&data).expect("load BuildData from repo data")
}

fn rain_of_arrows() -> Build {
    Build::new()
        .with_character(CharacterIdentity {
            level: 90,
            class_name: "Ranger".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(
            SocketGroup::new()
                .with_slot("weapon1")
                .with_active_skill("RainOfArrowsPlayer", 20),
        )
        .set_item(
            EquipmentSlot::Weapon1,
            Item {
                base: ItemBaseId::from("Crude Bow"),
                rarity: ItemRarity::Normal,
                quality: 0,
                corrupted: false,
                implicit_texts: vec![],
                modifier_texts: vec![],
                enchant_texts: vec![],
                rolled_defence: RolledDefence::default(),
                socketed: Vec::new(),
                parsed_stats: vec![],
            },
        )
}

/// `(hits per second, arrows per hit)` of the build with `lines` added,
/// under `options`.
fn rate(data: &BuildData, options: &DataOrchestratorOptions, lines: &[&str]) -> (f64, f64) {
    let options = DataOrchestratorOptions {
        extra_modifier_texts: lines.iter().map(|line| line.to_string()).collect(),
        ..options.clone()
    };
    let session =
        calculate_with_data_session(&rain_of_arrows(), data, &options).expect("calculates");
    let rain = session.rain().expect("Rain of Arrows is a rain");
    assert_eq!(session.output().action_rate, rain.hits_per_second);
    (rain.hits_per_second, rain.arrows_per_hit)
}

#[test]
fn the_hits_come_with_the_first_arrow_after_the_hit_interval() {
    let data = data();
    let options = DataOrchestratorOptions::default();
    // An arrow every 0.08 s: the one at 0.16 s is the first after 0.12 s.
    let (hits, per_hit) = rate(&data, &options, &[]);
    assert_eq!(per_hit, 2.0);
    assert!((hits - 6.25).abs() < 1e-6, "{hits}");
    // 50% faster: every 0.0533 s, and the third arrow, at 0.16 s, hits.
    let (hits, per_hit) = rate(&data, &options, &["50% increased Projectile Speed"]);
    assert_eq!(per_hit, 3.0);
    assert!((hits - 6.25).abs() < 1e-6, "{hits}");
    // Twice as fast: every 0.04 s, and the third arrow lands at 0.12 s.
    let (hits, per_hit) = rate(&data, &options, &["100% increased Projectile Speed"]);
    assert_eq!(per_hit, 3.0);
    assert!((hits - 1.0 / 0.12).abs() < 1e-6, "{hits}");
    // Attack speed makes no arrow land sooner.
    let (hits, _) = rate(&data, &options, &["50% increased Attack Speed"]);
    assert!((hits - 6.25).abs() < 1e-6, "{hits}");
}

/// The reference's exporter counts a hit whenever the enemy can be hit again.
#[test]
fn on_the_reference_basis_the_enemy_is_hit_whenever_it_can_be() {
    let data = data();
    let options = DataOrchestratorOptions::default().on_reference_basis();
    let (hits, per_hit) = rate(&data, &options, &[]);
    assert_eq!(per_hit, 1.0);
    assert!((hits - 1.0 / 0.12).abs() < 1e-6, "{hits}");
}

/// Hailstorm's 200 ms target lockout applies to bolts landing at the firing
/// cadence. Its 300 ms flight time delays the rain without setting that
/// cadence, and projectile speed does not change the magazine's firing time.
#[test]
fn hailstorm_lands_at_the_attack_cadence_and_keeps_reload_timing() {
    let data = data();
    let build = pobr_build::parse_build(
        r#"<PathOfBuilding2><Build level="90" className="Witch"/>
<Skills><Skill mainActiveSkill="1">
<Gem skillId="HailstormRoundsAmmoPlayer" level="20"/></Skill></Skills>
<Items activeItemSet="1"><Item id="1">Rarity: NORMAL
Makeshift Crossbow
</Item><ItemSet id="1"><Slot name="Weapon 1" itemId="1"/></ItemSet></Items>
</PathOfBuilding2>"#,
    )
    .unwrap();
    let run = |speed: &str, projectiles: bool, reference: bool| {
        let mut options = DataOrchestratorOptions::default();
        if reference {
            options = options.on_reference_basis();
        }
        options.extra_modifier_texts.push(speed.into());
        if projectiles {
            options.extra_modifier_texts.extend([
                "100% increased Projectile Speed".into(),
                "Skills fire 2 additional Projectiles".into(),
            ]);
        }
        calculate_with_data_session(&build, &data, &options).unwrap()
    };
    for speed in ["0% increased Attack Speed", "1000% increased Attack Speed"] {
        let session = run(speed, false, false);
        let rain = session.rain().expect("Hailstorm has a landing model");
        let reload = session.crossbow_reload().expect("reload cycle");
        assert_eq!(reload.effective_bolt_count, 50.0);
        assert_eq!(rain.hit_interval, 0.2);
        assert!((rain.landing_interval - 1.0 / reload.firing_rate).abs() < 1e-9);
        let per_hit = (0.2 * reload.firing_rate - 1e-9).ceil().max(1.0);
        let expected = reload.effective_rate / per_hit;
        assert!((session.output().action_rate - expected).abs() < 1e-6);
        assert_eq!(rain.hits_per_second, session.output().action_rate);
        assert_eq!(
            session.output().effective_action_rate,
            session.output().action_rate
        );
        for stat in ["ActionRate", "TotalDPS"] {
            assert!(
                session.stat_formula(stat).is_some(),
                "{stat} formula agrees with output"
            );
        }
        let faster_projectiles = run(speed, true, false);
        assert_eq!(
            faster_projectiles.output().action_rate,
            session.output().action_rate
        );
        assert_eq!(faster_projectiles.crossbow_reload(), Some(reload));
    }
    let fast = run("1000% increased Attack Speed", false, false);
    assert!(fast.rain().unwrap().arrows_per_hit > 1.0);
    let reference = run("1000% increased Attack Speed", false, true);
    let reload = reference.crossbow_reload().unwrap();
    let expected = reload.firing_rate.min(5.0) * reload.effective_rate / reload.firing_rate;
    assert!((reference.output().action_rate - expected).abs() < 1e-6);
}
