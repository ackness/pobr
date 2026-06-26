//! 多版本数据无关性 smoke——证明 calc **不强绑定某个数据版本**。
//!
//! 对 `data/` 下**每个**已入库版本目录，独立 `BuildData::load` + 同一真实 build
//! 端到端计算，断言关键输出有限、非负、life>0。golden 数值是版本特定的（见
//! `pobr_data::GOLDEN_PARITY_DATA_VERSION`，由 ninja_parity 钉定校验），故本 smoke
//! **只断言"能在该版本数据上正确装配并算出合理量纲"**，不比对黄金值——这正是
//! 「数据/计算分离」的回归保证：活动版本前进或新增版本目录时，calc 仍能在每个版本跑通。

use std::fs;

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build_from_code};
use pobr_core::calc::MinimalInput;
use pobr_data::monster::EnemyTier;
use pobr_gamedata::{GameData, repo_data_root};

const DEADEYE_CODE: &str = include_str!("../../../../examples/demo-bd-test/ninja-bd-deadeye.txt");

/// 发现 `data/` 下全部已入库版本目录（名首字符为数字 + 含 manifest.json），字典序。
fn committed_data_versions() -> Vec<String> {
    let root = repo_data_root();
    let mut versions: Vec<String> = fs::read_dir(&root)
        .expect("read data root")
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let is_ver = name.chars().next().is_some_and(|c| c.is_ascii_digit());
            (is_ver && root.join(&name).join("manifest.json").is_file()).then_some(name)
        })
        .collect();
    versions.sort();
    versions
}

#[test]
fn calc_runs_on_every_committed_data_version() {
    let versions = committed_data_versions();
    assert!(
        versions.len() >= 2,
        "期望 ≥2 个已入库数据版本以覆盖多版本无关性，实得 {versions:?}"
    );

    let build = parse_build_from_code(DEADEYE_CODE).expect("parse deadeye build");

    for ver in &versions {
        let data = BuildData::load(&GameData::new(repo_data_root().join(ver)))
            .unwrap_or_else(|e| panic!("BuildData::load 在版本 {ver} 失败：{e}"));

        let opts = DataOrchestratorOptions {
            base_input: MinimalInput::default(),
            inject_character_base: true,
            enemy_level: 80,
            enemy_tier: EnemyTier::Pinnacle,
            mode_effective: true,
            ..Default::default()
        };
        let out = calculate_with_data(&build, &data, &opts)
            .unwrap_or_else(|e| panic!("calc 在版本 {ver} 失败：{e}"));

        assert!(
            out.life.is_finite() && out.life > 0.0,
            "版本 {ver}: life 非法（{}）——CharacterBase/装备/天赋应注入正值",
            out.life
        );
        for (label, v) in [
            ("mana", out.mana),
            ("armour", out.armour),
            ("evasion", out.evasion),
            ("energy_shield", out.energy_shield),
            ("combined_dps", out.combined_dps),
        ] {
            assert!(v.is_finite() && v >= 0.0, "版本 {ver}: {label} 非法（{v}）");
        }
        assert!(
            (0.0..=1.0).contains(&out.hit_chance),
            "版本 {ver}: hit_chance 越界（{}）",
            out.hit_chance
        );
        eprintln!(
            "[multi-version] {ver}: life={:.0} dps={:.0} hit={:.2} ✓",
            out.life, out.combined_dps, out.hit_chance
        );
    }
}

/// 确定性跨版本：同版本两次加载+计算结果一致（数据加载无随机/遍历序依赖）。
#[test]
fn calc_is_deterministic_per_version() {
    let build = parse_build_from_code(DEADEYE_CODE).expect("parse");
    let opts = DataOrchestratorOptions {
        inject_character_base: true,
        mode_effective: false,
        ..Default::default()
    };
    for ver in committed_data_versions() {
        let load = || {
            let d = BuildData::load(&GameData::new(repo_data_root().join(&ver))).expect("load");
            calculate_with_data(&build, &d, &opts).expect("calc")
        };
        let (a, b) = (load(), load());
        assert_eq!(a.life, b.life, "版本 {ver}: life 非确定性");
        assert_eq!(a.combined_dps, b.combined_dps, "版本 {ver}: dps 非确定性");
    }
}
