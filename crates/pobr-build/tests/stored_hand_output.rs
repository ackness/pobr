//! M4-T2 W-B3：`Stored<Type>*` 族（HandOutput）真实 build 对拍。
//!
//! vendor `CalcOffence.lua:4047-4057`：`Stored<Type>CritAvg/HitAvg/CombinedAvg`
//! 是 ailment magnitude 的输入，必须满足两条结构恒等式（oracle 亲验，2026-06-12，
//! vendor 0.18.0，`tools/pob2-oracle/run.sh` 对三个 build 的 `mainHandOutput` dump）：
//!
//! 1. `StoredCritAvg == StoredHitAvg × CritMultiplier`（无 CriticalStrike 条件词条时；
//!    oracle flicker-strike：Physical 7524.775 == 1551.5 × 4.85 ✓、
//!    bow-shot：Physical 39061.245 == 8938.5 × 4.37 ✓、
//!    twister：Cold 17750.4 == 3440 × 5.16 ✓）。
//! 2. `StoredCombinedAvg == CritAvg×c + HitAvg×(1−c)`（c = 该手 CritChance；
//!    oracle flicker-strike：Physical 6549.84 == 7524.775×0.836784 + 1551.5×0.163216 ✓）。
//!
//! 绝对值逐位对拍属整体伤害 parity 工程（当前进攻 @10% ≈44%，ninja_parity 门禁
//! 渐进收敛）；本测试钉死 PoBR 侧与 vendor 同构的**结构恒等式**在 ≥3 个真实 build
//! 上成立 + Stored 族非空落 HandOutput（ailment 链不断的前提）。

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
        let game_data = GameData::new(repo_data_root().join("4.5.0.3.4"));
        BuildData::load(&game_data).expect("load BuildData")
    };

    for name in BUILDS {
        let out = run_build(&builds_dir().join(name), &data);
        let hand = out
            .main_hand
            .as_ref()
            .unwrap_or_else(|| panic!("{name}: 攻击 build 必须有 main_hand 子表"));

        assert!(
            !hand.stored_combined_avg.is_empty(),
            "{name}: Stored 族必须落 HandOutput（ailment 链输入）"
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
                // 恒等式 1（无 CriticalStrike 条件词条 → 两腿同输入，crit 腿 ×m）。
                let ratio = crit_avg / hit_avg;
                assert!(
                    (ratio - m).abs() < 1e-6,
                    "{name} {ty:?}: CritAvg/HitAvg={ratio} 应 == CritMultiplier={m}"
                );
            }
            // 恒等式 2（vendor :4048/:4053 的加权累计）。
            let blend = crit_avg * c + hit_avg * (1.0 - c);
            assert!(
                (combined - blend).abs() < 1e-6 * blend.abs().max(1.0),
                "{name} {ty:?}: CombinedAvg={combined} 应 == blend={blend}"
            );
        }
        assert!(any_nonzero, "{name}: 至少一个伤害类型非零");
    }
}
