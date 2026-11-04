//! Quality consumers backed by the 0.5.5.5 GrantedEffectQualityStats and stat sets.
//! These are selected-state checks, not estimates of retaliation or totem uptime.

use pobr_build::{
    Build, BuildData, DataOrchestratorOptions, SocketGroup, calculate_with_data_session,
};
use pobr_core::calc::damage::lucky_hit_chance;
use pobr_core::calc::{CompletedCalculationSession, DamageComponent};
use pobr_core::skill_env::{EffectStats, StatMapCtx, mapped_effect_stat_modifiers};
use pobr_core::{CalcConfig, ModDb, Modifier};
use pobr_data::catalog::SkillDamageStat;
use pobr_data::passive_tree::NodeId;
use pobr_data::prelude::*;

const NODE: u32 = 14429; // Advanced Thaumaturgy, AscendancyMercenary3Notable4.
const WIND: &str = "wind_dancer_damage_+%_final_per_stage";
const STORM: &str = "sandstorm_swipe_storm_damage_+%_final_per_stage";
const ARC: &str = "active_skill_projectile_damage_+%_final_for_each_remaining_chain";
const LUCKY: &str = "chance_for_extra_damage_roll_with_lightning_damage_%";

fn build(group: SocketGroup, node: bool) -> Build {
    let mut build = Build::new()
        .add_socket_group(group)
        .with_main_socket_group(1);
    if node {
        build.tree.allocated_nodes.push(NodeId(NODE));
    }
    build
}

fn run(build: &Build, data: &BuildData) -> CompletedCalculationSession {
    calculate_with_data_session(build, data, &DataOrchestratorOptions::default()).unwrap()
}

fn stat_mods(session: &CompletedCalculationSession, name: &str, stat: &str) -> Vec<Modifier> {
    session
        .mods_named(name)
        .into_iter()
        .filter(|modifier| {
            modifier
                .origin
                .as_ref()
                .is_some_and(|origin| origin.source_id.id.ends_with(stat))
        })
        .cloned()
        .collect()
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
}

fn more(mods: Vec<Modifier>, cfg: &CalcConfig) -> f64 {
    let mut db = ModDb::new();
    db.add_list(mods);
    db.more(cfg, &[ModName::from("Damage")])
}

fn quality_origin(
    modifier: &Modifier,
    effect: &str,
    stat: &str,
    quality: u32,
    base: f64,
    added: f64,
) {
    let origin = modifier.origin.as_ref().unwrap();
    assert_eq!(origin.source_id.kind, SourceKind::GemQuality);
    assert_eq!(
        origin.source_id.id,
        format!("gem.{effect}.q{quality}.{stat}")
    );
    let parent = origin.parent_source_id.as_ref().expect("base provenance");
    assert_eq!(parent.kind, SourceKind::SkillGem);
    assert_eq!(parent.id, format!("skill.{stat}"));
    assert!(
        origin
            .raw_text
            .as_ref()
            .unwrap()
            .contains(&format!("base {base} + quality {added}"))
    );
}

#[test]
fn wind_dancer_adds_alt_quality_before_its_capped_stack_multiplier() {
    let data = crate::test_data();
    assert_eq!(
        data.passive_nodes[&NODE].name.as_deref(),
        Some("Advanced Thaumaturgy")
    );
    for node in [false, true] {
        for quality in [0, 20, 26, 40] {
            // Retaliation is the active effect granted by the Wind Dancer gem.
            let group = SocketGroup::new()
                .with_gem_skill_quality("WindDancerPlayer", 20, quality)
                .with_main_active_skill(2);
            let session = run(&build(group, node), data);
            let mods = stat_mods(&session, "Damage", WIND);
            assert_eq!(mods.len(), 1, "q{quality}, node={node}");
            let added = if node { 2.5 * f64::from(quality) } else { 0.0 };
            assert_eq!(mods[0].value.as_number(), Some(50.0 + added));
            if added != 0.0 {
                quality_origin(
                    &mods[0],
                    "TriggeredWindDancerPlayer",
                    WIND,
                    quality,
                    50.0,
                    added,
                );
            } else {
                assert_eq!(
                    mods[0].origin.as_ref().unwrap().source_id.kind,
                    SourceKind::SkillGem
                );
            }
            for stacks in [0.0_f64, 1.0, 3.0, 9.0] {
                let cfg = CalcConfig::new()
                    .with_multiplier("WindDancerStacks", stacks)
                    .with_multiplier("WindDancerStacksLimit", 3.0);
                close(
                    more(mods.clone(), &cfg),
                    1.0 + stacks.min(3.0) * (50.0 + added) / 100.0,
                );
            }
        }
    }
}

