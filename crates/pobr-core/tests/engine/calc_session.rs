use pobr_core::calc::MinimalInput;

use crate::support::session;

#[test]
fn session_parses_modifier_texts_and_calculates_minimal_output() {
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

    let mut session = session(input);
    session
        .add_modifier_texts([
            "+50 to maximum Life",
            "20% increased maximum Life",
            "+35% to Fire Resistance",
            "Attacks deal 50% increased Physical Damage",
            "20% more Physical Damage",
            "10% increased Attack Speed",
        ])
        .unwrap();

    let output = session.perform_minimal();

    assert_eq!(output.life, 1_260.0);
    assert_eq!(output.fire_resistance, 35.0);
    assert_eq!(output.total_hit_avg, 270.0);
    assert_eq!(output.action_rate, 2.2);
    assert_eq!(output.dps, 594.0);
}

#[test]
fn session_preserves_accuracy_inputs_for_hit_chance_and_dps() {
    let input = MinimalInput {
        base_life: 1.0,
        base_mana: 1.0,
        base_fire_resistance: 0.0,
        base_cold_resistance: 0.0,
        base_lightning_resistance: 0.0,
        base_accuracy: 400.0,
        enemy_evasion: 1_000.0,
        base_hit_min: 100.0,
        base_hit_max: 100.0,
        base_action_rate: 1.0,
    };
    let mut session = session(input);
    session
        .add_modifier_texts(["+200 to Accuracy Rating"])
        .unwrap();

    let output = session.perform_minimal();
    let expected_hit_chance = pobr_core::calc::hit_chance(1_000.0, 600.0);

    assert_eq!(output.total_hit_avg, 100.0);
    assert_eq!(output.hit_chance, expected_hit_chance);
    assert_eq!(output.dps, 100.0 * expected_hit_chance);
}

/// 属性最终总量（PoB2 `calculateAttributes`，CalcPerform.lua:381-388）：
/// `round((class_base + Σbase) × (1 + Σinc/100) × Πmore)`，下限 0。
/// `N% increased Dexterity` 类词条必须缩放含职业起始在内的全部 BASE。
#[test]
fn attribute_total_applies_increased_attribute_modifiers() {
    // Arrange
    let mut session = session(MinimalInput::default());
    session
        .add_modifier_texts(["+100 to Dexterity", "8% increased Dexterity"])
        .unwrap();

    // Act + Assert：round((20 + 100) × 1.08) = round(129.6) = 130。
    assert_eq!(session.attribute_total("Dexterity", 20.0), 130.0);
    // 无 INC 词条的属性 = class_base + Σbase 直通（Strength 无任何词条）。
    assert_eq!(session.attribute_total("Strength", 7.0), 7.0);
}

/// 资源池最终总量（vendor PerStat 分母 = actor output，ModStore.lua:440-460）：
/// `pool_total` 必须吃满 base×(1+inc)×more 全管线，与 perform 内 offence 池值
/// 同源——BASE-only（`base_sum`）会漏掉 inc/more 缩放。
#[test]
fn pool_total_applies_full_pool_pipeline() {
    // Arrange
    let input = MinimalInput {
        base_mana: 100.0,
        ..MinimalInput::default()
    };
    let mut session = session(input);
    session
        .add_modifier_texts(["+200 to maximum Mana", "50% increased maximum Mana"])
        .unwrap();

    // Act + Assert：(100 + 200) × 1.5 = 450（base_sum 只会给 200）。
    assert_eq!(session.pool_total("MaximumMana"), 450.0);
    assert_eq!(session.base_sum("MaximumMana"), 200.0);

    // 池值与 perform 输出同源（同一 scaled_pool 管线）。
    let output = session.perform_minimal();
    assert_eq!(output.mana, 450.0);
}

#[test]
fn session_preserves_unsupported_modifier_texts() {
    let mut session = session(MinimalInput::default());
    session.add_modifier_texts(["Mirrored"]).unwrap();

    assert_eq!(session.unsupported_modifier_texts(), ["Mirrored"]);
}

/// tag 后缀从句（阈值/条件）合法地消费尾巴并给 mod 挂 tag，即使留装饰性残留，
/// mod 仍须注入——不得被「Parsed+残留」一刀切降级（曾致 RedSupportGems 阈值失效）。
#[test]
fn session_injects_tag_suffixed_mod_despite_leftover() {
    let mut session = session(MinimalInput::default());
    session
        .add_modifier_texts([
            "5% increased maximum Life if you have at least 10 Red Support Gems Socketed",
        ])
        .expect("engine never errors");

    // 阈值 mod 已注入（带 MultiplierThreshold tag，非空聚合）；残留碎片不阻断注入。
    assert!(
        !session.mods_named("Life").is_empty() || !session.mods_named("MaximumLife").is_empty(),
        "tag-suffixed threshold mod must be injected"
    );
}

