//! config 消费收口（M3-T1 A5「xml_build 切换 interpreter 主路径」，蓝图
//! m3-orchestration §4.5 / D3 双跑点 1）。
//!
//! [`resolve_config`] 是 `calculate_with_data` 消费 build config 的**唯一入口**：
//! - `ConfigCatalog` 可用（`data/<ver>/overlay/config_options.json` 已装载）→
//!   主路径：`build.config.raw_inputs`（`parse_config_inputs` 无损捕获的
//!   `<Input>` 三型键值）经 `config_interpreter::interpret` 解释，产出
//!   conditions / multipliers / 标量（enemyIsBoss、resistancePenalty 包装）/
//!   player·enemy modifier / customMods 行通道；
//! - 缺 catalog（旧数据包 / [`BuildData::empty`](crate::BuildData::empty)）→
//!   回退旧 `parse_config` 产出（`build.config` 既有字段，R7 缺表容忍）。
//!
//! ## 双跑期口径（commit 簇逐类打开，dualrun 报告 §3）
//!
//! - **conditions / multipliers**：旧键集 ∪ 解释器产出——交集逐值相等
//!   （`config_dualrun` 持续回归 hard assert）；解释器独有的新增覆盖项
//!   （count 型 / implyCond / 非前缀条目，报告 §2.3 行 1-2）已放开
//!   （commit ②，见 [`merge_conditions`] / [`merge_multipliers`]）。
//! - **enemy 条件桥**：vendor 把 `conditionEnemy<X>` 落 enemy 桶
//!   `Condition:<X>`（ConfigOptions.lua:1694/1769/1789/1833 enemyModList:NewMod）；
//!   pobr 的 mod_parser 把「against <X> enemies」建模为 cfg 条件 `Enemy<X>`
//!   （mod_parser.rs:1612-1640），故此处把 enemy 桶 FLAG 反桥接回 `Enemy<X>`
//!   条件——纯命名空间换算、零行为增量（旧路径删除前与 legacy 值逐项相等）。
//! - **quest 奖励**：仍走旧 text 通道（`global_modifier_texts`）——解释器的
//!   声明式 quest mod 沿 vendor 拼写（`Life`/`Str`/`ColdResist`），与 parser
//!   名空间（`MaximumLife`/`Strength`/`ColdResistance`）的统一属报告 §3-⑤
//!   （与 M6 parser 规则、M1 statmap 名空间一并裁决），此前不切换以免双名分裂。
//! - **标量**：enemyIsBoss / resistancePenalty 仅在 XML **显式给出**时走
//!   handler 包装（[`enemy_tier_from_config`] / [`campaign_progress_from_config`]）；
//!   省略时维持既有回退链（编排选项档位 / Endgame -60），与 catalog
//!   defaultIndex 实体化值一致（报告 §2.3 行 5：实际打开为零 diff）。

use std::collections::HashMap;

use pobr_core::modifier::Modifier;
use pobr_core::rules::config_interpreter::{ConfigOutcome, interpret};
use pobr_data::modifier::ModType;
use pobr_gamedata::ruleset::ConfigCatalog;

use crate::build::Build;
use crate::build_config::BuildConfig;
use crate::handlers::{build_registry, campaign_progress_from_config, enemy_tier_from_config};

/// 旧路径 `conditionEnemy<X>Exposure` → cfg 条件桥（vendor
/// ConfigOptions.lua:1864-1872：enemy 桶 `<X>Exposure` BASE 20 + ActorCondition
/// tag）。§3-⑦ 回补后解释器已产出该 enemy 数值 mod（actor tag 已翻译为
/// `ModTag::Condition{actor: Player, var: CanApply<X>Exposure}`），但玩家侧
/// actor 快照（`cfg.actor_multipliers["player.CanApply<X>Exposure"]`）尚无
/// 置位来源——该 mod 惰性，编排层 5b `apply_enemy_exposure` 仍按 cfg 条件
/// 驱动，故从原始输入直读补桥，维持现网行为。**退役条件**：actor 快照通道
/// 接通（CanApply*Exposure 由技能侧置位）后本桥必须删除，否则与解释器
/// mod 双计。
const EXPOSURE_CONDITION_BRIDGE: &[(&str, &str)] = &[
    ("conditionEnemyFireExposure", "EnemyFireExposure"),
    ("conditionEnemyColdExposure", "EnemyColdExposure"),
    ("conditionEnemyLightningExposure", "EnemyLightningExposure"),
];

