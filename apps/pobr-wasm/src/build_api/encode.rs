//! Edit state -> PoB2 Build XML -> a share code (`encode_build_json`).

use std::collections::BTreeMap;

use pobr_build::{decode_pob_code, encode_pob_code, merge_active_sets};
use pobr_data::item::EquipmentSlot;

use super::request::CalculateBuildRequest;
use super::slot_from_id;
use crate::state;

/// [`EquipmentSlot`] -> PoB `<Slot name>` (the inverse of `slot_from_pob_name`, used for encoding).
fn pob_slot_name(slot: EquipmentSlot) -> &'static str {
    match slot {
        EquipmentSlot::Weapon1 => "Weapon 1",
        EquipmentSlot::Weapon2 => "Weapon 2",
        EquipmentSlot::Helmet => "Helmet",
        EquipmentSlot::BodyArmour => "Body Armour",
        EquipmentSlot::Gloves => "Gloves",
        EquipmentSlot::Boots => "Boots",
        EquipmentSlot::Amulet => "Amulet",
        EquipmentSlot::Ring1 => "Ring 1",
        EquipmentSlot::Ring2 => "Ring 2",
        EquipmentSlot::Ring3 => "Ring 3",
        EquipmentSlot::Belt => "Belt",
    }
}

/// The PoB2 tree version tag matching the current data version.
///
/// ponytail: derived from `GOLDEN_PARITY_DATA_VERSION` (`4.<n>.…` = PoE2
/// `0.<n>`); automatically follows the golden-version constant on a major
/// data version bump, with no separate config entry.
fn current_tree_version() -> String {
    let minor = pobr_data::GOLDEN_PARITY_DATA_VERSION
        .split('.')
        .nth(1)
        .unwrap_or("5");
    format!("0_{minor}")
}

/// Edit state -> a PoB2 share code.
///
/// The request shape is the same as [`CalculateBuildRequest`] (the web side
/// always sends a full override, plus optional `notes`); `character.class_name`
/// is required. Round-trip contract: re-decoding and calculating the
/// produced code matches calculating directly from the request
/// (`contract_golden::encode_build_roundtrip`).
///
/// **Multi-set preservation**: when the request carries `base_code` (the
/// original code from import), the output is based on it, replacing only
/// the currently active set ([`merge_active_sets`]), with every other Spec
/// / SkillSet / ItemSet — along with each one's `title` — preserved as-is;
/// otherwise, exporting a multi-set build would leave only the set being
/// edited, breaking every loadout binding. A hand-built build has no `base_code` and still goes through full generation.
pub fn encode_build_json(request_json: &str) -> Result<String, String> {
    encode_build_impl(request_json).map_err(super::ApiError::into_json)
}

fn encode_build_impl(request_json: &str) -> Result<String, super::ApiError> {
    let req: CalculateBuildRequest = serde_json::from_str(request_json)
        .map_err(|e| super::ApiError::bad_request(format!("invalid request json: {e}")))?;
    let data = state::build_data().map_err(super::ApiError::not_initialized)?;
    let ch = req
        .character
        .as_ref()
        .ok_or_else(|| super::ApiError::bad_request("character is required to encode"))?;
    let class_name = ch.class_name.clone().ok_or_else(|| {
        super::ApiError::bad_request("character.class_name is required to encode")
    })?;

    let mut items: Vec<(String, String)> = Vec::new();
    for item in req.items.as_deref().unwrap_or_default() {
        let slot = slot_from_id(&item.slot)?;
        items.push((pob_slot_name(slot).to_string(), item.text.clone()));
    }
    let mut flasks: Vec<(String, String)> = Vec::new();
    for flask in req.flasks.as_deref().unwrap_or_default() {
        if !(flask.slot.starts_with("Flask ") || flask.slot.starts_with("Charm ")) {
            return Err(super::ApiError::bad_request(format!(
                "unknown flask/charm slot: {}",
                flask.slot
            ))
            .with_slot(flask.slot.as_str()));
        }
        flasks.push((flask.slot.clone(), flask.text.clone()));
    }
    let jewels: Vec<(u32, String)> = req
        .jewels
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|j| (j.socket_node, j.text.clone()))
        .collect();
    let socket_groups: Vec<crate::xml_write::XmlSkillGroup> = req
        .socket_groups
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|g| {
            let mut gems: Vec<(String, String, u32, u32)> = g
                .gems
                .iter()
                .filter(|gem| !gem.skill_id.is_empty())
                .map(|gem| {
                    let gem_id = data
                        .gem_effects
                        .get(&gem.skill_id)
                        .map(|e| e.gem_id.clone())
                        .unwrap_or_default();
                    (gem_id, gem.skill_id.clone(), gem.level, gem.quality)
                })
                .collect();
            // The XML parse path determines the active skill as "the first
            // gem in the group"; the calc path determines it as "the first
            // non-support" (via a data-table lookup). Moving the first
            // non-support gem to the front makes both determinations
            // converge, guaranteeing the active skill doesn't drift after an encode -> decode round trip.
            if let Some(active_pos) = gems.iter().position(|(gem_id, _, _, _)| {
                // A gem with an empty gemId gets dropped by XML parsing, so it can't be an active candidate.
                !gem_id.is_empty() && !data.is_support_gem(gem_id).unwrap_or(false)
            }) && active_pos > 0
            {
                let active = gems.remove(active_pos);
                gems.insert(0, active);
            }
            crate::xml_write::XmlSkillGroup {
                slot: g.slot.clone(),
                enabled: g.enabled,
                source: g.source.clone(),
                gems,
            }
        })
        .collect();

    // Default-true **condition** keys haven't been backfilled by the
    // request's direct-construction path yet — writing an explicit false
    // for any key not explicitly set makes the two paths' semantics
    // converge. Quest Stat rewards are already backfilled with
    // defaultState=true on both sides (the XML path's parse_config, the
    // direct-construction path's parse_build_from_request), so omitting them stays consistent.
    let mut config_inputs = req.config_inputs.clone();
    for key in pobr_build::default_true_condition_keys() {
        config_inputs
            .entry(key.to_string())
            .or_insert(serde_json::Value::Bool(false));
    }

    let empty_choices = BTreeMap::new();
    let tree_version = current_tree_version();
    let xml = crate::xml_write::write_build_xml(&crate::xml_write::XmlInput {
        level: ch.level.unwrap_or(1),
        class_name: &class_name,
        ascendancy_name: ch.ascendancy_name.as_deref().unwrap_or(""),
        tree_version: &tree_version,
        allocated_nodes: req.allocated_nodes.as_deref().unwrap_or_default(),
        attribute_choices: req.attribute_choices.as_ref().unwrap_or(&empty_choices),
        items,
        flasks,
        jewels,
        socket_groups,
        main_socket_group: req.main_socket_group,
        config_inputs: &config_inputs,
        notes: req.notes.as_deref(),
    });
    // With a base draft, write back into its active set, preserving the
    // other loadouts; if the base draft is corrupted, degrade to a
    // pure-edit-state output (better to lose the other sets than fail the export outright).
    let xml = match req
        .base_code
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
    {
        Some(code) => match decode_pob_code(code) {
            Ok(base) => merge_active_sets(&base, &xml),
            Err(_) => xml,
        },
        None => xml,
    };
    Ok(encode_pob_code(&xml).map_err(|e| format!("encode: {e}"))?)
}
