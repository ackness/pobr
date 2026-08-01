use pobr_gamedata::{GameData, repo_data_root};

fn version() -> String {
    pobr_gamedata::data_version()
}

fn game_data() -> GameData {
    GameData::new(repo_data_root().join(version()))
}

#[test]
fn manifest_lists_skill_gem_domains() {
    let manifest = game_data().manifest().expect("manifest should load");
    assert!(manifest.domains.base.iter().any(|d| d == "skill_gems"));
    assert!(manifest.domains.base.iter().any(|d| d == "granted_effects"));
    assert!(
        manifest
            .domains
            .base
            .iter()
            .any(|d| d == "granted_effect_levels"),
        "manifest should declare the granted_effect_levels domain"
    );
}

#[test]
fn skill_gems_load_with_identity_from_base_item() {
    let gems = game_data().skill_gems().expect("skill_gems should load");
    assert!(
        gems.len() > 500,
        "should have hundreds of gems, got {}",
        gems.len()
    );

    // Known active-skill gems: Fireball / Ice Nova.
    let fireball = gems
        .iter()
        .find(|g| g.id.ends_with("SkillGemFireball"))
        .expect("Fireball gem should exist");
    assert!(fireball.id.starts_with("Metadata/Items/Gem"));
    assert!(
        !fireball.is_support,
        "Fireball is an active skill, not a support"
    );

    let ice_nova = gems
        .iter()
        .find(|g| g.id.ends_with("SkillGemIceNova"))
        .expect("Ice Nova gem should exist");
    assert!(!ice_nova.is_support);
    assert!(
        ice_nova.int_pct > 0,
        "Ice Nova should have an intelligence requirement"
    );

    // A support gem is marked by GemType==1.
    let support = gems
        .iter()
        .find(|g| g.id.contains("SupportGem"))
        .expect("a support gem should exist");
    assert!(support.is_support);

    // Placeholder entries (e.g. [DNT/UNUSED]) are already filtered out.
    assert!(
        gems.iter().all(|g| !g.id.is_empty()),
        "should not contain empty-id placeholder entries"
    );
}

#[test]
fn skill_gems_sorted_by_id_for_stable_diffs() {
    let gems = game_data().skill_gems().unwrap();
    let mut sorted = gems.clone();
    sorted.sort_by(|a, b| a.id.cmp(&b.id));
    assert_eq!(gems, sorted, "skill_gems.json should be sorted by id");
}

#[test]
fn granted_effects_load_with_resolved_active_skill() {
    let effects = game_data()
        .granted_effects()
        .expect("granted_effects should load");
    assert!(
        effects.len() > 1000,
        "should have thousands of granted effects, got {}",
        effects.len()
    );

    // An active-skill effect: the ActiveSkill FK resolves to a string id (not an integer index).
    let fireball = effects
        .iter()
        .find(|e| e.id == "FireballPlayer")
        .expect("FireballPlayer granted effect should exist");
    assert!(!fireball.is_support);
    assert_eq!(fireball.active_skill.as_deref(), Some("fireball"));
    // The StatSet foreign-key index is already extracted (the entry point
    // for resolving damage stats, pending the stat-set table's download).
    assert!(
        fireball.stat_set.is_some(),
        "an active skill should have a StatSet index"
    );

    // A support effect has no linked active skill.
    let support = effects
        .iter()
        .find(|e| e.is_support)
        .expect("a support granted effect should exist");
    assert!(
        support.active_skill.is_none(),
        "a support effect should not link an active skill"
    );
}

#[test]
fn granted_effects_sorted_by_id_for_stable_diffs() {
    let effects = game_data().granted_effects().unwrap();
    let mut sorted = effects.clone();
    sorted.sort_by(|a, b| a.id.cmp(&b.id));
    assert_eq!(
        effects, sorted,
        "granted_effects.json should be sorted by id"
    );
}

#[test]
fn granted_effect_levels_load_with_ascending_levels() {
    let levels = game_data()
        .granted_effect_levels()
        .expect("granted_effect_levels should load");
    assert!(
        levels.len() > 1000,
        "should have per-level data for thousands of effects, got {}",
        levels.len()
    );

    // A known skill: ExplosiveGrenadePlayer should have multiple levels, ascending by level.
    let rows = levels
        .get("ExplosiveGrenadePlayer")
        .expect("ExplosiveGrenadePlayer per-level data should exist");
    assert!(
        rows.len() >= 20,
        "should have >=20 levels, got {}",
        rows.len()
    );
    assert!(
        rows.windows(2).all(|w| w[0].level <= w[1].level),
        "per-level array should be ascending by level"
    );

    // This skill is cooldown-driven (Cooldown 5000ms), with cost increasing by level.
    let l1 = rows.iter().find(|r| r.level == 1).expect("L1 should exist");
    let l20 = rows
        .iter()
        .find(|r| r.level == 20)
        .expect("L20 should exist");
    assert_eq!(l1.cooldown_ms, Some(5000));
    assert!(!l1.cost_amounts.is_empty(), "should have a cost amount");
    assert!(
        l20.cost_amounts.first() >= l1.cost_amounts.first(),
        "higher-level cost should not be lower than lower-level cost"
    );
}

#[test]
fn skill_displayed_names_available_for_localization() {
    let names = game_data()
        .skill_names("zh-TW")
        .expect("zh-TW skill name sidecar should load");
    assert!(!names.is_empty());
    // 裂地之擊 (the zh-TW name) = Ground Slam
    assert_eq!(
        names.get("ground_slam").map(String::as_str),
        Some("裂地之擊")
    );
}
