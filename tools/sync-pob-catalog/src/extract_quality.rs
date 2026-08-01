//! `extract-lua --what gem-quality`：vendor PoB2 `Data/Skills/*.lua` 的
//! `qualityStats` 字段 → `data/<版本>/overlay/gem_quality_stats.json`
//!
//! **通道说明**：原定从 `.dat` 表 `GrantedEffectQualityStats` 走 adapter →
//! `base/`，但该表所在 bundle 在钉定补丁 4.5.0.3.4 已无法下载（核验，见
//! `pipeline/config.json` 的 `_tablesUnavailableForPinnedPatch`），按 owner 裁决
//! 「生产工具定层」：extract-lua 抽取 → **overlay/**。vendor
//! 数据文件本就是导出产物（rate 已 `/1000`、辅助宝石已按导出条件跳过，
//! `Export/Scripts/skills.lua:304-313`），抽取为忠实转录。后续若 `.dat` 表通道
//! 恢复则迁回 `base/`（迁移 commit byte 等价）。
//!
//! 职责切分与 [`crate::extract_lua`] 一致：Lua 引导脚本
//! （`extract_gem_quality.lua`，编译期内嵌）只负责忠实抽取并以 JSONL 输出；
//! Rust 侧统一做排序（effect_id 升序，效果内保持 vendor 顺序）、数字最短往返
//! 表示与整体文档序列化，保证同输入重跑 **byte-stable**。
//!
//! NOTE(T2)：luajit 调用 / `_meta` 构建与 `extract_lua.rs` 存在少量同形代码——
//! 该文件归 T2（stat-map 抽取）owner，T2 落地时可顺势抽公共层统一。

use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use pobr_data::catalog::{GemQualityStatDef, QualityStat};
use serde::{Deserialize, Serialize};

use crate::extract_lua::{ExtractLuaArgs, OverlayMeta};

/// 引导脚本内容（经 stdin 注入 luajit，二进制自包含、不依赖运行目录）
const BOOTSTRAP_LUA: &str = include_str!("extract_gem_quality.lua");

/// 当前 overlay 文档 schema 标识（字段演化时递增）
pub const GEM_QUALITY_SCHEMA: &str = "gem_quality_stats/v1";

/// 引导脚本输出的单行 JSONL：一条 (效果, stat, 每品质斜率)。
#[derive(Debug, Clone, Deserialize)]
pub struct QualityRow {
    /// 授予效果 id（如 `CometPlayer`）。
    pub effect: String,
    /// 稳定 stat id。
    pub stat: String,
    /// 每 1 点品质的斜率（vendor 数据已 `/1000`，原样转录）。
    pub rate: f64,
    /// vendor `altQualityStats` 条目（仅 GemlingQuality flag build 生效）。
    #[serde(default)]
    pub alt: bool,
}

/// 完整 overlay 文档（生成侧；消费侧 schema 见
/// [`pobr_data::catalog::GemQualityStatsDef`]，serde 形状一致防字段漂移）。
#[derive(Debug, Serialize, Deserialize)]
pub struct GemQualityDoc {
    /// 头部元信息（serde 落为 `_meta`，置于文件最前）
    #[serde(rename = "_meta")]
    pub meta: OverlayMeta,
    /// 品质 stat 表，按 effect_id 升序。
    pub effects: Vec<GemQualityStatDef>,
}

/// 执行抽取，返回最终（byte-stable 的）JSON 文本。
pub fn run_extract_gem_quality(args: &ExtractLuaArgs) -> io::Result<String> {
    let rows = invoke_luajit(args)?;
    let meta = build_meta(args)?;
    Ok(assemble_quality_document(meta, rows))
}

