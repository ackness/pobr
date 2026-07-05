//! 数据驱动 parseMod 编排（蓝图 §4；vendor `ModParser.lua:6389-6755`）。
//!
//! 签名 [`parse_mod_engine`]`(text, &CompiledParserRules) -> ParseOutcome`。
//! 引擎已是 calc 主路径：编排层经 pobr-gamedata 恒 load `mod_parser_rules.json`
//! 编译 [`CompiledParserRules`] 注入 session；legacy 仅作回退（未注入引擎规则时）。
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
use super::outcome::{ParseOutcome, ParseStatus};
use super::template::{compile_flags, compile_keyword_flags, compile_tag};
use crate::{ModTag, ModValue, Modifier};
use pobr_data::modifier::{KeywordFlags, ModFlags};
use pobr_data::skill::SkillTypes;

/// engine 解析的静默降级诊断（A2 可见化）：随 [`parse_mod_engine_diag`] 返回，
/// corpus 报表按行聚合。不进 [`ParseOutcome`]（40 处构造点零改动）、不进
/// canonical（C1 dual-run gate 不比较）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EngineDiag {
    /// pre_flag 条目命中但 tag 含 pobr 无落点类型 → 静默丢弃的 tag 数
    /// （丢 tag = 词条作用域被放大为全局生效的近似，over-apply 风险面）。
    pub dropped_pre_flag_tags: u16,
}

/// 数据驱动解析一行词条。永不报错（与 legacy `Unsupported` 同款收口；空输入
/// 返回 Unsupported 空表）。
pub fn parse_mod_engine(text: &str, rules: &CompiledParserRules) -> ParseOutcome {
    parse_mod_engine_diag(text, rules).0
}

/// [`parse_mod_engine`] 的诊断版：额外返回静默降级计数（A2 报表用；两者共享
/// 同一实现）。
pub fn parse_mod_engine_diag(
    text: &str,
    rules: &CompiledParserRules,
) -> (ParseOutcome, EngineDiag) {
    let mut diag = EngineDiag::default();
    let outcome = parse_mod_engine_impl(text, rules, &mut diag);
    (outcome, diag)
}