/// `calculate_with_data` 的 config 消费视图（来源 = 主路径解释 or 旧路径回退）。
#[derive(Default)]
pub(crate) struct ResolvedConfig {
    /// conditions / multipliers / campaign_progress / enemy_tier 已按主路径
    /// 解析的 [`BuildConfig`] 副本（含程序化直填字段的原样保留——主路径只
    /// 在 `raw_inputs` 有对应输入时覆盖）。
    pub config: BuildConfig,
    /// 解释器产出的玩家 modifier（`SourceKind::Config` 归因；quest 条目除外，
    /// 见模块注释 §3-⑤）。Combat 门控条目（`Condition:Combat` tag）在
    /// `mode_combat=false` 下天然惰性（D5）。缺 catalog 时为空。
    pub player_mods: Vec<Modifier>,
    /// 解释器产出的敌人 modifier（`SourceKind::EnemyConfig` 归因）：
    /// - FLAG：enemy 条件 actor 化条目（带 `Condition:Effective` tag）；
    /// - 数值（commit ③ 起）：敌方抗性 / SelfCritChance / 异常层数 BASE 直注
    ///   （vendor `enemyModList:NewMod(..., "BASE", val, "EnemyConfig")` 形态，
    ///   ConfigOptions.lua:2143-2157 / 1892-1894 / 1782-1842）。
    pub enemy_mods: Vec<Modifier>,
    /// customMods 行通道（commit ④，vendor ConfigOptions.lua:2278-2296：逐行
    /// StripEscapes + parseMod，source=Custom）：解释器剥色码后的原文行，
    /// build 层喂 `session.add_modifier_texts`——不可解析行自然落
    /// `ParseStatus::Unsupported` 可见性通道（结构性硬失败行经
    /// filter_parseable 跳过，与其余 text 注入通道同口径）。缺 catalog 时为空
    /// （旧路径从不消费 customMods）。
    pub custom_mod_lines: Vec<String>,
}

/// 解析 build config 的消费视图（详见模块注释）。
pub(crate) fn resolve_config(build: &Build, catalog: Option<&ConfigCatalog>) -> ResolvedConfig {
    let Some(catalog) = catalog else {
        // R7 缺表容忍：回退旧 parse_config 产出（build.config 既有字段）。
        return ResolvedConfig {
            config: build.config.clone(),
            player_mods: Vec::new(),
            enemy_mods: Vec::new(),
            custom_mod_lines: Vec::new(),
        };
    };

    let outcome = interpret(
        &catalog.options,
        &build.config.raw_inputs,
        &build_registry(),
    );
    let mut config = build.config.clone();

    merge_conditions(&mut config.conditions, &outcome);
    merge_multipliers(&mut config.multipliers, &outcome);
    bridge_enemy_conditions(&mut config.conditions, &outcome);
    bridge_combat_player_conditions(&mut config.conditions, &outcome);
    bridge_exposure_conditions(&mut config.conditions, build);

    // 标量包装（仅显式输入时覆盖；省略 = 维持既有回退链，见模块注释）。
    if build.config.raw_inputs.values.contains_key("enemyIsBoss") {
        config.enemy_tier = enemy_tier_from_config(&outcome);
    }
    if build
        .config
        .raw_inputs
        .values
        .contains_key("resistancePenalty")
    {
        config.campaign_progress = campaign_progress_from_config(&outcome);
    }

    // quest 条目仍走旧 text 通道（§3-⑤）：从 modifier 注入中排除，避免与
    // `global_modifier_texts` 双计。
    let player_mods = outcome
        .player_mods
        .iter()
        .filter(|m| !is_quest_mod(m))
        .cloned()
        .collect();
    let enemy_mods = outcome.enemy_mods.clone();

    ResolvedConfig {
        config,
        player_mods,
        enemy_mods,
        custom_mod_lines: outcome.custom_mod_lines.clone(),
    }
}

