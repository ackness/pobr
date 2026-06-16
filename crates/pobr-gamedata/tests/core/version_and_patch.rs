//! 运行时版本发现 + 用户 patch 层（数据/代码隔离收尾）。

use pobr_gamedata::{GameData, current_data_dir, data_version};

/// 运行时版本发现：无 env 时落到 `data/CURRENT` 标记（仓库内 = DATA_VERSION），
/// `current_data_dir()` 指向该版本目录。
#[test]
fn data_version_resolves_and_dir_matches() {
    let v = data_version();
    assert!(!v.trim().is_empty(), "data_version 不应为空");
    // 仓库 data/CURRENT 与编译期 DATA_VERSION 一致（行为不变）。
    assert_eq!(v, pobr_gamedata::DATA_VERSION);
    assert!(
        current_data_dir().ends_with(&v),
        "current_data_dir 应以发现的版本结尾：{:?}",
        current_data_dir()
    );
}

/// 用户 patch 层：`patch/<相对路径>` 按 merge 规则叠在官方数据上——
/// 同 id 覆盖、新 id 追加；无 patch 目录 = 纯官方数据。
#[test]
fn user_patch_layer_merges_over_base() {
    let tmp = std::env::temp_dir().join(format!("pobr-patch-test-{}", std::process::id()));
    let base = tmp.join("base");
    let patch_base = tmp.join("patch").join("base");
    std::fs::create_dir_all(&base).unwrap();
    std::fs::create_dir_all(&patch_base).unwrap();

    // 官方 base：Mana(divisor=1) + Life(divisor=1)
    std::fs::write(
        base.join("cost_types.json"),
        r#"[{"id":"Mana","divisor":1},{"id":"Life","divisor":1}]"#,
    )
    .unwrap();

    // 无 patch：纯官方。
    let plain = GameData::new(&tmp)
        .cost_types()
        .expect("cost_types 无 patch");
    assert_eq!(plain.len(), 2);
    assert_eq!(plain.iter().find(|c| c.id == "Mana").unwrap().divisor, 1);

    // 用户 patch：覆盖 Mana 的 divisor + 追加自定义资源 Custom。
    std::fs::write(
        patch_base.join("cost_types.json"),
        r#"[{"id":"Mana","divisor":99},{"id":"Custom","divisor":7}]"#,
    )
    .unwrap();

    let patched = GameData::new(&tmp)
        .cost_types()
        .expect("cost_types 含 patch");
    assert_eq!(patched.len(), 3, "应为 Mana/Life/Custom 三条");
    assert_eq!(
        patched.iter().find(|c| c.id == "Mana").unwrap().divisor,
        99,
        "patch 应覆盖 Mana divisor"
    );
    assert_eq!(
        patched.iter().find(|c| c.id == "Life").unwrap().divisor,
        1,
        "未 patch 的 Life 保持"
    );
    assert!(
        patched.iter().any(|c| c.id == "Custom" && c.divisor == 7),
        "patch 应追加自定义 Custom"
    );

    std::fs::remove_dir_all(&tmp).ok();
}
