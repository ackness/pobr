//! Error types for the Build layer.
//!
//! Split into three layers:
//! - [`BuildCodeError`] — PoB Build Code codec (base64 / zlib / length guards).
//! - [`XmlError`] — PoB Build XML serialization / deserialization.
//! - [`BuildError`] — top-level aggregate error for the Build state machine / import / calc orchestration.

use thiserror::Error;

/// PoB Build Code (URL-safe Base64 + zlib) codec error.
#[derive(Debug, Error)]
pub enum BuildCodeError {
    /// Input is empty (no bytes left after trimming whitespace).
    #[error("build code is empty")]
    Empty,

    /// URL-safe Base64 decoding failed.
    #[error("invalid base64 in build code: {0}")]
    Base64(String),

    /// zlib decompression failed (corrupt data or not a zlib stream).
    #[error("zlib inflate failed: {0}")]
    Inflate(String),

    /// zlib compression failed.
    #[error("zlib deflate failed: {0}")]
    Deflate(String),

    /// Compressed input exceeds the allowed limit (zip-bomb guard, encode side).
    #[error("input too large: {len} bytes exceeds limit {limit}")]
    InputTooLarge { len: usize, limit: usize },

    /// Decompressed payload exceeds the allowed limit (zip-bomb guard, decode side).
    #[error("decompressed payload too large: exceeds limit {limit} bytes")]
    DecompressedTooLarge { limit: usize },

    /// Decompressed payload is not valid UTF-8.
    #[error("decompressed payload is not valid UTF-8: {0}")]
    Utf8(String),
}

/// PoB Build XML processing error.
#[derive(Debug, Error)]
pub enum XmlError {
    /// XML parsing failed (syntax / encoding issue).
    #[error("xml parse error: {0}")]
    Parse(String),

    /// A required element / attribute is missing from the XML.
    #[error("missing required xml node: {0}")]
    MissingNode(String),

    /// Root element is not PathOfBuilding (not a valid PoB Build).
    #[error("root element is not <PathOfBuilding>: found <{0}>")]
    NotPobRoot(String),

    /// Attribute value failed to parse (invalid number / enum).
    #[error("invalid attribute value for {attr}: {value}")]
    InvalidAttr { attr: String, value: String },
}

/// Aggregate error for the Build layer.
#[derive(Debug, Error)]
pub enum BuildError {
    /// Build Code codec error.
    #[error(transparent)]
    Code(#[from] BuildCodeError),

    /// XML serialization / deserialization error.
    #[error(transparent)]
    Xml(#[from] XmlError),

    /// Modifier text parse error (from pobr-core).
    #[error("modifier parse error: {0}")]
    Parse(String),

    /// Calculation orchestration error.
    #[error("calculation error: {0}")]
    Calc(String),

    /// Unrecognized import input.
    #[error("unrecognized import input")]
    UnrecognizedImport,
}