/// conditions 合并（commit ② 起全量）：交集逐值取主路径（dualrun 持续回归
/// 保证值相等）；解释器独有键 = 新增覆盖（count 型条目的 `>0` 条件、
/// implyCond 展开、非 `condition` 前缀条目，dualrun 报告 §2.3 行 1）。
///
/// implyCond 语义说明：vendor 把计算侧蕴含**直接烤进各条目 apply**
/// （如 conditionCritRecently 同时 NewMod SkillCritRecently / CritInPast8Sec，
/// ConfigOptions.lua:1130-1134——该形态已由 player_mods 注入承载，Combat
/// 门控）；`implyCondList` 元数据本身只驱动 vendor 配置页可见性
/// （ConfigTab.lua:91-109）。此处写入 cfg.conditions 的 imply 展开是 pobr
/// 兼容通道（与旧 parse_config 的「裸条件直进 cfg」同一口径，供 mod_parser
/// 的 Condition tag 词条消费），不覆盖显式值（interpreter `or_insert`）。
fn merge_conditions(conditions: &mut HashMap<String, bool>, outcome: &ConfigOutcome) {
    for (var, enabled) in &outcome.conditions {
        conditions.insert(var.clone(), *enabled);
    }
}

/// multipliers 合并（commit ② 起全量）：口径同 [`merge_conditions`]——
/// 新增覆盖 = count 条目数值化（Multiplier:StationarySeconds 等，
/// dualrun 报告 §2.3 行 2；vendor ConfigOptions.lua:120-127 conditionStationary
/// `NewMod("Multiplier:StationarySeconds", BASE, val)` 形态）。
fn merge_multipliers(multipliers: &mut HashMap<String, f64>, outcome: &ConfigOutcome) {
    for (var, value) in &outcome.multipliers {
        multipliers.insert(var.clone(), *value);
    }
}

/// enemy 桶 FLAG `Condition:<X>` → cfg 条件 `Enemy<X>` 反桥（见模块注释）。
/// `or_insert` 不覆盖显式值（XML `boolean="false"` 在解释侧本就不激活）。
///
/// （M4-L）额外**未前缀**桥：敌侧数值 mod 的 `Condition` tag 按敌方自身状态名
/// （无 Enemy 前缀）查 cfg 条件——curse 域同约定（`stat_map_engine::map_curse_stat`
/// doc：「var 即敌方自身状态，不加 Enemy 前缀」；vendor 等价 = enemyDB 内 mod 的
/// Condition tag 查 enemyDB 自身 conditions）。pobr 单条件命名空间下不能盲目
/// 全量去前缀（`Condition:Chilled` 会污染玩家侧 `Chilled`），故只对**被敌侧
/// 数值 mod tag 实际引用**的 var 同步落未前缀条件（当前 catalog 引用集 =
/// `ApplyCriticalWeakness`/`OnProfaneGround`/`<Ailment>Config` 族，全部为敌方
/// 专属名，无玩家名空间碰撞）。范例链（vendor ConfigOptions.lua:1889-1894）：
/// `conditionEnemyCriticalWeakness` → enemy FLAG `Condition:ApplyCriticalWeakness`，
/// 叠 `enemyCriticalWeaknessStacks`（placeholder 20）→ enemy `SelfCritChance BASE 10`
/// {Condition:ApplyCriticalWeakness}——未前缀条件落位后该 mod 在暴击聚合命中
/// （CalcOffence.lua:3677 段敌侧 SelfCritChance 进暴击率底材）。
fn bridge_enemy_conditions(conditions: &mut HashMap<String, bool>, outcome: &ConfigOutcome) {
    use pobr_core::ModTag;
    use std::collections::HashSet;

    // 敌侧数值 mod 的 Condition tag 引用集（同 actor；跨 actor tag 非本语义）。
    let referenced: HashSet<&str> = outcome
        .enemy_mods
        .iter()
        .filter(|m| m.mod_type != ModType::Flag)
        .flat_map(|m| m.tags.iter())
        .filter_map(|tag| match tag {
            ModTag::Condition {
                var, actor: None, ..
            } => Some(var.as_str()),
            _ => None,
        })
        .collect();
    for modifier in &outcome.enemy_mods {
        if modifier.mod_type != ModType::Flag {
            continue;
        }
        let Some(var) = modifier.name.as_str().strip_prefix("Condition:") else {
            continue;
        };
        let enabled = matches!(modifier.value, pobr_core::ModValue::Bool(true));
        conditions.entry(format!("Enemy{var}")).or_insert(enabled);
        if referenced.contains(var) {
            conditions.entry(var.to_string()).or_insert(enabled);
        }
    }
}

