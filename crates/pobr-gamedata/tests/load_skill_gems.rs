use pobr_gamedata::{GameData, repo_data_root};

const VERSION: &str = "4.5.0.3.4";

fn game_data() -> GameData {
    GameData::new(repo_data_root().join(VERSION))
}

#[test]
fn manifest_lists_skill_gem_domains() {
    let manifest = game_data().manifest().expect("manifest 可加载");
    assert!(manifest.domains.iter().any(|d| d == "skill_gems"));
    assert!(manifest.domains.iter().any(|d| d == "granted_effects"));
}

#[test]
fn skill_gems_load_with_identity_from_base_item() {
    let gems = game_data().skill_gems().expect("skill_gems 可加载");
    assert!(gems.len() > 500, "应有数百枚宝石，实得 {}", gems.len());

    // 已知主动技能宝石：Fireball / Ice Nova。
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

    // 辅助宝石由 GemType==1 标记。
    let support = gems
        .iter()
        .find(|g| g.id.contains("SupportGem"))
        .expect("存在辅助宝石");
    assert!(support.is_support);

    // 占位条目（[DNT/UNUSED] 等）已被过滤。
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

    // 主动技能效果：ActiveSkill FK 解析为字符串 id（非整型索引）。
    let fireball = effects
        .iter()
        .find(|e| e.id == "FireballPlayer")
        .expect("存在 FireballPlayer 授予效果");
    assert!(!fireball.is_support);
    assert_eq!(fireball.active_skill.as_deref(), Some("fireball"));

    // 辅助效果无关联主动技能。
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
fn skill_displayed_names_available_for_localization() {
    let names = game_data()
        .skill_names("zh-TW")
        .expect("zh-TW 技能边车可加载");
    assert!(!names.is_empty());
    // 裂地之擊 = Ground Slam
    assert_eq!(
        names.get("ground_slam").map(String::as_str),
        Some("裂地之擊")
    );
}
