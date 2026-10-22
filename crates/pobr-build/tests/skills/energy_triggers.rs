//! How often a meta gem triggers the spell socketed in it.
//!
//! Cast on Critical "gains 1 Energy per Power of enemies you Critically Hit
//! with Skills, modified by the percentage of the enemy's Ailment Threshold
//! the Critical Hit will deal" and "triggers all Socketed Spells and loses all
//! Energy on reaching maximum Energy", which is "10 maximum Energy per 0.1
//! seconds of base cast time of Socketed Spells". Comet's base cast time is
//! 1 second and it adds 1 second to its total cast time, which counts twice:
//! 300 Energy. `pobr_core::calc::energy` has the rules and their sources.

use pobr_build::{BuildData, DataOrchestratorOptions, calculate_with_data, parse_build};
use pobr_core::calc::OutputTable;
use pobr_data::monster::{EnemyTier, enemy_ailment_threshold};

/// Enemy level of every calculation here: the lowest a Pinnacle boss has.
const ENEMY_LEVEL: u32 = 82;
/// What every character here has besides its gems, so that a critical hit of
/// a level 20 spell is a visible share of that enemy's Ailment Threshold.
const SPELL_DAMAGE: &str = "10000% increased Spell Damage";
/// Level 20 Cast on Critical: 57% increased Energy gained.
const GEM_GAIN: f64 = 1.57;
/// A unique enemy's Power, and an ordinary monster's.
const BOSS: (EnemyTier, f64) = (EnemyTier::Pinnacle, 20.0);
const MONSTER: (EnemyTier, f64) = (EnemyTier::None, 1.0);

use crate::test_data as data;

#[test]
fn channel_time_to_trigger_uses_the_meta_gems_energy_capacity_and_continuous_gain() {
    let data = data();
    let xml = build_xml(&[], &[], COMET)
        .replace("MetaCastOnCritPlayer", "MetaCastWhileChannellingPlayer")
        .replace("Cast on Critical", "Cast while Channelling");
    let options = options(EnemyTier::None, &[]);
    let out = calculate(&data, &xml, &options);
    // 1 s base cast + 2 × Comet's 1 s added total cast time = 300 Energy;
    // cast_while_channelling_gain_X_energy_per_second_while_channelling = 60.
    assert_eq!(out.channel_time_to_trigger, 5.0);
    let faster = calculate(
        &data,
        &xml,
        &DataOrchestratorOptions {
            extra_modifier_texts: vec![
                SPELL_DAMAGE.into(),
                "Meta Skills gain 100% increased Energy".into(),
            ],
            ..options.clone()
        },
    );
    assert_eq!(faster.channel_time_to_trigger, 2.5);
    assert_eq!(out.dps, faster.dps);
    let two_spells = xml.replace("</Skill>", &format!("{}</Skill>", spark()));
    assert_eq!(
        calculate(&data, &two_spells, &options).channel_time_to_trigger,
        (370.0_f64 / 60.0 * 1e9).round() / 1e9
    );
    assert_eq!(
        comet(&data, EnemyTier::None, &[]).channel_time_to_trigger,
        0.0
    );
}

fn gem(skill_id: &str, name: &str) -> String {
    format!(
        r#"<Gem skillId="{skill_id}" nameSpec="{name}" level="20" quality="0" enabled="true"/>"#
    )
}

fn spark() -> String {
    gem("SparkPlayer", "Spark")
}

