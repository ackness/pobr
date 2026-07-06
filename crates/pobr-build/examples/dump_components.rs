//! parity 调试工具：按 demo build 名 dump PoBR 侧 damage_components 逐分量分解
//! （min/max/avg/type_path），与 `tools/pob2-oracle` 的 vendor 侧
//! `<Type>Stored{Hit,Crit}{Min,Max}` / `<Type>SummedBase` 对照定位逐类型偏差
//! （deadeye gain-as fallback 根因即由此钉出）。harness 同口径
//! （Pinnacle / enemy_level 0 / effective）。
//! 用法: cargo run -p pobr-build --example dump_components [build-dir-name]

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build_from_code};
use pobr_core::calc::MinimalInput;
use pobr_data::monster::EnemyTier;
use pobr_gamedata::{GameData, repo_data_root};
use std::path::Path;

fn main() {
    let name = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "ranger-deadeye-explosive-grenade".into());
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/demo-bd-test/builds")
        .join(&name);
    let data = GameData::new(repo_data_root().join(pobr_data::GOLDEN_PARITY_DATA_VERSION));
    let data = BuildData::load(&data).expect("load BuildData");
    let code = std::fs::read_to_string(dir.join("code.txt")).expect("code.txt");
    let build = parse_build_from_code(code.trim()).expect("parse code");
    let opts = DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        enemy_level: 0,
        enemy_tier: EnemyTier::Pinnacle,
        mode_effective: true,
        extra_modifier_texts: vec![],
        ..Default::default()
    };
    let out = calculate_with_data(&build, &data, &opts).expect("calc");
    println!("== {name} ==");
    println!(
        "total_hit_avg={:.2} dps={:.2} crit_chance={:.4} crit_multi={:.4} action_rate={:.4} hit_chance={:.4}",
        out.total_hit_avg,
        out.dps,
        out.crit_chance,
        out.crit_multiplier,
        out.action_rate,
        out.hit_chance
    );
    let mut sum = 0.0;
    for c in &out.damage_components {
        let avg = c.avg();
        sum += avg;
        println!(
            "  [{:?}] min={:.2} max={:.2} avg={:.2} path={:?} kind={:?} src={:?}",
            c.damage_type, c.min, c.max, avg, c.type_path, c.kind, c.source,
        );
    }
    println!("  sum(components avg) = {sum:.2}");
}
