//! Lua pattern 子集匹配器 + vendor `scan()` 的「最早 + 最长」匹配语义
//! （蓝图 §2.1 / §2.2；vendor `ModParser.lua:6362-6385`）。
//!
//! formList / preFlagList / tagList 的 pattern 实测只用到 Lua pattern 的一个
//! 子集：`^`/`$` 锚、字面量、`%d %a %D %s` 类、`%%`/`%-`/`%.`/`%+` 等转义、
//! 字符类 `[...]`（含字面量集合如 `[hd][ae][va][el]`）、量词 `+ - * ?`
//! （`-` 是惰性最短匹配）、捕获 `(...)`（≤5 个，按 vendor cap1..cap5 上限）。
//!
//! **不引入 regex crate 做翻译**：Lua `-` 惰性量词与 regex 语义差异是静默错误
//! 源；pattern 才 ~1300 条且行短，性能由上层 literal 预过滤兜住；匹配器本身是
//! 跨版本稳定的框架语义（蓝图 §2.2 裁决）。输入在调用前已小写化。

/// 编译后的 Lua pattern（一次解析，复用匹配）。
#[derive(Debug, Clone)]
pub struct LuaPattern {
    items: Vec<PatItem>,
    /// `^` 锚定（只在位置 0 起匹配）。
    anchored: bool,
    /// 末尾 `$` 锚定（匹配必须抵达字符串末尾）。
    anchored_end: bool,
    /// 原 pattern 字节长度（vendor tie-break 第三级 `#pattern` 用）。
    raw_len: usize,
}

/// 单个匹配单元 = 基础类 + 量词。
///
/// 捕获组（`(...)`）可跨多个 item（如 `([%+%-]?%d+)` = `[%+%-]?` 与 `%d+` 两
/// item 同组）。组在 `cap_open` 标记的 item 起点处开始记录、在 `cap_close`
/// 标记的 item 终点处结束（1-based 组号；同一 item 可同时 open+close = 单 item
/// 捕获）。
#[derive(Debug, Clone)]
struct PatItem {
    class: PatClass,
    quant: Quant,
    /// 本 item 起点开启的捕获组号（1-based）；`None` = 不开启。
    cap_open: Option<usize>,
    /// 本 item 终点关闭的捕获组号（1-based）；`None` = 不关闭。
    cap_close: Option<usize>,
}

#[derive(Debug, Clone)]
enum PatClass {
    /// 字面字符（已小写）。
    Literal(char),
    /// `%d` 数字 / `%a` 字母 / `%s` 空白 / `%D` 非数字 / `%w` 字母数字 / `.` 任意。
    Class(ClassKind),
    /// `[...]` 字符集（含 `%d` 等类元素与字面量；`negated` = `[^...]`）。
    Set {
        members: Vec<SetMember>,
        negated: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClassKind {
    Digit,    // %d
    NotDigit, // %D
    Alpha,    // %a
    NotAlpha, // %A
    Space,    // %s
    NotSpace, // %S
    Word,     // %w (字母数字)
    NotWord,  // %W
    Any,      // .
}

#[derive(Debug, Clone)]
enum SetMember {
    Char(char),
    Class(ClassKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Quant {
    /// 恰好一次（无量词）。
    One,
    /// `+` 一次或多次（贪婪）。
    Plus,
    /// `*` 零次或多次（贪婪）。
    Star,
    /// `-` 零次或多次（惰性，Lua 语义：最短匹配）。
    Lazy,
    /// `?` 零次或一次。
    Opt,
}

/// 一次成功匹配的结果：起止字节位置（半开 `[start, end)`，0-based）+ 捕获文本。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuaMatch {
    /// 匹配起点（字节 offset，0-based）。
    pub start: usize,
    /// 匹配终点（字节 offset，0-based，半开）。
    pub end: usize,
    /// 捕获文本（按出现序，最多 5 个；空捕获 = 空串）。
    pub captures: Vec<String>,
}

/// pattern 编译错误（pattern 数据固定、编译期一次性；运行时不应触发）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternError {
    /// 原 pattern 文本。
    pub pattern: String,
    /// 失败原因。
    pub reason: String,
}

impl std::fmt::Display for PatternError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid lua pattern {:?}: {}", self.pattern, self.reason)
    }
}

