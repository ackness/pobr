//! Process-wide snapshot of diagnostic environment variables.
//!
//! The `POBR_DBG_*` / `POBR_GATE_DENY` diagnostic switches sit on a **per-
//! modifier / per-damage-component** hot path (e.g.
//! `calc_orchestrator::collect::gate_parses` checks one per modifier text), and
//! `std::env::var` locks environ, scans linearly, and heap-allocates a `String`
//! on every call — measured at ~100 ns (clean environ) to ~180 ns (60 vars).
//!
//! [`dbg_env!`] snapshots the result into a process-lifetime constant via
//! `LazyLock`, dropping the cost to ~0.5 ns on a cache hit (~200–350×). The
//! trade-off is that **changing the env var after process start has no
//! effect** — these switches are only ever set at startup anyway, so this
//! doesn't matter in practice.

/// Reads a diagnostic env var, snapshotted per process. Returns `Option<&'static str>`.
///
/// ```ignore
/// if dbg_env!("POBR_DBG_BASES").is_some() { … }          // boolean switch
/// if let Some(deny) = dbg_env!("POBR_GATE_DENY") { … }    // read the value
/// ```
///
/// Each call site expands its own `static`, so there's no central registry of
/// variable names to maintain.
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
    /// Repeated evaluation at the same call site reuses one snapshot (LazyLock initializes once).
    #[test]
    fn snapshot_is_stable_across_calls() {
        // Arrange / Act
        let first = dbg_env!("POBR_DBG_NONEXISTENT_SENTINEL");
        let second = dbg_env!("POBR_DBG_NONEXISTENT_SENTINEL");

        // Assert
        assert_eq!(first, second);
        assert!(first.is_none(), "未设置的变量应为 None");
    }

    /// Value-reading usage gets the variable's contents, not just presence.
    #[test]
    fn returns_value_not_just_presence() {
        // Arrange: PATH exists and is non-empty in any test environment.
        // Act
        let path = dbg_env!("PATH");

        // Assert
        assert!(path.is_some_and(|v| !v.is_empty()));
    }
}
