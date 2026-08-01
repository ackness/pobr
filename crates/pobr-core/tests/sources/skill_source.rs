use pobr_core::calc::MinimalInput;
use pobr_core::skill_source::{
    ActiveSkillJudgeInput, ActiveSkillSpec, GemIngest, GemModSource, SkillGatingError,
    SupportGemSpec, SupportIngestError, SupportJudgeInput, can_support, ingest_active_gem_with_ctx,
    ingest_gem_leveled, ingest_gem_with_ctx, ingest_support_gem_with_ctx, judge_support,
};
use std::collections::HashSet;

/// Engine-backed ingest (signature matches the historical ctx-less entry points, but wired to the real rules).
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

/// Builds a set of active skill types (test convenience helper).
fn type_set(types: &[&str]) -> HashSet<String> {
    types.iter().map(|s| s.to_string()).collect()
}
use pobr_core::{CalcConfig, ModDb, ModTag};
use pobr_data::prelude::*;

// Original tests (backward compatibility with GemModSource)

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
    // Active gems have no parent skill source.
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

    // A support gem's modifier links to the active-skill source it supports.
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

// TODO(mana-multiplier): SupportManaMultiplier injection tests

/// A support gem carrying mana_multiplier = 40 → injects SupportManaMultiplier More +40.
#[test]
fn support_gem_mana_multiplier_injects_modifier() {
    let spec = SupportGemSpec::new("multistrike", [] as [&str; 0])
        .supporting("cleave")
        .with_mana_multiplier(40.0);

    let ingest = ingest_support_gem(&spec, &type_set(&[])).unwrap();

    // Only the mana multiplier modifier, no mod-text modifier.
    assert_eq!(ingest.modifiers.len(), 1);
    let m = &ingest.modifiers[0];
    assert_eq!(m.name, ModName::from("SupportManaMultiplier"));
    assert_eq!(m.mod_type, ModType::More);
    assert_eq!(m.value.as_number(), Some(40.0));

    // Attributed to the support gem source.
    let origin = m.origin.as_ref().unwrap();
    assert_eq!(origin.source_id.kind, SourceKind::SupportGem);
    assert_eq!(origin.source_id.id, "support.multistrike");
    // parent links to the supported active skill.
    let parent = origin.parent_source_id.as_ref().unwrap();
    assert_eq!(parent.kind, SourceKind::SkillGem);
    assert_eq!(parent.id, "gem.cleave");
}

/// Without mana_multiplier set → no SupportManaMultiplier modifier is injected.
#[test]
fn support_gem_without_mana_multiplier_no_extra_mod() {
    let spec = SupportGemSpec::new("added_fire", ["20% increased Fire Damage"]);

    let ingest = ingest_support_gem(&spec, &type_set(&[])).unwrap();

    // Only one mod-text modifier, no SupportManaMultiplier.
    let mana_mods: Vec<_> = ingest
        .modifiers
        .iter()
        .filter(|m| m.name == ModName::from("SupportManaMultiplier"))
        .collect();
    assert!(mana_mods.is_empty(), "expected no SupportManaMultiplier");
}

/// mana_multiplier correctly participates in the More product inside ModDb.
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

// TODO(more-multiplier isolation): supported_skill_types tag tests

