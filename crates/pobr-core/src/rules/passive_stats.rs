//! PoBR-owned exact-ID mapping for the bounded ordinary-passive native channel.

use pobr_data::modifier::ModType;
use pobr_data::native_stat::NativeStat;

use crate::Modifier;

/// Why a complete native packet could not supply this passive's modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativePassiveRejection {
    UnknownStat,
    UnsupportedContext,
    IncompletePacket,
    InexactValue,
    BindingMismatch,
    VersionMismatch,
    TransformedNode,
}

/// Maps a reviewed unconditional player stat, retaining exact signed integer values.
///
/// # Errors
/// Rejects unknown IDs and values not exactly representable by the engine's f64.
pub fn map_stat(stat: &NativeStat) -> Result<Modifier, NativePassiveRejection> {
    // A0 0.5.5.5 Stats row 139 / PassiveSkills row 1390, corroborated by
    // GGG's tree text "+5% to [Resistances|Fire Resistance]". Numeric native
    // Semantic/Category fields do not determine this engine aggregation type.
    let name = match stat.id.as_str() {
        "base_fire_damage_resistance_%" => "FireResistance",
        _ => return Err(NativePassiveRejection::UnknownStat),
    };
    let value = stat.value as f64;
    // i128 avoids the saturating f64 -> i64 cast incorrectly accepting i64::MAX.
    if value as i128 != i128::from(stat.value) {
        return Err(NativePassiveRejection::InexactValue);
    }
    Ok(Modifier::number(name, ModType::Base, value))
}
