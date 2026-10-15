//! Agreement with the reference values a build code carries, for any directory
//! of build codes.
//!
//! `ninja_parity` gates a fixed corpus whose PoB2 values are re-recorded with
//! the pinned vendor. That corpus ages: its builds predate the items and skills
//! of later leagues. This report needs no recorded values — it reads the
//! `<PlayerStat>` values each code was exported with, and its `<MinionStat>`
//! values for the main skill's minion — so it runs on freshly fetched builds:
//!
//! ```text
//! mise run accuracy                 # every committed set, totals only
//! mise run accuracy <dir of */code.txt>   # one set, with each build's differences
//! ```
//!
//! The exporting tool's version is unknown and its numbers are a reference,
//! not the game's: a row that differs is a lead to investigate, with
//! `mise run diagnose`, `mise run compare` (the pinned PoB2, stat by stat) and
//! the game's own modifier text as the next steps.
//!
//! The committed sets have different jobs (see `examples/reference-builds`):
//! differences are investigated and fixed on the worked sets, and the holdout
//! is only ever measured, so its total says how PoBR does on builds nobody
//! has worked on.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use pobr_build::{
    BuildData, DataOrchestratorOptions, ImportedBuild, ReferenceRow, calculate_with_data_session,
    compare_build_to_reference, damage_row, import_build_from_code,
};
use pobr_gamedata::GameData;

const TOL: f64 = 0.05;

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

/// Build directories (`*/code.txt`) under `root`, by name.
fn builds_in(root: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(root)
        .unwrap_or_else(|e| panic!("read {}: {e}", root.display()))
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.join("code.txt").is_file())
        .collect();
    dirs.sort();
    dirs
}

fn import(dir: &Path) -> ImportedBuild {
    let code = std::fs::read_to_string(dir.join("code.txt")).expect("read code.txt");
    import_build_from_code(&code)
        .unwrap_or_else(|e| panic!("{}: import failed: {e}", dir.display()))
}

/// Every committed build code keeps all the gear it uses. An item block the
/// parser rejects leaves its slot empty, and every number that follows is for
/// a different character.
#[test]
fn committed_builds_import_without_missing_items() {
    let fixtures = ["examples/demo-bd-test/builds"];
    for corpus in fixtures.into_iter().chain(BASELINES.map(|set| set.corpus)) {
        let dirs = builds_in(&repo_path(corpus));
        assert!(!dirs.is_empty(), "{corpus}: no builds");
        for dir in dirs {
            let missing: Vec<String> = import(&dir)
                .report
                .missing_items()
                .map(|item| format!("{} in {:?}: {}", item.name, item.used_in, item.reason))
                .collect();
            assert!(missing.is_empty(), "{}: {missing:?}", dir.display());
        }
    }
}

#[test]
fn build_codes_carry_their_reference_stats() {
    for set in BASELINES {
        for dir in builds_in(&repo_path(set.corpus)) {
            let stats = import(&dir).report.reference_stats;
            for key in ["Life", "TotalDPS", "FireResist"] {
                assert!(stats.contains_key(key), "{}: no {key}", dir.display());
            }
        }
    }
}

/// How a corpus compares with the reference its codes carry.
#[derive(Default)]
struct Agreement {
    /// Builds that carry a reference and calculated.
    builds: usize,
    /// Of those, the builds whose damage agrees ([`damage_row`]: the
    /// minion's `TotalDPS` when the main skill's minion has damage).
    dps_builds: usize,
    rows: usize,
    agreeing_rows: usize,
    /// stat id -> (agreeing builds, compared builds)
    per_stat: BTreeMap<String, (usize, usize)>,
}

