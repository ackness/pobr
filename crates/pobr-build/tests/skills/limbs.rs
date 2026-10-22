//! The Transcendent limbs: two arms and two legs worn beside the equipment
//! (the game's item classes `IncursionArm` and `IncursionLeg`). Every stat a
//! limb carries is global, so its lines are modifiers of the character,
//! attributed to the limb's slot.

use pobr_build::{
    Build, BuildData, CharacterIdentity, DataOrchestratorOptions, SocketGroup,
    calculate_with_data_session,
};
use pobr_core::calc::CalculationSession;
use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, LimbSlot, RolledDefence};

use crate::test_data as data;

fn item(base: &str, implicits: &[&str]) -> Item {
    Item {
        base: ItemBaseId::from(base),
        rarity: ItemRarity::Normal,
        quality: 0,
        corrupted: false,
        implicit_texts: implicits.iter().map(|line| line.to_string()).collect(),
        modifier_texts: vec![],
        enchant_texts: vec![],
        rolled_defence: RolledDefence::default(),
        socketed: Vec::new(),
        parsed_stats: vec![],
    }
}

/// An archer with a plain bow and Lightning Arrow.
fn archer() -> Build {
    Build::new()
        .with_character(CharacterIdentity {
            level: 90,
            class_name: "Ranger".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(
            SocketGroup::new()
                .with_slot("weapon1")
                .with_active_skill("LightningArrowPlayer", 20),
        )
        .set_item(EquipmentSlot::Weapon1, item("Crude Bow", &[]))
}

fn run(build: &Build, data: &BuildData) -> CalculationSession {
    calculate_with_data_session(build, data, &DataOrchestratorOptions::default())
        .expect("calculates")
}

/// A Combat Arm's "6% increased Attack Speed" speeds the attack like the
/// same line on any other item, and is the arm's.
#[test]
fn a_limb_line_is_a_global_modifier_of_its_slot() {
    let data = data();
    let bare = run(&archer(), &data);
    let armed = archer().set_limb(
        LimbSlot::Arm1,
        item("Combat Arm", &["6% increased Attack Speed"]),
    );
    let session = run(&armed, &data);
    let ratio = session.output().action_rate / bare.output().action_rate;
    assert!((ratio - 1.06).abs() < 1e-9, "{ratio}");

    let from_arm: Vec<_> = session
        .mods_named("AttackSpeed")
        .into_iter()
        .filter_map(|m| m.origin.as_ref())
        .filter(|origin| origin.slot.as_deref() == Some("arm1"))
        .map(|origin| origin.source_id.id.as_str())
        .collect();
    assert_eq!(from_arm, ["item.arm1.implicit"]);
    assert!(session.unsupported_modifier_texts().is_empty());
}

/// A limb line no rule reads is listed, not dropped.
#[test]
fn an_unread_limb_line_is_listed_as_unsupported() {
    let data = data();
    let build = archer().set_limb(
        LimbSlot::Leg2,
        item("Sturdy Leg", &["Your legs hum with an unknown power"]),
    );
    let session = run(&build, &data);
    assert_eq!(
        session.unsupported_modifier_texts(),
        ["Your legs hum with an unknown power"]
    );
}
