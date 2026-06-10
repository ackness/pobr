use pobr_core::calc::{CalculationSession, MinimalInput};
use pobr_core::{
    CalcConfig, CampaignProgress, CampaignReward, CampaignState, CharacterBase, ModDb,
};
use pobr_data::prelude::*;

fn db_of(mods: Vec<pobr_core::Modifier>) -> ModDb {
    let mut db = ModDb::new();
    db.add_list(mods);
    db
}

#[test]
fn resistance_penalty_table_matches_campaign_progress() {
    assert_eq!(CampaignProgress::Act1.resistance_penalty(), 0.0);
    assert_eq!(CampaignProgress::Act2.resistance_penalty(), -10.0);
    assert_eq!(CampaignProgress::Act3.resistance_penalty(), -20.0);
    assert_eq!(CampaignProgress::Act4.resistance_penalty(), -30.0);
    assert_eq!(
        CampaignProgress::Interlude54To59.resistance_penalty(),
        -40.0
    );
    assert_eq!(
        CampaignProgress::Interlude60To64.resistance_penalty(),
        -50.0
    );
    assert_eq!(CampaignProgress::Endgame.resistance_penalty(), -60.0);
}

#[test]
fn from_resistance_penalty_roundtrips_all_tiers() {
    // PoB2 `resistancePenalty` list 七档值 → 进度 → 惩罚值闭环一致。
    for value in [0.0, -10.0, -20.0, -30.0, -40.0, -50.0, -60.0] {
        let progress =
            CampaignProgress::from_resistance_penalty(value).expect("档位表内的值应可反查");
        assert_eq!(progress.resistance_penalty(), value);
    }
}

#[test]
fn from_resistance_penalty_rejects_unknown_values() {
    // 不在 PoB2 档位表内的值返回 None（调用方回退默认 Endgame）。
    assert_eq!(CampaignProgress::from_resistance_penalty(-15.0), None);
    assert_eq!(CampaignProgress::from_resistance_penalty(10.0), None);
}

#[test]
fn resistance_penalty_applies_base_modifier_to_three_elements_only() {
    let db = db_of(CampaignProgress::Act3.modifiers());
    let cfg = CalcConfig::new();

    assert_eq!(
        db.sum(ModType::Base, &cfg, &[ModName::from("FireResistance")]),
        -20.0
    );
    assert_eq!(
        db.sum(ModType::Base, &cfg, &[ModName::from("ColdResistance")]),
        -20.0
    );
    assert_eq!(
        db.sum(ModType::Base, &cfg, &[ModName::from("LightningResistance")]),
        -20.0
    );
    // Chaos resistance is not penalised by campaign progress.
    assert_eq!(
        db.sum(ModType::Base, &cfg, &[ModName::from("ChaosResistance")]),
        0.0
    );
}

#[test]
fn act1_resistance_penalty_emits_no_modifiers() {
    assert!(CampaignProgress::Act1.modifiers().is_empty());
}

#[test]
fn resistance_penalty_modifiers_carry_campaign_reward_source() {
    for modifier in CampaignProgress::Endgame.modifiers() {
        let origin = modifier.origin.as_ref().expect("penalty carries an origin");
        assert_eq!(origin.source_id.kind, SourceKind::CampaignReward);
        assert_eq!(origin.source_id.id, "campaign.resistance_penalty");
    }
}

#[test]
fn fixed_resistance_rewards_produce_expected_modifiers() {
    let cfg = CalcConfig::new();

    let fire = db_of(CampaignReward::TheFlameCore.modifiers());
    assert_eq!(
        fire.sum(ModType::Base, &cfg, &[ModName::from("FireResistance")]),
        10.0
    );

    let cold = db_of(CampaignReward::HeadOfTheWinterWolf.modifiers());
    assert_eq!(
        cold.sum(ModType::Base, &cfg, &[ModName::from("ColdResistance")]),
        10.0
    );

    let lightning = db_of(CampaignReward::SistersOfGarukhan.modifiers());
    assert_eq!(
        lightning.sum(ModType::Base, &cfg, &[ModName::from("LightningResistance")]),
        10.0
    );
}

#[test]
fn campaign_reward_carries_stable_source_id() {
    let mods = CampaignReward::TheFlameCore.modifiers();
    let origin = mods[0].origin.as_ref().unwrap();
    assert_eq!(origin.source_id.kind, SourceKind::CampaignReward);
    assert_eq!(origin.source_id.id, "campaign.the_flame_core");
}

#[test]
fn campaign_state_flattens_penalty_and_rewards() {
    let state = CampaignState {
        progress: CampaignProgress::Act3,
        rewards: vec![CampaignReward::TheFlameCore],
    };
    let db = db_of(state.modifiers());
    let cfg = CalcConfig::new();

    // -20 penalty + 10 reward = -10 fire; cold/lightning only penalised.
    assert_eq!(
        db.sum(ModType::Base, &cfg, &[ModName::from("FireResistance")]),
        -10.0
    );
    assert_eq!(
        db.sum(ModType::Base, &cfg, &[ModName::from("ColdResistance")]),
        -20.0
    );
}

#[test]
fn session_ingests_character_base_and_campaign_modifiers() {
    let mut session = CalculationSession::new(MinimalInput::default());

    let base = CharacterBase {
        level: 1,
        strength: 15.0,
        dexterity: 7.0,
        intelligence: 7.0,
    };
    let state = CampaignState {
        progress: CampaignProgress::Act3,
        rewards: vec![CampaignReward::TheFlameCore],
    };

    session.add_modifiers(base.modifiers());
    session.add_modifiers(state.modifiers());

    let output = session.perform_minimal();

    // Character base life: 12*1 + 16 + 2*15 = 58（PoB2 `Life BASE 12 × Level + 16`）。
    assert_eq!(output.life, 58.0);
    // Fire resistance: Act3 penalty (-20) + The Flame Core (+10) = -10.
    assert_eq!(output.fire_resistance, -10.0);
    assert_eq!(output.cold_resistance, -20.0);
}
