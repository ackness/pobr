use pobr_core::calc::{CalculationSession, MinimalInput};
use pobr_core::skill_source::{
    ActiveSkillSpec, GemIngest, GemModSource, SkillGatingError, SupportGemSpec, can_support,
    ingest_active_gem, ingest_gem, ingest_gem_leveled, ingest_support_gem,
};
use pobr_core::{CalcConfig, ModDb, ModTag};
use pobr_data::prelude::*;

// ─────────────────────────────────────────────────────────────────────────────
// 原始测试（向后兼容 GemModSource）
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn ingest_active_gem_attributes_to_skill_gem_source() {
    let gem = GemModSource::active("fireball", ["20% increased maximum Life"]);

    let GemIngest {
        modifiers,
        unsupported,
    } = ingest_gem(&gem).unwrap();

    assert!(unsupported.is_empty());
    assert_eq!(modifiers.len(), 1);

    let origin = modifiers[0]
        .origin
        .as_ref()
        .expect("gem modifier carries an origin");
    assert_eq!(origin.source_id.kind, SourceKind::SkillGem);
    assert_eq!(origin.source_id.id, "gem.fireball");
    assert!(origin.raw_text.is_some());
    // 主动宝石没有父技能来源。
    assert!(origin.parent_source_id.is_none());
}

#[test]
fn ingest_support_gem_attributes_to_support_gem_source_with_parent() {
    let gem =
        GemModSource::support("added_fire", ["10% increased maximum Life"]).supporting("fireball");

    let ingest = ingest_gem(&gem).unwrap();

    assert_eq!(ingest.modifiers.len(), 1);
    let origin = ingest.modifiers[0].origin.as_ref().unwrap();
    assert_eq!(origin.source_id.kind, SourceKind::SupportGem);
    assert_eq!(origin.source_id.id, "support.added_fire");

    // support gem 的 modifier 关联到所支援的主动技能 source。
    let parent = origin
        .parent_source_id
        .as_ref()
        .expect("support gem links to the active skill it supports");
    assert_eq!(parent.kind, SourceKind::SkillGem);
    assert_eq!(parent.id, "gem.fireball");
}

#[test]
fn ingest_gem_collects_unsupported_texts() {
    let gem = GemModSource::active("fireball", ["20% increased maximum Life", "mirrored"]);

    let ingest = ingest_gem(&gem).unwrap();

    assert_eq!(ingest.modifiers.len(), 1);
    assert_eq!(ingest.unsupported, vec!["mirrored".to_string()]);
}

#[test]
fn ingested_gem_modifiers_attribute_back_through_contributions() {
    let gem = GemModSource::active("fireball", ["20% increased maximum Life"]);
    let ingest = ingest_gem(&gem).unwrap();

    let mut db = ModDb::new();
    db.add_list(ingest.modifiers);
    let cfg = CalcConfig::new();

    let contributions = db.contributions(ModType::Inc, &cfg, &[ModName::from("MaximumLife")]);
    assert_eq!(contributions.len(), 1);
    let origin = contributions[0]
        .origin
        .as_ref()
        .expect("contribution carries the gem origin");
    assert_eq!(origin.source_id.kind, SourceKind::SkillGem);
    assert_eq!(origin.source_id.id, "gem.fireball");
}

#[test]
fn session_add_skill_gem_feeds_minimal_calc() {
    let input = MinimalInput {
        base_life: 100.0,
        ..MinimalInput::default()
    };
    let mut session = CalculationSession::new(input);

    let active = GemModSource::active(
        "fireball",
        [
            "+40 to maximum Life",
            "20% increased maximum Life",
            "mirrored",
        ],
    );
    session.add_skill_gem(&active).unwrap();

    let output = session.perform_minimal();

    // (100 base + 40 gem) * (1 + 20/100) = 168。
    assert_eq!(output.life, 168.0);
    assert_eq!(session.unsupported_modifier_texts(), ["mirrored"]);
}

#[test]
fn session_add_support_gem_feeds_minimal_calc() {
    let input = MinimalInput {
        base_life: 100.0,
        ..MinimalInput::default()
    };
    let mut session = CalculationSession::new(input);

    let support =
        GemModSource::support("added_fire", ["+50 to maximum Life"]).supporting("fireball");
    session.add_support_gem(&support).unwrap();

    let output = session.perform_minimal();
    assert_eq!(output.life, 150.0);
}

// ─────────────────────────────────────────────────────────────────────────────
// TODO(mana-multiplier)：SupportManaMultiplier 注入测试
// ─────────────────────────────────────────────────────────────────────────────

