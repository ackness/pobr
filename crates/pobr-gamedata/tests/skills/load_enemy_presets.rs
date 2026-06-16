//! `base/enemy_presets.json` 加载测试。
//!
//! 搬迁不变式校验：有 Rust 准源（`pobr_data::monster::EnemyTier` 各方法 +
//! `setup_env.rs` 注入数值）的字段**逐档逐值断言**与现有 Rust 表相等；
//! vendor-only 字段（KnockbackDistanceOnSelf / MinimumMovementSpeed /
//! PoiseThreshold 213/838 / player_mods / chaos_damage_div / speed・crit 占位）
//! 按 vendor `ConfigOptions.lua`（commit 2df5a74）行号做断言（值写死）。

use pobr_data::catalog::enemy_presets::{EnemyPresetsTable, EnemyTierPreset};
use pobr_data::monster::{EnemyTier, MAX_ENEMY_LEVEL, MONSTER_BASE_CRIT_DAMAGE_BONUS};
use pobr_gamedata::{GameData, repo_data_root};

const VERSION: &str = "4.5.0.3.4";

fn load() -> EnemyPresetsTable {
    GameData::new(repo_data_root().join(VERSION))
        .enemy_presets()
        .expect("enemy_presets 可加载")
}

/// 取指定档位（同时校验 JSON 档位顺序 = None → Boss → Pinnacle → Uber）。
fn tier<'a>(t: &'a EnemyPresetsTable, id: &str) -> &'a EnemyTierPreset {
    t.tiers
        .iter()
        .find(|p| p.id == id)
        .unwrap_or_else(|| panic!("缺少档位 {id}"))
}

/// 在某档位的 mod 组中找一条（name + value 双键，PoiseThreshold 有两条同名）。
fn find_mod<'a>(
    mods: &'a [pobr_data::catalog::enemy_presets::EnemyPresetMod],
    name: &str,
    value: f64,
) -> &'a pobr_data::catalog::enemy_presets::EnemyPresetMod {
    mods.iter()
        .find(|m| m.name == name && m.value == value)
        .unwrap_or_else(|| panic!("缺少 mod {name} = {value}"))
}

/// 四档齐全且顺序与 vendor list / pobr `EnemyTier` 枚举序一致。
#[test]
fn four_tiers_in_canonical_order() {
    let t = load();
    let ids: Vec<&str> = t.tiers.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, ["None", "Boss", "Pinnacle", "Uber"]);
    // 默认档位 = Pinnacle（vendor defaultIndex = 3；pobr `EnemyTier::default()`）。
    let default_id = format!("{:?}", EnemyTier::default());
    for p in &t.tiers {
        assert_eq!(p.is_default, p.id == default_id, "档位 {} 默认位", p.id);
    }
}

/// 顶层公共默认：pobr 准源逐值相等 + vendor-only 抽样。
#[test]
fn top_level_defaults_match_rust_sources() {
    let t = load();
    // pobr 准源：monster.rs::MAX_ENEMY_LEVEL / MONSTER_BASE_CRIT_DAMAGE_BONUS。
    assert_eq!(t.max_enemy_level, MAX_ENEMY_LEVEL);
    assert_eq!(
        t.default_enemy_crit_damage_bonus,
        MONSTER_BASE_CRIT_DAMAGE_BONUS
    );
    // pobr 准源：EnemyTierDefaults::compute 内联 `damage * 1.5 * dps_mult` 的 1.5。
    assert_eq!(t.ehp_base_damage_mult, 1.5);
    // vendor-only：ConfigOptions.lua L1965（enemySpeed 占位 700）/ L1966（crit 5%）。
    assert_eq!(t.default_enemy_speed, 700.0);
    assert_eq!(t.default_enemy_crit_chance, 5.0);
}

/// 逐档标量与 pobr `EnemyTier` 各方法逐值相等（搬迁不变式核心断言）。
#[test]
fn tier_scalars_match_enemy_tier_methods() {
    let t = load();
    for (id, rust_tier) in [
        ("None", EnemyTier::None),
        ("Boss", EnemyTier::Boss),
        ("Pinnacle", EnemyTier::Pinnacle),
        ("Uber", EnemyTier::Uber),
    ] {
        let p = tier(&t, id);
        assert_eq!(p.min_level, rust_tier.min_level(), "{id} min_level");
        assert_eq!(
            p.elemental_resist_bonus,
            rust_tier.elemental_resist_bonus(),
            "{id} elemental_resist_bonus"
        );
        assert_eq!(
            p.chaos_resist_bonus,
            rust_tier.chaos_resist_bonus(),
            "{id} chaos_resist_bonus"
        );
        // ExactRatio::value() 要求与 Rust 准源**逐 bit 相等**（分数形保证
        // serde_json 默认浮点解析无损，见 schema doc）。
        assert_eq!(
            p.armour_mult_pct.value(),
            rust_tier.armour_mult_pct(),
            "{id} armour_mult_pct"
        );
        assert_eq!(
            p.evasion_mult_pct.value(),
            rust_tier.evasion_mult_pct(),
            "{id} evasion_mult_pct"
        );
        assert_eq!(p.pen, rust_tier.pen(), "{id} pen");
        assert_eq!(p.dps_mult.value(), rust_tier.dps_mult(), "{id} dps_mult");
    }
}

/// 条件态与 pobr `setup_env.rs` 注入一致（Unique/RareOrUnique；Pinnacle/Uber 加 PinnacleBoss）。
#[test]
fn conditions_match_setup_env_injection() {
    let t = load();
    assert!(tier(&t, "None").conditions.is_empty());
    assert_eq!(tier(&t, "Boss").conditions, ["Unique", "RareOrUnique"]);
    for id in ["Pinnacle", "Uber"] {
        assert_eq!(
            tier(&t, id).conditions,
            ["Unique", "RareOrUnique", "PinnacleBoss"],
            "{id} conditions"
        );
    }
}

