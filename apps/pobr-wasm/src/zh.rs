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

/// 编译后的一条模板对。
#[derive(Debug, Clone)]
struct CompiledTemplate {
    src_segments: Vec<Segment>,
    en: String,
}

/// 中文输入行翻译器（thread_local 缓存于 [`crate::state`]，构建一次）。
pub struct LineTranslator {
    /// 骨架 → 候选模板（同骨架多候选逐个精确匹配）。
    buckets: HashMap<String, Vec<CompiledTemplate>>,
    /// 本地化基底名 → 英文 canonical 名（物品文本的基底行直译）。
    base_names: HashMap<String, String>,
}

impl LineTranslator {
    pub fn new(templates: &[StatLineTemplate], base_names: HashMap<String, String>) -> Self {
        let mut buckets: HashMap<String, Vec<CompiledTemplate>> = HashMap::new();
        for pair in templates {
            let Some(src_segments) = parse_template(&pair.src) else {
                continue;
            };
            buckets
                .entry(template_skeleton(&src_segments))
                .or_default()
                .push(CompiledTemplate {
                    src_segments,
                    en: pair.en.clone(),
                });
        }
        Self {
            buckets,
            base_names,
        }
    }

    /// 尝试翻译一行：基底名直译优先，其次词条模板匹配；不认识返回 `None`。
    pub fn translate_line(&self, line: &str) -> Option<String> {
        let line = line.trim();
        if let Some(en) = self.base_names.get(line) {
            return Some(en.clone());
        }
        let candidates = self.buckets.get(&skeleton(line))?;
        for candidate in candidates {
            if let Some(values) = match_segments(line, &candidate.src_segments) {
                return Some(render_en(&candidate.en, &values));
            }
        }
        None
    }
}

/// 逐段匹配：字面量顺序对齐，占位符之间的文本作为捕获值（须数值形）。
/// 成功返回 `占位符下标 → 捕获文本`。
fn match_segments(line: &str, segments: &[Segment]) -> Option<HashMap<usize, String>> {
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
                    let capture = &line[pos..pos + found];
                    if !is_numeric_capture(capture) {
                        return None;
                    }
                    values.insert(index, capture.trim().to_string());
                } else if found != 0 {
                    return None;
                }
                pos += found + lit.len();
            }
        }
    }
    if let Some(index) = pending {
        let capture = &line[pos..];
        if !is_numeric_capture(capture) {
            return None;
        }
        values.insert(index, capture.trim().to_string());
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
    text.chars()
        .any(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c) || ('\u{3400}'..='\u{4DBF}').contains(&c))
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
}