/// 辅助宝石携带 mana_multiplier = 40 → 注入 SupportManaMultiplier More +40。
#[test]
fn support_gem_mana_multiplier_injects_modifier() {
    let spec = SupportGemSpec::new("multistrike", [] as [&str; 0])
        .supporting("cleave")
        .with_mana_multiplier(40.0);

    let ingest = ingest_support_gem(&spec, SkillTypes::NONE).unwrap();

    // 只有 mana multiplier modifier，无词条文本 modifier。
    assert_eq!(ingest.modifiers.len(), 1);
    let m = &ingest.modifiers[0];
    assert_eq!(m.name, ModName::from("SupportManaMultiplier"));
    assert_eq!(m.mod_type, ModType::More);
    assert_eq!(m.value.as_number(), Some(40.0));

    // 归因到辅助宝石 source。
    let origin = m.origin.as_ref().unwrap();
    assert_eq!(origin.source_id.kind, SourceKind::SupportGem);
    assert_eq!(origin.source_id.id, "support.multistrike");
    // parent 关联到被支援主动技能。
    let parent = origin.parent_source_id.as_ref().unwrap();
    assert_eq!(parent.kind, SourceKind::SkillGem);
    assert_eq!(parent.id, "gem.cleave");
}

/// 不设置 mana_multiplier → 不注入 SupportManaMultiplier modifier。
#[test]
fn support_gem_without_mana_multiplier_no_extra_mod() {
    let spec = SupportGemSpec::new("added_fire", ["20% increased Fire Damage"]);

    let ingest = ingest_support_gem(&spec, SkillTypes::NONE).unwrap();

    // 只有一个词条 modifier，没有 SupportManaMultiplier。
    let mana_mods: Vec<_> = ingest
        .modifiers
        .iter()
        .filter(|m| m.name == ModName::from("SupportManaMultiplier"))
        .collect();
    assert!(mana_mods.is_empty(), "expected no SupportManaMultiplier");
}

/// mana_multiplier 在 ModDb 中正确参与 More 乘积。
#[test]
fn support_mana_multiplier_contributes_to_more_product() {
    let spec = SupportGemSpec::new("multistrike", [] as [&str; 0]).with_mana_multiplier(50.0);
    let ingest = ingest_support_gem(&spec, SkillTypes::NONE).unwrap();

    let mut db = ModDb::new();
    db.add_list(ingest.modifiers);
    let cfg = CalcConfig::new();

    // More(SupportManaMultiplier) = 1 + 50/100 = 1.5
    let factor = db.more(&cfg, &[ModName::from("SupportManaMultiplier")]);
    assert!((factor - 1.5).abs() < 1e-9, "expected 1.5, got {factor}");
}

// ─────────────────────────────────────────────────────────────────────────────
// TODO(more-multiplier 隔离)：supported_skill_types tag 测试
// ─────────────────────────────────────────────────────────────────────────────

/// 设置 supported_skill_types 后，More modifier 附加 SkillTypes tag，
/// 在不匹配的 CalcConfig 下不生效。
#[test]
fn support_more_multiplier_isolated_by_skill_types() {
    // 辅助宝石只作用于 ATTACK 技能，并提供 30% more Damage。
    let spec = SupportGemSpec::new("brutality", ["30% more Damage"])
        .with_supported_skill_types(SkillTypes::ATTACK);

    let ingest = ingest_support_gem(&spec, SkillTypes::NONE).unwrap();

    let more_mods: Vec<_> = ingest
        .modifiers
        .iter()
        .filter(|m| m.mod_type == ModType::More)
        .collect();
    assert_eq!(more_mods.len(), 1);

    // 在 SPELL 配置下不生效（无交集）。
    let spell_cfg = CalcConfig::spell();
    let mut db = ModDb::new();
    db.add_list(ingest.modifiers.clone());
    let factor_spell = db.more(&spell_cfg, &[ModName::from("Damage")]);
    assert!(
        (factor_spell - 1.0).abs() < 1e-9,
        "spell config should not trigger attack-only more, got {factor_spell}"
    );

    // 在 ATTACK 配置下生效（有交集）。
    let attack_cfg = CalcConfig::attack();
    let mut db2 = ModDb::new();
    db2.add_list(ingest.modifiers);
    let factor_attack = db2.more(&attack_cfg, &[ModName::from("Damage")]);
    assert!(
        (factor_attack - 1.3).abs() < 1e-9,
        "attack config should trigger more 30%, got {factor_attack}"
    );
}

