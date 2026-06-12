use std::ops::{BitOr, BitOrAssign};

use crate::stat::StatId;

/// PoE2 技能类型位掩码（对照 PoB2 `src/Data/Global.lua::SkillType`）。
///
/// 每个常量对应 PoB2 枚举值对应的位（`1 << (enum_value - 1)`）。
/// 未在此列出的较冷门类型可用 `SkillTypes::from_pob2_index` 构造。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SkillTypes(u64);

impl SkillTypes {
    pub const NONE: Self = Self(0);
    /// PoB2 SkillType.Attack = 1
    pub const ATTACK: Self = Self(1 << 0);
    /// PoB2 SkillType.Spell = 2
    pub const SPELL: Self = Self(1 << 1);
    /// PoB2 SkillType.Projectile = 3
    pub const PROJECTILE: Self = Self(1 << 2);
    /// PoB2 SkillType.Buff = 5
    pub const BUFF: Self = Self(1 << 4);
    /// PoB2 SkillType.Minion = 6
    pub const MINION: Self = Self(1 << 5);
    /// PoB2 SkillType.Area = 8
    pub const AREA: Self = Self(1 << 7);
    /// PoB2 SkillType.Duration = 9
    pub const DURATION: Self = Self(1 << 8);
    /// PoB2 SkillType.HasReservation = 12
    pub const HAS_RESERVATION: Self = Self(1 << 11);
    /// PoB2 SkillType.ReservationBecomesCost = 13
    pub const RESERVATION_BECOMES_COST: Self = Self(1 << 12);
    /// PoB2 SkillType.Chains = 19
    pub const CHAINS: Self = Self(1 << 18);
    /// PoB2 SkillType.Melee = 20
    pub const MELEE: Self = Self(1 << 19);
    /// PoB2 SkillType.Multicastable = 22（可被 Spell Echo 重复）
    pub const MULTICASTABLE: Self = Self(1 << 21);
    /// PoB2 SkillType.Triggerable = 31
    pub const TRIGGERABLE: Self = Self(1 << 30);
    /// PoB2 SkillType.Triggers = 32
    pub const TRIGGERS: Self = Self(1 << 31);
    /// PoB2 SkillType.Trapped = 33（陷阱投掷技能）
    pub const TRAPPED: Self = Self(1 << 32);
    /// PoB2 SkillType.RemoteMined = 36（地雷技能）
    pub const REMOTE_MINED: Self = Self(1 << 35);
    /// PoB2 SkillType.Triggered = 37
    pub const TRIGGERED: Self = Self(1 << 36);
    /// PoB2 SkillType.Aura = 39
    pub const AURA: Self = Self(1 << 38);
    /// PoB2 SkillType.Channel = 48
    pub const CHANNEL: Self = Self(1 << 47);
    /// PoB2 SkillType.Warcry = 63
    pub const WARCRY: Self = Self(1 << 62);

    /// 从 PoB2 枚举值（1-based）构造 `SkillTypes`，允许使用尚未命名为常量的类型。
    pub fn from_pob2_index(index: u32) -> Self {
        if index == 0 || index > 64 {
            Self::NONE
        } else {
            Self(1 << (index - 1))
        }
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn bits(self) -> u64 {
        self.0
    }
}

impl BitOr for SkillTypes {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for SkillTypes {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
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
