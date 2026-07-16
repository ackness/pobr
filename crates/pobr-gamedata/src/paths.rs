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
        if self.file_exists(&layered) {
            layered
        } else {
            self.root().join(rel)
        }
    }

    /// 定位某个 **overlay 层**域文件：恒为 `<root>/overlay/<rel>`，不回退版本根
    /// （overlay 是 vendor 抽取/转录层，旧平铺布局从未有过 overlay 文件，无兼容需求）。
    ///
    /// 与 [`Self::domain_path`] 同样**不检查存在性**——缺文件时由加载侧报出带路径的
    /// [`crate::LoadError::Io`]；是否降级回内建 fallback 由消费方（如 pobr-build 的
    /// `BuildData::load`）按域裁决。
    pub(crate) fn overlay_path(&self, rel: &str) -> PathBuf {
        self.root().join("overlay").join(rel)
    }

    /// 定位某个 **generated 层**域文件：恒为 `<root>/generated/<rel>`（确定性缓存层，
    /// 工具再生产物）。同 [`Self::overlay_path`] 不检查存在性、不回退版本根。
    pub(crate) fn generated_path(&self, rel: &str) -> PathBuf {
        self.root().join("generated").join(rel)
    }

    /// 定位 **版本无关策展层** overlay 文件：`data/overlay-common/<rel>`——版本目录的
    /// **同级** `overlay-common/` 兄弟目录（`<root>/../overlay-common/<rel>`）。
    ///
    /// 人工策展、随游戏版本不变的 vendor-语义修正放这里，新数据版本目录自动继承，免去
    /// 逐版本手迁（见 `docs/version-bump-architecture.md` P1-3）。加载侧把它 merge 到
    /// 版本层 `overlay/<rel>` **之下**（版本层按条目 key 覆盖，其余追加）。
    ///
    /// 返回 `None` 仅当版本根无父目录（如 root 为文件系统根）——正常磁盘/内存后端恒
    /// `Some`：内存后端 root=`<memory>`，父为空 → 键规约为 `overlay-common/<rel>`。
    pub(crate) fn overlay_common_path(&self, rel: &str) -> Option<PathBuf> {
        self.root()
            .parent()
            .map(|parent| parent.join("overlay-common").join(rel))
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
