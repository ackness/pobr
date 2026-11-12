//! `base/active_skill_types.json`: the game's ActiveSkillType rows, the
//! skill type names parser rules, skill data and build codes use.

use pobr_data::skill::SkillTypes;
use pobr_gamedata::{GameData, current_data_dir};

#[test]
fn the_active_pack_table_installs_and_names_every_type() {
    let data = GameData::new(current_data_dir());
    let rows = data
        .active_skill_types()
        .expect("load active_skill_types.json");
    data.install_skill_type_names()
        .expect("install the pack's table");
    assert!(rows.len() >= 290, "{} rows", rows.len());
    for row in &rows {
        assert_eq!(
            SkillTypes::from_pob2_name(&row.id),
            Some(SkillTypes::from_pob2_index(row.index)),
            "{}",
            row.id
        );
    }
    assert_eq!(
        SkillTypes::from_pob2_name("Attack"),
        Some(SkillTypes::ATTACK)
    );
}

#[test]
fn a_pack_without_the_table_is_refused_at_load() {
    let data = GameData::from_memory(std::collections::BTreeMap::from([(
        "base/game_constants.json".to_string(),
        b"{}".to_vec(),
    )]));
    for error in [
        data.active_skill_types().unwrap_err(),
        data.install_skill_type_names().unwrap_err(),
    ] {
        assert!(
            matches!(error, pobr_gamedata::LoadError::Integrity { .. }),
            "{error:?}"
        );
        assert!(
            error
                .to_string()
                .contains(pobr_gamedata::MISSING_SKILL_TYPES),
            "{error}"
        );
    }
    assert!(
        pobr_gamedata::MISSING_SKILL_TYPES
            .contains("data contract 1.1 package required: base/active_skill_types.json")
    );
}

#[test]
fn malformed_names_are_refused() {
    use pobr_data::catalog::skill_types::ActiveSkillTypeDef;
    for names in [["Attack", "Attack"], ["Attack", ""]] {
        let rows: Vec<_> = names
            .iter()
            .enumerate()
            .map(|(i, id)| ActiveSkillTypeDef {
                id: (*id).into(),
                index: i as u32 + 1,
            })
            .collect();
        assert!(pobr_data::skill::install_skill_type_names(&rows).is_err());
    }
}

// One installed package per process, including callers using GameData directly.
#[test]
fn missing_companion_is_a_ruleset_load_diagnostic() {
    const CASE: &str = "POBR_GAMEDATA_MISSING_COMPANION";
    if std::env::var_os(CASE).is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "load_active_skill_types::missing_companion_is_a_ruleset_load_diagnostic",
            ])
            .env(CASE, "1")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }
    let mut rows = GameData::new(current_data_dir())
        .active_skill_types()
        .unwrap();
    rows.retain(|row| row.id != "Companion");
    for (i, row) in rows.iter_mut().enumerate() {
        row.index = i as u32 + 1;
    }
    let data = GameData::from_memory(std::collections::BTreeMap::from([(
        "base/active_skill_types.json".into(),
        serde_json::to_vec(&rows).unwrap(),
    )]));
    let ruleset = data
        .load_ruleset()
        .expect("missing named type must not reject the package");
    assert_eq!(ruleset.degraded, data.skill_type_diagnostics().unwrap());
    assert_eq!(
        ruleset.degraded,
        ["active_skill_types: missing engine type \"Companion\"; it matches no installed type"]
    );
    assert!(!SkillTypes::COMPANION.intersects(SkillTypes::COMPANION));
    for row in rows {
        assert!(!SkillTypes::from_pob2_index(row.index).intersects(SkillTypes::COMPANION));
    }
}
