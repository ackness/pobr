//! `extract-lua --what special-mods`——vendor `specialModList` 批量抽取（V0 批次）。
//!
//! Lua 侧（`extract_special_mods.lua`）headless 引导 + 双哨兵探针 dump JSONL；
//! 本模块负责：
//!
//! 1. **Lua pattern → Rust regex**：严格白名单子集转换（数字捕获闭集、`%` 转义、
//!    已知字符类、`?+*-` 量词）；转不动整条跳过并计数——宁缺毋错；
//! 2. **忠实性白名单**：tag 形态 / flag 名 / value 模板逐项过
//!    `pobr-core::rules::{tag_is_mappable, flag_name_is_mappable, ...}` 预检。
//!    编译期会静默丢弃不可映射 tag（保守门控），批量条目一旦丢 tag 就会把条件
//!    词条变常驻——所以这里**整条跳过**而非丢 tag；
//! 3. **去重**：vendor key 已被 `overlay/special_mods.json`（人工策展，优先）或
//!    `generated/special_derived.json`（keystone 派生）覆盖 → 跳过；批内 regex
//!    字符串冲突（`targets?` 变体收敛等）→ 后到者跳过；
//! 4. **编译验证**：逐条 + 全量过 [`SpecialModRules::compile`]，保证产物
//!    `generated/special_vendor.json` 永远可被消费侧 fail-fast 加载；
//! 5. 跳过原因计数报表到 stderr（V1 扩围的输入：词类捕获→enums、enemy 包装等）。
//!
//! 产物条目：`batch:"V0"`、`verified:false`、id 形如 `vnd_<slug>_<hash8>`。

use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use pobr_core::rules::{
    HandlerRegistry, SpecialModRules, flag_name_is_mappable, keyword_flag_name_is_mappable,
    tag_is_mappable,
};
use pobr_data::catalog::parser_rules::{
    ModTemplateDef, SpecialModsDef, SpecialTemplateDef, TemplateNameDef, TemplateScalarDef,
    TemplateTagDef, TemplateValueDef, ValueExprDef, ValueOpDef,
};
use serde::{Deserialize, Serialize};

use crate::extract_lua::{ExtractLuaArgs, OverlayMeta, read_vendor_version, resolve_version_file};

/// 引导脚本内容（经 stdin 注入 luajit，二进制自包含）。
const BOOTSTRAP_LUA: &str = include_str!("extract_special_mods.lua");

const SPECIAL_MODS_SCHEMA: &str = "special_mods/v1";
const BATCH: &str = "V0";

/// 输出文档（`SpecialModsDef` 只派生 `Deserialize`，写盘侧自持结构）。
#[derive(Serialize)]
struct SpecialVendorDoc {
    #[serde(rename = "_meta")]
    meta: OverlayMeta,
    entries: Vec<SpecialTemplateDef>,
}

/// Lua 侧 JSONL 行。
#[derive(Deserialize)]
struct RawRow {
    pattern: String,
    kind: String,
    #[serde(default)]
    mods: serde_json::Value,
    #[serde(default)]
    reason: Option<String>,
    /// `kind:"enum"` 行：词槽的捕获序号（1-based）。
    #[serde(default)]
    word_slots: Vec<usize>,
    /// `kind:"enum"` 行：探针字典大小（开放词汇判定：命中数 == 字典全量
    /// 且 mods 依赖该词 → 闭集假设不成立，整条跳过）。
    #[serde(default)]
    dict_size: usize,
    /// `kind:"enum"` 行：逐词组合的推断结果。
    #[serde(default)]
    variants: Vec<RawEnumVariant>,
}

/// 词类探针的单个命中组合（words[i] 对应 word_slots[i] 槽）。
#[derive(Deserialize)]
struct RawEnumVariant {
    words: Vec<String>,
    mods: serde_json::Value,
}

/// 执行抽取，返回最终 JSON 文本。
pub fn run_extract_special_mods(args: &ExtractLuaArgs) -> io::Result<String> {
    let rows = invoke_headless_jsonl(args)?;
    let (existing_keys, existing_patterns) = load_existing_keys()?;

    let mut stats: BTreeMap<String, usize> = BTreeMap::new();
    let bump = |stats: &mut BTreeMap<String, usize>, key: &str| {
        *stats.entry(key.to_string()).or_insert(0) += 1;
    };
    let mut entries: Vec<SpecialTemplateDef> = Vec::new();
    let mut seen_patterns: BTreeSet<String> = BTreeSet::new();
    let mut seen_ids: BTreeSet<String> = BTreeSet::new();
    let registry = HandlerRegistry::new();

    // V2s5 预扫：全库已知 mod name 集（静态/数字推断行，非词类探针产物）。
    // 词类探针的开放槽用它过滤拼接型闭包的有效词——`FireResist` 在其他
    // 词条反复出现（有效），`StrengthResist` 全库唯一（字典噪声词）。
    // vendor 数据自证，无语义启发式。
    let mut known_names: BTreeSet<String> = BTreeSet::new();
    for row in &rows {
        if row.kind == "static" || row.kind == "inferred" {
            collect_mod_names(&row.mods, &mut known_names);
        }
    }

    for row in rows {
        bump(&mut stats, "total");
        if row.kind == "failed" {
            let reason = row.reason.as_deref().unwrap_or("unknown");
            // 探针失败归大类计数（细节原因已由 Lua stderr 透传给日志）
            let class = if reason == "nonnumeric_capture" {
                "skip_nonnumeric_capture"
            } else if reason.starts_with("probe:") {
                "skip_probe_failed"
            } else {
                "skip_lua_failed"
            };
            bump(&mut stats, class);
            continue;
        }
        if existing_keys.contains(&row.pattern) {
            bump(&mut stats, "skip_dedup_existing_key");
            continue;
        }
        // V2s5：词类捕获字典探针行 → enums 闭集统一化；统一化因结构差异
        // 失败时降级为单词条目（词特定值全字面量，tag 差异自然消解）。
        if row.kind == "enum" {
            let rescued = match rescue_open_variants(&row, &known_names) {
                Ok(v) => v,
                Err(reason) => {
                    bump(&mut stats, &format!("skip_{reason}"));
                    continue;
                }
            };
            let unified = build_enum_entry(
                &row,
                &rescued,
                &registry,
                &existing_patterns,
                &mut seen_patterns,
                &mut seen_ids,
            );
            match unified {
                Ok(entry) => {
                    bump(
                        &mut stats,
                        if entry.mods.is_empty() {
                            "emitted_empty"
                        } else {
                            "emitted_enum"
                        },
                    );
                    entries.push(entry);
                }
                Err(reason)
                    if matches!(
                        reason.as_str(),
                        "enum_diff_position"
                            | "enum_diff_scalar"
                            | "enum_diff_shape"
                            | "enum_diff_multislot"
                            | "enum_slot_conflict"
                    ) =>
                {
                    match build_singleton_entries(
                        &row,
                        &rescued,
                        &registry,
                        &existing_patterns,
                        &mut seen_patterns,
                        &mut seen_ids,
                    ) {
                        Ok(list) => {
                            for entry in list {
                                bump(&mut stats, "emitted_enum_singleton");
                                entries.push(entry);
                            }
                        }
                        Err(reason) => bump(&mut stats, &format!("skip_{reason}")),
                    }
                }
                Err(reason) => bump(&mut stats, &format!("skip_{reason}")),
            }
            continue;
        }
        // 静态条目的捕获值未被 mods 引用 → 降级非捕获组；inferred 保留捕获。
        let keep_captures = row.kind == "inferred";
        let (regex, caps) =
            match lua_pattern_to_regex(&row.pattern, keep_captures, &BTreeMap::new()) {
                Ok(v) => v,
                Err(reason) => {
                    bump(&mut stats, "skip_pattern_unconvertible");
                    eprintln!(
                        "extract-special-mods: pattern unconvertible `{}`：{reason}",
                        row.pattern
                    );
                    continue;
                }
            };
        if existing_patterns.contains(&regex) {
            bump(&mut stats, "skip_dedup_existing_pattern");
            continue;
        }
        if !seen_patterns.insert(regex.clone()) {
            bump(&mut stats, "skip_dedup_self");
            continue;
        }
        let mods = match transform_mods(&row.mods) {
            Ok(mods) => mods,
            Err(reason) => {
                bump(&mut stats, &format!("skip_{reason}"));
                continue;
            }
        };
        if let Err(reason) = validate_refs(&mods, caps) {
            bump(&mut stats, &format!("skip_{reason}"));
            continue;
        }
        let id = format!("vnd_{}_{}", slug(&row.pattern), stable_hash8(&row.pattern));
        if !seen_ids.insert(id.clone()) {
            bump(&mut stats, "skip_dedup_id");
            continue;
        }
        let is_empty = mods.is_empty();
        let entry = SpecialTemplateDef {
            id,
            pattern: regex,
            vendor_pattern: Some(row.pattern.clone()),
            mods,
            handler_id: None,
            handler_args: Vec::new(),
            enums: BTreeMap::new(),
            verified: false,
            batch: BATCH.to_string(),
            source_note: Some(format!(
                "vendor specialModList 批量抽取（{}）",
                if row.kind == "inferred" {
                    "闭包探针"
                } else {
                    "静态表"
                }
            )),
        };
        // 逐条编译验证：pattern regex 合法性 / mod_type / enums 引用。
        if let Err(error) = SpecialModRules::compile(std::slice::from_ref(&entry), &registry) {
            bump(&mut stats, "skip_compile_failed");
            eprintln!(
                "extract-special-mods: compile failed `{}`：{error}",
                entry.id
            );
            continue;
        }
        bump(
            &mut stats,
            if is_empty { "emitted_empty" } else { "emitted" },
        );
        entries.push(entry);
    }

    entries.sort_by(|a, b| a.id.cmp(&b.id));
    // 全量再编译一次：批内 id/pattern 唯一性兜底（与消费侧闸门同一函数）。
    SpecialModRules::compile(&entries, &registry).map_err(|error| {
        io::Error::other(format!("special_vendor 全量编译失败（不应发生）：{error}"))
    })?;

    eprintln!("extract-special-mods: ---- 统计 ----");
    for (key, count) in &stats {
        eprintln!("extract-special-mods:   {key}: {count}");
    }

    let doc = SpecialVendorDoc {
        meta: build_meta(args)?,
        entries,
    };
    let mut json = serde_json::to_string_pretty(&doc).expect("special_vendor 文档序列化不应失败");
    json.push('\n');
    Ok(json)
}

