//! Origins of rendered text (`translateLinesDetailed`) and the minions' game
//! variety ids: absent from today's data, carried when a pack has supplement rows.

use serde_json::Value;

const LINES: [&str; 4] = [
    "33% increased maximum Life",
    // No official or supplement template knows this line (the Fire one is official since locale/0.5.5.5-4).
    "Gain 3% of damage as Cold damage per 2% Chance to Block",
    "Not a line any template knows",
    "+12% increased maximum Life",
];

/// The active data version's runtime files, `i18n/zh-CN/stat_lines.json`
/// replaced by `stat_lines` when given.
fn pack(stat_lines: Option<Vec<u8>>) -> Vec<(String, Vec<u8>)> {
    fn walk(
        root: &std::path::Path,
        dir: &std::path::Path,
        prefix: &str,
        out: &mut Vec<(String, Vec<u8>)>,
    ) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, prefix, out);
            } else if path.extension().is_some_and(|ext| ext == "json") {
                let relative = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push((format!("{prefix}{relative}"), std::fs::read(&path).unwrap()));
            }
        }
    }
    let version = pobr_gamedata::current_data_dir();
    let mut files = Vec::new();
    walk(&version, &version, "", &mut files);
    let common = version.parent().unwrap().join("overlay-common");
    walk(&common, &common, "overlay-common/", &mut files);
    if let Some(stat_lines) = stat_lines {
        let (_, bytes) = files
            .iter_mut()
            .find(|(path, _)| path == "i18n/zh-CN/stat_lines.json")
            .expect("the zh-CN pack has templates");
        *bytes = stat_lines;
    }
    files
}

fn init(files: &[(String, Vec<u8>)]) {
    for (path, bytes) in files {
        pobr_wasm::stage_data_bytes(path, bytes.clone());
    }
    // The caller vouches for the bytes (the replaced file no longer matches the manifest).
    pobr_wasm::init_staged_data_verified().expect("verified init");
}

fn detailed(lang: &str) -> Vec<(String, Value)> {
    let json = serde_json::to_string(&LINES).unwrap();
    let out: Vec<Value> =
        serde_json::from_str(&pobr_wasm::translate_lines_detailed_json(lang, &json).unwrap())
            .unwrap();
    out.into_iter()
        .map(|row| {
            (
                row["text"].as_str().unwrap().to_string(),
                row["origin"].clone(),
            )
        })
        .collect()
}

fn plain(lang: &str) -> Vec<String> {
    let json = serde_json::to_string(&LINES).unwrap();
    serde_json::from_str(&pobr_wasm::translate_lines_json(lang, &json).unwrap()).unwrap()
}

#[test]
fn todays_data_has_only_official_text_and_the_plain_texts() {
    init(&pack(None));
    let rows = detailed("zh-CN");
    assert_eq!(
        rows.iter()
            .map(|(text, _)| text.clone())
            .collect::<Vec<_>>(),
        plain("zh-CN")
    );
    assert!(rows.iter().all(|(_, origin)| origin.is_null()), "{rows:?}");
    assert_eq!(rows[0].0, "生命上限提高 33%");
    // English and undeclared locales pass every line through, unmarked.
    for lang in ["en-US", "xx"] {
        let rows = detailed(lang);
        assert!(rows.iter().all(|(_, origin)| origin.is_null()));
        assert_eq!(
            rows.into_iter().map(|(text, _)| text).collect::<Vec<_>>(),
            LINES
        );
    }
    assert!(pobr_wasm::translate_lines_detailed_json("zh-CN", "{}").is_err());
}

#[test]
fn a_supplement_row_marks_the_lines_it_renders() {
    let base = std::fs::read(pobr_gamedata::current_data_dir().join("i18n/zh-CN/stat_lines.json"))
        .unwrap();
    let mut rows: Vec<Value> = serde_json::from_slice(&base).unwrap();
    let before = plain_after_init(None);
    assert_eq!(before[1], LINES[1], "today's data must not render the supplemented line");
    rows.push(serde_json::json!({
        "src": "每 2% 格挡几率获得相当于伤害 {0}% 的冰霜伤害",
        "en": "Gain {0}% of damage as Cold damage per 2% Chance to Block",
        "origin": "agent"
    }));
    // Official text wins a line it also renders, even listed first: this row is never used.
    rows.insert(
        0,
        serde_json::json!({
            "src": "我方补译 {0}% 生命", "en": "{0}% increased maximum Life", "origin": "human"
        }),
    );
    init(&pack(Some(serde_json::to_vec(&rows).unwrap())));
    let rows = detailed("zh-CN");
    assert_eq!(
        rows,
        [
            ("生命上限提高 33%".to_string(), Value::Null),
            (
                "每 2% 格挡几率获得相当于伤害 3% 的冰霜伤害".to_string(),
                Value::from("agent")
            ),
            (LINES[2].to_string(), Value::Null),
            (before[3].clone(), Value::Null),
        ]
    );
    // translateLines itself only gains the supplemented line.
    let after = plain("zh-CN");
    assert_eq!(after[1], rows[1].0);
    assert_eq!(
        [&after[0], &after[2], &after[3]],
        [&before[0], &before[2], &before[3]]
    );
}

fn plain_after_init(stat_lines: Option<Vec<u8>>) -> Vec<String> {
    init(&pack(stat_lines));
    plain("zh-CN")
}

#[test]
fn minions_carry_their_game_variety_id() {
    init(&pack(None));
    let catalog: Value = serde_json::from_str(&pobr_wasm::minion_catalog_json().unwrap()).unwrap();
    let monsters = catalog["monsters"].as_array().unwrap();
    let spectre = monsters
        .iter()
        .find(|monster| monster["spectre"] == true)
        .expect("a spectre");
    assert_eq!(spectre["monster_variety_id"], spectre["id"]);
    let linked = monsters
        .iter()
        .filter(|monster| monster["spectre"] == false)
        .filter(|monster| {
            monster["monster_variety_id"]
                .as_str()
                .is_some_and(|id| id.starts_with("Metadata/Monsters/"))
        })
        .count();
    assert!(linked > 0, "no ordinary minion links its variety");
}
