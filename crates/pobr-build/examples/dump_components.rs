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
    // POBR_DUMP_DEFENCE=1：附加防御/EHP 池分解（与 oracle mainOutput 的
    // <Type>TotalHitPool / LifeRecoverable / MaximumHitTaken 族对照）。
    if std::env::var("POBR_DUMP_DEFENCE").is_ok() {
        println!(
            "  life={:.2} life_unres={:.2} life_recoverable={:.2} es={:.2} es_cap={:.2} ward={:.2} mana={:.2} mana_unres={:.2}",
            out.life,
            out.life_unreserved,
            out.life_recoverable,
            out.energy_shield,
            out.energy_shield_recovery_cap,
            out.ward,
            out.mana,
            out.mana_unreserved,
        );
        for (i, m) in out.minions.iter().enumerate() {
            println!(
                "  minion[{i}]: level={} life={:.2} armour={:.2} evasion={:.2} es={:.2}",
                m.level, m.life, m.armour, m.evasion, m.energy_shield,
            );
        }
        println!(
            "  maxhit: phys={:.1} fire={:.1} cold={:.1} light={:.1} chaos={:.1} total_ehp={:.1}",
            out.physical_max_hit_pob2,
            out.fire_max_hit_pob2,
            out.cold_max_hit_pob2,
            out.lightning_max_hit_pob2,
            out.chaos_max_hit_pob2,
            out.total_ehp_pob2,
        );
    }
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
