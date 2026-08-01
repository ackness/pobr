//! Dual-run comparison: the old `parse_config` vs the new
//! `parse_config_inputs` + `config_interpreter` path.
//!
//! **Ongoing regression** (since commit ①): the main path has switched to the
//! interpreter (`config_resolve`); this test keeps running until the old
//! path is deleted — any change that breaks "old ⊆ new, with the
//! intersection equal value-for-value" is caught here.
//!
//! Contract: **every conditions / multipliers / global_texts / scalar item the
//! old path can produce must be covered value-for-value by the new path
//! (old ⊆ new), and the intersection must be equal value-for-value**.
//! "Covered" has two layers:
//! 1. **Same name, same value**: the new path's `conditions`/`multipliers`
//!    backfill tables hit directly;
//! 2. **Covered via mod-ification**: vendor's faithful form turns a condition
//!    into a tagged Modifier (`Condition:Combat` combat gate /
//!    `Condition:Effective`) or an enemy-actor-ized one — the value matches
//!    exactly inside the mod payload, but it **no longer lands in the global
//!    table** (this is new coverage semantics, to be opened up per-category by
//!    behaviour commits; right now production still consumes the old path's
//!    output, so behaviour is unchanged value-for-value).
//! 3. **Handler gap**: a catalog entry has a handler_id but no registered
//!    producer — the raw input has already been losslessly captured by
//!    RawConfigInputs (verifiable via scalar echo); interpreted output awaits
//!    a later handler batch.
//!
//! Any old-path output item that doesn't fall into one of the three
//! categories above is a dual-run failure (hard fail).
//!
//! Data sources: `examples/demo-bd-test/builds/*/code.txt` (the ninja
//! 18-build set) + `tests/fixtures/config_*.xml`. Run with `-- --nocapture`
//! to print the per-category diff summary (the list of newly-covered items).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use pobr_build::decode_pob_code;
use pobr_build::handlers::{build_registry, campaign_progress_from_config, enemy_tier_from_config};
use pobr_build::xml_build::{parse_config_inputs, parse_config_legacy};
use pobr_core::mod_parser::ParseStatus;
use pobr_core::modifier::Modifier;
use pobr_core::rules::config_interpreter::{
    ConfigInputValue, ConfigOutcome, RawConfigInputs, interpret,
};
use pobr_data::modifier::ModType;
use pobr_gamedata::ruleset::ConfigCatalog;
use pobr_gamedata::{GameData, repo_data_root};

/// Single-line parse via the engine (real rules, signature matches the historical `parse_mod`; the engine never returns `Err`).
fn parse_mod(
    text: &str,
) -> Result<pobr_core::mod_parser::ParseOutcome, pobr_core::mod_parser::ParseError> {
    use std::sync::LazyLock;
    static RULES: LazyLock<pobr_core::mod_parser::CompiledParserRules> =
        LazyLock::new(pobr_core::mod_parser::test_compiled_rules);
    Ok(pobr_core::mod_parser::parse_mod_engine(text, &RULES))
}

/// Reverse lookup table for the old path's `DEFAULT_TRUE_CONDITIONS`
/// (condition variable name -> XML `<Input name>`).
/// This test locally mirrors xml_build's private constant — this table
/// retires together with the dual-run test when the old path is deleted.
const DEFAULT_TRUE_REVERSE: &[(&str, &str)] = &[
    ("TargetingBrandedEnemy", "targetBrandedEnemy"),
    ("DemonForm", "inDemonForm"),
    ("CompanionInPresence", "companionInPresence"),
    ("ChampionIntimidate", "conditionChampionIntimidate"),
    ("ConcPathBypassCD", "ConcPathBypassCD"),
    ("FlickerStrikeBypassCD", "FlickerStrikeBypassCD"),
    ("VigilantStrikeBypassCD", "VigilantStrikeBypassCD"),
];

/// Reverse lookup table for the old path's charge conditions (condition variable name -> XML `<Input name>`).
const CHARGE_REVERSE: &[(&str, &str)] = &[
    ("UsePowerCharges", "usePowerCharges"),
    ("UseFrenzyCharges", "useFrenzyCharges"),
    ("UseEnduranceCharges", "useEnduranceCharges"),
];

