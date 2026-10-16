use pobr_core::calc::{RecoupResource, calc_recoup_from_db};
use pobr_core::{CalcConfig, ModDb, Modifier};
use pobr_data::modifier::ModType;

#[test]
fn life_recoup_transfer_adds_to_es_percentage_with_es_rate_and_duration() {
    let mut db = ModDb::new();
    db.add_list(vec![
        Modifier::number("LifeRecoup", ModType::Base, 20.0),
        Modifier::number("EnergyShieldRecoup", ModType::Base, 5.0),
        Modifier::number("LifeRecoveryRate", ModType::Inc, 300.0),
        Modifier::number("EnergyShieldRecoveryRate", ModType::Inc, 100.0),
        Modifier::flag("4SecondEnergyShieldRecoup"),
    ]);
    let cfg = CalcConfig::new();
    let rate =
        |db: &ModDb, resource| calc_recoup_from_db(db, &cfg, 1000.0, resource).rate_per_second;
    assert_eq!(rate(&db, RecoupResource::EnergyShield), 25.0);
    let life = rate(&db, RecoupResource::Life);
    db.add_mod(Modifier::flag("LifeRecoupAlsoAppliesToEnergyShield"));
    assert_eq!(rate(&db, RecoupResource::EnergyShield), 125.0);
    assert_eq!(rate(&db, RecoupResource::Life), life);
}