/// supported_skill_types 为 NONE → More modifier 不附加 tag，全局生效。
#[test]
fn support_more_multiplier_without_skill_types_tag_is_global() {
    let spec = SupportGemSpec::new("elemental_focus", ["30% more Fire Damage"]);

    let ingest = ingest_support_gem(&spec, SkillTypes::NONE).unwrap();

    let more_mods: Vec<_> = ingest
        .modifiers
        .iter()
        .filter(|m| m.mod_type == ModType::More)
        .collect();
    // 不应该有 SkillTypes tag。
    for m in &more_mods {
        let has_skill_types_tag = m.tags.iter().any(|t| matches!(t, ModTag::SkillTypes(_)));
        assert!(
            !has_skill_types_tag,
            "no SkillTypes tag expected when supported_skill_types is NONE"
        );
    }
}

/// more modifier 不被 supported_skill_types 过滤（非 More 不附加 tag）。
#[test]
fn support_inc_modifier_never_gets_skill_types_tag() {
    // 即使设置 supported_skill_types，Inc modifier 不应附加 SkillTypes tag。
    let spec = SupportGemSpec::new("concentrated_effect", ["40% increased Fire Damage"])
        .with_supported_skill_types(SkillTypes::SPELL);

    let ingest = ingest_support_gem(&spec, SkillTypes::NONE).unwrap();

    let inc_mods: Vec<_> = ingest
        .modifiers
        .iter()
        .filter(|m| m.mod_type == ModType::Inc)
        .collect();
    assert!(!inc_mods.is_empty(), "should have inc mods");
    for m in &inc_mods {
        let has_skill_types_tag = m.tags.iter().any(|t| matches!(t, ModTag::SkillTypes(_)));
        assert!(
            !has_skill_types_tag,
            "Inc modifiers should not get SkillTypes tag"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// TODO(skill-type-gating)：兼容性门控测试
// ─────────────────────────────────────────────────────────────────────────────

/// can_support：空 require → 始终允许。
#[test]
fn can_support_empty_require_always_ok() {
    assert!(can_support(SkillTypes::NONE, SkillTypes::NONE).is_ok());
    assert!(can_support(SkillTypes::NONE, SkillTypes::ATTACK).is_ok());
    assert!(can_support(SkillTypes::NONE, SkillTypes::SPELL).is_ok());
}

/// can_support：有交集 → 允许。
#[test]
fn can_support_intersecting_types_ok() {
    assert!(can_support(SkillTypes::ATTACK, SkillTypes::ATTACK).is_ok());
    // attack | projectile 辅助支援 attack 主动（有交集）。
    assert!(
        can_support(
            SkillTypes::ATTACK | SkillTypes::PROJECTILE,
            SkillTypes::ATTACK
        )
        .is_ok()
    );
}

/// can_support：无交集 → 返回 IncompatibleTypes 错误。
#[test]
fn can_support_disjoint_types_err() {
    let result = can_support(SkillTypes::ATTACK, SkillTypes::SPELL);
    assert!(matches!(
        result,
        Err(SkillGatingError::IncompatibleTypes { .. })
    ));
}

/// ingest_support_gem 门控：require_skill_types 有效时不兼容则报错。
#[test]
fn ingest_support_gem_gates_incompatible_active_skill() {
    let spec = SupportGemSpec::new("melee_physical", ["20% more Physical Damage"])
        .with_require_skill_types(SkillTypes::ATTACK | SkillTypes::MELEE);

    // 对 SPELL 主动技能失败。
    let result = ingest_support_gem(&spec, SkillTypes::SPELL);
    assert!(
        result.is_err(),
        "should reject non-attack/melee active skill"
    );
}

/// ingest_support_gem 门控：兼容时不报错。
#[test]
fn ingest_support_gem_allows_compatible_active_skill() {
    let spec = SupportGemSpec::new("added_fire", ["20% more Fire Damage"])
        .with_require_skill_types(SkillTypes::ATTACK);

    // 对 ATTACK 主动技能成功。
    let result = ingest_support_gem(&spec, SkillTypes::ATTACK);
    assert!(result.is_ok(), "should allow attack active skill");
}

/// ingest_support_gem 门控：require 为空时无论 active skill types 如何都通过。
#[test]
fn ingest_support_gem_no_require_always_ok() {
    let spec = SupportGemSpec::new("faster_casting", ["20% more Cast Speed"]);

    // 无 require → NONE, ATTACK, SPELL 均通过。
    for types in [SkillTypes::NONE, SkillTypes::ATTACK, SkillTypes::SPELL] {
        assert!(
            ingest_support_gem(&spec, types).is_ok(),
            "should pass for any active skill types when require is empty"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// TODO(level/quality 缩放)：等级/品质归因测试
// ─────────────────────────────────────────────────────────────────────────────

/// ingest_support_gem 等级 modifier 归因到 SourceKind::SkillLevel。
#[test]
fn support_gem_level_mods_attributed_to_skill_level_source() {
    let spec = SupportGemSpec::new("added_fire", [] as [&str; 0])
        .supporting("cleave")
        .with_level(20, [("ManaCost".to_string(), ModType::Base, 5.0)]);

    let ingest = ingest_support_gem(&spec, SkillTypes::NONE).unwrap();

    let level_mods: Vec<_> = ingest
        .modifiers
        .iter()
        .filter(|m| {
            m.origin
                .as_ref()
                .map(|o| o.source_id.kind == SourceKind::SkillLevel)
                .unwrap_or(false)
        })
        .collect();
    assert_eq!(level_mods.len(), 1, "expected 1 level modifier");
    assert_eq!(level_mods[0].name, ModName::from("ManaCost"));
    assert_eq!(level_mods[0].value.as_number(), Some(5.0));

    let origin = level_mods[0].origin.as_ref().unwrap();
    assert_eq!(origin.source_id.id, "support.added_fire.level20");
    // parent 关联到被支援主动技能。
    assert_eq!(origin.parent_source_id.as_ref().unwrap().id, "gem.cleave");
}

/// ingest_support_gem 品质 modifier 归因到 SourceKind::GemQuality。
#[test]
fn support_gem_quality_mods_attributed_to_gem_quality_source() {
    let spec = SupportGemSpec::new("added_fire", [] as [&str; 0])
        .with_quality(20, [("FireDamage".to_string(), ModType::Inc, 10.0)]);

    let ingest = ingest_support_gem(&spec, SkillTypes::NONE).unwrap();

    let quality_mods: Vec<_> = ingest
        .modifiers
        .iter()
        .filter(|m| {
            m.origin
                .as_ref()
                .map(|o| o.source_id.kind == SourceKind::GemQuality)
                .unwrap_or(false)
        })
        .collect();
    assert_eq!(quality_mods.len(), 1, "expected 1 quality modifier");
    assert_eq!(quality_mods[0].name, ModName::from("FireDamage"));
    assert_eq!(quality_mods[0].value.as_number(), Some(10.0));
    assert_eq!(
        quality_mods[0].origin.as_ref().unwrap().source_id.id,
        "support.added_fire.q20"
    );
}

/// ingest_gem_leveled 主动宝石等级归因测试。
#[test]
fn ingest_gem_leveled_active_gem() {
    let level_mods = vec![
        ("Damage".to_string(), ModType::Inc, 20.0),
        ("ManaCost".to_string(), ModType::Base, 10.0),
    ];
    let quality_mods = vec![("Damage".to_string(), ModType::Inc, 5.0)];

    let ingest = ingest_gem_leveled("fireball", 15, 20, &level_mods, &quality_mods);

    // 2 个等级 modifier + 1 个品质 modifier。
    assert_eq!(ingest.modifiers.len(), 3);

    let lvl: Vec<_> = ingest
        .modifiers
        .iter()
        .filter(|m| {
            m.origin
                .as_ref()
                .map(|o| o.source_id.kind == SourceKind::SkillLevel)
                .unwrap_or(false)
        })
        .collect();
    assert_eq!(lvl.len(), 2, "expected 2 level mods");
    for m in &lvl {
        let origin = m.origin.as_ref().unwrap();
        assert_eq!(origin.source_id.id, "gem.fireball.level15");
        // parent 为宝石 source。
        assert_eq!(origin.parent_source_id.as_ref().unwrap().id, "gem.fireball");
    }

    let qlt: Vec<_> = ingest
        .modifiers
        .iter()
        .filter(|m| {
            m.origin
                .as_ref()
                .map(|o| o.source_id.kind == SourceKind::GemQuality)
                .unwrap_or(false)
        })
        .collect();
    assert_eq!(qlt.len(), 1, "expected 1 quality mod");
    assert_eq!(
        qlt[0].origin.as_ref().unwrap().source_id.id,
        "gem.fireball.q20"
    );
}

/// level/quality 不设置 → 无对应 modifier。
#[test]
fn support_gem_without_level_quality_no_extra_mods() {
    let spec = SupportGemSpec::new("added_fire", ["20% more Fire Damage"]);
    let ingest = ingest_support_gem(&spec, SkillTypes::NONE).unwrap();

    let level_quality_mods: Vec<_> = ingest
        .modifiers
        .iter()
        .filter(|m| {
            m.origin
                .as_ref()
                .map(|o| {
                    o.source_id.kind == SourceKind::SkillLevel
                        || o.source_id.kind == SourceKind::GemQuality
                })
                .unwrap_or(false)
        })
        .collect();
    assert!(
        level_quality_mods.is_empty(),
        "no level/quality mods expected"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// SkillTypes 扩展位标志测试
// ─────────────────────────────────────────────────────────────────────────────

/// SkillTypes 的 ATTACK 和 SPELL 保持不变（向后兼容）。
#[test]
fn skill_types_attack_spell_bits_unchanged() {
    assert_eq!(SkillTypes::ATTACK.bits(), 1 << 0);
    assert_eq!(SkillTypes::SPELL.bits(), 1 << 1);
}

/// SkillTypes 的新常量对应正确的 bit（从 PoB2 枚举值 - 1）。
#[test]
fn skill_types_new_constants_correct_bits() {
    assert_eq!(SkillTypes::PROJECTILE.bits(), 1 << 2); // PoB2 index 3
    assert_eq!(SkillTypes::AREA.bits(), 1 << 7); // PoB2 index 8
    assert_eq!(SkillTypes::MELEE.bits(), 1 << 19); // PoB2 index 20
}

/// from_pob2_index 正确映射。
#[test]
fn skill_types_from_pob2_index() {
    assert_eq!(SkillTypes::from_pob2_index(1), SkillTypes::ATTACK);
    assert_eq!(SkillTypes::from_pob2_index(2), SkillTypes::SPELL);
    assert_eq!(SkillTypes::from_pob2_index(3), SkillTypes::PROJECTILE);
    assert_eq!(SkillTypes::from_pob2_index(0), SkillTypes::NONE);
}

/// BitOr 组合。
#[test]
fn skill_types_bitor_works() {
    let combined = SkillTypes::ATTACK | SkillTypes::PROJECTILE;
    assert!(combined.intersects(SkillTypes::ATTACK));
    assert!(combined.intersects(SkillTypes::PROJECTILE));
    assert!(!combined.intersects(SkillTypes::SPELL));
}

// ─────────────────────────────────────────────────────────────────────────────
// 综合测试：mana-multiplier + skill-type-gating + level/quality 联合
// ─────────────────────────────────────────────────────────────────────────────

/// 完整辅助宝石规格（mana mult + 门控 + 等级/品质）端到端验证。
#[test]
fn full_support_gem_spec_end_to_end() {
    let spec = SupportGemSpec::new("multistrike", ["20% more Attack Damage"])
        .supporting("cleave")
        .with_mana_multiplier(40.0)
        .with_supported_skill_types(SkillTypes::ATTACK)
        .with_require_skill_types(SkillTypes::ATTACK)
        .with_level(10, [("ManaCost".to_string(), ModType::Base, 8.0)])
        .with_quality(20, [("AttackSpeed".to_string(), ModType::Inc, 4.0)]);

    // ATTACK 主动技能通过门控。
    let ingest = ingest_support_gem(&spec, SkillTypes::ATTACK).unwrap();

    // 应有：1 SupportManaMultiplier + 1 攻击词条 more + 1 等级 + 1 品质 = 4 modifier。
    assert_eq!(
        ingest.modifiers.len(),
        4,
        "expected 4 modifiers, got {}",
        ingest.modifiers.len()
    );

    // SPELL 主动技能被门控拒绝。
    let rejected = ingest_support_gem(&spec, SkillTypes::SPELL);
    assert!(rejected.is_err(), "SPELL should be rejected by gating");
}

/// ingest_active_gem（新 API）主动宝石正确归因。
#[test]
fn ingest_active_gem_spec_attribution() {
    let spec = ActiveSkillSpec::new(
        "fireball",
        SkillTypes::SPELL | SkillTypes::PROJECTILE,
        ["20% increased maximum Life"],
    );
    let ingest = ingest_active_gem(&spec).unwrap();

    assert_eq!(ingest.modifiers.len(), 1);
    let origin = ingest.modifiers[0].origin.as_ref().unwrap();
    assert_eq!(origin.source_id.kind, SourceKind::SkillGem);
    assert_eq!(origin.source_id.id, "gem.fireball");
}
