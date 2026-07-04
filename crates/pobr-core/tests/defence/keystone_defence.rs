//! M2 Track C 集成测试：keystone_registry + CI 接线 + 防御资源转换矩阵。
//!
//! C-1：`DefenceKeystones::from_db` 契约（一次性快照、词条名即接口）。
//! 蓝图 m2-defence §2 Track C / §3.3 契约 2：E/D/B/F 以参数消费本结构，
//! 禁止各 track 散读 keystone flag。

use pobr_core::{CalcConfig, DefenceKeystones, ModDb, Modifier};
use pobr_data::prelude::*;

/// 从词条文本经 mod_parser 解析后驱动注册表（端到端：文本 → flag → 快照）。
///
/// `Maximum Life is 1`（Chaos Inoculation 节点词条）→ `ChaosInoculation` flag
/// （mod_parser keystone special 段）；`Converts all Energy Shield to Mana`
/// （Eldritch Battery 型）→ `EnergyShieldConvertToMana` BASE 100 → 全转换开关。
#[test]
fn keystones_from_parsed_mod_texts() {
    // Arrange
    let mut db = ModDb::new();
    for text in ["Maximum Life is 1", "Converts all Energy Shield to Mana"] {
        let outcome = pobr_core::mod_parser::parse_mod(text).expect("解析失败");
        db.add_list(outcome.mods);
    }
    let cfg = CalcConfig::new();

    // Act
    let ks = DefenceKeystones::from_db(&db, &cfg);

    // Assert
    assert!(ks.chaos_inoculation, "CI 词条应驱动 chaos_inoculation");
    assert!(
        ks.eldritch_battery_es_to_mana,
        "全转换词条应驱动 eldritch_battery_es_to_mana"
    );
    // 未出现的 keystone 保持关闭。
    assert!(!ks.unbreakable);
    assert!(!ks.energy_shield_to_ward);
}

/// W0.1 词条 `Energy Shield protects Mana instead of Life`（EB flag，
/// ModParser.lua:2439）端到端驱动 `energy_shield_protects_mana`。
#[test]
fn eb_flag_from_parsed_text() {
    // Arrange
    let mut db = ModDb::new();
    let outcome = pobr_core::mod_parser::parse_mod("Energy Shield protects Mana instead of Life")
        .expect("解析失败");
    db.add_list(outcome.mods);
    let cfg = CalcConfig::new();

    // Act
    let ks = DefenceKeystones::from_db(&db, &cfg);

    // Assert
    assert!(ks.energy_shield_protects_mana);
    assert!(
        !ks.eldritch_battery_es_to_mana,
        "EB flag 不等于 ES→Mana 全转换"
    );
}

/// IronReflexes 词条（`Converts all Evasion Rating to Armour`，ModParser.lua:2343）
/// 同时产出 flag（联动用）与 `EvasionConvertToArmour` BASE 100（矩阵数据通道）。
#[test]
fn iron_reflexes_flag_and_matrix_data_coexist() {
    // Arrange
    let mut db = ModDb::new();
    let outcome = pobr_core::mod_parser::parse_mod("Converts all Evasion Rating to Armour")
        .expect("解析失败");
    db.add_list(outcome.mods);
    let cfg = CalcConfig::new();

    // Act
    let ks = DefenceKeystones::from_db(&db, &cfg);
    let conv = db.sum(
        ModType::Base,
        &cfg,
        &[ModName::from("EvasionConvertToArmour")],
    );

    // Assert：flag 仅供 Unbreakable 联动；数据展开走转换矩阵（BASE 100）。
    assert!(ks.iron_reflexes);
    assert_eq!(conv, 100.0);
}

/// 快照语义：直接注入 flag Modifier 的最小路径（树 ingest 等非文本来源）。
#[test]
fn keystones_from_injected_flags() {
    // Arrange
    let mut db = ModDb::new();
    db.add_list([
        Modifier::flag("Unbreakable"),
        Modifier::flag("DoubleBodyArmourDefence"),
        Modifier::flag("WardNotBreak"),
        Modifier::flag("EternalLife"),
        Modifier::flag("BloodMagic"),
    ]);
    let cfg = CalcConfig::new();

    // Act
    let ks = DefenceKeystones::from_db(&db, &cfg);

    // Assert
    assert!(ks.unbreakable);
    assert!(ks.double_body_armour_defence);
    assert!(ks.ward_not_break);
    assert!(ks.eternal_life);
    assert!(ks.blood_magic);
    assert!(!ks.chaos_inoculation);
}