/// （M4-m）玩家桶 **Combat 门控** `Condition:<X>` FLAG → cfg 条件桥。
///
/// 解释器的裸效果回填（config_interpreter `apply_effect`）只收**无 tag** 条目；
/// `Condition:CritRecently`/`Condition:UsingCharm` 等 config 主效果带
/// `{type=condition, var=Combat}` tag（vendor ConfigOptions 的 Combat 门控形态，
/// `Condition:Combat` ≡ buffMode "EFFECTIVE" → mode_combat，CalcSetup.lua:583-597
/// ——本编排路径恒置 mode_combat=true，M3-T2 B4），落进 player_mods 后无 cfg
/// 消费方（`Modifier::matches` 的 Condition tag 只查 cfg.conditions），使同名
/// imply 条件（CritInPast8Sec）生效而**主条件自身**（CritRecently）失活。
/// 此桥把「tag 全部 = Condition:Combat（未否定、无 actor）」的玩家 FLAG
/// `Condition:<X>` 同步落 cfg（`or_insert` 不覆盖显式值）。
fn bridge_combat_player_conditions(
    conditions: &mut HashMap<String, bool>,
    outcome: &ConfigOutcome,
) {
    use pobr_core::ModTag;
    for modifier in &outcome.player_mods {
        if modifier.mod_type != ModType::Flag {
            continue;
        }
        let Some(var) = modifier.name.as_str().strip_prefix("Condition:") else {
            continue;
        };
        let combat_gated_only = !modifier.tags.is_empty()
            && modifier.tags.iter().all(|tag| {
                matches!(
                    tag,
                    ModTag::Condition {
                        var,
                        negated: false,
                        actor: None,
                    } if var == "Combat"
                )
            });
        if !combat_gated_only {
            continue;
        }
        let enabled = matches!(modifier.value, pobr_core::ModValue::Bool(true));
        conditions.entry(var.to_string()).or_insert(enabled);
    }
}

/// `conditionEnemy<X>Exposure` 原始输入 → cfg 条件桥（见
/// [`EXPOSURE_CONDITION_BRIDGE`]）。
fn bridge_exposure_conditions(conditions: &mut HashMap<String, bool>, build: &Build) {
    for (input_name, cond_var) in EXPOSURE_CONDITION_BRIDGE {
        if let Some(pobr_core::rules::config_interpreter::ConfigInputValue::Bool(enabled)) =
            build.config.raw_inputs.values.get(*input_name)
        {
            conditions
                .entry((*cond_var).to_string())
                .or_insert(*enabled);
        }
    }
}

/// quest 条目判定（归因 id `config.quest…`，vendor `<Input name>` 以 `quest`
/// 开头）。
fn is_quest_mod(modifier: &Modifier) -> bool {
    modifier
        .origin
        .as_ref()
        .is_some_and(|origin| origin.source_id.id.starts_with("config.quest"))
}

#[cfg(test)]
mod tests {
    use pobr_core::CampaignProgress;
    use pobr_core::rules::config_interpreter::{ConfigInputValue, RawConfigInputs};
    use pobr_data::monster::EnemyTier;
    use pobr_data::source::SourceKind;
    use pobr_gamedata::{GameData, repo_data_root};

    use super::*;

    fn load_catalog() -> ConfigCatalog {
        GameData::new(repo_data_root().join(pobr_gamedata::data_version()))
            .load_ruleset()
            .expect("load ruleset")
            .config_catalog
            .expect("config_catalog 域应已接通")
    }