// headless 调用（同 extract_parser_rules 约定：stdin 注入、JSONL 回收）

fn invoke_headless_jsonl(args: &ExtractLuaArgs) -> io::Result<Vec<RawRow>> {
    // 绝对化：cwd 会切到 vendor src/，相对 vendor_root 会让 LUA_PATH 失效。
    let vendor_root = args.vendor_root.canonicalize()?;
    let runtime = vendor_root.join("../runtime/lua");
    let lua_path = format!("{r}/?.lua;{r}/?/init.lua;./?.lua;;", r = runtime.display());
    let mut child = Command::new(&args.luajit)
        .arg("-")
        .arg(&vendor_root)
        .current_dir(&vendor_root)
        .env("LUA_PATH", lua_path)
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
            "special-mods 引导脚本执行失败（exit: {:?}）：{}",
            output.status.code(),
            stderr_text.trim()
        )));
    }
    for line in stderr_text.lines() {
        eprintln!("extract-special-mods(lua): {line}");
    }

    let stdout_text = String::from_utf8(output.stdout).map_err(io::Error::other)?;
    let mut rows = Vec::new();
    for line in stdout_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let row: RawRow = serde_json::from_str(line).map_err(|error| {
            io::Error::other(format!(
                "引导脚本输出了非法 JSONL 行：{error}；行内容：{line}"
            ))
        })?;
        rows.push(row);
    }
    Ok(rows)
}

// 去重输入：已有 overlay / derived 覆盖的 vendor key 与 pattern

fn repo_data_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data")
        .join(pobr_data::data_version())
}

/// 返回（raw key 集 = vendor_pattern ∪ pattern；regex pattern 集）。缺文件容忍
/// （version-bump 演练时可能只有部分文件）。
///
/// 去重源含**三处** special_mods：版本无关策展层 `data/overlay-common/special_mods.json`
/// （P1-3）、版本层 `overlay/special_mods.json`、派生表 `generated/special_derived.json`。
/// 漏读 overlay-common 会让抽取器把已策展的 133 条当作新 vendor 条目重复产出，
/// 下游 precompile-mods --check 报重复 id。
fn load_existing_keys() -> io::Result<(BTreeSet<String>, BTreeSet<String>)> {
    let mut raw_keys = BTreeSet::new();
    let mut patterns = BTreeSet::new();
    let paths = [
        repo_data_dir().join("../overlay-common/special_mods.json"),
        repo_data_dir().join("overlay/special_mods.json"),
        repo_data_dir().join("generated/special_derived.json"),
    ];
    for path in paths {
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        let doc: SpecialModsDef = serde_json::from_str(&text)
            .map_err(|error| io::Error::other(format!("{} 解析失败：{error}", path.display())))?;
        for entry in doc.entries {
            if let Some(vp) = entry.vendor_pattern {
                raw_keys.insert(vp);
            }
            raw_keys.insert(entry.pattern.clone());
            patterns.insert(entry.pattern);
        }
    }
    Ok((raw_keys, patterns))
}

// Lua pattern → Rust regex（严格白名单子集）

/// 数字捕获内容闭集 → 忠实 regex 体（不放宽：`%d+` 不接受小数）。
fn numeric_capture_body(content: &str) -> Option<&'static str> {
    match content {
        "%d+" => Some(r"\d+"),
        "%d+%.?%d*" => Some(r"\d+(?:\.\d+)?"),
        "%d*%.?%d+" => Some(r"\d*\.?\d+"),
        "[%d%.]+" => Some(r"[\d.]+"),
        // 带符号/单位数形态（V2s3）：捕获文本含 +/- 前缀，运行时
        // parse::<f64> 原生接受（"+3"→3、"-30"→-30）；探针侧闭包走
        // tonumber(cap) 同语义，线性推断对负值成立。
        "%d" => Some(r"\d"),
        "%-%d+" => Some(r"-\d+"),
        "%-?%d+" => Some(r"-?\d+"),
        "%+%d+" => Some(r"\+\d+"),
        "[%+%-]%d" => Some(r"[+-]\d"),
        "[%+%-]%d+" | "[%-%+]%d+" => Some(r"[+-]\d+"),
        "[%+%-][%d%.]+" => Some(r"[+-][\d.]+"),
        _ => None,
    }
}

fn is_regex_meta(c: char) -> bool {
    matches!(
        c,
        '.' | '^' | '$' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '\\'
    )
}