#[test]
fn session_collects_unknown_modifier_text_as_unsupported() {
    // 引擎对无法识别的文本永不报错——整行进 unsupported 收集面。
    let mut session = session(MinimalInput::default());

    session
        .add_modifier_texts(["not a real modifier"])
        .expect("engine never errors on unknown text");

    assert_eq!(
        session.unsupported_modifier_texts(),
        ["not a real modifier"]
    );
}

/// 投射物速度 → 投射物伤害转换（vendor CalcOffence.lua:840-845）：
/// `ProjectileSpeedAppliesToProjectileDamage` flag 激活时，INC ProjectileSpeed
/// 逐条复制为 Damage INC（flags 替换为 Projectile）；flag 缺位零行为。
#[test]
fn projectile_speed_applies_to_projectile_damage_conversion() {
    use pobr_core::{CalcConfig, Modifier};
    use pobr_data::modifier::ModFlags;

    let input = MinimalInput {
        base_hit_min: 100.0,
        base_hit_max: 100.0,
        base_action_rate: 1.0,
        ..MinimalInput::default()
    };
    let cfg =
        CalcConfig::attack().with_flags(ModFlags::ATTACK | ModFlags::PROJECTILE | ModFlags::HIT);

    // 无 flag：ProjectileSpeed 无消费方，零行为。
    let mut without = session(input).with_config(cfg.clone());
    without
        .add_modifier_texts(["8% increased Projectile Speed"])
        .unwrap();
    let base = without.perform_minimal();
    assert_eq!(base.total_hit_avg, 100.0);

    // flag 激活：8% Projectile Speed → 8% increased Damage (Projectile)。
    let mut with = session(input).with_config(cfg);
    with.add_modifier_texts(["8% increased Projectile Speed"])
        .unwrap();
    with.add_modifiers([Modifier::flag("ProjectileSpeedAppliesToProjectileDamage")]);
    let converted = with.perform_minimal();
    assert_eq!(converted.total_hit_avg, 108.0);

    // 带 flags 限定的源 mod（如 for Spell Skills）不参与转换（vendor Tabulate 空 cfg 口径）。
    let mut scoped = session(MinimalInput {
        base_hit_min: 100.0,
        base_hit_max: 100.0,
        base_action_rate: 1.0,
        ..MinimalInput::default()
    })
    .with_config(
        CalcConfig::attack().with_flags(ModFlags::ATTACK | ModFlags::PROJECTILE | ModFlags::HIT),
    );
    scoped
        .add_modifier_texts(["8% increased Projectile Speed for Spell Skills"])
        .unwrap();
    scoped.add_modifiers([Modifier::flag("ProjectileSpeedAppliesToProjectileDamage")]);
    let scoped_out = scoped.perform_minimal();
    assert_eq!(scoped_out.total_hit_avg, 100.0);
}

/// 弓变体（树 notable『Feathered Fletching』，ModParser.lua:3648 →
/// `ProjectileSpeedAppliesToBowDamage`；消费 CalcOffence.lua:796-802）：INC
/// ProjectileSpeed 复制为 Damage INC（flags 替换为 Bow|Hit，vendor Tabulate
/// `{ flags = ModFlag.Bow }`）；非弓 cfg（无 BOW 位）副本不命中。
#[test]
fn projectile_speed_applies_to_bow_damage_conversion() {
    use pobr_core::{CalcConfig, Modifier};
    use pobr_data::modifier::ModFlags;

    let input = MinimalInput {
        base_hit_min: 100.0,
        base_hit_max: 100.0,
        base_action_rate: 1.0,
        ..MinimalInput::default()
    };
    let bow_cfg = CalcConfig::attack()
        .with_flags(ModFlags::ATTACK | ModFlags::PROJECTILE | ModFlags::HIT | ModFlags::BOW);

    // 解析链端到端：notable 原文 → flag → 转换生效（46% 投速 → +46% Damage）。
    let mut with = session(input).with_config(bow_cfg.clone());
    with.add_modifier_texts([
        "46% increased Projectile Speed",
        "Increases and Reductions to [Projectile|Projectile] Speed also apply to Damage with [Bow|Bows]",
    ])
    .unwrap();
    let converted = with.perform_minimal();
    assert_eq!(converted.total_hit_avg, 146.0);

    // 非弓技能 cfg（无 BOW 位）：副本 flags=Bow|Hit 不是 cfg 子集 → 不命中。
    let mut non_bow = session(input).with_config(
        CalcConfig::attack().with_flags(ModFlags::ATTACK | ModFlags::PROJECTILE | ModFlags::HIT),
    );
    non_bow
        .add_modifier_texts(["46% increased Projectile Speed"])
        .unwrap();
    non_bow.add_modifiers([Modifier::flag("ProjectileSpeedAppliesToBowDamage")]);
    let non_bow_out = non_bow.perform_minimal();
    assert_eq!(non_bow_out.total_hit_avg, 100.0);
}
