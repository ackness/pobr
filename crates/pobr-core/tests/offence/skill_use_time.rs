use pobr_core::calc::skill_use_time::calc_skill_use_time;
use pobr_core::{CalcConfig, ModDb, Modifier};
use pobr_data::prelude::*;

#[test]
fn skill_speed_bucket_is_additive() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("AttackSpeed", ModType::Inc, 20.0));
    db.add_mod(Modifier::number("SkillSpeed", ModType::Inc, 10.0));

    // base use time 1.0s; +30% skill speed → use time 1/1.3 ≈ 0.769s, rate ≈ 1.3/s.
    let result = calc_skill_use_time(&db, &CalcConfig::attack(), 1.0, 0.0, false);

    assert_eq!(result.total_skill_speed, 30.0);
    assert!((result.tooltip_rate - 1.3).abs() < 1e-6);
}

#[test]
fn action_speed_is_an_independent_multiplier() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("ActionSpeed", ModType::Inc, 50.0));

    // base rate 2/s (use time 0.5s), action speed *1.5 → 3/s.
    let result = calc_skill_use_time(&db, &CalcConfig::attack(), 0.5, 0.0, false);

    assert_eq!(result.total_action_speed, 50.0);
    assert!((result.effective_rate - 3.0).abs() < 1e-6);
}

#[test]
fn non_channelling_is_capped_at_server_frame_rate() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("AttackSpeed", ModType::Inc, 900.0));

    // base 5/s * (1 + 9) = 50/s, capped to ~30.3/s for non-channelling.
    let result = calc_skill_use_time(&db, &CalcConfig::attack(), 0.2, 0.0, false);

    assert!(result.capped_by_server_tick);
    let server_rate = 1.0 / SERVER_TICK_SECONDS;
    assert!((result.effective_rate - server_rate).abs() < 1e-6);
}

#[test]
fn channelling_skills_bypass_the_server_frame_cap() {
    let mut db = ModDb::new();
    db.add_mod(Modifier::number("AttackSpeed", ModType::Inc, 900.0));

    let result = calc_skill_use_time(&db, &CalcConfig::attack(), 0.2, 0.0, true);

    assert!(!result.capped_by_server_tick);
    assert!(result.effective_rate > 1.0 / SERVER_TICK_SECONDS);
}