fn push_literal(out: &mut String, c: char) {
    if is_regex_meta(c) {
        out.push('\\');
    }
    out.push(c);
}

/// 词槽的 regex 形态（V2s5；键 = 捕获组序号，1-based 计**全部** `(` 组）。
enum WordAlt {
    /// enums 闭集：`(w1|w2|...)`。
    Alternation(Vec<String>),
    /// 无 enums 依赖且命中全字典（可选前缀捕获等）：按 Lua 词类原样泛化。
    Fragment,
}

/// 转换入口。`keep_captures=false` 时捕获组降级为 `(?:...)`（静态条目捕获值
/// 未被引用）。返回 `(regex, 捕获组数)`；白名单之外 → `Err(原因)`。
fn lua_pattern_to_regex(
    key: &str,
    keep_captures: bool,
    word_alts: &BTreeMap<usize, WordAlt>,
) -> Result<(String, usize), String> {
    let chars: Vec<char> = key.chars().collect();
    let n = chars.len();
    let mut out = String::with_capacity(key.len() + 8);
    let mut caps = 0usize;
    let mut ordinal = 0usize;
    let mut i = 0usize;
    // 引擎整行锚定（编译期包 ^...$），首 ^ / 尾 $ 直接吸收。
    if i < n && chars[i] == '^' {
        i += 1;
    }
    while i < n {
        let c = chars[i];
        match c {
            '%' => {
                i += 1;
                if i >= n {
                    return Err("dangling %".into());
                }
                let e = chars[i];
                match e {
                    'd' => out.push_str(r"\d"),
                    'D' => out.push_str(r"\D"),
                    'a' | 'l' => out.push_str("[a-z]"),
                    's' => out.push_str(r"\s"),
                    'w' => out.push_str("[a-z0-9]"),
                    c if !c.is_ascii_alphanumeric() => push_literal(&mut out, c),
                    other => return Err(format!("unsupported %{other}")),
                }
                i += 1;
            }
            '(' => {
                let close = chars[i + 1..]
                    .iter()
                    .position(|&c| c == ')')
                    .map(|p| i + 1 + p)
                    .ok_or("unbalanced (")?;
                let content: String = chars[i + 1..close].iter().collect();
                ordinal += 1;
                match word_alts.get(&ordinal) {
                    // 词类闭集：alternation（长词优先，避免 `evasion` 吞并
                    // `evasion rating` 前缀）。恒保留捕获（enums 按序号查表）。
                    Some(WordAlt::Alternation(words)) => {
                        let mut ws: Vec<&String> = words.iter().collect();
                        ws.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
                        let alt = ws
                            .iter()
                            .map(|w| regex::escape(w))
                            .collect::<Vec<_>>()
                            .join("|");
                        out.push('(');
                        out.push_str(&alt);
                        out.push(')');
                        caps += 1;
                    }
                    // 无 enums 依赖的词槽（可选前缀等）：按原词类泛化转换，
                    // 保留捕获以维持后续 $n 序号对齐。
                    Some(WordAlt::Fragment) => {
                        let (frag, _) = lua_pattern_to_regex(&content, false, &BTreeMap::new())?;
                        out.push('(');
                        out.push_str(&frag);
                        out.push(')');
                        caps += 1;
                    }
                    None => {
                        let body = match numeric_capture_body(&content) {
                            Some(b) => b.to_string(),
                            // 静态条目（捕获值未被 mods 引用）的词类捕获：按
                            // 原词类泛化、降级非捕获（V2s5，如 `(%D+)`）。
                            None if !keep_captures => {
                                lua_pattern_to_regex(&content, false, &BTreeMap::new())
                                    .map_err(|_| format!("capture `{content}`"))?
                                    .0
                            }
                            None => return Err(format!("capture `{content}`")),
                        };
                        if keep_captures {
                            out.push('(');
                            out.push_str(&body);
                            out.push(')');
                            caps += 1;
                        } else {
                            out.push_str("(?:");
                            out.push_str(&body);
                            out.push(')');
                        }
                    }
                }
                i = close + 1;
            }
            ')' => return Err("unbalanced )".into()),
            '[' => {
                out.push('[');
                i += 1;
                if i < n && chars[i] == '^' {
                    out.push('^');
                    i += 1;
                }
                while i < n && chars[i] != ']' {
                    match chars[i] {
                        '%' => {
                            i += 1;
                            if i >= n {
                                return Err("dangling % in class".into());
                            }
                            let e = chars[i];
                            match e {
                                'd' => out.push_str(r"\d"),
                                'D' => out.push_str(r"\D"),
                                'a' | 'l' => out.push_str("a-z"),
                                's' => out.push_str(r"\s"),
                                c if !c.is_ascii_alphanumeric() => {
                                    if matches!(c, '\\' | ']' | '^') {
                                        out.push('\\');
                                    }
                                    out.push(c);
                                }
                                other => return Err(format!("unsupported %{other} in class")),
                            }
                        }
                        '\\' => out.push_str(r"\\"),
                        c => out.push(c),
                    }
                    i += 1;
                }
                if i >= n {
                    return Err("unterminated class".into());
                }
                out.push(']');
                i += 1;
            }
            // Lua `.` 与 regex `.` 同义；`- `= 懒惰 0+ 量词 → `*?`
            '.' => {
                out.push('.');
                i += 1;
            }
            '-' => {
                out.push_str("*?");
                i += 1;
            }
            '*' | '+' | '?' => {
                out.push(c);
                i += 1;
            }
            '$' => {
                if i == n - 1 {
                    i += 1;
                } else {
                    return Err("mid-pattern $".into());
                }
            }
            '^' => return Err("mid-pattern ^".into()),
            c => {
                push_literal(&mut out, c);
                i += 1;
            }
        }
    }
    Ok((out, caps))
}

// V2s5：kind:"enum" 行 → 带 enums 闭集的模板条目