/// Compares every build under `root`; `verbose` prints each build's misses.
fn corpus_agreement(root: &Path, verbose: bool) -> Agreement {
    let data =
        BuildData::load(&GameData::new(pobr_gamedata::current_data_dir())).expect("load BuildData");
    // The same convention as `ninja_parity`: the conditions the references
    // were exported under.
    let options = DataOrchestratorOptions::reference_conditions();

    let mut total = Agreement::default();
    for dir in builds_in(root) {
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        let ImportedBuild { build, report } = import(&dir);
        if report.reference_stats.is_empty() {
            if verbose {
                eprintln!("\n##### {name} :: no reference stats in the code #####");
            }
            continue;
        }
        let session = match calculate_with_data_session(&build, &data, &options) {
            Ok(session) => session,
            Err(error) => {
                if verbose {
                    eprintln!("\n##### {name} :: calculation failed: {error} #####");
                }
                continue;
            }
        };
        let rows = compare_build_to_reference(&build, &data, session.output(), &report);
        let misses: Vec<&ReferenceRow> = rows.iter().filter(|row| !row.agrees(TOL)).collect();
        total.builds += 1;
        total.rows += rows.len();
        total.agreeing_rows += rows.len() - misses.len();
        for row in &rows {
            let entry = total.per_stat.entry(row.id.clone()).or_default();
            entry.0 += usize::from(row.agrees(TOL));
            entry.1 += 1;
        }
        total.dps_builds += usize::from(damage_row(&rows).is_some_and(|row| row.agrees(TOL)));
        if !verbose {
            continue;
        }

        eprintln!(
            "\n##### {name} :: {}/{} agree, {} unsupported lines #####",
            rows.len() - misses.len(),
            rows.len(),
            session.unsupported_modifier_texts().len(),
        );
        for item in report.missing_items() {
            eprintln!(
                "  ! missing item {:?} ({}): {}",
                item.used_in, item.name, item.reason
            );
        }
        for row in misses {
            let ratio = row
                .ratio()
                .map_or_else(|| "—".to_string(), |ratio| format!("{ratio:.2}x"));
            eprintln!(
                "    {:<28}{:>16.2}{:>16.2}{:>9}",
                row.id, row.pobr, row.reference, ratio
            );
        }
    }
    total
}

/// A committed set of build codes and its agreement when last raised: builds
/// whose `TotalDPS` agrees, and agreeing rows.
///
/// Raise both when a change improves them; never lower them to make a change
/// pass. The reference is whatever the exporting tool wrote into each code, so
/// this guards PoBR against losing ground, not against being wrong: a build
/// can agree for the wrong reason, and a data version change can move the
/// numbers for a good one (then re-record them, as for `ninja_parity`).
///
/// A reference can also be shown to be wrong: the game's own text or data
/// says otherwise. Then the engine follows the game, and the number drops by
/// the rows that agreed with the mistake. Record that next to the number:
/// which builds, which rule, and what shows the reference is the one at fault.
#[derive(Clone, Copy)]
struct Baseline {
    corpus: &'static str,
    dps_builds: usize,
    agreeing_rows: usize,
}

