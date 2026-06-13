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

    pub fn bits(self) -> u64 {
        self.0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    /// `self` 的所有置位都被 `other` 满足（PoB2 ModList `band(other, self) == self`）。
    /// 空集（`NONE`）是任意集合的子集。
    pub fn is_subset_of(self, other: Self) -> bool {
        self.0 & other.0 == self.0
    }

    /// 位差集：去掉 `other` 中的全部置位（PoB2 `band(self, bnot(other))`）。
    /// M4-T4 W-D1 dotCfg 的 flag 剥除原语（`CalcOffence.lua:5839-5856`）。
    pub fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }
}

/// PoB2 全位表（M4-T1 W-A1 引入，M4-I 切换 commit 起常驻——旧 5 位表与
/// `modflags-pob2` feature 双写通道已删，两次双跑 diff=0 报告见
/// `audits/rearchitecture-2026-06-10/m4-t1-modflags-dualrun-report.md`）。
///
/// 位值**逐位等于** vendor `Data/Global.lua:222-259` `ModFlag.*`（u64 字面量
/// 直接照搬，便于对拍调试）。逐常量断言见本文件 `modflags_pob2_tests`。
impl ModFlags {
    // -- Damage modes（ATTACK/SPELL 与旧表同值，定义在公共块）--
    /// `ModFlag.Hit = 0x0000000000000004`
    pub const HIT: Self = Self(0x4);
    /// `ModFlag.Dot = 0x0000000000000008`
    pub const DOT: Self = Self(0x8);
    /// `ModFlag.Cast = 0x0000000000000010`
    pub const CAST: Self = Self(0x10);
    /// `ModFlag.Thorns = 0x0000000000000020`
    pub const THORNS: Self = Self(0x20);
    // -- Damage sources --
    /// `ModFlag.Melee = 0x0000000000000100`（旧表 `1 << 2`，位值搬家）
    pub const MELEE: Self = Self(0x100);
    /// `ModFlag.Area = 0x0000000000000200`（旧表 `1 << 4`，位值搬家）
    pub const AREA: Self = Self(0x200);
    /// `ModFlag.Projectile = 0x0000000000000400`（旧表 `1 << 3`，位值搬家）
    pub const PROJECTILE: Self = Self(0x400);
    /// `ModFlag.SourceMask = 0x0000000000000600`（Area|Projectile）
    pub const SOURCE_MASK: Self = Self(0x600);
    /// `ModFlag.Ailment = 0x0000000000000800`
    pub const AILMENT: Self = Self(0x800);
    /// `ModFlag.MeleeHit = 0x0000000000001000`
    pub const MELEE_HIT: Self = Self(0x1000);
    /// `ModFlag.Weapon = 0x0000000000002000`
    pub const WEAPON: Self = Self(0x2000);
    // -- Weapon types --
    /// `ModFlag.Axe = 0x0000000000010000`
    pub const AXE: Self = Self(0x10000);
    /// `ModFlag.Bow = 0x0000000000020000`
    pub const BOW: Self = Self(0x20000);
    /// `ModFlag.Claw = 0x0000000000040000`
    pub const CLAW: Self = Self(0x40000);
    /// `ModFlag.Dagger = 0x0000000000080000`
    pub const DAGGER: Self = Self(0x80000);
    /// `ModFlag.Mace = 0x0000000000100000`
    pub const MACE: Self = Self(0x100000);
    /// `ModFlag.Staff = 0x0000000000200000`（PoE2 长杖 Quarterstaff 的 flag 名为 Staff）
    pub const STAFF: Self = Self(0x200000);
    /// `ModFlag.Sword = 0x0000000000400000`
    pub const SWORD: Self = Self(0x400000);
    /// `ModFlag.Wand = 0x0000000000800000`
    pub const WAND: Self = Self(0x800000);
    /// `ModFlag.Unarmed = 0x0000000001000000`
    pub const UNARMED: Self = Self(0x1000000);
    /// `ModFlag.Fishing = 0x0000000002000000`
    pub const FISHING: Self = Self(0x2000000);
    /// `ModFlag.Crossbow = 0x0000000004000000`
    pub const CROSSBOW: Self = Self(0x4000000);
    /// `ModFlag.Flail = 0x0000000008000000`
    pub const FLAIL: Self = Self(0x8000000);
    /// `ModFlag.Spear = 0x0000000010000000`
    pub const SPEAR: Self = Self(0x10000000);
    /// `ModFlag.Warstaff = 0x0000000020000000`
    pub const WARSTAFF: Self = Self(0x20000000);
    /// `ModFlag.Talisman = 0x0000000040000000`
    pub const TALISMAN: Self = Self(0x40000000);
    // -- Weapon classes --
    /// `ModFlag.WeaponMelee = 0x0000000100000000`
    pub const WEAPON_MELEE: Self = Self(0x1_0000_0000);
    /// `ModFlag.WeaponRanged = 0x0000000200000000`
    pub const WEAPON_RANGED: Self = Self(0x2_0000_0000);
    /// `ModFlag.Weapon1H = 0x0000000400000000`
    pub const WEAPON_1H: Self = Self(0x4_0000_0000);
    /// `ModFlag.Weapon2H = 0x0000000800000000`
    pub const WEAPON_2H: Self = Self(0x8_0000_0000);
    /// `ModFlag.WeaponMask = 0x0000000F5FFF0000`
    pub const WEAPON_MASK: Self = Self(0xF_5FFF_0000);
    /// [`weapon_flags`](Self::weapon_flags)（vendor getWeaponFlags）可产出的全部
    /// 位段并集 = `WEAPON_MASK ∪ WARSTAFF ∪ WEAPON`（vendor `WeaponMask` 字面量
    /// 不含 Warstaff 与 Weapon 位，见 `masks_are_unions_of_member_bits`）。供
    /// per-hand cfg 武器位替换（T2 W-B2）整段清位用——非 vendor 字面量，PoBR 派生。
    pub const WEAPON_SEGMENT: Self = Self(Self::WEAPON_MASK.0 | Self::WARSTAFF.0 | Self::WEAPON.0);
}

