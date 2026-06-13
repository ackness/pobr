//! 词条行级标注（`{key:value}` / `{key}` 前缀）的**保真**解析与重建。
//!
//! 与 `pobr-core::item_text::strip_pob_annotations`（剥离丢语义，喂 calc）不同，本模块
//! **保留**全部标注以支持编辑态往返（BuildRaw 契约）。标注顺序与命名严格对照 PoB2
//! `Classes/Item.lua` `writeModLine`（1345-1389）：`{range}` → `{corruptedRange}` →
//! `{rune}` → `{enchant}` → `{custom}` → `{fractured}` → `{desecrated}` → `{mutated}` →
//! `{crafted}` → `{unscalable}` → `{variant:…}` → `{tags:…}` → 文本。

/// 单条词条行的标注集合 + 剥标注后的干净文本。
///
/// 字段顺序即 BuildRaw 输出顺序（[`ModLineAnnotations::render_prefix`]）。布尔标记按
/// PoB2 `writeModLine` 的写出顺序排列；`range`/`corrupted_range`/`variants`/`tags`
/// 为带值标注。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModLineAnnotations {
    /// `{range:x}`（PoB2 仅在行含 `(min-max)` 模板时写出，本模块保留原值不做模板判定）。
    pub range: Option<f64>,
    /// `{corruptedRange:x}`。
    pub corrupted_range: Option<f64>,
    pub rune: bool,
    pub enchant: bool,
    pub custom: bool,
    pub fractured: bool,
    pub desecrated: bool,
    pub mutated: bool,
    pub crafted: bool,
    pub unscalable: bool,
    /// `{variant:1,2,3}`——行级 variant 门控集合（CheckModLineVariant）。
    pub variants: Option<Vec<u32>>,
    /// `{tags:a,b}`——未建模标注原样保留。
    pub tags: Vec<String>,
}

impl ModLineAnnotations {
    /// 是否完全无标注（用于 BuildRaw 时省略前缀）。
    pub fn is_empty(&self) -> bool {
        self.range.is_none()
            && self.corrupted_range.is_none()
            && !self.rune
            && !self.enchant
            && !self.custom
            && !self.fractured
            && !self.desecrated
            && !self.mutated
            && !self.crafted
            && !self.unscalable
            && self.variants.is_none()
            && self.tags.is_empty()
    }

    /// 按 PoB2 `writeModLine` 顺序重建标注前缀字符串（不含正文）。
    ///
    /// 数值标注的 round 语义对照 vendor：`{range}`/`{corruptedRange}` 用 [`round_to`]
    /// （range 3 位、corruptedRange 2 位），整数值省略小数。
    pub fn render_prefix(&self) -> String {
        let mut out = String::new();
        if let Some(r) = self.range {
            out.push_str(&format!("{{range:{}}}", fmt_round(r, 3)));
        }
        if let Some(r) = self.corrupted_range {
            out.push_str(&format!("{{corruptedRange:{}}}", fmt_round(r, 2)));
        }
        if self.rune {
            out.push_str("{rune}");
        }
        if self.enchant {
            out.push_str("{enchant}");
        }
        if self.custom {
            out.push_str("{custom}");
        }
        if self.fractured {
            out.push_str("{fractured}");
        }
        if self.desecrated {
            out.push_str("{desecrated}");
        }
        if self.mutated {
            out.push_str("{mutated}");
        }
        if self.crafted {
            out.push_str("{crafted}");
        }
        if self.unscalable {
            out.push_str("{unscalable}");
        }
        if let Some(vars) = &self.variants {
            let spec: Vec<String> = vars.iter().map(u32::to_string).collect();
            out.push_str(&format!("{{variant:{}}}", spec.join(",")));
        }
        if !self.tags.is_empty() {
            out.push_str(&format!("{{tags:{}}}", self.tags.join(",")));
        }
        out
    }
}

