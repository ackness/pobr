//! 中文词条行 → 英文 canonical 的输入翻译层（TODO Phase 7.1）。
//!
//! 数据 = `i18n/zh-CN/stat_lines.json` 模板对（GGG stat descriptions 简中模板，
//! 经 `pipeline/gen-zh-cn.mjs` 从国服客户端词典转录）+ 基底名边车反查表。
//! 流程：输入行含 CJK → 骨架桶定位候选模板 → 逐段字面量匹配提取数值 →
//! 代回英文模板 → 喂现有英文 parser。引擎（pobr-core）保持纯英文，零改动。
//!
//! 匹配不引 regex：模板解析为「字面量 / `{N}`、`{N:+d}` 占位符」分段序列，
//! 顺序扫描；捕获值须为纯数值形（`[0-9+.-]`），防骨架碰撞误配。

use std::collections::HashMap;

use pobr_data::catalog::StatLineTemplate;

/// 模板分段。
#[derive(Debug, Clone)]
enum Segment {
    Literal(String),
    /// 占位符的数值下标（`{0}` / `{0:+d}` → 0）。
    Placeholder(usize),
}

/// 解析模板文本为分段序列；返回 `None` 表示占位符语法异常（整条弃用）。
fn parse_template(text: &str) -> Option<Vec<Segment>> {
    let mut segments = Vec::new();
    let mut literal = String::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            let end = text[i..].find('}')? + i;
            let inner = &text[i + 1..end];
            // `{0}` / `{0:+d}`：冒号前是数值下标。
            let index_part = inner.split(':').next()?;
            let index: usize = index_part.parse().ok()?;
            if !literal.is_empty() {
                segments.push(Segment::Literal(std::mem::take(&mut literal)));
            }
            segments.push(Segment::Placeholder(index));
            i = end + 1;
        } else {
            let ch = text[i..].chars().next()?;
            literal.push(ch);
            i += ch.len_utf8();
        }
    }
    if !literal.is_empty() {
        segments.push(Segment::Literal(literal));
    }
    Some(segments)
}

/// 骨架：剔除数值形字符与空白（模板侧同时剔除占位符），作为候选桶键。
/// 字面量里的数字（如「每 15 点闪避」）在输入与模板两侧同规则剔除，仍可对齐。
fn skeleton(text: &str) -> String {
    text.chars()
        .filter(|c| {
            !c.is_ascii_digit() && !matches!(c, '+' | '-' | '.' | ',') && !c.is_whitespace()
        })
        .collect()
}

fn template_skeleton(segments: &[Segment]) -> String {
    let mut out = String::new();
    for seg in segments {
        if let Segment::Literal(lit) = seg {
            out.push_str(&skeleton(lit));
        }
    }
    out
}

/// 捕获值合法性：非空且全为数值形字符（含符号/小数点）。
fn is_numeric_capture(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty()
        && trimmed
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '+' | '-' | '.'))
}

fn is_cjk(c: char) -> bool {
    ('\u{4E00}'..='\u{9FFF}').contains(&c) || ('\u{3400}'..='\u{4DBF}').contains(&c)
}

/// 字面量锚点是否「足够强」——够强才允许它门控一次字符串捕获回退匹配。
/// 规则：骨架 ≥3 字符，或含 CJK 且 ≥2 字符（CJK 单字信息量高，如「配置」）。
fn is_strong_anchor(skel: &str) -> bool {
    let n = skel.chars().count();
    n >= 3 || (n >= 2 && skel.chars().any(is_cjk))
}

/// 模板是否含格式化占位符（`{N:...}`，如 `{0:+d}`）——带格式说明的槽位是纯数值槽，
/// 这类模板一律不进字符串回退索引，保证数值模板行为一丝不变。
fn has_numeric_format(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{'
            && let Some(rel) = text[i..].find('}')
        {
            if text[i + 1..i + rel].contains(':') {
                return true;
            }
            i += rel + 1;
            continue;
        }
        i += 1;
    }
    false
}