// ─────────────────────────────────────────────────────────────────
// C-2：CI 接线（perform.rs EhpOptions.chaos_inoculation 不再写死 false）
// ─────────────────────────────────────────────────────────────────

/// CI build 端到端：`Maximum Life is 1` → Life=1 + ChaosInoculation flag →
/// EHP 走 ES 池、混沌 max hit = ∞。
///
/// vendor 依据：CalcDefence.lua:85（flag 读出）/:120-123（Life=1）；CI 免疫混沌
/// （agent-docs/active-defences.md §五 Keystone 表；EhpOptions::chaos_inoculation 语义）。
/// 修复前（C-2 之前）该 flag 在 perform 写死 false：chaos_max_hit 为有限值
/// （1 + 500×0.5 = 251 的混沌池），EHP 池仍按 Life 口径。
#[test]
fn ci_build_ehp_uses_es_pool_and_chaos_immunity() {
    use pobr_core::calc::{CalculationSession, MinimalInput};

    // Arrange：1000 基础生命 + 500 ES + CI。
    let input = MinimalInput {
        base_life: 1_000.0,
        base_mana: 100.0,
        base_fire_resistance: 0.0,
        base_cold_resistance: 0.0,
        base_lightning_resistance: 0.0,
        base_accuracy: 0.0,
        enemy_evasion: 0.0,
        base_hit_min: 100.0,
        base_hit_max: 200.0,
        base_action_rate: 2.0,
    };
    let mut session = CalculationSession::new(input);
    session
        .add_modifier_texts(["+500 to maximum Energy Shield", "Maximum Life is 1"])
        .unwrap();

    // Act
    session.perform_minimal();
    let output = session.output().clone();

    // Assert：CI → Life Override 1；混沌免疫；火 max hit = ES 池 / (1−0%) = 500。
    assert_eq!(output.life, 1.0, "CI 应把最大生命覆盖为 1");
    assert_eq!(output.energy_shield, 500.0);
    assert!(
        output.chaos_max_hit.is_infinite(),
        "CI 应使混沌 max hit = ∞（实际 {}）",
        output.chaos_max_hit
    );
    // F-3 口径切换后 max hit 走 PoB2 TotalHitPool：CI 下 = LifeRecoverable(1) + ES(500)
    // = 501（vendor :3540-3545 池基底含 Life 1；旧口径的 500 是 ES 单池近似）。
    assert_eq!(
        output.fire_max_hit, 501.0,
        "CI 下命中池 = Life(1) + ES(500)"
    );
    assert!(output.total_ehp.is_finite());
}

// ─────────────────────────────────────────────────────────────────
// C-3：五元防御资源转换矩阵 + Body Armour 翻倍 flag
// vendor：CalcDefence.lua:1301-1390（矩阵）、:1150-1290 / :806-808（翻倍 flag）
// ─────────────────────────────────────────────────────────────────

use pobr_core::ModTag;
use pobr_core::calc::ActorBaseStats;
use pobr_core::calc::defence::calc_defence_resources;

/// 槽位限定 BASE 词条（装备 rolled 件级底值的测试替身）。
fn slot_base(name: &str, slot: &str, value: f64) -> Modifier {
    Modifier::number(name, ModType::Base, value).with_tag(ModTag::SlotName(slot.to_string()))
}

/// 无矩阵词条 / 无 keystone 时矩阵恒等：与旧 `scaled_defence_stat` 公式逐位一致
/// （C-3 行为 commit 的「无词条 build 逐值不变」最小见证）。
#[test]
fn matrix_is_identity_without_conversion_mods() {
    // Arrange：基底 100 + bodyarmour 槽位底 200 + 全局 50% increased Armour。
    let mut db = ModDb::new();
    db.add_list([
        slot_base("Armour", "bodyarmour", 200.0),
        Modifier::number("Armour", ModType::Inc, 50.0),
    ]);
    let cfg = CalcConfig::new();
    let base = ActorBaseStats {
        armour: 100.0,
        ..ActorBaseStats::default()
    };
    let ks = DefenceKeystones::from_db(&db, &cfg);

    // Act
    let res = calc_defence_resources(&db, &cfg, &base, &ks);

    // Assert：旧公式 total = 100×1.5 + 200×1.5 = 450；其余资源不受扰动。
    assert_eq!(res.armour, 450.0);
    assert_eq!(res.evasion, 0.0);
    assert_eq!(res.energy_shield, 0.0);
    assert_eq!(res.extra_life, 0.0);
    assert_eq!(res.extra_mana, 0.0);
}

