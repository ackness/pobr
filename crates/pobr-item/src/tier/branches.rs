//! Raw integer interval matching for branch-aware display templates.
use super::*;
use pobr_data::catalog::stat_descriptions::{StatCondition, StatValueFormat};
use std::cmp::Ordering;

type Range = (i64, i64);
type BoxRanges = Vec<Range>;
const MAX_BOXES: usize = 4096;

pub(super) fn valid(d: &StatDescriptor) -> bool {
    !d.member_stats.is_empty()
        && d.member_stats.iter().collect::<BTreeSet<_>>().len() == d.member_stats.len()
        && !d.branches.is_empty()
        && d.branches.iter().all(|b| {
            b.conditions.len() == d.member_stats.len()
                && b.formats.len() == d.member_stats.len()
                && (b.lookups.is_empty() || b.lookups.len() == d.member_stats.len())
                && b.lookups.iter().filter(|l| l.is_some()).count() <= 1
                && b.lookups
                    .iter()
                    .zip(&b.formats)
                    .all(|(l, f)| l.is_none() || *f == Default::default())
                && b.conditions
                    .iter()
                    .all(|c| c.min.zip(c.max).is_none_or(|(a, b)| a <= b))
                && b.formats.iter().all(|f| {
                    f.multiplier != 0 && f.divisor > 0 && f.precision.is_none_or(|p| p <= 12)
                })
        })
}

// Decimal comparison remains exact; capture float rounding never supplies raw values.
#[derive(Clone, Copy, Debug)]
struct Decimal(i128, i128);
impl Decimal {
    fn parse(text: &str) -> Option<Self> {
        let (text, exp) = text
            .split_once(['e', 'E'])
            .map_or(Some((text, 0)), |(a, b)| Some((a, b.parse::<i32>().ok()?)))?;
        let negative = text.starts_with('-');
        let text = text.trim_start_matches(['+', '-']);
        let (whole, frac) = text.split_once('.').unwrap_or((text, ""));
        if whole.is_empty()
            || !whole
                .bytes()
                .chain(frac.bytes())
                .all(|c| c.is_ascii_digit())
        {
            return None;
        }
        let digits = format!("{whole}{frac}").parse::<i128>().ok()?;
        let power = i32::try_from(frac.len()).ok()?.checked_sub(exp)?;
        let (num, den) = if power >= 0 {
            (digits, 10i128.checked_pow(power as u32)?)
        } else {
            (
                digits.checked_mul(10i128.checked_pow(power.unsigned_abs())?)?,
                1,
            )
        };
        Some(Self(if negative { num.checked_neg()? } else { num }, den))
    }
    fn cmp(self, other: Self) -> Option<Ordering> {
        Some(
            self.0
                .checked_mul(other.1)?
                .cmp(&other.0.checked_mul(self.1)?),
        )
    }
}

