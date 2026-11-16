use std::collections::BTreeMap;
use std::path::PathBuf;

use pobr_gamedata::test_pins::{
    parser_handler_fallbacks, parser_section_counts, stored_uses_count,
    write_repository_migration_pins,
};

#[test]
#[ignore = "use data:migrate with POBR_MIGRATION_PINS pointing to an absolute fresh file"]
fn repository_migration_pins() {
    let output = PathBuf::from(
        std::env::var_os("POBR_MIGRATION_PINS").expect("POBR_MIGRATION_PINS output path"),
    );
    assert!(output.is_absolute(), "POBR_MIGRATION_PINS must be absolute");
    assert!(!output.exists(), "POBR_MIGRATION_PINS must be a fresh file");
    let version_dir = pobr_gamedata::current_data_dir();
    let game = pobr_gamedata::GameData::new(&version_dir);
    let data = pobr_build::BuildData::load(&game).expect("load current repository game data");
    let parser = game
        .mod_parser_rules()
        .expect("load mod_parser_rules")
        .expect("repository data must include mod_parser_rules");
    let levels = game
        .granted_effect_levels()
        .expect("load + merge granted_effect_levels");
    let pins = BTreeMap::from([
        (
            "minions.raised_zombie".into(),
            super::minions::raised_zombie_pin(&data),
        ),
        (
            "minions.spectre_cocoon3".into(),
            super::minions::spectre_cocoon3_pin(&data),
        ),
        (
            "parser_rules.handler_fallbacks".into(),
            parser_handler_fallbacks(&parser).into(),
        ),
        (
            "parser_rules.section_counts".into(),
            parser_section_counts(&parser),
        ),
        (
            "skill_overrides.stored_uses_count".into(),
            stored_uses_count(&levels).into(),
        ),
    ]);
    write_repository_migration_pins(&version_dir, &output, pins)
        .unwrap_or_else(|error| panic!("write migration pins {}: {error}", output.display()));
}
