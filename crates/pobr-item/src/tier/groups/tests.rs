use super::*;
use pobr_data::catalog::stat_descriptions::{DescriptionBranch, StatCondition, StatValueFormat};

const EVASION: &str = "local_base_evasion_rating";
const SHIELD: &str = "local_energy_shield";
const PROJECTILE: &str = "projectile_skill_gem_level_+";
const EVASION_LINE: &str = "+74 to Evasion Rating";
const SHIELD_LINE: &str = "+25 to maximum Energy Shield";

fn modifier(id: &str, group: &str, stats: &[(&str, i64, i64)]) -> ModDef {
    ModDef {
        id: id.into(),
        name: Some(id.into()),
        mod_type: None,
        domain: 1,
        generation_type: Some(1),
        level: 1,
        stats: stats
            .iter()
            .map(|&(id, min, max)| ModStat {
                stat_id: id.into(),
                min,
                max,
            })
            .collect(),
        tags: Vec::new(),
        group: Some(group.into()),
        families: None,
        spawn_weights: vec![
            SpawnWeight {
                tag: "dex_int_armour".into(),
                weight: 1,
            },
            SpawnWeight {
                tag: "default".into(),
                weight: 0,
            },
        ],
    }
}

fn descriptor(stats: &[&str], template: &str) -> StatDescriptor {
    StatDescriptor {
        member_stats: stats.iter().map(|stat| (*stat).into()).collect(),
        branches: vec![DescriptionBranch {
            template: template.into(),
            conditions: vec![StatCondition::default(); stats.len()],
            formats: vec![StatValueFormat::default(); stats.len()],
            lookups: Vec::new(),
        }],
    }
}

fn descriptions() -> StatDescriptionsDef {
    let mut descriptions = StatDescriptionsDef::default();
    let scope = descriptions
        .scopes
        .entry("stat_descriptions".into())
        .or_default();
    for (stat, template) in [
        (EVASION, "{0:+d} to Evasion Rating"),
        (SHIELD, "{0:+d} to maximum Energy Shield"),
        (PROJECTILE, "{0:+d} to Level of all Projectile Skills"),
        ("additional_strength", "{0:+d} to Strength"),
    ] {
        scope
            .descriptors
            .insert(stat.into(), descriptor(&[stat], template));
    }
    descriptions
}

fn hybrid_pool() -> Vec<ModDef> {
    [
        (6, 10, 5, 8),
        (11, 41, 9, 15),
        (42, 64, 16, 21),
        (65, 78, 22, 25),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (ev_min, ev_max, es_min, es_max))| {
        let mut modifier = modifier(
            &format!("Hybrid{}", index + 1),
            "Hybrid",
            &[(EVASION, ev_min, ev_max), (SHIELD, es_min, es_max)],
        );
        modifier.level = index as u32 + 1;
        modifier
    })
    .collect()
}

fn tags() -> Vec<String> {
    vec!["dex_int_armour".into(), "gloves".into(), "default".into()]
}

#[test]
fn complete_hybrid_shares_one_native_mod_and_tier_in_either_line_order() {
    let index = TierIndex::build(&hybrid_pool(), &descriptions());
    for lines in [[EVASION_LINE, SHIELD_LINE], [SHIELD_LINE, EVASION_LINE]] {
        let results = index.lookup_lines(&lines, &tags(), 1);
        let info = results[0].as_ref().unwrap();
        assert_eq!(info.mod_id, "Hybrid4");
        assert_eq!((info.tier, info.total), (1, 4));
        assert!(info.is_prefix);
        assert_eq!(results[0], results[1]);
    }
    assert!(index.lookup(EVASION_LINE, &tags(), 1).is_none());
}

#[test]
fn missing_out_of_range_or_hidden_stats_never_match_a_subset() {
    let index = TierIndex::build(&hybrid_pool(), &descriptions());
    for lines in [
        vec![EVASION_LINE],
        vec![SHIELD_LINE],
        vec![EVASION_LINE, "+26 to maximum Energy Shield"],
        vec!["+79 to Evasion Rating", SHIELD_LINE],
        vec![EVASION_LINE, "+21 to maximum Energy Shield"],
    ] {
        assert_eq!(
            index.lookup_lines(&lines, &tags(), 1),
            vec![None; lines.len()]
        );
    }
    let mut mods = hybrid_pool();
    for modifier in &mut mods {
        modifier.stats.push(ModStat {
            stat_id: "additional_strength".into(),
            min: 10,
            max: 10,
        });
    }
    assert_eq!(
        TierIndex::build(&mods, &descriptions()).lookup_lines(
            &[EVASION_LINE, SHIELD_LINE],
            &tags(),
            1
        ),
        vec![None, None]
    );
}

