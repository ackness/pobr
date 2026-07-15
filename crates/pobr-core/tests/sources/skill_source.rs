use pobr_core::calc::MinimalInput;
use pobr_core::skill_source::{
    ActiveSkillJudgeInput, ActiveSkillSpec, GemIngest, GemModSource, SkillGatingError,
    SupportGemSpec, SupportIngestError, SupportJudgeInput, can_support, ingest_active_gem_with_ctx,
    ingest_gem_leveled, ingest_gem_with_ctx, ingest_support_gem_with_ctx, judge_support,
};
use std::collections::HashSet;

/// engine 版 ingest（签名对齐历史无 ctx 入口，注入真实规则）。
fn ingest_gem(gem: &GemModSource) -> Result<GemIngest, pobr_core::mod_parser::ParseError> {
    ingest_gem_with_ctx(gem, crate::support::ctx())
}
#[allow(dead_code)]
fn ingest_active_gem(
    spec: &ActiveSkillSpec,
) -> Result<GemIngest, pobr_core::mod_parser::ParseError> {
    ingest_active_gem_with_ctx(spec, crate::support::ctx())
}
fn ingest_support_gem(
    spec: &SupportGemSpec,
    active_skill_types: &HashSet<String>,
) -> Result<GemIngest, SupportIngestError> {
    ingest_support_gem_with_ctx(spec, active_skill_types, crate::support::ctx())
}

/// 构造主动技能类型集合（测试便捷）。
fn type_set(types: &[&str]) -> HashSet<String> {
    types.iter().map(|s| s.to_string()).collect()
}
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
    let mut session = crate::support::session(input);

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
    let mut session = crate::support::session(input);

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

    let ingest = ingest_support_gem(&spec, &type_set(&[])).unwrap();

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

    let ingest = ingest_support_gem(&spec, &type_set(&[])).unwrap();

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
    let ingest = ingest_support_gem(&spec, &type_set(&[])).unwrap();

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

    let ingest = ingest_support_gem(&spec, &type_set(&[])).unwrap();

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

    let ingest = ingest_support_gem(&spec, &type_set(&[])).unwrap();

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

    let ingest = ingest_support_gem(&spec, &type_set(&[])).unwrap();

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
// skill-type-gating：PoB2 四段裁决测试（CalcTools.lua:84-110）
// ─────────────────────────────────────────────────────────────────────────────

/// 构造 require 表达式 token 流（测试便捷）。
fn tokens(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

/// 默认主动侧输入：宝石授予、可被支援。
fn active_input(types: &HashSet<String>) -> ActiveSkillJudgeInput<'_> {
    ActiveSkillJudgeInput {
        cannot_be_supported: false,
        from_gem: true,
        skill_types: types,
    }
}

/// 第四段：空 require → 始终允许（无论主动技能类型集合如何）。
#[test]
fn judge_support_empty_require_always_ok() {
    let support = SupportJudgeInput::default();
    for types in [type_set(&[]), type_set(&["Attack"]), type_set(&["Spell"])] {
        assert!(can_support(&support, &active_input(&types)));
    }
}

/// 第四段：require 表达式匹配 → 允许；不匹配 → IncompatibleTypes。
/// 用真实 token 流：SupportAncestralWarriorTotemPlayer require =
/// `["Attack","Totemable","AND"]`（后缀 AND，两者皆备才通过）。
#[test]
fn judge_support_require_expression() {
    let require = tokens(&["Attack", "Totemable", "AND"]);
    let support = SupportJudgeInput {
        require_skill_types: &require,
        ..SupportJudgeInput::default()
    };

    // Attack + Totemable → 匹配。
    let both = type_set(&["Attack", "Totemable", "Melee"]);
    assert!(judge_support(&support, &active_input(&both)).is_ok());

    // 仅 Attack（缺 Totemable）→ AND 不成立 → 拒。
    let only_attack = type_set(&["Attack"]);
    assert!(matches!(
        judge_support(&support, &active_input(&only_attack)),
        Err(SkillGatingError::IncompatibleTypes { .. })
    ));
}

/// 第三段：exclude 表达式命中 → Excluded（优先于 require 判定）。
/// 用真实 token 流：SupportArcaneSurgePlayer exclude =
/// `["UsedByProxy","Triggered","Persistent","HasReservation",
///   "ReservationBecomesCost","NOT","AND"]`——栈机语义：前四项任一真即命中
/// （残留隐式 OR），末段 `ReservationBecomesCost NOT AND` 仅约束最后一项。
#[test]
fn judge_support_exclude_expression_hits() {
    let require = tokens(&["Spell"]);
    let exclude = tokens(&[
        "UsedByProxy",
        "Triggered",
        "Persistent",
        "HasReservation",
        "ReservationBecomesCost",
        "NOT",
        "AND",
    ]);
    let support = SupportJudgeInput {
        require_skill_types: &require,
        exclude_skill_types: &exclude,
        ..SupportJudgeInput::default()
    };

    // 普通法术：不命中 exclude，require 匹配 → 通过。
    let spell = type_set(&["Spell", "Damage"]);
    assert!(judge_support(&support, &active_input(&spell)).is_ok());

    // 触发法术：exclude 残留栈含 Triggered=true → 命中 → 拒（即使 require 也匹配）。
    let triggered = type_set(&["Spell", "Triggered"]);
    assert!(matches!(
        judge_support(&support, &active_input(&triggered)),
        Err(SkillGatingError::Excluded { .. })
    ));
}

