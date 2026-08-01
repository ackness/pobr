//! 集成测试：Block / Spirit / Ward / Deflection 面板族 + 预留 efficiency。
//!
//! 期望值全部手算自 PoB2 公式，注释标注 CalcDefence.lua 行号
//! （公式单测带 vendor 行号 + 手算期望值）。

use pobr_core::{CalcConfig, ModDb, calc};
use pobr_data::prelude::*;

/// 文本词条 → ModDb（经 mod_parser，W0.1 词条覆盖表即接口契约）。
fn db_from_texts(texts: &[&str]) -> ModDb {
    let mut db = ModDb::new();
    for text in texts {
        let outcome =
            crate::support::parse_mod(text).unwrap_or_else(|e| panic!("解析 `{text}` 失败：{e:?}"));
        assert!(
            !outcome.mods.is_empty(),
            "`{text}` 应解析出词条（Unsupported 会静默丢失覆盖）"
        );
        db.add_list(outcome.mods);
    }
    db
}

/// 直接注入数值词条（build 层注入语义，如 ShieldBlockChance）。
fn add_base(db: &mut ModDb, name: &str, value: f64) {
    db.add_list([pobr_core::Modifier::number(name, ModType::Base, value)]);
}

// Block（CalcDefence.lua:961-1058）

/// 无任何词条：格挡 0，上限 = 角色固有 BaseBlockChanceMax 50
/// （Misc.lua:147；CalcSetup.lua:28）。
#[test]
fn block_defaults_without_sources() {
    let db = ModDb::new();
    let cfg = CalcConfig::new();

    let block = calc::calc_block(&db, &cfg);

    assert_eq!(block.block_chance, 0.0);
    assert_eq!(block.block_chance_max, 50.0);
    assert_eq!(block.spell_block_chance_max, 50.0);
    assert_eq!(block.effective_block_chance, 0.0);
    assert_eq!(block.block_effect_taken_pct, 0.0, "默认完全格挡（承伤 0%）");
}

/// 盾基底 + inc 乘区：`(26 + 0) × 1.07 = 27.82`（CalcDefence.lua:989-991；
/// warrior-titan-shield-wall 的黄金形态：Tawhoan Tower Shield 基底 26 + 树 7% inc）。
#[test]
fn block_shield_base_times_increased() {
    let mut db = db_from_texts(&["7% increased Block chance"]);
    add_base(&mut db, "ShieldBlockChance", 26.0);
    let cfg = CalcConfig::new();

    let block = calc::calc_block(&db, &cfg);

    assert!(
        (block.block_chance - 27.82).abs() < 1e-9,
        "(26+0)×1.07 = 27.82，得 {}",
        block.block_chance
    );
    assert_eq!(
        block.effective_block_chance, 27.82,
        "无 luck flag 时有效值 = 原值"
    );
}

/// BlockChanceMax 体系（:961-965）：上限 = 50 固有 + ΣBASE BlockChanceMax，
/// 总量超限被 cap；BlockChanceCap 90 为硬上限。
#[test]
fn block_capped_by_max_chain() {
    let mut db = db_from_texts(&["+5% to Maximum Block Chance"]);
    add_base(&mut db, "ShieldBlockChance", 30.0);
    add_base(&mut db, "BlockChance", 40.0); // 30+40=70 > 55
    let cfg = CalcConfig::new();

    let block = calc::calc_block(&db, &cfg);

    assert_eq!(block.block_chance_max, 55.0, "50 固有 + 5 词条");
    assert_eq!(block.block_chance, 55.0, "70 被上限 55 截断");
}

/// 法术格挡（:1003-1014）：独立 BASE/乘区；`SpellBlockChanceIsBlockChance`
/// flag（ModParser.lua:3027）时等同攻击格挡。
#[test]
fn spell_block_independent_and_flag_unified() {
    // 独立路径：20% spell block 词条 → spell=20，攻击不受影响。
    let db = db_from_texts(&["20% chance to Block Spell Damage"]);
    let cfg = CalcConfig::new();
    let block = calc::calc_block(&db, &cfg);
    assert_eq!(block.spell_block_chance, 20.0);
    assert_eq!(block.block_chance, 0.0);

    // flag 路径：法术格挡 = 攻击格挡。词条文本（ModParser.lua:3027）不在当前
    // 数据集/引擎规则内——直接注入 flag（该 flag 的生产来源是引擎规则数据）。
    let mut db = ModDb::new();
    db.add_list([pobr_core::Modifier::flag("SpellBlockChanceIsBlockChance")]);
    add_base(&mut db, "ShieldBlockChance", 26.0);
    let block = calc::calc_block(&db, &cfg);
    assert_eq!(block.spell_block_chance, block.block_chance);
    assert_eq!(block.spell_block_chance, 26.0);
}

