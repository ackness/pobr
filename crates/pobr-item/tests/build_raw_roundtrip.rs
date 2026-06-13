//! BuildRaw 往返 golden 回归（P16 验收契约：编辑态无 parity 可依）。
//!
//! 强契约（门禁）：对真实 ninja build 全部 `<Item>` 文本块，
//! `parse(build_raw(parse(x))) == parse(x)`（语义不动点）。
//!
//! 语料 = `examples/demo-bd-test/builds/*/decoded.xml` 内嵌的全部 `<Item>` 块（~360 件），
//! 覆盖 rare/unique/jewel、Implicits 头、`{enchant}{rune}`/`{fractured}`/`{desecrated}`
//! 标注、Sockets/Rune、防御件、catalyst 等真实形态。
//!
//! 字节契约（报表，非门禁）：`build_raw(parse(x))` 与 x 规范化后的 byte-diff 计数打印，
//! 趋零是 M6 前目标；PoB2 自身 BuildRaw 亦不保证 byte-stable，故不作硬断言。

use pobr_item::ItemDraft;
use std::path::PathBuf;

/// 定位 `examples/demo-bd-test/builds`（从 crate manifest dir 上溯两级到 workspace root）。
fn builds_dir() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // crates/pobr-item → workspace root
    manifest
        .parent()
        .and_then(|p| p.parent())
        .map(|root| root.join("examples/demo-bd-test/builds"))
        .expect("workspace root")
}

/// 从 decoded.xml 抽取全部 `<Item ...>...</Item>` 内的物品文本块。
///
/// PoB XML 的 `<Item>` 内文是 HTML 转义的物品文本（`&apos;` 等）+ 末尾 `<ModRange/>`
/// 子元素。抽取时：取 `<Item ...>` 后到首个 `<`（子元素或闭合标签）的纯文本段，解转义。
fn extract_item_blocks(xml: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut rest = xml;
    while let Some(open) = rest.find("<Item ") {
        let after_open = &rest[open..];
        // 跳过开标签自身到 `>`。
        let Some(gt) = after_open.find('>') else {
            break;
        };
        let body_start = open + gt + 1;
        let body_region = &rest[body_start..];
        // 物品文本到首个内嵌子元素（`<ModRange`/`</Item`）为止。
        let text_end = body_region.find('<').unwrap_or(body_region.len());
        let raw_text = &body_region[..text_end];
        let unescaped = unescape_xml(raw_text);
        if unescaped.contains("Rarity:") {
            blocks.push(unescaped);
        }
        // 推进到本 Item 结束之后。
        let Some(close) = body_region.find("</Item>") else {
            break;
        };
        rest = &body_region[close + "</Item>".len()..];
    }
    blocks
}

fn unescape_xml(s: &str) -> String {
    s.replace("&apos;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// 收集语料：全部 build 的全部 Item 块。
fn corpus() -> Vec<(String, String)> {
    let dir = builds_dir();
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let xml_path = entry.path().join("decoded.xml");
        if let Ok(xml) = std::fs::read_to_string(&xml_path) {
            let build_name = entry.file_name().to_string_lossy().to_string();
            for (i, block) in extract_item_blocks(&xml).into_iter().enumerate() {
                out.push((format!("{build_name}#{i}"), block));
            }
        }
    }
    out
}

#[test]
fn corpus_is_non_empty() {
    let c = corpus();
    assert!(
        c.len() >= 100,
        "expected ≥100 real item blocks, got {} (builds dir = {:?})",
        c.len(),
        builds_dir()
    );
}

#[test]
fn semantic_fixpoint_roundtrip_all_real_items() {
    let mut failures = Vec::new();
    let corpus = corpus();
    assert!(!corpus.is_empty(), "empty corpus");

    for (id, raw) in &corpus {
        let Ok(d1) = ItemDraft::parse(raw) else {
            failures.push(format!("{id}: initial parse failed"));
            continue;
        };
        let rebuilt = d1.build_raw();
        let Ok(d2) = ItemDraft::parse(&rebuilt) else {
            failures.push(format!("{id}: re-parse of build_raw failed"));
            continue;
        };
        if d1 != d2 {
            failures.push(format!(
                "{id}: NOT a fixpoint\n--- d1.build_raw ---\n{rebuilt}\n--- d2.build_raw ---\n{}",
                d2.build_raw()
            ));
        }
    }

    if !failures.is_empty() {
        let shown: Vec<_> = failures.iter().take(5).collect();
        panic!(
            "{} / {} item blocks failed semantic-fixpoint roundtrip; first {}:\n{}",
            failures.len(),
            corpus.len(),
            shown.len(),
            shown
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("\n\n")
        );
    }
}

/// 字节契约报表（非门禁）：build_raw 与原文规范化后逐行 diff 计数。
#[test]
fn byte_diff_report() {
    let corpus = corpus();
    let mut exact = 0usize;
    let mut diff = 0usize;
    for (_, raw) in &corpus {
        let Ok(d) = ItemDraft::parse(raw) else {
            continue;
        };
        let rebuilt = d.build_raw();
        if normalize(raw) == normalize(&rebuilt) {
            exact += 1;
        } else {
            diff += 1;
        }
    }
    eprintln!(
        "[build_raw byte-diff report] exact={exact} diff={diff} total={}",
        corpus.len()
    );
}

fn normalize(s: &str) -> Vec<String> {
    s.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}
