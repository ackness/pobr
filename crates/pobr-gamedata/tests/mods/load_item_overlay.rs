//! pre-M5c 数据前置的加载测试：`overlay/mod_scalability.json` /
//! `overlay/catalysts.json` / `overlay/runes.json` / `overlay/uniques.json`
//! （M5c 蓝图 WI-B1 的抽样断言，vendor 行号注明来源，commit `2df5a74`）。

use pobr_gamedata::{GameData, repo_data_root};

fn version() -> String {
    pobr_gamedata::data_version()
}

fn game_data() -> GameData {
    GameData::new(repo_data_root().join(version()))
}

/// mod_scalability：条目量级 + 模板升序 +
/// `"# Armour per 2 Strength"`（ModScalability.lua:10）与
/// `"#"`（:9，formats=passive_keystone_index）抽查。
#[test]
fn mod_scalability_samples() {
    let def = game_data()
        .mod_scalability()
        .unwrap()
        .expect("mod_scalability.json 在库");
    assert!(
        def.entries.len() >= 15000,
        "ModScalability.lua 约 15037 行条目（实测 {}）",
        def.entries.len()
    );
    assert!(
        def.entries
            .windows(2)
            .all(|w| w[0].template < w[1].template),
        "template 严格升序（key 唯一）"
    );
    let find = |t: &str| {
        def.entries
            .iter()
            .find(|e| e.template == t)
            .unwrap_or_else(|| panic!("模板 {t:?} 缺失"))
    };
    let armour = find("# Armour per 2 Strength");
    assert_eq!(armour.slots.len(), 1);
    assert!(armour.slots[0].is_scalable);
    assert!(armour.slots[0].formats.is_empty());
    let keystone = find("#");
    assert_eq!(keystone.slots[0].formats, ["passive_keystone_index"]);
    // isScalable=false 路径（ModScalability.lua:14 Avatar of Fire 行）
    let unscalable = find("# Armour while you do not have Avatar of Fire");
    assert!(!unscalable.slots[0].is_scalable);
}

/// catalysts：12 条平行数组合并（Classes/Item.lua:14-29），第 3 条 =
/// Carapace / Defence / {defences, armour, evasion, energyshield}
/// （M5c 蓝图 WI-B1 指定抽样）。
#[test]
fn catalysts_twelve_entries() {
    let def = game_data()
        .catalysts()
        .unwrap()
        .expect("catalysts.json 在库");
    assert_eq!(def.catalysts.len(), 12);
    for (i, c) in def.catalysts.iter().enumerate() {
        assert_eq!(c.id as usize, i + 1, "id 1-based 连续");
    }
    let third = &def.catalysts[2];
    assert_eq!(third.name, "Carapace");
    assert_eq!(third.descriptor, "Defence");
    assert_eq!(
        third.tags,
        ["defences", "armour", "evasion", "energyshield"]
    );
    // 首尾抽查（Item.lua:14-15）
    assert_eq!(def.catalysts[0].name, "Flesh");
    assert_eq!(def.catalysts[0].descriptor, "Life");
    assert_eq!(def.catalysts[11].name, "Adaptive");
    assert_eq!(def.catalysts[11].descriptor, "Attribute");
}

/// runes：Hayoxi's Soul Core helmet 行（ModRunes.lua:5-13）与
/// Desert Rune weapon 槽（:660-666）抽查。
#[test]
fn runes_samples() {
    let def = game_data().runes().unwrap().expect("runes.json 在库");
    assert!(def.runes.len() >= 250, "符文条目量级（实测 283）");
    let find = |name: &str| {
        def.runes
            .iter()
            .find(|r| r.name == name)
            .unwrap_or_else(|| panic!("符文 {name:?} 缺失"))
    };
    let hayoxi = find("Hayoxi's Soul Core of Heatproofing");
    let helmet = &hayoxi.slots["helmet"];
    assert_eq!(helmet.kind, "SoulCore");
    assert_eq!(helmet.lines, ["+40% of Armour also applies to Cold Damage"]);
    assert_eq!(helmet.rank, [50.0]);
    let desert = find("Desert Rune");
    let weapon = &desert.slots["weapon"];
    assert_eq!(weapon.kind, "Rune");
    assert_eq!(
        weapon.lines,
        [
            "Adds 7 to 11 Fire Damage",
            "Bonded: 30% increased Ignite Magnitude"
        ]
    );
    assert_eq!(weapon.stat_order, [831.0, 1076.0]);
}

/// uniques：P15 双层——The Anvil（Data/Uniques/amulet.lua:5-15）的 raw 块
/// 逐字节保留 + 预解析索引（name/base/variants）。
#[test]
fn uniques_double_layer() {
    let def = game_data().uniques().unwrap().expect("uniques.json 在库");
    assert!(def.uniques.len() >= 350, "传奇条目量级（实测 392）");
    let anvil = def
        .uniques
        .iter()
        .find(|u| u.name == "The Anvil")
        .expect("The Anvil 在库");
    assert_eq!(anvil.base, "Bloodstone Amulet");
    assert_eq!(anvil.item_type, "amulet");
    assert_eq!(anvil.variants, ["Pre 0.2.0", "Pre 0.4.0", "Current"]);
    // raw 层逐字节保留 vendor 标注（{tags:...} 与 (min-max) 区间形态）
    assert!(anvil.raw.contains("{tags:life}+(30-40) to maximum Life"));
    assert!(anvil.raw.contains("Variant: Pre 0.2.0"));
    // 索引层最小化：League/Source 行（存在则收录）
    assert!(
        def.uniques.iter().any(|u| u.league.is_some()),
        "League 行预解析存在"
    );
    assert!(
        def.uniques.iter().any(|u| u.source.is_some()),
        "Source 行预解析存在"
    );
    // item_type 覆盖 vendor itemTypes 主要域
    for ty in ["amulet", "body", "helmet", "ring", "bow", "flask", "jewel"] {
        assert!(
            def.uniques.iter().any(|u| u.item_type == ty),
            "item_type {ty} 缺失"
        );
    }
}

/// 缺表容忍（R7）：空目录下四个新域返回 Ok(None) 不 panic。
#[test]
fn missing_overlay_files_yield_none() {
    let dir = std::env::temp_dir().join(format!("pobr-pre-m5c-missing-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let data = GameData::new(&dir);
    assert!(data.mod_scalability().unwrap().is_none());
    assert!(data.catalysts().unwrap().is_none());
    assert!(data.runes().unwrap().is_none());
    assert!(data.uniques().unwrap().is_none());
}
