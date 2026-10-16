//! Wind-ground "counts as boosted" rings (Berek's Grip/Pass/Respite, The Taming).
//!
//! The boost wraps across two physical lines of the item text; the pair parses
//! as one joined line and sets the Twister element conditions, so Twister gains
//! its element without ticking the Twister config checkboxes.

use pobr_build::{
    Build, BuildData, CharacterIdentity, DataOrchestratorOptions, SocketGroup,
    calculate_with_data_session,
};
use pobr_core::calc::{CalculationSession, MinimalInput};
use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};
use pobr_data::monster::EnemyTier;
use pobr_data::prelude::*;
use pobr_gamedata::GameData;

fn data() -> BuildData {
    BuildData::load(&GameData::new(pobr_gamedata::current_data_dir())).unwrap()
}

fn ring(mods: &[&str]) -> Item {
    Item {
        base: ItemBaseId::from("Prismatic Ring"),
        rarity: ItemRarity::Unique,
        quality: 0,
        corrupted: false,
        implicit_texts: vec![],
        modifier_texts: mods.iter().map(|s| (*s).into()).collect(),
        enchant_texts: vec![],
        rolled_defence: RolledDefence::default(),
        socketed: Vec::new(),
        parsed_stats: vec![],
    }
}

fn twister_build() -> Build {
    Build::new()
        .with_character(CharacterIdentity {
            level: 85,
            class_name: "Huntress".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(SocketGroup::new().with_active_skill("TwisterPlayer", 20))
}

fn run(build: &Build, data: &BuildData) -> CalculationSession {
    calculate_with_data_session(
        build,
        data,
        &DataOrchestratorOptions {
            base_input: MinimalInput::default(),
            inject_character_base: true,
            enemy_tier: EnemyTier::None,
            mode_effective: false,
            ..Default::default()
        },
    )
    .unwrap()
}

/// The Twister element gain for `element` (Cold/Fire/Lightning): the skill's
/// own 50% gain-as stat applies once its ground condition holds.
fn element_gain(session: &CalculationSession, element: &str) -> f64 {
    session.mod_db().sum(
        ModType::Base,
        session.config(),
        &[ModName::from(format!("SkillDamageGainAs{element}"))],
    )
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
}

#[test]
fn wrapped_wind_ground_lines_count_twister_as_boosted() {
    let data = data();
    let plain = run(&twister_build(), &data);
    for element in ["Cold", "Fire", "Lightning"] {
        close(element_gain(&plain, element), 0.0);
    }
    assert!(plain.unsupported_modifier_texts().is_empty());

    // The Taming: the multiple-surfaces line plus the wrapped triple-element
    // line (two physical lines). No Twister config is ticked.
    let taming = run(
        &twister_build().set_item(
            EquipmentSlot::Ring1,
            ring(&[
                "Wind Skills which can be boosted by Elemental Ground Surfaces can be boosted by multiple Elemental Ground Surfaces",
                "Wind Skills which can be boosted by Elemental Ground Surfaces count",
                "as being boosted by Ignited, Shocked, and Chilled Ground",
            ]),
        ),
        &data,
    );
    assert!(
        taming.unsupported_modifier_texts().is_empty(),
        "{:?}",
        taming.unsupported_modifier_texts()
    );
    for element in ["Cold", "Fire", "Lightning"] {
        close(element_gain(&taming, element), 50.0);
    }
    // Gaining 50% of damage as each of three elements adds 150% extra damage.
    close(taming.output().dps, plain.output().dps * 2.5);

    // Berek's Grip: only the wrapped Shocked line, so only Lightning.
    let grip = run(
        &twister_build().set_item(
            EquipmentSlot::Ring1,
            ring(&[
                "Wind Skills which can be boosted by Elemental Ground Surfaces count",
                "as being boosted by Shocked Ground",
            ]),
        ),
        &data,
    );
    assert!(
        grip.unsupported_modifier_texts().is_empty(),
        "{:?}",
        grip.unsupported_modifier_texts()
    );
    close(element_gain(&grip, "Cold"), 0.0);
    close(element_gain(&grip, "Fire"), 0.0);
    close(element_gain(&grip, "Lightning"), 50.0);
    close(grip.output().dps, plain.output().dps * 1.5);
}

#[test]
fn single_item_lines_still_parse_on_their_own() {
    let data = data();
    let result = run(
        &twister_build().set_item(EquipmentSlot::Ring1, ring(&["+100 to maximum Life"])),
        &data,
    );
    assert!(result.unsupported_modifier_texts().is_empty());
    close(
        result.output().life - run(&twister_build(), &data).output().life,
        100.0,
    );
}
