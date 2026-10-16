use pobr_core::calc::MinimalInput;
use pobr_data::{catalog::UtilityBaseStats, prelude::*};

#[test]
fn ultimate_flask_local_properties_and_global_bonuses_are_separate() {
    let mut session = super::support::session(MinimalInput::default());
    // Ultimate Life Flask: 920 life over 3 seconds, 75 charges, 10 per use.
    let base = UtilityBaseStats {
        life: 920.0,
        mana: 0.0,
        duration: 3.0,
        charges_max: 75.0,
        charges_used: 10.0,
    };
    let item = Item {
        base: pobr_data::item::ItemBaseId::from("Ultimate Life Flask"),
        quality: 20,
        modifier_texts: vec![
            "50% increased Amount Recovered".into(),
            "50% increased Recovery Rate".into(),
            "50% of Recovery applied Instantly".into(),
            "40% increased Charges".into(),
            "20% reduced Charges per use".into(),
        ],
        rarity: ItemRarity::Magic,
        corrupted: false,
        implicit_texts: vec![],
        enchant_texts: vec![],
        rolled_defence: RolledDefence::default(),
        socketed: vec![],
        parsed_stats: vec![],
    };
    session.add_utility_base("Flask 1", &item, &base);
    session.add_flask_charm("Flask 1", &item);
    let other = Item {
        modifier_texts: vec![],
        quality: 0,
        ..item.clone()
    };
    session.add_utility_base("Flask 2", &other, &base);
    session.add_flask_charm("Flask 2", &other);
    session
        .add_modifier_texts(&[
            "20% increased Life Recovery from Flasks",
            "20% of Flask Recovery applied Instantly",
            "25% increased Flask Charges gained",
            "Flasks gain 0.2 charges per Second",
            "10% reduced Flask Charges used",
        ])
        .unwrap();
    session.perform_minimal().unwrap();
    let out = &session.output().utility_stats;
    let near = |id: &str, expected: f64| {
        assert!(
            (out[id] - expected).abs() < 1e-8,
            "{id}: {} != {expected}",
            out[id]
        )
    };
    // 920 * 1.2 quality * 1.5 local recovery * 1.2 global recovery = 1987.2.
    near("Flask1Recovery", 1987.2);
    near("Flask1InstantRecovery", 1391.04); // 50% local + 20% global.
    near("Flask1Duration", 2.0); // 3 / 1.5 local recovery rate.
    near("Flask1RecoveryPerSecond", 298.08);
    near("Flask1ChargesMax", 105.0);
    near("Flask1ChargesUsed", 7.0); // floor(floor(10 * .8) * .9).
    near("Flask1Uses", 15.0);
    near("Flask1ChargeGen", 0.25); // .2 * 1.25 global charge gain.
    near("Flask2Recovery", 1104.0);
    near("Flask2Duration", 3.0);
    near("Flask2ChargesMax", 75.0);
    near("Flask2ChargesUsed", 9.0);
    assert!(
        session.unsupported_modifier_texts().is_empty(),
        "{:?}",
        session.unsupported_modifier_texts()
    );
    let details = session.derived_detail("Flask1Recovery").unwrap();
    let input = |id: &str| details.inputs.iter().find(|(key, _)| key == id).unwrap().1;
    assert!(
        (input("UtilityBaseRecovery")
            * input("UtilityQualityMod")
            * input("UtilityLocalRecoveryMod")
            * input("UtilityRecoveryMod")
            * input("UtilityEffectMod")
            - out["Flask1Recovery"])
            .abs()
            < 1e-8
    );
    assert_eq!(session.detail_sources("LifeFlaskChargeGen").len(), 2);
}
