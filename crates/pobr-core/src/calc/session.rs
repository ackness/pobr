use std::collections::BTreeMap;
use std::sync::Arc;

use pobr_data::catalog::buffs::BuffDef;
use pobr_data::prelude::*;

use crate::item::ingest_item;
use crate::mod_parser::{ParseError, ParseStatus, parse_mod};
use crate::passive::{AllocatedNode, ingest_passive_nodes};
use crate::rules::HandlerRegistry;
use crate::skill_source::{GemModSource, ingest_gem};
use crate::{CalcConfig, Modifier};

use super::{Actor, ActorBaseStats, Env, MinimalInput, MinimalOutput, OutputTable, perform};

/// buff 技能九类分发类别（M3 T0 接口契约，蓝图 m3-orchestration.md §2.4）。
///
/// 对应 PoB2 CalcPerform.lua:1831-2984 的 buff 分发九类。M3 实际实现
/// Aura/Curse/Debuff 三类的消费（T3 buff_pass），其余 kind 进框架但暂走
/// 「原值直注」兼容路径（行为与现状一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuffKind {
    Buff,
    Guard,
    Warcry,
    Aura,
    AuraDebuff,
    Debuff,
    Curse,
    CurseBuff,
    Link,
}

/// 一个 buff 技能的注入规格（M3 T0 接口契约，蓝图 §2.4；字段语义对照 PoB2 buff 表）。
///
/// pobr-build（T3）从 granted_effects 数据构造；分类规则：`skill_types` 含 Aura→Aura、
/// 含 Mark→Curse(is_mark)、granted_effect 的 buff 语义列（M1 statmap 边车）→其余类。
#[derive(Debug, Clone)]
pub struct BuffSpec {
    /// buff 名（PoB2 buff.name，`AffectedBy<名>` 条件用）。
    pub name: String,
    pub kind: BuffKind,
    /// 来源技能（归因 + curse priority socket 计算）。
    pub skill_id: String,
    /// buff 携带词条（granted_effect stat 经 statmap/映射产物）。
    pub mods: Vec<Modifier>,
    /// 默认 1.0（PoB2 calcLib.mod Magnitude 的来源值）。
    pub magnitude: f64,
    /// socket group 槽名（curse priority）。
    pub slot: Option<String>,
    /// 组内宝石序（curse priority，cap 8）。
    pub socket_index: u32,
    pub is_mark: bool,
    pub ignore_curse_limit: bool,
}

#[derive(Debug, Clone)]
pub struct CalculationSession {
    env: Env,
    unsupported_modifier_texts: Vec<String>,
}

impl CalculationSession {
    pub fn new(input: MinimalInput) -> Self {
        let enemy_evasion = input.enemy_evasion;
        let mut env = Env::new(Actor::new(1, ActorBaseStats::from(input)));
        env.enemy.base.evasion = enemy_evasion;

        Self {
            env,
            unsupported_modifier_texts: Vec::new(),
        }
    }

    pub fn with_config(mut self, cfg: CalcConfig) -> Self {
        self.env.cfg = cfg;
        self
    }

    /// 注入运行时常量包（M0-W3 注入管道）：写入 `env.cfg.constants`，随 cfg 线程化
    /// 到全部 calc 函数。未调用时为 `Default`（fallback，与入库 JSON 逐值相等）。
    ///
    /// **顺序约束**：[`with_config`](Self::with_config) 会整体覆盖 cfg（含本字段），
    /// 故须在其**之后**调用；编排层（pobr-build `calculate_with_data`）遵守此序。
    pub fn set_constants(&mut self, constants: RuntimeConstants) {
        self.env.cfg.constants = constants;
    }

    /// 在已注入全部来源后，向计算上下文写入一个资源/属性缩放量（PoB2 PerStat 的分母变量
    /// 的总量，如 `Spirit`/`Strength`/`Level`）。供 `+N to <stat> per M <resource>` 这类
    /// 词条经 [`crate::ModTag::Multiplier`] 在 `perform` 查询时按 `value / div` 展开。
    ///
    /// 须在 [`perform_minimal`](Self::perform_minimal) 之前调用；资源量通常由编排层在全部
    /// 来源注入后用 [`base_sum`](Self::base_sum) 读取（属性/Spirit 的 BASE 总量）后回填。
    pub fn set_multiplier(&mut self, name: impl Into<String>, value: f64) {
        self.env.cfg.multipliers.insert(name.into(), value);
    }

    /// 在已注入全部来源后，向计算上下文写入一个布尔条件（供 `ModTag::Condition` 词条
    /// 在 `perform` 查询时判定）。与 [`set_multiplier`](Self::set_multiplier) 同为
    /// 编排层回填入口，须在 [`perform_minimal`](Self::perform_minimal) 之前调用。
    pub fn set_condition(&mut self, name: impl Into<String>, value: bool) {
        self.env.cfg.conditions.insert(name.into(), value);
    }