/// 词类探针行的统一化：
/// - 逐词组合的推断 mods 做结构 diff——全组合相等的叶子按字面量保留；
/// - 差异叶子必须是字符串、位于 mod name 或 LIST 值字段（tags/flags/type
///   差异整条跳过——compile_tag 是编译期静态求值），且恰由一个词槽决定 →
///   收编为 `{"enum": slot}` 引用 + `enums[slot][word]` 映射；
/// - 词槽命中全字典且被 enums 依赖 → 开放词汇（触发技能名等，闭集假设
///   不成立）整条跳过；命中全字典且无依赖 → 词类原样泛化（可选前缀捕获）；
///   命中真子集 → alternation 闭集限定匹配面。
fn build_enum_entry(
    row: &RawRow,
    variants: &[&RawEnumVariant],
    registry: &HandlerRegistry,
    existing_patterns: &BTreeSet<String>,
    seen_patterns: &mut BTreeSet<String>,
    seen_ids: &mut BTreeSet<String>,
) -> Result<SpecialTemplateDef, String> {
    if variants.is_empty() || row.word_slots.is_empty() || row.dict_size == 0 {
        return Err("enum_row_malformed".into());
    }
    let slot_count = row.word_slots.len();
    if variants.iter().any(|v| v.words.len() != slot_count) {
        return Err("enum_row_malformed".into());
    }

    // 1. 结构 diff（JSON Pointer 路径收集差异字符串叶子）。
    let values: Vec<&serde_json::Value> = variants.iter().map(|v| &v.mods).collect();
    let mut diff_paths: Vec<String> = Vec::new();
    collect_diffs("", &values, &mut diff_paths)?;

    // 2. 差异位置白名单：`/<i>/name` 或 `/<i>/value/<field>`（field ≠ mod）。
    for path in &diff_paths {
        let segs: Vec<&str> = path.split('/').collect(); // ["", i, ...]
        let ok = match segs.as_slice() {
            ["", idx, "name"] => idx.parse::<usize>().is_ok(),
            ["", idx, "value", field] => idx.parse::<usize>().is_ok() && *field != "mod",
            _ => false,
        };
        if !ok {
            return Err("enum_diff_position".into());
        }
    }

    // 3. 每个差异归属恰一个词槽（叶子 = f(该槽词)），并组装 enums 映射。
    let mut enums: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut slot_for_diff: Vec<usize> = Vec::with_capacity(diff_paths.len());
    for path in &diff_paths {
        let mut chosen = None;
        for p in 0..slot_count {
            let mut word_to_leaf: BTreeMap<&str, &str> = BTreeMap::new();
            let mut consistent = true;
            for var in variants {
                let leaf = var
                    .mods
                    .pointer(path)
                    .and_then(serde_json::Value::as_str)
                    .ok_or("enum_diff_shape")?;
                match word_to_leaf.entry(var.words[p].as_str()) {
                    std::collections::btree_map::Entry::Occupied(e) if *e.get() != leaf => {
                        consistent = false;
                        break;
                    }
                    std::collections::btree_map::Entry::Vacant(e) => {
                        e.insert(leaf);
                    }
                    _ => {}
                }
            }
            if consistent {
                chosen = Some((p, word_to_leaf));
                break;
            }
        }
        let (p, word_to_leaf) = chosen.ok_or("enum_diff_multislot")?;
        slot_for_diff.push(p);
        let map: BTreeMap<String, String> = word_to_leaf
            .into_iter()
            .map(|(w, l)| (w.to_string(), l.to_string()))
            .collect();
        let key = row.word_slots[p].to_string();
        match enums.entry(key) {
            std::collections::btree_map::Entry::Occupied(e) => {
                // 同槽被多个差异位置引用：映射必须逐字一致（resolve_enum 对
                // 同一槽只查一张表）。
                if *e.get() != map {
                    return Err("enum_slot_conflict".into());
                }
            }
            std::collections::btree_map::Entry::Vacant(e) => {
                e.insert(map);
            }
        }
    }

    // 4. 词槽 → regex 形态（开放词汇判定按 dict_size）。
    let contents = capture_contents(&row.pattern);
    let mut word_alts: BTreeMap<usize, WordAlt> = BTreeMap::new();
    for (p, &slot) in row.word_slots.iter().enumerate() {
        let hits: BTreeSet<String> = variants.iter().map(|v| v.words[p].clone()).collect();
        let relevant = enums.contains_key(&slot.to_string());
        if relevant {
            if hits.len() >= row.dict_size {
                return Err("enum_open_vocabulary".into());
            }
            word_alts.insert(slot, WordAlt::Alternation(hits.into_iter().collect()));
        } else if hits.len() >= row.dict_size {
            // 全字典命中且 mods 不依赖该词：只有**纯可选前缀捕获**（`(i?t?e?m? ?)`
            // 等，内容无通配原子）可信为"闭包真忽略此词"→ 原样泛化；词类捕获
            // （`.+`/`%a+`）全命中往往是闭包对字典外词退化（skillId 置 nil 丢键
            // 使各组合输出趋同）——按开放词汇跳过，防假阳性。
            let junk = contents
                .get(slot.saturating_sub(1))
                .is_some_and(|c| is_optional_junk(c));
            if !junk {
                return Err("enum_open_vocabulary".into());
            }
            word_alts.insert(slot, WordAlt::Fragment);
        } else {
            word_alts.insert(slot, WordAlt::Alternation(hits.into_iter().collect()));
        }
    }

    // 5. 统一模板：diff 位置替换为 {"enum": slot}。
    let mut unified = variants[0].mods.clone();
    for (path, &p) in diff_paths.iter().zip(&slot_for_diff) {
        *unified.pointer_mut(path).ok_or("enum_row_malformed")? =
            serde_json::json!({ "enum": row.word_slots[p] });
    }

    // 6. regex + 去重 + 白名单转换 + 编译验证（与常规路径同一闸门）。
    let (regex, caps) = lua_pattern_to_regex(&row.pattern, true, &word_alts)
        .map_err(|_| "enum_pattern_unconvertible".to_string())?;
    if existing_patterns.contains(&regex) {
        return Err("dedup_existing_pattern".into());
    }
    if !seen_patterns.insert(regex.clone()) {
        return Err("dedup_self".into());
    }
    let mods = transform_mods(&unified)?;
    validate_refs(&mods, caps)?;
    let id = format!("vnd_{}_{}", slug(&row.pattern), stable_hash8(&row.pattern));
    if !seen_ids.insert(id.clone()) {
        return Err("dedup_id".into());
    }
    let entry = SpecialTemplateDef {
        id,
        pattern: regex,
        vendor_pattern: Some(row.pattern.clone()),
        mods,
        handler_id: None,
        handler_args: Vec::new(),
        enums,
        verified: false,
        batch: BATCH.to_string(),
        source_note: Some("vendor specialModList 批量抽取（词类字典探针）".to_string()),
    };
    SpecialModRules::compile(std::slice::from_ref(&entry), registry)
        .map_err(|_| "enum_compile_failed".to_string())?;
    Ok(entry)
}

/// 统一化失败的降级路径：逐词组合出**单词条目**——mods 全字面量（词特定
/// 的 tag / 数值 / 形状差异不再需要 enum 落点），pattern 的词槽收窄为该
/// 组合的单词 alternation。前置闸：任一词槽命中全字典 → 开放词汇整条
/// 跳过；组合数 > 40 → 跳过（多槽全叉积防数据爆炸）。单个组合转换失败
/// 只丢该组合（匹配面收窄，保守）。
fn build_singleton_entries(
    row: &RawRow,
    variants: &[&RawEnumVariant],
    registry: &HandlerRegistry,
    existing_patterns: &BTreeSet<String>,
    seen_patterns: &mut BTreeSet<String>,
    seen_ids: &mut BTreeSet<String>,
) -> Result<Vec<SpecialTemplateDef>, String> {
    let slot_count = row.word_slots.len();
    for p in 0..slot_count {
        let hits: BTreeSet<&str> = variants.iter().map(|v| v.words[p].as_str()).collect();
        if hits.len() >= row.dict_size {
            return Err("enum_open_vocabulary".into());
        }
    }
    if variants.len() > 40 {
        return Err("enum_singleton_too_many".into());
    }
    let mut out = Vec::new();
    for var in variants {
        let word_alts: BTreeMap<usize, WordAlt> = row
            .word_slots
            .iter()
            .enumerate()
            .map(|(p, &slot)| (slot, WordAlt::Alternation(vec![var.words[p].clone()])))
            .collect();
        let Ok((regex, caps)) = lua_pattern_to_regex(&row.pattern, true, &word_alts) else {
            continue;
        };
        if existing_patterns.contains(&regex) || !seen_patterns.insert(regex.clone()) {
            continue;
        }
        let Ok(mods) = transform_mods(&var.mods) else {
            continue;
        };
        if validate_refs(&mods, caps).is_err() {
            continue;
        }
        let mut word_slug = slug(&var.words.join("_"));
        if word_slug.is_empty() {
            word_slug = "blank".to_string();
        }
        let id = format!(
            "vnd_{}_{}_{}",
            slug(&row.pattern),
            stable_hash8(&row.pattern),
            word_slug
        );
        if !seen_ids.insert(id.clone()) {
            continue;
        }
        let entry = SpecialTemplateDef {
            id,
            pattern: regex,
            vendor_pattern: Some(row.pattern.clone()),
            mods,
            handler_id: None,
            handler_args: Vec::new(),
            enums: BTreeMap::new(),
            verified: false,
            batch: BATCH.to_string(),
            source_note: Some(
                "vendor specialModList 批量抽取（词类字典探针·单词降级）".to_string(),
            ),
        };
        if SpecialModRules::compile(std::slice::from_ref(&entry), registry).is_ok() {
            out.push(entry);
        }
    }
    if out.is_empty() {
        return Err("enum_singleton_all_failed".into());
    }
    Ok(out)
}

