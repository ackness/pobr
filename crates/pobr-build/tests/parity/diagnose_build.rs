//! One build, everything it said that the result does not reflect.
//!
//! ```text
//! mise run diagnose <build dir | code file | name fragment>
//! ```
//!
//! The first stop when a build's numbers look wrong. It needs no other tool:
//! it lists what the import dropped, where the result differs from the
//! reference values the build code carries, and every input the calculation
//! lost on the way (`pobr_build::diagnose`). Only when that list does not
//! explain a difference is a comparison with the pinned PoB2 the next step
//! (`mise run compare`, which runs `tools/pob2-oracle`).

use std::path::{Path, PathBuf};

use pobr_build::{
    BuildData, DataOrchestratorOptions, ImportedBuild, ModifierRef, compare_build_to_reference,
    diagnose, import_build_from_code,
};
use pobr_gamedata::GameData;

const TOL: f64 = 0.05;

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

/// The committed builds a name fragment is looked up in. The holdout set of
/// `examples/reference-builds` is left out on purpose: it is only measured.
const WORKED_CORPORA: [&str; 3] = [
    "examples/reference-builds/tuned/builds",
    "examples/wasm-benchmark/builds",
    "examples/demo-bd-test/builds",
];

/// The build code `target` names: a directory holding `code.txt`, a file
/// holding a code, or a fragment of a committed build's directory name.
fn build_code(target: &str) -> (String, String) {
    let path = PathBuf::from(target);
    let read = |file: &Path| {
        std::fs::read_to_string(file).unwrap_or_else(|e| panic!("read {}: {e}", file.display()))
    };
    if path.join("code.txt").is_file() {
        return (target.to_string(), read(&path.join("code.txt")));
    }
    if path.is_file() {
        return (target.to_string(), read(&path));
    }
    let mut matches: Vec<PathBuf> = WORKED_CORPORA
        .iter()
        .flat_map(|corpus| std::fs::read_dir(repo_path(corpus)).into_iter().flatten())
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|dir| dir.join("code.txt").is_file())
        .filter(|dir| {
            dir.file_name()
                .is_some_and(|name| name.to_string_lossy().contains(target))
        })
        .collect();
    matches.sort();
    match matches.as_slice() {
        [dir] => (
            dir.file_name().unwrap().to_string_lossy().to_string(),
            read(&dir.join("code.txt")),
        ),
        [] => panic!("POBR_DIAGNOSE={target}: no such directory, file or committed build"),
        many => panic!(
            "POBR_DIAGNOSE={target} matches {} builds: {:?}",
            many.len(),
            many.iter()
                .map(|dir| dir.file_name().unwrap().to_string_lossy().to_string())
                .collect::<Vec<_>>()
        ),
    }
}

/// Every committed build code, for the names any calculation reads.
fn committed_codes() -> Vec<String> {
    WORKED_CORPORA
        .iter()
        .flat_map(|corpus| std::fs::read_dir(repo_path(corpus)).into_iter().flatten())
        .filter_map(|entry| std::fs::read_to_string(entry.ok()?.path().join("code.txt")).ok())
        .collect()
}

/// Modifiers by name: `Name TYPE`, then each value and source.
fn print_modifiers<'a>(modifiers: impl Iterator<Item = &'a ModifierRef>) {
    let mut last = String::new();
    for modifier in modifiers {
        let head = format!("{} {:?}", modifier.name, modifier.mod_type);
        if head != last {
            eprintln!("    {head}");
            last = head;
        }
        eprintln!("        {:>10}  {}", modifier.value, modifier.source);
    }
}

/// Whether a modifier was written by a config input or default rather than by
/// something the character has.
fn from_config(modifier: &ModifierRef) -> bool {
    ["Config:", "EnemyConfig:", "GameConstant:"]
        .iter()
        .any(|kind| modifier.source.starts_with(kind))
}

/// Diagnosing a build is observing it: the numbers are those of a plain
/// calculation, for every committed build.
#[test]
fn diagnosis_does_not_change_the_result() {
    let data =
        BuildData::load(&GameData::new(pobr_gamedata::current_data_dir())).expect("load BuildData");
    let options = DataOrchestratorOptions::against_reference_enemy();
    let codes = committed_codes();
    assert!(codes.len() >= 20, "{} committed builds", codes.len());
    for code in codes {
        let build = import_build_from_code(&code).expect("import").build;
        let plain = pobr_build::calculate_with_data(&build, &data, &options).expect("calculate");
        let diagnosis = diagnose(&build, &data, &options).expect("diagnose");
        let name = format!(
            "{} {}",
            build.character.ascendancy_name, build.character.level
        );
        // Debug text, so a NaN output still compares equal to itself.
        assert_eq!(
            format!("{:?}", diagnosis.session.output()),
            format!("{plain:?}"),
            "{name}"
        );
        assert_eq!(
            diagnosis.unparsed_lines,
            diagnosis.session.unsupported_modifier_texts(),
            "{name}"
        );
        assert!(!diagnosis.read_names.is_empty(), "{name}");
    }
}