/// 校验一段捕获文本：数值形照旧接受；`allow_string` 下额外接受非空、无换行的
/// 字符串（供「配置 {0}」这类字符串占位符模板使用）。返回归一化后的捕获值。
fn accept_capture(capture: &str, allow_string: bool) -> Option<String> {
    if is_numeric_capture(capture) {
        return Some(capture.trim().to_string());
    }
    if allow_string {
        let trimmed = capture.trim();
        if !trimmed.is_empty() && !trimmed.contains('\n') {
            return Some(trimmed.to_string());
        }
    }
    None
}

/// 编译后的一条模板对。
#[derive(Debug, Clone)]
struct CompiledTemplate {
    src_segments: Vec<Segment>,
    en: String,
}

/// 中文输入行翻译器（thread_local 缓存于 [`crate::state`]，构建一次）。
pub struct LineTranslator {
    /// 全部编译模板（`buckets`/`*_index` 存下标引用，避免克隆重复占内存）。
    templates: Vec<CompiledTemplate>,
    /// 骨架 → 候选模板下标（同骨架多候选逐个精确匹配；数值型模板主通道）。
    buckets: HashMap<String, Vec<usize>>,
    /// 首段字面量骨架 → 候选下标（字符串占位符模板的前缀锚点索引）。
    prefix_index: HashMap<String, Vec<usize>>,
    /// 末段字面量骨架 → 候选下标（后缀锚点索引；如「{0} 继承」）。
    suffix_index: HashMap<String, Vec<usize>>,
    /// 本地化基底名 → 英文 canonical 名（物品文本的基底行直译）。
    base_names: HashMap<String, String>,
}

impl LineTranslator {
    pub fn new(templates: &[StatLineTemplate], base_names: HashMap<String, String>) -> Self {
        let mut compiled: Vec<CompiledTemplate> = Vec::new();
        let mut buckets: HashMap<String, Vec<usize>> = HashMap::new();
        let mut prefix_index: HashMap<String, Vec<usize>> = HashMap::new();
        let mut suffix_index: HashMap<String, Vec<usize>> = HashMap::new();
        for pair in templates {
            let Some(src_segments) = parse_template(&pair.src) else {
                continue;
            };
            let idx = compiled.len();
            buckets
                .entry(template_skeleton(&src_segments))
                .or_default()
                .push(idx);
            // 只有含占位符、且首/末字面量锚点足够强的模板才进锚点索引——回退通道
            // 靠强锚点门控字符串捕获，压误配。
            let has_placeholder = src_segments
                .iter()
                .any(|s| matches!(s, Segment::Placeholder(_)));
            if has_placeholder && !has_numeric_format(&pair.src) {
                if let Some(Segment::Literal(lit)) = src_segments.first() {
                    let key = skeleton(lit);
                    if is_strong_anchor(&key) {
                        prefix_index.entry(key).or_default().push(idx);
                    }
                }
                if let Some(Segment::Literal(lit)) = src_segments.last() {
                    let key = skeleton(lit);
                    if is_strong_anchor(&key) {
                        suffix_index.entry(key).or_default().push(idx);
                    }
                }
            }
            compiled.push(CompiledTemplate {
                src_segments,
                en: pair.en.clone(),
            });
        }
        Self {
            templates: compiled,
            buckets,
            prefix_index,
            suffix_index,
            base_names,
        }
    }

    /// 尝试翻译一行：基底名直译优先，其次数值型模板精确匹配，最后字符串占位符
    /// 模板回退；不认识返回 `None`。
    pub fn translate_line(&self, line: &str) -> Option<String> {
        let line = line.trim();
        if let Some(en) = self.base_names.get(line) {
            return Some(en.clone());
        }
        let line_skel = skeleton(line);
        // 主通道：数值严格匹配、精确骨架桶。数值行行为一丝不变（命中即返回，
        // 根本不进回退）。
        if let Some(idxs) = self.buckets.get(&line_skel) {
            for &i in idxs {
                let candidate = &self.templates[i];
                if let Some(values) = match_segments(line, &candidate.src_segments, false) {
                    return Some(render_en(&candidate.en, &values));
                }
            }
        }
        // 回退：字符串占位符模板。骨架桶查不到（捕获含名字字符，骨架不再对齐），
        // 改用强前/后缀锚点选候选。
        self.translate_string_line(line, &line_skel)
    }

