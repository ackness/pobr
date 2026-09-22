//! Batch support compatibility, using the same group judgement as calculation.

use super::{ApiError, SocketGroupInput, request::socket_group_from_input, state};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    groups: Vec<SocketGroupInput>,
}

/// Returns one boolean per input group, preserving order (including duplicate groups).
/// Unknown effects and groups without an active skill are incompatible.
pub fn support_groups_compatible_json(input: &str) -> Result<String, String> {
    inner(input).map_err(ApiError::into_json)
}

fn inner(input: &str) -> Result<String, ApiError> {
    let request: Request =
        serde_json::from_str(input).map_err(|err| ApiError::bad_request(err.to_string()))?;
    let data = state::build_data().map_err(ApiError::not_initialized)?;
    let compatible: Vec<_> = request
        .groups
        .iter()
        .map(|group| {
            if group.gems.iter().any(|gem| gem.skill_id.is_empty()) {
                return false;
            }
            pobr_build::support::support_group_compatible(
                &socket_group_from_input(group, &data),
                &data,
            )
        })
        .collect();
    serde_json::to_string(&compatible).map_err(|err| ApiError::from(err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn json_group_source_controls_gem_only_supports() {
        let mut data = pobr_build::BuildData::empty();
        for value in [
            json!({"id":"Spell", "is_support":false, "skill_types":["Spell"]}),
            json!({"id":"GemOnly", "is_support":true, "support_gems_only":true}),
        ] {
            let effect: pobr_data::catalog::GrantedEffectDef =
                serde_json::from_value(value).unwrap();
            data.granted_effects.insert(effect.id.clone(), effect);
        }
        for source in [None, Some("Item:weapon1")] {
            let input: SocketGroupInput = serde_json::from_value(json!({
                "source": source,
                "gems": [{"skill_id":"Spell"}, {"skill_id":"GemOnly"}],
            }))
            .unwrap();
            let group = socket_group_from_input(&input, &data);
            assert_eq!(
                pobr_build::support::support_group_compatible(&group, &data),
                source.is_none()
            );
        }
    }
}