/// Once supported_skill_types is set, the More modifier carries a SkillTypes tag
/// and doesn't apply under a non-matching CalcConfig.
#[test]
fn support_more_multiplier_isolated_by_skill_types() {
    // The support gem applies only to ATTACK skills and grants 30% more Damage.
    let spec = SupportGemSpec::new("brutality", ["30% more Damage"])
        .with_supported_skill_types(SkillTypes::ATTACK);

    let ingest = ingest_support_gem(&spec, &type_set(&[])).unwrap();

    let more_mods: Vec<_> = ingest
        .modifiers
        .iter()
        .filter(|m| m.mod_type == ModType::More)
        .collect();
    assert_eq!(more_mods.len(), 1);

    // Doesn't apply under a SPELL config (no overlap).
    let spell_cfg = CalcConfig::spell();
    let mut db = ModDb::new();
    db.add_list(ingest.modifiers.clone());
    let factor_spell = db.more(&spell_cfg, &[ModName::from("Damage")]);
    assert!(
        (factor_spell - 1.0).abs() < 1e-9,
        "spell config should not trigger attack-only more, got {factor_spell}"
    );

    // Applies under an ATTACK config (overlaps).
    let attack_cfg = CalcConfig::attack();
    let mut db2 = ModDb::new();
    db2.add_list(ingest.modifiers);
    let factor_attack = db2.more(&attack_cfg, &[ModName::from("Damage")]);
    assert!(
        (factor_attack - 1.3).abs() < 1e-9,
        "attack config should trigger more 30%, got {factor_attack}"
    );
}

/// supported_skill_types = NONE → the More modifier carries no tag and applies globally.
#[test]
fn support_more_multiplier_without_skill_types_tag_is_global() {
    let spec = SupportGemSpec::new("elemental_focus", ["30% more Fire Damage"]);

    let ingest = ingest_support_gem(&spec, &type_set(&[])).unwrap();

    let more_mods: Vec<_> = ingest
        .modifiers
        .iter()
        .filter(|m| m.mod_type == ModType::More)
        .collect();
    // Should not carry a SkillTypes tag.
    for m in &more_mods {
        let has_skill_types_tag = m.tags.iter().any(|t| matches!(t, ModTag::SkillTypes(_)));
        assert!(
            !has_skill_types_tag,
            "no SkillTypes tag expected when supported_skill_types is NONE"
        );
    }
}

/// Non-More modifiers aren't filtered by supported_skill_types (only More gets the tag).
#[test]
fn support_inc_modifier_never_gets_skill_types_tag() {
    // Even with supported_skill_types set, Inc modifiers should not carry a SkillTypes tag.
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

// skill-type-gating: PoB2's four-stage gating tests (CalcTools.lua:84-110)

/// Builds a require-expression token stream (test convenience helper).
fn tokens(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

/// Default active-side input: granted by a gem, supportable.
fn active_input(types: &HashSet<String>) -> ActiveSkillJudgeInput<'_> {
    ActiveSkillJudgeInput {
        cannot_be_supported: false,
        from_gem: true,
        skill_types: types,
    }
}

/// Stage 4: an empty require → always allowed (regardless of the active skill's type set).
#[test]
fn judge_support_empty_require_always_ok() {
    let support = SupportJudgeInput::default();
    for types in [type_set(&[]), type_set(&["Attack"]), type_set(&["Spell"])] {
        assert!(can_support(&support, &active_input(&types)));
    }
}

/// Stage 4: require expression matches → allowed; doesn't match → IncompatibleTypes.
/// Uses a real token stream: SupportAncestralWarriorTotemPlayer's require =
/// `["Attack","Totemable","AND"]` (postfix AND — both must be present to pass).
#[test]
fn judge_support_require_expression() {
    let require = tokens(&["Attack", "Totemable", "AND"]);
    let support = SupportJudgeInput {
        require_skill_types: &require,
        ..SupportJudgeInput::default()
    };

    // Attack + Totemable → matches.
    let both = type_set(&["Attack", "Totemable", "Melee"]);
    assert!(judge_support(&support, &active_input(&both)).is_ok());

    // Only Attack (missing Totemable) → AND doesn't hold → rejected.
    let only_attack = type_set(&["Attack"]);
    assert!(matches!(
        judge_support(&support, &active_input(&only_attack)),
        Err(SkillGatingError::IncompatibleTypes { .. })
    ));
}

/// Stage 3: an exclude expression hit → Excluded (takes priority over the require verdict).
/// Uses a real token stream: SupportArcaneSurgePlayer exclude =
/// `["UsedByProxy","Triggered","Persistent","HasReservation",
///   "ReservationBecomesCost","NOT","AND"]` — stack-machine semantics: a hit if any of the
/// first four items is true (an implicit residual OR); the trailing `ReservationBecomesCost NOT AND`
/// only constrains the last item.
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

    // A plain spell: doesn't hit exclude, require matches → passes.
    let spell = type_set(&["Spell", "Damage"]);
    assert!(judge_support(&support, &active_input(&spell)).is_ok());

    // A triggered spell: the exclude residual stack has Triggered=true → hit → rejected (even though require also matches).
    let triggered = type_set(&["Spell", "Triggered"]);
    assert!(matches!(
        judge_support(&support, &active_input(&triggered)),
        Err(SkillGatingError::Excluded { .. })
    ));
}

