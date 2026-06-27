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
//!
//! 本模块同时承载各 `--what` 抽取目标的**公共层**（luajit JSONL 调用
//! [`invoke_luajit_jsonl`] / vendor 版本解析 [`read_vendor_version`]）；
//! 其它目标见 [`crate::extract_stat_map`]（M1-T2）与
//! [`crate::extract_quality`]（M1-T1）。

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

/// 引导脚本内容（经 stdin 注入 luajit，二进制自包含、不依赖运行目录）
const BOOTSTRAP_LUA: &str = include_str!("extract_skill_overrides.lua");

/// 默认 luajit 路径（macOS Homebrew）；可被 `--luajit` 或 `POBR_LUAJIT` 覆盖
const DEFAULT_LUAJIT_HOMEBREW: &str = "/opt/homebrew/bin/luajit";

/// 默认抽取的 vendor 技能数据文件：玩家主动三系 + 召唤物 / 魂灵 / 其它 +
/// 辅助三系——覆盖 `.dat` 通道拿不到的 per-skill 值（baseMultiplier 分等级值 /
/// statSet baseMods Speed MORE）涉及的全部技能来源（消费侧 merge 需要全量，缺一系
/// 即丢值）。M1-T4.3 起 critChance / attackSpeedMultiplier 已改 `.dat` 表列直读，
/// 不再经本通道（见 `extract_skill_overrides.lua` 头注）。
pub const DEFAULT_SKILL_FILES: &[&str] = &[
    "act_dex", "act_int", "act_str", "minion", "other", "spectre", "sup_dex", "sup_int", "sup_str",
];

/// `--what stat-map`（[`crate::extract_stat_map`]）默认抽取的 `Data/Skills/`
/// 文件：玩家主动三系 + 辅助三系 + 其它。**不含** minion / spectre——
/// 召唤物 statMap 留 M5a（M1 蓝图 T2.1 范围）。
pub const DEFAULT_STAT_MAP_SKILL_FILES: &[&str] = &[
    "act_dex", "act_int", "act_str", "other", "sup_dex", "sup_int", "sup_str",
];

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
    /// 写入 `_meta.regen_command` 的 `--out` 参数（仅记录，不在此层执行写盘）。
    /// 约定已是 canonical 仓库相对路径——调用方在赋值处经
    /// [`canonical_out_for_meta`] 归一化（F1），与实际写盘路径解耦。
    pub out_for_meta: Option<String>,
}

/// 单条 per-skill 覆盖值——schema 单一来源是
/// [`pobr_data::catalog::skill_overrides::SkillOverrideEntry`]（生成 / 消费两侧
/// 共用同一 serde 形状，防止字段漂移）；此处保留原名 re-export 兼容既有引用。
pub use pobr_data::catalog::skill_overrides::SkillOverrideEntry as SkillOverride;

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
    invoke_luajit_jsonl(args, BOOTSTRAP_LUA)
}

/// 通用 luajit JSONL 抽取：注入给定引导脚本（约定参数 = `<vendor_src_dir>
/// <逗号分隔文件名>`），stdout 逐行解析为 `T`，stderr 非致命告警透传。
/// 各 `--what` 目标（skill-overrides / stat-map / gem-quality）共用此层，
/// 避免 luajit 调用同形代码三处漂移。
pub fn invoke_luajit_jsonl<T: serde::de::DeserializeOwned>(
    args: &ExtractLuaArgs,
    bootstrap: &str,
) -> io::Result<Vec<T>> {
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
        .write_all(bootstrap.as_bytes())?;

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
        let entry: T = serde_json::from_str(line).map_err(|error| {
            io::Error::other(format!(
                "引导脚本输出了非法 JSONL 行：{error}；行内容：{line}"
            ))
        })?;
        entries.push(entry);
    }
    Ok(entries)
}

