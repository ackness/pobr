//! A Lua-pattern-subset matcher, plus vendor `scan()`'s "earliest + longest"
//! matching semantics (vendor `ModParser.lua:6362-6385`).
//!
//! In practice formList / preFlagList / tagList patterns only use a subset of
//! Lua patterns: `^`/`$` anchors, literals, the `%d %a %D %s` classes,
//! escapes like `%%`/`%-`/`%.`/`%+`, character sets `[...]` (including
//! literal-only sets like `[hd][ae][va][el]`), quantifiers `+ - * ?` (`-` is
//! the lazy shortest-match form), and captures `(...)` (at most 5, matching
//! vendor's cap1..cap5 limit).
//!
//! **We deliberately don't translate to the `regex` crate**: Lua's `-` lazy
//! quantifier differs from regex semantics in ways that fail silently;
//! there are only ~1300 patterns over short lines, and the upper-layer
//! literal pre-filter covers performance — the matcher itself just needs to
//! be stable framework semantics across data versions. Input is already
//! lowercased before this is called.

/// A compiled Lua pattern (parsed once, matched repeatedly).
#[derive(Debug, Clone)]
pub struct LuaPattern {
    items: Vec<PatItem>,
    /// `^` anchor (only matches starting at position 0).
    anchored: bool,
    /// Trailing `$` anchor (the match must reach the end of the string).
    anchored_end: bool,
    /// Byte length of the original pattern (used for vendor tie-break level
    /// three, `#pattern`).
    raw_len: usize,
}

/// A single match unit = a base class plus a quantifier.
///
/// A capture group (`(...)`) can span multiple items (e.g. `([%+%-]?%d+)` is
/// one group spanning the `[%+%-]?` and `%d+` items). A group starts
/// recording at the item marked `cap_open` and closes at the item marked
/// `cap_close` (1-based group numbers; a single item can both open and close
/// = a single-item capture).
#[derive(Debug, Clone)]
struct PatItem {
    class: PatClass,
    quant: Quant,
    /// The capture group number (1-based) this item opens; `None` = opens
    /// nothing.
    cap_open: Option<usize>,
    /// The capture group number (1-based) this item closes; `None` = closes
    /// nothing.
    cap_close: Option<usize>,
}

#[derive(Debug, Clone)]
enum PatClass {
    /// Literal character (already lowercased).
    Literal(char),
    /// `%d` digit / `%a` letter / `%s` whitespace / `%D` non-digit / `%w`
    /// alphanumeric / `.` any.
    Class(ClassKind),
    /// `[...]` character set (mixing class elements like `%d` with literals;
    /// `negated` = `[^...]`).
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
    Word,     // %w (alphanumeric)
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
    /// Exactly once (no quantifier).
    One,
    /// `+` one or more (greedy).
    Plus,
    /// `*` zero or more (greedy).
    Star,
    /// `-` zero or more (lazy, Lua semantics: shortest match).
    Lazy,
    /// `?` zero or one.
    Opt,
}

/// The result of a successful match: start/end byte positions (half-open
/// `[start, end)`, 0-based) plus captured text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuaMatch {
    /// Match start (byte offset, 0-based).
    pub start: usize,
    /// Match end (byte offset, 0-based, half-open).
    pub end: usize,
    /// Captured text (in order of appearance, up to 5; an empty capture is
    /// an empty string).
    pub captures: Vec<String>,
}

/// Pattern compile error (pattern data is fixed and compiled once; should
/// never trigger at runtime).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternError {
    /// The original pattern text.
    pub pattern: String,
    /// Failure reason.
    pub reason: String,
}

impl std::fmt::Display for PatternError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid lua pattern {:?}: {}", self.pattern, self.reason)
    }
}