#[test]
fn whirling_slash_quality_uses_only_the_selected_storm_stat_set() {
    let data = crate::test_data();
    for node in [false, true] {
        for quality in [0, 20, 26, 40] {
            for set in [1, 2] {
                let mut group =
                    SocketGroup::new().with_gem_skill_quality("WhirlingSlashPlayer", 20, quality);
                group.gem_skills[0].stat_set_index = Some(set);
                let session = run(&build(group, node), data);
                let mods = stat_mods(&session, "Damage", STORM);
                if set == 1 {
                    assert!(
                        mods.is_empty(),
                        "the unselected storm cannot change the melee hit"
                    );
                    continue;
                }
                assert_eq!(mods.len(), 1);
                let value = 150.0 + 2.0 * f64::from(quality);
                assert_eq!(mods[0].value.as_number(), Some(value));
                for stages in [0.0_f64, 1.0, 2.0, 9.0] {
                    let cfg = CalcConfig::new().with_multiplier("WhirlwindStageAfterFirst", stages);
                    close(
                        more(mods.clone(), &cfg),
                        1.0 + stages.min(2.0) * value / 100.0,
                    );
                }
            }
        }
    }
}

#[test]
fn arc_quality_reads_remaining_chains_and_resolved_level_values() {
    let data = crate::test_data();
    for node in [false, true] {
        for quality in [0, 20, 40] {
            let group = SocketGroup::new().with_gem_skill_quality("ArcPlayer", 20, quality);
            let session = run(&build(group, node), data);
            let mods = stat_mods(&session, "Damage", ARC);
            // The current game set has no base per-chain damage stat; only alt quality.
            assert_eq!(mods.len(), usize::from(node && quality != 0));
            let mut db = ModDb::new();
            db.add_list(mods);
            let cfg = CalcConfig::new()
                .with_flags(ModFlags::PROJECTILE | ModFlags::HIT)
                .with_keyword_flags(KeywordFlags::HIT);
            for remaining in [0.0, 3.0, 9.0] {
                let selected = cfg.clone().with_multiplier("ChainRemaining", remaining);
                let factor = db.more(&selected, &[ModName::from("Damage")]);
                let value = if node {
                    (0.15 * f64::from(quality)).trunc()
                } else {
                    0.0
                };
                assert!(
                    (factor - (1.0 + remaining * value / 100.0)).abs() < 1e-9,
                    "q{quality} node={node} remaining={remaining}: {factor}; {:?}",
                    db.named("Damage").collect::<Vec<_>>()
                );
            }
        }
    }

    // Perturb the level table to exercise a shared stat at different effective
    // levels. Quality must merge into level 21's 25, not raw level 20's 15.
    let mut data = data.clone();
    for row in &mut data.skill_stat_sets.get_mut("ArcPlayer").unwrap().sets[0].levels {
        row.stats.push(SkillDamageStat {
            stat: ARC.into(),
            value: if row.gem_level >= 21 { 25.0 } else { 15.0 },
        });
    }
    let mut build = build(
        SocketGroup::new().with_gem_skill_quality("ArcPlayer", 20, 20),
        true,
    );
    build
        .config
        .global_modifier_texts
        .push("+1 to Level of all Spell Skills".into());
    let session = run(&build, &data);
    let mods = stat_mods(&session, "Damage", ARC);
    assert_eq!(mods.len(), 1);
    assert_eq!(mods[0].value.as_number(), Some(28.0));
    quality_origin(&mods[0], "ArcPlayer", ARC, 20, 25.0, 3.0);
}

#[test]
fn ordinary_and_alt_rows_of_one_stat_map_once_even_when_the_total_cancels() {
    let data = crate::test_data();
    for (added, expected) in [(30.0, 4.0), (-20.0, 2.5), (-70.0, 1.0)] {
        let row = |value| SkillDamageStat {
            stat: WIND.into(),
            value,
        };
        let stats = EffectStats {
            base: vec![row(50.0)],
            quality: vec![row(20.0), row(added)],
        };
        let mods = mapped_effect_stat_modifiers(
            &mut StatMapCtx::data(data.stat_map_catalog.as_deref()),
            &stats,
            SourceId::new(SourceKind::SkillGem, "skill"),
            "TriggeredWindDancerPlayer",
            Some("1"),
            20,
        );
        assert_eq!(mods.len(), usize::from(expected != 1.0));
        let cfg = CalcConfig::new()
            .with_multiplier("WindDancerStacks", 3.0)
            .with_multiplier("WindDancerStacksLimit", 3.0);
        close(more(mods, &cfg), expected);
    }
}

