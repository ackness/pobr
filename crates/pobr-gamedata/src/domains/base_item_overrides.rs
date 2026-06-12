//! `overlay/base_item_overrides.json` loader + 专用 merge——基底物品覆盖值
//! （vendor PoB2 `Data/Bases/*.lua` 抽取的盾牌 `block_chance` / 权杖 `spirit`，
//! 对应 GGG `ShieldTypes.Block` / `ItemSpirit.SpiritGranted`——两表 bundle 已被
//! CDN 对钉定 patch 剪除，`.dat` 路线不可得），schema 见
//! [`pobr_data::catalog::base_item_overrides`]。
//!
//! 为什么不走通用 [`crate::overlay`] merge 引擎：本表是「基底名称 → 字段」的
//! 扁平列表，base 侧是 `Vec<BaseItemDef>`（按 id 排序、以 `name` 关联），形状
//! 不适配 key 级递归 merge，故按域写专用 merge 函数并以单测锁定语义。
//!
//! merge 语义：
//!
//! 1. 按**英文 canonical 名称**关联（vendor `itemBases` 键 = `BaseItemDef::name`）；
//!    同名多基底（.dat 罕见重名）时**全部**应用（值来自同一 vendor 条目，幂等）。
//! 2. `block_chance` 写入 `BaseItemDef::armour.block_chance`；base 侧无 `armour`
//!    段（理论上盾必有 `ArmourTypes` 行）时补零值 [`ArmourBaseStats`] 再写，不丢值。
//! 3. `spirit` 写入 `BaseItemDef::spirit`。
//! 4. overlay 名称在 base 中不存在 → 跳过（vendor-only / 已移除基底），与
//!    `skill_overrides` 的规则 3 一致。
//! 5. `reload_time_ms`（M4-T4 W-D2，弩装填）写入 `BaseItemDef::weapon
//!    .reload_time_ms`；base 侧无 `weapon` 段（理论上弩必有 `WeaponTypes` 行）
//!    时补零值 [`WeaponBaseStats`] 再写，不丢值（与规则 2 同构）。

use pobr_data::catalog::base_item_overrides::BaseItemOverridesDef;
use pobr_data::catalog::{ArmourBaseStats, BaseItemDef, WeaponBaseStats};

use crate::{GameData, LoadError};

