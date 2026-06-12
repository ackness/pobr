//! pobr-data-adapter：把 pathofexile-dat 的原始 `.dat` JSON 适配为 PoBR 最小 JSON。
//!
//! 解析整型外键（ItemClass / Tags / Implicit_Mods → 稳定字符串 ID）、反范式化、
//! 过滤开发用占位条目，输出按 id 排序的 diff 友好 JSON 到 `data/<patch>/base/`
//! （三层布局的 base 层，见架构文档 20 §1 P1）；`manifest.json` 与 `i18n/`
//! 边车留在版本根。
//!
//! 用法：
//! ```text
//! # 物品基底域（pathofexile-dat 表）
//! cargo run -p pobr-data-adapter -- --raw pipeline/tables --out data --patch 4.5.0.3.4
//! # 被动天赋树域（GGG 官方树导出 data.json）
//! cargo run -p pobr-data-adapter -- --tree pipeline/tree/data.json --out data --patch 4.5.0.3.4
//! ```

mod tree;
mod tree_coords;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use pobr_data::catalog::{
    ArmourBaseStats, BaseItemDef, CATALOG_SCHEMA_VERSION, DataManifest, DomainSections,
    WeaponBaseStats,
};
use serde::Deserialize;

mod mods;
mod required_columns;
mod skills;

const ZH_TW: &str = "Traditional Chinese";

fn main() -> ExitCode {
    let result = match parse_args() {
        Ok(Mode::BaseItems(args)) => run(args),
        Ok(Mode::Tree(args)) => tree::run(args),
        Ok(Mode::TreeCoords(args)) => tree_coords::run(args),
        Err(err) => Err(err),
    };
    match result {
        Ok(summary) => {
            println!("{summary}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("pobr-data-adapter 失败：{err}");
            ExitCode::FAILURE
        }
    }
}

struct Args {
    raw: PathBuf,
    out: PathBuf,
    patch: String,
}

/// 适配器子命令：物品基底域（`--raw`）或被动树域（`--tree`），二者互斥。
enum Mode {
    BaseItems(Args),
    Tree(tree::TreeArgs),
    /// 从 vendor `tree.lua` 回填既有 `passive_tree.json` 的节点 x/y 坐标。
    TreeCoords(tree_coords::TreeCoordsArgs),
}

fn parse_args() -> Result<Mode, String> {
    let mut raw = None;
    let mut tree = None;
    let mut tree_coords = None;
    let mut out = None;
    let mut patch = None;
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let mut take = |name: &str| it.next().ok_or_else(|| format!("{name} 缺少参数值"));
        match flag.as_str() {
            "--raw" => raw = Some(PathBuf::from(take("--raw")?)),
            "--tree" => tree = Some(PathBuf::from(take("--tree")?)),
            "--tree-coords" => tree_coords = Some(PathBuf::from(take("--tree-coords")?)),
            "--out" => out = Some(PathBuf::from(take("--out")?)),
            "--patch" => patch = Some(take("--patch")?),
            other => return Err(format!("未知参数：{other}")),
        }
    }
    let out = out.ok_or("缺少 --out <data>")?;
    let patch = patch.ok_or("缺少 --patch <version>")?;
    match (raw, tree, tree_coords) {
        (Some(_), Some(_), _) | (Some(_), _, Some(_)) | (_, Some(_), Some(_)) => {
            Err("--raw / --tree / --tree-coords 互斥，请分别运行".into())
        }
        (Some(raw), None, None) => Ok(Mode::BaseItems(Args { raw, out, patch })),
        (None, Some(data_json), None) => Ok(Mode::Tree(tree::TreeArgs {
            data_json,
            out,
            patch,
        })),
        (None, None, Some(tree_lua)) => Ok(Mode::TreeCoords(tree_coords::TreeCoordsArgs {
            tree_lua,
            out,
            patch,
        })),
        (None, None, None) => Err(
            "缺少 --raw <pipeline/tables> / --tree <data.json> / --tree-coords <tree.lua>".into(),
        ),
    }
}

// ---- 原始 .dat JSON 行结构（只取我们需要的列）----

#[derive(Deserialize)]
pub(crate) struct RawIndexed {
    #[serde(rename = "_index")]
    index: usize,
    #[serde(rename = "Id")]
    id: String,
}

