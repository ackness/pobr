use super::{ApiError, encode::current_tree_version};
use crate::state;
use pobr_build::{active_selection, decode_pob_code, encode_pob_code, parse_build_sets};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    code: String,
    #[serde(default)]
    all_specs: bool,
}
#[derive(Serialize)]
struct Response {
    code: String,
    specs: Vec<pobr_build::tree_convert::SpecConversion>,
}

pub fn preview_tree_conversion_json(input: &str) -> Result<String, String> {
    preview(input).map_err(ApiError::into_json)
}
fn preview(input: &str) -> Result<String, ApiError> {
    let req: Request =
        serde_json::from_str(input).map_err(|e| ApiError::bad_request(e.to_string()))?;
    let xml = decode_pob_code(&req.code).map_err(|e| ApiError::bad_request(e.to_string()))?;
    let data = state::build_data().map_err(ApiError::not_initialized)?;
    let game = state::game_data().map_err(ApiError::not_initialized)?;
    let meta = game
        .passive_tree_meta()
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    let indices = if req.all_specs {
        parse_build_sets(&xml)
            .map_err(|e| ApiError::bad_request(e.to_string()))?
            .trees
            .iter()
            .map(|s| s.id)
            .collect()
    } else {
        vec![active_selection(&xml).tree.unwrap_or(1)]
    };
    let conversion = pobr_build::tree_convert::preview_conversion(
        &xml,
        &data,
        &meta,
        &current_tree_version().map_err(ApiError::not_initialized)?,
        &indices,
    )
    .map_err(ApiError::bad_request)?;
    let response = Response {
        code: encode_pob_code(&conversion.xml).map_err(|e| ApiError::bad_request(e.to_string()))?,
        specs: conversion.specs,
    };
    serde_json::to_string(&response).map_err(|e| ApiError::bad_request(e.to_string()))
}