/// 第一段：主动效果 cannotBeSupported → 无条件拒绝（即使 support 无任何限制）。
#[test]
fn judge_support_cannot_be_supported_rejects_first() {
    let support = SupportJudgeInput::default();
    let types = type_set(&["Spell"]);
    let active = ActiveSkillJudgeInput {
        cannot_be_supported: true,
        from_gem: true,
        skill_types: &types,
    };
    assert_eq!(
        judge_support(&support, &active),
        Err(SkillGatingError::CannotBeSupported)
    );
}

/// 第二段：supportGemsOnly 且主动技能非宝石授予 → 拒；宝石授予 → 通过。
#[test]
fn judge_support_support_gems_only() {
    let support = SupportJudgeInput {
        support_gems_only: true,
        ..SupportJudgeInput::default()
    };
    let types = type_set(&["Attack"]);

    // 非宝石授予（如物品授予技能）→ 拒。
    let from_item = ActiveSkillJudgeInput {
        cannot_be_supported: false,
        from_gem: false,
        skill_types: &types,
    };
    assert_eq!(
        judge_support(&support, &from_item),
        Err(SkillGatingError::SupportGemsOnly)
    );

    // 宝石授予 → 通过。
    assert!(judge_support(&support, &active_input(&types)).is_ok());
}

/// 裁决顺序：四段按 cannotBeSupported → supportGemsOnly → exclude → require，
/// 同时满足多个拒绝条件时报最先命中的段（对齐 CalcTools.lua:84-110 早退顺序）。
#[test]
fn judge_support_stage_order() {
    let require = tokens(&["Totemable"]);
    let exclude = tokens(&["Triggered"]);
    let support = SupportJudgeInput {
        support_gems_only: true,
        exclude_skill_types: &exclude,
        require_skill_types: &require,
    };
    // 同时触发段 2/3/4 → 报段 2（SupportGemsOnly）。
    let types = type_set(&["Triggered"]);
    let active = ActiveSkillJudgeInput {
        cannot_be_supported: false,
        from_gem: false,
        skill_types: &types,
    };
    assert_eq!(
        judge_support(&support, &active),
        Err(SkillGatingError::SupportGemsOnly)
    );
    // 段 1 优先于一切。
    let active1 = ActiveSkillJudgeInput {
        cannot_be_supported: true,
        ..active
    };
    assert_eq!(
        judge_support(&support, &active1),
        Err(SkillGatingError::CannotBeSupported)
    );
}

/// ingest_support_gem 门控：require 表达式不匹配则报 Gating 错误。
#[test]
fn ingest_support_gem_gates_incompatible_active_skill() {
    let spec = SupportGemSpec::new("melee_physical", ["20% more Physical Damage"])
        .with_require_skill_types(["Attack", "Melee", "AND"]);

    // 对纯法术主动技能失败。
    let result = ingest_support_gem(&spec, &type_set(&["Spell"]));
    assert!(
        result.is_err(),
        "should reject non-attack/melee active skill"
    );
}

/// ingest_support_gem 门控：兼容时不报错。
#[test]
fn ingest_support_gem_allows_compatible_active_skill() {
    let spec = SupportGemSpec::new("added_fire", ["20% more Fire Damage"])
        .with_require_skill_types(["Attack"]);

    let result = ingest_support_gem(&spec, &type_set(&["Attack", "Melee"]));
    assert!(result.is_ok(), "should allow attack active skill");
}

/// ingest_support_gem 门控：exclude 命中时拒收（数值全不吃）。
#[test]
fn ingest_support_gem_gates_excluded_active_skill() {
    let spec = SupportGemSpec::new("arcane_surge", ["10% more Cast Speed"])
        .with_require_skill_types(["Spell"])
        .with_exclude_skill_types(["Triggered"]);

    let result = ingest_support_gem(&spec, &type_set(&["Spell", "Triggered"]));
    assert!(result.is_err(), "excluded active skill should be rejected");
}

/// ingest_support_gem 门控：require 为空时无论 active skill types 如何都通过。
#[test]
fn ingest_support_gem_no_require_always_ok() {
    let spec = SupportGemSpec::new("faster_casting", ["20% more Cast Speed"]);

    for types in [type_set(&[]), type_set(&["Attack"]), type_set(&["Spell"])] {
        assert!(
            ingest_support_gem(&spec, &types).is_ok(),
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

    let ingest = ingest_support_gem(&spec, &type_set(&[])).unwrap();

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

    let ingest = ingest_support_gem(&spec, &type_set(&[])).unwrap();

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
    let ingest = ingest_support_gem(&spec, &type_set(&[])).unwrap();

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
        .with_require_skill_types(["Attack"])
        .with_level(10, [("ManaCost".to_string(), ModType::Base, 8.0)])
        .with_quality(20, [("AttackSpeed".to_string(), ModType::Inc, 4.0)]);

    // Attack 主动技能通过门控。
    let ingest = ingest_support_gem(&spec, &type_set(&["Attack", "Melee"])).unwrap();

    // 应有：1 SupportManaMultiplier + 1 攻击词条 more + 1 等级 + 1 品质 = 4 modifier。
    assert_eq!(
        ingest.modifiers.len(),
        4,
        "expected 4 modifiers, got {}",
        ingest.modifiers.len()
    );

    // 纯法术主动技能被门控拒绝（数值/manaMultiplier 全不吃）。
    let rejected = ingest_support_gem(&spec, &type_set(&["Spell"]));
    assert!(rejected.is_err(), "Spell should be rejected by gating");
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
