use pobr_data::prelude::*;

use crate::CalcConfig;

#[derive(Debug, Clone, PartialEq)]
pub enum ModValue {
    Number(f64),
    Bool(bool),
    Text(String),
}

impl ModValue {
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Self::Number(value) => Some(*value),
            Self::Bool(value) => Some(if *value { 1.0 } else { 0.0 }),
            Self::Text(_) => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            Self::Number(value) => Some(*value != 0.0),
            Self::Text(_) => None,
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(value) => Some(value),
            Self::Number(_) | Self::Bool(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ModTag {
    Condition { var: String, negated: bool },
    Multiplier { var: String, limit: Option<f64> },
    DamageType(DamageType),
    SkillTypes(SkillTypes),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Modifier {
    pub name: ModName,
    pub mod_type: ModType,
    pub value: ModValue,
    pub source: Option<String>,
    pub origin: Option<ModifierSource>,
    pub flags: ModFlags,
    pub keyword_flags: KeywordFlags,
    pub tags: Vec<ModTag>,
}

impl Modifier {
    pub fn number(name: impl Into<ModName>, mod_type: ModType, value: f64) -> Self {
        Self::new(name, mod_type, ModValue::Number(value))
    }

    pub fn flag(name: impl Into<ModName>) -> Self {
        Self::new(name, ModType::Flag, ModValue::Bool(true))
    }

    pub fn text(name: impl Into<ModName>, mod_type: ModType, value: impl Into<String>) -> Self {
        Self::new(name, mod_type, ModValue::Text(value.into()))
    }

    pub fn new(name: impl Into<ModName>, mod_type: ModType, value: ModValue) -> Self {
        Self {
            name: name.into(),
            mod_type,
            value,
            source: None,
            origin: None,
            flags: ModFlags::NONE,
            keyword_flags: KeywordFlags::NONE,
            tags: Vec::new(),
        }
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    pub fn with_origin(mut self, mut origin: ModifierSource) -> Self {
        if origin.stat_id.is_none() {
            origin.stat_id = Some(self.name.clone());
        }
        if origin.mod_type.is_none() {
            origin.mod_type = Some(self.mod_type);
        }
        self.origin = Some(origin);
        self
    }

    pub fn with_flags(mut self, flags: ModFlags) -> Self {
        self.flags = flags;
        self
    }

    pub fn with_keyword_flags(mut self, keyword_flags: KeywordFlags) -> Self {
        self.keyword_flags = keyword_flags;
        self
    }

    pub fn with_tag(mut self, tag: ModTag) -> Self {
        self.tags.push(tag);
        self
    }

    pub fn matches(&self, cfg: &CalcConfig) -> bool {
        if !self.flags.is_empty() && !self.flags.intersects(cfg.flags) {
            return false;
        }

        if !self.keyword_flags.is_empty() && !self.keyword_flags.intersects(cfg.keyword_flags) {
            return false;
        }

        self.tags.iter().all(|tag| match tag {
            ModTag::Condition { var, negated } => {
                let enabled = cfg.condition(var);
                if *negated { !enabled } else { enabled }
            }
            ModTag::Multiplier { .. } => true,
            ModTag::DamageType(damage_type) => cfg.damage_type == Some(*damage_type),
            ModTag::SkillTypes(skill_types) => {
                skill_types.is_empty() || skill_types.intersects(cfg.skill_types)
            }
        })
    }

    pub fn effective_number(&self, cfg: &CalcConfig) -> Option<f64> {
        let mut value = self.value.as_number()?;

        for tag in &self.tags {
            if let ModTag::Multiplier { var, limit } = tag {
                let multiplier = cfg.multiplier(var);
                value *= limit.map_or(multiplier, |max| multiplier.min(max));
            }
        }

        Some(value)
    }
}