/// 武器位派生（W-A1 commit-2 引入，切换 commit 起常驻）。
impl ModFlags {
    /// `weapon_types.json` 的 `flag` 名 → 武器类型位（vendor `ModFlag[info.flag]`，
    /// `CalcActiveSkill.lua:291`）。名称→位映射表留代码侧（P1 L4 刹车：位枚举是
    /// 框架语义）；未知 flag 名 → `None`。
    pub fn weapon_type_bit(flag: &str) -> Option<Self> {
        Some(match flag {
            "Axe" => Self::AXE,
            "Bow" => Self::BOW,
            "Claw" => Self::CLAW,
            "Dagger" => Self::DAGGER,
            "Mace" => Self::MACE,
            "Staff" => Self::STAFF,
            "Sword" => Self::SWORD,
            "Wand" => Self::WAND,
            "Unarmed" => Self::UNARMED,
            "Fishing" => Self::FISHING,
            "Crossbow" => Self::CROSSBOW,
            "Flail" => Self::FLAIL,
            "Spear" => Self::SPEAR,
            "Warstaff" => Self::WARSTAFF,
            "Talisman" => Self::TALISMAN,
            _ => return None,
        })
    }

    /// 武器条目 → 完整武器位集（vendor `CalcActiveSkill.lua:274-309 getWeaponFlags`
    /// 主干逐字对照）：
    /// - `flags = ModFlag[info.flag]`（武器类型位；未知 flag → 空）；
    /// - `type ~= "None"`（非空手）时再并 `Weapon` + `Weapon1H`/`Weapon2H`
    ///   （`info.oneHand`）+ `WeaponMelee`/`WeaponRanged`（`info.melee`）。
    ///
    /// 入参对应 `weapon_types.json` 条目字段（`WeaponTypeDef` 的
    /// id/flag/one_hand/melee）。`countsAsAll1H`/`asThoughUsing` 分支本阶段不做
    /// （无消费 build，登记 M5+，蓝图 W-A1）；`MeleeHit` 不在 getWeaponFlags 内
    /// （vendor 由技能侧 `:537` 另并，归 T2 per-hand cfg）。
    pub fn weapon_flags(type_id: &str, flag: &str, one_hand: bool, melee: bool) -> Self {
        let Some(mut flags) = Self::weapon_type_bit(flag) else {
            return Self::NONE;
        };
        if type_id != "None" {
            flags |= Self::WEAPON;
            flags |= if one_hand {
                Self::WEAPON_1H
            } else {
                Self::WEAPON_2H
            };
            flags |= if melee {
                Self::WEAPON_MELEE
            } else {
                Self::WEAPON_RANGED
            };
        }
        flags
    }

