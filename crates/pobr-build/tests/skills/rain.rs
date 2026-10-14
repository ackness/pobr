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
