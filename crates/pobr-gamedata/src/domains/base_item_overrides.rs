//! `overlay/base_item_overrides.json` loader + a dedicated merge — base
//! item overrides (a shield's `block_chance` / a sceptre's `spirit`,
//! extracted from vendor PoB2 `Data/Bases/*.lua`; corresponds to GGG's
//! `ShieldTypes.Block` / `ItemSpirit.SpiritGranted` — both tables' bundles
//! have been pruned from the CDN at the pinned patch, so the `.dat` route
//! is unavailable), schema in
//! [`pobr_data::catalog::base_item_overrides`].
//!
//! Why not go through the generic [`crate::overlay`] merge engine: this
//! table is a flat "base name → field" list, while base's side is a
//! `Vec<BaseItemDef>` (sorted by id, associated by `name`) — the shape
//! doesn't fit key-level recursive merge, so this domain has its own merge
//! function, locked in by unit tests.
//!
//! Merge semantics:
//!
//! 1. Associated by **English canonical name** (vendor's `itemBases` key =
//!    `BaseItemDef::name`); when several bases share a name (a rare `.dat`
//!    name collision), **all of them** get the value applied (it comes
//!    from the same vendor entry, so this is idempotent).
//! 2. `block_chance` is written into `BaseItemDef::armour.block_chance`;
//!    when base's side has no `armour` section (in principle a shield
//!    should always have an `ArmourTypes` row), a zero-valued
//!    [`ArmourBaseStats`] is inserted first so the value isn't lost.
//! 3. `spirit` is written into `BaseItemDef::spirit`.
//! 4. An overlay name absent from base → skipped (a vendor-only / removed
//!    base), consistent with `skill_overrides`'s rule 3.
//! 5. `reload_time_ms` (crossbow reload) is written into
//!    `BaseItemDef::weapon.reload_time_ms`; when base's side has no
//!    `weapon` section (in principle a crossbow should always have a
//!    `WeaponTypes` row), a zero-valued [`WeaponBaseStats`] is inserted
//!    first so the value isn't lost (mirrors rule 2).
//! 6. `charm_buff` (a charm base's inherent buff mod, vendor
//!    `flask.lua`'s `charm.buff`) overrides `BaseItemDef::charm_buff`
//!    (cloned over when `Some`, left untouched when `None`).
//! 7. `tags` (the full base tag set, the merged product of vendor's `.it`
//!    inheritance chain) is written into `BaseItemDef::tags` as a **union**
//!    with base's `.dat` leaf tags (sorted and deduplicated; left untouched
//!    when `None`) — mod spawn-weight checks (tier reverse-lookup) need
//!    category tags like `body_armour`/`armour`.

use pobr_data::catalog::base_item_overrides::BaseItemOverridesDef;
use pobr_data::catalog::{ArmourBaseStats, BaseItemDef, WeaponBaseStats};

use crate::{GameData, LoadError};

