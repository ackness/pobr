//! PoB2 SkillType 全量枚举名 → id（1-based）查找——[`crate::skill::SkillTypes::from_pob2_name`]
//! 的后备表。
//!
//! 数据本体在边车 `skill_type_names.txt`（`gen-skill-types` 从 vendor
//! `Global.lua::SkillType` 生成，`include_str!` 内嵌——与 pobr-i18n locale toml
//! 同模式；数据表不进 `.rs`，`no_embedded_data` 守卫不触）。行格式 `name id`，
//! `#` 行为注释；生成时已按 name 字典序，此处再排序兜底（二分查找前提不依赖
//! 生成端约定）。

use std::sync::LazyLock;

static RAW: &str = include_str!("skill_type_names.txt");

/// (name, id) 按 name 字典序——二分查找用。
pub(crate) static SKILL_TYPE_IDS: LazyLock<Vec<(&'static str, u32)>> = LazyLock::new(|| {
    let mut entries: Vec<(&str, u32)> = RAW
        .lines()
        .filter(|l| !l.trim_start().starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let (name, id) = l
                .split_once(' ')
                .unwrap_or_else(|| panic!("skill_type_names.txt 行格式错误：{l}"));
            let id: u32 = id
                .trim()
                .parse()
                .unwrap_or_else(|e| panic!("skill_type_names.txt id 解析失败（{l}）：{e}"));
            (name, id)
        })
        .collect();
    entries.sort_unstable();
    assert!(!entries.is_empty(), "skill_type_names.txt 为空");
    entries
});

/// 名查 id（1-based）；未知名 → None。
pub(crate) fn lookup(name: &str) -> Option<u32> {
    SKILL_TYPE_IDS
        .binary_search_by_key(&name, |(n, _)| n)
        .ok()
        .map(|i| SKILL_TYPE_IDS[i].1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_covers_low_and_high_ids() {
        assert_eq!(lookup("Attack"), Some(1));
        assert_eq!(lookup("Persistent"), Some(140));
        assert_eq!(lookup("SupportedByCreepingChill"), Some(290));
        assert_eq!(lookup("NotAType"), None);
    }

    #[test]
    fn table_is_full_vendor_enum() {
        // 全量抽取的粗校验：vendor 0.5.x 枚举 290 条；vendor 升级只增不减。
        assert!(SKILL_TYPE_IDS.len() >= 290);
    }
}
