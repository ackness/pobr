//! `extract-lua --what parser-rules`：vendor `Modules/ModParser.lua` 解析规则
//! 六表（special 除外）→ `data/<版本>/overlay/mod_parser_rules.json`
//!
//! 职责切分（与既有抽取目标同形）：
//! - Lua 引导脚本（`extract_parser_rules.lua`）负责 headless 加载 + upvalue
//!   取表 + 掩码/枚举反查 + 闭包探针推断，JSONL 输出；
//! - Rust 侧负责派生字段（[`derive_pattern_meta`]：literal / anchored）、
//!   排序、计数自检（钉定 vendor commit 时容差 0）与 byte-stable 序列化。
//!
//! 与其它 `--what` 目标不同：ModParser.lua 依赖完整 PoB2 环境（ModTools /
//! Data 全量），引导走 **pob2-oracle 同款 headless 方式**——子进程 cwd 必须是
//! vendor `src/`、`LUA_PATH` 指向 `runtime/lua`，故不复用
//! [`crate::extract_lua::invoke_luajit_jsonl`]（其不设 cwd/env）。
//!
//! 另含 parser-rules drift diff（`sync-pob-catalog parser-rules-drift`）：
//! 重抽 vs 已提交 byte-diff + 分段差异摘要（任务 3）。

use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write};
use std::process::{Command, Stdio};

use pobr_data::catalog::parser_rules::{
    FlagTypeDef, FormDef, MOD_PARSER_RULES_SCHEMA, ModParserRulesDoc, NameMapDef, PhraseNamesDef,
    PhraseValueDef, PreFlagDef, RuleEffectsDef, StatMapValue, TagPhraseDef, TagTemplate,
};
use serde::{Deserialize, Serialize};

use crate::extract_lua::{ExtractLuaArgs, OverlayMeta, read_vendor_version, resolve_version_file};

/// 引导脚本内容（经 stdin 注入 luajit，二进制自包含、不依赖运行目录）。
const BOOTSTRAP_LUA: &str = include_str!("extract_parser_rules.lua");

/// 计数自检钉定的 vendor commit（`.pob2-version.txt`）。该 commit 下各段条目数
/// 必须与 [`PINNED_SECTION_COUNTS`] 完全一致；其它
/// commit（version-bump 演练）只告警不报错，保证抽取器可吸收 vendor 漂移。
pub const PINNED_VENDOR_COMMIT: &str = "2df5a7433dd2f1609e2fad8a6c3c917f923fe34f";

/// 钉定 commit 下的各段条目数（2026-06 实测；的 776/684 为估值，
/// 以实测为准——偏差记录见 blueprints/m6-extraction-report.md）。
///
/// `flag_types` = 24（vendor 主表）+ 1（路线 B 抽取期补回的 legacy `hindered`
/// 特例，见 [`normalize_legacy_consistency`]）= 25。
pub const PINNED_SECTION_COUNTS: &[(&str, usize)] = &[
    ("forms", 91),
    ("name_map", 775),
    // 202 抽取 − 死条目剔除（[`VENDOR_DEAD_FLAG_PHRASES`]）。
    ("flag_phrases", 202 - VENDOR_DEAD_FLAG_PHRASES.len()),
    ("pre_flags", 219),
    // 682 抽取 + pobr 自加（[`POBR_EXTRA_TAG_PHRASES`]）。
    ("tag_phrases", 682 + POBR_EXTRA_TAG_PHRASES.len()),
    ("suffix_types", 40),
    ("damage_types", 5),
    ("pen_types", 6),
    ("regen_types", 32),
    ("degen_types", 32),
    ("cost_types_map", 32),
    ("base_cost_types", 32),
    ("flag_types", 25),
    ("unsupported", 1),
];

/// 钉定 commit 下 formList 的 form id 全集。
pub const PINNED_FORM_IDS: &[&str] = &[
    "BASE",
    "BASECOST",
    "CHANCE",
    "DEGEN",
    "DEGENFLAT",
    "DEGENPERCENT",
    "DMG",
    "DMGATTACKS",
    "DMGBOTH",
    "DMGSPELLS",
    "DMGTHORNS",
    "DMGTHORNSBASE",
    "DOUBLED",
    "FLAG",
    "GAIN",
    "GRANTS",
    "GRANTS_GLOBAL",
    "INC",
    "LESS",
    "LOSE",
    "MORE",
    "OVERRIDE",
    "PEN",
    "RED",
    "REGENFLAT",
    "REGENPERCENT",
    "REMOVES",
    "TOTALCOST",
];

/// pobr 自加的 unsupported 项（vendor 仅 `mirrored`；`split` 来自现
/// `pobr-core::mod_parser` 硬编码，要求迁表保留并注明来源）。
///
/// B3 闸门切换暂缓行。历史欠条已**全部**解冻（与 overlay JSON 手术同步，此表是
/// regen 的单源）：deadeye 两条随 gain-as fallback 修复解冻（PR#50）；gemling
/// grenade 行随「grenade 短语 skillNameList 抢先」修复解冻（PR#53）；blood-mage
/// curse 行随 curse 机制全链验证 + 同组辅助授予等级修复解冻（诚实显形其
/// per-hit 存量低估,见 ninja_parity dot 基线注）。冻结榜只剩 legacy `split`。
const POBR_EXTRA_UNSUPPORTED: &[&str] = &["split"];

