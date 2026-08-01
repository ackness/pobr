//! End-to-end integration test: a real PoB2 ninja build code -> full calculation.
//!
//! Goal: verify the `decode -> parse_build -> calculate_with_data` pipeline
//! runs end-to-end and key outputs land within a sane range (doesn't require
//! matching PoB2's numbers exactly).
//!
//! Unlike earlier versions: this test **uses the production parser directly**
//! ([`pobr_build::parse_build_from_code`]), no more hand-written XML
//! extraction inside the test. This also folds the production XML->Build
//! parsing path into the end-to-end regression.
//!
//! Assembly scope (handled by `parse_build`):
//! - Character identity (level / class / ascendancy): fully parsed.
//! - Passive tree nodes: `<Tree activeSpec>` picks the `<Spec nodes>`.
//! - Skill gem groups: `<Skills activeSkillSet>` picks each `<Skill>` under the `<SkillSet>`.
//! - Gear: `<Item>` text blocks are parsed via `parse_pob_xml_item`; `<ItemSet>` slots map to `EquipmentSlot`.
//! - CharacterBase: injected via BuildData's per-class attribute table (`inject_character_base`).
//! - Enemy: Pinnacle level 80 (effective-DPS convention).
//!
//! Gaps (recorded as deferred, don't block e2e passing):
//! - The gem -> modifier-text pipeline isn't exported yet (see the
//!   calc_orchestrator module doc) -> gems themselves contribute no modifier,
//!   but source attribution registration still works.
//! - Slots outside the `EquipmentSlot` enum (Charm / Flask / Ring 3, etc.) are
//!   ignored (not fed into the current calculation).
//! - Mastery effects (masteryEffects) aren't parsed yet -> Mastery node mods
//!   don't feed into the calculation.

use pobr_build::{
    BuildData, DataOrchestratorOptions, calculate_full_dps, calculate_with_data,
    parse_build_from_code,
};
use pobr_core::calc::MinimalInput;
use pobr_data::monster::EnemyTier;
use pobr_data::passive_tree::PassiveTreeSpec;
use pobr_gamedata::{GameData, repo_data_root};

// fixtures

const DEADEYE_CODE: &str = include_str!("../../../../examples/demo-bd-test/ninja-bd-deadeye.txt");

const MARTIAL_ARTIST_CODE: &str =
    include_str!("../../../../examples/demo-bd-test/ninja-bd-marial-artist.txt");

// Data loading (shared GameData + BuildData across every test in this suite)

fn load_game_data() -> BuildData {
    let data = GameData::new(repo_data_root().join(pobr_gamedata::data_version()));
    BuildData::load(&data).expect("load BuildData from repo data")
}

// Test bodies

/// Deadeye build: the full end-to-end pipeline (gear / passive tree / gems / CharacterBase / enemy).
#[test]
fn deadeye_e2e_full_pipeline_succeeds() {
    let build_data = load_game_data();
    let build = parse_build_from_code(DEADEYE_CODE).expect("parse deadeye build");

    // Assert: the production parser correctly reconstructs gear / passive tree / socket groups.
    assert!(
        !build.items.is_empty(),
        "expected items from deadeye build, got none"
    );
    assert!(
        !build.tree.allocated_nodes.is_empty(),
        "expected passive nodes from deadeye build, got none"
    );
    assert!(
        !build.socket_groups.is_empty(),
        "expected socket groups from deadeye build, got none"
    );
    // Character identity parses correctly.
    assert_eq!(build.character.level, 98);
    assert_eq!(build.character.class_name, "Ranger");
    assert!(build.character.ascendancy_name.contains("Deadeye"));

    let opts = DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        enemy_level: 80,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: true,
        extra_modifier_texts: vec![],
        ..Default::default()
    };

    let out = calculate_with_data(&build, &build_data, &opts)
        .expect("calculate_with_data should succeed for deadeye build");

    // Sanity assertions
    // CharacterBase (level 98 Ranger: 28 + 12*98 + 2*7 = 1218) comes from the per-class attribute table.
    // Gear / passive nodes should push life higher still. Conservative assertion: life > CharacterBase's own value.
    let ranger_char_base_life = 28.0 + 12.0 * 98.0 + 2.0 * 7.0;
    assert!(
        out.life > ranger_char_base_life,
        "life ({}) should exceed CharacterBase alone ({}); items/passives should add more",
        out.life,
        ranger_char_base_life
    );
    assert!(out.life.is_finite(), "life must be finite");

    // Finiteness assertions: defence fields.
    assert!(out.mana.is_finite(), "mana must be finite");
    assert!(out.armour.is_finite(), "armour must be finite");
    assert!(out.evasion.is_finite(), "evasion must be finite");
    assert!(
        out.energy_shield.is_finite(),
        "energy_shield must be finite"
    );

    // Under the effective convention, hit chance should be within (0, 1].
    assert!(
        out.hit_chance >= 0.0 && out.hit_chance <= 1.0,
        "hit_chance out of range: {}",
        out.hit_chance
    );

    // Resistances should be within a sane range (percent convention: no lower bound, hard cap at 90 on the top).
    for (label, res) in [
        ("fire", out.fire_resistance),
        ("cold", out.cold_resistance),
        ("lightning", out.lightning_resistance),
    ] {
        assert!(
            res.is_finite() && (-200.0..=90.0).contains(&res),
            "{label}_resistance out of range: {res}"
        );
    }
}

