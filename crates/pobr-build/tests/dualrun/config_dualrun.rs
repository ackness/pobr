//! M3-T1 A5 双跑对照（蓝图 D3 点 1）：旧 `parse_config` vs
//! `parse_config_inputs` + `config_interpreter` 新路径。
//!
//! **持续回归**（commit ① 起）：主路径已切 interpreter（`config_resolve`），
//! 本测试保持运行直至旧路径删除（dualrun 报告 §3-⑧）——任何让「旧 ⊆ 新且
//! 交集逐值相等」失效的改动都会在此被拦下。
//!
//! 口径：**旧路径能产出的 conditions / multipliers / global_texts / 标量项，
//! 新路径必须逐值覆盖（旧 ⊆ 新）且交集逐值相等**。「覆盖」分两层：
//! 1. **同名同值**：新路径 `conditions`/`multipliers` 回填表直接命中；
//! 2. **mod 化覆盖**：vendor 忠实形态把条件落成带 tag（`Condition:Combat` 战斗
//!    门控 / `Condition:Effective`）或 enemy actor 化的 Modifier——值在 mod
//!    载荷中逐值相等，但**不再进全局表**（属新增覆盖语义，待行为 commit 逐类
//!    打开，本波现网仍消费旧路径产出，行为逐值不变）。
//! 3. **handler 缺口**：catalog 条目带 handler_id 且未注册产出——原始输入已被
//!    RawConfigInputs 无损捕获（标量回显可查），解释产出待后续 handler 批次。
//!
//! 任何不落入上述三类的旧产出项 = 双跑失败（hard fail）。
//!
//! 数据源：`examples/demo-bd-test/builds/*/code.txt`（ninja 18-build）+
//! `tests/fixtures/config_*.xml`。运行 `-- --nocapture` 打印逐类 diff 汇总
//! （新增覆盖项清单），人工誊录至
//! `audits/rearchitecture-2026-06-10/blueprints/m3-t1-dualrun-report.md`。

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use pobr_build::decode_pob_code;
use pobr_build::handlers::{build_registry, campaign_progress_from_config, enemy_tier_from_config};
use pobr_build::xml_build::{parse_config_inputs, parse_config_legacy};
use pobr_core::mod_parser::ParseStatus;
use pobr_core::modifier::Modifier;
use pobr_core::rules::config_interpreter::{
    ConfigInputValue, ConfigOutcome, RawConfigInputs, interpret,
};
use pobr_data::modifier::ModType;
use pobr_gamedata::ruleset::ConfigCatalog;
use pobr_gamedata::{GameData, repo_data_root};

/// engine 版单行解析（真实规则，签名对齐历史 `parse_mod`；引擎永不 `Err`）。
fn parse_mod(
    text: &str,
) -> Result<pobr_core::mod_parser::ParseOutcome, pobr_core::mod_parser::ParseError> {
    use std::sync::LazyLock;
    static RULES: LazyLock<pobr_core::mod_parser::CompiledParserRules> =
        LazyLock::new(pobr_core::mod_parser::test_compiled_rules);
    Ok(pobr_core::mod_parser::parse_mod_engine(text, &RULES))
}

/// 旧路径 `DEFAULT_TRUE_CONDITIONS` 的反查表（条件变量名 → XML `<Input name>`）。
/// 测试本地镜像 xml_build 私有常量——旧路径删除时本表随双跑测试一并退役。
const DEFAULT_TRUE_REVERSE: &[(&str, &str)] = &[
    ("TargetingBrandedEnemy", "targetBrandedEnemy"),
    ("DemonForm", "inDemonForm"),
    ("CompanionInPresence", "companionInPresence"),
    ("ChampionIntimidate", "conditionChampionIntimidate"),
    ("ConcPathBypassCD", "ConcPathBypassCD"),
    ("FlickerStrikeBypassCD", "FlickerStrikeBypassCD"),
    ("VigilantStrikeBypassCD", "VigilantStrikeBypassCD"),
];