    /// per-hand cfg 武器位替换（T2 W-B2；vendor `CalcOffence.lua:2369-2449`
    /// weapon1Cfg/weapon2Cfg 语义：per-hand flags 由「技能位 + **该手**武器位」
    /// 构造，不继承另一手 / 全局的武器位）。
    ///
    /// - `weapon` 为空（该 hand source 无武器位供给——非武器攻击 source 如
    ///   Shield Wall）→ 原样返回，cfg 沿用上游供给（单手 build 的全局武器位与
    ///   per-hand 位同源同值，替换 ≡ 恒等）。
    /// - 非空 → 清掉 [`WEAPON_SEGMENT`](Self::WEAPON_SEGMENT) 段后并入该手
    ///   武器位（双持下另一手的武器类型位不得泄漏进本手 pass）。
    pub fn replace_weapon_flags(self, weapon: Self) -> Self {
        if weapon.is_empty() {
            self
        } else {
            Self(self.0 & !Self::WEAPON_SEGMENT.0 | weapon.0)
        }
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

    // 具体 keyword 位，位值对齐 PoB2 `Global.lua` `KeywordFlag.*`（src/Data/Global.lua:263-292）。
    /// `KeywordFlag.Aura = 0x00000001`
    pub const AURA: Self = Self(0x0000_0001);
    /// `KeywordFlag.Curse = 0x00000002`
    pub const CURSE: Self = Self(0x0000_0002);
    /// `KeywordFlag.Totem = 0x00004000`（M6-B parser 引擎 flag_phrases 需要）
    pub const TOTEM: Self = Self(0x0000_4000);
    /// `KeywordFlag.Attack = 0x00010000`（M6-B：DMGATTACKS 默认补位 / flag_phrases）
    pub const ATTACK: Self = Self(0x0001_0000);
    /// `KeywordFlag.Spell = 0x00020000`（M6-B：DMGSPELLS 默认补位 / flag_phrases）
    pub const SPELL: Self = Self(0x0002_0000);
    /// `KeywordFlag.Hit = 0x00040000`
    pub const HIT: Self = Self(0x0004_0000);
    /// `KeywordFlag.Ailment = 0x00080000`
    pub const AILMENT: Self = Self(0x0008_0000);
    /// `KeywordFlag.Poison = 0x00200000`
    pub const POISON: Self = Self(0x0020_0000);
    /// `KeywordFlag.Bleed = 0x00400000`
    pub const BLEED: Self = Self(0x0040_0000);
    /// `KeywordFlag.Ignite = 0x00800000`
    pub const IGNITE: Self = Self(0x0080_0000);
    /// `KeywordFlag.PhysicalDot = 0x01000000`
    pub const PHYSICAL_DOT: Self = Self(0x0100_0000);
    /// `KeywordFlag.LightningDot = 0x02000000`
    pub const LIGHTNING_DOT: Self = Self(0x0200_0000);
    /// `KeywordFlag.ColdDot = 0x04000000`
    pub const COLD_DOT: Self = Self(0x0400_0000);
    /// `KeywordFlag.FireDot = 0x08000000`
    pub const FIRE_DOT: Self = Self(0x0800_0000);
    /// `KeywordFlag.ChaosDot = 0x10000000`
    pub const CHAOS_DOT: Self = Self(0x1000_0000);

    /// PoB2 `KeywordFlag.MatchAll = 0x40000000`：带此位时 mod 的 keyword 改为 ALL（子集）
    /// 匹配而非默认 ANY（参见 PoB2 Global.lua `MatchKeywordFlags`）。
    pub const MATCH_ALL: Self = Self(0x4000_0000);

    pub fn bits(self) -> u64 {
        self.0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    /// 是否携带 [`MATCH_ALL`](Self::MATCH_ALL) 位（要求 ALL 匹配）。
    pub fn requires_match_all(self) -> bool {
        self.0 & Self::MATCH_ALL.0 != 0
    }

    /// 去掉 `MATCH_ALL` 位后的实际 keyword 集合（PoB2 `band(x, MatchAllMask)`）。
    pub fn without_match_all(self) -> Self {
        Self(self.0 & !Self::MATCH_ALL.0)
    }

    /// 位差集：去掉 `other` 中的全部置位（PoB2 `band(self, bnot(other))`）。
    /// M4-T4 W-D1 dotCfg 的 `keywordFlags &= ~KeywordFlag.Hit`（`CalcOffence.lua:5838`）。
    pub fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    /// PoB2 `MatchKeywordFlags(cfg, self)`（Global.lua:316-341）：`self` 为 mod 的
    /// keywordFlags，`cfg` 为上下文 keywordFlags。
    /// - mod 去 MatchAll 后为空 → 恒匹配；
    /// - 带 MatchAll → 上下文须为 mod（去 MatchAll）的超集（ALL）；
    /// - 否则 → 任一重叠即可（ANY）。
    pub fn matches_context(self, cfg: Self) -> bool {
        let mod_masked = self.without_match_all();
        let cfg_masked = cfg.without_match_all();
        if mod_masked.is_empty() {
            return true;
        }
        if self.requires_match_all() {
            cfg_masked.0 & mod_masked.0 == mod_masked.0
        } else {
            cfg_masked.intersects(mod_masked)
        }
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

/// 位值断言（W-A1 commit-1 门禁）：逐常量 == vendor `Data/Global.lua:222-259`
/// 的 `ModFlag.*` 字面量（vendor commit 见 `vendor/.pob2-version.txt`）。
#[cfg(test)]
mod modflags_pob2_tests {
    use super::ModFlags;

    #[test]
    fn bit_values_match_pob2_global_lua() {
        // Damage modes（Global.lua:223-229）
        assert_eq!(ModFlags::ATTACK.bits(), 0x0000000000000001);
        assert_eq!(ModFlags::SPELL.bits(), 0x0000000000000002);
        assert_eq!(ModFlags::HIT.bits(), 0x0000000000000004);
        assert_eq!(ModFlags::DOT.bits(), 0x0000000000000008);
        assert_eq!(ModFlags::CAST.bits(), 0x0000000000000010);
        assert_eq!(ModFlags::THORNS.bits(), 0x0000000000000020);
        // Damage sources（:231-237）
        assert_eq!(ModFlags::MELEE.bits(), 0x0000000000000100);
        assert_eq!(ModFlags::AREA.bits(), 0x0000000000000200);
        assert_eq!(ModFlags::PROJECTILE.bits(), 0x0000000000000400);
        assert_eq!(ModFlags::SOURCE_MASK.bits(), 0x0000000000000600);
        assert_eq!(ModFlags::AILMENT.bits(), 0x0000000000000800);
        assert_eq!(ModFlags::MELEE_HIT.bits(), 0x0000000000001000);
        assert_eq!(ModFlags::WEAPON.bits(), 0x0000000000002000);
        // Weapon types（:239-253）
        assert_eq!(ModFlags::AXE.bits(), 0x0000000000010000);
        assert_eq!(ModFlags::BOW.bits(), 0x0000000000020000);
        assert_eq!(ModFlags::CLAW.bits(), 0x0000000000040000);
        assert_eq!(ModFlags::DAGGER.bits(), 0x0000000000080000);
        assert_eq!(ModFlags::MACE.bits(), 0x0000000000100000);
        assert_eq!(ModFlags::STAFF.bits(), 0x0000000000200000);
        assert_eq!(ModFlags::SWORD.bits(), 0x0000000000400000);
        assert_eq!(ModFlags::WAND.bits(), 0x0000000000800000);
        assert_eq!(ModFlags::UNARMED.bits(), 0x0000000001000000);
        assert_eq!(ModFlags::FISHING.bits(), 0x0000000002000000);
        assert_eq!(ModFlags::CROSSBOW.bits(), 0x0000000004000000);
        assert_eq!(ModFlags::FLAIL.bits(), 0x0000000008000000);
        assert_eq!(ModFlags::SPEAR.bits(), 0x0000000010000000);
        assert_eq!(ModFlags::WARSTAFF.bits(), 0x0000000020000000);
        assert_eq!(ModFlags::TALISMAN.bits(), 0x0000000040000000);
        // Weapon classes（:255-259）
        assert_eq!(ModFlags::WEAPON_MELEE.bits(), 0x0000000100000000);
        assert_eq!(ModFlags::WEAPON_RANGED.bits(), 0x0000000200000000);
        assert_eq!(ModFlags::WEAPON_1H.bits(), 0x0000000400000000);
        assert_eq!(ModFlags::WEAPON_2H.bits(), 0x0000000800000000);
        assert_eq!(ModFlags::WEAPON_MASK.bits(), 0x0000000F5FFF0000);
    }

    /// `SourceMask`/`WeaponMask` 是成员位的并（vendor 注释语义；mask 非独立位）。
    #[test]
    fn masks_are_unions_of_member_bits() {
        assert_eq!(
            ModFlags::SOURCE_MASK.bits(),
            (ModFlags::AREA | ModFlags::PROJECTILE).bits()
        );
        let weapon_union = ModFlags::AXE
            | ModFlags::BOW
            | ModFlags::CLAW
            | ModFlags::DAGGER
            | ModFlags::MACE
            | ModFlags::STAFF
            | ModFlags::SWORD
            | ModFlags::WAND
            | ModFlags::UNARMED
            | ModFlags::FISHING
            | ModFlags::CROSSBOW
            | ModFlags::FLAIL
            | ModFlags::SPEAR
            | ModFlags::WARSTAFF
            | ModFlags::TALISMAN
            | ModFlags::WEAPON_MELEE
            | ModFlags::WEAPON_RANGED
            | ModFlags::WEAPON_1H
            | ModFlags::WEAPON_2H;
        // 0xF5FFF0000 = 武器类型位段 + 武器分类位段（0xF00000000），唯独**不含
        // Warstaff(0x20000000)**——vendor 字面量如此（Warstaff 是遗留条目，基底
        // 数据无使用，见 catalog/weapon_types.rs 模块 doc），逐位照搬。
        assert_eq!(
            ModFlags::WEAPON_MASK.bits(),
            weapon_union.bits() & !ModFlags::WARSTAFF.bits()
        );
    }

    /// getWeaponFlags 派生（vendor CalcActiveSkill.lua:274-309）：武器类型位 +
    /// Weapon + 1H/2H + Melee/Ranged；空手（type=None）只有 Unarmed 位。
    #[test]
    fn weapon_flags_derivation_matches_get_weapon_flags() {
        // One Hand Mace（one_hand=true, melee=true, flag=Mace）
        assert_eq!(
            ModFlags::weapon_flags("One Hand Mace", "Mace", true, true),
            ModFlags::MACE | ModFlags::WEAPON | ModFlags::WEAPON_1H | ModFlags::WEAPON_MELEE
        );
        // Bow（one_hand=false, melee=false）
        assert_eq!(
            ModFlags::weapon_flags("Bow", "Bow", false, false),
            ModFlags::BOW | ModFlags::WEAPON | ModFlags::WEAPON_2H | ModFlags::WEAPON_RANGED
        );
        // 长杖（weapon_types 表 id=Staff / flag=Staff，label=Quarterstaff）
        assert_eq!(
            ModFlags::weapon_flags("Staff", "Staff", false, true),
            ModFlags::STAFF | ModFlags::WEAPON | ModFlags::WEAPON_2H | ModFlags::WEAPON_MELEE
        );
        // 空手：type == "None" → 不并 Weapon/1H2H/MeleeRanged（vendor :296 守卫）。
        assert_eq!(
            ModFlags::weapon_flags("None", "Unarmed", true, true),
            ModFlags::UNARMED
        );
        // 未知 flag 名 → 空。
        assert_eq!(
            ModFlags::weapon_flags("Sceptre", "Sceptre", true, false),
            ModFlags::NONE
        );
    }

    /// per-hand 武器位替换（W-B2）：非空 → 清 WEAPON_SEGMENT 段再并入；
    /// 空 → 恒等（非武器攻击 source 沿用上游供给）。
    #[test]
    fn replace_weapon_flags_swaps_weapon_segment_only() {
        let mace = ModFlags::weapon_flags("One Hand Mace", "Mace", true, true);
        let sword = ModFlags::weapon_flags("One Hand Sword", "Sword", true, true);
        let cfg = ModFlags::ATTACK | ModFlags::HIT | mace;
        // 同位替换 ≡ 恒等（单手 build 等价性依据）。
        assert_eq!(cfg.replace_weapon_flags(mace), cfg);
        // 异位替换：MH 锤位整段换成 OH 剑位，非武器段（ATTACK|HIT）保留。
        assert_eq!(
            cfg.replace_weapon_flags(sword),
            ModFlags::ATTACK | ModFlags::HIT | sword
        );
        // 空供给 → 恒等（Shield Wall 类非武器攻击 source）。
        assert_eq!(cfg.replace_weapon_flags(ModFlags::NONE), cfg);
        // WEAPON_SEGMENT = getWeaponFlags 值域并集。
        assert_eq!(
            ModFlags::WEAPON_SEGMENT.bits(),
            ModFlags::WEAPON_MASK.bits() | ModFlags::WARSTAFF.bits() | ModFlags::WEAPON.bits()
        );
        assert!(mace.is_subset_of(ModFlags::WEAPON_SEGMENT));
        assert!(
            ModFlags::weapon_flags("None", "Unarmed", true, true)
                .is_subset_of(ModFlags::WEAPON_SEGMENT)
        );
    }

    /// 新位宽下 `is_subset_of` 语义不变（既有语义测试在新表的搬迁锚点）。
    #[test]
    fn is_subset_of_semantics_hold_on_new_bits() {
        let mod_flags = ModFlags::MACE | ModFlags::WEAPON_1H;
        let cfg_match = ModFlags::ATTACK | ModFlags::MACE | ModFlags::WEAPON_1H | ModFlags::WEAPON;
        let cfg_miss = ModFlags::ATTACK | ModFlags::BOW;
        assert!(mod_flags.is_subset_of(cfg_match));
        assert!(!mod_flags.is_subset_of(cfg_miss));
        assert!(ModFlags::NONE.is_subset_of(cfg_miss));
    }
}

#[cfg(test)]
mod keyword_flags_tests {
    use super::KeywordFlags;

    // 用两个任意非保留位模拟具体 keyword（如 Poison / Curse）。
    const A: KeywordFlags = KeywordFlags(1 << 0);
    const B: KeywordFlags = KeywordFlags(1 << 1);

    #[test]
    fn matches_context_implements_pob2_match_keyword_flags() {
        // 空 mod keyword → 恒匹配任意上下文（PoB2: modMasked==0 → true）。
        assert!(KeywordFlags::NONE.matches_context(KeywordFlags::NONE));
        assert!(KeywordFlags::NONE.matches_context(A));

        // 无 MatchAll → ANY（任一重叠即可）。
        assert!(A.matches_context(A));
        assert!(A.matches_context(KeywordFlags(A.0 | B.0)));
        assert!(!A.matches_context(B));

        // 带 MatchAll → ALL（上下文须为 mod 去 MatchAll 后的超集）。
        let ab_all = KeywordFlags(A.0 | B.0 | KeywordFlags::MATCH_ALL.0);
        assert!(ab_all.matches_context(KeywordFlags(A.0 | B.0)));
        assert!(
            !ab_all.matches_context(A),
            "仅 A 不是 {{A,B}} 的超集 → ALL 拒绝"
        );
        assert!(
            ab_all.matches_context(KeywordFlags(A.0 | B.0 | (1 << 2))),
            "超集命中"
        );

        // 只带 MatchAll、无实际 keyword → 去 MatchAll 后为空 → 恒匹配。
        assert!(KeywordFlags::MATCH_ALL.matches_context(KeywordFlags::NONE));
    }
}