/// B3 迁表：legacy 硬写的具名 herald 条件短语（`legacy.rs` herald buff 条件族）
/// 的 engine 数据等价。vendor ModParser.lua:6437 对 aura/herald 宝石名**运行时
/// 动态**注册 `while affected by <skillname>` → `Condition AffectedBy<名去空格>`
/// ——静态抽取拿不到（headless 抽取不引导 gem 数据）。当前按 legacy 集合静态
/// 枚举；系统化 = 从 gem catalog 全量生成（C5 范畴）。
const POBR_EXTRA_TAG_PHRASES: &[(&str, &str)] = &[
    ("while affected by herald of ash", "AffectedByHeraldofAsh"),
    (
        "while affected by herald of blood",
        "AffectedByHeraldofBlood",
    ),
    ("while affected by herald of ice", "AffectedByHeraldofIce"),
    (
        "while affected by herald of plague",
        "AffectedByHeraldofPlague",
    ),
    (
        "while affected by herald of thunder",
        "AffectedByHeraldofThunder",
    ),
];

/// vendor 死条目：modFlagList 里存在、但 vendor `parseMod` 运行时**永不
/// 命中**的 flag 短语——skillNameList 的 SkillName 剥离（order=1，在 flag scan
/// 之前）抢先吃掉短语中的技能名，残留非空 → 整行不生效。裁决方法：
/// `tools/pob2-oracle/run-parsemod.sh` 实喂词条看 leftover；版本升级后 parity
/// 出现同型 over/under-apply 时用同一方法重审。
///
/// **0.5.4b（vendor 0.22.0）清空 grenade 项**：vendor gem 名注册循环新增
/// `not grantedEffect.fromItem` 排除（ModParser.lua:6423，0.21→0.22 delta），
/// `MeleeGrenadeLauncherPlayer`（name "Grenade"，fromItem）不再注册
/// skillNameList → `grenade` / `for grenade skills` 恢复为 modFlagList 的
/// **live** `SkillType.Grenade` tag（run-parsemod 实证：`15% increased
/// Cooldown Recovery Rate for Grenade Skills` → CooldownRecovery INC 15 +
/// SkillType 159，leftover 空）。0.21 时代的死条目/抢先改写（PR#53）随之撤销；
/// deadeye 3×15 CDR 树词条 under-apply（Speed 0.164 vs oracle 0.254）即此根因。
const VENDOR_DEAD_FLAG_PHRASES: &[&str] = &[];

/// vendor skillNameList 抢先条目（机制说明见 [`VENDOR_DEAD_FLAG_PHRASES`]——
/// 「保留行 + 惰性 SkillName tag」改写变体）。0.5.4b 起清空（同上 fromItem 排除）。
const VENDOR_SKILLNAME_PREEMPTED_FLAG_PHRASES: &[(&str, &str)] = &[];

/// 完整 overlay 文档（生成侧；消费侧 schema =
/// [`pobr_data::catalog::parser_rules::ModParserRulesDoc`]，serde 形状一致）。
#[derive(Debug, Serialize, Deserialize)]
pub struct ParserRulesDoc {
    /// 头部元信息（serde 落为 `_meta`，置于文件最前）。
    #[serde(rename = "_meta")]
    pub meta: OverlayMeta,
    /// 规则各段。
    #[serde(flatten)]
    pub rules: ModParserRulesDoc,
}

/// 执行抽取，返回最终（byte-stable 的）JSON 文本。
pub fn run_extract_parser_rules(args: &ExtractLuaArgs) -> io::Result<String> {
    let rows = invoke_headless_jsonl(args)?;
    let mut rules = assemble_rules(rows)?;
    finalize_rules(&mut rules);
    let meta = build_meta(args)?;
    for warning in self_check(&rules, &meta.vendor_commit)? {
        eprintln!("extract-parser-rules: {warning}");
    }
    let doc = ParserRulesDoc { meta, rules };
    let mut json = serde_json::to_string_pretty(&doc).expect("parser rules 文档序列化不应失败");
    json.push('\n');
    Ok(json)
}

/// headless 引导调用：cwd = vendor `src/`、`LUA_PATH` 指向 `runtime/lua`、
/// `CI=true`（与 `tools/pob2-oracle/run.sh` 同款约定）。
fn invoke_headless_jsonl(args: &ExtractLuaArgs) -> io::Result<Vec<serde_json::Value>> {
    let runtime = args.vendor_root.join("../runtime/lua");
    let lua_path = format!("{r}/?.lua;{r}/?/init.lua;./?.lua;;", r = runtime.display());
    let mut child = Command::new(&args.luajit)
        .arg("-") // 从 stdin 读脚本
        .arg(&args.vendor_root)
        .current_dir(&args.vendor_root)
        .env("LUA_PATH", lua_path)
        .env("CI", "true")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "无法启动 luajit（{}）：{error}；请安装 luajit 或用 --luajit / POBR_LUAJIT 指定路径",
                    args.luajit.display()
                ),
            )
        })?;

    child
        .stdin
        .take()
        .expect("stdin 已配置为 piped")
        .write_all(BOOTSTRAP_LUA.as_bytes())?;

    let output = child.wait_with_output()?;
    let stderr_text = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "parser-rules 引导脚本执行失败（exit: {:?}）：{}",
            output.status.code(),
            stderr_text.trim()
        )));
    }
    for line in stderr_text.lines() {
        eprintln!("extract-parser-rules(lua): {line}");
    }

    let stdout_text = String::from_utf8(output.stdout).map_err(io::Error::other)?;
    let mut rows = Vec::new();
    for line in stdout_text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let row: serde_json::Value = serde_json::from_str(line).map_err(|error| {
            io::Error::other(format!(
                "引导脚本输出了非法 JSONL 行：{error}；行内容：{line}"
            ))
        })?;
        rows.push(row);
    }
    Ok(rows)
}

