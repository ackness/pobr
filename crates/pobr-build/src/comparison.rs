//! 两份 [`OutputTable`] 的标量字段差异比较。
//!
//! 用于「改一件装备 / 一个配置后 DPS/EHP 变化多少」这类对比，也用于回归测试里与
//! PoB 基准做容差比较。只比较 [`OutputTable`] 的标量数值字段（不含 `damage_components`
//! 向量；那部分另行专门比较）。

use pobr_core::calc::OutputTable;

/// 单个标量字段的差异。
#[derive(Debug, Clone, PartialEq)]
pub struct FieldDiff {
    /// 字段稳定名（用于显示 / 排序，显示文本走 i18n）。
    pub field: &'static str,
    pub before: f64,
    pub after: f64,
    pub delta: f64,
    /// 相对变化（`delta / before`）。`before` 为 0 时为 `None`。
    pub relative: Option<f64>,
}

/// 两份输出的比较结果：仅包含发生变化（超过容差）的字段。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OutputComparison {
    pub diffs: Vec<FieldDiff>,
}

impl OutputComparison {
    /// 是否完全一致（在容差内无差异）。
    pub fn is_unchanged(&self) -> bool {
        self.diffs.is_empty()
    }

    /// 查找某字段的差异。
    pub fn field(&self, name: &str) -> Option<&FieldDiff> {
        self.diffs.iter().find(|d| d.field == name)
    }
}

/// 默认浮点比较容差。
pub const DEFAULT_EPSILON: f64 = 1e-9;

/// 比较两份 [`OutputTable`] 的标量字段，超过 `epsilon` 的差异收进结果。
pub fn compare_outputs(
    before: &OutputTable,
    after: &OutputTable,
    epsilon: f64,
) -> OutputComparison {
    let fields: [(&'static str, f64, f64); 19] = [
        ("life", before.life, after.life),
        ("mana", before.mana, after.mana),
        ("armour", before.armour, after.armour),
        ("evasion", before.evasion, after.evasion),
        ("energy_shield", before.energy_shield, after.energy_shield),
        (
            "chance_to_be_hit",
            before.chance_to_be_hit,
            after.chance_to_be_hit,
        ),
        (
            "fire_resistance",
            before.fire_resistance,
            after.fire_resistance,
        ),
        (
            "cold_resistance",
            before.cold_resistance,
            after.cold_resistance,
        ),
        (
            "lightning_resistance",
            before.lightning_resistance,
            after.lightning_resistance,
        ),
        ("crit_chance", before.crit_chance, after.crit_chance),
        (
            "crit_multiplier",
            before.crit_multiplier,
            after.crit_multiplier,
        ),
        ("total_hit_avg", before.total_hit_avg, after.total_hit_avg),
        ("hit_chance", before.hit_chance, after.hit_chance),
        ("action_rate", before.action_rate, after.action_rate),
        ("dps", before.dps, after.dps),
        (
            "fire_resistance_over_cap",
            before.fire_resistance_over_cap,
            after.fire_resistance_over_cap,
        ),
        (
            "cold_resistance_over_cap",
            before.cold_resistance_over_cap,
            after.cold_resistance_over_cap,
        ),
        (
            "lightning_resistance_over_cap",
            before.lightning_resistance_over_cap,
            after.lightning_resistance_over_cap,
        ),
        (
            "max_fire_resistance",
            before.max_fire_resistance,
            after.max_fire_resistance,
        ),
    ];

    let mut diffs = Vec::new();
    for (name, b, a) in fields {
        let delta = a - b;
        if delta.abs() > epsilon {
            diffs.push(FieldDiff {
                field: name,
                before: b,
                after: a,
                delta,
                relative: relative_change(b, delta),
            });
        }
    }

    OutputComparison { diffs }
}

fn relative_change(before: f64, delta: f64) -> Option<f64> {
    if before.abs() <= f64::EPSILON {
        None
    } else {
        Some(delta / before)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_outputs_unchanged() {
        let out = OutputTable {
            life: 100.0,
            dps: 5000.0,
            ..OutputTable::default()
        };
        let cmp = compare_outputs(&out, &out, DEFAULT_EPSILON);
        assert!(cmp.is_unchanged());
    }

    #[test]
    fn detects_dps_change() {
        let before = OutputTable {
            dps: 1000.0,
            ..OutputTable::default()
        };
        let after = OutputTable {
            dps: 1500.0,
            ..OutputTable::default()
        };
        let cmp = compare_outputs(&before, &after, DEFAULT_EPSILON);
        let diff = cmp.field("dps").expect("dps diff");
        assert_eq!(diff.delta, 500.0);
        assert_eq!(diff.relative, Some(0.5));
    }

    #[test]
    fn relative_none_when_before_zero() {
        let before = OutputTable::default();
        let after = OutputTable {
            life: 50.0,
            ..OutputTable::default()
        };
        let cmp = compare_outputs(&before, &after, DEFAULT_EPSILON);
        let diff = cmp.field("life").expect("life diff");
        assert_eq!(diff.relative, None);
    }

    #[test]
    fn within_epsilon_ignored() {
        let before = OutputTable {
            life: 100.0,
            ..OutputTable::default()
        };
        let after = OutputTable {
            life: 100.0 + 1e-12,
            ..OutputTable::default()
        };
        let cmp = compare_outputs(&before, &after, DEFAULT_EPSILON);
        assert!(cmp.is_unchanged());
    }
}
