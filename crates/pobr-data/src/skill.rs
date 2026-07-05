use std::ops::{BitOr, BitOrAssign};

use crate::stat::StatId;

/// 位掩码字数（320 位，覆盖 PoB2 SkillType 枚举全域——当前最大 id=290，
/// 见 `Global.lua` SkillType 表；Companion=198、Grenade=159 等高位 id 都在域内）。
const SKILL_TYPE_WORDS: usize = 5;

/// PoE2 技能类型位掩码（对照 PoB2 `src/Data/Global.lua::SkillType`）。
///
/// 每个常量对应 PoB2 枚举值对应的位（`bit(enum_value - 1)`）。
/// 未在此列出的较冷门类型可用 [`SkillTypes::from_pob2_index`] 构造。
///
/// 内部为 `[u64; SKILL_TYPE_WORDS]`（320 位）——历史 u64 版本装不下 id > 64 的
/// 类型（Meta=122、Grenade=159、Companion=198 等，旧实现对其静默归 NONE）。
/// [`Debug`] 输出对高位全零的值保持旧 newtype 格式（`SkillTypes(2)`），预编译
/// 缓存（`parsed_mods.json` 以 Debug 字符串存 tag）对既有条目逐字节不变。
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct SkillTypes([u64; SKILL_TYPE_WORDS]);

impl SkillTypes {
    pub const NONE: Self = Self([0; SKILL_TYPE_WORDS]);
    /// PoB2 SkillType.Attack = 1
    pub const ATTACK: Self = Self::bit(0);
    /// PoB2 SkillType.Spell = 2
    pub const SPELL: Self = Self::bit(1);
    /// PoB2 SkillType.Projectile = 3
    pub const PROJECTILE: Self = Self::bit(2);
    /// PoB2 SkillType.Buff = 5
    pub const BUFF: Self = Self::bit(4);
    /// PoB2 SkillType.Minion = 6
    pub const MINION: Self = Self::bit(5);
    /// PoB2 SkillType.Area = 8
    pub const AREA: Self = Self::bit(7);
    /// PoB2 SkillType.Duration = 9
    pub const DURATION: Self = Self::bit(8);
    /// PoB2 SkillType.HasReservation = 12
    pub const HAS_RESERVATION: Self = Self::bit(11);
    /// PoB2 SkillType.ReservationBecomesCost = 13
    pub const RESERVATION_BECOMES_COST: Self = Self::bit(12);
    /// PoB2 SkillType.Chains = 19
    pub const CHAINS: Self = Self::bit(18);
    /// PoB2 SkillType.Melee = 20
    pub const MELEE: Self = Self::bit(19);
    /// PoB2 SkillType.Multicastable = 22（可被 Spell Echo 重复）
    pub const MULTICASTABLE: Self = Self::bit(21);
    /// PoB2 SkillType.Triggerable = 31
    pub const TRIGGERABLE: Self = Self::bit(30);
    /// PoB2 SkillType.Triggers = 32
    pub const TRIGGERS: Self = Self::bit(31);
    /// PoB2 SkillType.Trapped = 33（陷阱投掷技能）
    pub const TRAPPED: Self = Self::bit(32);
    /// PoB2 SkillType.RemoteMined = 36（地雷技能）
    pub const REMOTE_MINED: Self = Self::bit(35);
    /// PoB2 SkillType.Triggered = 37
    pub const TRIGGERED: Self = Self::bit(36);
    /// PoB2 SkillType.Aura = 39
    pub const AURA: Self = Self::bit(38);
    /// PoB2 SkillType.Channel = 48
    pub const CHANNEL: Self = Self::bit(47);
    /// PoB2 SkillType.Warcry = 63
    pub const WARCRY: Self = Self::bit(62);
    /// PoB2 SkillType.Herald = 52
    pub const HERALD: Self = Self::bit(51);
    /// PoB2 SkillType.SummonsTotem = 25（Ancestral Bond「Totems reserve N
    /// Spirit each」的 ExtraSpirit tag 作用域 + 预留循环入选位）
    pub const SUMMONS_TOTEM: Self = Self::bit(24);
    /// PoB2 SkillType.Banner = 89（「Banner Skills have N% increased Aura
    /// Magnitudes」等 banner 域词条的作用域）
    pub const BANNER: Self = Self::bit(88);
    /// PoB2 SkillType.Meta = 122（Blasphemy / Spellslinger / Archmage /
    /// Cast-on-X 等 meta gem；「Meta Skills have N% increased Reservation
    /// Efficiency」词条的作用域）
    pub const META: Self = Self::bit(121);
    /// PoB2 SkillType.Persistent = 140（banner/herald/aura 等持续预留效果；
    /// 「Persistent Buffs have N% less Reservation」（Tactician『A Solid
    /// Plan』）以 Persistent+Buff 双 tag AND 限定作用域）
    pub const PERSISTENT: Self = Self::bit(139);