    /// 查询玩家 modDB 中某 FLAG modifier 是否为真（按当前 cfg）。供编排层把来源授予的
    /// `Condition:<X>` flag（如 Bonded 激活源）桥接为 cfg 条件。
    pub fn has_flag(&self, name: &str) -> bool {
        self.env
            .player
            .mod_db
            .flag(&self.env.cfg, ModName::from(name))
    }

    pub fn add_modifier_texts(
        &mut self,
        texts: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> Result<(), ParseError> {
        for text in texts {
            let text = text.as_ref();
            let outcome = parse_mod(text)?;
            match outcome.status {
                ParseStatus::Parsed => self.env.player.mod_db.add_list(outcome.mods),
                ParseStatus::Unsupported => {
                    if let Some(unparsed) = outcome.unparsed {
                        self.unsupported_modifier_texts.push(unparsed);
                    }
                }
            }
        }

        Ok(())
    }

    /// 直接注入已构造好的 modifier（角色基础值 / 战役奖励 / 抗性惩罚等入口的产物），
    /// 保留其 `SourceId` 归因。
    pub fn add_modifiers(&mut self, modifiers: impl IntoIterator<Item = Modifier>) {
        self.env.player.mod_db.add_list(modifiers);
    }

    /// 接入一件装备：按 section（implicit / explicit / enchant）解析其词条文本为
    /// 带槽位 + 来源类别归因的 modifier 并注入计算，无法解析的词条收集进
    /// `unsupported_modifier_texts`。
    pub fn add_item(&mut self, slot: EquipmentSlot, item: &Item) -> Result<(), ParseError> {
        let ingest = ingest_item(slot, item)?;
        self.env.player.mod_db.add_list(ingest.modifiers);
        self.unsupported_modifier_texts.extend(ingest.unsupported);
        Ok(())
    }

    /// 接入一组已分配天赋节点：解析每个节点的词条文本为带节点归因
    /// （[`SourceKind::PassiveNode`] / [`SourceKind::AscendancyNode`]）的 modifier 并注入计算，
    /// 无法解析的词条收集进 `unsupported_modifier_texts`。
    ///
    /// [`SourceKind::PassiveNode`]: pobr_data::source::SourceKind::PassiveNode
    /// [`SourceKind::AscendancyNode`]: pobr_data::source::SourceKind::AscendancyNode
    pub fn add_passive_nodes(&mut self, nodes: &[AllocatedNode]) -> Result<(), ParseError> {
        let ingest = ingest_passive_nodes(nodes)?;
        self.env.player.mod_db.add_list(ingest.modifiers);
        self.unsupported_modifier_texts.extend(ingest.unsupported);
        Ok(())
    }

    /// 接入一颗宝石：解析其词条文本为带宝石归因的 modifier 并注入计算，
    /// 无法解析的词条收集进 `unsupported_modifier_texts`。
    ///
    /// 主动宝石归因到 `SourceKind::SkillGem`，辅助宝石归因到 `SourceKind::SupportGem`
    /// （并链接到被支援主动技能的 source，若 `supported_gem_id` 可得）。
    pub fn add_gem(&mut self, gem: &GemModSource) -> Result<(), ParseError> {
        let ingest = ingest_gem(gem)?;
        self.env.player.mod_db.add_list(ingest.modifiers);
        self.unsupported_modifier_texts.extend(ingest.unsupported);
        Ok(())
    }

    /// 接入一颗主动技能宝石（`SourceKind::SkillGem` 归因）。`add_gem` 的便捷封装。
    pub fn add_skill_gem(&mut self, gem: &GemModSource) -> Result<(), ParseError> {
        self.add_gem(gem)
    }

    /// 接入一颗辅助宝石（`SourceKind::SupportGem` 归因）。`add_gem` 的便捷封装。
    pub fn add_support_gem(&mut self, gem: &GemModSource) -> Result<(), ParseError> {
        self.add_gem(gem)
    }

    /// 注入一个 buff 技能规格（M3 T0-4 接口契约，蓝图 §2.4）。
    ///
    /// **本阶段只存不消费**：spec 入 `Env::buff_skills`，等 T3 的 `buff_pass`
    /// （env_finalize 阶段 4）落地后才参与计算——在此之前调用本 API 对输出逐值无影响。
    pub fn add_buff_skill(&mut self, spec: BuffSpec) {
        self.env.buff_skills.push(spec);
    }

    /// 注入「keystone 名 → 该 keystone 的 modifier 列表」映射（M3 T0-4 接口契约，
    /// 蓝图 §2.4，T5 mergeKeystones 消费）。
    ///
    /// **本阶段只存不消费**：map 入 `Env::keystone_mods`，等 T5 的 `merge_keystones`
    /// （env_finalize 阶段 1/5）落地后，词条授予的 keystone 才据此注入 modDB。
    pub fn set_keystone_mods(&mut self, map: BTreeMap<String, Vec<Modifier>>) {
        self.env.keystone_mods = map;
    }

    /// 注入内建 buff 定义表（M3-T2 B3；`overlay/buff_definitions.json` 经
    /// pobr-gamedata 加载后由编排层喂入）。env_finalize 阶段 6
    /// `expand_misc_buffs` 消费——整段吃 `cfg.mode_combat` 门控（默认 false），
    /// 故未显式置位 mode_combat 时注入与否输出逐值不变。
    pub fn set_buff_definitions(&mut self, defs: Vec<BuffDef>) {
        self.env.buff_definitions = defs;
    }

    /// 注入 handler 注册表（数据条目 `handler_id` → Rust 真逻辑；聚合点 =
    /// pobr-build `handlers::build_registry()`）。未调用时为空注册表——
    /// handler 条目保守零输出（进 unhandled 报表，宁缺勿错值）。
    pub fn set_buff_handler_registry(&mut self, registry: Arc<HandlerRegistry>) {
        self.env.buff_handler_registry = registry;
    }

    /// 按 `(config_level, tier)` 初始化敌人（怪物缩放 + 档位加成），写入 `Env.enemy`
    /// 的标量基础与 modDB（归因 [`SourceKind::EnemyConfig`]）。
    ///
    /// 仅在 `CalcConfig::mode_effective == true`（经 [`with_config`](Self::with_config)
    /// 设置）时影响有效 DPS / 命中 / 敌人减伤；面板口径不读取敌人交互，保持与历史输出一致。
    ///
    /// [`SourceKind::EnemyConfig`]: pobr_data::source::SourceKind::EnemyConfig
    pub fn setup_enemy(&mut self, config_level: u32, tier: EnemyTier) {
        super::setup_env::setup_enemy(&mut self.env, config_level, tier);
    }

    /// 直接向**敌人** modDB 注入已构造好的 modifier（保留 `SourceId` 归因）。
    ///
    /// M3-T1 A5 config 主路径入口：config 解释器把 `conditionEnemy<X>` 等条目
    /// 落成 enemy 桶产物（vendor `enemyModList:NewMod` 语义，ConfigOptions.lua
    /// 各 enemy 条目；`SourceKind::EnemyConfig` 归因），编排层经此注入。
    /// 须在 [`setup_enemy`](Self::setup_enemy) 之后调用（与 vendor enemy modDB
    /// 装配序一致；BASE 求和本身次序无关）。
    pub fn add_enemy_modifiers(&mut self, modifiers: impl IntoIterator<Item = Modifier>) {
        self.env.enemy.mod_db.add_list(modifiers);
    }

    /// 注入玩家施加的元素**曝光**（`[fire, cold, lightning]`，PoB2 config 默认每点 -20%
    /// 抗），写入 enemy modDB 并按 [`reduce_enemy_exposure`] 折算为 `<Element>Resist` 减项。
    /// 仅在有效口径（`mode_effective`）下对伤害生效。须在 [`setup_enemy`](Self::setup_enemy)
    /// 之后调用。
    ///
    /// [`reduce_enemy_exposure`]: super::setup_env::reduce_enemy_exposure
    pub fn apply_enemy_exposure(&mut self, elements: [bool; 3], magnitude: f64) {
        let names = ["FireExposure", "ColdExposure", "LightningExposure"];
        for (on, name) in elements.iter().zip(names) {
            if *on {
                self.env.enemy.mod_db.add_list([Modifier::number(
                    ModName::from(name),
                    ModType::Base,
                    magnitude,
                )
                .with_source("config exposure")]);
            }
        }
        super::setup_env::reduce_enemy_exposure(&mut self.env.enemy.mod_db, &self.env.cfg);
    }

    pub fn perform_minimal(&mut self) -> MinimalOutput {
        perform(&mut self.env).expect("CalculationSession constructs a valid player actor");
        MinimalOutput::from_output_and_breakdown(
            &self.env.player.output,
            &self.env.player.breakdown,
        )
    }

    pub fn unsupported_modifier_texts(&self) -> &[String] {
        &self.unsupported_modifier_texts
    }

    /// 取玩家 modDB 中某 ModName 的 BASE 之和（按当前 cfg）。供编排层在全部来源注入后
    /// 读取总属性（Strength/Dexterity/Intelligence）以派生 life/mana/accuracy（属性派生需
    /// 用**最终**属性，而非仅职业基础）。
    pub fn base_sum(&self, name: &str) -> f64 {
        self.env
            .player
            .mod_db
            .sum(ModType::Base, &self.env.cfg, &[ModName::from(name)])
    }

    /// 取玩家完整 [`OutputTable`]（`perform`/`perform_minimal` 后填满）。包含
    /// armour/evasion/ES、异常、EHP、技能机制等全部 fill 阶段字段——`MinimalOutput`
    /// 仅是其攻击/抗性子集，需要完整输出时用此。
    pub fn output(&self) -> &OutputTable {
        &self.env.player.output
    }

    /// 诊断辅助：列出玩家 modDB 中名为 `name` 的全部 modifier（含 cfg 不匹配者），
    /// 供 parity 调试逐来源核对贡献。
    pub fn mods_named(&self, name: &str) -> Vec<&Modifier> {
        let target = ModName::from(name);
        self.env
            .player
            .mod_db
            .iter_mods()
            .filter(|m| m.name == target)
            .collect()
    }
}
