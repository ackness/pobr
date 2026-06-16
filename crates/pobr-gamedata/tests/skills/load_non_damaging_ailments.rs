//! `base/non_damaging_ailments.json` 加载测试。
//!
//! 搬迁不变式（架构文档 20 §1.1）：凡 pobr 现有 Rust 准源已有的数值，JSON
//! 必须与之逐值相等——chill/shock 边界对照 `pobr_data::monster` 与
//! `pobr_data::constants` 的 pub 常量；伤害型异常 DoT 类型对照
//! `AilmentType::damage_type()`。pobr 没有的字段（vendor-only）按
//! vendor PoB2 行号写死期望值抽样断言。

use pobr_data::constants::{AilmentType, DamageType, SHOCK_MIN_EFFECT};
use pobr_data::monster::{
    BASE_SHOCK_MAGNITUDE, CHILL_MAX_EFFECT, CHILL_MIN_EFFECT, SHOCK_MAX_EFFECT,
};
use pobr_gamedata::{GameData, repo_data_root};

fn version() -> String {
    pobr_gamedata::data_version()
}

fn game_data() -> GameData {
    GameData::new(repo_data_root().join(version()))
}

/// chill/shock 边界逐值等于 pobr Rust 准源常量（搬迁不变式核心断言）。
#[test]
fn chill_and_shock_bounds_match_rust_source_constants() {
    let table = game_data()
        .non_damaging_ailments()
        .expect("non_damaging_ailments 可加载");

    let chill = table.ailments.get("Chill").expect("存在 Chill");
    // 准源：pobr-data/src/monster.rs CHILL_MIN_EFFECT=30 / CHILL_MAX_EFFECT=50。
    assert_eq!(chill.default, Some(CHILL_MIN_EFFECT));
    assert_eq!(chill.min, CHILL_MIN_EFFECT);
    assert_eq!(chill.max, CHILL_MAX_EFFECT);

    let shock = table.ailments.get("Shock").expect("存在 Shock");
    // 准源：pobr-data/src/monster.rs BASE_SHOCK_MAGNITUDE=20 / SHOCK_MAX_EFFECT=100；
    // pobr-data/src/constants.rs SHOCK_MIN_EFFECT=20（两常量同值，均为准源）。
    assert_eq!(shock.default, Some(BASE_SHOCK_MAGNITUDE));
    assert_eq!(shock.min, BASE_SHOCK_MAGNITUDE);
    assert_eq!(shock.min, SHOCK_MIN_EFFECT);
    assert_eq!(shock.max, SHOCK_MAX_EFFECT);
}

/// vendor-only 字段抽样断言（pobr 无准源，期望值写死，引用 vendor 行号）。
#[test]
fn vendor_only_fields_match_pob2_data_lua() {
    let table = game_data().non_damaging_ailments().unwrap();
    assert_eq!(table.ailments.len(), 3, "nonDamagingAilment 恰三项");

    // Modules/Data.lua:348（Chill 行）+ Data/Misc.lua:91 BaseChillDuration=8。
    let chill = &table.ailments["Chill"];
    assert_eq!(chill.associated_type, DamageType::Cold);
    assert!(!chill.alt);
    assert_eq!(chill.precision, 0);
    assert_eq!(chill.duration, 8.0);

    // Modules/Data.lua:349（Freeze 行，default=nil/min=0.3/max=3/precision=2）
    // + Data/Misc.lua:56 FreezeDuration=4。
    let freeze = &table.ailments["Freeze"];
    assert_eq!(freeze.associated_type, DamageType::Cold);
    assert!(!freeze.alt);
    assert_eq!(freeze.default, None);
    assert_eq!(freeze.min, 0.3);
    assert_eq!(freeze.max, 3.0);
    assert_eq!(freeze.precision, 2);
    assert_eq!(freeze.duration, 4.0);

    // Modules/Data.lua:350（Shock 行）+ Data/Misc.lua:93 BaseShockDuration=8。
    let shock = &table.ailments["Shock"];
    assert_eq!(shock.associated_type, DamageType::Lightning);
    assert!(!shock.alt);
    assert_eq!(shock.precision, 0);
    assert_eq!(shock.duration, 8.0);
}