fn parse_mod_engine_impl(
    text: &str,
    rules: &CompiledParserRules,
    diag: &mut EngineDiag,
) -> ParseOutcome {
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

    // 2b. specialModList 通道（M6-conv2；vendor `parseMod` 在 formList 之前查
    //     specialModList 整行表，ModParser.lua:6151-6160）。命中 → 直接返回已
    //     实例化 mods（统一补 source 原文），对齐 vendor specialModList 锚定优先级。
    //     未注入数据时 rules.special 恒空，此分支恒不命中（行为 = conv1 引擎）。
    if let Some(matched) = rules.special.try_match(&lower, &rules.special_handlers) {
        let mods: Vec<Modifier> = matched
            .mods
            .into_iter()
            .map(|mut inner| {
                inner.source = Some(original.to_string());
                inner
            })
            .collect();
        // vendor specialModList 命中即「已处理整行」——无剩余 unparsed。空 mods 条目
        //（纯识别 / 未注册 handler）按 vendor 语义仍是 Parsed（识别但无产出）。
        return ParseOutcome {
            mods,
            status: ParseStatus::Parsed,
            unparsed: None,
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
                // 注：absorb 丢弃数 >0（tag 含 pobr 无落点的类型）时**沿用静默丢
                // tag**——大量 pre_flag 词条（buff/aura 域前缀）依赖丢 tag 后全局
                // 生效的近似（消费侧 cfg 多不带 skill_types 位）；收紧为保守失配
                // 实测 -4 def@5% / -9 @10%。作用域收紧随消费侧 per-skill cfg
                // 铺开逐步落地（同 tag_phrase 口径）。丢弃数进 diag（A2 报表）。
                diag.dropped_pre_flag_tags += effects_acc.absorb_pre_flag(payload, &_m.captures);
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
                let dropped = effects_acc.absorb_tag_phrase(&payload.effects, &m.captures);
                work = rest;
                if dropped > 0 {
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
    let mut flags = effects_acc.flags | form_result.extra_flags;
    // DMG 族默认 keyword：仅当无显式 keyword 时补（vendor `modFlag or {kw=...}`）。
    let mut keyword_flags = if effects_acc.keyword_flags.is_empty() {
        form_result.default_keyword
    } else {
        effects_acc.keyword_flags
    };
    // M6.3 归一（C3）：legacy 无 Attack/Spell keyword 位，统一折为 ModFlag
    //（legacy parse 约定，见 legacy.rs:1904 / :397）。把 keyword ATTACK/SPELL
    // 折进 flags，清 keyword 对应位——子集匹配语义等价、与 legacy 逐字节对齐。
    if keyword_flags.intersects(KeywordFlags::ATTACK) {
        flags |= ModFlags::ATTACK;
        keyword_flags = keyword_flags.without(KeywordFlags::ATTACK);
    }
    if keyword_flags.intersects(KeywordFlags::SPELL) {
        flags |= ModFlags::SPELL;
        keyword_flags = keyword_flags.without(KeywordFlags::SPELL);
    }

    let tags = effects_acc.tags.clone();

    // M6-conv2（bug #3 收敛）：`Triggered Spells deal …` 前缀的 SPELL flag 与
    // `Spell Damage` 作用域 SPELL 在 legacy 是两个独立来源——legacy 专名 `SpellDamage`
    // 仍带前缀 SPELL 位（flags=0x2）。引擎单 flag 通道下 C3 归一会把 `Damage`+SPELL
    // 折名后清 SPELL；当存在 triggered SkillType tag（= 前缀来源）时保留 SPELL 位，
    // 与 legacy 逐字节对齐（消费侧 SPELL 子集匹配语义不变）。
    let has_triggered = tags
        .iter()
        .any(|t| matches!(t, ModTag::SkillTypes(st) if st.intersects(SkillTypes::TRIGGERED)));

    let mut mods = Vec::with_capacity(form_result.names.len());
    for ((name, ty), value) in form_result
        .names
        .iter()
        .zip(form_result.types.iter())
        .zip(form_result.values.iter())
    {
        // form 自带后缀 + pre_flag `mod_suffix`（vendor `modSuffix`，如 `enemies
        // you curse take ` → `"Taken"`，inner 名 `Damage` → `DamageTaken`，
        // ModParser.lua:6683 `name .. misc.modSuffix`）。
        let full_name = format!("{}{}{}", name, form_result.suffix, effects_acc.mod_suffix);
        // M6.3 路线 B 引擎归一：把 vendor「泛名 + flag/kw」组合归一为 PoBR 专名
        //（C3 damage flag→专名 / Speed flag→AttackSpeed,CastSpeed），并按最终名
        // 补 DamageType tag（C5）+ 收吸被专名吸收的 flag/kw。
        let norm = normalize_pobr_name(&full_name, flags, keyword_flags, has_triggered);
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
        for cond in &norm.extra_conditions {
            m = m.with_tag(ModTag::condition(*cond, false));
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

    // C3 续（flagless `Speed` 拆名）：vendor「attack and cast speed」「use speed」「attack,
    // cast and movement speed」映射为泛名 `Speed`（无 attack/cast flag，故上面 C3 Speed 族
    // 不触发）。但 PoBR 速度计算的 bucket 按**名** `[AttackSpeed, CastSpeed, SkillSpeed]`
    // 聚合（calc/skill_use_time.rs），bare `Speed` **不被任何速度计算消费**——attack/cast
    // 速度全丢（bow-shot/ice-shot 等 Speed 偏低根因）。legacy 从不产 bare `Speed`
    // （expand_compound `attack and cast speed`→`["attack speed","cast speed"]`→AttackSpeed+
    // CastSpeed，legacy.rs:735）。对齐：把每条 bare `Speed` 拆成 AttackSpeed + CastSpeed。
    let mut split_mods = Vec::with_capacity(mods.len());
    for mut m in mods {
        if m.name.as_str() == "Speed" {
            let mut cast = m.clone();
            cast.name = "CastSpeed".into();
            m.name = "AttackSpeed".into();
            split_mods.push(m);
            split_mods.push(cast);
        } else {
            split_mods.push(m);
        }
    }
    let mods = split_mods;

    // 9. misc LIST 包装（addToMinion / addToAura / newAura / addToSkill / applyToEnemy）。
    let mods = effects_acc.wrap_list(mods);

    ParseOutcome {
        mods,
        status: ParseStatus::Parsed,
        unparsed: tail_unparsed(&work),
        special_meta: None,
    }
}

/// 累积 pre_flag / flag_phrase / tag_phrase 的效果（flags / kw / tags + minion /
/// enemy 包装）。
///
/// 已实现 LIST 包装：`addToMinion`（MinionModifier）、`applyToEnemy`
/// （EnemyModifier，M6-conv2）、`newAura`+`newAuraOnlyAllies`（ExtraAura，仅盟友
/// 时玩家不消费——vendor `ModParser.lua:6877` + `CalcPerform.lua:3104`）。其余 misc
/// 包装（addToAura / addToSkill）暂不接（对应行产物原样返回）。
#[derive(Default)]
struct EffectsAccumulator {
    flags: ModFlags,
    keyword_flags: KeywordFlags,
    tags: Vec<ModTag>,
    add_to_minion: bool,
    add_to_minion_tags: Vec<ModTag>,
    /// `applyToEnemy`（vendor `applyToEnemy` / `actorEnemy`）——产物包成
    /// `EnemyModifier LIST`，inner 附敌侧条件 + `Condition(Effective)`。
    apply_to_enemy: bool,
    /// `newAura`（vendor）——产物包成 `ExtraAura LIST`（玩家进攻管线不消费）。
    new_aura: bool,
    /// `newAuraOnlyAllies`（vendor）——光环仅作用盟友，玩家本人贡献为 0
    /// （`CalcPerform.lua:3104` 的 `if not onlyAllies` 跳过玩家）。
    new_aura_only_allies: bool,
    /// `modSuffix`（vendor，如 `take ` → `"Taken"`）——附加到 inner 名末尾。
    mod_suffix: String,
}

impl EffectsAccumulator {
    /// 吸收一个 RuleEffectsDef（flags/kw/tags + minion/enemy 包装指令）。返回
    /// **丢弃的 tag 数**（>0 = 含 pobr 无落点的 tag 类型；A2 报表可见化）。
    fn absorb_effects(&mut self, eff: &RuleEffectsDef, captures: &[String]) -> u16 {
        self.flags |= compile_flags(&eff.flags);
        self.keyword_flags = self.keyword_flags | compile_keyword_flags(&eff.keyword_flags);
        let mut dropped: u16 = 0;
        for tag in &eff.tags {
            match compile_tag(tag, captures) {
                Some(t) => self.tags.push(t),
                None => dropped += 1,
            }
        }
        // minion 包装指令。
        self.add_to_minion |= eff.add_to_minion;
        for tag in &eff.add_to_minion_tags {
            if let Some(t) = compile_tag(tag, captures) {
                self.add_to_minion_tags.push(t);
            }
        }
        // enemy 包装指令（applyToEnemy / actorEnemy 同走 EnemyModifier 包装）+ modSuffix。
        self.apply_to_enemy |= eff.apply_to_enemy || eff.actor_enemy;
        // aura 包装指令（newAura + newAuraOnlyAllies）。
        self.new_aura |= eff.new_aura;
        self.new_aura_only_allies |= eff.new_aura_only_allies;
        if let Some(suffix) = &eff.mod_suffix {
            self.mod_suffix = suffix.clone();
        }
        dropped
    }

    fn absorb_pre_flag(
        &mut self,
        payload: &super::compiled::PreFlagPayload,
        captures: &[String],
    ) -> u16 {
        self.absorb_effects(&payload.effects, captures)
    }

    fn absorb_tag_phrase(&mut self, eff: &RuleEffectsDef, captures: &[String]) -> u16 {
        self.absorb_effects(eff, captures)
    }

    /// misc 包装：把生成的 mods 转为 LIST 包裹 mod（vendor :6680-6750）。
    /// 已实现 MinionModifier、EnemyModifier、ExtraAura（newAura+onlyAllies）；其余
    /// （addToAura / addToSkill）保守跳过——对应行产物原样返回。
    fn wrap_list(&self, mods: Vec<Modifier>) -> Vec<Modifier> {
        // `newAura` + `newAuraOnlyAllies`（vendor `ModParser.lua:6877`）：把每条内层 mod
        // 包成 `ExtraAura LIST`——玩家进攻管线只读 `Damage`/`SpellDamage` 等专名、不读
        // `ExtraAura` 包装，故玩家贡献为 0（对齐 `CalcPerform.lua:3104` 的 `if not
        // onlyAllies` 跳过玩家、仅发随从）。**非** onlyAllies（「You and Allies in your
        // Presence …」）不包装——玩家本人也吃该内层 mod（vendor 同口径：player 在
        // `if not onlyAllies` 分支纳入）。随从侧的 ExtraAura 消费属后续（本 build 集随从
        // 不参与玩家 DPS，包装即修复玩家虚高）。
        if self.new_aura && self.new_aura_only_allies && !mods.is_empty() {
            return mods
                .into_iter()
                .map(|inner| {
                    let mut wrapper = Modifier::new(
                        "ExtraAura",
                        ModType::List,
                        ModValue::NestedMods(vec![inner.clone()]),
                    );
                    if let Some(src) = &inner.source {
                        wrapper = wrapper.with_source(src.clone());
                    }
                    wrapper
                })
                .collect();
        }
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
        // EnemyModifier 包装（vendor `applyToEnemy`，ModParser.lua:6733-6748）：单条
        // 外层 `EnemyModifier LIST NestedMods([inner...])`，inner 统一附
        // `Condition(Effective)`（pobr 敌侧 debuff 口径，legacy.rs:1236）；敌侧条件
        // 用 `Enemy<X>` 约定（vendor enemy-actor 条件 → pobr 加 `Enemy` 前缀；已带
        // `Enemy` 前缀的如 `EnemyInPresence` 不再二次加，legacy.rs:1198/:1207）。
        if self.apply_to_enemy && !mods.is_empty() {
            let src = mods.first().and_then(|m| m.source.clone());
            let inner: Vec<Modifier> = mods
                .into_iter()
                .map(|mut m| {
                    m.tags = m.tags.into_iter().map(prefix_enemy_condition).collect();
                    m = m.with_tag(ModTag::condition("Effective", false));
                    m
                })
                .collect();
            let mut wrapper =
                Modifier::new("EnemyModifier", ModType::List, ModValue::NestedMods(inner));
            if let Some(src) = src {
                wrapper = wrapper.with_source(src);
            }
            return vec![wrapper];
        }
        mods
    }
}

/// 敌侧 `Condition(var)` 加 `Enemy` 前缀（vendor enemy-actor 条件 → pobr 约定；
/// 已带 `Enemy` 前缀的不二次加，如 `EnemyInPresence`/`EnemyCursed`）。非 Condition
/// tag 原样返回。
fn prefix_enemy_condition(tag: ModTag) -> ModTag {
    match tag {
        ModTag::Condition {
            var,
            negated,
            actor,
        } if !var.starts_with("Enemy") => ModTag::Condition {
            var: format!("Enemy{var}"),
            negated,
            actor,
        },
        other => other,
    }
}

/// 引擎归一产物：PoBR 专名 + 收吸后的 flag/kw + 应补的 DamageType + 额外 condition。
struct NormalizedName {
    name: String,
    flags: ModFlags,
    keyword_flags: KeywordFlags,
    damage_type: Option<pobr_data::prelude::DamageType>,
    /// 额外条件 tag（C3 武器作用域在非伤害名上转 `Condition(UsingX)`）。
    extra_conditions: Vec<&'static str>,
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
fn normalize_pobr_name(
    name: &str,
    flags: ModFlags,
    kw: KeywordFlags,
    keep_spell: bool,
) -> NormalizedName {
    use pobr_data::prelude::DamageType;

    let mut out_name = name.to_string();
    let mut out_flags = flags;
    let out_kw = kw;

    // M6-conv2（bug #4 收敛）+ fork-a：池资源**前缀**的后缀拼接族（`GainAs<Dst>` /
    // `ConvertTo<Dst>`）的源池名须用 vendor 短名（`Life`/`Mana`），而非抽取期别名化的
    // `MaximumLife`/`MaximumMana`。vendor `CalcDefence.lua:92,1316` 与 pobr 消费侧
    // `calc/defence.rs` `format!("{src}ConvertTo{dst}")`（src=`Life`）都按短名重建查询
    // 名——若引擎产 `MaximumLifeConvertToEnergyShield` 则资源转换矩阵查不到（→转换被
    // 静默丢弃，detonate-dead 全防御 0.38x / comet PhysMaxHit 偏低根因）。`GainAs` 早
    // 前已修，`ConvertTo` 同族此前漏了——一并回退 `Maximum` 前缀，与 legacy 逐字节对齐。
    if name.contains("GainAs") || name.contains("ConvertTo") {
        if let Some(rest) = name.strip_prefix("MaximumLife") {
            out_name = format!("Life{rest}");
        } else if let Some(rest) = name.strip_prefix("MaximumMana") {
            out_name = format!("Mana{rest}");
        }
    }

    // C3 Damage 族：泛名 Damage + 作用域 flag → 专名。优先级：法术 > 投射 > 范围 >
    // 武器（与 legacy 专名映射对齐；item 实测每行至多一个作用域 flag）。
    if name == "Damage" {
        // 武器 flag（含 HIT 伴随位）→ 专名，清武器位与 HIT 位。
        const WEAPON_SPECIALS: &[(ModFlags, &str)] = &[
            (ModFlags::SPEAR, "SpearDamage"),
            (ModFlags::CROSSBOW, "CrossbowDamage"),
            (ModFlags::BOW, "BowDamage"),
            (ModFlags::MACE, "MaceDamage"),
            // PoE2 长杖位 = `Staff`（0x200000），非 `Warstaff`——与下方 WEAPON_COND
            // 同口径修正（此前用 WARSTAFF 永不命中 Staff-flag 长杖词条，QuarterstaffDamage
            // 不可达；改 STAFF 后与 legacy 专名映射对齐）。
            (ModFlags::STAFF, "QuarterstaffDamage"),
        ];
        if flags.intersects(ModFlags::SPELL) {
            out_name = "SpellDamage".to_string();
            // 默认清作用域 SPELL 位（legacy 专名不带）；但 triggered 前缀来源的
            // SPELL 位是独立来源，legacy 保留（bug #3 收敛）——keep_spell 时不清。
            if !keep_spell {
                out_flags = out_flags.without(ModFlags::SPELL);
            }
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
    let mut extra_conditions: Vec<&'static str> = Vec::new();
    if name == "Speed" {
        if flags.intersects(ModFlags::ATTACK) {
            out_name = "AttackSpeed".to_string();
            out_flags = out_flags.without(ModFlags::ATTACK);
        } else if flags.intersects(ModFlags::CAST) {
            out_name = "CastSpeed".to_string();
            out_flags = out_flags.without(ModFlags::CAST);
        }
    }

    // C3 武器作用域在非伤害名（Speed/Crit 等）上：legacy 转 `Condition(UsingX)` +
    // 清 HIT 位（保留武器位）。伤害名已由上面专名分支吸收，不进此分支。
    if !out_name.ends_with("Damage") && !out_name.starts_with("Damage") {
        // 组合武器类别（Weapon1H/2H + WeaponMelee）→ `UsingOneHandedMelee` /
        // `UsingTwoHandedMelee`（vendor `with one handed melee weapons` = Weapon1H|
        // WeaponMelee|Hit）。按**全位子集**判定且**先于**单类型——避免纯
        // `with melee weapons`（仅 WeaponMelee）被误判为单/双手 melee。`|` 非 const → let。
        let weapon_combo_cond: [(ModFlags, &str); 2] = [
            (
                ModFlags::WEAPON_1H | ModFlags::WEAPON_MELEE,
                "UsingOneHandedMelee",
            ),
            (
                ModFlags::WEAPON_2H | ModFlags::WEAPON_MELEE,
                "UsingTwoHandedMelee",
            ),
        ];
        // 单武器类型 → `Using<Type>`。PoE2 长杖（Quarterstaff）的位 = `Staff`（0x200000），
        // 旧表错用 `WARSTAFF`（0x20000000）→ 长杖攻速/暴击类全局词条漏挂条件、漏匹配。
        const WEAPON_COND: &[(ModFlags, &str)] = &[
            (ModFlags::SPEAR, "UsingSpear"),
            (ModFlags::CROSSBOW, "UsingCrossbow"),
            (ModFlags::BOW, "UsingBow"),
            (ModFlags::MACE, "UsingMace"),
            (ModFlags::STAFF, "UsingQuarterstaff"),
        ];
        if let Some((_, cond)) = weapon_combo_cond
            .iter()
            .find(|(b, _)| b.is_subset_of(out_flags))
        {
            extra_conditions.push(cond);
            out_flags = out_flags.without(ModFlags::HIT);
        } else if let Some((_, cond)) = WEAPON_COND.iter().find(|(b, _)| out_flags.intersects(*b)) {
            extra_conditions.push(cond);
            out_flags = out_flags.without(ModFlags::HIT);
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
        extra_conditions,
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
        PathBuf::from(manifest)
            .join("../../data")
            .join(pobr_data::data_version())
            .join("overlay/mod_parser_rules.json")
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

    #[test]
    fn life_convert_to_es_strips_maximum_prefix() {
        // vendor「N% of Maximum Life Converted to Energy Shield」前导名经抽取期别名→
        // `MaximumLife`，但池转换族 `ConvertTo<Dst>`/`GainAs<Dst>` 的源池名须用 vendor
        // 短名 `Life`（calc/defence.rs 按 `{src}ConvertTo{dst}` src=`Life` 重建查询名；
        // vendor CalcDefence.lua:92）。引擎须产 `LifeConvertToEnergyShield`、非
        // `MaximumLifeConvertToEnergyShield`（后者查不到转换矩阵 → 转换被静默丢弃）。
        let r = real_rules();
        let o = parse_mod_engine("5% of Maximum Life Converted to Energy Shield", &r);
        assert_eq!(o.status, ParseStatus::Parsed, "unparsed={:?}", o.unparsed);
        assert!(
            o.mods
                .iter()
                .any(|m| m.name.as_str() == "LifeConvertToEnergyShield"),
            "应产 LifeConvertToEnergyShield: {:?}",
            o.mods
        );
        assert!(
            !o.mods
                .iter()
                .any(|m| m.name.as_str() == "MaximumLifeConvertToEnergyShield"),
            "不应残留 MaximumLifeConvertToEnergyShield: {:?}",
            o.mods
        );
    }

    #[test]
    fn attack_and_cast_speed_splits_to_attack_cast() {
        // vendor「attack and cast speed」映射为泛名 `Speed`（无 attack/cast flag）；PoBR
        // 速度 bucket 按名 [AttackSpeed,CastSpeed,SkillSpeed] 聚合、不消费 bare `Speed`，
        // 故拆为 AttackSpeed + CastSpeed（对齐 legacy expand_compound，legacy.rs:735）。
        let r = real_rules();
        let o = parse_mod_engine("8% increased Attack and Cast Speed", &r);
        assert_eq!(o.status, ParseStatus::Parsed, "unparsed={:?}", o.unparsed);
        assert!(
            o.mods.iter().any(|m| m.name.as_str() == "AttackSpeed"
                && m.mod_type == ModType::Inc
                && m.value == ModValue::Number(8.0)),
            "应产 AttackSpeed Inc 8: {:?}",
            o.mods
        );
        assert!(
            o.mods
                .iter()
                .any(|m| m.name.as_str() == "CastSpeed" && m.value == ModValue::Number(8.0)),
            "应产 CastSpeed Inc 8: {:?}",
            o.mods
        );
        assert!(
            !o.mods.iter().any(|m| m.name.as_str() == "Speed"),
            "不应残留 bare Speed: {:?}",
            o.mods
        );
    }
}
