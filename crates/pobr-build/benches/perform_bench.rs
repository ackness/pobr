//! 完整编排热路径基准（M4 T0/W-F1，蓝图 m4-offence-deep.md §2-T0）。
//!
//! 双 pass（2 hand × 2 crit）让进攻热路径计算量理论 ×4——本基准是 R6 风险的
//! 量化闸门：**M4 结束时单 build `calculate_with_data` 耗时 ≤ M4 开始基线的
//! 2.5×**；超预算必须做惰性短路（非双持跳 OffHand、无暴击词条短路 crit pass），
//! 且短路自带等价性测试。
//!
//! 运行：`cargo bench -p pobr-build --bench perform_bench`。
//! CI 不跑 criterion（时长）；门禁为合并前手动跑 + 结果贴 PR，与
//! `mod_db_bench` 惯例一致。基线记录流程见 `devs/scripts/bench-baseline.md`。
//!
//! 语料选择（对蓝图「1 个双持攻击 build」的偏离说明）：ninja 18-build 集中无
//! 严格双持（dual-wield）build——取攻击侧最重的 monk flicker-strike（夺标
//! 双 pass 的 hand/crit 主路径）+ 法术对照 sorceress comet（crit pass 路径、
//! 无 hand pass 分叉），双持 fixture 落地（W-B2）后可补真双持 case。

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
    // 与 ninja_parity 默认口径一致（effective，M3-W5 切换后的主口径）。
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
        let game = GameData::new(repo_data_root().join("4.5.0.3.4"));
        BuildData::load(&game).expect("load BuildData")
    };
    let attack = load_build("monk-martial-artist-flicker-strike");
    let spell = load_build("sorceress-stormweaver-comet");
    let opts = options();

    // 攻击 build：hand pass + crit pass 的主受益/受损路径。
    c.bench_function("perform_attack_flicker", |b| {
        b.iter(|| {
            black_box(calculate_with_data(black_box(&attack), &data, &opts))
                .expect("calculate attack build")
        })
    });

    // 法术 build 对照：无 hand 分叉，隔离 crit pass 的单独贡献。
    c.bench_function("perform_spell_comet", |b| {
        b.iter(|| {
            black_box(calculate_with_data(black_box(&spell), &data, &opts))
                .expect("calculate spell build")
        })
    });
}

criterion_group!(benches, bench_perform);
criterion_main!(benches);
