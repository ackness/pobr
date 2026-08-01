//! End-to-end integration tests for the minion pipeline (/B/C).
//!
//! Coverage: the BuildData minion query API (A5); the orchestrator recognizing
//! a summon gem -> `OutputTable.minions` non-empty (B2); createMinionSkills +
//! feeding the main skill into offence (C1/C2); modDB assembly completeness
//! (C3). Golden values come from PoB2's panel with matching parameters (oracle
//! intermediate values noted where relevant).

use pobr_build::{
    Build, BuildData, CharacterIdentity, DataOrchestratorOptions, SocketGroup, calculate_with_data,
};
use pobr_core::calc::build_minion_context_from_def;
use pobr_core::calc::minion::{
    AttributeInfusion, minion_level_from_gem_level, resolve_minion_level,
};
use pobr_gamedata::{GameData, repo_data_root};

// Minion golden values (life/damage/DPS, from Minions.lua/Spectres.lua + the
// PoB2 panel) are version-specific, so pin the data version they're checked
// against (decoupled from the active DATA_VERSION; see
// pobr_data::GOLDEN_PARITY_DATA_VERSION).
fn version() -> String {
    pobr_data::GOLDEN_PARITY_DATA_VERSION.to_string()
}

fn load_data() -> BuildData {
    let data = GameData::new(repo_data_root().join(version()));
    BuildData::load(&data).expect("load golden parity version data")
}

fn default_opts() -> DataOrchestratorOptions {
    DataOrchestratorOptions {
        inject_character_base: true,
        ..Default::default()
    }
}

