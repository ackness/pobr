//! End-to-end verification of support-applicability gating (18-G2).
//!
//! Via the full **XML fixture -> parse_build -> calculate_with_data** chain, asserts:
//! 1. **Incompatible support is rejected**: `SupportFerociousRoarPlayer`
//!    (requires `[Warcry]`, PoB2 `Data/Skills/sup_str.lua` Ferocious Roar
//!    `requireSkillTypes`) socketed into a Fireball group (a spell, no
//!    `Warcry` type) must have its `damage_+%` **excluded** from hit/DPS —
//!    matching PoB2 `Modules/CalcActiveSkill.lua:210-214`: only a support that
//!    passes `canGrantedEffectSupportActiveSkill` (CalcTools.lua:84-110) makes
//!    it into `effectList`.
//! 2. **Compatible support still injects**: `SupportMetaCastFireSpellOnHitPlayer`
//!    (requires `[Spell, Triggerable, Fire, AND, AND]`, all satisfied by
//!    Fireball; exclude `[InbuiltTrigger]` doesn't match) has its `damage_+%`
//!    boost hit as normal (the INC channel isn't collateral damage).
//!
//! Uses real ingested data from `data/4.5.0.3.4/` (granted_effects.json's
//! require/exclude token stream is parsed from the GrantedEffects .dat type
//! columns).

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};
use pobr_core::calc::MinimalInput;
use pobr_data::monster::EnemyTier;
use pobr_gamedata::{GameData, repo_data_root};

fn load_build_data() -> BuildData {
    let data = GameData::new(repo_data_root().join(pobr_gamedata::data_version()));
    BuildData::load(&data).expect("load BuildData from repo data")
}

fn panel_opts() -> DataOrchestratorOptions {
    DataOrchestratorOptions {
        base_input: MinimalInput::default(),
        inject_character_base: true,
        enemy_level: 0,
        enemy_tier: EnemyTier::None,
        mode_effective: false,
        extra_modifier_texts: vec![],
        ..Default::default()
    }
}

/// Minimal Fireball build XML (with an optional additional support gem).
fn fireball_xml(support: Option<(&str, &str)>) -> String {
    let support_gem = support
        .map(|(gem_id, skill_id)| {
            format!(
                r#"<Gem enabled="true" gemId="{gem_id}" skillId="{skill_id}" level="20" quality="0"/>"#
            )
        })
        .unwrap_or_default();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<PathOfBuilding2>
  <Build level="90" className="Sorceress" ascendClassName="" mainSocketGroup="1"/>
  <Skills activeSkillSet="1">
    <SkillSet id="1">
      <Skill enabled="true" slot="weapon1">
        <Gem enabled="true" gemId="Metadata/Items/Gems/Fireball" skillId="FireballPlayer" level="20" quality="0"/>
        {support_gem}
      </Skill>
    </SkillSet>
  </Skills>
</PathOfBuilding2>"#
    )
}

/// Incompatible support (requires `[Warcry]` vs a spell) is rejected: its
/// multiplier does not reach hit/DPS.
///
/// Before the fix (injecting unconditionally based on `is_support`), this
/// support's `damage_+%` (L20 ≈ +30) would be wrongly injected into Fireball —
/// in PoB2, Ferocious Roar can only support Warcry skills.
#[test]
fn incompatible_support_is_rejected_end_to_end() {
    let build_data = load_build_data();

    // Precondition: this support does carry a mappable damage_+% (the data channel isn't broken — rejection isn't a false positive from "no value").
    let sup = build_data.effect_stats("SupportFerociousRoarPlayer", 20, 0, None);
    let inc = sup
        .base
        .iter()
        .find(|s| s.stat == "damage_+%")
        .expect("FerociousRoar should carry damage_+%");
    assert!(inc.value > 0.0);

    let bare = parse_build(&fireball_xml(None)).expect("parse bare build");
    let with_incompatible = parse_build(&fireball_xml(Some((
        "Metadata/Items/Gem/SupportGemFerociousRoar",
        "SupportFerociousRoarPlayer",
    ))))
    .expect("parse build with incompatible support");

    let base = calculate_with_data(&bare, &build_data, &panel_opts()).expect("bare calc");
    let gated =
        calculate_with_data(&with_incompatible, &build_data, &panel_opts()).expect("gated calc");

    assert!(base.total_hit_avg > 0.0, "Fireball 基线击中应非零");
    assert!(
        (gated.total_hit_avg - base.total_hit_avg).abs() < 1e-9,
        "不兼容 support 的 damage_+% 不得进击中：base {} vs gated {}",
        base.total_hit_avg,
        gated.total_hit_avg
    );
    assert!(
        (gated.dps - base.dps).abs() < 1e-9,
        "不兼容 support 不得改变 DPS：base {} vs gated {}",
        base.dps,
        gated.dps
    );
}

/// Compatible support (requires `[Spell, Triggerable, Fire, AND, AND]`, all
/// matched) injects normally: `damage_+%` (L20 = 200) boosts hit x3 through the
/// INC channel — gating doesn't collateral-damage the compatible list.
#[test]
fn compatible_support_still_injects() {
    let build_data = load_build_data();

    let sup = build_data.effect_stats("SupportMetaCastFireSpellOnHitPlayer", 20, 0, None);
    let inc = sup
        .base
        .iter()
        .find(|s| s.stat == "damage_+%")
        .expect("MetaCastFireSpellOnHit should carry damage_+%");
    assert!(inc.value > 0.0);

    let bare = parse_build(&fireball_xml(None)).expect("parse bare build");
    let with_compatible = parse_build(&fireball_xml(Some((
        "Metadata/Items/Gem/SupportGemCastOnFireHit",
        "SupportMetaCastFireSpellOnHitPlayer",
    ))))
    .expect("parse build with compatible support");

    let base = calculate_with_data(&bare, &build_data, &panel_opts()).expect("bare calc");
    let boosted =
        calculate_with_data(&with_compatible, &build_data, &panel_opts()).expect("boosted calc");

    let expected = base.total_hit_avg * (1.0 + inc.value / 100.0);
    assert!(
        (boosted.total_hit_avg - expected).abs() < 1.0,
        "兼容 support damage_+% ({}) 应缩放击中：base {} → 期望 {}（实得 {}）",
        inc.value,
        base.total_hit_avg,
        expected,
        boosted.total_hit_avg
    );
}