/// JSONL 行按 `section` 分发反序列化为各段 typed defs。
fn assemble_rules(rows: Vec<serde_json::Value>) -> io::Result<ModParserRulesDoc> {
    let mut doc = ModParserRulesDoc::default();
    for mut row in rows {
        let Some(object) = row.as_object_mut() else {
            return Err(io::Error::other("JSONL 行不是对象"));
        };
        let Some(section) = object.remove("section").and_then(|s| match s {
            serde_json::Value::String(s) => Some(s),
            _ => None,
        }) else {
            return Err(io::Error::other("JSONL 行缺少 section 字段"));
        };
        let context = |error: serde_json::Error| {
            io::Error::other(format!("section `{section}` 行反序列化失败：{error}"))
        };
        match section.as_str() {
            "forms" => doc.forms.push(from_row::<FormDef>(row).map_err(context)?),
            "name_map" => doc
                .name_map
                .push(from_row::<NameMapDef>(row).map_err(context)?),
            "flag_phrases" => doc.flag_phrases.push(from_row(row).map_err(context)?),
            "pre_flags" => doc
                .pre_flags
                .push(from_row::<PreFlagDef>(row).map_err(context)?),
            "tag_phrases" => doc
                .tag_phrases
                .push(from_row::<TagPhraseDef>(row).map_err(context)?),
            "suffix_types" => doc
                .suffix_types
                .push(from_row::<PhraseValueDef>(row).map_err(context)?),
            "damage_types" => doc.damage_types.push(from_row(row).map_err(context)?),
            "pen_types" => doc.pen_types.push(from_row(row).map_err(context)?),
            "regen_types" => doc
                .regen_types
                .push(from_row::<PhraseNamesDef>(row).map_err(context)?),
            "degen_types" => doc.degen_types.push(from_row(row).map_err(context)?),
            "cost_types_map" => doc.cost_types_map.push(from_row(row).map_err(context)?),
            "base_cost_types" => doc.base_cost_types.push(from_row(row).map_err(context)?),
            "flag_types" => doc
                .flag_types
                .push(from_row::<FlagTypeDef>(row).map_err(context)?),
            "unsupported" => {
                #[derive(Deserialize)]
                struct Row {
                    phrase: String,
                }
                doc.unsupported
                    .push(from_row::<Row>(row).map_err(context)?.phrase);
            }
            other => {
                return Err(io::Error::other(format!("未知 JSONL section：{other}")));
            }
        }
    }
    Ok(doc)
}

fn from_row<T: serde::de::DeserializeOwned>(row: serde_json::Value) -> serde_json::Result<T> {
    serde_json::from_value(row)
}

/// 派生字段（literal / anchored）+ 各段排序 + pobr 自加 unsupported 项。
fn finalize_rules(doc: &mut ModParserRulesDoc) {
    for form in &mut doc.forms {
        let (literal, anchored) = derive_pattern_meta(&form.pattern);
        form.literal = literal;
        form.anchored = anchored;
    }
    for entry in &mut doc.pre_flags {
        let (literal, anchored) = derive_pattern_meta(&entry.pattern);
        entry.literal = literal;
        entry.anchored = anchored;
    }
    // pobr 自加 tag 短语（B3 迁表，见 [`POBR_EXTRA_TAG_PHRASES`]）——在 derive
    // 循环前插入，literal/anchored 与抽取条目走同一派生。
    for (phrase, var) in POBR_EXTRA_TAG_PHRASES {
        let mut fields = std::collections::BTreeMap::new();
        fields.insert("var".to_string(), StatMapValue::Text((*var).to_string()));
        doc.tag_phrases.push(TagPhraseDef {
            pattern: (*phrase).to_string(),
            literal: None,
            anchored: false,
            effects: RuleEffectsDef {
                tags: vec![TagTemplate {
                    tag_type: "Condition".to_string(),
                    fields,
                }],
                ..Default::default()
            },
            inferred: false,
            handler_id: None,
        });
    }
    for entry in &mut doc.tag_phrases {
        let (literal, anchored) = derive_pattern_meta(&entry.pattern);
        entry.literal = literal;
        entry.anchored = anchored;
    }

    // vendor 死条目剔除（B3，见 [`VENDOR_DEAD_FLAG_PHRASES`]）：engine 对这些短语
    // 不再产 flag/tag → 词条残留 unparsed → 与 vendor「extra 非空整行不生效」对齐。
    doc.flag_phrases
        .retain(|e| !VENDOR_DEAD_FLAG_PHRASES.contains(&e.phrase.as_str()));

    // skillNameList 抢先改写（见 [`VENDOR_SKILLNAME_PREEMPTED_FLAG_PHRASES`]）：
    // payload 由 SkillType 改为惰性 SkillName（vendor 运行时实际产物）。
    for entry in &mut doc.flag_phrases {
        if let Some((_, skill_name)) = VENDOR_SKILLNAME_PREEMPTED_FLAG_PHRASES
            .iter()
            .find(|(p, _)| *p == entry.phrase)
        {
            entry.effects.tags = vec![TagTemplate {
                tag_type: "SkillName".into(),
                fields: [(
                    "skillName".to_string(),
                    StatMapValue::Text((*skill_name).to_string()),
                )]
                .into_iter()
                .collect(),
            }];
        }
    }

    // 排序纪律：Lua pairs 无序 → 每段按 pattern/phrase 字典序。
    doc.forms.sort_by(|a, b| a.pattern.cmp(&b.pattern));
    doc.name_map.sort_by(|a, b| a.phrase.cmp(&b.phrase));
    doc.flag_phrases.sort_by(|a, b| a.phrase.cmp(&b.phrase));
    doc.pre_flags.sort_by(|a, b| a.pattern.cmp(&b.pattern));
    doc.tag_phrases.sort_by(|a, b| a.pattern.cmp(&b.pattern));
    doc.suffix_types.sort_by(|a, b| a.phrase.cmp(&b.phrase));
    doc.damage_types.sort_by(|a, b| a.phrase.cmp(&b.phrase));
    doc.pen_types.sort_by(|a, b| a.phrase.cmp(&b.phrase));
    doc.regen_types.sort_by(|a, b| a.phrase.cmp(&b.phrase));
    doc.degen_types.sort_by(|a, b| a.phrase.cmp(&b.phrase));
    doc.cost_types_map.sort_by(|a, b| a.phrase.cmp(&b.phrase));
    doc.base_cost_types.sort_by(|a, b| a.phrase.cmp(&b.phrase));
    doc.flag_types.sort_by(|a, b| a.phrase.cmp(&b.phrase));
    doc.unsupported.sort();
    doc.unsupported_pobr_extra = POBR_EXTRA_UNSUPPORTED
        .iter()
        .map(|s| s.to_string())
        .collect();

    // .3 路线 B：抽取期 vendor→PoBR 词表归一（别名 rename + 聚合展开 +
    // DamageType tag）。引擎直接产 PoBR StatId，下游零改动、无运行期翻译层。
    normalize_name_map_to_pobr(doc);
}