    /// 第 `index` 位（0-based）置 1 的掩码（const 构造）。
    const fn bit(index: u32) -> Self {
        let mut words = [0u64; SKILL_TYPE_WORDS];
        words[(index / 64) as usize] = 1u64 << (index % 64);
        Self(words)
    }

    /// 从 PoB2 枚举值（1-based）构造 `SkillTypes`，允许使用尚未命名为常量的类型。
    /// 超出位域容量（>320）归 NONE（保守）。
    pub fn from_pob2_index(index: u32) -> Self {
        if index == 0 || index > (SKILL_TYPE_WORDS as u32) * 64 {
            Self::NONE
        } else {
            Self::bit(index - 1)
        }
    }

    /// 从 PoB2 枚举名构造（`Global.lua::SkillType` **全量** 290 名，生成边车
    /// `skill_type_names.txt`，vendor 升级后 regen）——SkillType 名→位映射的
    /// **单一入口**，parser tag 侧与编排 cfg 侧共用（数据驱动 A1：取代
    /// template.rs / special_mod.rs / conditions.rs 的手工白名单副本）。
    /// 未知名（vendor 枚举外）→ `None`，由调用侧决定丢弃语义。
    pub fn from_pob2_name(name: &str) -> Option<Self> {
        crate::skill_type_names::lookup(name).map(Self::from_pob2_index)
    }

    pub fn is_empty(self) -> bool {
        self.0 == [0; SKILL_TYPE_WORDS]
    }

    pub fn intersects(self, other: Self) -> bool {
        let mut i = 0;
        while i < SKILL_TYPE_WORDS {
            if self.0[i] & other.0[i] != 0 {
                return true;
            }
            i += 1;
        }
        false
    }

    pub fn contains(self, other: Self) -> bool {
        let mut i = 0;
        while i < SKILL_TYPE_WORDS {
            if self.0[i] & other.0[i] != other.0[i] {
                return false;
            }
            i += 1;
        }
        true
    }

    /// 低 64 位窗口（id 1-64 的类型）。仅供旧格式序列化 / 断言用；高位类型
    /// （Meta 等）不在此窗口——完整位域走 [`Debug`] / [`SkillTypes::words`]。
    pub fn bits(self) -> u64 {
        self.0[0]
    }

    /// 完整位域字数组（低位在前）。
    pub fn words(self) -> [u64; SKILL_TYPE_WORDS] {
        self.0
    }
}

/// 高位全零时保持历史 u64 newtype 的 Debug 形（`SkillTypes(2)`）——预编译缓存
/// （`parsed_mods.json`）以 Debug 字符串存既有 tag，格式漂移会伪失效全部缓存。
/// 高位非零时输出全字数组形（`SkillTypes([lo, mid, hi])`）。
impl std::fmt::Debug for SkillTypes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0[1..] == [0; SKILL_TYPE_WORDS - 1] {
            write!(f, "SkillTypes({})", self.0[0])
        } else {
            write!(f, "SkillTypes({:?})", self.0)
        }
    }
}

impl BitOr for SkillTypes {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        let mut words = self.0;
        for (w, r) in words.iter_mut().zip(rhs.0) {
            *w |= r;
        }
        Self(words)
    }
}

impl BitOrAssign for SkillTypes {
    fn bitor_assign(&mut self, rhs: Self) {
        for (w, r) in self.0.iter_mut().zip(rhs.0) {
            *w |= r;
        }
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
