use crate::skill::{SkillFlags, SkillTypes};
use crate::stat::StatId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GemLevel(pub u8);

#[derive(Debug, Clone)]
pub struct Gem {
    pub granted_effect: String,
    pub level: GemLevel,
    pub quality: u8,
    pub is_support: bool,
}

#[derive(Debug, Clone)]
pub struct SkillEffect {
    pub id: String,
    pub skill_types: SkillTypes,
    pub flags: SkillFlags,
    pub base_stats: Vec<(StatId, f64)>,
}
