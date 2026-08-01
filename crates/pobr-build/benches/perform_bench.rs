//! Full-orchestration hot-path benchmark.
//!
//! The dual pass (2 hand × 2 crit) theoretically multiplies the offence
//! hot-path calculation cost by 4x — this benchmark is the quantitative gate
//! against that risk: **per-build `calculate_with_data` at the end must stay
//! within 2.5x of the starting baseline**. Going over budget requires lazy
//! short-circuits (skip OffHand when not dual-wielding, skip the crit pass
//! when there's no crit mod), and any short-circuit must ship with an
//! equivalence test.
//!
//! Run with: `cargo bench -p pobr-build --bench perform_bench`.
//! CI doesn't run criterion (too slow); the gate is a manual run before merge
//! with results posted to the PR, matching the `mod_db_bench` convention. See
//! `devs/scripts/bench-baseline.md` for the baseline-recording process.
//!
//! Corpus choice: the ninja 18-build set has no strict dual-wield build, so we
//! use the heaviest attack-side build, monk flicker-strike (exercises the
//! hand/crit main path of the dual pass), plus a spell control, sorceress
//! comet (crit-pass path, no hand-pass branching). A real dual-wield case can
//! be added once a dual-wield fixture lands.

use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use std::path::{Path, PathBuf};

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build_from_code};
use pobr_core::calc::MinimalInput;
use pobr_data::monster::EnemyTier;
use pobr_gamedata::{GameData, repo_data_root};

fn builds_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/demo-bd-test/builds")
        .canonicalize()
        .expect("builds dir exists")
}

fn load_build(name: &str) -> pobr_build::Build {
    let code =
        std::fs::read_to_string(builds_dir().join(name).join("code.txt")).expect("read code.txt");
    parse_build_from_code(code.trim()).expect("parse build code")
}

fn options() -> DataOrchestratorOptions {
    // Matches the default settings used by ninja_parity.
    DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        enemy_level: 0,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: true,
        extra_modifier_texts: vec![],
        ..Default::default()
    }
}

fn bench_perform(c: &mut Criterion) {
    let data = {
        let game = GameData::new(repo_data_root().join(pobr_gamedata::data_version()));
        BuildData::load(&game).expect("load BuildData")
    };
    let attack = load_build("monk-martial-artist-flicker-strike");
    let spell = load_build("sorceress-stormweaver-comet");
    let opts = options();

    // Attack build: the main path exercised by both the hand pass and crit pass.
    c.bench_function("perform_attack_flicker", |b| {
        b.iter(|| {
            black_box(calculate_with_data(black_box(&attack), &data, &opts))
                .expect("calculate attack build")
        })
    });

    // Spell build control: no hand-pass branching, isolates the crit pass's individual contribution.
    c.bench_function("perform_spell_comet", |b| {
        b.iter(|| {
            black_box(calculate_with_data(black_box(&spell), &data, &opts))
                .expect("calculate spell build")
        })
    });
}

criterion_group!(benches, bench_perform);
criterion_main!(benches);