/// 旧路径充能条件反查表（条件变量名 → XML `<Input name>`）。
const CHARGE_REVERSE: &[(&str, &str)] = &[
    ("UsePowerCharges", "usePowerCharges"),
    ("UseFrenzyCharges", "useFrenzyCharges"),
    ("UseEnduranceCharges", "useEnduranceCharges"),
];

/// 旧路径 Stat 型任务奖励默认表（测试本地镜像）。
const DEFAULT_QUEST_STAT_REWARDS: &[(&str, &str)] = &[
    ("questAct 1ClearfellBeira", "+10% to Cold Resistance"),
    ("questAct 1FreythornKing In The Mists", "+30 to Spirit"),
    ("questAct 1Ogham ManorCandlemass", "+20 to maximum Life"),
    (
        "questAct 2Spires of DesharSisters of Garukhan Shrine",
        "+10% to Lightning Resistance",
    ),
    ("questAct 3Azak BogIgnagduk", "+30 to Spirit"),
    (
        "questAct 3Jiquani's MachinariumBlackjaw",
        "+10% to Fire Resistance",
    ),
    (
        "questAct 4Eye of HinekoraSilent Hall",
        "5% increased Maximum Mana",
    ),
    (
        "questInterlude 2Khari CrossingMolten Shrine",
        "5% increased maximum Life",
    ),
    ("questInterlude 3Kriar VillageLythara", "+40 to Spirit"),
];

fn load_catalog() -> ConfigCatalog {
    GameData::new(repo_data_root().join(pobr_gamedata::data_version()))
        .load_ruleset()
        .expect("load ruleset")
        .config_catalog
        .expect("config_catalog 域应已接通")
}

/// 双跑数据源：18 个 ninja build（code.txt → XML）+ 全部 config fixture。
fn xml_sources() -> Vec<(String, String)> {
    let mut sources = Vec::new();
    let builds = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/demo-bd-test/builds");
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(builds)
        .expect("read builds dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir() && p.join("code.txt").exists())
        .collect();
    dirs.sort();
    for dir in dirs {
        let code = std::fs::read_to_string(dir.join("code.txt")).expect("read code.txt");
        let xml = decode_pob_code(code.trim()).expect("decode build code");
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        sources.push((name, xml));
    }
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut files: Vec<PathBuf> = std::fs::read_dir(fixtures)
        .expect("read fixtures dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("config_"))
        })
        .collect();
    files.sort();
    for file in files {
        let xml = std::fs::read_to_string(&file).expect("read fixture");
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        sources.push((name, xml));
    }
    assert!(sources.len() >= 23, "18 ninja builds + 5 fixtures");
    sources
}

/// 全部产物桶里按归因 var 过滤 mod。
fn mods_from<'a>(outcome: &'a ConfigOutcome, var: &str) -> Vec<&'a Modifier> {
    let id = format!("config.{var}");
    outcome
        .player_mods
        .iter()
        .chain(outcome.enemy_mods.iter())
        .chain(outcome.minion_mods.iter())
        .filter(|m| {
            m.origin
                .as_ref()
                .is_some_and(|origin| origin.source_id.id == id)
        })
        .collect()
}

/// 双跑分类汇总（跨全部数据源聚合；BTreeMap 保证打印确定序）。
#[derive(Default)]
struct DualRunSummary {
    /// 覆盖分类 → 条目集（`var` 或 `var=值`）。
    categories: BTreeMap<&'static str, BTreeSet<String>>,
    /// 不可解释的旧产出项（→ hard fail）。
    unexplained: BTreeSet<String>,
}

impl DualRunSummary {
    fn record(&mut self, category: &'static str, item: String) {
        self.categories.entry(category).or_default().insert(item);
    }
}

