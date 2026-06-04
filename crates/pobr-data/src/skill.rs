use crate::stat::StatId;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SkillTypes(u64);

impl SkillTypes {
    pub const NONE: Self = Self(0);
    pub const ATTACK: Self = Self(1 << 0);
    pub const SPELL: Self = Self(1 << 1);

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SkillFlags(u64);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SkillId(String);

impl From<&str> for SkillId {
    fn from(value: &str) -> Self {
        Self(value.into())
    }
}

#[derive(Debug, Clone)]
pub struct SkillDefinition {
    pub id: SkillId,
    pub stats: Vec<StatId>,
    pub skill_types: SkillTypes,
}
