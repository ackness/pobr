//! Web JSON 契约 golden（TODO.md 0.6）：钉住 `decode_build_json` /
//! `calculate_build_json` / `attribution_json` 的 **JSON 形状**（键集合），
//! 与 `web/src/api/types.ts` 的手写 TS 类型一一对应。
//!
//! 断言键集合而非具体数值——数值随 parity 修复演进，形状才是前端契约；
//! 本测试挂 = 契约破坏，必须同步改 `web/src/api/types.ts` 再更新这里。

use pobr_gamedata::repo_data_root;
use serde_json::Value;

/// 真实 demo build（与 ninja_parity 同源）。
fn demo_code() -> String {
    let path =
        repo_data_root().join("../examples/demo-bd-test/builds/monk-invoker-frost-bomb/code.txt");
    std::fs::read_to_string(path).expect("read demo code")
}

/// 数据初始化（存储是 thread_local——cargo test 每测试一线程，各自 init）。
fn ensure_data() {
    if !pobr_wasm::is_data_ready() {
        let dir = repo_data_root().join(pobr_data::GOLDEN_PARITY_DATA_VERSION);
        pobr_wasm::init_data_from_dir(dir.to_str().unwrap()).expect("init data");
    }
}

/// 断言 JSON 对象的键集合恰好等于 `expected`（契约冻结点）。
fn assert_keys(value: &Value, expected: &[&str], label: &str) {
    let obj = value
        .as_object()
        .unwrap_or_else(|| panic!("{label}: not an object"));
    let mut actual: Vec<&str> = obj.keys().map(String::as_str).collect();
    actual.sort_unstable();
    let mut want = expected.to_vec();
    want.sort_unstable();
    assert_eq!(
        actual, want,
        "{label}: JSON 键集合变化 = 契约破坏，需同步 web/src/api/types.ts"
    );
}