/// ConvertTo（defence→defence）：槽位底逐槽转移进**目标的同槽位桶**，享目标乘区；
/// 源按 (100−total)/100 缩残（CalcDefence.lua:1340-1352）。
#[test]
fn convert_to_moves_slot_base_into_target_slot_bucket() {
    // Arrange：bodyarmour Armour 槽位底 200 + 全局 100% increased Evasion +
    // 文本词条「50% of Armour converted to Evasion Rating」。
    let mut db = ModDb::new();
    db.add_list([
        slot_base("Armour", "bodyarmour", 200.0),
        Modifier::number("Evasion", ModType::Inc, 100.0),
    ]);
    let outcome = pobr_core::mod_parser::parse_mod("50% of Armour converted to Evasion Rating")
        .expect("解析失败");
    db.add_list(outcome.mods);
    let cfg = CalcConfig::new();
    let ks = DefenceKeystones::from_db(&db, &cfg);

    // Act
    let res = calc_defence_resources(&db, &cfg, &ActorBaseStats::default(), &ks);

    // Assert：armour 留存 200×0.5=100；evasion 槽位桶收 100 × (1+100%) = 200。
    assert_eq!(res.armour, 100.0);
    assert_eq!(res.evasion, 200.0);
}

/// ConvertTo 行总和 >100 → 按比例归一化（cap 100，CalcDefence.lua:1315-1320 意图；
/// vendor 的归一化循环因 `ipairs` 误用不生效，按蓝图裁决实现真归一化）。
#[test]
fn conversion_rates_over_100_are_normalised() {
    // Arrange：全局 armour 底 100；Armour→Evasion 80 + Armour→ES 40（总 120 → ×5/6）。
    let mut db = ModDb::new();
    db.add_list([
        Modifier::number("ArmourConvertToEvasion", ModType::Base, 80.0),
        Modifier::number("ArmourConvertToEnergyShield", ModType::Base, 40.0),
    ]);
    let cfg = CalcConfig::new();
    let base = ActorBaseStats {
        armour: 100.0,
        ..ActorBaseStats::default()
    };
    let ks = DefenceKeystones::from_db(&db, &cfg);

    // Act
    let res = calc_defence_resources(&db, &cfg, &base, &ks);

    // Assert：armour 全转出（total=100）；evasion=100×(80×5/6)/100=66.67、ES=33.33。
    assert_eq!(res.armour, 0.0);
    assert!(
        (res.evasion - 200.0 / 3.0).abs() < 1e-6,
        "evasion={}",
        res.evasion
    );
    assert!(
        (res.energy_shield - 100.0 / 3.0).abs() < 1e-6,
        "es={}",
        res.energy_shield
    );
}

/// GainAs 不减源（CalcDefence.lua:1336-1337：rate 叠加 gainRate，但 totalConversion
/// 只含 ConvertTo → 缩残不受 GainAs 影响）。
#[test]
fn gain_as_does_not_reduce_source() {
    // Arrange：全局 armour 底 100 + ArmourGainAsEvasion 25。
    let mut db = ModDb::new();
    db.add_list([Modifier::number("ArmourGainAsEvasion", ModType::Base, 25.0)]);
    let cfg = CalcConfig::new();
    let base = ActorBaseStats {
        armour: 100.0,
        ..ActorBaseStats::default()
    };
    let ks = DefenceKeystones::from_db(&db, &cfg);

    // Act
    let res = calc_defence_resources(&db, &cfg, &base, &ks);

    // Assert
    assert_eq!(res.armour, 100.0, "GainAs 不减源");
    assert_eq!(res.evasion, 25.0);
}

/// defence→非 defence（ES→Mana，既有 es_to_mana_rate 通道并入矩阵）：槽位底 + 全局底
/// 按 rate 转入 `extra_mana`（defence 源不取整，:1340-1355），ES 侧缩残。
#[test]
fn es_to_mana_merged_into_matrix() {
    // Arrange：ES 基底 100 + bodyarmour 槽位底 400 +
    // 「30% of Maximum Energy Shield converted to Mana」。
    let mut db = ModDb::new();
    db.add_list([slot_base("EnergyShield", "bodyarmour", 400.0)]);
    let outcome =
        pobr_core::mod_parser::parse_mod("30% of Maximum Energy Shield converted to Mana")
            .expect("解析失败");
    db.add_list(outcome.mods);
    let cfg = CalcConfig::new();
    let base = ActorBaseStats {
        energy_shield: 100.0,
        ..ActorBaseStats::default()
    };
    let ks = DefenceKeystones::from_db(&db, &cfg);

    // Act
    let res = calc_defence_resources(&db, &cfg, &base, &ks);

    // Assert：ES = (400+100)×0.7 = 350；extra_mana = (400+100)×0.3 = 150。
    assert_eq!(res.energy_shield, 350.0);
    assert_eq!(res.extra_mana, 150.0);
    assert_eq!(res.extra_life, 0.0);
}

