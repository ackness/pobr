//! `extract-bases` 子命令：用 luajit 在最小 stub 环境下执行 vendor PoB2 的
//! `Data/Bases/*.lua`，把 GGG `.dat` 路线不可得的基底列（`ShieldTypes.Block` →
//! `block_chance`、`ItemSpirit.SpiritGranted` → `spirit`）固化为**确定性 JSON**
//! 落到 `data/<版本>/overlay/base_item_overrides.json`——蓝图 m2-defence §6
//! 开放问题 1/2 的 vendor 抽取兜底路线（与 M0 的 `skill_overrides` 通道同构）。
//!
//! 职责切分（同 [`crate::extract_lua`]）：
//! - Lua 引导脚本（`extract_base_overrides.lua`，编译期内嵌）只负责忠实抽取
//!   并以 JSONL 输出；
//! - Rust 侧统一做排序（按 name 升序）与整体文档序列化，保证同输入重跑
//!   **byte-stable**。

use std::io::{self, Write};
use std::process::{Command, Stdio};

use serde::Serialize;

use crate::extract_lua::{ExtractLuaArgs, OverlayMeta, build_overlay_meta};

/// 引导脚本内容（经 stdin 注入 luajit，二进制自包含、不依赖运行目录）
const BOOTSTRAP_LUA: &str = include_str!("extract_base_overrides.lua");

/// 默认抽取的 vendor 基底数据文件：盾（`armour.BlockChance`）、权杖（`spirit`）
/// 与弩（`weapon.ReloadTimeBase`，M4-T4 W-D2）。vendor 全量 Data/Bases 中仅
/// 这三类携带目标字段（`grep -l` 核实：block/spirit 2026-06-11、
/// ReloadTimeBase 2026-06-12 仅 crossbow.lua）。
pub const DEFAULT_BASE_FILES: &[&str] = &["crossbow", "sceptre", "shield"];

/// 当前 overlay 文档 schema 标识（字段演化时递增）
pub const BASE_ITEM_OVERRIDES_SCHEMA: &str = "base_item_overrides/v1";

/// 单条基底覆盖值——schema 单一来源是
/// [`pobr_data::catalog::base_item_overrides::BaseItemOverrideEntry`]
/// （生成 / 消费两侧共用同一 serde 形状，防止字段漂移）。
pub use pobr_data::catalog::base_item_overrides::BaseItemOverrideEntry;

/// 完整 overlay 文档（生成侧；消费侧形状见
/// `pobr_data::catalog::base_item_overrides::BaseItemOverridesDef`）。
#[derive(Debug, Serialize)]
pub struct BaseItemOverridesDoc {
    /// 头部元信息（serde 落为 `_meta`，置于文件最前）
    #[serde(rename = "_meta")]
    pub meta: OverlayMeta,
    /// 覆盖值列表，按 `name` 升序
    pub overrides: Vec<BaseItemOverrideEntry>,
}

/// 执行抽取，返回最终（byte-stable 的）JSON 文本
pub fn run_extract_bases(args: &ExtractLuaArgs) -> io::Result<String> {
    let entries = invoke_luajit(args)?;
    let meta = build_overlay_meta(
        args,
        BASE_ITEM_OVERRIDES_SCHEMA,
        "sync-pob-catalog extract-bases",
        "Data/Bases",
        "extract-bases",
    )?;
    Ok(assemble_base_overrides_document(meta, entries))
}

/// 组装最终文档：排序 + serde_json 统一序列化（同输入必然同输出）
pub fn assemble_base_overrides_document(
    meta: OverlayMeta,
    mut entries: Vec<BaseItemOverrideEntry>,
) -> String {
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    let doc = BaseItemOverridesDoc {
        meta,
        overrides: entries,
    };
    let mut json =
        serde_json::to_string_pretty(&doc).expect("base item overrides 文档序列化不应失败");
    json.push('\n');
    json
}

