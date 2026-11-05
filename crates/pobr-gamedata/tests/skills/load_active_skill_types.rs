//! `base/active_skill_types.json`: the game's ActiveSkillType rows, the
//! skill type names parser rules, skill data and build codes use.

use pobr_data::catalog::skill_types::ActiveSkillTypeDef;
use pobr_data::skill::SkillTypes;
use pobr_gamedata::{GameData, current_data_dir};

#[test]
fn the_active_pack_table_installs_and_names_every_type() {
    let data = GameData::new(current_data_dir());
    let Some(rows) = data
        .active_skill_types()
        .expect("load active_skill_types.json")
    else {
        // A pack published before RePoE exported the table: the frozen names stay in use.
        data.install_skill_type_names().unwrap();
        return;
    };
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
    assert_eq!(rows[0].id, "Attack");
}

#[test]
fn a_reordered_table_is_refused() {
    let rows: Vec<ActiveSkillTypeDef> = ["Spell", "Attack"]
        .iter()
        .enumerate()
        .map(|(i, id)| ActiveSkillTypeDef {
            id: (*id).into(),
            index: i as u32 + 1,
        })
        .collect();
    let message = pobr_data::skill::install_skill_type_names(&rows).unwrap_err();
    assert!(message.contains("\"Attack\""), "{message}");
}