/// .3 路线 B 抽取期归一：把 `name_map` 的 vendor ModName 归一为 PoBR canonical
/// StatId（别名表 [`VENDOR_NAME_ALIASES`]）+ 按短语展开聚合名
/// （[`AGGREGATE_EXPANSION`]）。**源真理 =
/// `data/overlay-common/vendor_name_aliases.json`**（本表的 real-rename 子集
/// 与之一致）；结构归一规格见 `blueprints/m6-alias-table.md` §3。
///
/// 设计：仅改 `names`，不增删条目（保 [`PINNED_SECTION_COUNTS`] 计数）。
/// DamageType tag（C5，按最终名挂、避免被 suffix 形变名误挂）、DMG 族名
/// （`PhysicalMin`→`PhysicalDamageMin`）、damage flag→专名（C3）、
/// PerStat→Multiplier（C2）由引擎归一（组合期产物，非静态 name_map 可表达）。
fn normalize_name_map_to_pobr(doc: &mut ModParserRulesDoc) {
    let alias: std::collections::HashMap<&str, &str> =
        VENDOR_NAME_ALIASES.iter().copied().collect();
    let aggregate: std::collections::HashMap<&str, &[&str]> =
        AGGREGATE_EXPANSION.iter().copied().collect();

    for entry in &mut doc.name_map {
        // 1. 聚合短语展开优先（整组替换 names）。
        if let Some(children) = aggregate.get(entry.phrase.as_str()) {
            entry.names = children.iter().map(|s| s.to_string()).collect();
        } else {
            // 2. 逐名别名归一（real-rename 生效、identity 恒等）。
            for n in &mut entry.names {
                if let Some(pobr) = alias.get(n.as_str()) {
                    *n = (*pobr).to_string();
                }
            }
        }
        // 3. 内嵌作用域词被专名吸收的短语：清 vendor 残留 flag（legacy parse_name
        //    把 `critical spell damage bonus` 整体映射为 `CriticalStrikeMultiplier`
        //    不带 Spell flag；vendor 把 `spell` 拆为 flag）。仅这一确证短语。
        if FLAGLESS_NAME_PHRASES.contains(&entry.phrase.as_str()) {
            entry.effects.flags.clear();
        }
        // 4. 专名归一（C3）：legacy parse_name 把 `attack damage` 整体映射为专名
        //    `AttackDamage`（vendor `Damage`+Attack flag）。改名清 flag。
        if let Some(special) = SPECIAL_NAME_PHRASES
            .iter()
            .find(|(p, _)| *p == entry.phrase.as_str())
        {
            entry.names = vec![special.1.to_string()];
            entry.effects.flags.clear();
        }
    }

    // 武器作用域 keyword→flag（C3）：vendor flag_phrases 把 `with bow skills` 记为
    // keywordFlags Bow（PoBR 无武器 keyword 位会被丢），legacy 折为 ModFlag(Hit,Bow)
    // 并由专名吸收。归一为 `flags:[Hit,<Weapon>]`（与 `with bows` 同形），引擎 C3
    // 据此产 BowDamage 等专名。
    for entry in &mut doc.flag_phrases {
        let weapon = entry
            .effects
            .keyword_flags
            .iter()
            .find(|k| WEAPON_KEYWORDS.contains(&k.as_str()))
            .cloned();
        if let Some(w) = weapon {
            entry.effects.keyword_flags.retain(|k| k != &w);
            entry.effects.flags = vec!["Hit".to_string(), w];
        }
    }

    normalize_legacy_consistency(doc);
}

