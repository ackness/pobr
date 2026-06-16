use pobr_gamedata::{GameData, repo_data_root};

fn version() -> String {
    pobr_gamedata::data_version()
}

fn game_data() -> GameData {
    GameData::new(repo_data_root().join(version()))
}

#[test]
fn manifest_describes_committed_bundle() {
    let manifest = game_data().manifest().expect("manifest 可加载");
    assert_eq!(manifest.poe_version, version());
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

/// M4-T4 W-D2：弩装填时间经 overlay merge 进 weapon 段（vendor
/// `Data/Bases/crossbow.lua` Makeshift Crossbow `ReloadTimeBase = 0.8`），
/// 非弩武器保持 `None`。
#[test]
fn crossbow_reload_time_merged_from_overlay() {
    let bases = game_data().base_items().expect("base_items 可加载");
    let crossbow = bases
        .iter()
        .find(|b| b.name == "Makeshift Crossbow")
        .expect("存在 Makeshift Crossbow 基底");
    let weapon = crossbow.weapon.as_ref().expect("弩必有 weapon 段");
    assert_eq!(weapon.reload_time_ms, Some(800));
    assert!(weapon.physical_min > 0, "merge 不得扰动既有 weapon 数值");

    let hatchet = bases.iter().find(|b| b.name == "Dull Hatchet").unwrap();
    assert_eq!(
        hatchet.weapon.as_ref().and_then(|w| w.reload_time_ms),
        None,
        "非弩武器无 reload"
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