/// Stage 1: active-effect cannotBeSupported → unconditional rejection (even if support has no restrictions at all).
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

/// Stage 2: supportGemsOnly and the active skill isn't granted by a gem → rejected; granted by a gem → passes.
#[test]
fn judge_support_support_gems_only() {
    let support = SupportJudgeInput {
        support_gems_only: true,
        ..SupportJudgeInput::default()
    };
    let types = type_set(&["Attack"]);

    // Not granted by a gem (e.g. an item-granted skill) → rejected.
    let from_item = ActiveSkillJudgeInput {
        cannot_be_supported: false,
        from_gem: false,
        skill_types: &types,
    };
    assert_eq!(
        judge_support(&support, &from_item),
        Err(SkillGatingError::SupportGemsOnly)
    );

    // Granted by a gem → passes.
    assert!(judge_support(&support, &active_input(&types)).is_ok());
}

/// Gating order: the four stages run cannotBeSupported → supportGemsOnly → exclude → require;
/// when multiple rejection conditions hold at once, the earliest-hit stage is reported
/// (matches CalcTools.lua:84-110's early-exit order).
#[test]
fn judge_support_stage_order() {
    let require = tokens(&["Totemable"]);
    let exclude = tokens(&["Triggered"]);
    let support = SupportJudgeInput {
        support_gems_only: true,
        exclude_skill_types: &exclude,
        require_skill_types: &require,
    };
    // Triggers stages 2/3/4 at once → reports stage 2 (SupportGemsOnly).
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
    // Stage 1 takes priority over everything.
    let active1 = ActiveSkillJudgeInput {
        cannot_be_supported: true,
        ..active
    };
    assert_eq!(
        judge_support(&support, &active1),
        Err(SkillGatingError::CannotBeSupported)
    );
}

/// ingest_support_gem gating: a mismatched require expression reports a Gating error.
#[test]
fn ingest_support_gem_gates_incompatible_active_skill() {
    let spec = SupportGemSpec::new("melee_physical", ["20% more Physical Damage"])
        .with_require_skill_types(["Attack", "Melee", "AND"]);

    // Fails for a pure-spell active skill.
    let result = ingest_support_gem(&spec, &type_set(&["Spell"]));
    assert!(
        result.is_err(),
        "should reject non-attack/melee active skill"
    );
}

/// ingest_support_gem gating: no error when compatible.
#[test]
fn ingest_support_gem_allows_compatible_active_skill() {
    let spec = SupportGemSpec::new("added_fire", ["20% more Fire Damage"])
        .with_require_skill_types(["Attack"]);

    let result = ingest_support_gem(&spec, &type_set(&["Attack", "Melee"]));
    assert!(result.is_ok(), "should allow attack active skill");
}

/// ingest_support_gem gating: rejects when exclude hits (none of the values apply).
#[test]
fn ingest_support_gem_gates_excluded_active_skill() {
    let spec = SupportGemSpec::new("arcane_surge", ["10% more Cast Speed"])
        .with_require_skill_types(["Spell"])
        .with_exclude_skill_types(["Triggered"]);

    let result = ingest_support_gem(&spec, &type_set(&["Spell", "Triggered"]));
    assert!(result.is_err(), "excluded active skill should be rejected");
}

/// ingest_support_gem gating: passes regardless of active skill types when require is empty.
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

// TODO(level/quality scaling): level/quality attribution tests

