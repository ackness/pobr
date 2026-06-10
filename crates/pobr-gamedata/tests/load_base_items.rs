use pobr_gamedata::{GameData, repo_data_root};

const VERSION: &str = "4.5.0.3.4";

fn game_data() -> GameData {
    GameData::new(repo_data_root().join(VERSION))
}

#[test]
fn manifest_describes_committed_bundle() {
    let manifest = game_data().manifest().expect("manifest 可加载");
    assert_eq!(manifest.poe_version, VERSION);
    assert_eq!(manifest.schema_version, 2);
    assert!(manifest.domains.base.iter().any(|d| d == "base_items"));
    assert!(manifest.languages.iter().any(|l| l == "zh-TW"));
}

#[test]
fn base_items_load_with_resolved_foreign_keys() {
    let bases = game_data().base_items().expect("base_items 可加载");
    assert!(bases.len() > 1000, "应有数千条基底，实得 {}", bases.len());

    let hatchet = bases
        .iter()
        .find(|b| b.name == "Dull Hatchet")
        .expect("存在 Dull Hatchet 基底");
    // 外键已解析为稳定字符串 ID（非整型索引）。
    assert_eq!(hatchet.item_class, "One Hand Axe");
    assert!(hatchet.id.starts_with("Metadata/Items/Weapons/"));
    assert!(hatchet.tags.iter().any(|t| t == "ezomyte_basetype"));

    // 占位条目（[DNT-UNUSED] 等）已被过滤。
    assert!(
        !bases.iter().any(|b| b.name.contains("[DNT")),
        "不应包含开发占位条目"
    );
}

#[test]
fn base_items_sorted_by_id_for_stable_diffs() {
    let bases = game_data().base_items().unwrap();
    let mut sorted = bases.clone();
    sorted.sort_by(|a, b| a.id.cmp(&b.id));
    assert_eq!(bases, sorted, "base_items.json 应按 id 排序");
}

#[test]
fn traditional_chinese_names_available_for_localization() {
    let names = game_data()
        .base_item_names("zh-TW")
        .expect("zh-TW 边车可加载");
    assert!(names.len() > 1000);
    // 磨刀石 = Blacksmith's Whetstone
    let whetstone_id = "Metadata/Items/Currency/CurrencyWeaponQuality";
    assert_eq!(names.get(whetstone_id).map(String::as_str), Some("磨刀石"));
}
