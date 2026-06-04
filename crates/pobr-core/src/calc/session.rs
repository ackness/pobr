use pobr_data::prelude::*;

use crate::item::ingest_item;
use crate::mod_parser::{ParseError, ParseStatus, parse_mod};
use crate::{CalcConfig, Modifier};

use super::{Actor, ActorBaseStats, Env, MinimalInput, MinimalOutput, perform};

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
}