/// 非 defence 源（Life）→ defence 目标：全局 ceil 取整（CalcDefence.lua:1364-1366）、
/// 源本体不在矩阵内扣减（doActorLifeManaSpirit 域，:73-126）。
#[test]
fn non_defence_source_gain_uses_ceil() {
    // Arrange：基础 Life 1001 + 「Gain 25% of Maximum Life as Extra Maximum Energy Shield」。
    let mut db = ModDb::new();
    let outcome =
        pobr_core::mod_parser::parse_mod("Gain 25% of Maximum Life as Extra Maximum Energy Shield")
            .expect("解析失败");
    db.add_list(outcome.mods);
    let cfg = CalcConfig::new();
    let base = ActorBaseStats {
        life: 1001.0,
        ..ActorBaseStats::default()
    };
    let ks = DefenceKeystones::from_db(&db, &cfg);

    // Act
    let res = calc_defence_resources(&db, &cfg, &base, &ks);

    // Assert：ceil(1001×0.25) = ceil(250.25) = 251。
    assert_eq!(res.energy_shield, 251.0);
}

/// Unbreakable：Body Armour 槽位 armour 基底 ×2（CalcDefence.lua:1217），其他槽位不变。
#[test]
fn unbreakable_doubles_body_armour_slot_armour() {
    // Arrange
    let mut db = ModDb::new();
    db.add_list([
        slot_base("Armour", "bodyarmour", 300.0),
        slot_base("Armour", "helmet", 100.0),
        Modifier::flag("Unbreakable"),
    ]);
    let cfg = CalcConfig::new();
    let ks = DefenceKeystones::from_db(&db, &cfg);

    // Act
    let res = calc_defence_resources(&db, &cfg, &ActorBaseStats::default(), &ks);

    // Assert：300×2 + 100 = 700。
    assert_eq!(res.armour, 700.0);
}

/// DoubleBodyArmourDefence：Body Armour 槽位 armour/evasion/ES 基底皆 ×2
/// （CalcDefence.lua:1189/:1214/:1232）。
#[test]
fn double_body_armour_defence_doubles_all_three() {
    // Arrange
    let mut db = ModDb::new();
    db.add_list([
        slot_base("Armour", "bodyarmour", 100.0),
        slot_base("Evasion", "bodyarmour", 150.0),
        slot_base("EnergyShield", "bodyarmour", 200.0),
        Modifier::flag("DoubleBodyArmourDefence"),
    ]);
    let cfg = CalcConfig::new();
    let ks = DefenceKeystones::from_db(&db, &cfg);

    // Act
    let res = calc_defence_resources(&db, &cfg, &ActorBaseStats::default(), &ks);

    // Assert
    assert_eq!(res.armour, 200.0);
    assert_eq!(res.evasion, 300.0);
    assert_eq!(res.energy_shield, 400.0);
}

/// Unbreakable × IronReflexes 联动端到端：Body Armour 闪避基底 ×2（:1235-1237 / :806-808），
/// 再经 `EvasionConvertToArmour` 100（IronReflexes 数据展开）全转进 armour。
#[test]
fn unbreakable_iron_reflexes_doubles_then_converts_evasion() {
    // Arrange：bodyarmour armour 100 + evasion 200 + Unbreakable + IronReflexes 词条。
    let mut db = ModDb::new();
    db.add_list([
        slot_base("Armour", "bodyarmour", 100.0),
        slot_base("Evasion", "bodyarmour", 200.0),
        Modifier::flag("Unbreakable"),
    ]);
    let outcome = pobr_core::mod_parser::parse_mod("Converts all Evasion Rating to Armour")
        .expect("解析失败");
    db.add_list(outcome.mods);
    let cfg = CalcConfig::new();
    let ks = DefenceKeystones::from_db(&db, &cfg);

    // Act
    let res = calc_defence_resources(&db, &cfg, &ActorBaseStats::default(), &ks);

    // Assert：armour 槽位底 100×2（Unbreakable）；evasion 槽位底 200×2（联动）全转入
    // armour 的 bodyarmour 槽位桶 → armour = 200 + 400 = 600，evasion = 0。
    assert_eq!(res.armour, 600.0);
    assert_eq!(res.evasion, 0.0);
}