const BASELINES: [Baseline; 3] = [
    // Was 7 and 357. These codes were exported on 2026-10-11 by a tool that
    // left out "Idols socketed in this item gain the benefits of their Bonded
    // modifiers" (the Fox Idol): the references of the Gemling Legionnaire
    // (TotalDPS and 3 more rows) and the Martial Artist (4 resistance and
    // maximum hit rows, while its TotalEHP came to agree) are for a character
    // without those modifiers. The Martial Artist's reference is short by
    // exactly its Idols' +12% Cold and +8% Chaos Resistance. The codes of the
    // other two sets, exported two days later, include them, and both sets
    // gained rows from the same change; the pinned PoB2 applies them to these
    // two builds as well.
    //
    // Then 350, 886 and 948 rows. The Lament, Portent and Absent Amulets grant
    // a skill without reservation (`ItemInherentSkills.NoReservation`). The
    // exporting tool lists that skill a second time as a gem and reserves its
    // 30, 60 or 100 Spirit, which leaves most of these characters with less
    // than no Spirit: the references of the six benchmark builds that wear one
    // are -66 to -100, and without the second count they are 0 to 7
    // (`amulet_granted_skills_do_not_overdraw_spirit`). `SpiritUnreserved` is
    // the only row the rule can move; it stopped agreeing on 4, 6 and 6 builds.
    //
    // Then 6 builds and 346 rows. Every minion the calculation creates is in
    // the character's Presence, a Companion with no limit stat included, such
    // as the Rhoa of Rhoa Mount (skill types `Minion`, `Companion`). The Ice
    // Shot of the Chaos Inoculation Deadeye has Minion Pact II, "Supported
    // Skills remove 150% of your maximum Life from a nearby Minion on use to
    // deal 35% more damage": 1.5 of its Rhoa's 4,182 Life. Its reference
    // leaves the 35% out of TotalDPS and CombinedDPS, and out of TotalDotDPS
    // and IgniteDPS, which follow the hit twice (about 1.35 squared): the
    // exporting tool counts only the minions of skills that name a limit stat.
    //
    // Then 349 rows. Temporalis's "Skills have -2.00 seconds to Cooldown"
    // comes off a skill's cooldown before Cooldown Recovery Rate, down to 0.1
    // seconds: the Oracle's Lightning Bolt (0.5 seconds) has 0.1, as its
    // reference does, where the line was read by nothing (0.27).
    //
    // Then 448 rows: the sidebar's attributes, Rage, resistance over cap,
    // unreserved percentages, pre-effective critical chance, stored uses,
    // skill DoT, crossbow timing, culling and projectile speed became
    // display stats, so the reference's rows for them are compared too
    // (new rows, not changed calculations).
    Baseline {
        corpus: "examples/wasm-benchmark/builds",
        dps_builds: 5,
        agreeing_rows: 448,
    },
    // Was 22 builds and 1024 rows. A code whose main skill summons a minion
    // also carries `<MinionStat>` values for that minion, which were not
    // compared: such a build counted as agreeing on damage because the
    // character's TotalDPS is 0 on both sides. The minion's TotalDPS, Life
    // and Energy Shield are now compared with the first minion the main skill
    // summoned, and its TotalDPS decides whether the build's damage agrees.
    // PoBR computes a minion's damage with the minimal pipeline, without the
    // minion's own skills: the Spirit Walker's companion has 463 DPS against
    // 1,280,037, the Disciple of Varashta's 0 against 206,838, and neither's
    // Life agrees. Their Energy Shield (0) adds 2 rows. The Spirit Walker's
    // main skill is a tamed beast's, whose monster PoBR did not create
    // before; its rows are for the beast the gem chose.
    //
    // Then 21 builds and 1029 rows. A minion attacks with its own skill (the
    // one `skillMinionSkill` picks, the first by default), with the skill's
    // flags, its stats and the group's supports, the game's ally damage
    // table, and its weapon's critical chance. The Spirit Walker's tamed beast
    // also has the beast's hidden level scaling, the share of the character's
    // main hand weapon "Companions gain", the auras "Allies in your Presence
    // have", and "Gain #% of Physical Damage as Chaos Damage": 1,282,913
    // against 1,280,037. The Disciple of Varashta's minion casts a spell,
    // which is not modelled yet (0 against 206,838).
    //
    // Then 1035 rows. A support's buff is a buff of each skill it supports,
    // a Companion's named after its monster, unless its payload names the
    // buff (Refraction's "Refractive Plating"); and each group's summoning
    // skill creates its own minions. The Spirit Walker has Loyalty or
    // Romira's Requital on eight differently named Companions, "10% of
    // Damage from Hits is taken from your Damageable Companion's Life before
    // you" each: 80% instead of 20%, before 168,405 Life of fourteen
    // Companions. Its TotalEHP and five maximum hits agree.
    //
    // Then 22 builds and 1036 rows. A minion casting a spell deals the
    // spell's own damage, its stat set resolved at the minion's level with
    // the game's effectiveness growth, once per cast time and within its
    // cooldown; a Command skill sets `CommandableSkill`, which Bidding's
    // "30% more Damage with Command Skills" and the tree's lines for Command
    // Skills wait for. The Disciple of Varashta's Water Djinn: 206,838 against
    // 206,838.
    //
    // Then 1037 rows. A buff, aura, curse or debuff is the one of its skill's
    // level, with the levels the group's supports grant it, and Uhtred's
    // supports grant theirs by how many other supports modify the skill
    // ("+3 to level of Supported Skills if no other Supports are modifying
    // them"). The Abyssal Lich's ChaosMaxHit.
    //
    // Then 36 builds and 2022 rows: the holdout set of 2026-10-13 joined, at
    // the 14 builds and 985 rows it was measured at, once the next holdout
    // was fetched. What was found on those builds without diagnosing them
    // stays true here: the Ritualist whose main skill is Purity of Fire has
    // "Critical Spell Damage Bonus" lines that only spells take (its
    // `CritMultiplier` reference applies them to the aura); the Gemling
    // Legionnaire's Twister ignites against the lowest elemental resistance
    // ("Enemies in your Presence Resist Elemental Damage based on their
    // Lowest Resistance"), 1.21 times the reference that keeps it to hits;
    // the Blood Mage's (Arc) CombinedDPS is 1.057 times the reference with
    // the Spectre it chose.
    //
    // Then 41 builds and 2041 rows: a hand's attacks read the hand's own
    // Accuracy ("1% increased Attack Speed per 400 Accuracy Rating"); Spell
    // Echo repeats a spell under its cooldown; Barrage repeats once more per
    // Frenzy Charge consumed; a support that adds a cooldown (Breachlord's
    // Amalgam) gives the skill its cooldown and stored uses; Gigantic
    // companions, "Companions gain your Strength", Pain Offering's aura on
    // minions with Sacrificial Offering's increased Buff effect, and a
    // companion's share of Deflected Hits.
    //
    // Then 43 builds and 2054 rows: an ailment that deals its damage faster
    // ends sooner, so fewer instances are active, and "increased duration of
    // Ailments you inflict" counts; Bringer of Flame's "All Damage from you
    // and Allies in your Presence contributes to … Ignite" is also the
    // character's; Uhtred's Omen counts support gems, not a meta gem's
    // trigger; "Skills from Corrupted Gems" asks whether the main skill's gem
    // is corrupted; Embitter gains every extra damage as Cold; a gem's
    // quality and Projectile Acceleration add Projectile Speed, which
    // Projectile Acceleration III adds to projectile damage.
    //
    // Then 2062 rows: a rule's keyword is kept ("Damage with Warcries" is no
    // longer every skill's damage) and a skill has the keywords of its
    // types; Smith of Kitava's Fire Resistance also grants Cold and
    // Lightning; Refraction III's banner applies exposure per 1000 Armour;
    // Culling Strike kills a unique at 5% of its Life; a configured Arcane
    // Surge applies in combat; Fire Spell on Hit's triggered spells cost no
    // Mana.
    //
    // Then 2067 rows: an item's own "+N to maximum Runic Ward" and "Gain % of
    // maximum Life as Extra maximum Runic Ward" are Runic Ward; Renly's
    // Training replaces the shield's block chance; the Gemling's Motes count
    // a skill an item grants once; the character's auras reach its minions
    // (Discipline's Energy Shield on a companion); Decompose's cloud cannot
    // crit. Rows that went the other way, each against a reference the game
    // proves wrong: the Amazon (Rain of Arrows) has its body armour's "Gain
    // 5% of maximum Life as Extra maximum Runic Ward", which its reference
    // leaves out, so its four largest-hit rows now agree with the pinned PoB2
    // and not the reference; the Ritualist (Purity of Fire) and Stormweaver
    // (Frost Bomb) take Evergrasping Ring's Chaos from all of an enemy's
    // damage, not only from Physical, so their TotalEHP is lower; Decompose's
    // reference keeps a critical damage bonus for a cloud that cannot crit.
    //
    // Then 56 builds and 3055 rows: the holdout set of 2026-10-15 joined, at
    // the 13 builds and 988 rows it was measured at, once the next holdout
    // was fetched.
    //
    // Then 61 builds and 3063 rows: a jewel's "Allocates" opens the socket it
    // names; "increased bonuses gained from Equipped Rings and Amulets"
    // scales their gem levels; a group bound to the other weapon set takes
    // its gem levels from that set's weapons; "You have Unholy Might" grants
    // the buff; a weapon's Accuracy Rating is its own attacks' (Shield Wall
    // attacks with the shield); "Spells which cost Life" asks for a Life
    // cost; "Defend against Hits as though you had 1% more Armour per 1%
    // current Energy Shield"; Oil Barrage's Oil applies Exposure. Rows that
    // went the other way, against a reference the game proves wrong: the
    // Titan (His Scattering Calamity, 686e) casts its main skill from the
    // staff of weapon set 1 while the item set holds set 2. The skill is used
    // holding that staff, so the calculation now uses set 1's weapons and
    // passives, as the pinned PoB2 does; the reference, exported with set 2,
    // gives its ActionRate, critical chance and bonus, Mana and EHP with the
    // other staff (nine rows, now agreeing with the pinned PoB2).
    //
    // Then 58 builds and 3049 rows. Gained: a line about "this Weapon"
    // ("Attacks with this Weapon Penetrate 15% Elemental Resistances") takes
    // the hand of the weapon's slot; "Is the enemy Exposed to Fire?" needs a
    // character that inflicts it (the Martial Artist, Flicker Strike, has no
    // source); "Damage with Maces" is for attacks with the mace, not Shield
    // Wall's; Styrn's Ferocity Crushes the enemy. Lost, against a reference
    // the game's text proves wrong: the Lich's Crystalline Phylactery gives
    // "100% increased Effect of bonuses gained from Socketed Jewel" and is
    // not a "Jewel Socket Passive Skill", so its jewel counts twice and The
    // Adorned leaves it alone. The reference, as the pinned PoB2, reads
    // neither: Abyssal Lich Twister 4028 and 9d1c, Lich Spark ce24 and 0a17
    // (4 builds' damage, 20 rows).
    //
    // Then 3902 rows: the sidebar's attributes, Rage, resistance over cap,
    // unreserved percentages, pre-effective critical chance, stored uses,
    // skill DoT, crossbow timing, culling and projectile speed became
    // display stats, so the reference's rows for them are compared too
    // (new rows, not changed calculations).
    Baseline {
        corpus: "examples/reference-builds/tuned/builds",
        dps_builds: 58,
        agreeing_rows: 3902,
    },
    // Measured, never worked on: do not diagnose these builds to choose a fix.
    //
    // Fetched 2026-10-15 from poe.ninja's snapshot of 2026-10-15, the last
    // character of each ascendancy not already committed
    // (`mise run builds:fetch examples/reference-builds/holdout --pick last`),
    // and measured before any change: 19 builds and 1008 rows. (The holdout
    // before it, fetched the day before and measured at 11 builds and 971
    // rows, is part of the tuned set now.)
    //
    // Then 18 builds and 1003 rows: two of its builds socket a jewel in the
    // Lich's Crystalline Phylactery, which doubles the jewel; their
    // references don't (see the tuned set's note). Measured only, by
    // switching that one rule back.
    //
    // Then 1291 rows: the sidebar's attributes, Rage, resistance over cap,
    // unreserved percentages, pre-effective critical chance, stored uses,
    // skill DoT, crossbow timing, culling and projectile speed became
    // display stats, so the reference's rows for them are compared too
    // (new rows, not changed calculations).
    //
    // Then 1293 rows: unique lines that were not parsed now compute by their
    // game stats (Blackheart's Armour against Chaos hits, Vigilant View's and
    // Goregirdle's caps, Matsya's halved Spirit reservation and others).
    Baseline {
        corpus: "examples/reference-builds/holdout/builds",
        dps_builds: 18,
        agreeing_rows: 1293,
    },
];