/// buildupTypes 与 vendor 一致（Modules/Data.lua:353-376，vendor-only）。
#[test]
fn buildup_types_match_pob2_data_lua() {
    let table = game_data().non_damaging_ailments().unwrap();
    assert_eq!(table.buildup_types.len(), 4);

    // Data.lua:354-357 Electrocute / :372-375 Pin：ScalesFrom 为空。
    assert!(table.buildup_types["Electrocute"].scales_from.is_empty());
    assert!(table.buildup_types["Pin"].scales_from.is_empty());
    // Data.lua:358-362 Freeze：仅 Cold。
    assert_eq!(
        table.buildup_types["Freeze"].scales_from,
        vec![DamageType::Cold]
    );
    // Data.lua:363-371 HeavyStun：全部五种伤害类型（canonical 序）。
    assert_eq!(
        table.buildup_types["HeavyStun"].scales_from,
        vec![
            DamageType::Physical,
            DamageType::Fire,
            DamageType::Cold,
            DamageType::Lightning,
            DamageType::Chaos,
        ]
    );
}

/// defaultAilmentDamageTypes：DoT 伤害类型逐值等于 Rust 准源
/// `AilmentType::damage_type()`（pobr-data/src/constants.rs:124-134）；
/// ScalesFrom 为 vendor-only（Modules/Data.lua:378-410）。
#[test]
fn default_ailment_damage_types_match_rust_source_and_vendor() {
    let table = game_data().non_damaging_ailments().unwrap();
    assert_eq!(table.default_ailment_damage_types.len(), 5);

    // 伤害型异常：damage_type 与 Rust 准源逐值相等。
    for (key, ailment) in [
        ("Bleed", AilmentType::Bleed),
        ("Poison", AilmentType::Poison),
        ("Ignite", AilmentType::Ignite),
    ] {
        assert_eq!(
            table.default_ailment_damage_types[key].damage_type,
            ailment.damage_type(),
            "{key} 的 DoT 伤害类型应与 AilmentType::damage_type() 一致"
        );
    }
    // 非伤害异常无 DoT 类型（vendor 行无 DamageType 字段；准源同样返回 None）。
    for (key, ailment) in [("Shock", AilmentType::Shock), ("Chill", AilmentType::Chill)] {
        assert_eq!(table.default_ailment_damage_types[key].damage_type, None);
        assert_eq!(ailment.damage_type(), None);
    }

    // ScalesFrom 抽样（vendor-only）：Data.lua:386-392 Poison 物理+混沌双来源、
    // :400-404 Shock 仅闪电、:405-409 Chill 仅冰冷。
    assert_eq!(
        table.default_ailment_damage_types["Poison"].scales_from,
        vec![DamageType::Physical, DamageType::Chaos]
    );
    assert_eq!(
        table.default_ailment_damage_types["Shock"].scales_from,
        vec![DamageType::Lightning]
    );
    assert_eq!(
        table.default_ailment_damage_types["Chill"].scales_from,
        vec![DamageType::Cold]
    );
}

/// 提交的 JSON 与 serde 序列化往返字节一致（可再生性铁律：禁手改后重生 byte-diff 零）。
#[test]
fn committed_json_is_serde_pretty_roundtrip_stable() {
    let path = repo_data_root()
        .join(version())
        .join("base/non_damaging_ailments.json");
    let committed = std::fs::read_to_string(&path).expect("读取已提交 JSON");
    let table = game_data().non_damaging_ailments().unwrap();
    let regenerated = serde_json::to_string_pretty(&table).expect("序列化");
    assert_eq!(
        committed.trim_end(),
        regenerated,
        "non_damaging_ailments.json 应与 serde pretty 输出字节一致（含键序）"
    );
}
