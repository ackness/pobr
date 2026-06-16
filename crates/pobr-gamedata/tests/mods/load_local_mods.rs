//! `overlay/local_mods.json` 加载测试。
//!
//! 搬迁不变式校验：JSON 内容必须与内建 fallback [`LocalModsDef::default`]
//! （= `pobr-build::calc_orchestrator::is_weapon_local_mod` 原硬编码枚举的
//! 镜像）**逐值相等**——两条路径（读文件 / 缺文件降级）行为恒一致，
//! 接线切换零 parity 变化。

use pobr_data::catalog::local_mods::LocalModsDef;
use pobr_gamedata::{GameData, repo_data_root};

fn version() -> String {
    pobr_gamedata::data_version()
}

fn load() -> LocalModsDef {
    GameData::new(repo_data_root().join(version()))
        .local_mods()
        .expect("local_mods 可加载")
}

/// JSON 与内建 fallback 逐值相等（搬迁不变式的核心断言：文件路径与
/// 降级路径必须给出同一份白名单）。
#[test]
fn json_matches_builtin_fallback_mirror() {
    assert_eq!(load(), LocalModsDef::default());
}

/// 武器白名单逐值 = 原 `is_weapon_local_mod` 硬编码枚举（小写 clean 文本口径）：
/// 两个 increased 后缀 + 一个 adds 伤害后缀。
#[test]
fn weapon_whitelist_matches_original_hardcode() {
    let def = load();
    assert_eq!(
        def.weapon.increased_suffixes,
        vec![
            "% increased physical damage".to_string(),
            "% increased attack speed".to_string(),
        ]
    );
    assert_eq!(
        def.weapon.adds_damage_suffixes,
        vec!["physical damage".to_string()]
    );
}

/// 白名单条目必须全小写（`clean_item_text` 产物口径，大小写敏感匹配）。
#[test]
fn whitelist_entries_are_lowercase() {
    let def = load();
    for entry in def
        .weapon
        .increased_suffixes
        .iter()
        .chain(&def.weapon.adds_damage_suffixes)
    {
        assert_eq!(
            entry,
            &entry.to_lowercase(),
            "白名单条目 {entry:?} 含大写字符"
        );
    }
}
