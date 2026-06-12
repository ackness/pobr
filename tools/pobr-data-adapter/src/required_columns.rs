//! F2（drill 发现项）：`.dat` 表必需列 fail-fast 断言。
//!
//! 适配器各行结构对缺列走 serde `Option` / `default` 静默降级（如 `ArmourTypes`
//! 缺 `IncreasedMovementSpeed` 列 → 产物 `movement_penalty` 整列缺失；
//! `GrantedEffects` 缺 `AdditionalStatSets` → `additional_stat_set_ids` 缺失），
//! 结构合法但语义降级，只能靠产物 byte-diff 事后发现。本模块在 `--raw` 适配
//! 开始前对每张消费表的**首行**做列存在性检查（pathofexile-dat 导出每行带全部
//! 列键，首行缺键 = 快照漂移 / 导出配置缺列），缺列即报「表名 + 列名」错误。
//!
//! 清单跟随现有 schema 需求：列集合 = 各 `Raw*` 行结构 serde rename 的全集
//! （含 Option / default 列）。适配器新增消费列时同步更新此表。

use std::path::Path;

/// 英文表 → 适配器消费的必需列。
const REQUIRED_EN: &[(&str, &[&str])] = &[
    ("ItemClasses.json", &["_index", "Id"]),
    ("Tags.json", &["_index", "Id"]),
    (
        "Mods.json",
        &[
            "_index",
            "Id",
            "Name",
            "ModType",
            "Domain",
            "GenerationType",
            "Level",
            "Stat1",
            "Stat2",
            "Stat3",
            "Stat4",
            "Stat1Value",
            "Stat2Value",
            "Stat3Value",
            "Stat4Value",
            "Tags",
        ],
    ),
    (
        "Stats.json",
        &["_index", "Id", "IsLocal", "Semantic", "Category"],
    ),
    (
        "BaseItemTypes.json",
        &[
            "_index",
            "Id",
            "Name",
            "ItemClass",
            "DropLevel",
            "Width",
            "Height",
            "Tags",
            "Implicit_Mods",
            "ModDomain",
        ],
    ),
    (
        "WeaponTypes.json",
        &[
            "BaseItemType",
            "DamageMin",
            "DamageMax",
            "Speed",
            "CritChance",
            "RangeMax",
            // M4-T4 W-D2：弩装填时间（vendor Export/spec.lua:62483 确认列存在；
            // 本地 tables 快照缺失期间由 overlay 兜底，列断言面向下次重跑）。
            "ReloadTime",
        ],
    ),
    (
        "ArmourTypes.json",
        &[
            "BaseItemType",
            "Armour",
            "Evasion",
            "EnergyShield",
            "Ward",
            "IncreasedMovementSpeed",
        ],
    ),
    (
        "SkillGems.json",
        &[
            "BaseItemType",
            "GemType",
            "GemColour",
            "MinLevelReq",
            "StrengthRequirementPercent",
            "DexterityRequirementPercent",
            "IntelligenceRequirementPercent",
        ],
    ),
    (
        "GrantedEffects.json",
        &[
            "Id",
            "IsSupport",
            "ActiveSkill",
            "CastTime",
            "AllowedActiveSkillTypes",
            "AddedActiveSkillTypes",
            "ExcludedActiveSkillTypes",
            "CannotBeSupported",
            "SupportsGemsOnly",
            "StatSet",
            "AdditionalStatSets",
            "CostTypes",
        ],
    ),
    (
        "GrantedEffectsPerLevel.json",
        &[
            "GrantedEffect",
            "Level",
            "Cooldown",
            "AttackTime",
            "CostAmounts",
            "AttackSpeedMultiplier",
            "BaseMultiplier",
            "CostMultiplier",
            "Reservation",
            "EffectOnPlayer",
            "StoredUses",
        ],
    ),
    (
        "GrantedEffectStatSets.json",
        &[
            "Id",
            "BaseEffectiveness",
            "ConstantStats",
            "ConstantStatsValues",
        ],
    ),
    (
        "GrantedEffectStatSetsPerLevel.json",
        &[
            "StatSet",
            "GemLevel",
            "FloatStats",
            "BaseResolvedValues",
            "AdditionalStats",
            "AdditionalStatsValues",
            "BaseMultiplier",
            "SpellCritChance",
            "AttackCritChance",
        ],
    ),
    (
        "ActiveSkills.json",
        &["Id", "DisplayedName", "ActiveSkillTypes"],
    ),
    ("ActiveSkillType.json", &["_index", "Id", "Name"]),
    ("CostTypes.json", &["_index", "Id", "Divisor", "PerMinute"]),
];

/// 繁中边车表 → 必需列。
const REQUIRED_TW: &[(&str, &[&str])] = &[
    ("BaseItemTypes.json", &["_index", "Name"]),
    ("Mods.json", &["_index", "Name"]),
    ("ActiveSkills.json", &["_index", "DisplayedName"]),
];

