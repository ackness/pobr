//! Re-export shim — the mapping itself moved to `pobr-core::skill_env` (the
//! engine-semantics layer owns it now; kept so existing call sites stay unchanged).

pub use pobr_core::skill_env::buff_stat_map::{
    MappedStat, map_aura_buff_stat, map_self_buff_offensive_stat,
};