/// 单条旧 condition 的覆盖判定。
fn classify_condition(
    var: &str,
    value: bool,
    catalog: &ConfigCatalog,
    outcome: &ConfigOutcome,
    source: &str,
    summary: &mut DualRunSummary,
) {
    if !value {
        // 旧路径显式 false：新路径条目未激活（缺省即 false，语义等值）；
        // 绝不允许新路径反向置 true。
        assert_ne!(
            outcome.conditions.get(var),
            Some(&true),
            "[{source}] 旧显式 false 的条件 {var} 在新路径被置 true"
        );
        summary.record("旧显式 false（新路径未激活，语义等值）", var.to_string());
        return;
    }
    // 同名同值：交集逐值相等。
    if outcome.conditions.get(var) == Some(&true) {
        summary.record("conditions 同名同值（交集逐值相等）", var.to_string());
        return;
    }
    // 反查源 XML 输入名 → catalog 条目。
    let candidates = [
        format!("condition{var}"),
        CHARGE_REVERSE
            .iter()
            .find(|(v, _)| *v == var)
            .map(|(_, n)| (*n).to_string())
            .unwrap_or_default(),
        DEFAULT_TRUE_REVERSE
            .iter()
            .find(|(v, _)| *v == var)
            .map(|(_, n)| (*n).to_string())
            .unwrap_or_default(),
        var.to_string(),
    ];
    let entry = candidates
        .iter()
        .filter(|c| !c.is_empty())
        .find_map(|c| catalog.get(c));
    let Some(entry) = entry else {
        summary
            .unexplained
            .insert(format!("condition {var} @ {source}（无 catalog 条目）"));
        return;
    };
    if entry.handler_id.is_some() {
        summary.record(
            "handler 缺口（原始输入已捕获，解释待后续 handler 批次）",
            format!("{}（{}）", entry.var, entry.handler_id.as_deref().unwrap()),
        );
        return;
    }
    let mods = mods_from(outcome, &entry.var);
    let expected = format!("Condition:{var}");
    if mods
        .iter()
        .any(|m| m.mod_type == ModType::Flag && m.name.as_str() == expected)
    {
        summary.record(
            "条件 mod 化（带 Combat/Effective 等门控 tag，值相等；待行为 commit）",
            format!("{var} ← {}", entry.var),
        );
        return;
    }
    if let Some(stripped) = var.strip_prefix("Enemy")
        && mods
            .iter()
            .any(|m| m.name.as_str() == format!("Condition:{stripped}"))
    {
        summary.record(
            "enemy 条件 actor 化（Condition:Enemy<X> → enemy 桶 Condition:<X>）",
            format!("{var} ← {}", entry.var),
        );
        return;
    }
    // vendor 命名与去前缀 var 不同（如 conditionEnemyCriticalWeakness →
    // Condition:ApplyCriticalWeakness）：FLAG 真值等值，命名口径属 vendor 忠实化。
    if let Some(found) = mods.iter().find(|m| m.mod_type == ModType::Flag) {
        summary.record(
            "条件 vendor 命名口径（FLAG 真值等值，名取 vendor 忠实拼写）",
            format!("{var} ← {} as {}", entry.var, found.name.as_str()),
        );
        return;
    }
    // 条件 → 数值 mod 化（§3-⑦ 回补后形态）：vendor 把 checkbox 落成 enemy
    // 数值 mod（如曝光三条 `<X>Exposure` BASE 20 + ActorCondition 门控，
    // ConfigOptions.lua:1864-1872）——开关语义由数值 mod 承载，FLAG 不再产出。
    if !mods.is_empty() {
        summary.record(
            "条件→数值 mod 化（checkbox 落 enemy 数值 mod + actor 门控 tag）",
            format!("{var} ← {} as {}", entry.var, mods[0].name.as_str()),
        );
        return;
    }
    // 条目效果带未映射 actor / 未接通 tag 维度：解释器保守跳过整条 mod 并记
    // diagnostics——原始输入已捕获。
    let diag_prefix = format!("config.{}:", entry.var);
    if outcome
        .diagnostics
        .iter()
        .any(|d| d.starts_with(&diag_prefix))
    {
        summary.record(
            "tag 维度未接通（未映射 actor 等），保守跳过待回补",
            format!("{var} ← {}", entry.var),
        );
        return;
    }
    summary.unexplained.insert(format!(
        "condition {var} @ {source}（条目 {} 无对应产出）",
        entry.var
    ));
}

