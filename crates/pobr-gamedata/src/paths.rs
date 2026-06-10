//! 三层数据目录下的域文件定位。
//!
//! M0 起 `data/<版本>/` 采用三层物理布局（架构文档 20 §1 P1）：
//! `base/`（.dat 全自动再生）、`overlay/`（vendor 抽取）、`generated/`（确定性缓存）。
//! 域文件定位规则：**`base/` 优先，版本根回退**——兼容尚未迁移的旧布局
//! （旧数据包把域 JSON 直接放在版本根）。`manifest.json` 与 `i18n/` 恒在版本根，
//! 不走本定位。

use std::path::PathBuf;

use crate::GameData;

impl GameData {
    /// 定位某个数据域文件：`<root>/base/<rel>` 存在则用之，否则回退 `<root>/<rel>`。
    ///
    /// 注意回退路径**不检查存在性**——文件确实缺失时由加载侧报出带路径的
    /// [`crate::LoadError::Io`]（错误信息指向回退位置）。
    pub(crate) fn domain_path(&self, rel: &str) -> PathBuf {
        let layered = self.root().join("base").join(rel);
        if layered.exists() {
            layered
        } else {
            self.root().join(rel)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::GameData;

    /// 创建带唯一名的临时目录（测试结束不强制清理，落在系统 temp 下）。
    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("pobr-gamedata-paths-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// base/ 下存在域文件时优先使用 base/。
    #[test]
    fn prefers_base_subdirectory() {
        let dir = temp_dir("prefer-base");
        std::fs::create_dir_all(dir.join("base")).unwrap();
        std::fs::write(dir.join("base/stats.json"), "[]").unwrap();
        std::fs::write(dir.join("stats.json"), "[]").unwrap();
        let gd = GameData::new(&dir);
        assert_eq!(gd.domain_path("stats.json"), dir.join("base/stats.json"));
    }

    /// base/ 缺失时回退版本根（旧布局兼容）。
    #[test]
    fn falls_back_to_version_root() {
        let dir = temp_dir("fallback-root");
        std::fs::write(dir.join("stats.json"), "[]").unwrap();
        let gd = GameData::new(&dir);
        assert_eq!(gd.domain_path("stats.json"), dir.join("stats.json"));
    }
}