/// `--raw` 模式入口断言：对全部消费表做必需列存在性检查。
///
/// - 表文件缺失 / 非对象数组 → 立即报错（含路径，覆盖快照整目录缺失场景）；
/// - 缺列 → 跨表汇总后一次性报「表名 + 列名」清单；
/// - 空表（零行）→ 无法校验列，放行给行级解析（不视为缺列）。
pub(crate) fn assert_required_columns(en: &Path, tw: &Path) -> Result<(), String> {
    let mut missing = Vec::new();
    for (dir_label, dir, tables) in [("English", en, REQUIRED_EN), ("繁中边车", tw, REQUIRED_TW)]
    {
        for (file, columns) in tables {
            check_table_columns(&dir.join(file), dir_label, file, columns, &mut missing)?;
        }
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "必需列断言失败（快照漂移 / 导出配置缺列，serde 默认值会静默降级产物，\
             拒绝继续）：\n  - {}",
            missing.join("\n  - ")
        ))
    }
}

/// 单表检查：解析为对象数组并核对首行键集合，缺列追加到 `missing`。
fn check_table_columns(
    path: &Path,
    dir_label: &str,
    file: &str,
    columns: &[&str],
    missing: &mut Vec<String>,
) -> Result<(), String> {
    let rows: Vec<serde_json::Map<String, serde_json::Value>> = crate::read_json(path)?;
    let Some(first) = rows.first() else {
        return Ok(()); // 空表：无行可核对列键
    };
    for column in columns {
        if !first.contains_key(*column) {
            missing.push(format!("{dir_label}/{file} 缺必需列 `{column}`"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    /// 写一个最小临时表文件，返回其目录（不依赖本地 pipeline 快照——F8：
    /// `pipeline/tables/` 可能整目录缺失）。
    fn temp_table(name: &str, json: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pobr-required-columns-{}-{name}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("T.json"), json).unwrap();
        dir
    }

    /// 缺列 → 报「表名 + 列名」，且一次性汇总全部缺列。
    #[test]
    fn reports_table_and_column_for_missing_columns() {
        let dir = temp_table("missing", r#"[{"BaseItemType": 1, "Armour": 5}]"#);
        let mut missing = Vec::new();
        check_table_columns(
            &dir.join("T.json"),
            "English",
            "ArmourTypes.json",
            &["BaseItemType", "Armour", "IncreasedMovementSpeed", "Ward"],
            &mut missing,
        )
        .unwrap();
        assert_eq!(
            missing,
            vec![
                "English/ArmourTypes.json 缺必需列 `IncreasedMovementSpeed`".to_string(),
                "English/ArmourTypes.json 缺必需列 `Ward`".to_string(),
            ]
        );
    }

    /// 列齐全（值可为 null）→ 不报缺列；空表 → 放行。
    #[test]
    fn passes_when_columns_present_or_table_empty() {
        let dir = temp_table("ok", r#"[{"Id": "x", "AdditionalStatSets": null}]"#);
        let mut missing = Vec::new();
        check_table_columns(
            &dir.join("T.json"),
            "English",
            "GrantedEffects.json",
            &["Id", "AdditionalStatSets"],
            &mut missing,
        )
        .unwrap();
        assert!(missing.is_empty());

        let empty = temp_table("empty", "[]");
        check_table_columns(
            &empty.join("T.json"),
            "English",
            "GrantedEffects.json",
            &["Id"],
            &mut missing,
        )
        .unwrap();
        assert!(missing.is_empty());
    }

    /// 表文件缺失 → 立即报错（含路径），不静默跳过。
    #[test]
    fn fails_fast_when_table_file_missing() {
        let dir = temp_table("absent", "[]");
        let mut missing = Vec::new();
        let err = check_table_columns(
            &dir.join("NoSuchTable.json"),
            "English",
            "NoSuchTable.json",
            &["Id"],
            &mut missing,
        )
        .unwrap_err();
        assert!(err.contains("NoSuchTable.json"), "err = {err}");
    }

    /// 入口断言：构造缺列目录 → 错误消息含表名 + 列名。
    #[test]
    fn entry_assertion_aggregates_across_tables() {
        let root = std::env::temp_dir().join(format!(
            "pobr-required-columns-entry-{}",
            std::process::id()
        ));
        let en = root.join("English");
        let tw = root.join("Traditional Chinese");
        fs::create_dir_all(&en).unwrap();
        fs::create_dir_all(&tw).unwrap();
        // 全部表先写空数组（空表放行），再把 ArmourTypes 写成缺
        // IncreasedMovementSpeed 的单行表。
        for (file, _) in REQUIRED_EN {
            fs::write(en.join(file), "[]").unwrap();
        }
        for (file, _) in REQUIRED_TW {
            fs::write(tw.join(file), "[]").unwrap();
        }
        fs::write(
            en.join("ArmourTypes.json"),
            r#"[{"BaseItemType": 0, "Armour": 1, "Evasion": 0, "EnergyShield": 0, "Ward": 0}]"#,
        )
        .unwrap();
        let err = assert_required_columns(&en, &tw).unwrap_err();
        assert!(
            err.contains("ArmourTypes.json 缺必需列 `IncreasedMovementSpeed`"),
            "err = {err}"
        );
    }
}
