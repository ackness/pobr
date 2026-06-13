//! 数据驱动 parseMod 编排（蓝图 §4；vendor `ModParser.lua:6389-6755`）。
//!
//! 签名 [`parse_mod_engine`]`(text, &CompiledParserRules) -> ParseOutcome`。
//! 与 legacy 并存（`feature = "parser-engine"`），**不接调用方**——本 track 只
//! 建引擎 + 双跑 diff=0，切换是 D-T8 的独立 commit。
//!
//! 主序（vendor 逐段对照）：
//! 1. PoBR pre-pass（括号剥离 + 空白归一 + 小写视图，复用 legacy 同款语义）；
//! 2. unsupported 表查询；
//! 3. pre_flags 扫描（行尾补空格的 vendor 怪癖照搬）；
//! 4. formList 扫描，失配 → Unsupported；
//! 5. modTagList 扫描 ×2；
//! 6. 按 form 分发（forms.rs）→ name/suffix/value；
//! 7. modFlagList 扫描；
//! 8. 合并 flags/keywordFlags/tagList → 生成 Vec<Modifier>；
//! 9. misc 包装（addToAura/newAura/addToMinion/addToSkill/applyToEnemy → LIST mod）；
//! 10. 剩余文本非空白 → unparsed。
//!
//! special 通道 / skillNameList / order=1/2 双 pass 的接入随双跑覆盖率推进
//! （蓝图 §4 步 3/5；本批先实现 form 主路径以达成 C1 语料 diff=0 的形态对齐，
//! special 接入降级为 M6 内迭代项，见报告 §2.4）。

use pobr_data::catalog::parser_rules::RuleEffectsDef;
use pobr_data::prelude::ModType;

use super::compiled::CompiledParserRules;
use super::forms::{FormReject, eval_form};
use super::legacy::{ParseOutcome, ParseStatus};
use super::template::{compile_flags, compile_keyword_flags, compile_tag};
use crate::{ModTag, ModValue, Modifier};
use pobr_data::modifier::{KeywordFlags, ModFlags};

