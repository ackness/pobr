//! `extract-lua --what special-mods` — bulk extraction of vendor's
//! `specialModList` (batch V0).
//!
//! The Lua side (`extract_special_mods.lua`) does a headless bootstrap plus
//! dual-sentinel probing and dumps JSONL; this module handles:
//!
//! 1. **Lua pattern -> Rust regex**: converts a strict whitelisted subset
//!    (a closed set of numeric captures, `%` escapes, known character
//!    classes, `?+*-` quantifiers); anything that can't convert is skipped
//!    whole and counted — better to miss an entry than get it wrong;
//! 2. **A faithfulness whitelist**: tag shapes / flag names / value
//!    templates are each pre-checked against
//!    `pobr-core::rules::{tag_is_mappable, flag_name_is_mappable, ...}`.
//!    The compiler silently drops unmappable tags (a conservative gate), but
//!    for bulk-extracted entries dropping a tag would turn a conditional mod
//!    into an always-on one — so this **skips the whole entry** instead of dropping the tag;
//! 3. **Deduplication**: a vendor key already covered by
//!    `overlay/special_mods.json` (hand-curated, takes priority) or
//!    `generated/special_derived.json` (keystone-derived) is skipped;
//!    within-batch regex string collisions (e.g. `targets?` variants
//!    converging) skip whichever arrives later;
//! 4. **Compile validation**: every entry, individually and as a whole,
//!    goes through [`SpecialModRules::compile`], guaranteeing the
//!    `generated/special_vendor.json` artifact can always be loaded
//!    fail-fast by consumers;
//! 5. A skip-reason count report to stderr (input for V1's scope expansion:
//!    word-class capture -> enums, enemy wrapping, etc.).
//!
//! Output entries: `batch:"V0"`, `verified:false`, id shaped like `vnd_<slug>_<hash8>`.

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

/// Bootstrap script content (piped into luajit via stdin; self-contained binary).
const BOOTSTRAP_LUA: &str = include_str!("extract_special_mods.lua");

const SPECIAL_MODS_SCHEMA: &str = "special_mods/v1";
const BATCH: &str = "V0";

/// The output document (`SpecialModsDef` only derives `Deserialize`, so the write side keeps its own structure).
#[derive(Serialize)]
struct SpecialVendorDoc {
    #[serde(rename = "_meta")]
    meta: OverlayMeta,
    entries: Vec<SpecialTemplateDef>,
}

/// A JSONL row from the Lua side.
#[derive(Deserialize)]
struct RawRow {
    pattern: String,
    kind: String,
    #[serde(default)]
    mods: serde_json::Value,
    #[serde(default)]
    reason: Option<String>,
    /// `kind:"enum"` rows: capture indices (1-based) of the word slots.
    #[serde(default)]
    word_slots: Vec<usize>,
    /// `kind:"enum"` rows: the probe dictionary size (used for the
    /// open-vocabulary check: if the hit count equals the full dictionary
    /// and mods depend on that word, the closed-set assumption fails and the whole entry is skipped).
    #[serde(default)]
    dict_size: usize,
    /// `kind:"enum"` rows: the inferred result for each word combination.
    #[serde(default)]
    variants: Vec<RawEnumVariant>,
}

/// One hit combination from the word-class probe (words[i] corresponds to the word_slots[i] slot).
#[derive(Deserialize)]
struct RawEnumVariant {
    words: Vec<String>,
    mods: serde_json::Value,
}

/// Run the extraction, returning the final JSON text.
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

    // V2s5 pre-scan: the whole-database known mod-name set (from
    // static/numeric-inferred rows, not word-class-probe output). Open
    // slots in the word-class probe use this to filter for valid words in
    // string-concatenation closures -- `FireResist` recurs across other
    // mods (valid), while `StrengthResist` is unique across the database (dictionary noise).
    // The vendor data proves this itself; no semantic heuristic involved.
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
            // Bucket probe failures into broad categories for counting (the detailed reason is already logged via Lua stderr)
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
        // V2s5: rows from the word-class-capture dictionary probe -> unified
        // into an enums closed set; when unification fails due to a
        // structural difference, fall back to per-word singleton entries
        // (word-specific values become plain literals, resolving tag differences naturally).
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
        // Static entries whose captured value isn't referenced by mods -> downgrade to a non-capturing group; inferred entries keep captures.
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
        // Per-entry compile validation: pattern regex validity / mod_type / enums references.
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
    // Recompile everything once more: a final backstop for within-batch id/pattern uniqueness (same function as the consumption-side gate).
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