    /// 字符串占位符模板回退：按强前/后缀锚点选候选，允许字符串捕获，命中后
    /// 对捕获名尽力二次翻译再代回目标模板。
    fn translate_string_line(&self, line: &str, line_skel: &str) -> Option<String> {
        // idx → 已见最强锚点长度（同一模板可被前后缀同时选中，取更强者排序）。
        let mut candidates: HashMap<usize, usize> = HashMap::new();
        for (key, idxs) in &self.prefix_index {
            if line_skel.starts_with(key.as_str()) {
                let len = key.chars().count();
                for &i in idxs {
                    let entry = candidates.entry(i).or_insert(0);
                    *entry = (*entry).max(len);
                }
            }
        }
        for (key, idxs) in &self.suffix_index {
            if line_skel.ends_with(key.as_str()) {
                let len = key.chars().count();
                for &i in idxs {
                    let entry = candidates.entry(i).or_insert(0);
                    *entry = (*entry).max(len);
                }
            }
        }
        if candidates.is_empty() {
            return None;
        }
        // 确定性顺序：最强锚点优先，同长按下标升序。
        let mut ordered: Vec<(usize, usize)> =
            candidates.into_iter().map(|(i, len)| (len, i)).collect();
        ordered.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        for (_, i) in ordered {
            let candidate = &self.templates[i];
            if let Some(mut values) = match_segments(line, &candidate.src_segments, true) {
                // 尽力而为：把捕获出的名字（非数值）二次翻译再代回。
                for value in values.values_mut() {
                    if !is_numeric_capture(value)
                        && let Some(translated) = self.translate_value(value)
                    {
                        *value = translated;
                    }
                }
                return Some(render_en(&candidate.en, &values));
            }
        }
        None
    }

    /// 捕获值二次翻译：仅走基底名 + 数值型精确桶（不递归回退，杜绝无限递归），
    /// 命不中返回 `None`（调用方保留英文原名）。
    fn translate_value(&self, value: &str) -> Option<String> {
        let value = value.trim();
        if let Some(en) = self.base_names.get(value) {
            return Some(en.clone());
        }
        let idxs = self.buckets.get(&skeleton(value))?;
        for &i in idxs {
            let candidate = &self.templates[i];
            if let Some(values) = match_segments(value, &candidate.src_segments, false) {
                return Some(render_en(&candidate.en, &values));
            }
        }
        None
    }
}

/// 逐段匹配：字面量顺序对齐，占位符之间的文本作为捕获值。`allow_string=false`
/// 时捕获须数值形（主通道）；`true` 时额外允许非空、无换行的字符串（回退通道，
/// 候选已由强锚点门控）。成功返回 `占位符下标 → 捕获文本`。
fn match_segments(
    line: &str,
    segments: &[Segment],
    allow_string: bool,
) -> Option<HashMap<usize, String>> {
    let mut values: HashMap<usize, String> = HashMap::new();
    let mut pos = 0usize;
    let mut pending: Option<usize> = None;
    for seg in segments {
        match seg {
            Segment::Placeholder(index) => {
                // 相邻双占位符无字面量分隔——模板不该出现，保守失败。
                if pending.is_some() {
                    return None;
                }
                pending = Some(*index);
            }
            Segment::Literal(lit) => {
                let found = line[pos..].find(lit.as_str())?;
                if let Some(index) = pending.take() {
                    let value = accept_capture(&line[pos..pos + found], allow_string)?;
                    values.insert(index, value);
                } else if found != 0 {
                    return None;
                }
                pos += found + lit.len();
            }
        }
    }
    if let Some(index) = pending {
        let value = accept_capture(&line[pos..], allow_string)?;
        values.insert(index, value);
    } else if pos != line.len() {
        return None;
    }
    Some(values)
}