/// 递归收集 mods JSON 里全部 `name` 字符串（含嵌套载荷）。
fn collect_mod_names(v: &serde_json::Value, out: &mut BTreeSet<String>) {
    match v {
        serde_json::Value::Object(map) => {
            if let Some(serde_json::Value::String(s)) = map.get("name") {
                out.insert(s.clone());
            }
            for val in map.values() {
                collect_mod_names(val, out);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_mod_names(item, out);
            }
        }
        _ => {}
    }
}

/// 开放槽救援：词槽命中全字典且非可选前缀捕获时，用全库已知 name 集过滤
/// 组合——拼接型闭包对噪声词造出的 name（`StrengthResist`）全库唯一，
/// 有效词的 name（`FireResist`）在其他词条反复出现。过滤后仍开放交给
/// 调用方 step-4 判定；全滤空 → 开放词汇跳过。无开放槽的行原样返回
/// （防止误伤本条目独有的合法 name）。
fn rescue_open_variants<'a>(
    row: &'a RawRow,
    known_names: &BTreeSet<String>,
) -> Result<Vec<&'a RawEnumVariant>, String> {
    let contents = capture_contents(&row.pattern);
    let has_open_word_slot = (0..row.word_slots.len()).any(|p| {
        let hits: BTreeSet<&str> = row.variants.iter().map(|v| v.words[p].as_str()).collect();
        hits.len() >= row.dict_size
            && !contents
                .get(row.word_slots[p].saturating_sub(1))
                .is_some_and(|c| is_optional_junk(c))
    });
    if !has_open_word_slot {
        return Ok(row.variants.iter().collect());
    }
    let filtered: Vec<&RawEnumVariant> = row
        .variants
        .iter()
        .filter(|v| {
            if v.mods.as_array().is_none_or(Vec::is_empty) {
                return false; // 闭包对噪声词短路返回空表
            }
            let mut names = BTreeSet::new();
            collect_mod_names(&v.mods, &mut names);
            !names.is_empty() && names.iter().all(|n| known_names.contains(n))
        })
        .collect();
    if filtered.is_empty() {
        return Err("enum_open_vocabulary".into());
    }
    Ok(filtered)
}

/// 逐捕获组提取 Lua pattern 的括号内容（1-based 序号对齐 `word_slots`）。
fn capture_contents(pattern: &str) -> Vec<String> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '%' => i += 2,
            '(' => {
                let close = chars[i + 1..]
                    .iter()
                    .position(|&c| c == ')')
                    .map(|p| i + 1 + p)
                    .unwrap_or(chars.len());
                out.push(chars[i + 1..close.min(chars.len())].iter().collect());
                i = close + 1;
            }
            _ => i += 1,
        }
    }
    out
}

/// 纯可选前缀捕获判定：内容不含通配原子（`.` / `%字母` 类 / `[]` 字符类），
/// 只由字面字符与 `?` 量词组成（如 `i?t?e?m? ?`）。
fn is_optional_junk(content: &str) -> bool {
    let chars: Vec<char> = content.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '.' | '[' | ']' | '+' | '*' | '-' => return false,
            '%' => {
                if chars.get(i + 1).is_some_and(char::is_ascii_alphanumeric) {
                    return false;
                }
                i += 2;
            }
            _ => i += 1,
        }
    }
    true
}

/// 结构 diff：全组合相等 → 跳过；对象/数组同形递归；字符串叶子差异 →
/// 记录 JSON Pointer；其余（形状不一致 / 数字布尔差异）→ Err 整条跳过。
fn collect_diffs(
    path: &str,
    variants: &[&serde_json::Value],
    out: &mut Vec<String>,
) -> Result<(), String> {
    let first = variants[0];
    if variants.iter().all(|v| *v == first) {
        return Ok(());
    }
    match first {
        serde_json::Value::Object(map) => {
            for v in variants {
                let o = v.as_object().ok_or("enum_diff_shape")?;
                if o.len() != map.len() || !map.keys().all(|k| o.contains_key(k)) {
                    return Err("enum_diff_shape".into());
                }
            }
            for k in map.keys() {
                if k.contains('/') || k.contains('~') {
                    return Err("enum_diff_shape".into()); // JSON Pointer 转义不建模
                }
                let subs: Vec<&serde_json::Value> =
                    variants.iter().map(|v| &v[k.as_str()]).collect();
                collect_diffs(&format!("{path}/{k}"), &subs, out)?;
            }
        }
        serde_json::Value::Array(arr) => {
            for v in variants {
                if v.as_array().map(Vec::len) != Some(arr.len()) {
                    return Err("enum_diff_shape".into());
                }
            }
            for idx in 0..arr.len() {
                let subs: Vec<&serde_json::Value> = variants.iter().map(|v| &v[idx]).collect();
                collect_diffs(&format!("{path}/{idx}"), &subs, out)?;
            }
        }
        serde_json::Value::String(_) => {
            for v in variants {
                if !v.is_string() {
                    return Err("enum_diff_shape".into());
                }
            }
            out.push(path.to_string());
        }
        // 数字/布尔随词变化（分数词值等）：TemplateValueDef 无 enum 数值落点。
        _ => return Err("enum_diff_scalar".into()),
    }
    Ok(())
}

// raw mod JSON → ModTemplateDef（忠实性白名单）

fn transform_mods(v: &serde_json::Value) -> Result<Vec<ModTemplateDef>, String> {
    let arr = v.as_array().ok_or("mods_not_array")?;
    arr.iter().map(transform_mod).collect()
}

/// PercentStat 捕获 percent 的等价改写：`value × stat × percent/100` 对
/// value/percent 可交换——`value=1, percent=$n` ≡ `value=$n, percent=1`
/// （vendor 主流形态 "gain X equal to (N)% of stat"，percent 来自捕获）。
/// 改写后 tag 字段全字面量，走既有白名单；不满足形态（value≠1 / 多个捕获
/// percent）→ None，条目按原路径落 `tag_field_capture` 跳过。
fn rewrite_percent_capture(
    obj: &serde_json::Map<String, serde_json::Value>,
) -> Option<serde_json::Map<String, serde_json::Value>> {
    if obj.get("value").and_then(serde_json::Value::as_f64) != Some(1.0) {
        return None;
    }
    let tags = obj.get("tags")?.as_array()?;
    let is_pure_capture = |v: &serde_json::Value| {
        v.as_str()
            .and_then(|s| s.strip_prefix('$'))
            .is_some_and(|rest| rest.parse::<u32>().is_ok())
    };
    let hits: Vec<usize> = tags
        .iter()
        .enumerate()
        .filter(|(_, t)| {
            t.get("type").and_then(serde_json::Value::as_str) == Some("PercentStat")
                && t.get("percent").is_some_and(&is_pure_capture)
        })
        .map(|(i, _)| i)
        .collect();
    let [idx] = hits[..] else { return None };
    let capture = tags[idx].get("percent").cloned().unwrap();
    let mut out = obj.clone();
    out.insert("value".into(), capture);
    let tags = out.get_mut("tags").unwrap().as_array_mut().unwrap();
    tags[idx]
        .as_object_mut()
        .unwrap()
        .insert("percent".into(), serde_json::json!(1.0));
    Some(out)
}