#[derive(Deserialize)]
struct RawBaseItem {
    #[serde(rename = "Id")]
    id: String,
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "ItemClass")]
    item_class: Option<usize>,
    #[serde(rename = "DropLevel")]
    drop_level: Option<i64>,
    #[serde(rename = "Width")]
    width: Option<u8>,
    #[serde(rename = "Height")]
    height: Option<u8>,
    #[serde(rename = "Tags", default)]
    tags: Vec<usize>,
    #[serde(rename = "Implicit_Mods", default)]
    implicit_mods: Vec<usize>,
    #[serde(rename = "ModDomain")]
    mod_domain: Option<u32>,
}

#[derive(Deserialize)]
pub(crate) struct RawNamed {
    #[serde(rename = "_index")]
    pub(crate) index: usize,
    #[serde(rename = "Name")]
    pub(crate) name: Option<String>,
}

#[derive(Deserialize)]
struct RawWeaponType {
    /// `BaseItemTypes` 行索引。
    #[serde(rename = "BaseItemType")]
    base_item_type: Option<usize>,
    #[serde(rename = "DamageMin")]
    damage_min: Option<i64>,
    #[serde(rename = "DamageMax")]
    damage_max: Option<i64>,
    #[serde(rename = "Speed")]
    speed: Option<i64>,
    #[serde(rename = "CritChance")]
    crit_chance: Option<i64>,
    #[serde(rename = "RangeMax")]
    range_max: Option<i64>,
    /// 弩装填时间（毫秒，M4-T4 W-D2；vendor `Export/spec.lua:62483`、
    /// `bases.lua:268-269` 仅 >0 导出）。旧导出（无此列的 tables 快照）回退
    /// `None`，产物字段保持缺省（schema 兼容；当前由 overlay
    /// `base_item_overrides.json` 兜底填充）。
    #[serde(rename = "ReloadTime", default)]
    reload_time: Option<i64>,
}

#[derive(Deserialize)]
struct RawArmourType {
    #[serde(rename = "BaseItemType")]
    base_item_type: Option<usize>,
    #[serde(rename = "Armour")]
    armour: Option<i64>,
    #[serde(rename = "Evasion")]
    evasion: Option<i64>,
    #[serde(rename = "EnergyShield")]
    energy_shield: Option<i64>,
    #[serde(rename = "Ward")]
    ward: Option<i64>,
    /// 移动速度修正原始值（万分比，负值为减速；如 `-300` = 3% 移速惩罚）。
    /// 旧导出（无此列的 tables 快照）回退 `None`，产物字段保持缺省（schema 兼容）。
    #[serde(rename = "IncreasedMovementSpeed", default)]
    increased_movement_speed: Option<i64>,
}

fn nn(v: Option<i64>) -> u32 {
    v.unwrap_or(0).max(0) as u32
}

/// `WeaponTypes` / `ArmourTypes` 按 base item 行索引的基底数值查表对。
type BaseStatsLookups = (
    BTreeMap<usize, WeaponBaseStats>,
    BTreeMap<usize, ArmourBaseStats>,
);

/// 把 `WeaponTypes` / `ArmourTypes`（按 `BaseItemType` 行索引）建成基底数值查表。
fn weapon_armour_lookups(en: &Path) -> Result<BaseStatsLookups, String> {
    let weapons = read_json::<Vec<RawWeaponType>>(&en.join("WeaponTypes.json"))?;
    let mut weapon_map = BTreeMap::new();
    for w in weapons {
        if let Some(idx) = w.base_item_type {
            weapon_map.insert(
                idx,
                WeaponBaseStats {
                    physical_min: nn(w.damage_min),
                    physical_max: nn(w.damage_max),
                    speed_ms: nn(w.speed),
                    crit_chance: nn(w.crit_chance),
                    range: nn(w.range_max),
                    // 仅 >0 入库（对齐 vendor bases.lua:268 `if ReloadTime > 0`；
                    // 非弩武器该列恒 0）。
                    reload_time_ms: w.reload_time.filter(|&v| v > 0).map(|v| v as u32),
                },
            );
        }
    }
    let armours = read_json::<Vec<RawArmourType>>(&en.join("ArmourTypes.json"))?;
    let mut armour_map = BTreeMap::new();
    for a in armours {
        if let Some(idx) = a.base_item_type {
            armour_map.insert(
                idx,
                ArmourBaseStats {
                    armour: nn(a.armour),
                    evasion: nn(a.evasion),
                    energy_shield: nn(a.energy_shield),
                    ward: nn(a.ward),
                    // 盾牌格挡（`ShieldTypes.Block`）：对应 bundle 已被 CDN 对钉定
                    // patch 剪除，`.dat` 路线不可得 → 恒 None，由 overlay
                    // `base_item_overrides.json` 在 gamedata 加载侧 merge 填充
                    // （蓝图 m2-defence §6 开放问题 1 的双路线兜底）。
                    block_chance: None,
                    // 移速惩罚：PoB2 口径 `-raw/10000`（Export/Scripts/bases.lua:298），
                    // 如 raw=-300 → 0.03（3% 减速）；raw=0 → None（diff 友好）。
                    movement_penalty: a
                        .increased_movement_speed
                        .filter(|&v| v != 0)
                        .map(|v| -(v as f64) / 10000.0),
                },
            );
        }
    }
    Ok((weapon_map, armour_map))
}