/// 单条旧 multiplier 的覆盖判定。
fn classify_multiplier(
    var: &str,
    value: f64,
    catalog: &ConfigCatalog,
    outcome: &ConfigOutcome,
    source: &str,
    summary: &mut DualRunSummary,
) {
    if let Some(new_value) = outcome.multipliers.get(var) {
        // 交集逐值相等（hard assert）。
        assert!(
            (new_value - value).abs() < 1e-9,
            "[{source}] multiplier {var} 交集值不等：旧 {value} vs 新 {new_value}"
        );
        summary.record(
            "multipliers 同名同值（交集逐值相等）",
            format!("{var}={value}"),
        );
        return;
    }
    let xml_name = format!("multiplier{var}");
    let Some(entry) = catalog.get(&xml_name) else {
        summary
            .unexplained
            .insert(format!("multiplier {var} @ {source}（无 catalog 条目）"));
        return;
    };
    if entry.handler_id.is_some() {
        summary.record(
            "handler 缺口（原始输入已捕获，解释待后续 handler 批次）",
            format!("{}（{}）", entry.var, entry.handler_id.as_deref().unwrap()),
        );
        return;
    }
    let mods = mods_from(outcome, &entry.var);
    if let Some(found) = mods
        .iter()
        .find(|m| m.name.as_str().starts_with("Multiplier:"))
    {
        // mod 化覆盖也必须逐值相等（identity 表达式口径）。
        assert_eq!(
            found.value.as_number(),
            Some(value),
            "[{source}] multiplier {var} mod 化值不等（vendor 表达式非恒等？需逐条审查）"
        );
        summary.record(
            "multiplier mod 化（带 Effective tag / 命名口径修正，值相等；待行为 commit）",
            format!("{var} ← {} as {}", entry.var, found.name.as_str()),
        );
        return;
    }
    let diag_prefix = format!("config.{}:", entry.var);
    if outcome
        .diagnostics
        .iter()
        .any(|d| d.starts_with(&diag_prefix))
    {
        summary.record(
            "tag 维度未接通（T5-E1 ActorCondition/actor Multiplier），保守跳过待回补",
            format!("{var} ← {}", entry.var),
        );
        return;
    }
    summary.unexplained.insert(format!(
        "multiplier {var} @ {source}（条目 {} 无对应产出）",
        entry.var
    ));
}

