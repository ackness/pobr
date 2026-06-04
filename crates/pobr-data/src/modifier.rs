use std::fmt;
use std::ops::{BitOr, BitOrAssign};

use serde::{Deserialize, Serialize};

use crate::stat::StatId;

pub type ModName = StatId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModType {
    Base,
    Inc,
    More,
    Flag,
    Override,
    List,
}

impl ModType {
    pub fn as_trace_label(self) -> &'static str {
        match self {
            Self::Base => "BASE",
            Self::Inc => "INC",
            Self::More => "MORE",
            Self::Flag => "FLAG",
            Self::Override => "OVERRIDE",
            Self::List => "LIST",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ModFlags(u64);

impl ModFlags {
    pub const NONE: Self = Self(0);
    pub const ATTACK: Self = Self(1 << 0);
    pub const SPELL: Self = Self(1 << 1);
    pub const MELEE: Self = Self(1 << 2);
    pub const PROJECTILE: Self = Self(1 << 3);
    pub const AREA: Self = Self(1 << 4);

    pub fn bits(self) -> u64 {
        self.0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }
}

impl BitOr for ModFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for ModFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl fmt::Debug for ModFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ModFlags({:#x})", self.0)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct KeywordFlags(u64);

impl KeywordFlags {
    pub const NONE: Self = Self(0);

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }
}

impl BitOr for KeywordFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl fmt::Debug for KeywordFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "KeywordFlags({:#x})", self.0)
    }
}
