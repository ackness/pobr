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
//! Uses real ingested data from `data/<version>/` (granted_effects.json's
//! require/exclude token stream is parsed from the GrantedEffects .dat type
//! columns).

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};
use pobr_core::calc::MinimalInput;
use pobr_data::monster::EnemyTier;
use pobr_gamedata::repo_data_root;

use crate::test_data as load_build_data;

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

/// Pinned PoB2 Controlled Destruction supplies 25% more hit damage and
/// global_cannot_crit. The downside belongs only to its supported group.
#[test]
fn controlled_destruction_disables_critical_hits_only_in_its_group() {
    use pobr_build::{Build, CharacterIdentity, SocketGroup};
    let data = load_build_data();
    let plain = SocketGroup::new().with_gem_skill("FrostBombPlayer", 19);
    let supported = plain
        .clone()
        .with_gem_skill("SupportControlledDestructionPlayer", 1);
    let build = Build::new()
        .with_character(CharacterIdentity {
            level: 98,
            class_name: "Monk".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(supported)
        .add_socket_group(plain);
    let bare = calculate_with_data(
        &build.clone().with_main_socket_group(2),
        &data,
        &panel_opts(),
    )
    .unwrap();
    let blocked =
        calculate_with_data(&build.with_main_socket_group(1), &data, &panel_opts()).unwrap();
    assert!(bare.crit_chance > 0.0);
    assert_eq!(blocked.crit_chance, 0.0);
    assert_eq!(blocked.crit_multiplier, 0.0);
    let non_crit_hit: f64 = bare.damage_components.iter().map(|c| c.avg()).sum();
    assert!(
        (blocked.total_hit_avg - non_crit_hit * 1.25).abs() < 1e-8,
        "retain the damage benefit without multiplying by critical damage"
    );
}

#[test]
fn elemental_focus_disables_elemental_ailments_only_in_its_group() {
    use pobr_build::{Build, SocketGroup};

    let data = load_build_data();
    let plain = SocketGroup::new().with_gem_skill("FireballPlayer", 20);
    let build = Build::new()
        .add_socket_group(
            plain
                .clone()
                .with_gem_skill("SupportElementalFocusPlayer", 1),
        )
        .add_socket_group(plain);
    for effective in [false, true] {
        let mut opts = panel_opts();
        opts.mode_effective = effective;
        opts.extra_modifier_texts = vec![
            "Adds 100000 to 100000 Cold Damage to Spells".into(),
            "Adds 100000 to 100000 Lightning Damage to Spells".into(),
        ];
        let bare =
            calculate_with_data(&build.clone().with_main_socket_group(2), &data, &opts).unwrap();
        let blocked =
            calculate_with_data(&build.clone().with_main_socket_group(1), &data, &opts).unwrap();
        for (name, normal, prohibited) in [
            ("Ignite", bare.ignite_dps, blocked.ignite_dps),
            ("Chill", bare.chill_effect, blocked.chill_effect),
            ("Shock", bare.shock_effect, blocked.shock_effect),
            (
                "Freeze",
                bare.freeze_buildup_pct,
                blocked.freeze_buildup_pct,
            ),
            (
                "Electrocute",
                bare.electrocute_buildup_pct,
                blocked.electrocute_buildup_pct,
            ),
        ] {
            assert!(normal > 0.0, "fixture must inflict {name}");
            assert_eq!(prohibited, 0.0, "{name}");
        }
        assert_eq!(blocked.ignite_stacked_dps, 0.0);
        assert_eq!(blocked.ignite_active_stacks, 0.0);
        assert!((blocked.total_hit_avg - bare.total_hit_avg * 1.25).abs() < 1e-6);
        let mut without_support = build.clone().with_main_socket_group(2);
        without_support.socket_groups[0].gem_skills.remove(1);
        assert_eq!(
            calculate_with_data(&without_support, &data, &opts).unwrap(),
            bare,
            "the support must not change another group's output",
        );
    }
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

// Distinct canonical mechanics: maximum-pool loss versus Life sacrifice.
const UNMODELED_RESOURCE_EFFECTS: [(&str, &str, f64, &str); 2] = [
    (
        "SupportAtzirisImpatiencePlayer",
        "lose_%_of_life_es_and_mana_when_skill_is_used",
        25.0,
        "FireballPlayer",
    ),
    (
        "SupportExtractionPlayer",
        "sacrifice_%_life_on_skill_use",
        10.0,
        "ArmourBreakerPlayer",
    ),
];

#[test]
fn support_coverage_resource_effects_are_attributed_without_inventing_costs() {
    for (support, stat, value, _) in UNMODELED_RESOURCE_EFFECTS {
        let mut data = load_build_data().clone();
        let build = coverage_build("FrostBombPlayer", Some(support));
        let supported = coverage_session(&build, &data);
        let receipts: Vec<_> = supported
            .unsupported_modifier_texts()
            .iter()
            .filter(|text| text.contains(stat))
            .collect();
        assert_eq!(receipts.len(), 1, "{support}");
        for token in [
            format!("Support:{support}:"),
            format!("stat={stat}:"),
            "set=1:".into(),
            format!("value={value}:"),
            "host=FrostBombPlayer:".into(),
        ] {
            assert!(receipts[0].contains(&token));
        }
        let bare = coverage_session(&coverage_build("FrostBombPlayer", None), &data);
        assert!(bare.unsupported_modifier_texts().is_empty());
        assert!(supported.output().dps > bare.output().dps);
        assert_eq!(supported.output().mana_cost, bare.output().mana_cost);
        assert_eq!(supported.output().life_cost, bare.output().life_cost);

        // Zero or invalid downside values must not invent a numeric limit.
        for downside in [0.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            data.skill_stat_sets.get_mut(support).unwrap().sets[0]
                .constant_stats
                .iter_mut()
                .find(|entry| entry.stat == stat)
                .unwrap()
                .value = downside;
            let ignored = coverage_session(&build, &data);
            assert!(ignored.unsupported_modifier_texts().is_empty());
            assert_eq!(ignored.output(), supported.output());
        }
    }
}

#[test]
fn support_coverage_resource_receipts_stop_when_the_stat_is_recognized() {
    use pobr_core::rules::stat_map_engine::{MappedOutcome, StatMapCatalog, map_stat};
    use std::sync::Arc;

    for (support, stat, value, _) in UNMODELED_RESOURCE_EFFECTS {
        let mut data = load_build_data().clone();
        let build = coverage_build("FrostBombPlayer", Some(support));
        // Reuse real empty, numerical and separately-consumed LIST mappings.
        // A later recognized mapping must not retain an Unknown-only receipt.
        for (source, source_stat) in [
            (
                "SupportHourglassPlayer",
                "support_hourglass_display_cooldown_time_ms",
            ),
            (
                "SupportExtractionPlayer",
                "non_skill_base_all_damage_%_to_gain_as_chaos",
            ),
            (
                "SupportFireMasteryPlayer",
                "supported_fire_skill_gem_level_+",
            ),
        ] {
            let mut mapping = coverage_mapping();
            let entry = mapping
                .per_stat_set
                .get(source)
                .and_then(|sets| sets.get("1"))
                .and_then(|stats| stats.get(source_stat))
                .or_else(|| mapping.global.get(source_stat))
                .unwrap()
                .clone();
            mapping
                .per_stat_set
                .entry(support.into())
                .or_default()
                .entry("1".into())
                .or_default()
                .insert(stat.into(), entry);
            data.stat_map_catalog = Some(Arc::new(StatMapCatalog::new(mapping)));
            assert!(!matches!(
                map_stat(
                    data.stat_map_catalog.as_deref().unwrap(),
                    support,
                    Some("1"),
                    stat,
                    value
                ),
                MappedOutcome::Unknown
            ));
            let recognized = coverage_session(&build, &data);
            assert!(
                recognized.unsupported_modifier_texts().is_empty(),
                "{support}/{source}"
            );
        }
    }
}

#[test]
fn support_coverage_resource_effects_do_not_leak_from_other_or_incompatible_groups() {
    for (support, _, _, incompatible_host) in UNMODELED_RESOURCE_EFFECTS {
        let data = load_build_data();
        let build = coverage_build("FrostBombPlayer", Some(support))
            .add_socket_group(pobr_build::SocketGroup::new().with_gem_skill("FireballPlayer", 20))
            .with_main_socket_group(2);
        let other = coverage_session(&build, &data);
        let mut without_other_support = build.clone();
        without_other_support.socket_groups[0].gem_skills.remove(1);
        let other_bare = coverage_session(&without_other_support, &data);
        let incompatible =
            coverage_session(&coverage_build(incompatible_host, Some(support)), &data);
        let bare = coverage_session(&coverage_build(incompatible_host, None), &data);
        assert!(other.unsupported_modifier_texts().is_empty());
        assert!(incompatible.unsupported_modifier_texts().is_empty());
        assert_same_skill_outputs(other.output(), other_bare.output());
        assert_same_skill_outputs(incompatible.output(), bare.output());
    }
}

#[test]
fn support_coverage_resource_effects_follow_the_stat_not_the_gem_name() {
    for (source, stat, _, _) in UNMODELED_RESOURCE_EFFECTS {
        let mut data = load_build_data().clone();
        let effect = data.skill_stat_sets[source].sets[0]
            .constant_stats
            .iter()
            .find(|entry| entry.stat == stat)
            .unwrap()
            .clone();
        let support = "SupportEfficiencyPlayerTwo";
        data.skill_stat_sets.get_mut(support).unwrap().sets[0]
            .constant_stats
            .push(effect);
        let session = coverage_session(&coverage_build("FrostBombPlayer", Some(support)), &data);
        assert!(
            session.unsupported_modifier_texts().iter().any(|text| {
                text.contains(stat) && text.contains(&format!("Support:{support}:"))
            })
        );
    }
}

/// A real support whose stat stays a skipped conditional number: Swift Affliction III adds 50% damage over time
/// only if the skill's duration is below one second, and the stat map cannot express an upper threshold on a duration.
/// (Fork was the exemplar until its damage change was modelled: it now follows the configured fork count.)
const SWIFT: &str = "SupportSwiftAfflictionPlayerThree";
const SWIFT_STAT: &str =
    "support_swift_affliction_non_ailment_damage_over_time_+%_final_if_duration_below_1_second";

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
) -> pobr_core::calc::CompletedCalculationSession {
    pobr_build::calculate_with_data_session(build, data, &panel_opts()).unwrap()
}

// Socketed gems contribute to build-wide requirements even when inspecting
// another group. Payload isolation compares every other output field.
fn assert_same_skill_outputs(
    left: &pobr_core::calc::OutputTable,
    right: &pobr_core::calc::OutputTable,
) {
    let without_requirements = |output: &pobr_core::calc::OutputTable| {
        let mut output = output.clone();
        output.req_str = 0.0;
        output.req_dex = 0.0;
        output.req_int = 0.0;
        output
    };
    assert_eq!(without_requirements(left), without_requirements(right));
}

/// The support's own cost multiplier (when it has one) is modelled apart from the skipped number: the supported cost
/// is the bare cost times that multiplier.
fn assert_cost_multiplier_preserved(
    supported: &pobr_core::calc::OutputTable,
    bare: &pobr_core::calc::OutputTable,
    data: &BuildData,
) {
    use pobr_core::skill_env::StatSetLookup;
    let multiplier = data
        .effect_level_row(SWIFT, 1)
        .and_then(|row| row.mana_multiplier)
        // Swift Affliction III gives no cost multiplier: the cost is the bare cost.
        .unwrap_or(100.0);
    assert!(
        (supported.mana_cost - bare.mana_cost * multiplier / 100.0).abs() <= 1.0,
        "{} vs {} x {multiplier}%",
        supported.mana_cost,
        bare.mana_cost
    );
}

fn swift_receipt(session: &pobr_core::calc::CalculationSession) -> &str {
    let receipts: Vec<_> = session
        .unsupported_modifier_texts()
        .iter()
        .filter(|text| text.contains(SWIFT_STAT))
        .collect();
    assert_eq!(
        receipts.len(),
        1,
        "one limit for the calculated Swift Affliction payload"
    );
    receipts[0]
}

fn assert_swift_receipt(text: &str, set: u32, value: f64, host: &str) {
    // Stable source identity, not explanatory prose or Debug formatting.
    for token in [
        format!("Support:{SWIFT}:"),
        format!("set={set}:"),
        format!("stat={SWIFT_STAT}:"),
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

/// Every catalog stat carrying a crit or ailment prohibition must map to the
/// consumed flags, not only the supports exercised above.
#[test]
fn prohibition_stats_map_to_consumed_flags() {
    use pobr_core::rules::stat_map_engine::{MappedItem, MappedOutcome, StatMapCatalog, map_stat};

    let catalog = StatMapCatalog::new(coverage_mapping());
    let cases: [(&str, &[&str]); 9] = [
        ("global_cannot_crit", &["NeverCrit"]),
        ("never_ignite", &["CannotIgnite"]),
        ("never_shock", &["CannotShock"]),
        ("never_chill", &["CannotChill"]),
        ("never_freeze", &["CannotFreeze"]),
        ("never_freeze_or_chill", &["CannotFreeze", "CannotChill"]),
        (
            "active_skill_never_freeze_shock_ignite",
            &["CannotFreeze", "CannotShock", "CannotIgnite"],
        ),
        (
            "cannot_inflict_status_ailments",
            &["CannotShock", "CannotChill", "CannotFreeze", "CannotIgnite"],
        ),
        (
            "cannot_inflict_elemental_ailments",
            &[
                "CannotShock",
                "CannotChill",
                "CannotFreeze",
                "CannotIgnite",
                "CannotElectrocute",
            ],
        ),
    ];
    for (stat, flags) in cases {
        let MappedOutcome::Mapped(items) = map_stat(&catalog, "AnySkill", None, stat, 1.0) else {
            panic!("{stat} must be consumed");
        };
        let names: Vec<_> = items
            .iter()
            .filter_map(|item| match item {
                MappedItem::Modifier(modifier) => Some(modifier.name.as_str()),
                MappedItem::SkillData { .. } => None,
            })
            .collect();
        assert_eq!(names, flags, "{stat}");
    }
}

/// A stat the engine models is not reported as unmodelled. Fork's "Projectiles that have Forked deal 30% less
/// Damage" is applied from the configured fork count (`skill_stats::player_buff_modifiers`), so the support coverage
/// has no skipped conditional number to disclose for it, whatever the stat map could not express.
#[test]
fn support_coverage_does_not_report_a_stat_the_engine_models() {
    let data = load_build_data();
    let session = coverage_session(
        &coverage_build("FireballPlayer", Some("SupportForkPlayer")),
        &data,
    );
    assert!(
        session.unsupported_modifier_texts().is_empty(),
        "{:?}",
        session.unsupported_modifier_texts()
    );
}

#[test]
fn support_coverage_normal_session_discloses_actual_swift_affliction_without_changing_numbers() {
    use pobr_core::rules::stat_map_engine::{MappedOutcome, UnsupportedReason, map_stat};

    let data = load_build_data();
    let payload = data.effect_stats(SWIFT, 1, 0, Some(1));
    let stat = payload
        .base
        .iter()
        .find(|stat| stat.stat == SWIFT_STAT)
        .unwrap();
    assert_eq!(stat.value, 50.0);
    assert_eq!(
        map_stat(
            data.stat_map_catalog.as_deref().unwrap(),
            SWIFT,
            Some("1"),
            SWIFT_STAT,
            stat.value
        ),
        MappedOutcome::Unsupported(UnsupportedReason::UnsupportedTag("StatThreshold".into()))
    );
    let build = coverage_build("FireballPlayer", Some(SWIFT));
    let normal = coverage_session(&build, &data);
    assert_swift_receipt(swift_receipt(&normal), 1, 50.0, "FireballPlayer");
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

    // The skipped bonus belongs to damage over time under one second, not to the primary hit.
    let bare = coverage_session(&coverage_build("FireballPlayer", None), &data);
    assert!(bare.output().dps > 0.0);
    assert_eq!(normal.output().total_hit_avg, bare.output().total_hit_avg);
    assert_eq!(normal.output().dps, bare.output().dps);
    assert_eq!(
        normal.output().damage_components,
        bare.output().damage_components
    );
    assert_cost_multiplier_preserved(normal.output(), bare.output(), &data);
}

#[test]
fn support_coverage_resource_loss_is_disclosed_without_becoming_a_cost() {
    let data = load_build_data();
    let support = "SupportAtzirisImpatiencePlayer";
    let loss_stat = "lose_%_of_life_es_and_mana_when_skill_is_used";
    let build = coverage_build("FrostBombPlayer", Some(support));
    let session = coverage_session(&build, &data);
    let bare = coverage_session(&coverage_build("FrostBombPlayer", None), &data);
    let limits: Vec<_> = session
        .unsupported_modifier_texts()
        .iter()
        .filter(|text| text.contains(loss_stat))
        .collect();
    assert_eq!(
        limits.len(),
        1,
        "Atziri's Impatience loses 25% of maximum Life/Mana/ES per use; \
         an unmodeled downside must not silently disappear"
    );
    for token in [
        format!("Support:{support}:"),
        "set=1:".into(),
        format!("stat={loss_stat}:"),
        "value=25:".into(),
        "host=FrostBombPlayer:".into(),
        "host_set=1:".into(),
    ] {
        assert!(limits[0].contains(&token), "{token:?}: {:?}", limits[0]);
    }
    assert!(bare.output().cooldown > 0.0);
    assert_eq!(session.output().cooldown, bare.output().cooldown / 2.0);
    assert_eq!(session.output().mana_cost, bare.output().mana_cost);
    assert_eq!(session.output().life_cost, bare.output().life_cost);
    assert_eq!(session.output().life_regen, bare.output().life_regen);
    assert_eq!(session.output().mana_regen, bare.output().mana_regen);
    assert_eq!(
        session.output().energy_shield_regen,
        bare.output().energy_shield_regen
    );
    let compare = pobr_build::calculate_with_data_report(
        &build,
        &data,
        &DataOrchestratorOptions {
            stat_map_mode: pobr_build::StatMapMode::Compare,
            ..panel_opts()
        },
    )
    .unwrap();
    assert_eq!(session.output(), compare.session.output());
    assert_eq!(
        session.unsupported_modifier_texts(),
        compare.session.unsupported_modifier_texts()
    );
}

#[test]
fn support_coverage_resource_loss_requires_a_nonzero_calculated_compatible_payload() {
    let mut data = load_build_data().clone();
    let support = "SupportAtzirisImpatiencePlayer";
    let loss_stat = "lose_%_of_life_es_and_mana_when_skill_is_used";
    let assert_no_loss = |build: &pobr_build::Build, data: &BuildData| {
        assert!(
            !coverage_session(build, data)
                .unsupported_modifier_texts()
                .iter()
                .any(|text| text.contains(loss_stat))
        );
    };
    assert_no_loss(&coverage_build("FrostBombPlayer", None), &data);
    assert_no_loss(&coverage_build("FireballPlayer", Some(support)), &data);
    let mut other_group = coverage_build("FireballPlayer", None);
    other_group.socket_groups.push(
        pobr_build::SocketGroup::new()
            .with_gem_skill("FrostBombPlayer", 20)
            .with_gem_skill(support, 1),
    );
    assert_no_loss(&other_group, &data);
    other_group.socket_groups[1].enabled = false;
    assert_no_loss(&other_group, &data);

    // Synthetic zero on the real stat: no loss to disclose, but the independently
    // modeled cooldown recovery must still apply.
    data.skill_stat_sets.get_mut(support).unwrap().sets[0]
        .constant_stats
        .iter_mut()
        .find(|stat| stat.stat == loss_stat)
        .unwrap()
        .value = 0.0;
    let build = coverage_build("FrostBombPlayer", Some(support));
    assert_no_loss(&build, &data);
    assert_eq!(
        coverage_session(&build, &data).output().cooldown,
        coverage_session(&coverage_build("FrostBombPlayer", None), &data)
            .output()
            .cooldown
            / 2.0
    );
}

#[test]
fn support_coverage_removed_disabled_and_incompatible_payloads_do_not_disclose_swift_affliction() {
    let data = load_build_data();
    let enabled = fireball_xml(Some((
        "Metadata/Items/Gem/SupportGemSwiftAfflictionThree",
        SWIFT,
    )));
    let disabled = enabled.replace(
        r#"<Gem enabled="true" gemId="Metadata/Items/Gem/SupportGemSwiftAfflictionThree""#,
        r#"<Gem enabled="false" gemId="Metadata/Items/Gem/SupportGemSwiftAfflictionThree""#,
    );
    let equipped = coverage_session(&parse_build(&enabled).unwrap(), &data);
    assert_swift_receipt(swift_receipt(&equipped), 1, 50.0, "FireballPlayer");
    let bare_build = parse_build(&fireball_xml(None)).unwrap();
    let bare = coverage_session(&bare_build, &data);
    let disabled = coverage_session(&parse_build(&disabled).unwrap(), &data);
    assert_eq!(disabled.output(), bare.output());
    assert!(disabled.unsupported_modifier_texts().is_empty());
    assert!(bare.unsupported_modifier_texts().is_empty());
    let incompatible = coverage_session(&coverage_build("ConductivityPlayer", Some(SWIFT)), &data);
    let melee_bare = coverage_session(&coverage_build("ConductivityPlayer", None), &data);
    assert_same_skill_outputs(incompatible.output(), melee_bare.output());
    assert_eq!(incompatible.output().req_dex, 5.0);
    assert!(incompatible.unsupported_modifier_texts().is_empty());
    let reequipped = coverage_session(&parse_build(&enabled).unwrap(), &data);
    assert_eq!(
        reequipped.unsupported_modifier_texts(),
        equipped.unsupported_modifier_texts()
    );
}

#[test]
fn support_coverage_zero_payload_is_not_a_limit() {
    let mut data = load_build_data().clone();
    // Synthetic zero-valued input to the real mapping/ingestion boundary.
    let stat = data.skill_stat_sets.get_mut(SWIFT).unwrap().sets[0]
        .constant_stats
        .iter_mut()
        .find(|stat| stat.stat == SWIFT_STAT)
        .unwrap();
    stat.value = 0.0;
    let session = coverage_session(&coverage_build("FireballPlayer", Some(SWIFT)), &data);
    assert!(session.unsupported_modifier_texts().is_empty());
    let bare = coverage_session(&coverage_build("FireballPlayer", None), &data);
    assert_eq!(session.output().total_hit_avg, bare.output().total_hit_avg);
    assert_eq!(session.output().dps, bare.output().dps);
    assert_cost_multiplier_preserved(session.output(), bare.output(), &data);
}

#[test]
fn support_coverage_selected_set_owns_payload_value_and_identity() {
    use pobr_core::rules::stat_map_engine::StatMapCatalog;
    use pobr_data::catalog::stat_map::SkillStatMapDef;
    use std::sync::Arc;

    let mut data = load_build_data().clone();
    // Swift Affliction III has one real set. A synthetic second form reuses its actual mapping;
    // only input set/value vary, never the engine outcome or diagnostic.
    let sets = &mut data.skill_stat_sets.get_mut(SWIFT).unwrap().sets;
    sets[0].vendor_set_index = Some(1);
    let mut second = sets[0].clone();
    second.vendor_set_index = Some(2);
    second.set_id = "SyntheticSwiftSecondForm".into();
    second
        .constant_stats
        .iter_mut()
        .find(|stat| stat.stat == SWIFT_STAT)
        .unwrap()
        .value = 25.0;
    sets.push(second);
    let mut mapping: SkillStatMapDef = coverage_mapping();
    let swift_sets = mapping.per_stat_set.get_mut(SWIFT).unwrap();
    swift_sets.insert("2".into(), swift_sets["1"].clone());
    data.stat_map_catalog = Some(Arc::new(StatMapCatalog::new(mapping)));
    let build = |set| {
        pobr_build::Build::new()
            .add_socket_group(
                pobr_build::SocketGroup::new()
                    .with_gem_skill("FireballPlayer", 20)
                    .with_gem_skill_stat_set(SWIFT, 1, set),
            )
            .with_main_socket_group(1)
    };
    let first = coverage_session(&build(1), &data);
    let second = coverage_session(&build(2), &data);
    assert_swift_receipt(swift_receipt(&first), 1, 50.0, "FireballPlayer");
    assert_swift_receipt(swift_receipt(&second), 2, 25.0, "FireballPlayer");
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
    data.skill_stat_sets.get_mut(SWIFT).unwrap().sets[1]
        .constant_stats
        .iter_mut()
        .find(|stat| stat.stat == SWIFT_STAT)
        .unwrap()
        .value = 0.0;
    assert!(
        coverage_session(&build(2), &data)
            .unsupported_modifier_texts()
            .is_empty()
    );
    assert_swift_receipt(
        swift_receipt(&coverage_session(&build(1), &data)),
        1,
        50.0,
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
                .with_gem_skill(SWIFT, 1),
        )
        .add_socket_group(
            SocketGroup::new()
                .with_gem_skill("ContagionPlayer", 20)
                .with_gem_skill(SWIFT, 1),
        );
    let bare = coverage_session(&build.clone().with_main_socket_group(1), &data);
    assert!(bare.unsupported_modifier_texts().is_empty());
    let fire = coverage_session(&build.clone().with_main_socket_group(2), &data);
    let ice = coverage_session(&build.clone().with_main_socket_group(3), &data);
    assert_swift_receipt(swift_receipt(&fire), 1, 50.0, "FireballPlayer");
    assert_swift_receipt(swift_receipt(&ice), 1, 50.0, "ContagionPlayer");
    assert_ne!(
        fire.unsupported_modifier_texts(),
        ice.unsupported_modifier_texts()
    );
    for session in [&bare, &fire, &ice] {
        assert_eq!(
            session.output().req_dex,
            10.0,
            "two socketed Swift Affliction supports"
        );
    }
    assert_same_skill_outputs(
        fire.output(),
        coverage_session(&coverage_build("FireballPlayer", Some(SWIFT)), &data).output(),
    );
    assert_same_skill_outputs(
        ice.output(),
        coverage_session(&coverage_build("ContagionPlayer", Some(SWIFT)), &data).output(),
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
    let baseline = coverage_session(&coverage_build("FireballPlayer", Some(SWIFT)), &data);
    let group = pobr_build::SocketGroup::new()
        .with_gem_skill("FireballPlayer", 20)
        .with_gem_skill("SupportFireMasteryPlayer", 1)
        .with_gem_skill_stat_set(SWIFT, 1, 1);
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
        map_stat(catalog, SWIFT, Some("1"), unknown, 100.0),
        MappedOutcome::Unknown
    );
    let fork = coverage_session(&coverage_build("FireballPlayer", Some(SWIFT)), &data);
    assert!(
        !fork
            .unsupported_modifier_texts()
            .iter()
            .any(|text| text.contains(unknown))
    );
    assert_swift_receipt(swift_receipt(&fork), 1, 50.0, "FireballPlayer");
}

#[test]
fn support_coverage_real_host_forms_have_distinct_effective_identity() {
    let data = load_build_data();
    let build = |set| {
        pobr_build::Build::new()
            .add_socket_group(
                pobr_build::SocketGroup::new()
                    .with_gem_skill_stat_set("FireballPlayer", 20, set)
                    .with_gem_skill(SWIFT, 1),
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
        let receipt = swift_receipt(&session);
        assert_swift_receipt(receipt, 1, 50.0, "FireballPlayer");
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
            .with_gem_skill("ContagionPlayer", 20)
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

    let mut data = load_build_data().clone();
    let build = coverage_build("FireballPlayer", Some(SWIFT));
    let baseline = coverage_session(&build, &data);
    let mut mapping = coverage_mapping();
    // Nonzero raw payload, but the actual catalog merge makes the number zero.
    mapping
        .per_stat_set
        .get_mut(SWIFT)
        .unwrap()
        .get_mut("1")
        .unwrap()
        .get_mut(SWIFT_STAT)
        .unwrap()
        .mult = Some(0.0);
    data.stat_map_catalog = Some(Arc::new(StatMapCatalog::new(mapping)));
    assert!(matches!(
        map_stat(
            data.stat_map_catalog.as_deref().unwrap(),
            SWIFT,
            Some("1"),
            SWIFT_STAT,
            50.0
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

    let mut data = load_build_data().clone();
    let build = coverage_build("FireballPlayer", Some(SWIFT));
    let baseline = coverage_session(&build, &data);
    let mut mapping = coverage_mapping();
    // Explicitly synthetic mixed entry: reuse Precision II's real routed buff
    // before Swift Affliction's real unmodeled conditional damage, in the same stat payload.
    let routed = mapping.per_stat_set["SupportPrecisionPlayerTwo"]["1"]
        ["support_precision_accuracy_rating_+%"].mods[0].clone();
    mapping
        .per_stat_set
        .get_mut(SWIFT)
        .unwrap()
        .get_mut("1")
        .unwrap()
        .get_mut(SWIFT_STAT)
        .unwrap()
        .mods
        .insert(0, routed);
    data.stat_map_catalog = Some(Arc::new(StatMapCatalog::new(mapping)));
    let MappedOutcome::Mapped(items) = map_player_buff_stat(
        data.stat_map_catalog.as_deref().unwrap(),
        SWIFT,
        Some("1"),
        SWIFT_STAT,
        50.0,
    ) else {
        panic!("the routed sibling must really map through the player-buff channel");
    };
    assert!(items.iter().any(|item| matches!(item,
        MappedItem::Modifier(modifier) if modifier.name.to_string() == "Accuracy" && modifier.value.as_number() == Some(50.0)
    )));
    let mixed = coverage_session(&build, &data);
    assert_swift_receipt(swift_receipt(&mixed), 1, 50.0, "FireballPlayer");
    assert!(
        mixed.attribute_total("Accuracy", 100.0) > baseline.attribute_total("Accuracy", 100.0),
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
        .with_gem_skill(SWIFT, 1);
    for selected in [1, 2] {
        group.main_active_skill = Some(selected);
        let build = Build::new()
            .add_socket_group(group.clone())
            .with_main_socket_group(1);
        let session = coverage_session(&build, &data);
        assert!(swift_receipt(&session).contains(&format!("host_set={selected}:")));
    }
}

mod elemental_armament {
    use super::*;
    use pobr_build::{Build, SocketGroup, calculate_with_data_session};
    use pobr_core::calc::CalculationSession;
    use pobr_core::rules::config_interpreter::ConfigInputValue;
    use pobr_data::prelude::{DamageType, ModType, SourceKind};

    const SUPPORT: &str = "SupportElementalArmamentPlayerThree";
    const STAT: &str =
        "support_elemental_armament_attack_damage_+%_final_per_elemental_ailment_on_target";
    const AILMENTS: [&str; 5] = [
        "EnemyIgnited",
        "EnemyChilled",
        "EnemyFrozen",
        "EnemyShocked",
        "EnemyElectrocuted",
    ];

    use crate::test_data as data;

    fn options(effective: bool) -> DataOrchestratorOptions {
        DataOrchestratorOptions {
            mode_effective: effective,
            extra_modifier_texts: [
                "Adds 100000 to 100000 Physical Damage to Attacks",
                "Adds 100000 to 100000 Fire Damage to Attacks",
                "Adds 100000 to 100000 Cold Damage to Attacks",
                "Adds 100000 to 100000 Lightning Damage to Attacks",
                "Adds 100000 to 100000 Chaos Damage to Attacks",
                "100% chance to Poison on Hit",
                "100% chance to inflict Bleeding on Hit",
            ]
            .map(str::to_owned)
            .into(),
            ..panel_opts()
        }
    }

    fn build(conditions: &[&str]) -> Build {
        let mut build = coverage_build("ArmourBreakerPlayer", Some(SUPPORT));
        for condition in conditions {
            build.config.raw_inputs.values.insert(
                format!("condition{condition}"),
                ConfigInputValue::Bool(true),
            );
        }
        build
    }

    fn close(actual: f64, expected: f64, label: &str) {
        assert!(
            (actual - expected).abs() <= expected.abs().max(1.0) * 1e-10,
            "{label}: expected {expected}, got {actual}"
        );
    }

    fn assert_supported_pair(build: &Build, opts: &DataOrchestratorOptions, factor: f64) {
        let mut bare_build = build.clone();
        bare_build.socket_groups[0].gem_skills.remove(1);
        let bare = calculate_with_data_session(&bare_build, data(), opts).unwrap();
        let supported = calculate_with_data_session(build, data(), opts).unwrap();
        assert!(bare.output().total_hit_avg > 0.0);
        close(
            supported.output().total_hit_avg,
            bare.output().total_hit_avg * factor,
            "hit: identical target inputs with and without Armament",
        );
        if opts.mode_effective && factor > 1.0 {
            // EnemyTier::None still supplies positive level-scaled monster armour.
            // Larger physical hits lose a smaller fraction to the same positive
            // enemy armour. The exact raw-hit factor above is a lower bound on
            // effective DPS, not an equality after nonlinear armour mitigation.
            assert!(supported.output().dps >= bare.output().dps * factor);
        } else {
            close(
                supported.output().dps,
                bare.output().dps * factor,
                "neutral or panel hit DPS",
            );
        }
        // Bleeding and poison inherit the increased source hit exactly once;
        // Attack Damage must not also multiply their ailment-magnitude stage.
        for (name, normal, boosted) in [
            (
                "Bleed",
                bare.output().bleed_dps,
                supported.output().bleed_dps,
            ),
            (
                "Poison",
                bare.output().poison_dps,
                supported.output().poison_dps,
            ),
        ] {
            assert!(normal > 0.0, "fixture must inflict {name}");
            close(boosted, normal * factor, name);
        }
        // This is Attack Damage, not Elemental Damage. Both non-elemental
        // components must be nonzero and receive the same MORE term.
        for kind in [DamageType::Physical, DamageType::Chaos] {
            let component = |session: &CalculationSession| -> f64 {
                session
                    .output()
                    .damage_components
                    .iter()
                    .filter(|component| component.damage_type == kind)
                    .map(|component| component.avg())
                    .sum()
            };
            assert!(component(&bare) > 0.0, "{kind:?} fixture");
            close(
                component(&supported),
                component(&bare) * factor,
                "component",
            );
        }
        for (name, normal, prohibited) in [
            (
                "Ignite",
                bare.output().ignite_dps,
                supported.output().ignite_dps,
            ),
            (
                "Chill",
                bare.output().chill_effect,
                supported.output().chill_effect,
            ),
            (
                "Shock",
                bare.output().shock_effect,
                supported.output().shock_effect,
            ),
            (
                "Freeze",
                bare.output().freeze_buildup_pct,
                supported.output().freeze_buildup_pct,
            ),
            (
                "Electrocute",
                bare.output().electrocute_buildup_pct,
                supported.output().electrocute_buildup_pct,
            ),
        ] {
            assert!(normal > 0.0, "bare fixture must inflict {name}");
            assert_eq!(prohibited, 0.0, "Armament still prohibits {name}");
        }
        assert_eq!(supported.output().ignite_stacked_dps, 0.0);
        assert_eq!(supported.output().ignite_active_stacks, 0.0);
        assert!(
            supported
                .unsupported_modifier_texts()
                .iter()
                .all(|text| !text.contains(STAT)),
            "a modeled numerical benefit must not retain its old limit receipt"
        );
        let damage_mods = supported.mods_named("Damage");
        let source_id = format!("{SUPPORT}.{STAT}");
        let attributed: Vec<_> = damage_mods
            .iter()
            .filter(|modifier| {
                modifier
                    .origin
                    .as_ref()
                    .is_some_and(|origin| origin.source_id.id == source_id)
            })
            .collect();
        assert_eq!(attributed.len(), 1, "one source-attributed MORE term");
        assert_eq!(attributed[0].mod_type, ModType::More);
        assert_eq!(attributed[0].value.as_number(), Some(12.0));
        assert_eq!(
            attributed[0].origin.as_ref().unwrap().source_id.kind,
            SourceKind::SupportGem
        );
    }

    // Source-grounded model, NOT an Armament Lua-oracle golden: pinned
    // sup_str.lua:2891-2920 and gem_stat_descriptions.lua entry 954 give 12%
    // MORE Attack Damage; the five PoE2 elemental ailment types are counted.
    // SkillStatMap.lua:922-924 / ModStore.lua:550-571 establish a single
    // final-per-count term. This support itself has no pinned Lua consumer.
    #[test]
    fn conditional_attack_damage_counts_five_independent_ailment_types() {
        let payload = data().effect_stats(SUPPORT, 1, 0, None);
        assert_eq!(
            payload
                .base
                .iter()
                .find(|entry| entry.stat == STAT)
                .unwrap()
                .value,
            12.0
        );
        let effective = options(true);
        assert_supported_pair(&build(&[]), &effective, 1.00);
        for ailment in AILMENTS {
            // Frozen alone is one; Ignited's implied Burning and
            // Electrocuted's implied Immobilised do not add another type.
            assert_supported_pair(&build(&[ailment]), &effective, 1.12);
        }
        for (conditions, factor) in [
            (&AILMENTS[1..3], 1.24), // explicit Frozen + Chilled
            (&AILMENTS[..3], 1.36),
            (&AILMENTS[..4], 1.48),
            (&AILMENTS[..], 1.60), // NOT 1.12^5
        ] {
            assert_supported_pair(&build(conditions), &effective, factor);
        }
        for condition in [
            "EnemyBurning",
            "EnemyImmobilised",
            "EnemyPoisoned",
            "EnemyBleeding",
            "EnemyScorched",
            "EnemyBrittle",
            "EnemySapped",
        ] {
            let mut excluded = build(&[condition]);
            excluded.config.conditions.insert(condition.into(), true);
            assert_supported_pair(&excluded, &effective, 1.00);
        }
        let mut stacked = build(&AILMENTS);
        for multiplier in ["IgniteStacks", "ChillStacks", "ShockStacks", "PoisonStack"] {
            stacked.config.multipliers.insert(multiplier.into(), 100.0);
        }
        assert_supported_pair(&stacked, &effective, 1.60);
        let mut counts_only = build(&[]);
        counts_only.config.multipliers = stacked.config.multipliers.clone();
        assert_supported_pair(&counts_only, &effective, 1.00);
        assert_supported_pair(&stacked, &options(false), 1.00);
    }

    #[test]
    fn raw_false_edits_override_imported_legacy_ailment_conditions() {
        let opts = options(true);
        for ailment in AILMENTS {
            let xml = format!(
                r#"<PathOfBuilding2>
  <Build level="90" className="Warrior" mainSocketGroup="1"/>
  <Skills><Skill enabled="true">
    <Gem skillId="ArmourBreakerPlayer" level="20" enabled="true"/>
    <Gem skillId="{SUPPORT}" level="1" enabled="true"/>
  </Skill></Skills>
  <Config><Input name="condition{ailment}" boolean="true"/></Config>
</PathOfBuilding2>"#
            );
            let mut imported = parse_build(&xml).unwrap();
            // Retain the legacy snapshot even after the live raw-input edit.
            assert_eq!(imported.config.conditions.get(ailment), Some(&true));
            assert_supported_pair(&imported, &opts, 1.12);
            imported
                .config
                .raw_inputs
                .values
                .insert(format!("condition{ailment}"), ConfigInputValue::Bool(false));
            assert_supported_pair(&imported, &opts, 1.00);
            assert_eq!(imported.config.conditions.get(ailment), Some(&true));
        }
    }

    #[test]
    fn benefit_and_prohibition_stay_in_the_enabled_compatible_group() {
        let opts = options(true);
        let supported = build(&AILMENTS);
        let mut disabled = supported.clone();
        disabled.socket_groups[0] = parse_build(&format!(
            r#"<PathOfBuilding2><Build level="1" className="Warrior"/>
<Skills><Skill enabled="true">
  <Gem skillId="ArmourBreakerPlayer" level="20" enabled="true"/>
  <Gem skillId="{SUPPORT}" level="1" enabled="false"/>
</Skill></Skills></PathOfBuilding2>"#
        ))
        .unwrap()
        .socket_groups
        .remove(0);
        let mut bare = supported.clone();
        bare.socket_groups[0].gem_skills.remove(1);
        let bare_session = calculate_with_data_session(&bare, data(), &opts).unwrap();
        let disabled_session = calculate_with_data_session(&disabled, data(), &opts).unwrap();
        assert_eq!(disabled_session.output(), bare_session.output());
        assert!(disabled_session.unsupported_modifier_texts().is_empty());
        let source_id = format!("{SUPPORT}.{STAT}");

        for host in ["ArmourBreakerPlayer", "FireballPlayer"] {
            let other = supported
                .clone()
                .add_socket_group(SocketGroup::new().with_gem_skill(host, 20))
                .with_main_socket_group(2);
            let mut without_support = other.clone();
            without_support.socket_groups[0].gem_skills.remove(1);
            let other_session = calculate_with_data_session(&other, data(), &opts).unwrap();
            let other_bare = calculate_with_data_session(&without_support, data(), &opts).unwrap();
            assert_same_skill_outputs(other_session.output(), other_bare.output());
            assert!(other_session.unsupported_modifier_texts().is_empty());
            assert!(other_session.all_mods().iter().all(|modifier| {
                modifier
                    .origin
                    .as_ref()
                    .is_none_or(|origin| origin.source_id.id != source_id)
            }));
        }
        let mut spell = coverage_build("FireballPlayer", Some(SUPPORT));
        spell.config = supported.config.clone();
        let mut bare_spell = spell.clone();
        bare_spell.socket_groups[0].gem_skills.remove(1);
        let rejected = calculate_with_data_session(&spell, data(), &opts).unwrap();
        let plain = calculate_with_data_session(&bare_spell, data(), &opts).unwrap();
        assert!(plain.output().total_hit_avg > 0.0);
        assert!(plain.output().ignite_dps > 0.0);
        assert_same_skill_outputs(rejected.output(), plain.output());
        assert_eq!(rejected.output().req_str, 5.0);
        assert!(rejected.unsupported_modifier_texts().is_empty());
    }
}

/// Uhtred's supports grant levels by how many other supports modify the
/// skill: Exodus "+3 to level of Supported Skills if no other Supports are
/// modifying them", Omen +2 "if exactly one other Support is", Augury +2 "if
/// exactly two other Supports are". The buff the skill grants is the one of
/// that level: Elemental Conflux's damage is 59% at level 20 and one more per
/// level.
#[test]
fn uhtreds_supports_grant_levels_by_the_other_supports() {
    use pobr_build::{Build, CharacterIdentity, SocketGroup};
    let data = load_build_data();
    let conflux = |supports: &[&str]| {
        let mut group = SocketGroup::new().with_gem_skill("ElementalConfluxPlayer", 20);
        for support in supports {
            group = group.with_gem_skill(*support, 1);
        }
        let build = Build::new()
            .with_character(CharacterIdentity {
                level: 90,
                class_name: "Witch".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(group);
        let diagnosis = pobr_build::diagnose(&build, &data, &panel_opts()).unwrap();
        let level = diagnosis
            .skills
            .iter()
            .find(|skill| skill.skill_id == "ElementalConfluxPlayer")
            .map(|skill| skill.gem_level + skill.bonus_levels)
            .unwrap();
        let usage = pobr_build::usage::line_usage(&diagnosis, &build, &data);
        let buff = usage
            .lines
            .iter()
            .find(|line| {
                line.source_id.as_deref().is_some_and(|id| {
                    id.starts_with("buff.ElementalConfluxPlayer.skill_elemental_conflux")
                })
            })
            .map(|line| line.modifiers[0].value.clone());
        (level, buff)
    };
    let (level, buff) = conflux(&[]);
    assert_eq!(level, 20);
    let exodus = conflux(&["SupportUhtredExodusPlayer"]);
    assert_eq!(exodus.0, 23);
    assert_ne!(exodus.1, buff, "the buff must follow the level");
    // Lightning Mastery's own level makes another support: Exodus gives none.
    assert_eq!(
        conflux(&["SupportUhtredExodusPlayer", "SupportLightningMasteryPlayer"]).0,
        21
    );
    assert_eq!(
        conflux(&["SupportUhtredOmenPlayer", "SupportLightningMasteryPlayer"]).0,
        23
    );
    assert_eq!(
        conflux(&[
            "SupportUhtredAuguryPlayer",
            "SupportLightningMasteryPlayer",
            "SupportDiallasDesirePlayer",
        ])
        .0,
        24
    );
}

/// A support on any of a gem's effects modifies the gem: Physical Mastery
/// ("Supported Physical Skills have +1 to Level", requires Physical) supports
/// Iron Ward's Physical nova, not its buff, and Iron Ward is a level higher.
#[test]
fn a_support_on_a_secondary_effect_levels_the_whole_gem() {
    use pobr_build::{Build, CharacterIdentity, SocketGroup};
    let data = load_build_data();
    let iron_ward = |supports: &[&str]| {
        let mut group = SocketGroup::new().with_gem_skill("IronWardPlayer", 20);
        for support in supports {
            group = group.with_gem_skill(*support, 1);
        }
        let build = Build::new()
            .with_character(CharacterIdentity {
                level: 90,
                class_name: "Warrior".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(group);
        let diagnosis = pobr_build::diagnose(&build, &data, &panel_opts()).unwrap();
        diagnosis
            .skills
            .iter()
            .find(|skill| skill.skill_id == "IronWardPlayer")
            .map(|skill| skill.gem_level + skill.bonus_levels)
            .unwrap()
    };
    assert_eq!(iron_ward(&[]), 20);
    assert_eq!(iron_ward(&["SupportPhysicalMasteryPlayer"]), 21);
}

// Brutality III's game stat set grants 30% more Physical damage and forbids
// Elemental/Chaos damage. These restrictions are applied after conversion.
#[test]
fn brutality_iii_restricts_selected_player_damage_and_preserves_support_gates() {
    use pobr_build::diagnose;
    use pobr_core::calc::OutputTable;
    use pobr_data::prelude::{DamageType, SourceKind};

    let data = load_build_data();
    let stats = data.effect_stats("SupportBrutalityPlayerThree", 1, 0, None);
    for (stat, value) in [
        ("support_brutality_physical_damage_+%_final", 30.0),
        ("deal_no_elemental_damage", 1.0),
        ("base_deal_no_chaos_damage", 1.0),
    ] {
        assert!(
            stats.all().any(|s| s.stat == stat && s.value == value),
            "{stat}"
        );
    }
    let support = |enabled| {
        format!(r#"<Gem skillId="SupportBrutalityPlayerThree" level="1" enabled="{enabled}"/>"#)
    };
    let run = |skill: &str, linked: &str, other: &str, convert: bool| {
        let build = parse_build(&format!(r#"<PathOfBuilding2>
<Build level="80" className="Warrior" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="{skill}" level="20" enabled="true"/>{linked}</Skill>{other}</Skills>
<Items activeItemSet="1"><Item id="1">Rarity: NORMAL
Wooden Club
Implicits: 0
</Item><Item id="2">Rarity: NORMAL
Splintered Tower Shield
Armour: 1000
Implicits: 0
</Item><ItemSet id="1"><Slot name="Weapon 1" itemId="1"/><Slot name="Weapon 2" itemId="2"/></ItemSet></Items>
</PathOfBuilding2>"#)).unwrap();
        let mut options = panel_opts();
        options.extra_modifier_texts = vec![
            "Adds 100 to 100 Fire Damage to Attacks".into(),
            "Adds 100 to 100 Cold Damage to Attacks".into(),
            "Adds 100 to 100 Lightning Damage to Attacks".into(),
            "Gain 10% of Physical Damage as Extra Chaos Damage".into(),
            "100% chance to Poison on Hit".into(),
            "100% chance to inflict Bleeding on Hit".into(),
        ];
        if convert {
            options
                .extra_modifier_texts
                .push("50% of Physical Damage Converted to Fire Damage".into());
        }
        diagnose(&build, data, &options).unwrap().session
    };
    let typed = |out: &OutputTable, ty| {
        out.damage_components
            .iter()
            .filter(|c| c.damage_type == ty)
            .map(|c| c.avg())
            .sum::<f64>()
    };
    let bare = run("ShieldWallPlayer", "", "", true);
    let full_physical = run("ShieldWallPlayer", "", "", false);
    let supported = run("ShieldWallPlayer", &support(true), "", true);
    assert!(typed(bare.output(), DamageType::Physical) > 0.0);
    assert!(
        (typed(bare.output(), DamageType::Physical) * 2.0
            - typed(full_physical.output(), DamageType::Physical))
        .abs()
            < 1e-6
    );
    assert!(
        (typed(supported.output(), DamageType::Physical)
            - typed(bare.output(), DamageType::Physical) * 1.3)
            .abs()
            < 1e-6
    );
    for ty in [
        DamageType::Fire,
        DamageType::Cold,
        DamageType::Lightning,
        DamageType::Chaos,
    ] {
        assert!(typed(bare.output(), ty) > 0.0, "{ty:?}");
        assert_eq!(typed(supported.output(), ty), 0.0, "{ty:?}");
    }
    assert!(bare.output().ignite_dps > 0.0);
    assert_eq!(supported.output().ignite_dps, 0.0);
    assert!(bare.output().poison_dps > 0.0);
    assert_eq!(supported.output().poison_dps, 0.0);
    assert!(supported.output().bleed_dps > 0.0);
    assert!(supported.ailment_stage_terms("PoisonDPS").is_none());
    assert!(supported.ailment_stage_terms("BleedDPS").is_some());
    for stat in [
        "PoisonDuration",
        "PoisonStacksMax",
        "PoisonStackPotentialPercent",
        "PoisonChance",
    ] {
        assert_eq!(
            pobr_core::display_value(supported.output(), stat),
            0.0,
            "{stat}"
        );
    }
    assert_eq!(
        supported.stat_formula("TotalDPS").unwrap().value,
        supported.output().dps
    );
    for name in ["DealNoFire", "DealNoCold", "DealNoLightning", "DealNoChaos"] {
        let mods = supported.mods_named(name);
        assert_eq!(mods.len(), 1);
        let origin = mods[0].origin.as_ref().unwrap();
        assert_eq!(origin.source_id.kind, SourceKind::SupportGem);
        assert!(origin.source_id.id.contains("SupportBrutalityPlayerThree"));
        assert!(origin.raw_text.is_some());
    }
    let other = format!(
        r#"<Skill enabled="true"><Gem skillId="ShieldWallPlayer" level="20" enabled="true"/>{}</Skill>"#,
        support(true)
    );
    for scoped_out in [
        run("ShieldWallPlayer", &support(false), "", true),
        run("ShieldWallPlayer", "", &other, true),
    ] {
        assert_eq!(
            scoped_out.output().damage_components,
            bare.output().damage_components
        );
        assert!(scoped_out.mods_named("DealNoFire").is_empty());
        assert!(scoped_out.mods_named("DealNoChaos").is_empty());
        assert_eq!(scoped_out.output().poison_dps, bare.output().poison_dps);
    }
    let unsupported = run("ConvalescencePlayer", &support(true), "", true);
    let unsupported_bare = run("ConvalescencePlayer", "", "", true);
    assert!(unsupported.mods_named("DealNoFire").is_empty());
    assert!(unsupported.mods_named("DealNoChaos").is_empty());
    assert_eq!(
        unsupported.output().damage_components,
        unsupported_bare.output().damage_components
    );
}

#[test]
fn brutality_iii_restricts_linked_warrior_and_reaver_minions_only() {
    use pobr_build::diagnose;
    use pobr_data::prelude::{DamageType, SourceKind};

    let data = load_build_data();
    let run = |enabled| {
        let build = parse_build(&format!(r#"<PathOfBuilding2>
<Build level="80" className="Sorceress" ascendClassName="Disciple of Varashta" mainSocketGroup="1"/>
<Tree activeSpec="1"><Spec nodes="30720"/></Tree>
<Skills><Skill enabled="true"><Gem skillId="FireballPlayer" level="20" enabled="true"/></Skill>
<Skill enabled="true"><Gem skillId="SummonSkeletalWarriorsPlayer" level="20" enabled="true" skillMinion="RaisedSkeletonWarriors" skillMinionSkill="1"/><Gem skillId="SupportBrutalityPlayerThree" level="1" enabled="{enabled}"/></Skill>
<Skill enabled="true"><Gem skillId="SummonSkeletalReaversPlayer" level="20" enabled="true" skillMinion="RaisedSkeletonReaver" skillMinionSkill="1"/><Gem skillId="SupportBrutalityPlayerThree" level="1" enabled="{enabled}"/></Skill>
</Skills></PathOfBuilding2>"#)).unwrap();
        let mut options = panel_opts();
        options.extra_modifier_texts = ["Fire", "Cold", "Lightning"]
            .map(|ty| format!("Minions gain 10% of Physical Damage as Extra {ty} Damage"))
            .into();
        diagnose(&build, data, &options).unwrap().session
    };
    let bare = run(false);
    let supported = run(true);
    assert_eq!(bare.minion_count(), 2);
    assert_eq!(supported.minion_count(), 2);
    assert_eq!(
        supported.output().damage_components,
        bare.output().damage_components
    );
    assert!(supported.mods_named("DealNoFire").is_empty());
    assert!(supported.mods_named("DealNoChaos").is_empty());
    for (index, summoner) in [
        "SummonSkeletalWarriorsPlayer",
        "SummonSkeletalReaversPlayer",
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(supported.output().minions[index].summoned_by, summoner);
        assert_eq!(
            supported.output().minions[index].skill_id,
            "MinionMeleeStep"
        );
        let before = bare.minion_output(index).unwrap();
        let after = supported.minion_output(index).unwrap();
        let typed = |out: &pobr_core::calc::OutputTable, ty| {
            out.damage_components
                .iter()
                .filter(|c| c.damage_type == ty)
                .map(|c| c.avg())
                .sum::<f64>()
        };
        let phys = typed(before, DamageType::Physical);
        assert!(phys > 0.0);
        assert!((typed(before, DamageType::Chaos) - phys * 0.1).abs() < 1e-6);
        assert!((typed(after, DamageType::Physical) - phys * 1.3).abs() < 1e-6);
        for ty in [
            DamageType::Fire,
            DamageType::Cold,
            DamageType::Lightning,
            DamageType::Chaos,
        ] {
            assert!(typed(before, ty) > 0.0, "{index} {ty:?}");
            assert_eq!(typed(after, ty), 0.0, "{index} {ty:?}");
        }
        for name in ["DealNoFire", "DealNoCold", "DealNoLightning", "DealNoChaos"] {
            assert!(bare.minion_stat_mods(index, name).is_empty());
            let mods = supported.minion_stat_mods(index, name);
            assert_eq!(mods.len(), 1);
            assert!(mods[0].active);
            let origin = mods[0].modifier.origin.as_ref().unwrap();
            assert_eq!(origin.source_id.kind, SourceKind::SupportGem);
            assert!(origin.source_id.id.contains("SupportBrutalityPlayerThree"));
        }
        let formula = supported.minion_stat_formula(index, "TotalDPS").unwrap();
        assert!((formula.value - after.dps).abs() < 1e-6);
    }
}

/// The source hit and the ailment's dealt type are independent. A prohibited
/// destination has neither damage nor a duration/instance to pass to stacking.
#[test]
fn brutality_iii_ailment_destinations_preserve_scope_and_trace() {
    use pobr_core::calc::DamageComponent;
    use pobr_core::calc::ailment::{
        AilmentSource, bleed_traced, cross_type_source_hit, ignite_traced, poison_traced,
        shock_traced,
    };
    use pobr_core::{CalcConfig, ModDb, ModTag, Modifier, TraceGraph};
    use pobr_data::prelude::*;

    let source = AilmentSource::new(1000.0, 1.0, 0.0, false);
    let cfg = CalcConfig::spell();
    let enemy = ModDb::new();
    let origin = ModifierSource::new(SourceId::new(
        SourceKind::SupportGem,
        "support.destination_gate",
    ));
    let mut base = ModDb::new();
    base.add_mod(Modifier::number("AilmentChance", ModType::Base, 100.0));
    let baseline_shock = shock_traced(&source, &base, &enemy, &cfg, 1000.0, &mut TraceGraph::new());
    for (ailment, destination, other) in [
        (AilmentType::Bleed, "DealNoPhysical", "DealNoChaos"),
        (AilmentType::Ignite, "DealNoFire", "DealNoChaos"),
        (AilmentType::Poison, "DealNoChaos", "DealNoPhysical"),
    ] {
        for (flag, blocked) in [
            (Modifier::flag(destination), true),
            (Modifier::flag("DealNoDamage"), true),
            (Modifier::flag(other), false),
            (
                Modifier::flag(destination).with_keyword_flags(KeywordFlags::HIT),
                false,
            ),
            (
                Modifier::flag(destination).with_tag(ModTag::condition("NotEnabled", false)),
                false,
            ),
        ] {
            let mut player = base.clone();
            player.add_mod(flag.with_origin(origin.clone()));
            let mut trace = TraceGraph::new();
            let (out, node) = match ailment {
                AilmentType::Bleed => bleed_traced(&source, &player, &enemy, &cfg, &mut trace),
                AilmentType::Ignite => {
                    ignite_traced(&source, &player, &enemy, &cfg, 1000.0, &mut trace)
                }
                AilmentType::Poison => poison_traced(&source, &player, &enemy, &cfg, &mut trace),
                _ => unreachable!(),
            };
            if blocked {
                assert_eq!(out.chance, 0.0);
                assert_eq!(out.magnitude_dps, 0.0);
                assert_eq!(out.expected_dps, 0.0);
                assert_eq!(out.duration_secs, 0.0);
                assert_eq!(out.source_damage, 0.0);
            } else {
                assert_eq!(out.chance, 1.0);
                assert!(out.magnitude_dps > 0.0);
                assert!(out.duration_secs > 0.0);
            }
            assert_eq!(
                trace
                    .source_ancestors(node)
                    .iter()
                    .any(|s| s.id == origin.source_id.id),
                blocked
            );
            let (chance, effect, shock_node) =
                shock_traced(&source, &player, &enemy, &cfg, 1000.0, &mut trace);
            assert_eq!((chance, effect), (baseline_shock.0, baseline_shock.1));
            assert!(
                !trace
                    .source_ancestors(shock_node)
                    .iter()
                    .any(|s| s.id == origin.source_id.id)
            );
        }
    }

    // Native passive 42680 changes both source eligibility and dealt type,
    // but only for Fire Spells. Normal Ignite remains Fire even from a
    // separately eligible non-Fire source hit.
    let data = load_build_data();
    let fire = SkillTypes::from_pob2_name("Fire").unwrap();
    for (types, covenant) in [
        (SkillTypes::SPELL | fire, true),
        (SkillTypes::SPELL, false),
        (SkillTypes::ATTACK | fire, false),
    ] {
        let cfg = CalcConfig::default().with_skill_types(types);
        let mut player = base.clone();
        for text in [
            "Chaos Damage from Fire Spells Contributes to Flammability and Ignite Magnitudes",
            "Ignite inflicted with Fire Spells deals Chaos Damage instead of Fire Damage",
        ] {
            let parsed =
                pobr_core::mod_parser::parse_mod_engine(text, data.parser_rules.as_ref().unwrap());
            assert!(parsed.is_complete());
            player.add_list(parsed.mods.into_iter().map(|m| {
                m.with_origin(
                    ModifierSource::new(SourceId::new(SourceKind::PassiveNode, "node.42680"))
                        .with_raw_text(text),
                )
            }));
        }
        let chaos_hit = [DamageComponent::new(DamageType::Chaos, 1000.0, 1000.0)];
        assert_eq!(
            cross_type_source_hit(AilmentType::Ignite, &chaos_hit, &player, &cfg),
            if covenant { 1000.0 } else { 0.0 }
        );
        for (flag, blocked) in [("DealNoChaos", covenant), ("DealNoFire", !covenant)] {
            let mut restricted = player.clone();
            restricted.add_mod(Modifier::flag(flag).with_origin(origin.clone()));
            let mut trace = TraceGraph::new();
            let (ignite, node) =
                ignite_traced(&source, &restricted, &enemy, &cfg, 1000.0, &mut trace);
            assert_eq!(ignite.expected_dps == 0.0, blocked);
            assert_eq!(ignite.chance == 0.0, blocked);
            assert_eq!(
                trace
                    .source_ancestors(node)
                    .iter()
                    .any(|s| s.id == origin.source_id.id),
                blocked
            );
            assert_eq!(
                trace
                    .source_ancestors(node)
                    .iter()
                    .any(|s| s.id == "node.42680"),
                covenant
            );
        }
    }
}