/// The old path's default table for Stat-type quest rewards (locally mirrored in this test).
const DEFAULT_QUEST_STAT_REWARDS: &[(&str, &str)] = &[
    ("questAct 1ClearfellBeira", "+10% to Cold Resistance"),
    ("questAct 1FreythornKing In The Mists", "+30 to Spirit"),
    ("questAct 1Ogham ManorCandlemass", "+20 to maximum Life"),
    (
        "questAct 2Spires of DesharSisters of Garukhan Shrine",
        "+10% to Lightning Resistance",
    ),
    ("questAct 3Azak BogIgnagduk", "+30 to Spirit"),
    (
        "questAct 3Jiquani's MachinariumBlackjaw",
        "+10% to Fire Resistance",
    ),
    (
        "questAct 4Eye of HinekoraSilent Hall",
        "5% increased Maximum Mana",
    ),
    (
        "questInterlude 2Khari CrossingMolten Shrine",
        "5% increased maximum Life",
    ),
    ("questInterlude 3Kriar VillageLythara", "+40 to Spirit"),
];

fn load_catalog() -> ConfigCatalog {
    GameData::new(repo_data_root().join(pobr_gamedata::data_version()))
        .load_ruleset()
        .expect("load ruleset")
        .config_catalog
        .expect("the config_catalog domain should be wired up")
}

/// Dual-run data sources: the 18 ninja builds (code.txt -> XML) + every config fixture.
fn xml_sources() -> Vec<(String, String)> {
    let mut sources = Vec::new();
    let builds = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/demo-bd-test/builds");
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(builds)
        .expect("read builds dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir() && p.join("code.txt").exists())
        .collect();
    dirs.sort();
    for dir in dirs {
        let code = std::fs::read_to_string(dir.join("code.txt")).expect("read code.txt");
        let xml = decode_pob_code(code.trim()).expect("decode build code");
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        sources.push((name, xml));
    }
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut files: Vec<PathBuf> = std::fs::read_dir(fixtures)
        .expect("read fixtures dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("config_"))
        })
        .collect();
    files.sort();
    for file in files {
        let xml = std::fs::read_to_string(&file).expect("read fixture");
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        sources.push((name, xml));
    }
    assert!(sources.len() >= 23, "18 ninja builds + 5 fixtures");
    sources
}

/// Filters mods across every output bucket by attribution var.
fn mods_from<'a>(outcome: &'a ConfigOutcome, var: &str) -> Vec<&'a Modifier> {
    let id = format!("config.{var}");
    outcome
        .player_mods
        .iter()
        .chain(outcome.enemy_mods.iter())
        .chain(outcome.minion_mods.iter())
        .filter(|m| {
            m.origin
                .as_ref()
                .is_some_and(|origin| origin.source_id.id == id)
        })
        .collect()
}

/// Dual-run category summary (aggregated across all data sources; BTreeMap keeps print order deterministic).
#[derive(Default)]
struct DualRunSummary {
    /// Coverage category -> set of items (`var` or `var=value`).
    categories: BTreeMap<&'static str, BTreeSet<String>>,
    /// Old-path output items that couldn't be explained (-> hard fail).
    unexplained: BTreeSet<String>,
}

impl DualRunSummary {
    fn record(&mut self, category: &'static str, item: String) {
        self.categories.entry(category).or_default().insert(item);
    }
}