impl LuaPattern {
    /// Compiles a Lua pattern (subset syntax). Input should use the same
    /// lowercased casing convention as the matcher.
    pub fn compile(pattern: &str) -> Result<Self, PatternError> {
        let chars: Vec<char> = pattern.chars().collect();
        let mut i = 0;
        let anchored = chars.first() == Some(&'^');
        if anchored {
            i = 1;
        }
        let mut items: Vec<PatItem> = Vec::new();
        let mut capture_counter = 0usize;
        // The capture group number waiting to be claimed by the next item
        // after an open paren (supports nested groups in principle — in
        // practice there's no nesting, but a stack tracks open groups so
        // `)` always closes the most recently unclosed one).
        let mut open_groups: Vec<usize> = Vec::new();
        // The group number that the next pushed item should open (the item
        // right after a `(`).
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
                    // Only a freshly opened outermost group gets claimed by
                    // the next item; since nesting never occurs in practice,
                    // this simplifies to: every `(` makes the next item open
                    // that group.
                    next_open = Some(capture_counter);
                    i += 1;
                }
                ')' => {
                    let group = open_groups.pop().ok_or_else(|| err("unmatched ')'"))?;
                    // Closing means tagging the group number onto the last
                    // pushed item.
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

    /// Whether the pattern is `^`-anchored.
    pub fn is_anchored(&self) -> bool {
        self.anchored
    }

    /// Byte length of the original pattern (vendor tie-break level three).
    pub fn raw_len(&self) -> usize {
        self.raw_len
    }

    /// Finds the **earliest** match in `text` (equivalent to vendor `find`:
    /// tries each position starting at 0 and returns on the first
    /// successful start; an anchored pattern only tries position 0). `text`
    /// should already be lowercased.
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

    /// Tries to match `items[item_idx..]` starting at `pos`, returning the
    /// match end (byte offset). `st` accumulates capture group start/end
    /// positions. Backtracking-based (greedy quantifiers back off from the
    /// longest match, lazy ones expand from the shortest).
    fn match_at(
        &self,
        text: &str,
        pos: usize,
        item_idx: usize,
        st: &mut MatchState,
    ) -> Option<usize> {
        if item_idx >= self.items.len() {
            // All items consumed — if end-anchored, require reaching the end
            // of the string.
            if self.anchored_end && pos != text.len() {
                return None;
            }
            return Some(pos);
        }
        let item = &self.items[item_idx];
        // If this item opens a capture group, record the group's start
        // (restored on backtrack).
        let saved_open = item.cap_open.map(|n| (n, st.starts[n - 1]));
        if let Some(n) = item.cap_open {
            st.starts[n - 1] = Some(pos);
        }

        let result = match item.quant {
            Quant::One => class_match_one(text, pos, &item.class)
                .and_then(|next| self.consume_then(text, next, item, item_idx, st)),
            Quant::Opt => {
                // Try matching once first (greedy), fall back to zero times.
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

        // Backtrack: restore this item's group start.
        if result.is_none()
            && let Some((n, prev)) = saved_open
        {
            st.starts[n - 1] = prev;
        }
        result
    }

    /// After this item consumes up to `consumed_end`: if it closes a capture
    /// group, finalize the group's span, then recursively match the
    /// remaining items. Restores the group's end on failure.
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

/// Backtracking match state: each group's start (recorded on open) and
/// finalized span (recorded on close).
#[derive(Default)]
struct MatchState {
    /// Group starts (1-based group number -> byte offset).
    starts: [Option<usize>; 5],
    /// Finalized group spans (1-based group number -> `[start, end)`).
    spans: [Option<(usize, usize)>; 5],
}

/// Parses a single base class (`%x` / `[...]` / a literal), returning
/// (class, next_index).
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
                // Escaped literals such as %% %- %. %+.
                Ok((PatClass::Literal(next), i + 2))
            }
        }
        '.' => Ok((PatClass::Class(ClassKind::Any), i + 1)),
        '[' => parse_set(chars, i, err),
        _ => Ok((PatClass::Literal(c), i + 1)),
    }
}

/// Parses a `[...]` character set.
fn parse_set(
    chars: &[char],
    start: usize,
    err: &impl Fn(&str) -> PatternError,
) -> Result<(PatClass, usize), PatternError> {
    let mut i = start + 1; // skip '['
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
            // Range, e.g. a-z.
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

/// Parses a trailing quantifier.
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

/// Matches a single class (one character) at `pos`, returning the position
/// after consuming it on success.
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

    // Literals + anchors

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

    // %d / %a / captures

    #[test]
    fn digit_capture() {
        let r = m("^(%d+)%% increased", "50% increased fire").unwrap();
        assert_eq!(r.captures, vec!["50"]);
        assert_eq!(r.start, 0);
        // end covers "50% increased"
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

    // Escapes

    #[test]
    fn percent_escape() {
        let r = m("(%d+)%% more", "30% more").unwrap();
        assert_eq!(r.captures, vec!["30"]);
    }

    #[test]
    fn hyphen_range_escape() {
        // vendor's `(%d+)%-(%d+)` range notation.
        let r = m("(%d+)%-(%d+) added", "3-7 added").unwrap();
        assert_eq!(r.captures, vec!["3", "7"]);
    }

    // Character-set class blends, e.g. `[hd][ae][va][el]`

    #[test]
    fn char_set_class_blend_have_deal() {
        // vendor `[cthd][ae][ukva][sel]e?` matches the deal/have/take
        // variants ("use"'s 'u' is not in the first character set [cthd],
        // it's covered by a separate `^minions ` pattern instead — so it
        // doesn't match "use" here).
        let pat = LuaPattern::compile("^minions [cthd][ae][ukva][sel]e? ").unwrap();
        assert!(pat.find("minions deal increased ").is_some());
        assert!(pat.find("minions have increased ").is_some());
        assert!(pat.find("minions take increased ").is_some());
        assert!(pat.find("minions use increased ").is_none());
    }

    #[test]
    fn negated_set() {
        // [^%s] non-whitespace.
        let r = m("([^%s]+)", " abc").unwrap();
        assert_eq!(r.captures, vec!["abc"]);
    }

    // Quantifier semantics

    #[test]
    fn star_zero_or_more() {
        let r = m("ab*c", "ac").unwrap();
        assert_eq!((r.start, r.end), (0, 2));
        let r2 = m("ab*c", "abbbc").unwrap();
        assert_eq!(r2.end, 5);
    }

    #[test]
    fn lazy_shortest_match() {
        // Lua's `-` is lazy: `a.-c` matches the first c in "axcyc" (shortest).
        let r = m("a(.-)c", "axcyc").unwrap();
        assert_eq!(r.captures, vec!["x"]);
        assert_eq!(r.end, 3);
    }

    #[test]
    fn greedy_vs_lazy_distinction() {
        // Greedy `.+` eats up to the last c.
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

    // ---- Earliest + longest (tie-break) is tested at the table level in
    //      compiled.rs::scan; here we just verify a single pattern's find
    //      returns the earliest start ----

    #[test]
    fn find_returns_earliest_start() {
        let r = m("(%a+)", "  word").unwrap();
        assert_eq!(r.start, 2);
        assert_eq!(r.captures, vec!["word"]);
    }
}