// Headless invocation (same convention as extract_parser_rules: stdin injection, JSONL collection)

fn invoke_headless_jsonl(args: &ExtractLuaArgs) -> io::Result<Vec<RawRow>> {
    // Canonicalize: cwd switches to vendor src/, and a relative vendor_root would break LUA_PATH.
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

// Deduplication input: vendor keys and patterns already covered by an existing overlay / derived table

fn repo_data_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data")
        .join(pobr_data::data_version())
}

/// Returns (raw key set = vendor_pattern ∪ pattern; regex pattern set).
/// Tolerates missing files (during a version-bump drill only some files may exist yet).
///
/// The dedup source spans **three** special_mods locations: the
/// version-independent curation layer `data/overlay-common/special_mods.json`
/// (P1-3), the version layer `overlay/special_mods.json`, and the derived
/// table `generated/special_derived.json`. Missing overlay-common would make
/// the extractor re-emit the 133 already-curated entries as if they were new
/// vendor entries, and downstream `precompile-mods --check` would report duplicate ids.
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

// Lua pattern -> Rust regex (a strict whitelisted subset)

/// A closed set of numeric capture bodies -> their faithful regex bodies (no loosening: `%d+` doesn't accept decimals).
fn numeric_capture_body(content: &str) -> Option<&'static str> {
    match content {
        "%d+" => Some(r"\d+"),
        "%d+%.?%d*" => Some(r"\d+(?:\.\d+)?"),
        "%d*%.?%d+" => Some(r"\d*\.?\d+"),
        "[%d%.]+" => Some(r"[\d.]+"),
        // Signed / single-digit forms (V2s3): the captured text carries a
        // +/- prefix, which runtime's parse::<f64> accepts natively
        // ("+3" -> 3, "-30" -> -30); the probe-side closure uses tonumber(cap)
        // with the same semantics, so linear inference holds for negative values too.
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

/// The regex shape of a word slot (V2s5; keys are capture-group indices,
/// 1-based counting **every** `(` group).
enum WordAlt {
    /// An enums closed set: `(w1|w2|...)`.
    Alternation(Vec<String>),
    /// No enums dependency and hits the whole dictionary (e.g. an optional prefix capture): generalized as-is from its Lua word class.
    Fragment,
}

/// The conversion entry point. When `keep_captures=false`, capture groups
/// downgrade to `(?:...)` (a static entry whose captured value isn't
/// referenced). Returns `(regex, capture group count)`; anything outside the whitelist -> `Err(reason)`.
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
    // The engine anchors the whole line (wraps it in ^...$ at compile time), so a leading ^ / trailing $ is simply absorbed.
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
                    // A closed word class: alternation (longer words first,
                    // so `evasion` doesn't swallow the `evasion rating`
                    // prefix). Always keeps the capture (enums are looked up by index).
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
                    // A word slot with no enums dependency (e.g. an optional
                    // prefix): converted generically from its word class,
                    // keeping the capture so later $n indices stay aligned.
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
                            // A word-class capture on a static entry (its
                            // captured value isn't referenced by mods):
                            // generalize by word class, downgraded to non-capturing (V2s5, e.g. `(%D+)`).
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
            // Lua `.` means the same as regex `.`; `-` is a lazy 0+ quantifier -> `*?`
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

// V2s5: kind:"enum" rows -> template entries carrying an enums closed set

/// Unification for word-class probe rows:
/// - The inferred mods for each word combination get a structural diff —
///   leaves equal across all combinations are kept as literals;
/// - A differing leaf must be a string, located at the mod name or a LIST
///   value field (tags/flags/type differences skip the whole entry —
///   compile_tag is evaluated statically at compile time), and be
///   determined by exactly one word slot -> it's converted into a
///   `{"enum": slot}` reference plus an `enums[slot][word]` mapping;
/// - A word slot that hits the whole dictionary and is depended on by enums
///   -> open vocabulary (a triggered skill name, etc.; the closed-set
///   assumption fails), skips the whole entry; hits the whole dictionary
///   with no dependency -> generalized as-is from its word class (an
///   optional prefix capture); hits a genuine subset -> constrained to an alternation closed set.
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

    // 1. Structural diff (collect differing string leaves by their JSON Pointer paths).
    let values: Vec<&serde_json::Value> = variants.iter().map(|v| &v.mods).collect();
    let mut diff_paths: Vec<String> = Vec::new();
    collect_diffs("", &values, &mut diff_paths)?;

    // 2. Whitelist of diff positions: `/<i>/name` or `/<i>/value/<field>` (field != mod).
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

    // 3. Each diff must belong to exactly one word slot (leaf = f(that slot's word)); assemble the enums mapping.
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
                // The same slot is referenced by multiple diff positions:
                // its mapping must match byte-for-byte (resolve_enum only looks up one table per slot).
                if *e.get() != map {
                    return Err("enum_slot_conflict".into());
                }
            }
            std::collections::btree_map::Entry::Vacant(e) => {
                e.insert(map);
            }
        }
    }

    // 4. Word slot -> regex shape (open-vocabulary is judged by dict_size).
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
            // Full-dictionary hit with mods not depending on this word: only
            // a **pure optional-prefix capture** (`(i?t?e?m? ?)` and the
            // like, with no wildcard atoms) can be trusted as "the closure
            // truly ignores this word" -> generalize as-is; a word-class
            // capture (`.+`/`%a+`) hitting everything is usually the closure
            // degenerating for out-of-dictionary words (setting skillId to
            // nil drops a key so combinations converge) — skip as open vocabulary to avoid false positives.
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

    // 5. Unified template: replace diff positions with {"enum": slot}.
    let mut unified = variants[0].mods.clone();
    for (path, &p) in diff_paths.iter().zip(&slot_for_diff) {
        *unified.pointer_mut(path).ok_or("enum_row_malformed")? =
            serde_json::json!({ "enum": row.word_slots[p] });
    }

    // 6. regex + dedup + whitelist conversion + compile validation (same gate as the regular path).
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