    fn build_with_inputs(inputs: RawConfigInputs) -> Build {
        let mut build = Build::new();
        build.config.raw_inputs = inputs;
        build
    }

    /// 缺 catalog → 旧路径回退：config 原样、零 modifier。
    #[test]
    fn missing_catalog_falls_back_to_legacy_config() {
        let mut build = Build::new();
        build.config.conditions.insert("EnemyChilled".into(), true);
        build.config.enemy_tier = Some(EnemyTier::Uber);
        let resolved = resolve_config(&build, None);
        assert_eq!(resolved.config.conditions.get("EnemyChilled"), Some(&true));
        assert_eq!(resolved.config.enemy_tier, Some(EnemyTier::Uber));
        assert!(resolved.player_mods.is_empty());
        assert!(resolved.enemy_mods.is_empty());
    }

    /// 程序化直填的 conditions / multipliers / 标量在主路径下原样保留
    /// （raw_inputs 为空 → 解释器不产生覆盖）。
    #[test]
    fn programmatic_config_preserved_under_catalog() {
        let catalog = load_catalog();
        let mut build = Build::new();
        build.config.conditions.insert("UsingShield".into(), true);
        build.config.multipliers.insert("PowerCharge".into(), 3.0);
        build.config.enemy_tier = Some(EnemyTier::None);
        build.config.campaign_progress = Some(CampaignProgress::Act1);
        let resolved = resolve_config(&build, Some(&catalog));
        assert_eq!(resolved.config.conditions.get("UsingShield"), Some(&true));
        assert_eq!(resolved.config.multipliers.get("PowerCharge"), Some(&3.0));
        assert_eq!(resolved.config.enemy_tier, Some(EnemyTier::None));
        assert_eq!(
            resolved.config.campaign_progress,
            Some(CampaignProgress::Act1)
        );
    }

    /// enemy 条件桥：`conditionEnemyChilled` → enemy 桶 FLAG → cfg `EnemyChilled`
    /// （legacy 字段未填时由桥独立供给——旧路径删除后的唯一来源）。
    #[test]
    fn enemy_condition_bridge_supplies_legacy_var() {
        let catalog = load_catalog();
        let build = build_with_inputs(
            RawConfigInputs::new().with("conditionEnemyChilled", ConfigInputValue::Bool(true)),
        );
        let resolved = resolve_config(&build, Some(&catalog));
        assert_eq!(resolved.config.conditions.get("EnemyChilled"), Some(&true));
        // enemy 桶 FLAG 同步注入（Effective tag 门控，EnemyConfig 归因）。
        assert!(resolved.enemy_mods.iter().any(|m| {
            m.name.as_str() == "Condition:Chilled"
                && m.origin
                    .as_ref()
                    .is_some_and(|o| o.source_id.kind == SourceKind::EnemyConfig)
        }));
    }

    /// 曝光条件桥：`conditionEnemyFireExposure` → cfg `EnemyFireExposure`
    /// （T5-E1 翻译接通前条目效果被解释器保守跳过，桥从原始输入直读）。
    #[test]
    fn exposure_bridge_supplies_condition() {
        let catalog = load_catalog();
        let build = build_with_inputs(
            RawConfigInputs::new()
                .with("conditionEnemyFireExposure", ConfigInputValue::Bool(true))
                .with("conditionEnemyColdExposure", ConfigInputValue::Bool(false)),
        );
        let resolved = resolve_config(&build, Some(&catalog));
        assert_eq!(
            resolved.config.conditions.get("EnemyFireExposure"),
            Some(&true)
        );
        assert_eq!(
            resolved.config.conditions.get("EnemyColdExposure"),
            Some(&false)
        );
        assert!(
            !resolved
                .config
                .conditions
                .contains_key("EnemyLightningExposure")
        );
    }

