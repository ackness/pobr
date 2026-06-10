//! `extract-lua` 子命令：用 luajit 在最小 stub 环境下执行 vendor PoB2 的 Lua
//! 数据文件，把人工策展层（Export 模板 #baseMod / per-skill 覆盖值等）固化为
//! **确定性 JSON** 落到 `data/<版本>/overlay/`，替代"绕过适配器手改产物 JSON"
//! 的一次性补丁（架构裁决 P13，缺口 15-data-pipeline Gap3）。
//!
//! 职责切分：
//! - Lua 引导脚本（`extract_skill_overrides.lua`，编译期内嵌）只负责忠实抽取
//!   并以 JSONL 输出；
//! - Rust 侧统一做排序、数字格式（serde_json 最短往返表示）与整体文档序列化，
//!   保证同输入重跑 **byte-stable**。

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

/// 引导脚本内容（经 stdin 注入 luajit，二进制自包含、不依赖运行目录）
const BOOTSTRAP_LUA: &str = include_str!("extract_skill_overrides.lua");

/// 默认 luajit 路径（macOS Homebrew）；可被 `--luajit` 或 `POBR_LUAJIT` 覆盖
const DEFAULT_LUAJIT_HOMEBREW: &str = "/opt/homebrew/bin/luajit";

/// 默认抽取的 vendor 技能数据文件（玩家主动技能三系；小表起步，后续按需扩列）
pub const DEFAULT_SKILL_FILES: &[&str] = &["act_dex", "act_int", "act_str"];

/// 当前 overlay 文档 schema 标识（字段演化时递增）
pub const SKILL_OVERRIDES_SCHEMA: &str = "skill_overrides/v1";

/// 解析 luajit 可执行路径：显式参数 > `POBR_LUAJIT` 环境变量 > Homebrew 默认
/// 路径（存在时）> PATH 上的 `luajit`。
pub fn resolve_luajit(explicit: Option<&Path>) -> PathBuf {
    if let Some(path) = explicit {
        return path.to_path_buf();
    }
    if let Ok(env_path) = std::env::var("POBR_LUAJIT")
        && !env_path.is_empty()
    {
        return PathBuf::from(env_path);
    }
    let homebrew = Path::new(DEFAULT_LUAJIT_HOMEBREW);
    if homebrew.exists() {
        return homebrew.to_path_buf();
    }
    PathBuf::from("luajit")
}

/// luajit 是否可执行（CI 无 luajit 时测试据此跳过）
pub fn luajit_available(luajit: &Path) -> bool {
    Command::new(luajit)
        .arg("-v")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// extract-lua 的运行参数
#[derive(Debug)]
pub struct ExtractLuaArgs {
    /// vendor PoB2 源码目录（`vendor/PathOfBuilding-PoE2/src`，只读输入）
    pub vendor_root: PathBuf,
    /// luajit 可执行路径
    pub luajit: PathBuf,
    /// 抽取的技能数据文件名（不含 `.lua` 后缀）
    pub files: Vec<String>,
    /// vendor 版本记录文件；缺省取 `<vendor_root>/../../.pob2-version.txt`
    pub version_file: Option<PathBuf>,
    /// 写入 `_meta.regen_command` 的 `--out` 参数（仅记录，不在此层执行写盘）
    pub out_for_meta: Option<String>,
}

/// 单条 per-skill 覆盖值。`value` 与 `per_level` 二选一：
/// 所有等级同值时压缩为 `value`，否则保留 `per_level: [[level, value], ...]`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillOverride {
    /// vendor 技能 id（= GrantedEffects.Id，如 `FlickerStrikePlayer`）
    pub skill: String,
    /// 入库 stat 名（`crit_chance` / `attack_speed_multiplier` / `skill_attack_speed_more`）
    pub stat: String,
    /// statSet 序号（仅 statSet 级覆盖值携带，如 baseMods 的 Speed MORE）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stat_set: Option<u32>,
    /// 全等级同值（或与等级无关）时的单值
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    /// 按等级明细：`[[level, value], ...]`，level 升序
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_level: Option<Vec<(u32, f64)>>,
}

