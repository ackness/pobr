//! RuleSet 聚合入口（架构文档 20 §2.3，P9 注入方式；M0-W3 起逐域接通）。
//!
//! `GameData::load_ruleset()` 一次性加载计算引擎需要的规则/常量域，由 pobr-build
//! 合并为 [`pobr_data::catalog::RuntimeConstants`] 注入 pobr-core
//! （经 `CalculationSession::set_constants` → `CalcConfig.constants`，
//! pobr-core 保持零 I/O）。
//!
//! 字段为 `Option`：`None` = 该域尚未数据化或数据目录缺该文件——消费方
//! （pobr-build `BuildData::load`）对 `None` 回退 `Default`（fallback 与 JSON
//! 逐值相等，搬迁不变式）。阶段 2 各域 agent 在此逐域追加加载。

use std::collections::HashMap;

use pobr_data::catalog::character_constants::CharacterConstantsDef;
use pobr_data::catalog::config_def::ConfigOptionDef;
use pobr_data::catalog::enemy_presets::EnemyPresetsTable;
use pobr_data::catalog::game_constants::GameConstantsDef;
use pobr_data::catalog::high_precision_mods::HighPrecisionModsDef;
use pobr_data::catalog::jewel_radii::JewelRadiiDef;
use pobr_data::catalog::monster_scaling::MonsterScalingTable;
use pobr_data::catalog::unarmed_data::UnarmedDataTable;
use pobr_data::catalog::weapon_types::WeaponTypeTable;

use crate::{GameData, LoadError};

/// 解析规则占位（M6 起由 `overlay/mod_parser_rules.json` 等填充，
/// 真实类型落 `pobr_data::catalog`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParserRules;

/// config 选项目录（M3-T1 A3：`overlay/config_options.json` 实体化）。
///
/// `options` 保持文件序（`var` 升序，byte-stable）；`by_var` 为 var → 下标
/// 索引，供 `config_interpreter` 消费方按 XML `<Input name>` 直查条目。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConfigCatalog {
    /// 全部条目（schema 见 [`pobr_data::catalog::config_def`]）。
    pub options: Vec<ConfigOptionDef>,
    /// `var` → [`Self::options`] 下标。
    pub by_var: HashMap<String, usize>,
}

impl ConfigCatalog {
    /// 从条目列表构建（同 `var` 重复时后写覆盖——overlay 产物按 var 唯一，
    /// 此分支实际不可达，保守取后写与 loader 容错一致）。
    pub fn new(options: Vec<ConfigOptionDef>) -> Self {
        let by_var = options
            .iter()
            .enumerate()
            .map(|(index, option)| (option.var.clone(), index))
            .collect();
        Self { options, by_var }
    }

    /// 按 `var` 查条目。
    pub fn get(&self, var: &str) -> Option<&ConfigOptionDef> {
        self.by_var.get(var).map(|&index| &self.options[index])
    }

    /// 未通过抽取期 oracle 对拍的条目数（`verified:false`，parity 报表单列，
    /// 蓝图 §4.6 A6 监控口径）。
    pub fn unverified_count(&self) -> usize {
        self.options.iter().filter(|o| !o.verified).count()
    }
}