impl GameData {
    /// 加载基底物品覆盖值 overlay（恒走 `overlay/` 定位）。文件缺失（旧数据包
    /// 无 overlay 层）返回 `Ok(None)`——消费侧行为 = 纯 base，向后兼容；
    /// 其余 IO / 解析错误照常上抛，不静默。
    pub fn base_item_overrides(&self) -> Result<Option<BaseItemOverridesDef>, LoadError> {
        match self
            .load_json_at::<BaseItemOverridesDef>(self.overlay_path("base_item_overrides.json"))
        {
            Ok(def) => Ok(Some(def)),
            Err(LoadError::Io { ref source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }
}

/// 把覆盖值 merge 进 base 基底列表（语义见模块文档）。
pub fn apply_base_item_overrides(bases: &mut [BaseItemDef], overrides: &BaseItemOverridesDef) {
    use std::collections::HashMap;
    // name → override 条目索引（条目按 name 升序且唯一，由生成侧排序保证）。
    let by_name: HashMap<&str, usize> = overrides
        .overrides
        .iter()
        .enumerate()
        .map(|(i, e)| (e.name.as_str(), i))
        .collect();
    for base in bases.iter_mut() {
        let Some(&idx) = by_name.get(base.name.as_str()) else {
            continue;
        };
        let entry = &overrides.overrides[idx];
        if let Some(spirit) = entry.spirit {
            base.spirit = Some(spirit);
        }
        if let Some(block) = entry.block_chance {
            base.armour
                .get_or_insert(ArmourBaseStats {
                    armour: 0,
                    evasion: 0,
                    energy_shield: 0,
                    ward: 0,
                    block_chance: None,
                    movement_penalty: None,
                })
                .block_chance = Some(block);
        }
        if let Some(reload) = entry.reload_time_ms {
            base.weapon
                .get_or_insert(WeaponBaseStats {
                    physical_min: 0,
                    physical_max: 0,
                    speed_ms: 0,
                    crit_chance: 0,
                    range: 0,
                    reload_time_ms: None,
                })
                .reload_time_ms = Some(reload);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pobr_data::catalog::base_item_overrides::BaseItemOverrideEntry;

    fn base(name: &str, armour: Option<ArmourBaseStats>) -> BaseItemDef {
        BaseItemDef {
            id: format!("Metadata/{name}"),
            name: name.to_string(),
            item_class: "Shield".to_string(),
            drop_level: 1,
            width: 2,
            height: 2,
            tags: vec![],
            implicits: vec![],
            mod_domain: 1,
            weapon: None,
            armour,
            spirit: None,
        }
    }

    fn armour_stats(armour: u32) -> ArmourBaseStats {
        ArmourBaseStats {
            armour,
            evasion: 0,
            energy_shield: 0,
            ward: 0,
            block_chance: None,
            movement_penalty: None,
        }
    }

    /// 规则 1/2/3：按名称关联，block 写入 armour 段、spirit 写入顶层。
    #[test]
    fn merges_block_and_spirit_by_name() {
        let mut bases = vec![
            base("Crude Tower Shield", Some(armour_stats(18))),
            base("Omen Sceptre", None),
        ];
        let overrides = BaseItemOverridesDef {
            overrides: vec![
                BaseItemOverrideEntry {
                    name: "Crude Tower Shield".to_string(),
                    block_chance: Some(26.0),
                    spirit: None,
                    reload_time_ms: None,
                },
                BaseItemOverrideEntry {
                    name: "Omen Sceptre".to_string(),
                    block_chance: None,
                    spirit: Some(100),
                    reload_time_ms: None,
                },
            ],
        };
        apply_base_item_overrides(&mut bases, &overrides);
        assert_eq!(
            bases[0].armour.as_ref().unwrap().block_chance,
            Some(26.0),
            "盾基底 block 应 merge 进 armour 段"
        );
        assert_eq!(bases[0].armour.as_ref().unwrap().armour, 18, "原值不受扰动");
        assert_eq!(bases[1].spirit, Some(100), "权杖基底 spirit 应写入顶层");
        assert!(bases[1].armour.is_none(), "无 block 覆盖时不虚构 armour 段");
    }

    /// 规则 2 兜底：base 侧无 armour 段时补零值结构再写 block，不丢值。
    #[test]
    fn creates_armour_section_when_missing() {
        let mut bases = vec![base("Phantom Buckler", None)];
        let overrides = BaseItemOverridesDef {
            overrides: vec![BaseItemOverrideEntry {
                name: "Phantom Buckler".to_string(),
                block_chance: Some(20.0),
                spirit: None,
                reload_time_ms: None,
            }],
        };
        apply_base_item_overrides(&mut bases, &overrides);
        let armour = bases[0].armour.as_ref().unwrap();
        assert_eq!(armour.block_chance, Some(20.0));
        assert_eq!(armour.armour, 0);
    }

    /// 规则 4：overlay 名称在 base 中不存在 → 跳过不报错。
    #[test]
    fn unknown_names_are_skipped() {
        let mut bases = vec![base("Crude Tower Shield", Some(armour_stats(18)))];
        let overrides = BaseItemOverridesDef {
            overrides: vec![BaseItemOverrideEntry {
                name: "Removed Legacy Shield".to_string(),
                block_chance: Some(30.0),
                spirit: None,
                reload_time_ms: None,
            }],
        };
        apply_base_item_overrides(&mut bases, &overrides);
        assert_eq!(bases[0].armour.as_ref().unwrap().block_chance, None);
    }

    /// 规则 5（M4-T4 W-D2）：reload_time_ms 写入 weapon 段，原值不受扰动；
    /// base 侧无 weapon 段时补零值结构再写，不丢值。
    #[test]
    fn merges_reload_time_into_weapon_section() {
        use pobr_data::catalog::WeaponBaseStats;
        let mut crossbow = base("Makeshift Crossbow", None);
        crossbow.weapon = Some(WeaponBaseStats {
            physical_min: 7,
            physical_max: 12,
            speed_ms: 625,
            crit_chance: 500,
            range: 120,
            reload_time_ms: None,
        });
        let mut bases = vec![crossbow, base("Weaponless Oddity", None)];
        let overrides = BaseItemOverridesDef {
            overrides: vec![
                BaseItemOverrideEntry {
                    name: "Makeshift Crossbow".to_string(),
                    block_chance: None,
                    spirit: None,
                    reload_time_ms: Some(800),
                },
                BaseItemOverrideEntry {
                    name: "Weaponless Oddity".to_string(),
                    block_chance: None,
                    spirit: None,
                    reload_time_ms: Some(750),
                },
            ],
        };
        apply_base_item_overrides(&mut bases, &overrides);
        let weapon = bases[0].weapon.as_ref().unwrap();
        assert_eq!(weapon.reload_time_ms, Some(800));
        assert_eq!(weapon.physical_min, 7, "原值不受扰动");
        let synthesized = bases[1].weapon.as_ref().unwrap();
        assert_eq!(
            synthesized.reload_time_ms,
            Some(750),
            "无 weapon 段时补结构"
        );
        assert_eq!(synthesized.physical_min, 0);
    }
}