/// 数据驱动解析一行词条。永不报错（与 legacy `Unsupported` 同款收口；空输入
/// 返回 Unsupported 空表）。
pub fn parse_mod_engine(text: &str, rules: &CompiledParserRules) -> ParseOutcome {
    let original = text.trim();
    if original.is_empty() {
        return unsupported(text);
    }

    // 1. pre-pass：括号剥离 + 空白归一（保留原大小写）；小写视图供匹配。
    let cleaned = strip_pob_brackets(original);
    let normalized = normalize_spaces(&cleaned);
    let lower = normalized.to_ascii_lowercase();

    // 2. unsupported 整行查（小写）。
    if rules.unsupported.contains(&lower) {
        return ParseOutcome {
            mods: Vec::new(),
            status: ParseStatus::Unsupported,
            unparsed: Some(normalized),
            special_meta: None,
        };
    }

    // working line：vendor 在 form 扫描前 `line = line .. " "`（行尾补空格怪癖）。
    let mut work = format!("{normalized} ");

    // 3. pre_flags 扫描（pattern，^锚定多）。
    let mut effects_acc = EffectsAccumulator::default();
    {
        let lw = work.to_ascii_lowercase();
        if let Some((idx, _m, rest)) = rules.pre_flags.scan(&lw, &work) {
            let payload = rules.pre_flags.payload(idx);
            // handler 兜底条目：保守跳过效果（不产 mod，留 unparsed）——本批不接
            // handler 注册表（Track A 实测仅 3 条 handler 兜底）。
            if payload.handler_id.is_none() {
                effects_acc.absorb_pre_flag(payload, &_m.captures);
                work = rest;
            }
        }
    }

    // 4. formList 扫描。
    let (form_id, form_match, after_form) = {
        let lw = work.to_ascii_lowercase();
        match rules.forms.scan(&lw, &work) {
            Some((idx, m, rest)) => (rules.forms.payload(idx).clone(), m, rest),
            None => return unsupported_remaining(&work),
        }
    };
    work = after_form;

    // 5. modTagList 扫描 ×2（双 tag 词条）。
    for _ in 0..2 {
        let lw = work.to_ascii_lowercase();
        match rules.tag_phrases.scan(&lw, &work) {
            Some((idx, m, rest)) => {
                let payload = rules.tag_phrases.payload(idx);
                if payload.handler_id.is_some() {
                    // handler 兜底：本批不接，停止 tag 扫描（保守）。
                    break;
                }
                let absorbed = effects_acc.absorb_tag_phrase(&payload.effects, &m.captures);
                work = rest;
                if !absorbed {
                    // tag 含 pobr 无落点的类型 → 整行保守失配。
                    return unsupported_remaining(&work);
                }
            }
            None => break,
        }
    }

    // 6. form 分发（name/suffix/value 在 forms.rs 内完成；含 form 内部 scan）。
    let lw = work.to_ascii_lowercase();
    let form_result = match eval_form(&form_id, &form_match, &lw, &work, rules) {
        Ok(r) => r,
        Err(FormReject::EmptyTable) => {
            // vendor `return {}, line`：识别但无产出（Parsed 空表）。
            return ParseOutcome {
                mods: Vec::new(),
                status: ParseStatus::Parsed,
                unparsed: tail_unparsed(&work),
                special_meta: None,
            };
        }
        Err(FormReject::Nil) => return unsupported_remaining(&work),
    };
    work = form_result.remaining.clone();

    // 7. modFlagList 扫描（plain）。
    {
        let lw = work.to_ascii_lowercase();
        if let Some((idx, rest)) = rules.flag_phrases.scan(&lw, &work) {
            let eff = rules.flag_phrases.payload(idx);
            effects_acc.absorb_effects(eff, &[]);
            work = rest;
        }
    }

    // 8. 合并 flags/kw/tags → Vec<Modifier>。
    let flags = effects_acc.flags | form_result.extra_flags;
    // DMG 族默认 keyword：仅当无显式 keyword 时补（vendor `modFlag or {kw=...}`）。
    let keyword_flags = if effects_acc.keyword_flags.is_empty() {
        form_result.default_keyword
    } else {
        effects_acc.keyword_flags
    };

    let tags = effects_acc.tags.clone();

    let mut mods = Vec::with_capacity(form_result.names.len());
    for ((name, ty), value) in form_result
        .names
        .iter()
        .zip(form_result.types.iter())
        .zip(form_result.values.iter())
    {
        let full_name = format!("{}{}", name, form_result.suffix);
        let modv = match ty {
            ModType::Flag => ModValue::Bool(*value != 0.0),
            _ => ModValue::Number(*value),
        };
        let mut m = Modifier::new(full_name, *ty, modv).with_source(original);
        if !flags.is_empty() {
            m = m.with_flags(flags);
        }
        if !keyword_flags.is_empty() {
            m = m.with_keyword_flags(keyword_flags);
        }
        for t in &tags {
            m = m.with_tag(t.clone());
        }
        if form_result.hand_attack_condition {
            // GRANTS/REMOVES local：`{Hand}Attack` 条件——pobr 无 hand 占位实例化
            // 上下文（item ingest 时才知 hand），引擎侧暂以裸 var 记录（消费侧
            // 实例化）。本批保守用固定 `MainHandAttack`（与 legacy 行为对齐由
            // 双跑裁决；见报告 §2.4 D5）。
            m = m.with_tag(ModTag::condition("MainHandAttack", false));
        }
        mods.push(m);
    }

    // 9. misc LIST 包装（addToMinion / addToAura / newAura / addToSkill / applyToEnemy）。
    let mods = effects_acc.wrap_list(mods);

    ParseOutcome {
        mods,
        status: ParseStatus::Parsed,
        unparsed: tail_unparsed(&work),
        special_meta: None,
    }
}

/// 累积 pre_flag / flag_phrase / tag_phrase 的效果（flags / kw / tags + minion
/// 包装）。
///
/// 本批仅实现 `addToMinion` 包装（最高频）；其余 misc 包装指令（addToAura /
/// newAura / addToSkill / applyToEnemy / actorEnemy）在 vendor parseMod :6680-6750
/// 有对应 LIST 包装，本 track 暂不接（对应行产物原样返回，由双跑裁决登记，见报告
/// §2.4 D8）——故此处不存这些字段（避免 dead_code；接入时按数据字段补回）。
#[derive(Default)]
struct EffectsAccumulator {
    flags: ModFlags,
    keyword_flags: KeywordFlags,
    tags: Vec<ModTag>,
    add_to_minion: bool,
    add_to_minion_tags: Vec<ModTag>,
}

impl EffectsAccumulator {
    /// 吸收一个 RuleEffectsDef（flags/kw/tags + minion 包装指令）。返回 tags 是否
    /// 全部可映射（false = 含 pobr 无落点的 tag 类型）。
    fn absorb_effects(&mut self, eff: &RuleEffectsDef, captures: &[String]) -> bool {
        self.flags |= compile_flags(&eff.flags);
        self.keyword_flags = self.keyword_flags | compile_keyword_flags(&eff.keyword_flags);
        let mut all_mapped = true;
        for tag in &eff.tags {
            match compile_tag(tag, captures) {
                Some(t) => self.tags.push(t),
                None => all_mapped = false,
            }
        }
        // minion 包装指令（其余 misc 包装本批不接，见结构体注释）。
        self.add_to_minion |= eff.add_to_minion;
        for tag in &eff.add_to_minion_tags {
            if let Some(t) = compile_tag(tag, captures) {
                self.add_to_minion_tags.push(t);
            }
        }
        all_mapped
    }