/// What the current-league builds gained stays gained. `ninja_parity` cannot
/// see them: its corpus predates their items and skills.
#[test]
fn reference_agreement_does_not_regress() {
    for set in BASELINES {
        let total = corpus_agreement(&repo_path(set.corpus), false);
        assert!(
            total.dps_builds >= set.dps_builds,
            "{}: TotalDPS agrees on {} builds, was {}",
            set.corpus,
            total.dps_builds,
            set.dps_builds
        );
        assert!(
            total.agreeing_rows >= set.agreeing_rows,
            "{}: {} of {} rows agree, was {}",
            set.corpus,
            total.agreeing_rows,
            total.rows,
            set.agreeing_rows
        );
    }
}

/// A character cannot reserve more Spirit than it has. These builds wear a
/// Lament, Portent or Absent Amulet, whose granted skill reserves nothing. The
/// references in their codes are 66 to 100 Spirit below zero: the exporting
/// tool counts the granted skill a second time, as a gem.
#[test]
fn amulet_granted_skills_do_not_overdraw_spirit() {
    const BUILDS: [&str; 6] = [
        "huntress-amazon-ice-shot",
        "mercenary-gemling-legionnaire-comet",
        "monk-martial-artist-ice-shot",
        "sorceress-stormweaver-comet",
        "warrior-titan-his-scattering-calamity",
        "witch-blood-mage-impending-doom",
    ];
    let data =
        BuildData::load(&GameData::new(pobr_gamedata::current_data_dir())).expect("load BuildData");
    let options = DataOrchestratorOptions::reference_conditions();
    let mut checked = 0;
    for dir in builds_in(&repo_path("examples/wasm-benchmark/builds")) {
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        if !BUILDS.iter().any(|build| name.contains(build)) {
            continue;
        }
        let ImportedBuild { build, report } = import(&dir);
        let reference = report.reference_stats["SpiritUnreserved"];
        assert!(reference <= -66.0, "{name}: reference {reference}");
        let unreserved = calculate_with_data_session(&build, &data, &options)
            .unwrap_or_else(|e| panic!("{name}: {e}"))
            .output()
            .spirit_unreserved;
        assert!(unreserved >= 0.0, "{name}: {unreserved} Spirit unreserved");
        checked += 1;
    }
    assert_eq!(checked, BUILDS.len());
}