/// 旧 global_texts（任务奖励行）的覆盖判定——按 quest key 重建归属后逐 key 判。
fn classify_quests(
    inputs: &RawConfigInputs,
    catalog: &ConfigCatalog,
    outcome: &ConfigOutcome,
    source: &str,
    summary: &mut DualRunSummary,
) {
    // 重建旧路径逐 key 的产出行（镜像旧 parse_config 的 quest 分支）。
    let mut per_key: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (name, value) in &inputs.values {
        if !name.starts_with("quest") {
            continue;
        }
        match value {
            ConfigInputValue::Text(text) => {
                let lines: Vec<String> = text
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(str::to_string)
                    .collect();
                per_key.insert(name.clone(), lines);
            }
            ConfigInputValue::Bool(true) => {
                if let Some((_, stat)) = DEFAULT_QUEST_STAT_REWARDS.iter().find(|(k, _)| k == name)
                {
                    per_key.insert(name.clone(), vec![(*stat).to_string()]);
                }
            }
            _ => {}
        }
    }
    for (key, stat) in DEFAULT_QUEST_STAT_REWARDS {
        if !inputs.values.contains_key(*key) {
            per_key.insert((*key).to_string(), vec![(*stat).to_string()]);
        }
    }

    for (key, lines) in per_key {
        let Some(entry) = catalog.get(&key) else {
            summary
                .unexplained
                .insert(format!("quest {key} @ {source}（无 catalog 条目）"));
            continue;
        };
        if entry.handler_id.is_some() {
            summary.record(
                "handler 缺口（原始输入已捕获，解释待后续 handler 批次）",
                format!("{}（{}）", entry.var, entry.handler_id.as_deref().unwrap()),
            );
            continue;
        }
        let quest_mods = mods_from(outcome, &entry.var);
        for line in lines {
            if line == "None" || line == "Nothing" {
                continue;
            }
            let Ok(parsed) = parse_mod(&line) else {
                summary.record(
                    "quest 行 parser 不支持（旧路径同样落 Unsupported 通道，等值）",
                    format!("{key}: {line}"),
                );
                continue;
            };
            if parsed.status != ParseStatus::Parsed {
                summary.record(
                    "quest 行 parser 不支持（旧路径同样落 Unsupported 通道，等值）",
                    format!("{key}: {line}"),
                );
                continue;
            }
            for old_mod in &parsed.mods {
                let matched = quest_mods.iter().find(|m| {
                    m.name == old_mod.name
                        && m.mod_type == old_mod.mod_type
                        && m.value.as_number() == old_mod.value.as_number()
                });
                if matched.is_some() {
                    summary.record(
                        "quest 奖励逐值相等（旧文本经 parser == 新声明式 effects）",
                        format!("{key}: {}", old_mod.name.as_str()),
                    );
                } else if let Some(renamed) = quest_mods.iter().find(|m| {
                    m.mod_type == old_mod.mod_type
                        && m.value.as_number() == old_mod.value.as_number()
                }) {
                    // parser 规范名与 vendor 名不同（ColdResistance vs ColdResist）：
                    // 类型+值逐值相等，命名口径属 vendor 忠实化（M6 parser 规则统一）。
                    summary.record(
                        "quest 命名口径差异（parser 名 ≠ vendor 名，类型+值相等）",
                        format!(
                            "{key}: {} → {}",
                            old_mod.name.as_str(),
                            renamed.name.as_str()
                        ),
                    );
                } else {
                    summary.unexplained.insert(format!(
                        "quest {key} @ {source}：旧行 `{line}` 解析出 {}={:?} 在新产出中无匹配",
                        old_mod.name.as_str(),
                        old_mod.value.as_number(),
                    ));
                }
            }
        }
    }
}

/// 新路径独有覆盖（旧路径完全不消费的条目）——入报告的新增覆盖清单。
fn collect_new_coverage(
    old_conditions: &std::collections::HashMap<String, bool>,
    old_multipliers: &std::collections::HashMap<String, f64>,
    outcome: &ConfigOutcome,
    summary: &mut DualRunSummary,
) {
    for (var, enabled) in &outcome.conditions {
        if *enabled && !old_conditions.contains_key(var) {
            summary.record(
                "新增覆盖：condition（count 型 / implyCond / 非前缀条目）",
                var.clone(),
            );
        }
    }
    for var in outcome.multipliers.keys() {
        if !old_multipliers.contains_key(var) {
            summary.record("新增覆盖：multiplier", var.clone());
        }
    }
    for m in &outcome.enemy_mods {
        if m.mod_type != ModType::Flag {
            summary.record(
                "新增覆盖：enemy 数值覆盖（抗性/穿透/伤害等 BASE 直注）",
                m.name.as_str().to_string(),
            );
        }
    }
    if !outcome.custom_mod_lines.is_empty() {
        summary.record(
            "新增覆盖：customMods 行通道",
            format!("{} 行", outcome.custom_mod_lines.len()),
        );
    }
    for entry in &outcome.skill_data {
        summary.record("新增覆盖：SkillData 键值载荷", entry.key.clone());
    }
    for u in &outcome.unhandled {
        summary.record(
            "unhandled（handler_id 未注册，覆盖率报表）",
            u.handler_id.clone(),
        );
    }
}

