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

    assert!(
        base.total_hit_avg > 0.0,
        "Fireball's baseline hit should be non-zero"
    );
    assert!(
        (gated.total_hit_avg - base.total_hit_avg).abs() < 1e-9,
        "an incompatible support's damage_+% must not reach hit: base {} vs gated {}",
        base.total_hit_avg,
        gated.total_hit_avg
    );
    assert!(
        (gated.dps - base.dps).abs() < 1e-9,
        "an incompatible support must not change DPS: base {} vs gated {}",
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
        "a compatible support's damage_+% ({}) should scale hit: base {} → expected {} (got {})",
        inc.value,
        base.total_hit_avg,
        expected,
        boosted.total_hit_avg
    );
}

/// Supported levels belong to a gem instance's group, not the first matching
/// skill id in the build. This also applies to an explicitly selected disabled group.
#[test]
fn supported_levels_stay_in_the_selected_group() {
    use pobr_build::{Build, CharacterIdentity, SocketGroup, calculate_skill_metrics};
    let data = load_build_data();
    let plain = SocketGroup::new().with_gem_skill("FireballPlayer", 20);
    let mastery = plain.clone().with_gem_skill("SupportFireMasteryPlayer", 1);
    let base = Build::new().with_character(CharacterIdentity {
        level: 90,
        class_name: "Witch".into(),
        ascendancy_name: String::new(),
    });
    let single = |group: SocketGroup| {
        calculate_with_data(
            &base
                .clone()
                .add_socket_group(group)
                .with_main_socket_group(1),
            &data,
            &panel_opts(),
        )
        .unwrap()
    };
    let plain_out = single(plain.clone());
    let mastery_out = single(mastery.clone());
    assert!(mastery_out.dps > plain_out.dps);
    for (groups, expected) in [
        ([plain.clone(), mastery.clone()], [&plain_out, &mastery_out]),
        ([mastery.clone(), plain.clone()], [&mastery_out, &plain_out]),
    ] {
        let build = base
            .clone()
            .add_socket_group(groups[0].clone())
            .add_socket_group(groups[1].clone());
        for (index, wanted) in expected.iter().enumerate() {
            let out = calculate_with_data(
                &build.clone().with_main_socket_group(index + 1),
                &data,
                &panel_opts(),
            )
            .unwrap();
            assert_eq!(
                out.dps, wanted.dps,
                "group {index} must use its own supports"
            );
            assert_eq!(out.damage_components, wanted.damage_components);
        }
        let report = calculate_skill_metrics(&build, &data, &panel_opts()).unwrap();
        for (index, wanted) in expected.iter().enumerate() {
            let row = report
                .skills
                .iter()
                .find(|row| row.group_index == index && row.active_skill_index == 1)
                .unwrap();
            assert_eq!(row.output.as_ref().unwrap().dps, wanted.dps);
        }
    }
    let disabled = single(mastery.with_enabled(false));
    assert_eq!(disabled.dps, mastery_out.dps);
}

const FORK: &str = "SupportForkPlayer";
const FORK_STAT: &str = "support_fork_forked_projectile_damage_+%_final";

fn coverage_build(host: &str, support: Option<&str>) -> pobr_build::Build {
    let mut group = pobr_build::SocketGroup::new().with_gem_skill(host, 20);
    if let Some(support) = support {
        group = group.with_gem_skill(support, 1);
    }
    pobr_build::Build::new()
        .add_socket_group(group)
        .with_main_socket_group(1)
}

fn coverage_session(
    build: &pobr_build::Build,
    data: &BuildData,
) -> pobr_core::calc::CalculationSession {
    pobr_build::calculate_with_data_session(build, data, &panel_opts()).unwrap()
}

fn fork_receipt(session: &pobr_core::calc::CalculationSession) -> &str {
    let receipts: Vec<_> = session
        .unsupported_modifier_texts()
        .iter()
        .filter(|text| text.contains(FORK_STAT))
        .collect();
    assert_eq!(
        receipts.len(),
        1,
        "one limit for the calculated Fork payload"
    );
    receipts[0]
}