fn transform_mod(v: &serde_json::Value) -> Result<ModTemplateDef, String> {
    let obj = v.as_object().ok_or("mod_not_object")?;
    let rewritten = rewrite_percent_capture(obj);
    let obj = rewritten.as_ref().unwrap_or(obj);
    for key in obj.keys() {
        if !matches!(
            key.as_str(),
            "name" | "type" | "value" | "flags" | "keywordFlags" | "tags"
        ) {
            return Err("mod_unknown_key".into());
        }
    }
    let name = match obj.get("name") {
        Some(serde_json::Value::String(s)) => {
            if s.contains('$') || s.contains('+') {
                return Err("mod_name_nonliteral".into());
            }
            TemplateNameDef::Literal(s.clone())
        }
        // enums 闭集引用（V2s5 统一化产物 `{"enum": n}`）。
        Some(v) => match enum_ref(v) {
            Some(idx) => TemplateNameDef::Enum { capture_index: idx },
            None => return Err("mod_name_missing".into()),
        },
        None => return Err("mod_name_missing".into()),
    };
    let mod_type = obj
        .get("type")
        .and_then(serde_json::Value::as_str)
        .ok_or("mod_type_missing")?;
    if !matches!(
        mod_type,
        "BASE" | "INC" | "MORE" | "FLAG" | "OVERRIDE" | "LIST"
    ) {
        return Err("mod_type_unknown".into());
    }
    let value = transform_value(obj.get("value").ok_or("mod_value_missing")?)?;
    let flags = transform_flag_names(obj.get("flags"), flag_name_is_mappable, "flag_unmappable")?;
    let keyword_flags = transform_flag_names(
        obj.get("keywordFlags"),
        keyword_flag_name_is_mappable,
        "keyword_flag_unmappable",
    )?;
    let tags = match obj.get("tags") {
        None => Vec::new(),
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .map(transform_tag)
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => return Err("tags_not_array".into()),
    };
    // 嵌套载荷 + 缩放 tag：vendor 会缩放内层 value.mod.value（ModStore.lua
    // table 分支），但 pobr 的 list_nested 转发只透传不求值——放行会静默丢缩放。
    if matches!(value, TemplateValueDef::Nested { .. })
        && tags.iter().any(|t| {
            matches!(
                t.tag_type.as_str(),
                "Multiplier" | "PerStat" | "PercentStat"
            )
        })
    {
        return Err("nested_scaling_tag".into());
    }
    Ok(ModTemplateDef {
        name,
        mod_type: mod_type.to_string(),
        value,
        flags,
        keyword_flags,
        tags,
        target: None,
    })
}

fn transform_flag_names(
    v: Option<&serde_json::Value>,
    mappable: fn(&str) -> bool,
    err: &str,
) -> Result<Vec<String>, String> {
    let Some(v) = v else {
        return Ok(Vec::new());
    };
    let arr = v.as_array().ok_or("flags_not_array")?;
    let mut names = Vec::with_capacity(arr.len());
    for item in arr {
        let name = item.as_str().ok_or("flags_not_string")?;
        if !mappable(name) {
            return Err(err.into());
        }
        names.push(name.to_string());
    }
    Ok(names)
}

/// value 模板迷你语法：`$n` / `$n:negate` / `$n:base(c)` / `$n:mult(k)` / `$n:div(k)`。
fn parse_capture_template(s: &str) -> Option<TemplateValueDef> {
    let rest = s.strip_prefix('$')?;
    let (idx_str, op_str) = match rest.split_once(':') {
        None => (rest, None),
        Some((idx, op)) => (idx, Some(op)),
    };
    idx_str.parse::<u32>().ok()?;
    let capture = format!("${idx_str}");
    let Some(op_str) = op_str else {
        return Some(TemplateValueDef::Capture(capture));
    };
    let op = if op_str == "negate" {
        ValueOpDef::Negate {}
    } else {
        let (op_name, arg) = op_str.split_once('(')?;
        let arg: f64 = arg.strip_suffix(')')?.parse().ok()?;
        match op_name {
            "base" => ValueOpDef::Base(arg),
            "mult" => ValueOpDef::Mult(arg),
            "div" => ValueOpDef::Div(arg),
            _ => return None,
        }
    };
    Some(TemplateValueDef::Expr(ValueExprDef {
        capture,
        ops: vec![op],
    }))
}

fn transform_value(v: &serde_json::Value) -> Result<TemplateValueDef, String> {
    match v {
        serde_json::Value::Number(n) => Ok(TemplateValueDef::Number(
            n.as_f64().ok_or("value_nonfinite")?,
        )),
        serde_json::Value::Bool(b) => Ok(TemplateValueDef::Flag(*b)),
        serde_json::Value::String(s) => {
            // 非 `$` 开头的字面字符串没有对应的 TemplateValueDef 形态
            //（untagged serde 会误读成 Capture），保守跳过。
            parse_capture_template(s).ok_or("value_form".into())
        }
        serde_json::Value::Object(map) => {
            // 嵌套 mod 载荷：纯 `{ "mod": <mod|[mod...]> }` 形态 → Nested
            //（运行时 ModValue::NestedMods，编排层转发）。混合形态
            //（mod + 其他标量键，如 ExtraAura 的 onlyAllies）运行时无法
            // 表达 → 整条跳过。
            if map.contains_key("mod") {
                if map.len() != 1 {
                    return Err("value_mixed_nested".into());
                }
                let mods = match &map["mod"] {
                    inner @ serde_json::Value::Object(_) => vec![transform_mod(inner)?],
                    serde_json::Value::Array(items) => items
                        .iter()
                        .map(transform_mod)
                        .collect::<Result<Vec<_>, _>>()?,
                    _ => return Err("value_form".into()),
                };
                return Ok(TemplateValueDef::Nested { mods });
            }
            let mut fields = BTreeMap::new();
            for (k, val) in map {
                fields.insert(k.clone(), transform_scalar(val)?);
            }
            Ok(TemplateValueDef::List(fields))
        }
        _ => Err("value_form".into()),
    }
}

/// LIST 值 / tag 字段里的标量：数字 / 布尔 / 字面字符串或裸 `$n` 引用。
/// 带算子链的 `$n:...`、`$n:cap`、`+` 拼接段、嵌套表（如 `{ mod = ... }`
/// 嵌套 mod）都不是标量 → 整条跳过。
fn transform_scalar(v: &serde_json::Value) -> Result<TemplateScalarDef, String> {
    match v {
        serde_json::Value::Number(n) => Ok(TemplateScalarDef::Number(
            n.as_f64().ok_or("value_nonfinite")?,
        )),
        serde_json::Value::Bool(b) => Ok(TemplateScalarDef::Bool(*b)),
        serde_json::Value::String(s) => {
            if s.contains('+') || s.contains(':') {
                return Err("scalar_template_form".into());
            }
            if s.starts_with('$') && s[1..].parse::<u32>().is_err() {
                return Err("scalar_template_form".into());
            }
            Ok(TemplateScalarDef::Text(s.clone()))
        }
        // enums 闭集引用（V2s5 统一化产物 `{"enum": n}`）——其余对象形态仍拒。
        serde_json::Value::Object(_) => match enum_ref(v) {
            Some(idx) => Ok(TemplateScalarDef::Enum { capture_index: idx }),
            None => Err("value_nested".into()),
        },
        serde_json::Value::Array(_) => Err("value_nested".into()),
        serde_json::Value::Null => Err("value_form".into()),
    }
}