/// .3 路线 B（D-T8 第二波 2a）：把三处 vendor↔legacy 形态差异从抽取期归一，
/// 使引擎产 legacy-一致值（dual-run C1 DIFF=0/OLD_ONLY=0），并保留 `data/` 由工具
/// 再生的不变式（不再手改 `mod_parser_rules.json`）。三处均为「4 真 bug」收敛项：
///
/// 1. `from equipped focus`（flag_phrases）：vendor 额外挂 `Condition(UsingFocus)`，
///    legacy 仅按 `SlotName(Weapon 2)` 作用域生效——去掉冗余 UsingFocus 条件。
/// 2. helmet PerStat/StatThreshold 的 `stat` 字段（tag_phrases）：vendor 写
///    `*OnHelmet`（大写 H），legacy 注册的统计名是 `*Onhelmet`（小写 h，源自
///    slotName 小写化路径）——降为小写 h 与统计名对齐。
/// 3. `hindered`（flag_types）：legacy `parseEnemyInner` 特例把它当 `Condition:Hindered`
///    flag_type，vendor 主表无此条——补回（与 2a 收敛同口径，保段计数靠
///    [`PINNED_SECTION_COUNTS`] 钉值同步）。
fn normalize_legacy_consistency(doc: &mut ModParserRulesDoc) {
    // 1. focus：去 Condition(UsingFocus)，仅留 SlotName 作用域。
    for entry in &mut doc.flag_phrases {
        if entry.phrase == FOCUS_PHRASE {
            entry.effects.tags.retain(|t| {
                !(t.tag_type == "Condition"
                    && matches!(
                        t.fields.get("var"),
                        Some(StatMapValue::Text(v)) if v == "UsingFocus"
                    ))
            });
        }
    }

    // 2. helmet stat：`*OnHelmet` → `*Onhelmet`（仅末段 `OnHelmet`，避免误改
    //    `OnBody Armour` 等其它 slot 名）。
    for entry in &mut doc.tag_phrases {
        for tag in &mut entry.effects.tags {
            if let Some(StatMapValue::Text(stat)) = tag.fields.get_mut("stat")
                && let Some(prefix) = stat.strip_suffix("OnHelmet")
            {
                *stat = format!("{prefix}Onhelmet");
            }
        }
    }

    // 3. hindered flag_type：补回 legacy 特例（vendor 主表缺，2a 收敛同口径）。
    if !doc
        .flag_types
        .iter()
        .any(|e| e.phrase == HINDERED_FLAG_TYPE_PHRASE)
    {
        doc.flag_types.push(FlagTypeDef {
            phrase: HINDERED_FLAG_TYPE_PHRASE.to_string(),
            condition: Some("Condition:Hindered".to_string()),
            mod_def: None,
        });
        doc.flag_types.sort_by(|a, b| a.phrase.cmp(&b.phrase));
    }
}

/// focus 作用域短语（去 UsingFocus 冗余条件）。
const FOCUS_PHRASE: &str = "from equipped focus";

/// legacy `parseEnemyInner` 特例补回的 flag_type 短语。
const HINDERED_FLAG_TYPE_PHRASE: &str = "hindered";

/// 武器类型名（在 flag_phrases 的 keyword_flags 里出现时归一为 ModFlag）。
const WEAPON_KEYWORDS: &[&str] = &[
    "Bow",
    "Crossbow",
    "Spear",
    "Mace",
    "Quarterstaff",
    "Warstaff",
    "Sword",
    "Claw",
    "Wand",
    "Staff",
];

/// vendor name_map 带作用域 flag、但 legacy 把作用域内嵌进专名（无 flag）的短语。
/// 抽取期清其 flag 以与 legacy 对齐（C3 内嵌作用域子类）。
const FLAGLESS_NAME_PHRASES: &[&str] = &["critical spell damage bonus"];

/// vendor `Damage`+作用域 flag、legacy 用独立专名的短语（C3）：抽取期改名清 flag。
const SPECIAL_NAME_PHRASES: &[(&str, &str)] = &[("attack damage", "AttackDamage")];

/// vendor→PoBR 别名表（real-rename 20 + identity 56，源真理 =
/// `vendor_name_aliases.json`）。抽取期对 `name_map` 每个 ModName 套此表。
/// identity 项可省略（套表恒等），此处仅列 20 real-rename。
const VENDOR_NAME_ALIASES: &[(&str, &str)] = &[
    ("ChaosResist", "ChaosResistance"),
    ("ChaosResistMax", "MaximumChaosResistance"),
    ("ColdResist", "ColdResistance"),
    ("ColdResistMax", "MaximumColdResistance"),
    ("CritChance", "CriticalStrikeChance"),
    ("CritMultiplier", "CriticalStrikeMultiplier"),
    ("Dex", "Dexterity"),
    ("ElementalResistMax", "MaximumAllElementalResistances"),
    ("EnemyBleedDuration", "BleedDuration"),
    ("EnemyFreezeBuildup", "FreezeBuildup"),
    ("EnemyIgniteDuration", "IgniteDuration"),
    ("EnemyPoisonDuration", "PoisonDuration"),
    ("FireResist", "FireResistance"),
    ("FireResistMax", "MaximumFireResistance"),
    ("Int", "Intelligence"),
    ("Life", "MaximumLife"),
    ("LightningResist", "LightningResistance"),
    ("LightningResistMax", "MaximumLightningResistance"),
    ("Mana", "MaximumMana"),
    ("Str", "Strength"),
];