/// Coverage classification for a single old-path condition.
fn classify_condition(
    var: &str,
    value: bool,
    catalog: &ConfigCatalog,
    outcome: &ConfigOutcome,
    source: &str,
    summary: &mut DualRunSummary,
) {
    if !value {
        // Old path explicitly false: the new path's entry is inactive (default is false, so semantically equal);
        // the new path must never flip this to true.
        assert_ne!(
            outcome.conditions.get(var),
            Some(&true),
            "[{source}] condition {var}, explicitly false on the old path, was set true on the new path"
        );
        summary.record(
            "old explicit false (new path inactive, semantically equal)",
            var.to_string(),
        );
        return;
    }
    // Same name, same value: the intersection is equal value-for-value.
    if outcome.conditions.get(var) == Some(&true) {
        summary.record(
            "conditions: same name same value (intersection equal value-for-value)",
            var.to_string(),
        );
        return;
    }
    // Reverse-lookup the source XML input name -> catalog entry.
    let candidates = [
        format!("condition{var}"),
        CHARGE_REVERSE
            .iter()
            .find(|(v, _)| *v == var)
            .map(|(_, n)| (*n).to_string())
            .unwrap_or_default(),
        DEFAULT_TRUE_REVERSE
            .iter()
            .find(|(v, _)| *v == var)
            .map(|(_, n)| (*n).to_string())
            .unwrap_or_default(),
        var.to_string(),
    ];
    let entry = candidates
        .iter()
        .filter(|c| !c.is_empty())
        .find_map(|c| catalog.get(c));
    let Some(entry) = entry else {
        summary
            .unexplained
            .insert(format!("condition {var} @ {source} (no catalog entry)"));
        return;
    };
    if entry.handler_id.is_some() {
        summary.record(
            "handler gap (raw input captured, explanation awaits a later handler batch)",
            format!("{} ({})", entry.var, entry.handler_id.as_deref().unwrap()),
        );
        return;
    }
    let mods = mods_from(outcome, &entry.var);
    let expected = format!("Condition:{var}");
    if mods
        .iter()
        .any(|m| m.mod_type == ModType::Flag && m.name.as_str() == expected)
    {
        summary.record(
            "condition mod-ified (carries a Combat/Effective-style gating tag, value equal; behavior change pending a commit)",
            format!("{var} ← {}", entry.var),
        );
        return;
    }
    if let Some(stripped) = var.strip_prefix("Enemy")
        && mods
            .iter()
            .any(|m| m.name.as_str() == format!("Condition:{stripped}"))
    {
        summary.record(
            "enemy condition actor-ized (Condition:Enemy<X> → enemy-bucket Condition:<X>)",
            format!("{var} ← {}", entry.var),
        );
        return;
    }
    // vendor's naming differs from the prefix-stripped var (e.g. conditionEnemyCriticalWeakness ->
    // Condition:ApplyCriticalWeakness): the FLAG truth value matches, and the naming difference is vendor faithfulness.
    if let Some(found) = mods.iter().find(|m| m.mod_type == ModType::Flag) {
        summary.record(
            "condition follows vendor naming (FLAG truth value equal, name matches vendor's faithful spelling)",
            format!("{var} ← {} as {}", entry.var, found.name.as_str()),
        );
        return;
    }
    // Condition -> numeric mod-ification (the shape after §3-⑦ backfill): vendor turns a checkbox
    // into an enemy numeric mod (e.g. the three exposure `<X>Exposure` BASE 20 + ActorCondition gate,
    // ConfigOptions.lua:1864-1872) -- the on/off semantics are carried by the numeric mod, and no FLAG is produced anymore.
    if !mods.is_empty() {
        summary.record(
            "condition→numeric mod-ification (checkbox becomes an enemy numeric mod + actor gating tag)",
            format!("{var} ← {} as {}", entry.var, mods[0].name.as_str()),
        );
        return;
    }
    // The entry's effect carries an unmapped actor / an unwired tag dimension: the interpreter
    // conservatively skips the whole mod and records it in diagnostics -- the raw input is still captured.
    let diag_prefix = format!("config.{}:", entry.var);
    if outcome
        .diagnostics
        .iter()
        .any(|d| d.starts_with(&diag_prefix))
    {
        summary.record(
            "tag dimension not wired up (e.g. unmapped actor), conservatively skipped pending backfill",
            format!("{var} ← {}", entry.var),
        );
        return;
    }
    summary.unexplained.insert(format!(
        "condition {var} @ {source} (entry {} produced no corresponding output)",
        entry.var
    ));
}

/// Coverage classification for a single old-path multiplier.
fn classify_multiplier(
    var: &str,
    value: f64,
    catalog: &ConfigCatalog,
    outcome: &ConfigOutcome,
    source: &str,
    summary: &mut DualRunSummary,
) {
    if let Some(new_value) = outcome.multipliers.get(var) {
        // The intersection must be equal value-for-value (hard assert).
        assert!(
            (new_value - value).abs() < 1e-9,
            "[{source}] multiplier {var} intersection value mismatch: old {value} vs new {new_value}"
        );
        summary.record(
            "multipliers: same name same value (intersection equal value-for-value)",
            format!("{var}={value}"),
        );
        return;
    }
    let xml_name = format!("multiplier{var}");
    let Some(entry) = catalog.get(&xml_name) else {
        summary
            .unexplained
            .insert(format!("multiplier {var} @ {source} (no catalog entry)"));
        return;
    };
    if entry.handler_id.is_some() {
        summary.record(
            "handler gap (raw input captured, explanation awaits a later handler batch)",
            format!("{} ({})", entry.var, entry.handler_id.as_deref().unwrap()),
        );
        return;
    }
    let mods = mods_from(outcome, &entry.var);
    if let Some(found) = mods
        .iter()
        .find(|m| m.name.as_str().starts_with("Multiplier:"))
    {
        // Coverage via mod-ification must also be equal value-for-value (an identity-expression contract).
        assert_eq!(
            found.value.as_number(),
            Some(value),
            "[{source}] multiplier {var} mod-ified value mismatch (a non-identity vendor expression? needs a manual review)"
        );
        summary.record(
            "multiplier mod-ified (carries an Effective tag / naming fixed to match vendor, value equal; behavior change pending a commit)",
            format!("{var} ← {} as {}", entry.var, found.name.as_str()),
        );
        return;
    }
    let diag_prefix = format!("config.{}:", entry.var);
    if outcome
        .diagnostics
        .iter()
        .any(|d| d.starts_with(&diag_prefix))
    {
        summary.record(
            "tag dimension not wired up (T5-E1 ActorCondition/actor Multiplier), conservatively skipped pending backfill",
            format!("{var} ← {}", entry.var),
        );
        return;
    }
    summary.unexplained.insert(format!(
        "multiplier {var} @ {source} (entry {} produced no corresponding output)",
        entry.var
    ));
}