/// `{"enum": n}` 引用识别（单键对象 + 非负整数）。
fn enum_ref(v: &serde_json::Value) -> Option<u32> {
    let map = v.as_object()?;
    if map.len() != 1 {
        return None;
    }
    map.get("enum")?.as_u64().map(|n| n as u32)
}

/// tag 形态白名单：与 `pobr-core::rules::special_mod::compile_tag` 的**忠实映射**
/// 字段集对齐（字段超集会被编译静默忽略造成语义漂移，如 Multiplier 的 `actor`）。
fn transform_tag(v: &serde_json::Value) -> Result<TemplateTagDef, String> {
    let obj = v.as_object().ok_or("tag_not_object")?;
    let tag_type = obj
        .get("type")
        .and_then(serde_json::Value::as_str)
        .ok_or("tag_type_missing")?;
    let allowed: &[&str] = match tag_type {
        "Condition" => &["var", "neg"],
        "ActorCondition" => &["var", "neg", "actor"],
        "SkillType" => &["skillType"],
        "DamageType" => &["damageType"],
        "Multiplier" => &["var", "div", "limit"],
        "PerStat" => &["stat", "div", "limit"],
        // statList/percentVar/actor/base/limit/floor 形态无落点，字段超集整条跳过。
        "PercentStat" => &["stat", "percent"],
        "MultiplierThreshold" => &["var", "threshold", "upper"],
        // statList/thresholdStat/thresholdPercent(Var)/actor 形态无落点，整条跳过。
        "StatThreshold" => &["stat", "threshold", "upper"],
        // includeTransfigured 编译侧忽略（PoE2 无变体宝石，gem name→gameId
        // 等值退化为名字等值）；partialMatch/summonSkill/neg 零出现，不放行。
        "SkillName" => &["skillName", "skillNameList", "includeTransfigured"],
        _ => return Err("tag_type_unmappable".into()),
    };
    let mut fields = BTreeMap::new();
    for (k, val) in obj {
        if k == "type" {
            continue;
        }
        if !allowed.contains(&k.as_str()) {
            return Err("tag_field_shape".into());
        }
        // skillNameList 是唯一允许的数组字段（字符串字面量列表 → TextList）。
        if k == "skillNameList" {
            let items = val.as_array().ok_or("tag_field_shape")?;
            let names = items
                .iter()
                .map(|v| match v.as_str() {
                    Some(s) if !s.contains('$') => Ok(s.to_string()),
                    _ => Err("tag_field_shape".to_string()),
                })
                .collect::<Result<Vec<_>, _>>()?;
            if names.is_empty() {
                return Err("tag_field_shape".into());
            }
            fields.insert(k.clone(), TemplateScalarDef::TextList(names));
            continue;
        }
        let scalar = transform_scalar(val).map_err(|_| "tag_field_shape".to_string())?;
        // tag 字段禁捕获引用（compile_tag 会把 `$n` var 当字面量或静默丢 tag）
        if let TemplateScalarDef::Text(s) = &scalar
            && s.contains('$')
        {
            return Err("tag_field_capture".into());
        }
        fields.insert(k.clone(), scalar);
    }
    let tag = TemplateTagDef {
        tag_type: tag_type.to_string(),
        fields,
    };
    if !tag_is_mappable(&tag) {
        return Err("tag_unmappable".into());
    }
    Ok(tag)
}

// 捕获引用越界校验

fn validate_refs(mods: &[ModTemplateDef], caps: usize) -> Result<(), String> {
    let check = |s: &str| -> Result<(), String> {
        if let Some(rest) = s.strip_prefix('$')
            && let Some(idx_str) = rest.split(':').next()
            && let Ok(idx) = idx_str.parse::<usize>()
            && (idx == 0 || idx > caps)
        {
            return Err("capture_ref_out_of_range".into());
        }
        Ok(())
    };
    for m in mods {
        match &m.value {
            TemplateValueDef::Capture(s) => check(s)?,
            TemplateValueDef::Expr(e) => check(&e.capture)?,
            TemplateValueDef::List(map) => {
                for scalar in map.values() {
                    if let TemplateScalarDef::Text(s) = scalar {
                        check(s)?;
                    }
                }
            }
            TemplateValueDef::Nested { mods } => validate_refs(mods, caps)?,
            TemplateValueDef::Flag(_) | TemplateValueDef::Number(_) => {}
        }
    }
    Ok(())
}

// id / meta

fn slug(s: &str) -> String {
    let mut out = String::new();
    let mut last_us = true;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            last_us = false;
        } else if !last_us {
            out.push('_');
            last_us = true;
        }
        if out.len() >= 40 {
            break;
        }
    }
    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        "pattern".to_string()
    } else {
        trimmed.to_string()
    }
}

/// FNV-1a 64 截 8 hex（vendor key 稳定指纹，保 id 唯一）。
fn stable_hash8(s: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in s.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{:08x}", (hash >> 32) as u32 ^ hash as u32)
}