#[test]
fn full_multiline_descriptor_binds_all_captures_together() {
    let mut descriptions = descriptions();
    let scope = descriptions.scopes.get_mut("stat_descriptions").unwrap();
    scope.descriptors.remove(EVASION);
    scope.descriptors.remove(SHIELD);
    scope.descriptors.insert(
        EVASION.into(),
        descriptor(
            &[EVASION, SHIELD],
            "{0:+d} to Evasion Rating\n{1:+d} to maximum Energy Shield\nEvasion roll: {0}",
        ),
    );
    let index = TierIndex::build(&hybrid_pool(), &descriptions);
    let lines = [SHIELD_LINE, "Evasion roll: 74", EVASION_LINE];
    let results = index.lookup_lines(&lines, &tags(), 1);
    assert!(
        results
            .iter()
            .all(|info| info.as_ref().unwrap().mod_id == "Hybrid4")
    );
    assert_eq!(
        index.lookup_lines(&[EVASION_LINE, SHIELD_LINE], &tags(), 1),
        vec![None, None]
    );
    assert_eq!(
        index.lookup_lines(&[EVASION_LINE, SHIELD_LINE, "Evasion roll: 73"], &tags(), 1),
        vec![None, None, None]
    );
}

#[test]
fn independent_native_affixes_compete_with_the_complete_hybrid() {
    let mut mods = hybrid_pool();
    mods.push(modifier("FlatEvasion", "FlatEvasion", &[(EVASION, 65, 78)]));
    mods.push(modifier("FlatShield", "FlatShield", &[(SHIELD, 22, 25)]));
    let lines = [EVASION_LINE, SHIELD_LINE];
    assert_eq!(
        TierIndex::build(&mods, &descriptions()).lookup_lines(&lines, &tags(), 1),
        vec![None, None]
    );
    let separate = TierIndex::build(&mods[4..], &descriptions()).lookup_lines(&lines, &tags(), 1);
    assert_eq!(separate[0].as_ref().unwrap().mod_id, "FlatEvasion");
    assert_eq!(separate[1].as_ref().unwrap().mod_id, "FlatShield");
}

#[test]
fn duplicates_competing_mods_and_repeated_groups_remain_unknown() {
    let pool = hybrid_pool();
    let index = TierIndex::build(&pool, &descriptions());
    assert_eq!(
        index.lookup_lines(&[EVASION_LINE, SHIELD_LINE, EVASION_LINE], &tags(), 1),
        vec![None, None, None]
    );
    assert_eq!(
        index.lookup_lines(
            &[
                "+6 to Evasion Rating",
                "+5 to maximum Energy Shield",
                EVASION_LINE,
                SHIELD_LINE
            ],
            &tags(),
            1
        ),
        vec![None, None, None, None]
    );
    let mut mods = pool;
    let mut other = mods[3].clone();
    other.id = "OtherHybrid".into();
    other.group = Some("OtherGroup".into());
    mods.push(other);
    assert_eq!(
        TierIndex::build(&mods, &descriptions()).lookup_lines(
            &[EVASION_LINE, SHIELD_LINE],
            &tags(),
            1
        ),
        vec![None, None]
    );
}

#[test]
fn unrelated_unknown_lines_preserve_order_and_do_not_hide_complete_groups() {
    let mut mods = hybrid_pool();
    mods.push(modifier(
        "Strength",
        "Strength",
        &[("additional_strength", 10, 10)],
    ));
    let index = TierIndex::build(&mods, &descriptions());
    let results = index.lookup_lines(
        &["unknown", SHIELD_LINE, "+10 to Strength", EVASION_LINE],
        &tags(),
        1,
    );
    assert!(results[0].is_none());
    assert_eq!(results[1], results[3]);
    assert_eq!(results[1].as_ref().unwrap().mod_id, "Hybrid4");
    assert_eq!(results[2].as_ref().unwrap().mod_id, "Strength");
    assert_eq!(
        index.lookup_lines(&["+10 to Strength", "+10 to Strength"], &tags(), 1),
        vec![None, None]
    );
}

fn special_pool() -> Vec<ModDef> {
    [1, 2]
        .into_iter()
        .map(|level| {
            let mut modifier = modifier(
                &format!("Special{level}"),
                "ProjectileLevel",
                &[(PROJECTILE, level, level)],
            );
            modifier.generation_type = Some(2);
            modifier.level = level as u32;
            modifier.spawn_weights[0].tag = "marksman".into();
            modifier
        })
        .collect()
}

