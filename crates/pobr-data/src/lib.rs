/// 仓库内置的 PoE2 数据版本（CDN 补丁号，对应 `data/<DATA_VERSION>/`）——
/// **编译期默认值**。运行时优先级见 [`data_version`]。
///
/// 切换内置版本只改这一处常量；临时指向其它版本无需改代码，设
/// `POBR_DATA_VERSION` 环境变量或写 `data/CURRENT` 标记文件即可（见
/// [`data_version`] 与 `pobr_gamedata::current_data_dir`）。
pub const DATA_VERSION: &str = "4.5.0.3.4";

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