fn build_meta(args: &ExtractLuaArgs) -> io::Result<OverlayMeta> {
    let (commit, subject) = read_vendor_version(&resolve_version_file(args))?;
    let mut regen = String::from(
        "cargo run -p sync-pob-catalog -- extract-lua --what special-mods --vendor-root vendor/PathOfBuilding-PoE2/src",
    );
    if let Some(out) = &args.out_for_meta {
        regen.push_str(&format!(" --out {out}"));
    }
    Ok(OverlayMeta {
        schema: SPECIAL_MODS_SCHEMA.to_string(),
        generator: "sync-pob-catalog extract-lua".to_string(),
        vendor: "PathOfBuilding-PoE2".to_string(),
        vendor_commit: commit,
        vendor_commit_subject: subject,
        extracted_files: vec!["Modules/ModParser.lua".to_string()],
        regen_command: regen,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_captures_convert_faithfully() {
        let (re, caps) = lua_pattern_to_regex("gain (%d+) rage", true, &BTreeMap::new()).unwrap();
        assert_eq!(re, r"gain (\d+) rage");
        assert_eq!(caps, 1);
        let (re, caps) =
            lua_pattern_to_regex("^(%d+%.?%d*)%% of damage", true, &BTreeMap::new()).unwrap();
        assert_eq!(re, r"(\d+(?:\.\d+)?)% of damage");
        assert_eq!(caps, 1);
    }

    #[test]
    fn static_mode_discards_captures() {
        let (re, caps) =
            lua_pattern_to_regex("has (%d+) sockets?", false, &BTreeMap::new()).unwrap();
        assert_eq!(re, r"has (?:\d+) sockets?");
        assert_eq!(caps, 0);
    }

    #[test]
    fn lua_escapes_and_quantifiers() {
        let (re, _) =
            lua_pattern_to_regex("50%% increased effect", true, &BTreeMap::new()).unwrap();
        assert_eq!(re, "50% increased effect");
        let (re, _) = lua_pattern_to_regex("armou?r", true, &BTreeMap::new()).unwrap();
        assert_eq!(re, "armou?r");
        // `%-` 转义 = 字面连字符；裸 `-` = Lua 懒惰量词 → regex `*?`
        let (re, _) = lua_pattern_to_regex("off%-hand", true, &BTreeMap::new()).unwrap();
        assert_eq!(re, "off-hand");
        let (re, _) = lua_pattern_to_regex("a-b", true, &BTreeMap::new()).unwrap();
        assert_eq!(re, "a*?b");
    }

    #[test]
    fn open_and_word_captures_rejected() {
        assert!(lua_pattern_to_regex("deal (.-) damage", true, &BTreeMap::new()).is_err());
        assert!(lua_pattern_to_regex("(%a+) skills", true, &BTreeMap::new()).is_err());
        // 静态行（keep_captures=false）的词类捕获按泛化非捕获组放行（V2s5）。
        let (re, caps) = lua_pattern_to_regex("(%D+) skills", false, &BTreeMap::new()).unwrap();
        assert_eq!(re, r"(?:\D+) skills");
        assert_eq!(caps, 0);
    }

    #[test]
    fn word_alt_slots_convert_to_alternation_and_fragment() {
        // 词槽 alternation：长词优先（evasion rating 不被 evasion 前缀吞并）。
        let mut alts = BTreeMap::new();
        alts.insert(
            2,
            WordAlt::Alternation(vec!["evasion".into(), "evasion rating".into()]),
        );
        let (re, caps) = lua_pattern_to_regex("gain (%d+) (%a+) per level", true, &alts).unwrap();
        assert_eq!(re, r"gain (\d+) (evasion rating|evasion) per level");
        assert_eq!(caps, 2);

        // Fragment：可选前缀捕获按原词类泛化，保留捕获维持序号对齐。
        let mut alts = BTreeMap::new();
        alts.insert(1, WordAlt::Fragment);
        let (re, caps) = lua_pattern_to_regex("(i?t?e?m? ?)armour applies", true, &alts).unwrap();
        assert_eq!(re, "(i?t?e?m? ?)armour applies");
        assert_eq!(caps, 1);
    }

    #[test]
    fn enum_diff_collects_string_leaves_only() {
        let a = serde_json::json!([{"name": "FireResist", "type": "BASE", "value": "$1"}]);
        let b = serde_json::json!([{"name": "ColdResist", "type": "BASE", "value": "$1"}]);
        let mut out = Vec::new();
        collect_diffs("", &[&a, &b], &mut out).unwrap();
        assert_eq!(out, vec!["/0/name"]);

        // 数字叶子差异（分数词值）→ Err 整条跳过（单词降级兜接）。
        let a = serde_json::json!([{"name": "X", "value": 25.0}]);
        let b = serde_json::json!([{"name": "X", "value": 10.0}]);
        let mut out = Vec::new();
        assert!(collect_diffs("", &[&a, &b], &mut out).is_err());

        // 形状差异（键缺失，闭包对噪声词丢 skillId 键）→ Err。
        let a = serde_json::json!([{"name": "X", "skillId": "A"}]);
        let b = serde_json::json!([{"name": "X"}]);
        let mut out = Vec::new();
        assert!(collect_diffs("", &[&a, &b], &mut out).is_err());
    }

    #[test]
    fn optional_junk_detection() {
        assert!(is_optional_junk("i?t?e?m? ?"));
        assert!(is_optional_junk("a?l?s?o? ?"));
        assert!(!is_optional_junk(".+"));
        assert!(!is_optional_junk("%a+"));
        assert!(!is_optional_junk("[lr][ei][fg][th]t?"));
    }

    #[test]
    fn value_template_parses_ops() {
        assert_eq!(
            parse_capture_template("$1"),
            Some(TemplateValueDef::Capture("$1".into()))
        );
        let TemplateValueDef::Expr(e) = parse_capture_template("$2:div(2)").unwrap() else {
            panic!("expected expr");
        };
        assert_eq!(e.capture, "$2");
        assert_eq!(e.ops, vec![ValueOpDef::Div(2.0)]);
        assert!(parse_capture_template("$1:cap").is_none());
        assert!(parse_capture_template("plain text").is_none());
    }

    #[test]
    fn tag_whitelist_rejects_extra_fields() {
        // Multiplier 带 actor 字段：compile_tag 会静默忽略 actor → 语义漂移，必须拒
        let tag = serde_json::json!({
            "type": "Multiplier", "var": "PowerCharge", "actor": "enemy"
        });
        assert!(transform_tag(&tag).is_err());
        let ok = serde_json::json!({ "type": "Condition", "var": "LowLife" });
        assert_eq!(transform_tag(&ok).unwrap().tag_type, "Condition");
    }

    #[test]
    fn skill_name_tag_transforms() {
        // 单名 + includeTransfigured（编译侧忽略）通过。
        let single = serde_json::json!({
            "type": "SkillName", "skillName": "Fireball", "includeTransfigured": true
        });
        assert_eq!(transform_tag(&single).unwrap().tag_type, "SkillName");

        // skillNameList 数组 → TextList。
        let list = serde_json::json!({
            "type": "SkillName", "skillNameList": ["Flicker Strike", "Viper Strike"]
        });
        let tag = transform_tag(&list).unwrap();
        assert_eq!(
            tag.fields.get("skillNameList"),
            Some(&TemplateScalarDef::TextList(vec![
                "Flicker Strike".into(),
                "Viper Strike".into()
            ]))
        );

        // 白名单外字段（partialMatch）/ 空列表 / 含捕获名 → 拒。
        let partial = serde_json::json!({
            "type": "SkillName", "skillName": "Fireball", "partialMatch": true
        });
        assert!(transform_tag(&partial).is_err());
        let empty = serde_json::json!({ "type": "SkillName", "skillNameList": [] });
        assert!(transform_tag(&empty).is_err());
        let cap = serde_json::json!({ "type": "SkillName", "skillNameList": ["$1"] });
        assert!(transform_tag(&cap).is_err());
    }

    #[test]
    fn pure_nested_mod_value_transforms() {
        let raw = serde_json::json!([{
            "name": "EnemyModifier", "type": "LIST",
            "value": { "mod": { "name": "FireExposure", "type": "BASE", "value": "$1:negate" } }
        }]);
        let mods = transform_mods(&raw).unwrap();
        let TemplateValueDef::Nested { mods: inner } = &mods[0].value else {
            panic!("expected nested value");
        };
        assert_eq!(inner.len(), 1);
        assert!(matches!(
            &inner[0].name,
            TemplateNameDef::Literal(n) if n == "FireExposure"
        ));
    }

    #[test]
    fn mixed_nested_mod_value_rejected() {
        // ExtraAura 的 { mod = ..., onlyAllies = true } 混合形态运行时无法表达
        let raw = serde_json::json!([{
            "name": "ExtraAura", "type": "LIST",
            "value": {
                "mod": { "name": "Speed", "type": "INC", "value": 10 },
                "onlyAllies": true
            }
        }]);
        assert_eq!(transform_mods(&raw), Err("value_mixed_nested".to_string()));
    }

    #[test]
    fn nested_mod_with_unmappable_tag_rejected() {
        // 内层 mod 的 tag 白名单同样生效（丢 tag = 条件词条变常驻）
        let raw = serde_json::json!([{
            "name": "MinionModifier", "type": "LIST",
            "value": { "mod": {
                "name": "Damage", "type": "INC", "value": "$1",
                "tags": [{ "type": "GlobalEffect", "effectType": "Buff" }]
            } }
        }]);
        assert!(transform_mods(&raw).is_err());
    }
}