/// 把捕获值代回英文模板（`{N}` / `{N:+d}` 按下标替换；缺值保留原样）。
fn render_en(template: &str, values: &HashMap<usize, String>) -> String {
    let Some(segments) = parse_template(template) else {
        return template.to_string();
    };
    let mut out = String::new();
    for seg in &segments {
        match seg {
            Segment::Literal(lit) => out.push_str(lit),
            Segment::Placeholder(index) => match values.get(index) {
                Some(value) => out.push_str(value),
                None => out.push_str(&format!("{{{index}}}")),
            },
        }
    }
    out
}

/// 行内是否含 CJK 字符（触发翻译尝试的门）。
pub fn has_cjk(text: &str) -> bool {
    text.chars().any(is_cjk)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn translator() -> LineTranslator {
        let templates = vec![
            StatLineTemplate {
                src: "{0:+d} 生命上限".into(),
                en: "{0:+d} to maximum Life".into(),
            },
            StatLineTemplate {
                src: "盾牌上每有 15 点闪避，附加 {0} - {1} 火焰伤害".into(),
                en: "{0} to {1} Added Fire damage per 15 Evasion on Shield".into(),
            },
            StatLineTemplate {
                src: "能量护盾提高 {0}%".into(),
                en: "{0}% increased maximum Energy Shield".into(),
            },
            // EN→ZH 方向的字符串占位符模板（swapped：src=英文、en=简中）。
            StatLineTemplate {
                src: "Allocates {0}".into(),
                en: "配置 {0}".into(),
            },
            StatLineTemplate {
                src: "Legacy of {0}".into(),
                en: "{0} 继承".into(),
            },
        ];
        let mut names = HashMap::new();
        names.insert("蓝玉戒指".to_string(), "Sapphire Ring".to_string());
        LineTranslator::new(&templates, names)
    }

    #[test]
    fn translates_simple_line() {
        assert_eq!(
            translator().translate_line("+50 生命上限").as_deref(),
            Some("+50 to maximum Life")
        );
    }

    #[test]
    fn translates_two_placeholders_with_literal_digits() {
        assert_eq!(
            translator()
                .translate_line("盾牌上每有 15 点闪避，附加 10 - 20 火焰伤害")
                .as_deref(),
            Some("10 to 20 Added Fire damage per 15 Evasion on Shield")
        );
    }

    #[test]
    fn translates_base_name_and_suffix_percent() {
        let t = translator();
        assert_eq!(
            t.translate_line("蓝玉戒指").as_deref(),
            Some("Sapphire Ring")
        );
        assert_eq!(
            t.translate_line("能量护盾提高 33%").as_deref(),
            Some("33% increased maximum Energy Shield")
        );
    }

    #[test]
    fn rejects_non_numeric_capture_and_unknown_line() {
        let t = translator();
        assert_eq!(t.translate_line("很多 生命上限"), None);
        assert_eq!(t.translate_line("不认识的词条"), None);
    }

    #[test]
    fn translates_string_placeholder_prefix_template() {
        // 前缀锚点「Allocates」选中，捕获天赋名（无 zh 名字表 → 保留英文原名）。
        assert_eq!(
            translator()
                .translate_line("Allocates Grace of the Ancestors")
                .as_deref(),
            Some("配置 Grace of the Ancestors")
        );
    }

    #[test]
    fn translates_string_placeholder_suffix_template() {
        let out = translator()
            .translate_line("Legacy of Silver")
            .expect("should translate");
        assert!(out.contains("继承"), "got {out:?}");
    }

    #[test]
    fn numeric_lines_unchanged_by_string_fallback() {
        // 数值行仍走主通道、行为不变（回退不介入）。
        let t = translator();
        assert_eq!(
            t.translate_line("能量护盾提高 33%").as_deref(),
            Some("33% increased maximum Energy Shield")
        );
        assert_eq!(
            t.translate_line("+50 生命上限").as_deref(),
            Some("+50 to maximum Life")
        );
    }

    #[test]
    fn string_fallback_rejects_unanchored_line() {
        // 与任何强前/后缀锚点都不沾边——原样落空（消费侧回退英文原文）。
        assert_eq!(translator().translate_line("Zzz Qqq Wibble"), None);
    }
}
