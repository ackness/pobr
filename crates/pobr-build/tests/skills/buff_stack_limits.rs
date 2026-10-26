//! Stacking player buffs must retain their data-driven stack limit: the cap is
//! a `Multiplier:<X>Limit` BASE in the buff payload, read back as the per-stack
//! MORE tag's `limitVar`.
//!
//! - Wind Dancer (PoB2 act_dex.lua:11042-11053): 10% more Evasion per stage,
//!   capped by `wind_dancer_maximum_number_of_stages` (3).
//! - Scavenged Plating (PoB2 act_str.lua:16685-16703): 4% more Armour per
//!   stack, capped by `scavenged_plating_maximum_stacks_display` (10).

use pobr_build::{DataOrchestratorOptions, calculate_with_data, parse_build};
use pobr_core::calc::OutputTable;

struct StackingBuff {
    gem_id: &'static str,
    skill_id: &'static str,
    name: &'static str,
    config_input: &'static str,
    base_modifier: &'static str,
    stat: fn(&OutputTable) -> f64,
}

/// Each case is `(stacks, group enabled, gem enabled, expected stat)`.
fn assert_stacking_buff(buff: &StackingBuff, cases: &[(u32, bool, bool, f64)]) {
    let data = crate::test_data();
    let options = DataOrchestratorOptions {
        inject_character_base: false,
        mode_effective: true,
        extra_modifier_texts: vec![buff.base_modifier.into()],
        ..Default::default()
    };
    let StackingBuff {
        gem_id,
        skill_id,
        name,
        config_input,
        ..
    } = buff;
    for &(stacks, group_enabled, gem_enabled, expected) in cases {
        let xml = format!(
            r#"<PathOfBuilding2>
<Build level="80" className="Monk" ascendClassName="None" mainSocketGroup="1"/>
<Skills defaultGemLevel="20" activeSkillSet="1"><SkillSet id="1">
  <Skill enabled="true" mainActiveSkill="1">
    <Gem gemId="Metadata/Items/Gem/SkillGemFireball" skillId="FireballPlayer" nameSpec="Fireball" level="20" quality="0" enabled="true"/>
  </Skill>
  <Skill enabled="{group_enabled}" mainActiveSkill="1">
    <Gem gemId="{gem_id}" skillId="{skill_id}" nameSpec="{name}" level="20" quality="0" enabled="{gem_enabled}"/>
  </Skill>
</SkillSet></Skills>
<Config><Input name="{config_input}" number="{stacks}"/></Config>
</PathOfBuilding2>"#
        );
        let build = parse_build(&xml).expect("parse synthetic build");
        assert_eq!(build.socket_groups.len(), 2);
        assert_eq!(build.socket_groups[1].gem_skills[0].enabled, gem_enabled);
        let output = calculate_with_data(&build, &data, &options).expect("calculate");
        let actual = (buff.stat)(&output);
        assert!(
            (actual - expected).abs() < 1e-8,
            "{name}: stacks={stacks}, group={group_enabled}, gem={gem_enabled}: expected {expected}, got {actual}"
        );
    }
}

#[test]
fn wind_dancer_evasion_uses_stack_limit_and_enabled_state() {
    assert_stacking_buff(
        &StackingBuff {
            gem_id: "Metadata/Items/Gem/SkillGemWindDancer",
            skill_id: "WindDancerPlayer",
            name: "Wind Dancer",
            config_input: "windDancerStacks",
            base_modifier: "+1000 to Evasion Rating",
            stat: |output| output.evasion,
        },
        &[
            (0, true, true, 1000.0),
            (1, true, true, 1100.0),
            (3, true, true, 1300.0),
            (10, true, true, 1300.0),
            (3, false, true, 1000.0),
            (3, true, false, 1000.0),
        ],
    );
}

#[test]
fn scavenged_plating_armour_uses_stack_limit_and_enabled_state() {
    assert_stacking_buff(
        &StackingBuff {
            gem_id: "Metadata/Items/Gem/SkillGemScavengedPlating",
            skill_id: "ScavengedPlatingPlayer",
            name: "Scavenged Plating",
            config_input: "scavengedPlatingStacks",
            base_modifier: "+1000 to Armour",
            stat: |output| output.armour,
        },
        &[
            (0, true, true, 1000.0),
            (1, true, true, 1040.0),
            (10, true, true, 1400.0),
            (25, true, true, 1400.0),
            (10, false, true, 1000.0),
            (10, true, false, 1000.0),
        ],
    );
}