impl GameData {
    /// Loads the base item override overlay (always resolved under
    /// `overlay/`). Returns `Ok(None)` when the file is missing (an old
    /// data pack without the overlay layer) — the consumer behaves as
    /// plain base, backward compatible; other I/O / parse errors still
    /// propagate, not silenced.
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

/// Merges override values into the base item list (semantics in the module doc).
pub fn apply_base_item_overrides(bases: &mut [BaseItemDef], overrides: &BaseItemOverridesDef) {
    use std::collections::HashMap;
    // name → override entry index (entries are ascending and unique by
    // name, guaranteed by the generation side's sort).
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
        if let Some(charm_buff) = entry.charm_buff.as_ref() {
            base.charm_buff = charm_buff.clone();
        }
        if let Some(tags) = entry.tags.as_ref() {
            // Rule 7: tag union (.dat leaf tags + vendor's full set), sorted
            // and deduplicated for determinism.
            let mut merged: Vec<String> = base.tags.iter().chain(tags.iter()).cloned().collect();
            merged.sort_unstable();
            merged.dedup();
            base.tags = merged;
        }
        // Base attribute requirements (vendor's `req = { str/dex/int }`,
        // the data source for the equipment-requirement snapshot).
        if let Some(req) = entry.req_str {
            base.req_str = req;
        }
        if let Some(req) = entry.req_dex {
            base.req_dex = req;
        }
        if let Some(req) = entry.req_int {
            base.req_int = req;
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
            charm_buff: Vec::new(),
            req_str: 0,
            req_dex: 0,
            req_int: 0,
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

    /// Rules 1/2/3: associated by name, block written into the armour
    /// section, spirit written at the top level.
    #[test]
    fn merges_block_and_spirit_by_name() {
        let mut bases = vec![
            base("Crude Tower Shield", Some(armour_stats(18))),
            base("Omen Sceptre", None),
        ];
        let overrides = BaseItemOverridesDef {
            overrides: vec![
                BaseItemOverrideEntry {
                    req_str: None,
                    req_dex: None,
                    req_int: None,
                    name: "Crude Tower Shield".to_string(),
                    block_chance: Some(26.0),
                    spirit: None,
                    reload_time_ms: None,
                    charm_buff: None,
                    tags: None,
                },
                BaseItemOverrideEntry {
                    req_str: None,
                    req_dex: None,
                    req_int: None,
                    name: "Omen Sceptre".to_string(),
                    block_chance: None,
                    spirit: Some(100),
                    reload_time_ms: None,
                    charm_buff: None,
                    tags: None,
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

    /// Rule 2 fallback: when base's side has no armour section, insert a
    /// zero-valued struct first, then write block, so the value isn't lost.
    #[test]
    fn creates_armour_section_when_missing() {
        let mut bases = vec![base("Phantom Buckler", None)];
        let overrides = BaseItemOverridesDef {
            overrides: vec![BaseItemOverrideEntry {
                req_str: None,
                req_dex: None,
                req_int: None,
                name: "Phantom Buckler".to_string(),
                block_chance: Some(20.0),
                spirit: None,
                reload_time_ms: None,
                charm_buff: None,
                tags: None,
            }],
        };
        apply_base_item_overrides(&mut bases, &overrides);
        let armour = bases[0].armour.as_ref().unwrap();
        assert_eq!(armour.block_chance, Some(20.0));
        assert_eq!(armour.armour, 0);
    }

    /// Rule 4: an overlay name absent from base → skipped, no error.
    #[test]
    fn unknown_names_are_skipped() {
        let mut bases = vec![base("Crude Tower Shield", Some(armour_stats(18)))];
        let overrides = BaseItemOverridesDef {
            overrides: vec![BaseItemOverrideEntry {
                req_str: None,
                req_dex: None,
                req_int: None,
                name: "Removed Legacy Shield".to_string(),
                block_chance: Some(30.0),
                spirit: None,
                reload_time_ms: None,
                charm_buff: None,
                tags: None,
            }],
        };
        apply_base_item_overrides(&mut bases, &overrides);
        assert_eq!(bases[0].armour.as_ref().unwrap().block_chance, None);
    }

    /// Rule 5: reload_time_ms is written into the weapon section, the
    /// original value untouched; when base's side has no weapon section, a
    /// zero-valued struct is inserted first so the value isn't lost.
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
                    req_str: None,
                    req_dex: None,
                    req_int: None,
                    name: "Makeshift Crossbow".to_string(),
                    block_chance: None,
                    spirit: None,
                    reload_time_ms: Some(800),
                    charm_buff: None,
                    tags: None,
                },
                BaseItemOverrideEntry {
                    req_str: None,
                    req_dex: None,
                    req_int: None,
                    name: "Weaponless Oddity".to_string(),
                    block_chance: None,
                    spirit: None,
                    reload_time_ms: Some(750),
                    charm_buff: None,
                    tags: None,
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

    /// Rule 6: a `Some` `charm_buff` overrides `BaseItemDef::charm_buff`
    /// (including replacing an old value); `None` leaves it untouched, and
    /// an unmatched name doesn't write anything.
    #[test]
    fn merges_charm_buff_overriding_base() {
        let ruby = base("Ruby Charm", None);
        let mut topaz = base("Topaz Charm", None);
        topaz.charm_buff = vec!["stale".to_string()]; // Should be overridden by Some
        let mut bases = vec![
            ruby,
            topaz,
            base("Crude Tower Shield", Some(armour_stats(18))),
        ];
        let overrides = BaseItemOverridesDef {
            overrides: vec![
                BaseItemOverrideEntry {
                    req_str: None,
                    req_dex: None,
                    req_int: None,
                    name: "Ruby Charm".to_string(),
                    block_chance: None,
                    spirit: None,
                    reload_time_ms: None,
                    charm_buff: Some(vec!["+25% to Fire Resistance".to_string()]),
                    tags: None,
                },
                BaseItemOverrideEntry {
                    req_str: None,
                    req_dex: None,
                    req_int: None,
                    name: "Topaz Charm".to_string(),
                    block_chance: None,
                    spirit: None,
                    reload_time_ms: None,
                    charm_buff: Some(vec!["+25% to Lightning Resistance".to_string()]),
                    tags: None,
                },
                // charm_buff None → left untouched (stays empty), and
                // verifies a non-charm base is unaffected.
                BaseItemOverrideEntry {
                    req_str: None,
                    req_dex: None,
                    req_int: None,
                    name: "Crude Tower Shield".to_string(),
                    block_chance: Some(26.0),
                    spirit: None,
                    reload_time_ms: None,
                    charm_buff: None,
                    tags: None,
                },
            ],
        };
        apply_base_item_overrides(&mut bases, &overrides);
        assert_eq!(
            bases[0].charm_buff,
            vec!["+25% to Fire Resistance".to_string()]
        );
        assert_eq!(
            bases[1].charm_buff,
            vec!["+25% to Lightning Resistance".to_string()],
            "Some 覆盖旧值"
        );
        assert!(bases[2].charm_buff.is_empty(), "charm_buff None 不写入");
        assert_eq!(
            bases[2].armour.as_ref().unwrap().block_chance,
            Some(26.0),
            "其它字段照常 merge"
        );
    }
}
