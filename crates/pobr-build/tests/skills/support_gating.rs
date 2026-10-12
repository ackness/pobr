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
        let mut data = load_build_data();
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
        let mut data = load_build_data();
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
        assert_eq!(other.output(), other_bare.output());
        assert_eq!(incompatible.output(), bare.output());
    }
}

#[test]
fn support_coverage_resource_effects_follow_the_stat_not_the_gem_name() {
    for (source, stat, _, _) in UNMODELED_RESOURCE_EFFECTS {
        let mut data = load_build_data();
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
    let mut data = load_build_data();
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

mod elemental_armament {
    use super::*;
    use pobr_build::{Build, SocketGroup, calculate_with_data_session};
    use pobr_core::calc::CalculationSession;
    use pobr_core::rules::config_interpreter::ConfigInputValue;
    use pobr_data::prelude::{DamageType, ModType, SourceKind};
    use std::sync::LazyLock;

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

    static DATA: LazyLock<BuildData> = LazyLock::new(load_build_data);

    fn data() -> &'static BuildData {
        &DATA
    }

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
    // MORE Attack Damage; ModParser.lua:3866-3872 enumerates five types.
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
            assert_eq!(other_session.output(), other_bare.output(), "{host}");
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
        assert_eq!(rejected.output(), plain.output());
        assert!(rejected.unsupported_modifier_texts().is_empty());
    }
}
