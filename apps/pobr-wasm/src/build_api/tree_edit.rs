use super::ApiError;
use super::request::{CalculateBuildRequest, apply_request_overrides, parse_build_from_request};
use crate::state;
use pobr_data::catalog::PassiveNodeKind;
use pobr_tree::allocation::{Allocation, AllocationTree, EditAction, EditResult, Grant};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EditRequest {
    request: CalculateBuildRequest,
    node: u32,
    mode: u8,
    action: EditAction,
    /// Campaign reward points earned. Planning defaults to the complete campaign.
    #[serde(default = "campaign_points")]
    quest_points: u32,
}
fn campaign_points() -> u32 {
    24
}
#[derive(Serialize)]
struct Response {
    valid: bool,
    reason: Option<String>,
    result: Option<EditResult>,
}

pub fn preview_tree_edit_json(input: &str) -> Result<String, String> {
    state::cached_response("tree_edit", input, || {
        preview(input).map_err(ApiError::into_json)
    })
}
fn preview(input: &str) -> Result<String, ApiError> {
    let req: EditRequest =
        serde_json::from_str(input).map_err(|e| ApiError::bad_request(e.to_string()))?;
    if req.quest_points > 24 {
        return Err(ApiError::bad_request(
            "quest_points must be between 0 and 24",
        ));
    }
    let data = state::build_data().map_err(ApiError::not_initialized)?;
    let mut build = parse_build_from_request(&req.request)?;
    let _ = apply_request_overrides(&mut build, &req.request, &data)?;
    let effects = pobr_build::calc_orchestrator::passive_jewel_state(&build, &data);
    let nodes = data.passive_nodes_for(build.tree_version.as_deref());
    let active = usize::from(build.weapon_set.number() - 1);
    let allocated: BTreeSet<_> = build.tree.allocated_nodes.iter().map(|id| id.0).collect();
    let allocation = if let Some(swap) = &req.request.weapon_swap {
        let exclusive = swap
            .exclusive_nodes
            .each_ref()
            .map(|s| s.iter().copied().collect::<BTreeSet<_>>());
        if !exclusive[active].is_subset(&allocated)
            || !exclusive[1 - active].is_disjoint(&allocated)
        {
            return Err(ApiError::bad_request(
                "allocated_nodes must materialize the active weapon set",
            ));
        }
        Allocation {
            shared: allocated.difference(&exclusive[active]).copied().collect(),
            exclusive,
        }
    } else if let Some(other) = &build.other_weapon_set_nodes {
        let other: BTreeSet<_> = other.iter().map(|id| id.0).collect();
        let mut exclusive = [BTreeSet::new(), BTreeSet::new()];
        exclusive[active] = allocated.difference(&other).copied().collect();
        exclusive[1 - active] = other.difference(&allocated).copied().collect();
        Allocation {
            shared: allocated.intersection(&other).copied().collect(),
            exclusive,
        }
    } else {
        Allocation {
            shared: allocated,
            ..Default::default()
        }
    };
    let mut tree = AllocationTree::new(nodes.values());
    tree.class_starts = effects.class_starts.values().copied().collect();
    let Some(root) = effects
        .class_starts
        .get(&build.character.class_name.to_lowercase())
    else {
        return Err(ApiError::bad_request("class start unavailable"));
    };
    tree.roots.insert(*root);
    let game = state::game_data().map_err(ApiError::not_initialized)?;
    let meta = game
        .passive_tree_meta()
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    tree.ascendancy = meta
        .classes
        .iter()
        .filter(|c| c.name == build.character.class_name)
        .flat_map(|c| &c.ascendancies)
        .find(|a| a.name == build.character.ascendancy_name)
        .map(|a| a.id.clone());
    tree.roots.extend(
        nodes
            .values()
            .filter(|n| {
                n.kind == PassiveNodeKind::AscendancyStart
                    && n.ascendancy_id.is_some()
                    && n.ascendancy_id == tree.ascendancy
            })
            .map(|n| n.skill),
    );
    tree.grants = effects
        .allocation_grants
        .iter()
        .map(|g| Grant {
            source: g.source,
            nodes: g.nodes.clone(),
            roots: g.roots.clone(),
        })
        .collect();
    tree.points_max = build.character.level.saturating_sub(1) + req.quest_points;
    tree.weapon_points_base = req.quest_points;
    let response = match tree.edit(&allocation, req.node, req.mode, req.action) {
        Ok(result) => Response {
            valid: true,
            reason: None,
            result: Some(result),
        },
        Err(reason) => Response {
            valid: false,
            reason: Some(reason),
            result: None,
        },
    };
    serde_json::to_string(&response).map_err(|e| e.to_string().into())
}