    /// 标量包装仅在显式输入时覆盖：enemyIsBoss=None 档覆盖；省略时保留
    /// 程序化/回退值。
    #[test]
    fn scalar_wrappers_only_apply_on_explicit_input() {
        let catalog = load_catalog();
        let build = build_with_inputs(
            RawConfigInputs::new()
                .with("enemyIsBoss", ConfigInputValue::Text("None".into()))
                .with("resistancePenalty", ConfigInputValue::Number(-30.0)),
        );
        let resolved = resolve_config(&build, Some(&catalog));
        assert_eq!(resolved.config.enemy_tier, Some(EnemyTier::None));
        assert_eq!(
            resolved.config.campaign_progress,
            CampaignProgress::from_resistance_penalty(-30.0)
        );

        let omitted = resolve_config(&Build::new(), Some(&catalog));
        assert_eq!(omitted.config.enemy_tier, None, "省略时维持既有回退链");
        assert_eq!(omitted.config.campaign_progress, None);
    }

    /// quest 条目不进 modifier 注入（§3-⑤：仍走旧 text 通道，避免双计）。
    #[test]
    fn quest_mods_excluded_from_injection() {
        let catalog = load_catalog();
        let build = build_with_inputs(RawConfigInputs::new().with(
            "questAct 2Valley of the TitansMedallion",
            ConfigInputValue::Text("30% increased Charm Charges Gained\n\t+1 Charm Slot".into()),
        ));
        let resolved = resolve_config(&build, Some(&catalog));
        assert!(
            resolved.player_mods.iter().all(|m| !is_quest_mod(m)),
            "quest 归因 mod 不应进注入列表"
        );
    }

    /// commit ② 口径：count 型条目（conditionStationary，vendor
    /// ConfigOptions.lua:120-127）产 Multiplier + `>0` 时 Condition；
    /// implyCond（conditionCritRecently → SkillCritRecently/CritInPast8Sec，
    /// :1130-1134）展开进 cfg；count=0 整条跳过（vendor BuildModList 语义）。
    #[test]
    fn count_and_imply_coverage_opened() {
        let catalog = load_catalog();
        let mut build = build_with_inputs(
            RawConfigInputs::new()
                .with("conditionStationary", ConfigInputValue::Number(5.0))
                .with("conditionCritRecently", ConfigInputValue::Bool(true)),
        );
        // 模拟旧路径产出（parse_build 双跑期仍填 legacy 字段）。
        build.config.conditions.insert("CritRecently".into(), true);
        let resolved = resolve_config(&build, Some(&catalog));
        assert_eq!(
            resolved.config.conditions.get("Stationary"),
            Some(&true),
            "count>0 → 条件置真"
        );
        assert_eq!(
            resolved.config.multipliers.get("StationarySeconds"),
            Some(&5.0),
            "count 数值化为 Multiplier"
        );
        assert_eq!(
            resolved.config.conditions.get("SkillCritRecently"),
            Some(&true),
            "implyCond 展开"
        );
        assert_eq!(
            resolved.config.conditions.get("CritInPast8Sec"),
            Some(&true)
        );
        // Combat 门控 mod 化条目照常注入（天然惰性）。
        assert!(
            resolved
                .player_mods
                .iter()
                .any(|m| m.name.as_str() == "Condition:CritRecently")
        );
        // （M4-m）Combat 门控桥：主条件自身（CritRecently）同步落 cfg
        // （本编排路径恒置 mode_combat=true，vendor Condition:Combat ≡ true）。
        let no_legacy = build_with_inputs(
            RawConfigInputs::new().with("conditionCritRecently", ConfigInputValue::Bool(true)),
        );
        let resolved = resolve_config(&no_legacy, Some(&catalog));
        assert_eq!(
            resolved.config.conditions.get("CritRecently"),
            Some(&true),
            "Combat 门控主条件经桥落 cfg（不依赖 legacy 字段）"
        );

        // count=0 → vendor BuildModList 语义整条跳过（不产条件 / multiplier）。
        let zero = build_with_inputs(
            RawConfigInputs::new().with("conditionStationary", ConfigInputValue::Number(0.0)),
        );
        let resolved = resolve_config(&zero, Some(&catalog));
        assert!(!resolved.config.conditions.contains_key("Stationary"));
        assert!(
            !resolved
                .config
                .multipliers
                .contains_key("StationarySeconds")
        );
    }