#[test]
fn allowed_native_ids_extend_only_the_display_pool_without_changing_tags() {
    let index = TierIndex::build(&special_pool(), &descriptions());
    let lines = ["+2 to Level of all Projectile Skills"];
    let allowed = BTreeSet::from(["Special1".into(), "Special2".into()]);
    let results = index.lookup_lines_with_allowed(&lines, &tags(), 1, &allowed);
    let info = results[0].as_ref().unwrap();
    assert_eq!(info.mod_id, "Special2");
    assert_eq!((info.tier, info.total), (1, 2));
    assert!(!info.is_prefix);
    assert!(index.lookup(lines[0], &tags(), 1).is_none());
    assert_eq!(index.lookup_lines(&lines, &tags(), 1), vec![None]);
    for allowed in [
        BTreeSet::new(),
        BTreeSet::from(["influence:Special2".into()]),
        BTreeSet::from(["unrelated".into()]),
    ] {
        assert_eq!(
            index.lookup_lines_with_allowed(&lines, &tags(), 1, &allowed),
            vec![None]
        );
    }
    assert_eq!(
        index.lookup_lines_with_allowed(&lines, &tags(), 2, &allowed),
        vec![None]
    );
}

#[test]
fn allowed_ids_cannot_admit_other_generation_types_or_missing_mods() {
    let mut mods = special_pool();
    mods[1].generation_type = Some(3);
    let allowed = BTreeSet::from(["Special2".into()]);
    let index = TierIndex::build(&mods, &descriptions());
    assert_eq!(
        index.lookup_lines_with_allowed(
            &["+2 to Level of all Projectile Skills"],
            &tags(),
            1,
            &allowed
        ),
        vec![None]
    );
    mods[1].generation_type = Some(2);
    mods[1].spawn_weights.clear();
    let index = TierIndex::build(&mods, &descriptions());
    let results = index.lookup_lines_with_allowed(
        &["+2 to Level of all Projectile Skills"],
        &tags(),
        1,
        &allowed,
    );
    assert_eq!(results[0].as_ref().unwrap().mod_id, "Special2");
    assert_eq!(
        index.lookup_lines(&["+2 to Level of all Projectile Skills"], &tags(), 1),
        vec![None]
    );
}

#[test]
fn ordinary_and_allowed_candidates_with_the_same_text_never_choose_a_preference() {
    let mut mods = special_pool();
    mods.push(modifier(
        "Ordinary",
        "ProjectileLevel",
        &[(PROJECTILE, 2, 2)],
    ));
    let index = TierIndex::build(&mods, &descriptions());
    assert_eq!(
        index.lookup_lines_with_allowed(
            &["+2 to Level of all Projectile Skills"],
            &tags(),
            1,
            &BTreeSet::from(["Special2".into()])
        ),
        vec![None]
    );
}

#[test]
fn stronger_allowed_affixes_never_demote_natural_tiers_or_enter_the_natural_pool() {
    let mut mods = special_pool();
    for modifier in &mut mods {
        modifier.stats[0].min += 2;
        modifier.stats[0].max += 2;
        modifier.level += 10;
    }
    let mut ordinary = modifier("Ordinary", "ProjectileLevel", &[(PROJECTILE, 2, 2)]);
    ordinary.generation_type = Some(2);
    mods.push(ordinary);
    let index = TierIndex::build(&mods, &descriptions());
    let allowed = BTreeSet::from(["Ordinary".into(), "Special1".into(), "Special2".into()]);
    let ordinary = index.lookup_lines_with_allowed(
        &["+2 to Level of all Projectile Skills"],
        &tags(),
        1,
        &allowed,
    );
    let ordinary = ordinary[0].as_ref().unwrap();
    assert_eq!(ordinary.mod_id, "Ordinary");
    assert_eq!((ordinary.tier, ordinary.total), (1, 1));
    let special = index.lookup_lines_with_allowed(
        &["+3 to Level of all Projectile Skills"],
        &tags(),
        1,
        &allowed,
    );
    let special = special[0].as_ref().unwrap();
    assert_eq!(special.mod_id, "Special1");
    assert_eq!((special.tier, special.total), (2, 2));
    assert_eq!(
        index.lookup_lines(&["+3 to Level of all Projectile Skills"], &tags(), 1),
        vec![None]
    );
}

#[test]
fn no_mods_no_descriptions_and_empty_inputs_degrade_without_data() {
    let lines = [EVASION_LINE, SHIELD_LINE];
    let allowed = BTreeSet::from(["Hybrid4".into()]);
    for index in [
        TierIndex::default(),
        TierIndex::build(&[], &descriptions()),
        TierIndex::build(&hybrid_pool(), &StatDescriptionsDef::default()),
    ] {
        assert_eq!(
            index.lookup_lines_with_allowed(&lines, &tags(), 1, &allowed),
            vec![None, None]
        );
        assert!(index.lookup_lines(&[], &tags(), 1).is_empty());
    }
}
