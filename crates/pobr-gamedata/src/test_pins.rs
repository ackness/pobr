//! Test-pin snapshots：数据内容计数钉的 bless 快照（v0.0.3 P0-1）。
//!
//! 「断言值 = 当前数据文件内容的计数/统计量」这类测试钉（段条数、覆盖率、某条
//! 数据的具体数值）随每次数据 regen 必然变化，手工重钉是零信息量劳动。此类钉
//! 统一存进 `data/<ver>/generated/test_pins.json`（扁平 map：pin 名 → JSON 值），
//! 测试经 [`assert_pin`] 对照；regen 后以 `POBR_BLESS_PINS=1` 重跑同一批测试
//! 一步刷新（`pipeline/regen-all.sh` 末步已编排）。
//!
//! 甄别边界：**结构性守卫**（schema 非空、单调棘轮、去重不变式）仍写死在代码
//! 里，不进快照——快照只承载「会随数据内容漂移、且漂移本身无对错」的值。
//!
//! 注意：bless 写回在进程内串行（全局锁）；跨进程并发 bless 同一文件会整文件
//! 互相覆盖。请用 `cargo test`（单进程多线程）而非 nextest（进程/测试）执行
//! bless 命令。

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use serde_json::Value;

/// bless 开关环境变量：`POBR_BLESS_PINS=1` 时 [`assert_pin`] 把实际值写回快照。
pub const BLESS_ENV: &str = "POBR_BLESS_PINS";

/// bless 写回锁（同进程内多测试线程对同一快照文件 read-modify-write 串行）。
static WRITE_LOCK: Mutex<()> = Mutex::new(());

/// 对照（或 bless 时写回）`<version_dir>/generated/test_pins.json` 中的一个 pin。
///
/// `version_dir` = 该测试实际加载数据的版本目录（golden 测试传
/// `GOLDEN_PARITY_DATA_VERSION` 对应目录，活动版本测试传 `DATA_VERSION` 对应
/// 目录）——pin 跟数据同目录，regen 哪个版本就刷新哪个版本的快照。
///
/// 正常模式：pin 缺失或值不等 → panic，报错信息给出 bless 刷新命令。
/// bless 模式（`POBR_BLESS_PINS=1`）：写回实际值并通过。
pub fn assert_pin(version_dir: &Path, name: &str, actual: impl Into<Value>) {
    let actual = actual.into();
    let path = version_dir.join("generated/test_pins.json");

    if std::env::var(BLESS_ENV).is_ok_and(|v| v == "1") {
        let _guard = WRITE_LOCK.lock().unwrap();
        let mut pins = read_pins(&path);
        pins.insert(name.to_string(), actual);
        let mut json = serde_json::to_string_pretty(&pins).expect("serialize test pins");
        json.push('\n');
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap_or_else(|e| {
                panic!("create {} failed: {e}", parent.display());
            });
        }
        std::fs::write(&path, json)
            .unwrap_or_else(|e| panic!("write {} failed: {e}", path.display()));
        return;
    }

    let pins = read_pins(&path);
    match pins.get(name) {
        Some(expected) if *expected == actual => {}
        found => {
            let expected = found.map_or("<missing>".to_string(), Value::to_string);
            panic!(
                "test pin `{name}` out of date in {}:\n  blessed: {expected}\n  actual:  {actual}\n\
                 数据内容变了（regen 后属预期）。刷新：POBR_BLESS_PINS=1 重跑本测试\
                 （pipeline/regen-all.sh 末步会批量刷新并提交快照）。",
                path.display()
            );
        }
    }
}

/// 读快照（缺文件 → 空 map；损坏文件 → panic，不静默）。
fn read_pins(path: &Path) -> BTreeMap<String, Value> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .unwrap_or_else(|e| panic!("parse {} failed: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
        Err(e) => panic!("read {} failed: {e}", path.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_version_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("pobr-test-pins-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn bless_then_match_roundtrip() {
        let dir = temp_version_dir("roundtrip");
        // ponytail: env var is process-global — this test only exercises the
        // non-bless read path directly via a hand-written snapshot file.
        std::fs::create_dir_all(dir.join("generated")).unwrap();
        std::fs::write(
            dir.join("generated/test_pins.json"),
            r#"{"a.count": 3, "b.obj": {"x": 2.5}}"#,
        )
        .unwrap();
        assert_pin(&dir, "a.count", 3);
        assert_pin(&dir, "b.obj", serde_json::json!({"x": 2.5}));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    #[should_panic(expected = "out of date")]
    fn mismatch_panics_with_bless_hint() {
        let dir = temp_version_dir("mismatch");
        std::fs::create_dir_all(dir.join("generated")).unwrap();
        std::fs::write(dir.join("generated/test_pins.json"), r#"{"a.count": 3}"#).unwrap();
        assert_pin(&dir, "a.count", 4);
    }

    #[test]
    #[should_panic(expected = "out of date")]
    fn missing_pin_panics() {
        let dir = temp_version_dir("missing");
        assert_pin(&dir, "no.such.pin", 1);
    }
}
