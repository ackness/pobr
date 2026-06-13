//! 召唤物链路端到端集成测试（M5a Track A/B/C）。
//!
//! 覆盖：BuildData 召唤物查询 API（A5）、orchestrator 识别召唤宝石 →
//! `OutputTable.minions` 非空（B2）、createMinionSkills + 主技能喂 offence（C1/C2）、
//! modDB 装配补全（C3）。golden 数值取 PoB2 同参数面板（oracle 中间值注明）。

use pobr_build::BuildData;
use pobr_gamedata::{GameData, repo_data_root};

const VERSION: &str = "4.5.0.3.4";

fn load_data() -> BuildData {
    let data = GameData::new(repo_data_root().join(VERSION));
    BuildData::load(&data).expect("加载 4.5.0.3.4 数据")
}

// ---------------------------------------------------------------------------
// A5：BuildData 召唤物查询 API
// ---------------------------------------------------------------------------

#[test]
fn build_data_minion_def_zombie() {
    let data = load_data();
    let zombie = data.minion_def("RaisedZombie").expect("RaisedZombie 在库");
    assert_eq!(zombie.life, 0.7); // Minions.lua:12
    assert_eq!(zombie.damage, 0.75); // :18
    assert!(zombie.base_damage_ignores_attack_speed);
}

#[test]
fn build_data_minion_def_spectre_falls_back() {
    let data = load_data();
    // spectre key = 完整 metadata 路径（minions.json miss → spectres.json）
    let c = data
        .minion_def("Metadata/Monsters/LeagueAbyss/Lightless/Cocoon3Spectre")
        .expect("Lightless Abomination 在库（落 spectres）");
    assert_eq!(c.life, 3.0); // Spectres.lua
    assert_eq!(c.armour, 0.4);
}

#[test]
fn build_data_effect_minion_list() {
    let data = load_data();
    assert_eq!(
        data.effect_minion_list("RaiseZombiePlayer"),
        ["RaisedZombie"]
    );
    assert_eq!(
        data.effect_minion_list("RagingSpiritsPlayer"),
        ["SummonedRagingSpirit"]
    );
    // 非召唤技能 → 空切片
    assert!(data.effect_minion_list("FireballPlayer").is_empty());
    assert!(data.effect_minion_list("NonexistentSkill").is_empty());
}

// Track B/C 端到端测试在 orchestrator 接线 commit 中追加（见本文件后续段）。