/// 解析单条词条行：剥出**所有**行首 `{...}` 标注为结构化 [`ModLineAnnotations`]，返回
/// `(annotations, clean_text)`。
///
/// PoB2 `ParseRaw` 用 `for varSpec in line:gmatch("{variant:([%d,]+)}")` 等逐标注扫描；
/// 本实现等价地从行首循环吃 `{key[:value]}` 段。未识别的 `{key}` 归入 [`ModLineAnnotations::tags`]
/// 兜底（保证往返不丢字节）——但已知键（range/variant/各布尔）走专门字段。
pub fn parse_mod_line(line: &str) -> (ModLineAnnotations, String) {
    let mut ann = ModLineAnnotations::default();
    let mut rest = line;

    loop {
        let trimmed = rest.trim_start();
        if !trimmed.starts_with('{') {
            rest = trimmed;
            break;
        }
        let Some(close) = trimmed.find('}') else {
            rest = trimmed;
            break;
        };
        let inner = &trimmed[1..close];
        let after = &trimmed[close + 1..];

        let (key, value) = match inner.split_once(':') {
            Some((k, v)) => (k, Some(v)),
            None => (inner, None),
        };

        match key {
            "range" => ann.range = value.and_then(|v| v.trim().parse::<f64>().ok()),
            "corruptedRange" => {
                ann.corrupted_range = value.and_then(|v| v.trim().parse::<f64>().ok());
            }
            "rune" => ann.rune = true,
            "enchant" => ann.enchant = true,
            "custom" => ann.custom = true,
            "fractured" => ann.fractured = true,
            "desecrated" => ann.desecrated = true,
            "mutated" => ann.mutated = true,
            "crafted" => ann.crafted = true,
            "unscalable" => ann.unscalable = true,
            "variant" => {
                let ids: Vec<u32> = value
                    .unwrap_or("")
                    .split(',')
                    .filter_map(|s| s.trim().parse::<u32>().ok())
                    .collect();
                ann.variants = Some(ids);
            }
            "tags" => {
                ann.tags = value
                    .unwrap_or("")
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            // 未知键：原样塞回 tags 兜底，保证 lossless（PoB2 未来键不致丢字节）。
            other => ann.tags.push(other.to_string()),
        }
        rest = after;
    }

    (ann, rest.trim().to_string())
}

/// 把数值按 `digits` 位四舍五入；整数结果省略小数（对照 PoB2 `round(v, n)` + 字符串拼接）。
fn fmt_round(value: f64, digits: u32) -> String {
    let r = round_to(value, digits);
    if (r.fract()).abs() < f64::EPSILON {
        format!("{}", r as i64)
    } else {
        // 去掉尾随零（PoB2 Lua tostring 同行为：0.500 → 0.5）。
        let mut s = format!("{r:.*}", digits as usize);
        while s.contains('.') && s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
        s
    }
}

/// 四舍五入到 `digits` 位小数（对照 PoB2 `round`）。
pub fn round_to(value: f64, digits: u32) -> f64 {
    let f = 10f64.powi(digits as i32);
    (value * f).round() / f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_no_annotation_line() {
        let (ann, text) = parse_mod_line("+40 to maximum Life");
        assert!(ann.is_empty());
        assert_eq!(text, "+40 to maximum Life");
    }

    #[test]
    fn parses_enchant_rune_compound_prefix() {
        let (ann, text) = parse_mod_line("{enchant}{rune}+30 to maximum Runic Ward");
        assert!(ann.enchant);
        assert!(ann.rune);
        assert_eq!(text, "+30 to maximum Runic Ward");
    }

    #[test]
    fn parses_range_and_variant() {
        let (ann, text) = parse_mod_line("{range:0.5}{variant:1,3}+(40-50) to maximum Life");
        assert_eq!(ann.range, Some(0.5));
        assert_eq!(ann.variants, Some(vec![1, 3]));
        assert_eq!(text, "+(40-50) to maximum Life");
    }

    #[test]
    fn round_trips_prefix() {
        let (ann, _) = parse_mod_line("{crafted}{fractured}34% increased Critical Damage Bonus");
        assert_eq!(ann.render_prefix(), "{fractured}{crafted}");
    }

    #[test]
    fn unknown_key_preserved_in_tags() {
        let (ann, text) = parse_mod_line("{exotic}some future modifier");
        assert_eq!(ann.tags, vec!["exotic".to_string()]);
        assert_eq!(text, "some future modifier");
    }

    #[test]
    fn fmt_round_strips_trailing_zeros() {
        assert_eq!(fmt_round(0.5, 3), "0.5");
        assert_eq!(fmt_round(1.0, 3), "1");
        assert_eq!(fmt_round(0.333333, 3), "0.333");
    }
}
