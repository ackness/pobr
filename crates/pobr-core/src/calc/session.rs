use pobr_data::prelude::*;

use crate::item::ingest_item;
use crate::mod_parser::{ParseError, ParseStatus, parse_mod};
use crate::passive::{AllocatedNode, ingest_passive_nodes};
use crate::skill_source::{GemModSource, ingest_gem};
use crate::{CalcConfig, Modifier};

use super::{Actor, ActorBaseStats, Env, MinimalInput, MinimalOutput, OutputTable, perform};

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

    /// 取玩家完整 [`OutputTable`]（`perform`/`perform_minimal` 后填满）。包含
    /// armour/evasion/ES、异常、EHP、技能机制等全部 fill 阶段字段——`MinimalOutput`
    /// 仅是其攻击/抗性子集，需要完整输出时用此。
    pub fn output(&self) -> &OutputTable {
        &self.env.player.output
    }
}