/// Deadeye build: verifies each source's contribution step by step (base vs passives vs gear).
#[test]
fn deadeye_e2e_contributions_are_additive() {
    let build_data = load_game_data();
    let full = parse_build_from_code(DEADEYE_CODE).expect("parse deadeye");

    let base_opts = DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        mode_effective: false,
        ..Default::default()
    };

    // Step 1: CharacterBase only (no gear / no passives).
    let build_base = pobr_build::Build::new().with_character(full.character.clone());
    let out_base =
        calculate_with_data(&build_base, &build_data, &base_opts).expect("base only calc");

    // Step 2: CharacterBase + passive tree.
    let build_with_tree = build_base.clone().with_tree(PassiveTreeSpec {
        allocated_nodes: full.tree.allocated_nodes.clone(),
        ..Default::default()
    });
    let out_with_tree =
        calculate_with_data(&build_with_tree, &build_data, &base_opts).expect("with tree calc");

    // Step 3: the full build (CharacterBase + passive tree + gear + gems).
    let out_full = calculate_with_data(&full, &build_data, &base_opts).expect("full calc");

    // Assert: life increases monotonically (CharacterBase <= +tree <= +items).
    assert!(
        out_with_tree.life >= out_base.life,
        "adding passive tree should not decrease life: base={} tree={}",
        out_base.life,
        out_with_tree.life
    );
    assert!(
        out_full.life >= out_base.life,
        "full build life ({}) should be >= base only ({})",
        out_full.life,
        out_base.life
    );
}

/// Martial Artist build: end-to-end pipeline for a non-Ranger class.
#[test]
fn martial_artist_e2e_full_pipeline_succeeds() {
    let build_data = load_game_data();
    let build = parse_build_from_code(MARTIAL_ARTIST_CODE).expect("parse martial artist build");

    assert_eq!(build.character.class_name, "Monk");
    assert!(!build.items.is_empty(), "expected items");
    assert!(!build.tree.allocated_nodes.is_empty(), "expected nodes");

    let opts = DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        enemy_level: 80,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: true,
        ..Default::default()
    };

    let out = calculate_with_data(&build, &build_data, &opts)
        .expect("calculate_with_data should succeed for martial artist build");

    assert!(out.life.is_finite(), "life must be finite");
    assert!(out.mana.is_finite(), "mana must be finite");
    assert!(
        out.life > 0.0,
        "martial artist life should be > 0 (CharacterBase injected)"
    );
}

/// Determinism: two calls with identical input produce identical results.
#[test]
fn deadeye_e2e_is_deterministic() {
    let build_data = load_game_data();
    let build = parse_build_from_code(DEADEYE_CODE).expect("parse");

    let opts = DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        enemy_level: 80,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: true,
        ..Default::default()
    };

    let out1 = calculate_with_data(&build, &build_data, &opts).expect("first calc");
    let out2 = calculate_with_data(&build, &build_data, &opts).expect("second calc");

    assert_eq!(out1.life, out2.life, "life is not deterministic");
    assert_eq!(out1.mana, out2.mana, "mana is not deterministic");
    assert_eq!(
        out1.fire_resistance, out2.fire_resistance,
        "fire_resistance is not deterministic"
    );
    assert_eq!(out1.dps, out2.dps, "dps is not deterministic");
}

