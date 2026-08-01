//! Config enums the build layer needs.
//!
//! `BuildConfig` itself lives in `pobr-build`, because `to_calc_config()` needs
//! `pobr-core` and this crate may not depend on it. Only the stable, logic-free
//! enums belong here.

use serde::{Deserialize, Serialize};

/// Which tab the build is currently showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ViewMode {
    Calcs,
    Items,
    #[default]
    Tree,
    Skills,
    Config,
    Import,
}

/// Bandit quest reward. Round-trips through the PoB build XML.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum BanditChoice {
    #[default]
    None,
    Kraityn,
    Alira,
    Oak,
}
