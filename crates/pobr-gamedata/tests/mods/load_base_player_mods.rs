//! Loads base_player_mods.json and cross-checks it value-for-value against
//! pobr's existing Rust source of truth (a migration invariant).
//!
//! Source-of-truth distribution (all 14 entries in this table have a Rust
//! source of truth, and the JSON value must be value-equal to it):
//! - Charges: `pobr-core/src/calc/survivability.rs`
//!   (`DEFAULT_MAX_CHARGES` / `DEFAULT_CHARGE_DURATION_SECONDS`);
//! - Curse/mark limits: `pobr-core/src/calc/buff_pass.rs`
//!   (`DEFAULT_ENEMY_CURSE_LIMIT` / `DEFAULT_ENEMY_MARK_LIMIT`);
//! - Level-derived baselines: `pobr-core/src/character.rs` (`CharacterBase`
//!   formula constants);
//! - Crit damage baseline: `pobr-data/src/constants.rs`
//!   (`PLAYER_BASE_CRIT_DAMAGE_BONUS`);
//! - Endgame resistance penalty: `pobr-core/src/campaign.rs`
//!   (`CampaignProgress::Endgame`, matching
//!   `pobr-build/src/calc_orchestrator.rs`'s `ENDGAME_RESISTANCE_PENALTY`
//!   at −60).
//!
//! See each assertion's comment for the vendor cross-reference line number
//! (`vendor/PathOfBuilding-PoE2/src/Modules/CalcSetup.lua`).

use pobr_core::CharacterBase;
use pobr_core::calc::survivability::{DEFAULT_CHARGE_DURATION_SECONDS, DEFAULT_MAX_CHARGES};
use pobr_core::campaign::CampaignProgress;
use pobr_core::modifier::ModValue;
use pobr_data::catalog::base_player_mods::{BasePlayerModDef, BasePlayerModTag};
use pobr_data::catalog::character_constants::CharacterConstantsDef;
use pobr_data::constants::PLAYER_BASE_CRIT_DAMAGE_BONUS;
use pobr_data::modifier::ModType;
use pobr_gamedata::{GameData, repo_data_root};

fn version() -> String {
    pobr_gamedata::data_version()
}

fn load() -> Vec<BasePlayerModDef> {
    GameData::new(repo_data_root().join(version()))
        .base_player_mods()
        .expect("base_player_mods 可加载")
}

/// Gets an entry by id (fails if absent).
fn entry<'a>(mods: &'a [BasePlayerModDef], id: &str) -> &'a BasePlayerModDef {
    mods.iter()
        .find(|m| m.id == id)
        .unwrap_or_else(|| panic!("缺少条目 {id}"))
}

/// A `CharacterBase` with zero attributes (isolates the pure level-derived term).
fn bare_character(level: u32) -> CharacterBase {
    CharacterBase {
        level,
        strength: 0.0,
        dexterity: 0.0,
        intelligence: 0.0,
    }
}

#[test]
fn loads_fourteen_entries_with_unique_ids_and_base_type() {
    let mods = load();
    assert_eq!(mods.len(), 14, "当前收录 14 条有 Rust 准源的基线 mod");

    let mut ids: Vec<&str> = mods.iter().map(|m| m.id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 14, "条目 id 应唯一");

    for m in &mods {
        assert_eq!(m.mod_type, ModType::Base, "{}: 基线条目均为 BASE", m.id);
        assert_eq!(m.flags, 0, "{}: 当前收录条目均无 ModFlags 限定", m.id);
        assert_eq!(
            m.keyword_flags, 0,
            "{}: 当前收录条目均无 KeywordFlags",
            m.id
        );
    }
}

/// Charge baselines == survivability.rs's existing constants.
/// vendor: CalcSetup.lua:31 (ChargeDuration 15) / :32-34 (the three charge
/// types' cap, 3).
#[test]
fn charge_baselines_match_survivability_constants() {
    let mods = load();

    let duration = entry(&mods, "charge_duration");
    assert_eq!(duration.mod_name, "ChargeDuration");
    assert_eq!(duration.value, DEFAULT_CHARGE_DURATION_SECONDS); // = 15.0

    for (id, name) in [
        ("power_charges_max", "PowerChargesMax"),
        ("frenzy_charges_max", "FrenzyChargesMax"),
        ("endurance_charges_max", "EnduranceChargesMax"),
    ] {
        let e = entry(&mods, id);
        assert_eq!(e.mod_name, name);
        assert_eq!(e.value, f64::from(DEFAULT_MAX_CHARGES)); // = 3.0
        assert!(e.tags.is_empty(), "{id}: 充能上限无 tag");
    }
}

/// Curse/mark limit baselines == buff_pass.rs's existing constants (the
/// curse limit's baseline term: `EnemyCurseLimit = baseline 1 + Σ BASE
/// mods`, with buff_pass adding the constant on top of the Sum; this table
/// migrates it into an equivalent mod entry).
/// vendor: CalcSetup.lua:648 (`NewMod("EnemyCurseLimit","BASE",1,"Base")`) /
/// :649 (same for `EnemyMarkLimit`).
#[test]
fn curse_and_mark_limit_baselines_match_buff_pass_constants() {
    use pobr_core::calc::buff_pass::{DEFAULT_ENEMY_CURSE_LIMIT, DEFAULT_ENEMY_MARK_LIMIT};

    let mods = load();
    let curse = entry(&mods, "enemy_curse_limit");
    assert_eq!(curse.mod_name, "EnemyCurseLimit");
    assert_eq!(curse.value, DEFAULT_ENEMY_CURSE_LIMIT); // = 1.0
    assert!(curse.tags.is_empty());

    let mark = entry(&mods, "enemy_mark_limit");
    assert_eq!(mark.mod_name, "EnemyMarkLimit");
    assert_eq!(mark.value, DEFAULT_ENEMY_MARK_LIMIT); // = 1.0
    assert!(mark.tags.is_empty());
}