fn image_range(format: &StatValueFormat, capture: Decimal, range: Range) -> Option<Range> {
    // The known affine transforms and decimal format are monotone. Binary search
    // their exact forward image on supplied raw integers, including rounding ties.
    let compare = |raw: i64| -> Option<Ordering> {
        let order = Decimal::parse(&format.display(raw, false)?)?.cmp(capture)?;
        Some(if format.multiplier < 0 {
            order.reverse()
        } else {
            order
        })
    };
    let lower = |strict: bool| -> Option<i128> {
        let mut lo = i128::from(range.0);
        let mut hi = i128::from(range.1) + 1;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let order = compare(i64::try_from(mid).ok()?)?;
            if order == Ordering::Less || (strict && order == Ordering::Equal) {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        Some(lo)
    };
    let lo = lower(false)?;
    let hi = lower(true)?.checked_sub(1)?;
    if lo > hi || lo > i128::from(range.1) {
        return None;
    }
    Some((i64::try_from(lo).ok()?, i64::try_from(hi).ok()?))
}

fn intersect(a: Range, b: Range) -> Option<Range> {
    let out = (a.0.max(b.0), a.1.min(b.1));
    (out.0 <= out.1).then_some(out)
}
fn condition_ranges(c: &StatCondition, range: Range) -> Vec<Range> {
    let Some((lo, hi)) = intersect(
        range,
        (c.min.unwrap_or(i64::MIN), c.max.unwrap_or(i64::MAX)),
    ) else {
        return vec![];
    };
    match c.not {
        Some(n) if lo <= n && n <= hi => {
            let mut out = Vec::new();
            if lo < n {
                out.push((lo, n - 1));
            }
            if n < hi {
                out.push((n + 1, hi));
            }
            out
        }
        _ => vec![(lo, hi)],
    }
}
fn product(ranges: &[Vec<Range>]) -> Option<Vec<BoxRanges>> {
    let mut boxes = vec![vec![]];
    for choices in ranges {
        let mut next = Vec::new();
        for b in boxes {
            for &r in choices {
                let mut b = b.clone();
                b.push(r);
                next.push(b);
                if next.len() > MAX_BOXES {
                    return None;
                }
            }
        }
        boxes = next;
    }
    Some(boxes)
}
fn subtract(mut b: BoxRanges, cut: &[Range]) -> Vec<BoxRanges> {
    let Some(overlap) = b
        .iter()
        .zip(cut)
        .map(|(&a, &c)| intersect(a, c))
        .collect::<Option<Vec<_>>>()
    else {
        return vec![b];
    };
    let mut out = Vec::new();
    for (i, &(lo, hi)) in overlap.iter().enumerate() {
        if b[i].0 < lo {
            let mut piece = b.clone();
            piece[i].1 = lo - 1;
            out.push(piece);
            b[i].0 = lo;
        }
        if hi < b[i].1 {
            let mut piece = b.clone();
            piece[i].0 = hi + 1;
            out.push(piece);
            b[i].1 = hi;
        }
    }
    out
}

pub(super) fn matches(
    stats: &[ModStat],
    entry: &TemplateEntry,
    d: &StatDescriptor,
    branch: usize,
    captures: &[&str],
) -> bool {
    matches_inner(stats, entry, d, branch, captures).unwrap_or(false)
}
fn matches_inner(
    stats: &[ModStat],
    entry: &TemplateEntry,
    d: &StatDescriptor,
    branch: usize,
    captures: &[&str],
) -> Option<bool> {
    if stats.len() != d.member_stats.len() || captures.len() != entry.slots.len() {
        return Some(false);
    }
    let mut values = vec![None; d.member_stats.len()];
    for (capture, slot) in captures.iter().zip(&entry.slots) {
        let capture = Decimal::parse(capture)?;
        match slot {
            Slot::Literal(v) => {
                if capture.cmp(Decimal::parse(&v.to_string())?)? != Ordering::Equal {
                    return Some(false);
                }
            }
            Slot::Value(k) => {
                let slot = values.get_mut(*k)?;
                if slot.is_some_and(|prev: Decimal| prev.cmp(capture) != Some(Ordering::Equal)) {
                    return Some(false);
                }
                *slot = Some(capture);
            }
        }
    }
    let current = d.branches.get(branch)?;
    let mut ranges = Vec::new();
    for (i, id) in d.member_stats.iter().enumerate() {
        let stat = stats.iter().find(|s| s.stat_id == *id)?;
        let range = if let Some((member, key)) = entry.lookup_key {
            if member == i {
                intersect((stat.min, stat.max), (key, key))?
            } else {
                (stat.min, stat.max)
            }
        } else {
            (stat.min, stat.max)
        };
        let mut parts = Vec::new();
        for range in condition_ranges(&current.conditions[i], range) {
            if let Some(value) = values[i] {
                if let Some(range) = image_range(&current.formats[i], value, range) {
                    parts.push(range);
                }
            } else {
                parts.push(range);
            }
        }
        if parts.is_empty() {
            return Some(false);
        }
        ranges.push(parts);
    }
    let mut boxes = product(&ranges)?;
    for earlier in d.branches.iter().take(branch) {
        let cuts = product(
            &earlier
                .conditions
                .iter()
                .map(|c| condition_ranges(c, (i64::MIN, i64::MAX)))
                .collect::<Vec<_>>(),
        )?;
        for cut in cuts {
            boxes = boxes.into_iter().flat_map(|b| subtract(b, &cut)).collect();
            if boxes.len() > MAX_BOXES {
                return None;
            }
            if boxes.is_empty() {
                return Some(false);
            }
        }
    }
    Some(!boxes.is_empty())
}

// Bind one already normalized numeric placeholder to a source-owned name.
// This does not parse source CSD or infer a key from the name.
pub(super) fn literal_template(template: &str, index: usize, literal: &str) -> Option<String> {
    if literal.trim().is_empty() || literal.contains(['{', '}']) {
        return None;
    }
    let mut rest = template;
    let mut out = String::new();
    while let Some((before, after)) = rest.split_once('{') {
        let (token, tail) = after.split_once('}')?;
        let (key, format) = token.split_once(':').unwrap_or((token, ""));
        let key = key.parse::<usize>().ok()?;
        out.push_str(before);
        if key == index {
            if !format.is_empty() {
                return None;
            }
            out.push_str(literal);
        } else {
            out.push('{');
            out.push_str(token);
            out.push('}');
        }
        rest = tail;
    }
    out.push_str(rest);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pobr_data::catalog::stat_descriptions::DescriptionBranch;
    fn descriptor(format: StatValueFormat) -> StatDescriptor {
        StatDescriptor {
            member_stats: vec!["value".into()],
            branches: vec![DescriptionBranch {
                template: "{0} effect".into(),
                conditions: vec![StatCondition::default()],
                lookups: Vec::new(),
                formats: vec![format],
            }],
        }
    }
    fn matches_value(d: &StatDescriptor, branch: usize, min: i64, max: i64, capture: &str) -> bool {
        let entry = TemplateEntry {
            stat_ids: d.member_stats.clone(),
            slots: vec![Slot::Value(0)],
            descriptor: Some((Arc::new(d.clone()), branch)),
            lookup_key: None,
        };
        matches(
            &[ModStat {
                stat_id: "value".into(),
                min,
                max,
            }],
            &entry,
            d,
            branch,
            &[capture],
        )
    }
    #[test]
    fn raw_integer_images_cover_units_offset_negative_rounding_and_exact_decimal_captures() {
        for (format, raw, capture) in [
            (
                StatValueFormat {
                    divisor: 60,
                    precision: Some(1),
                    ..Default::default()
                },
                150,
                "2.5",
            ),
            (
                StatValueFormat {
                    divisor: 1000,
                    ..Default::default()
                },
                1500,
                "1.5",
            ),
            (
                StatValueFormat {
                    offset: 200,
                    ..Default::default()
                },
                25,
                "225",
            ),
            (
                StatValueFormat {
                    multiplier: -1,
                    divisor: 1000,
                    precision: Some(1),
                    ..Default::default()
                },
                -1550,
                "1.6",
            ),
            (
                StatValueFormat {
                    divisor: 1000,
                    precision: Some(1),
                    ..Default::default()
                },
                -1550,
                "-1.6",
            ),
            (
                StatValueFormat {
                    divisor: 1000,
                    precision: Some(0),
                    ..Default::default()
                },
                -1999,
                "-1",
            ),
            (
                StatValueFormat {
                    divisor: 3,
                    ..Default::default()
                },
                1,
                "0.333333",
            ),
        ] {
            let d = descriptor(format);
            assert!(
                matches_value(&d, 0, raw, raw, capture),
                "{d:?}, {raw}, {capture}"
            );
            assert!(!matches_value(&d, 0, raw, raw, "999"));
        }
        let d = descriptor(Default::default());
        assert!(!matches_value(&d, 0, 1, 1, "1.0000000000000000001"));
        assert!(!matches_value(&d, 0, 1, 1, "NaN"));
        assert!(!matches_value(&d, 0, 1, 1, "9".repeat(100).as_str()));
    }
    #[test]
    fn first_matching_branch_excludes_shadowed_raw_rectangles() {
        let mut d = descriptor(Default::default());
        d.branches[0].conditions[0].max = Some(10);
        let mut second = d.branches[0].clone();
        second.conditions[0] = StatCondition::default();
        d.branches.push(second);
        assert!(!matches_value(&d, 1, 1, 20, "5"));
        assert!(matches_value(&d, 1, 1, 20, "15"));
        d.branches[0].conditions[0].not = Some(5);
        assert!(matches_value(&d, 1, 1, 20, "5"));
        d.branches[1].conditions[0].not = Some(15);
        assert!(!matches_value(&d, 1, 1, 20, "15"));
    }
    #[test]
    fn malformed_metadata_and_overflow_are_conservative() {
        for f in [
            StatValueFormat {
                divisor: 0,
                ..Default::default()
            },
            StatValueFormat {
                precision: Some(255),
                ..Default::default()
            },
            StatValueFormat {
                multiplier: 0,
                ..Default::default()
            },
        ] {
            assert!(!valid(&descriptor(f)));
        }
        let d = descriptor(StatValueFormat {
            multiplier: 2,
            ..Default::default()
        });
        assert!(!matches_value(&d, 0, i64::MAX, i64::MAX, "1"));
        assert_eq!(
            image_range(
                &StatValueFormat {
                    divisor: 10,
                    precision: Some(0),
                    ..Default::default()
                },
                Decimal::parse("-1").unwrap(),
                (-30, 0)
            ),
            Some((-19, -10))
        );
    }
}