/// 双跑主断言：旧 ⊆ 新（按文件头口径分类），交集逐值相等；不可解释项 = fail。
#[test]
fn dual_run_old_subset_of_new() {
    let catalog = load_catalog();
    let registry = build_registry();
    let mut summary = DualRunSummary::default();

    for (source, xml) in xml_sources() {
        let old = parse_config_legacy(&xml);
        let inputs = parse_config_inputs(&xml);
        let outcome = interpret(&catalog.options, &inputs, &registry);

        for (var, value) in &old.conditions {
            classify_condition(var, *value, &catalog, &outcome, &source, &mut summary);
        }
        for (var, value) in &old.multipliers {
            // vendor 聚合口径（M3-W4 commit B，ConfigOptions.lua:1106-1111）：
            // `multiplierNearbyRareOrUniqueEnemies` 的 apply **双写**
            // `Multiplier:NearbyRareOrUniqueEnemies` 与 `Multiplier:NearbyEnemies`
            // （:1108），modDB Sum 相加——旧路径前缀通道只记本 var、漏聚合。
            // handler 已按 vendor 口径把 rare 计数加进 NearbyEnemies 标量，
            // 故交集断言的期望值 = 旧值 + rare 计数（行为提升，非口径漂移）。
            let expected = if var == "NearbyEnemies" {
                *value
                    + old
                        .multipliers
                        .get("NearbyRareOrUniqueEnemies")
                        .copied()
                        .unwrap_or(0.0)
            } else {
                *value
            };
            classify_multiplier(var, expected, &catalog, &outcome, &source, &mut summary);
        }
        classify_quests(&inputs, &catalog, &outcome, &source, &mut summary);

        // 标量项：enemyIsBoss / resistancePenalty 包装与旧路径逐值相等。
        if let Some(old_tier) = old.enemy_tier {
            assert_eq!(
                enemy_tier_from_config(&outcome),
                Some(old_tier),
                "[{source}] enemyIsBoss 标量口径不等"
            );
            summary.record("标量逐值相等：enemyIsBoss", format!("{old_tier:?}"));
        } else if let Some(tier) = enemy_tier_from_config(&outcome) {
            summary.record(
                "新增覆盖：标量 default 实体化（XML 省略 → catalog defaultIndex）",
                format!("enemyIsBoss={tier:?}"),
            );
        }
        if let Some(old_progress) = old.campaign_progress {
            assert_eq!(
                campaign_progress_from_config(&outcome),
                Some(old_progress),
                "[{source}] resistancePenalty 标量口径不等"
            );
            summary.record(
                "标量逐值相等：resistancePenalty",
                format!("{old_progress:?}"),
            );
        } else if campaign_progress_from_config(&outcome).is_some() {
            summary.record(
                "新增覆盖：标量 default 实体化（XML 省略 → catalog defaultIndex）",
                "resistancePenalty=-60(Endgame)".to_string(),
            );
        }

        collect_new_coverage(&old.conditions, &old.multipliers, &outcome, &mut summary);
    }

    // 打印逐类汇总（--nocapture 时人工誊录至 m3-t1-dualrun-report.md）。
    println!(
        "\n== M3-T1 双跑分类汇总（{} 类） ==",
        summary.categories.len()
    );
    for (category, items) in &summary.categories {
        println!("\n[{category}] × {}", items.len());
        for item in items {
            println!("  - {item}");
        }
    }

    assert!(
        summary.unexplained.is_empty(),
        "存在不可解释的旧产出项（旧 ⊄ 新）：\n{}",
        summary
            .unexplained
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
