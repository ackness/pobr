/// 仓库内置的 PoE2 数据版本（CDN 补丁号，对应 `data/<DATA_VERSION>/`）——
/// **编译期默认值**。运行时优先级见 [`data_version`]。
///
/// 切换内置版本只改这一处常量；临时指向其它版本无需改代码，设
/// `POBR_DATA_VERSION` 环境变量或写 `data/CURRENT` 标记文件即可（见
/// [`data_version`] 与 `pobr_gamedata::current_data_dir`）。
///
/// **当前内置默认 = `4.5.4.3`（PoE2 0.5.4b）**，与 `data/CURRENT` 一致；golden
/// 校验版本同为 [`GOLDEN_PARITY_DATA_VERSION`]（`4.5.4.3`，fixture 已用 0.5.4b
/// oracle 重录）。黄金值版本特定，parity / golden 测试加载钉定版本而非活动默认，
/// 两者解耦（见下）；活动默认在新版本上的可跑性由 `multi_version` smoke 覆盖。
pub const DATA_VERSION: &str = "4.5.4.3";

/// 仓库内 golden / parity 黄金值所对照的数据版本——**与活动版本 [`DATA_VERSION`]
/// 解耦的单一真源**。
///
/// 黄金值（`examples/demo-bd-test/*/meta.json` 的 PoB2 player_stats、gamedata 段
/// 计数 / vendor-commit 抽样钉值）是版本特定的；golden/parity 测试钉定本常量加载其
/// 被校验的版本，使 [`DATA_VERSION`] 可独立前进到更新的数据而不误红 parity。重录
/// golden 后同步更新此常量。多版本**无关性**由 `multi_version` smoke 覆盖（对每个
/// `data/<ver>/` 跑 calc，断言量纲合理而不比对黄金值）。
pub const GOLDEN_PARITY_DATA_VERSION: &str = "4.5.4.3";

/// 运行时数据版本：`POBR_DATA_VERSION` 环境变量优先，否则回退 [`DATA_VERSION`]
/// 常量。
///
/// 这是 pobr-data 层（零文件 I/O）能提供的发现入口——只读进程环境，不碰文件。
/// 需要叠加 `data/CURRENT` 标记文件发现的，用 `pobr_gamedata::data_version`
/// （I/O 层）。更新数据后 `export POBR_DATA_VERSION=<新版本>` 即可让全部读
/// 此函数的路径（含测试）指向新数据，**零代码改动**。
pub fn data_version() -> String {
    std::env::var("POBR_DATA_VERSION").unwrap_or_else(|_| DATA_VERSION.to_string())
}

pub mod build_config;
pub mod catalog;
pub mod constants;
pub mod damage;
pub mod display_stat;
pub mod game_data;
pub mod gem;
pub mod item;
pub mod minion;
pub mod modifier;
pub mod monster;
pub mod passive_tree;
pub mod skill;
mod skill_type_names;
pub mod source;
pub mod stat;

pub mod prelude {
    pub use crate::build_config::*;
    pub use crate::catalog::*;
    pub use crate::constants::*;
    pub use crate::damage::*;
    pub use crate::display_stat::*;
    pub use crate::game_data::*;
    pub use crate::gem::*;
    pub use crate::item::*;
    pub use crate::minion::*;
    pub use crate::modifier::*;
    pub use crate::monster::*;
    pub use crate::passive_tree::*;
    pub use crate::skill::*;
    pub use crate::source::*;
    pub use crate::stat::*;
}
