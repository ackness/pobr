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

    // 6b. name_map 命中条目自带效果（keyword_flags / flags / tags）注入累加器
    //     （M6.3 归一：vendor modNameList 条目的 keywordFlags/tag——如各伤害专名
    //     的 DamageType tag、`magnitude of poison you inflict` 的 Poison kw）。
    if let Some(name_eff) = &form_result.name_effects {
        effects_acc.absorb_effects(name_eff, &[]);
    }

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
        // M6.3 路线 B 引擎归一：把 vendor「泛名 + flag/kw」组合归一为 PoBR 专名
        //（C3 damage flag→专名 / Speed flag→AttackSpeed,CastSpeed），并按最终名
        // 补 DamageType tag（C5）+ 收吸被专名吸收的 flag/kw。
        let norm = normalize_pobr_name(&full_name, flags, keyword_flags);
        let modv = match ty {
            ModType::Flag => ModValue::Bool(*value != 0.0),
            _ => ModValue::Number(*value),
        };
        let mut m = Modifier::new(norm.name, *ty, modv).with_source(original);
        if !norm.flags.is_empty() {
            m = m.with_flags(norm.flags);
        }
        if !norm.keyword_flags.is_empty() {
            m = m.with_keyword_flags(norm.keyword_flags);
        }
        for t in &tags {
            m = m.with_tag(t.clone());
        }
        if let Some(dt) = norm.damage_type {
            m = m.with_tag(ModTag::DamageType(dt));
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

/// 引擎归一产物：PoBR 专名 + 收吸后的 flag/kw + 应补的 DamageType。
struct NormalizedName {
    name: String,
    flags: ModFlags,
    keyword_flags: KeywordFlags,
    damage_type: Option<pobr_data::prelude::DamageType>,
}

/// M6.3 路线 B 引擎归一（C3 + C5）：把 vendor「泛名 + flag」组合归一为 PoBR 专名，
/// 收吸被专名吸收的 flag 位；并按最终名补 DamageType tag。
///
/// - **C3 Damage 族**：`Damage` + 武器/作用域 flag → 专名（`SpellDamage` /
///   `ProjectileDamage` / `AreaDamage` / `{Weapon}Damage`），清吸收位（legacy
///   专名不带这些 flag）。
/// - **C3 Speed 族**：`Speed` + ATTACK → `AttackSpeed`、+ CAST → `CastSpeed`
///   （武器隐含攻击的 `Condition(UsingX)` 由 flag_phrases 另挂，引擎此处保守只改名清攻击位）。
/// - **C5 DamageType**：最终名是五类基础伤害名 → 补对应 DamageType（legacy
///   `damage_type_for_name` 同表）。
fn normalize_pobr_name(name: &str, flags: ModFlags, kw: KeywordFlags) -> NormalizedName {
    use pobr_data::prelude::DamageType;

    let mut out_name = name.to_string();
    let mut out_flags = flags;
    let out_kw = kw;

    // C3 Damage 族：泛名 Damage + 作用域 flag → 专名。优先级：法术 > 投射 > 范围 >
    // 武器（与 legacy 专名映射对齐；item 实测每行至多一个作用域 flag）。
    if name == "Damage" {
        // 武器 flag（含 HIT 伴随位）→ 专名，清武器位与 HIT 位。
        const WEAPON_SPECIALS: &[(ModFlags, &str)] = &[
            (ModFlags::SPEAR, "SpearDamage"),
            (ModFlags::CROSSBOW, "CrossbowDamage"),
            (ModFlags::BOW, "BowDamage"),
            (ModFlags::MACE, "MaceDamage"),
            (ModFlags::WARSTAFF, "QuarterstaffDamage"),
        ];
        if flags.intersects(ModFlags::SPELL) {
            out_name = "SpellDamage".to_string();
            out_flags = out_flags.without(ModFlags::SPELL);
        } else if let Some((bit, special)) =
            WEAPON_SPECIALS.iter().find(|(b, _)| flags.intersects(*b))
        {
            out_name = (*special).to_string();
            out_flags = out_flags.without(*bit | ModFlags::HIT);
        } else if flags.intersects(ModFlags::PROJECTILE) {
            out_name = "ProjectileDamage".to_string();
            out_flags = out_flags.without(ModFlags::PROJECTILE);
        } else if flags.intersects(ModFlags::AREA) {
            out_name = "AreaDamage".to_string();
            out_flags = out_flags.without(ModFlags::AREA);
        }
    }

    // C3 Speed 族：泛名 Speed + ATTACK/CAST → AttackSpeed/CastSpeed，清对应位。
    if name == "Speed" {
        if flags.intersects(ModFlags::ATTACK) {
            out_name = "AttackSpeed".to_string();
            out_flags = out_flags.without(ModFlags::ATTACK);
        } else if flags.intersects(ModFlags::CAST) {
            out_name = "CastSpeed".to_string();
            out_flags = out_flags.without(ModFlags::CAST);
        }
    }

    // C5 DamageType：最终名是五类基础伤害名 → 补对应 DamageType。
    let damage_type = match out_name.as_str() {
        "PhysicalDamage" => Some(DamageType::Physical),
        "FireDamage" => Some(DamageType::Fire),
        "ColdDamage" => Some(DamageType::Cold),
        "LightningDamage" => Some(DamageType::Lightning),
        "ChaosDamage" => Some(DamageType::Chaos),
        _ => None,
    };

    NormalizedName {
        name: out_name,
        flags: out_flags,
        keyword_flags: out_kw,
        damage_type,
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
        // M6.3 路线 B：抽取期归一后引擎产 PoBR StatId `MaximumLife`（非 vendor `Life`）。
        assert!(o.mods.iter().any(|m| m.name.as_str() == "MaximumLife"
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