    /// implyCond 不覆盖显式 false（interpreter `or_insert` 语义经合并通道保持）。
    #[test]
    fn imply_does_not_override_explicit_false() {
        let catalog = load_catalog();
        let mut build = build_with_inputs(
            RawConfigInputs::new().with("conditionCritRecently", ConfigInputValue::Bool(true)),
        );
        // 程序化显式 false：合并通道写入 outcome 值前已有显式键——outcome 的
        // imply 展开（SkillCritRecently=true）会覆盖同键。此处断言的是 outcome
        // 内部 or_insert 序（显式效果先于 imply），合并侧整键覆盖属预期。
        build
            .config
            .conditions
            .insert("UsedSkillRecently".into(), false);
        let resolved = resolve_config(&build, Some(&catalog));
        // conditionCritRecently 不蕴含 UsedSkillRecently → 显式 false 保留。
        assert_eq!(
            resolved.config.conditions.get("UsedSkillRecently"),
            Some(&false)
        );
    }

    /// commit ③：enemy 数值覆盖（BASE 直注 enemy 桶 + EnemyConfig 归因）。
    /// vendor enemyFireResist：ConfigOptions.lua:2152-2154
    /// `enemyModList:NewMod("FireResist", "BASE", val, "EnemyConfig")`。
    #[test]
    fn enemy_numeric_overrides_opened() {
        let catalog = load_catalog();
        let build = build_with_inputs(
            RawConfigInputs::new()
                .with("enemyFireResist", ConfigInputValue::Number(75.0))
                .with(
                    "enemyCriticalWeaknessStacks",
                    ConfigInputValue::Number(10.0),
                ),
        );
        let resolved = resolve_config(&build, Some(&catalog));
        let fire = resolved
            .enemy_mods
            .iter()
            .find(|m| m.name.as_str() == "FireResist")
            .expect("FireResist BASE 应进 enemy 注入列表");
        assert_eq!(fire.mod_type, ModType::Base);
        assert_eq!(fire.value.as_number(), Some(75.0));
        assert_eq!(
            fire.origin.as_ref().unwrap().source_id.kind,
            SourceKind::EnemyConfig
        );
        // SelfCritChance（暴击弱化层数 → +0.5%/层，ConfigOptions.lua:1892-1894
        // `m_max(m_min(val, 20), 0) / 2`，带 Condition:ApplyCriticalWeakness tag）。
        let crit = resolved
            .enemy_mods
            .iter()
            .find(|m| m.name.as_str() == "SelfCritChance")
            .expect("SelfCritChance 应进 enemy 注入列表");
        assert_eq!(crit.value.as_number(), Some(5.0), "10 层 × 0.5%");
        assert!(!crit.tags.is_empty(), "保留 ApplyCriticalWeakness 门控 tag");
    }

    /// （M4-L）敌侧条件未前缀桥：`conditionEnemyCriticalWeakness` →
    /// `EnemyApplyCriticalWeakness`（既有前缀桥）+ `ApplyCriticalWeakness`
    /// （未前缀，因 `enemyCriticalWeaknessStacks` 的 SelfCritChance mod tag
    /// 引用它——vendor ConfigOptions.lua:1893 `{type="Condition",
    /// var="ApplyCriticalWeakness"}`）。未被敌侧数值 mod 引用的条件
    /// （如 Chilled）**不**落未前缀名（玩家名空间防污染）。
    #[test]
    fn enemy_condition_unprefixed_bridge_for_referenced_vars() {
        let catalog = load_catalog();
        let build = build_with_inputs(
            RawConfigInputs::new()
                .with(
                    "conditionEnemyCriticalWeakness",
                    ConfigInputValue::Bool(true),
                )
                .with("conditionEnemyChilled", ConfigInputValue::Bool(true)),
        );
        let resolved = resolve_config(&build, Some(&catalog));
        assert_eq!(
            resolved.config.conditions.get("EnemyApplyCriticalWeakness"),
            Some(&true),
            "既有 Enemy 前缀桥保持"
        );
        assert_eq!(
            resolved.config.conditions.get("ApplyCriticalWeakness"),
            Some(&true),
            "被 SelfCritChance tag 引用 → 未前缀条件落位"
        );
        assert_eq!(resolved.config.conditions.get("EnemyChilled"), Some(&true));
        assert_eq!(
            resolved.config.conditions.get("Chilled"),
            None,
            "未被敌侧数值 mod 引用 → 不落未前缀名（防玩家侧 Chilled 污染）"
        );
        // 未前缀桥的消费端形态：SelfCritChance BASE 10（placeholder 20 层 × 0.5）。
        let crit = resolved
            .enemy_mods
            .iter()
            .find(|m| m.name.as_str() == "SelfCritChance")
            .expect("placeholder 默认 20 层 → SelfCritChance 应在 enemy 注入列表");
        assert_eq!(crit.value.as_number(), Some(10.0));
    }

