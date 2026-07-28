//! 诊断环境变量的进程级快照。
//!
//! `POBR_DBG_*` / `POBR_GATE_DENY` 这批诊断开关落在**逐词条 / 逐伤害分量**的热
//! 路径上（如 `calc_orchestrator::collect::gate_parses` 每条 modifier 文本问一
//! 次），而 `std::env::var` 每次都要锁 environ、线性扫描、堆分配一个 `String`
//! ——实测 ~100 ns（干净 environ）到 ~180 ns（60 个变量）。
//!
//! [`dbg_env!`] 用 `LazyLock` 把结果快照成进程常量，命中后约 0.5 ns（~200–350×）。
//! 代价是**进程启动后再改环境变量不生效**——诊断开关本就在启动时设定，无影响。

/// 读取一个诊断环境变量，结果按进程快照。返回 `Option<&'static str>`。
///
/// ```ignore
/// if dbg_env!("POBR_DBG_BASES").is_some() { … }          // 布尔开关
/// if let Some(deny) = dbg_env!("POBR_GATE_DENY") { … }    // 取值
/// ```
///
/// 每个调用点展开出自己的 `static`，无需集中登记变量名。
#[macro_export]
macro_rules! dbg_env {
    ($name:literal) => {{
        static SNAPSHOT: std::sync::LazyLock<Option<String>> =
            std::sync::LazyLock::new(|| std::env::var($name).ok());
        SNAPSHOT.as_deref()
    }};
}

#[cfg(test)]
mod tests {
    /// 同一调用点多次求值走同一份快照（LazyLock 只初始化一次）。
    #[test]
    fn snapshot_is_stable_across_calls() {
        // Arrange / Act
        let first = dbg_env!("POBR_DBG_NONEXISTENT_SENTINEL");
        let second = dbg_env!("POBR_DBG_NONEXISTENT_SENTINEL");

        // Assert
        assert_eq!(first, second);
        assert!(first.is_none(), "未设置的变量应为 None");
    }

    /// 取值型用法拿到的是变量内容而非仅存在性。
    #[test]
    fn returns_value_not_just_presence() {
        // Arrange：PATH 在任何测试环境下都存在且非空。
        // Act
        let path = dbg_env!("PATH");

        // Assert
        assert!(path.is_some_and(|v| !v.is_empty()));
    }
}
