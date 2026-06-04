//! PoB Build Code 编解码：URL-safe Base64 + zlib（与 PathOfBuilding / pobb.in 字节级兼容）。
//!
//! PoB 的 share code 流程是：Build XML → `zlib.compress`（带 zlib header `0x78 0x9c`）→
//! 标准 Base64 → 把 `+`/`/` 替换为 `-`/`_`（URL-safe），并去掉 `=` 填充。
//!
//! 本模块还原该流程，并在 **解码侧做容错**：
//! - 容忍是否带 `=` 填充（PoB 去填充，pobb.in 偶尔带）；
//! - 容忍 URL-safe 与标准两种字母表（先按 URL-safe 解，失败回退标准）；
//! - 去除一切 ASCII 空白 / 换行（粘贴时常见污染）。
//!
//! 以及 **zip-bomb 防护**：限制压缩输入与解压产物的长度上限。

use std::io::Read;

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use std::io::Write;

use crate::error::BuildCodeError;

/// 解压产物长度上限（PoB Build XML 通常 < 1 MiB；留足余量同时防 zip-bomb）。
pub const MAX_DECOMPRESSED_BYTES: usize = 16 * 1024 * 1024;

/// 压缩输入长度上限（编码侧；正常 Build XML 远小于此）。
pub const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;

/// 解码 PoB Build Code → Build XML 字符串。
///
/// 容错：忽略空白、容忍填充缺失、URL-safe 与标准字母表均可。
/// 防护：解压产物超过 [`MAX_DECOMPRESSED_BYTES`] 时报错（zip-bomb）。
pub fn decode_pob_code(code: &str) -> Result<String, BuildCodeError> {
    let cleaned = strip_ascii_whitespace(code);
    if cleaned.is_empty() {
        return Err(BuildCodeError::Empty);
    }

    let compressed = decode_base64_tolerant(&cleaned)?;
    let xml_bytes = inflate_bounded(&compressed, MAX_DECOMPRESSED_BYTES)?;

    String::from_utf8(xml_bytes).map_err(|e| BuildCodeError::Utf8(e.to_string()))
}

/// 编码 Build XML 字符串 → PoB Build Code（URL-safe Base64，无填充，与 PoB 一致）。
pub fn encode_pob_code(xml: &str) -> Result<String, BuildCodeError> {
    let bytes = xml.as_bytes();
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(BuildCodeError::InputTooLarge {
            len: bytes.len(),
            limit: MAX_INPUT_BYTES,
        });
    }

    let compressed = deflate(bytes)?;
    Ok(URL_SAFE_NO_PAD.encode(compressed))
}

/// 去除所有 ASCII 空白字符（空格 / 制表 / 换行 / 回车）。
fn strip_ascii_whitespace(input: &str) -> String {
    input.chars().filter(|c| !c.is_ascii_whitespace()).collect()
}

/// 容错 Base64 解码：依次尝试 URL-safe（无填充 / 有填充）与标准（无填充 / 有填充）。
fn decode_base64_tolerant(cleaned: &str) -> Result<Vec<u8>, BuildCodeError> {
    // 优先 URL-safe（PoB 默认）；padding 容错：先 NO_PAD 再带 PAD。
    let attempts: [&base64::engine::GeneralPurpose; 4] =
        [&URL_SAFE_NO_PAD, &URL_SAFE, &STANDARD_NO_PAD, &STANDARD];

    let mut last_err = String::new();
    for engine in attempts {
        match engine.decode(cleaned.as_bytes()) {
            Ok(bytes) => return Ok(bytes),
            Err(e) => last_err = e.to_string(),
        }
    }

    Err(BuildCodeError::Base64(last_err))
}

/// zlib 压缩。
fn deflate(bytes: &[u8]) -> Result<Vec<u8>, BuildCodeError> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(bytes)
        .map_err(|e| BuildCodeError::Deflate(e.to_string()))?;
    encoder
        .finish()
        .map_err(|e| BuildCodeError::Deflate(e.to_string()))
}

/// zlib 解压，带产物长度上限（zip-bomb 防护）。
fn inflate_bounded(compressed: &[u8], limit: usize) -> Result<Vec<u8>, BuildCodeError> {
    let mut decoder = ZlibDecoder::new(compressed);
    let mut out = Vec::new();
    // 多读 1 字节用于探测是否越界：take(limit + 1)。
    let mut limited = (&mut decoder).take((limit as u64) + 1);
    limited
        .read_to_end(&mut out)
        .map_err(|e| BuildCodeError::Inflate(e.to_string()))?;

    if out.len() > limit {
        return Err(BuildCodeError::DecompressedTooLarge { limit });
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_encode_decode() {
        let xml = "<PathOfBuilding2><Build level=\"1\"/></PathOfBuilding2>";
        let code = encode_pob_code(xml).expect("encode");
        // URL-safe / 无填充：不应含 +、/、=。
        assert!(!code.contains('+'));
        assert!(!code.contains('/'));
        assert!(!code.contains('='));
        let decoded = decode_pob_code(&code).expect("decode");
        assert_eq!(decoded, xml);
    }

    #[test]
    fn decode_tolerates_whitespace_and_newlines() {
        let xml = "<PathOfBuilding2/>";
        let code = encode_pob_code(xml).expect("encode");
        let mut polluted = String::new();
        for (i, c) in code.chars().enumerate() {
            polluted.push(c);
            if i % 4 == 0 {
                polluted.push('\n');
            }
        }
        polluted.push_str("  \t");
        let decoded = decode_pob_code(&polluted).expect("decode polluted");
        assert_eq!(decoded, xml);
    }

    #[test]
    fn decode_tolerates_standard_alphabet_with_padding() {
        // 用标准 base64 + 填充模拟来自非 PoB 来源的 code。
        let xml = "<PathOfBuilding2>x</PathOfBuilding2>";
        let compressed = deflate(xml.as_bytes()).unwrap();
        let std_code = STANDARD.encode(&compressed);
        let decoded = decode_pob_code(&std_code).expect("decode std");
        assert_eq!(decoded, xml);
    }

    #[test]
    fn empty_code_errors() {
        assert!(matches!(
            decode_pob_code("   \n\t "),
            Err(BuildCodeError::Empty)
        ));
    }

    #[test]
    fn garbage_base64_errors() {
        assert!(matches!(
            decode_pob_code("!!!not base64!!!"),
            Err(BuildCodeError::Base64(_))
        ));
    }

    #[test]
    fn non_zlib_payload_errors() {
        // 合法 base64 但不是 zlib 流。
        let code = URL_SAFE_NO_PAD.encode(b"hello world not zlib");
        assert!(matches!(
            decode_pob_code(&code),
            Err(BuildCodeError::Inflate(_))
        ));
    }
}