/// 格挡承伤（:1054-1058 / ModParser.lua:2479）：`You take 30% of Damage from
/// Blocked Hits` → 承伤份额 30%。
#[test]
fn block_effect_taken_share() {
    // `You take 30% of Damage from Blocked Hits`（ModParser.lua:2479）不在当前
    // 引擎规则内——直接注入 BlockEffect BASE（calc 消费口径不变）。
    let mut db = ModDb::new();
    add_base(&mut db, "BlockEffect", 30.0);
    let cfg = CalcConfig::new();

    let block = calc::calc_block(&db, &cfg);

    assert_eq!(block.block_effect_taken_pct, 30.0);
}

// Spirit 池（CalcDefence.lua:73-126）

/// 池公式（:87-95）：`(ΣBASE × (1−conv) + extra) × (1+inc) × more`，round 取整。
/// 手算：(100+30) × 1.20 = 156。
#[test]
fn spirit_pool_base_times_increased() {
    let mut db = db_from_texts(&["+30 to Spirit"]);
    add_base(&mut db, "Spirit", 100.0); // 任务奖励 / 件级注入语义
    db.add_list([pobr_core::Modifier::number("Spirit", ModType::Inc, 20.0)]);
    let cfg = CalcConfig::new();

    assert_eq!(calc::calc_spirit_pool(&db, &cfg), 156.0);
}

/// 空来源下限 1（:95 `m_max(round(...), 1)`，与 Life/Mana 一致）。
#[test]
fn spirit_pool_floor_is_one() {
    let db = ModDb::new();
    let cfg = CalcConfig::new();

    assert_eq!(calc::calc_spirit_pool(&db, &cfg), 1.0);
}

/// 转换扣减（:92）：`SpiritConvertToEnergyShield` 30 → 100×0.7 = 70。
#[test]
fn spirit_pool_conversion_reduces_base() {
    let mut db = ModDb::new();
    add_base(&mut db, "Spirit", 100.0);
    add_base(&mut db, "SpiritConvertToEnergyShield", 30.0);
    let cfg = CalcConfig::new();

    assert_eq!(calc::calc_spirit_pool(&db, &cfg), 70.0);
}

// Ward 池（CalcDefence.lua:1144-1296）

/// 聚合公式（:1158/:1286）：`ΣBASE Ward × (1 + Σinc(Ward,Defences)/100) × more`。
/// 手算：(34+241+50) × 1.20 = 390。
#[test]
fn ward_base_times_increased() {
    let mut db = db_from_texts(&["+50 to Ward", "20% increased Ward"]);
    add_base(&mut db, "Ward", 34.0); // 件级注入语义（rolled `Ward:` 行）
    add_base(&mut db, "Ward", 241.0);
    let cfg = CalcConfig::new();

    assert_eq!(calc::calc_ward(&db, &cfg, false), 390.0);
}

/// `EnergyShieldToWard` keystone（:1162-1163）：ES 的 inc 借给 Ward——
/// inc 名集追加 EnergyShield。手算：100 × (1 + (20+30)/100) = 150。
#[test]
fn ward_es_to_ward_borrows_es_increases() {
    let mut db = db_from_texts(&["20% increased Ward"]);
    add_base(&mut db, "Ward", 100.0);
    db.add_list([pobr_core::Modifier::number(
        "EnergyShield",
        ModType::Inc,
        30.0,
    )]);
    let cfg = CalcConfig::new();

    assert_eq!(calc::calc_ward(&db, &cfg, false), 120.0, "无 keystone 不借");
    assert_eq!(
        calc::calc_ward(&db, &cfg, true),
        150.0,
        "keystone 借入 ES inc"
    );
}

/// 无 Ward 来源 → 0（verify 零值不误报）。
#[test]
fn ward_zero_without_sources() {
    let db = ModDb::new();
    let cfg = CalcConfig::new();
    assert_eq!(calc::calc_ward(&db, &cfg, false), 0.0);
}

// Deflection（CalcDefence.lua:48-54 / :1487-1506）

/// `deflectChance` 公式手算（:48-54）：rating=5000、acc=2000 →
/// notDeflect = 2000/(2000+600)×150−50 = 65.3846…，chance = 100−round(65) = 35。
#[test]
fn deflection_chance_formula() {
    let mut db = ModDb::new();
    add_base(&mut db, "DeflectionRating", 5000.0);
    let cfg = CalcConfig::new();

    let d = calc::calc_deflection(&db, &cfg, 0.0, 0.0, 2000.0);

    assert_eq!(d.rating, 5000.0);
    assert_eq!(d.chance, 35.0);
    assert_eq!(d.effect_pct, 40.0, "基础 DeflectEffect 40（Misc.lua:111）");
}

