use std::collections::HashMap;

use pobr_data::prelude::*;

#[derive(Debug, Clone, Default)]
pub struct CalcConfig {
    pub flags: ModFlags,
    pub keyword_flags: KeywordFlags,
    pub skill_types: SkillTypes,
    pub damage_type: Option<DamageType>,
    pub conditions: HashMap<String, bool>,
    pub multipliers: HashMap<String, f64>,
}

impl CalcConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn attack() -> Self {
        Self::new()
            .with_flags(ModFlags::ATTACK)
            .with_skill_types(SkillTypes::ATTACK)
    }

    pub fn with_flags(mut self, flags: ModFlags) -> Self {
        self.flags = flags;
        self
    }

    pub fn with_keyword_flags(mut self, keyword_flags: KeywordFlags) -> Self {
        self.keyword_flags = keyword_flags;
        self
    }

    pub fn with_skill_types(mut self, skill_types: SkillTypes) -> Self {
        self.skill_types = skill_types;
        self
    }

    pub fn with_damage_type(mut self, damage_type: DamageType) -> Self {
        self.damage_type = Some(damage_type);
        self
    }

    pub fn with_condition(mut self, name: impl Into<String>, enabled: bool) -> Self {
        self.conditions.insert(name.into(), enabled);
        self
    }

    pub fn with_multiplier(mut self, name: impl Into<String>, value: f64) -> Self {
        self.multipliers.insert(name.into(), value);
        self
    }

    pub fn condition(&self, name: &str) -> bool {
        self.conditions.get(name).copied().unwrap_or(false)
    }

    pub fn multiplier(&self, name: &str) -> f64 {
        self.multipliers.get(name).copied().unwrap_or(0.0)
    }
}