#[test]
fn decode_build_json_shape() {
    let json: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(&demo_code()).expect("decode"))
            .expect("valid json");
    assert_keys(
        &json,
        &[
            "character",
            "tree",
            "items",
            "socket_groups",
            "main_socket_group",
            "config_inputs",
            "notes",
        ],
        "BuildJson",
    );
    assert_keys(
        &json["character"],
        &["level", "class_name", "ascendancy_name"],
        "character",
    );
    assert_keys(
        &json["tree"],
        &["allocated_nodes", "tree_version", "attribute_choices"],
        "tree",
    );
    assert_keys(
        &json["items"],
        &["equipped", "jewels", "socket_jewels", "flasks"],
        "items",
    );

    // 真实 build 的内容 sanity：有职业、有装备、有已加点、有技能组。
    assert!(!json["character"]["class_name"].as_str().unwrap().is_empty());
    assert!(
        !json["tree"]["allocated_nodes"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let equipped = json["items"]["equipped"].as_array().unwrap();
    assert!(!equipped.is_empty());
    assert_keys(&equipped[0], &["slot", "text"], "equipped[0]");
    let groups = json["socket_groups"].as_array().unwrap();
    assert!(!groups.is_empty());
    assert_keys(
        &groups[0],
        &["slot", "enabled", "source", "active_skill_id", "gems"],
        "socket_groups[0]",
    );
    let gems = groups[0]["gems"].as_array().unwrap();
    assert!(!gems.is_empty());
    assert_keys(&gems[0], &["skill_id", "level", "quality"], "gems[0]");
}

#[test]
fn calculate_build_json_shape() {
    ensure_data();
    let request = serde_json::json!({ "pob_code": demo_code() }).to_string();
    let json: Value =
        serde_json::from_str(&pobr_wasm::calculate_build_json(&request).expect("calculate"))
            .expect("valid json");
    assert_keys(
        &json,
        &["stats", "unsupported_modifiers", "breakdowns", "main_skill"],
        "CalculateBuildResponse",
    );

    // main_skill：真实伤害 build 必有主技能，含逐类型击中分量 + hit/dot/combined DPS。
    let main_skill = &json["main_skill"];
    assert_keys(
        main_skill,
        &[
            "group_index",
            "skill_id",
            "hit_damage",
            "hit_dps",
            "dot_dps",
            "combined_dps",
        ],
        "main_skill",
    );
    assert!(!main_skill["skill_id"].as_str().unwrap().is_empty());
    let hit_damage = main_skill["hit_damage"].as_array().unwrap();
    assert!(!hit_damage.is_empty(), "damage build should have hit parts");
    assert_keys(
        &hit_damage[0],
        &["damage_type", "min", "max", "avg"],
        "main_skill.hit_damage[0]",
    );
    assert!(main_skill["hit_dps"].as_f64().unwrap() > 0.0);

    let stats = json["stats"].as_array().unwrap();
    assert!(!stats.is_empty());
    assert_keys(&stats[0], &["id", "value", "category"], "stats[0]");

    // 侧边栏关键字段存在且有限值。
    for key in ["TotalDPS", "Life", "FireResist"] {
        let stat = stats
            .iter()
            .find(|s| s["id"] == key)
            .unwrap_or_else(|| panic!("missing display stat {key}"));
        assert!(
            stat["value"].as_f64().unwrap().is_finite(),
            "{key} not finite"
        );
    }
    // 真实 build：Life 必为正。
    let life = stats.iter().find(|s| s["id"] == "Life").unwrap();
    assert!(
        life["value"].as_f64().unwrap() > 0.0,
        "Life should be positive"
    );

    // breakdown：demo build 是 ES 系，EnergyShield 一定有词条来源（Life 类
    // 聚合名只有存在词条时才出现在 breakdowns——基础生命走 MinimalInput 注入）。
    let es_bd = &json["breakdowns"]["EnergyShield"];
    assert_keys(
        es_bd,
        &["base_total", "inc_total", "mods"],
        "breakdowns.EnergyShield",
    );
    let mods = es_bd["mods"].as_array().unwrap();
    assert!(!mods.is_empty());
    assert_keys(
        &mods[0],
        &[
            "mod_type",
            "value",
            "source_text",
            "origin_kind",
            "origin_id",
            "slot",
        ],
        "breakdown mod",
    );
}

#[test]
fn calculate_build_json_main_group_override_changes_output() {
    ensure_data();
    let base: Value = serde_json::from_str(
        &pobr_wasm::calculate_build_json(
            &serde_json::json!({ "pob_code": demo_code() }).to_string(),
        )
        .expect("baseline"),
    )
    .unwrap();
    // 额外词条注入应改变输出（demo build 是 CI/ES 系，用 ES 词条验证）。
    let overridden: Value = serde_json::from_str(
        &pobr_wasm::calculate_build_json(
            &serde_json::json!({
                "pob_code": demo_code(),
                "extra_modifiers": ["100% increased maximum Energy Shield"],
            })
            .to_string(),
        )
        .expect("override"),
    )
    .unwrap();
    let es = |v: &Value| {
        v["stats"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == "EnergyShield")
            .unwrap()["value"]
            .as_f64()
            .unwrap()
    };
    assert!(
        es(&overridden) > es(&base),
        "extra modifier 应改变输出（base={} overridden={}）",
        es(&base),
        es(&overridden)
    );
}

/// main_socket_group 覆盖（0-based）反映在 main_skill.group_index，且各组
/// hit_dps 与逐组 full_dps 的 scoped 重算同源一致。
#[test]
fn main_skill_follows_main_group_override() {
    ensure_data();
    let full: Value = serde_json::from_str(
        &pobr_wasm::full_dps_json(&serde_json::json!({ "pob_code": demo_code() }).to_string())
            .expect("full dps"),
    )
    .unwrap();
    let per_skill = full["per_skill"].as_array().unwrap();
    assert!(
        per_skill.len() >= 2,
        "demo build has multiple damage groups"
    );
    for entry in per_skill {
        let group_index = entry["group_index"].as_u64().unwrap();
        let calc: Value = serde_json::from_str(
            &pobr_wasm::calculate_build_json(
                &serde_json::json!({
                    "pob_code": demo_code(),
                    "main_socket_group": group_index,
                })
                .to_string(),
            )
            .expect("calc"),
        )
        .unwrap();
        let main_skill = &calc["main_skill"];
        assert_eq!(main_skill["group_index"].as_u64().unwrap(), group_index);
        assert_eq!(main_skill["skill_id"], entry["skill_id"]);
        // 语义不变约束：主技能口径 CombinedDPS == 逐组 full_dps 的该组数值。
        let combined = main_skill["combined_dps"].as_f64().unwrap();
        let scoped = entry["dps"].as_f64().unwrap();
        assert!(
            (combined - scoped).abs() <= scoped.abs() * 1e-9 + 1e-9,
            "group {group_index}: main-skill combined {combined} != full_dps scoped {scoped}"
        );
    }
}

/// 白手起 build（无 pob_code，PoB2 新建语义）：character 即可计算，等级驱动基础量。
#[test]
fn calculate_scratch_build_without_code() {
    ensure_data();
    let stat_at = |level: u32, extra: Value, id: &str| -> f64 {
        let mut request = serde_json::json!({
            "character": { "class_name": "Warrior", "level": level },
            "allocated_nodes": [],
        });
        if let Some(obj) = extra.as_object() {
            request["config_inputs"] = Value::Object(obj.clone());
        }
        let json: Value = serde_json::from_str(
            &pobr_wasm::calculate_build_json(&request.to_string()).expect("scratch"),
        )
        .expect("valid json");
        json["stats"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == id)
            .unwrap()["value"]
            .as_f64()
            .unwrap()
    };
    let life_at = |level: u32| stat_at(level, Value::Null, "Life");
    let level1 = life_at(1);
    let level90 = life_at(90);
    assert!(
        level1 > 0.0,
        "level 1 scratch build Life should be positive"
    );
    assert!(
        level90 > level1,
        "Life should scale with level (lv1={level1} lv90={level90})"
    );

    // 任务奖励 defaultState=true（PoB2 新建语义）：省略 = 已领取。
    // Spirit = +30 +30 +40，火抗 = -60（默认惩罚）+10（Blackjaw 奖励）。
    assert_eq!(stat_at(1, Value::Null, "Spirit"), 100.0, "default Spirit");
    assert_eq!(
        stat_at(1, Value::Null, "FireResist"),
        -50.0,
        "default FireResist"
    );
    // 显式 false = 放弃对应奖励（前端 Config 勾选通道）。
    let no_spirit = serde_json::json!({
        "questAct 1FreythornKing In The Mists": false,
        "questAct 3Azak BogIgnagduk": false,
        "questInterlude 3Kriar VillageLythara": false,
    });
    assert_eq!(
        stat_at(1, no_spirit, "Spirit"),
        1.0,
        "opted-out Spirit falls back to pool floor"
    );

    // HitChance 展示口径为百分制：空 build 无敌方闪避 → 100（而非 fraction 1.0）。
    assert_eq!(stat_at(1, Value::Null, "HitChance"), 100.0, "HitChance %");

    // 缺 code 又缺 character → 可读错误而非 panic。
    let err = pobr_wasm::calculate_build_json("{}").unwrap_err();
    assert!(
        err.contains("pob_code or character"),
        "unexpected error: {err}"
    );
}

/// 手动添加技能组 / 装备（无 code 的完整手搓路径）+ 宝石目录形状。
#[test]
fn manual_skills_and_items_without_code() {
    ensure_data();

    // 宝石目录：非空，条目形状 {skill_id, name, is_support}，含 active 与 support 两类。
    let catalog: Value =
        serde_json::from_str(&pobr_wasm::gem_catalog_json().expect("gem catalog")).unwrap();
    let entries = catalog.as_array().unwrap();
    assert!(entries.len() > 100, "catalog too small: {}", entries.len());
    assert_keys(
        &entries[0],
        &[
            "skill_id",
            "name",
            "name_zh_tw",
            "name_zh_cn",
            "colour",
            "is_support",
            "is_lineage",
        ],
        "gem catalog entry",
    );
    // 简中名边车（Phase 7.2）已接线：多数宝石应有简中名。
    let cn_count = entries
        .iter()
        .filter(|e| e["name_zh_cn"].is_string())
        .count();
    assert!(
        cn_count * 2 > entries.len(),
        "most gems should have zh-CN names ({cn_count}/{})",
        entries.len()
    );
    // 繁中名边车已接线：多数宝石应有中文名。
    let zh_count = entries
        .iter()
        .filter(|e| e["name_zh_tw"].is_string())
        .count();
    assert!(
        zh_count * 2 > entries.len(),
        "most gems should have zh-TW names ({zh_count}/{})",
        entries.len()
    );
    assert!(entries.iter().any(|e| e["is_support"] == false));
    assert!(entries.iter().any(|e| e["is_support"] == true));
    // 找一个已知法术（Comet 来自 druid 参考 build）。
    let comet = entries
        .iter()
        .find(|e| e["name"] == "Comet")
        .expect("Comet gem in catalog");
    let comet_id = comet["skill_id"].as_str().unwrap();

    let base_req = serde_json::json!({
        "character": { "class_name": "Sorceress", "level": 90 },
        "allocated_nodes": [],
        // 放弃默认任务奖励里的 5% inc life，保持下方 flat delta 断言的纯增量口径。
        "config_inputs": { "questInterlude 2Khari CrossingMolten Shrine": false },
    });
    let stat = |resp: &Value, id: &str| -> f64 {
        resp["stats"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == id)
            .unwrap()["value"]
            .as_f64()
            .unwrap_or(0.0)
    };

    // 手动技能组：白手 build + Comet → TotalDPS > 0。
    let mut with_skill = base_req.clone();
    with_skill["socket_groups"] =
        serde_json::json!([{ "gems": [{ "skill_id": comet_id, "level": 20, "quality": 0 }] }]);
    let resp: Value = serde_json::from_str(
        &pobr_wasm::calculate_build_json(&with_skill.to_string()).expect("manual skill"),
    )
    .unwrap();
    assert!(
        stat(&resp, "TotalDPS") > 0.0,
        "manual Comet group should produce DPS"
    );

    // 手动装备：+50 Life 戒指 → Life 恰好 +50（无任何 inc 来源）。
    let baseline: Value = serde_json::from_str(
        &pobr_wasm::calculate_build_json(&base_req.to_string()).expect("baseline"),
    )
    .unwrap();
    let mut with_item = base_req.clone();
    with_item["items"] = serde_json::json!([{
        "slot": "ring1",
        "text": "Rarity: MAGIC\nSapphire Ring\n+50 to maximum Life",
    }]);
    let resp: Value = serde_json::from_str(
        &pobr_wasm::calculate_build_json(&with_item.to_string()).expect("manual item"),
    )
    .unwrap();
    let delta = stat(&resp, "Life") - stat(&baseline, "Life");
    assert!(
        (delta - 50.0).abs() < 0.5,
        "manual +50 Life ring should add 50 Life, got {delta}"
    );

    // 非法槽位 → 可读错误。
    let mut bad = base_req.clone();
    bad["items"] = serde_json::json!([{ "slot": "hat", "text": "Rarity: NORMAL\nIron Hat" }]);
    let err = pobr_wasm::calculate_build_json(&bad.to_string()).unwrap_err();
    assert!(err.contains("unknown equipment slot"), "unexpected: {err}");
}

/// 导入 build（pob_code）后 Config 页切任务奖励生效：quest 覆盖在计算前按
/// 合并输入整份重建（旧行为 = 解码时固定，请求覆盖无效）。
#[test]
fn quest_reward_override_applies_to_imported_build() {
    ensure_data();
    let spirit_with = |config_inputs: Value| -> f64 {
        let mut request = serde_json::json!({ "pob_code": demo_code() });
        if !config_inputs.is_null() {
            request["config_inputs"] = config_inputs;
        }
        let json: Value = serde_json::from_str(
            &pobr_wasm::calculate_build_json(&request.to_string()).expect("calc"),
        )
        .expect("valid json");
        json["stats"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == "Spirit")
            .unwrap()["value"]
            .as_f64()
            .unwrap()
    };
    let granted = spirit_with(Value::Null);
    let opted_out = spirit_with(serde_json::json!({
        "questAct 1FreythornKing In The Mists": false,
        "questAct 3Azak BogIgnagduk": false,
        "questInterlude 3Kriar VillageLythara": false,
    }));
    // 三个 Spirit 任务共 +100 base（inc 乘区 ≥ 0 时差值不小于 100）。
    assert!(
        granted - opted_out >= 99.0,
        "quest opt-out should drop Spirit (granted={granted} opted_out={opted_out})"
    );
}

/// 药剂/护符覆盖通道：`flasks` 整份替换 utility_slots——charm 基底 buff 生效、
/// 非法槽名报错、归因视图出 `flask` 条目。
#[test]
fn manual_flasks_override_utility_slots() {
    ensure_data();
    let stat = |resp: &Value, id: &str| -> f64 {
        resp["stats"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == id)
            .unwrap()["value"]
            .as_f64()
            .unwrap_or(0.0)
    };
    // 腰带提供 charm 槽预算（无 CharmLimit 来源时 charm 全不生效）。
    let base_req = serde_json::json!({
        "character": { "class_name": "Sorceress", "level": 90 },
        "allocated_nodes": [],
        "items": [{
            "slot": "belt",
            "text": "Rarity: MAGIC\nTest Belt\nHeavy Belt\nHas 1 Charm Slot",
        }],
    });
    let baseline: Value = serde_json::from_str(
        &pobr_wasm::calculate_build_json(&base_req.to_string()).expect("baseline"),
    )
    .unwrap();

    // Ruby Charm 基底 buff = +25% 火抗（inject_flasks_charms 从 base_items 并入）。
    let mut with_charm = base_req.clone();
    with_charm["flasks"] = serde_json::json!([{
        "slot": "Charm 1",
        "text": "Rarity: MAGIC\nRuby Charm\nRuby Charm",
    }]);
    let resp: Value = serde_json::from_str(
        &pobr_wasm::calculate_build_json(&with_charm.to_string()).expect("manual charm"),
    )
    .unwrap();
    let delta = stat(&resp, "FireResist") - stat(&baseline, "FireResist");
    assert!(
        (delta - 25.0).abs() < 0.5,
        "Ruby Charm base buff should add 25% fire res, got {delta}"
    );

    // 非法槽名 → 可读错误。
    let mut bad = base_req.clone();
    bad["flasks"] =
        serde_json::json!([{ "slot": "Boot 1", "text": "Rarity: MAGIC\nRuby Charm\nRuby Charm" }]);
    let err = pobr_wasm::calculate_build_json(&bad.to_string()).unwrap_err();
    assert!(
        err.contains("unknown flask/charm slot"),
        "unexpected: {err}"
    );

    // 归因视图列出 flask 槽条目。
    let attr_req = serde_json::json!({
        "pob_code": "",
        "character": { "class_name": "Sorceress", "level": 90 },
        "items": base_req["items"],
        "flasks": with_charm["flasks"],
        "fields": ["Life"],
    });
    let attr: Value = serde_json::from_str(
        &pobr_wasm::attribution_json(&attr_req.to_string()).expect("attribution"),
    )
    .unwrap();
    let has_flask_entry = attr["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "flask" && e["id"] == "Charm 1");
    assert!(has_flask_entry, "attribution should list the charm slot");
}

/// 装备授予技能：`Grants Skill: Level N X` 词条合成技能组——白手 build 仅凭
/// 授予装备就有 DPS；full_dps 列出该组；手动同技能组仍保持独立。
#[test]
fn item_granted_skill_synthesizes_group() {
    ensure_data();
    // Comet 的 skill_id 从宝石目录反查（与引擎名字反查同源）。
    let catalog: Value =
        serde_json::from_str(&pobr_wasm::gem_catalog_json().expect("gem catalog")).unwrap();
    let comet_id = catalog
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "Comet")
        .expect("Comet in catalog")["skill_id"]
        .as_str()
        .unwrap()
        .to_string();

    let base_req = serde_json::json!({
        "character": { "class_name": "Sorceress", "level": 90 },
        "allocated_nodes": [],
        "items": [{
            "slot": "ring1",
            "text": "Rarity: UNIQUE\nTest Ring\nSapphire Ring\nGrants Skill: Level 20 Comet",
        }],
    });
    let dps = |resp: &Value| -> f64 {
        resp["stats"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == "TotalDPS")
            .unwrap()["value"]
            .as_f64()
            .unwrap_or(0.0)
    };
    let resp: Value = serde_json::from_str(
        &pobr_wasm::calculate_build_json(&base_req.to_string()).expect("granted calc"),
    )
    .unwrap();
    assert!(
        dps(&resp) > 0.0,
        "granted Comet should produce DPS without any socket group"
    );

    // full_dps 列出合成组（skill_id = Comet 主效果）。
    let full: Value =
        serde_json::from_str(&pobr_wasm::full_dps_json(&base_req.to_string()).expect("full dps"))
            .unwrap();
    let per_skill = full["per_skill"].as_array().unwrap();
    assert!(
        per_skill.iter().any(|s| s["skill_id"] == comet_id.as_str()),
        "full dps should list the granted skill, got {per_skill:?}"
    );

    // 手动组无装备 source，即使槽位、技能和等级相同也应与授予组独立存在。
    let mut with_group = base_req.clone();
    with_group["socket_groups"] = serde_json::json!([
        { "gems": [{ "skill_id": comet_id, "level": 20, "quality": 0 }] }
    ]);
    let full2: Value = serde_json::from_str(
        &pobr_wasm::full_dps_json(&with_group.to_string()).expect("full dps manual group"),
    )
    .unwrap();
    let comet_groups = full2["per_skill"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["skill_id"] == comet_id.as_str())
        .count();
    assert_eq!(
        comet_groups, 2,
        "manual and item-granted skill groups must remain independent"
    );
}

/// Ring 3 未解锁时，物品授予技能与物品词条一样不参与普通计算或 FullDPS。
#[test]
fn ring3_granted_skill_is_gated_from_full_dps() {
    ensure_data();
    let request = serde_json::json!({
        "character": { "class_name": "Sorceress", "level": 90 },
        "allocated_nodes": [],
        "items": [{
            "slot": "ring3",
            "text": "Rarity: UNIQUE\nTest Ring\nSapphire Ring\nGrants Skill: Level 20 Comet",
        }],
    })
    .to_string();

    let regular: Value =
        serde_json::from_str(&pobr_wasm::calculate_build_json(&request).expect("regular dps"))
            .unwrap();
    let total_dps = regular["stats"]
        .as_array()
        .unwrap()
        .iter()
        .find(|stat| stat["id"] == "TotalDPS")
        .unwrap()["value"]
        .as_f64()
        .unwrap_or(0.0);
    assert_eq!(total_dps, 0.0, "locked Ring 3 must not grant regular DPS");

    let full: Value =
        serde_json::from_str(&pobr_wasm::full_dps_json(&request).expect("full dps")).unwrap();
    assert_eq!(full["full_dps"].as_f64().unwrap_or(0.0), 0.0);
    assert!(
        full["per_skill"].as_array().unwrap().is_empty(),
        "locked Ring 3 must not contribute FullDPS skills"
    );
}

/// 逐技能组 DPS：demo build 至少一个伤害组，分项和 = full_dps。
#[test]
fn full_dps_json_shape() {
    ensure_data();
    let request = serde_json::json!({ "pob_code": demo_code() }).to_string();
    let json: Value = serde_json::from_str(&pobr_wasm::full_dps_json(&request).expect("full dps"))
        .expect("valid json");
    assert_keys(&json, &["full_dps", "per_skill"], "FullDpsResponse");
    let per_skill = json["per_skill"].as_array().unwrap();
    assert!(
        !per_skill.is_empty(),
        "demo build should have damage skills"
    );
    assert_keys(
        &per_skill[0],
        &["group_index", "skill_id", "dps"],
        "per_skill[0]",
    );
    let sum: f64 = per_skill.iter().map(|s| s["dps"].as_f64().unwrap()).sum();
    let full = json["full_dps"].as_f64().unwrap();
    assert!(full > 0.0, "full_dps should be positive");
    assert!((sum - full).abs() < 1e-6, "sum {sum} != full {full}");
}

/// encode 往返契约：编辑态请求 → 分享 code → 重新解码计算，与直接按请求计算
/// 的全部展示字段一致（树/装备/药剂/技能组/config/属性小点全覆盖）。
#[test]
fn encode_build_roundtrip_matches_direct_calculation() {
    ensure_data();
    // 取真实 demo build 的解码结果拼一个「全量覆盖」请求（含手动附加项）。
    let decoded: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(&demo_code()).expect("decode")).unwrap();
    let ch = &decoded["character"];
    let items: Vec<Value> = decoded["items"]["equipped"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| serde_json::json!({ "slot": i["slot"], "text": i["text"] }))
        .collect();
    let socket_groups: Vec<Value> = decoded["socket_groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| {
            serde_json::json!({
                "slot": g["slot"],
                "enabled": g["enabled"],
                "source": g["source"],
                "gems": g["gems"],
            })
        })
        .collect();
    let request = serde_json::json!({
        "character": {
            "level": ch["level"],
            "class_name": ch["class_name"],
            "ascendancy_name": ch["ascendancy_name"],
        },
        "allocated_nodes": decoded["tree"]["allocated_nodes"],
        "attribute_choices": decoded["tree"]["attribute_choices"],
        "socket_groups": socket_groups,
        "items": items,
        "flasks": [{ "slot": "Charm 1", "text": "Rarity: MAGIC\nRuby Charm\nRuby Charm" }],
        "main_socket_group": decoded["main_socket_group"],
        "config_inputs": { "conditionEnemyChilled": true },
        "notes": "roundtrip <check> & escape",
    });

    let direct: Value = serde_json::from_str(
        &pobr_wasm::calculate_build_json(&request.to_string()).expect("direct calc"),
    )
    .unwrap();

    let code = pobr_wasm::encode_build_json(&request.to_string()).expect("encode");

    // 结构往返：树/技能组/装备/药剂数量与输入一致（数值对比前先钉住形状）。
    let redecoded: Value =
        serde_json::from_str(&pobr_wasm::decode_build_json(&code).expect("redecode")).unwrap();
    assert_eq!(
        redecoded["tree"]["allocated_nodes"]
            .as_array()
            .unwrap()
            .len(),
        decoded["tree"]["allocated_nodes"].as_array().unwrap().len(),
        "allocated node count"
    );
    assert_eq!(
        redecoded["socket_groups"].as_array().unwrap().len(),
        socket_groups.len(),
        "socket group count"
    );
    assert_eq!(
        redecoded["items"]["equipped"].as_array().unwrap().len(),
        items.len(),
        "equipped item count"
    );
    assert_eq!(
        redecoded["items"]["flasks"].as_array().unwrap().len(),
        1,
        "flask/charm count"
    );
    for (i, (orig, rt)) in decoded["socket_groups"]
        .as_array()
        .unwrap()
        .iter()
        .zip(redecoded["socket_groups"].as_array().unwrap())
        .enumerate()
    {
        assert_eq!(
            orig["gems"].as_array().unwrap().len(),
            rt["gems"].as_array().unwrap().len(),
            "group {i} gem count"
        );
        assert_eq!(
            orig["active_skill_id"], rt["active_skill_id"],
            "group {i} active skill"
        );
        assert_eq!(orig["source"], rt["source"], "group {i} source");
    }
    assert_eq!(
        redecoded["main_socket_group"], decoded["main_socket_group"],
        "main socket group"
    );
    assert_eq!(
        redecoded["tree"]["attribute_choices"], decoded["tree"]["attribute_choices"],
        "attribute choices"
    );
    {
        let norm = |v: &Value| -> Vec<(String, String)> {
            let mut out: Vec<(String, String)> = v
                .as_array()
                .unwrap()
                .iter()
                .map(|i| {
                    (
                        i["slot"].as_str().unwrap_or_default().to_string(),
                        i["text"].as_str().unwrap_or_default().trim().to_string(),
                    )
                })
                .collect();
            out.sort();
            out
        };
        assert_eq!(
            norm(&redecoded["items"]["equipped"]),
            norm(&decoded["items"]["equipped"]),
            "equipped item texts"
        );
    }
    let roundtrip_req = serde_json::json!({ "pob_code": code });
    let via_code: Value = serde_json::from_str(
        &pobr_wasm::calculate_build_json(&roundtrip_req.to_string()).expect("roundtrip calc"),
    )
    .unwrap();

    // 全部展示字段逐一相等（数值容差 1e-6）。
    let stats = |v: &Value| -> Vec<(String, Option<f64>)> {
        v["stats"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| (s["id"].as_str().unwrap().to_string(), s["value"].as_f64()))
            .collect()
    };
    let a = stats(&direct);
    let b = stats(&via_code);
    assert_eq!(a.len(), b.len(), "stat count mismatch");
    for ((id_a, va), (id_b, vb)) in a.iter().zip(&b) {
        assert_eq!(id_a, id_b);
        match (va, vb) {
            (Some(x), Some(y)) => {
                if (x - y).abs() >= 1e-6 {
                    // 失败前 dump 两侧该字段的 breakdown 差异，便于定位来源。
                    let dump = |v: &Value| -> Vec<String> {
                        v["breakdowns"][id_a]["mods"]
                            .as_array()
                            .map(|mods| {
                                mods.iter()
                                    .map(|m| {
                                        format!(
                                            "{} {} {:?}",
                                            m["mod_type"], m["value"], m["source_text"]
                                        )
                                    })
                                    .collect()
                            })
                            .unwrap_or_default()
                    };
                    let da = dump(&direct);
                    let db = dump(&via_code);
                    let only_a: Vec<_> = da.iter().filter(|l| !db.contains(l)).collect();
                    let only_b: Vec<_> = db.iter().filter(|l| !da.contains(l)).collect();
                    let probe = |v: &Value| -> Vec<String> {
                        v["stats"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .filter(|s| {
                                let id = s["id"].as_str().unwrap_or_default();
                                id.contains("Reserv")
                                    || id.contains("Str")
                                    || id.contains("Int")
                                    || id.contains("Dex")
                                    || id.contains("Spirit")
                            })
                            .map(|s| format!("{}={}", s["id"], s["value"]))
                            .collect()
                    };
                    panic!(
                        "{id_a}: direct={x} roundtrip={y}\nonly-direct: {only_a:#?}\nonly-roundtrip: {only_b:#?}\nprobe-direct: {:?}\nprobe-roundtrip: {:?}",
                        probe(&direct),
                        probe(&via_code)
                    );
                }
            }
            (x, y) => assert_eq!(x, y, "{id_a}: null-ness mismatch"),
        }
    }

    // Notes 也往返（含转义字符）。
    assert_eq!(redecoded["notes"], "roundtrip <check> & escape");
}

