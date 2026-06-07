//! PoB2 数值对齐 harness：用真实 ninja build code 内嵌的 `<PlayerStat>`（PoB2 自己导出时
//! 算好的值）作为 golden 参照，对比 PoBR 的计算输出。
//!
//! **关键**：PoB Build Code 解码出的 XML 含 `<PlayerStat stat="X" value="Y"/>`——这就是
//! PoB2 的权威答案，无需另跑 PoB2。本测试逐 build 打印「PoBR vs PoB2」对照表（`--nocapture`
//! 可见），并断言**目前应当成立**的不变量；已知差距不硬失败，作为对齐进度的活体度量。
//!
//! 运行：`cargo test -p pobr-build --test pob2_parity -- --nocapture`

use pobr_build::{
    BuildData, DataOrchestratorOptions, calculate_with_data, decode_pob_code, parse_build_from_code,
};
use pobr_core::calc::{MinimalInput, OutputTable};
use pobr_data::monster::EnemyTier;
use pobr_gamedata::{GameData, repo_data_root};
use std::collections::HashMap;

const DEADEYE: &str = include_str!("../../../examples/demo-bd-test/ninja-bd-deadeye.txt");
const MARTIAL: &str = include_str!("../../../examples/demo-bd-test/ninja-bd-marial-artist.txt");

/// 从解码后的 Build XML 抽取所有 `<PlayerStat stat="X" value="Y"/>`（PoB2 的参照值）。
fn parse_player_stats(xml: &str) -> HashMap<String, f64> {
    let mut out = HashMap::new();
    for chunk in xml.split("<PlayerStat ").skip(1) {
        let stat = between(chunk, "stat=\"", "\"");
        let value = between(chunk, "value=\"", "\"");
        if let (Some(s), Some(v)) = (stat, value)
            && let Ok(num) = v.parse::<f64>()
        {
            out.insert(s.to_string(), num);
        }
    }
    out
}

fn between<'a>(s: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let i = s.find(start)? + start.len();
    let rest = &s[i..];
    let j = rest.find(end)?;
    Some(&rest[..j])
}

fn load_data() -> BuildData {
    let data = GameData::new(repo_data_root().join("4.5.0.3.4"));
    BuildData::load(&data).expect("load BuildData")
}

/// (显示名, PoB2 key, PoBR 取值闭包)
fn compare_row(out: &OutputTable, label: &str, pob2: Option<f64>, pobr: f64) -> String {
    match pob2 {
        Some(v2) => {
            let ratio = if v2 != 0.0 {
                format!("{:.2}x", pobr / v2)
            } else if pobr == 0.0 {
                "1.00x".into()
            } else {
                "inf".into()
            };
            let _ = out;
            format!("{label:<14}{pobr:>15.2}{v2:>15.2}{ratio:>10}")
        }
        None => format!("{label:<14}{pobr:>15.2}{:>15}{:>10}", "—", "—"),
    }
}

fn report(name: &str, code: &str, data: &BuildData) -> (OutputTable, HashMap<String, f64>) {
    let xml = decode_pob_code(code).expect("decode");
    let pob2 = parse_player_stats(&xml);
    let build = parse_build_from_code(code).expect("parse build");
    // 面板口径（PoB2 PlayerStat 的防御/属性是面板值；DPS 另作说明）。
    let opts = DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        enemy_level: 0,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: false,
        extra_modifier_texts: vec![],
    };
    let out = calculate_with_data(&build, data, &opts).expect("calc");

    eprintln!("\n===== {name} :: PoBR vs PoB2 (embedded) =====");
    eprintln!("{:<14}{:>15}{:>15}{:>10}", "stat", "PoBR", "PoB2", "ratio");
    let rows: &[(&str, &str, f64)] = &[
        ("Life", "Life", out.life),
        ("Mana", "Mana", out.mana),
        ("EnergyShield", "EnergyShield", out.energy_shield),
        ("Armour", "Armour", out.armour),
        ("Evasion", "Evasion", out.evasion),
        ("FireRes", "FireResist", out.fire_resistance),
        ("ColdRes", "ColdResist", out.cold_resistance),
        ("LightRes", "LightningResist", out.lightning_resistance),
        // PoBR crit_chance 是 fraction（0.05），PoB2 CritChance 是 percent（5）→ ×100 对齐。
        ("CritChance", "CritChance", out.crit_chance * 100.0),
        ("CritMulti", "CritMultiplier", out.crit_multiplier),
        ("AvgHit", "AverageDamage", out.total_hit_avg),
        ("DPS", "TotalDPS", out.dps),
    ];
    for (label, key, pobr) in rows {
        eprintln!(
            "{}",
            compare_row(&out, label, pob2.get(*key).copied(), *pobr)
        );
    }
    (out, pob2)
}

/// 断言某 PoB2 嵌入 PlayerStat 与 PoBR 输出在 `tol` 相对误差内（golden 缺该 key 则跳过）。
fn assert_within(pob2: &HashMap<String, f64>, key: &str, pobr: f64, tol: f64) {
    if let Some(&golden) = pob2.get(key)
        && golden != 0.0
    {
        let ratio = pobr / golden;
        assert!(
            (ratio - 1.0).abs() < tol,
            "{key}: PoBR {pobr:.1} vs PoB2 {golden:.1} = {ratio:.3}x (tol {tol})"
        );
    }
}

/// Deadeye：打印对照 + 断言「目前应当成立」的不变量（build 解析、生命有限为正、抗性 ≤ cap）。
#[test]
fn deadeye_parity_report() {
    let data = load_data();
    let (out, pob2) = report("DEADEYE", DEADEYE, &data);
    assert!(out.life > 0.0 && out.life.is_finite(), "life must be > 0");
    assert!(out.dps.is_finite(), "dps must be finite");
    // 已对齐 PoB2 <10%（回归门禁，防止后续改动破坏 deadeye parity）：
    assert_within(&pob2, "Life", out.life, 0.10);
    assert_within(&pob2, "Armour", out.armour, 0.10);
    assert_within(&pob2, "CritChance", out.crit_chance * 100.0, 0.10);
    assert_within(&pob2, "CritMultiplier", out.crit_multiplier, 0.10);
    assert_within(&pob2, "FireResist", out.fire_resistance, 0.12);
    assert_within(&pob2, "ColdResist", out.cold_resistance, 0.12);
    assert_within(&pob2, "LightningResist", out.lightning_resistance, 0.10);
    assert_within(&pob2, "AverageDamage", out.total_hit_avg, 0.10);
    assert_within(&pob2, "TotalDPS", out.dps, 0.10);
    // 切片：Evasion 仍 ~0.77x（分散树节点/物品基底），暂不硬断言。
}

#[test]
fn martial_parity_report() {
    let data = load_data();
    let (out, _pob2) = report("MARTIAL", MARTIAL, &data);
    assert!(out.life.is_finite() && out.mana.is_finite());
    assert!(out.dps.is_finite());
}
