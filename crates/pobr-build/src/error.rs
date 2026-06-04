//! Build 层错误类型。
//!
//! 分三层：
//! - [`BuildCodeError`] — PoB Build Code 编解码（base64 / zlib / 长度防护）。
//! - [`XmlError`] — PoB Build XML 序列化 / 反序列化。
//! - [`BuildError`] — Build 状态机 / 导入 / 计算编排的上层聚合错误。

use thiserror::Error;

/// PoB Build Code（URL-safe Base64 + zlib）编解码错误。
#[derive(Debug, Error)]
pub enum BuildCodeError {
    /// 输入为空（去除空白后没有任何字节）。
    #[error("build code is empty")]
    Empty,

    /// URL-safe Base64 解码失败。
    #[error("invalid base64 in build code: {0}")]
    Base64(String),

    /// zlib 解压失败（数据损坏或并非 zlib 流）。
    #[error("zlib inflate failed: {0}")]
    Inflate(String),

    /// zlib 压缩失败。
    #[error("zlib deflate failed: {0}")]
    Deflate(String),

    /// 压缩输入超过允许上限（zip-bomb 防护，编码侧）。
    #[error("input too large: {len} bytes exceeds limit {limit}")]
    InputTooLarge { len: usize, limit: usize },

    /// 解压产物超过允许上限（zip-bomb 防护，解码侧）。
    #[error("decompressed payload too large: exceeds limit {limit} bytes")]
    DecompressedTooLarge { limit: usize },

    /// 解压产物不是合法 UTF-8。
    #[error("decompressed payload is not valid UTF-8: {0}")]
    Utf8(String),
}

/// PoB Build XML 处理错误。
#[derive(Debug, Error)]
pub enum XmlError {
    /// XML 解析失败（语法 / 编码问题）。
    #[error("xml parse error: {0}")]
    Parse(String),

    /// XML 中缺少必需的元素 / 属性。
    #[error("missing required xml node: {0}")]
    MissingNode(String),

    /// 根元素不是 PathOfBuilding（不是合法的 PoB Build）。
    #[error("root element is not <PathOfBuilding>: found <{0}>")]
    NotPobRoot(String),

    /// 属性值解析失败（数值 / 枚举不合法）。
    #[error("invalid attribute value for {attr}: {value}")]
    InvalidAttr { attr: String, value: String },
}

/// Build 层聚合错误。
#[derive(Debug, Error)]
pub enum BuildError {
    /// Build Code 编解码错误。
    #[error(transparent)]
    Code(#[from] BuildCodeError),

    /// XML 序列化 / 反序列化错误。
    #[error(transparent)]
    Xml(#[from] XmlError),

    /// modifier 文本解析错误（来自 pobr-core）。
    #[error("modifier parse error: {0}")]
    Parse(String),

    /// 计算编排错误。
    #[error("calculation error: {0}")]
    Calc(String),

    /// 无法识别的导入输入。
    #[error("unrecognized import input")]
    UnrecognizedImport,
}