/// 注入计算引擎的规则/常量聚合。
///
/// 字段为 `None` 表示对应域尚未数据化（或数据目录缺该文件）——消费方在过渡期
/// 回退 `Default` fallback（与硬编码路径逐值一致）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RuleSet {
    /// modifier 文本解析规则（forms/name_map/flag_phrases/tag_phrases…，M6）。
    pub parser_rules: Option<ParserRules>,
    /// 机制公式消费的游戏常量（抗性边界/护甲系数/服务器帧率…，M0-W3 起接通）。
    pub game_constants: Option<GameConstantsDef>,
    /// 角色等级/属性派生常量（life/mana/accuracy 派生系数，M0-W3 阶段 2 接通）。
    pub character_constants: Option<CharacterConstantsDef>,
    /// 范围珠宝环形档（距离乘数 + 按树版本档位表，M0-W3 阶段 2 接通）。
    /// 消费侧在 pobr-build（树几何，pobr-core 不消费此域，不入 `RuntimeConstants`）。
    pub jewel_radii: Option<JewelRadiiDef>,
    /// 怪物百级缩放表（enemy 装配查表，M0-W3 阶段 2 接通）。
    pub monster_scaling: Option<MonsterScalingTable>,
    /// 敌人档位预设（enemyIsBoss 四档加成，M0-W3 阶段 2 接通）。
    pub enemy_presets: Option<EnemyPresetsTable>,
    /// per-class 空手基底表（空手攻击 weaponData，M0-W3 阶段 2 接通）。
    pub unarmed_data: Option<UnarmedDataTable>,
    /// 武器类型表（持握/近战条件与武器类伤害关键词查表，M0-W3 阶段 2 接通）。
    pub weapon_types: Option<WeaponTypeTable>,
    /// config 选项目录（声明式 effects + imply_conditions，M3）。
    pub config_catalog: Option<ConfigCatalog>,
    /// 取整精度例外表（ScaleAddMod / MORE 聚合精度，M4-T1 W-A2 接通；
    /// 消费侧 = pobr-core `HighPrecisionRules`）。
    pub high_precision_mods: Option<HighPrecisionModsDef>,
}

