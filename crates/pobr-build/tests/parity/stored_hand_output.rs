//! Comparison of the `Stored<Type>*` family (HandOutput) against real builds.
//!
//! vendor `CalcOffence.lua:4047-4057`: `Stored<Type>CritAvg/HitAvg/CombinedAvg`
//! feeds ailment magnitude, and must satisfy two structural identities
//! (verified against the oracle 2026-06-12, vendor 0.18.0, via
//! `tools/pob2-oracle/run.sh`'s `mainHandOutput` dump for three builds):
//!
//! 1. `StoredCritAvg == StoredHitAvg × CritMultiplier` (when there's no
//!    CriticalStrike condition mod; oracle flicker-strike: Physical
//!    7524.775 == 1551.5 × 4.85 ✓, bow-shot: Physical 39061.245 == 8938.5 ×
//!    4.37 ✓, twister: Cold 17750.4 == 3440 × 5.16 ✓).
//! 2. `StoredCombinedAvg == CritAvg×c + HitAvg×(1−c)` (c = that hand's
//!    CritChance; oracle flicker-strike: Physical
//!    6549.84 == 7524.775×0.836784 + 1551.5×0.163216 ✓).
//!
//! Bit-for-bit absolute-value comparison is the job of the overall damage
//! parity effort (offence currently @10% ≈44%, converging gradually via the
//! ninja_parity gate); this test only pins down that PoBR's **structural
//! identities**, isomorphic to vendor's, hold on >=3 real builds, and that the
//! Stored family is non-empty in HandOutput (a precondition for an unbroken
//! ailment chain).

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build_from_code};
use pobr_core::calc::{MinimalInput, OutputTable};
use pobr_data::monster::EnemyTier;
use pobr_gamedata::{GameData, repo_data_root};
use std::path::{Path, PathBuf};

const BUILDS: &[&str] = &[
    "monk-martial-artist-flicker-strike",
    "huntress-ritualist-bow-shot",
    "monk-martial-artist-twister",
];

fn builds_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/demo-bd-test/builds")
        .canonicalize()
        .expect("builds dir exists")
}

fn run_build(dir: &Path, data: &BuildData) -> OutputTable {
    let code = std::fs::read_to_string(dir.join("code.txt")).expect("read code.txt");
    let build = parse_build_from_code(code.trim()).expect("parse build");
    let opts = DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        enemy_level: 0,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: true,
        extra_modifier_texts: vec![],
        ..Default::default()
    };
    calculate_with_data(&build, data, &opts).expect("calculate")
}

#[test]
fn stored_family_holds_vendor_identities_on_three_real_builds() {
    let data = {
        // Pins the data version being checked against the golden values (decoupled from the active DATA_VERSION); see pobr_data::GOLDEN_PARITY_DATA_VERSION.
        let game_data = GameData::new(repo_data_root().join(pobr_data::GOLDEN_PARITY_DATA_VERSION));
        BuildData::load(&game_data).expect("load BuildData")
    };

    for name in BUILDS {
        let out = run_build(&builds_dir().join(name), &data);
        let hand = out
            .main_hand
            .as_ref()
            .unwrap_or_else(|| panic!("{name}: an attack build must have a main_hand sub-table"));

        assert!(
            !hand.stored_combined_avg.is_empty(),
            "{name}: the Stored family must land in HandOutput (ailment-chain input)"
        );
        assert_eq!(hand.stored_crit_avg.len(), hand.stored_hit_avg.len());
        assert_eq!(hand.stored_crit_avg.len(), hand.stored_combined_avg.len());

        let c = hand.crit_chance;
        let m = hand.crit_multiplier;
        let mut any_nonzero = false;
        for (((ty, crit_avg), (_, hit_avg)), (_, combined)) in hand
            .stored_crit_avg
            .iter()
            .zip(hand.stored_hit_avg.iter())
            .zip(hand.stored_combined_avg.iter())
        {
            if *hit_avg > 0.0 {
                any_nonzero = true;
                // Identity 1 (no CriticalStrike condition mod -> both legs share the same input, the crit leg is ×m).
                let ratio = crit_avg / hit_avg;
                assert!(
                    (ratio - m).abs() < 1e-6,
                    "{name} {ty:?}: CritAvg/HitAvg={ratio} should == CritMultiplier={m}"
                );
            }
            // Identity 2 (the weighted accumulation from vendor :4048/:4053).
            let blend = crit_avg * c + hit_avg * (1.0 - c);
            assert!(
                (combined - blend).abs() < 1e-6 * blend.abs().max(1.0),
                "{name} {ty:?}: CombinedAvg={combined} should == blend={blend}"
            );
        }
        assert!(
            any_nonzero,
            "{name}: at least one damage type should be non-zero"
        );
    }
}
