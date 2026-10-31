//! Explicit native saved-request evaluation through the existing strict codec and shared core.

use pobr_build::native_document::NativeBuildRequest;
use pobr_build::native_skill;

use crate::{build_api::ApiError, state};

/// Reopens and admits a complete native request against the active immutable native catalogs.
///
/// Returns the existing `pobr_build::native_skill::NativeSkillResult` JSON without
/// projection or response caching. Every call decodes and admits the document afresh.
///
/// # Errors
/// Returns the existing JSON error envelope: `not_initialized` outside native mode,
/// `bad_request` for document/admission rejection, or `internal` for evaluation or
/// serialization failure. No legacy XML/build-code fallback is attempted.
pub fn calculate_native_build_json(request_json: &str) -> Result<String, String> {
    let data = state::native_data().map_err(|e| ApiError::not_initialized(e).into_json())?;
    let request = NativeBuildRequest::from_json(request_json.as_bytes())
        .map_err(|e| ApiError::bad_request(e.to_string()).into_json())?;
    let admitted = native_skill::admit_native_skill(&data, &request)
        .map_err(|e| ApiError::bad_request(e.to_string()).into_json())?;
    let result = native_skill::calculate_native_skill(&admitted)
        .map_err(|e| ApiError::from(e.to_string()).into_json())?;
    serde_json::to_string(&result)
        .map_err(|e| ApiError::from(format!("serialize native result: {e}")).into_json())
}