/// The fallback path when unification fails: emit a **singleton entry** per
/// word combination — mods are all literals (word-specific tag / value /
/// shape differences no longer need an enum landing spot), and the
/// pattern's word slots narrow to that combination's single-word
/// alternation. Front gate: any word slot hitting the whole dictionary ->
/// skip the whole entry as open vocabulary; combination count > 40 -> skip
/// (guards against a data explosion from a full cross-product over multiple
/// slots). A single combination that fails to convert just drops that
/// combination (narrowing the match surface, conservatively).
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

/// Recursively collects every `name` string in the mods JSON (including nested payloads).
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

/// Open-slot rescue: when a word slot hits the whole dictionary and isn't an
/// optional-prefix capture, filter combinations using the whole database's
/// known name set — a string-concatenation closure's noise-word-derived
/// name (`StrengthResist`) is unique across the database, whereas a valid
/// word's name (`FireResist`) recurs across other mods. Whatever remains
/// open after filtering is left to the caller's step-4 judgment; filtering
/// everything out -> skipped as open vocabulary. Rows with no open slot are
/// returned unchanged (avoids wrongly rejecting a name that's legitimately unique to this entry).
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
                return false; // the closure short-circuits to an empty table for noise words
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

/// Extracts the parenthesized content of a Lua pattern per capture group (1-based index aligned with `word_slots`).
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

/// Detects a pure optional-prefix capture: content has no wildcard atoms
/// (`.` / `%letter` classes / `[]` character classes), made up only of literal characters and `?` quantifiers (e.g. `i?t?e?m? ?`).
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