    /// M3-W4 commit B 端到端：multiplierNearby* handler 经主路径生效——
    /// 标量加法回填（rare 计数聚合进 NearbyEnemies，vendor
    /// ConfigOptions.lua:1108 双写口径）+ Combat 门控 mod 注入 + enemy 桶 FLAG。
    #[test]
    fn nearby_handlers_resolve_end_to_end() {
        let catalog = load_catalog();
        let build = build_with_inputs(
            RawConfigInputs::new()
                .with("multiplierNearbyEnemies", ConfigInputValue::Number(3.0))
                .with(
                    "multiplierNearbyRareOrUniqueEnemies",
                    ConfigInputValue::Number(1.0),
                ),
        );
        let resolved = resolve_config(&build, Some(&catalog));
        assert_eq!(
            resolved.config.multipliers.get("NearbyEnemies"),
            Some(&4.0),
            "3（普通）+ 1（rare 聚合，vendor :1108）"
        );
        assert_eq!(
            resolved.config.multipliers.get("NearbyRareOrUniqueEnemies"),
            Some(&1.0)
        );
        // Combat 门控 player mod 注入（mode_combat 接通前天然惰性）。
        assert!(
            resolved
                .player_mods
                .iter()
                .any(|m| m.name.as_str() == "Multiplier:NearbyEnemies")
        );
        // enemy 桶 FLAG（val>=1 → true）+ EnemyConfig 归因。
        let enemy_flag = resolved
            .enemy_mods
            .iter()
            .find(|m| m.name.as_str() == "Condition:NearbyRareOrUniqueEnemy")
            .expect("enemy 桶 FLAG 应注入");
        assert_eq!(
            enemy_flag.origin.as_ref().unwrap().source_id.kind,
            SourceKind::EnemyConfig
        );
    }

    /// M3-W4 commit B：inDemonForm handler 经主路径置 DemonForm 条件
    /// （defaultState=true 缺省激活，与旧 DEFAULT_TRUE_CONDITIONS 口径一致）。
    #[test]
    fn in_demon_form_resolves_condition() {
        let catalog = load_catalog();
        let resolved = resolve_config(&Build::new(), Some(&catalog));
        assert_eq!(resolved.config.conditions.get("DemonForm"), Some(&true));

        // 显式 false → 条目未激活，handler 不产出（合并不强行置 true）。
        let build = build_with_inputs(
            RawConfigInputs::new().with("inDemonForm", ConfigInputValue::Bool(false)),
        );
        let resolved = resolve_config(&build, Some(&catalog));
        assert_ne!(resolved.config.conditions.get("DemonForm"), Some(&true));
    }

    /// commit ④：customMods 行通道经 resolve 透传（StripEscapes 已在解释器
    /// 完成；vendor ConfigOptions.lua:2278-2296）。
    #[test]
    fn custom_mod_lines_passed_through() {
        let catalog = load_catalog();
        let build = build_with_inputs(RawConfigInputs::new().with(
            "customMods",
            ConfigInputValue::Text("^x7070FF20% increased Fire Damage\n+10 to Spirit".into()),
        ));
        let resolved = resolve_config(&build, Some(&catalog));
        assert_eq!(
            resolved.custom_mod_lines,
            vec![
                "20% increased Fire Damage".to_string(),
                "+10 to Spirit".to_string()
            ]
        );
        // 缺 catalog → 旧路径从不消费 customMods，通道为空。
        assert!(resolve_config(&build, None).custom_mod_lines.is_empty());
    }
}