#[test]
fn ordinary_buff_copies_keep_strongest_quality_and_source() {
    use pobr_build::{Build, SocketGroup, calculate_with_data_session};
    use pobr_data::{passive_tree::NodeId, source::SourceKind};

    let coc = |quality| {
        let mut group = SocketGroup::new().with_gem_skill("MetaCastOnCritPlayer", 20);
        group.gem_skills[0].quality = quality;
        group
    };
    let mut group_disabled = coc(50);
    group_disabled.enabled = false;
    let mut gem_disabled = coc(50);
    gem_disabled.gem_skills[0].enabled = false;
    let mut alternate_only = coc(50);
    alternate_only.weapon_set = Some(2);
    let mut alternate_disabled = alternate_only.clone();
    alternate_disabled.enabled = false;
    let cases = [
        (vec![coc(22), coc(50)], 50.0),
        (vec![coc(50), coc(22)], 50.0),
        (vec![coc(50), coc(50)], 50.0),
        (vec![group_disabled, coc(22)], 22.0),
        (vec![coc(22), gem_disabled], 22.0),
        // Ordinary global buffs already include enabled alternate-set sources;
        // weapon-set binding alone does not disable their persistent buff.
        (vec![alternate_only, coc(22)], 50.0),
        (vec![alternate_disabled, coc(22)], 22.0),
    ];
    for (case, (copies, expected)) in cases.into_iter().enumerate() {
        let mut build = Build::new()
            .add_socket_group(SocketGroup::new().with_gem_skill("SparkPlayer", 20))
            .with_main_socket_group(1);
        // Advanced Thaumaturgy enables CoC's additional quality effect:
        // 1% increased global critical hit chance per quality.
        build.tree.allocated_nodes.push(NodeId(14429));
        build.socket_groups.extend(copies);
        let result = calculate_with_data_session(
            &build,
            crate::test_data(),
            &DataOrchestratorOptions::against_reference_enemy(),
        )
        .unwrap();
        assert!(
            (result.output().crit_chance - 0.09 * (1.0 + expected / 100.0)).abs() < 1e-9,
            "case {case}: crit={}, expected buff={expected}",
            result.output().crit_chance
        );
        let applied: Vec<_> = result
            .all_mods()
            .into_iter()
            .filter(|modifier| {
                modifier.origin.as_ref().is_some_and(|origin| {
                    origin.source_id.id
                        == "buff.MetaCastOnCritPlayer.cast_on_crit_global_critical_hit_chance_granted_+%"
                })
            })
            .collect();
        assert_eq!(applied.len(), 1, "same-name buff copies must not stack");
        assert_eq!(applied[0].value.as_number(), Some(expected));
        let origin = applied[0].origin.as_ref().unwrap();
        assert_eq!(origin.source_id.kind, SourceKind::SkillGem);
        assert_eq!(
            origin.raw_text.as_deref(),
            Some(
                format!(
                    "buff MetaCastOnCritPlayer cast_on_crit_global_critical_hit_chance_granted_+% ({expected})"
                )
                .as_str()
            )
        );
    }
}

