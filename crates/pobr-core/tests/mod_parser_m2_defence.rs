//! M2 防御词条覆盖表（W0.1 契约测试）。
//!
//! 本表是 M2 各 track（A 扣池 / B taken-as / C keystone·转换矩阵 / D 面板族 / E evade·stun /
//! F EHP）的**接口契约**：每行 = 一个文本模式 → 期望 (ModName, ModType, 值)。各 track 的
//! ModDb 查询名以本表为准；改名 = 破坏性变更，须同步全部消费者。
//!
//! vendor 参照逐行标注（`vendor/PathOfBuilding-PoE2/src/Modules/ModParser.lua` /
//! `CalcDefence.lua` 行号，2026-06-11 核实）。W0.1 为纯解析增量（所有新 ModName 在该批
//! 合并时无 calc 消费者）；W0.1 过渡期 ArmourAppliesTo「instead of physical」的旧 flag
//! 双轨已由 Track B-2 收敛为百分比单一形态（覆盖表 3）。

use pobr_core::ModValue;
use pobr_core::mod_parser::{ParseStatus, parse_mod};
use pobr_data::prelude::*;

/// 期望产物：数值 mod（名 / 类型 / 值）。
struct N(&'static str, ModType, f64);

/// 断言 `text` 解析成功且产出 mods 与 `expected` 完全一致（顺序、数量、名、类型、值）。
/// flag 以 `ModType::Flag` + 值 1.0（`ModValue::Bool(true)` 视作命中）表达。
fn assert_parses(text: &str, expected: &[N]) {
    let outcome = parse_mod(text).unwrap_or_else(|e| panic!("`{text}` 解析失败: {e}"));
    assert_eq!(outcome.status, ParseStatus::Parsed, "`{text}` 应为 Parsed");
    assert_eq!(
        outcome.mods.len(),
        expected.len(),
        "`{text}` 产物数量不符: {:?}",
        outcome
            .mods
            .iter()
            .map(|m| m.name.as_str())
            .collect::<Vec<_>>()
    );
    for (i, (m, want)) in outcome.mods.iter().zip(expected).enumerate() {
        assert_eq!(m.name.as_str(), want.0, "`{text}` 第 {i} 条名不符");
        assert_eq!(
            m.mod_type, want.1,
            "`{text}` 第 {i} 条 ({}) 类型不符",
            want.0
        );
        if want.1 == ModType::Flag {
            assert_eq!(
                m.value,
                ModValue::Bool(true),
                "`{text}` 第 {i} 条应为 flag true"
            );
        } else {
            assert_eq!(
                m.value.as_number(),
                Some(want.2),
                "`{text}` 第 {i} 条 ({}) 值不符",
                want.0
            );
        }
    }
}

const FLAG: f64 = 0.0; // flag 行的占位值（不参与断言）。

/// 覆盖表 1：MoM 族 + EB（ModParser.lua:415-418 / :2389 / :2439）。
#[test]
fn m2_coverage_mom_and_eb() {
    use ModType::*;
    // ModParser.lua:415 nameList「damage is taken from mana before life」。
    assert_parses(
        "30% of damage is taken from mana before life",
        &[N("DamageTakenFromManaBeforeLife", Base, 30.0)],
    );
    // ModParser.lua:418 变体（无 is）。
    assert_parses(
        "25% of damage taken from mana before life",
        &[N("DamageTakenFromManaBeforeLife", Base, 25.0)],
    );
    // ModParser.lua:2389 全额形 → BASE 100。
    assert_parses(
        "All Damage is taken from Mana before Life",
        &[N("DamageTakenFromManaBeforeLife", Base, 100.0)],
    );
    // ModParser.lua:416 elemental 展开（Lightning/Cold/Fire 顺序与 vendor 一致）。
    assert_parses(
        "20% of Elemental Damage is taken from Mana before Life",
        &[
            N("LightningDamageTakenFromManaBeforeLife", Base, 20.0),
            N("ColdDamageTakenFromManaBeforeLife", Base, 20.0),
            N("FireDamageTakenFromManaBeforeLife", Base, 20.0),
        ],
    );
    // ModParser.lua:417 单类型（per-type 族对全部五类型同构）。
    assert_parses(
        "15% of Lightning Damage is taken from Mana before Life",
        &[N("LightningDamageTakenFromManaBeforeLife", Base, 15.0)],
    );
    assert_parses(
        "10% of Fire Damage taken from Mana before Life",
        &[N("FireDamageTakenFromManaBeforeLife", Base, 10.0)],
    );
    // ModParser.lua:2439 EB flag。
    assert_parses(
        "Energy Shield protects Mana instead of Life",
        &[N("EnergyShieldProtectsMana", Flag, FLAG)],
    );
    // ModParser.lua:3121 EternalLife flag（Lich 树「Eternal Life」节点文本；
    // CalcDefence.lua:587-594 / :2948-2950 互斥 ES 分支消费——M2-F 新管线）。
    assert_parses(
        "Your Life cannot change while you have Energy Shield",
        &[N("EternalLife", Flag, FLAG)],
    );
}

/// 覆盖表 2：ES / Ward bypass 族（ModParser.lua:2485-2507 / :430 / :4970 / :5393）。
#[test]
fn m2_coverage_es_ward_bypass() {
    use ModType::*;
    // ModParser.lua:2485-2491：全类型 OVERRIDE 100（胜过 does-not-bypass）。
    assert_parses(
        "All Damage taken bypasses Energy Shield",
        &[
            N("PhysicalEnergyShieldBypass", Override, 100.0),
            N("LightningEnergyShieldBypass", Override, 100.0),
            N("ColdEnergyShieldBypass", Override, 100.0),
            N("FireEnergyShieldBypass", Override, 100.0),
            N("ChaosEnergyShieldBypass", Override, 100.0),
        ],
    );
    // ModParser.lua:2492-2494。
    assert_parses(
        "Physical Damage taken bypasses Energy Shield",
        &[N("PhysicalEnergyShieldBypass", Base, 100.0)],
    );
    // ModParser.lua:2495-2501：百分比全类型。
    assert_parses(
        "25% of Damage taken bypasses Energy Shield",
        &[
            N("PhysicalEnergyShieldBypass", Base, 25.0),
            N("LightningEnergyShieldBypass", Base, 25.0),
            N("ColdEnergyShieldBypass", Base, 25.0),
            N("FireEnergyShieldBypass", Base, 25.0),
            N("ChaosEnergyShieldBypass", Base, 25.0),
        ],
    );
    // ModParser.lua:430 nameList「non-chaos damage taken bypasses energy shield」。
    assert_parses(
        "Non-Chaos Damage taken bypasses Energy Shield",
        &[
            N("PhysicalEnergyShieldBypass", Base, 100.0),
            N("LightningEnergyShieldBypass", Base, 100.0),
            N("ColdEnergyShieldBypass", Base, 100.0),
            N("FireEnergyShieldBypass", Base, 100.0),
        ],
    );
    // ModParser.lua:4970：does-not-bypass 抵扣形（BASE -N）。
    assert_parses(
        "50% of Chaos Damage taken does not bypass Energy Shield",
        &[N("ChaosEnergyShieldBypass", Base, -50.0)],
    );
    // ModParser.lua:5393 flag 形（裸句，无 during effect）。
    assert_parses(
        "Chaos Damage taken does not bypass Energy Shield",
        &[N("ChaosNotBypassEnergyShield", Flag, FLAG)],
    );
    // ModParser.lua:2507：Ward bypass。
    assert_parses(
        "10% of Damage taken bypasses Ward",
        &[N("WardBypass", Base, 10.0)],
    );
    // ModParser.lua:2506：ES inc/red 借给 Ward（keystone flag，Track C/D 消费）。
    assert_parses(
        "Increases and Reductions to Maximum Energy Shield instead apply to Ward",
        &[N("EnergyShieldToWard", Flag, FLAG)],
    );
}

/// 覆盖表 3：ArmourAppliesTo 百分比族（ModParser.lua:2519-2544；Track B 消费）。
#[test]
fn m2_coverage_armour_applies_to() {
    use ModType::*;
    // ModParser.lua:2519-2524「instead of physical」——百分比 BASE 100 +
    // ArmourDoesNotApplyToPhysicalDamageTaken flag（13-G7 单一形态；W0.1 过渡期的
    // ArmourAppliesTo<Element> 旧 flag 双轨已由 Track B-2 移除，消费侧 perform 改由
    // taken::build_mitigation_ctx 百分比快照派生 [bool;3]）。
    assert_parses(
        "Armour applies to Fire, Cold and Lightning Damage taken from Hits instead of Physical Damage",
        &[
            N("ArmourAppliesToFireDamageTaken", Base, 100.0),
            N("ArmourAppliesToColdDamageTaken", Base, 100.0),
            N("ArmourAppliesToLightningDamageTaken", Base, 100.0),
            N("ArmourDoesNotApplyToPhysicalDamageTaken", Flag, FLAG),
        ],
    );
    // ModParser.lua:2525-2529：百分比三元素（无 instead，物理护甲保留）。
    assert_parses(
        "50% of Armour applies to Fire, Cold and Lightning Damage taken from Hits",
        &[
            N("ArmourAppliesToFireDamageTaken", Base, 50.0),
            N("ArmourAppliesToColdDamageTaken", Base, 50.0),
            N("ArmourAppliesToLightningDamageTaken", Base, 50.0),
        ],
    );
    // ModParser.lua:2530-2534：elemental 全额形。
    assert_parses(
        "Armour applies to Elemental Damage",
        &[
            N("ArmourAppliesToFireDamageTaken", Base, 100.0),
            N("ArmourAppliesToColdDamageTaken", Base, 100.0),
            N("ArmourAppliesToLightningDamageTaken", Base, 100.0),
        ],
    );
    // ModParser.lua:2536-2540：「+N% of armour also applies to elemental damage」。
    assert_parses(
        "+30% of Armour also applies to Elemental Damage",
        &[
            N("ArmourAppliesToFireDamageTaken", Base, 30.0),
            N("ArmourAppliesToColdDamageTaken", Base, 30.0),
            N("ArmourAppliesToLightningDamageTaken", Base, 30.0),
        ],
    );
    // ModParser.lua:2541-2542：单类型全额 also 形。
    assert_parses(
        "Armour also applies to Chaos Damage taken from Hits",
        &[N("ArmourAppliesToChaosDamageTaken", Base, 100.0)],
    );
    // ModParser.lua:2543-2544：单类型百分比 also 形。
    assert_parses(
        "20% of Armour also applies to Fire Damage",
        &[N("ArmourAppliesToFireDamageTaken", Base, 20.0)],
    );
}

/// 覆盖表 4：taken-as 族（ModParser.lua:5599-5608；CalcDefence.lua:356-417 shiftTable 消费）。
#[test]
fn m2_coverage_taken_as() {
    use ModType::*;
    // ModParser.lua:5599。
    assert_parses(
        "30% of Cold Damage taken as Lightning",
        &[N("ColdDamageTakenAsLightning", Base, 30.0)],
    );
    // ModParser.lua:5600（带 damage 尾词变体）。
    assert_parses(
        "40% of Fire Damage taken as Lightning Damage",
        &[N("FireDamageTakenAsLightning", Base, 40.0)],
    );
    // from-hits 变体（CalcDefence.lua:361-364 与 TakenAs 同入 shiftTable）。
    assert_parses(
        "50% of Physical Damage from Hits taken as Fire Damage",
        &[N("PhysicalDamageFromHitsTakenAsFire", Base, 50.0)],
    );
    // ModParser.lua:5605-5609：随机元素三均分。
    assert_parses(
        "60% of Physical Damage from Hits taken as Damage of a random Element",
        &[
            N("PhysicalDamageFromHitsTakenAsFire", Base, 20.0),
            N("PhysicalDamageFromHitsTakenAsCold", Base, 20.0),
            N("PhysicalDamageFromHitsTakenAsLightning", Base, 20.0),
        ],
    );
}

/// 覆盖表 5：防御五元资源转换矩阵（CalcDefence.lua:1301-1390；ModParser.lua:2343）。
#[test]
fn m2_coverage_resource_conversion() {
    use ModType::*;
    // ModParser.lua:2343 IronReflexes：flag + EvasionConvertToArmour BASE 100。
    assert_parses(
        "Converts all Evasion Rating to Armour",
        &[
            N("IronReflexes", Flag, FLAG),
            N("EvasionConvertToArmour", Base, 100.0),
        ],
    );
    // 通用 ConvertTo（CalcDefence.lua:1313 `<Src>ConvertTo<Dst>` BASE 求和，cap 100）。
    assert_parses(
        "50% of Armour converted to Evasion Rating",
        &[N("ArmourConvertToEvasion", Base, 50.0)],
    );
    assert_parses(
        "30% of Maximum Energy Shield converted to Mana",
        &[N("EnergyShieldConvertToMana", Base, 30.0)],
    );
    // 通用 GainAs（CalcDefence.lua:1337 `<Src>GainAs<Dst>`，不减源）。
    assert_parses(
        "Gain 25% of Maximum Life as Extra Maximum Energy Shield",
        &[N("LifeGainAsEnergyShield", Base, 25.0)],
    );
    // 注：src == dst（如 Armour as Extra Armour）非法，反例见 m2_coverage_negative_guards。
}

/// 覆盖表 6：Block 族（ModParser.lua:365-383 / :2479 / :3027）。
#[test]
fn m2_coverage_block() {
    use ModType::*;
    // nameList +N% 符号形（parse_form Base 路径）。
    assert_parses("+5% Chance to Block", &[N("BlockChance", Base, 5.0)]);
    // 无符号「N% chance to ...」句形（防御几率族专用路径）。
    assert_parses("15% chance to Block", &[N("BlockChance", Base, 15.0)]);
    assert_parses(
        "20% chance to Block Spell Damage",
        &[N("SpellBlockChance", Base, 20.0)],
    );
    // 攻击+法术双格挡展开（ModParser.lua:378-380）。
    assert_parses(
        "10% chance to Block Attack and Spell Damage",
        &[
            N("BlockChance", Base, 10.0),
            N("SpellBlockChance", Base, 10.0),
        ],
    );
    // inc 形（聚合走 `(base+ΣBASE)×(1+inc)`，CalcDefence.lua:996-1006）。
    assert_parses("25% increased Block chance", &[N("BlockChance", Inc, 25.0)]);
    // 上限族（ModParser.lua:381-383；CalcDefence.lua:961-966 BlockChanceMax）。
    assert_parses(
        "+5% to Maximum Block Chance",
        &[N("BlockChanceMax", Base, 5.0)],
    );
    assert_parses(
        "+4% to Maximum Chance to Block Spell Damage",
        &[N("SpellBlockChanceMax", Base, 4.0)],
    );
    // 格挡承伤（ModParser.lua:2479 → BlockEffect BASE）。
    assert_parses(
        "You take 30% of Damage from Blocked Hits",
        &[N("BlockEffect", Base, 30.0)],
    );
    // 法术格挡等同攻击格挡（ModParser.lua:3027）。
    assert_parses(
        "Chance to Block Spell Damage is equal to Chance to Block Attack Damage",
        &[N("SpellBlockChanceIsBlockChance", Flag, FLAG)],
    );
}

/// 覆盖表 7：Evade 四分型（ModParser.lua:250-259 / :4988-4992）。
#[test]
fn m2_coverage_evade() {
    use ModType::*;
    assert_parses("+10% chance to Evade", &[N("EvadeChance", Base, 10.0)]);
    assert_parses(
        "8% chance to Evade Attack Hits",
        &[N("EvadeChance", Base, 8.0)],
    );
    assert_parses(
        "12% chance to Evade Projectile Attacks",
        &[N("ProjectileEvadeChance", Base, 12.0)],
    );
    assert_parses(
        "10% chance to Evade Spells",
        &[N("SpellEvadeChance", Base, 10.0)],
    );
    assert_parses(
        "15% chance to Evade Melee Attacks",
        &[N("MeleeEvadeChance", Base, 15.0)],
    );
    // ModParser.lua:4991（vendor MAX → PoBR Override，消费侧 clamp 语义）。
    assert_parses(
        "Maximum chance to Evade is 50%",
        &[N("EvadeChanceMax", Override, 50.0)],
    );
    // ModParser.lua:4992 / :4988 / :4989。
    assert_parses(
        "Chance to Evade is Unlucky",
        &[N("UnluckyEvade", Flag, FLAG)],
    );
    assert_parses(
        "Cannot Evade Enemy Attacks",
        &[N("CannotEvade", Flag, FLAG)],
    );
    assert_parses("Attacks cannot Hit you", &[N("AlwaysEvade", Flag, FLAG)]);
}

/// 覆盖表 8：Deflection（ModParser.lua:360-361 / :4158-4161）。
#[test]
fn m2_coverage_deflection() {
    use ModType::*;
    assert_parses(
        "+100 to Deflection Rating",
        &[N("DeflectionRating", Base, 100.0)],
    );
    assert_parses(
        "20% increased Deflection Rating",
        &[N("DeflectionRating", Inc, 20.0)],
    );
    // ModParser.lua:4158-4159。
    assert_parses(
        "Gain Deflection Rating equal to 30% of Evasion Rating",
        &[N("EvasionGainAsDeflection", Base, 30.0)],
    );
    assert_parses(
        "Gain Deflection Rating equal to 20% of Armour",
        &[N("ArmourGainAsDeflection", Base, 20.0)],
    );
    // ModParser.lua:4160。
    assert_parses(
        "Prevent +10% of Damage from Deflected Hits",
        &[N("DeflectEffect", Base, 10.0)],
    );
    // ModParser.lua:4161。
    assert_parses(
        "Chance to Deflect is Lucky",
        &[N("DeflectIsLucky", Flag, FLAG)],
    );
}

/// 覆盖表 9：预留效率 + Spirit + Ward（ModParser.lua:171-172 / :220-231 / :241）。
#[test]
fn m2_coverage_reservation_efficiency_spirit_ward() {
    use ModType::*;
    assert_parses(
        "20% increased Reservation Efficiency of Skills",
        &[N("ReservationEfficiency", Inc, 20.0)],
    );
    assert_parses(
        "15% increased Mana Reservation Efficiency of Skills",
        &[N("ManaReservationEfficiency", Inc, 15.0)],
    );
    assert_parses(
        "10% increased Life Reservation Efficiency",
        &[N("LifeReservationEfficiency", Inc, 10.0)],
    );
    assert_parses(
        "25% increased Spirit Reservation Efficiency",
        &[N("SpiritReservationEfficiency", Inc, 25.0)],
    );
    // Spirit 池本值（nameList :171-172，既有名）。
    assert_parses("+30 to Spirit", &[N("Spirit", Base, 30.0)]);
    // Ward 池（nameList :241）。
    assert_parses("+50 to Ward", &[N("Ward", Base, 50.0)]);
    assert_parses("20% increased Ward", &[N("Ward", Inc, 20.0)]);
}

/// 覆盖表 10：损失防止 + 眩晕（ModParser.lua:2816 / :5395-5398 / :399 / :449-450 / :5219-5220）。
#[test]
fn m2_coverage_loss_prevention_and_stun() {
    use ModType::*;
    // ModParser.lua:2816。
    assert_parses(
        "25% of Life Loss from Hits is Prevented, then that much Life is lost over 4 seconds instead",
        &[N("LifeLossPrevented", Base, 25.0)],
    );
    // ModParser.lua:5395-5398 双数值形。
    assert_parses(
        "When taking Damage from Hits, 30% of Life Loss is Prevented, then 50% of Life Loss Prevented this way is lost over 4 seconds",
        &[
            N("LifeLossPrevented", Base, 30.0),
            N("LifeLossLost", Base, 50.0),
        ],
    );
    // ModParser.lua:399 避免眩晕（calc_avoidance 既有消费者）。
    assert_parses(
        "20% chance to Avoid being Stunned",
        &[N("AvoidStun", Base, 20.0)],
    );
    assert_parses(
        "+15% chance to Avoid being Stunned",
        &[N("AvoidStun", Base, 15.0)],
    );
    // ModParser.lua:449（既有名）/ :450（组合展开）。
    assert_parses("+120 to Stun Threshold", &[N("StunThreshold", Base, 120.0)]);
    assert_parses(
        "+100 to Ailment and Stun Threshold",
        &[
            N("StunThreshold", Base, 100.0),
            N("AilmentThreshold", Base, 100.0),
        ],
    );
    assert_parses(
        "30% increased Stun Threshold",
        &[N("StunThreshold", Inc, 30.0)],
    );
    // ModParser.lua:5219-5220：阈值翻倍 → MORE 100。
    assert_parses(
        "Your Stun Threshold is doubled",
        &[N("StunThreshold", More, 100.0)],
    );
}

/// 反例守卫：同资源自转换（src == dst）不产出；未知句式仍归 Err/Unsupported 而非误吞。
#[test]
fn m2_coverage_negative_guards() {
    // src == dst 的资源转换非法 → 不应产出 ConvertTo/GainAs。
    let outcome = parse_mod("Gain 15% of Armour as Extra Armour");
    if let Ok(o) = outcome {
        assert!(
            o.mods.iter().all(|m| !m.name.as_str().contains("GainAs")),
            "src==dst 不应产出 GainAs"
        );
    }
    // 未知防御句式不被误吞为已解析数值（Err 也可接受——上层统一收集）。
    if let Ok(o) = parse_mod("50% of Sanity taken from Hope before Despair") {
        assert_eq!(
            o.status,
            ParseStatus::Unsupported,
            "未知句式应归 Unsupported"
        );
    }
}

/// M2 补刀：Blood Mage「Crimson Power」percent-stat 生命（ModParser.lua:2808-2813）
/// 与 Smith of Kitava「Body Armour grants」前缀族（:1418 / :3255-3268）。
#[test]
fn m2_followup_ascendancy_defence_words() {
    use pobr_core::modifier::ModTag;

    // Crimson Power：体甲件级 ES 的 N% 作额外生命（PercentStat 等价为 Multiplier）。
    let o = parse_mod(
        "Gain additional maximum Life equal to 100% of the [ItemEnergyShield|Item Energy Shield] on [Equipped] Body Armour",
    )
    .expect("parses");
    assert_eq!(o.status, ParseStatus::Parsed);
    let m = &o.mods[0];
    assert_eq!(m.name.as_str(), "MaximumLife");
    assert_eq!(m.mod_type, ModType::Base);
    assert!(m.tags.iter().any(|t| matches!(
        t,
        ModTag::Multiplier { var, div, .. } if var == "EnergyShieldOnbodyarmour" && *div == 1.0
    )));
    // 50% 变体 → div = 2（floor(bodyES/2)）。
    let o = parse_mod(
        "Gain additional maximum Life equal to 50% of the Energy Shield on Equipped Body Armour",
    )
    .expect("parses");
    assert!(o.mods[0].tags.iter().any(|t| matches!(
        t,
        ModTag::Multiplier { var, div, .. } if var == "EnergyShieldOnbodyarmour" && *div == 2.0
    )));

    // Body Armour grants 前缀：内层照常解析，全部 mod 追加 Normal 体甲装备条件。
    let o = parse_mod("Body Armour grants 30% increased [Spirit]").expect("parses");
    assert_eq!(o.status, ParseStatus::Parsed);
    let m = &o.mods[0];
    assert_eq!(m.name.as_str(), "Spirit");
    assert_eq!(m.mod_type, ModType::Inc);
    assert!(m.tags.iter().any(|t| matches!(
        t,
        ModTag::Condition { var, negated: false } if var == "NormalBodyArmourEquipped"
    )));
    // taken-as 内层（Molten Symbol）：前缀条件叠加在既有 taken-as 产物上。
    let o = parse_mod(
        "Body Armour grants 25% of [Physical|Physical] Damage from [HitDamage|Hits] taken as [Fire] Damage",
    )
    .expect("parses");
    let m = &o.mods[0];
    assert_eq!(m.name.as_str(), "PhysicalDamageFromHitsTakenAsFire");
    assert!(m.tags.iter().any(|t| matches!(
        t,
        ModTag::Condition { var, negated: false } if var == "NormalBodyArmourEquipped"
    )));
}

/// M2 补刀：固定减伤 / 暴击额外伤害减免词条（ninja 18-build 实测掉词补齐）。
#[test]
fn m2_followup_flat_dr_and_crit_reduction() {
    use ModType::*;

    // 「N% additional Physical Damage Reduction」（form ModParser.lua:75 `additional`
    // → BASE + nameList :263）；CalcDefence.lua:2381 固定减伤与护甲减伤相加后 clamp。
    assert_parses(
        "8% additional Physical Damage Reduction",
        &[N("PhysicalDamageReduction", Base, 8.0)],
    );
    // 元素变体（nameList :265）。
    assert_parses(
        "5% additional Elemental Damage Reduction",
        &[N("ElementalDamageReduction", Base, 5.0)],
    );
    // 「Take no Extra Damage from Critical Hits」（ModParser.lua:6129 →
    // ReduceCritExtraDamage BASE 100；CalcDefence.lua:1961/:2070 消费）。
    assert_parses(
        "Take no Extra Damage from Critical Hits",
        &[N("ReduceCritExtraDamage", Base, 100.0)],
    );
}