/// 聚合短语 → PoBR 子名集（C1，legacy `resolve_names` 同表）。vendor name_map
/// 把这些短语解析为单一聚合名 / 含 vendor 组合名（`All`/`StrInt`），PoBR 下游无
/// ModStore 展开层，故抽取期展开为 PoBR 子名。
const AGGREGATE_EXPANSION: &[(&str, &[&str])] = &[
    (
        "all elemental resistances",
        &["FireResistance", "ColdResistance", "LightningResistance"],
    ),
    // vendor `["all resistances"] = { "ElementalResist", "ChaosResist" }`（ModParser.lua:288）。
    // PoBR 玩家侧抗性 calc 只读离散 `FireResistance`/`ColdResistance`/`LightningResistance`
    // （+ChaosResistance），不识别聚合名 `ElementalResist`（仅敌侧消费）→ 不展开则元素
    // 抗部分静默丢弃。含混沌：vendor 的 `ChaosResist` → PoBR `ChaosResistance`。
    (
        "all resistances",
        &[
            "FireResistance",
            "ColdResistance",
            "LightningResistance",
            "ChaosResistance",
        ],
    ),
    ("all attributes", &["Strength", "Dexterity", "Intelligence"]),
    ("attributes", &["Strength", "Dexterity", "Intelligence"]),
    ("strength and intelligence", &["Strength", "Intelligence"]),
    ("strength and dexterity", &["Strength", "Dexterity"]),
    ("dexterity and intelligence", &["Dexterity", "Intelligence"]),
    // vendor `["skill speed"] = { "Speed", "WarcrySpeed", "TotemPlacementSpeed" }`
    // （ModParser.lua:770）。裸 `Speed` → PoBR 速度桶名 `SkillSpeed`；WarcrySpeed /
    // TotemPlacementSpeed 保留原名扇出（存量 #9 起 WarcrySpeed 有真消费方 =
    // `pobr-core::calc::warcry` 的喊叫时间，CalcOffence.lua:350-359；
    // TotemPlacementSpeed 仍为惰性作用域名）。此前只落 `SkillSpeed` 单名，
    // 「N% increased Skill Speed」文本对 warcry 喊叫速度静默丢失。
    (
        "skill speed",
        &["SkillSpeed", "WarcrySpeed", "TotemPlacementSpeed"],
    ),
];

/// 抽取自检：钉定 commit 下计数 / form id 集容差 0（Err）；
/// 其它 commit 只产出告警（演练时吸收 vendor 漂移）。各段键唯一性恒检查。
fn self_check(doc: &ModParserRulesDoc, vendor_commit: &str) -> io::Result<Vec<String>> {
    let mut warnings = Vec::new();
    let counts: &[(&str, usize)] = &[
        ("forms", doc.forms.len()),
        ("name_map", doc.name_map.len()),
        ("flag_phrases", doc.flag_phrases.len()),
        ("pre_flags", doc.pre_flags.len()),
        ("tag_phrases", doc.tag_phrases.len()),
        ("suffix_types", doc.suffix_types.len()),
        ("damage_types", doc.damage_types.len()),
        ("pen_types", doc.pen_types.len()),
        ("regen_types", doc.regen_types.len()),
        ("degen_types", doc.degen_types.len()),
        ("cost_types_map", doc.cost_types_map.len()),
        ("base_cost_types", doc.base_cost_types.len()),
        ("flag_types", doc.flag_types.len()),
        ("unsupported", doc.unsupported.len()),
    ];
    let pinned: BTreeMap<&str, usize> = PINNED_SECTION_COUNTS.iter().copied().collect();
    let is_pinned_commit = vendor_commit == PINNED_VENDOR_COMMIT;
    for (section, actual) in counts {
        let expected = pinned[section];
        if *actual != expected {
            let message = format!(
                "section `{section}` 条目数 {actual} ≠ 钉定值 {expected}（vendor {vendor_commit}）"
            );
            if is_pinned_commit {
                return Err(io::Error::other(format!("抽取自检失败：{message}")));
            }
            warnings.push(message);
        }
    }

    let form_ids: BTreeSet<&str> = doc.forms.iter().map(|f| f.form.as_str()).collect();
    let pinned_forms: BTreeSet<&str> = PINNED_FORM_IDS.iter().copied().collect();
    if form_ids != pinned_forms {
        let message = format!(
            "form id 集与钉定集不一致：多出 {:?}，缺少 {:?}",
            form_ids.difference(&pinned_forms).collect::<Vec<_>>(),
            pinned_forms.difference(&form_ids).collect::<Vec<_>>()
        );
        if is_pinned_commit {
            return Err(io::Error::other(format!("抽取自检失败：{message}")));
        }
        warnings.push(message);
    }

    // 键唯一性（任何 commit 下都硬性）
    for (section, keys) in [
        (
            "forms",
            doc.forms
                .iter()
                .map(|f| f.pattern.as_str())
                .collect::<Vec<_>>(),
        ),
        (
            "name_map",
            doc.name_map.iter().map(|f| f.phrase.as_str()).collect(),
        ),
        (
            "flag_phrases",
            doc.flag_phrases.iter().map(|f| f.phrase.as_str()).collect(),
        ),
        (
            "pre_flags",
            doc.pre_flags.iter().map(|f| f.pattern.as_str()).collect(),
        ),
        (
            "tag_phrases",
            doc.tag_phrases.iter().map(|f| f.pattern.as_str()).collect(),
        ),
    ] {
        let unique: BTreeSet<&&str> = keys.iter().collect();
        if unique.len() != keys.len() {
            return Err(io::Error::other(format!(
                "抽取自检失败：section `{section}` 存在重复键"
            )));
        }
    }

    // 闭包推断统计（报告输入；handler 预算监控走全局台账，这里仅提示）
    let inferred = doc.pre_flags.iter().filter(|e| e.inferred).count()
        + doc.tag_phrases.iter().filter(|e| e.inferred).count();
    let handlers = doc
        .pre_flags
        .iter()
        .filter(|e| e.handler_id.is_some())
        .count()
        + doc
            .tag_phrases
            .iter()
            .filter(|e| e.handler_id.is_some())
            .count();
    warnings.push(format!(
        "闭包条目统计：探针推断成功 {inferred} / handler 兜底 {handlers}（预算 ≤15，全局 <100 台账见 00-index §3.3）"
    ));
    if handlers > 15 {
        warnings.push(format!(
            "警告：handler 兜底条目 {handlers} 超出蓝图预估 ≤15"
        ));
    }
    Ok(warnings)
}