#[test]
fn ordinary_buff_copies_preserve_wind_dancer_stack_limit() {
    use pobr_build::{Build, SocketGroup};
    use pobr_core::rules::config_interpreter::ConfigInputValue;

    let mut build = Build::new()
        .add_socket_group(SocketGroup::new().with_gem_skill("SparkPlayer", 20))
        .with_main_socket_group(1);
    build
        .config
        .raw_inputs
        .values
        .insert("windDancerStacks".into(), ConfigInputValue::Number(10.0));
    for _ in 0..2 {
        build
            .socket_groups
            .push(SocketGroup::new().with_gem_skill("WindDancerPlayer", 20));
    }
    let output = calculate_with_data(
        &build,
        crate::test_data(),
        &DataOrchestratorOptions {
            inject_character_base: false,
            extra_modifier_texts: vec!["+1000 to Evasion Rating".into()],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(output.evasion, 1300.0, "three stages, one strongest buff");
}

#[test]
fn mana_remnants_persistent_regen_does_not_scale_with_remnant_effect() {
    use pobr_build::{Build, SocketGroup, calculate_with_data_session};
    use pobr_core::calc::{CalculationSession, MinimalInput};
    use pobr_core::skill_env::{StatMapCtx, buff_skill_specs};
    use pobr_core::{CalcConfig, Modifier};
    use pobr_data::{modifier::ModType, passive_tree::NodeId, source::SourceKind};

    const SOURCE: &str =
        "buff.ManaRemnantsPlayer.mana_remnants_global_mana_regeneration_rate_granted_+%";
    let remnants = |quality, support: Option<&str>| {
        let mut group = SocketGroup::new().with_gem_skill("ManaRemnantsPlayer", 20);
        group.gem_skills[0].quality = quality;
        if let Some(support) = support {
            group = group.with_gem_skill(support, 1);
        }
        group
    };
    let data = crate::test_data();
    // The real Gemling quality path grants 2% permanent ManaRegen per quality.
    // Interlude III increases the Remnants themselves, not this active buff.
    let mut build = Build::new()
        .add_socket_group(SocketGroup::new().with_gem_skill("SparkPlayer", 20))
        .add_socket_group(remnants(41, Some("SupportInterludePlayerThree")))
        .with_main_socket_group(1);
    build.tree.allocated_nodes.push(NodeId(14429));
    let result = calculate_with_data_session(&build, data, &Default::default()).unwrap();
    let expected = (result.output().mana * 0.04 * 1.82 * 10.0).round() / 10.0;
    assert_eq!(result.output().mana_regen, expected);

    // Exercise both scaling zones with typed global modifiers, retaining real
    // game stats/support eligibility and the ordinary strongest-buff merger.
    let run = |groups: Vec<SocketGroup>, remnant_effect: bool, buff_effect: bool| {
        let mut build = Build::new();
        build.socket_groups = groups;
        let specs = buff_skill_specs(
            &mut StatMapCtx::data(data.stat_map_catalog.as_deref()),
            &build,
            data,
            &[],
            true,
        );
        let mut session = CalculationSession::new(MinimalInput::default()).with_config(
            CalcConfig::spell()
                .with_mode_buffs(true)
                .with_multiplier("PurpleFlamesCount", 3.0),
        );
        session.add_modifiers([Modifier::number("ManaRegen", ModType::Base, 10.0)]);
        if remnant_effect {
            session.add_modifiers([
                Modifier::number("RemnantEffect", ModType::Inc, 60.0),
                Modifier::number("RemnantEffect", ModType::More, 100.0),
            ]);
        }
        if buff_effect {
            session.add_modifiers([
                Modifier::number("BuffEffect", ModType::Inc, 50.0),
                Modifier::number("BuffEffect", ModType::More, 20.0),
            ]);
        }
        for spec in specs {
            session.add_buff_skill(spec);
        }
        session.perform_minimal().unwrap();
        session
    };
    let weak = remnants(41, Some("SupportInterludePlayerThree"));
    let strong = remnants(50, None);
    let cases = [
        (vec![remnants(41, None)], false, false, 82.0, 82),
        (
            vec![remnants(41, Some("SupportInterludePlayer"))],
            false,
            false,
            82.0,
            82,
        ),
        (vec![weak.clone()], false, false, 82.0, 82),
        (vec![remnants(41, None)], true, false, 82.0, 82),
        (vec![weak.clone()], true, true, 147.0, 82),
        (vec![weak.clone(), strong.clone()], false, false, 100.0, 100),
        (vec![strong.clone(), weak], false, false, 100.0, 100),
        (vec![strong.clone(), strong], true, false, 100.0, 100),
    ];
    for (groups, remnant_effect, buff_effect, expected, unscaled) in cases {
        let session = run(groups, remnant_effect, buff_effect);
        let applied: Vec<_> = session
            .all_mods()
            .into_iter()
            .filter(|modifier| {
                modifier
                    .origin
                    .as_ref()
                    .is_some_and(|origin| origin.source_id.id == SOURCE)
            })
            .collect();
        assert_eq!(applied.len(), 1, "keep one strongest, source-bearing copy");
        assert_eq!(applied[0].value.as_number(), Some(expected));
        let origin = applied[0].origin.as_ref().unwrap();
        assert_eq!(origin.source_id.kind, SourceKind::SkillGem);
        assert!(origin.raw_text.as_deref().unwrap().contains(&format!(
            "mana_remnants_global_mana_regeneration_rate_granted_+% ({unscaled})"
        )));
        assert!((session.output().mana_regen - (10.0 + expected / 10.0)).abs() < 1e-9);
    }

    // A Purple Flame really is a Remnant buff. At level 1 its own effect
    // increase is zero: (7 * (1 + .4 + .6 + .5) * 2 * 1.2) = 42 per flame.
    let breach = SocketGroup::new()
        .with_gem_skill("IntoTheBreachPlayer", 1)
        .with_gem_skill("SupportInterludePlayerThree", 1);
    let session = run(vec![breach], true, true);
    let applied: Vec<_> = session
        .all_mods()
        .into_iter()
        .filter(|modifier| {
            modifier.origin.as_ref().is_some_and(|origin| {
                origin.source_id.id == "buff.IntoTheBreachPlayer.breach_flame_chaos_addition_%"
            })
        })
        .collect();
    assert_eq!(applied.len(), 1);
    assert_eq!(applied[0].value.as_number(), Some(42.0));
    assert_eq!(session.base_sum("DamageGainAsChaos"), 126.0);
}