#[test]
#[ignore = "diagnostic report; run with POBR_DIAGNOSE=<build> and --ignored --nocapture"]
fn diagnose_build() {
    let target = std::env::var("POBR_DIAGNOSE")
        .expect("set POBR_DIAGNOSE to a build directory, a code file or a committed build's name");
    let (name, code) = build_code(&target);
    let ImportedBuild { build, report } =
        import_build_from_code(&code).unwrap_or_else(|e| panic!("{name}: import failed: {e}"));
    let data =
        BuildData::load(&GameData::new(pobr_gamedata::current_data_dir())).expect("load BuildData");
    // The build as the game plays it, against the enemy its reference values
    // were exported for: the inputs a calculation drops are asked of this one.
    let options = DataOrchestratorOptions::against_reference_enemy();
    let diagnosis = diagnose(&build, &data, &options)
        .unwrap_or_else(|e| panic!("{name}: calculation failed: {e}"));
    let output = diagnosis.session.output();

    eprintln!("\n################ {name} ################");
    eprintln!(
        "{} / {} level {}, data {}",
        build.character.class_name,
        build.character.ascendancy_name,
        build.character.level,
        pobr_gamedata::data_version(),
    );

    match &diagnosis.main_skill {
        Some(skill) => {
            let from = if skill.gem_id == skill.skill_id {
                String::new()
            } else {
                format!(", at the level of {}", skill.gem_id)
            };
            eprintln!(
                "main skill {}{from}: gem level {} + {} from items, passives and supports, quality {}, stat set {}",
                skill.skill_id,
                skill.gem_level,
                skill.bonus_levels,
                skill.quality,
                skill
                    .stat_set_index
                    .map_or_else(|| "default".to_string(), |index| index.to_string()),
            );
        }
        None => eprintln!("main skill: none resolves"),
    }
    if output.energy_trigger_maximum > 0.0 {
        match &output.energy_trigger_source {
            Some(source) => eprintln!(
                "triggered {:.3} times a second: its meta gem holds {:.0} Energy and {source} gives it {:.2} per critical hit",
                output.skill_trigger_rate,
                output.energy_trigger_maximum,
                output.energy_per_trigger_event,
            ),
            None => eprintln!(
                "in a meta gem that holds {:.0} Energy; no skill of the build fills it, so the spell is at its own rate",
                output.energy_trigger_maximum,
            ),
        }
    }

    eprintln!("\n== 1. Import: what the build code holds that the build does not");
    let mut import_losses = 0;
    for item in report.missing_items() {
        import_losses += 1;
        eprintln!(
            "    item not read, in {:?}: {} ({})",
            item.used_in, item.name, item.reason
        );
    }
    for item in &report.unmodelled_slots {
        import_losses += 1;
        eprintln!("    slot not modelled, {}: {}", item.slot, item.name);
    }
    if import_losses == 0 {
        eprintln!("    nothing");
    }

    eprintln!(
        "\n== 2. Result against the reference the code carries (tolerance {:.0}%)",
        TOL * 100.0
    );
    if report.reference_stats.is_empty() {
        eprintln!("    the code carries no reference stats");
    } else {
        // The reference counts a spell in a meta gem at its own rate of use;
        // the comparison is with the same question.
        let on_reference_basis =
            pobr_build::calculate_with_data(&build, &data, &options.clone().on_reference_basis())
                .unwrap_or_else(|e| panic!("{name}: calculation failed: {e}"));
        if output.energy_trigger_source.is_some() {
            eprintln!(
                "    (the triggered spell is counted at its own rate of use, as the reference counts it)"
            );
        }
        let rows = compare_build_to_reference(&build, &data, &on_reference_basis, &report);
        let misses: Vec<_> = rows.iter().filter(|row| !row.agrees(TOL)).collect();
        eprintln!(
            "    {} of {} stats agree",
            rows.len() - misses.len(),
            rows.len()
        );
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

    eprintln!(
        "\n== 3. Lines no rule parses ({})",
        diagnosis.unparsed_lines.len()
    );
    for line in &diagnosis.unparsed_lines {
        eprintln!("    {line}");
    }

    eprintln!(
        "\n== 4. Gem stats with a stat-map entry the engine rejected ({}; {} more have no entry)",
        diagnosis.unmapped_stats.len(),
        diagnosis.unknown_stats.len(),
    );
    for stat in &diagnosis.unmapped_stats {
        eprintln!("    {:<40} {:<46} {}", stat.owner, stat.stat, stat.reason);
    }
    // Most stats without an entry are display or animation data; the list is
    // short enough to read for the one that is a mechanic.
    let mut owner = "";
    for (stat_owner, stat) in &diagnosis.unknown_stats {
        if stat_owner != owner {
            owner = stat_owner;
            eprintln!("   no entry, {owner}:");
        }
        eprintln!("        {stat}");
    }

    // A multiplier a config option can set is the user's to provide; one no
    // option sets has to come from the engine, and nothing in it did.
    let config_sets = |variable: &str| {
        let name = format!("Multiplier:{variable}");
        data.config_catalog.as_ref().is_some_and(|catalog| {
            catalog.options.iter().any(|option| {
                option
                    .effects
                    .iter()
                    .chain(option.option_effects.values().flatten())
                    .any(|effect| effect.name == name)
            })
        })
    };
    eprintln!(
        "\n== 5. Modifiers that scale by a multiplier nothing set ({})",
        diagnosis.unprovided_multipliers.len()
    );
    let mut last = "";
    for (variable, modifier) in &diagnosis.unprovided_multipliers {
        if variable != last {
            let provider = if config_sets(variable) {
                "a config option can set it; this build did not"
            } else {
                "no config option sets it: the engine must, and did not"
            };
            eprintln!("    Multiplier:{variable}  ({provider})");
            last = variable;
        }
        eprintln!(
            "        {} {:?} {}  {}",
            modifier.name, modifier.mod_type, modifier.value, modifier.source
        );
    }

    // A condition no pass of the engine works out and no config option sets
    // drops every modifier that waits for it; one a config option sets is the
    // user's to switch on.
    let (config_off, nothing_sets): (Vec<_>, Vec<_>) = diagnosis
        .unset_conditions
        .iter()
        .partition(|unset| unset.config_option.is_some());
    eprintln!(
        "\n== 6. Modifiers that wait for a condition nothing sets ({} conditions)",
        nothing_sets.len()
    );
    for unset in nothing_sets {
        let flag = if unset.flagged {
            "; a stored Condition flag says it holds, and nothing copies that into the calculation"
        } else {
            ""
        };
        eprintln!(
            "    {}  ({} modifiers{flag})",
            unset.condition,
            unset.modifiers.len()
        );
        for modifier in unset.modifiers.iter().take(3) {
            eprintln!(
                "        {} {:?} {}  {}",
                modifier.name, modifier.mod_type, modifier.value, modifier.source
            );
        }
        if unset.modifiers.len() > 3 {
            eprintln!("        … and {} more", unset.modifiers.len() - 3);
        }
    }
    if !config_off.is_empty() {
        eprintln!("   config: off (a config option sets these; this build leaves it off):");
        for unset in config_off {
            eprintln!(
                "    {:<36}{:<40}{} modifiers",
                unset.condition,
                unset.config_option.as_deref().unwrap_or_default(),
                unset.modifiers.len()
            );
        }
    }

    // A name unread here and read in some other committed build is one this
    // build's skill has no use for. A name no committed build reads has no
    // reader that any of them exercises.
    let mut read_somewhere: std::collections::BTreeSet<String> =
        diagnosis.read_names.iter().cloned().collect();
    for other in committed_codes() {
        if let Ok(imported) = import_build_from_code(&other)
            && let Ok(other) = diagnose(&imported.build, &data, &options)
        {
            read_somewhere.extend(other.read_names);
        }
    }
    let (no_reader, unused): (Vec<&ModifierRef>, Vec<&ModifierRef>) = diagnosis
        .unread_modifiers
        .iter()
        .partition(|modifier| !read_somewhere.contains(&modifier.name));
    let (from_config, from_build): (Vec<&ModifierRef>, Vec<&ModifierRef>) = no_reader
        .into_iter()
        .partition(|modifier| from_config(modifier));

    eprintln!(
        "\n== 7. Stored modifiers no committed build's calculation reads ({})",
        from_build.len()
    );
    eprintln!("   The character has these; the name they are stored under has no reader.");
    print_modifiers(from_build.into_iter());

    // An input the build code sets is the user's statement about the build; a
    // default is only the catalog's.
    let set_by_build = |modifier: &ModifierRef| {
        modifier
            .source
            .strip_prefix("Config:config.")
            .and_then(|rest| rest.split(' ').next())
            .is_some_and(|var| build.config.raw_inputs.values.contains_key(var))
    };
    let (set, defaults): (Vec<&ModifierRef>, Vec<&ModifierRef>) = from_config
        .into_iter()
        .partition(|modifier| set_by_build(modifier));
    eprintln!(
        "\n== 8. Config inputs this build sets, written to a name nothing reads ({})",
        set.len()
    );
    for modifier in set {
        eprintln!(
            "    {:<44}{:>8}  {}",
            modifier.name, modifier.value, modifier.source
        );
    }
    let mut default_names: Vec<&str> = defaults
        .iter()
        .map(|modifier| modifier.name.as_str())
        .collect();
    default_names.dedup();
    eprintln!(
        "   and {} config defaults under names nothing reads: {}",
        defaults.len(),
        default_names.join(", ")
    );

    let mut names: Vec<&str> = unused
        .iter()
        .map(|modifier| modifier.name.as_str())
        .collect();
    names.dedup();
    eprintln!(
        "\n== 9. Stored modifiers this build's skill had no use for ({} under {} names; other builds read them)",
        unused.len(),
        names.len()
    );
    eprintln!("    {}", names.join(", "));
}