impl GameData {
    /// 加载 RuleSet 聚合：逐域加载已数据化的 JSON（M0-W3：`game_constants`）。
    ///
    /// 缺文件（`LoadError::Io`，如旧数据包/测试目录）按「该域未数据化」处理返回
    /// `None`（消费方回退 `Default`）；JSON 解析错误（`LoadError::Parse`）则向上
    /// 抛出——文件存在但坏了不允许静默回退。
    pub fn load_ruleset(&self) -> Result<RuleSet, LoadError> {
        let game_constants = match self.game_constants() {
            Ok(v) => Some(v),
            Err(LoadError::Io { .. }) => None,
            Err(e) => return Err(e),
        };
        let character_constants = match self.character_constants() {
            Ok(v) => Some(v),
            Err(LoadError::Io { .. }) => None,
            Err(e) => return Err(e),
        };
        let jewel_radii = match self.jewel_radii() {
            Ok(v) => Some(v),
            Err(LoadError::Io { .. }) => None,
            Err(e) => return Err(e),
        };
        let monster_scaling = match self.monster_scaling() {
            Ok(v) => Some(v),
            Err(LoadError::Io { .. }) => None,
            Err(e) => return Err(e),
        };
        let enemy_presets = match self.enemy_presets() {
            Ok(v) => Some(v),
            Err(LoadError::Io { .. }) => None,
            Err(e) => return Err(e),
        };
        // 域 loader 返回 Vec（W2 口径），此处包成注入用表 newtype。
        let unarmed_data = match self.unarmed_data() {
            Ok(v) => Some(UnarmedDataTable(v)),
            Err(LoadError::Io { .. }) => None,
            Err(e) => return Err(e),
        };
        let weapon_types = match self.weapon_types() {
            Ok(v) => Some(WeaponTypeTable(v)),
            Err(LoadError::Io { .. }) => None,
            Err(e) => return Err(e),
        };
        // R7 缺表容忍：表不存在 → None，消费方回退旧 parse_config 路径。
        let config_catalog = match self.config_options() {
            Ok(v) => Some(ConfigCatalog::new(v.options)),
            Err(LoadError::Io { .. }) => None,
            Err(e) => return Err(e),
        };
        let high_precision_mods = match self.high_precision_mods() {
            Ok(v) => Some(v),
            Err(LoadError::Io { .. }) => None,
            Err(e) => return Err(e),
        };
        Ok(RuleSet {
            parser_rules: None,
            game_constants,
            character_constants,
            jewel_radii,
            monster_scaling,
            enemy_presets,
            unarmed_data,
            weapon_types,
            config_catalog,
            high_precision_mods,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::GameData;

    /// 数据目录不存在时：load_ruleset 仍成功，各域为未填充（None）→ 消费方走 Default。
    #[test]
    fn missing_dir_ruleset_loads_with_all_domains_unfilled() {
        let ruleset = GameData::new("nonexistent-dir").load_ruleset().unwrap();
        assert!(ruleset.parser_rules.is_none());
        assert!(ruleset.game_constants.is_none());
        assert!(ruleset.character_constants.is_none());
        assert!(ruleset.jewel_radii.is_none());
        assert!(ruleset.monster_scaling.is_none());
        assert!(ruleset.enemy_presets.is_none());
        assert!(ruleset.unarmed_data.is_none());
        assert!(ruleset.weapon_types.is_none());
        assert!(ruleset.config_catalog.is_none());
        assert!(ruleset.high_precision_mods.is_none());
    }

    /// 仓库数据目录：high_precision_mods 域已接通（M4-T1 W-A2，
    /// `overlay/high_precision_mods.json` ← vendor Data.lua:413-530）。
    #[test]
    fn repo_data_ruleset_loads_high_precision_mods() {
        let data = GameData::new(crate::repo_data_root().join("4.5.0.3.4"));
        let ruleset = data.load_ruleset().unwrap();
        let loaded = ruleset
            .high_precision_mods
            .expect("high_precision_mods 域应已接通");
        assert_eq!(loaded.default_high_precision, 1);
        assert_eq!(loaded.mods.len(), 38, "vendor highPrecisionMods 38 条");
    }

    /// 仓库数据目录：game_constants 域已接通（Some），且与 Default fallback 逐值相等
    /// （搬迁不变式：注入与回退两条路径输出一致）。
    #[test]
    fn repo_data_ruleset_loads_game_constants_equal_to_default() {
        let data = GameData::new(crate::repo_data_root().join("4.5.0.3.4"));
        let ruleset = data.load_ruleset().unwrap();
        let loaded = ruleset.game_constants.expect("game_constants 域应已接通");
        assert_eq!(
            loaded,
            pobr_data::catalog::game_constants::GameConstantsDef::default(),
            "JSON 与 Default fallback 必须逐值相等（搬迁不变式）",
        );
    }

    /// 仓库数据目录：character_constants 域已接通（Some），且与 Default fallback 逐值
    /// 相等（搬迁不变式：注入与回退两条路径输出一致）。
    #[test]
    fn repo_data_ruleset_loads_character_constants_equal_to_default() {
        let data = GameData::new(crate::repo_data_root().join("4.5.0.3.4"));
        let ruleset = data.load_ruleset().unwrap();
        let loaded = ruleset
            .character_constants
            .expect("character_constants 域应已接通");
        assert_eq!(
            loaded,
            pobr_data::catalog::character_constants::CharacterConstantsDef::default(),
            "JSON 与 Default fallback 必须逐值相等（搬迁不变式）",
        );
    }

    /// 仓库数据目录：jewel_radii 域已接通（Some），且与 Default fallback 逐值相等
    /// （搬迁不变式：注入与回退两条路径输出一致）。
    #[test]
    fn repo_data_ruleset_loads_jewel_radii_equal_to_default() {
        let data = GameData::new(crate::repo_data_root().join("4.5.0.3.4"));
        let ruleset = data.load_ruleset().unwrap();
        let loaded = ruleset.jewel_radii.expect("jewel_radii 域应已接通");
        assert_eq!(
            loaded,
            pobr_data::catalog::jewel_radii::JewelRadiiDef::default(),
            "JSON 与 Default fallback 必须逐值相等（搬迁不变式）",
        );
    }

    /// 仓库数据目录：monster_scaling 域已接通（Some），且与 Default fallback **整表
    /// 逐值相等**（含 vendor-only ally 两表；搬迁不变式：注入/回退两条路径输出一致）。
    #[test]
    fn repo_data_ruleset_loads_monster_scaling_equal_to_default() {
        let data = GameData::new(crate::repo_data_root().join("4.5.0.3.4"));
        let ruleset = data.load_ruleset().unwrap();
        let loaded = ruleset.monster_scaling.expect("monster_scaling 域应已接通");
        assert_eq!(
            loaded,
            pobr_data::catalog::monster_scaling::MonsterScalingTable::default(),
            "JSON 与 Default fallback 必须逐值相等（搬迁不变式）",
        );
    }

    /// 仓库数据目录：unarmed_data 域已接通（Some），且与 Default fallback **整表
    /// 逐值相等**（含 vendor-only class_id/weapon_type 字段；搬迁不变式）。
    #[test]
    fn repo_data_ruleset_loads_unarmed_data_equal_to_default() {
        let data = GameData::new(crate::repo_data_root().join("4.5.0.3.4"));
        let ruleset = data.load_ruleset().unwrap();
        let loaded = ruleset.unarmed_data.expect("unarmed_data 域应已接通");
        assert_eq!(
            loaded,
            pobr_data::catalog::unarmed_data::UnarmedDataTable::default(),
            "JSON 与 Default fallback 必须逐值相等（搬迁不变式）",
        );
    }

    /// 仓库数据目录：weapon_types 域已接通（Some），且与 Default fallback **整表
    /// 逐值相等**（19 条 vendor weaponTypeInfo 含 label；搬迁不变式）。
    #[test]
    fn repo_data_ruleset_loads_weapon_types_equal_to_default() {
        let data = GameData::new(crate::repo_data_root().join("4.5.0.3.4"));
        let ruleset = data.load_ruleset().unwrap();
        let loaded = ruleset.weapon_types.expect("weapon_types 域应已接通");
        assert_eq!(
            loaded,
            pobr_data::catalog::weapon_types::WeaponTypeTable::default(),
            "JSON 与 Default fallback 必须逐值相等（搬迁不变式）",
        );
    }

    /// 仓库数据目录：config_catalog 域已接通（M3-T1 A3）——条目数覆盖 vendor 542+、
    /// `by_var` 索引与条目一一对应、典型条目可查；并打印 `verified:false` 条目数
    /// （蓝图 §4.6 A6：未对拍条目入 parity 报表口径）。
    #[test]
    fn repo_data_ruleset_loads_config_catalog() {
        let data = GameData::new(crate::repo_data_root().join("4.5.0.3.4"));
        let ruleset = data.load_ruleset().unwrap();
        let catalog = ruleset.config_catalog.expect("config_catalog 域应已接通");
        assert!(
            catalog.options.len() >= 542,
            "config 条目数 {} 低于 vendor 静态条目数 542",
            catalog.options.len()
        );
        // vendor ConfigOptions.lua 存在 1 处同 var 条目（conditionEnemyExitedPresenceRecently
        // 同 var 承载 Exited/Entered 两条，vendor 侧本身共用同一输入键）——by_var 按
        // 后写覆盖收敛为唯一索引。
        let unique_vars: std::collections::BTreeSet<&str> =
            catalog.options.iter().map(|o| o.var.as_str()).collect();
        assert_eq!(
            catalog.by_var.len(),
            unique_vars.len(),
            "by_var 索引必须与去重后的 var 集一一对应"
        );
        for (var, &index) in &catalog.by_var {
            assert_eq!(&catalog.options[index].var, var, "by_var 索引指向错误条目");
        }
        let stationary = catalog
            .get("conditionStationary")
            .expect("conditionStationary 条目存在");
        assert_eq!(
            stationary.input_type,
            pobr_data::catalog::config_def::ConfigInputType::Count
        );
        assert!(catalog.get("不存在的var").is_none());

        // A6 监控：verified:false 条目数（运行时照用，parity 报告单列）。
        let unverified = catalog.unverified_count();
        println!(
            "config_catalog verified:false 条目数 = {unverified} / {}",
            catalog.options.len()
        );
        assert!(
            unverified <= 54,
            "verified:false 条目数 {unverified} 超过 handler 预算口径 54（DSL 切分失败信号）"
        );
    }

    /// 仓库数据目录：enemy_presets 域已接通（Some），且与 Default fallback **整表
    /// 逐值相等**（含 vendor-only mod 组/占位列；搬迁不变式）。
    #[test]
    fn repo_data_ruleset_loads_enemy_presets_equal_to_default() {
        let data = GameData::new(crate::repo_data_root().join("4.5.0.3.4"));
        let ruleset = data.load_ruleset().unwrap();
        let loaded = ruleset.enemy_presets.expect("enemy_presets 域应已接通");
        assert_eq!(
            loaded,
            pobr_data::catalog::enemy_presets::EnemyPresetsTable::default(),
            "JSON 与 Default fallback 必须逐值相等（搬迁不变式）",
        );
    }
}
