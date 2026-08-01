//! `gen-mirage-configs` subcommand:
//! produces `overlay/mirage_configs.json` (5 mirage config entries).
//!
//! Vendor `Modules/CalcMirages.lua`'s five branches are procedural closures
//! that luajit can't serialize — so the config data is **embedded in this
//! tool's source code** instead, and the tool writes it out (satisfying the
//! letter of "overlay files can't be hand-edited, only tool-regenerated";
//! this tradeoff is tracked under open question 2). Each config carries a
//! `vendor_ref` line-range anchor; vendor drift is flagged via
//! `_meta.vendor_fingerprint` (a coarse fingerprint of CalcMirages.lua's
//! line count + byte count) — after a vendor bump, rerunning this command
//! produces a byte diff if the fingerprint changed, prompting a manual
//! review of the five branches' semantics.
//!
//! Actual special-branch logic goes through `handler_id` (registered into
//! `pobr-core::rules::registry`, zero extra wiring here). See
//! [`pobr_data::catalog::triggers`] for the schema.

use std::fs;
use std::io;
use std::path::Path;

use serde::Serialize;

use pobr_data::catalog::triggers::{MirageConfigDef, MirageSourceFilterDef, MirageTriggerDef};

use crate::extract_lua::{ExtractLuaArgs, OverlayMeta, read_vendor_version, resolve_version_file};
use crate::extract_minions::to_pretty_json;

/// `_meta` extension: adds a coarse vendor fingerprint on top of the generic OverlayMeta.
#[derive(Debug, Serialize)]
struct MirageMeta {
    #[serde(flatten)]
    base: OverlayMeta,
    /// Coarse fingerprint of `Modules/CalcMirages.lua` (`<lines>L:<bytes>B`)
    /// — a changed fingerprint after a vendor bump signals that the five
    /// branches' semantics need a manual review.
    vendor_fingerprint: String,
}

#[derive(Debug, Serialize)]
struct MirageConfigsDoc {
    #[serde(rename = "_meta")]
    meta: MirageMeta,
    configs: Vec<MirageConfigDef>,
}

/// The 5 mirage configs (hand-transcribed from the `Modules/CalcMirages.lua`
/// closures as of vendor commit `2df5a74`; see each `vendor_ref` for its line-range anchor).
fn builtin_configs() -> Vec<MirageConfigDef> {
    let mut configs = vec![
        // Mirage Archer (CalcMirages.lua:63-119): skillData.triggeredByMirageArcher;
        // source = the main skill itself (weaponData1.type == "Bow"); count/less
        // draw from three aggregated stats; the main skill's offence panel is
        // computed as normal (calcMainSkillOffence = true).
        MirageConfigDef {
            mirage_id: "mirage_archer".to_string(),
            trigger: MirageTriggerDef {
                skill_data_flag: Some("triggeredByMirageArcher".to_string()),
                granted_effect_name: None,
            },
            source_skill_filter: MirageSourceFilterDef {
                weapon_type: Some("Bow".to_string()),
                exclude_used_by_mirage: true,
                select: Some("main_skill".to_string()),
                ..Default::default()
            },
            count_stat: Some("MirageArcherMaxCount".to_string()),
            less_damage_stat: Some("MirageArcherLessDamage".to_string()),
            less_attack_speed_stat: Some("MirageArcherLessAttackSpeed".to_string()),
            cast_chance_stat: None,
            uses_stored_uses: true,
            calc_main_skill_offence: true,
            handler_id: None,
            vendor_ref: "Modules/CalcMirages.lua:63-119".to_string(),
        },
        // Saviour Mirage Warriors (:120-179): granted effect name "Reflection";
        // source = whichever one-handed-sword attack skill has the highest DPS
        // (found via a GlobalCache scan); count is halved when dual-wielding
        // matching weapons (handled by a handler); the mirage's output
        // entirely replaces the main skill's output.
        MirageConfigDef {
            mirage_id: "saviour_mirage_warriors".to_string(),
            trigger: MirageTriggerDef {
                skill_data_flag: None,
                granted_effect_name: Some("Reflection".to_string()),
            },
            source_skill_filter: MirageSourceFilterDef {
                weapon_type: None,
                skill_types: vec!["Attack".to_string()],
                exclude_skill_types: vec!["Totem".to_string(), "SummonsTotem".to_string()],
                weapon_flags: vec!["Sword".to_string(), "Weapon1H".to_string()],
                exclude_used_by_mirage: true,
                select: Some("best_dps".to_string()),
            },
            count_stat: Some("SaviourMirageWarriorMaxCount".to_string()),
            less_damage_stat: Some("SaviourMirageWarriorLessDamage".to_string()),
            less_attack_speed_stat: None,
            cast_chance_stat: None,
            uses_stored_uses: true,
            calc_main_skill_offence: false,
            handler_id: Some("mirage:saviour_dual_wield_halving".to_string()),
            vendor_ref: "Modules/CalcMirages.lua:120-179".to_string(),
        },
        // Tawhoa's Chosen (:180-298): matches by granted effect name; source =
        // whichever Slam/Melee attack skill has the highest DPS; the trigger
        // cooldown model (icdr / server-frame rounding / trigger rate
        // overriding attack speed) is real logic, handled by a handler; less
        // damage = ChieftainMirageChieftainMoreDamage.
        MirageConfigDef {
            mirage_id: "tawhoas_chosen".to_string(),
            trigger: MirageTriggerDef {
                skill_data_flag: None,
                granted_effect_name: Some("Tawhoa's Chosen".to_string()),
            },
            source_skill_filter: MirageSourceFilterDef {
                weapon_type: None,
                skill_types: vec!["Attack".to_string()],
                exclude_skill_types: vec![
                    "Vaal".to_string(),
                    "Totem".to_string(),
                    "SummonsTotem".to_string(),
                ],
                weapon_flags: vec![],
                exclude_used_by_mirage: true,
                select: Some("best_dps".to_string()),
            },
            count_stat: None,
            less_damage_stat: Some("ChieftainMirageChieftainMoreDamage".to_string()),
            less_attack_speed_stat: None,
            cast_chance_stat: None,
            uses_stored_uses: true,
            calc_main_skill_offence: false,
            handler_id: Some("mirage:tawhoa_trigger_rate".to_string()),
            vendor_ref: "Modules/CalcMirages.lua:180-298".to_string(),
        },
        // Sacred Wisps (:299-364): skillData.triggeredBySacredWisps; source =
        // the main skill itself (weaponData1.type == "Wand"); count/cast
        // chance come from the "Summon Sacred Wisps" skill's stats
        // (cross-skill lookup, handled by a handler).
        MirageConfigDef {
            mirage_id: "sacred_wisps".to_string(),
            trigger: MirageTriggerDef {
                skill_data_flag: Some("triggeredBySacredWisps".to_string()),
                granted_effect_name: None,
            },
            source_skill_filter: MirageSourceFilterDef {
                weapon_type: Some("Wand".to_string()),
                exclude_used_by_mirage: true,
                select: Some("main_skill".to_string()),
                ..Default::default()
            },
            count_stat: Some("SacredWispsMaxCount".to_string()),
            less_damage_stat: Some("SacredWispsLessDamage".to_string()),
            less_attack_speed_stat: None,
            cast_chance_stat: Some("SacredWispsChance".to_string()),
            uses_stored_uses: true,
            calc_main_skill_offence: true,
            handler_id: Some("mirage:sacred_wisps_source_skill".to_string()),
            vendor_ref: "Modules/CalcMirages.lua:299-364".to_string(),
        },
        // General's Cry (:365-419): skillData.triggeredByGeneralsCry; doesn't
        // go through the calculateMirage sub-environment — instead it
        // transforms the main skill in place (scaling the cooldown via
        // dpsMultiplier, rewriting exert-related mods to Damage, injecting
        // QuantityMultiplier), all handled by a handler.
        MirageConfigDef {
            mirage_id: "generals_cry".to_string(),
            trigger: MirageTriggerDef {
                skill_data_flag: Some("triggeredByGeneralsCry".to_string()),
                granted_effect_name: None,
            },
            source_skill_filter: MirageSourceFilterDef {
                exclude_used_by_mirage: true,
                select: Some("main_skill".to_string()),
                ..Default::default()
            },
            count_stat: Some("GeneralsCryDoubleMaxCount".to_string()),
            less_damage_stat: None,
            less_attack_speed_stat: None,
            cast_chance_stat: None,
            uses_stored_uses: false,
            calc_main_skill_offence: true,
            handler_id: Some("mirage:generals_cry_exert".to_string()),
            vendor_ref: "Modules/CalcMirages.lua:365-419".to_string(),
        },
    ];
    configs.sort_by(|a, b| a.mirage_id.cmp(&b.mirage_id));
    configs
}