fn assert_fork_receipt(text: &str, set: u32, value: f64, host: &str) {
    // Stable source identity, not explanatory prose or Debug formatting.
    for token in [
        format!("Support:{FORK}:"),
        format!("set={set}:"),
        format!("stat={FORK_STAT}:"),
        format!("value={value}:"),
        format!("host={host}:"),
    ] {
        assert!(text.contains(&token), "missing {token:?} in {text:?}");
    }
}

fn coverage_mapping() -> pobr_data::catalog::stat_map::SkillStatMapDef {
    let path = repo_data_root()
        .join(pobr_gamedata::data_version())
        .join("overlay/skill_stat_map.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn support_coverage_normal_session_discloses_actual_fork_without_changing_numbers() {
    use pobr_core::rules::stat_map_engine::{MappedOutcome, UnsupportedReason, map_stat};

    let data = load_build_data();
    let payload = data.effect_stats(FORK, 1, 0, Some(1));
    let stat = payload
        .base
        .iter()
        .find(|stat| stat.stat == FORK_STAT)
        .unwrap();
    assert_eq!(stat.value, -30.0);
    assert_eq!(
        map_stat(
            data.stat_map_catalog.as_deref().unwrap(),
            FORK,
            Some("1"),
            FORK_STAT,
            stat.value
        ),
        MappedOutcome::Unsupported(UnsupportedReason::UnsupportedTag("StatThreshold".into()))
    );
    let build = coverage_build("FireballPlayer", Some(FORK));
    let normal = coverage_session(&build, &data);
    assert_fork_receipt(fork_receipt(&normal), 1, -30.0, "FireballPlayer");
    let compare = pobr_build::calculate_with_data_report(
        &build,
        &data,
        &DataOrchestratorOptions {
            stat_map_mode: pobr_build::StatMapMode::Compare,
            ..panel_opts()
        },
    )
    .unwrap();
    assert_eq!(normal.output(), compare.session.output());
    assert_eq!(
        normal.unsupported_modifier_texts(),
        compare.session.unsupported_modifier_texts()
    );
    let repeat = coverage_session(&build, &data);
    assert_eq!(normal.output(), repeat.output());
    assert_eq!(
        normal.unsupported_modifier_texts(),
        repeat.unsupported_modifier_texts()
    );

    // The skipped penalty belongs to forked projectiles, not every primary hit.
    let bare = coverage_session(&coverage_build("FireballPlayer", None), &data);
    assert!(bare.output().dps > 0.0);
    assert_eq!(normal.output().total_hit_avg, bare.output().total_hit_avg);
    assert_eq!(normal.output().dps, bare.output().dps);
    assert_eq!(
        normal.output().damage_components,
        bare.output().damage_components
    );
    assert!(
        normal.output().mana_cost > bare.output().mana_cost,
        "Fork's separately modeled cost multiplier is preserved"
    );
}

#[test]
fn support_coverage_removed_disabled_and_incompatible_payloads_do_not_disclose_fork() {
    let data = load_build_data();
    let enabled = fireball_xml(Some(("Metadata/Items/Gem/SupportGemFork", FORK)));
    let disabled = enabled.replace(
        r#"<Gem enabled="true" gemId="Metadata/Items/Gem/SupportGemFork""#,
        r#"<Gem enabled="false" gemId="Metadata/Items/Gem/SupportGemFork""#,
    );
    let equipped = coverage_session(&parse_build(&enabled).unwrap(), &data);
    assert_fork_receipt(fork_receipt(&equipped), 1, -30.0, "FireballPlayer");
    let bare_build = parse_build(&fireball_xml(None)).unwrap();
    let bare = coverage_session(&bare_build, &data);
    let disabled = coverage_session(&parse_build(&disabled).unwrap(), &data);
    assert_eq!(disabled.output(), bare.output());
    assert!(disabled.unsupported_modifier_texts().is_empty());
    assert!(bare.unsupported_modifier_texts().is_empty());
    let incompatible = coverage_session(&coverage_build("ArmourBreakerPlayer", Some(FORK)), &data);
    let melee_bare = coverage_session(&coverage_build("ArmourBreakerPlayer", None), &data);
    assert_eq!(incompatible.output(), melee_bare.output());
    assert!(incompatible.unsupported_modifier_texts().is_empty());
    let reequipped = coverage_session(&parse_build(&enabled).unwrap(), &data);
    assert_eq!(
        reequipped.unsupported_modifier_texts(),
        equipped.unsupported_modifier_texts()
    );
}

#[test]
fn support_coverage_zero_payload_is_not_a_limit() {
    let mut data = load_build_data();
    // Synthetic zero-valued input to the real mapping/ingestion boundary.
    let stat = data.skill_stat_sets.get_mut(FORK).unwrap().sets[0]
        .constant_stats
        .iter_mut()
        .find(|stat| stat.stat == FORK_STAT)
        .unwrap();
    stat.value = 0.0;
    let session = coverage_session(&coverage_build("FireballPlayer", Some(FORK)), &data);
    assert!(session.unsupported_modifier_texts().is_empty());
    let bare = coverage_session(&coverage_build("FireballPlayer", None), &data);
    assert_eq!(session.output().total_hit_avg, bare.output().total_hit_avg);
    assert_eq!(session.output().dps, bare.output().dps);
    assert!(session.output().mana_cost > bare.output().mana_cost);
}

#[test]
fn support_coverage_selected_set_owns_payload_value_and_identity() {
    use pobr_core::rules::stat_map_engine::StatMapCatalog;
    use pobr_data::catalog::stat_map::SkillStatMapDef;
    use std::sync::Arc;

    let mut data = load_build_data();
    // Fork has one real set. A synthetic second form reuses its actual mapping;
    // only input set/value vary, never the engine outcome or diagnostic.
    let sets = &mut data.skill_stat_sets.get_mut(FORK).unwrap().sets;
    sets[0].vendor_set_index = Some(1);
    let mut second = sets[0].clone();
    second.vendor_set_index = Some(2);
    second.set_id = "SyntheticForkSecondForm".into();
    second
        .constant_stats
        .iter_mut()
        .find(|stat| stat.stat == FORK_STAT)
        .unwrap()
        .value = -15.0;
    sets.push(second);
    let mut mapping: SkillStatMapDef = coverage_mapping();
    let fork_sets = mapping.per_stat_set.get_mut(FORK).unwrap();
    fork_sets.insert("2".into(), fork_sets["1"].clone());
    data.stat_map_catalog = Some(Arc::new(StatMapCatalog::new(mapping)));
    let build = |set| {
        pobr_build::Build::new()
            .add_socket_group(
                pobr_build::SocketGroup::new()
                    .with_gem_skill("FireballPlayer", 20)
                    .with_gem_skill_stat_set(FORK, 1, set),
            )
            .with_main_socket_group(1)
    };
    let first = coverage_session(&build(1), &data);
    let second = coverage_session(&build(2), &data);
    assert_fork_receipt(fork_receipt(&first), 1, -30.0, "FireballPlayer");
    assert_fork_receipt(fork_receipt(&second), 2, -15.0, "FireballPlayer");
    assert_ne!(
        first.unsupported_modifier_texts(),
        second.unsupported_modifier_texts()
    );
    assert_eq!(first.output(), second.output());
    assert_eq!(
        coverage_session(&build(1), &data).unsupported_modifier_texts(),
        first.unsupported_modifier_texts()
    );

    // Set 1 still contains -30; selecting a zero-valued set 2 must not leak it.
    data.skill_stat_sets.get_mut(FORK).unwrap().sets[1]
        .constant_stats
        .iter_mut()
        .find(|stat| stat.stat == FORK_STAT)
        .unwrap()
        .value = 0.0;
    assert!(
        coverage_session(&build(2), &data)
            .unsupported_modifier_texts()
            .is_empty()
    );
    assert_fork_receipt(
        fork_receipt(&coverage_session(&build(1), &data)),
        1,
        -30.0,
        "FireballPlayer",
    );
}

#[test]
fn support_coverage_multiple_groups_do_not_leak_another_hosts_payload() {
    use pobr_build::{Build, SocketGroup};

    let data = load_build_data();
    let build = Build::new()
        .add_socket_group(SocketGroup::new().with_gem_skill("FireballPlayer", 20))
        .add_socket_group(
            SocketGroup::new()
                .with_gem_skill("FireballPlayer", 20)
                .with_gem_skill(FORK, 1),
        )
        .add_socket_group(
            SocketGroup::new()
                .with_gem_skill("BallLightningPlayer", 20)
                .with_gem_skill(FORK, 1),
        );
    let bare = coverage_session(&build.clone().with_main_socket_group(1), &data);
    assert!(bare.unsupported_modifier_texts().is_empty());
    let fire = coverage_session(&build.clone().with_main_socket_group(2), &data);
    let ice = coverage_session(&build.clone().with_main_socket_group(3), &data);
    assert_fork_receipt(fork_receipt(&fire), 1, -30.0, "FireballPlayer");
    assert_fork_receipt(fork_receipt(&ice), 1, -30.0, "BallLightningPlayer");
    assert_ne!(
        fire.unsupported_modifier_texts(),
        ice.unsupported_modifier_texts()
    );
    assert_eq!(
        fire.output(),
        coverage_session(&coverage_build("FireballPlayer", Some(FORK)), &data).output()
    );
    assert_eq!(
        ice.output(),
        coverage_session(&coverage_build("BallLightningPlayer", Some(FORK)), &data).output()
    );
    assert!(
        coverage_session(&build.with_main_socket_group(1), &data)
            .unsupported_modifier_texts()
            .is_empty()
    );
}

#[test]
fn support_coverage_mapped_distance_ramp_and_separately_handled_levels_are_not_limits() {
    use pobr_core::rules::stat_map_engine::{
        MappedItem, MappedOutcome, UnsupportedReason, map_stat,
    };

    let data = load_build_data();
    let catalog = data.stat_map_catalog.as_deref().unwrap();
    let ramp_stat = "support_close_combat_attack_damage_+%_final_from_distance";
    let ramp_payload = data.effect_stats("SupportCloseCombatPlayer", 1, 0, Some(1));
    let ramp_value = ramp_payload
        .base
        .iter()
        .find(|stat| stat.stat == ramp_stat)
        .unwrap()
        .value;
    let MappedOutcome::Mapped(items) = map_stat(
        catalog,
        "SupportCloseCombatPlayer",
        Some("1"),
        ramp_stat,
        ramp_value,
    ) else {
        panic!("Close Combat must really map, not be an empty negative control");
    };
    assert!(items.iter().any(|item| matches!(item,
        MappedItem::Modifier(modifier) if modifier.tags.iter().any(|tag| matches!(tag, pobr_core::ModTag::DistanceRamp { .. }))
    )));
    let close_combat = coverage_session(
        &coverage_build("ArmourBreakerPlayer", Some("SupportCloseCombatPlayer")),
        &data,
    );
    assert!(close_combat.unsupported_modifier_texts().is_empty());

    let level_stat = "supported_fire_skill_gem_level_+";
    let mastery_payload = data.effect_stats("SupportFireMasteryPlayer", 1, 0, Some(1));
    let level = mastery_payload
        .base
        .iter()
        .find(|stat| stat.stat == level_stat)
        .unwrap()
        .value;
    assert_eq!(level, 1.0);
    assert_eq!(
        map_stat(
            catalog,
            "SupportFireMasteryPlayer",
            Some("1"),
            level_stat,
            level
        ),
        MappedOutcome::Unsupported(UnsupportedReason::UnsupportedModType("LIST".into()))
    );
    let mastery = coverage_session(
        &coverage_build("FireballPlayer", Some("SupportFireMasteryPlayer")),
        &data,
    );
    let bare = coverage_session(&coverage_build("FireballPlayer", None), &data);
    assert!(
        mastery.output().dps > bare.output().dps,
        "the separate gem-level consumer must actually affect Fireball"
    );
    assert!(mastery.unsupported_modifier_texts().is_empty());
}

#[test]
fn support_coverage_identity_uses_effective_set_and_survives_support_compaction() {
    let data = load_build_data();
    let baseline = coverage_session(&coverage_build("FireballPlayer", Some(FORK)), &data);
    let group = pobr_build::SocketGroup::new()
        .with_gem_skill("FireballPlayer", 20)
        .with_gem_skill("SupportFireMasteryPlayer", 1)
        .with_gem_skill_stat_set(FORK, 1, 1);
    let build = pobr_build::Build::new()
        .add_socket_group(group)
        .with_main_socket_group(1);
    assert_eq!(
        coverage_session(&build, &data).unsupported_modifier_texts(),
        baseline.unsupported_modifier_texts()
    );
    let mut fallback = build.clone();
    fallback.socket_groups[0].gem_skills[2].stat_set_index = Some(999);
    assert_eq!(
        coverage_session(&fallback, &data).unsupported_modifier_texts(),
        baseline.unsupported_modifier_texts()
    );
    fallback.socket_groups[0].gem_skills.remove(1);
    assert_eq!(
        coverage_session(&fallback, &data).unsupported_modifier_texts(),
        baseline.unsupported_modifier_texts()
    );
}

#[test]
fn support_coverage_unknown_and_display_metadata_are_not_numerical_limits() {
    use pobr_core::rules::stat_map_engine::{MappedOutcome, map_stat};

    let data = load_build_data();
    let catalog = data.stat_map_catalog.as_deref().unwrap();
    let metadata = "support_hourglass_display_cooldown_time_ms";
    let payload = data.effect_stats("SupportHourglassPlayer", 1, 0, Some(1));
    let value = payload
        .base
        .iter()
        .find(|stat| stat.stat == metadata)
        .unwrap()
        .value;
    assert_eq!(value, 10000.0);
    assert_eq!(
        map_stat(
            catalog,
            "SupportHourglassPlayer",
            Some("1"),
            metadata,
            value
        ),
        MappedOutcome::Mapped(vec![])
    );
    let hourglass = coverage_session(
        &coverage_build("FireballPlayer", Some("SupportHourglassPlayer")),
        &data,
    );
    let bare = coverage_session(&coverage_build("FireballPlayer", None), &data);
    assert!(hourglass.output().total_hit_avg > bare.output().total_hit_avg);
    assert!(hourglass.unsupported_modifier_texts().is_empty());
    let unknown = "terrain_arrow_attachment_chance_reduction_+%";
    assert_eq!(
        map_stat(catalog, FORK, Some("1"), unknown, 100.0),
        MappedOutcome::Unknown
    );
    let fork = coverage_session(&coverage_build("FireballPlayer", Some(FORK)), &data);
    assert!(
        !fork
            .unsupported_modifier_texts()
            .iter()
            .any(|text| text.contains(unknown))
    );
    assert_fork_receipt(fork_receipt(&fork), 1, -30.0, "FireballPlayer");
}

#[test]
fn support_coverage_real_host_forms_have_distinct_effective_identity() {
    let data = load_build_data();
    let build = |set| {
        pobr_build::Build::new()
            .add_socket_group(
                pobr_build::SocketGroup::new()
                    .with_gem_skill_stat_set("FireballPlayer", 20, set)
                    .with_gem_skill(FORK, 1),
            )
            .with_main_socket_group(1)
    };
    let mut identities = std::collections::BTreeSet::new();
    for set in 1..=3 {
        assert_eq!(
            data.selected_set_key("FireballPlayer", Some(set)),
            Some(set.to_string())
        );
        let session = coverage_session(&build(set), &data);
        let receipt = fork_receipt(&session);
        assert_fork_receipt(receipt, 1, -30.0, "FireballPlayer");
        assert!(receipt.contains(&format!("host_set={set}:")));
        identities.insert(receipt.to_owned());
        let repeat = coverage_session(&build(set), &data);
        assert_eq!(repeat.output(), session.output());
        assert_eq!(
            repeat.unsupported_modifier_texts(),
            session.unsupported_modifier_texts()
        );
    }
    assert_eq!(
        identities.len(),
        3,
        "real projectile/explosion/firebolt host identities must not alias"
    );
    let primary = coverage_session(&build(1), &data);
    assert_eq!(
        coverage_session(&build(999), &data).unsupported_modifier_texts(),
        primary.unsupported_modifier_texts()
    );
    let mut with_disabled_prefix = build(1);
    with_disabled_prefix.socket_groups.insert(
        0,
        pobr_build::SocketGroup::new()
            .with_gem_skill("BallLightningPlayer", 20)
            .with_enabled(false),
    );
    with_disabled_prefix.main_socket_group = Some(2);
    assert_eq!(
        coverage_session(&with_disabled_prefix, &data).unsupported_modifier_texts(),
        primary.unsupported_modifier_texts()
    );
}

#[test]
fn support_coverage_effective_zero_conditional_number_is_not_a_limit() {
    use pobr_core::rules::stat_map_engine::{MappedOutcome, StatMapCatalog, map_stat};
    use std::sync::Arc;

    let mut data = load_build_data();
    let build = coverage_build("FireballPlayer", Some(FORK));
    let baseline = coverage_session(&build, &data);
    let mut mapping = coverage_mapping();
    // Nonzero raw payload, but the actual catalog merge makes the number zero.
    mapping
        .per_stat_set
        .get_mut(FORK)
        .unwrap()
        .get_mut("1")
        .unwrap()
        .get_mut(FORK_STAT)
        .unwrap()
        .mult = Some(0.0);
    data.stat_map_catalog = Some(Arc::new(StatMapCatalog::new(mapping)));
    assert!(matches!(
        map_stat(
            data.stat_map_catalog.as_deref().unwrap(),
            FORK,
            Some("1"),
            FORK_STAT,
            -30.0
        ),
        MappedOutcome::Unsupported(_)
    ));
    let zero = coverage_session(&build, &data);
    assert!(zero.unsupported_modifier_texts().is_empty());
    assert_eq!(zero.output(), baseline.output());
}

#[test]
fn support_coverage_routed_sibling_does_not_hide_a_skipped_conditional_number() {
    use pobr_core::rules::stat_map_engine::{
        MappedItem, MappedOutcome, StatMapCatalog, map_player_buff_stat,
    };
    use std::sync::Arc;

    let mut data = load_build_data();
    let build = coverage_build("FireballPlayer", Some(FORK));
    let baseline = coverage_session(&build, &data);
    let mut mapping = coverage_mapping();
    // Explicitly synthetic mixed entry: reuse Precision II's real routed buff
    // before Fork's real unmodeled conditional damage, in the same stat payload.
    let routed = mapping.per_stat_set["SupportPrecisionPlayerTwo"]["1"]
        ["support_precision_accuracy_rating_+%"].mods[0].clone();
    mapping
        .per_stat_set
        .get_mut(FORK)
        .unwrap()
        .get_mut("1")
        .unwrap()
        .get_mut(FORK_STAT)
        .unwrap()
        .mods
        .insert(0, routed);
    data.stat_map_catalog = Some(Arc::new(StatMapCatalog::new(mapping)));
    let MappedOutcome::Mapped(items) = map_player_buff_stat(
        data.stat_map_catalog.as_deref().unwrap(),
        FORK,
        Some("1"),
        FORK_STAT,
        -30.0,
    ) else {
        panic!("the routed sibling must really map through the player-buff channel");
    };
    assert!(items.iter().any(|item| matches!(item,
        MappedItem::Modifier(modifier) if modifier.name.to_string() == "Accuracy" && modifier.value.as_number() == Some(-30.0)
    )));
    let mixed = coverage_session(&build, &data);
    assert_fork_receipt(fork_receipt(&mixed), 1, -30.0, "FireballPlayer");
    assert!(
        mixed.attribute_total("Accuracy", 100.0) < baseline.attribute_total("Accuracy", 100.0),
        "the alternate-domain sibling is actually consumed without covering the skipped damage"
    );
    assert_eq!(mixed.output().dps, baseline.output().dps);
    assert_eq!(
        mixed.output().total_hit_avg,
        baseline.output().total_hit_avg
    );
}

#[test]
fn support_coverage_duplicate_active_ids_report_the_calculated_host_form() {
    use pobr_build::{Build, SocketGroup};

    let data = load_build_data();
    let mut group = SocketGroup::new()
        .with_gem_skill_stat_set("FireballPlayer", 20, 1)
        .with_gem_skill_stat_set("FireballPlayer", 20, 2)
        .with_gem_skill(FORK, 1);
    for selected in [1, 2] {
        group.main_active_skill = Some(selected);
        let build = Build::new()
            .add_socket_group(group.clone())
            .with_main_socket_group(1);
        let session = coverage_session(&build, &data);
        assert!(fork_receipt(&session).contains(&format!("host_set={selected}:")));
    }
}