/// ingest_support_gem attributes level modifiers to SourceKind::SkillLevel.
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
    // parent links to the supported active skill.
    assert_eq!(origin.parent_source_id.as_ref().unwrap().id, "gem.cleave");
}

/// ingest_support_gem attributes quality modifiers to SourceKind::GemQuality.
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

/// ingest_gem_leveled active-gem level attribution test.
#[test]
fn ingest_gem_leveled_active_gem() {
    let level_mods = vec![
        ("Damage".to_string(), ModType::Inc, 20.0),
        ("ManaCost".to_string(), ModType::Base, 10.0),
    ];
    let quality_mods = vec![("Damage".to_string(), ModType::Inc, 5.0)];

    let ingest = ingest_gem_leveled("fireball", 15, 20, &level_mods, &quality_mods);

    // 2 level modifiers + 1 quality modifier.
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
        // parent is the gem source.
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

/// Without level/quality set → no corresponding modifiers.
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

// SkillTypes extended bit-flag tests

/// SkillTypes's ATTACK and SPELL bits remain unchanged (backward compatibility).
#[test]
fn skill_types_attack_spell_bits_unchanged() {
    assert_eq!(SkillTypes::ATTACK.bits(), 1 << 0);
    assert_eq!(SkillTypes::SPELL.bits(), 1 << 1);
}

/// SkillTypes's new constants map to the correct bits (PoB2 enum value minus 1).
#[test]
fn skill_types_new_constants_correct_bits() {
    assert_eq!(SkillTypes::PROJECTILE.bits(), 1 << 2); // PoB2 index 3
    assert_eq!(SkillTypes::AREA.bits(), 1 << 7); // PoB2 index 8
    assert_eq!(SkillTypes::MELEE.bits(), 1 << 19); // PoB2 index 20
}

/// from_pob2_index maps correctly.
#[test]
fn skill_types_from_pob2_index() {
    assert_eq!(SkillTypes::from_pob2_index(1), SkillTypes::ATTACK);
    assert_eq!(SkillTypes::from_pob2_index(2), SkillTypes::SPELL);
    assert_eq!(SkillTypes::from_pob2_index(3), SkillTypes::PROJECTILE);
    assert_eq!(SkillTypes::from_pob2_index(0), SkillTypes::NONE);
}

/// BitOr combination.
#[test]
fn skill_types_bitor_works() {
    let combined = SkillTypes::ATTACK | SkillTypes::PROJECTILE;
    assert!(combined.intersects(SkillTypes::ATTACK));
    assert!(combined.intersects(SkillTypes::PROJECTILE));
    assert!(!combined.intersects(SkillTypes::SPELL));
}

// Combined test: mana-multiplier + skill-type-gating + level/quality together

/// End-to-end verification of a full support-gem spec (mana mult + gating + level/quality).
#[test]
fn full_support_gem_spec_end_to_end() {
    let spec = SupportGemSpec::new("multistrike", ["20% more Attack Damage"])
        .supporting("cleave")
        .with_mana_multiplier(40.0)
        .with_supported_skill_types(SkillTypes::ATTACK)
        .with_require_skill_types(["Attack"])
        .with_level(10, [("ManaCost".to_string(), ModType::Base, 8.0)])
        .with_quality(20, [("AttackSpeed".to_string(), ModType::Inc, 4.0)]);

    // An Attack active skill passes gating.
    let ingest = ingest_support_gem(&spec, &type_set(&["Attack", "Melee"])).unwrap();

    // Expect: 1 SupportManaMultiplier + 1 attack-mod more + 1 level + 1 quality = 4 modifiers.
    assert_eq!(
        ingest.modifiers.len(),
        4,
        "expected 4 modifiers, got {}",
        ingest.modifiers.len()
    );

    // A pure-spell active skill is rejected by gating (neither the values nor manaMultiplier apply).
    let rejected = ingest_support_gem(&spec, &type_set(&["Spell"]));
    assert!(rejected.is_err(), "Spell should be rejected by gating");
}

/// ingest_active_gem (new API) attributes the active gem correctly.
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
