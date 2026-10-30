//! F2 (a drill finding): fail-fast assertion for `.dat` table required columns.
//!
//! The adapter's row structures silently degrade a missing column via serde
//! `Option` / `default` (e.g. `ArmourTypes` missing the
//! `IncreasedMovementSpeed` column means the output's `movement_penalty`
//! column is missing entirely; `GrantedEffects` missing `AdditionalStatSets`
//! means `additional_stat_set_ids` is missing) — the structure is still
//! valid but semantically degraded, discoverable only after the fact via an
//! output byte-diff. This module checks column presence on the **first
//! row** of every consumed table before `--raw` adaptation starts
//! (pathofexile-dat exports carry the full column-key set on every row, so
//! a missing key on the first row means either a snapshot drift or an
//! export config missing a column), and reports a "table name + column name" error for any missing column.
//!
//! The checklist tracks the existing schema's needs: the column set is the
//! full union of serde renames across the `Raw*` row structures (including
//! Option / default columns). Update this table when the adapter starts consuming a new column.

use std::path::Path;

/// English tables -> the required columns the adapter consumes.
const REQUIRED_EN: &[(&str, &[&str])] = &[
    ("ItemClasses.json", &["_index", "Id"]),
    ("Tags.json", &["_index", "Id"]),
    (
        "Mods.json",
        &[
            "_index",
            "Id",
            "Name",
            "ModType",
            "Domain",
            "GenerationType",
            "Level",
            "Stat1",
            "Stat2",
            "Stat3",
            "Stat4",
            "Stat1Value",
            "Stat2Value",
            "Stat3Value",
            "Stat4Value",
            "Tags",
            "SpawnWeight_Tags",
            "SpawnWeight_Values",
        ],
    ),
    // Note: `ModType.json` (the source of the group field) isn't in the
    // required list — when the table is missing, the adapter soft-degrades
    // in `mod_type_lookup` (the group column defaults + a warning), matching F2's resilience approach.
    (
        "Stats.json",
        &["_index", "Id", "IsLocal", "Semantic", "Category"],
    ),
    (
        "BaseItemTypes.json",
        &[
            "_index",
            "Id",
            "Name",
            "ItemClass",
            "DropLevel",
            "Width",
            "Height",
            "Tags",
            "Implicit_Mods",
            "ModDomain",
        ],
    ),
    (
        "Flasks.json",
        &["BaseItemType", "LifePerUse", "ManaPerUse", "RecoveryTime"],
    ),
    (
        "ComponentCharges.json",
        &["BaseItemTypesKey", "MaxCharges", "PerCharge"],
    ),
    (
        "WeaponTypes.json",
        &[
            "BaseItemType",
            "DamageMin",
            "DamageMax",
            "Speed",
            "CritChance",
            "RangeMax",
            // Note: `ReloadTime` is PoB2 spec.lua's column name; the
            // community dat-schema's (used for pathofexile-dat downloads)
            // WeaponTypes table has no such column, and crossbow reload
            // time is backfilled by the overlay/vendor instead
            // (RawWeaponType.reload_time is Option/default). So it isn't
            // asserted as a required column — otherwise the whole table export would fail for lacking a community-schema column.
        ],
    ),
    (
        "ArmourTypes.json",
        &[
            "BaseItemType",
            "Armour",
            "Evasion",
            "EnergyShield",
            "Ward",
            "IncreasedMovementSpeed",
        ],
    ),
    (
        "SkillGems.json",
        &[
            "BaseItemType",
            "GemType",
            "GemColour",
            "MinLevelReq",
            "StrengthRequirementPercent",
            "DexterityRequirementPercent",
            "IntelligenceRequirementPercent",
            "GemEffects",
            "ItemExperienceType",
        ],
    ),
    (
        "GrantedEffects.json",
        &[
            "Id",
            "IsSupport",
            "ActiveSkill",
            "CastTime",
            "AllowedActiveSkillTypes",
            "AddedActiveSkillTypes",
            "ExcludedActiveSkillTypes",
            "CannotBeSupported",
            "SupportsGemsOnly",
            "StatSet",
            "AdditionalStatSets",
            "CostTypes",
        ],
    ),
    (
        "GrantedEffectsPerLevel.json",
        &[
            "GrantedEffect",
            "Level",
            "Cooldown",
            "AttackTime",
            "CostAmounts",
            "AttackSpeedMultiplier",
            // Note: `BaseMultiplier` isn't in the community
            // GrantedEffectsPerLevel table (see skills/levels.rs: always
            // missing -> None; the real source is the stat-set table, and
            // per-level values are merged in from overlay/skill_overrides.json).
            // So it isn't asserted as a required column — otherwise
            // pathofexile-dat's whole table export would fail for lacking a community-schema column.
            "CostMultiplier",
            "Reservation",
            "EffectOnPlayer",
            "StoredUses",
        ],
    ),
    (
        "GrantedEffectStatSets.json",
        &[
            "Id",
            "Label",
            "BaseEffectiveness",
            "ConstantStats",
            "ConstantStatsValues",
            // (Backlog #7-2) The community schema name IgnoredStats = vendor's
            // RemoveStats: a DoT-variant set removes the main set's hit
            // damage stat (a missing column means it isn't removed, causing phantom hit damage to reappear).
            "IgnoredStats",
            // What a set's hits are (`base_is_projectile`, `is_area_damage`, …):
            // without the column every set is written without its valueless stats.
            "ImplicitStats",
            // How a minion skill's damage grows with the minion's level.
            "IncrementalEffectiveness",
            "DamageIncrementalEffectiveness",
        ],
    ),
    ("GrantedEffectLabels.json", &["_index", "Id", "Text"]),
    (
        "GrantedEffectStatSetsPerLevel.json",
        &[
            "StatSet",
            "GemLevel",
            "FloatStats",
            "BaseResolvedValues",
            "AdditionalStats",
            "AdditionalStatsValues",
            "AdditionalFlags",
            "BaseMultiplier",
            "SpellCritChance",
            "AttackCritChance",
            // A minion skill's stat per point of effectiveness.
            "FloatStatsValues",
        ],
    ),
    (
        "ActiveSkills.json",
        &["Id", "DisplayedName", "ActiveSkillTypes"],
    ),
    // Note: `Name` isn't in the community dat-schema's ActiveSkillType table
    // (the adapter reads it as Option/default, always missing); only the
    // actually-present `_index` / `Id` are asserted.
    ("ActiveSkillType.json", &["_index", "Id"]),
    ("CostTypes.json", &["_index", "Id", "Divisor", "PerMinute"]),
    // What a gem grants and what kind of gem it is.
    (
        "GemEffects.json",
        &["Id", "GrantedEffect", "AdditionalGrantedEffects", "GemTags"],
    ),
    ("GemTags.json", &["Id"]),
    ("MonsterVarieties.json", &["Id", "MonsterType"]),
    ("MonsterTypes.json", &["MonsterResistances"]),
    (
        "MonsterResistances.json",
        &["Fire1", "Cold1", "Lightning1", "Chaos1"],
    ),
    // The character level each gem level requires.
    (
        "ItemExperiencePerLevel.json",
        &["ItemExperienceType", "ItemCurrentLevel", "Level"],
    ),
];