#[test]
fn lightning_lucky_quality_reaches_hit_rolls_without_changing_cold_or_crit_chance() {
    let data = crate::test_data();
    for skill in ["LightningArrowPlayer", "GatheringStormPlayer"] {
        for quality in [0, 20, 26, 40] {
            let group = SocketGroup::new().with_gem_skill_quality(skill, 20, quality);
            let off = run(&build(group.clone(), false), data);
            let on = run(&build(group, true), data);
            close(on.output().crit_chance, off.output().crit_chance);
            for (session, node) in [(&off, false), (&on, true)] {
                let probability = if node {
                    1.5 * f64::from(quality) / 100.0
                } else {
                    0.0
                };
                assert_eq!(
                    stat_mods(session, "LightningLuckyHitsChance", LUCKY).len(),
                    usize::from(probability > 0.0)
                );
                for crit in [false, true] {
                    let chance = lucky_hit_chance(
                        session.mod_db(),
                        session.config(),
                        DamageType::Lightning,
                        crit,
                    );
                    close(chance, probability);
                    close(
                        DamageComponent::new(DamageType::Lightning, 10.0, 100.0)
                            .avg_with_lucky(chance),
                        55.0 + 15.0 * probability,
                    );
                    close(
                        lucky_hit_chance(
                            session.mod_db(),
                            session.config(),
                            DamageType::Cold,
                            crit,
                        ),
                        0.0,
                    );
                }
            }
        }
    }
    // The native mapping must also work without the compatibility stat map.
    let mods = mapped_effect_stat_modifiers(
        &mut StatMapCtx::data(None),
        &EffectStats {
            base: vec![],
            quality: vec![SkillDamageStat {
                stat: LUCKY.into(),
                value: 30.0,
            }],
        },
        SourceId::new(SourceKind::SkillGem, "skill"),
        "LightningArrowPlayer",
        None,
        20,
    );
    assert_eq!(mods.len(), 1);
    assert_eq!(mods[0].name.as_str(), "LightningLuckyHitsChance");
    assert_eq!(mods[0].value.as_number(), Some(30.0));
}

#[test]
fn blasphemy_quality_scales_only_its_compatible_enabled_curse() {
    use pobr_core::skill_env::{buff_skill_specs, gemling_quality_flag};
    let data = crate::test_data();
    let specs = |build: &Build| {
        let ids: Vec<_> = build.tree.allocated_nodes.iter().map(|id| id.0).collect();
        let alt = gemling_quality_flag(
            &ids,
            &[],
            data.passive_nodes_for(build.tree_version.as_deref()),
        );
        buff_skill_specs(
            &mut StatMapCtx::data(data.stat_map_catalog.as_deref()),
            build,
            data,
            &[],
            alt,
        )
    };
    for node in [false, true] {
        for quality in [0, 20, 26, 40] {
            let group = SocketGroup::new()
                .with_gem_skill_quality("BlasphemyPlayer", 20, quality)
                .with_gem_skill("DespairPlayer", 20)
                .with_main_active_skill(2);
            let host = build(group, node);
            let curse = specs(&host)
                .into_iter()
                .find(|spec| spec.skill_id == "DespairPlayer")
                .unwrap();
            close(
                curse.local_effect_inc,
                if node { f64::from(quality) } else { 0.0 },
            );
            // Level 20 support_blasphemy_curse_effect_+%_final = -41.
            close(curse.local_effect_more, 0.59);

            let mut disabled = host.clone();
            disabled.socket_groups[0].gem_skills[0].enabled = false;
            disabled.socket_groups[0].main_active_skill = Some(1);
            // Disabled gems are removed by build preparation, before core
            // buff/spec assembly; exercise that real orchestration boundary.
            let session = run(&disabled, data);
            assert!(stat_mods(&session, "SelectedCurseEffect", "curse_effect_+%").is_empty());
            close(session.output().curse_effect_mod, 1.0);

            let mut elsewhere = host.clone();
            let gem = elsewhere.socket_groups[0].gem_skills.remove(0);
            elsewhere.socket_groups[0].main_active_skill = Some(1);
            let mut other = SocketGroup::new();
            other.gem_skills.push(gem);
            elsewhere.socket_groups.push(other);
            let curse = specs(&elsewhere)
                .into_iter()
                .find(|spec| spec.skill_id == "DespairPlayer")
                .unwrap();
            close(curse.local_effect_inc, 0.0);
            close(curse.local_effect_more, 1.0);
        }
    }
    let group = SocketGroup::new()
        .with_gem_skill_quality("BlasphemyPlayer", 20, 40)
        .with_gem_skill("FireballPlayer", 20)
        .with_main_active_skill(2);
    let session = run(&build(group, true), data);
    assert!(
        stat_mods(&session, "SelectedCurseEffect", "curse_effect_+%").is_empty(),
        "Blasphemy cannot support Fireball"
    );
}