/// overlay 文档头部元信息：记录 vendor 版本与再生成命令，保证可追溯、可重跑
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayMeta {
    /// schema 标识
    pub schema: String,
    /// 生成器标识
    pub generator: String,
    /// vendor 仓库名
    pub vendor: String,
    /// vendor commit 完整 hash（读自 `.pob2-version.txt`）
    pub vendor_commit: String,
    /// vendor commit 标题行（人类可读对照）
    pub vendor_commit_subject: String,
    /// 实际抽取的 vendor 文件（相对 vendor src 根）
    pub extracted_files: Vec<String>,
    /// 再生成命令（从仓库根目录执行；vendor 路径按约定写 canonical 相对路径）
    pub regen_command: String,
}

/// 完整 overlay 文档
#[derive(Debug, Serialize, Deserialize)]
pub struct SkillOverridesDoc {
    /// 头部元信息（serde 落为 `_meta`，置于文件最前）
    #[serde(rename = "_meta")]
    pub meta: OverlayMeta,
    /// 覆盖值列表，按 (skill, stat, stat_set) 排序
    pub overrides: Vec<SkillOverride>,
}

/// 执行抽取，返回最终（byte-stable 的）JSON 文本
pub fn run_extract_lua(args: &ExtractLuaArgs) -> io::Result<String> {
    let entries = invoke_luajit(args)?;
    let meta = build_meta(args)?;
    Ok(assemble_overrides_document(meta, entries))
}

/// 组装最终文档：排序 + serde_json 统一序列化（同输入必然同输出）
pub fn assemble_overrides_document(meta: OverlayMeta, mut entries: Vec<SkillOverride>) -> String {
    entries.sort_by(|a, b| {
        a.skill
            .cmp(&b.skill)
            .then_with(|| a.stat.cmp(&b.stat))
            .then_with(|| a.stat_set.unwrap_or(0).cmp(&b.stat_set.unwrap_or(0)))
    });
    let doc = SkillOverridesDoc {
        meta,
        overrides: entries,
    };
    let mut json = serde_json::to_string_pretty(&doc).expect("skill overrides 文档序列化不应失败");
    json.push('\n');
    json
}

/// 启动 luajit 执行引导脚本（脚本经 stdin 注入），解析 JSONL 输出
fn invoke_luajit(args: &ExtractLuaArgs) -> io::Result<Vec<SkillOverride>> {
    if args.files.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "extract-lua: --files 不能为空",
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
    let mut entries = Vec::new();
    for line in stdout_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let entry: SkillOverride = serde_json::from_str(line).map_err(|error| {
            io::Error::other(format!(
                "引导脚本输出了非法 JSONL 行：{error}；行内容：{line}"
            ))
        })?;
        entries.push(entry);
    }
    Ok(entries)
}

/// 读取 vendor 版本文件并构建 `_meta`
fn build_meta(args: &ExtractLuaArgs) -> io::Result<OverlayMeta> {
    let version_path = match &args.version_file {
        Some(path) => path.clone(),
        // 约定布局 vendor/PathOfBuilding-PoE2/src → 版本文件在 vendor/.pob2-version.txt
        None => args.vendor_root.join("../../.pob2-version.txt"),
    };
    let version_text = fs::read_to_string(&version_path).map_err(|error| {
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

    let extracted_files: Vec<String> = args
        .files
        .iter()
        .map(|name| format!("Data/Skills/{name}.lua"))
        .collect();

    // regen_command 按约定写 canonical 相对路径（从仓库根执行），与实际传入的
    // 绝对路径解耦，保证不同机器上重跑产物 byte 一致。
    let mut regen = format!(
        "cargo run -p sync-pob-catalog -- extract-lua --vendor-root vendor/PathOfBuilding-PoE2/src --files {}",
        args.files.join(",")
    );
    if let Some(out) = &args.out_for_meta {
        regen.push_str(&format!(" --out {out}"));
    }

    Ok(OverlayMeta {
        schema: SKILL_OVERRIDES_SCHEMA.to_string(),
        generator: "sync-pob-catalog extract-lua".to_string(),
        vendor: "PathOfBuilding-PoE2".to_string(),
        vendor_commit: commit,
        vendor_commit_subject: subject,
        extracted_files,
        regen_command: regen,
    })
}
