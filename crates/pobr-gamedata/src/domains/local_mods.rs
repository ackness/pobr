//! `overlay/local_mods.json` loader——局部词条白名单（武器局部
//! increased 后缀 + adds 形式伤害后缀），schema 见
//! [`pobr_data::catalog::local_mods`]。
//!
//! pobr 准源为 `pobr-build::calc_orchestrator::is_weapon_local_mod` 原硬编码
//! 枚举（搬迁不变式）；消费方 `BuildData::load` 在缺文件时降级回
//! `LocalModsDef::default()`（与本文件逐值一致的内建镜像）。

use pobr_data::catalog::local_mods::LocalModsDef;

use crate::{GameData, LoadError};

impl GameData {
    /// 加载局部词条白名单（单对象域，版本无关策展）：版本 `overlay/` 优先、
    /// `overlay-common/` 兜底（[`Self::load_overlay_or_common`]），两层皆缺即报错——
    /// 降级语义由消费方裁决，不在 loader 层吞错。
    pub fn local_mods(&self) -> Result<LocalModsDef, LoadError> {
        self.load_overlay_or_common("local_mods.json")
    }
}
