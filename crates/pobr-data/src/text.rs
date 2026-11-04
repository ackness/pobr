//! Pure text helpers for game display strings.
//!
//! Shared by the engine (modifier parsing, item and tree text) and by the data
//! adapter that projects game facts into runtime JSON, so they live in the
//! contract crate rather than in `pobr-core`.

/// The game's display markup reduced to the text it shows: a link
/// `[Hit|Hits]` shows its last part, a style `<underline>{Sorcery Ward}` the
/// text in its braces (the passive tree words "Grants Skill: <underline>{Sorcery
/// Ward}" this way).
pub fn strip_display_markup(text: &str) -> String {
    strip_pob_brackets(&strip_style_markup(text))
}

/// `<style>{text}` → `text`, where the style is a word such as `underline`.
/// Any other `<` stays as it is.
fn strip_style_markup(text: &str) -> String {
    if !text.contains(">{") {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('<') {
        let after = &rest[open + 1..];
        let styled = after.find(">{").filter(|&close| {
            close > 0
                && after[..close]
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
        });
        let inner = styled.and_then(|close| {
            let start = close + 2;
            let len = after[start..].find('}')?;
            Some((start, len))
        });
        let Some((start, len)) = inner else {
            out.push_str(&rest[..=open]);
            rest = after;
            continue;
        };
        out.push_str(&rest[..open]);
        out.push_str(&after[start..start + len]);
        rest = &after[start + len + 1..];
    }
    out.push_str(rest);
    out
}

fn strip_pob_brackets(text: &str) -> String {
    if !text.contains('[') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '[' {
            let mut inner = String::new();
            for ic in chars.by_ref() {
                if ic == ']' {
                    break;
                }
                inner.push(ic);
            }
            let display = inner.rsplit('|').next().unwrap_or(&inner);
            out.push_str(display);
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::strip_display_markup;

    #[test]
    fn links_show_their_last_part() {
        assert_eq!(
            strip_display_markup("[Hit|Hits] have 15% chance to [Lightning] [Shock]"),
            "Hits have 15% chance to Lightning Shock"
        );
    }

    #[test]
    fn styles_show_the_text_in_braces() {
        assert_eq!(
            strip_display_markup("Grants Skill: <underline>{Sorcery Ward}"),
            "Grants Skill: Sorcery Ward"
        );
        assert_eq!(
            strip_display_markup("<bold>{[Hit|Hits]} and <italic>{x}"),
            "Hits and x"
        );
    }

    #[test]
    fn other_angle_brackets_and_plain_text_stay() {
        assert_eq!(strip_display_markup("a < b > {c}"), "a < b > {c}");
        assert_eq!(strip_display_markup("<a b>{c}"), "<a b>{c}");
        assert_eq!(strip_display_markup("<>{c}"), "<>{c}");
        assert_eq!(strip_display_markup("<u>{open"), "<u>{open");
        assert_eq!(strip_display_markup("plain text"), "plain text");
    }

    #[test]
    fn unterminated_link_keeps_its_display_part() {
        assert_eq!(strip_display_markup("x [a|b"), "x b");
    }
}