impl LuaPattern {
    /// 编译 Lua pattern（子集语法）。输入按小写规范化的同款大小写处理。
    pub fn compile(pattern: &str) -> Result<Self, PatternError> {
        let chars: Vec<char> = pattern.chars().collect();
        let mut i = 0;
        let anchored = chars.first() == Some(&'^');
        if anchored {
            i = 1;
        }
        let mut items: Vec<PatItem> = Vec::new();
        let mut capture_counter = 0usize;
        // 开括号后等待被下一个 item 承接的捕获组号（支持嵌套子集：实测无嵌套，
        // 但用栈记 open 组保证 `)` 关闭最近未闭组）。
        let mut open_groups: Vec<usize> = Vec::new();
        // 下一个 push 的 item 要 open 的组号（`(` 紧接的 item）。
        let mut next_open: Option<usize> = None;
        let mut anchored_end = false;

        let err = |reason: &str| PatternError {
            pattern: pattern.to_string(),
            reason: reason.to_string(),
        };

        while i < chars.len() {
            let c = chars[i];
            match c {
                '(' => {
                    capture_counter += 1;
                    if capture_counter > 5 {
                        return Err(err("more than 5 capture groups"));
                    }
                    open_groups.push(capture_counter);
                    // 只有「最外层新开」的组由下一个 item 承接 open 标记；嵌套
                    // 子集不出现，简化为：每个 `(` 都让下一个 item open 该组。
                    next_open = Some(capture_counter);
                    i += 1;
                }
                ')' => {
                    let group = open_groups.pop().ok_or_else(|| err("unmatched ')'"))?;
                    // 关闭 = 把该组号标到最后一个已 push 的 item 上。
                    let last = items.last_mut().ok_or_else(|| err("empty capture group"))?;
                    last.cap_close = Some(group);
                    i += 1;
                }
                '$' if i == chars.len() - 1 => {
                    anchored_end = true;
                    i += 1;
                }
                _ => {
                    let (class, next) = parse_class(&chars, i, &err)?;
                    i = next;
                    let (quant, next) = parse_quant(&chars, i);
                    i = next;
                    items.push(PatItem {
                        class,
                        quant,
                        cap_open: next_open.take(),
                        cap_close: None,
                    });
                }
            }
        }
        if !open_groups.is_empty() {
            return Err(err("unclosed capture group"));
        }
        Ok(Self {
            items,
            anchored,
            anchored_end,
            raw_len: pattern.len(),
        })
    }

    /// 是否 `^` 锚定。
    pub fn is_anchored(&self) -> bool {
        self.anchored
    }

    /// 原 pattern 字节长度（vendor tie-break 第三级）。
    pub fn raw_len(&self) -> usize {
        self.raw_len
    }

    /// 在 `text` 上查找**最早**的匹配（vendor `find` 同义：从位置 0 起逐位尝试，
    /// 首个成功起点即返回；锚定 pattern 只在位置 0 试）。`text` 应已小写化。
    pub fn find(&self, text: &str) -> Option<LuaMatch> {
        let len = text.len();
        let mut start = 0usize;
        loop {
            if start > len {
                break;
            }
            if text.is_char_boundary(start) {
                let mut st = MatchState::default();
                if let Some(end) = self.match_at(text, start, 0, &mut st) {
                    let captures = st
                        .spans
                        .iter()
                        .filter_map(|c| c.map(|(s, e)| text[s..e].to_string()))
                        .collect();
                    return Some(LuaMatch {
                        start,
                        end,
                        captures,
                    });
                }
            }
            if self.anchored {
                break;
            }
            start += 1;
        }
        None
    }