/// GainAs 合成（:1490）：rating = 0 + (evasion×30% + armour×20%) × (1+inc)；
/// 手算：(10000×0.30 + 5000×0.20) × 1.10 = 4400。inc 只作用于 GainAs 部分。
#[test]
fn deflection_rating_gain_as_with_increased() {
    let mut db = db_from_texts(&[
        "Gain Deflection Rating equal to 30% of Evasion Rating",
        "Gain Deflection Rating equal to 20% of Armour",
        "10% increased Deflection Rating",
    ]);
    add_base(&mut db, "DeflectionRating", 100.0);
    let cfg = CalcConfig::new();

    let d = calc::calc_deflection(&db, &cfg, 5000.0, 10000.0, 2000.0);

    // 100（裸 BASE 不吃乘区）+ 4000×1.1 = 4500。
    assert_eq!(d.rating, 4500.0);
}

/// rating < 1 → 0 几率（:49-51）；DeflectIsLucky 幂（:1492-1495）。
#[test]
fn deflection_zero_and_lucky() {
    let db = ModDb::new();
    let cfg = CalcConfig::new();
    assert_eq!(
        calc::calc_deflection(&db, &cfg, 0.0, 0.0, 2000.0).chance,
        0.0
    );

    // lucky：35% → (1−0.65²)×100 = 57.75。`Chance to Deflect is Lucky` 文本不在
    // 当前引擎规则内——直接注入 DeflectIsLucky flag。
    let mut db = ModDb::new();
    db.add_list([pobr_core::Modifier::flag("DeflectIsLucky")]);
    add_base(&mut db, "DeflectionRating", 5000.0);
    let d = calc::calc_deflection(&db, &cfg, 0.0, 0.0, 2000.0);
    assert!((d.chance - 57.75).abs() < 1e-9, "got {}", d.chance);
}

// Reservation Efficiency（CalcDefence.lua:172-350）

/// 效率除法（:240-241/:249-258）：`reserved = (flat + pool×pct) × mult ÷
/// (1+eff/100) ÷ effMore`。手算：pool 1000、50% 预留、20% 效率 →
/// 500 / 1.2 = 416.666…。
#[test]
fn reservation_efficiency_divides() {
    let r = calc::reservation_with_efficiency(1000.0, 0.0, 50.0, 1.0, 20.0, 1.0);
    assert!(
        (r.reserved - 500.0 / 1.2).abs() < 1e-6,
        "got {}",
        r.reserved
    );

    // 负效率（reduced efficiency）→ 预留变多：500 / 0.8 = 625。
    let r = calc::reservation_with_efficiency(1000.0, 0.0, 50.0, 1.0, -20.0, 1.0);
    assert_eq!(r.reserved, 625.0);

    // 效率 −100%（除数 0）→ 发散，钳到池满（vendor :251 more>0 守卫之外的退化边界）。
    let r = calc::reservation_with_efficiency(1000.0, 100.0, 0.0, 1.0, -100.0, 1.0);
    assert_eq!(r.reserved, 1000.0);
}

/// ReservationMultiplier（:197 `floor(more, 4)`）：mult 1.3 → 预留 ×1.3；
/// floor 到四位小数（1.23456 → 1.2345）。
#[test]
fn reservation_multiplier_floor4() {
    let r = calc::reservation_with_efficiency(1000.0, 100.0, 0.0, 1.3, 0.0, 1.0);
    assert_eq!(r.reserved, 130.0);

    let r = calc::reservation_with_efficiency(10000.0, 10000.0, 0.0, 1.23456, 0.0, 1.0);
    // floor(1.23456, 4) = 1.2345 → 10000×1.2345 = 12345 → clamp 池 10000。
    assert_eq!(r.reserved, 10000.0);
    let r = calc::reservation_with_efficiency(100000.0, 10000.0, 0.0, 1.23456, 0.0, 1.0);
    assert_eq!(r.reserved, 12345.0);
}

/// Spirit 预留效率**不在**聚合侧二次应用：efficiency 是 per-skill 语义
/// （vendor CalcDefence.lua:240-243 按 skillCfg 求），PoBR 的应用点在注入侧
/// `spirit_reservation_modifiers`（每条 SkillSpiritReservationBase 注入前已除
/// 完）——`calc_spirit_reservation` 对聚合总量**不再**除 efficiency（旧实现的
/// 全局除法与注入侧构成双重应用）。
#[test]
fn spirit_reservation_efficiency_not_applied_twice_at_aggregate() {
    let db = db_from_texts(&["25% increased Spirit Reservation Efficiency"]);
    let cfg = CalcConfig::new();

    let r = calc::skill_mechanics::calc_spirit_reservation(&db, &cfg, 100.0);

    assert_eq!(r.final_cost, 100.0);
}
