//! End-to-end integration tests for the minion pipeline (/B/C).
//!
//! Coverage: the BuildData minion query API (A5); the orchestrator recognizing
//! a summon gem -> `OutputTable.minions` non-empty (B2); createMinionSkills +
//! feeding the main skill into offence (C1/C2); modDB assembly completeness
//! (C3). Golden values come from PoB2's panel with matching parameters (oracle
//! intermediate values noted where relevant).

use pobr_build::{
    Build, BuildData, CharacterIdentity, DataOrchestratorOptions, SocketGroup, calculate_with_data,
};
use pobr_core::calc::build_minion_context_from_def;
use pobr_core::calc::minion::{
    AttributeInfusion, minion_level_from_gem_level, resolve_minion_level,
};
use pobr_gamedata::repo_data_root;

// Minion reference values (life/damage/DPS, from Minions.lua/Spectres.lua + the
// PoB2 panel) are checked against the active data version.
fn version() -> String {
    pobr_gamedata::data_version()
}

use crate::test_data as load_data;

fn default_opts() -> DataOrchestratorOptions {
    DataOrchestratorOptions {
        inject_character_base: true,
        ..Default::default()
    }
}

/// A Witch build with only a Raise Zombie (summon gem level N) active skill.
fn zombie_build(gem_level: u32) -> Build {
    Build::new()
        .with_character(CharacterIdentity {
            level: 90,
            class_name: "Witch".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(
            SocketGroup::new()
                .with_slot("weapon1")
                .with_gem("Metadata/Items/Gems/SkillGemRaiseZombie")
                .with_gem_skill("RaiseZombiePlayer", gem_level),
        )
}

// A5: BuildData minion query API

#[test]
fn build_data_minion_def_zombie() {
    let data = load_data();
    let zombie = data
        .minion_def("RaisedZombie")
        .expect("RaisedZombie is in the data pack");
    // Values come from Minions.lua and drift with balance patches -> blessed snapshot (refresh with POBR_BLESS_PINS=1).
    pobr_gamedata::test_pins::assert_pin(
        &repo_data_root().join(version()),
        "minions.raised_zombie",
        serde_json::json!({ "life": zombie.life, "damage": zombie.damage }),
    );
    assert!(zombie.base_damage_ignores_attack_speed);
}

#[test]
fn build_data_minion_def_spectre_falls_back() {
    let data = load_data();
    // spectre key = full metadata path (minions.json miss -> falls through to spectres.json)
    let c = data
        .minion_def("Metadata/Monsters/LeagueAbyss/Lightless/Cocoon3Spectre")
        .expect("Lightless Abomination is in the data pack (falls into spectres)");
    // Values come from Spectres.lua and drift with balance patches (0.5.4b: life 3.0->2.2) -> blessed snapshot.
    pobr_gamedata::test_pins::assert_pin(
        &repo_data_root().join(version()),
        "minions.spectre_cocoon3",
        serde_json::json!({ "life": c.life, "armour": c.armour }),
    );
}

#[test]
fn build_data_effect_minion_list() {
    let data = load_data();
    assert_eq!(
        data.effect_minion_list("RaiseZombiePlayer"),
        ["RaisedZombie"]
    );
    assert_eq!(
        data.effect_minion_list("RagingSpiritsPlayer"),
        ["SummonedRagingSpirit"]
    );
    // Non-summon skill -> empty slice
    assert!(data.effect_minion_list("FireballPlayer").is_empty());
    assert!(data.effect_minion_list("NonexistentSkill").is_empty());
}

// B2: orchestrator recognizes a summon gem -> OutputTable.minions non-empty

#[test]
fn summon_build_populates_output_minions() {
    let data = load_data();
    let out = calculate_with_data(&zombie_build(20), &data, &default_opts()).expect("calculate");
    // A summon gem is recognized -> OutputTable.minions non-empty (one of the G4 closure criteria).
    assert!(
        !out.minions.is_empty(),
        "a Raise Zombie build should produce a minion snapshot"
    );
    let zombie = &out.minions[0];
    // Minion level = minionLevelTable[gem_level=20] = 40 (CalcActiveSkill.lua:896 default rule).
    assert_eq!(zombie.level, minion_level_from_gem_level(20));
    assert_eq!(zombie.level, 40);
    // Life > 0 (derived from the monster-level base table × the 0.7 normalization multiplier).
    assert!(
        zombie.life > 0.0,
        "minion life should be > 0, measured {}",
        zombie.life
    );
}

#[test]
fn non_summon_build_has_empty_minions() {
    let data = load_data();
    // Non-summon main skill (Fireball) -> minions must be empty (gate correctness: zero behavioural leakage).
    let build = Build::new()
        .with_character(CharacterIdentity {
            level: 90,
            class_name: "Witch".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(
            SocketGroup::new()
                .with_slot("weapon1")
                .with_gem("Metadata/Items/Gems/SkillGemFireball")
                .with_gem_skill("FireballPlayer", 20),
        );
    let out = calculate_with_data(&build, &data, &default_opts()).expect("calculate");
    assert!(
        out.minions.is_empty(),
        "a non-summon build's minions should be empty"
    );
}

/// Pounce's third effect summons one wolf plus the rarity bonus from its
/// game stat set: Rare +0, Unique +1. Its level-20 limit of six wolves is
/// not the number created by one activation.
#[test]
fn pounce_wolves_exist_only_for_the_selected_summon_effect() {
    let data = load_data();
    let make_build = |main_group, effect, enemy: &str| {
        pobr_build::parse_build(&format!(
            r#"<PathOfBuilding2><Build level="90" className="Witch" mainSocketGroup="{main_group}"/>
<Skills>
<Skill enabled="false"><Gem skillId="FireballPlayer" level="20" enabled="true"/></Skill>
<Skill enabled="true" mainActiveSkill="{effect}"><Gem skillId="WolfPouncePlayer" level="20" enabled="true" skillMinion="WolfMinion"/></Skill>
<Skill enabled="true" mainActiveSkill="3"><Gem skillId="WolfPouncePlayer" level="20" enabled="true" skillMinion="WolfMinion"/></Skill>
<Skill enabled="true"><Gem skillId="FireballPlayer" level="20" enabled="true"/></Skill>
</Skills><Config>{enemy}</Config></PathOfBuilding2>"#
        ))
        .unwrap()
    };
    let options = DataOrchestratorOptions::reference_conditions();
    for (enemy, count) in [
        (r#"<Input name="enemyIsBoss" string="None"/>"#, 1.0),
        (
            r#"<Input name="enemyIsBoss" string="None"/><Input name="conditionEnemyRareOrUnique" boolean="true"/>"#,
            1.0,
        ),
        (r#"<Input name="enemyIsBoss" string="Pinnacle"/>"#, 2.0),
    ] {
        let build = make_build(2, 3, enemy);
        let session = pobr_build::calculate_with_data_session(&build, &data, &options).unwrap();
        let output = session.output();
        assert_eq!(output.minions.len(), 1, "only the selected group summons");
        let wolf = pobr_build::reference_check::main_skill_minion(&build, &data, output)
            .expect("the additional summon effect has a main minion");
        assert_eq!(wolf.minion_id, "WolfMinion");
        assert_eq!(wolf.summoned_by, "WolfPounceSummonWolfPlayer");
        assert_eq!(wolf.skill_id, "MeleeAtAnimationSpeedWolfCompanion");
        assert_eq!(wolf.summoner, Some((0, 0)));
        assert_eq!(wolf.level, 40);
        assert!(wolf.dps > 0.0);
        assert_eq!(output.dps, 0.0);
        assert_eq!(session.config().multiplier("SummonedMinion"), count);
        assert_eq!(session.config().multiplier("MinionPresenceCount"), count);
        assert_eq!(output.active_minion_limit, count);
    }
    for (main_group, effect) in [(2, 1), (2, 2), (4, 3)] {
        let build = make_build(main_group, effect, "");
        let session = pobr_build::calculate_with_data_session(&build, &data, &options).unwrap();
        assert!(session.output().minions.is_empty());
        assert_eq!(session.config().multiplier("SummonedMinion"), 0.0);
        assert_eq!(session.config().multiplier("MinionPresenceCount"), 0.0);
    }
    let build = make_build(2, 3, "");
    let boosted = calculate_with_data(
        &build,
        &data,
        &DataOrchestratorOptions {
            extra_modifier_texts: vec!["+2 to Level of all Minion Skills".into()],
            ..options
        },
    )
    .unwrap();
    assert_eq!(boosted.minions[0].level, 44, "the parent gem gains levels");

    // Selecting the child effect must preserve the prepared alternate-set
    // bonus, replacing the current context's +5. Gem 20 + 2 gives monster 44.
    let mut other_set = build;
    other_set.socket_groups[1].gem_skills[0].global_level_bonus = Some(2);
    other_set.config.global_modifier_texts = vec!["+5 to Level of all Minion Skills".into()];
    let output = calculate_with_data(
        &other_set,
        &data,
        &DataOrchestratorOptions::reference_conditions(),
    )
    .unwrap();
    assert_eq!(
        output.minions[0].level, 44,
        "use the parent's prepared bonus"
    );
}

/// Game MonsterResistances: Skeletal Arsonist has 50 Fire, 0 Cold,
/// Lightning and Chaos. The player's campaign penalty never transfers.
#[test]
fn minion_resistances_use_monster_base_modifiers_and_caps() {
    let data = load_data();
    // MonsterTypes can link several resistance records. The official
    // table preserves a vulnerability which the old overlay dropped.
    let golem = data
        .minion_def("Metadata/Monsters/MudGolem/MudGolem")
        .unwrap();
    assert_eq!(
        (golem.fire_resist, golem.cold_resist, golem.lightning_resist),
        (30.0, 30.0, -30.0)
    );
    assert_eq!(
        data.minion_def("RaisedSkeletonArsonist")
            .unwrap()
            .fire_resist,
        50.0
    );
    let build = Build::new()
        .with_character(CharacterIdentity {
            level: 90,
            class_name: "Witch".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(
            SocketGroup::new().with_active_skill("SummonSkeletalArsonistsPlayer", 20),
        );
    let mut options = default_opts();
    options.extra_modifier_texts = vec![
        "Minions have +40% to all Elemental Resistances".into(),
        "Minions have +18% to Chaos Resistance".into(),
    ];
    let session = pobr_build::calculate_with_data_session(&build, &data, &options).unwrap();
    let minion = &session.output().minions[0];
    // 50 + 40 = 90, capped at 75, with 15 over-cap; other elements get 40.
    for (id, expected) in [
        ("FireResist", 75.0),
        ("FireResistMax", 75.0),
        ("FireResistOverCap", 15.0),
        ("ColdResist", 40.0),
        ("LightningResist", 40.0),
        ("ChaosResist", 18.0),
        ("ChaosResistMax", 75.0),
    ] {
        assert_eq!(minion.stats[id], expected, "{id}");
    }
    let detail = pobr_build::breakdown::minion_stat_breakdown(&session, 0, "FireResist").unwrap();
    assert_eq!(detail.formula.unwrap().value, 75.0);
    options
        .extra_modifier_texts
        .push("Minions have +5% to all maximum Elemental Resistances".into());
    let raised = calculate_with_data(&build, &data, &options).unwrap();
    assert_eq!(raised.minions[0].stats["FireResist"], 80.0);
    assert_eq!(raised.minions[0].stats["FireResistMax"], 80.0);
    assert_eq!(raised.minions[0].stats["FireResistOverCap"], 10.0);

    // Elemental Army's game stat grants the supported minion +30% to
    // elemental resistances; an ally aura adds another 12%.
    let supported = Build::new().add_socket_group(
        SocketGroup::new()
            .with_gem_skill("SummonSkeletalArsonistsPlayer", 20)
            .with_gem_skill("SupportElementalArmyPlayer", 1),
    );
    let mut aura = default_opts();
    aura.extra_modifier_texts
        .push("Allies in your Presence have +12% to all Elemental Resistances".into());
    let output = calculate_with_data(&supported, &data, &aura).unwrap();
    assert_eq!(output.minions[0].stats["ColdResist"], 42.0);
    assert_eq!(output.minions[0].stats["FireResist"], 75.0);
    assert_eq!(output.minions[0].stats["FireResistOverCap"], 17.0);
}

/// Minion life = monster-ally life table(level) × the life normalization
/// multiplier (0.7) — matches core's `build_minion_context_from_def`
/// derivation (the B2 wiring introduces no extra scaling).
#[test]
fn summon_zombie_life_matches_core_derivation() {
    let data = load_data();
    let out = calculate_with_data(&zombie_build(20), &data, &default_opts()).expect("calculate");
    let zombie_out = &out.minions[0];

    // Derives the same minion directly via core's pure function and compares against the life it produces after the defence pipeline.
    let def = data
        .minion_def("RaisedZombie")
        .expect("RaisedZombie is in the data pack");
    let ctx = build_minion_context_from_def(def, 20, vec![], vec![], AttributeInfusion::default());
    let core_life = ctx.base.life;
    assert_eq!(zombie_out.level, resolve_minion_level(20));
    // OutputTable.minions' life is emitted by the defence pipeline (life pool) and shares the same order as base.life;
    // B2 doesn't add any minion mod, so they should match.
    assert!(
        (zombie_out.life - core_life).abs() < 1.0,
        "orchestrator minion life {} should ≈ core-derived {}",
        zombie_out.life,
        core_life
    );
}

/// Storm Mage quality grants a percentage of its own maximum Life as extra
/// ES. The retained handler uses PercentStat (ceil the flat contribution),
/// then the ordinary ES factors, without deducting any Life.
#[test]
fn storm_mage_quality_gains_es_from_its_own_life() {
    let data = load_data();
    const SUMMON: &str = "SummonSkeletalStormMagesPlayer";
    const STAT: &str = "minion_maximum_life_%_to_gain_as_maximum_energy_shield";
    let quality_stat = data.gem_quality_stats[SUMMON]
        .iter()
        .find(|stat| stat.stat == STAT)
        .unwrap();
    assert_eq!(quality_stat.per_quality_rate, 1.0);

    for (life_inc, gigantic, es_inc) in [(0, false, 0), (213, false, 0), (213, true, 50)] {
        let mut options = default_opts();
        options.extra_modifier_texts = vec![
            "+12345 to maximum Life".into(),
            "+987 to maximum Energy Shield".into(),
            format!("Minions have {life_inc}% increased maximum Life"),
            format!("Minions have {es_inc}% increased maximum Energy Shield"),
        ];
        if gigantic {
            options
                .extra_modifier_texts
                .push("Your Minions are Gigantic".into());
        }
        let calculate = |quality| {
            let build = Build::new()
                .with_character(CharacterIdentity {
                    level: 90,
                    class_name: "Witch".into(),
                    ascendancy_name: String::new(),
                })
                .add_socket_group(SocketGroup::new().with_gem_skill_quality(SUMMON, 38, quality))
                .add_socket_group(SocketGroup::new().with_gem_skill("RaiseZombiePlayer", 20));
            pobr_build::calculate_with_data_session(&build, &data, &options).unwrap()
        };
        let baseline = calculate(0);
        let expected_life =
            4729.0 * (1.0 + f64::from(life_inc) / 100.0) * if gigantic { 1.2 } else { 1.0 };
        for quality in [35, 0, 1, 100] {
            let session = calculate(quality);
            let output = session.output();
            let (index, mage) = output
                .minions
                .iter()
                .enumerate()
                .find(|(_, minion)| minion.summoned_by == SUMMON)
                .unwrap();
            assert_eq!(mage.level, 76);
            assert!((mage.life - expected_life).abs() < 1e-6);
            let gained = (mage.life * f64::from(quality) / 100.0).ceil();
            // Level 76: floor(8924 × 0.53) = 4729 Life; the existing
            // intrinsic ES is 4729 × 0.15 = 709.35, rounded after ES scaling.
            let expected_es = ((709.35 + gained) * (1.0 + f64::from(es_inc) / 100.0)).round();
            assert_eq!(mage.energy_shield, expected_es, "quality {quality}");
            assert_eq!(output.life, baseline.output().life);
            assert_eq!(output.energy_shield, baseline.output().energy_shield);
            let zombie = output
                .minions
                .iter()
                .find(|minion| minion.summoned_by == "RaiseZombiePlayer")
                .unwrap();
            let baseline_zombie = baseline
                .output()
                .minions
                .iter()
                .find(|minion| minion.summoned_by == "RaiseZombiePlayer")
                .unwrap();
            assert_eq!(zombie.life, baseline_zombie.life);
            assert_eq!(zombie.energy_shield, baseline_zombie.energy_shield);
            assert_eq!(zombie.dps, baseline_zombie.dps);

            let life_detail =
                pobr_build::breakdown::minion_stat_breakdown(&session, index, "Life").unwrap();
            assert_eq!(life_detail.formula.unwrap().value, mage.life);
            let detail =
                pobr_build::breakdown::minion_stat_breakdown(&session, index, "EnergyShield")
                    .unwrap();
            assert_eq!(detail.formula.unwrap().value, mage.energy_shield);
            assert_eq!(detail.base_total, 709.35 + gained);
            assert_eq!(detail.inc_total, f64::from(es_inc));
            let origin = format!("minion.{SUMMON}.{STAT}");
            let contribution = detail
                .mods
                .iter()
                .find(|row| row.origin_id.as_deref() == Some(&origin));
            if quality == 0 {
                assert!(contribution.is_none());
            } else {
                let contribution =
                    contribution.expect("quality is attributed in the selected minion's ES");
                assert_eq!(contribution.value, Some(gained));
                assert!(contribution.active);
                assert_eq!(contribution.origin_kind.as_deref(), Some("SkillGem"));
            }
        }
    }
}

/// The Life snapshot used by ES and details is also the one offence sees.
/// A synthetic Life-scaled damage line detects an inherited player snapshot.
#[test]
fn storm_mage_quality_life_snapshot_matches_offence_details() {
    use pobr_core::calc::{CalculationSession, MinimalInput, MinionModifierEntry, MinionOrigin};
    use pobr_core::{ModTag, Modifier};
    use pobr_data::modifier::ModType;

    let data = load_data();
    let calculate = |with_life_damage| {
        let mut session = CalculationSession::new(MinimalInput {
            base_life: 1000.0,
            ..Default::default()
        });
        session.set_stat("Life", 1000.0);
        let mut modifiers = vec![Modifier::number("MaximumLife", ModType::Inc, 213.0)];
        if with_life_damage {
            modifiers.push(Modifier::number("Damage", ModType::More, 1.0).with_tag(
                ModTag::PercentStat {
                    stat: "Life".into(),
                    percent: Some(1.0),
                },
            ));
        }
        session.add_minion_from_def(
            data.minion_def("RaisedSkeletonStormMage").unwrap(),
            38,
            1,
            modifiers
                .into_iter()
                .map(|inner| MinionModifierEntry {
                    inner,
                    minion_type: None,
                })
                .collect(),
            vec![],
            AttributeInfusion::default(),
            MinionOrigin::default(),
        );
        session.finish().unwrap()
    };
    let baseline = calculate(false);
    let scaled = calculate(true);
    let minion = &scaled.output().minions[0];
    assert_eq!(minion.life, 14801.77);
    assert!(baseline.output().minions[0].dps > 0.0);
    let ratio = minion.dps / baseline.output().minions[0].dps;
    assert!(
        (ratio - 2.49).abs() < 1e-6,
        "ceil(1% of 14801.77) = 149% more: {ratio}"
    );
    let detail = pobr_build::breakdown::minion_stat_breakdown(&scaled, 0, "TotalDPS").unwrap();
    assert_eq!(detail.formula.unwrap().value, minion.dps);
}

// B3: MinionModifier channel -- `Minions deal X% increased Damage` reaches the minion ModDb

/// Player gear with `Minions deal 50% increased Damage` (attributed to the
/// item) -> minion DPS increases; the player itself is unaffected (a minion
/// mod is Unsupported in the player's main pipeline, but reaches the minion
/// ModDb).
#[test]
fn minion_increased_damage_raises_minion_dps() {
    use pobr_data::item::{EquipmentSlot, Item, ItemBaseId, ItemRarity, RolledDefence};

    let data = load_data();

    let base = calculate_with_data(&zombie_build(20), &data, &default_opts()).expect("base calc");
    let base_dps = base.minions[0].dps;
    assert!(base_dps > 0.0, "baseline minion DPS should be > 0");

    // Ring carries a minion mod (attributed via collect_item_texts -> parse_minion_modifier).
    let ring = Item {
        base: ItemBaseId::from("Ring"),
        rarity: ItemRarity::Rare,
        quality: 0,
        corrupted: false,
        implicit_texts: vec![],
        modifier_texts: vec!["Minions deal 50% increased Damage".into()],
        enchant_texts: vec![],
        rolled_defence: RolledDefence::default(),
        socketed: Vec::new(),
        parsed_stats: vec![],
    };
    let buffed_build = zombie_build(20).set_item(EquipmentSlot::Ring1, ring);
    let buffed = calculate_with_data(&buffed_build, &data, &default_opts()).expect("buffed calc");
    let buffed_dps = buffed.minions[0].dps;

    // Minion DPS should increase with +50% increased Damage (the exact multiplier depends on other stacked inc mods).
    assert!(
        buffed_dps > base_dps,
        "minion DPS should grow with Minions deal 50% increased Damage: base {base_dps} → buffed {buffed_dps}"
    );
    // Player's own DPS isn't affected by the minion mod (a minion mod doesn't feed the player's aggregation).
    assert_eq!(
        buffed.dps, base.dps,
        "the player's own DPS should not be changed by a minion mod"
    );
}

#[test]
fn minion_limit_reads_completed_player_sources() {
    let data = load_data();
    let mut options = default_opts();
    for (line, named_bonus, expected) in [
        (None, 0.0, 10.0),
        (Some("+3 to maximum number of Raised Zombies"), 3.0, 13.0),
        (
            Some("Temporary Minion Skills have +5 to Limit of Minions summoned"),
            3.0,
            18.0,
        ),
    ] {
        options.extra_modifier_texts.extend(line.map(str::to_owned));
        let session =
            pobr_build::calculate_with_data_session(&zombie_build(20), &data, &options).unwrap();
        // The native cap is read from the originating skill; the player's
        // named stat supplies only the additional limit, not a replacement.
        assert_eq!(session.base_sum("ActiveZombieLimit"), named_bonus);
        assert_eq!(session.output().active_minion_limit, expected);
        assert_eq!(session.base_sum("Multiplier:SummonedMinion"), expected);
        assert_eq!(session.base_sum("Multiplier:MinionPresenceCount"), expected);
    }

    // These zombie/temporary bonuses do not change a persistent wolf pack.
    let wolves =
        Build::new().add_socket_group(SocketGroup::new().with_gem_skill("WolfPackPlayer", 20));
    let session = pobr_build::calculate_with_data_session(&wolves, &data, &options).unwrap();
    assert_eq!(session.output().active_minion_limit, 6.0);
    assert_eq!(session.base_sum("Multiplier:SummonedMinion"), 6.0);
}

/// Cackling Companions: "Summons {0} Hyenas", seven at gem level 7. The
/// character has that many minions, though nothing raises the Hyenas' own
/// limit.
#[test]
fn cackling_companions_creates_as_many_hyenas_as_it_says() {
    let data = load_data();
    let build = Build::new()
        .with_character(CharacterIdentity {
            level: 90,
            class_name: "Witch".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(
            SocketGroup::new()
                .with_slot("weapon1")
                .with_gem_skill("FireballPlayer", 20),
        )
        .add_socket_group(SocketGroup::new().with_gem_skill("HyenaCacklePlayer", 7));
    let options = default_opts();
    let session = pobr_build::calculate_with_data_session(&build, &data, &options).unwrap();
    assert_eq!(session.base_sum("Multiplier:SummonedMinion"), 7.0);
    assert_eq!(session.base_sum("Multiplier:MinionPresenceCount"), 7.0);
}

/// The zombie's Life with `lines` among the character's modifiers.
fn zombie_life_with(data: &BuildData, lines: &[&str]) -> f64 {
    let mut options = default_opts();
    options
        .extra_modifier_texts
        .extend(lines.iter().map(|line| line.to_string()));
    let out = calculate_with_data(&zombie_build(20), data, &options).expect("calculate");
    out.minions[0].life
}

/// What the character grants minions reaches them from any source, not only
/// from item text, and a line for Companions only reaches Companions.
#[test]
fn the_characters_minion_modifiers_reach_its_minions() {
    let data = load_data();
    let base = zombie_life_with(&data, &[]);
    let more = zombie_life_with(&data, &["Minions have 50% increased maximum Life"]);
    assert!((more / base - 1.5).abs() < 1e-3, "{base} -> {more}");
    let companions = zombie_life_with(&data, &["Companions have 50% increased maximum Life"]);
    assert_eq!(companions, base, "a zombie is no Companion");
    // "Your Minions are Gigantic": 20% more Life.
    let gigantic = zombie_life_with(&data, &["Your Minions are Gigantic"]);
    assert!((gigantic / base - 1.2).abs() < 1e-3, "{base} -> {gigantic}");
}

/// A jewel socket's effect scales the jewel's minion payload, including
/// damage, speed and critical bonuses, just as it scales player modifiers.
#[test]
fn adorned_scales_nested_companion_modifiers() {
    use pobr_build::build::JewelSocket;
    use pobr_data::item::ItemRarity;

    let data = load_data();
    let mut group = SocketGroup::new().with_gem_skill("SummonBeastPlayer", 20);
    group.gem_skills[0].minion = Some("Metadata/Monsters/Quadrilla/QuadrillaBossMinion2".into());
    let base = Build::new()
        .with_character(CharacterIdentity {
            level: 90,
            class_name: "Huntress".into(),
            ascendancy_name: String::new(),
        })
        .add_socket_group(group);
    let jewel = pobr_core::parse_pob_xml_item(
        "Rarity: RARE\nFixture\nSapphire\nImplicits: 0\nMinions deal 13% increased Damage\nMinions have 6% increased Attack and Cast Speed\nMinions have 33% increased Critical Hit Chance\nMinions have 23% increased Critical Damage Bonus\n+100 to maximum Life\nCorrupted",
    ).unwrap();
    let run = |named: &str, effect: u32, rarity, corrupted, socket| {
        let adorned = pobr_core::parse_pob_xml_item(&format!(
            "Rarity: UNIQUE\nThe Adorned\nDiamond\nImplicits: 0\n{effect}% increased Effect of Jewel Socket Passive Skills\ncontaining Corrupted {named} Jewels"
        )).unwrap();
        let mut jewel = jewel.clone();
        jewel.rarity = rarity;
        jewel.corrupted = corrupted;
        let mut build = base.clone().with_jewels(vec![adorned]);
        build.jewels.push((socket, jewel));
        pobr_build::calculate_with_data_session(&build, &data, &default_opts()).unwrap()
    };
    let plain = run("Rare", 0, ItemRarity::Rare, true, JewelSocket::Tree);
    for (named, effect, rarity, corrupted, socket, expected) in [
        (
            "Rare",
            69,
            ItemRarity::Rare,
            true,
            JewelSocket::Tree,
            [21.0, 10.0, 55.0, 38.0],
        ),
        (
            "Rare",
            50,
            ItemRarity::Rare,
            true,
            JewelSocket::Tree,
            [19.0, 9.0, 49.0, 34.0],
        ),
        (
            "Magic",
            147,
            ItemRarity::Magic,
            true,
            JewelSocket::Tree,
            [32.0, 14.0, 81.0, 56.0],
        ),
        (
            "Rare",
            69,
            ItemRarity::Rare,
            false,
            JewelSocket::Tree,
            [13.0, 6.0, 33.0, 23.0],
        ),
        (
            "Rare",
            69,
            ItemRarity::Magic,
            true,
            JewelSocket::Tree,
            [13.0, 6.0, 33.0, 23.0],
        ),
        (
            "Rare",
            69,
            ItemRarity::Rare,
            true,
            JewelSocket::Granted,
            [13.0, 6.0, 33.0, 23.0],
        ),
        (
            "Rare",
            69,
            ItemRarity::Rare,
            true,
            JewelSocket::Equipment,
            [13.0, 6.0, 33.0, 23.0],
        ),
    ] {
        let session = run(named, effect, rarity, corrupted, socket);
        let boosted = expected[0] > 13.0;
        for (stat, expected) in ["Damage", "ActionRate", "CritChance", "CritMultiplier"]
            .into_iter()
            .zip(expected)
        {
            let rows = session.minion_stat_mods(0, stat);
            let row = rows
                .iter()
                .find(|row| {
                    row.modifier
                        .origin
                        .as_ref()
                        .is_some_and(|origin| origin.source_id.id == "jewel.2")
                })
                .unwrap();
            assert_eq!(row.value, Some(expected), "{named} {effect}% {stat}");
            assert!(row.active);
        }
        let minion = &session.output().minions[0];
        let detail = pobr_build::breakdown::minion_stat_breakdown(&session, 0, "TotalDPS").unwrap();
        assert_eq!(detail.formula.unwrap().value, minion.dps);
        assert_eq!(minion.life, plain.output().minions[0].life);
        if boosted {
            assert!(minion.dps > plain.output().minions[0].dps);
        } else {
            assert_eq!(minion.dps, plain.output().minions[0].dps);
        }
        // The pre-existing top-level numeric scaling still applies once.
        let life = session.evaluated_stat_mods("Life");
        let life = life
            .iter()
            .find(|row| {
                row.modifier
                    .origin
                    .as_ref()
                    .is_some_and(|origin| origin.source_id.id == "jewel.2")
            })
            .unwrap();
        assert_eq!(
            life.value,
            Some(if boosted {
                100.0 + f64::from(effect)
            } else {
                100.0
            })
        );
        assert_eq!(session.output().dps, plain.output().dps);
    }
}

const QUADRILLA: &str = "Metadata/Monsters/Quadrilla/Quadrilla";

/// The minions of a Huntress with one tamed beast's gem, `lines` on her
/// amulet.
fn tamed_beast_minions(data: &BuildData, lines: &[&str]) -> Vec<pobr_core::calc::MinionOutput> {
    let xml = format!(
        r#"<PathOfBuilding2>
<Build level="90" className="Huntress" ascendClassName="None"/>
<Skills defaultGemLevel="20" activeSkillSet="1"><SkillSet id="1"><Skill enabled="true"><Gem skillId="SummonBeastPlayer" level="20" quality="0" enabled="true" skillMinion="{QUADRILLA}"/></Skill></SkillSet></Skills>
<Items activeItemSet="1"><Item id="1">Rarity: RARE
Test Beads
Gold Amulet
Implicits: 0
{}
</Item><ItemSet id="1"><Slot name="Amulet" itemId="1"/></ItemSet></Items>
</PathOfBuilding2>"#,
        lines.join("\n")
    );
    let build = pobr_build::parse_build(&xml).expect("parse synthetic build");
    calculate_with_data(&build, data, &default_opts())
        .expect("calculate")
        .minions
}

/// A tamed beast's gem summons the monster the player chose for it, and
/// "+# to Level of all Tamed Companion Skills" (Prism of Belief) raises that
/// gem: two levels are four monster levels.
#[test]
fn a_tamed_beasts_gem_summons_the_beast_it_chose() {
    let data = load_data();
    let minions = tamed_beast_minions(&data, &[]);
    let beast = minions
        .iter()
        .find(|minion| minion.minion_id == QUADRILLA)
        .expect("the chosen beast");
    assert_eq!(beast.summoned_by, "SummonBeastPlayer");

    let raised = tamed_beast_minions(&data, &["+2 to Level of all Tamed Companion Skills"]);
    let raised = raised
        .iter()
        .find(|minion| minion.minion_id == QUADRILLA)
        .expect("the chosen beast");
    assert_eq!(raised.level, beast.level + 4);
}

/// The Quadrilla a Huntress's tamed beast's gem summons: `skill` is the
/// gem's `skillMinionSkill` (none: the default), `amulet` the lines of her
/// amulet, and with `spear` she holds a Hardwood Spear in her main hand.
fn quadrilla(
    data: &BuildData,
    skill: Option<u32>,
    amulet: &[&str],
    spear: bool,
) -> pobr_core::calc::MinionOutput {
    let skill = skill
        .map(|index| format!(r#" skillMinionSkill="{index}""#))
        .unwrap_or_default();
    let (weapon, weapon_slot) = if spear {
        (
            "<Item id=\"2\">Rarity: NORMAL\nHardwood Spear\n</Item>",
            "<Slot name=\"Weapon 1\" itemId=\"2\"/>",
        )
    } else {
        ("", "")
    };
    let xml = format!(
        r#"<PathOfBuilding2>
<Build level="90" className="Huntress" ascendClassName="None"/>
<Skills defaultGemLevel="20" activeSkillSet="1"><SkillSet id="1"><Skill enabled="true"><Gem skillId="SummonBeastPlayer" level="20" quality="0" enabled="true" skillMinion="{QUADRILLA}"{skill}/></Skill></SkillSet></Skills>
<Items activeItemSet="1"><Item id="1">Rarity: RARE
Test Beads
Gold Amulet
Implicits: 0
{}
</Item>{weapon}<ItemSet id="1"><Slot name="Amulet" itemId="1"/>{weapon_slot}</ItemSet></Items>
</PathOfBuilding2>"#,
        amulet.join("\n")
    );
    let build = pobr_build::parse_build(&xml).expect("parse synthetic build");
    calculate_with_data(&build, data, &default_opts())
        .expect("calculate")
        .minions
        .into_iter()
        .find(|minion| minion.minion_id == QUADRILLA)
        .expect("the chosen beast")
}

/// A minion attacks with one of its monster's skills: the first by default,
/// or the one the gem's `skillMinionSkill` picks (1-based, among the
/// monster's skills the data has).
#[test]
fn a_minion_attacks_with_the_skill_its_gem_picks() {
    let data = load_data();
    let first = quadrilla(&data, None, &[], false);
    assert_eq!(first.skill_id, "GAQuadrillaSunder");
    assert!(first.dps > 0.0, "{first:?}");
    assert_eq!(
        quadrilla(&data, Some(1), &[], false).skill_id,
        first.skill_id
    );
    let other = quadrilla(&data, Some(3), &[], false);
    assert_ne!(other.skill_id, first.skill_id);
}

/// The Water Djinn Navira, the Last Mirage summons, using `skill` (the
/// gem's `skillMinionSkill`), with Bidding II in the group or not.
fn water_djinn(data: &BuildData, skill: u32, bidding: bool) -> pobr_core::calc::MinionOutput {
    let support = if bidding {
        r#"<Gem skillId="SupportBiddingPlayerTwo" level="1" quality="0" enabled="true"/>"#
    } else {
        ""
    };
    let xml = format!(
        r#"<PathOfBuilding2>
<Build level="90" className="Sorceress" ascendClassName="Disciple of Varashta"/>
<Skills defaultGemLevel="20" activeSkillSet="1"><SkillSet id="1"><Skill enabled="true"><Gem skillId="SummonWaterDjinnPlayer" level="20" quality="0" enabled="true" skillMinion="WaterDjinn" skillMinionSkill="{skill}"/>{support}</Skill></SkillSet></Skills>
</PathOfBuilding2>"#
    );
    let build = pobr_build::parse_build(&xml).expect("parse synthetic build");
    calculate_with_data(&build, data, &default_opts())
        .expect("calculate")
        .minions
        .into_iter()
        .find(|minion| minion.minion_id == "WaterDjinn")
        .expect("the Water Djinn")
}

/// A minion casting a spell deals the spell's own damage, at the minion's
/// level, once per cast time and no more often than its cooldown allows.
/// Navira's Fracturing is a Command skill: Bidding II's "Supported Minions
/// deal 30% more Damage with Command Skills" and 30% increased Cooldown
/// Recovery Rate for them apply to it.
#[test]
fn a_minion_casts_its_spell_at_its_level() {
    let data = load_data();
    let spell = water_djinn(&data, 2, false);
    assert_eq!(spell.skill_id, "ChilledGroundBurstWaterDjinn");
    assert!(spell.dps > 0.0, "{spell:?}");

    let bidding = water_djinn(&data, 2, true).dps / spell.dps;
    // 1.3 times the damage, about 1.3 times as often (the cooldown is
    // rounded to the server tick).
    assert!((1.6..1.75).contains(&bidding), "{bidding}");
}

fn djinn_with_hit_supports(
    data: &BuildData,
    minion: &str,
    skill: u32,
    supports: &[&str],
) -> pobr_core::calc::CompletedCalculationSession {
    let supports: String = supports
        .iter()
        .map(|id| format!(r#"<Gem skillId="{id}" level="1" quality="0" enabled="true"/>"#))
        .collect();
    let xml = format!(
        r#"<PathOfBuilding2>
<Build level="90" className="Sorceress" ascendClassName="Disciple of Varashta"/>
<Skills defaultGemLevel="20" activeSkillSet="1"><SkillSet id="1"><Skill enabled="true"><Gem skillId="Summon{minion}Player" level="20" quality="0" enabled="true" skillMinion="{minion}" skillMinionSkill="{skill}"/>{supports}</Skill></SkillSet></Skills>
</PathOfBuilding2>"#
    );
    let build = pobr_build::parse_build(&xml).expect("parse synthetic build");
    pobr_build::calculate_with_data_session(
        &build,
        data,
        &DataOrchestratorOptions::against_reference_enemy(),
    )
    .expect("calculate")
}

#[test]
fn minion_hit_supports_use_selected_damage() {
    use pobr_core::skill_env::EffectLookup;

    let data = load_data();
    let raw_types = &data.effect("LivingBombFireDjinn").unwrap().skill_types;
    assert!(raw_types.iter().any(|kind| kind == "Spell"));
    assert!(
        !raw_types
            .iter()
            .any(|kind| kind == "Damage" || kind == "Attack")
    );

    let baseline = djinn_with_hit_supports(&data, "FireDjinn", 1, &[]);
    let brand = &baseline.output().minions[0];
    assert_eq!(brand.skill_id, "LivingBombFireDjinn");
    assert!(brand.dps > 0.0);
    for (supports, ratio, invert, gain) in [
        (vec!["SupportRakiatasFlowPlayer"], 3.0, 1.0, 0.0),
        (vec!["SupportXophsPyrePlayer"], 1.8, 0.0, 40.0),
        (
            vec!["SupportRakiatasFlowPlayer", "SupportXophsPyrePlayer"],
            3.8,
            1.0,
            40.0,
        ),
        // Hit eligibility does not grant Attack: Rapid Attacks stays rejected.
        (vec!["SupportRapidAttacksPlayer"], 1.0, 0.0, 0.0),
    ] {
        let supported = djinn_with_hit_supports(&data, "FireDjinn", 1, &supports);
        let actual = &supported.output().minions[0];
        assert_eq!(actual.skill_id, brand.skill_id);
        assert_eq!(actual.level, brand.level);
        // The reference enemy has 50% fire resistance and no chaos resistance.
        // Inversion changes fire's multiplier from 0.5 to 1.5; Xoph adds 0.4
        // times the original fire damage as chaos. Neither changes the rate.
        assert!(
            (actual.dps / brand.dps - ratio).abs() < 1e-8,
            "{supports:?}: {actual:?}"
        );
        assert_eq!(
            supported.minion_stat_totals(0, "HitsInvertEleResChance").0,
            invert
        );
        assert_eq!(
            supported.minion_stat_totals(0, "FireDamageGainAsChaos").0,
            gain
        );
        assert_eq!(
            supported.minion_stat_totals(0, "AttackSpeed"),
            baseline.minion_stat_totals(0, "AttackSpeed"),
        );
    }
}

#[test]
fn minion_hit_supports_reject_non_damaging_spell() {
    let data = load_data();
    // Navira's Well is a non-damaging Spell. Other skills on this same minion
    // hit, so neither Spell nor the sibling skills may supply hit eligibility.
    for supports in [
        vec![],
        vec!["SupportRakiatasFlowPlayer", "SupportXophsPyrePlayer"],
    ] {
        let session = djinn_with_hit_supports(&data, "WaterDjinn", 3, &supports);
        let well = &session.output().minions[0];
        assert_eq!(well.skill_id, "ESRechargeForceRestartWaterDjinn");
        assert_eq!(well.dps, 0.0);
        assert_eq!(
            session.minion_stat_totals(0, "HitsInvertEleResChance").0,
            0.0
        );
        assert_eq!(
            session.minion_stat_totals(0, "FireDamageGainAsChaos").0,
            0.0
        );
    }
}

#[test]
fn minion_hit_supports_respect_summoner_exclusions() {
    let data = load_data();
    // Extraction excludes CreatesMinion on the summoner, even though the
    // selected damaging spell is not itself a summoning skill. This applies
    // to both inferred Damage (Brand) and a raw Damage type (Fracturing).
    for (minion, skill) in [("FireDjinn", 1), ("WaterDjinn", 2)] {
        let baseline = djinn_with_hit_supports(&data, minion, skill, &[]);
        let extraction =
            djinn_with_hit_supports(&data, minion, skill, &["SupportExtractionPlayer"]);
        let ratio = extraction.output().minions[0].dps / baseline.output().minions[0].dps;
        assert!(
            (ratio - 1.0).abs() < 1e-9,
            "{minion} Extraction DPS ratio: {ratio}"
        );
        assert_eq!(extraction.minion_stat_totals(0, "DamageGainAsChaos").0, 0.0);
    }

    // A directly compatible player spell still receives Extraction's damage.
    let direct = Build::new().add_socket_group(
        SocketGroup::new()
            .with_gem_skill("FrostBombPlayer", 20)
            .with_gem_skill("SupportExtractionPlayer", 1),
    );
    let session = pobr_build::calculate_with_data_session(&direct, data, &default_opts()).unwrap();
    assert_eq!(session.stat_totals("DamageGainAsChaos").0, 35.0);
}

/// The sweep's level-20 summons, with no equipment or supports, against
/// the reference enemy. These exercise the minion's own attack multiplier,
/// attack speed, the summoning gem's conversions and the player's attributes.
#[test]
fn summoned_attacks_use_their_skill_and_summoning_stats() {
    let data = load_data();
    let opts = DataOrchestratorOptions::against_reference_enemy().on_reference_basis();
    for (summon, attack, expected) in [
        (
            "SupportingFirePlayer",
            "DeathFromAboveDaemonMinion",
            416.3720964,
        ),
        (
            "SummonAzmerianWolfPlayer",
            "MeleeAtAnimationSpeedWolfPackleader",
            805.1703503,
        ),
        ("RagingSpiritsPlayer", "MinionMeleeStep", 89.11875),
        ("AzmerianSwarmPlayer", "AzmerianSwarmAttack", 67.2091875),
        ("WildProtectorPlayer", "CompanionBearMaul", 791.501979),
        (
            "AncestralSpiritsPlayer",
            "MeleeAtAnimationSpeed",
            73.84260429,
        ),
    ] {
        let build = Build::new()
            .with_character(CharacterIdentity {
                level: 90,
                class_name: "Witch".into(),
                ascendancy_name: String::new(),
            })
            .add_socket_group(SocketGroup::new().with_gem_skill(summon, 20));
        let out = calculate_with_data(&build, &data, &opts).expect("calculate summon");
        let minion = out
            .minions
            .iter()
            .find(|minion| minion.skill_id == attack)
            .expect(attack);
        assert_eq!(minion.level, 40);
        assert!(
            (minion.dps / expected - 1.0).abs() < 0.05,
            "{summon}: {} vs {expected}",
            minion.dps
        );
    }
}

fn manifested_weapon(
    data: &BuildData,
    weapon: &str,
    level: u32,
) -> Vec<pobr_core::calc::MinionOutput> {
    let xml = format!(
        r#"<PathOfBuilding2>
<Build level="90" className="Witch"/>
<Skills><Skill enabled="true"><Gem skillId="ManifestWeaponPlayer" level="{level}" enabled="true"/></Skill></Skills>
<Items activeItemSet="1"><Item id="1">Rarity: NORMAL
{weapon}
</Item><ItemSet id="1"><Slot name="Weapon 1" itemId="1"/></ItemSet></Items>
</PathOfBuilding2>"#
    );
    let build = pobr_build::parse_build(&xml).expect("manifest build");
    calculate_with_data(&build, data, &default_opts())
        .expect("calculate manifest")
        .minions
}

/// Manifest Weapon copies a main-hand melee weapon, never an unarmed or
/// monster-level fallback. Local added elemental damage is copied too.
#[test]
fn manifest_weapon_requires_and_copies_the_main_hand_weapon() {
    let data = load_data();
    assert!(manifested_weapon(&data, "", 20).is_empty());
    assert!(manifested_weapon(&data, "Crude Bow", 20).is_empty());
    let low = manifested_weapon(&data, "Hardwood Spear", 1);
    let high = manifested_weapon(&data, "Hardwood Spear", 20);
    assert_eq!(low.len(), 1);
    assert!(low[0].dps > 0.0);
    assert_eq!(
        low[0].dps, high[0].dps,
        "weapon damage is independent of monster level"
    );
    let fire = manifested_weapon(&data, "Hardwood Spear\nAdds 100 to 100 Fire Damage", 20);
    assert!(fire[0].dps > high[0].dps * 2.0);
}

/// What reaches a companion's attack besides the lines for minions: the
/// character's auras that affect only allies, a share of the character's
/// main hand weapon, and physical damage gained as chaos damage.
#[test]
fn a_companion_attacks_with_what_the_character_gives_it() {
    let data = load_data();
    let base = quadrilla(&data, None, &[], false).dps;

    // "Allies in your Presence have …" is an aura for the allies alone.
    let aura = quadrilla(
        &data,
        None,
        &["Allies in your Presence have 20% increased Attack Speed"],
        false,
    )
    .dps;
    assert!((aura / base - 1.2).abs() < 1e-6, "{base} -> {aura}");

    // "Gain #% of Physical Damage as Chaos Damage" adds damage, as "as
    // extra Chaos Damage" does.
    let chaos = quadrilla(
        &data,
        None,
        &["Minions gain 20% of Physical Damage as Chaos Damage"],
        false,
    )
    .dps;
    assert!(chaos > base * 1.1, "{base} -> {chaos}");

    // "Companions gain added Attack damage equal to #% of your main hand
    // Weapon's damage" needs a weapon in the main hand.
    let line =
        ["Companions gain added Attack damage equal to 30% of your main hand Weapon's damage"];
    let without_spear = quadrilla(&data, None, &line, false).dps;
    assert_eq!(without_spear, base);
    let spear = quadrilla(&data, None, &[], true).dps;
    let shared = quadrilla(&data, None, &line, true).dps;
    assert!(shared > spear * 1.01, "{spear} -> {shared}");
}

/// A Huntress whose tamed beast's gems, one group each, summon `beasts`,
/// every group with Loyalty.
fn loyal_companions(
    data: &BuildData,
    beasts: &[&str],
) -> pobr_core::calc::CompletedCalculationSession {
    let groups: String = beasts
        .iter()
        .map(|beast| {
            format!(
                r#"<Skill enabled="true"><Gem skillId="SummonBeastPlayer" level="20" quality="0" enabled="true" skillMinion="{beast}"/><Gem skillId="SupportLoyaltyPlayer" level="1" quality="0" enabled="true"/></Skill>"#
            )
        })
        .collect();
    let xml = format!(
        r#"<PathOfBuilding2>
<Build level="90" className="Huntress" ascendClassName="None"/>
<Skills defaultGemLevel="20" activeSkillSet="1"><SkillSet id="1">{groups}</SkillSet></Skills>
</PathOfBuilding2>"#
    );
    let build = pobr_build::parse_build(&xml).expect("parse synthetic build");
    pobr_build::calculate_with_data_session(&build, data, &default_opts()).expect("calculate")
}

/// Every Companion counts, with its own supports: Loyalty is a buff of the
/// Companion it supports, so on two different beasts it is two buffs, and
/// on the same beast twice one. Each group's beast has its Life taken
/// before the character's, though a second group of a beast does not add
/// to how many minions the character has.
#[test]
fn every_companion_counts_with_its_own_supports() {
    const WEREWOLF: &str = "Metadata/Monsters/Werewolves/WerewolfPack1";
    let data = load_data();
    let one = loyal_companions(&data, &[QUADRILLA]);
    let share = one.base_sum("TakenFromCompanionBeforeYou");
    assert!(share > 0.0);
    let life = one.base_sum("TotalCompanionLife");
    assert!(life > 0.0);

    let two = loyal_companions(&data, &[QUADRILLA, WEREWOLF]);
    assert_eq!(two.base_sum("TakenFromCompanionBeforeYou"), 2.0 * share);

    let twice = loyal_companions(&data, &[QUADRILLA, QUADRILLA]);
    assert_eq!(twice.base_sum("TakenFromCompanionBeforeYou"), share);
    assert!((twice.base_sum("TotalCompanionLife") - 2.0 * life).abs() < 1e-6);
    assert_eq!(
        twice.base_sum("Multiplier:SummonedMinion"),
        one.base_sum("Multiplier:SummonedMinion")
    );
}

/// The Catha's Balance shares every damage type on the weapon. The
/// monster's damage multiplier and its attack's multiplier both scale the
/// added damage; weapon damage without that sharing modifier stays local.
#[test]
fn companions_scale_shared_weapon_damage_of_every_type() {
    let data = load_data();
    for (summon, monster, attack_multiplier) in [
        ("SummonAzmerianWolfPlayer", "AzmerianWolf", 2.5),
        ("WildProtectorPlayer", "BearCompanion", 1.0),
    ] {
        let hit = |added: &str, share: bool| {
            let xml = format!(
                r#"<PathOfBuilding2><Build level="90" className="Witch"/>
<Skills><Skill><Gem skillId="{summon}" level="20"/></Skill></Skills>
<Items activeItemSet="1"><Item id="1">Rarity: NORMAL
Wooden Club
{added}
</Item><ItemSet id="1"><Slot name="Weapon 1" itemId="1"/></ItemSet></Items>
</PathOfBuilding2>"#
            );
            let build = pobr_build::parse_build(&xml).unwrap();
            let options = DataOrchestratorOptions {
                extra_modifier_texts: if share {
                    vec!["Companions gain added Attack damage equal to 60% of your main hand Weapon's damage".into()]
                } else {
                    vec![]
                },
                ..default_opts()
            };
            let session = pobr_build::calculate_with_data_session(&build, &data, &options).unwrap();
            session.minion_stat_formula(0, "TotalHitAvg").unwrap().value
        };
        let base = hit("", true);
        for kind in ["Physical", "Fire", "Cold", "Lightning", "Chaos"] {
            let added = format!("Adds 100 to 100 {kind} Damage");
            let delta = hit(&added, true) - base;
            // The AddedDamage MORE product uses the engine's two-decimal
            // aggregation precision (2.43 × 2.5 becomes 6.08).
            let effectiveness =
                (data.minions[monster].damage * attack_multiplier * 100.0).round() / 100.0;
            let expected = 100.0 * 0.6 * effectiveness * 1.05;
            assert!(
                (delta - expected).abs() < 1e-6,
                "{summon} {kind}: {delta} vs {expected}"
            );
            assert_eq!(hit(&added, false), hit("", false));
        }
    }
}

#[test]
fn minion_static_support_cost_follows_the_minions_skills() {
    let data = load_data();
    let opts = default_opts();
    let bare = zombie_build(20);
    let base = calculate_with_data(&bare, data, &opts).unwrap();
    let mut supported = bare.clone();
    supported.socket_groups[0]
        .gem_skills
        .push(pobr_build::build::GemSkillRef {
            skill_id: "SupportMinionMeleeSplashPlayerTwo".into(),
            gem_level: 1,
            ..Default::default()
        });
    let boosted = calculate_with_data(&supported, data, &opts).unwrap();
    // Minion Splash II requires CreatesMinion AND MeleeSingleTarget. The
    // zombie's melee strike qualifies and the summon pays its 110% cost.
    assert_eq!(base.mana_cost, 102.0);
    assert_eq!(boosted.mana_cost, 112.0);
    assert_eq!(boosted.action_rate, base.action_rate);
    assert_eq!(boosted.minions[0].dps, base.minions[0].dps);

    // Clash excludes minions by type; Bursting Plague's game description
    // prohibits minion skills via IgnoreMinionTypes. Neither may borrow the
    // zombie's attack types and charge its 120% multiplier to Raise Zombie.
    for support in ["SupportClashPlayer", "SupportBurstingPlaguePlayer"] {
        let mut excluded = bare.clone();
        excluded.socket_groups[0]
            .gem_skills
            .push(pobr_build::build::GemSkillRef {
                skill_id: support.into(),
                gem_level: 1,
                ..Default::default()
            });
        assert_eq!(
            calculate_with_data(&excluded, data, &opts)
                .unwrap()
                .mana_cost,
            base.mana_cost,
            "{support}"
        );
    }

    // Unearth's own damaging spell directly qualifies for Bursting Plague,
    // even though its summoned minion's skills cannot grant compatibility.
    let unearth =
        Build::new().add_socket_group(SocketGroup::new().with_gem_skill("UnearthPlayer", 20));
    let plain = calculate_with_data(&unearth, data, &opts).unwrap();
    let mut direct = unearth;
    direct.socket_groups[0]
        .gem_skills
        .push(pobr_build::build::GemSkillRef {
            skill_id: "SupportBurstingPlaguePlayer".into(),
            gem_level: 1,
            ..Default::default()
        });
    assert!(plain.mana_cost > 0.0);
    assert_eq!(
        calculate_with_data(&direct, data, &opts).unwrap().mana_cost,
        (plain.mana_cost * 1.2).floor()
    );

    // Raise Zombie's unrelated selected monster does not replace its zombie.
    supported.socket_groups[0].gem_skills[0].minion = Some("RaisedSkeletonCleric".into());
    assert_eq!(
        calculate_with_data(&supported, data, &opts)
            .unwrap()
            .mana_cost,
        112.0
    );

    // A player attack/spell is not a minion's strike, and another group's
    // support never changes this summoning skill's cost.
    for skill in ["FireballPlayer", "SummonSkeletalArsonistsPlayer"] {
        let build = Build::new().add_socket_group(SocketGroup::new().with_gem_skill(skill, 20));
        let plain = calculate_with_data(&build, data, &opts).unwrap();
        let mut with_support = build;
        with_support.socket_groups[0]
            .gem_skills
            .push(pobr_build::build::GemSkillRef {
                skill_id: "SupportMinionMeleeSplashPlayerTwo".into(),
                gem_level: 1,
                ..Default::default()
            });
        assert_eq!(
            calculate_with_data(&with_support, data, &opts)
                .unwrap()
                .mana_cost,
            plain.mana_cost
        );
    }
    let other_group = bare.add_socket_group(
        SocketGroup::new()
            .with_gem_skill("FireballPlayer", 20)
            .with_gem_skill("SupportMinionMeleeSplashPlayerTwo", 1),
    );
    assert_eq!(
        calculate_with_data(&other_group, data, &opts)
            .unwrap()
            .mana_cost,
        102.0
    );
}

#[test]
fn minion_static_presence_auras_reach_allies_once_and_stay_off_the_weapon() {
    use pobr_build::calculate_with_data_session;
    use pobr_data::modifier::{ModFlags, ModName, ModType};

    let data = load_data();
    let build = |line: &str, slot: &str, base: &str| {
        pobr_build::parse_build(&format!(r#"<PathOfBuilding2>
<Build level="90" className="Witch" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="RaiseZombiePlayer" level="20" enabled="true"/></Skill></Skills>
<Items activeItemSet="1"><Item id="1">Rarity: RARE
Aura Test
{base}
{line}
</Item><ItemSet id="1"><Slot name="{slot}" itemId="1"/></ItemSet></Items>
</PathOfBuilding2>"#)).unwrap()
    };
    let calc = |line, slot, base| {
        calculate_with_data_session(&build(line, slot, base), data, &default_opts()).unwrap()
    };
    for (slot, item) in [("Ring 1", "Gold Ring"), ("Weapon 1", "Rattling Sceptre")] {
        let bare = calc("", slot, item);
        for (line, player_bonus) in [
            (
                "You and Allies in your Presence have 8% increased Attack Speed",
                8.0,
            ),
            (
                "Allies in your Presence have 8% increased Attack Speed",
                0.0,
            ),
        ] {
            let aura = calc(line, slot, item);
            assert!(
                (aura.output().minions[0].dps / bare.output().minions[0].dps - 1.08).abs() < 1e-8,
                "{slot}: {line}"
            );
            let cfg = aura.config().clone().with_flags(ModFlags::ATTACK);
            assert_eq!(
                aura.mod_db()
                    .sum(ModType::Inc, &cfg, &[ModName::from("AttackSpeed")]),
                player_bonus
            );
        }
        let resist = calc(
            "You and Allies in your Presence have +19% to Chaos Resistance",
            slot,
            item,
        );
        assert_eq!(
            resist.output().chaos_resistance - bare.output().chaos_resistance,
            19.0
        );
        assert_eq!(
            resist.output().minions[0].stats["ChaosResist"]
                - bare.output().minions[0].stats["ChaosResist"],
            19.0
        );
    }
}

#[test]
fn minion_static_offering_effect_uses_the_offerings_types() {
    use pobr_build::calculate_with_data_session;

    let data = load_data();
    let mut build = zombie_build(20)
        .add_socket_group(SocketGroup::new().with_gem_skill("PainOfferingPlayer", 20));
    let baseline = calculate_with_data_session(&build, data, &default_opts()).unwrap();
    let boosted = calculate_with_data_session(
        &build,
        data,
        &DataOrchestratorOptions {
            extra_modifier_texts: vec!["Offering Skills have 30% increased Buff effect".into()],
            ..default_opts()
        },
    )
    .unwrap();
    // Game level-20 Pain Offering: 58% damage, 29% speed. Existing buff
    // precision truncates these to 75 and 37 after 30% increased effect.
    assert_eq!(baseline.minion_stat_totals(0, "Damage").1, 58.0);
    assert_eq!(boosted.minion_stat_totals(0, "Damage").1, 75.0);
    assert_eq!(baseline.minion_stat_totals(0, "AttackSpeed").1, 29.0);
    assert_eq!(boosted.minion_stat_totals(0, "AttackSpeed").1, 37.0);

    // Changing the selected player skill must not change the Offering buff.
    build.main_socket_group = Some(2);
    let selected = calculate_with_data_session(
        &build,
        data,
        &DataOrchestratorOptions {
            extra_modifier_texts: vec!["Offering Skills have 30% increased Buff effect".into()],
            ..default_opts()
        },
    )
    .unwrap();
    assert_eq!(
        selected.output().minions[0].dps,
        boosted.output().minions[0].dps
    );
}
