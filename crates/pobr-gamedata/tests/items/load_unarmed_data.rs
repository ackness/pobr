//! `base/unarmed_data.json` 加载测试：逐值对照 pobr 现有 Rust 准源
//! `pobr-build::calc_orchestrator::unarmed_contribution`（搬迁不变式），
//! vendor-only 字段（class_id / weapon_type）按 vendor 行号抽样断言。

use pobr_gamedata::{GameData, repo_data_root};

fn version() -> String {
    pobr_gamedata::data_version()
}

fn game_data() -> GameData {
    GameData::new(repo_data_root().join(version()))
}

/// pobr 准源（`calc_orchestrator.rs::unarmed_contribution`）的逐职业期望值：
/// `(phys_min, phys_max, attack_rate, crit_chance)`。phys_max 按 class_name match
/// （Warrior 8、Scion/Mercenary/Druid 6、其余 5），其余三项全职业同值。
fn rust_source_expectation(class_name: &str) -> (f64, f64, f64, f64) {
    let phys_max = match class_name {
        "Warrior" => 8.0,
        "Scion" | "Mercenary" | "Druid" => 6.0,
        _ => 5.0, // Witch/Ranger/Sorceress/Huntress/Monk
    };
    (2.0, phys_max, 1.65, 0.05)
}

/// 全表逐值等于 pobr 现有 Rust 表（搬迁不变式：commit 内数值不变）。
#[test]
fn values_match_existing_rust_table() {
    let entries = game_data().unarmed_data().expect("unarmed_data 可加载");
    assert_eq!(entries.len(), 9, "vendor unarmedWeaponData 共 9 个职业条目");

    for e in &entries {
        let (min, max, rate, crit) = rust_source_expectation(&e.class_name);
        assert_eq!(e.physical_min, min, "{} physical_min", e.class_name);
        assert_eq!(e.physical_max, max, "{} physical_max", e.class_name);
        assert_eq!(e.attack_rate, rate, "{} attack_rate", e.class_name);
        // pobr 现值 0.05（vendor 为百分数 5，出入已记 schema TODO(parity)，本任务不改值）。
        assert_eq!(e.crit_chance, crit, "{} crit_chance", e.class_name);
    }
}

/// vendor-only 字段抽样断言（vendor `src/Modules/Data.lua:554-562`：classId 表键 +
/// 行尾职业注释；`type = "None"`）。
#[test]
fn vendor_only_fields_sampled() {
    let entries = game_data().unarmed_data().unwrap();

    // classId → 职业名 全集（Data.lua:554-562 行尾注释；PoE2 跳号 3/4/5 不存在）。
    let expected_ids: &[(u32, &str)] = &[
        (0, "Scion"),
        (1, "Witch"),
        (2, "Ranger"),
        (6, "Warrior"),
        (7, "Sorceress"),
        (8, "Huntress"),
        (9, "Mercenary"),
        (10, "Monk"),
        (11, "Druid"),
    ];
    let actual: Vec<(u32, &str)> = entries
        .iter()
        .map(|e| (e.class_id, e.class_name.as_str()))
        .collect();
    assert_eq!(
        actual, expected_ids,
        "classId↔职业名应与 vendor 一致且按 class_id 升序"
    );

    // 抽样：Warrior（Data.lua:557）PhysicalMax = 8；type = "None"。
    let warrior = entries.iter().find(|e| e.class_id == 6).unwrap();
    assert_eq!(warrior.class_name, "Warrior");
    assert_eq!(warrior.weapon_type, "None");
    assert_eq!(warrior.physical_max, 8.0);

    // 抽样：Witch（Data.lua:555）PhysicalMax = 5、AttackRate = 1.65。
    let witch = entries.iter().find(|e| e.class_id == 1).unwrap();
    assert_eq!(witch.physical_max, 5.0);
    assert_eq!(witch.attack_rate, 1.65);

    // weapon_type 全表为 "None"（每行 `type = "None"`）。
    assert!(entries.iter().all(|e| e.weapon_type == "None"));
}

/// 数组按 class_id 升序（diff 友好约定，对齐其它 base 表的排序口径）。
#[test]
fn sorted_by_class_id_for_stable_diffs() {
    let entries = game_data().unarmed_data().unwrap();
    let mut sorted = entries.clone();
    sorted.sort_by_key(|e| e.class_id);
    assert_eq!(entries, sorted, "unarmed_data.json 应按 class_id 排序");
}
