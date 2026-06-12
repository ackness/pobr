//! 技能 DoT golden fixture（M4-T4 W-D1）：essence-drain build（语料现成的纯 DoT
//! build）端到端走真实数据编排，对拍 PoB2 黄金值 `TotalDot`（meta.json::player_stats，
//! 与 tools/pob2-oracle 同源的 Lua 侧导出）。
//!
//! 当前覆盖与已知缺口（验收报告登记，详见 commit message / skill_dot.rs 模块文档）：
//! - `<Type>Dot` 基值链路（statmap `base_<type>_damage_to_deal_per_minute / 60`）
//!   ✅ 已通——`skill_total_dot` 首次非零。
//! - `modflags-pob2` 开（PoB2 全位表）：本 build TotalDot 对 golden **逐值命中
//!   （ratio = 1.0000）**——Swift Affliction `Damage MORE (Dot)` 经 DOT 位入桶。
//! - legacy 位表（默认 feature）：`Damage MORE (Dot)` 族整条 Unsupported（无 Dot
//!   位可译，宁可欠算），结构性欠算 ×1.30 → ratio = 0.7670。
//! - dotIsSpell 数据缺口：`.dat` 入库不含 value-less 布尔 stat
//!   （`spell_damage_modifiers_apply_to_skill_dot`），保守剥 Spell 位；本 build
//!   feature-on 逐值命中说明该缺口在此语料未表现，留待 dotIsSpell 消费 build 验证。
//!
//! 默认 feature 的销钉区间防倒退，位表切换 commit 落地后收紧为 5% 命中门。

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build_from_code};
use pobr_core::calc::MinimalInput;
use pobr_data::monster::EnemyTier;
use pobr_gamedata::{GameData, repo_data_root};
use std::path::{Path, PathBuf};

fn build_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/demo-bd-test/builds")
        .join(name)
        .canonicalize()
        .expect("build dir exists")
}

fn load_data() -> BuildData {
    let data = GameData::new(repo_data_root().join("4.5.0.3.4"));
    BuildData::load(&data).expect("load BuildData")
}

fn golden_stat(dir: &Path, key: &str) -> Option<f64> {
    let text = std::fs::read_to_string(dir.join("meta.json")).expect("read meta.json");
    let sanitized = text
        .replace("-Infinity", "-1e308")
        .replace("Infinity", "1e308")
        .replace("NaN", "0");
    let json: serde_json::Value = serde_json::from_str(&sanitized).expect("parse meta.json");
    json.get("player_stats")?.get(key)?.as_f64()
}

fn run_build(dir: &Path, data: &BuildData) -> pobr_core::calc::OutputTable {
    let code = std::fs::read_to_string(dir.join("code.txt")).expect("read code.txt");
    let build = parse_build_from_code(code.trim()).expect("parse build");
    let opts = DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        enemy_level: 0,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: true, // PoB2 主面板（golden 导出）恒 EFFECTIVE
        extra_modifier_texts: vec![],
        ..Default::default()
    };
    calculate_with_data(&build, data, &opts).expect("calculate")
}

/// essence-drain：技能 DoT 链路活体 + PoB2 oracle `TotalDot` 对拍（回归销钉）。
#[test]
fn essence_drain_skill_dot_vs_pob2_golden() {
    let dir = build_dir("sorceress-chronomancer-essence-drain");
    let data = load_data();
    let out = run_build(&dir, &data);
    let golden_total_dot = golden_stat(&dir, "TotalDot").expect("golden TotalDot");

    assert!(
        out.skill_total_dot > 0.0,
        "DoT 链路必须活体（<Type>Dot 基值 → instance → TotalDot）：{out:#?}"
    );
    let ratio = out.skill_total_dot / golden_total_dot;
    println!(
        "essence-drain TotalDot: pobr={:.2} golden={:.2} ratio={:.4}",
        out.skill_total_dot, golden_total_dot, ratio
    );

    // 回归销钉（feature 双态分别钉）：
    // - `modflags-pob2` 开（PoB2 全位表，DOT 位入桶）：实测 ratio = 1.0000
    //   （Swift Affliction `Damage MORE (Dot)` 经 DOT 位命中 dotCfg）→ 5% 命中门；
    // - legacy（默认）：`Damage MORE (Dot)` 族整条 Unsupported（结构性欠算
    //   ×1.30，文件头登记），实测 ratio = 0.7670 → 钉防倒退区间，位表切换后删除。
    #[cfg(feature = "modflags-pob2")]
    let pin = 0.95..=1.05;
    #[cfg(not(feature = "modflags-pob2"))]
    let pin = 0.70..=0.85;
    assert!(
        pin.contains(&ratio),
        "TotalDot 对 golden 比值超出销钉区间 {pin:?}：ratio={ratio:.4}（pobr={} golden={}）",
        out.skill_total_dot,
        golden_total_dot
    );

    // 合并族口径自检：WithDotDPS = TotalDPS + TotalDot；CombinedDPS ≥ WithDotDPS。
    assert!(
        (out.with_dot_dps - (out.dps + out.skill_total_dot)).abs() < 1e-6,
        "WithDotDPS 恒等式失效：{out:#?}"
    );
    assert!(
        out.total_dot_dps >= out.skill_total_dot,
        "TotalDotDPS 必须 ≥ 技能 TotalDot：{out:#?}"
    );
}

/// 非 DoT build（comet）：技能 DoT 契约字段保持中性零（skill dot 不误触发），
/// 合并族字段满足组合恒等式。
#[test]
fn non_dot_build_keeps_skill_dot_neutral() {
    let dir = build_dir("sorceress-stormweaver-comet");
    let data = load_data();
    let out = run_build(&dir, &data);
    assert_eq!(
        out.skill_dot_instance, 0.0,
        "非 DoT 技能不得产生技能 DoT instance：{out:#?}"
    );
    assert_eq!(out.skill_total_dot, 0.0);
    assert_eq!(out.with_dot_dps, 0.0, "无技能 dot：WithDotDPS 维持中性");
    // CombinedDPS = TotalDPS + TotalDotDPS（异常 DoT 仍并入合并族）。
    assert!(
        (out.combined_dps - (out.dps + out.total_dot_dps)).abs() < 1e-6,
        "CombinedDPS 恒等式失效：{out:#?}"
    );
}
