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
use pobr_gamedata::{GameData, data_version, repo_data_root};

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

fn data() -> BuildData {
    BuildData::load(&GameData::new(repo_data_root().join(data_version())))
        .expect("load repository data")
}

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
