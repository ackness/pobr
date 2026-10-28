//! Every passive node, allocated at once, shows which lines of the tree the calculation cannot read. Keystones are
//! left out because they exclude one another (Resolute Technique never crits). The lines the engine drops are a
//! snapshot of the active data version: a new one means a rule regressed or the tree changed, and one that is now
//! read must come out of the snapshot. `POBR_UPDATE_SNAPSHOTS=1` rewrites it.

use std::collections::BTreeSet;

use pobr_build::usage::{LineStatus, line_usage};
use pobr_build::{DataOrchestratorOptions, calculate_with_data_session, diagnose, parse_build};
use pobr_data::catalog::PassiveNodeKind;
use pobr_data::prelude::{ModName, ModType};

use crate::test_data as data;

const SNAPSHOT: &str = include_str!("../fixtures/passive_tree_unparsed.txt");
const SNAPSHOT_PATH: &str = "tests/fixtures/passive_tree_unparsed.txt";

fn dropped_lines() -> BTreeSet<String> {
    let data = data();
    let mut nodes: Vec<u32> = data
        .passive_nodes
        .iter()
        .filter(|(_, node)| node.kind != PassiveNodeKind::Keystone)
        .map(|(id, _)| *id)
        .collect();
    nodes.sort_unstable();
    let list = nodes
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let build = parse_build(&format!(
        r#"<PathOfBuilding2>
<Build level="100" className="Ranger" ascendClassName="Deadeye" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="IceShotPlayer" level="20" enabled="true"/></Skill></Skills>
<Tree activeSpec="1"><Spec nodes="{list}"/></Tree>
</PathOfBuilding2>"#
    ))
    .unwrap();
    let diagnosis = diagnose(&build, data, &DataOrchestratorOptions::default()).unwrap();
    let usage = line_usage(&diagnosis, &build, data);
    usage
        .lines
        .iter()
        .filter(|line| line.status == LineStatus::Unparsed)
        .filter_map(|line| {
            Some(format!(
                "{}\t{}",
                line.source_id.as_deref()?,
                line.text.as_deref()?
            ))
        })
        .collect()
}

