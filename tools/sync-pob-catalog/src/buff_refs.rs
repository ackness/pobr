//! `check-buff-refs`：`overlay/buff_definitions.json` 的 vendor 行段对账
//! （人工归纳例外通道的 drift 防线）。
//!
//! `buff_definitions.json` 由人工从 `CalcPerform.lua doActorMisc` if-chain
//! 归纳（过程代码无法 luajit 序列化），每条带 `vendor_ref`（文件 + 行段 +
//! `fnv1a64` 行段 hash）。本模块：
//! - `check`：重算各行段 hash，与登记值比对——vendor 升级后行段漂移即告警
//!   （提示人工复核归纳是否仍忠实）；
//! - `--write`：人工复核后回写最新 hash（机械步骤，归纳内容仍是人工责任）。
//!
//! hash 用 FNV-1a 64（自包含、非加密用途——只做 drift 检测）。

use std::fs;
use std::io;
use std::path::Path;

use pobr_data::catalog::buffs::BuffDef;
use serde::{Deserialize, Serialize};

/// 完整文档（生产/对账侧；`_meta` 透传保序）。
#[derive(Debug, Serialize, Deserialize)]
pub struct BuffDefinitionsDoc {
    /// 头部元信息（人工策展表：记 vendor commit + 维护说明）。
    #[serde(rename = "_meta")]
    pub meta: serde_json::Value,
    /// buff 定义列表。
    pub buffs: Vec<BuffDef>,
}

/// FNV-1a 64 位 hash。
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

/// 计算文件 `[line_start, line_end]`（1-based，含端点）行段的登记 hash 值。
/// 行段以 `\n` 重连（与平台换行无关）；行号越界返回 `None`。
pub fn segment_hash(file_text: &str, line_start: u32, line_end: u32) -> Option<String> {
    if line_start == 0 || line_end < line_start {
        return None;
    }
    let lines: Vec<&str> = file_text.lines().collect();
    let start = (line_start - 1) as usize;
    let end = line_end as usize;
    if end > lines.len() {
        return None;
    }
    let segment = lines[start..end].join("\n");
    Some(format!("fnv1a64:{:016x}", fnv1a64(segment.as_bytes())))
}

/// 对账结果（单条）。
#[derive(Debug)]
pub struct RefDrift {
    /// buff id。
    pub id: String,
    /// 登记 hash。
    pub recorded: String,
    /// 实算 hash（行号越界时为 `None`）。
    pub actual: Option<String>,
}

/// 对账：返回漂移清单（空 = 全部一致）。`write = true` 时回写实算 hash
/// 并重新序列化 defs 文件（机械刷新，人工复核后使用）。
pub fn run_check_buff_refs(
    vendor_root: &Path,
    defs_path: &Path,
    write: bool,
) -> io::Result<Vec<RefDrift>> {
    let defs_text = fs::read_to_string(defs_path)?;
    let mut doc: BuffDefinitionsDoc = serde_json::from_str(&defs_text)
        .map_err(|error| io::Error::other(format!("buff_definitions 解析失败：{error}")))?;

    let mut drifts = Vec::new();
    for buff in &mut doc.buffs {
        let vendor_file = vendor_root.join(&buff.vendor_ref.file);
        let file_text = fs::read_to_string(&vendor_file).map_err(|error| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("无法读取 vendor 文件 {}：{error}", vendor_file.display()),
            )
        })?;
        let actual = segment_hash(
            &file_text,
            buff.vendor_ref.line_start,
            buff.vendor_ref.line_end,
        );
        if actual.as_deref() != Some(buff.vendor_ref.segment_hash.as_str()) {
            drifts.push(RefDrift {
                id: buff.id.clone(),
                recorded: buff.vendor_ref.segment_hash.clone(),
                actual: actual.clone(),
            });
            if write && let Some(actual) = actual {
                buff.vendor_ref.segment_hash = actual;
            }
        }
    }

    if write && !drifts.is_empty() {
        let mut json =
            serde_json::to_string_pretty(&doc).expect("buff definitions 文档序列化不应失败");
        json.push('\n');
        fs::write(defs_path, json)?;
    }
    Ok(drifts)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FNV-1a 64 已知向量（标准测试向量）。
    #[test]
    fn fnv1a64_known_vectors() {
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a64(b"foobar"), 0x85944171f73967e8);
    }

    /// 行段 hash：1-based 含端点；越界返回 None；与平台换行无关。
    #[test]
    fn segment_hash_lines_and_bounds() {
        let text = "line1\nline2\nline3\n";
        let h12 = segment_hash(text, 1, 2).unwrap();
        assert_eq!(h12, format!("fnv1a64:{:016x}", fnv1a64(b"line1\nline2")));
        // CRLF 同值
        let crlf = "line1\r\nline2\r\nline3\r\n";
        assert_eq!(segment_hash(crlf, 1, 2).unwrap(), h12);
        assert!(segment_hash(text, 0, 1).is_none());
        assert!(segment_hash(text, 2, 1).is_none());
        assert!(segment_hash(text, 1, 4).is_none());
    }
}
