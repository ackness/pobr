use pobr_data::monster::EnemyTier;
use pobr_gamedata::{GameData, current_data_dir};

#[test]
fn curated_poise_percentages_are_positive_and_route_by_tier() {
    let table = GameData::new(current_data_dir())
        .boss_poise_thresholds()
        .unwrap();
    assert!(table.boss > 0 && table.pinnacle > 0 && table.uber > 0);
    assert_eq!(table.for_tier(EnemyTier::None), 100);
    for (tier, value) in [
        (EnemyTier::Boss, table.boss),
        (EnemyTier::Pinnacle, table.pinnacle),
        (EnemyTier::Uber, table.uber),
    ] {
        assert_eq!(table.for_tier(tier), value);
    }
}