#[test]
fn every_tree_line_the_engine_drops_is_known() {
    let dropped = dropped_lines();
    if std::env::var_os("POBR_UPDATE_SNAPSHOTS").is_some() {
        let text: String = dropped.iter().map(|line| format!("{line}\n")).collect();
        std::fs::write(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(SNAPSHOT_PATH),
            text,
        )
        .unwrap();
        return;
    }
    let known: BTreeSet<String> = SNAPSHOT.lines().map(str::to_owned).collect();
    let new: Vec<_> = dropped.difference(&known).collect();
    let fixed: Vec<_> = known.difference(&dropped).collect();
    assert!(
        new.is_empty(),
        "tree lines the calculation now drops ({}):\n{}",
        new.len(),
        new.iter()
            .take(40)
            .map(|l| l.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        fixed.is_empty(),
        "tree lines that are read now: remove them from {SNAPSHOT_PATH} (POBR_UPDATE_SNAPSHOTS=1) ({}):\n{}",
        fixed.len(),
        fixed
            .iter()
            .take(40)
            .map(|l| l.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
    // The snapshot names the node, so a line moving between nodes is a change too; the total is the headline.
    assert_eq!(dropped.len(), known.len());
}

/// A passive's "Grants Skill: <name>" line is read by the skill group of that skill. Without the group the line
/// stays unparsed: the grant is not in the calculation.
#[test]
fn a_granted_skill_line_is_read_by_the_skill_the_build_lists() {
    let data = data();
    // Node 5817 (Deadeye) grants Mirage Deadeye.
    let line = |groups: &str| {
        let build = parse_build(&format!(
            r#"<PathOfBuilding2>
<Build level="96" className="Ranger" ascendClassName="Deadeye" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="IceShotPlayer" level="20" enabled="true"/></Skill>{groups}</Skills>
<Tree activeSpec="1"><Spec nodes="5817"/></Tree>
</PathOfBuilding2>"#
        ))
        .unwrap();
        let diagnosis = diagnose(&build, data, &DataOrchestratorOptions::default()).unwrap();
        line_usage(&diagnosis, &build, data)
            .lines
            .into_iter()
            .find(|line| {
                line.text
                    .as_deref()
                    .is_some_and(|t| t.contains("Grants Skill"))
            })
            .expect("the node's line is in the ledger")
    };
    let absent = line("");
    assert_eq!(absent.status, LineStatus::Unparsed, "{absent:?}");
    let listed = line(
        r#"<Skill enabled="true"><Gem skillId="MetaMirageDeadeyePlayer" level="20" enabled="true"/></Skill>"#,
    );
    assert_ne!(listed.status, LineStatus::Unparsed, "{listed:?}");
    let disabled = line(
        r#"<Skill enabled="false"><Gem skillId="MetaMirageDeadeyePlayer" level="20" enabled="true"/></Skill>"#,
    );
    assert_eq!(disabled.status, LineStatus::Unparsed, "{disabled:?}");
}

/// Passive 11526: "Arrows gain Critical Hit Chance as they travel farther, up to 40% increased Critical Hit
/// Chance after 7 metres". The bonus follows the configured enemy distance (tenths of a metre) up to 70. Without a
/// configured distance the effective mode uses the default 20, like every distance ramp. The tree words the stat
/// over two lines.
#[test]
fn arrow_critical_chance_grows_with_the_configured_distance() {
    let data = data();
    let inc = |node: bool, distance: Option<f64>| -> f64 {
        let config = distance.map_or_else(String::new, |distance| {
            format!(r#"<Input name="enemyDistance" number="{distance}"/>"#)
        });
        let tree = if node {
            r#"<Tree activeSpec="1"><Spec nodes="11526"/></Tree>"#
        } else {
            ""
        };
        let build = parse_build(&format!(
            r#"<PathOfBuilding2>
<Build level="96" className="Ranger" ascendClassName="Deadeye" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="IceShotPlayer" level="20" enabled="true"/></Skill></Skills>
<Items activeItemSet="1"><Item id="1">Rarity: NORMAL
Crude Bow
Implicits: 0
</Item><ItemSet id="1"><Slot name="Weapon 1" itemId="1"/></ItemSet></Items>
{tree}<Config>{config}</Config></PathOfBuilding2>"#
        ))
        .unwrap();
        let options = DataOrchestratorOptions {
            mode_effective: true,
            ..Default::default()
        };
        let session = calculate_with_data_session(&build, data, &options).unwrap();
        session.mod_db().sum(
            ModType::Inc,
            session.config(),
            &[ModName::from("CriticalStrikeChance")],
        )
    };
    for (distance, gained) in [
        (None, 40.0 * 20.0 / 70.0),
        (Some(0.0), 0.0),
        (Some(35.0), 20.0),
        (Some(70.0), 40.0),
        (Some(100.0), 40.0),
    ] {
        let delta = inc(true, distance) - inc(false, distance);
        assert!(
            (delta - gained).abs() < 1e-9,
            "distance {distance:?}: gained {delta}, expected {gained}"
        );
    }
}

/// "Projectiles that have Forked deal 30% less Damage" (Fork) and "Projectiles that have Pierced deal 20% less
/// Damage" (Pierce I; the other Pierce sources are increases) read the configured "# of times Skill has Forked/Pierced", at most what the skill can do. With
/// no count the hit is the first one and neither applies. They used to be dropped with a "conditional numerical
/// effect not modeled" diagnostic, so setting the count changed nothing.
#[test]
fn forked_and_pierced_projectile_damage_follows_the_configured_count() {
    let data = data();
    let more = |support: &str, input: &str, count: Option<u32>| -> f64 {
        let config = count.map_or_else(String::new, |count| {
            format!(r#"<Input name="{input}" number="{count}"/>"#)
/// The Parry passives apply to the Parry skill's own hit and to no other skill: the Area of Effect of its hit and
/// its Heavy Stun buildup are matched by the skill's name.
#[test]
fn parry_passives_apply_to_the_parry_skill_only() {
    let data = data();
    let sum = |skill: &str, line: &str, name: &str| -> f64 {
        let build = parse_build(&format!(
            r#"<PathOfBuilding2>
<Build level="90" className="Warrior" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="{skill}" level="20" enabled="true"/></Skill></Skills>
<Items activeItemSet="1"><Item id="1">Rarity: NORMAL
Leather Buckler
Implicits: 0
</Item><ItemSet id="1"><Slot name="Weapon 2" itemId="1"/></ItemSet></Items>
</PathOfBuilding2>"#
        ))
        .unwrap();
        let options = DataOrchestratorOptions {
            mode_effective: true,
            extra_modifier_texts: vec![line.to_string()],
            ..Default::default()
        };
        let session = calculate_with_data_session(&build, data, &options).unwrap();
        session
            .mod_db()
            .sum(ModType::Inc, session.config(), &[ModName::from(name)])
    };
    for (line, name) in [
        ("20% increased Parry Hit Area of Effect", "AreaOfEffect"),
        (
            "Parry has 25% increased Stun Buildup",
            "EnemyHeavyStunBuildup",
        ),
    ] {
        let none = sum("ParryPlayer", "", name);
        let parry = sum("ParryPlayer", line, name);
        let other = sum("FireballPlayer", line, name);
        assert!(parry > none, "{line}: parry {parry} vs {none}");
        assert_eq!(
            other,
            sum("FireballPlayer", "", name),
            "{line} leaked to another skill"
        );
    }
}

/// "N% chance for Projectiles to Pierce Enemies within 3m distance of you" is a Pierce chance while the enemy is
/// within 3 metres: it applies at the default distance (20, tenths of a metre) and stops past 30.
#[test]
fn close_range_pierce_chance_follows_the_configured_distance() {
    let data = data();
    let chance = |distance: Option<f64>, with_line: bool| -> f64 {
        let config = distance.map_or_else(String::new, |distance| {
            format!(r#"<Input name="enemyDistance" number="{distance}"/>"#)
/// Passives that act against a monster type read the type you declare (`conditionEnemy<Type>`): without it the
/// calculation has no monster type, so the bonus stays off.
#[test]
fn monster_type_passives_follow_the_declared_type() {
    let data = data();
    let inc = |line: &str, declared: Option<&str>, name: &str| -> f64 {
        let config = declared.map_or_else(String::new, |input| {
            format!(r#"<Input name="{input}" boolean="true"/>"#)
        });
        let build = parse_build(&format!(
            r#"<PathOfBuilding2>
<Build level="96" className="Ranger" ascendClassName="Deadeye" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="IceShotPlayer" level="20" enabled="true"/><Gem skillId="{support}" level="1" enabled="true"/></Skill></Skills>
<Skills><Skill enabled="true"><Gem skillId="IceShotPlayer" level="20" enabled="true"/></Skill></Skills>
<Items activeItemSet="1"><Item id="1">Rarity: NORMAL
Crude Bow
Implicits: 0
</Item><ItemSet id="1"><Slot name="Weapon 1" itemId="1"/></ItemSet></Items>
<Config>{config}</Config></PathOfBuilding2>"#
        ))
        .unwrap();
        let options = DataOrchestratorOptions {
            mode_effective: true,
            extra_modifier_texts: vec![line.to_string()],
            ..Default::default()
        };
        let session = calculate_with_data_session(&build, data, &options).unwrap();
        // The enemy's modifiers (ailment duration, immobilisation buildup) live on the player's side as `Enemy*`.
        session
            .mod_db()
            .sum(ModType::Inc, session.config(), &[ModName::from(name)])
    };
    for (line, input, name, expected) in [
        (
            "30% increased damage against Undead Enemies",
            "conditionEnemyUndead",
            "Damage",
            30.0,
        ),
        (
            "50% increased Damage against Demons",
            "conditionEnemyDemon",
            "Damage",
            50.0,
        ),
        (
            "50% increased Critical Hit Chance against Humanoids",
            "conditionEnemyHumanoid",
            "CriticalStrikeChance",
            50.0,
        ),
        (
            "50% increased Immobilisation buildup against Constructs",
            "conditionEnemyConstruct",
            "EnemyImmobilisationBuildup",
            50.0,
        ),
        (
            "50% increased Duration of Ailments on Beasts",
            "conditionEnemyBeast",
            "EnemyAilmentDuration",
            50.0,
        ),
    ] {
        let none = inc("", None, name);
        assert_eq!(inc(line, None, name), none, "{line}: on without the type");
        assert_eq!(
            inc(line, Some("conditionEnemyDemon"), name) - none,
            if input == "conditionEnemyDemon" {
                expected
            } else {
                0.0
            },
            "{line}: another type"
        );
        let declared = inc(line, Some(input), name) - none;
        assert!(
            (declared - expected).abs() < 1e-9,
            "{line}: gained {declared}"
        );
    }
}

/// "N% increased Fire Damage per Endurance Charge consumed Recently" and the extra Fire gain scale with the
/// configured number of consumed charges; charge consumption itself is not simulated.
#[test]
fn consumed_endurance_charges_scale_the_passives_that_count_them() {
    let data = data();
    let sum = |count: Option<u32>, line: &str, name: &str, kind: ModType| -> f64 {
        let config = count.map_or_else(String::new, |count| {
            format!(
                r#"<Input name="multiplierEnduranceChargesConsumedRecently" number="{count}"/>"#
            )
        });
/// A count typed into a `multiplier<X>` input is counted once. The input lands in the multiplier table when the
/// build is read, and the effective mode's bridge for the option's tagged effect used to add it again (an input of
/// 4 read as 8), for every count option that carries a Combat or Effective tag.
#[test]
fn a_typed_count_is_not_counted_twice_in_the_effective_mode() {
    let data = data();
    let count = |input: &str, variable: &str, effective: bool| -> Option<f64> {
        let build = parse_build(&format!(
            r#"<PathOfBuilding2>
<Build level="96" className="Warrior" mainSocketGroup="1"/>
<Skills><Skill enabled="true"><Gem skillId="FireballPlayer" level="20" enabled="true"/></Skill></Skills>
<Config>{config}</Config></PathOfBuilding2>"#
        ))
        .unwrap();
        let options = DataOrchestratorOptions {
            mode_effective: true,
            extra_modifier_texts: vec![line.to_string()],
            ..Default::default()
        };
        let session = calculate_with_data_session(&build, data, &options).unwrap();
        session
            .mod_db()
            .more(session.config(), &[ModName::from("Damage")])
    };
    // Fork: 30% less damage once the projectile has forked, at most as many forks as the support gives.
    let plain = more("SupportForkPlayer", "skillForkCount", None);
    assert_eq!(more("SupportForkPlayer", "skillForkCount", Some(0)), plain);
    let forked = more("SupportForkPlayer", "skillForkCount", Some(1));
    assert!((forked / plain - 0.7).abs() < 1e-9, "{forked} / {plain}");
    assert_eq!(more("SupportForkPlayer", "skillForkCount", Some(5)), forked);
    // Without the support the skill cannot fork, so a configured count is not a forked projectile.
    assert_eq!(
        more("SupportPiercePlayer", "skillForkCount", Some(1)),
        more("SupportPiercePlayer", "skillForkCount", None)
    );
    // Pierce I: 20% less damage once the projectile has pierced an enemy.
    let pierce_plain = more("SupportPiercePlayer", "skillPierceCount", None);
    assert_eq!(
        more("SupportPiercePlayer", "skillPierceCount", Some(0)),
        pierce_plain
    );
    let pierced = more("SupportPiercePlayer", "skillPierceCount", Some(1));
    assert!(
        (pierced / pierce_plain - 0.8).abs() < 1e-9,
        "{pierced} / {pierce_plain}"
    );
            extra_modifier_texts: with_line
                .then(|| {
                    "25% chance for Projectiles to Pierce Enemies within 3m distance of you"
                        .to_string()
                })
                .into_iter()
                .collect(),
            ..Default::default()
        };
        let session = calculate_with_data_session(&build, data, &options).unwrap();
        session.mod_db().sum(
            ModType::Base,
            session.config(),
            &[ModName::from("PierceChance")],
        )
    };
    for (distance, gained) in [
        (None, 25.0),
        (Some(0.0), 25.0),
        (Some(30.0), 25.0),
        (Some(31.0), 0.0),
        (Some(70.0), 0.0),
    ] {
        let delta = chance(distance, true) - chance(distance, false);
        assert!(
            (delta - gained).abs() < 1e-9,
            "distance {distance:?}: gained {delta}, expected {gained}"
        );
            .sum(kind, session.config(), &[ModName::from(name)])
    };
    let inc = "3% increased Fire Damage per Endurance Charge consumed Recently";
    let gain = "Gain 2% of Damage as Extra Fire Damage per Endurance Charge consumed Recently";
    for count in [None, Some(0), Some(4)] {
        let n = f64::from(count.unwrap_or(0));
        let more_inc = sum(count, inc, "FireDamage", ModType::Inc)
            - sum(count, "", "FireDamage", ModType::Inc);
        assert!((more_inc - 3.0 * n).abs() < 1e-9, "{count:?}: {more_inc}");
        let more_gain = sum(count, gain, "DamageGainAsFire", ModType::Base)
            - sum(count, "", "DamageGainAsFire", ModType::Base);
        assert!((more_gain - 2.0 * n).abs() < 1e-9, "{count:?}: {more_gain}");
    }
<Config><Input name="{input}" number="4"/></Config></PathOfBuilding2>"#
        ))
        .unwrap();
        let options = DataOrchestratorOptions {
            mode_effective: effective,
            ..Default::default()
        };
        let session = calculate_with_data_session(&build, data, &options).unwrap();
        session.config().multiplier_if_set(variable)
    };
    // "# of Recently Applied Ailments" carries a Combat tag.
    for effective in [false, true] {
        assert_eq!(
            count(
                "multiplierAppliedAilmentsRecently",
                "AppliedAilmentsRecently",
                effective
            ),
            Some(4.0),
            "effective: {effective}"
        );
    }
    // Options whose input name is not `multiplier<X>` were never doubled, and still are not.
    assert_eq!(
        count(
            "multiplierEnduranceChargesConsumedRecently",
            "EnduranceChargesConsumedRecently",
            true
        ),
        Some(4.0)
    );
}
