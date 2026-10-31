//! Native CLI adapter preserving the complete shared conditional result and its provenance.

use pobr_build::{native_document::NativeBuildRequest, native_skill};
use pobr_gamedata::{GameData, NativeData};

use crate::CliError;

/// Decodes, loads, admits and evaluates a request using only the supplied native dataset.
///
/// Returns the full typed native result serialized as JSON, including diagnostics,
/// exact identities, assumptions and unavailable-output reasons. Every call admits
/// the request again; document decoding alone never authorizes calculation.
///
/// # Errors
/// Rejects malformed or oversized documents, unavailable or mismatched snapshots,
/// unsupported requests, native calculation failures and JSON serialization errors.
pub fn calculate_native_document(data: &GameData, bytes: &[u8]) -> Result<String, CliError> {
    let request = NativeBuildRequest::from_json(bytes)?;
    let native = NativeData::load(data)?;
    let admitted = native_skill::admit_native_skill(&native, &request)
        .map_err(|error| CliError::NativeSkill(format!("admission failed: {error}")))?;
    let result = native_skill::calculate_native_skill(&admitted)
        .map_err(|error| CliError::NativeSkill(format!("calculation failed: {error}")))?;
    Ok(serde_json::to_string_pretty(&result)?)
}