/// Comet in a Cast on Critical, with `socketed` more gems in the same meta
/// gem, and `others` as further skill groups. `main` is the 1-based group and
/// the active skill in it that the build calculates.
fn build_xml(socketed: &[String], others: &[String], main: (usize, usize)) -> String {
    let meta = format!(
        r#"<Skill enabled="true" mainActiveSkill="{}">{}{}{}</Skill>"#,
        if main.0 == 1 { main.1 } else { 1 },
        gem("MetaCastOnCritPlayer", "Cast on Critical"),
        gem("CometPlayer", "Comet"),
        socketed.concat(),
    );
    let others: String = others
        .iter()
        .enumerate()
        .map(|(index, gems)| {
            let active = if main.0 == index + 2 { main.1 } else { 1 };
            format!(r#"<Skill enabled="true" mainActiveSkill="{active}">{gems}</Skill>"#)
        })
        .collect();
    format!(
        r#"<PathOfBuilding2>
<Build level="90" className="Sorceress" ascendClassName="None" mainSocketGroup="{}"/>
<Skills defaultGemLevel="20" activeSkillSet="1"><SkillSet id="1">{meta}{others}</SkillSet></Skills>
</PathOfBuilding2>"#,
        main.0
    )
}

/// The Comet of the build above, in the first group.
const COMET: (usize, usize) = (1, 2);
/// The first skill of the group after the meta gem's.
const OTHER: (usize, usize) = (2, 1);

fn options(tier: EnemyTier, extra: &[&str]) -> DataOrchestratorOptions {
    DataOrchestratorOptions {
        enemy_level: ENEMY_LEVEL,
        enemy_tier: tier,
        mode_effective: true,
        extra_modifier_texts: std::iter::once(SPELL_DAMAGE)
            .chain(extra.iter().copied())
            .map(str::to_string)
            .collect(),
        ..Default::default()
    }
}

fn calculate(data: &BuildData, xml: &str, options: &DataOrchestratorOptions) -> OutputTable {
    let build = parse_build(xml).expect("parse synthetic build");
    calculate_with_data(&build, data, options).expect("calculate")
}

/// The Comet triggered by a Cast on Critical that Spark fills.
fn comet(data: &BuildData, tier: EnemyTier, extra: &[&str]) -> OutputTable {
    calculate(
        data,
        &build_xml(&[], &[spark()], COMET),
        &options(tier, extra),
    )
}

/// Spark itself, in the same build.
fn spark_by_hand(data: &BuildData, tier: EnemyTier) -> OutputTable {
    calculate(
        data,
        &build_xml(&[], &[spark()], OTHER),
        &options(tier, &[]),
    )
}

/// A rate the calculation rounds to nine decimal places.
fn assert_rate(actual: f64, expected: f64) {
    assert!(
        expected > 1e-4,
        "{expected}: too small for the rounding to tell"
    );
    assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
}

/// Energy one critical hit of `source` gives a gem.
fn energy_per_hit(source: &OutputTable, enemy_power: f64, gain: f64) -> f64 {
    assert!(source.crit_hit_avg > 0.0);
    let threshold = f64::from(enemy_ailment_threshold(ENEMY_LEVEL));
    enemy_power * source.crit_hit_avg / threshold * gain
}

fn critical_hits_per_second(source: &OutputTable) -> f64 {
    source.effective_action_rate * source.hit_chance * source.crit_chance
}

/// What the gem's rule gives from the source skill's own numbers: its
/// critical hits per second over the whole number of them that fills the gem.
fn expected_rate(source: &OutputTable, enemy_power: f64, gain: f64, maximum: f64) -> f64 {
    let hits_to_fill = (maximum / energy_per_hit(source, enemy_power, gain)).ceil();
    critical_hits_per_second(source) / hits_to_fill
}

#[test]
fn the_gem_holds_ten_energy_per_tenth_of_a_second_of_its_spells() {
    let data = data();
    let options = options(BOSS.0, &[]);
    let comet = calculate(&data, &build_xml(&[], &[spark()], COMET), &options);
    assert_eq!(comet.energy_trigger_maximum, 300.0);
    // Spark's base cast time is 0.7 seconds.
    let both = calculate(&data, &build_xml(&[spark()], &[spark()], COMET), &options);
    assert_eq!(both.energy_trigger_maximum, 370.0);
}

#[test]
fn the_spell_is_used_as_often_as_critical_hits_fill_the_gem() {
    let data = data();
    for (tier, power) in [BOSS, MONSTER] {
        let source = spark_by_hand(&data, tier);
        let triggered = comet(&data, tier, &[]);
        let own = calculate(
            &data,
            &build_xml(&[], &[spark()], COMET),
            &DataOrchestratorOptions {
                energy_triggers: false,
                ..options(tier, &[])
            },
        );

        // It takes several critical hits: the rule's rounding up is in play.
        let energy = energy_per_hit(&source, power, GEM_GAIN);
        assert!(energy < 150.0, "{tier:?}: {energy}");
        assert_rate(triggered.energy_per_trigger_event, energy);
        let rate = expected_rate(&source, power, GEM_GAIN, 300.0);
        assert_rate(triggered.skill_trigger_rate, rate);
        assert_rate(triggered.effective_action_rate, rate);
        assert_eq!(
            triggered.energy_trigger_source.as_deref(),
            Some("SparkPlayer")
        );
        // The damage of one Comet stands; it happens at the other rate.
        let dps = own.dps * rate / own.effective_action_rate;
        assert!(
            (triggered.dps / dps - 1.0).abs() < 1e-6,
            "{tier:?}: {} != {dps}",
            triggered.dps
        );
        assert_eq!(triggered.total_hit_avg, own.total_hit_avg);
        // Off, the gem is left out: nothing of it is reported.
        assert_eq!(own.skill_trigger_rate, 0.0, "{tier:?}");
        assert_eq!(own.energy_trigger_maximum, 0.0);
        assert_eq!(own.energy_trigger_source, None);
    }
}

#[test]
fn an_ordinary_monster_fills_the_gem_more_slowly_than_a_unique_one() {
    let data = data();
    let boss = comet(&data, BOSS.0, &[]);
    let monster = comet(&data, MONSTER.0, &[]);
    assert!(
        monster.skill_trigger_rate < boss.skill_trigger_rate / 10.0,
        "{} against {}",
        monster.skill_trigger_rate,
        boss.skill_trigger_rate
    );
}

#[test]
fn a_hit_that_fills_the_gem_triggers_it_once() {
    let data = data();
    // A hundred times the damage again: one critical hit gives more than the
    // gem holds, and what is over is lost.
    let more = ["9900% more Spell Damage"];
    let source = calculate(
        &data,
        &build_xml(&[], &[spark()], OTHER),
        &options(BOSS.0, &more),
    );
    assert!(energy_per_hit(&source, BOSS.1, GEM_GAIN) > 600.0);
    let triggered = comet(&data, BOSS.0, &more);
    assert_rate(
        triggered.skill_trigger_rate,
        critical_hits_per_second(&source),
    );
}

#[test]
fn modifiers_to_energy_gained_fill_the_gem_sooner() {
    let data = data();
    let source = spark_by_hand(&data, BOSS.0);
    let plain = expected_rate(&source, BOSS.1, GEM_GAIN, 300.0);
    for (line, gain) in [
        // 57% from the gem's level and 100%: increases add.
        ("Meta Skills gain 100% increased Energy", 2.57),
        ("Meta Skills gain 20% reduced Energy", 1.37),
        ("Meta Skills gain 50% more Energy", GEM_GAIN * 1.5),
        ("Energy Generation is doubled", GEM_GAIN * 2.0),
        // The Energy comes with a critical hit, so the condition holds
        // without the build's configuration saying so.
        (
            "Meta Skills gain 100% increased Energy if you've dealt a Critical Hit Recently",
            2.57,
        ),
    ] {
        let rate = expected_rate(&source, BOSS.1, gain, 300.0);
        assert!((rate / plain - 1.0).abs() > 0.05, "{line}: {rate}");
        assert_rate(comet(&data, BOSS.0, &[line]).skill_trigger_rate, rate);
    }
}

/// "25% chance for Trigger skills to refund half of Energy Spent": a quarter
/// of the rounds start with the gem half full.
#[test]
fn a_refund_of_half_the_energy_spent_shortens_the_next_round() {
    let data = data();
    let source = spark_by_hand(&data, BOSS.0);
    let energy = energy_per_hit(&source, BOSS.1, GEM_GAIN);
    let hits_to_gain = |target: f64| (target / energy).ceil();
    let hits_to_fill = 0.75 * hits_to_gain(300.0) + 0.25 * hits_to_gain(150.0);
    let with = comet(
        &data,
        BOSS.0,
        &["25% chance for Trigger skills to refund half of Energy Spent"],
    );
    assert_rate(
        with.skill_trigger_rate,
        critical_hits_per_second(&source) / hits_to_fill,
    );
    assert!(with.skill_trigger_rate > comet(&data, BOSS.0, &[]).skill_trigger_rate);
}

#[test]
fn the_source_is_the_skill_that_fills_the_gem_most_often() {
    let data = data();
    let fireball = gem("FireballPlayer", "Fireball");
    let others = [spark(), fireball];
    let by_hand = |group: usize| {
        calculate(
            &data,
            &build_xml(&[], &others, (group, 1)),
            &options(BOSS.0, &[]),
        )
    };
    let rates = [
        (
            "SparkPlayer",
            expected_rate(&by_hand(2), BOSS.1, GEM_GAIN, 300.0),
        ),
        (
            "FireballPlayer",
            expected_rate(&by_hand(3), BOSS.1, GEM_GAIN, 300.0),
        ),
    ];
    assert!((rates[0].1 / rates[1].1 - 1.0).abs() > 0.01, "{rates:?}");
    let best = if rates[0].1 > rates[1].1 {
        rates[0]
    } else {
        rates[1]
    };
    let triggered = calculate(
        &data,
        &build_xml(&[], &others, COMET),
        &options(BOSS.0, &[]),
    );
    assert_eq!(triggered.energy_trigger_source.as_deref(), Some(best.0));
    assert_rate(triggered.skill_trigger_rate, best.1);
}

#[test]
fn a_spell_with_nothing_to_fill_its_gem_keeps_its_own_rate() {
    let data = data();
    let options = options(BOSS.0, &[]);
    let xml = build_xml(&[], &[], COMET);
    let alone = calculate(&data, &xml, &options);
    let own = calculate(
        &data,
        &xml,
        &DataOrchestratorOptions {
            energy_triggers: false,
            ..options.clone()
        },
    );
    assert_eq!(alone.energy_trigger_maximum, 300.0);
    assert_eq!(alone.skill_trigger_rate, 0.0);
    assert_eq!(alone.energy_trigger_source, None);
    assert!(alone.dps > 0.0);
    assert_eq!(alone.dps, own.dps);
    assert_eq!(alone.effective_action_rate, own.effective_action_rate);
}

/// "Energy cannot be gained from direct effects of Triggered Skills."
#[test]
fn a_spell_in_another_meta_gem_is_not_a_source() {
    let data = data();
    // The only other Spark is itself triggered by a Cast on Critical.
    let other_meta = format!(
        "{}{}",
        gem("MetaCastOnCritPlayer", "Cast on Critical"),
        spark()
    );
    let comet = calculate(
        &data,
        &build_xml(&[], &[other_meta], COMET),
        &options(BOSS.0, &[]),
    );
    assert_eq!(comet.energy_trigger_source, None);
    assert_eq!(comet.skill_trigger_rate, 0.0);
    // Nor is the Spark socketed next to the Comet.
    let together = calculate(
        &data,
        &build_xml(&[spark()], &[], COMET),
        &options(BOSS.0, &[]),
    );
    assert_eq!(together.energy_trigger_maximum, 370.0);
    assert_eq!(together.energy_trigger_source, None);
}

#[test]
fn a_skill_outside_a_meta_gem_is_untouched() {
    let data = data();
    let options = options(BOSS.0, &[]);
    let xml = build_xml(&[], &[spark()], OTHER);
    let by_hand = calculate(&data, &xml, &options);
    let off = calculate(
        &data,
        &xml,
        &DataOrchestratorOptions {
            energy_triggers: false,
            ..options.clone()
        },
    );
    assert_eq!(by_hand.energy_trigger_maximum, 0.0);
    assert_eq!(by_hand, off);
}

fn meta_xml(meta: &str, others: &[String], inputs: &[(&str, f64)]) -> String {
    let config: String = inputs
        .iter()
        .map(|(key, value)| format!(r#"<Input name="{key}" number="{value}"/>"#))
        .collect();
    build_xml(&[], others, COMET)
        .replace("MetaCastOnCritPlayer", meta)
        .replace(
            "</PathOfBuilding2>",
            &format!("<Config>{config}</Config></PathOfBuilding2>"),
        )
}

#[test]
fn cast_on_block_uses_25_energy_per_block_and_configured_incoming_hits() {
    let data = data();
    let xml = meta_xml(
        "MetaCastOnBlockPlayer",
        &[],
        &[("energyIncomingHitRate", 8.0)],
    );
    let options = options(BOSS.0, &["+50% to Block chance"]);
    let out = calculate(&data, &xml, &options);
    assert_eq!(out.effective_block_chance, 50.0);
    assert_eq!(out.energy_trigger_maximum, 300.0);
    assert_eq!(out.energy_per_trigger_event, 25.0 * 1.57);
    // 8 incoming hits/s × 50% block / ceil(300 / 39.25) = 0.5.
    assert_eq!(out.skill_trigger_rate, 0.5);
    assert_eq!(out.trigger_rate_cap, 4.0);
    let zero = calculate(
        &data,
        &xml.replace("number=\"8\"", "number=\"0\""),
        &options,
    );
    assert_eq!(zero.dps, 0.0);
    assert_eq!(zero.skill_trigger_rate, 0.0);
    assert!(!zero.energy_trigger.as_ref().unwrap().at_own_rate);
    let speed = xml
        .replace("energyIncomingHitRate", "enemySpeed")
        .replace("number=\"8\"", "number=\"125\"");
    assert_eq!(calculate(&data, &speed, &options).skill_trigger_rate, 0.5);
    let both = meta_xml(
        "MetaCastOnBlockPlayer",
        &[],
        &[("energyIncomingHitRate", 0.0), ("enemySpeed", 125.0)],
    );
    assert_eq!(calculate(&data, &both, &options).dps, 0.0);
    let both = both.replace(
        "energyIncomingHitRate\" number=\"0",
        "energyIncomingHitRate\" number=\"4",
    );
    assert_eq!(calculate(&data, &both, &options).skill_trigger_rate, 0.25);
    let stopped = meta_xml("MetaCastOnBlockPlayer", &[], &[("enemySpeed", 0.0)]);
    assert_eq!(calculate(&data, &stopped, &options).dps, 0.0);
    let no_block = calculate(
        &data,
        &xml,
        &DataOrchestratorOptions {
            extra_modifier_texts: vec![SPELL_DAMAGE.into()],
            ..options
        },
    );
    assert!(no_block.energy_trigger.as_ref().unwrap().at_own_rate);
    assert_eq!(
        no_block.energy_trigger.as_ref().unwrap().missing_source,
        Some("EnergyNeedsBlockChance")
    );
}

#[test]
fn cast_on_minion_death_uses_50_energy_times_killed_minion_power() {
    let data = data();
    let xml = meta_xml(
        "MetaCastOnMinionDeathPlayer",
        &[],
        &[
            ("energyMinionDeathsPerSecond", 6.0),
            ("energyMinionPower", 2.0),
        ],
    );
    let out = calculate(&data, &xml, &options(BOSS.0, &[]));
    assert_eq!(out.energy_trigger_maximum, 300.0);
    // 100 / stat value 2 = 50; 2 Power × 57% increased = 157 Energy.
    assert_eq!(out.energy_per_trigger_event, 157.0);
    assert_eq!(out.skill_trigger_rate, 3.0);
    let refund = calculate(
        &data,
        &xml,
        &options(
            BOSS.0,
            &["25% chance for Trigger skills to refund half of Energy Spent"],
        ),
    );
    assert_rate(refund.skill_trigger_rate, 6.0 / 1.75);
    let unset = calculate(
        &data,
        &meta_xml("MetaCastOnMinionDeathPlayer", &[], &[]),
        &options(BOSS.0, &[]),
    );
    assert!(unset.dps > 0.0);
    assert!(unset.energy_trigger.as_ref().unwrap().at_own_rate);
    let stopped = meta_xml(
        "MetaCastOnMinionDeathPlayer",
        &[],
        &[("energyMinionDeathsPerSecond", 0.0)],
    );
    let stopped = calculate(&data, &stopped, &options(BOSS.0, &[]));
    assert_eq!(stopped.dps, 0.0);
    assert!(!stopped.energy_trigger.as_ref().unwrap().at_own_rate);
}

#[test]
fn cast_on_elemental_ailment_uses_shocks_from_the_source_skill() {
    let data = data();
    let xml = meta_xml("MetaCastOnElementalAilmentPlayer", &[spark()], &[]);
    let options = options(BOSS.0, &["100% chance to Shock"]);
    let out = calculate(&data, &xml, &options);
    let source = calculate(&data, &build_xml(&[], &[spark()], OTHER), &options);
    assert_eq!(out.energy_trigger_maximum, 300.0);
    // 100 centienergy / 100 × unique Power 20 × 1.57 = 31.4/shock.
    assert_eq!(out.energy_per_trigger_event, 31.4);
    assert_rate(out.skill_trigger_rate, source.effective_action_rate / 10.0);
    assert_eq!(out.energy_trigger_source.as_deref(), Some("SparkPlayer"));
}

#[test]
fn spellslinger_stores_500_but_spends_300_on_comet_and_gains_14_per_spark() {
    let data = data();
    let xml = meta_xml("MetaSpellslingerPlayer", &[spark()], &[]);
    let options = options(BOSS.0, &[]);
    let out = calculate(&data, &xml, &options);
    let source = calculate(&data, &build_xml(&[], &[spark()], OTHER), &options);
    let terms = out.energy_trigger.as_ref().unwrap();
    assert_eq!(out.energy_trigger_maximum, 500.0);
    assert_eq!(terms.energy_per_trigger, 300.0);
    // 20 centienergy per 10 ms = 14 per 700 ms cast, then 57% increased.
    assert_eq!(out.energy_per_trigger_event, 21.98);
    assert_rate(
        out.skill_trigger_rate,
        source.effective_action_rate * 21.98 / 300.0,
    );
    assert!(terms.invocation);
    let stopped = meta_xml(
        "MetaSpellslingerPlayer",
        &[spark()],
        &[("energyInvocationUsesPerSecond", 0.0)],
    );
    assert_eq!(calculate(&data, &stopped, &options).dps, 0.0);
}

#[test]
fn spellslinger_counts_casts_while_critical_triggers_count_periodic_hits() {
    let data = data();
    let ball = gem("BallLightningPlayer", "Ball Lightning");
    for (extra, casts) in [(vec![], 1.0), (vec!["100% increased Cast Speed"], 2.0)] {
        let opts = options(BOSS.0, &extra);
        let source = calculate(&data, &build_xml(&[], &[ball.clone()], OTHER), &opts);
        // A 1000 ms base cast, but a hit every 200 ms while in range.
        assert_rate(source.action_rate, 5.0);
        let out = calculate(
            &data,
            &meta_xml("MetaSpellslingerPlayer", &[ball.clone()], &[]),
            &opts,
        );
        let terms = out.energy_trigger.as_ref().unwrap();
        assert_eq!(terms.source.as_deref(), Some("BallLightningPlayer"));
        assert_rate(terms.events_per_second, casts);
        assert_rate(terms.energy_per_event, 20.0 * GEM_GAIN);
        assert_rate(out.skill_trigger_rate, casts * 20.0 * GEM_GAIN / 300.0);

        let critical = calculate(
            &data,
            &meta_xml("MetaCastOnCritPlayer", &[ball.clone()], &[]),
            &opts,
        );
        assert_rate(
            critical.energy_trigger.as_ref().unwrap().events_per_second,
            critical_hits_per_second(&source),
        );
    }
}

#[test]
fn energy_triggered_periodic_payloads_keep_their_hit_cadence() {
    let data = data();
    for (skill, duration, active_rate, duration_modifier) in [
        ("SolarOrbPlayer", Some(10.0), 1.0 / 1.4, None),
        ("FirestormPlayer", Some(6.0), 5.0, None),
        ("BallLightningPlayer", None, 5.0, None),
        (
            "SolarOrbPlayer",
            Some(0.1),
            1.0 / 1.4,
            Some("99% reduced Skill Effect Duration"),
        ),
    ] {
        let mut opts = options(BOSS.0, &["+50% to Block chance"]);
        opts.extra_modifier_texts
            .extend(duration_modifier.map(str::to_string));
        let xml = meta_xml("MetaCastOnBlockPlayer", &[], &[]).replace("CometPlayer", skill);
        let unknown = calculate(&data, &xml, &opts);
        assert!(unknown.energy_trigger.as_ref().unwrap().at_own_rate);
        let own = calculate(
            &data,
            &xml,
            &DataOrchestratorOptions {
                energy_triggers: false,
                ..opts.clone()
            },
        );
        assert_eq!(unknown.action_rate, own.action_rate);
        assert_eq!(unknown.dps, own.dps);
        // Reference mode never applies the Energy cadence, even with an explicit zero.
        let reference = calculate(
            &data,
            &meta_xml(
                "MetaCastOnBlockPlayer",
                &[],
                &[("energyIncomingHitRate", 0.0)],
            )
            .replace("CometPlayer", skill),
            &opts.clone().on_reference_basis(),
        );
        assert_eq!(reference.energy_trigger, None);
        assert_rate(reference.action_rate, active_rate);

        for incoming in [100.0, 0.06, 0.0] {
            let xml = meta_xml(
                "MetaCastOnBlockPlayer",
                &[],
                &[("energyIncomingHitRate", incoming)],
            )
            .replace("CometPlayer", skill);
            let build = parse_build(&xml).unwrap();
            let session = pobr_build::calculate_with_data_session(&build, &data, &opts).unwrap();
            let out = session.output();
            let terms = out.energy_trigger.as_ref().unwrap();
            assert!(!terms.at_own_rate);
            let uptime = if incoming == 0.0 {
                0.0
            } else {
                duration.map_or(1.0, |seconds| {
                    (seconds * terms.triggers_per_second).min(1.0)
                })
            };
            let expected = active_rate * uptime;
            assert!(
                (out.action_rate - expected).abs() < 1e-9,
                "{skill}: {} != {expected}",
                out.action_rate
            );
            assert_eq!(out.effective_action_rate, out.action_rate);
            assert_eq!(
                out.skill_use_time.as_ref().unwrap().effective_rate,
                out.action_rate
            );
            assert!((out.dps - unknown.dps * expected / unknown.action_rate).abs() < 1e-6);
            for (stat, value) in [("ActionRate", out.action_rate), ("TotalDPS", out.dps)] {
                assert_eq!(
                    session.stat_formula(stat).unwrap().value,
                    value,
                    "{skill}: {stat}"
                );
            }
            assert_eq!(
                session
                    .derived_detail("EffectiveActionRate")
                    .unwrap()
                    .inputs,
                vec![("ActionRate".into(), out.action_rate)],
            );
            if incoming == 0.0 {
                let sustained = calculate(
                    &data,
                    &xml,
                    &DataOrchestratorOptions {
                        periodic_uptime: false,
                        ..opts.clone()
                    },
                );
                assert_eq!(sustained.action_rate, 0.0);
                assert_eq!(sustained.dps, 0.0);
            }
        }
    }
}

#[test]
fn energy_trigger_cadence_reuses_rain_limits_and_summary() {
    use pobr_core::calc::{CalculationSession, MinimalInput, energy, rain};
    use pobr_core::{CalcConfig, Modifier};
    use pobr_data::prelude::ModType;

    // Exercise the shared rain conversion directly; this does not assert that
    // a particular attack can be socketed in a spell-only meta gem.
    for reference_hits in [false, true] {
        for rate in [10.0, 0.5, 0.0] {
            let mut session = CalculationSession::new(MinimalInput {
                base_action_rate: 2.0,
                base_hit_min: 100.0,
                base_hit_max: 100.0,
                ..Default::default()
            })
            .with_config(
                CalcConfig::spell()
                    .with_multiplier("EnergyMinionDeathsPerSecond", rate)
                    .with_rain_hits_whenever_allowed(reference_hits),
            );
            session.add_modifiers([
                Modifier::number(energy::MAXIMUM_ENERGY, ModType::Base, 100.0),
                Modifier::number(energy::ENERGY_PER_MINION_POWER, ModType::Base, 100.0),
                Modifier::number(rain::LANDING_INTERVAL, ModType::Base, 80.0),
                Modifier::number(rain::HIT_INTERVAL, ModType::Base, 120.0),
                Modifier::number("ProjectileCount", ModType::Base, 5.0),
            ]);
            session.perform_minimal().unwrap();
            let out = session.output();
            let expected = if rate == 0.0 {
                0.0
            } else if reference_hits {
                1.0 / 0.12
            } else {
                (3.0 * rate).min(6.25)
            };
            assert!((out.action_rate - expected).abs() < 1e-9);
            assert_eq!(out.effective_action_rate, out.action_rate);
            assert_eq!(session.rain().unwrap().hits_per_second, out.action_rate);
            assert_eq!(
                session.stat_formula("ActionRate").unwrap().value,
                out.action_rate
            );
            assert_eq!(session.stat_formula("TotalDPS").unwrap().value, out.dps);
        }
    }
}

#[test]
fn elemental_invocation_spends_cast_time_energy_instead_of_its_500_storage() {
    let data = data();
    let xml = meta_xml("MetaElementalInvocationPlayer", &[spark()], &[]);
    let options = options(BOSS.0, &["100% chance to Shock"]);
    let out = calculate(&data, &xml, &options);
    let source = calculate(&data, &build_xml(&[], &[spark()], OTHER), &options);
    assert_eq!(out.energy_trigger_maximum, 500.0);
    assert_eq!(
        out.energy_trigger.as_ref().unwrap().energy_per_trigger,
        300.0
    );
    // 1 Energy × Power 20 × 68% increased at level 20 = 33.6.
    assert_eq!(out.energy_per_trigger_event, 33.6);
    assert_rate(
        out.skill_trigger_rate,
        source.effective_action_rate * 33.6 / 300.0,
    );
    let quality = xml.replace(
        "MetaElementalInvocationPlayer\" nameSpec=\"Cast on Critical\" level=\"20\" quality=\"0",
        "MetaElementalInvocationPlayer\" nameSpec=\"Cast on Critical\" level=\"20\" quality=\"20",
    );
    assert_eq!(
        calculate(&data, &quality, &options).energy_trigger_maximum,
        600.0
    );
}

#[test]
fn meta_supports_modify_energy_without_changing_the_payloads_own_rate() {
    let data = data();
    let xml = meta_xml(
        "MetaCastOnBlockPlayer",
        &[],
        &[("energyIncomingHitRate", 8.0)],
    )
    .replace(
        "</Skill>",
        &format!(
            "{}{}{}{}",
            gem("SupportBoundlessEnergyPlayerTwo", "Boundless Energy II"),
            gem("SupportEnergyRetentionPlayer", "Energy Retention"),
            "",
            "</Skill>"
        ),
    );
    let options = options(BOSS.0, &["+50% to Block chance"]);
    let out = calculate(&data, &xml, &options);
    // 25 × (1 + 0.57 + 0.45) = 50.5; 6 events from empty, 3 from half;
    // Energy Retention refunds half in 35% of cycles.
    assert_eq!(out.energy_per_trigger_event, 50.5);
    assert_rate(out.skill_trigger_rate, 4.0 / (0.65 * 6.0 + 0.35 * 3.0));
    let own = calculate(
        &data,
        &xml,
        &DataOrchestratorOptions {
            energy_triggers: false,
            ..options
        },
    );
    let expected = own.dps * out.energy_trigger.as_ref().unwrap().triggers_per_second
        / own.effective_action_rate;
    assert!((out.dps / expected - 1.0).abs() < 1e-9);
    assert_eq!(own.energy_trigger, None);
}

#[test]
fn ailment_energy_counts_ignite_damage_and_freeze_buildup_but_not_chill() {
    let data = data();
    let options = options(BOSS.0, &["100% chance to Ignite"]);
    let fireball = gem("FireballPlayer", "Fireball");
    let xml = meta_xml("MetaCastOnElementalAilmentPlayer", &[fireball.clone()], &[]);
    let build = parse_build(&build_xml(&[], &[fireball], OTHER)).unwrap();
    let source = pobr_build::calculate_with_data_session(&build, &data, &options).unwrap();
    let stats = source.energy_source_ailments();
    assert_eq!(stats.ignite_chance, 1.0);
    assert!(stats.ignite_damage > 0.0);
    let out = calculate(&data, &xml, &options);
    assert_rate(
        out.energy_per_trigger_event,
        20.0 * 1.57 * stats.ignite_damage / f64::from(enemy_ailment_threshold(ENEMY_LEVEL)),
    );
    assert!(out.skill_trigger_rate > 0.0);

    let cold = gem("FrostboltPlayer", "Frostbolt");
    let xml = meta_xml("MetaCastOnElementalAilmentPlayer", &[cold], &[]);
    let frozen = calculate(&data, &xml, &options);
    assert_eq!(frozen.energy_per_trigger_event, 314.0); // 10 × 20 × 1.57.
    assert!(frozen.skill_trigger_rate > 0.0);
    let chilled = calculate(
        &data,
        &xml,
        &DataOrchestratorOptions {
            extra_modifier_texts: vec![SPELL_DAMAGE.into(), "Cannot immobilise enemies".into()],
            ..options
        },
    );
    assert_eq!(chilled.skill_trigger_rate, 0.0);
    assert!(chilled.dps > 0.0);
    assert!(chilled.energy_trigger.as_ref().unwrap().at_own_rate);
    assert_eq!(chilled.energy_trigger_source, None);
}

#[test]
fn energy_meta_worked_builds_keep_reference_dps_and_expose_game_rates() {
    let data = data();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for (path, meta, spell, ordinal, inputs) in [
        (
            "examples/wasm-benchmark/builds/ninja-2026-10-11-sorceress-stormweaver-comet-f085a4410120",
            "MetaCastOnElementalAilmentPlayer",
            "CometPlayer",
            2,
            vec![],
        ),
        (
            "examples/reference-builds/tuned/builds/ninja-2026-10-13-druid-shaman-comet-dd7f512456b6",
            "MetaCastOnBlockPlayer",
            "IceNovaPlayer",
            3,
            vec![("energyIncomingHitRate", 8.0)],
        ),
        (
            "examples/wasm-benchmark/builds/ninja-2026-10-11-witch-blood-mage-impending-doom-add19135df93",
            "MetaSpellslingerPlayer",
            "LivingBombPlayer",
            2,
            vec![],
        ),
        (
            "examples/reference-builds/tuned/builds/ninja-2026-10-13-druid-shaman-comet-dd7f512456b6",
            "MetaCastOnMinionDeathPlayer",
            "CometPlayer",
            2,
            vec![("energyMinionDeathsPerSecond", 6.0)],
        ),
    ] {
        let code = std::fs::read_to_string(root.join(path).join("code.txt")).unwrap();
        let mut build = parse_build(&pobr_build::decode_pob_code(&code).unwrap()).unwrap();
        let group = build
            .socket_groups
            .iter()
            .position(|g| {
                g.gem_skills.iter().any(|gem| gem.skill_id == meta)
                    && g.gem_skills.iter().any(|gem| gem.skill_id == spell)
            })
            .unwrap();
        build.main_socket_group = Some(group + 1);
        build.socket_groups[group].main_active_skill = Some(ordinal);
        for (name, value) in inputs {
            build.config.raw_inputs.values.insert(
                name.into(),
                pobr_core::rules::config_interpreter::ConfigInputValue::Number(value),
            );
        }
        assert_eq!(
            pobr_build::resolve_main_skill_selection(&build, &data)
                .unwrap()
                .2,
            spell
        );
        let options = DataOrchestratorOptions::against_reference_enemy();
        let session = pobr_build::calculate_with_data_session(&build, &data, &options).unwrap();
        let game = session.output();
        let reference =
            calculate_with_data(&build, &data, &options.clone().on_reference_basis()).unwrap();
        let own = calculate_with_data(
            &build,
            &data,
            &DataOrchestratorOptions {
                energy_triggers: false,
                // Own-use comparison also omits the exporter's missing charm rule.
                exporter_absent_rules: false,
                ..options
            },
        )
        .unwrap();
        assert!(game.energy_trigger.is_some(), "{meta}");
        assert!(!game.energy_trigger.as_ref().unwrap().at_own_rate, "{meta}");
        assert!(
            game.skill_trigger_rate > 0.0,
            "{meta}: {:?}",
            game.energy_trigger
        );
        assert!(game.dps != reference.dps, "{meta}");
        assert_eq!(reference.dps, own.dps, "{meta}");
        let expected_reference = match meta {
            "MetaCastOnElementalAilmentPlayer" => 419_890.254_016_925,
            "MetaCastOnBlockPlayer" => 102_277.722_856_159,
            "MetaSpellslingerPlayer" => 173_633.921_810_672,
            "MetaCastOnMinionDeathPlayer" => 522_250.578_473_133,
            _ => unreachable!(),
        };
        assert!(
            (reference.dps / expected_reference - 1.0).abs() < 1e-9,
            "{meta}: {}",
            reference.dps
        );
        assert_eq!(reference.energy_trigger, None);
        if matches!(
            meta,
            "MetaCastOnBlockPlayer" | "MetaCastOnMinionDeathPlayer"
        ) {
            for key in [
                "energyIncomingHitRate",
                "enemySpeed",
                "energyMinionDeathsPerSecond",
            ] {
                build.config.raw_inputs.values.remove(key);
            }
            let unset = calculate_with_data(
                &build,
                &data,
                &DataOrchestratorOptions::against_reference_enemy(),
            )
            .unwrap();
            assert!(unset.energy_trigger.as_ref().unwrap().at_own_rate, "{meta}");
            assert_eq!(unset.energy_trigger_source, None, "{meta}");
            assert_eq!(unset.dps, reference.dps, "{meta}");
        }
        assert!(session.derived_detail("SkillTriggerRate").is_some());
        let steps = session.stat_formula("TotalDPS").expect("trigger DPS steps");
        assert_eq!(steps.value, game.dps);
        assert!(
            steps
                .steps
                .iter()
                .any(|step| step.op == pobr_core::calc::FormulaOp::EnergyTrigger)
        );
        eprintln!(
            "{meta} {spell}: game TotalDPS={} reference TotalDPS={}, maximum={}, energy/event={}, rate={}",
            game.dps,
            reference.dps,
            game.energy_trigger_maximum,
            game.energy_per_trigger_event,
            game.skill_trigger_rate
        );
    }
}

#[test]
fn meta_gems_keep_their_own_rate_and_explain_missing_energy_sources() {
    let data = data();
    for meta in [
        "MetaCastOnCritPlayer",
        "MetaCastOnElementalAilmentPlayer",
        "MetaCastOnBlockPlayer",
        "MetaSpellslingerPlayer",
        "MetaCastOnMinionDeathPlayer",
        "MetaElementalInvocationPlayer",
    ] {
        let xml = meta_xml(meta, &[], &[]).replace("CometPlayer", "FireballPlayer");
        let build = parse_build(&xml).unwrap();
        let session =
            pobr_build::calculate_with_data_session(&build, &data, &options(BOSS.0, &[])).unwrap();
        let game = session.output();
        let own = calculate(&data, &xml, &options(BOSS.0, &[]).on_reference_basis());
        assert!(own.dps > 0.0, "{meta}");
        assert_eq!(own.energy_trigger, None, "{meta}");
        assert_eq!(game.dps, own.dps, "{meta}");
        assert_eq!(game.ignite_dps, own.ignite_dps, "{meta}");
        assert_eq!(game.combined_dps, own.combined_dps, "{meta}");
        assert_eq!(
            game.effective_action_rate, own.effective_action_rate,
            "{meta}"
        );
        let terms = game.energy_trigger.as_ref().unwrap();
        assert!(terms.at_own_rate, "{meta}");
        assert_eq!(terms.source, None, "{meta}");
        assert_eq!(terms.rate_multiplier, 1.0, "{meta}");
        for stat in ["SkillTriggerRate", "TriggerRateCap"] {
            let detail = session.derived_detail(stat).unwrap();
            assert_eq!(detail.formula, "0 * EnergyAtOwnRate", "{meta}: {stat}");
            assert!(
                detail
                    .context
                    .iter()
                    .any(|(id, value)| Some(id.as_str()) == terms.missing_source && *value == 1.0)
            );
        }
        let detail = session.derived_detail("EffectiveActionRate").unwrap();
        assert_eq!(
            detail.inputs,
            vec![("ActionRate".into(), own.action_rate)],
            "{meta}"
        );
    }
}

#[test]
fn explicit_zero_invocation_use_stops_even_without_an_energy_source() {
    let data = data();
    for meta in ["MetaSpellslingerPlayer", "MetaElementalInvocationPlayer"] {
        let xml = meta_xml(meta, &[], &[("energyInvocationUsesPerSecond", 0.0)])
            .replace("CometPlayer", "FireballPlayer");
        let out = calculate(&data, &xml, &options(BOSS.0, &[]));
        assert_eq!(out.dps, 0.0, "{meta}");
        assert_eq!(out.ignite_dps, 0.0, "{meta}");
        assert_eq!(out.combined_dps, 0.0, "{meta}");
        assert!(!out.energy_trigger.as_ref().unwrap().at_own_rate, "{meta}");
    }
}

#[test]
fn manual_invocation_cadence_caps_energy_throughput() {
    let data = data();
    let xml = meta_xml(
        "MetaSpellslingerPlayer",
        &[spark()],
        &[("energyInvocationUsesPerSecond", 0.1)],
    );
    let out = calculate(
        &data,
        &xml,
        &options(BOSS.0, &["Meta Skills gain 10000% increased Energy"]),
    );
    assert_eq!(out.energy_trigger_maximum, 500.0);
    assert_eq!(out.trigger_rate_cap, 0.1); // one 300-Energy cycle per activation fits in 500.
    assert_eq!(out.skill_trigger_rate, 0.1);
    assert!(out.energy_trigger.as_ref().unwrap().energy_limited_rate > 0.1);
}