/// 组装最终文档：按 effect_id 分组排序（效果内保持到达顺序 = vendor 顺序）+
/// serde_json 统一序列化（同输入必然同输出）。
pub fn assemble_quality_document(meta: OverlayMeta, rows: Vec<QualityRow>) -> String {
    // 分组：effect_id → 斜率列表（到达顺序即 vendor ipairs 顺序，效果内不重排）。
    let mut by_effect: std::collections::BTreeMap<String, Vec<QualityStat>> =
        std::collections::BTreeMap::new();
    for row in rows {
        by_effect.entry(row.effect).or_default().push(QualityStat {
            stat: row.stat,
            per_quality_rate: row.rate,
            alt: row.alt,
        });
    }
    let effects = by_effect
        .into_iter()
        .map(|(effect_id, stats)| GemQualityStatDef { effect_id, stats })
        .collect();
    let doc = GemQualityDoc { meta, effects };
    let mut json = serde_json::to_string_pretty(&doc).expect("gem quality 文档序列化不应失败");
    json.push('\n');
    json
}

/// 启动 luajit 执行引导脚本（脚本经 stdin 注入），解析 JSONL 输出。
fn invoke_luajit(args: &ExtractLuaArgs) -> io::Result<Vec<QualityRow>> {
    if args.files.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "extract-lua --what gem-quality: --files 不能为空",
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
        eprintln!("extract-lua(lua): {line}");
    }

    let stdout_text = String::from_utf8(output.stdout).map_err(io::Error::other)?;
    let mut rows = Vec::new();
    for line in stdout_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let row: QualityRow = serde_json::from_str(line).map_err(|error| {
            io::Error::other(format!(
                "引导脚本输出了非法 JSONL 行：{error}；行内容：{line}"
            ))
        })?;
        rows.push(row);
    }
    Ok(rows)
}

/// 读取 vendor 版本文件并构建 `_meta`（与 `extract_lua::build_meta` 同约定：
/// regen_command 写 canonical 相对路径，跨机器重跑产物 byte 一致）。
fn build_meta(args: &ExtractLuaArgs) -> io::Result<OverlayMeta> {
    let version_path = match &args.version_file {
        Some(path) => path.clone(),
        // 约定布局 vendor/PathOfBuilding-PoE2/src → 版本文件在 vendor/.pob2-version.txt
        None => args.vendor_root.join("../../.pob2-version.txt"),
    };
    let (commit, subject) = read_vendor_version(&version_path)?;

    let extracted_files: Vec<String> = args
        .files
        .iter()
        .map(|name| format!("Data/Skills/{name}.lua"))
        .collect();

    let mut regen = format!(
        "cargo run -p sync-pob-catalog -- extract-lua --what gem-quality --vendor-root vendor/PathOfBuilding-PoE2/src --files {}",
        args.files.join(",")
    );
    if let Some(out) = &args.out_for_meta {
        regen.push_str(&format!(" --out {out}"));
    }

    Ok(OverlayMeta {
        schema: GEM_QUALITY_SCHEMA.to_string(),
        generator: "sync-pob-catalog extract-lua".to_string(),
        vendor: "PathOfBuilding-PoE2".to_string(),
        vendor_commit: commit,
        vendor_commit_subject: subject,
        extracted_files,
        regen_command: regen,
    })
}

/// 解析 `.pob2-version.txt`：首行为 commit 标题，文件中 40 位 hex 行为完整 hash。
fn read_vendor_version(version_path: &Path) -> io::Result<(String, String)> {
    let version_text = fs::read_to_string(version_path).map_err(|error| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "无法读取 vendor 版本文件 {}：{error}；可用 --version-file 显式指定",
                version_path.display()
            ),
        )
    })?;
    let subject = version_text.lines().next().unwrap_or("").trim().to_string();
    let commit = version_text
        .lines()
        .map(str::trim)
        .find(|line| line.len() == 40 && line.bytes().all(|b| b.is_ascii_hexdigit()))
        .unwrap_or("")
        .to_string();
    if commit.is_empty() {
        return Err(io::Error::other(format!(
            "vendor 版本文件 {} 中未找到 40 位 commit hash",
            version_path.display()
        )));
    }
    Ok((commit, subject))
}