/// A Witch build with only a Raise Zombie (summon gem level N) active skill.
fn zombie_build(gem_level: u32) -> Build {
    Build::new()
        .with_character(CharacterIdentity {
            level: 90,
            class_name: "Witch".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(
            SocketGroup::new()
                .with_slot("weapon1")
                .with_gem("Metadata/Items/Gems/SkillGemRaiseZombie")
                .with_gem_skill("RaiseZombiePlayer", gem_level),
        )
}

// A5: BuildData minion query API

#[test]
fn build_data_minion_def_zombie() {
    let data = load_data();
    let zombie = data
        .minion_def("RaisedZombie")
        .expect("RaisedZombie is in the data pack");
    // Values come from Minions.lua and drift with balance patches -> blessed snapshot (refresh with POBR_BLESS_PINS=1).
    pobr_gamedata::test_pins::assert_pin(
        &repo_data_root().join(version()),
        "minions.raised_zombie",
        serde_json::json!({ "life": zombie.life, "damage": zombie.damage }),
    );
    assert!(zombie.base_damage_ignores_attack_speed);
}

#[test]
fn build_data_minion_def_spectre_falls_back() {
    let data = load_data();
    // spectre key = full metadata path (minions.json miss -> falls through to spectres.json)
    let c = data
        .minion_def("Metadata/Monsters/LeagueAbyss/Lightless/Cocoon3Spectre")
        .expect("Lightless Abomination is in the data pack (falls into spectres)");
    // Values come from Spectres.lua and drift with balance patches (0.5.4b: life 3.0->2.2) -> blessed snapshot.
    pobr_gamedata::test_pins::assert_pin(
        &repo_data_root().join(version()),
        "minions.spectre_cocoon3",
        serde_json::json!({ "life": c.life, "armour": c.armour }),
    );
}

#[test]
fn build_data_effect_minion_list() {
    let data = load_data();
    assert_eq!(
        data.effect_minion_list("RaiseZombiePlayer"),
        ["RaisedZombie"]
    );
    assert_eq!(
        data.effect_minion_list("RagingSpiritsPlayer"),
        ["SummonedRagingSpirit"]
    );
    // Non-summon skill -> empty slice
    assert!(data.effect_minion_list("FireballPlayer").is_empty());
    assert!(data.effect_minion_list("NonexistentSkill").is_empty());
}

// B2: orchestrator recognizes a summon gem -> OutputTable.minions non-empty

#[test]
fn summon_build_populates_output_minions() {
    let data = load_data();
    let out = calculate_with_data(&zombie_build(20), &data, &default_opts()).expect("calculate");
    // A summon gem is recognized -> OutputTable.minions non-empty (one of the G4 closure criteria).
    assert!(
        !out.minions.is_empty(),
        "a Raise Zombie build should produce a minion snapshot"
    );
    let zombie = &out.minions[0];
    // Minion level = minionLevelTable[gem_level=20] = 40 (CalcActiveSkill.lua:896 default rule).
    assert_eq!(zombie.level, minion_level_from_gem_level(20));
    assert_eq!(zombie.level, 40);
    // Life > 0 (derived from the monster-level base table × the 0.7 normalization multiplier).
    assert!(
        zombie.life > 0.0,
        "minion life should be > 0, measured {}",
        zombie.life
    );
}

#[test]
fn non_summon_build_has_empty_minions() {
    let data = load_data();
    // Non-summon main skill (Fireball) -> minions must be empty (gate correctness: zero behavioural leakage).
    let build = Build::new()
        .with_character(CharacterIdentity {
            level: 90,
            class_name: "Witch".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(
            SocketGroup::new()
                .with_slot("weapon1")
                .with_gem("Metadata/Items/Gems/SkillGemFireball")
                .with_gem_skill("FireballPlayer", 20),
        );
    let out = calculate_with_data(&build, &data, &default_opts()).expect("calculate");
    assert!(
        out.minions.is_empty(),
        "a non-summon build's minions should be empty"
    );
}

/// Minion life = monster-ally life table(level) × the life normalization
/// multiplier (0.7) — matches core's `build_minion_context_from_def`
/// derivation (the B2 wiring introduces no extra scaling).
#[test]
fn summon_zombie_life_matches_core_derivation() {
    let data = load_data();
    let out = calculate_with_data(&zombie_build(20), &data, &default_opts()).expect("calculate");
    let zombie_out = &out.minions[0];

    // Derives the same minion directly via core's pure function and compares against the life it produces after the defence pipeline.
    let def = data
        .minion_def("RaisedZombie")
        .expect("RaisedZombie is in the data pack");
    let ctx = build_minion_context_from_def(def, 20, vec![], vec![], AttributeInfusion::default());
    let core_life = ctx.base.life;
    assert_eq!(zombie_out.level, resolve_minion_level(20));
    // OutputTable.minions' life is emitted by the defence pipeline (life pool) and shares the same order as base.life;
    // B2 doesn't add any minion mod, so they should match.
    assert!(
        (zombie_out.life - core_life).abs() < 1.0,
        "orchestrator minion life {} should ≈ core-derived {}",
        zombie_out.life,
        core_life
    );
}

// B3: MinionModifier channel -- `Minions deal X% increased Damage` reaches the minion ModDb

/// Player gear with `Minions deal 50% increased Damage` (attributed to the
/// item) -> minion DPS increases; the player itself is unaffected (a minion
/// mod is Unsupported in the player's main pipeline, but reaches the minion
/// ModDb).
#[test]
fn minion_increased_damage_raises_minion_dps() {
    use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};

    let data = load_data();

    let base = calculate_with_data(&zombie_build(20), &data, &default_opts()).expect("base calc");
    let base_dps = base.minions[0].dps;
    assert!(base_dps > 0.0, "baseline minion DPS should be > 0");

    // Ring carries a minion mod (attributed via collect_item_texts -> parse_minion_modifier).
    let ring = Item {
        base: ItemBaseId::from("Ring"),
        rarity: ItemRarity::Rare,
        quality: 0,
        corrupted: false,
        implicit_texts: vec![],
        modifier_texts: vec!["Minions deal 50% increased Damage".into()],
        enchant_texts: vec![],
        rolled_defence: RolledDefence::default(),
        parsed_stats: vec![],
    };
    let buffed_build = zombie_build(20).set_item(EquipmentSlot::Ring1, ring);
    let buffed = calculate_with_data(&buffed_build, &data, &default_opts()).expect("buffed calc");
    let buffed_dps = buffed.minions[0].dps;

    // Minion DPS should increase with +50% increased Damage (the exact multiplier depends on other stacked inc mods).
    assert!(
        buffed_dps > base_dps,
        "minion DPS should grow with Minions deal 50% increased Damage: base {base_dps} → buffed {buffed_dps}"
    );
    // Player's own DPS isn't affected by the minion mod (a minion mod doesn't feed the player's aggregation).
    assert_eq!(
        buffed.dps, base.dps,
        "the player's own DPS should not be changed by a minion mod"
    );
}
