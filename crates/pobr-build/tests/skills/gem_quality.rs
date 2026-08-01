//! Gem quality four-layer integration test (18-G1 / 15-G5).
//!
//! Coverage: XML `<Gem quality>` parsing -> `BuildData::effect_stats` quality
//! segment (trunc semantics) -> dual-run directional assertion
//! (stormweaver-comet 15×q20 fixture).
//!
//! **Oracle comparison**: golden values for the quality segment come from
//! PoB2's real framework function `calcLib.buildSkillInstanceStats`
//! (Modules/CalcTools.lua:138) at vendor commit
//! `2df5a7433dd2f1609e2fad8a6c3c917f923fe34f`, taken as the output delta
//! (quality=Q − quality=0), run via `tools/pob2-oracle/quality_stats.lua`
//! (recorded 2026-06-11):
//!
//! ```json
//! {"skill":"CometPlayer","level":21,"quality":20,
//!  "quality_contribution":{"base_spell_%_chance_to_echo":10}}
//! {"skill":"SparkPlayer","level":21,"quality":20,
//!  "quality_contribution":{"base_projectile_speed_+%":30}}
//! {"skill":"ArcticArmourPlayer","level":19,"quality":20,
//!  "quality_contribution":{"maximum_number_of_arctic_armour_stationary_stacks":1}}
//! {"skill":"ArcticArmourPlayer","level":19,"quality":19,"quality_contribution":{}}
//! ```
//!
//! Note ArcticArmour q19 = empty (trunc(0.05×19)=trunc(0.95)=0) — real-data
//! evidence of PoB2's `math.modf` truncation semantics; PoBR must match it
//! value-for-value.

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build_from_code};
use pobr_core::calc::MinimalInput;
use pobr_data::monster::EnemyTier;
use pobr_gamedata::{GameData, repo_data_root};
use std::path::Path;

fn load_data() -> BuildData {
    let data = GameData::new(repo_data_root().join(pobr_gamedata::data_version()));
    BuildData::load(&data).expect("load BuildData")
}

/// Build code for the stormweaver-comet fixture (15 q20 gems).
fn stormweaver_code() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/demo-bd-test/builds/sorceress-stormweaver-comet/code.txt");
    std::fs::read_to_string(path).expect("read stormweaver code.txt")
}

fn opts() -> DataOrchestratorOptions {
    DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        enemy_level: 0,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: false,
        extra_modifier_texts: vec![],
        ..Default::default()
    }
}

/// Unit test for trunc semantics: rate=0.55, q19 -> trunc(10.45)=10;
/// negative rate rounds toward zero (unlike floor); q0 -> empty segment.
/// Synthetic data, doesn't depend on real tables.
#[test]
fn quality_segment_truncates_toward_zero() {
    use pobr_data::catalog::QualityStat;
    let mut bd = BuildData::empty();
    bd.gem_quality_stats.insert(
        "SynthSkill".into(),
        vec![
            QualityStat {
                stat: "synth_pos".into(),
                per_quality_rate: 0.55,
                alt: false,
            },
            QualityStat {
                stat: "synth_neg".into(),
                per_quality_rate: -0.55,
                alt: false,
            },
        ],
    );

    let es = bd.effect_stats("SynthSkill", 20, 19, None);
    let get = |stat: &str| {
        es.quality
            .iter()
            .find(|s| s.stat == stat)
            .map(|s| s.value)
            .expect("stat present")
    };
    assert_eq!(get("synth_pos"), 10.0, "trunc(0.55×19)=trunc(10.45)=10");
    assert_eq!(
        get("synth_neg"),
        -10.0,
        "negative slope truncates toward zero: trunc(-10.45)=-10 (floor would give -11)"
    );

    // quality=0 -> empty segment; unknown skill -> empty segment.
    assert!(
        bd.effect_stats("SynthSkill", 20, 0, None)
            .quality
            .is_empty()
    );
    assert!(bd.effect_stats("NoSuch", 20, 20, None).quality.is_empty());
}

/// Real data + oracle comparison: each effect's q20 quality segment matches
/// the PoB2 `buildSkillInstanceStats` delta value-for-value (golden values are
/// the oracle run log in the module doc comment).
#[test]
fn quality_segment_matches_pob2_oracle() {
    let bd = load_data();
    let seg = |skill: &str, level: u32, q: u32| -> Vec<(String, f64)> {
        bd.effect_stats(skill, level, q, None)
            .quality
            .iter()
            .filter(|s| s.value != 0.0)
            .map(|s| (s.stat.clone(), s.value))
            .collect()
    };

    assert_eq!(
        seg("CometPlayer", 21, 20),
        vec![("base_spell_%_chance_to_echo".to_string(), 10.0)]
    );
    assert_eq!(
        seg("SparkPlayer", 21, 20),
        vec![("base_projectile_speed_+%".to_string(), 30.0)]
    );
    assert_eq!(
        seg("ArcticArmourPlayer", 19, 20),
        vec![(
            "maximum_number_of_arctic_armour_stationary_stacks".to_string(),
            1.0
        )]
    );
    // trunc evidence (oracle: q19 contribution is empty, trunc(0.95)=0).
    assert_eq!(seg("ArcticArmourPlayer", 19, 19), vec![]);
    // support effect: no entry in the quality table (skipped by PoB2's export condition) -> always an empty segment.
    assert_eq!(seg("SupportSpellEchoPlayer", 19, 20), vec![]);
}

/// q20 fixture (stormweaver-comet) end-to-end: XML quality parses into the
/// build model, plus a dual-run directional assertion (quality active vs
/// quality zeroed, DPS/defence must not decrease).
#[test]
fn stormweaver_comet_q20_fixture_dual_run() {
    let bd = load_data();
    let build = parse_build_from_code(stormweaver_code().trim()).expect("parse build");

    // XML parsing layer: the fixture has 15 quality=20 gems (per decoded.xml).
    let q20_count: usize = build
        .socket_groups
        .iter()
        .flat_map(|g| g.gem_skills.iter())
        .filter(|g| g.quality == 20)
        .count();
    assert_eq!(
        q20_count, 15,
        "all 15 q20 gems in decoded.xml should enter the build model"
    );

    // Dual run: a copy with quality zeroed out (everything else identical).
    let mut stripped = build.clone();
    for group in &mut stripped.socket_groups {
        group.active_gem_quality = Some(0);
        for gem in &mut group.gem_skills {
            gem.quality = 0;
        }
    }

    let with_q = calculate_with_data(&build, &bd, &opts()).expect("calc with quality");
    let no_q = calculate_with_data(&stripped, &bd, &opts()).expect("calc without quality");

    assert!(with_q.dps.is_finite() && with_q.life.is_finite());
    // Directional assertion: quality can only help (every quality stat in this build is positive), so DPS/defence must not decrease.
    assert!(
        with_q.dps >= no_q.dps,
        "DPS must not decrease once quality applies: {} < {}",
        with_q.dps,
        no_q.dps
    );
    assert!(
        with_q.life >= no_q.life && with_q.energy_shield >= no_q.energy_shield,
        "defence must not decrease once quality applies"
    );
}