    fn absorb_pre_flag(
        &mut self,
        payload: &super::compiled::PreFlagPayload,
        captures: &[String],
    ) -> bool {
        self.absorb_effects(&payload.effects, captures)
    }

    fn absorb_tag_phrase(&mut self, eff: &RuleEffectsDef, captures: &[String]) -> bool {
        self.absorb_effects(eff, captures)
    }

    /// misc 包装：把生成的 mods 转为 LIST 包裹 mod（vendor :6680-6750）。
    /// 本批仅实现 MinionModifier（最高频）；其余包装（ExtraAura / EnemyModifier /
    /// ExtraSkillMod）保守跳过——对应行的产物原样返回（由双跑裁决，见报告 §2.4 D8）。
    fn wrap_list(&self, mods: Vec<Modifier>) -> Vec<Modifier> {
        if self.add_to_minion && !mods.is_empty() {
            return mods
                .into_iter()
                .map(|inner| {
                    let mut wrapper = Modifier::new(
                        "MinionModifier",
                        ModType::List,
                        ModValue::NestedMods(vec![inner.clone()]),
                    );
                    if let Some(src) = &inner.source {
                        wrapper = wrapper.with_source(src.clone());
                    }
                    for t in &self.add_to_minion_tags {
                        wrapper = wrapper.with_tag(t.clone());
                    }
                    wrapper
                })
                .collect();
        }
        mods
    }
}

fn unsupported(text: &str) -> ParseOutcome {
    ParseOutcome {
        mods: Vec::new(),
        status: ParseStatus::Unsupported,
        unparsed: Some(text.trim().to_string()),
        special_meta: None,
    }
}

fn unsupported_remaining(work: &str) -> ParseOutcome {
    ParseOutcome {
        mods: Vec::new(),
        status: ParseStatus::Unsupported,
        unparsed: Some(work.trim().to_string()),
        special_meta: None,
    }
}

/// vendor `line:match("%S") and line`：剩余文本含非空白则作 unparsed，否则 None。
fn tail_unparsed(work: &str) -> Option<String> {
    if work.chars().any(|c| !c.is_whitespace()) {
        Some(work.trim().to_string())
    } else {
        None
    }
}

// ---- pre-pass 复用 legacy 同款语义（蓝图 §4 步 1，禁两套实现——这两个函数
//      legacy 私有，引擎侧复刻同款逻辑并保持逐字节一致；双跑会兜住差异）----

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

fn normalize_spaces(text: &str) -> String {
    // legacy normalize_spaces 含 to_ascii_lowercase——但引擎要保留原大小写视图
    // 以便切文本/source 保真。此处只做空白归一，小写视图单独算。
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 测试辅助：真实规则表路径（compiled.rs / 双跑测试共用）。
#[cfg(test)]
pub mod test_support {
    use std::path::PathBuf;

    /// 仓库真实 mod_parser_rules.json 路径。
    pub fn real_rules_path() -> PathBuf {
        // engine.rs 在 crates/pobr-core/src/mod_parser/，向上 4 级到 repo root。
        let manifest = env!("CARGO_MANIFEST_DIR"); // crates/pobr-core
        PathBuf::from(manifest).join("../../data/4.5.0.3.4/overlay/mod_parser_rules.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pobr_data::catalog::parser_rules::ModParserRulesDoc;

    fn real_rules() -> CompiledParserRules {
        let json = std::fs::read_to_string(test_support::real_rules_path()).unwrap();
        let doc: ModParserRulesDoc = serde_json::from_str(&json).unwrap();
        CompiledParserRules::compile(&doc).unwrap()
    }

    #[test]
    fn inc_fire_damage() {
        let r = real_rules();
        let o = parse_mod_engine("50% increased Fire Damage", &r);
        assert_eq!(o.status, ParseStatus::Parsed, "unparsed={:?}", o.unparsed);
        assert_eq!(o.mods.len(), 1);
        assert_eq!(o.mods[0].name.as_str(), "FireDamage");
        assert_eq!(o.mods[0].mod_type, ModType::Inc);
        assert_eq!(o.mods[0].value, ModValue::Number(50.0));
    }

    #[test]
    fn flat_max_life() {
        let r = real_rules();
        let o = parse_mod_engine("+50 to maximum Life", &r);
        assert_eq!(o.status, ParseStatus::Parsed, "unparsed={:?}", o.unparsed);
        assert!(o.mods.iter().any(|m| m.name.as_str() == "Life"
            && m.mod_type == ModType::Base
            && m.value == ModValue::Number(50.0)));
    }

    #[test]
    fn unsupported_garbage() {
        let r = real_rules();
        let o = parse_mod_engine("this is not a modifier xyzzy", &r);
        assert_eq!(o.status, ParseStatus::Unsupported);
    }

    #[test]
    fn mirrored_is_unsupported() {
        let r = real_rules();
        let o = parse_mod_engine("Mirrored", &r);
        assert_eq!(o.status, ParseStatus::Unsupported);
    }
}