/// 启动 luajit 执行引导脚本（脚本经 stdin 注入），解析 JSONL 输出
fn invoke_luajit(args: &ExtractLuaArgs) -> io::Result<Vec<BaseItemOverrideEntry>> {
    if args.files.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "extract-bases: --files 不能为空",
        ));
    }
    let mut child = Command::new(&args.luajit)
        .arg("-") // 从 stdin 读脚本
        .arg(&args.vendor_root)
        .arg(args.files.join(","))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "无法启动 luajit（{}）：{error}；请安装 luajit 或用 --luajit / POBR_LUAJIT 指定路径",
                    args.luajit.display()
                ),
            )
        })?;

    child
        .stdin
        .take()
        .expect("stdin 已配置为 piped")
        .write_all(BOOTSTRAP_LUA.as_bytes())?;

    let output = child.wait_with_output()?;
    let stderr_text = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "luajit 引导脚本执行失败（exit: {:?}）：{}",
            output.status.code(),
            stderr_text.trim()
        )));
    }
    // 引导脚本的非致命告警透传给用户
    for line in stderr_text.lines() {
        eprintln!("extract-bases(lua): {line}");
    }

    let stdout_text = String::from_utf8(output.stdout).map_err(io::Error::other)?;
    let mut entries = Vec::new();
    for line in stdout_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let entry: BaseItemOverrideEntry = serde_json::from_str(line).map_err(|error| {
            io::Error::other(format!(
                "引导脚本输出了非法 JSONL 行：{error}；行内容：{line}"
            ))
        })?;
        entries.push(entry);
    }
    Ok(entries)
}

/// 供 main 拼装默认文件名列表。
pub fn default_base_files() -> Vec<String> {
    DEFAULT_BASE_FILES.iter().map(|s| s.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 文档组装：按 name 排序 + byte-stable（同输入两次组装逐字节相等）。
    #[test]
    fn assembles_sorted_and_byte_stable() {
        let meta = OverlayMeta {
            schema: BASE_ITEM_OVERRIDES_SCHEMA.to_string(),
            generator: "test".to_string(),
            vendor: "PathOfBuilding-PoE2".to_string(),
            vendor_commit: "0".repeat(40),
            vendor_commit_subject: "subject".to_string(),
            extracted_files: vec!["Data/Bases/shield.lua".to_string()],
            regen_command: "cargo run -p sync-pob-catalog -- extract-bases".to_string(),
        };
        let entries = vec![
            BaseItemOverrideEntry {
                name: "Omen Sceptre".to_string(),
                block_chance: None,
                spirit: Some(100),
                reload_time_ms: None,
            },
            BaseItemOverrideEntry {
                name: "Crude Tower Shield".to_string(),
                block_chance: Some(26.0),
                spirit: None,
                reload_time_ms: None,
            },
            BaseItemOverrideEntry {
                name: "Makeshift Crossbow".to_string(),
                block_chance: None,
                spirit: None,
                reload_time_ms: Some(800),
            },
        ];
        let a = assemble_base_overrides_document(meta.clone(), entries.clone());
        let b = assemble_base_overrides_document(meta, entries);
        assert_eq!(a, b);
        // 排序：Crude... < Makeshift... < Omen...。
        let crude = a.find("Crude Tower Shield").unwrap();
        let makeshift = a.find("Makeshift Crossbow").unwrap();
        let omen = a.find("Omen Sceptre").unwrap();
        assert!(crude < makeshift && makeshift < omen);
        // 消费侧 schema 能解析生成侧产物（防字段漂移）。
        let parsed: pobr_data::catalog::base_item_overrides::BaseItemOverridesDef =
            serde_json::from_str(&a).unwrap();
        assert_eq!(parsed.overrides.len(), 3);
        assert_eq!(parsed.overrides[0].block_chance, Some(26.0));
        assert_eq!(parsed.overrides[1].reload_time_ms, Some(800));
        assert_eq!(parsed.overrides[2].spirit, Some(100));
    }
}