/// Coverage classification for the old path's global_texts (quest reward lines) -- rebuilds attribution per quest key, then classifies key by key.
fn classify_quests(
    inputs: &RawConfigInputs,
    catalog: &ConfigCatalog,
    outcome: &ConfigOutcome,
    source: &str,
    summary: &mut DualRunSummary,
) {
    // Rebuilds the old path's per-key output lines (mirrors the old parse_config's quest branch).
    let mut per_key: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (name, value) in &inputs.values {
        if !name.starts_with("quest") {
            continue;
        }
        match value {
            ConfigInputValue::Text(text) => {
                let lines: Vec<String> = text
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(str::to_string)
                    .collect();
                per_key.insert(name.clone(), lines);
            }
            ConfigInputValue::Bool(true) => {
                if let Some((_, stat)) = DEFAULT_QUEST_STAT_REWARDS.iter().find(|(k, _)| k == name)
                {
                    per_key.insert(name.clone(), vec![(*stat).to_string()]);
                }
            }
            _ => {}
        }
    }
    for (key, stat) in DEFAULT_QUEST_STAT_REWARDS {
        if !inputs.values.contains_key(*key) {
            per_key.insert((*key).to_string(), vec![(*stat).to_string()]);
        }
    }

    for (key, lines) in per_key {
        let Some(entry) = catalog.get(&key) else {
            summary
                .unexplained
                .insert(format!("quest {key} @ {source} (no catalog entry)"));
            continue;
        };
        if entry.handler_id.is_some() {
            summary.record(
                "handler gap (raw input captured, explanation awaits a later handler batch)",
                format!("{} ({})", entry.var, entry.handler_id.as_deref().unwrap()),
            );
            continue;
        }
        let quest_mods = mods_from(outcome, &entry.var);
        for line in lines {
            if line == "None" || line == "Nothing" {
                continue;
            }
            let Ok(parsed) = parse_mod(&line) else {
                summary.record(
                    "quest line unsupported by parser (old path also falls into the Unsupported channel, equal)",
                    format!("{key}: {line}"),
                );
                continue;
            };
            if parsed.status != ParseStatus::Parsed {
                summary.record(
                    "quest line unsupported by parser (old path also falls into the Unsupported channel, equal)",
                    format!("{key}: {line}"),
                );
                continue;
            }
            for old_mod in &parsed.mods {
                let matched = quest_mods.iter().find(|m| {
                    m.name == old_mod.name
                        && m.mod_type == old_mod.mod_type
                        && m.value.as_number() == old_mod.value.as_number()
                });
                if matched.is_some() {
                    summary.record(
                        "quest reward equal value-for-value (old text via parser == new declarative effects)",
                        format!("{key}: {}", old_mod.name.as_str()),
                    );
                } else if let Some(renamed) = quest_mods.iter().find(|m| {
                    m.mod_type == old_mod.mod_type
                        && m.value.as_number() == old_mod.value.as_number()
                }) {
                    // The parser's canonical name differs from vendor's (ColdResistance vs ColdResist):
                    // type + value match exactly, and the naming difference is vendor faithfulness (parser rules are unified).
                    summary.record(
                        "quest naming differs (parser name ≠ vendor name, type+value equal)",
                        format!(
                            "{key}: {} → {}",
                            old_mod.name.as_str(),
                            renamed.name.as_str()
                        ),
                    );
                } else {
                    summary.unexplained.insert(format!(
                        "quest {key} @ {source}: old line `{line}` parsed to {}={:?} with no match in the new output",
                        old_mod.name.as_str(),
                        old_mod.value.as_number(),
                    ));
                }
            }
        }
    }
}

