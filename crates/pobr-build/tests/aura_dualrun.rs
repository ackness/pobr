//! M3-T3 C5-1：aura 双跑 diff 报告（蓝图 m3-orchestration.md §1 D3 双跑点 2 / §6.5）。
//!
//! 对 ninja 18-build 跑两路：
//! - **旧路径**（现网行为）：`aura_buff_modifiers` 静态直注 + `mode_buffs = false`
//!   （buff_pass 整段空转，与 feature 关的默认产线逐值一致）；
//! - **新路径**（C5 切换目标）：feature `buff-pass-aura` 开 + `mode_buffs = true` +
//!   关静态直注（aura 经 BuffSpec → buff_pass 乘区，CalcPerform.lua:2085-2120；
//!   mode_buffs 同步激活 curse 路径 :2286-2337 + :2829-2896——EnemyCursed 条件与
//!   curse 面板字段属同一报告范围）。
//!
//! 逐 build 逐 stat diff 经 `pobr_core::extract_display_values`（全部 Computed
//! 展示字段）输出；报告固化在
//! `audits/rearchitecture-2026-06-10/blueprints/m3-c5-dualrun-report.md`。
//!
//! 运行：`cargo nextest run -p pobr-build --features buff-pass-aura \
//!   --test aura_dualrun --no-capture`
#![cfg(feature = "buff-pass-aura")]

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build_from_code};
use pobr_core::calc::{MinimalInput, OutputTable};
use pobr_core::extract_display_values;
use pobr_data::monster::EnemyTier;
use pobr_gamedata::{GameData, repo_data_root};
use std::path::{Path, PathBuf};

fn builds_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/demo-bd-test/builds")
        .canonicalize()
        .expect("builds dir exists")
}

fn discover_builds() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(builds_dir())
        .expect("read builds dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir() && p.join("code.txt").exists() && p.join("meta.json").exists())
        .collect();
    dirs.sort();
    dirs
}

/// ninja_parity 同口径的编排选项（`mode_effective = false` 面板口径），仅
/// `buff_pass_aura` 维度切换新旧路径。
fn opts(buff_pass_aura: bool) -> DataOrchestratorOptions {
    DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        enemy_level: 0,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: false,
        extra_modifier_texts: vec![],
        buff_pass_aura,
        ..Default::default()
    }
}

fn run_build(dir: &Path, data: &BuildData, buff_pass_aura: bool) -> Option<OutputTable> {
    let code = std::fs::read_to_string(dir.join("code.txt")).ok()?;
    let build = parse_build_from_code(code.trim()).ok()?;
    calculate_with_data(&build, data, &opts(buff_pass_aura)).ok()
}

/// 双跑 diff 报告（打印型仪表盘，照 `statmap_dual_run` / `ehp_dual_run_report`
/// 先例不设硬门禁——分类裁决在报告 md 落地）。同时输出 curse 面板新增字段
/// （`enemy_curse_limit` / `curse_slots`，旧路径恒 0/空）。
#[test]
fn aura_dualrun_report() {
    let data = {
        let gd = GameData::new(repo_data_root().join("4.5.0.3.4"));
        BuildData::load(&gd).expect("load BuildData")
    };
    let builds = discover_builds();
    assert!(!builds.is_empty(), "no builds discovered");

    let mut identical = 0usize;
    let mut changed = 0usize;
    eprintln!("\n========== M3-C5 aura 双跑 diff（旧静态直注 vs 新 buff_pass 乘区） ==========");
    for dir in &builds {
        let name = dir.file_name().unwrap().to_string_lossy();
        let (Some(old), Some(new)) = (run_build(dir, &data, false), run_build(dir, &data, true))
        else {
            eprintln!("\n##### {name} :: PARSE/CALC FAILED #####");
            continue;
        };
        let old_vals = extract_display_values(&old);
        let new_vals = extract_display_values(&new);
        assert_eq!(old_vals.len(), new_vals.len(), "display catalog 两路同列");
        let mut diffs = Vec::new();
        for (o, n) in old_vals.iter().zip(&new_vals) {
            assert_eq!(o.id, n.id);
            // 逐位比较（双跑应为确定性重算；NaN 双侧视为相等）。
            let both_nan = o.value.is_nan() && n.value.is_nan();
            if o.value != n.value && !both_nan {
                diffs.push((o.id.as_str().to_string(), o.value, n.value));
            }
        }
        if old.enemy_curse_limit != new.enemy_curse_limit || old.curse_slots != new.curse_slots {
            eprintln!(
                "\n##### {name} :: curse 面板（新增字段） old limit={} slots={:?} -> new limit={} slots={:?}",
                old.enemy_curse_limit, old.curse_slots, new.enemy_curse_limit, new.curse_slots
            );
        }
        if diffs.is_empty() {
            identical += 1;
            eprintln!("\n##### {name} :: 全列逐值持平 #####");
        } else {
            changed += 1;
            eprintln!("\n##### {name} :: {} 列差异 #####", diffs.len());
            eprintln!("  {:<34}{:>16}{:>16}{:>10}", "stat", "old", "new", "ratio");
            for (id, o, n) in &diffs {
                let ratio = if *o != 0.0 { n / o } else { f64::INFINITY };
                eprintln!("  {id:<34}{o:>16.4}{n:>16.4}{ratio:>10.4}");
            }
        }
    }
    eprintln!(
        "\n双跑汇总：{identical} build 逐值持平 / {changed} build 有差异（共 {}）",
        builds.len()
    );
}