    /// 从 `pos` 起尝试匹配 `items[item_idx..]`，返回匹配终点（字节 offset）。
    /// `st` 累积捕获组的起点 / 终点。回溯式（贪婪量词从最长回退、惰性从最短扩张）。
    fn match_at(
        &self,
        text: &str,
        pos: usize,
        item_idx: usize,
        st: &mut MatchState,
    ) -> Option<usize> {
        if item_idx >= self.items.len() {
            // 全部 item 消费完——若末尾锚定则要求抵达串末。
            if self.anchored_end && pos != text.len() {
                return None;
            }
            return Some(pos);
        }
        let item = &self.items[item_idx];
        // item 起点开启捕获组（记录组起点；回溯时还原）。
        let saved_open = item.cap_open.map(|n| (n, st.starts[n - 1]));
        if let Some(n) = item.cap_open {
            st.starts[n - 1] = Some(pos);
        }

        let result = match item.quant {
            Quant::One => class_match_one(text, pos, &item.class)
                .and_then(|next| self.consume_then(text, next, item, item_idx, st)),
            Quant::Opt => {
                // 先试匹配一次（贪婪），失败再试零次。
                class_match_one(text, pos, &item.class)
                    .and_then(|next| self.consume_then(text, next, item, item_idx, st))
                    .or_else(|| self.consume_then(text, pos, item, item_idx, st))
            }
            Quant::Plus | Quant::Star => {
                let min = if item.quant == Quant::Plus { 1 } else { 0 };
                let mut ends = vec![pos];
                let mut cur = pos;
                while let Some(next) = class_match_one(text, cur, &item.class) {
                    ends.push(next);
                    cur = next;
                }
                let mut found = None;
                for take in (min..ends.len()).rev() {
                    if let Some(end) = self.consume_then(text, ends[take], item, item_idx, st) {
                        found = Some(end);
                        break;
                    }
                }
                found
            }
            Quant::Lazy => {
                let mut cur = pos;
                let mut found = None;
                loop {
                    if let Some(end) = self.consume_then(text, cur, item, item_idx, st) {
                        found = Some(end);
                        break;
                    }
                    match class_match_one(text, cur, &item.class) {
                        Some(next) => cur = next,
                        None => break,
                    }
                }
                found
            }
        };

        // 回溯：还原本 item 的组起点。
        if result.is_none()
            && let Some((n, prev)) = saved_open
        {
            st.starts[n - 1] = prev;
        }
        result
    }

    /// 本 item 消费到 `consumed_end` 后：若该 item 关闭捕获组则定型组区间，再
    /// 递归匹配后续 item。失败时还原组终点。
    fn consume_then(
        &self,
        text: &str,
        consumed_end: usize,
        item: &PatItem,
        item_idx: usize,
        st: &mut MatchState,
    ) -> Option<usize> {
        let saved_close = item.cap_close.map(|n| (n, st.spans[n - 1]));
        if let Some(n) = item.cap_close {
            let start = st.starts[n - 1].unwrap_or(consumed_end);
            st.spans[n - 1] = Some((start, consumed_end));
        }
        let result = self.match_at(text, consumed_end, item_idx + 1, st);
        if result.is_none()
            && let Some((n, prev)) = saved_close
        {
            st.spans[n - 1] = prev;
        }
        result
    }
}

/// 匹配回溯状态：每组的起点（open 时记）与定型区间（close 时记）。
#[derive(Default)]
struct MatchState {
    /// 组起点（1-based 组号 → 字节 offset）。
    starts: [Option<usize>; 5],
    /// 组定型区间（1-based 组号 → `[start, end)`）。
    spans: [Option<(usize, usize)>; 5],
}

/// 解析一个基础类（`%x` / `[...]` / 字面量），返回 (class, next_index)。
fn parse_class(
    chars: &[char],
    i: usize,
    err: &impl Fn(&str) -> PatternError,
) -> Result<(PatClass, usize), PatternError> {
    let c = chars[i];
    match c {
        '%' => {
            let next = *chars.get(i + 1).ok_or_else(|| err("dangling % escape"))?;
            if let Some(kind) = class_kind(next) {
                Ok((PatClass::Class(kind), i + 2))
            } else {
                // %% %- %. %+ 等转义字面量。
                Ok((PatClass::Literal(next), i + 2))
            }
        }
        '.' => Ok((PatClass::Class(ClassKind::Any), i + 1)),
        '[' => parse_set(chars, i, err),
        _ => Ok((PatClass::Literal(c), i + 1)),
    }
}

