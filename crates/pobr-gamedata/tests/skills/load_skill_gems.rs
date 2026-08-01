use pobr_gamedata::{GameData, repo_data_root};

fn version() -> String {
    pobr_gamedata::data_version()
}

fn game_data() -> GameData {
    GameData::new(repo_data_root().join(version()))
}

#[test]
fn manifest_lists_skill_gem_domains() {
    let manifest = game_data().manifest().expect("manifest 可加载");
    assert!(manifest.domains.base.iter().any(|d| d == "skill_gems"));
    assert!(manifest.domains.base.iter().any(|d| d == "granted_effects"));
    assert!(
        manifest
            .domains
            .base
            .iter()
            .any(|d| d == "granted_effect_levels"),
        "manifest 应声明 granted_effect_levels 域"
    );
}

#[test]
fn skill_gems_load_with_identity_from_base_item() {
    let gems = game_data().skill_gems().expect("skill_gems 可加载");
    assert!(gems.len() > 500, "应有数百枚宝石，实得 {}", gems.len());

    // Known active-skill gems: Fireball / Ice Nova.
    let fireball = gems
        .iter()
        .find(|g| g.id.ends_with("SkillGemFireball"))
        .expect("存在 Fireball 宝石");
    assert!(fireball.id.starts_with("Metadata/Items/Gem"));
    assert!(!fireball.is_support, "Fireball 是主动技能而非辅助");

    let ice_nova = gems
        .iter()
        .find(|g| g.id.ends_with("SkillGemIceNova"))
        .expect("存在 Ice Nova 宝石");
    assert!(!ice_nova.is_support);
    assert!(ice_nova.int_pct > 0, "Ice Nova 应有智慧需求");

    // A support gem is marked by GemType==1.
    let support = gems
        .iter()
        .find(|g| g.id.contains("SupportGem"))
        .expect("存在辅助宝石");
    assert!(support.is_support);

    // Placeholder entries (e.g. [DNT/UNUSED]) are already filtered out.
    assert!(
        gems.iter().all(|g| !g.id.is_empty()),
        "不应包含空 id 占位条目"
    );
}

#[test]
fn skill_gems_sorted_by_id_for_stable_diffs() {
    let gems = game_data().skill_gems().unwrap();
    let mut sorted = gems.clone();
    sorted.sort_by(|a, b| a.id.cmp(&b.id));
    assert_eq!(gems, sorted, "skill_gems.json 应按 id 排序");
}

#[test]
fn granted_effects_load_with_resolved_active_skill() {
    let effects = game_data()
        .granted_effects()
        .expect("granted_effects 可加载");
    assert!(
        effects.len() > 1000,
        "应有数千条授予效果，实得 {}",
        effects.len()
    );

    // An active-skill effect: the ActiveSkill FK resolves to a string id (not an integer index).
    let fireball = effects
        .iter()
        .find(|e| e.id == "FireballPlayer")
        .expect("存在 FireballPlayer 授予效果");
    assert!(!fireball.is_support);
    assert_eq!(fireball.active_skill.as_deref(), Some("fireball"));
    // The StatSet foreign-key index is already extracted (the entry point
    // for resolving damage stats, pending the stat-set table's download).
    assert!(fireball.stat_set.is_some(), "主动技能应有 StatSet 索引");

    // A support effect has no linked active skill.
    let support = effects
        .iter()
        .find(|e| e.is_support)
        .expect("存在辅助授予效果");
    assert!(support.active_skill.is_none(), "辅助效果不应链接主动技能");
}

#[test]
fn granted_effects_sorted_by_id_for_stable_diffs() {
    let effects = game_data().granted_effects().unwrap();
    let mut sorted = effects.clone();
    sorted.sort_by(|a, b| a.id.cmp(&b.id));
    assert_eq!(effects, sorted, "granted_effects.json 应按 id 排序");
}

#[test]
fn granted_effect_levels_load_with_ascending_levels() {
    let levels = game_data()
        .granted_effect_levels()
        .expect("granted_effect_levels 可加载");
    assert!(
        levels.len() > 1000,
        "应有数千个效果的分等级数据，实得 {}",
        levels.len()
    );

    // A known skill: ExplosiveGrenadePlayer should have multiple levels, ascending by level.
    let rows = levels
        .get("ExplosiveGrenadePlayer")
        .expect("存在 ExplosiveGrenadePlayer 分等级数据");
    assert!(rows.len() >= 20, "应有 >=20 级，实得 {}", rows.len());
    assert!(
        rows.windows(2).all(|w| w[0].level <= w[1].level),
        "分等级数组应按 level 升序"
    );

    // This skill is cooldown-driven (Cooldown 5000ms), with cost increasing by level.
    let l1 = rows.iter().find(|r| r.level == 1).expect("L1 存在");
    let l20 = rows.iter().find(|r| r.level == 20).expect("L20 存在");
    assert_eq!(l1.cooldown_ms, Some(5000));
    assert!(!l1.cost_amounts.is_empty(), "应有消耗量");
    assert!(
        l20.cost_amounts.first() >= l1.cost_amounts.first(),
        "高等级消耗应不低于低等级"
    );
}

#[test]
fn skill_displayed_names_available_for_localization() {
    let names = game_data()
        .skill_names("zh-TW")
        .expect("zh-TW 技能边车可加载");
    assert!(!names.is_empty());
    // 裂地之擊 (the zh-TW name) = Ground Slam
    assert_eq!(
        names.get("ground_slam").map(String::as_str),
        Some("裂地之擊")
    );
}
