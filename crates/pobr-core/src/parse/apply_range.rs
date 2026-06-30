//! `{range:x}` 词条取值引擎（WI-B3）：把 `+(40-50) to maximum Life` 类带区间词条按
//! `range`（0..1）线性具体化为单一数值文本，喂 `mod_parser`。
//!
//! 对照 PoB2 `Modules/ItemTools.lua::applyRange`（77-326）。
//!
//! **当前实现 = 朴素线性档（无 modScalability 表消费）**，对应蓝图 §3.B3 钉死的「无表降级」
//! 方向：`value = min + range*(max-min)`，含负号翻转与 increased/reduced 等 antonym 处理。
//! 这覆盖绝大多数单数值槽词条（PoB 导出物品 + ninja 语料里的 `(min-max)` 形态）。
//!
//! 未实现（待后续接 `mod_scalability.json`）：多数值槽的 modScalability 组合反查、
//! `divide_by_*` / `per_minute_to_per_second` 等 30 余种 format 换算、catalyst scalar
//! 的 per-tag 缩放、corruptedRange。这些走 [`apply_range`] 的 `scalability` 参数（当前恒
//! 传 None → 线性档）。接表后此处只新增 format 分发，不动线性兜底。
//!
//! 取值后的文本带 `approx` 语义（无表时严格说是中值近似而非游戏精确值），但**不丢词条**
//! ——这正是审计修复方向（区间词条不再静默丢失）。

/// 把 `line` 中所有 `(min-max)` 区间按 `range`（0..1）线性具体化。
///
/// - `+`/`-` 前缀保留：正值结果且原带 `+` 时保留 `+`；负号区间（`-(min-max)`）整体取负。
/// - `-N% increased` → `N% reduced` 等 antonym 改写（对照 ItemTools.lua:67-72 antonymFunc，
///   先于区间取值的负百分比场景）。
/// - 无区间时原样返回。
///
/// `scalability`：modScalability 表（当前未消费，预留接口；None = 朴素线性）。
/// `value_scalar`：modifier magnitude 乘子（当前线性档不施加，预留）。
pub fn apply_range(line: &str, range: f64, scalability: Option<&()>, value_scalar: f64) -> String {
    // 接表前：忽略 scalability/value_scalar（线性兜底），但保留签名供后续扩展。
    let _ = (scalability, value_scalar);
    let antonymed = apply_antonyms(line);
    substitute_ranges(&antonymed, range)
}

/// `increased`↔`reduced`、`more`↔`less` antonym 改写：把 `-N% increased X` 规范为
/// `N% reduced X`（对照 ItemTools.lua antonymFunc + `:gsub("%-(%d+%.?%d*%%) (%a+)")`）。
fn apply_antonyms(line: &str) -> String {
    // 匹配 `-<num>% <word>`，word ∈ {increased,reduced,more,less} → 翻转词、去负号。
    let mut out = String::with_capacity(line.len());
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'-'
            && let Some((consumed, replacement)) = try_antonym_at(&line[i..])
        {
            out.push_str(&replacement);
            i += consumed;
            continue;
        }
        // push 当前字符（按 UTF-8 字符推进）。
        let ch_len = utf8_len(bytes[i]);
        out.push_str(&line[i..i + ch_len]);
        i += ch_len;
    }
    out
}

/// 尝试在 `s`（以 `-` 开头）匹配 `-<num>% <antonym-word>`；命中返回 (消费字节数, 替换串)。
fn try_antonym_at(s: &str) -> Option<(usize, String)> {
    debug_assert!(s.starts_with('-'));
    let rest = &s[1..];
    // 解析 num（含小数）。
    let num_end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(rest.len());
    if num_end == 0 {
        return None;
    }
    let num = &rest[..num_end];
    let after_num = &rest[num_end..];
    // 要求紧跟 `% `。
    let after_pct = after_num.strip_prefix("% ")?;
    // 取词。
    let word_end = after_pct
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(after_pct.len());
    let word = &after_pct[..word_end];
    let antonym = match word {
        "increased" => "reduced",
        "reduced" => "increased",
        "more" => "less",
        "less" => "more",
        _ => return None,
    };
    // 消费 `-` + num + `% ` + word。
    let consumed = 1 + num_end + 2 + word_end;
    Some((consumed, format!("{num}% {antonym}")))
}

/// 替换全部 `(min-max)` 区间为 `min + range*(max-min)` 的具体值，保留 `+`/`-` 前缀语义。
///
/// 对照 ItemTools.lua:80-84 的首个 gsub：
/// `([%+-]?)%((%-?%d+%.?%d*)%-(%-?%d+%.?%d*)%)`。
fn substitute_ranges(line: &str, range: f64) -> String {
    let mut out = String::with_capacity(line.len());
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if let Some((consumed, value_text)) = try_range_at(&line[i..], range) {
            out.push_str(&value_text);
            i += consumed;
            continue;
        }
        let ch_len = utf8_len(bytes[i]);
        out.push_str(&line[i..i + ch_len]);
        i += ch_len;
    }
    out
}