/// EnergyShieldToWard：装备 ES 槽位底不再聚合进 ES（转 Ward 由 Track D 消费，
/// CalcDefence.lua:1192-1205）；非装备的全局 flat ES 不受影响。
#[test]
fn energy_shield_to_ward_excludes_gear_es_bases() {
    // Arrange
    let mut db = ModDb::new();
    db.add_list([
        slot_base("EnergyShield", "bodyarmour", 300.0),
        Modifier::flag("EnergyShieldToWard"),
    ]);
    let cfg = CalcConfig::new();
    let base = ActorBaseStats {
        energy_shield: 50.0,
        ..ActorBaseStats::default()
    };
    let ks = DefenceKeystones::from_db(&db, &cfg);

    // Act
    let res = calc_defence_resources(&db, &cfg, &base, &ks);

    // Assert：装备 300 不聚合，仅留全局 50。
    assert_eq!(res.energy_shield, 50.0);
}

/// 端到端（perform 注入路径）：全转 ES→Mana 经矩阵转入 MaximumMana BASE、
/// ES 面板归零（旧 es_to_mana_rate 行为的等价回归）。
#[test]
fn perform_injects_matrix_extra_mana() {
    use pobr_core::calc::{CalculationSession, MinimalInput};

    // Arrange：100 基础魔力 + 500 flat ES + 全转词条。
    let input = MinimalInput {
        base_life: 1_000.0,
        base_mana: 100.0,
        base_fire_resistance: 0.0,
        base_cold_resistance: 0.0,
        base_lightning_resistance: 0.0,
        base_accuracy: 0.0,
        enemy_evasion: 0.0,
        base_hit_min: 100.0,
        base_hit_max: 200.0,
        base_action_rate: 2.0,
    };
    let mut session = CalculationSession::new(input);
    session
        .add_modifier_texts([
            "+500 to maximum Energy Shield",
            "Converts all Energy Shield to Mana",
        ])
        .unwrap();

    // Act
    session.perform_minimal();
    let output = session.output().clone();

    // Assert：ES 面板 0；Mana = 100 + 500（转入享 Mana 全局乘区，此处乘区为 1）。
    assert_eq!(output.energy_shield, 0.0);
    assert_eq!(output.mana, 600.0);
}

/// Total 直加通道（vendor CalcDefence.lua:1331/:1394）：`<Res>Total` BASE（如
/// Discipline 光环的 `EnergyShieldTotal`）**不乘 inc/more 直加终值**，与普通
/// flat（进 base 吃 inc）区分。旧实现曾把 Discipline buff 错并入 `EnergyShield`
/// 桶吃全局 inc（essence-drain ES 多算 1.13x 根因）。
#[test]
fn total_channel_adds_flat_without_scaling() {
    let mut db = ModDb::new();
    db.add_list([
        Modifier::number("EnergyShield", ModType::Base, 100.0),
        Modifier::number("EnergyShield", ModType::Inc, 100.0),
        Modifier::number("EnergyShieldTotal", ModType::Base, 235.0),
    ]);
    let cfg = CalcConfig::new();
    let ks = DefenceKeystones::from_db(&db, &cfg);

    let res = calc_defence_resources(&db, &cfg, &ActorBaseStats::default(), &ks);

    // 100×(1+100%) + 235（直加，不吃 inc）= 435；旧错位口径会给 (100+235)×2 = 670。
    assert_eq!(res.energy_shield, 435.0);
}

/// Total 通道随转换传播并缩残（vendor :1362-1366/:1388）：ES→Armour 60% 转换时
/// EnergyShieldTotal 的 60% 直加进 Armour 终值（同样不乘 Armour inc），残余 40%
/// 直加 ES。
#[test]
fn total_channel_follows_conversion() {
    let mut db = ModDb::new();
    db.add_list([
        Modifier::number("EnergyShieldTotal", ModType::Base, 100.0),
        Modifier::number("EnergyShieldConvertToArmour", ModType::Base, 60.0),
        Modifier::number("Armour", ModType::Inc, 50.0),
    ]);
    let cfg = CalcConfig::new();
    let ks = DefenceKeystones::from_db(&db, &cfg);

    let res = calc_defence_resources(&db, &cfg, &ActorBaseStats::default(), &ks);

    // Armour 收到 total 60（直加，不吃自身 50% inc）；ES 残余 40。
    assert_eq!(res.armour, 60.0);
    assert_eq!(res.energy_shield, 40.0);
}
