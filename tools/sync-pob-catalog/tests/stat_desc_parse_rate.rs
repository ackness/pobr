//! §B 可行性测量（探索性，`#[ignore]`，非 CI 门禁）。
//!
//! 问题：`overlay/stat_descriptions.json`（段 A 抽取的 stat_id → canonical 文本）
//! 里，有多少条能被现行 `parse_mod_engine` 成功解析成 Modifier？这个解析率决定
//! 「stat_id → Modifier 第二通道」是否可行——率高则通道成立、可逐域切换；率低则
//! 要先补引擎解析能力。
//!
//! 运行：`cargo test -p sync-pob-catalog --test stat_desc_parse_rate -- --ignored --nocapture`

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use pobr_core::mod_parser::{CompiledParserRules, ParseStatus, parse_mod_engine};
use pobr_data::catalog::parser_rules::ModParserRulesDoc;
use pobr_data::catalog::stat_descriptions::StatDescriptionsDef;

fn overlay_path(file: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/4.5.0.3.4/overlay")
        .join(file)
}

fn load_rules() -> CompiledParserRules {
    let text = fs::read_to_string(overlay_path("mod_parser_rules.json")).expect("read rules");
    // serde 忽略 `_meta` 头部；只读规则六表。
    let doc: ModParserRulesDoc = serde_json::from_str(&text).expect("deserialize rules doc");
    CompiledParserRules::compile(&doc).expect("compile rules")
}

fn load_descriptions() -> StatDescriptionsDef {
    let text = fs::read_to_string(overlay_path("stat_descriptions.json")).expect("read descs");
    serde_json::from_str(&text).expect("deserialize descriptions")
}

#[test]
#[ignore = "探索性测量，非门禁；用 --ignored 跑"]
fn measure_single_stat_parse_rate() {
    let rules = load_rules();
    let descs = load_descriptions();

    let mut sample_unsupported: Vec<String> = Vec::new();
    println!("\n===== stat_descriptions 单 stat 文本解析率（parse_mod_engine）=====");
    println!(
        "{:<42}{:>10}{:>10}{:>9}",
        "scope", "parsed", "total", "rate"
    );

    let mut grand_parsed = 0usize;
    let mut grand_total = 0usize;
    // 每个文本里命中的 ModName（解析成功时）频次，抽样观察。
    let mut name_freq: BTreeMap<String, usize> = BTreeMap::new();

    for (scope_name, scope) in &descs.scopes {
        let mut parsed = 0usize;
        let mut total = 0usize;
        for lines in scope.single.values() {
            for line in lines {
                total += 1;
                let outcome = parse_mod_engine(line, &rules);
                if outcome.status == ParseStatus::Parsed && !outcome.mods.is_empty() {
                    parsed += 1;
                    for m in &outcome.mods {
                        *name_freq.entry(m.name.to_string()).or_default() += 1;
                    }
                } else if sample_unsupported.len() < 40 {
                    sample_unsupported.push(format!("[{scope_name}] {line}"));
                }
            }
        }
        grand_parsed += parsed;
        grand_total += total;
        let rate = if total > 0 {
            parsed as f64 / total as f64 * 100.0
        } else {
            0.0
        };
        println!("{scope_name:<42}{parsed:>10}{total:>10}{rate:>8.1}%");
    }

    let grand_rate = if grand_total > 0 {
        grand_parsed as f64 / grand_total as f64 * 100.0
    } else {
        0.0
    };
    println!(
        "{:<42}{grand_parsed:>10}{grand_total:>10}{grand_rate:>8.1}%",
        "TOTAL"
    );

    println!("\n===== 命中 ModName Top 25（解析成功的文本里）=====");
    let mut freq: Vec<_> = name_freq.into_iter().collect();
    freq.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    for (name, count) in freq.iter().take(25) {
        println!("  {count:>6}  {name}");
    }

    println!("\n===== 未解析样本（前 40 条）=====");
    for s in &sample_unsupported {
        println!("  {s}");
    }
}