/// 从 Lua pattern 派生（最长字面量片段, 是否 `^` 锚定）。
///
/// 字面量片段语义：去掉锚、捕获括号、字符类（`[...]`、`%d` 等单字符类）与
/// 被量词（`+ - * ?`）作用的可变字符后，剩余连续字面字符的最长一段
/// （aho-corasick 预过滤用；引擎对 `None`/过短 literal 走 always-check 桶）。
/// pattern 均为小写 ASCII（vendor scan 对输入 `lower()` 后匹配）。
pub fn derive_pattern_meta(pattern: &str) -> (Option<String>, bool) {
    let bytes = pattern.as_bytes();
    let anchored = pattern.starts_with('^');
    let mut runs: Vec<String> = Vec::new();
    let mut cur = String::new();
    let flush = |cur: &mut String, runs: &mut Vec<String>| {
        if !cur.is_empty() {
            runs.push(std::mem::take(cur));
        }
    };
    let is_quantifier = |b: u8| matches!(b, b'+' | b'-' | b'*' | b'?');
    let mut i = usize::from(anchored);
    let n = bytes.len();
    while i < n {
        match bytes[i] {
            b'%' => {
                if i + 1 >= n {
                    i += 1;
                    continue;
                }
                let escaped = bytes[i + 1];
                if escaped.is_ascii_alphanumeric() {
                    // 类元素（%d / %a / %D…）：单字符通配，断开字面 run
                    flush(&mut cur, &mut runs);
                    i += 2;
                    if i < n && is_quantifier(bytes[i]) {
                        i += 1;
                    }
                } else if i + 2 < n && is_quantifier(bytes[i + 2]) {
                    // 转义标点带量词：该字符出现次数可变，不入 run
                    flush(&mut cur, &mut runs);
                    i += 3;
                } else {
                    // 转义标点（%% / %- / %.）= 字面字符
                    cur.push(escaped as char);
                    i += 2;
                }
            }
            b'[' => {
                // 字符类：跳到配对 `]`（含 `%]` 转义），断开 run，吞后续量词
                flush(&mut cur, &mut runs);
                i += 1;
                while i < n {
                    if bytes[i] == b'%' {
                        i += 2;
                    } else if bytes[i] == b']' {
                        i += 1;
                        break;
                    } else {
                        i += 1;
                    }
                }
                if i < n && is_quantifier(bytes[i]) {
                    i += 1;
                }
            }
            b'(' | b')' => {
                flush(&mut cur, &mut runs);
                i += 1;
            }
            b'.' => {
                flush(&mut cur, &mut runs);
                i += 1;
                if i < n && is_quantifier(bytes[i]) {
                    i += 1;
                }
            }
            b'$' if i == n - 1 => {
                flush(&mut cur, &mut runs);
                i += 1;
            }
            b if is_quantifier(b) && !cur.is_empty() => {
                // 量词作用于前一个字面字符：弹出该字符并断开 run
                cur.pop();
                flush(&mut cur, &mut runs);
                i += 1;
            }
            b => {
                cur.push(b as char);
                i += 1;
            }
        }
    }
    flush(&mut cur, &mut runs);
    // 最长 run；等长取最先出现者（确定性 tie-break）
    let literal = runs
        .into_iter()
        .fold(None::<String>, |best, run| match best {
            Some(b) if run.len() > b.len() => Some(run),
            None => Some(run),
            other => other,
        });
    (literal.filter(|l| !l.is_empty()), anchored)
}

/// 读取 vendor 版本文件并构建 `_meta`。
fn build_meta(args: &ExtractLuaArgs) -> io::Result<OverlayMeta> {
    let (commit, subject) = read_vendor_version(&resolve_version_file(args))?;
    let mut regen = String::from(
        "cargo run -p sync-pob-catalog -- extract-lua --what parser-rules --vendor-root vendor/PathOfBuilding-PoE2/src",
    );
    if let Some(out) = &args.out_for_meta {
        regen.push_str(&format!(" --out {out}"));
    }
    Ok(OverlayMeta {
        schema: MOD_PARSER_RULES_SCHEMA.to_string(),
        generator: "sync-pob-catalog extract-lua".to_string(),
        vendor: "PathOfBuilding-PoE2".to_string(),
        vendor_commit: commit,
        vendor_commit_subject: subject,
        extracted_files: vec!["Modules/ModParser.lua".to_string()],
        regen_command: regen,
    })
}

// parser-rules drift diff（重抽 vs 已提交）

/// drift diff 结果：byte 等价与人类可读差异摘要。
#[derive(Debug)]
pub struct ParserRulesDrift {
    /// 重抽产物与已提交文件 byte 等价。
    pub identical: bool,
    /// 差异摘要行（byte 等价时为空）。
    pub lines: Vec<String>,
}