/// Structural diff: all combinations equal -> skip; objects/arrays of the
/// same shape recurse; differing string leaves -> record the JSON Pointer;
/// anything else (a shape mismatch / a number or bool difference) -> Err, skipping the whole entry.
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
                    return Err("enum_diff_shape".into()); // JSON Pointer escaping isn't modeled
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
        // A number/bool varying by word (e.g. numeric word values): TemplateValueDef has no enum numeric landing spot.
        _ => return Err("enum_diff_scalar".into()),
    }
    Ok(())
}

// raw mod JSON -> ModTemplateDef (the faithfulness whitelist)

fn transform_mods(v: &serde_json::Value) -> Result<Vec<ModTemplateDef>, String> {
    let arr = v.as_array().ok_or("mods_not_array")?;
    arr.iter().map(transform_mod).collect()
}

/// Equivalent rewrite for a captured PercentStat percent: `value × stat ×
/// percent/100` is commutative between value/percent — `value=1,
/// percent=$n` is equivalent to `value=$n, percent=1` (vendor's common form
/// "gain X equal to (N)% of stat", where percent comes from the capture).
/// After the rewrite the tag field is a plain literal and goes through the
/// existing whitelist; when the shape doesn't match (value != 1, or
/// multiple captured percents), returns None and the entry falls through
/// its original path to be skipped as `tag_field_capture`.
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
        // An enums closed-set reference (V2s5's unification output `{"enum": n}`).
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
    // A nested payload plus a scaling tag: vendor scales the inner
    // value.mod.value (ModStore.lua's table branch), but pobr's list_nested
    // forwarding only passes it through without evaluating -- allowing this would silently drop the scaling.
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

/// The value-template mini-syntax: `$n` / `$n:negate` / `$n:base(c)` / `$n:mult(k)` / `$n:div(k)`.
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
            // A literal string not starting with `$` has no corresponding
            // TemplateValueDef shape (untagged serde would misread it as a
            // Capture), so skip conservatively.
            parse_capture_template(s).ok_or("value_form".into())
        }
        serde_json::Value::Object(map) => {
            // A nested mod payload: a pure `{ "mod": <mod|[mod...]> }` shape
            // -> Nested (runtime ModValue::NestedMods, forwarded by the
            // orchestration layer). A mixed shape (mod plus other scalar
            // keys, e.g. ExtraAura's onlyAllies) can't be expressed at
            // runtime -> skip the whole entry.
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

/// A scalar inside a LIST value / tag field: a number / bool / literal
/// string or a bare `$n` reference. `$n:...` with an op chain, `$n:cap`, a
/// `+`-concatenated segment, or a nested table (e.g. a `{ mod = ... }`
/// nested mod) are all not scalars -> skip the whole entry.
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
        // An enums closed-set reference (V2s5's unification output `{"enum": n}`) — any other object shape is still rejected.
        serde_json::Value::Object(_) => match enum_ref(v) {
            Some(idx) => Ok(TemplateScalarDef::Enum { capture_index: idx }),
            None => Err("value_nested".into()),
        },
        serde_json::Value::Array(_) => Err("value_nested".into()),
        serde_json::Value::Null => Err("value_form".into()),
    }
}

/// Recognizes a `{"enum": n}` reference (a single-key object with a non-negative integer).
fn enum_ref(v: &serde_json::Value) -> Option<u32> {
    let map = v.as_object()?;
    if map.len() != 1 {
        return None;
    }
    map.get("enum")?.as_u64().map(|n| n as u32)
}

/// The tag-shape whitelist: aligned with the **faithfully-mapped** field set
/// of `pobr-core::rules::special_mod::compile_tag` (extra fields get
/// silently ignored at compile time, causing semantic drift — e.g.
/// Multiplier's `actor`).
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
        // statList/percentVar/actor/base/limit/floor shapes have no landing spot, so any extra field skips the whole entry.
        "PercentStat" => &["stat", "percent"],
        "MultiplierThreshold" => &["var", "threshold", "upper"],
        // statList/thresholdStat/thresholdPercent(Var)/actor shapes have no landing spot, so skip the whole entry.
        "StatThreshold" => &["stat", "threshold", "upper"],
        // includeTransfigured is ignored on the compile side (PoE2 has no
        // gem variants, so gem name -> gameId equivalence degenerates to
        // name equivalence); partialMatch/summonSkill/neg never occur, not allowed.
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
        // skillNameList is the only array field allowed (a list of string literals -> TextList).
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
        // Tag fields disallow capture references (compile_tag would treat a `$n` var as a literal or silently drop the tag)
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