/// Traditional Chinese sidecar tables -> required columns.
const REQUIRED_TW: &[(&str, &[&str])] = &[
    ("BaseItemTypes.json", &["_index", "Name"]),
    ("Mods.json", &["_index", "Name"]),
    ("ActiveSkills.json", &["_index", "DisplayedName"]),
];

/// `--raw` mode's entry check: checks column presence for every consumed
/// table, **returning the list of missing columns** (rather than aborting).
///
/// Resilience (keeping data and code decoupled): a missing column **is no
/// longer fatal** — serde already degrades a missing column via
/// `Option`/`default` (the relevant output field is missing/empty), and
/// this check only loudly reports the drift for the caller to decide what
/// to do (defaults to warn-and-continue; turns into a hard error under
/// `--strict-columns`). Two cases are distinguished:
/// - A missing table file / a non-array-of-objects -> still an immediate
///   `Err` (the input itself is broken and can't be adapted, not "drift");
/// - A missing column -> collected into the returned list (`Ok(missing)`, empty means everything's present);
/// - An empty table (zero rows) -> columns can't be validated, so it's allowed through (not counted as drift).
pub(crate) fn check_required_columns(en: &Path, tw: &Path) -> Result<Vec<String>, String> {
    #[derive(serde::Deserialize)]
    struct LabelReference {
        #[serde(rename = "Label")]
        label: Option<i64>,
    }
    let sets: Vec<LabelReference> = crate::read_json(&en.join("GrantedEffectStatSets.json"))?;
    let needs_labels = sets.iter().any(|set| set.label.is_some_and(|i| i >= 0));
    let mut missing = Vec::new();
    for (dir_label, dir, tables) in [
        ("English", en, REQUIRED_EN),
        ("Traditional Chinese sidecar", tw, REQUIRED_TW),
    ] {
        for (file, columns) in tables {
            // Legacy raw exports can omit this table when no set uses it.
            // Keep checking any supplied table, and require it for real FKs.
            if *file == "GrantedEffectLabels.json" && !needs_labels && !dir.join(file).exists() {
                continue;
            }
            check_table_columns(&dir.join(file), dir_label, file, columns, &mut missing)?;
        }
    }
    Ok(missing)
}