/// 已提交文本 vs 重抽文本的 drift diff：先 byte 比较，不等时按段给出
/// 增/删/改计数与样例键（每类至多 5 个）。
pub fn diff_parser_rules(committed: &str, regenerated: &str) -> io::Result<ParserRulesDrift> {
    if committed == regenerated {
        return Ok(ParserRulesDrift {
            identical: true,
            lines: Vec::new(),
        });
    }
    let parse = |text: &str, label: &str| -> io::Result<serde_json::Value> {
        serde_json::from_str(text)
            .map_err(|error| io::Error::other(format!("{label} 不是合法 JSON：{error}")))
    };
    let committed_doc = parse(committed, "已提交文件")?;
    let regenerated_doc = parse(regenerated, "重抽产物")?;

    let mut lines = Vec::new();
    if committed_doc.get("_meta") != regenerated_doc.get("_meta") {
        lines.push("[_meta] 头部不一致（vendor commit / regen_command 漂移？）".to_string());
    }

    let empty = serde_json::Map::new();
    let committed_map = committed_doc.as_object().unwrap_or(&empty);
    let regenerated_map = regenerated_doc.as_object().unwrap_or(&empty);
    let sections: BTreeSet<&String> = committed_map
        .keys()
        .chain(regenerated_map.keys())
        .filter(|k| *k != "_meta")
        .collect();
    for section in sections {
        let by_key = |doc: &serde_json::Map<String, serde_json::Value>| -> BTreeMap<String, serde_json::Value> {
            let mut map = BTreeMap::new();
            if let Some(serde_json::Value::Array(items)) = doc.get(section.as_str()) {
                for (index, item) in items.iter().enumerate() {
                    let key = item
                        .get("pattern")
                        .or_else(|| item.get("phrase"))
                        .and_then(|v| v.as_str())
                        .map(str::to_string)
                        .unwrap_or_else(|| match item {
                            serde_json::Value::String(s) => s.clone(),
                            _ => format!("#{index}"),
                        });
                    map.insert(key, item.clone());
                }
            }
            map
        };
        let old = by_key(committed_map);
        let new = by_key(regenerated_map);
        let added: Vec<&String> = new.keys().filter(|k| !old.contains_key(*k)).collect();
        let removed: Vec<&String> = old.keys().filter(|k| !new.contains_key(*k)).collect();
        let changed: Vec<&String> = old
            .iter()
            .filter(|(k, v)| new.get(*k).is_some_and(|nv| nv != *v))
            .map(|(k, _)| k)
            .collect();
        if added.is_empty() && removed.is_empty() && changed.is_empty() {
            continue;
        }
        let sample = |keys: &[&String]| -> String {
            let mut text = keys
                .iter()
                .take(5)
                .map(|k| format!("`{k}`"))
                .collect::<Vec<_>>()
                .join(", ");
            if keys.len() > 5 {
                text.push_str(", …");
            }
            text
        };
        lines.push(format!(
            "[{section}] +{} -{} ~{}{}{}{}",
            added.len(),
            removed.len(),
            changed.len(),
            if added.is_empty() {
                String::new()
            } else {
                format!("；新增 {}", sample(&added))
            },
            if removed.is_empty() {
                String::new()
            } else {
                format!("；删除 {}", sample(&removed))
            },
            if changed.is_empty() {
                String::new()
            } else {
                format!("；变更 {}", sample(&changed))
            },
        ));
    }
    if lines.is_empty() {
        lines.push("byte 不等但结构 diff 为空（空白/键序差异？请直接 diff 文件）".to_string());
    }
    Ok(ParserRulesDrift {
        identical: false,
        lines,
    })
}

#[cfg(test)]
mod tests {
    use super::derive_pattern_meta;

    /// 例：`^` 锚 + `%%` 转义的 literal 派生。
    #[test]
    fn literal_for_increased_form() {
        let (literal, anchored) = derive_pattern_meta("^(%d+)%% increased");
        assert_eq!(literal.as_deref(), Some("% increased"));
        assert!(anchored);
    }

    /// 量词作用的字面字符（`s?`）不入 run；非锚定 pattern。
    #[test]
    fn literal_drops_quantified_char() {
        let (literal, anchored) = derive_pattern_meta("costs? ([%+%-]?%d+)");
        assert_eq!(literal.as_deref(), Some("cost"));
        assert!(!anchored);
    }

    /// 纯字面 pattern：整体即 literal。
    #[test]
    fn literal_for_plain_pattern() {
        let (literal, anchored) = derive_pattern_meta("is doubled");
        assert_eq!(literal.as_deref(), Some("is doubled"));
        assert!(!anchored);
    }

    /// 字符类（`[...]`）断开 run；例。
    #[test]
    fn literal_breaks_on_char_class() {
        let (literal, anchored) = derive_pattern_meta("^minions [cthd][ae][ukva][sel]e? ");
        assert_eq!(literal.as_deref(), Some("minions "));
        assert!(anchored);
    }

    /// 全类元素 pattern（无任何字面段）→ None。
    #[test]
    fn literal_none_for_all_class_pattern() {
        let (literal, anchored) = derive_pattern_meta("^(%d+)");
        assert_eq!(literal, None);
        assert!(anchored);
    }

    /// `.`/`.-` 通配断开 run；`$` 尾锚不入 literal。
    #[test]
    fn literal_handles_wildcard_and_tail_anchor() {
        let (literal, _) = derive_pattern_meta("^regenerate ([%d%.]+) (.-) per second$");
        assert_eq!(literal.as_deref(), Some("regenerate "));
    }

    /// drift diff：byte 等价 → identical；条目变更 → 分段摘要。
    #[test]
    fn drift_diff_reports_sections() {
        let committed = r#"{"_meta":{"schema":"mod_parser_rules/v1"},"forms":[{"pattern":"a","form":"INC"},{"pattern":"b","form":"RED"}]}"#;
        let same = super::diff_parser_rules(committed, committed).unwrap();
        assert!(same.identical);

        let regenerated = r#"{"_meta":{"schema":"mod_parser_rules/v1"},"forms":[{"pattern":"a","form":"MORE"},{"pattern":"c","form":"RED"}]}"#;
        let drift = super::diff_parser_rules(committed, regenerated).unwrap();
        assert!(!drift.identical);
        let joined = drift.lines.join("\n");
        assert!(joined.contains("[forms]"), "应有 forms 段摘要：{joined}");
        assert!(joined.contains("+1 -1 ~1"), "增删改计数应各为 1：{joined}");
        assert!(joined.contains("`c`") && joined.contains("`b`") && joined.contains("`a`"));
    }
}