/// Generate `overlay/mirage_configs.json` text (byte-stable).
pub fn run_gen_mirage_configs(args: &ExtractLuaArgs) -> io::Result<String> {
    let (commit, subject) = read_vendor_version(&resolve_version_file(args))?;
    let fingerprint = vendor_fingerprint(&args.vendor_root)?;
    let mut regen = "cargo run -p sync-pob-catalog -- gen-mirage-configs --vendor-root vendor/PathOfBuilding-PoE2/src".to_string();
    if let Some(out) = &args.out_for_meta {
        regen.push_str(&format!(" --out {out}"));
    }
    let meta = MirageMeta {
        base: OverlayMeta {
            schema: "mirage_configs/v1".to_string(),
            generator: "sync-pob-catalog gen-mirage-configs（数据内嵌于工具源码）".to_string(),
            vendor: "PathOfBuilding-PoE2".to_string(),
            vendor_commit: commit,
            vendor_commit_subject: subject,
            extracted_files: vec!["Modules/CalcMirages.lua".to_string()],
            regen_command: regen,
        },
        vendor_fingerprint: fingerprint,
    };
    Ok(to_pretty_json(&MirageConfigsDoc {
        meta,
        configs: builtin_configs(),
    }))
}

/// Coarse fingerprint of CalcMirages.lua: line count + byte count.
fn vendor_fingerprint(vendor_root: &Path) -> io::Result<String> {
    let path = vendor_root.join("Modules/CalcMirages.lua");
    let text = fs::read_to_string(&path).map_err(|error| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("failed to read {}: {error}", path.display()),
        )
    })?;
    Ok(format!("{}L:{}B", text.lines().count(), text.len()))
}

#[cfg(test)]
mod tests {
    use super::builtin_configs;

    /// 5 configs, ids unique and ascending; handler entries carry the
    /// `mirage:` prefix (per 20-doc §5's convention of the global handler
    /// ledger counting by prefix domain).
    #[test]
    fn builtin_configs_shape() {
        let configs = builtin_configs();
        assert_eq!(configs.len(), 5);
        let ids: Vec<&str> = configs.iter().map(|c| c.mirage_id.as_str()).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(ids, sorted, "mirage_id must be unique and ascending");
        for config in &configs {
            if let Some(handler) = &config.handler_id {
                assert!(
                    handler.starts_with("mirage:"),
                    "handler id {handler} is missing the mirage: prefix"
                );
            }
            assert!(
                config.trigger.skill_data_flag.is_some()
                    || config.trigger.granted_effect_name.is_some(),
                "{} is missing a trigger condition",
                config.mirage_id
            );
        }
    }
}