/// Single-table check: parses as an array of objects and checks the first
/// row's key set, appending any missing column to `missing`.
fn check_table_columns(
    path: &Path,
    dir_label: &str,
    file: &str,
    columns: &[&str],
    missing: &mut Vec<String>,
) -> Result<(), String> {
    let rows: Vec<serde_json::Map<String, serde_json::Value>> = crate::read_json(path)?;
    let Some(first) = rows.first() else {
        return Ok(()); // An empty table has no row to check keys against
    };
    for column in columns {
        if !first.contains_key(*column) {
            missing.push(format!(
                "{dir_label}/{file} is missing required column `{column}`"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    /// Writes a minimal temp table file and returns its directory (doesn't
    /// depend on a local pipeline snapshot — F8: `pipeline/tables/` may be missing entirely).
    fn temp_table(name: &str, json: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pobr-required-columns-{}-{name}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("T.json"), json).unwrap();
        dir
    }

    fn raw_snapshot_without_labels(name: &str, stat_sets: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "pobr-required-columns-labels-{}-{name}",
            std::process::id()
        ));
        for (language, tables) in [("English", REQUIRED_EN), (crate::ZH_TW, REQUIRED_TW)] {
            let dir = root.join(language);
            fs::create_dir_all(&dir).unwrap();
            for (file, _) in tables {
                if *file != "GrantedEffectLabels.json" {
                    fs::write(dir.join(file), "[]").unwrap();
                }
            }
        }
        for file in [
            "MonsterVarieties.json",
            "MonsterTypes.json",
            "MonsterResistances.json",
        ] {
            fs::write(root.join("English").join(file), "[]").unwrap();
        }
        fs::write(root.join("English/GrantedEffectStatSets.json"), stat_sets).unwrap();
        let mut files = std::collections::BTreeMap::new();
        for language in ["English", crate::ZH_TW] {
            for entry in fs::read_dir(root.join(language)).unwrap() {
                let path = entry.unwrap().path();
                let name = format!("{language}/{}", path.file_name().unwrap().to_string_lossy());
                files.insert(
                    name,
                    crate::raw_source::sha256_hex(&fs::read(path).unwrap()),
                );
            }
        }
        let source = serde_json::json!({
            "schema": "official-raw-export/v1", "patch": "label-test", "files": files
        });
        fs::write(root.join("export-source.json"), source.to_string()).unwrap();
        root
    }

    #[test]
    fn raw_entry_accepts_legacy_exports_without_an_unused_labels_table() {
        for (name, sets) in [
            ("absent", r#"[{"Id":"Legacy"}]"#),
            ("null", r#"[{"Id":"Legacy","Label":null}]"#),
            ("negative", r#"[{"Id":"Legacy","Label":-1}]"#),
        ] {
            let raw = raw_snapshot_without_labels(name, sets);
            let out = raw.join("out");
            let summary = crate::run(crate::Args {
                raw: raw.clone(),
                out: out.clone(),
                patch: "label-test".into(),
                strict_columns: false,
            })
            .unwrap_or_else(|error| panic!("legacy {name}: {error}"));
            assert!(summary.contains("adaptation complete"), "{summary}");
            assert!(
                out.join("label-test/manifest.json").exists(),
                "legacy {name}"
            );
            fs::remove_dir_all(raw).unwrap();
        }
    }

    #[test]
    fn raw_entry_rejects_a_missing_referenced_labels_table_before_writing() {
        let raw = raw_snapshot_without_labels("referenced", r#"[{"Id":"Native","Label":0}]"#);
        let out = raw.join("out");
        let error = crate::run(crate::Args {
            raw: raw.clone(),
            out: out.clone(),
            patch: "label-test".into(),
            strict_columns: false,
        })
        .unwrap_err();
        assert!(error.contains("GrantedEffectLabels.json"), "{error}");
        assert!(
            !out.exists(),
            "a broken label reference must fail before output"
        );
        fs::remove_dir_all(raw).unwrap();
    }

    /// A missing column -> reports "table name + column name", and aggregates all missing columns in one pass.
    #[test]
    fn reports_table_and_column_for_missing_columns() {
        let dir = temp_table("missing", r#"[{"BaseItemType": 1, "Armour": 5}]"#);
        let mut missing = Vec::new();
        check_table_columns(
            &dir.join("T.json"),
            "English",
            "ArmourTypes.json",
            &["BaseItemType", "Armour", "IncreasedMovementSpeed", "Ward"],
            &mut missing,
        )
        .unwrap();
        assert_eq!(
            missing,
            vec![
                "English/ArmourTypes.json is missing required column `IncreasedMovementSpeed`"
                    .to_string(),
                "English/ArmourTypes.json is missing required column `Ward`".to_string(),
            ]
        );
    }

    /// All columns present (values may be null) -> no missing columns reported; an empty table -> allowed through.
    #[test]
    fn passes_when_columns_present_or_table_empty() {
        let dir = temp_table("ok", r#"[{"Id": "x", "AdditionalStatSets": null}]"#);
        let mut missing = Vec::new();
        check_table_columns(
            &dir.join("T.json"),
            "English",
            "GrantedEffects.json",
            &["Id", "AdditionalStatSets"],
            &mut missing,
        )
        .unwrap();
        assert!(missing.is_empty());

        let empty = temp_table("empty", "[]");
        check_table_columns(
            &empty.join("T.json"),
            "English",
            "GrantedEffects.json",
            &["Id"],
            &mut missing,
        )
        .unwrap();
        assert!(missing.is_empty());
    }

    /// A missing table file -> errors immediately (with the path), no silent skip.
    #[test]
    fn fails_fast_when_table_file_missing() {
        let dir = temp_table("absent", "[]");
        let mut missing = Vec::new();
        let err = check_table_columns(
            &dir.join("NoSuchTable.json"),
            "English",
            "NoSuchTable.json",
            &["Id"],
            &mut missing,
        )
        .unwrap_err();
        assert!(err.contains("NoSuchTable.json"), "err = {err}");
    }

    /// Entry check: build a directory with a missing column -> the returned
    /// missing-columns list contains "table name + column name" (resilience: returns rather than aborting).
    #[test]
    fn entry_check_aggregates_across_tables() {
        let root = std::env::temp_dir().join(format!(
            "pobr-required-columns-entry-{}",
            std::process::id()
        ));
        let en = root.join("English");
        let tw = root.join("Traditional Chinese");
        fs::create_dir_all(&en).unwrap();
        fs::create_dir_all(&tw).unwrap();
        // Write every table as an empty array first (empty tables are
        // allowed through), then rewrite ArmourTypes as a single-row table missing IncreasedMovementSpeed.
        for (file, _) in REQUIRED_EN {
            fs::write(en.join(file), "[]").unwrap();
        }
        for (file, _) in REQUIRED_TW {
            fs::write(tw.join(file), "[]").unwrap();
        }
        fs::write(
            en.join("ArmourTypes.json"),
            r#"[{"BaseItemType": 0, "Armour": 1, "Evasion": 0, "EnergyShield": 0, "Ward": 0}]"#,
        )
        .unwrap();
        let missing = check_required_columns(&en, &tw).unwrap();
        assert!(
            missing.iter().any(|m| m
                .contains("ArmourTypes.json is missing required column `IncreasedMovementSpeed`")),
            "missing = {missing:?}"
        );
    }
}
