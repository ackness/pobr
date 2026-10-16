use pobr_data::catalog::boss_poise_thresholds::BossPoiseThresholdsDef;
use pobr_data::monster::EnemyTier;
use pobr_gamedata::{GameData, data_version, repo_data_root};

#[test]
fn curated_game_poise_rows_load_and_match_no_data_defaults() {
    let table = GameData::new(repo_data_root().join(data_version()))
        .boss_poise_thresholds()
        .unwrap();
    // Official 0.5.5.4.2 MonsterVarieties: MAP mode 313 (275/437/687),
    // Xesht 1700 (1946). No distinct Uber Xesht row.
    assert_eq!(table, BossPoiseThresholdsDef::default());
    assert_eq!(table.for_tier(EnemyTier::None), 100);
    assert_eq!(table.for_tier(EnemyTier::Boss), 313);
    assert_eq!(table.for_tier(EnemyTier::Pinnacle), 1700);
    assert_eq!(table.for_tier(EnemyTier::Uber), 1700);
}
