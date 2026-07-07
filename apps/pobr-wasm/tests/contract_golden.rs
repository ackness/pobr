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
        ],
        "BuildJson",
    );
    assert_keys(
        &json["character"],
        &["level", "class_name", "ascendancy_name"],
        "character",
    );
    assert_keys(&json["tree"], &["allocated_nodes", "tree_version"], "tree");
    assert_keys(&json["items"], &["equipped", "jewels", "flasks"], "items");

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
        &["slot", "enabled", "active_skill_id", "gems"],
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
        &["stats", "unsupported_modifiers", "breakdowns"],
        "CalculateBuildResponse",
    );

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

/// 白手起 build（无 pob_code，PoB2 新建语义）：character 即可计算，等级驱动基础量。
#[test]
fn calculate_scratch_build_without_code() {
    ensure_data();
    let life_at = |level: u32| -> f64 {
        let request = serde_json::json!({
            "character": { "class_name": "Warrior", "level": level },
            "allocated_nodes": [],
        })
        .to_string();
        let json: Value =
            serde_json::from_str(&pobr_wasm::calculate_build_json(&request).expect("scratch"))
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
        &["skill_id", "name", "is_support"],
        "gem catalog entry",
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
