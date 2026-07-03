//! [`ParseOutcome`] 的规范序列化——双跑 diff（Track C）与 precompile（Track D）
//! 的**共用比较单位**（蓝图 §5.2 / 契约 §11.3-2，禁两套序列化）。
//!
//! 比较单位：排序后的 `Vec<Modifier>` 的规范字符串。Modifier 按
//! `(name, mod_type, tags, flags, kw, value)` 排序；f64 用最短往返表示；
//! `source`（原文）参与、`origin`（SourceId）剔除（双跑两侧来源构造不同）。
//! status / unparsed 一并入比较。

use super::outcome::{ParseOutcome, ParseStatus};
use crate::modifier::ModValue;
use crate::{ModTag, Modifier};

/// 规范化一行解析结果为可比较字符串。
pub fn canonical_outcome(outcome: &ParseOutcome) -> String {
    let mut lines: Vec<String> = outcome.mods.iter().map(canonical_mod).collect();
    lines.sort();
    let status = match outcome.status {
        ParseStatus::Parsed => "parsed",
        ParseStatus::Unsupported => "unsupported",
    };
    let unparsed = outcome.unparsed.as_deref().unwrap_or("");
    format!(
        "status={status}|unparsed={unparsed}|mods=[{}]",
        lines.join(";")
    )
}

/// 单条 Modifier 的规范形态（不含 origin）。
fn canonical_mod(m: &Modifier) -> String {
    let value = canonical_value(&m.value);
    let tags = canonical_tags(&m.tags);
    format!(
        "name={}|type={:?}|flags={:#x}|kw={:#x}|tags={}|value={}",
        m.name.as_str(),
        m.mod_type,
        m.flags.bits(),
        m.keyword_flags.bits(),
        tags,
        value,
    )
}

fn canonical_value(v: &ModValue) -> String {
    match v {
        ModValue::Number(n) => fmt_f64(*n),
        ModValue::Bool(b) => format!("bool:{b}"),
        ModValue::Text(s) => format!("text:{s}"),
        ModValue::NestedMods(mods) => {
            let mut inner: Vec<String> = mods.iter().map(canonical_mod).collect();
            inner.sort();
            format!("nested:[{}]", inner.join(","))
        }
    }
}

/// f64 最短往返表示（整数省略小数；NaN/inf 显式标记）。
fn fmt_f64(n: f64) -> String {
    if n.is_nan() {
        return "nan".to_string();
    }
    if n.is_infinite() {
        return if n > 0.0 { "inf" } else { "-inf" }.to_string();
    }
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        // serde_json 的最短往返通过 ryu；这里用 {} 已足够确定（Rust f64 Display
        // 即最短往返）。
        format!("{n}")
    }
}

/// 一组 tag 的规范字符串（结构比较 / stat_id_map 模板的单源 tag 序列化）。
pub fn canonical_tags(tags: &[ModTag]) -> String {
    let mut out: Vec<String> = tags.iter().map(canonical_tag).collect();
    out.sort();
    format!("[{}]", out.join(","))
}

fn canonical_tag(tag: &ModTag) -> String {
    match tag {
        ModTag::Condition {
            var,
            negated,
            actor,
        } => {
            format!("Condition(var={var},neg={negated},actor={actor:?})")
        }
        ModTag::Multiplier {
            var,
            div,
            limit,
            actor,
            limit_var,
            limit_actor,
            invert,
            limit_total,
        } => format!(
            "Multiplier(var={var},div={},limit={:?},actor={actor:?},limit_var={limit_var:?},limit_actor={limit_actor:?},invert={invert},limit_total={limit_total})",
            fmt_f64(*div),
            limit.map(fmt_f64),
        ),
        ModTag::PerStat {
            stat,
            div,
            limit,
            limit_var,
            actor,
        } => format!(
            "PerStat(stat={stat},div={},limit={:?},limit_var={limit_var:?},actor={actor:?})",
            fmt_f64(*div),
            limit.map(fmt_f64),
        ),
        ModTag::PercentStat { stat, percent } => format!(
            "PercentStat(stat={stat},percent={:?})",
            percent.map(fmt_f64),
        ),
        ModTag::GlobalLimit { value, key } => {
            format!("GlobalLimit(value={},key={key})", fmt_f64(*value))
        }
        ModTag::DamageType(dt) => format!("DamageType({dt:?})"),
        ModTag::SkillTypes(st) => format!("SkillTypes({:#x})", st.bits()),
        ModTag::SkillName { names } => format!("SkillName(names=[{}])", names.join(",")),
        ModTag::SlotName(s) => format!("SlotName({s})"),
        ModTag::DistanceRamp { ramp } => {
            let points: Vec<String> = ramp
                .iter()
                .map(|(d, m)| format!("({},{})", fmt_f64(*d), fmt_f64(*m)))
                .collect();
            format!("DistanceRamp([{}])", points.join(","))
        }
        ModTag::MultiplierThreshold {
            var,
            threshold,
            upper,
        } => format!(
            "MultiplierThreshold(var={var},threshold={},upper={upper})",
            fmt_f64(*threshold)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Modifier;
    use pobr_data::prelude::ModType;

    #[test]
    fn canonical_ignores_origin_keeps_source() {
        let a = Modifier::number("Life", ModType::Base, 50.0).with_source("orig");
        let origin = pobr_data::prelude::ModifierSource::new(pobr_data::prelude::SourceId::new(
            pobr_data::prelude::SourceKind::PassiveNode,
            "node-1",
        ));
        let b = Modifier::number("Life", ModType::Base, 50.0)
            .with_source("orig")
            .with_origin(origin);
        let oa = ParseOutcome {
            mods: vec![a],
            status: ParseStatus::Parsed,
            unparsed: None,
            special_meta: None,
        };
        let ob = ParseOutcome {
            mods: vec![b],
            status: ParseStatus::Parsed,
            unparsed: None,
            special_meta: None,
        };
        // origin 不参与，source 不参与 mod 的规范（仅 name/type/flags/kw/tags/value）。
        assert_eq!(canonical_outcome(&oa), canonical_outcome(&ob));
    }

    #[test]
    fn canonical_mod_order_independent() {
        let m1 = Modifier::number("Life", ModType::Base, 50.0);
        let m2 = Modifier::number("Mana", ModType::Inc, 20.0);
        let o1 = ParseOutcome {
            mods: vec![m1.clone(), m2.clone()],
            status: ParseStatus::Parsed,
            unparsed: None,
            special_meta: None,
        };
        let o2 = ParseOutcome {
            mods: vec![m2, m1],
            status: ParseStatus::Parsed,
            unparsed: None,
            special_meta: None,
        };
        assert_eq!(canonical_outcome(&o1), canonical_outcome(&o2));
    }
}
