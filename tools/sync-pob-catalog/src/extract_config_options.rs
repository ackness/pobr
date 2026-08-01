//! `extract-lua --what config-options`：ConfigOptions.lua 探针法抽取。
//!
//! 与其它 `--what` 目标不同，本目标需要**完整 PoB2 headless 环境**
//! （HeadlessWrapper：真实 `data` / `modLib` / `LoadModule`），因此 luajit
//! 以 `cwd = <vendor_root>` + `LUA_PATH` 指向 `runtime/lua` 的方式启动
//! （与 `tools/pob2-oracle/run.sh` 同款引导），而非走
//! [`crate::extract_lua::invoke_luajit_jsonl`] 的无 cwd 通道。
//!
//! 职责切分（确定性抽取约定）：
//! - Lua 引导脚本（`extract_config_options.lua`，编译期内嵌）做探针归纳并
//!   逐行输出 serde 形状的条目 JSON（JSONL）；
//! - 本模块负责启动 / 解析 / 按 `var` 排序 / `_meta` 组装 / byte-stable
//!   序列化。

use std::io::{self, Write};
use std::process::{Command, Stdio};

use pobr_data::catalog::config_def::{CONFIG_OPTIONS_SCHEMA, ConfigOptionDef};
use serde::{Deserialize, Serialize};

use crate::extract_lua::{ExtractLuaArgs, OverlayMeta, read_vendor_version, resolve_version_file};

/// 引导脚本内容（经 stdin 注入 luajit）。
const BOOTSTRAP_LUA: &str = include_str!("extract_config_options.lua");

/// 完整 overlay 文档（生产侧；消费侧用 `ConfigOptionsDef` 忽略 `_meta`）。
#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigOptionsDoc {
    /// 头部元信息。
    #[serde(rename = "_meta")]
    pub meta: OverlayMeta,
    /// 条目列表，按 `var` 升序。
    pub options: Vec<ConfigOptionDef>,
}

/// 执行抽取，返回最终（byte-stable 的）JSON 文本。
pub fn run_extract_config_options(args: &ExtractLuaArgs) -> io::Result<String> {
    let entries = invoke_headless_luajit(args)?;
    let meta = build_meta(args)?;
    Ok(assemble_document(meta, entries))
}

/// 组装最终文档：按 var 排序 + serde_json 统一序列化（同输入必然同输出）。
pub fn assemble_document(meta: OverlayMeta, mut entries: Vec<ConfigOptionDef>) -> String {
    entries.sort_by(|a, b| a.var.cmp(&b.var).then_with(|| a.section.cmp(&b.section)));
    let doc = ConfigOptionsDoc {
        meta,
        options: entries,
    };
    let mut json = serde_json::to_string_pretty(&doc).expect("config options 文档序列化不应失败");
    json.push('\n');
    json
}

/// headless 模式启动 luajit（cwd = vendor src、LUA_PATH 指 runtime/lua）。
fn invoke_headless_luajit(args: &ExtractLuaArgs) -> io::Result<Vec<ConfigOptionDef>> {
    let vendor_root = &args.vendor_root;
    if !vendor_root.join("HeadlessWrapper.lua").exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "vendor root {} 下找不到 HeadlessWrapper.lua（config-options 抽取需要完整 PoB2 src）",
                vendor_root.display()
            ),
        ));
    }
    let mut child = Command::new(&args.luajit)
        .arg("-") // 从 stdin 读脚本
        .current_dir(vendor_root)
        .env(
            "LUA_PATH",
            "../runtime/lua/?.lua;../runtime/lua/?/init.lua;./?.lua;;",
        )
        .env("CI", "true")
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
    for line in stderr_text.lines() {
        eprintln!("extract-config-options(lua): {line}");
    }

    let stdout_text = String::from_utf8(output.stdout).map_err(io::Error::other)?;
    let mut entries = Vec::new();
    for line in stdout_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let entry: ConfigOptionDef = serde_json::from_str(line).map_err(|error| {
            io::Error::other(format!(
                "引导脚本输出了非法条目 JSON：{error}；行内容：{line}"
            ))
        })?;
        entries.push(entry);
    }
    if entries.is_empty() {
        return Err(io::Error::other(
            "config-options 抽取产出 0 条目（引导脚本异常）",
        ));
    }
    Ok(entries)
}

/// 构建 `_meta`（vendor commit + canonical 再生成命令）。
fn build_meta(args: &ExtractLuaArgs) -> io::Result<OverlayMeta> {
    let (commit, subject) = read_vendor_version(&resolve_version_file(args))?;
    let mut regen = String::from(
        "cargo run -p sync-pob-catalog -- extract-lua --vendor-root vendor/PathOfBuilding-PoE2/src --what config-options",
    );
    if let Some(out) = &args.out_for_meta {
        regen.push_str(&format!(" --out {out}"));
    }
    Ok(OverlayMeta {
        schema: CONFIG_OPTIONS_SCHEMA.to_string(),
        generator: "sync-pob-catalog extract-lua --what config-options".to_string(),
        vendor: "PathOfBuilding-PoE2".to_string(),
        vendor_commit: commit,
        vendor_commit_subject: subject,
        extracted_files: vec!["Modules/ConfigOptions.lua".to_string()],
        regen_command: regen,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta() -> OverlayMeta {
        OverlayMeta {
            schema: CONFIG_OPTIONS_SCHEMA.to_string(),
            generator: "test".to_string(),
            vendor: "PathOfBuilding-PoE2".to_string(),
            vendor_commit: "0".repeat(40),
            vendor_commit_subject: "test".to_string(),
            extracted_files: vec!["Modules/ConfigOptions.lua".to_string()],
            regen_command: "test".to_string(),
        }
    }

    fn entry(var: &str) -> ConfigOptionDef {
        serde_json::from_str(&format!(
            r#"{{"var":"{var}","input_type":"check","section":"General","verified":true}}"#
        ))
        .unwrap()
    }

    /// 组装按 var 排序且 byte-stable（同输入两次组装逐字节一致）。
    #[test]
    fn assemble_sorts_and_is_byte_stable() {
        let entries = vec![entry("b"), entry("a")];
        let one = assemble_document(meta(), entries.clone());
        let two = assemble_document(meta(), entries);
        assert_eq!(one, two);
        let doc: ConfigOptionsDoc = serde_json::from_str(&one).unwrap();
        assert_eq!(doc.options[0].var, "a");
        assert_eq!(doc.options[1].var, "b");
        assert_eq!(doc.meta.schema, CONFIG_OPTIONS_SCHEMA);
    }
}