/// 各抽取目标的 canonical overlay 产物文件名（`--what` 目标 / 子命令别名 →
/// `data/<version>/overlay/` 下的约定文件）。F1 归一化的回退查表。
fn canonical_overlay_file(target: &str) -> Option<&'static str> {
    Some(match target {
        "skill-overrides" => "skill_overrides.json",
        "gem-quality" => "gem_quality_stats.json",
        "stat-map" => "skill_stat_map.json",
        "stat-descriptions" => "stat_descriptions.json",
        "gem-effects" => "gem_effects.json",
        "stat-set-labels" => "stat_set_labels.json",
        "config-options" => "config_options.json",
        "curse-priority" => "curse_priority.json",
        "minions" => "minions.json",
        "spectres" => "spectres.json",
        "minion-list" => "granted_effect_minions.json",
        "mod-scalability" => "mod_scalability.json",
        "runes" => "runes.json",
        "uniques" => "uniques.json",
        "catalysts" => "catalysts.json",
        "parser-rules" => "mod_parser_rules.json",
        // 子命令别名（非 `--what` 值）：extract-bases / gen-mirage-configs
        "bases" => "base_item_overrides.json",
        "mirage-configs" => "mirage_configs.json",
        _ => return None,
    })
}

/// 路径组件是否形如 PoE patch 版本号（`4.5.0.3.4`：纯数字点分、至少两段）。
fn looks_like_version(component: &str) -> bool {
    let parts: Vec<&str> = component.split('.').collect();
    parts.len() >= 2
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

/// F1（drill 发现项）：把实际传入的 `--out` 路径归一化为 **canonical 仓库相对
/// 路径**，供 `_meta.regen_command` 记录——与调用方入参（绝对 / 临时路径）解耦，
/// 重放到任意位置不再产生 `_meta` 自指差异。
///
/// 归一化规则（依序）：
/// 1. out 路径含 `data` 组件（且其后还有内容）→ 截取最后一个 `data/...` 相对段
///    （统一 `/` 分隔；临时重放只要保留 `data/<ver>/overlay/<file>` 结构即可
///    还原 canonical）；
/// 2. 否则按抽取目标的 canonical 默认路径表 `data/<version>/overlay/<file>`，
///    `<version>` 从 out 路径或 `--version-file` 路径组件推导；
/// 3. 推不出 → `None`（`regen_command` 省略 `--out`）。
pub fn canonical_out_for_meta(
    out: Option<&Path>,
    target: &str,
    version_file: Option<&Path>,
) -> Option<String> {
    let out = out?;
    let comps: Vec<&str> = out.iter().filter_map(|c| c.to_str()).collect();
    if let Some(pos) = comps.iter().rposition(|c| *c == "data")
        && pos + 1 < comps.len()
    {
        return Some(comps[pos..].join("/"));
    }
    let file = canonical_overlay_file(target)?;
    let version = comps
        .iter()
        .copied()
        .find(|c| looks_like_version(c))
        .map(str::to_string)
        .or_else(|| {
            version_file?
                .iter()
                .filter_map(|c| c.to_str())
                .find(|c| looks_like_version(c))
                .map(str::to_string)
        })?;
    Some(format!("data/{version}/overlay/{file}"))
}

/// 解析 vendor 版本文件路径：显式 `--version-file` 优先，否则按约定布局
/// `vendor/PathOfBuilding-PoE2/src` → `vendor/.pob2-version.txt`。
pub fn resolve_version_file(args: &ExtractLuaArgs) -> PathBuf {
    match &args.version_file {
        Some(path) => path.clone(),
        None => args.vendor_root.join("../../.pob2-version.txt"),
    }
}

/// 解析 `.pob2-version.txt`：首行为 commit 标题，文件中 40 位 hex 行为完整
/// hash。返回 `(commit, subject)`。各 `--what` 目标共用。
pub fn read_vendor_version(version_path: &Path) -> io::Result<(String, String)> {
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

/// 读取 vendor 版本文件并构建 `_meta`（skill_overrides 专用参数的薄封装）
fn build_meta(args: &ExtractLuaArgs) -> io::Result<OverlayMeta> {
    build_overlay_meta(
        args,
        SKILL_OVERRIDES_SCHEMA,
        "sync-pob-catalog extract-lua",
        "Data/Skills",
        "extract-lua",
    )
}

/// 读取 vendor 版本文件并构建任意 overlay 文档的 `_meta`（extract-lua /
/// extract-bases 共用：schema / generator / vendor 文件目录前缀 / 子命令名参数化）。
pub fn build_overlay_meta(
    args: &ExtractLuaArgs,
    schema: &str,
    generator: &str,
    file_dir_prefix: &str,
    subcommand: &str,
) -> io::Result<OverlayMeta> {
    let (commit, subject) = read_vendor_version(&resolve_version_file(args))?;

    let extracted_files: Vec<String> = args
        .files
        .iter()
        .map(|name| format!("{file_dir_prefix}/{name}.lua"))
        .collect();

    // regen_command 按约定写 canonical 相对路径（从仓库根执行），与实际传入的
    // 绝对路径解耦，保证不同机器 / 任意 out 位置重跑产物 byte 一致——vendor-root
    // 在此固定；out 已由调用方经 canonical_out_for_meta 归一化（F1）。
    let mut regen = format!(
        "cargo run -p sync-pob-catalog -- {subcommand} --vendor-root vendor/PathOfBuilding-PoE2/src --files {}",
        args.files.join(",")
    );
    if let Some(out) = &args.out_for_meta {
        regen.push_str(&format!(" --out {out}"));
    }

    Ok(OverlayMeta {
        schema: schema.to_string(),
        generator: generator.to_string(),
        vendor: "PathOfBuilding-PoE2".to_string(),
        vendor_commit: commit,
        vendor_commit_subject: subject,
        extracted_files,
        regen_command: regen,
    })
}

#[cfg(test)]
mod tests {
    use super::canonical_out_for_meta;
    use std::path::Path;

    /// 规则 1：out 路径含 `data/` 组件 → 截取相对段（绝对路径 / 临时重放均还原
    /// canonical，与机器及临时目录无关）。
    #[test]
    fn truncates_at_last_data_component() {
        for raw in [
            "data/4.5.0.3.4/overlay/curse_priority.json",
            "/Users/x/codes/pobr/data/4.5.0.3.4/overlay/curse_priority.json",
            "/tmp/pobr-drill.AbC/replay/data/4.5.0.3.4/overlay/curse_priority.json",
        ] {
            assert_eq!(
                canonical_out_for_meta(Some(Path::new(raw)), "curse-priority", None).as_deref(),
                Some("data/4.5.0.3.4/overlay/curse_priority.json"),
                "out = {raw}"
            );
        }
    }

    /// 规则 2：临时路径无 `data/` 组件 → 按 what-target 默认表 + 路径内版本号回退。
    #[test]
    fn falls_back_to_what_target_table_with_version_from_path() {
        assert_eq!(
            canonical_out_for_meta(
                Some(Path::new("/tmp/4.5.0.3.4/regen.json")),
                "minion-list",
                None,
            )
            .as_deref(),
            Some("data/4.5.0.3.4/overlay/granted_effect_minions.json"),
        );
        // 版本号也可从 --version-file 路径组件推导
        assert_eq!(
            canonical_out_for_meta(
                Some(Path::new("/tmp/regen.json")),
                "bases",
                Some(Path::new("/snapshots/4.5.0.3.4/.pob2-version.txt")),
            )
            .as_deref(),
            Some("data/4.5.0.3.4/overlay/base_item_overrides.json"),
        );
    }

    /// 规则 3：版本推不出 / 未知抽取目标 → None（regen_command 省略 --out）。
    #[test]
    fn omits_out_when_underivable() {
        assert_eq!(
            canonical_out_for_meta(Some(Path::new("/tmp/regen.json")), "curse-priority", None),
            None,
        );
        assert_eq!(
            canonical_out_for_meta(
                Some(Path::new("/tmp/4.5.0.3.4/x.json")),
                "no-such-target",
                None,
            ),
            None,
        );
        assert_eq!(canonical_out_for_meta(None, "curse-priority", None), None);
    }

    /// 末尾孤立 `data` 组件不触发规则 1（无相对段可截）。
    #[test]
    fn trailing_bare_data_component_is_ignored() {
        assert_eq!(
            canonical_out_for_meta(Some(Path::new("/tmp/4.5.0.3.4/data")), "runes", None)
                .as_deref(),
            Some("data/4.5.0.3.4/overlay/runes.json"),
        );
    }
}
