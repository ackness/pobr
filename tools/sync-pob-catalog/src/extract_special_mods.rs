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
        // 静态条目的捕获值未被 mods 引用 → 降级非捕获组；inferred 保留捕获。
        let keep_captures = row.kind == "inferred";
        let (regex, caps) = match lua_pattern_to_regex(&row.pattern, keep_captures) {
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

// ---- headless 调用（同 extract_parser_rules 约定：stdin 注入、JSONL 回收）----

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

// ---- 去重输入：已有 overlay / derived 覆盖的 vendor key 与 pattern ----

fn repo_data_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data")
        .join(pobr_data::data_version())
}

/// 返回（raw key 集 = vendor_pattern ∪ pattern；regex pattern 集）。缺文件容忍
/// （version-bump 演练时可能只有部分文件）。
fn load_existing_keys() -> io::Result<(BTreeSet<String>, BTreeSet<String>)> {
    let mut raw_keys = BTreeSet::new();
    let mut patterns = BTreeSet::new();
    for rel in [
        "overlay/special_mods.json",
        "generated/special_derived.json",
    ] {
        let path = repo_data_dir().join(rel);
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        let doc: SpecialModsDef = serde_json::from_str(&text)
            .map_err(|error| io::Error::other(format!("{rel} 解析失败：{error}")))?;
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

// ---- Lua pattern → Rust regex（严格白名单子集）----

/// 数字捕获内容闭集 → 忠实 regex 体（不放宽：`%d+` 不接受小数）。
fn numeric_capture_body(content: &str) -> Option<&'static str> {
    match content {
        "%d+" => Some(r"\d+"),
        "%d+%.?%d*" => Some(r"\d+(?:\.\d+)?"),
        "%d*%.?%d+" => Some(r"\d*\.?\d+"),
        "[%d%.]+" => Some(r"[\d.]+"),
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

/// 转换入口。`keep_captures=false` 时捕获组降级为 `(?:...)`（静态条目捕获值
/// 未被引用）。返回 `(regex, 捕获组数)`；白名单之外 → `Err(原因)`。
fn lua_pattern_to_regex(key: &str, keep_captures: bool) -> Result<(String, usize), String> {
    let chars: Vec<char> = key.chars().collect();
    let n = chars.len();
    let mut out = String::with_capacity(key.len() + 8);
    let mut caps = 0usize;
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
                let body = numeric_capture_body(&content).ok_or(format!("capture `{content}`"))?;
                if keep_captures {
                    out.push('(');
                    out.push_str(body);
                    out.push(')');
                    caps += 1;
                } else {
                    out.push_str("(?:");
                    out.push_str(body);
                    out.push(')');
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
                                'a' | 'l' => out.push_str("a-z"),
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

// ---- raw mod JSON → ModTemplateDef（忠实性白名单）----

fn transform_mods(v: &serde_json::Value) -> Result<Vec<ModTemplateDef>, String> {
    let arr = v.as_array().ok_or("mods_not_array")?;
    arr.iter().map(transform_mod).collect()
}

fn transform_mod(v: &serde_json::Value) -> Result<ModTemplateDef, String> {
    let obj = v.as_object().ok_or("mod_not_object")?;
    for key in obj.keys() {
        if !matches!(
            key.as_str(),
            "name" | "type" | "value" | "flags" | "keywordFlags" | "tags"
        ) {
            return Err("mod_unknown_key".into());
        }
    }
    let name = obj
        .get("name")
        .and_then(serde_json::Value::as_str)
        .ok_or("mod_name_missing")?;
    if name.contains('$') || name.contains('+') {
        return Err("mod_name_nonliteral".into());
    }
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
    Ok(ModTemplateDef {
        name: TemplateNameDef::Literal(name.to_string()),
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
        serde_json::Value::Object(_) | serde_json::Value::Array(_) => Err("value_nested".into()),
        serde_json::Value::Null => Err("value_form".into()),
    }
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

// ---- 捕获引用越界校验 ----

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

// ---- id / meta ----

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
        let (re, caps) = lua_pattern_to_regex("gain (%d+) rage", true).unwrap();
        assert_eq!(re, r"gain (\d+) rage");
        assert_eq!(caps, 1);
        let (re, caps) = lua_pattern_to_regex("^(%d+%.?%d*)%% of damage", true).unwrap();
        assert_eq!(re, r"(\d+(?:\.\d+)?)% of damage");
        assert_eq!(caps, 1);
    }

    #[test]
    fn static_mode_discards_captures() {
        let (re, caps) = lua_pattern_to_regex("has (%d+) sockets?", false).unwrap();
        assert_eq!(re, r"has (?:\d+) sockets?");
        assert_eq!(caps, 0);
    }

    #[test]
    fn lua_escapes_and_quantifiers() {
        let (re, _) = lua_pattern_to_regex("50%% increased effect", true).unwrap();
        assert_eq!(re, "50% increased effect");
        let (re, _) = lua_pattern_to_regex("armou?r", true).unwrap();
        assert_eq!(re, "armou?r");
        // `%-` 转义 = 字面连字符；裸 `-` = Lua 懒惰量词 → regex `*?`
        let (re, _) = lua_pattern_to_regex("off%-hand", true).unwrap();
        assert_eq!(re, "off-hand");
        let (re, _) = lua_pattern_to_regex("a-b", true).unwrap();
        assert_eq!(re, "a*?b");
    }

    #[test]
    fn open_and_word_captures_rejected() {
        assert!(lua_pattern_to_regex("deal (.-) damage", true).is_err());
        assert!(lua_pattern_to_regex("(%a+) skills", true).is_err());
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