/// pobr 已注入的 boss 共通 mod（setup_env.rs::inject_enemy_mods）逐档逐值相等：
/// Curse/Exposure/Slow `MORE -50`（Effective 门控）+ `PoiseThreshold MORE 500`（无门控）。
#[test]
fn boss_common_mods_match_setup_env_values() {
    let t = load();
    assert!(tier(&t, "None").enemy_mods.is_empty(), "None 档无 mod 组");
    for id in ["Boss", "Pinnacle", "Uber"] {
        let mods = &tier(&t, id).enemy_mods;
        for name in [
            "CurseEffectOnSelf",
            "ExposureEffectOnSelf",
            "SlowEffectOnSelf",
        ] {
            let m = find_mod(mods, name, -50.0);
            assert_eq!(m.mod_type, "MORE", "{id} {name} 类型");
            assert!(
                m.effective_only,
                "{id} {name} 应带 Effective 门控（pobr 现状）"
            );
        }
        let poise = find_mod(mods, "PoiseThreshold", 500.0);
        assert_eq!(poise.mod_type, "MORE", "{id} PoiseThreshold 类型");
        // pobr setup_env.rs 用 push_enemy_number 注入（无 Effective 门控）；
        // vendor L2005 带门控——出入已记 schema doc TODO(parity)，此处按 pobr 现状断言。
        assert!(
            !poise.effective_only,
            "{id} PoiseThreshold 500 按 pobr 现状不带门控"
        );
    }
}

/// Uber 专属：`DamageTaken MORE -70`（pobr `EnemyTier::damage_taken_more()`；
/// vendor L2087，无 Effective 门控），其余档位不得出现。
#[test]
fn uber_damage_taken_matches_rust_source() {
    let t = load();
    let m = find_mod(
        &tier(&t, "Uber").enemy_mods,
        "DamageTaken",
        EnemyTier::Uber.damage_taken_more(),
    );
    assert_eq!(m.mod_type, "MORE");
    assert!(!m.effective_only);
    for (id, rust_tier) in [
        ("None", EnemyTier::None),
        ("Boss", EnemyTier::Boss),
        ("Pinnacle", EnemyTier::Pinnacle),
    ] {
        assert!(
            !tier(&t, id)
                .enemy_mods
                .iter()
                .any(|m| m.name == "DamageTaken"),
            "{id} 档不应有 DamageTaken"
        );
        assert_eq!(rust_tier.damage_taken_more(), 0.0, "{id} Rust 准源亦为 0");
    }
}

/// vendor-only enemy mod 抽样断言（pobr 此前未实现，值写死自 vendor）：
/// - `KnockbackDistanceOnSelf MORE -75`（ConfigOptions.lua L2002/L2044/L2084）
/// - `MinimumMovementSpeed BASE 20`（L2004/L2046/L2086）
/// - `PoiseThreshold MORE 213 "Map Boss"`（仅 Boss 档，L2006）
/// - `PoiseThreshold MORE 838 "Xesht"`（Pinnacle/Uber 档，L2048/L2089）
#[test]
fn vendor_only_enemy_mods_sampled() {
    let t = load();
    for id in ["Boss", "Pinnacle", "Uber"] {
        let mods = &tier(&t, id).enemy_mods;
        let kb = find_mod(mods, "KnockbackDistanceOnSelf", -75.0);
        assert_eq!(kb.mod_type, "MORE");
        assert!(kb.effective_only);
        let mms = find_mod(mods, "MinimumMovementSpeed", 20.0);
        assert_eq!(mms.mod_type, "BASE");
        assert!(mms.effective_only);
    }
    let map_boss = find_mod(&tier(&t, "Boss").enemy_mods, "PoiseThreshold", 213.0);
    assert_eq!(map_boss.source_label, "Map Boss");
    for id in ["Pinnacle", "Uber"] {
        let xesht = find_mod(&tier(&t, id).enemy_mods, "PoiseThreshold", 838.0);
        assert_eq!(xesht.source_label, "Xesht", "{id} Xesht poise");
        assert!(
            !tier(&t, id).enemy_mods.iter().any(|m| m.value == 213.0),
            "{id} 不应有 Map Boss 213"
        );
    }
}

/// vendor-only player mod：Boss/Pinnacle/Uber 三档均注入
/// `WarcryPower BASE 20` + `Multiplier:EnemyPower BASE 20`（L2007-2008/L2049-2050/L2090-2091）。
#[test]
fn vendor_only_player_mods_sampled() {
    let t = load();
    assert!(tier(&t, "None").player_mods.is_empty());
    for id in ["Boss", "Pinnacle", "Uber"] {
        let mods = &tier(&t, id).player_mods;
        assert_eq!(mods.len(), 2, "{id} player_mods 条数");
        for name in ["WarcryPower", "Multiplier:EnemyPower"] {
            let m = find_mod(mods, name, 20.0);
            assert_eq!(m.mod_type, "BASE", "{id} {name} 类型");
            assert_eq!(m.source_label, "Boss", "{id} {name} 来源标签");
            assert!(!m.effective_only, "{id} {name} 无 Effective 门控");
        }
    }
}

/// vendor-only 混沌伤害除数：None/Boss/Pinnacle = 2.5（L1987/L2028/L2070），
/// Uber = 4（L2111）。
#[test]
fn chaos_damage_divisor_sampled_from_vendor() {
    let t = load();
    for id in ["None", "Boss", "Pinnacle"] {
        assert_eq!(tier(&t, id).chaos_damage_div, 2.5, "{id} chaos div");
    }
    assert_eq!(tier(&t, "Uber").chaos_damage_div, 4.0);
}