/// 解析 `[...]` 字符集。
fn parse_set(
    chars: &[char],
    start: usize,
    err: &impl Fn(&str) -> PatternError,
) -> Result<(PatClass, usize), PatternError> {
    let mut i = start + 1; // 跳过 '['
    let negated = chars.get(i) == Some(&'^');
    if negated {
        i += 1;
    }
    let mut members = Vec::new();
    while i < chars.len() && chars[i] != ']' {
        let c = chars[i];
        if c == '%' {
            let next = *chars.get(i + 1).ok_or_else(|| err("dangling % in set"))?;
            if let Some(kind) = class_kind(next) {
                members.push(SetMember::Class(kind));
            } else {
                members.push(SetMember::Char(next));
            }
            i += 2;
        } else if i + 2 < chars.len() && chars[i + 1] == '-' && chars[i + 2] != ']' {
            // 区间 a-z。
            let lo = c;
            let hi = chars[i + 2];
            for ch in lo..=hi {
                members.push(SetMember::Char(ch));
            }
            i += 3;
        } else {
            members.push(SetMember::Char(c));
            i += 1;
        }
    }
    if chars.get(i) != Some(&']') {
        return Err(err("unclosed character set"));
    }
    Ok((PatClass::Set { members, negated }, i + 1))
}

/// 解析量词后缀。
fn parse_quant(chars: &[char], i: usize) -> (Quant, usize) {
    match chars.get(i) {
        Some('+') => (Quant::Plus, i + 1),
        Some('*') => (Quant::Star, i + 1),
        Some('-') => (Quant::Lazy, i + 1),
        Some('?') => (Quant::Opt, i + 1),
        _ => (Quant::One, i),
    }
}

fn class_kind(c: char) -> Option<ClassKind> {
    Some(match c {
        'd' => ClassKind::Digit,
        'D' => ClassKind::NotDigit,
        'a' => ClassKind::Alpha,
        'A' => ClassKind::NotAlpha,
        's' => ClassKind::Space,
        'S' => ClassKind::NotSpace,
        'w' => ClassKind::Word,
        'W' => ClassKind::NotWord,
        _ => return None,
    })
}

/// 在 `pos` 处匹配单个 class（一个字符），成功返回消费后的位置。
fn class_match_one(text: &str, pos: usize, class: &PatClass) -> Option<usize> {
    let c = text[pos..].chars().next()?;
    let ok = match class {
        PatClass::Literal(lit) => c == *lit,
        PatClass::Class(kind) => class_kind_match(c, *kind),
        PatClass::Set { members, negated } => {
            let hit = members.iter().any(|m| match m {
                SetMember::Char(ch) => c == *ch,
                SetMember::Class(kind) => class_kind_match(c, *kind),
            });
            hit != *negated
        }
    };
    if ok { Some(pos + c.len_utf8()) } else { None }
}