fn percent(part: usize, whole: usize) -> f64 {
    100.0 * part as f64 / whole.max(1) as f64
}

/// One set with each build's differences (`POBR_REFERENCE_BUILDS`), or the
/// totals of every committed set.
#[test]
#[ignore = "reference dashboard; run explicitly with --ignored --nocapture"]
fn reference_report() {
    let Ok(root) = std::env::var("POBR_REFERENCE_BUILDS").map(PathBuf::from) else {
        eprintln!(
            "\n================ REFERENCE SUMMARY (tol {:.0}%) ================",
            TOL * 100.0
        );
        eprintln!(
            "{:<44}{:>8}{:>14}{:>20}",
            "set", "builds", "TotalDPS", "rows"
        );
        for set in BASELINES {
            let total = corpus_agreement(&repo_path(set.corpus), false);
            eprintln!(
                "{:<44}{:>8}{:>8} {:>4.0}%{:>11}/{:<5}{:>5.1}%",
                set.corpus,
                total.builds,
                total.dps_builds,
                percent(total.dps_builds, total.builds),
                total.agreeing_rows,
                total.rows,
                percent(total.agreeing_rows, total.rows)
            );
        }
        return;
    };
    let total = corpus_agreement(&root, true);

    eprintln!(
        "\n================ REFERENCE SUMMARY (tol {:.0}%) ================",
        TOL * 100.0
    );
    eprintln!("corpus: {}", root.display());
    eprintln!(
        "builds: {}; TotalDPS agrees on {}",
        total.builds, total.dps_builds
    );
    eprintln!(
        "rows: {}/{} = {:.1}%",
        total.agreeing_rows,
        total.rows,
        percent(total.agreeing_rows, total.rows)
    );
    eprintln!("stats that disagree on at least one build (agree/compared):");
    let mut stats: Vec<_> = total
        .per_stat
        .into_iter()
        .filter(|(_, (agree, compared))| agree < compared)
        .collect();
    stats.sort_by_key(|(id, (agree, compared))| (*agree * 1000 / *compared, id.clone()));
    for (id, (agree, compared)) in stats {
        eprintln!("    {id:<28}{agree}/{compared}");
    }
}