/// 在 `s` 处尝试匹配 `[+-]?(min-max)`；命中返回 (消费字节数, 具体值文本)。
fn try_range_at(s: &str, range: f64) -> Option<(usize, String)> {
    let mut idx = 0;
    let sign = match s.as_bytes().first() {
        Some(b'+') => {
            idx = 1;
            Some('+')
        }
        Some(b'-') => {
            idx = 1;
            Some('-')
        }
        _ => None,
    };
    let after_sign = &s[idx..];
    let inner = after_sign.strip_prefix('(')?;
    let close = inner.find(')')?;
    let body = &inner[..close];
    // body = "min-max"（min/max 可含负号、小数）。拆分中间的连字符（非首字符的 `-`）。
    let (min_str, max_str) = split_range_body(body)?;
    let min: f64 = min_str.parse().ok()?;
    let max: f64 = max_str.parse().ok()?;
    let mut value = min + range * (max - min);
    if sign == Some('-') {
        value = -value;
    }
    let consumed = idx + 1 + close + 1; // sign + '(' + body + ')'
    // `+` 前缀：仅当结果为正才保留 `+`（对照 vendor `(sign == "+" and value > 0)`）。
    let text = if sign == Some('+') && value > 0.0 {
        format!("+{}", fmt_value(value))
    } else {
        fmt_value(value)
    };
    Some((consumed, text))
}

/// 拆 `min-max`：从首字符之后找第一个 `-`（min 自身的前导负号不算分隔符）。
fn split_range_body(body: &str) -> Option<(&str, &str)> {
    let bytes = body.as_bytes();
    // 跳过 min 的可选前导负号。
    let start = if bytes.first() == Some(&b'-') { 1 } else { 0 };
    let sep_rel = body[start..].find('-')?;
    let sep = start + sep_rel;
    Some((&body[..sep], &body[sep + 1..]))
}

/// 数值 → 文本：整数省略小数；否则保留（去尾零），对标 Lua `tostring`。
fn fmt_value(v: f64) -> String {
    if v.fract().abs() < f64::EPSILON {
        format!("{}", v as i64)
    } else {
        let mut s = format!("{v}");
        if s.contains('.') {
            while s.ends_with('0') {
                s.pop();
            }
            if s.ends_with('.') {
                s.pop();
            }
        }
        s
    }
}

/// UTF-8 首字节 → 该字符字节长度。
fn utf8_len(first: u8) -> usize {
    match first {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ar(line: &str, range: f64) -> String {
        apply_range(line, range, None, 1.0)
    }

    #[test]
    fn midpoint_single_range() {
        assert_eq!(ar("+(40-50) to maximum Life", 0.5), "+45 to maximum Life");
    }

    #[test]
    fn range_endpoints() {
        assert_eq!(ar("+(40-50) to maximum Life", 0.0), "+40 to maximum Life");
        assert_eq!(ar("+(40-50) to maximum Life", 1.0), "+50 to maximum Life");
    }

    #[test]
    fn fractional_result_preserved() {
        // 40 + 0.3*(50-40) = 43.
        assert_eq!(ar("+(40-50) to maximum Life", 0.3), "+43 to maximum Life");
        // 40 + 0.25*(45-40) = 41.25.
        assert_eq!(
            ar("(40-45)% increased Damage", 0.25),
            "41.25% increased Damage"
        );
    }

    #[test]
    fn two_ranges_in_one_line() {
        // adds (1-2) to (3-5) physical: 两个区间各自具体化。
        assert_eq!(
            ar("Adds (1-2) to (3-5) Physical Damage", 0.5),
            "Adds 1.5 to 4 Physical Damage"
        );
    }

    #[test]
    fn no_range_returns_unchanged() {
        assert_eq!(ar("+30 to maximum Life", 0.5), "+30 to maximum Life");
    }

    #[test]
    fn negative_range_prefix_negates() {
        // -(10-20)% → 取负：range 0.5 → -(15) = -15。
        assert_eq!(
            ar("-(10-20)% to Fire Resistance", 0.5),
            "-15% to Fire Resistance"
        );
    }

    #[test]
    fn plus_sign_dropped_when_result_non_positive() {
        // +(0-0) → value 0，非正 → 不保留 `+`（对照 vendor `value > 0`）。
        assert_eq!(ar("+(0-0) to Strength", 0.5), "0 to Strength");
    }

    #[test]
    fn antonym_negative_increased_to_reduced() {
        assert_eq!(
            apply_antonyms("-20% increased Damage"),
            "20% reduced Damage"
        );
        assert_eq!(
            apply_antonyms("-15% more Fire Damage"),
            "15% less Fire Damage"
        );
    }

    #[test]
    fn negative_min_range() {
        // (-5-10) → min=-5,max=10; range 0.5 → -5 + 0.5*15 = 2.5。
        assert_eq!(ar("(-5-10)% increased Speed", 0.5), "2.5% increased Speed");
    }
}