/// Phase 7.1：中文词条行输入翻译——简中物品文本与英文等价，未知中文行落 unsupported。
#[test]
fn chinese_mod_lines_translate_to_english() {
    ensure_data();
    let base_req = serde_json::json!({
        "character": { "class_name": "Sorceress", "level": 90 },
        "allocated_nodes": [],
    });
    let life = |resp: &Value| -> f64 {
        resp["stats"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == "Life")
            .unwrap()["value"]
            .as_f64()
            .unwrap()
    };
    let calc = |req: &Value| -> Value {
        serde_json::from_str(&pobr_wasm::calculate_build_json(&req.to_string()).expect("calc"))
            .unwrap()
    };

    // 简中物品（基底名 + 两条词条行）与英文等价物逐值一致。
    let mut zh_req = base_req.clone();
    zh_req["items"] = serde_json::json!([{
        "slot": "ring1",
        "text": "Rarity: MAGIC\n奇异戒指\n蓝玉戒指\n+50 生命上限\n生命上限提高 10%",
    }]);
    let mut en_req = base_req.clone();
    en_req["items"] = serde_json::json!([{
        "slot": "ring1",
        "text": "Rarity: MAGIC\nStrange Ring\nSapphire Ring\n+50 to maximum Life\n10% increased maximum Life",
    }]);
    let baseline = calc(&base_req);
    let zh = calc(&zh_req);
    let en = calc(&en_req);
    assert!(
        (life(&zh) - life(&en)).abs() < f64::EPSILON,
        "中文物品应与英文等价（zh={} en={}）",
        life(&zh),
        life(&en)
    );
    assert!(life(&zh) > life(&baseline), "中文词条应实际生效");

    // 简中 extra_modifiers 同样翻译。
    let mut extra_req = base_req.clone();
    extra_req["extra_modifiers"] = serde_json::json!(["能量护盾上限提高 100%"]);
    let es = |resp: &Value| -> f64 {
        resp["stats"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == "EnergyShield")
            .unwrap()["value"]
            .as_f64()
            .unwrap_or(0.0)
    };
    // 白手 build 无 ES base，此断言只验证不报错且不落 unsupported。
    let extra = calc(&extra_req);
    assert!(
        !extra["unsupported_modifiers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_str().unwrap_or_default().contains("能量护盾")),
        "已知中文词条不应落 unsupported"
    );
    let _ = es(&extra);

    // 未知中文行：原样保留进物品文本 → 与英文未知行同语义（orchestrator 的
    // filter_parseable 闸门静默跳过，不崩、不改变数值）。
    let mut unknown_req = base_req.clone();
    unknown_req["items"] = serde_json::json!([{
        "slot": "ring1",
        "text": "Rarity: MAGIC\n某戒指\n这是一条不存在的词条",
    }]);
    let unknown = calc(&unknown_req);
    assert!(
        (life(&unknown) - life(&baseline)).abs() < f64::EPSILON,
        "未知中文行应静默跳过不影响数值"
    );
}

/// 属性小点三选一：同一节点选 dex 提升命中（vs 选 str），引擎 attribute_overrides 通道。
#[test]
fn attribute_choice_changes_derived_stats() {
    ensure_data();
    // 节点 722 = `+5 to any Attribute` 属性小点（data/base/passive_tree.json）。
    let accuracy_with = |choice: &str| -> f64 {
        let request = serde_json::json!({
            "character": { "class_name": "Ranger", "level": 1 },
            "allocated_nodes": [722],
            "attribute_choices": { "722": choice },
        })
        .to_string();
        let json: Value =
            serde_json::from_str(&pobr_wasm::calculate_build_json(&request).expect("calc"))
                .expect("valid json");
        json["breakdowns"]["Accuracy"]["base_total"]
            .as_f64()
            .unwrap_or(0.0)
    };
    let dex = accuracy_with("dex");
    let str_ = accuracy_with("str");
    assert!(
        dex > str_,
        "dex 三选一应提升命中派生（dex={dex} str={str_}）"
    );
}

/// 国服 `.build` 文件导入：真实样例解码（天赋 slug 映射 / 宝石效果映射 /
/// 简中装备词条）→ 可直接计算。
#[test]
fn decode_cn_build_file_and_calculate() {
    ensure_data();
    let path = repo_data_root().join("../examples/build-file/召唤奥黛丽黑本.build");
    let content = std::fs::read_to_string(path).expect("read .build");
    let decoded: Value =
        serde_json::from_str(&pobr_wasm::decode_build_file_json(&content).expect("decode .build"))
            .expect("valid json");
    // 形状与 decode_build_json 同构。
    assert_eq!(decoded["character"]["class_name"], "Sorceress");
    assert_eq!(decoded["character"]["level"], 98);
    assert!(decoded["tree"]["allocated_nodes"].as_array().unwrap().len() > 100);
    let groups = decoded["socket_groups"].as_array().unwrap();
    assert!(groups.len() >= 10, "groups={}", groups.len());
    assert!(
        groups
            .iter()
            .all(|g| !g["gems"].as_array().unwrap().is_empty())
    );
    let items = decoded["items"]["equipped"].as_array().unwrap();
    assert!(items.len() >= 10, "items={}", items.len());

    // 直接用解码产物计算（简中词条走翻译层）：Life/ES 应为正。
    let request = serde_json::json!({
        "character": decoded["character"],
        "allocated_nodes": decoded["tree"]["allocated_nodes"],
        "socket_groups": decoded["socket_groups"].as_array().unwrap().iter().map(|g| {
            serde_json::json!({ "enabled": true, "gems": g["gems"] })
        }).collect::<Vec<_>>(),
        "items": decoded["items"]["equipped"],
    })
    .to_string();
    let calc: Value =
        serde_json::from_str(&pobr_wasm::calculate_build_json(&request).expect("calc"))
            .expect("valid json");
    let stat = |id: &str| -> f64 {
        calc["stats"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == id)
            .unwrap()["value"]
            .as_f64()
            .unwrap_or(0.0)
    };
    assert!(stat("Life") > 0.0, "Life={}", stat("Life"));
    assert!(
        stat("EnergyShield") > 0.0,
        "ES build 应有能量护盾（简中装备词条经翻译层生效），ES={}",
        stat("EnergyShield")
    );
}

/// 手动树插槽珠宝：插槽加点才生效（与 XML 门控一致）；范围珠宝 grant 行
/// 经几何展开改天赋词条（引擎既有链路）。
#[test]
fn manual_jewels_respect_socket_allocation() {
    ensure_data();
    // 节点 7960 = jewel_slot1969（珠宝插槽）。
    let life = |allocated: bool| -> f64 {
        let request = serde_json::json!({
            "character": { "class_name": "Witch", "level": 1 },
            "allocated_nodes": if allocated { vec![7960u32] } else { vec![] },
            "jewels": [{
                "socket_node": 7960,
                "text": "Rarity: RARE\nTest Jewel\nEmerald\n+50 to maximum Life",
            }],
            // 放弃默认任务奖励里的 5% inc life，保持 flat delta 断言的纯增量口径。
            "config_inputs": { "questInterlude 2Khari CrossingMolten Shrine": false },
        })
        .to_string();
        let json: Value =
            serde_json::from_str(&pobr_wasm::calculate_build_json(&request).expect("calc"))
                .expect("valid json");
        json["stats"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == "Life")
            .unwrap()["value"]
            .as_f64()
            .unwrap()
    };
    let with = life(true);
    let without = life(false);
    assert!(
        (with - without - 50.0).abs() < 0.5,
        "已加点插槽的珠宝应 +50 Life（with={with} without={without}）"
    );
}

#[test]
fn attribution_json_shape() {
    ensure_data();
    let fields = ["Life", "EnergyShield", "Evasion", "TotalDPS"];
    let request = serde_json::json!({
        "pob_code": demo_code(),
        "fields": fields,
    })
    .to_string();
    let json: Value =
        serde_json::from_str(&pobr_wasm::attribution_json(&request).expect("attribution"))
            .expect("valid json");
    assert_keys(&json, &["baseline", "entries"], "AttributionResponse");
    assert!(json["baseline"]["Life"].as_f64().unwrap() > 0.0);
    let entries = json["entries"].as_array().unwrap();
    assert!(!entries.is_empty());
    assert_keys(&entries[0], &["kind", "id", "deltas"], "entries[0]");
    // 至少一个装备来源对任一字段有非零贡献（不同 build 属性分布不同，不钉具体字段）。
    assert!(
        entries.iter().any(|e| e["kind"] == "item"
            && fields
                .iter()
                .any(|f| e["deltas"][f].as_f64().unwrap_or(0.0).abs() > f64::EPSILON)),
        "应有装备对至少一个字段产生贡献: {entries:?}"
    );
}

#[test]
fn memory_backend_matches_dir_backend() {
    // GameData 内存后端（wasm 数据注入路径）与目录后端产出一致的计算结果。
    ensure_data();
    let request = serde_json::json!({ "pob_code": demo_code() }).to_string();
    let from_dir = pobr_wasm::calculate_build_json(&request).expect("dir backend");

    // 把版本目录整体读入内存表，走 stage/init 路径重建。
    let root = repo_data_root().join(pobr_data::GOLDEN_PARITY_DATA_VERSION);
    for entry in walk_files(&root) {
        // GameData 只读 JSON；跳过杂项文件（.DS_Store 等）。
        if entry.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let rel = entry
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let content = std::fs::read_to_string(&entry).expect("read data file");
        pobr_wasm::stage_data_file(&rel, &content);
    }
    pobr_wasm::init_staged_data().expect("init memory backend");
    let from_memory = pobr_wasm::calculate_build_json(&request).expect("memory backend");
    assert_eq!(
        from_dir, from_memory,
        "内存后端与目录后端计算结果应逐字节一致"
    );

    // 还原目录后端，避免影响同线程后续测试。
    let dir = repo_data_root().join(pobr_data::GOLDEN_PARITY_DATA_VERSION);
    pobr_wasm::init_data_from_dir(dir.to_str().unwrap()).expect("restore dir backend");
}

/// 递归枚举目录下全部文件。
fn walk_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in rd.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk_files(&path));
        } else {
            out.push(path);
        }
    }
    out
}