// Out-of-range capture reference validation

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

// id / meta helpers

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

/// FNV-1a 64 truncated to 8 hex digits (a stable fingerprint of the vendor key, keeping ids unique).
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
        // `%-` escape = a literal hyphen; a bare `-` = Lua's lazy quantifier -> regex `*?`
        let (re, _) = lua_pattern_to_regex("off%-hand", true, &BTreeMap::new()).unwrap();
        assert_eq!(re, "off-hand");
        let (re, _) = lua_pattern_to_regex("a-b", true, &BTreeMap::new()).unwrap();
        assert_eq!(re, "a*?b");
    }

    #[test]
    fn open_and_word_captures_rejected() {
        assert!(lua_pattern_to_regex("deal (.-) damage", true, &BTreeMap::new()).is_err());
        assert!(lua_pattern_to_regex("(%a+) skills", true, &BTreeMap::new()).is_err());
        // A word-class capture on a static row (keep_captures=false) is allowed through as a generalized non-capturing group (V2s5).
        let (re, caps) = lua_pattern_to_regex("(%D+) skills", false, &BTreeMap::new()).unwrap();
        assert_eq!(re, r"(?:\D+) skills");
        assert_eq!(caps, 0);
    }

    #[test]
    fn word_alt_slots_convert_to_alternation_and_fragment() {
        // Word-slot alternation: longer words first (so `evasion` doesn't swallow `evasion rating` as a prefix).
        let mut alts = BTreeMap::new();
        alts.insert(
            2,
            WordAlt::Alternation(vec!["evasion".into(), "evasion rating".into()]),
        );
        let (re, caps) = lua_pattern_to_regex("gain (%d+) (%a+) per level", true, &alts).unwrap();
        assert_eq!(re, r"gain (\d+) (evasion rating|evasion) per level");
        assert_eq!(caps, 2);

        // Fragment: an optional-prefix capture is generalized from its word class, keeping the capture so indices stay aligned.
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

        // A differing numeric leaf (a numeric word value) -> Err, skipping the whole entry (caught by the singleton fallback).
        let a = serde_json::json!([{"name": "X", "value": 25.0}]);
        let b = serde_json::json!([{"name": "X", "value": 10.0}]);
        let mut out = Vec::new();
        assert!(collect_diffs("", &[&a, &b], &mut out).is_err());

        // A shape mismatch (a missing key, since the closure drops the skillId key for noise words) -> Err.
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
        // A Multiplier with an actor field: compile_tag would silently ignore actor -> semantic drift, so this must be rejected
        let tag = serde_json::json!({
            "type": "Multiplier", "var": "PowerCharge", "actor": "enemy"
        });
        assert!(transform_tag(&tag).is_err());
        let ok = serde_json::json!({ "type": "Condition", "var": "LowLife" });
        assert_eq!(transform_tag(&ok).unwrap().tag_type, "Condition");
    }

    #[test]
    fn skill_name_tag_transforms() {
        // A single name plus includeTransfigured (ignored on the compile side) passes.
        let single = serde_json::json!({
            "type": "SkillName", "skillName": "Fireball", "includeTransfigured": true
        });
        assert_eq!(transform_tag(&single).unwrap().tag_type, "SkillName");

        // A skillNameList array -> TextList.
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

        // A field outside the whitelist (partialMatch) / an empty list / a name containing a capture -> reject.
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
        // ExtraAura's { mod = ..., onlyAllies = true } mixed shape can't be expressed at runtime
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
        // The tag whitelist also applies to inner mods (dropping a tag would turn a conditional mod into an always-on one)
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