fn class_kind_match(c: char, kind: ClassKind) -> bool {
    match kind {
        ClassKind::Digit => c.is_ascii_digit(),
        ClassKind::NotDigit => !c.is_ascii_digit(),
        ClassKind::Alpha => c.is_ascii_alphabetic(),
        ClassKind::NotAlpha => !c.is_ascii_alphabetic(),
        ClassKind::Space => c.is_whitespace(),
        ClassKind::NotSpace => !c.is_whitespace(),
        ClassKind::Word => c.is_ascii_alphanumeric(),
        ClassKind::NotWord => !c.is_ascii_alphanumeric(),
        ClassKind::Any => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(pat: &str, text: &str) -> Option<LuaMatch> {
        LuaPattern::compile(pat).unwrap().find(text)
    }

    // ---- 字面量 + 锚定 ----

    #[test]
    fn anchored_literal_at_start_only() {
        let r = m("^increased", "increased damage").unwrap();
        assert_eq!((r.start, r.end), (0, 9));
        assert!(m("^increased", "much increased").is_none());
    }

    #[test]
    fn unanchored_literal_finds_earliest() {
        let r = m("damage", "fire damage and cold damage").unwrap();
        assert_eq!(r.start, 5);
    }

    #[test]
    fn end_anchor_requires_string_end() {
        assert!(m("damage$", "fire damage").is_some());
        assert!(m("damage$", "damage taken").is_none());
    }

    // ---- %d / %a / 捕获 ----

    #[test]
    fn digit_capture() {
        let r = m("^(%d+)%% increased", "50% increased fire").unwrap();
        assert_eq!(r.captures, vec!["50"]);
        assert_eq!(r.start, 0);
        // end 覆盖 "50% increased"
        assert_eq!(r.end, "50% increased".len());
    }

    #[test]
    fn alpha_capture() {
        let r = m("added (%a+) damage", "10 added fire damage").unwrap();
        assert_eq!(r.captures, vec!["fire"]);
    }

    #[test]
    fn multi_capture_dmg_range() {
        let r = m("(%d+) to (%d+) (%a+) damage", "5 to 12 cold damage").unwrap();
        assert_eq!(r.captures, vec!["5", "12", "cold"]);
    }

    #[test]
    fn decimal_class_capture() {
        let r = m("^([%d%.]+) (%a+)", "1.5 fire").unwrap();
        assert_eq!(r.captures, vec!["1.5", "fire"]);
    }

    #[test]
    fn signed_optional_capture() {
        let r = m("^([%+%-]?%d+)%% additional", "+5% additional").unwrap();
        assert_eq!(r.captures, vec!["+5"]);
        let r2 = m("^([%+%-]?%d+)%% additional", "5% additional").unwrap();
        assert_eq!(r2.captures, vec!["5"]);
    }

    // ---- 转义 ----

    #[test]
    fn percent_escape() {
        let r = m("(%d+)%% more", "30% more").unwrap();
        assert_eq!(r.captures, vec!["30"]);
    }

    #[test]
    fn hyphen_range_escape() {
        // vendor `(%d+)%-(%d+)` 区间写法。
        let r = m("(%d+)%-(%d+) added", "3-7 added").unwrap();
        assert_eq!(r.captures, vec!["3", "7"]);
    }

    // ---- 字符类集合 `[hd][ae][va][el]` 类糅合 ----

    #[test]
    fn char_set_class_blend_have_deal() {
        // vendor `[cthd][ae][ukva][sel]e?` = deal/have/take 变体（"use" 的 'u'
        // 不在首字符集 [cthd]，由另一条 `^minions ` pattern 兜——故此处不命中 "use"）。
        let pat = LuaPattern::compile("^minions [cthd][ae][ukva][sel]e? ").unwrap();
        assert!(pat.find("minions deal increased ").is_some());
        assert!(pat.find("minions have increased ").is_some());
        assert!(pat.find("minions take increased ").is_some());
        assert!(pat.find("minions use increased ").is_none());
    }

    #[test]
    fn negated_set() {
        // [^%s] 非空白。
        let r = m("([^%s]+)", " abc").unwrap();
        assert_eq!(r.captures, vec!["abc"]);
    }

    // ---- 量词语义 ----

    #[test]
    fn star_zero_or_more() {
        let r = m("ab*c", "ac").unwrap();
        assert_eq!((r.start, r.end), (0, 2));
        let r2 = m("ab*c", "abbbc").unwrap();
        assert_eq!(r2.end, 5);
    }

    #[test]
    fn lazy_shortest_match() {
        // Lua `-` 惰性：`a.-c` 在 "axcyc" 匹配到第一个 c（最短）。
        let r = m("a(.-)c", "axcyc").unwrap();
        assert_eq!(r.captures, vec!["x"]);
        assert_eq!(r.end, 3);
    }

    #[test]
    fn greedy_vs_lazy_distinction() {
        // 贪婪 `.+` 吃到最后一个 c。
        let r = m("a(.+)c", "axcyc").unwrap();
        assert_eq!(r.captures, vec!["xcy"]);
    }

    #[test]
    fn optional_quant() {
        let r = m("colou?r", "color").unwrap();
        assert_eq!(r.end, 5);
        let r2 = m("colou?r", "colour").unwrap();
        assert_eq!(r2.end, 6);
    }

    // ---- 最早+最长（tie-break）由 compiled.rs 的表级 scan 测；
    //      此处验证单 pattern 的 find 返回最早起点 ----

    #[test]
    fn find_returns_earliest_start() {
        let r = m("(%a+)", "  word").unwrap();
        assert_eq!(r.start, 2);
        assert_eq!(r.captures, vec!["word"]);
    }
}
