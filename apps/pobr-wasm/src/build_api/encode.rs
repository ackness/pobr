//! 编辑态 → PoB2 Build XML → 分享 code（`encode_build_json`）。

use std::collections::BTreeMap;

use pobr_build::encode_pob_code;
use pobr_data::item::EquipmentSlot;

use super::request::CalculateBuildRequest;
use super::slot_from_id;
use crate::state;

/// [`EquipmentSlot`] → PoB `<Slot name>`（`slot_from_pob_name` 的逆向，encode 用）。
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

/// 当前数据版本对应的 PoB2 树版本标注。
///
/// ponytail: 由 `GOLDEN_PARITY_DATA_VERSION`（`4.<n>.…` = PoE2 `0.<n>`）派生；
/// 数据大版本升级时随黄金版本常量自动跟进，无独立配置项。
fn current_tree_version() -> String {
    let minor = pobr_data::GOLDEN_PARITY_DATA_VERSION
        .split('.')
        .nth(1)
        .unwrap_or("5");
    format!("0_{minor}")
}

/// 编辑态 → PoB2 分享 code。
///
/// 请求形状与 [`CalculateBuildRequest`] 相同（web 端始终发全量覆盖 + 可选
/// `notes`）；`character.class_name` 必填。往返契约：产出的 code 重新解码计算
/// 与直接按请求计算结果一致（`contract_golden::encode_build_roundtrip`）。
pub fn encode_build_json(request_json: &str) -> Result<String, String> {
    let req: CalculateBuildRequest =
        serde_json::from_str(request_json).map_err(|e| format!("invalid request json: {e}"))?;
    let data = state::build_data()?;
    let ch = req
        .character
        .as_ref()
        .ok_or("character is required to encode")?;
    let class_name = ch
        .class_name
        .clone()
        .ok_or("character.class_name is required to encode")?;

    let mut items: Vec<(String, String)> = Vec::new();
    for item in req.items.as_deref().unwrap_or_default() {
        let slot = slot_from_id(&item.slot)?;
        items.push((pob_slot_name(slot).to_string(), item.text.clone()));
    }
    let mut flasks: Vec<(String, String)> = Vec::new();
    for flask in req.flasks.as_deref().unwrap_or_default() {
        if !(flask.slot.starts_with("Flask ") || flask.slot.starts_with("Charm ")) {
            return Err(format!("unknown flask/charm slot: {}", flask.slot));
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
            // XML 解析路径按「组内首个 gem = 主动技能」判定；计算路径按「首个非
            // support」（查数据表）。把首个非 support gem 前置，使两种判定收敛，
            // 保证 encode → decode 往返后主动技能不漂移。
            if let Some(active_pos) = gems.iter().position(|(gem_id, _, _, _)| {
                // gemId 为空的 gem 会被 XML 解析丢弃，不能当 active 候选。
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

    // 默认 true **条件**在请求直连路径尚未补注——对未显式设置的 key 写显式 false，
    // 两条路径语义收敛。quest Stat 奖励两侧都已按 defaultState=true 补注
    // （XML 路径 parse_config、直连路径 parse_build_from_request），省略即一致。
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
    encode_pob_code(&xml).map_err(|e| format!("encode: {e}"))
}