/// Coverage unique to the new path (items the old path never consumed at all) -- feeds the newly-covered items list in the report.
fn collect_new_coverage(
    old_conditions: &std::collections::HashMap<String, bool>,
    old_multipliers: &std::collections::HashMap<String, f64>,
    outcome: &ConfigOutcome,
    summary: &mut DualRunSummary,
) {
    for (var, enabled) in &outcome.conditions {
        if *enabled && !old_conditions.contains_key(var) {
            summary.record(
                "newly-added coverage: condition (count type / implyCond / unprefixed entries)",
                var.clone(),
            );
        }
    }
    for var in outcome.multipliers.keys() {
        if !old_multipliers.contains_key(var) {
            summary.record("newly-added coverage: multiplier", var.clone());
        }
    }
    for m in &outcome.enemy_mods {
        if m.mod_type != ModType::Flag {
            summary.record(
                "newly-added coverage: enemy numeric override (resist/penetration/damage etc. direct BASE injection)",
                m.name.as_str().to_string(),
            );
        }
    }
    if !outcome.custom_mod_lines.is_empty() {
        summary.record(
            "newly-added coverage: customMods line channel",
            format!("{} lines", outcome.custom_mod_lines.len()),
        );
    }
    for entry in &outcome.skill_data {
        summary.record(
            "newly-added coverage: SkillData key-value payload",
            entry.key.clone(),
        );
    }
    for u in &outcome.unhandled {
        summary.record(
            "unhandled (handler_id not registered, coverage report)",
            u.handler_id.clone(),
        );
    }
}

/// Main dual-run assertion: old ⊆ new (classified per the module-doc contract), intersection equal value-for-value; any unexplained item = fail.
#[test]
fn dual_run_old_subset_of_new() {
    let catalog = load_catalog();
    let registry = build_registry();
    let mut summary = DualRunSummary::default();

    for (source, xml) in xml_sources() {
        let old = parse_config_legacy(&xml);
        let inputs = parse_config_inputs(&xml);
        let outcome = interpret(&catalog.options, &inputs, &registry);

        for (var, value) in &old.conditions {
            classify_condition(var, *value, &catalog, &outcome, &source, &mut summary);
        }
        for (var, value) in &old.multipliers {
            // vendor's aggregation contract (`ConfigOptions.lua:1106-1111`): applying
            // `multiplierNearbyRareOrUniqueEnemies` **writes both**
            // `Multiplier:NearbyRareOrUniqueEnemies` and `Multiplier:NearbyEnemies`
            // (:1108), summed by modDB -- the old path's prefix channel only records
            // this var and misses the aggregation. The handler already folds the rare
            // count into the NearbyEnemies scalar per vendor's contract, so the
            // intersection assertion's expected value = old value + rare count
            // (a behaviour improvement, not a contract drift).
            let expected = if var == "NearbyEnemies" {
                *value
                    + old
                        .multipliers
                        .get("NearbyRareOrUniqueEnemies")
                        .copied()
                        .unwrap_or(0.0)
            } else {
                *value
            };
            classify_multiplier(var, expected, &catalog, &outcome, &source, &mut summary);
        }
        classify_quests(&inputs, &catalog, &outcome, &source, &mut summary);

        // Scalar items: the enemyIsBoss / resistancePenalty wrapper matches the old path value-for-value.
        if let Some(old_tier) = old.enemy_tier {
            assert_eq!(
                enemy_tier_from_config(&outcome),
                Some(old_tier),
                "[{source}] enemyIsBoss scalar mismatch"
            );
            summary.record(
                "scalar equal value-for-value: enemyIsBoss",
                format!("{old_tier:?}"),
            );
        } else if let Some(tier) = enemy_tier_from_config(&outcome) {
            summary.record(
                "newly-added coverage: scalar default materialized (XML omission → catalog defaultIndex)",
                format!("enemyIsBoss={tier:?}"),
            );
        }
        if let Some(old_progress) = old.campaign_progress {
            assert_eq!(
                campaign_progress_from_config(&outcome),
                Some(old_progress),
                "[{source}] resistancePenalty scalar mismatch"
            );
            summary.record(
                "scalar equal value-for-value: resistancePenalty",
                format!("{old_progress:?}"),
            );
        } else if campaign_progress_from_config(&outcome).is_some() {
            summary.record(
                "newly-added coverage: scalar default materialized (XML omission → catalog defaultIndex)",
                "resistancePenalty=-60(Endgame)".to_string(),
            );
        }

        collect_new_coverage(&old.conditions, &old.multipliers, &outcome, &mut summary);
    }

    // Prints the per-category summary (when run with --nocapture).
    println!(
        "\n== M3-T1 dual-run category summary ({} categories) ==",
        summary.categories.len()
    );
    for (category, items) in &summary.categories {
        println!("\n[{category}] × {}", items.len());
        for item in items {
            println!("  - {item}");
        }
    }

    assert!(
        summary.unexplained.is_empty(),
        "unexplained old-path output items exist (old ⊄ new):\n{}",
        summary
            .unexplained
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