/// Parse determinism: parsing the same build code twice yields equivalent Builds (item-slot->base mapping / nodes / socket groups).
#[test]
fn parse_build_is_deterministic() {
    let a = parse_build_from_code(DEADEYE_CODE).expect("parse a");
    let b = parse_build_from_code(DEADEYE_CODE).expect("parse b");

    assert_eq!(
        a.tree.allocated_nodes, b.tree.allocated_nodes,
        "allocated nodes differ"
    );
    assert_eq!(
        a.socket_groups, b.socket_groups,
        "socket groups differ (slot / enabled / gem ids)"
    );

    // Slot -> (base, mods) mapping matches item-by-item: proves HashMap iteration order doesn't affect item-slot binding.
    let mapping = |build: &pobr_build::Build| -> Vec<(String, String, Vec<String>)> {
        build
            .equipped_items()
            .into_iter()
            .map(|(slot, item)| {
                (
                    slot.id().to_string(),
                    item.base.to_string(),
                    item.modifier_texts.clone(),
                )
            })
            .collect()
    };
    assert_eq!(mapping(&a), mapping(&b), "slot→item mapping differs");
}

/// Effective vs panel convention: with mode_effective=true, hit chance should be <= mode_effective=false (panel = 100%).
#[test]
fn deadeye_e2e_effective_dps_lower_hit_chance() {
    let build_data = load_game_data();
    let build = parse_build_from_code(DEADEYE_CODE).expect("parse");

    let panel_opts = DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        mode_effective: false,
        ..Default::default()
    };
    let effective_opts = DataOrchestratorOptions {
        enemy_level: 80,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: true,
        ..panel_opts.clone()
    };

    let panel = calculate_with_data(&build, &build_data, &panel_opts).expect("panel calc");
    let effective =
        calculate_with_data(&build, &build_data, &effective_opts).expect("effective calc");

    // Panel-convention hit chance should be >= effective (panel doesn't account for enemy evasion).
    assert!(
        panel.hit_chance >= effective.hit_chance,
        "panel hit_chance ({}) should be >= effective ({})",
        panel.hit_chance,
        effective.hit_chance
    );
    assert!(
        effective.hit_chance >= 0.0 && effective.hit_chance <= 1.0,
        "effective hit_chance out of range: {}",
        effective.hit_chance
    );
}

/// FullDPS (multi-skill scaffolding): iterates enabled damaging skill groups, recalculates the whole build per group, and sums the CombinedDPS.
#[test]
fn deadeye_full_dps_sums_enabled_damaging_skills() {
    let build_data = load_game_data();
    let build = parse_build_from_code(DEADEYE_CODE).expect("parse deadeye");

    let opts = DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        enemy_level: 80,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: true,
        ..Default::default()
    };

    let report =
        calculate_full_dps(&build, &build_data, &opts).expect("calculate_full_dps should succeed");

    // Invariant: full_dps == the sum of each entry's CombinedDPS.
    let sum: f64 = report.per_skill.iter().map(|s| s.combined_dps).sum();
    assert_eq!(
        report.full_dps, sum,
        "full_dps must equal sum of per_skill CombinedDPS"
    );
    assert!(
        report.full_dps.is_finite() && report.full_dps >= 0.0,
        "full_dps must be finite & non-negative: {}",
        report.full_dps
    );

    // Each entry: comes from an enabled group, has CombinedDPS>0, and has a valid index.
    for s in &report.per_skill {
        assert!(
            s.combined_dps > 0.0,
            "per_skill 分项须 CombinedDPS>0：{s:?}"
        );
        assert!(
            s.group_index < build.socket_groups.len(),
            "group_index 越界：{s:?}"
        );
        assert!(
            build.socket_groups[s.group_index].enabled,
            "per_skill 分项须来自启用组：{s:?}"
        );
    }

    // The primary skill's DPS contribution is included (when it deals damage and its group is enabled) -> full_dps >= primary CombinedDPS.
    if report.primary.combined_dps > 0.0 {
        assert!(
            report.full_dps >= report.primary.combined_dps - 1e-6,
            "full_dps ({}) 应包含 primary CombinedDPS ({})",
            report.full_dps,
            report.primary.combined_dps
        );
    }

    // Determinism: two calls give the same result.
    let report2 = calculate_full_dps(&build, &build_data, &opts).expect("second full_dps calc");
    assert_eq!(report.full_dps, report2.full_dps, "full_dps 非确定性");
    assert_eq!(
        report.per_skill.len(),
        report2.per_skill.len(),
        "per_skill 数量非确定性"
    );
}