/// Level-derived baselines (life/mana/accuracy per level + base, inherent
/// evasion) == character.rs's formula constants.
///
/// `CharacterBase`'s constants are private, so they're reconstructed by
/// differencing the public formula: `per_level = f(level=2) − f(level=1)`,
/// `base constant = f(level=0)` (with attributes zeroed).
/// vendor: CalcSetup.lua:615 (Life 12/level, base 16) / :616 (Mana 4/level,
/// base 30) / :621 (Evasion 7) / :622 (Accuracy 6/level, base −6).
/// Naming discrepancy: pobr uses `MaximumLife`/`MaximumMana` (vendor uses
/// `Life`/`Mana`) — pobr's naming is kept per the migration invariant.
#[test]
fn level_derived_baselines_match_character_base_formulas() {
    let mods = load();
    let c0 = bare_character(0);
    let c1 = bare_character(1);
    let c2 = bare_character(2);
    // The formula coefficients ultimately come from injection; here we
    // take the Default fallback (value-equal to the JSON).
    let cc = CharacterConstantsDef::default();

    // Entries of the form "value/level + base": compared field by field
    // against the (difference, intercept) of the matching formula.
    type LevelFormula = fn(&CharacterBase, &CharacterConstantsDef) -> f64;
    let cases: [(&str, &str, LevelFormula); 3] = [
        (
            "base_life_per_level",
            "MaximumLife",
            CharacterBase::base_life,
        ),
        (
            "base_mana_per_level",
            "MaximumMana",
            CharacterBase::base_mana,
        ),
        (
            "base_accuracy_per_level",
            "Accuracy",
            CharacterBase::base_accuracy,
        ),
    ];
    for (id, name, f) in cases {
        let e = entry(&mods, id);
        assert_eq!(e.mod_name, name, "{id}: ModName 对齐 character.rs 注入名");
        assert_eq!(
            e.value,
            f(&c2, &cc) - f(&c1, &cc),
            "{id}: per-level 值 == 公式差分"
        );
        match e.tags.as_slice() {
            [
                BasePlayerModTag::Multiplier {
                    var,
                    div,
                    limit,
                    base,
                },
            ] => {
                assert_eq!(var, "Level", "{id}: 按等级缩放");
                assert_eq!(*div, 1.0, "{id}: 每 1 级缩放一次");
                assert_eq!(*limit, None, "{id}: 无缩放上限");
                assert_eq!(*base, Some(f(&c0, &cc)), "{id}: base 常量 == 公式截距");
            }
            other => panic!("{id}: 应恰有一个 Multiplier:Level tag，实得 {other:?}"),
        }
    }

    // Inherent evasion: compared value-for-value against the Evasion BASE
    // `CharacterBase::modifiers()` injects.
    let evasion = entry(&mods, "base_evasion");
    assert_eq!(evasion.mod_name, "Evasion");
    assert!(evasion.tags.is_empty(), "固有闪避与等级无关");
    let injected = c1
        .modifiers(&cc)
        .into_iter()
        .find(|m| m.name == "Evasion".into())
        .expect("CharacterBase::modifiers() 注入 Evasion BASE");
    assert_eq!(ModValue::Number(evasion.value), injected.value); // = 7.0
}

/// Crit damage baseline == pobr-data's existing constant (crit.rs adds the
/// `PLAYER_BASE_CRIT_DAMAGE_BONUS` constant on top of
/// `Sum(BASE, CritMultiplier)`; this table migrates it into an equivalent
/// mod entry).
/// vendor: CalcSetup.lua:623 + Data/Misc.lua:156
/// (base_critical_hit_damage_bonus = 100).
#[test]
fn crit_multiplier_baseline_matches_constant() {
    let mods = load();
    let e = entry(&mods, "base_crit_multiplier");
    assert_eq!(e.mod_name, "CritMultiplier");
    assert_eq!(e.value, PLAYER_BASE_CRIT_DAMAGE_BONUS); // = 100.0
    assert!(e.tags.is_empty());
}

/// Endgame elemental resistance penalty == campaign.rs's
/// `CampaignProgress::Endgame` injection (−60 ×3, matching
/// calc_orchestrator.rs's `ENDGAME_RESISTANCE_PENALTY`).
/// vendor: CalcSetup.lua:625-627 (`configInput.resistancePenalty or -60`;
/// vendor names them `FireResist` etc., pobr uses `FireResistance` etc. —
/// pobr's naming is kept per the migration invariant).
#[test]
fn resistance_penalty_matches_campaign_endgame_modifiers() {
    let mods = load();
    let injected = CampaignProgress::Endgame.modifiers();
    assert_eq!(injected.len(), 3, "Endgame 注入三元素惩罚");

    for (id, name) in [
        ("resist_penalty_fire", "FireResistance"),
        ("resist_penalty_cold", "ColdResistance"),
        ("resist_penalty_lightning", "LightningResistance"),
    ] {
        let e = entry(&mods, id);
        assert_eq!(e.mod_name, name);
        assert_eq!(e.value, CampaignProgress::Endgame.resistance_penalty()); // = -60.0
        let matched = injected
            .iter()
            .find(|m| m.name == name.into())
            .unwrap_or_else(|| panic!("campaign.rs 应注入 {name}"));
        assert_eq!(ModValue::Number(e.value), matched.value, "{id}: 逐值相等");
    }
}