pub(crate) fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let bytes = fs::read(path).map_err(|e| format!("读取 {} 失败：{e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("解析 {} 失败：{e}", path.display()))
}

/// 把 `_index -> Id` 建成按位置索引的查表（`_index` 连续，越界返回 None）。
fn id_lookup(rows: &[RawIndexed]) -> Vec<String> {
    let max = rows.iter().map(|r| r.index).max().map_or(0, |m| m + 1);
    let mut table = vec![String::new(); max];
    for r in rows {
        table[r.index] = r.id.clone();
    }
    table
}

pub(crate) fn resolve(lookup: &[String], idx: usize) -> Option<String> {
    lookup.get(idx).filter(|s| !s.is_empty()).cloned()
}

/// 开发用占位 / 未启用条目（不入库）。
pub(crate) fn is_placeholder(name: &str) -> bool {
    name.is_empty() || name.contains("[DNT") || name.contains("[UNUSED") || name.contains("[OLD")
}

fn run(args: Args) -> Result<String, String> {
    let en = args.raw.join("English");
    let tw = args.raw.join(ZH_TW);

    // F2：必需列 fail-fast——缺列即报「表名 + 列名」，拒绝静默降级产出。
    required_columns::assert_required_columns(&en, &tw)?;

    // 外键解析表
    let classes = id_lookup(&read_json::<Vec<RawIndexed>>(&en.join("ItemClasses.json"))?);
    let tags = id_lookup(&read_json::<Vec<RawIndexed>>(&en.join("Tags.json"))?);
    let mods = id_lookup(&read_json::<Vec<RawIndexed>>(&en.join("Mods.json"))?);
    let stats = id_lookup(&read_json::<Vec<RawIndexed>>(&en.join("Stats.json"))?);

    // 基底（英文 canonical + 繁中名称边车）
    let raw_bases = read_json::<Vec<RawBaseItem>>(&en.join("BaseItemTypes.json"))?;
    let tw_names = read_json::<Vec<RawNamed>>(&tw.join("BaseItemTypes.json"))?;
    let tw_by_index: BTreeMap<usize, String> = tw_names
        .into_iter()
        .filter_map(|r| r.name.map(|n| (r.index, n)))
        .collect();

    // 武器/护甲基底数值（WeaponTypes / ArmourTypes，按 base item 行索引 join）。
    let (weapon_map, armour_map) = weapon_armour_lookups(&en)?;

    let mut bases = Vec::new();
    let mut i18n_zh: BTreeMap<String, String> = BTreeMap::new();
    let total = raw_bases.len();
    for (index, raw) in raw_bases.into_iter().enumerate() {
        if is_placeholder(&raw.name) {
            continue;
        }
        let item_class = raw
            .item_class
            .and_then(|i| resolve(&classes, i))
            .unwrap_or_default();
        let tag_ids: Vec<String> = raw.tags.iter().filter_map(|&i| resolve(&tags, i)).collect();
        let implicits: Vec<String> = raw
            .implicit_mods
            .iter()
            .filter_map(|&i| resolve(&mods, i))
            .collect();

        if let Some(zh) = tw_by_index.get(&index)
            && !zh.is_empty()
            && *zh != raw.name
        {
            i18n_zh.insert(raw.id.clone(), zh.clone());
        }

        bases.push(BaseItemDef {
            id: raw.id,
            name: raw.name,
            item_class,
            drop_level: raw.drop_level.unwrap_or(0).max(0) as u32,
            width: raw.width.unwrap_or(1),
            height: raw.height.unwrap_or(1),
            tags: tag_ids,
            implicits,
            mod_domain: raw.mod_domain.unwrap_or(0),
            weapon: weapon_map.get(&index).cloned(),
            armour: armour_map.get(&index).cloned(),
            // 基底 Spirit（`ItemSpirit.SpiritGranted`）：同 block_chance，`.dat`
            // bundle 不可得 → 恒 None，由 overlay 在 gamedata 加载侧 merge 填充。
            spirit: None,
        });
    }

    bases.sort_by(|a, b| a.id.cmp(&b.id));

    // 写出：数据域 JSON 落 base/ 层；manifest 与 i18n 边车留版本根。
    let version_dir = args.out.join(&args.patch);
    let base_dir = version_dir.join("base");
    fs::create_dir_all(&base_dir).map_err(|e| format!("创建输出目录失败：{e}"))?;
    fs::create_dir_all(version_dir.join("i18n").join("zh-TW"))
        .map_err(|e| format!("创建输出目录失败：{e}"))?;

    write_pretty(&base_dir.join("base_items.json"), &bases)?;
    write_pretty(&version_dir.join("i18n/zh-TW/base_items.json"), &i18n_zh)?;

    // Mods + Stats 域（stat 注册表 + 词缀池）。
    let (stat_count, mod_count, mod_filtered, mod_zh) =
        mods::adapt(&en, &tw, &stats, &tags, &base_dir, &version_dir)?;

    // 技能宝石域（SkillGems / GrantedEffects / GrantedEffectsPerLevel / ActiveSkills）
    let skills = skills::adapt_skills(&en, &tw)?;
    write_pretty(&base_dir.join("skill_gems.json"), &skills.gems)?;
    write_pretty(&base_dir.join("granted_effects.json"), &skills.effects)?;
    write_pretty(&base_dir.join("granted_effect_levels.json"), &skills.levels)?;
    write_pretty(
        &version_dir.join("i18n/zh-TW/skills.json"),
        &skills.zh_skill_names,
    )?;

    // 分等级伤害 stat 集（GrantedEffectStatSets* → effect id → 每级伤害）。
    let stat_sets = skills::adapt_stat_sets(&en)?;
    write_pretty(
        &base_dir.join("granted_effect_stat_sets.json"),
        &stat_sets.sets,
    )?;

    // 技能消耗资源类型（CostTypes → cost_types FK 目标，解析 Mana/Life/ES/... 资源名）。
    let cost_types = skills::adapt_cost_types(&en)?;
    write_pretty(&base_dir.join("cost_types.json"), &cost_types)?;

    let manifest = DataManifest {
        schema_version: CATALOG_SCHEMA_VERSION,
        poe_version: args.patch.clone(),
        languages: vec!["zh-TW".into()],
        domains: DomainSections {
            base: vec![
                "base_items".into(),
                "mods".into(),
                "stats".into(),
                "skill_gems".into(),
                "granted_effects".into(),
                "granted_effect_levels".into(),
                "granted_effect_stat_sets".into(),
                "cost_types".into(),
            ],
            overlay: Vec::new(),
            generated: Vec::new(),
        },
    };
    write_pretty(&version_dir.join("manifest.json"), &manifest)?;

    Ok(format!(
        "适配完成：base_items {}/{} 条（过滤 {} 个占位），zh-TW 名称 {} 条；\
         stats {} 条；mods {} 条（过滤 {} 个空壳），mods zh-TW 名称 {} 条；\
         skill_gems {}/{} 条，granted_effects {}/{} 条，\
         granted_effect_levels {} 个效果 / {} 行，zh-TW 技能名 {} 条，\
         granted_effect_stat_sets {} 个伤害效果 / {} 级（共 {} stat-set） → {}",
        bases.len(),
        total,
        total - bases.len(),
        i18n_zh.len(),
        stat_count,
        mod_count,
        mod_filtered,
        mod_zh,
        skills.gems.len(),
        skills.gems_total,
        skills.effects.len(),
        skills.effects_total,
        skills.levels.len(),
        skills.level_rows_total,
        skills.zh_skill_names.len(),
        stat_sets.sets.len(),
        stat_sets.damage_levels_total,
        stat_sets.sets_total,
        version_dir.display()
    ))
}

pub(crate) fn write_pretty<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|e| format!("序列化 {} 失败：{e}", path.display()))?;
    fs::write(path, format!("{json}\n")).map_err(|e| format!("写入 {} 失败：{e}", path.display()))
}
